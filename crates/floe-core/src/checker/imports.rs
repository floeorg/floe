use super::*;

impl Checker {
    pub(crate) fn check_import(&mut self, decl: &ImportDecl, item_span: Span) {
        // If this import targets a .ts/.tsx file and tsgo is not installed,
        // emit a hard error instead of silently falling back to unknown types.
        if self.ts_imports_missing_tsgo.contains(&decl.source) {
            self.emit_error_with_help(
                "tsgo is required to resolve TypeScript imports",
                item_span,
                ErrorCode::TsgoNotFound,
                "cannot resolve types without tsgo",
                "install with: npm i -g @typescript/native-preview",
            );
            return;
        }

        // Look up resolved symbols for this import source
        let resolved = self.resolved_imports.get(&decl.source).cloned();
        let dts_exports = self.dts_imports.get(&decl.source).cloned();

        // Handle default import: `import X from "module"`
        if let Some(ref default_name) = decl.default_import {
            let ty = if let Some(ref exports) = dts_exports {
                if let Some(dts_export) = exports.iter().find(|e| e.name == "default") {
                    interop::wrap_boundary_type(&dts_export.ts_type)
                } else {
                    Type::Foreign(default_name.clone())
                }
            } else {
                Type::Foreign(default_name.clone())
            };
            self.env.define(default_name, ty);
            self.unused.defined_sources.insert(
                default_name.clone(),
                format!("import from \"{}\"", decl.source),
            );
            self.unused.imported_names.push((
                default_name.clone(),
                Span {
                    start: 0,
                    end: 0,
                    line: 0,
                    column: 0,
                },
            ));
        }

        for spec in &decl.specifiers {
            let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);

            // Try to find the actual type from resolved imports
            let ty = if let Some(ref resolved) = resolved {
                match self.lookup_resolved_symbol(&spec.name, resolved) {
                    Some(ty) => ty,
                    None => {
                        self.emit_error(
                            format!(
                                "module \"{}\" has no export named `{}`",
                                decl.source, spec.name
                            ),
                            spec.span,
                            ErrorCode::ExportNotFound,
                            "not found in module",
                        );
                        Type::Error
                    }
                }
            } else if let Some(ref exports) = dts_exports {
                // Look up in .d.ts exports
                if let Some(dts_export) = exports.iter().find(|e| e.name == spec.name) {
                    if let interop::TsType::Function { params, .. } = &dts_export.ts_type {
                        let required = params.iter().filter(|p| !p.optional).count();
                        if required < params.len() {
                            self.fn_required_params
                                .insert(effective_name.to_string(), required);
                        }
                    }
                    let ty = interop::wrap_boundary_type(&dts_export.ts_type);
                    // npm imports that resolve to Unknown (unrecognized primitive,
                    // type-only exports, overloaded signatures tsgo can't map)
                    // should fall back to Foreign. They're at an explicit npm
                    // boundary — Foreign produces a warning on call, while Unknown
                    // would produce an error.
                    if matches!(ty, Type::Unknown) {
                        Type::Foreign(spec.name.clone())
                    } else {
                        ty
                    }
                } else {
                    self.emit_error(
                        format!(
                            "module \"{}\" has no export named `{}`",
                            decl.source, spec.name
                        ),
                        spec.span,
                        ErrorCode::ExportNotFound,
                        "not found in module",
                    );
                    Type::Error
                }
            } else {
                // No .fl resolution and no .d.ts — type is foreign to Floe
                Type::Foreign(spec.name.clone())
            };

            // Check for duplicate import names (#812). We check imported_names
            // rather than the full scope because resolved imports pre-register
            // types before import statements are processed.
            if self
                .unused
                .imported_names
                .iter()
                .any(|(name, _)| name == effective_name)
            {
                self.emit_error(
                    format!("`{effective_name}` is already defined in this scope"),
                    spec.span,
                    ErrorCode::DuplicateDefinition,
                    "already defined",
                );
            }
            self.env.define(effective_name, ty);
            self.unused.defined_sources.insert(
                effective_name.to_string(),
                format!("import from \"{}\"", decl.source),
            );
            self.unused
                .imported_names
                .push((effective_name.to_string(), spec.span));

            // Track npm imports (resolved.is_none() means not a .fl file).
            if resolved.is_none() {
                self.npm_imports.insert(effective_name.to_string());
                // Track untrusted imports (not marked `trusted` at module or specifier level).
                if !decl.trusted && !spec.trusted {
                    self.untrusted_imports.insert(effective_name.to_string());
                }
            }
        }

        // Auto-import for-blocks when importing a type from the same file
        // (importing a type brings its for-block functions from that file)
        if let Some(ref resolved) = resolved {
            for spec in &decl.specifiers {
                // Check if this specifier is a type in the resolved module
                let is_type = resolved.type_decls.iter().any(|d| d.name == spec.name);
                if is_type {
                    for block in &resolved.for_blocks {
                        let base_type_name = match &block.type_name.kind {
                            TypeExprKind::Named { name, .. } => name.clone(),
                            _ => continue,
                        };
                        if base_type_name == spec.name {
                            self.check_for_block_imported_with_source(block, &decl.source);
                        }
                    }
                }
            }
        }

        // Handle `for Type` import specifiers (cross-file for-blocks)
        if !decl.for_specifiers.is_empty()
            && let Some(ref resolved) = resolved
        {
            for for_spec in &decl.for_specifiers {
                // Find all for-blocks in the resolved module that match this type
                for block in &resolved.for_blocks {
                    let base_type_name = match &block.type_name.kind {
                        TypeExprKind::Named { name, .. } => name.clone(),
                        _ => continue,
                    };
                    if base_type_name == for_spec.type_name {
                        self.check_for_block_imported_with_source(block, &decl.source);
                        // Mark the for-import functions as used (suppress unused import)
                        for func in &block.functions {
                            self.unused.used_names.insert(func.name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Look up a symbol name in resolved imports and return its type.
    /// Returns `None` if the name is not exported by the module.
    pub(crate) fn lookup_resolved_symbol(
        &mut self,
        name: &str,
        resolved: &ResolvedImports,
    ) -> Option<Type> {
        // Check type declarations
        for decl in &resolved.type_decls {
            if decl.name == name {
                // The type was already registered in the pre-registration pass,
                // so just look it up from the env
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return Some(ty);
                }
                return Some(Type::Named(name.to_string()));
            }
        }

        // Check function declarations
        for func in &resolved.function_decls {
            if func.name == name {
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return Some(ty);
                }
                return Some(Type::Unknown);
            }
        }

        // Check const names
        for const_name in &resolved.const_names {
            if const_name == name {
                return Some(Type::Unknown);
            }
        }

        // Check trait declarations (traits are registered during pre-registration,
        // but we still need to recognize them here to avoid false ExportNotFound errors)
        for trait_decl in &resolved.trait_decls {
            if trait_decl.name == name {
                return Some(Type::Named(name.to_string()));
            }
        }

        // Not found in resolved module
        None
    }

    /// Register for-block functions from an imported module without checking bodies.
    pub(crate) fn check_for_block_imported_with_source(&mut self, block: &ForBlock, source: &str) {
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => String::new(),
        };

        // Ensure the for-block's target type is defined before resolving.
        // The type may be foreign (from npm/TS, not defined in Floe).
        if !type_name.is_empty() && self.env.lookup(&type_name).is_none() {
            self.env
                .define(&type_name, Type::Foreign(type_name.clone()));
        }

        let for_type = self.resolve_type(&block.type_name);

        for func in &block.functions {
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t))
                .unwrap_or(Type::Unknown);

            let param_types: Vec<_> = func
                .params
                .iter()
                .map(|p| {
                    if p.name == "self" {
                        for_type.clone()
                    } else {
                        p.type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or(Type::Unknown)
                    }
                })
                .collect();

            let required_params = param_types.len();
            let fn_type = Type::Function {
                params: param_types,
                return_type: Box::new(return_type),
                required_params,
            };
            self.env.define(&func.name, fn_type.clone());
            self.unused.defined_sources.insert(
                func.name.clone(),
                format!("for-block function from \"{}\"", source),
            );
            self.for_block_overloads
                .entry(func.name.clone())
                .or_default()
                .push((type_name.clone(), fn_type));

            // Track required (non-default) parameter count
            let required_params = func.params.iter().filter(|p| p.default.is_none()).count();
            if required_params < func.params.len() {
                self.fn_required_params
                    .insert(func.name.clone(), required_params);
            }

            // Track parameter names for named argument validation
            self.fn_param_names.insert(
                func.name.clone(),
                func.params.iter().map(|p| p.name.clone()).collect(),
            );
        }
    }
}
