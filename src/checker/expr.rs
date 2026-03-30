use super::*;
use crate::type_layout;

/// Check if a string is a single uppercase letter (generic type parameter).
fn is_generic_param(s: &str) -> bool {
    s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Parse a Foreign type string like "Context<AuthContextValue>" into
/// ("Context", ["AuthContextValue"]). Returns None if no generics.
fn parse_foreign_generics(s: &str) -> Option<(String, Vec<String>)> {
    let open = s.find('<')?;
    let base = s[..open].to_string();
    let inner = s.get(open + 1..s.len() - 1)?; // strip < and >
    // Split by top-level commas (respecting nested <>)
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    Some((base, args))
}

// ── Expression Checking ──────────────────────────────────────

impl Checker {
    pub(super) fn check_expr(&mut self, expr: &Expr) -> Type {
        let ty = self.check_expr_inner(expr);
        self.expr_types.insert(expr.id, ty.clone());
        ty
    }

    fn check_expr_inner(&mut self, expr: &Expr) -> Type {
        match &expr.kind {
            ExprKind::Number(_) => Type::Number,
            ExprKind::String(_) => Type::String,
            ExprKind::TemplateLiteral(parts) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        self.check_expr(e);
                    }
                }
                Type::String
            }
            ExprKind::Bool(_) => Type::Bool,

            ExprKind::Identifier(name) => {
                self.unused.used_names.insert(name.clone());
                // Check for ambiguous bare variant usage
                if let Some(unions) = self.ambiguous_variants.get(name) {
                    let union_list = unions.join("` and `");
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "variant `{name}` is ambiguous — defined in both `{union_list}`"
                            ),
                            expr.span,
                        )
                        .with_help(format!(
                            "use a qualified name: {}",
                            unions
                                .iter()
                                .map(|u| format!("`{u}.{name}`"))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ))
                        .with_code("E017"),
                    );
                }
                if let Some(ty) = self.env.lookup(name).cloned() {
                    // Non-unit variant as bare identifier → constructor function
                    if let Type::Union { ref variants, .. } = ty
                        && let Some((_, field_types)) = variants.iter().find(|(v, _)| v == name)
                        && !field_types.is_empty()
                    {
                        return Type::Function {
                            params: field_types.clone(),
                            return_type: Box::new(ty),
                        };
                    }
                    ty
                } else if self.stdlib.is_module(name) {
                    // Stdlib module names (Array, String, etc.) are valid identifiers
                    Type::Unknown
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(format!("`{name}` is not defined"), expr.span)
                            .with_label("not found in scope")
                            .with_code("E002"),
                    );
                    Type::Unknown
                }
            }

            ExprKind::Placeholder => Type::Unknown,

            ExprKind::Binary { left, op, right } => self.check_binary(left, *op, right, expr.span),

            ExprKind::Unary { op, operand } => {
                let ty = self.check_expr(operand);
                match op {
                    UnaryOp::Neg => {
                        if !ty.is_numeric() && !matches!(ty, Type::Unknown | Type::Var(_)) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "cannot negate type `{}`, expected `number`",
                                        ty.display_name()
                                    ),
                                    expr.span,
                                )
                                .with_label("expected `number`")
                                .with_code("E001"),
                            );
                        }
                        Type::Number
                    }
                    UnaryOp::Not => Type::Bool,
                }
            }

            ExprKind::Pipe { left, right } => {
                let left_ty = self.check_expr(left);
                self.check_pipe_right(&left_ty, right)
            }

            ExprKind::Unwrap(inner) => {
                let ty = self.check_expr(inner);
                // Rule 5: ? only allowed in functions returning Result/Option,
                // OR inside a collect block (where ? accumulates errors)
                if !self.ctx.inside_collect {
                    match &self.ctx.current_return_type {
                        Some(ret) if ret.is_result() || ret.is_option() => {}
                        Some(_) => {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "`?` operator requires function to return `Result` or `Option`",
                                    expr.span,
                                )
                                .with_label(
                                    "enclosing function does not return `Result` or `Option`",
                                )
                                .with_help(
                                    "change the function's return type to `Result` or `Option`",
                                )
                                .with_code("E005"),
                            );
                        }
                        None => {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "`?` operator can only be used inside a function",
                                    expr.span,
                                )
                                .with_label("not inside a function")
                                .with_code("E005"),
                            );
                        }
                    }
                }
                // Unwrap the inner type
                match ty {
                    Type::Result { ok, err } => {
                        if self.ctx.inside_collect {
                            self.ctx.collect_err_type = Some(*err);
                        }
                        *ok
                    }
                    Type::Option(inner) => *inner,
                    _ => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "`?` can only be used on `Result` or `Option`, found `{}`",
                                    ty.display_name()
                                ),
                                expr.span,
                            )
                            .with_label("not a `Result` or `Option`")
                            .with_code("E005"),
                        );
                        Type::Unknown
                    }
                }
            }

            ExprKind::Call {
                callee,
                type_args,
                args,
            } => self.check_call(callee, type_args, args, expr.span),

            ExprKind::Construct {
                type_name,
                spread,
                args,
            } => self.check_construct(type_name, spread.as_deref(), args, expr.span),

            ExprKind::Member { object, field } => {
                let obj_ty = self.check_expr(object);

                // Check for npm member access via tsgo probes (e.g. z.object, z.string)
                if let ExprKind::Identifier(name) = &object.kind {
                    let member_key = format!("__member_{name}_{field}");
                    for exports in self.dts_imports.values() {
                        if let Some(export) = exports.iter().find(|e| e.name == member_key) {
                            let ty = crate::interop::wrap_boundary_type(&export.ts_type);
                            self.name_types.insert(member_key, ty.display_name());
                            return ty;
                        }
                    }
                }

                // Allow stdlib module access (e.g. JSON.parse) before unknown check
                if matches!(obj_ty, Type::Unknown)
                    && let ExprKind::Identifier(name) = &object.kind
                    && self.stdlib.is_module(name)
                    && let Some(stdlib_fn) = self.stdlib.lookup(name, field)
                {
                    return stdlib_fn.return_type.clone();
                }

                self.resolve_member_type(&obj_ty, field, expr.span)
            }

            ExprKind::Index { object, index } => {
                let obj_ty = self.check_expr(object);
                self.check_expr(index);
                if let Type::Array(inner) = obj_ty {
                    Type::Option(inner)
                } else {
                    Type::Unknown
                }
            }

            ExprKind::Arrow { params, body, .. } => {
                self.env.push_scope();
                let param_types: Vec<_> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or_else(|| {
                                // Use lambda param hint from calling context if available
                                if let Some(hint) = self.ctx.lambda_param_hint.take() {
                                    return hint;
                                }
                                // In event handler context, infer Event type for the parameter
                                if self.ctx.event_handler_context && p.destructure.is_none() {
                                    Type::Named("Event".to_string())
                                } else {
                                    self.fresh_type_var()
                                }
                            });
                        self.env.define(&p.name, ty.clone());
                        // Persist lambda param type for LSP hover (scope is
                        // popped after the arrow body is checked, so the param
                        // would be lost from the final name_types merge)
                        self.name_types.insert(p.name.clone(), ty.display_name());
                        // For destructured params, also define the field names
                        if let Some(ref destructure) = p.destructure {
                            match destructure {
                                ParamDestructure::Object(fields) => {
                                    for f in fields {
                                        let name = f.bound_name();
                                        let field_ty = match f.field.as_str() {
                                            "error" => {
                                                Type::Named(type_layout::TYPE_ERROR.to_string())
                                            }
                                            _ => Type::Unknown,
                                        };
                                        self.env.define(name, field_ty);
                                    }
                                }
                                ParamDestructure::Array(fields) => {
                                    for field in fields {
                                        let field_ty = match field.as_str() {
                                            "error" => {
                                                Type::Named(type_layout::TYPE_ERROR.to_string())
                                            }
                                            _ => Type::Unknown,
                                        };
                                        self.env.define(field, field_ty);
                                    }
                                }
                            }
                        }
                        ty
                    })
                    .collect();
                let return_type = self.check_expr(body);
                self.env.pop_scope();
                Type::Function {
                    params: param_types,
                    return_type: Box::new(return_type),
                }
            }

            ExprKind::Match { subject, arms } => {
                let subject_ty = self.check_expr(subject);
                self.check_match_exhaustiveness(&subject_ty, arms, expr.span);

                let mut result_type: Option<Type> = None;
                for arm in arms {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &subject_ty);
                    // Type-check guard expression if present
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard);
                    }
                    let arm_type = self.check_expr(&arm.body);
                    self.env.pop_scope();

                    if let Some(ref first_type) = result_type {
                        if !self.types_unifiable(first_type, &arm_type)
                            && !matches!(arm_type, Type::Unknown | Type::Var(_))
                            && !matches!(first_type, Type::Unknown | Type::Var(_))
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "match arms have incompatible types: first arm returns `{}`, this arm returns `{}`",
                                        first_type.display_name(),
                                        arm_type.display_name()
                                    ),
                                    arm.body.span,
                                )
                                .with_label(format!("expected `{}`", first_type.display_name()))
                                .with_code("E001"),
                            );
                        }
                        result_type = Some(Self::merge_types(first_type, &arm_type));
                    } else {
                        result_type = Some(arm_type);
                    }
                }
                result_type.unwrap_or(Type::Unit)
            }

            ExprKind::Await(inner) => {
                let ty = self.check_expr(inner);
                match ty {
                    Type::Promise(inner) => *inner,
                    other => other,
                }
            }

            ExprKind::Try(inner) => {
                let inner_ty =
                    self.with_context(|ctx| ctx.inside_try = true, |this| this.check_expr(inner));
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(Type::Named(type_layout::TYPE_ERROR.to_string())),
                }
            }

            ExprKind::Parse { type_arg, value } => {
                // parse<T>(value) returns Result<T, Error>
                let t = self.resolve_type(type_arg);
                // Check the value expression (should be unknown/any at runtime)
                if !matches!(value.kind, ExprKind::Placeholder) {
                    self.check_expr(value);
                }
                Type::Result {
                    ok: Box::new(t),
                    err: Box::new(Type::Named(type_layout::TYPE_ERROR.to_string())),
                }
            }

            ExprKind::Mock {
                type_arg,
                overrides,
            } => {
                // mock<T> returns T — check override expressions
                let t = self.resolve_type(type_arg);
                for arg in overrides {
                    match arg {
                        Arg::Positional(e) => {
                            self.check_expr(e);
                        }
                        Arg::Named { value, .. } => {
                            self.check_expr(value);
                        }
                    }
                }
                t
            }

            ExprKind::Ok(inner) => {
                let inner_ty = self.check_expr(inner);
                // Infer error type from enclosing function's return type if available
                let err_ty = match &self.ctx.current_return_type {
                    Some(Type::Result { err, .. }) => (**err).clone(),
                    _ => Type::Unknown,
                };
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(err_ty),
                }
            }

            ExprKind::Err(inner) => {
                let err_ty = self.check_expr(inner);
                // Infer ok type from enclosing function's return type if available
                let ok_ty = match &self.ctx.current_return_type {
                    Some(Type::Result { ok, .. }) => (**ok).clone(),
                    _ => Type::Unknown,
                };
                Type::Result {
                    ok: Box::new(ok_ty),
                    err: Box::new(err_ty),
                }
            }

            ExprKind::Some(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Option(Box::new(inner_ty))
            }

            ExprKind::None => Type::Option(Box::new(Type::Unknown)),

            ExprKind::Value(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Settable(Box::new(inner_ty))
            }
            ExprKind::Clear => Type::Settable(Box::new(Type::Unknown)),
            ExprKind::Unchanged => Type::Settable(Box::new(Type::Unknown)),

            ExprKind::Todo => {
                self.diagnostics.push(
                    Diagnostic::warning(
                        "`todo` is a placeholder that will panic at runtime",
                        expr.span,
                    )
                    .with_label("not yet implemented")
                    .with_help("replace with actual implementation before shipping")
                    .with_code("W002"),
                );
                Type::Never
            }

            ExprKind::Unreachable => Type::Never,

            ExprKind::Unit => Type::Unit,

            ExprKind::Jsx(element) => {
                self.check_jsx(element);
                Type::Named("JSX.Element".to_string())
            }

            ExprKind::Collect(items) => {
                // collect { ... } — accumulates errors from ? instead of short-circuiting
                // The block returns Result<T, Array<E>> where T is the last expression type
                // and E is the error type from ? operations
                self.env.push_scope();
                let prev_inside_collect = self.ctx.inside_collect;
                self.ctx.inside_collect = true;
                let mut last_type = Type::Unit;
                let mut err_type: Option<Type> = None;

                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last {
                        if let ItemKind::Expr(e) = &item.kind {
                            last_type = self.check_expr(e);
                        } else {
                            self.check_item(item);
                        }
                    } else {
                        self.check_item(item);
                    }
                    // Collect error types from ? operations within
                    // (The checker tracks them via collect_err_type)
                    if let Some(ref et) = self.ctx.collect_err_type
                        && err_type.is_none()
                    {
                        err_type = Some(et.clone());
                    }
                }

                self.ctx.inside_collect = prev_inside_collect;
                self.ctx.collect_err_type = None;
                self.env.pop_scope();

                let e = err_type.unwrap_or(Type::Unknown);
                Type::Result {
                    ok: Box::new(last_type),
                    err: Box::new(Type::Array(Box::new(e))),
                }
            }

            ExprKind::Block(items) => self.in_scope(|this| {
                let mut last_type = Type::Unit;
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last {
                        if let ItemKind::Expr(expr) = &item.kind {
                            // Check last expression once and use its type as block type
                            last_type = this.check_expr(expr);
                        } else {
                            this.check_item(item);
                        }
                    } else {
                        this.check_item(item);
                    }
                }
                last_type
            }),

            ExprKind::Grouped(inner) => self.check_expr(inner),

            ExprKind::Array(elements) => {
                let mut elem_type: Option<Type> = None;
                let mut mixed = false;
                for el in elements {
                    let ty = self.check_expr(el);
                    if let Some(ref prev) = elem_type {
                        if !self.types_compatible(prev, &ty)
                            && !matches!(ty, Type::Unknown | Type::Var(_))
                            && !matches!(prev, Type::Unknown | Type::Var(_))
                        {
                            mixed = true;
                        }
                    } else {
                        elem_type = Some(ty);
                    }
                }
                if mixed {
                    Type::Array(Box::new(Type::Unknown))
                } else {
                    Type::Array(Box::new(elem_type.unwrap_or(Type::Unknown)))
                }
            }

            ExprKind::Tuple(elements) => {
                let types: Vec<Type> = elements.iter().map(|el| self.check_expr(el)).collect();
                Type::Tuple(types)
            }

            ExprKind::Spread(inner) => self.check_expr(inner),

            ExprKind::Object(fields) => {
                for (_key, value) in fields {
                    self.check_expr(value);
                }
                Type::Unknown
            }

            ExprKind::DotShorthand { predicate, .. } => {
                // Check the predicate RHS expression if present
                if let Some((_op, rhs)) = predicate {
                    self.check_expr(rhs);
                }
                // Dot shorthand produces a function
                Type::Unknown
            }
        }
    }

    // ── Call Expression Checking ──────────────────────────────────

    fn check_call(
        &mut self,
        callee: &Expr,
        type_args: &[TypeExpr],
        args: &[Arg],
        span: Span,
    ) -> Type {
        // Check for stdlib call: Array.sort(arr), Option.map(opt, fn), etc.
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
        {
            let ret = stdlib_fn.return_type.clone();
            let expected_param_count = stdlib_fn.params.len();
            let variadic = stdlib_fn.is_variadic();
            let display = format!("{module}.{field}");
            self.unused.used_names.insert(module.clone());

            let mut arg_count = 0;
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        self.check_expr(e);
                        arg_count += 1;
                    }
                }
            }

            if !variadic && arg_count != expected_param_count {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "`{display}` expects {} argument{}, found {}",
                            expected_param_count,
                            if expected_param_count == 1 { "" } else { "s" },
                            arg_count
                        ),
                        span,
                    )
                    .with_label("wrong number of arguments")
                    .with_code("E001"),
                );
            }

            return ret;
        }

        // Check for untrusted import call without try
        if let ExprKind::Identifier(name) = &callee.kind
            && !self.ctx.inside_try
            && self.untrusted_imports.contains(name)
        {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("calling untrusted import `{name}` requires `try`"),
                    span,
                )
                .with_label("untrusted import")
                .with_help(format!(
                    "use `try {name}(...)` or mark the import as `trusted`"
                ))
                .with_code("E014"),
            );
        }

        // Save pipe context before checking callee (which would consume it)
        let piped_ty = self.ctx.pipe_input_type.take();
        let piped_ty_was_none = piped_ty.is_none();

        // Infer lambda param type from piped array element type
        if let Some(ref piped) = piped_ty
            && let Type::Array(elem_ty) = piped
        {
            self.ctx.lambda_param_hint = Some((**elem_ty).clone());
        }

        // Detect placeholder args for partial application
        let placeholder_count = args
            .iter()
            .filter(|a| match a {
                Arg::Positional(e) | Arg::Named { value: e, .. } => {
                    matches!(e.kind, ExprKind::Placeholder)
                }
            })
            .count();
        let has_placeholder = placeholder_count > 0;

        if placeholder_count > 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    "only one `_` placeholder allowed per call - use `(x, y) => f(x, y)` for multiple parameters",
                    span,
                )
                .with_label("multiple `_` placeholders")
                .with_code("E023"),
            );
        }

        let callee_ty = self.check_expr(callee);
        let mut arg_types: Vec<Type> = args
            .iter()
            .map(|arg| match arg {
                Arg::Positional(e) | Arg::Named { value: e, .. } => self.check_expr(e),
            })
            .collect();
        self.ctx.lambda_param_hint = None;

        // Handle piped value insertion
        if let Some(piped_ty) = piped_ty {
            if has_placeholder {
                for (i, arg) in args.iter().enumerate() {
                    let is_placeholder = match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => {
                            matches!(e.kind, ExprKind::Placeholder)
                        }
                    };
                    if is_placeholder {
                        arg_types[i] = piped_ty.clone();
                    }
                }
            } else {
                arg_types.insert(0, piped_ty);
            }
        }

        // For-block overload resolution: if callee is a for-block function with
        // multiple overloads, select the one matching the first argument's type
        let callee_ty = if let ExprKind::Identifier(name) = &callee.kind
            && let Some(first_arg) = arg_types.first()
            && let Some(resolved) = self.resolve_for_block_overload(name, first_arg)
        {
            resolved
        } else {
            callee_ty
        };

        match callee_ty {
            Type::Function {
                params,
                return_type,
            } => {
                let callee_name = match &callee.kind {
                    ExprKind::Identifier(name) => name.as_str(),
                    _ => "<anonymous>",
                };

                // Validate named argument labels
                if let Some(param_names) = self.fn_param_names.get(callee_name) {
                    for arg in args.iter() {
                        if let Arg::Named { label, .. } = arg
                            && !param_names.iter().any(|p| p == label)
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "unknown argument `{label}` in call to `{callee_name}`"
                                    ),
                                    span,
                                )
                                .with_label(format!(
                                    "`{label}` is not a parameter of `{callee_name}`"
                                ))
                                .with_help(format!(
                                    "expected one of: {}",
                                    param_names
                                        .iter()
                                        .map(|n| format!("`{n}`"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ))
                                .with_code("E015"),
                            );
                        }
                    }
                }

                let required_params = self
                    .fn_required_params
                    .get(callee_name)
                    .copied()
                    .unwrap_or(params.len());

                // Validate argument count
                if arg_types.len() < required_params || arg_types.len() > params.len() {
                    let expected_msg = if required_params == params.len() {
                        format!(
                            "{} argument{}",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" }
                        )
                    } else {
                        format!("{} to {} arguments", required_params, params.len())
                    };
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "`{callee_name}` expects {expected_msg}, found {}",
                                arg_types.len()
                            ),
                            span,
                        )
                        .with_label("wrong number of arguments")
                        .with_code("E001"),
                    );
                }

                // Resolve generics
                let generic_params = Self::collect_generic_params(&params, &return_type);
                let return_type = if !generic_params.is_empty() {
                    let substitutions = if !type_args.is_empty() {
                        let resolved: Vec<Type> =
                            type_args.iter().map(|t| self.resolve_type(t)).collect();
                        generic_params.into_iter().zip(resolved).collect()
                    } else {
                        Self::infer_generic_params(&generic_params, &params, &arg_types)
                    };
                    if substitutions.is_empty() {
                        *return_type
                    } else {
                        Self::substitute_generics(&return_type, &substitutions)
                    }
                } else {
                    *return_type
                };

                if has_placeholder && piped_ty_was_none {
                    // Partial application: type-check non-placeholder args, return function
                    for (i, (arg_ty, param_ty)) in arg_types.iter().zip(params.iter()).enumerate() {
                        let is_placeholder = match &args[i] {
                            Arg::Positional(e) | Arg::Named { value: e, .. } => {
                                matches!(e.kind, ExprKind::Placeholder)
                            }
                        };
                        if is_placeholder {
                            continue;
                        }
                        if !self.types_compatible(param_ty, arg_ty) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "argument {} to `{callee_name}`: expected `{}`, found `{}`",
                                        i + 1,
                                        param_ty.display_name(),
                                        arg_ty.display_name()
                                    ),
                                    span,
                                )
                                .with_label(format!("expected `{}`", param_ty.display_name()))
                                .with_code("E001"),
                            );
                        }
                    }

                    let placeholder_param_types: Vec<Type> = args
                        .iter()
                        .enumerate()
                        .filter_map(|(i, arg)| {
                            let is_placeholder = match arg {
                                Arg::Positional(e) | Arg::Named { value: e, .. } => {
                                    matches!(e.kind, ExprKind::Placeholder)
                                }
                            };
                            if is_placeholder {
                                params.get(i).cloned()
                            } else {
                                None
                            }
                        })
                        .collect();

                    Type::Function {
                        params: placeholder_param_types,
                        return_type: Box::new(return_type),
                    }
                } else {
                    // Normal call: check all argument types
                    for (i, (arg_ty, param_ty)) in arg_types.iter().zip(params.iter()).enumerate() {
                        if !self.types_compatible(param_ty, arg_ty) {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "argument {} to `{callee_name}`: expected `{}`, found `{}`",
                                        i + 1,
                                        param_ty.display_name(),
                                        arg_ty.display_name()
                                    ),
                                    span,
                                )
                                .with_label(format!("expected `{}`", param_ty.display_name()))
                                .with_code("E001"),
                            );
                        }
                    }
                    return_type
                }
            }
            _ => Type::Unknown,
        }
    }

    // ── Binary Expression Checking ───────────────────────────────

    fn check_binary(&mut self, left: &Expr, op: BinOp, right: &Expr, span: Span) -> Type {
        let left_ty = self.check_expr(left);
        let right_ty = self.check_expr(right);

        match op {
            // Rule 8: == only between same types
            BinOp::Eq | BinOp::NotEq => {
                if !self.types_compatible(&left_ty, &right_ty)
                    && !matches!(left_ty, Type::Unknown | Type::Var(_))
                    && !matches!(right_ty, Type::Unknown | Type::Var(_))
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot compare `{}` with `{}`",
                                left_ty.display_name(),
                                right_ty.display_name()
                            ),
                            span,
                        )
                        .with_label("mismatched types")
                        .with_help("both sides of `==` must have the same type")
                        .with_code("E008"),
                    );
                }
                Type::Bool
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => Type::Bool,
            BinOp::And | BinOp::Or => Type::Bool,
            BinOp::Add => {
                // Rule 12: String concat with + warning
                if matches!(left_ty, Type::String) || matches!(right_ty, Type::String) {
                    self.diagnostics.push(
                        Diagnostic::warning(
                            "use template literal instead of `+` for string concatenation",
                            span,
                        )
                        .with_label("prefer template literal")
                        .with_help("use `\"${a}${b}\"` instead")
                        .with_code("W002"),
                    );
                }
                if matches!(left_ty, Type::Number) && matches!(right_ty, Type::Number) {
                    Type::Number
                } else if matches!(left_ty, Type::String) || matches!(right_ty, Type::String) {
                    Type::String
                } else {
                    left_ty
                }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Type::Number,
        }
    }

    fn check_construct(
        &mut self,
        type_name: &str,
        spread: Option<&Expr>,
        args: &[Arg],
        span: Span,
    ) -> Type {
        self.unused.used_names.insert(type_name.to_string());

        let type_info = self.env.lookup_type(type_name).cloned();
        if type_info.is_none() {
            let is_variant = self
                .env
                .lookup(type_name)
                .is_some_and(|ty| matches!(ty, Type::Union { .. }));
            let is_known_value = self.env.lookup(type_name).is_some();
            if !is_variant && !is_known_value {
                self.diagnostics.push(
                    Diagnostic::error(format!("unknown type `{type_name}`"), span)
                        .with_label("not defined")
                        .with_code("E002"),
                );
            }
        }

        // Zero-arg reference to non-unit variant → constructor function
        if args.is_empty()
            && spread.is_none()
            && let Some(ty) = self.env.lookup(type_name).cloned()
            && let Type::Union { variants, .. } = &ty
            && let Some((_, field_types)) = variants.iter().find(|(v, _)| v == type_name)
            && !field_types.is_empty()
        {
            return Type::Function {
                params: field_types.clone(),
                return_type: Box::new(ty),
            };
        }

        // Rule 3: Opaque enforcement
        if let Some(ref info) = type_info
            && info.opaque
        {
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "cannot construct opaque type `{type_name}` outside its defining module"
                    ),
                    span,
                )
                .with_label("opaque type cannot be constructed directly")
                .with_help("use the module's exported constructor function instead")
                .with_code("E003"),
            );
        }

        // Collect valid field names for this type
        let valid_fields: Option<Vec<String>> = if let Some(ref info) = type_info {
            match &info.def {
                TypeDef::Record(entries) => Some(
                    entries
                        .iter()
                        .filter_map(|e| e.as_field())
                        .map(|f| f.name.clone())
                        .collect(),
                ),
                _ => None,
            }
        } else {
            self.env
                .lookup(type_name)
                .cloned()
                .and_then(|ty| {
                    if let Type::Union { name, .. } = &ty {
                        self.env.lookup_type(name).cloned()
                    } else {
                        None
                    }
                })
                .and_then(|info| {
                    if let TypeDef::Union(variants) = &info.def {
                        variants
                            .iter()
                            .find(|v| v.name == *type_name)
                            .map(|v| v.fields.iter().filter_map(|f| f.name.clone()).collect())
                    } else {
                        None
                    }
                })
        };

        // Validate named arguments against known fields
        if let Some(ref fields) = valid_fields {
            let named_labels: Vec<&str> = args
                .iter()
                .filter_map(|a| {
                    if let Arg::Named { label, .. } = a {
                        Some(label.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            for label in &named_labels {
                if !fields.iter().any(|f| f == label) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!("unknown field `{label}` on type `{type_name}`"),
                            span,
                        )
                        .with_label(format!("`{label}` is not a field of `{type_name}`"))
                        .with_help(format!(
                            "available fields: {}",
                            fields
                                .iter()
                                .map(|f| format!("`{f}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ))
                        .with_code("E015"),
                    );
                }
            }

            // Check for missing required fields (only when no spread)
            if spread.is_none() {
                let has_defaults: Vec<String> = if let Some(ref info) = type_info {
                    if let TypeDef::Record(record_entries) = &info.def {
                        record_entries
                            .iter()
                            .filter_map(|e| e.as_field())
                            .filter(|f| f.default.is_some())
                            .map(|f| f.name.clone())
                            .collect()
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                let positional_count = args
                    .iter()
                    .filter(|a| matches!(a, Arg::Positional(_)))
                    .count();

                for (i, field) in fields.iter().enumerate() {
                    let provided_by_name = named_labels.contains(&field.as_str());
                    let provided_by_position = i < positional_count;
                    let has_default = has_defaults.contains(field);

                    if !provided_by_name && !provided_by_position && !has_default {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "missing required field `{field}` in `{type_name}` constructor"
                                ),
                                span,
                            )
                            .with_label(format!("field `{field}` is required"))
                            .with_code("E016"),
                        );
                    }
                }
            }
        }

        if let Some(spread_expr) = spread {
            let spread_type = self.check_expr(spread_expr);

            if let Type::Record(spread_fields) = &spread_type {
                let spread_keys: Vec<&str> =
                    spread_fields.iter().map(|(k, _)| k.as_str()).collect();
                for arg in args.iter() {
                    if let Arg::Named { label, .. } = arg
                        && spread_keys.contains(&label.as_str())
                    {
                        self.diagnostics.push(
                            Diagnostic::warning(
                                format!(
                                    "field `{label}` from spread is overwritten by explicit field"
                                ),
                                span,
                            )
                            .with_label(format!("`{label}` exists in the spread source"))
                            .with_help("the spread value will be replaced by the explicit field")
                            .with_code("W003"),
                        );
                    }
                }
            }
        }

        // Build field type map for type checking arguments
        let field_type_map: Option<Vec<(String, Type)>> = if let Some(ref info) = type_info {
            match &info.def {
                TypeDef::Record(entries) => Some(
                    entries
                        .iter()
                        .filter_map(|e| e.as_field())
                        .map(|f| (f.name.clone(), self.resolve_type(&f.type_ann)))
                        .collect(),
                ),
                _ => None,
            }
        } else {
            None
        };

        for arg in args {
            match arg {
                Arg::Named {
                    label, value: e, ..
                } => {
                    let arg_ty = self.check_expr(e);
                    if let Some(ref field_types) = field_type_map
                        && let Some((_, expected_ty)) = field_types.iter().find(|(n, _)| n == label)
                    {
                        // Refine None type: if arg is Option<Unknown> and expected
                        // is Option<T>, record the concrete type for hover display
                        if matches!(&arg_ty, Type::Option(inner) if matches!(**inner, Type::Unknown))
                            && matches!(expected_ty, Type::Option(_))
                        {
                            self.expr_types.insert(e.id, expected_ty.clone());
                        }
                    }
                    if let Some(ref field_types) = field_type_map
                        && let Some((_, expected_ty)) = field_types.iter().find(|(n, _)| n == label)
                        && !self.types_compatible(expected_ty, &arg_ty)
                        && !matches!(arg_ty, Type::Unknown | Type::Var(_))
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "field `{label}`: expected `{}`, found `{}`",
                                    expected_ty.display_name(),
                                    arg_ty.display_name()
                                ),
                                span,
                            )
                            .with_label(format!("expected `{}`", expected_ty.display_name()))
                            .with_code("E001"),
                        );
                    }
                }
                Arg::Positional(e) => {
                    self.check_expr(e);
                }
            }
        }

        // Return parent union type for variant constructors
        if let Some(ty) = self.env.lookup(type_name).cloned()
            && let Type::Union { .. } = &ty
        {
            return ty;
        }
        Type::Named(type_name.to_string())
    }

    fn check_pipe_right(&mut self, left_ty: &Type, right: &Expr) -> Type {
        // Handle `x |> Module.func` or `x |> Module.func(args)` — stdlib member access
        let member_info = match &right.kind {
            ExprKind::Member { object, field } => {
                if let ExprKind::Identifier(module) = &object.kind {
                    Some((module.as_str(), field.as_str()))
                } else {
                    None
                }
            }
            ExprKind::Call { callee, .. } => {
                if let ExprKind::Member { object, field } = &callee.kind {
                    if let ExprKind::Identifier(module) = &object.kind {
                        Some((module.as_str(), field.as_str()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((module, func_name)) = member_info
            && let Some(stdlib_fn) = self.stdlib.lookup(module, func_name).cloned()
        {
            self.unused.used_names.insert(module.to_string());
            let display = format!("{module}.{func_name}");
            return self.validate_stdlib_pipe_call(&stdlib_fn, &display, left_ty, right);
        }

        // Extract the bare function name from the right side
        let bare_name = match &right.kind {
            ExprKind::Identifier(name) => Some(name.as_str()),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Identifier(name) => Some(name.as_str()),
                _ => None,
            },
            _ => None,
        };

        // If it's a bare name not locally defined (or is a known stdlib function),
        // try stdlib resolution
        if let Some(name) = bare_name
            && !self.stdlib.is_module(name)
            && (self.env.lookup(name).is_none() || !self.stdlib.lookup_by_name(name).is_empty())
        {
            let module = type_layout::type_to_stdlib_module(left_ty);
            let fallback_matches = self.stdlib.lookup_by_name(name);

            if let Some(m) = module
                && let Some(stdlib_fn) = self.stdlib.lookup(m, name).cloned()
            {
                // Found via type-directed resolution
                self.unused.used_names.insert(name.to_string());
                let display = format!("{m}.{name}");
                return self.validate_stdlib_pipe_call(&stdlib_fn, &display, left_ty, right);
            } else if !fallback_matches.is_empty() && self.env.lookup(name).is_none() {
                // Found via name-based fallback (only if not locally defined)
                let stdlib_fn = fallback_matches[0].clone();
                self.unused.used_names.insert(name.to_string());
                return self.validate_stdlib_pipe_call(&stdlib_fn, name, left_ty, right);
            }
        }

        // For-block overload resolution: if the function has multiple overloads
        // (e.g. toModel on AccentRow vs EntryRow), select based on piped type.
        // Uses a temporary scope so the overload doesn't leak to subsequent code.
        let has_overload = bare_name.is_some_and(|name| {
            self.resolve_for_block_overload(name, left_ty)
                .is_some_and(|fn_type| {
                    self.env.push_scope();
                    self.env.define(name, fn_type);
                    true
                })
        });

        // Default: check normally, with pipe context for arg validation
        let left_ty_clone = left_ty.clone();
        let right_ty = self.with_context(
            |ctx| ctx.pipe_input_type = Some(left_ty_clone),
            |this| this.check_expr(right),
        );

        if has_overload {
            self.env.pop_scope();
        }

        // If the right side is a bare function identifier (not a call),
        // the pipe effectively calls it: `a |> f` means `f(a)`.
        // Return the function's return type, not the function type itself.
        if let ExprKind::Identifier(name) = &right.kind {
            match right_ty {
                Type::Function {
                    params,
                    return_type,
                } => {
                    // Validate the piped value as the first (and only) argument
                    if let Some(first_param) = params.first()
                        && !self.types_compatible(first_param, left_ty)
                    {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "argument 1 to `{name}`: expected `{}`, found `{}`",
                                    first_param.display_name(),
                                    left_ty.display_name()
                                ),
                                right.span,
                            )
                            .with_label(format!("expected `{}`", first_param.display_name()))
                            .with_code("E001"),
                        );
                    }
                    return *return_type;
                }
                // Unknown types: don't error (not enough info)
                Type::Unknown | Type::Var(_) => {}
                // Non-function types: error
                _ => {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot pipe into `{name}`: expected a function, found `{}`",
                                right_ty.display_name()
                            ),
                            right.span,
                        )
                        .with_label("not a function")
                        .with_code("E001"),
                    );
                }
            }
        }

        right_ty
    }

    /// Validate a stdlib function call in a pipe, checking the first parameter type,
    /// resolving generic return types, and checking additional arguments.
    fn validate_stdlib_pipe_call(
        &mut self,
        stdlib_fn: &crate::stdlib::StdlibFn,
        display_name: &str,
        left_ty: &Type,
        right: &Expr,
    ) -> Type {
        if let Some(first_param) = stdlib_fn.params.first()
            && !self.types_compatible(first_param, left_ty)
        {
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "argument 1 to `{display_name}`: expected `{}`, found `{}`",
                        first_param.display_name(),
                        left_ty.display_name()
                    ),
                    right.span,
                )
                .with_label(format!("expected `{}`", first_param.display_name()))
                .with_code("E001"),
            );
        }
        if let Type::Array(elem) = left_ty {
            self.ctx.lambda_param_hint = Some((**elem).clone());
        } else if let Type::Option(inner) = left_ty {
            self.ctx.lambda_param_hint = Some((**inner).clone());
        }

        // Check lambda args and capture return type for generic inference
        let lambda_return = self.check_pipe_right_args_with_return(right);
        self.ctx.lambda_param_hint = None;

        // Resolve return type: if the stdlib fn's return type uses a different
        // type var than the input (e.g. map: Array<T> -> Array<U>), infer U
        // from the lambda's actual return type.
        let infer_from_lambda = lambda_return.is_some()
            && match (&stdlib_fn.return_type, stdlib_fn.params.first()) {
                (Type::Array(ret_elem), Some(Type::Array(in_elem))) => ret_elem != in_elem,
                (Type::Option(ret_elem), Some(Type::Option(in_elem))) => ret_elem != in_elem,
                _ => false,
            };
        match (&stdlib_fn.return_type, left_ty) {
            (Type::Array(_), _) if infer_from_lambda => {
                Type::Array(Box::new(lambda_return.unwrap()))
            }
            (Type::Array(_), Type::Array(elem)) => Type::Array(elem.clone()),
            (Type::Option(_), _) if infer_from_lambda => {
                Type::Option(Box::new(lambda_return.unwrap()))
            }
            (Type::Option(_), Type::Option(inner)) => Type::Option(inner.clone()),
            _ => stdlib_fn.return_type.clone(),
        }
    }

    /// Check arguments in the right side of a pipe and return the lambda's return type.
    fn check_pipe_right_args_with_return(&mut self, right: &Expr) -> Option<Type> {
        let mut lambda_return = None;
        if let ExprKind::Call { args, .. } = &right.kind {
            for (i, arg) in args.iter().enumerate() {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        let ty = self.check_expr(e);
                        if i == 0
                            && let Type::Function { return_type, .. } = &ty
                        {
                            lambda_return = Some(*return_type.clone());
                        }
                    }
                }
            }
        }
        lambda_return
    }

    /// Collect single-letter type param names used in a function signature.
    /// These are `Named("S")`, `Named("T")`, etc. that represent generic params.
    fn collect_generic_params(params: &[Type], return_type: &Type) -> Vec<String> {
        let mut names = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for ty in params.iter().chain(std::iter::once(return_type)) {
            Self::collect_generic_params_from_type(ty, &mut names, &mut seen);
        }
        names
    }

    fn collect_generic_params_from_type(
        ty: &Type,
        names: &mut Vec<String>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        match ty {
            Type::Named(n) if is_generic_param(n) => {
                if seen.insert(n.clone()) {
                    names.push(n.clone());
                }
            }
            Type::Array(inner) | Type::Option(inner) => {
                Self::collect_generic_params_from_type(inner, names, seen);
            }
            Type::Tuple(types) => {
                for t in types {
                    Self::collect_generic_params_from_type(t, names, seen);
                }
            }
            Type::Function {
                params,
                return_type,
            } => {
                for p in params {
                    Self::collect_generic_params_from_type(p, names, seen);
                }
                Self::collect_generic_params_from_type(return_type, names, seen);
            }
            Type::Result { ok, err } => {
                Self::collect_generic_params_from_type(ok, names, seen);
                Self::collect_generic_params_from_type(err, names, seen);
            }
            Type::Map { key, value } => {
                Self::collect_generic_params_from_type(key, names, seen);
                Self::collect_generic_params_from_type(value, names, seen);
            }
            Type::Set { element } => {
                Self::collect_generic_params_from_type(element, names, seen);
            }
            Type::Foreign(s) => {
                // Extract generic params from Foreign strings like "Context<T>"
                if let Some((_, args)) = parse_foreign_generics(s) {
                    for arg in &args {
                        if is_generic_param(arg) && seen.insert(arg.clone()) {
                            names.push(arg.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Infer generic params by matching argument types against parameter types.
    /// e.g., param `S` with arg `string` → S = string
    fn infer_generic_params(
        generic_params: &[String],
        param_types: &[Type],
        arg_types: &[Type],
    ) -> HashMap<String, Type> {
        let mut subs = HashMap::new();
        for (param_ty, arg_ty) in param_types.iter().zip(arg_types.iter()) {
            Self::unify_for_inference(param_ty, arg_ty, generic_params, &mut subs);
        }
        subs
    }

    /// Try to unify a parameter type with an argument type to infer generic params.
    fn unify_for_inference(
        param: &Type,
        arg: &Type,
        generics: &[String],
        subs: &mut HashMap<String, Type>,
    ) {
        match (param, arg) {
            // Named("S") matches anything if S is a generic param
            (Type::Named(n), _) if generics.contains(n) && !matches!(arg, Type::Unknown) => {
                subs.entry(n.clone()).or_insert_with(|| arg.clone());
            }
            // Recurse into compound types
            (Type::Array(p), Type::Array(a)) => {
                Self::unify_for_inference(p, a, generics, subs);
            }
            (Type::Map { key: pk, value: pv }, Type::Map { key: ak, value: av }) => {
                Self::unify_for_inference(pk, ak, generics, subs);
                Self::unify_for_inference(pv, av, generics, subs);
            }
            (Type::Set { element: pe }, Type::Set { element: ae }) => {
                Self::unify_for_inference(pe, ae, generics, subs);
            }
            (Type::Option(p), Type::Option(a)) => {
                Self::unify_for_inference(p, a, generics, subs);
            }
            (Type::Result { ok: po, err: pe }, Type::Result { ok: ao, err: ae }) => {
                Self::unify_for_inference(po, ao, generics, subs);
                Self::unify_for_inference(pe, ae, generics, subs);
            }
            // Foreign types with matching base names: extract and unify generic args
            // e.g., Foreign("Context<T>") with Foreign("Context<AuthContextValue>")
            (Type::Foreign(p_str), Type::Foreign(a_str)) => {
                if let Some((p_base, p_args)) = parse_foreign_generics(p_str)
                    && let Some((a_base, a_args)) = parse_foreign_generics(a_str)
                    && p_base == a_base
                    && p_args.len() == a_args.len()
                {
                    for (p_arg, a_arg) in p_args.iter().zip(a_args.iter()) {
                        // If the param arg is a single uppercase letter (generic), bind it
                        if is_generic_param(p_arg) && generics.contains(p_arg) {
                            subs.entry(p_arg.clone())
                                .or_insert_with(|| Type::Foreign(a_arg.clone()));
                        }
                    }
                }
            }
            // Union param: try matching arg against first non-generic member
            // e.g., S | (() => S) with arg "hello" → S = string
            (Type::Named(n), _) if generics.contains(n) => {
                subs.entry(n.clone()).or_insert_with(|| arg.clone());
            }
            _ => {}
        }
    }

    /// Substitute generic type params (e.g. Named("S") → Array<Todo>) in a type.
    fn substitute_generics(ty: &Type, subs: &HashMap<String, Type>) -> Type {
        match ty {
            Type::Named(n) if subs.contains_key(n) => subs[n].clone(),
            Type::Array(inner) => Type::Array(Box::new(Self::substitute_generics(inner, subs))),
            Type::Map { key, value } => Type::Map {
                key: Box::new(Self::substitute_generics(key, subs)),
                value: Box::new(Self::substitute_generics(value, subs)),
            },
            Type::Set { element } => Type::Set {
                element: Box::new(Self::substitute_generics(element, subs)),
            },
            Type::Option(inner) => Type::Option(Box::new(Self::substitute_generics(inner, subs))),
            Type::Tuple(types) => Type::Tuple(
                types
                    .iter()
                    .map(|t| Self::substitute_generics(t, subs))
                    .collect(),
            ),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params
                    .iter()
                    .map(|t| Self::substitute_generics(t, subs))
                    .collect(),
                return_type: Box::new(Self::substitute_generics(return_type, subs)),
            },
            Type::Result { ok, err } => Type::Result {
                ok: Box::new(Self::substitute_generics(ok, subs)),
                err: Box::new(Self::substitute_generics(err, subs)),
            },
            other => other.clone(),
        }
    }

    /// Resolve the correct for-block overload for a function name based on the
    /// dispatch type (first arg or piped value). Returns None if no overload matches
    /// or if the function has only one definition.
    fn resolve_for_block_overload(&self, name: &str, dispatch_ty: &Type) -> Option<Type> {
        let overloads = self.for_block_overloads.get(name)?;
        if overloads.len() <= 1 {
            return None;
        }
        let dispatch_name = match dispatch_ty {
            Type::Named(n) | Type::Foreign(n) => n.as_str(),
            _ => &dispatch_ty.display_name(),
        };
        let (_, fn_type) = overloads
            .iter()
            .find(|(type_name, _)| type_name == dispatch_name)?;
        Some(fn_type.clone())
    }

    /// Resolve the type of a member access (`obj_ty.field`), producing diagnostics for errors.
    fn resolve_member_type(&mut self, obj_ty: &Type, field: &str, span: Span) -> Type {
        // Rule 6: No property access on unnarrowed unions
        if let Type::Result { .. } = obj_ty {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("cannot access `.{field}` on `Result` - use `match` or `?` first"),
                    span,
                )
                .with_label("`Result` must be narrowed first")
                .with_help("use `match result { Ok(v) -> ..., Err(e) -> ... }`")
                .with_code("E006"),
            );
            return Type::Unknown;
        }
        if let Type::Union { name, .. } = obj_ty {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("cannot access `.{field}` on union `{name}` - use `match` first"),
                    span,
                )
                .with_label("union must be narrowed first")
                .with_help("use `match` to narrow the union first")
                .with_code("E006"),
            );
            return Type::Unknown;
        }

        // Error on member access on Promise — must await first
        if let Type::Promise(_) = obj_ty {
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "cannot access `.{field}` on `{}` — use `await` first",
                        obj_ty.display_name()
                    ),
                    span,
                )
                .with_label("must `await` the Promise before accessing members")
                .with_code("E021"),
            );
            return Type::Unknown;
        }

        // Error on member access on `unknown` — must narrow first
        if matches!(obj_ty, Type::Unknown) {
            self.diagnostics.push(
                Diagnostic::error(format!("cannot access `.{field}` on `unknown`"), span)
                    .with_label("`unknown` must be narrowed before member access")
                    .with_help("use `match`, type validation (e.g. Zod), or pattern matching")
                    .with_code("E020"),
            );
            return Type::Unknown;
        }

        // Resolve Named types to their concrete definition
        let concrete = self.resolve_type_to_concrete(obj_ty);

        if let Type::Record(fields) = &concrete {
            if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                return ty.clone();
            }
            let type_name = if let Type::Named(name) = obj_ty {
                format!("`{name}`")
            } else {
                format!("`{}`", obj_ty.display_name())
            };
            self.diagnostics.push(
                Diagnostic::error(format!("type {type_name} has no field `{field}`"), span)
                    .with_label("unknown field")
                    .with_help(format!(
                        "available fields: {}",
                        fields
                            .iter()
                            .map(|(n, _)| format!("`{n}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .with_code("E017"),
            );
            return Type::Unknown;
        }

        // Tuple index access: pair.0, pair.1
        if let Type::Tuple(elements) = &concrete
            && let Ok(idx) = field.parse::<usize>()
        {
            if idx < elements.len() {
                return elements[idx].clone();
            }
            self.diagnostics.push(
                Diagnostic::error(
                    format!(
                        "tuple index `{field}` out of bounds — tuple has {} element(s)",
                        elements.len()
                    ),
                    span,
                )
                .with_code("E017"),
            );
            return Type::Unknown;
        }

        // Error on member access on primitive types
        match obj_ty {
            Type::Number | Type::String | Type::Bool | Type::Unit => {
                self.diagnostics.push(
                    Diagnostic::error(
                        format!(
                            "cannot access `.{field}` on type `{}`",
                            obj_ty.display_name()
                        ),
                        span,
                    )
                    .with_label("not a record type")
                    .with_code("E017"),
                );
                return Type::Unknown;
            }
            _ => {}
        }

        // Foreign types: allow member access, return Foreign for chained access
        if let Type::Foreign(name) = obj_ty {
            return Type::Foreign(format!("{name}.{field}"));
        }

        // Named type that couldn't resolve to concrete — if no local type definition
        // exists, treat as foreign (the type came from npm through cross-file propagation).
        // If it HAS a local definition, it's a genuine error (missing field).
        if let Type::Named(name) = obj_ty {
            if self.env.lookup_type(name).is_none() {
                return Type::Foreign(format!("{name}.{field}"));
            }
            self.diagnostics.push(
                Diagnostic::error(
                    format!("cannot access `.{field}` on unresolved type `{name}`"),
                    span,
                )
                .with_label("type definition not found")
                .with_help("ensure the type's source module has a .d.ts file or is a .fl file")
                .with_code("E020"),
            );
            return Type::Unknown;
        }

        Type::Unknown
    }

    /// Resolve a type to its concrete definition, following Named type lookups.
    fn resolve_type_to_concrete(&mut self, ty: &Type) -> Type {
        let resolved = self.env.resolve_to_concrete(ty, &simple_resolve_type_expr);
        // If still Named after type_defs resolution, check if it's a known
        // value (e.g. built-in Response, Error) that has a concrete type
        if let Type::Named(name) = &resolved
            && let Some(val_ty) = self.env.lookup(name).cloned()
            && matches!(val_ty, Type::Record(_))
        {
            return val_ty;
        }
        resolved
    }
}

/// Simple type expression resolver for concrete type resolution.
/// Handles Named, Array, Record, and Function type expressions without
/// needing mutable access to the checker (no self parameter).
pub(crate) fn simple_resolve_type_expr(type_expr: &crate::parser::ast::TypeExpr) -> Type {
    use crate::parser::ast::TypeExprKind;
    match &type_expr.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => match name.as_str() {
            type_layout::TYPE_NUMBER => Type::Number,
            type_layout::TYPE_STRING => Type::String,
            type_layout::TYPE_BOOLEAN => Type::Bool,
            type_layout::TYPE_UNIT => Type::Unit,
            type_layout::TYPE_UNDEFINED => Type::Undefined,
            type_layout::TYPE_ARRAY => {
                let inner = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Array(Box::new(inner))
            }
            type_layout::TYPE_OPTION => {
                let inner = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Option(Box::new(inner))
            }
            type_layout::TYPE_SETTABLE => {
                let inner = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Settable(Box::new(inner))
            }
            type_layout::TYPE_RESULT => {
                let ok = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                let err = type_args
                    .get(1)
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Result {
                    ok: Box::new(ok),
                    err: Box::new(err),
                }
            }
            _ => Type::Named(name.to_string()),
        },
        TypeExprKind::Array(inner) => Type::Array(Box::new(simple_resolve_type_expr(inner))),
        TypeExprKind::Record(fields) => {
            let field_types: Vec<_> = fields
                .iter()
                .map(|f| (f.name.clone(), simple_resolve_type_expr(&f.type_ann)))
                .collect();
            Type::Record(field_types)
        }
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            let param_types: Vec<_> = params.iter().map(simple_resolve_type_expr).collect();
            let ret = simple_resolve_type_expr(return_type);
            Type::Function {
                params: param_types,
                return_type: Box::new(ret),
            }
        }
        TypeExprKind::Tuple(types) => {
            Type::Tuple(types.iter().map(simple_resolve_type_expr).collect())
        }
        TypeExprKind::TypeOf(_) => {
            // Without environment context, typeof can't be resolved
            Type::Unknown
        }
        TypeExprKind::StringLiteral(value) => Type::Foreign(format!("\"{value}\"")),
        TypeExprKind::Intersection(types) => {
            let resolved: Vec<Type> = types.iter().map(simple_resolve_type_expr).collect();
            let mut fields = Vec::new();
            let mut all_records = true;
            for ty in resolved.iter() {
                if let Type::Record(f) = ty {
                    fields.extend(f.clone());
                } else {
                    all_records = false;
                }
            }
            if all_records && !fields.is_empty() {
                Type::Record(fields)
            } else {
                resolved.into_iter().next().unwrap_or(Type::Unknown)
            }
        }
    }
}
