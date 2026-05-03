use std::sync::Arc;

use super::{
    Checker, ErrorCode, FunctionDecl, HashMap, Param, Span, TraitDecl, TraitMethodSig, Type,
    params_have_self,
};

impl Checker {
    /// Whether `name` refers to a registered trait. Traits don't live
    /// in the value namespace, so callers that resolve identifiers
    /// check this before letting them show up in expression position.
    pub(crate) fn is_trait(&self, name: &str) -> bool {
        self.traits.trait_defs.contains_key(name)
    }

    pub(crate) fn register_trait_decl(&mut self, decl: &TraitDecl) {
        let methods: Vec<TraitMethodSig> = decl
            .methods
            .iter()
            .map(|m| TraitMethodSig {
                name: m.name.clone(),
                has_default: m.body.is_some(),
                has_self: params_have_self(&m.params),
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
            self.validate_self_receiver(&method.params, "trait method", &method.name, method.span);
            self.validate_non_self_params_have_types(&method.params, "trait method", &method.name);

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

    /// Enforce that `self` is present and is the first parameter.
    /// Used by both trait methods and for-block methods — the rules are
    /// identical in both contexts.
    pub(crate) fn validate_self_receiver(
        &mut self,
        params: &[Param],
        kind: &str,
        method_name: &str,
        span: Span,
    ) {
        let self_index = params.iter().position(|p| p.name == "self");
        match self_index {
            None => self.emit_error_with_help(
                format!("{kind} `{method_name}` must take `self` as its first parameter"),
                span,
                ErrorCode::TraitMethodSignatureMismatch,
                "missing `self` parameter",
                "add `self` as the first parameter",
            ),
            Some(i) if i > 0 => {
                let self_param = &params[i];
                self.emit_error_with_help(
                    format!("`self` must be the first parameter of {kind} `{method_name}`"),
                    self_param.span,
                    ErrorCode::TraitMethodSignatureMismatch,
                    "move `self` to the first position",
                    "rewrite as `(self, ...)`",
                );
            }
            _ => {}
        }
    }

    /// Every non-`self` parameter on a trait method or for-block method
    /// must carry an explicit type annotation — these signatures are
    /// public contracts and inference is not available.
    pub(crate) fn validate_non_self_params_have_types(
        &mut self,
        params: &[Param],
        kind: &str,
        method_name: &str,
    ) {
        for param in params {
            if param.name == "self" {
                continue;
            }
            if param.type_ann.is_none() {
                self.emit_error_with_help(
                    format!(
                        "parameter `{}` of {kind} `{method_name}` must have a type annotation",
                        param.name
                    ),
                    param.span,
                    ErrorCode::TraitMethodSignatureMismatch,
                    "missing type annotation",
                    format!("write `{}: Type`", param.name),
                );
            }
        }
    }

    /// Validate that a `for Type: Trait` block satisfies the trait contract.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn check_trait_impl(
        &mut self,
        type_name: &str,
        trait_name: &str,
        functions: &[FunctionDecl],
        span: Span,
    ) {
        let trait_methods = if let Some(methods) = self.traits.trait_defs.get(trait_name) {
            methods.clone()
        } else {
            self.emit_error_with_help(
                format!("unknown trait `{trait_name}`"),
                span,
                ErrorCode::UnknownTrait,
                "not defined",
                "check the spelling or define this trait",
            );
            return;
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

            // Check self presence
            if params_have_self(&impl_fn.params) != method.has_self {
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

            let impl_params: Vec<&Param> =
                impl_fn.params.iter().filter(|p| p.name != "self").collect();

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
                        format!("expected `{trait_ty}`"),
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
                        format!("expected `{trait_ret}`"),
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

    /// Return the name of the first trait in `bounds` that defines a method
    /// called `method_name`. Used to diagnose dot-access on trait methods
    /// reached through a type-parameter bound, which must use pipe syntax.
    pub(crate) fn trait_defining_method_in_bounds(
        &self,
        method_name: &str,
        bounds: &[String],
    ) -> Option<String> {
        for bound_trait in bounds {
            if let Some(methods) = self.traits.trait_defs.get(bound_trait.as_str())
                && methods.iter().any(|m| m.name == method_name)
            {
                return Some(bound_trait.clone());
            }
        }
        None
    }

    /// Look up a trait method by name across a list of trait bounds.
    /// Returns the method as a `Type::Function` if found.
    pub(crate) fn resolve_trait_method(
        &mut self,
        method_name: &str,
        bounds: &[String],
    ) -> Option<Type> {
        for bound_trait in bounds {
            let methods = self.traits.trait_defs.get(bound_trait.as_str())?.clone();
            for method in &methods {
                if method.name == method_name {
                    // Build a Type::Function from the trait method signature
                    // Include self as first param (type Unknown — it's the generic receiver)
                    let mut param_types = vec![Type::Unknown]; // self
                    for param in &method.params {
                        let ty = param
                            .type_ann
                            .as_ref()
                            .map(|ta| self.resolve_type(ta))
                            .unwrap_or(Type::Unknown);
                        param_types.push(ty);
                    }
                    let return_type = method
                        .return_type
                        .as_ref()
                        .map(|rt| self.resolve_type(rt))
                        .unwrap_or(Type::Unknown);

                    return Some(Type::Function {
                        params: param_types,
                        return_type: Arc::new(return_type),
                        required_params: method.params.len() + 1, // +1 for self
                    });
                }
            }
        }
        None
    }
}
