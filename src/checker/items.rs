use super::*;

/// Returns the span of the last item in a block body, falling back to the body's own span.
/// Used to point return-type errors at the actual return value instead of the whole function.
fn last_expr_span(body: &Expr) -> Span {
    if let ExprKind::Block(items) = &body.kind
        && let Some(last) = items.last()
    {
        return last.span;
    }
    body.span
}

impl Checker {
    // ── Item Checking ────────────────────────────────────────────

    pub(crate) fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.check_import(decl, item.span),
            ItemKind::ReExport(_) => {
                // Re-exports don't introduce names into the current scope
            }
            ItemKind::Const(decl) => self.check_const(decl, item.span),
            ItemKind::Function(decl) => self.check_function(decl, item.span),
            ItemKind::TypeDecl(decl) => self.validate_type_decl_annotations(decl),
            ItemKind::ForBlock(block) => self.check_for_block(block, item.span),
            ItemKind::TraitDecl(decl) => self.check_trait_decl(decl),
            ItemKind::TestBlock(block) => self.check_test_block(block),
            ItemKind::Expr(expr) => {
                let ty = self.check_expr(expr);
                // Rule 5: No floating Results/Options
                if ty.is_result() {
                    self.emit_error_with_help(
                        "unhandled `Result` value",
                        expr.span,
                        ErrorCode::UnhandledResult,
                        "this `Result` is not used",
                        "use `?`, `match`, or assign to `_`",
                    );
                }
            }
        }
    }

    pub(crate) fn check_const(&mut self, decl: &ConstDecl, span: Span) {
        let value_type = self.check_expr(&decl.value);
        let declared_type = decl.type_ann.as_ref().map(|t| self.resolve_type(t));

        // Refine None: if value is Option<Unknown> and declared type is Option<T>,
        // Refine None: record the concrete Option type for hover
        if let Some(ref declared) = declared_type
            && value_type.is_option()
            && matches!(value_type.option_inner(), Some(Type::Unknown))
            && declared.is_option()
        {
            self.expr_types.insert(decl.value.id, declared.clone());
        }
        let tsgo_type = self.find_and_consume_tsgo_probe(&decl.binding).map(|ty| {
            // The tsgo probe generates the raw call expression without `await`,
            // so adjust the probe type to match the Floe expression:
            // - `await`: unwrap Promise<T> → T
            if Self::expr_has_promise_await(&decl.value)
                && let Type::Promise(inner) = ty
            {
                *inner
            } else {
                ty
            }
        });
        let final_type =
            self.resolve_const_type(value_type, declared_type, &tsgo_type, &decl.value, span);

        match &decl.binding {
            ConstBinding::Name(name) => {
                self.define_const_binding(name, final_type, decl.exported, span);
            }
            ConstBinding::Array(names) => {
                let corrected_type = self.correct_usestate_option_type(&final_type, &decl.value);
                let effective_type = corrected_type.as_ref().unwrap_or(&final_type);

                for (i, name) in names.iter().enumerate() {
                    let elem_ty = Self::array_element_type(effective_type, i);
                    self.define_const_binding(name, elem_ty, false, span);
                }
            }
            ConstBinding::Tuple(names) => {
                for (i, name) in names.iter().enumerate() {
                    let elem_ty = Self::tuple_element_type(&final_type, i);
                    self.define_const_binding(name, elem_ty, false, span);
                }
            }
            ConstBinding::Object(names) => {
                self.define_object_destructured_bindings(
                    names,
                    &final_type,
                    tsgo_type.is_some(),
                    span,
                );
            }
        }
    }

    /// Find, consume, and return a probe export matching the predicate.
    /// Prefer "inlined" variants when `prefer_inlined` is true.
    fn consume_probe(
        &mut self,
        predicate: impl Fn(&str) -> bool,
        prefer_inlined: bool,
    ) -> Option<Type> {
        let mut found: Option<(String, usize, DtsExport)> = None;
        let mut found_inlined: Option<(String, usize, DtsExport)> = None;
        'outer: for (spec, exports) in &self.dts_imports {
            for (exp_idx, export) in exports.iter().enumerate() {
                let key = (spec.clone(), exp_idx);
                if self.probe_consumed.contains(&key) || !predicate(&export.name) {
                    continue;
                }
                if prefer_inlined && export.name.contains("inlined") {
                    found_inlined = Some((spec.clone(), exp_idx, export.clone()));
                    break 'outer;
                } else if found.is_none() {
                    found = Some((spec.clone(), exp_idx, export.clone()));
                    if !prefer_inlined {
                        break 'outer;
                    }
                }
            }
        }
        let result = if prefer_inlined {
            found_inlined.or(found)
        } else {
            found
        };
        if let Some((spec, exp_idx, _)) = &result {
            self.probe_consumed.insert((spec.clone(), *exp_idx));
        }
        result.map(|(_, _, e)| interop::wrap_boundary_type(&e.ts_type))
    }

    /// Search dts_imports for a tsgo probe matching the binding name, consume it, and return its type.
    fn find_and_consume_tsgo_probe(&mut self, binding: &ConstBinding) -> Option<Type> {
        let binding_name = binding.binding_name();
        let probe_key = format!("__probe_{binding_name}");
        let probe_prefix = format!("__probe_{binding_name}_");
        self.consume_probe(
            |name| name == probe_key || name.starts_with(&probe_prefix),
            true,
        )
    }

    /// Search for a per-field probe (e.g. `__probe_data_4` or `__probe_data_inlined_N`).
    fn find_per_field_probe(&mut self, field_name: &str) -> Option<Type> {
        let prefix = format!("__probe_{field_name}_");
        self.consume_probe(|name| name.starts_with(&prefix), true)
    }

    /// Determine the final type for a const binding given value type, declared type, and tsgo probe.
    fn resolve_const_type(
        &mut self,
        value_type: Type,
        declared_type: Option<Type>,
        tsgo_type: &Option<Type>,
        value_expr: &Expr,
        span: Span,
    ) -> Type {
        if let Some(tsgo_ty) = tsgo_type {
            if matches!(tsgo_ty, Type::Unknown)
                && !matches!(value_type, Type::Unknown | Type::Var(_))
            {
                // Probe resolved to Unknown (e.g. useMemo callback with free
                // variables) but the checker inferred a concrete type via
                // generic inference — prefer the checker's type.
                value_type
            } else if let Type::Function {
                params: tsgo_params,
                return_type: tsgo_ret,
                required_params: tsgo_req,
            } = tsgo_ty
                && matches!(tsgo_ret.as_ref(), Type::Unknown)
            {
                // Probe has concrete param types but Unknown return (e.g.
                // useCallback with free variables in the body). Try to get
                // the return type from the checker's inference of the arrow arg.
                let checker_ret = Self::find_arrow_arg(value_expr)
                    .and_then(|arrow| self.expr_types.get(&arrow.id))
                    .and_then(|ty| match ty {
                        Type::Function { return_type, .. } => Some(return_type.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| tsgo_ret.clone());
                Type::Function {
                    params: tsgo_params.clone(),
                    required_params: *tsgo_req,
                    return_type: checker_ret,
                }
            } else {
                tsgo_ty.clone()
            }
        } else if let Some(ref declared) = declared_type {
            if matches!(value_type, Type::Unknown) && !matches!(declared, Type::Unknown) {
                self.emit_error_with_help(
                    format!(
                        "cannot narrow `unknown` to `{}` — use runtime validation instead",
                        declared
                    ),
                    span,
                    ErrorCode::UnsafeNarrowing,
                    "unsafe narrowing from `unknown`",
                    "use a validation library like Zod, or match on the value",
                );
            } else if !self.types_compatible(declared, &value_type) {
                self.emit_error(
                    format!("expected `{}`, found `{}`", declared, value_type),
                    span,
                    ErrorCode::TypeMismatch,
                    format!("expected `{}`", declared),
                );
            }
            declared.clone()
        } else {
            value_type
        }
    }

    /// Find the first arrow function argument in a call expression.
    /// For `useCallback((item) => {...}, [])`, returns the `(item) => {...}` expr.
    fn find_arrow_arg(expr: &Expr) -> Option<&Expr> {
        if let ExprKind::Call { args, .. } = &expr.kind {
            for arg in args {
                let arg_expr = match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => e,
                };
                if matches!(arg_expr.kind, ExprKind::Arrow { .. }) {
                    return Some(arg_expr);
                }
            }
        }
        None
    }

    /// Check if an expression contains a `Promise.await` pipe call.
    fn expr_has_promise_await(expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Pipe { left, right } => {
                Self::is_promise_await_member(right)
                    || Self::expr_has_promise_await(left)
                    || Self::expr_has_promise_await(right)
            }
            ExprKind::Unwrap(inner) => Self::expr_has_promise_await(inner),
            _ => false,
        }
    }

    fn is_promise_await_member(expr: &Expr) -> bool {
        matches!(&expr.kind, ExprKind::Member { object, field }
            if field == "await" && matches!(&object.kind, ExprKind::Identifier(m) if m == "Promise"))
            || matches!(&expr.kind, ExprKind::Identifier(name) if name == "await")
    }

    /// Infer the type of an element from an array/tuple destructuring at a given index.
    fn array_element_type(effective_type: &Type, i: usize) -> Type {
        match effective_type {
            Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
            Type::Unknown | Type::Var(_) => Type::Unknown,
            other if i == 0 => other.clone(),
            _ => Type::Unknown,
        }
    }

    /// Infer the type of a tuple element at a given index.
    fn tuple_element_type(final_type: &Type, i: usize) -> Type {
        match final_type {
            Type::Tuple(types) => types.get(i).cloned().unwrap_or(Type::Unknown),
            Type::Unknown | Type::Var(_) => Type::Unknown,
            _ => Type::Unknown,
        }
    }

    /// Define a single const binding (handles no-redefinition check, name_types, env, etc.)
    fn define_const_binding(&mut self, name: &str, ty: Type, exported: bool, span: Span) {
        self.check_no_redefinition(name, span);
        // Catch unresolved tsgo probes, missing .d.ts types, and untyped npm
        // imports early instead of letting them cascade into confusing errors.
        if matches!(ty, Type::Unknown)
            && !name.starts_with('_')
            && !self.has_error_within_span(span)
        {
            self.emit_warning_with_help(
                format!("binding `{name}` has type `unknown`"),
                span,
                ErrorCode::UnknownBinding,
                "type could not be resolved",
                "add a type annotation or check that the import has type definitions",
            );
        }
        self.name_types.insert(name.to_string(), ty.to_string());
        self.env.define(name, ty);
        self.unused
            .defined_sources
            .insert(name.to_string(), "const".to_string());
        if exported {
            self.unused.used_names.insert(name.to_string());
        }
        self.unused.defined_names.push((name.to_string(), span));
    }

    /// Handle object destructuring for const bindings.
    fn define_object_destructured_bindings(
        &mut self,
        fields: &[ObjectDestructureField],
        final_type: &Type,
        has_tsgo: bool,
        span: Span,
    ) {
        let concrete = self.resolve_type_to_concrete(final_type);

        let field_map: Option<std::collections::HashMap<&str, &Type>> = match &concrete {
            Type::Record(rec_fields) => {
                Some(rec_fields.iter().map(|(n, t)| (n.as_str(), t)).collect())
            }
            _ => None,
        };

        // If tsgo resolved a single-field destructure and the type isn't a Record
        // with that field, assign it directly (legacy behavior for field-level probes)
        if has_tsgo
            && fields.len() == 1
            && field_map
                .as_ref()
                .is_none_or(|m| !m.contains_key(fields[0].field.as_str()))
        {
            self.define_const_binding(fields[0].bound_name(), final_type.clone(), false, span);
            return;
        }

        for f in fields {
            // Look up by the original field name in the source type
            let field_ty = field_map
                .as_ref()
                .and_then(|m| m.get(f.field.as_str()))
                .cloned()
                .cloned();
            // If no field found (e.g. Foreign type), try a per-field probe lookup
            let ty = field_ty
                .unwrap_or_else(|| self.find_per_field_probe(&f.field).unwrap_or(Type::Unknown));
            self.define_const_binding(f.bound_name(), ty, false, span);
        }
    }

    pub(crate) fn check_function(&mut self, decl: &FunctionDecl, span: Span) {
        // Rule: Exported functions must declare return types
        if decl.exported && decl.return_type.is_none() {
            self.emit_error_with_help(
                format!(
                    "exported function `{}` must declare a return type",
                    decl.name
                ),
                span,
                ErrorCode::MissingReturnType,
                "missing return type",
                "add `-> ReturnType` after the parameter list",
            );
        }

        // Register generic type parameters so they're recognized during type resolution
        for tp in &decl.type_params {
            self.env.define(tp, Type::Named(tp.clone()));
        }

        let return_type = decl
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or_else(|| self.fresh_type_var());

        // Define function in outer scope before checking body
        let param_types: Vec<_> = decl
            .params
            .iter()
            .map(|p| {
                p.type_ann
                    .as_ref()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| self.fresh_type_var())
            })
            .collect();

        // Track required (non-default) parameter count
        let required_params = decl.params.iter().filter(|p| p.default.is_none()).count();
        let fn_type = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
            required_params,
        };
        self.check_no_redefinition(&decl.name, span);
        self.env.define(&decl.name, fn_type);
        self.unused
            .defined_sources
            .insert(decl.name.clone(), "function".to_string());
        if required_params < decl.params.len() {
            self.fn_required_params
                .insert(decl.name.clone(), required_params);
        }

        // Track parameter names for named argument validation
        self.fn_param_names.insert(
            decl.name.clone(),
            decl.params.iter().map(|p| p.name.clone()).collect(),
        );

        if decl.exported {
            self.unused.used_names.insert(decl.name.clone());
        }
        self.unused.defined_names.push((decl.name.clone(), span));

        // Set up scope for function body
        let prev_return_type = self.ctx.current_return_type.take();
        // For Promise<T> return types, unwrap so ? sees the inner type
        let effective_return = match &return_type {
            Type::Promise(inner) => *inner.clone(),
            _ => return_type.clone(),
        };
        self.ctx.current_return_type = Some(effective_return);

        self.env.push_scope();

        // Define parameters (check for shadowing, but skip `self`)
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            if param.name != "self" {
                self.check_no_redefinition(&param.name, span);
            }
            self.env.define(&param.name, ty.clone());

            // For destructured params, define the individual field names in scope
            if let Some(ref destructure) = param.destructure {
                self.define_destructured_bindings(destructure, ty, param.span);
            }
        }

        // Type-check default parameter values
        for (param, ty) in decl.params.iter().zip(param_types.iter()) {
            if let Some(default_expr) = &param.default {
                let default_ty = self.check_expr(default_expr);
                if !self.types_compatible(ty, &default_ty) {
                    self.emit_error(
                        format!(
                            "default value for `{}`: expected `{}`, found `{}`",
                            param.name, ty, default_ty
                        ),
                        param.span,
                        ErrorCode::TypeMismatch,
                        format!("expected `{}`", ty),
                    );
                }
            }
        }

        // Check body
        let body_type = self.check_expr(&decl.body);
        let uses_await = super::body_has_promise_await(&decl.body);

        // When no return type annotation, infer from body and update the function type
        if decl.return_type.is_none() && !matches!(body_type, Type::Var(_) | Type::Unknown) {
            // If the body uses await, wrap the inferred return type in Promise<T>
            let inferred_return = if uses_await {
                Type::Promise(Box::new(body_type.clone()))
            } else {
                body_type.clone()
            };
            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(inferred_return),
                required_params,
            };
            // Update in the name_types map for hover display
            self.name_types
                .insert(decl.name.clone(), fn_type.to_string());
            // Mark for updating in outer scope after pop
            self.env.define_in_parent_scope(&decl.name, fn_type);
        }

        // Check return type compatibility
        if let Some(ref declared_return) = decl.return_type {
            let resolved = self.resolve_type(declared_return);

            // Error if function uses await but return type is not Promise<T>
            if uses_await && !matches!(resolved, Type::Promise(_)) {
                self.emit_error_with_help(
                    format!(
                        "function `{}` uses `await` but return type is `{}`, not `Promise<{}>`",
                        decl.name, resolved, body_type
                    ),
                    span,
                    ErrorCode::MissingPromiseReturn,
                    format!("expected `Promise<{}>`", body_type),
                    "change the return type to `Promise<T>`, or remove the `await`",
                );
            }

            // For functions with Promise<T> return type, unwrap Promise since
            // the body type is the inner value (async wrapping is automatic)
            let effective_declared = match &resolved {
                Type::Promise(inner) => inner.as_ref().clone(),
                _ => resolved.clone(),
            };
            if !self.types_compatible(&effective_declared, &body_type)
                && !matches!(body_type, Type::Var(_))
            {
                let (msg, label) = if let Some((annotation, lbl)) =
                    self.extra_mismatch_detail(&effective_declared, &body_type)
                {
                    (
                        format!(
                            "function `{}`: expected return type `{}`, {}",
                            decl.name, resolved, annotation
                        ),
                        lbl,
                    )
                } else {
                    (
                        format!(
                            "function `{}`: expected return type `{}`, found `{}`",
                            decl.name, resolved, body_type
                        ),
                        format!("expected `{}`", resolved),
                    )
                };
                self.emit_error(
                    msg,
                    last_expr_span(&decl.body),
                    ErrorCode::TypeMismatch,
                    label,
                );
            }

            // Rule: non-unit functions must have an explicit return value
            if !matches!(resolved, Type::Unit)
                && matches!(body_type, Type::Unit)
                && !self.body_has_return(&decl.body)
            {
                self.emit_error_with_help(
                    format!(
                        "function `{}` must return a value of type `{}`",
                        decl.name, resolved
                    ),
                    span,
                    ErrorCode::MissingReturnValue,
                    "missing return value",
                    "add a return expression or change return type to `()`",
                );
            }
        }

        self.env.pop_scope();
        self.ctx.current_return_type = prev_return_type;
    }

    pub(crate) fn check_for_block(&mut self, block: &ForBlock, _span: Span) {
        let for_type = self.resolve_type(&block.type_name);
        let type_name = match &block.type_name.kind {
            TypeExprKind::Named { name, .. } => name.clone(),
            _ => String::new(),
        };

        // If this is a trait impl block, validate the trait contract
        if let Some(ref trait_name) = block.trait_name {
            self.unused.used_names.insert(trait_name.clone());
            let type_display = for_type.to_string();
            self.check_trait_impl(&type_display, trait_name, &block.functions, block.span);
        }

        for func in &block.functions {
            // Check each function, injecting `self` type for self params
            let return_type = func
                .return_type
                .as_ref()
                .map(|t| self.resolve_type(t))
                .unwrap_or_else(|| self.fresh_type_var());

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
                            .unwrap_or_else(|| self.fresh_type_var())
                    }
                })
                .collect();

            let required_params = param_types.len();
            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(return_type.clone()),
                required_params,
            };
            // Allow for-block functions with the same name on different types
            // (e.g. Entry.fromRow and Accent.fromRow are not in conflict)
            let is_different_for_block = self
                .for_block_overloads
                .get(&func.name)
                .and_then(|o| o.last())
                .is_some_and(|(existing_type, _)| *existing_type != type_name);
            if !is_different_for_block {
                self.check_no_redefinition(&func.name, block.span);
            }
            self.env.define(&func.name, fn_type.clone());
            self.unused
                .defined_sources
                .insert(func.name.clone(), "for-block function".to_string());
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

            if func.exported {
                self.unused.used_names.insert(func.name.clone());
            }
            self.unused
                .defined_names
                .push((func.name.clone(), block.span));

            // Check the function body
            let prev_return_type = self.ctx.current_return_type.take();
            // For Promise<T> return types, unwrap so ? sees the inner type
            let effective_return = match &return_type {
                Type::Promise(inner) => *inner.clone(),
                _ => return_type.clone(),
            };
            self.ctx.current_return_type = Some(effective_return);

            self.env.push_scope();

            for (param, ty) in func.params.iter().zip(param_types.iter()) {
                self.env.define(&param.name, ty.clone());
            }

            // Type-check default parameter values
            for (param, ty) in func.params.iter().zip(param_types.iter()) {
                if let Some(default_expr) = &param.default {
                    let default_ty = self.check_expr(default_expr);
                    if !self.types_compatible(ty, &default_ty) {
                        self.emit_error(
                            format!(
                                "default value for `{}`: expected `{}`, found `{}`",
                                param.name, ty, default_ty
                            ),
                            param.span,
                            ErrorCode::TypeMismatch,
                            format!("expected `{}`", ty),
                        );
                    }
                }
            }

            let body_type = self.check_expr(&func.body);

            if let Some(ref declared_return) = func.return_type {
                let resolved = self.resolve_type(declared_return);
                // For Promise<T> return types, unwrap Promise since the body
                // type is the inner value (async wrapping is automatic)
                let effective_declared = match &resolved {
                    Type::Promise(inner) => inner.as_ref().clone(),
                    _ => resolved.clone(),
                };
                if !self.types_compatible(&effective_declared, &body_type)
                    && !matches!(body_type, Type::Var(_))
                {
                    let (msg, label) = if let Some((annotation, lbl)) =
                        self.extra_mismatch_detail(&effective_declared, &body_type)
                    {
                        (
                            format!(
                                "function `{}`: expected return type `{}`, {}",
                                func.name, resolved, annotation
                            ),
                            lbl,
                        )
                    } else {
                        (
                            format!(
                                "function `{}`: expected return type `{}`, found `{}`",
                                func.name, resolved, body_type
                            ),
                            format!("expected `{}`", resolved),
                        )
                    };
                    self.emit_error(
                        msg,
                        last_expr_span(&func.body),
                        ErrorCode::TypeMismatch,
                        label,
                    );
                }
            }

            self.env.pop_scope();
            self.ctx.current_return_type = prev_return_type;
        }
    }

    pub(crate) fn check_test_block(&mut self, block: &TestBlock) {
        // Type-check test block body in its own scope
        self.env.push_scope();

        for stmt in &block.body {
            match stmt {
                TestStatement::Assert(expr, span) => {
                    let ty = self.check_expr(expr);
                    // Ensure assert expression evaluates to boolean
                    if !matches!(ty, Type::Bool | Type::Unknown | Type::Var(_)) {
                        self.emit_error(
                            format!("assert expression must be boolean, found `{}`", ty),
                            *span,
                            ErrorCode::AssertNotBoolean,
                            "expected boolean expression",
                        );
                    }
                }
                TestStatement::Expr(expr) => {
                    self.check_expr(expr);
                }
            }
        }

        self.env.pop_scope();
    }

    /// Checks if a function body contains a value-producing expression
    /// (implicit return). With implicit returns, the last expression in
    /// a block is the return value.
    pub(crate) fn body_has_return(&self, body: &Expr) -> bool {
        match &body.kind {
            // `todo` and `unreachable` are never-returning, so they satisfy return requirements
            ExprKind::Todo | ExprKind::Unreachable => true,
            ExprKind::Block(items) => {
                // Check if the last item is an expression (implicit return)
                items.last().is_some_and(|item| {
                    if let ItemKind::Expr(e) = &item.kind {
                        self.body_has_return(e)
                    } else {
                        false
                    }
                })
            }
            ExprKind::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.body_has_return(&arm.body))
            }
            // Any other expression is a value-producing expression
            _ => true,
        }
    }

    // ── JSX Checking ─────────────────────────────────────────────

    pub(crate) fn check_jsx(&mut self, element: &JsxElement) {
        match &element.kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                ..
            } => {
                // Resolve the component's props type for prop validation
                let props_map: Option<std::collections::HashMap<String, Type>> =
                    if name.starts_with(|c: char| c.is_uppercase()) {
                        if let Some((root, _)) = name.split_once('.') {
                            // Member expression: mark root as used, skip prop resolution
                            self.unused.used_names.insert(root.to_string());
                            None
                        } else {
                            self.unused.used_names.insert(name.clone());
                            if let Some(comp_ty) = self.env.lookup(name).cloned() {
                                self.resolve_jsx_props_fields(&comp_ty)
                            } else {
                                self.emit_error(
                                    format!("component `{name}` is not defined"),
                                    element.span,
                                    ErrorCode::UndefinedName,
                                    "not found in scope",
                                );
                                None
                            }
                        }
                    } else {
                        None
                    };
                for prop in props {
                    match prop {
                        JsxProp::Named {
                            name: prop_name,
                            value,
                            span: prop_span,
                        } => {
                            if let Some(value) = value {
                                let value_ty = if prop_name.starts_with("on") && prop_name.len() > 2
                                {
                                    let prev = self.ctx.event_handler_context;
                                    self.ctx.event_handler_context = true;
                                    let ty = self.check_expr(value);
                                    self.ctx.event_handler_context = prev;
                                    ty
                                } else if matches!(value.kind, ExprKind::Arrow { .. }) {
                                    // Set callback param hint from tsgo probe if available
                                    let prev = std::mem::take(&mut self.ctx.lambda_param_hints);
                                    if let Some(hint_ty) = self
                                        .jsx_callback_hints
                                        .get(name)
                                        .and_then(|m| m.get(prop_name))
                                        .cloned()
                                    {
                                        self.ctx.lambda_param_hints = vec![hint_ty];
                                    }
                                    let ty = self.check_expr(value);
                                    self.ctx.lambda_param_hints = prev;
                                    ty
                                } else {
                                    self.check_expr(value)
                                };

                                // Validate prop value type against expected prop type.
                                // Skip when either side is unresolvable (Foreign, Named,
                                // Unknown, Var) since we can't validate npm type
                                // compatibility (e.g. JSX.Element vs ReactNode).
                                if let Some(ref map) = props_map
                                    && let Some(expected_ty) = map.get(prop_name.as_str())
                                    && self.is_jsx_prop_type_checkable(expected_ty)
                                    && self.is_jsx_prop_type_checkable(&value_ty)
                                    && !self.types_compatible(expected_ty, &value_ty)
                                {
                                    self.emit_error(
                                        format!(
                                            "prop `{prop_name}`: expected `{expected_ty}`, found `{value_ty}`"
                                        ),
                                        *prop_span,
                                        ErrorCode::TypeMismatch,
                                        format!("expected `{expected_ty}`"),
                                    );
                                }
                            }
                        }
                        JsxProp::Spread { expr, .. } => {
                            self.check_expr(expr);
                        }
                    }
                }
                self.check_jsx_children(children, Some(name));
            }
            JsxElementKind::Fragment { children } => {
                self.check_jsx_children(children, None);
            }
        }
    }

    /// Check if a type is concrete enough for JSX prop validation.
    /// Skip Foreign, Named (npm types), Unknown, Var, and Option-wrapped versions.
    fn is_jsx_prop_type_checkable(&self, ty: &Type) -> bool {
        match ty {
            Type::Unknown | Type::Foreign(_) | Type::Named(_) | Type::Var(_) => false,
            _ if ty.is_option() => {
                // Option<T> — check if T is checkable
                ty.option_inner()
                    .is_some_and(|inner| self.is_jsx_prop_type_checkable(inner))
            }
            _ => true,
        }
    }

    /// Resolve a component's function type to its props as a HashMap for O(1) lookup.
    /// Returns None if the type can't be resolved to known fields.
    fn resolve_jsx_props_fields(
        &mut self,
        comp_ty: &Type,
    ) -> Option<std::collections::HashMap<String, Type>> {
        let props_ty = match comp_ty {
            Type::Function { params, .. } if !params.is_empty() => &params[0],
            _ => return None,
        };
        let concrete = self.resolve_type_to_concrete(props_ty);
        match concrete {
            Type::Record(fields) => Some(fields.into_iter().collect()),
            _ => None,
        }
    }

    pub(crate) fn check_jsx_children(
        &mut self,
        children: &[JsxChild],
        component_name: Option<&str>,
    ) {
        for child in children {
            match child {
                JsxChild::Expr(e) => {
                    // Set children render prop hints for arrow function children
                    if matches!(e.kind, ExprKind::Arrow { .. })
                        && let Some(name) = component_name
                        && let Some(hints) = self.jsx_children_hints.get(name).cloned()
                    {
                        let prev = std::mem::replace(&mut self.ctx.lambda_param_hints, hints);
                        self.check_expr(e);
                        self.ctx.lambda_param_hints = prev;
                    } else {
                        self.check_expr(e);
                    }
                }
                JsxChild::Element(el) => {
                    self.check_jsx(el);
                }
                JsxChild::Text(_) => {}
            }
        }
    }
}
