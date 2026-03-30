use super::*;

impl Checker {
    pub(crate) fn check_import(&mut self, decl: &ImportDecl) {
        // Look up resolved symbols for this import source
        let resolved = self.resolved_imports.get(&decl.source).cloned();
        let dts_exports = self.dts_imports.get(&decl.source).cloned();

        for spec in &decl.specifiers {
            let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);

            // Try to find the actual type from resolved imports
            let ty = if let Some(ref resolved) = resolved {
                self.lookup_resolved_symbol(&spec.name, resolved)
            } else if let Some(ref exports) = dts_exports {
                // Look up in .d.ts exports
                exports
                    .iter()
                    .find(|e| e.name == spec.name)
                    .map(|e| interop::wrap_boundary_type(&e.ts_type))
                    .unwrap_or_else(|| Type::Foreign(spec.name.clone()))
            } else {
                // No .fl resolution and no .d.ts — type is foreign to Floe
                Type::Foreign(spec.name.clone())
            };

            self.env.define(effective_name, ty);
            self.unused.defined_sources.insert(
                effective_name.to_string(),
                format!("import from \"{}\"", decl.source),
            );
            self.unused
                .imported_names
                .push((effective_name.to_string(), spec.span));

            // Track untrusted imports (not trusted at module or specifier level).
            // Floe-to-Floe imports (resolved.is_some()) are always trusted.
            if !decl.trusted && !spec.trusted && resolved.is_none() {
                self.untrusted_imports.insert(effective_name.to_string());
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
    pub(crate) fn lookup_resolved_symbol(
        &mut self,
        name: &str,
        resolved: &ResolvedImports,
    ) -> Type {
        // Check type declarations
        for decl in &resolved.type_decls {
            if decl.name == name {
                // The type was already registered in the pre-registration pass,
                // so just look it up from the env
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return ty;
                }
                return Type::Named(name.to_string());
            }
        }

        // Check function declarations
        for func in &resolved.function_decls {
            if func.name == name {
                if let Some(ty) = self.env.lookup(name).cloned() {
                    return ty;
                }
                return Type::Unknown;
            }
        }

        // Check const names
        for const_name in &resolved.const_names {
            if const_name == name {
                return Type::Unknown;
            }
        }

        // Not found in resolved module — fall back to Unknown
        Type::Unknown
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

            let fn_type = Type::Function {
                params: param_types,
                return_type: Box::new(return_type),
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
