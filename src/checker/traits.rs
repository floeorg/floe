use super::*;

impl Checker {
    pub(crate) fn register_trait_decl(&mut self, decl: &TraitDecl) {
        let methods: Vec<TraitMethodSig> = decl
            .methods
            .iter()
            .map(|m| TraitMethodSig {
                name: m.name.clone(),
                has_default: m.body.is_some(),
                has_self: m.params.first().is_some_and(|p| p.name == "self"),
                params: m
                    .params
                    .iter()
                    .filter(|p| p.name != "self")
                    .cloned()
                    .collect(),
                return_type: m.return_type.clone(),
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

        // Build a map from method name to impl function for signature checking
        let impl_fns: HashMap<&str, &FunctionDecl> =
            functions.iter().map(|f| (f.name.as_str(), f)).collect();

        for method in &trait_methods {
            if !impl_fns.contains_key(method.name.as_str()) {
                if !method.has_default {
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
                continue;
            }

            let impl_fn = impl_fns[method.name.as_str()];
            let impl_has_self = impl_fn.params.first().is_some_and(|p| p.name == "self");
            let impl_params: Vec<&Param> =
                impl_fn.params.iter().filter(|p| p.name != "self").collect();

            // Check self presence
            if impl_has_self != method.has_self {
                let msg = if method.has_self {
                    format!(
                        "method `{}` in `{type_name}` is missing `self` but trait `{trait_name}` requires it",
                        method.name
                    )
                } else {
                    format!(
                        "method `{}` in `{type_name}` has `self` but trait `{trait_name}` does not",
                        method.name
                    )
                };
                self.emit_error_with_help(
                    msg,
                    span,
                    ErrorCode::TraitMethodSignatureMismatch,
                    if method.has_self {
                        "add `self` as the first parameter"
                    } else {
                        "remove `self` parameter"
                    },
                    format!("change the signature to match trait `{trait_name}`"),
                );
                continue;
            }

            // Check parameter count
            if impl_params.len() != method.params.len() {
                self.emit_error_with_help(
                    format!(
                        "method `{}` in `{type_name}` has {} parameter(s) but trait `{trait_name}` requires {}",
                        method.name,
                        impl_params.len(),
                        method.params.len(),
                    ),
                    span,
                    ErrorCode::TraitMethodSignatureMismatch,
                    format!("expected {} parameter(s)", method.params.len()),
                    format!("change the signature to match trait `{trait_name}`"),
                );
                continue;
            }

            // Check each parameter type
            for (i, (impl_param, trait_param)) in
                impl_params.iter().zip(method.params.iter()).enumerate()
            {
                let impl_ty = impl_param
                    .type_ann
                    .as_ref()
                    .map(|ta| self.resolve_type(ta))
                    .unwrap_or(Type::Unknown);
                let trait_ty = trait_param
                    .type_ann
                    .as_ref()
                    .map(|ta| self.resolve_type(ta))
                    .unwrap_or(Type::Unknown);

                if !impl_ty.is_undetermined() && !trait_ty.is_undetermined() && impl_ty != trait_ty
                {
                    let param_span = impl_param
                        .type_ann
                        .as_ref()
                        .map(|ta| ta.span)
                        .unwrap_or(span);
                    self.emit_error_with_help(
                        format!(
                            "parameter {} of method `{}` has type `{}` but trait `{trait_name}` requires `{}`",
                            i + 1,
                            method.name,
                            impl_ty,
                            trait_ty,
                        ),
                        param_span,
                        ErrorCode::TraitMethodSignatureMismatch,
                        format!("expected `{}`", trait_ty),
                        format!("change to match trait `{trait_name}`"),
                    );
                }
            }

            // Check return type
            if let Some(ref trait_rt) = method.return_type.clone() {
                let trait_ret = self.resolve_type(trait_rt);
                let impl_ret = impl_fn
                    .return_type
                    .as_ref()
                    .map(|rt| self.resolve_type(rt))
                    .unwrap_or(Type::Unknown);

                if !impl_ret.is_undetermined()
                    && !trait_ret.is_undetermined()
                    && impl_ret != trait_ret
                {
                    let ret_span = impl_fn
                        .return_type
                        .as_ref()
                        .map(|rt| rt.span)
                        .unwrap_or(span);
                    self.emit_error_with_help(
                        format!(
                            "method `{}` returns `{}` but trait `{trait_name}` requires `{}`",
                            method.name, impl_ret, trait_ret,
                        ),
                        ret_span,
                        ErrorCode::TraitMethodSignatureMismatch,
                        format!("expected `{}`", trait_ret),
                        format!("change to match trait `{trait_name}`"),
                    );
                }
            }
        }

        // Record the implementation
        self.traits
            .trait_impls
            .insert((type_name.to_string(), trait_name.to_string()));
    }
}
