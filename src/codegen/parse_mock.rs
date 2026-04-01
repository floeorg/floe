use super::*;

impl Codegen {
    // ── Parse<T> Validation Codegen ─────────────────────────────

    pub(super) fn emit_parse(&mut self, type_arg: &TypeExpr, value: &Expr) {
        // Generate: (() => { const __v = <value>; <checks>; return { ok: true, value: __v as T }; })()
        self.push("(() => { const __v = ");
        self.emit_expr(value);
        self.push("; ");
        self.emit_parse_checks("__v", type_arg, "");
        self.push(&format!(
            "return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: __v as "
        ));
        self.emit_type_expr(type_arg);
        self.push(" }; })()");
    }

    /// Emit validation checks for a given accessor path against a type expression.
    /// `accessor` is the JS expression to check (e.g., "__v", "(__v as any).name").
    /// `path` is a human-readable path for error messages (e.g., "", "field 'name'").
    fn emit_parse_checks(&mut self, accessor: &str, type_expr: &TypeExpr, path: &str) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                match name.as_str() {
                    "string" => {
                        self.emit_typeof_check(accessor, "string", path);
                    }
                    "number" => {
                        self.emit_typeof_check(accessor, "number", path);
                    }
                    "boolean" => {
                        self.emit_typeof_check(accessor, "boolean", path);
                    }
                    "Array" => {
                        // Array.isArray check + element validation
                        let err_prefix = if path.is_empty() {
                            String::new()
                        } else {
                            format!("{path}: ")
                        };
                        self.push(&format!(
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
                            self.push(&format!(
                                "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                            ));
                            self.emit_parse_checks(&elem_accessor, elem_type, &elem_path);
                            self.push("} ");
                        }
                    }
                    "Option" => {
                        // Allow undefined or validate inner type
                        if let Some(inner_type) = type_args.first() {
                            self.push(&format!("if ({accessor} !== undefined) {{ "));
                            self.emit_parse_checks(accessor, inner_type, path);
                            self.push("} ");
                        }
                    }
                    _ => {
                        // Named type — look up in expr_types to find if it's a known record.
                        // For now, just check it's an object (non-null).
                        let err_prefix = if path.is_empty() {
                            String::new()
                        } else {
                            format!("{path}: ")
                        };
                        self.push(&format!(
                            "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                        ));
                    }
                }
            }
            TypeExprKind::Record(fields) => {
                // Check it's an object
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                self.push(&format!(
                    "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                ));
                // Check each field
                for field in fields {
                    let field_accessor = format!("({accessor} as any).{}", field.name);
                    let field_path = if path.is_empty() {
                        format!("field '{}'", field.name)
                    } else {
                        format!("{path}.{}", field.name)
                    };
                    self.emit_parse_checks(&field_accessor, &field.type_ann, &field_path);
                }
            }
            TypeExprKind::Array(inner) => {
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                self.push(&format!(
                    "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
                ));
                let idx_var = format!("__i{}", accessor.len());
                let elem_accessor = format!("{accessor}[{idx_var}]");
                let elem_path = if path.is_empty() {
                    format!("element [\" + {idx_var} + \"]")
                } else {
                    format!("{path} element [\" + {idx_var} + \"]")
                };
                self.push(&format!(
                    "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                ));
                self.emit_parse_checks(&elem_accessor, inner, &elem_path);
                self.push("} ");
            }
            TypeExprKind::StringLiteral(_)
            | TypeExprKind::Function { .. }
            | TypeExprKind::Tuple(_)
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => {
                // Can't validate string literals, functions, tuples, typeof, or intersections at runtime — skip
            }
        }
    }

    fn emit_typeof_check(&mut self, accessor: &str, expected: &str, path: &str) {
        let err_prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}: ")
        };
        self.push(&format!(
            "if (typeof {accessor} !== \"{expected}\") return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected {expected}, got \" + typeof {accessor}) }}; "
        ));
    }

    // ── Mock codegen ─────────────────────────────────────────────

    /// Emit a mock value for the given type expression.
    /// `counter` is used to generate unique sequential values.
    /// `overrides` provides named field overrides from `mock<T>(field: value)`.
    pub(super) fn emit_mock(
        &mut self,
        type_arg: &TypeExpr,
        overrides: &[Arg],
        counter: &mut usize,
    ) {
        self.emit_mock_for_type(type_arg, overrides, counter, "");
    }

    fn emit_mock_for_type(
        &mut self,
        type_expr: &TypeExpr,
        overrides: &[Arg],
        counter: &mut usize,
        field_name: &str,
    ) {
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
                    self.push(&format!("\"mock-{label}-{}\"", counter));
                }
                "number" => {
                    *counter += 1;
                    self.push(&format!("{}", counter));
                }
                "boolean" => {
                    // Alternate true/false based on counter
                    self.push(if (*counter).is_multiple_of(2) {
                        "true"
                    } else {
                        "false"
                    });
                    *counter += 1;
                }
                "Array" => {
                    if let Some(elem_type) = type_args.first() {
                        self.push("[");
                        self.emit_mock_for_type(elem_type, &[], counter, field_name);
                        self.push("]");
                    } else {
                        self.push("[]");
                    }
                }
                "Option" => {
                    // Option<T> → Some(mock<T>) — emit the inner value
                    if let Some(inner_type) = type_args.first() {
                        self.emit_mock_for_type(inner_type, &[], counter, field_name);
                    } else {
                        self.push("undefined");
                    }
                }
                _ => {
                    // Named user type — look up in type_defs
                    if let Some(type_def) = self.type_defs.get(name).cloned() {
                        self.emit_mock_for_typedef(&type_def, name, overrides, counter);
                    } else {
                        // Unknown type — emit empty object
                        self.push("{}");
                    }
                }
            },
            TypeExprKind::Record(fields) => {
                self.push("{ ");
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    // Check if this field has an override
                    let has_override = overrides.iter().find(|arg| {
                        if let Arg::Named { label, .. } = arg {
                            label == &field.name
                        } else {
                            false
                        }
                    });
                    self.push(&format!("{}: ", field.name));
                    if let Some(Arg::Named { value, .. }) = has_override {
                        self.emit_expr(value);
                    } else {
                        self.emit_mock_for_type(&field.type_ann, &[], counter, &field.name);
                    }
                }
                self.push(" }");
            }
            TypeExprKind::Array(inner) => {
                self.push("[");
                self.emit_mock_for_type(inner, &[], counter, field_name);
                self.push("]");
            }
            TypeExprKind::Tuple(types) => {
                self.push("[");
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_mock_for_type(ty, &[], counter, "");
                }
                self.push("]");
            }
            TypeExprKind::StringLiteral(value) => {
                self.push(&format!("\"{value}\""));
            }
            TypeExprKind::Function { .. }
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => {
                self.push(THROW_MOCK_FUNCTION);
            }
        }
    }

    fn emit_mock_for_typedef(
        &mut self,
        type_def: &TypeDef,
        type_name: &str,
        overrides: &[Arg],
        counter: &mut usize,
    ) {
        match type_def {
            TypeDef::Record(entries) => {
                self.push("{ ");
                let mut first = true;
                for entry in entries {
                    match entry {
                        RecordEntry::Field(field) => {
                            if !first {
                                self.push(", ");
                            }
                            first = false;
                            let has_override = overrides.iter().find(|arg| {
                                if let Arg::Named { label, .. } = arg {
                                    label == &field.name
                                } else {
                                    false
                                }
                            });
                            self.push(&format!("{}: ", field.name));
                            if let Some(Arg::Named { value, .. }) = has_override {
                                self.emit_expr(value);
                            } else {
                                self.emit_mock_for_type(&field.type_ann, &[], counter, &field.name);
                            }
                        }
                        RecordEntry::Spread(spread) => {
                            // Spread in mock: recursively mock the spread type
                            if let Some(TypeDef::Record(spread_entries)) =
                                self.type_defs.get(&spread.type_name).cloned()
                            {
                                for spread_entry in &spread_entries {
                                    if let RecordEntry::Field(field) = spread_entry {
                                        if !first {
                                            self.push(", ");
                                        }
                                        first = false;
                                        let has_override = overrides.iter().find(|arg| {
                                            if let Arg::Named { label, .. } = arg {
                                                label == &field.name
                                            } else {
                                                false
                                            }
                                        });
                                        self.push(&format!("{}: ", field.name));
                                        if let Some(Arg::Named { value, .. }) = has_override {
                                            self.emit_expr(value);
                                        } else {
                                            self.emit_mock_for_type(
                                                &field.type_ann,
                                                &[],
                                                counter,
                                                &field.name,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.push(" }");
            }
            TypeDef::Union(variants) => {
                // Pick first variant
                if let Some(variant) = variants.first() {
                    if variant.fields.is_empty() {
                        // Unit variant
                        self.push(&format!("{{ {TAG_FIELD}: \"{}\" as const }}", variant.name));
                    } else {
                        self.push(&format!("{{ {TAG_FIELD}: \"{}\" as const", variant.name));
                        for field in &variant.fields {
                            let fname = field.name.clone().unwrap_or_else(|| "value".to_string());
                            self.push(&format!(", {fname}: "));
                            self.emit_mock_for_type(&field.type_ann, &[], counter, &fname);
                        }
                        self.push(" }");
                    }
                } else {
                    self.push("{}");
                }
            }
            TypeDef::StringLiteralUnion(variants) => {
                // Pick first variant
                if let Some(first) = variants.first() {
                    self.push(&format!("\"{first}\""));
                } else {
                    self.push("\"\"");
                }
            }
            TypeDef::Alias(type_expr) => {
                // Newtype: mock the inner type
                self.emit_mock_for_type(type_expr, overrides, counter, type_name);
            }
        }
    }
}
