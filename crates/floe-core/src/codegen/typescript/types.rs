use crate::parser::ast::*;
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
                if name == type_layout::TYPE_OPTION && type_args.len() == 1 {
                    return pretty::concat([
                        self.emit_type_expr(&type_args[0]),
                        pretty::str(" | null | undefined"),
                    ]);
                }
                if name == type_layout::TYPE_SETTABLE && type_args.len() == 1 {
                    return pretty::concat([
                        self.emit_type_expr(&type_args[0]),
                        pretty::str(" | null | undefined"),
                    ]);
                }
                if name == type_layout::TYPE_RESULT && type_args.len() == 2 {
                    return pretty::concat([
                        pretty::str(format!("{{ {OK_FIELD}: true; {VALUE_FIELD}: ")),
                        self.emit_type_expr(&type_args[0]),
                        pretty::str(format!(" }} | {{ {OK_FIELD}: false; {ERROR_FIELD}: ")),
                        self.emit_type_expr(&type_args[1]),
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

                if type_args.is_empty() {
                    pretty::str(name)
                } else {
                    let mut docs = vec![pretty::str(name), pretty::str("<")];
                    for (i, arg) in type_args.iter().enumerate() {
                        if i > 0 {
                            docs.push(pretty::str(", "));
                        }
                        docs.push(self.emit_type_expr(arg));
                    }
                    docs.push(pretty::str(">"));
                    pretty::concat(docs)
                }
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
                    docs.push(pretty::str(format!("_p{i}: ")));
                    docs.push(self.emit_type_expr(param));
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
