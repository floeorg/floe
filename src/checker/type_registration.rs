use super::*;

impl Checker {
    pub(crate) fn register_type_decl(&mut self, decl: &TypeDecl, span: Span) {
        // Enforce naming conventions
        if span.start != 0 || span.end != 0 {
            // Only check local declarations (not imports with dummy spans)
            if decl.name.starts_with(char::is_lowercase) {
                self.emit_error_with_help(
                    format!(
                        "type name `{}` must start with an uppercase letter",
                        decl.name
                    ),
                    span,
                    ErrorCode::TypeNameCase,
                    "must be uppercase",
                    format!(
                        "rename to `{}{}`",
                        decl.name[..1].to_uppercase(),
                        &decl.name[1..]
                    ),
                );
            }
            match &decl.def {
                TypeDef::Union(variants) => {
                    for variant in variants {
                        if variant.name.starts_with(char::is_lowercase) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "variant name `{}` must start with an uppercase letter",
                                        variant.name
                                    ),
                                    variant.span,
                                )
                                .with_help(format!(
                                    "rename to `{}{}`",
                                    variant.name[..1].to_uppercase(),
                                    &variant.name[1..]
                                ))
                                .with_error_code(ErrorCode::TypeNameCase),
                            );
                        }
                    }
                }
                // Record field names: uppercase fields are already rejected by the parser
                // (uppercase identifiers are parsed as types/variants, not field names)
                TypeDef::Record(_) => {}
                _ => {}
            }

            // Reject & intersection in { } type definitions (records/unions).
            // & should only appear in = type aliases (TS bridge types).
            if matches!(decl.def, TypeDef::Record(_) | TypeDef::Union(_)) {
                self.check_no_intersection_in_type_def(&decl.def, span);
            }

            // Reject bridge type syntax (= ...) that doesn't reference any TS import.
            // String literal unions never reference TS types, so always error.
            // Aliases must reference at least one foreign/npm type.
            // Opaque types are exempt — `opaque type X = T` is valid Floe syntax.
            let bridge_help = match &decl.def {
                TypeDef::StringLiteralUnion(_) if !decl.opaque => {
                    Some("use a union type instead: `type Name { | Variant1 | Variant2 }`")
                }
                TypeDef::Alias(type_expr)
                    if !decl.opaque && !self.type_expr_references_foreign(type_expr) =>
                {
                    Some(
                        "`type Name = ...` is for TypeScript interop only — use `type Name { }` for Floe-native types",
                    )
                }
                _ => None,
            };
            if let Some(help) = bridge_help {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "bridge type `{}` uses `=` syntax but doesn't reference any TypeScript import",
                            decl.name
                        ),
                        span,
                    )
                    .with_help(help)
                    .with_error_code(ErrorCode::BridgeTypeWithoutImport),
                );
            }
        }

        // Flatten record spreads into a flat record definition
        let flattened_def = self.flatten_record_spreads(&decl.def, &decl.name);

        let info = TypeInfo {
            def: flattened_def.clone(),
            opaque: decl.opaque,
            type_params: decl.type_params.clone(),
        };
        self.env.define_type(&decl.name, info);

        match &flattened_def {
            TypeDef::Record(entries) => {
                // If the record has unresolved spreads (foreign generic types),
                // register the probe type in the value namespace so member access
                // resolves through the foreign type system. Plain record type names
                // are not runtime values and stay out of the value namespace.
                let has_unresolved_spreads =
                    entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
                if has_unresolved_spreads && let Some(probe_ty) = self.find_type_probe(&decl.name) {
                    self.env.define(&decl.name, probe_ty);
                }

                // Populate __field_ entries for dot-access completions
                for entry in entries {
                    if let RecordEntry::Field(field) = entry {
                        let field_ty = self.resolve_type(&field.type_ann);
                        self.name_types.insert(
                            format!("__field_{}_{}", decl.name, field.name),
                            field_ty.to_string(),
                        );
                    }
                }
            }
            TypeDef::Union(variants) => {
                let var_types: Vec<_> = variants
                    .iter()
                    .map(|v| {
                        let field_types: Vec<_> = v
                            .fields
                            .iter()
                            .map(|f| self.resolve_type(&f.type_ann))
                            .collect();
                        (v.name.clone(), field_types)
                    })
                    .collect();
                let union_type = Type::Union {
                    name: decl.name.clone(),
                    variants: var_types.clone(),
                };
                // Union type names are not runtime values — only variants are.
                // Register each variant constructor and track ambiguity.
                for (vname, _) in &var_types {
                    // Check if this variant name is already defined by another union
                    if let Some(existing) = self.env.lookup(vname)
                        && let Type::Union {
                            name: existing_union,
                            ..
                        } = existing
                        && *existing_union != decl.name
                    {
                        let existing_union = existing_union.clone();
                        self.ambiguous_variants
                            .entry(vname.clone())
                            .or_insert_with(|| vec![existing_union])
                            .push(decl.name.clone());
                    }
                    self.env.define(vname, union_type.clone());
                }
            }
            TypeDef::StringLiteralUnion(_) => {
                // String literal union names are not runtime values — nothing to register
                // in the value namespace.
            }
            TypeDef::Alias(type_expr) => {
                let mut ty = self.resolve_type(type_expr);
                // If tsgo resolved a type probe for this alias, use it.
                // This handles conditional/mapped types from npm packages
                // (e.g. VariantProps<T> which uses Extract<...> internally).
                if let Some(probe_ty) = self.find_type_probe(&decl.name) {
                    ty = probe_ty;
                }
                self.env.define(&decl.name, ty);
            }
        }
    }

    /// Search dts_imports for a tsgo type probe matching the given type alias name.
    pub(crate) fn find_type_probe(&self, type_name: &str) -> Option<Type> {
        let probe_key = format!("__tprobe_{type_name}");
        for exports in self.dts_imports.values() {
            for export in exports {
                if export.name == probe_key {
                    return Some(interop::wrap_boundary_type(&export.ts_type));
                }
            }
        }
        None
    }

    /// Check whether a type expression references any foreign/npm type.
    /// Returns `true` if at least one named type in the expression is exported
    /// from a .d.ts package (found in `dts_imports`) or resolves to `Type::Foreign`
    /// in the environment.
    pub(crate) fn type_expr_references_foreign(&self, type_expr: &TypeExpr) -> bool {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                if self.is_dts_export_name(name) {
                    return true;
                }
                if let Some(ty) = self.env.lookup(name)
                    && matches!(ty, Type::Foreign(_))
                {
                    return true;
                }
                type_args
                    .iter()
                    .any(|a| self.type_expr_references_foreign(a))
            }
            TypeExprKind::Intersection(parts) => {
                parts.iter().any(|p| self.type_expr_references_foreign(p))
            }
            TypeExprKind::Array(inner) => self.type_expr_references_foreign(inner),
            TypeExprKind::Tuple(parts) => {
                parts.iter().any(|p| self.type_expr_references_foreign(p))
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                params.iter().any(|p| self.type_expr_references_foreign(p))
                    || self.type_expr_references_foreign(return_type)
            }
            TypeExprKind::Record(fields) => fields
                .iter()
                .any(|f| self.type_expr_references_foreign(&f.type_ann)),
            TypeExprKind::TypeOf(_) | TypeExprKind::StringLiteral(_) => false,
        }
    }

    /// Check if a name is an export from any .d.ts package.
    fn is_dts_export_name(&self, name: &str) -> bool {
        self.dts_imports
            .values()
            .any(|exports| exports.iter().any(|e| e.name == name))
    }

    /// Check that `&` intersection types don't appear in `{ }` type definitions.
    pub(crate) fn check_no_intersection_in_type_def(&mut self, def: &TypeDef, span: Span) {
        fn has_intersection(ty: &TypeExpr) -> bool {
            match &ty.kind {
                TypeExprKind::Intersection(_) => true,
                TypeExprKind::Array(inner) => has_intersection(inner),
                TypeExprKind::Tuple(parts) => parts.iter().any(has_intersection),
                TypeExprKind::Function {
                    params,
                    return_type,
                } => params.iter().any(has_intersection) || has_intersection(return_type),
                TypeExprKind::Named { type_args, .. } => type_args.iter().any(has_intersection),
                TypeExprKind::Record(fields) => {
                    fields.iter().any(|f| has_intersection(&f.type_ann))
                }
                _ => false,
            }
        }

        let found = match def {
            TypeDef::Record(entries) => entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| &f.type_ann))
                .any(has_intersection),
            TypeDef::Union(variants) => variants
                .iter()
                .flat_map(|v| v.fields.iter().map(|f| &f.type_ann))
                .any(has_intersection),
            _ => return,
        };

        if found {
            self.diagnostics.push(
                Diagnostic::error(
                    "`&` intersection types cannot be used in `{ }` type definitions".to_string(),
                    span,
                )
                .with_help("use `...Spread` for record composition, or `=` for TS interop types")
                .with_error_code(ErrorCode::InvalidEnumSpread),
            );
        }
    }

    /// Flatten record type spreads (`...OtherType`) into regular fields.
    /// Returns the original `TypeDef` unchanged if it's not a record or has no spreads.
    pub(crate) fn flatten_record_spreads(&mut self, def: &TypeDef, type_name: &str) -> TypeDef {
        let entries = match def {
            TypeDef::Record(entries) => entries,
            other => return other.clone(),
        };

        // Check if there are any spreads at all
        let has_spreads = entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
        if !has_spreads {
            return def.clone();
        }

        let mut flat_fields: Vec<RecordField> = Vec::new();
        let mut preserved_spreads: Vec<RecordEntry> = Vec::new();
        let mut seen_names: std::collections::HashMap<String, Span> =
            std::collections::HashMap::new();

        for entry in entries {
            match entry {
                RecordEntry::Field(field) => {
                    if seen_names.contains_key(&field.name) {
                        self.emit_error_with_help(
                            format!(
                                "duplicate field `{}` in record type `{}`",
                                field.name, type_name
                            ),
                            field.span,
                            ErrorCode::DuplicateField,
                            "duplicate field",
                            "field was already defined elsewhere in this record type",
                        );
                    } else {
                        seen_names.insert(field.name.clone(), field.span);
                        flat_fields.push(field.as_ref().clone());
                    }
                }
                RecordEntry::Spread(spread) => {
                    // Look up the referenced type
                    if let Some(info) = self.env.lookup_type(&spread.type_name) {
                        let info = info.clone();
                        match &info.def {
                            TypeDef::Record(spread_entries) => {
                                // Get only the direct fields from the spread target
                                // (which should already be flattened if it was registered first)
                                let spread_fields: Vec<RecordField> = spread_entries
                                    .iter()
                                    .filter_map(|e| e.as_field().cloned())
                                    .collect();
                                for field in &spread_fields {
                                    if seen_names.contains_key(&field.name) {
                                        self.emit_error_with_help(
                                            format!(
                                                    "field `{}` from spread `...{}` conflicts with existing field in `{}`",
                                                    field.name, spread.type_name, type_name
                                                ),
                                            spread.span,
                                            ErrorCode::SpreadFieldConflict,
                                            format!("field `{}` already defined", field.name),
                                            "field was already defined elsewhere in this record type",
                                        );
                                    } else {
                                        seen_names.insert(field.name.clone(), spread.span);
                                        flat_fields.push(field.clone());
                                    }
                                }
                            }
                            TypeDef::Union(_) => {
                                self.emit_error(
                                    format!(
                                        "cannot spread union type `{}` into record type `{}`",
                                        spread.type_name, type_name
                                    ),
                                    spread.span,
                                    ErrorCode::InvalidSpreadType,
                                    "spread target must be a record type",
                                );
                            }
                            TypeDef::Alias(_) | TypeDef::StringLiteralUnion(_) => {
                                // If the alias is a foreign type, preserve the spread for codegen
                                preserved_spreads.push(RecordEntry::Spread(spread.clone()));
                            }
                        }
                    } else if spread.type_expr.is_some() {
                        // Foreign/generic type not in local env — preserve for codegen
                        // (e.g. ...VariantProps<typeof cardVariants>)
                        self.unused.used_names.insert(spread.type_name.clone());
                        preserved_spreads.push(RecordEntry::Spread(spread.clone()));
                    } else {
                        self.emit_error(
                            format!("unknown type `{}` in spread", spread.type_name),
                            spread.span,
                            ErrorCode::UndefinedName,
                            "type not found",
                        );
                    }
                }
            }
        }

        let mut result_entries: Vec<RecordEntry> = preserved_spreads;
        result_entries.extend(
            flat_fields
                .into_iter()
                .map(|f| RecordEntry::Field(Box::new(f))),
        );

        TypeDef::Record(result_entries)
    }

    /// Second-pass validation of type annotations within type declarations.
    /// The first pass (register_type_decl) skips unknown type errors for forward references.
    pub(crate) fn validate_type_decl_annotations(&mut self, decl: &TypeDecl) {
        match &decl.def {
            TypeDef::Record(entries) => {
                let mut seen_default = false;
                for entry in entries {
                    if let RecordEntry::Field(field) = entry {
                        let field_ty = self.resolve_type(&field.type_ann);
                        if let Some(ref default_expr) = field.default {
                            seen_default = true;
                            let default_ty = self.check_expr(default_expr);
                            if !self.types_compatible(&field_ty, &default_ty) {
                                self.emit_error(
                                    format!(
                                        "default value for `{}`: expected `{}`, found `{}`",
                                        field.name, field_ty, default_ty
                                    ),
                                    field.span,
                                    ErrorCode::TypeMismatch,
                                    format!("expected `{}`", field_ty),
                                );
                            }
                        } else if seen_default {
                            self.emit_error(
                                format!(
                                    "required field `{}` must come before fields with defaults",
                                    field.name
                                ),
                                field.span,
                                ErrorCode::TypeMismatch,
                                "move this field before defaulted fields",
                            );
                        }
                    }
                    // Spreads are validated during register_type_decl
                }
            }
            TypeDef::Union(variants) => {
                for variant in variants {
                    for field in &variant.fields {
                        self.resolve_type(&field.type_ann);
                    }
                }
            }
            TypeDef::StringLiteralUnion(_) => {
                // No type annotations to validate
            }
            TypeDef::Alias(type_expr) => {
                let ty = self.resolve_type(type_expr);
                // typeof aliases resolved to Unknown in the first pass (bindings
                // weren't registered yet). Now that bindings exist, update the env.
                if matches!(type_expr.kind, TypeExprKind::TypeOf(_)) {
                    self.env.define(&decl.name, ty);
                }
            }
        }

        // Validate and register deriving clause
        if !decl.deriving.is_empty() {
            self.check_deriving(decl);
        }
    }

    /// Validate a `deriving` clause and register the derived functions.
    pub(crate) fn check_deriving(&mut self, decl: &TypeDecl) {
        let span = Span::new(0, 0, 0, 0); // deriving doesn't have its own span yet

        // deriving only works on record types
        if !matches!(&decl.def, TypeDef::Record(_)) {
            self.emit_error_with_help(
                format!(
                    "`deriving` can only be used on record types, but `{}` is not a record",
                    decl.name
                ),
                span,
                ErrorCode::InvalidDerive,
                "not a record type",
                "remove the `deriving` clause or change this to a record type",
            );
            return;
        }

        let type_name = &decl.name;

        for trait_name in &decl.deriving {
            match trait_name.as_str() {
                "Eq" => {
                    self.emit_error_with_help(
                        "`Eq` cannot be derived — structural equality is built-in for all types via `==`".to_string(),
                        span,
                        ErrorCode::InvalidDerive,
                        "not needed",
                        "remove `Eq` from the deriving clause — use `==` for equality comparison",
                    );
                }
                "Display" => {
                    // Register display function: fn display(self) -> string
                    let fn_name = "display".to_string();
                    let self_type = Type::Named(type_name.clone());
                    let fn_type = Type::Function {
                        params: vec![self_type],
                        return_type: Box::new(Type::String),
                        required_params: 1,
                    };
                    self.env.define(&fn_name, fn_type);
                    self.unused
                        .defined_sources
                        .insert(fn_name.clone(), format!("derived Display for {type_name}"));
                    self.unused.used_names.insert(fn_name.clone());
                    self.traits
                        .trait_impls
                        .insert((type_name.clone(), "Display".to_string()));
                }
                _ => {
                    self.emit_error_with_help(
                        format!("trait `{trait_name}` cannot be derived"),
                        span,
                        ErrorCode::InvalidDerive,
                        "not a derivable trait",
                        "only `Display` can be derived",
                    );
                }
            }

            // Mark the trait name as used
            self.unused.used_names.insert(trait_name.clone());
        }
    }
}
