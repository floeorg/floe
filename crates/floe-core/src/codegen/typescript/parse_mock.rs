use crate::checker::{Type, simple_resolve_type_expr};
use crate::parser::ast::{
    Arg, RecordEntry, TypeDef, TypeExprKind, TypedArg, TypedExpr, TypedTypeDef, TypedTypeExpr,
};
use crate::pretty::{self, Document};
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

use super::expression::expr_contains_await;
use super::generator::{THROW_MOCK_FUNCTION, TypeScriptGenerator};

impl<'a> TypeScriptGenerator<'a> {
    // ── Parse<T> Validation Codegen ─────────────────────────────

    /// Emit `parse<T>(value)`.
    ///
    /// `result_ty` is the type the checker gave the whole `parse` call,
    /// which is `Result<T, Error>` with `T` already resolved. The
    /// validation reads that resolved type, never the annotation the user
    /// wrote: an alias, a type parameter or a bare `unknown` makes the two
    /// disagree, and the annotation loses. See #1521.
    pub(super) fn emit_parse(
        &mut self,
        type_arg: &TypedTypeExpr,
        value: &TypedExpr,
        result_ty: &Type,
    ) -> Document {
        let value_doc = self.emit_expr(value);
        let parsed = result_ty
            .result_ok()
            .cloned()
            .unwrap_or_else(|| result_ty.clone());
        let mut checks = String::new();
        self.emit_parse_checks(&mut checks, "__v", &parsed, "", &mut Vec::new());
        let type_doc = self.emit_type_expr(type_arg);
        let type_str = Self::doc_to_string(&type_doc);

        let (open, close) = if expr_contains_await(value) {
            ("(await (async () => { const __v = ", "; })())")
        } else {
            ("(() => { const __v = ", "; })()")
        };

        pretty::concat([
            pretty::str(open),
            value_doc,
            pretty::str("; "),
            pretty::str(checks),
            pretty::str(format!(
                "return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: __v as {type_str} }}{close}"
            )),
        ])
    }

    /// Walk the resolved type and emit one run-time check per level.
    ///
    /// A type with no run-time witness emits nothing. `unknown`, a type
    /// parameter and a function type are all in that group: the checker
    /// validated nothing about their shape, so neither does the emitted
    /// code.
    ///
    /// `expanding` holds the type names open on the current path. A name is
    /// the only way a type reaches itself, so stopping on a repeat bounds
    /// the walk. `typealias Tree = { l: Tree, r: Tree }` would otherwise
    /// double the output at every level.
    fn emit_parse_checks(
        &mut self,
        out: &mut String,
        accessor: &str,
        ty: &Type,
        path: &str,
        expanding: &mut Vec<String>,
    ) {
        match &ty.resolved() {
            Type::String | Type::StringLiteral(_) => {
                self.emit_typeof_check(out, accessor, "string", path);
            }
            Type::Number => self.emit_typeof_check(out, accessor, "number", path),
            Type::Bool => self.emit_typeof_check(out, accessor, "boolean", path),
            Type::TsUnion(members)
                if !members.is_empty()
                    && members.iter().all(|m| matches!(m, Type::StringLiteral(_))) =>
            {
                // A union of string literals is a string at run time.
                //
                // `OneOf<"GET", "POST">` does not reach here. The checker
                // resolves `OneOf` to `Type::Named("OneOf")` and models
                // nothing about its members, so `parse` reads it as an
                // opaque name and checks for an object. #1524 carries that.
                self.emit_typeof_check(out, accessor, "string", path);
            }
            Type::Array(inner) => {
                self.emit_array_check(out, accessor, path);
                let idx_var = format!("__i{}", accessor.len());
                let elem_accessor = format!("{accessor}[{idx_var}]");
                let elem_path = element_path(path, &format!("\" + {idx_var} + \""));
                out.push_str(&format!(
                    "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                ));
                self.emit_parse_checks(out, &elem_accessor, inner, &elem_path, expanding);
                out.push_str("} ");
            }
            Type::Record(fields) => {
                self.emit_object_check(out, accessor, path);
                for (name, field_ty) in fields {
                    let field_accessor = format!("({accessor} as any).{name}");
                    let field_path = if path.is_empty() {
                        format!("field '{name}'")
                    } else {
                        format!("{path}.{name}")
                    };
                    self.emit_parse_checks(out, &field_accessor, field_ty, &field_path, expanding);
                }
            }
            Type::Opaque { base, .. } => {
                self.emit_parse_checks(out, accessor, base, path, expanding);
            }
            Type::Settable(inner) => {
                out.push_str(&format!(
                    "if ({accessor} !== undefined && {accessor} !== null) {{ "
                ));
                self.emit_parse_checks(out, accessor, inner, path, expanding);
                out.push_str("} ");
            }
            resolved @ Type::Union { .. } => {
                if let Some(inner) = resolved.option_inner() {
                    out.push_str(&format!("if ({accessor} !== undefined) {{ "));
                    self.emit_parse_checks(out, accessor, inner, path, expanding);
                    out.push_str("} ");
                } else {
                    // A tagged union, including `Result`, is an object at
                    // run time.
                    self.emit_object_check(out, accessor, path);
                }
            }
            Type::Named(name) => {
                // The checker keeps a user type as its name. An alias means
                // whatever its right-hand side means, so follow the chain
                // that `mock<T>` follows and validate what it lands on.
                if expanding.iter().any(|open| open == name) {
                    return;
                }
                let Some(target) = self.ctx.alias_target(name) else {
                    self.emit_object_check(out, accessor, path);
                    return;
                };
                let target_ty = simple_resolve_type_expr(target);
                expanding.push(name.clone());
                self.emit_parse_checks(out, accessor, &target_ty, path, expanding);
                expanding.pop();
            }
            Type::Tuple(members) => {
                // A tuple is an array at run time, with a fixed length.
                self.emit_array_check(out, accessor, path);
                self.emit_length_check(out, accessor, members.len(), path);
                for (index, member) in members.iter().enumerate() {
                    let member_accessor = format!("{accessor}[{index}]");
                    let member_path = element_path(path, &index.to_string());
                    self.emit_parse_checks(out, &member_accessor, member, &member_path, expanding);
                }
            }
            Type::Function { .. } => self.emit_typeof_check(out, accessor, "function", path),
            Type::Foreign { .. }
            | Type::Promise(_)
            | Type::Map { .. }
            | Type::RecordMap { .. }
            | Type::Set { .. } => self.emit_object_check(out, accessor, path),
            Type::Unknown
            | Type::Error
            | Type::Never
            | Type::Unit
            | Type::Undefined
            | Type::TsUnion(_)
            | Type::Var(_) => {
                // Nothing to check at run time.
                //
                // `unknown` is safe to leave open, because the checker
                // refuses to use an `unknown` where a concrete type
                // belongs. Nothing downstream trusts the value.
                //
                // A type parameter is not safe that way: the caller reads
                // the result as a concrete type. So the checker refuses
                // the program with E058 rather than let this arm emit a
                // silent `Ok`, and this arm never reaches a built file.
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn emit_typeof_check(&self, out: &mut String, accessor: &str, expected: &str, path: &str) {
        let err_prefix = error_prefix(path);
        out.push_str(&format!(
            "if (typeof {accessor} !== \"{expected}\") return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected {expected}, got \" + typeof {accessor}) }}; "
        ));
    }

    #[allow(clippy::unused_self)]
    fn emit_object_check(&self, out: &mut String, accessor: &str, path: &str) {
        let err_prefix = error_prefix(path);
        out.push_str(&format!(
            "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
        ));
    }

    /// A tuple carries a fixed number of elements, so the length is part
    /// of the shape the checker validated.
    #[allow(clippy::unused_self)]
    fn emit_length_check(&self, out: &mut String, accessor: &str, expected: usize, path: &str) {
        let err_prefix = error_prefix(path);
        out.push_str(&format!(
            "if ({accessor}.length !== {expected}) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected {expected} elements, got \" + {accessor}.length) }}; "
        ));
    }

    #[allow(clippy::unused_self)]
    fn emit_array_check(&self, out: &mut String, accessor: &str, path: &str) {
        let err_prefix = error_prefix(path);
        out.push_str(&format!(
            "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
        ));
    }

    // ── Mock codegen ─────────────────────────────────────────────

    pub(super) fn emit_mock(
        &mut self,
        type_arg: &TypedTypeExpr,
        overrides: &[TypedArg],
        counter: &mut usize,
    ) -> Document {
        self.emit_mock_for_type(type_arg, overrides, counter, "")
    }

    #[allow(clippy::too_many_lines)]
    fn emit_mock_for_type(
        &mut self,
        type_expr: &TypedTypeExpr,
        overrides: &[TypedArg],
        counter: &mut usize,
        field_name: &str,
    ) -> Document {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => match name.as_str() {
                "string" => {
                    *counter += 1;
                    let label = if field_name.is_empty() {
                        "string"
                    } else {
                        field_name
                    };
                    pretty::str(format!("\"mock-{label}-{counter}\""))
                }
                "number" => {
                    *counter += 1;
                    pretty::str(format!("{counter}"))
                }
                "boolean" => {
                    let result = if (*counter).is_multiple_of(2) {
                        "true"
                    } else {
                        "false"
                    };
                    *counter += 1;
                    pretty::str(result)
                }
                "Array" => {
                    if let Some(elem_type) = type_args.first() {
                        pretty::concat([
                            pretty::str("["),
                            self.emit_mock_for_type(elem_type, &[], counter, field_name),
                            pretty::str("]"),
                        ])
                    } else {
                        pretty::str("[]")
                    }
                }
                "Option" => {
                    if let Some(inner_type) = type_args.first() {
                        self.emit_mock_for_type(inner_type, &[], counter, field_name)
                    } else {
                        pretty::str("undefined")
                    }
                }
                _ => self.emit_mock_for_name(name, overrides, counter),
            },
            TypeExprKind::Record(fields) => {
                let mut docs = vec![pretty::str("{ ")];
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    let has_override = overrides.iter().find(|arg| {
                        if let Arg::Named { label, .. } = arg {
                            label == &field.name
                        } else {
                            false
                        }
                    });
                    docs.push(pretty::str(format!("{}: ", field.name)));
                    if let Some(Arg::Named { value, .. }) = has_override {
                        docs.push(self.emit_expr(value));
                    } else {
                        docs.push(self.emit_mock_for_type(
                            &field.type_ann,
                            &[],
                            counter,
                            &field.name,
                        ));
                    }
                }
                docs.push(pretty::str(" }"));
                pretty::concat(docs)
            }
            TypeExprKind::Array(inner) => pretty::concat([
                pretty::str("["),
                self.emit_mock_for_type(inner, &[], counter, field_name),
                pretty::str("]"),
            ]),
            TypeExprKind::Tuple(types) => {
                let mut docs = vec![pretty::str("[")];
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    docs.push(self.emit_mock_for_type(ty, &[], counter, ""));
                }
                docs.push(pretty::str("]"));
                pretty::concat(docs)
            }
            TypeExprKind::StringLiteral(value) => pretty::str(format!("\"{value}\"")),
            TypeExprKind::Function { .. }
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => pretty::str(THROW_MOCK_FUNCTION),
        }
    }

    /// Mock a value for a written type name.
    ///
    /// An alias means whatever its right-hand side means, so this asks
    /// `TypeContext::alias_target` for the chain, the same function
    /// `parse<T>` reads. A name that declares its own shape mocks that
    /// shape, and an unknown name mocks an empty object.
    fn emit_mock_for_name(
        &mut self,
        name: &str,
        overrides: &[TypedArg],
        counter: &mut usize,
    ) -> Document {
        if let Some(target) = self.ctx.alias_target(name).cloned() {
            return self.emit_mock_for_type(&target, overrides, counter, name);
        }
        let Some(type_def) = self.ctx.type_defs.get(name).cloned() else {
            return pretty::str("{}");
        };

        self.emit_mock_for_typedef(&type_def, name, overrides, counter)
    }

    #[allow(clippy::too_many_lines)]
    fn emit_mock_for_typedef(
        &mut self,
        type_def: &TypedTypeDef,
        type_name: &str,
        overrides: &[TypedArg],
        counter: &mut usize,
    ) -> Document {
        match type_def {
            TypeDef::Record(entries) => {
                let mut docs = vec![pretty::str("{ ")];
                let mut first = true;
                for entry in entries {
                    match entry {
                        RecordEntry::Field(field) => {
                            if !first {
                                docs.push(pretty::str(", "));
                            }
                            first = false;
                            let has_override = overrides.iter().find(|arg| {
                                if let Arg::Named { label, .. } = arg {
                                    label == &field.name
                                } else {
                                    false
                                }
                            });
                            docs.push(pretty::str(format!("{}: ", field.name)));
                            if let Some(Arg::Named { value, .. }) = has_override {
                                docs.push(self.emit_expr(value));
                            } else {
                                docs.push(self.emit_mock_for_type(
                                    &field.type_ann,
                                    &[],
                                    counter,
                                    &field.name,
                                ));
                            }
                        }
                        RecordEntry::Spread(spread) => {
                            if let Some(TypeDef::Record(spread_entries)) =
                                self.ctx.type_defs.get(&spread.type_name).cloned()
                            {
                                for spread_entry in &spread_entries {
                                    if let RecordEntry::Field(field) = spread_entry {
                                        if !first {
                                            docs.push(pretty::str(", "));
                                        }
                                        first = false;
                                        let has_override = overrides.iter().find(|arg| {
                                            if let Arg::Named { label, .. } = arg {
                                                label == &field.name
                                            } else {
                                                false
                                            }
                                        });
                                        docs.push(pretty::str(format!("{}: ", field.name)));
                                        if let Some(Arg::Named { value, .. }) = has_override {
                                            docs.push(self.emit_expr(value));
                                        } else {
                                            docs.push(self.emit_mock_for_type(
                                                &field.type_ann,
                                                &[],
                                                counter,
                                                &field.name,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                docs.push(pretty::str(" }"));
                pretty::concat(docs)
            }
            TypeDef::Union(variants) => {
                if let Some(variant) = variants.first() {
                    if variant.fields.is_empty() {
                        pretty::str(format!("{{ {TAG_FIELD}: \"{}\" as const }}", variant.name))
                    } else {
                        let mut docs = vec![pretty::str(format!(
                            "{{ {TAG_FIELD}: \"{}\" as const",
                            variant.name
                        ))];
                        for field in &variant.fields {
                            let fname = field
                                .name
                                .clone()
                                .unwrap_or_else(|| VALUE_FIELD.to_string());
                            docs.push(pretty::str(format!(", {fname}: ")));
                            docs.push(self.emit_mock_for_type(
                                &field.type_ann,
                                &[],
                                counter,
                                &fname,
                            ));
                        }
                        docs.push(pretty::str(" }"));
                        pretty::concat(docs)
                    }
                } else {
                    pretty::str("{}")
                }
            }
            TypeDef::StringLiteralUnion(variants) => {
                if let Some(first) = variants.first() {
                    pretty::str(format!("\"{first}\""))
                } else {
                    pretty::str("\"\"")
                }
            }
            // `emit_mock_for_name` follows an alias chain before it gets
            // here, so this arm only fires for a definition handed in
            // directly, such as the target of a record spread.
            TypeDef::Alias(type_expr) => {
                self.emit_mock_for_type(type_expr, overrides, counter, type_name)
            }
        }
    }
}

/// The `field 'name': ` prefix an error message carries when the check is
/// nested inside a record or an array. The top level has no prefix.
fn error_prefix(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }

    format!("{path}: ")
}

/// The path an element reports, built from the path of the array or the
/// tuple that holds it. `index` is already rendered: an array passes the
/// loop variable inside a string concatenation, and a tuple passes a
/// literal position.
fn element_path(path: &str, index: &str) -> String {
    if path.is_empty() {
        return format!("element [{index}]");
    }

    format!("{path} element [{index}]")
}
