use super::*;

impl Checker {
    pub(crate) fn register_trait_decl(&mut self, decl: &TraitDecl) {
        let methods: Vec<TraitMethodSig> = decl
            .methods
            .iter()
            .map(|m| TraitMethodSig {
                name: m.name.clone(),
                has_default: m.body.is_some(),
            })
            .collect();
        self.traits.trait_defs.insert(decl.name.clone(), methods);
    }

    pub(crate) fn check_trait_decl(&mut self, decl: &TraitDecl) {
        // Validate method signatures (return types, param types)
        for method in &decl.methods {
            if let Some(ref rt) = method.return_type {
                self.resolve_type(rt);
            }
            for param in &method.params {
                if let Some(ref ta) = param.type_ann {
                    self.resolve_type(ta);
                }
            }
            // Default bodies are NOT type-checked here. They reference other
            // trait methods (like `self |> eq(other)`) which aren't defined yet.
            // The bodies will be checked when used in a concrete for-block impl.
        }

        if decl.exported {
            self.unused.used_names.insert(decl.name.clone());
        }
    }

    /// Validate that a `for Type: Trait` block satisfies the trait contract.
    pub(crate) fn check_trait_impl(
        &mut self,
        type_name: &str,
        trait_name: &str,
        functions: &[FunctionDecl],
        span: Span,
    ) {
        let trait_methods = match self.traits.trait_defs.get(trait_name) {
            Some(methods) => methods.clone(),
            None => {
                self.emit_error_with_help(
                    format!("unknown trait `{trait_name}`"),
                    span,
                    ErrorCode::UnknownTrait,
                    "not defined",
                    "check the spelling or define this trait",
                );
                return;
            }
        };

        // Check that all required methods are implemented
        let impl_names: HashSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();

        for method in &trait_methods {
            if !method.has_default && !impl_names.contains(method.name.as_str()) {
                self.emit_error_with_help(
                    format!(
                            "trait `{trait_name}` requires method `{}` but it is not implemented for `{type_name}`",
                            method.name
                        ),
                    span,
                    ErrorCode::MissingTraitMethod,
                    format!("missing method `{}`", method.name),
                    format!(
                        "add `fn {}(self, ...) {{ ... }}` to the for block",
                        method.name
                    ),
                );
            }
        }

        // Record the implementation
        self.traits
            .trait_impls
            .insert((type_name.to_string(), trait_name.to_string()));
    }
}
