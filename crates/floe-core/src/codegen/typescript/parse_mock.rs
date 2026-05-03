use crate::parser::ast::{
    Arg, RecordEntry, TypeDef, TypeExprKind, TypedArg, TypedExpr, TypedTypeDef, TypedTypeExpr,
};
use crate::pretty::{self, Document};
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

use super::expression::expr_contains_await;
use super::generator::{THROW_MOCK_FUNCTION, TypeScriptGenerator};

impl<'a> TypeScriptGenerator<'a> {
    // ── Parse<T> Validation Codegen ─────────────────────────────

    pub(super) fn emit_parse(&mut self, type_arg: &TypedTypeExpr, value: &TypedExpr) -> Document {
        let value_doc = self.emit_expr(value);
        let mut checks = String::new();
        self.emit_parse_checks(&mut checks, "__v", type_arg, "");
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

    fn emit_parse_checks(
        &mut self,
        out: &mut String,
        accessor: &str,
        type_expr: &TypedTypeExpr,
        path: &str,
    ) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => match name.as_str() {
                "string" => self.emit_typeof_check(out, accessor, "string", path),
                "number" => self.emit_typeof_check(out, accessor, "number", path),
                "boolean" => self.emit_typeof_check(out, accessor, "boolean", path),
                "Array" => {
                    let err_prefix = if path.is_empty() {
                        String::new()
                    } else {
                        format!("{path}: ")
                    };
                    out.push_str(&format!(
                        "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
                    ));
                    if let Some(elem_type) = type_args.first() {
                        let idx_var = format!("__i{}", accessor.len());
                        let elem_accessor = format!("{accessor}[{idx_var}]");
                        let elem_path = if path.is_empty() {
                            format!("element [\" + {idx_var} + \"]")
                        } else {
                            format!("{path} element [\" + {idx_var} + \"]")
                        };
                        out.push_str(&format!(
                            "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                        ));
                        self.emit_parse_checks(out, &elem_accessor, elem_type, &elem_path);
                        out.push_str("} ");
                    }
                }
                "Option" => {
                    if let Some(inner_type) = type_args.first() {
                        out.push_str(&format!("if ({accessor} !== undefined) {{ "));
                        self.emit_parse_checks(out, accessor, inner_type, path);
                        out.push_str("} ");
                    }
                }
                _ => {
                    let err_prefix = if path.is_empty() {
                        String::new()
                    } else {
                        format!("{path}: ")
                    };
                    out.push_str(&format!(
                        "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                    ));
                }
            },
            TypeExprKind::Record(fields) => {
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                out.push_str(&format!(
                    "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                ));
                for field in fields {
                    let field_accessor = format!("({accessor} as any).{}", field.name);
                    let field_path = if path.is_empty() {
                        format!("field '{}'", field.name)
                    } else {
                        format!("{path}.{}", field.name)
                    };
                    self.emit_parse_checks(out, &field_accessor, &field.type_ann, &field_path);
                }
            }
            TypeExprKind::Array(inner) => {
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                out.push_str(&format!(
                    "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
                ));
                let idx_var = format!("__i{}", accessor.len());
                let elem_accessor = format!("{accessor}[{idx_var}]");
                let elem_path = if path.is_empty() {
                    format!("element [\" + {idx_var} + \"]")
                } else {
                    format!("{path} element [\" + {idx_var} + \"]")
                };
                out.push_str(&format!(
                    "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                ));
                self.emit_parse_checks(out, &elem_accessor, inner, &elem_path);
                out.push_str("} ");
            }
            TypeExprKind::StringLiteral(_)
            | TypeExprKind::Function { .. }
            | TypeExprKind::Tuple(_)
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => {
                // Cannot validate at runtime
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn emit_typeof_check(&self, out: &mut String, accessor: &str, expected: &str, path: &str) {
        let err_prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}: ")
        };
        out.push_str(&format!(
            "if (typeof {accessor} !== \"{expected}\") return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected {expected}, got \" + typeof {accessor}) }}; "
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
                _ => {
                    if let Some(type_def) = self.ctx.type_defs.get(name).cloned() {
                        self.emit_mock_for_typedef(&type_def, name, overrides, counter)
                    } else {
                        pretty::str("{}")
                    }
                }
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
            TypeDef::Alias(type_expr) => {
                self.emit_mock_for_type(type_expr, overrides, counter, type_name)
            }
        }
    }
}
