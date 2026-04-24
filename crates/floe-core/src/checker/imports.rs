use std::sync::Arc;

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
        // Trust follows the `import` declaration and specifier — trusted unless
        // the file is an npm source that wasn't explicitly marked `trusted`.
        let is_npm = !decl.source.starts_with("./") && !decl.source.starts_with("../");
        let default_untrusted = is_npm && !decl.trusted;
        if let Some(ref default_name) = decl.default_import {
            let ty = if let Some(ref exports) = dts_exports {
                if let Some(dts_export) = exports.iter().find(|e| e.name == "default") {
                    let raw = interop::wrap_boundary_type(&dts_export.ts_type);
                    mark_foreign_untrusted(raw, default_untrusted)
                } else if default_untrusted {
                    Type::untrusted_foreign(default_name.clone())
                } else {
                    Type::foreign(default_name.clone())
                }
            } else if default_untrusted {
                Type::untrusted_foreign(default_name.clone())
            } else {
                Type::foreign(default_name.clone())
            };
            self.env.define_with_span(default_name, ty, item_span);
            self.references.register_definition(default_name, item_span);
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
            // Per-specifier trust: npm source without a `trusted` marker at
            // either module or specifier level flows the untrusted bit into
            // the resulting type.
            let spec_untrusted = is_npm && !decl.trusted && !spec.trusted;

            // Trait imports: plain `import { Display }` now activates
            // every `impl Display for T { ... }` in the import's transitive
            // closure. No separate `import { for Display }` needed.
            if let Some(ref resolved) = resolved
                && let Some(trait_decl) = resolved.trait_decls.iter().find(|t| t.name == spec.name)
            {
                self.activate_trait_impls(trait_decl, resolved, &decl.source);
                continue;
            }

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
                if let Some(dts_export) = exports.iter().find(|e| e.name == spec.name) {
                    if let interop::TsType::Function { params, .. } = &dts_export.ts_type {
                        let required = params.iter().filter(|p| !p.optional).count();
                        if required < params.len() {
                            self.fn_required_params
                                .insert(effective_name.to_string(), required);
                        }
                    }
                    let raw_ty = interop::wrap_boundary_type(&dts_export.ts_type);
                    // Hydrate single-letter type params (`T`, `S`, …) into Generic
                    // vars so the imported signature participates in real HM
                    // unification at call sites instead of string-matching by letter.
                    let ty = super::hydrator::hydrate_single_letter_generics(
                        &raw_ty,
                        &mut self.next_var,
                    );
                    // npm imports that resolve to Unknown (unrecognized primitive,
                    // type-only exports, overloaded signatures tsgo can't map)
                    // should fall back to Foreign. They're at an explicit npm
                    // boundary — Foreign produces a warning on call, while Unknown
                    // would produce an error.
                    let resolved = if matches!(ty, Type::Unknown) {
                        if spec_untrusted {
                            Type::untrusted_foreign(spec.name.clone())
                        } else {
                            Type::foreign(spec.name.clone())
                        }
                    } else {
                        ty
                    };
                    mark_foreign_untrusted(resolved, spec_untrusted)
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
            } else if spec_untrusted {
                Type::untrusted_foreign(spec.name.clone())
            } else {
                Type::foreign(spec.name.clone())
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
            self.env.define_with_span(effective_name, ty, spec.span);
            self.references
                .register_definition(effective_name, spec.span);
            self.unused.defined_sources.insert(
                effective_name.to_string(),
                format!("import from \"{}\"", decl.source),
            );
            self.unused
                .imported_names
                .push((effective_name.to_string(), spec.span));

            if resolved.is_none() {
                self.npm_imports.insert(effective_name.to_string());
                if spec_untrusted {
                    // The checker side-table is still used for diagnostics
                    // that identify by name (e.g. detecting chain propagation).
                    // Codegen no longer reads from here — trust travels on the
                    // type itself via `Type::Foreign { untrusted }`.
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

            // Also activate any trait impls from this module whose trait is
            // already in scope. A trait impl lives either with the trait or
            // with the type; importing either makes the impl visible, so we
            // pick up impls from any imported module where the trait has been
            // registered. Deduped against `impl_sources` so the same impl
            // reached via multiple specifiers (trait + type from same module)
            // doesn't double-register.
            for block in &resolved.for_blocks {
                if block.trait_name.is_some() {
                    self.activate_impl_block(block, &decl.source);
                }
            }

            // `import { for X } from "./mod"` — bring in every inherent
            // for-block defined in ./mod for type X. Used for foreign-type
            // inherent extensions where X can't be imported as a value (e.g.
            // `for Array<T>` or `for string`).
            for for_spec in &decl.for_specifiers {
                let mut matched = false;
                for block in &resolved.for_blocks {
                    if block.trait_name.is_some() {
                        continue; // trait impls activate via trait import
                    }
                    let base_type_name = match &block.type_name.kind {
                        TypeExprKind::Named { name, .. } => name.clone(),
                        _ => continue,
                    };
                    if base_type_name == for_spec.type_name {
                        matched = true;
                        self.check_for_block_imported_with_source(block, &decl.source);
                        for func in &block.functions {
                            self.unused.used_names.insert(func.name.clone());
                        }
                    }
                }
                if !matched {
                    self.emit_error_with_help(
                        format!(
                            "module \"{}\" has no `for {}` extension block",
                            decl.source, for_spec.type_name
                        ),
                        for_spec.span,
                        ErrorCode::ExportNotFound,
                        "no inherent methods defined for this type here",
                        format!(
                            "check that `./{}` defines a `for {} {{ ... }}` block, or drop the `for {}` specifier",
                            decl.source.trim_start_matches("./"), for_spec.type_name, for_spec.type_name
                        ),
                    );
                }
            }
        }
    }

    /// When a trait is imported, activate every `impl Trait for T { ... }`
    /// block in the imported module's transitive closure. This replaces
    /// the old explicit `import { for Display }` specifier — trait
    /// impls now flow with trait imports.
    fn activate_trait_impls(
        &mut self,
        trait_decl: &TraitDecl,
        resolved: &crate::resolve::ResolvedImports,
        source: &str,
    ) {
        // Register the trait itself so later bound-checks + impl-matching
        // find it.
        self.register_trait_decl(trait_decl);

        for block in &resolved.for_blocks {
            if block.trait_name.as_deref() == Some(trait_decl.name.as_str()) {
                self.activate_impl_block(block, source);
            }
        }
    }

    /// Shared activation helper that dedupes via `impl_sources` and
    /// emits **E056 DuplicateImpl** when the same `(Type, Trait)` pair
    /// is reachable from two different sources.
    fn activate_impl_block(&mut self, block: &ForBlock, source: &str) {
        let Some(trait_name) = &block.trait_name else {
            return;
        };
        let Some(base) = block.type_name.base_name() else {
            return;
        };
        let key = (base, trait_name.clone());

        match self.impl_sources.get(&key) {
            Some(existing) if existing == source => {
                // Already activated via this source — diamond import, dedupe.
                return;
            }
            Some(existing) => {
                let (t, tr) = &key;
                let message = if existing == "this module" {
                    format!("`impl {tr} for {t}` is defined here and also imported from `{source}`")
                } else if source == "this module" {
                    format!(
                        "`impl {tr} for {t}` is imported from `{existing}` and also defined here"
                    )
                } else {
                    format!(
                        "`impl {tr} for {t}` is reachable from both `{existing}` and `{source}`"
                    )
                };
                self.emit_error_with_help(
                    message,
                    block.trait_name_span.unwrap_or(block.span),
                    ErrorCode::DuplicateImpl,
                    "trait impl defined by two different modules in scope",
                    "narrow one of the imports so only one impl activates, or remove the duplicate impl",
                );
                return;
            }
            None => {
                self.impl_sources.insert(key, source.to_string());
            }
        }

        self.check_for_block_imported_with_source(block, source);
        for func in &block.functions {
            self.unused.used_names.insert(func.name.clone());
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
                .define(&type_name, Type::foreign(type_name.clone()));
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
                return_type: Arc::new(return_type),
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

/// If `ty` is a `Type::Foreign`, tag it with the given trust. Non-foreign
/// types pass through unchanged — trust only applies at the npm boundary.
fn mark_foreign_untrusted(ty: Type, untrusted: bool) -> Type {
    if !untrusted {
        return ty;
    }
    match ty {
        Type::Foreign { name, .. } => Type::Foreign {
            name,
            untrusted: true,
        },
        other => other,
    }
}
