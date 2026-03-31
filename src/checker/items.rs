use super::*;

impl Checker {
    // ── Item Checking ────────────────────────────────────────────

    pub(crate) fn check_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.check_import(decl),
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
                        "E005",
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
            // so if the Floe code has `await`, unwrap Promise from the probe type.
            if Self::expr_has_await(&decl.value)
                && let Type::Promise(inner) = ty
            {
                return *inner;
            }
            ty
        });
        let final_type = self.resolve_const_type(value_type, declared_type, &tsgo_type, span);

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

    /// Search for a per-field inlined probe (e.g. `__probe_data_inlined_N` for field `data`).
    fn find_per_field_probe(&mut self, field_name: &str) -> Option<Type> {
        let prefix = format!("__probe_{field_name}_inlined_");
        self.consume_probe(|name| name.starts_with(&prefix), false)
    }

    /// Determine the final type for a const binding given value type, declared type, and tsgo probe.
    fn resolve_const_type(
        &mut self,
        value_type: Type,
        declared_type: Option<Type>,
        tsgo_type: &Option<Type>,
        span: Span,
    ) -> Type {
        if let Some(tsgo_ty) = tsgo_type {
            tsgo_ty.clone()
        } else if let Some(ref declared) = declared_type {
            if matches!(value_type, Type::Unknown) && !matches!(declared, Type::Unknown) {
                self.emit_error_with_help(
                    format!(
                        "cannot narrow `unknown` to `{}` — use runtime validation instead",
                        declared.display_name()
                    ),
                    span,
                    "E019",
                    "unsafe narrowing from `unknown`",
                    "use a validation library like Zod, or match on the value",
                );
            } else if !self.types_compatible(declared, &value_type) {
                self.emit_error(
                    format!(
                        "expected `{}`, found `{}`",
                        declared.display_name(),
                        value_type.display_name()
                    ),
                    span,
                    "E001",
                    format!("expected `{}`", declared.display_name()),
                );
            }
            declared.clone()
        } else {
            value_type
        }
    }

    /// Check if an expression is wrapped in `await` (possibly through `try`).
    fn expr_has_await(expr: &Expr) -> bool {
        expr_has_await(expr)
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
        self.name_types.insert(name.to_string(), ty.display_name());
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
        let concrete = self
            .env
            .resolve_to_concrete(final_type, &expr::simple_resolve_type_expr);

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
                "E010",
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

        let fn_type = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
        };
        self.check_no_redefinition(&decl.name, span);
        self.env.define(&decl.name, fn_type);
        self.unused
            .defined_sources
            .insert(decl.name.clone(), "function".to_string());

        // Track required (non-default) parameter count
        let required_params = decl.params.iter().filter(|p| p.default.is_none()).count();
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
        let prev_inside_async = self.ctx.inside_async;
        self.ctx.current_return_type = Some(return_type.clone());
        self.ctx.inside_async = decl.async_fn;

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
                            param.name,
                            ty.display_name(),
                            default_ty.display_name()
                        ),
                        param.span,
                        "E001",
                        format!("expected `{}`", ty.display_name()),
                    );
                }
            }
        }

        // Check body
        let body_type = self.check_expr(&decl.body);

        // When no return type annotation, infer from body and update the function type
        if decl.return_type.is_none() && !matches!(body_type, Type::Var(_) | Type::Unknown) {
            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(body_type.clone()),
            };
            // Update in the name_types map for hover display
            self.name_types
                .insert(decl.name.clone(), fn_type.display_name());
            // Mark for updating in outer scope after pop
            self.env.define_in_parent_scope(&decl.name, fn_type);
        }

        // Check return type compatibility
        if let Some(ref declared_return) = decl.return_type {
            let resolved = self.resolve_type(declared_return);
            // For async functions, unwrap Promise from the declared type since
            // the body type is the unwrapped inner value
            let effective_declared = match (&resolved, decl.async_fn) {
                (Type::Promise(inner), true) => inner.as_ref().clone(),
                _ => resolved.clone(),
            };
            if !self.types_compatible(&effective_declared, &body_type)
                && !matches!(body_type, Type::Var(_))
            {
                self.emit_error(
                    format!(
                        "function `{}`: expected return type `{}`, found `{}`",
                        decl.name,
                        resolved.display_name(),
                        body_type.display_name()
                    ),
                    span,
                    "E001",
                    format!("expected `{}`", resolved.display_name()),
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
                        decl.name,
                        resolved.display_name()
                    ),
                    span,
                    "E013",
                    "missing return value",
                    "add a return expression or change return type to `()`",
                );
            }
        }

        self.env.pop_scope();
        self.ctx.current_return_type = prev_return_type;
        self.ctx.inside_async = prev_inside_async;
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
            let type_display = for_type.display_name();
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

            let fn_type = Type::Function {
                params: param_types.clone(),
                return_type: Box::new(return_type.clone()),
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
            self.ctx.current_return_type = Some(return_type.clone());

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
                                param.name,
                                ty.display_name(),
                                default_ty.display_name()
                            ),
                            param.span,
                            "E001",
                            format!("expected `{}`", ty.display_name()),
                        );
                    }
                }
            }

            let body_type = self.check_expr(&func.body);

            if let Some(ref declared_return) = func.return_type {
                let resolved = self.resolve_type(declared_return);
                if !self.types_compatible(&resolved, &body_type)
                    && !matches!(body_type, Type::Var(_))
                {
                    self.emit_error(
                        format!(
                            "function `{}`: expected return type `{}`, found `{}`",
                            func.name,
                            resolved.display_name(),
                            body_type.display_name()
                        ),
                        block.span,
                        "E001",
                        format!("expected `{}`", resolved.display_name()),
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
                            format!(
                                "assert expression must be boolean, found `{}`",
                                ty.display_name()
                            ),
                            *span,
                            "E017",
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
                if name.starts_with(|c: char| c.is_uppercase()) {
                    self.unused.used_names.insert(name.clone());
                    if self.env.lookup(name).is_none() {
                        self.emit_error(
                            format!("component `{name}` is not defined"),
                            element.span,
                            "E002",
                            "not found in scope",
                        );
                    }
                }
                for prop in props {
                    match prop {
                        JsxProp::Named {
                            name: prop_name,
                            value,
                            ..
                        } => {
                            if let Some(value) = value {
                                if prop_name.starts_with("on") && prop_name.len() > 2 {
                                    let prev = self.ctx.event_handler_context;
                                    self.ctx.event_handler_context = true;
                                    self.check_expr(value);
                                    self.ctx.event_handler_context = prev;
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
                                    self.check_expr(value);
                                    self.ctx.lambda_param_hints = prev;
                                } else {
                                    self.check_expr(value);
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
