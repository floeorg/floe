use crate::parser::ast::{TypeExprKind, TypedTypeExpr};
use crate::pretty::{self, Document};
use crate::type_layout;
use crate::type_layout::{ERROR_FIELD, OK_FIELD, VALUE_FIELD};

use super::generator::TypeScriptGenerator;

impl<'a> TypeScriptGenerator<'a> {
    // ── Type Expressions ─────────────────────────────────────────

    pub(super) fn emit_type_expr(&mut self, type_expr: &TypedTypeExpr) -> Document {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                // `Option`, `Settable` and `Result` match on the name alone,
                // and a missing argument means `unknown`. The checker
                // decides these three the same way, in
                // `Checker::resolve_named_type`, which reads the name and
                // defaults every absent argument to `Type::Unknown`.
                //
                // Matching on the arity as well used to part the two
                // passes: the checker read a bare `Option` as
                // `Option<Unknown>` and codegen emitted the bare name
                // `Option`, which TypeScript does not declare. See #1521.
                if name == type_layout::TYPE_OPTION || name == type_layout::TYPE_SETTABLE {
                    return pretty::concat([
                        self.emit_type_arg_or_unknown(type_args, 0),
                        pretty::str(" | null | undefined"),
                    ]);
                }
                if name == type_layout::TYPE_RESULT {
                    return pretty::concat([
                        pretty::str(format!("{{ {OK_FIELD}: true; {VALUE_FIELD}: ")),
                        self.emit_type_arg_or_unknown(type_args, 0),
                        pretty::str(format!(" }} | {{ {OK_FIELD}: false; {ERROR_FIELD}: ")),
                        self.emit_type_arg_or_unknown(type_args, 1),
                        pretty::str(" }"),
                    ]);
                }
                if name == type_layout::TYPE_UNIT {
                    return pretty::str("void");
                }

                if name == type_layout::TYPE_ONE_OF {
                    return if type_args.is_empty() {
                        pretty::str("never")
                    } else {
                        self.emit_type_joined(type_args, " | ")
                    };
                }

                if name == type_layout::TYPE_INTERSECT {
                    return if type_args.is_empty() {
                        pretty::str("unknown")
                    } else {
                        self.emit_type_joined(type_args, " & ")
                    };
                }

                pretty::concat([pretty::str(name), self.emit_type_args(type_args)])
            }
            TypeExprKind::Record(fields) => self.emit_record_type(fields),
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                let mut docs = vec![pretty::str("(")];
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    let name = param.label.clone().unwrap_or_else(|| format!("_p{i}"));
                    docs.push(pretty::str(format!("{name}: ")));
                    docs.push(self.emit_type_expr(&param.type_ann));
                }
                docs.push(pretty::str(") => "));
                docs.push(self.emit_type_expr(return_type));
                pretty::concat(docs)
            }
            TypeExprKind::Array(inner) => {
                pretty::concat([self.emit_type_expr(inner), pretty::str("[]")])
            }
            TypeExprKind::Tuple(types) => {
                let mut docs = vec![pretty::str("readonly [")];
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    docs.push(self.emit_type_expr(t));
                }
                docs.push(pretty::str("]"));
                pretty::concat(docs)
            }
            TypeExprKind::TypeOf(name) => pretty::str(format!("typeof {name}")),
            TypeExprKind::Intersection(types) => self.emit_type_joined(types, " & "),
            TypeExprKind::StringLiteral(value) => pretty::str(format!("\"{value}\"")),
        }
    }

    /// Emit a type-argument list as `<A, B>`, or nothing when the list is
    /// empty. Both a named type (`Result<T, E>`) and a call site
    /// (`useState<Filter>(All)`) print their arguments this way.
    ///
    /// A call site prints only the arguments the user wrote. Codegen never
    /// prints an argument the checker inferred: Floe's inference and
    /// TypeScript's are separate, so a printed guess would pin the call to
    /// Floe's answer and break every call where the two differ. An absent
    /// list leaves the call to TypeScript, which is what it did before.
    ///
    /// A stdlib call carries no list either. Its template rewrites the whole
    /// call, and the checker drops explicit arguments on that path as well,
    /// so the two passes stay on one answer.
    pub(super) fn emit_type_args(&mut self, type_args: &[TypedTypeExpr]) -> Document {
        if type_args.is_empty() {
            return pretty::nil();
        }

        let mut docs = vec![pretty::str("<")];
        for (i, arg) in type_args.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(", "));
            }
            docs.push(self.emit_type_expr(arg));
        }
        docs.push(pretty::str(">"));

        pretty::concat(docs)
    }

    /// Emit the type argument at `index`, or `unknown` when the user wrote
    /// no argument there. The checker defaults an absent argument to
    /// `Type::Unknown`, so this keeps the two passes on the same answer.
    fn emit_type_arg_or_unknown(&mut self, type_args: &[TypedTypeExpr], index: usize) -> Document {
        let Some(arg) = type_args.get(index) else {
            return pretty::str(type_layout::TYPE_UNKNOWN);
        };

        self.emit_type_expr(arg)
    }

    fn emit_type_joined(&mut self, types: &[TypedTypeExpr], sep: &str) -> Document {
        let mut docs = Vec::with_capacity(types.len() * 2);
        for (i, ty) in types.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(sep.to_string()));
            }
            docs.push(self.emit_type_expr(ty));
        }
        pretty::concat(docs)
    }
}
