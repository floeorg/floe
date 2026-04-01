use super::*;
use crate::type_layout;

/// Check if a string is a single uppercase letter (generic type parameter).
fn is_generic_param(s: &str) -> bool {
    s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// When a destructured param's type is unresolved, use heuristics for known field names.
/// The "error" field maps to Error because Floe's error-handling callbacks (use blocks,
/// fallbackRender) destructure `{ error }`.
fn unresolved_field_heuristic_type(name: &str) -> Type {
    match name {
        type_layout::ERROR_FIELD => Type::Named(type_layout::TYPE_ERROR.to_string()),
        _ => Type::Unknown,
    }
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
            ExprKind::Identifier(name) => self.check_identifier(name, expr.span),
            ExprKind::Placeholder => Type::Unknown,
            ExprKind::Binary { left, op, right } => self.check_binary(left, *op, right, expr.span),
            ExprKind::Unary { op, operand } => self.check_unary(*op, operand, expr.span),
            ExprKind::Pipe { left, right } => {
                let left_ty = self.check_expr(left);
                self.check_pipe_right(&left_ty, right)
            }
            ExprKind::Unwrap(inner) => self.check_unwrap(inner, expr.span),
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
            ExprKind::Member { object, field } => self.check_member(object, field, expr.span),
            ExprKind::Index { object, index } => self.check_index(object, index, expr.span),
            ExprKind::Arrow {
                params,
                body,
                async_fn,
            } => self.check_arrow(params, body, *async_fn),
            ExprKind::Match { subject, arms } => self.check_match(subject, arms, expr.span),
            ExprKind::Await(inner) => {
                if !self.ctx.inside_async {
                    self.emit_error_with_help(
                        "`await` can only be used inside an `async` function",
                        expr.span,
                        ErrorCode::AwaitOutsideAsync,
                        "not inside an `async` function",
                        "add `async` to the enclosing function declaration",
                    );
                }
                let ty = self.check_expr(inner);
                match &ty {
                    Type::Promise(inner) => *inner.clone(),
                    _ if ty.is_result() && matches!(ty.result_ok(), Some(Type::Promise(_))) => {
                        self.emit_error_with_help(
                            "`await try` wraps the Promise in a Result before awaiting",
                            expr.span,
                            ErrorCode::AwaitTryOrder,
                            "wrong order",
                            "use `try await` instead — await the Promise first, then wrap in Result",
                        );
                        ty
                    }
                    _ => ty,
                }
            }
            ExprKind::Try(inner) => {
                let inner_ty =
                    self.with_context(|ctx| ctx.inside_try = true, |this| this.check_expr(inner));
                Type::result_of(inner_ty, Type::Named(type_layout::TYPE_ERROR.to_string()))
            }
            ExprKind::Parse { type_arg, value } => {
                let t = self.resolve_type(type_arg);
                if !matches!(value.kind, ExprKind::Placeholder) {
                    self.check_expr(value);
                }
                Type::result_of(t, Type::Named(type_layout::TYPE_ERROR.to_string()))
            }
            ExprKind::Mock {
                type_arg,
                overrides,
            } => {
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
            // Ok/Err/Some/None are handled by check_construct as variant constructors
            ExprKind::Value(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Settable(Box::new(inner_ty))
            }
            ExprKind::Clear => Type::Settable(Box::new(Type::Unknown)),
            ExprKind::Unchanged => Type::Settable(Box::new(Type::Unknown)),
            ExprKind::Todo => {
                self.emit_warning_with_help(
                    "`todo` is a placeholder that will panic at runtime",
                    expr.span,
                    ErrorCode::TodoPlaceholder,
                    "not yet implemented",
                    "replace with actual implementation before shipping",
                );
                Type::Never
            }
            ExprKind::Unreachable => Type::Never,
            ExprKind::Unit => Type::Unit,
            ExprKind::Jsx(element) => {
                self.check_jsx(element);
                Type::Named("JSX.Element".to_string())
            }
            ExprKind::Collect(items) => self.check_collect(items),
            ExprKind::Block(items) => self.in_scope(|this| {
                let mut last_type = Type::Unit;
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last {
                        if let ItemKind::Expr(expr) = &item.kind {
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
            ExprKind::Array(elements) => self.check_array(elements),
            ExprKind::Tuple(elements) => {
                let types: Vec<Type> = elements.iter().map(|el| self.check_expr(el)).collect();
                Type::Tuple(types)
            }
            ExprKind::Spread(inner) => self.check_expr(inner),
            ExprKind::Object(fields) => {
                let field_types: Vec<(String, Type)> = fields
                    .iter()
                    .map(|(key, value)| (key.clone(), self.check_expr(value)))
                    .collect();
                Type::Record(field_types)
            }
            ExprKind::DotShorthand { predicate, .. } => {
                if let Some((_op, rhs)) = predicate {
                    self.check_expr(rhs);
                }
                Type::Unknown
            }
        }
    }

    // ── Extracted Expression Checkers ────────────────────────────────

    fn check_identifier(&mut self, name: &str, span: Span) -> Type {
        self.unused.used_names.insert(name.to_string());
        // Check for ambiguous bare variant usage
        if let Some(unions) = self.ambiguous_variants.get(name) {
            let union_list = unions.join("` and `");
            self.diagnostics.push(
                Diagnostic::error(
                    format!("variant `{name}` is ambiguous — defined in both `{union_list}`"),
                    span,
                )
                .with_help(format!(
                    "use a qualified name: {}",
                    unions
                        .iter()
                        .map(|u| format!("`{u}.{name}`"))
                        .collect::<Vec<_>>()
                        .join(" or ")
                ))
                .with_error_code(ErrorCode::AmbiguousVariant),
            );
        }
        if let Some(ty) = self.env.lookup(name).cloned() {
            // Non-unit variant as bare identifier → constructor function
            if let Type::Union { ref variants, .. } = ty
                && let Some((_, field_types)) = variants.iter().find(|(v, _)| v == name)
                && !field_types.is_empty()
            {
                let required_params = field_types.len();
                return Type::Function {
                    params: field_types.clone(),
                    return_type: Box::new(ty),
                    required_params,
                };
            }
            ty
        } else if self.stdlib.is_module(name) {
            // Stdlib module names (Array, String, etc.) are valid identifiers
            Type::Unknown
        } else {
            self.emit_error(
                format!("`{name}` is not defined"),
                span,
                ErrorCode::UndefinedName,
                "not found in scope",
            );
            Type::Unknown
        }
    }

    fn check_unary(&mut self, op: UnaryOp, operand: &Expr, span: Span) -> Type {
        let ty = self.check_expr(operand);
        match op {
            UnaryOp::Neg => {
                if !ty.is_numeric() && !matches!(ty, Type::Unknown | Type::Var(_)) {
                    self.emit_error(
                        format!("cannot negate type `{}`, expected `number`", ty),
                        span,
                        ErrorCode::TypeMismatch,
                        "expected `number`",
                    );
                }
                Type::Number
            }
            UnaryOp::Not => {
                let concrete = self.resolve_type_to_concrete(&ty);
                self.check_boolean_operand(&ty, &concrete, span, "!");
                Type::Bool
            }
        }
    }

    fn check_unwrap(&mut self, inner: &Expr, span: Span) -> Type {
        let ty = self.check_expr(inner);
        // Rule 5: ? only allowed in functions returning Result/Option,
        // OR inside a collect block (where ? accumulates errors)
        if !self.ctx.inside_collect {
            match &self.ctx.current_return_type {
                Some(ret) if ret.is_result() || ret.is_option() => {}
                Some(_) => {
                    self.emit_error_with_help(
                        "`?` operator requires function to return `Result` or `Option`",
                        span,
                        ErrorCode::InvalidTryOperator,
                        "enclosing function does not return `Result` or `Option`",
                        "change the function's return type to `Result` or `Option`",
                    );
                }
                None => {
                    self.emit_error(
                        "`?` operator can only be used inside a function",
                        span,
                        ErrorCode::InvalidTryOperator,
                        "not inside a function",
                    );
                }
            }
        }
        // Unwrap the inner type
        if ty.is_result() {
            let ok = ty.result_ok().cloned().unwrap_or(Type::Unknown);
            let err = ty.result_err().cloned().unwrap_or(Type::Unknown);
            if self.ctx.inside_collect {
                self.ctx.collect_err_type = Some(err);
            }
            return ok;
        }
        match ty {
            _ if ty.is_option() => ty.unwrap_option(),
            _ => {
                self.emit_error(
                    format!(
                        "`?` can only be used on `Result` or `Option`, found `{}`",
                        ty
                    ),
                    span,
                    ErrorCode::InvalidTryOperator,
                    "not a `Result` or `Option`",
                );
                Type::Unknown
            }
        }
    }

    fn check_member(&mut self, object: &Expr, field: &str, span: Span) -> Type {
        let obj_ty = self.check_expr(object);

        // Check for npm member access via tsgo probes (e.g. z.object, z.string)
        if let ExprKind::Identifier(name) = &object.kind {
            let member_key = format!("__member_{name}_{field}");
            for exports in self.dts_imports.values() {
                if let Some(export) = exports.iter().find(|e| e.name == member_key) {
                    let ty = crate::interop::wrap_boundary_type(&export.ts_type);
                    self.name_types.insert(member_key, ty.to_string());
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

        self.resolve_member_type(&obj_ty, field, span)
    }

    fn check_index(&mut self, object: &Expr, index: &Expr, span: Span) -> Type {
        let obj_ty = self.check_expr(object);
        let idx_ty = self.check_expr(index);

        // Resolve Named types to their concrete definition
        let concrete = self.resolve_type_to_concrete(&obj_ty);

        match &concrete {
            Type::Array(inner) => {
                // Index must be a number
                if !matches!(idx_ty, Type::Number | Type::Unknown) {
                    self.emit_error(
                        format!("array index must be `number`, found `{}`", idx_ty),
                        index.span,
                        ErrorCode::InvalidArrayIndex,
                        "expected `number`",
                    );
                }
                Type::option_of((**inner).clone())
            }
            Type::Tuple(elements) => {
                // Tuple indexing requires a numeric literal
                if let ExprKind::Number(n) = &index.kind {
                    if let Ok(idx) = n.parse::<usize>() {
                        if idx < elements.len() {
                            elements[idx].clone()
                        } else {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    format!(
                                        "tuple index `{}` out of bounds — tuple has {} element(s)",
                                        n,
                                        elements.len()
                                    ),
                                    index.span,
                                )
                                .with_error_code(ErrorCode::InvalidTupleIndex),
                            );
                            Type::Unknown
                        }
                    } else {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!("tuple index must be a non-negative integer, found `{n}`"),
                                index.span,
                            )
                            .with_error_code(ErrorCode::InvalidTupleIndex),
                        );
                        Type::Unknown
                    }
                } else {
                    self.emit_error(
                        "tuple index must be a numeric literal",
                        index.span,
                        ErrorCode::InvalidTupleIndex,
                        "dynamic indexing is not allowed on tuples",
                    );
                    Type::Unknown
                }
            }
            Type::Unknown | Type::Foreign(_) | Type::Never => Type::Unknown,
            Type::Var(_) => Type::Unknown,
            _ => {
                if let Type::Named(name) = &obj_ty
                    && self.env.lookup_type(name).is_none()
                {
                    return Type::Unknown;
                }
                self.emit_error(
                    format!("cannot use bracket access on type `{}`", obj_ty),
                    span,
                    ErrorCode::InvalidBracketAccess,
                    "not an array or tuple type",
                );
                Type::Unknown
            }
        }
    }

    fn check_arrow(&mut self, params: &[Param], body: &Expr, async_fn: bool) -> Type {
        let prev_inside_async = self.ctx.inside_async;
        self.ctx.inside_async = async_fn;
        self.env.push_scope();
        let param_hints = std::mem::take(&mut self.ctx.lambda_param_hints);
        let param_types: Vec<_> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ty = p
                    .type_ann
                    .as_ref()
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| {
                        // Use lambda param hint from calling context if available
                        if let Some(hint) = param_hints.get(i).cloned() {
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
                self.name_types.insert(p.name.clone(), ty.to_string());
                // For destructured params, also define the field names
                if let Some(ref destructure) = p.destructure {
                    self.define_destructured_bindings(destructure, &ty, p.span);
                }
                ty
            })
            .collect();
        let return_type = self.check_expr(body);
        self.env.pop_scope();
        self.ctx.inside_async = prev_inside_async;
        let required_params = param_types.len();
        Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
            required_params,
        }
    }

    fn check_match(&mut self, subject: &Expr, arms: &[MatchArm], span: Span) -> Type {
        let subject_ty = self.check_expr(subject);
        self.check_match_exhaustiveness(&subject_ty, arms, span);

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
                    self.emit_error(
                        format!(
                            "match arms have incompatible types: first arm returns `{}`, this arm returns `{}`",
                            first_type,
                            arm_type
                        ),
                        arm.body.span,
                        ErrorCode::TypeMismatch,
                        format!("expected `{}`", first_type),
                    );
                }
                result_type = Some(Self::merge_types(first_type, &arm_type));
            } else {
                result_type = Some(arm_type);
            }
        }
        result_type.unwrap_or(Type::Unit)
    }

    fn check_collect(&mut self, items: &[Item]) -> Type {
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
        Type::result_of(last_type, Type::Array(Box::new(e)))
    }

    fn check_array(&mut self, elements: &[Expr]) -> Type {
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
            let stdlib_params = stdlib_fn.params.clone();
            let display = format!("{module}.{field}");
            self.unused.used_names.insert(module.clone());

            // Two-pass argument checking: first check non-arrow args to resolve
            // type variables, then check arrow args with lambda_param_hints set.
            // This allows `use x <- Option.guard(opt, default)` to infer the
            // callback parameter type from the option's inner type.
            let mut type_var_bindings: HashMap<usize, Type> = HashMap::new();
            let mut arg_count = 0;

            // Pass 1: check non-arrow args and unify with stdlib param types
            for (i, arg) in args.iter().enumerate() {
                let (Arg::Positional(e) | Arg::Named { value: e, .. }) = arg;
                if !matches!(e.kind, ExprKind::Arrow { .. }) {
                    let actual_ty = self.check_expr(e);
                    if let Some(param_ty) = stdlib_params.get(i) {
                        Self::collect_type_var_bindings(
                            param_ty,
                            &actual_ty,
                            &mut type_var_bindings,
                        );
                    }
                    arg_count += 1;
                }
            }

            // Pass 2: check arrow args with resolved lambda_param_hints
            for (i, arg) in args.iter().enumerate() {
                let (Arg::Positional(e) | Arg::Named { value: e, .. }) = arg;
                if matches!(e.kind, ExprKind::Arrow { .. }) {
                    if let Some(Type::Function { params, .. }) = stdlib_params.get(i) {
                        self.ctx.lambda_param_hints = params
                            .iter()
                            .map(|p| Self::substitute_type_vars(p, &type_var_bindings))
                            .collect();
                    }
                    self.check_expr(e);
                    self.ctx.lambda_param_hints.clear();
                    arg_count += 1;
                }
            }

            if !variadic && arg_count != expected_param_count {
                self.emit_error(
                    format!(
                        "`{display}` expects {} argument{}, found {}",
                        expected_param_count,
                        if expected_param_count == 1 { "" } else { "s" },
                        arg_count
                    ),
                    span,
                    ErrorCode::TypeMismatch,
                    "wrong number of arguments",
                );
            }

            return ret;
        }

        // Check for untrusted import call without try
        if let ExprKind::Identifier(name) = &callee.kind
            && !self.ctx.inside_try
            && self.untrusted_imports.contains(name)
        {
            self.emit_error_with_help(
                format!("calling untrusted import `{name}` requires `try`"),
                span,
                ErrorCode::UntrustedImport,
                "untrusted import",
                format!("use `try {name}(...)` or mark the import as `trusted`"),
            );
        }

        // Save pipe context before checking callee (which would consume it)
        let piped_ty = self.ctx.pipe_input_type.take();
        let piped_ty_was_none = piped_ty.is_none();

        // Infer lambda param type from piped array element type
        if let Some(ref piped) = piped_ty
            && let Type::Array(elem_ty) = piped
        {
            self.ctx.lambda_param_hints = vec![(**elem_ty).clone()];
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
            self.emit_error(
                "only one `_` placeholder allowed per call - use `(x, y) => f(x, y)` for multiple parameters",
                span,
                ErrorCode::MultiplePlaceholders,
                "multiple `_` placeholders",
            );
        }

        let callee_ty = self.check_expr(callee);
        let mut arg_types: Vec<Type> = args
            .iter()
            .map(|arg| match arg {
                Arg::Positional(e) | Arg::Named { value: e, .. } => self.check_expr(e),
            })
            .collect();
        self.ctx.lambda_param_hints.clear();

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

        // Resolve type args eagerly for validation (catches unknown type names
        // even when the callee type is unknown/unresolved)
        let resolved_type_args: Vec<Type> =
            type_args.iter().map(|t| self.resolve_type(t)).collect();

        match callee_ty {
            Type::Function {
                params,
                return_type,
                ..
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
                                .with_error_code(ErrorCode::UnknownField),
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
                    self.emit_error(
                        format!(
                            "`{callee_name}` expects {expected_msg}, found {}",
                            arg_types.len()
                        ),
                        span,
                        ErrorCode::TypeMismatch,
                        "wrong number of arguments",
                    );
                }

                // Resolve generics
                let generic_params = Self::collect_generic_params(&params, &return_type);
                let return_type = if !generic_params.is_empty() {
                    let substitutions = if !resolved_type_args.is_empty() {
                        generic_params
                            .into_iter()
                            .zip(resolved_type_args.iter().cloned())
                            .collect()
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
                            self.emit_error(
                                format!(
                                    "argument {} to `{callee_name}`: expected `{}`, found `{}`",
                                    i + 1,
                                    param_ty,
                                    arg_ty
                                ),
                                span,
                                ErrorCode::TypeMismatch,
                                format!("expected `{}`", param_ty),
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

                    let required_params = placeholder_param_types.len();
                    Type::Function {
                        params: placeholder_param_types,
                        return_type: Box::new(return_type),
                        required_params,
                    }
                } else {
                    // Resolve dot shorthand args against expected function params.
                    // `.field` becomes `(x) => x.field` when the expected param is a function.
                    // When a piped arg is prepended, args are offset by 1 in arg_types/params.
                    let param_offset = if !piped_ty_was_none && !has_placeholder {
                        1
                    } else {
                        0
                    };
                    for (i, arg) in args.iter().enumerate() {
                        let e = match arg {
                            Arg::Positional(e) | Arg::Named { value: e, .. } => e,
                        };
                        if let ExprKind::DotShorthand { field, predicate } = &e.kind
                            && let Some(Type::Function {
                                params: fn_params, ..
                            }) = params.get(i + param_offset)
                            && let Some(self_ty) = fn_params.first()
                        {
                            let field_ty = self.resolve_member_type(self_ty, field, e.span);
                            let resolved_ret = if predicate.is_some() {
                                Type::Bool
                            } else {
                                field_ty
                            };
                            let resolved = Type::Function {
                                params: fn_params.clone(),
                                return_type: Box::new(resolved_ret),
                                required_params: fn_params.len(),
                            };
                            self.expr_types.insert(e.id, resolved.clone());
                            arg_types[i + param_offset] = resolved;
                        }
                    }

                    // Normal call: check all argument types
                    for (i, (arg_ty, param_ty)) in arg_types.iter().zip(params.iter()).enumerate() {
                        if !self.types_compatible(param_ty, arg_ty) {
                            self.emit_error(
                                format!(
                                    "argument {} to `{callee_name}`: expected `{}`, found `{}`",
                                    i + 1,
                                    param_ty,
                                    arg_ty
                                ),
                                span,
                                ErrorCode::TypeMismatch,
                                format!("expected `{}`", param_ty),
                            );
                        }
                    }
                    return_type
                }
            }
            Type::Unknown => {
                self.check_args_unchecked(args);
                let callee_name = match &callee.kind {
                    ExprKind::Identifier(name) => name.as_str(),
                    ExprKind::Member { field, .. } => field.as_str(),
                    _ => "<expression>",
                };
                self.emit_warning_with_help(
                    format!("`{callee_name}` has unknown type - arguments are not type-checked"),
                    span,
                    ErrorCode::UncheckedArguments,
                    "Type could not be resolved",
                    "Check that the import source has type declarations",
                );
                Type::Unknown
            }
            _ => {
                self.check_args_unchecked(args);
                Type::Unknown
            }
        }
    }

    /// Check argument expressions without validating types against parameters.
    fn check_args_unchecked(&mut self, args: &[Arg]) {
        for arg in args {
            match arg {
                Arg::Positional(e) | Arg::Named { value: e, .. } => {
                    self.check_expr(e);
                }
            }
        }
    }

    fn check_boolean_operand(&mut self, ty: &Type, concrete: &Type, span: Span, op: &str) {
        if !concrete.is_boolean()
            && !matches!(concrete, Type::Unknown | Type::Var(_) | Type::Foreign(_))
        {
            self.emit_error_with_help(
                format!("expected boolean operand for `{op}`, found `{}`", ty),
                span,
                ErrorCode::TypeMismatch,
                "expected `boolean`",
                "use `match` for non-boolean conditional logic",
            );
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
                    self.emit_error_with_help(
                        format!("cannot compare `{}` with `{}`", left_ty, right_ty),
                        span,
                        ErrorCode::InvalidComparison,
                        "mismatched types",
                        "both sides of `==` must have the same type",
                    );
                }
                Type::Bool
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => Type::Bool,
            BinOp::And | BinOp::Or => {
                let op_str = if op == BinOp::And { "&&" } else { "||" };
                let left_concrete = self.resolve_type_to_concrete(&left_ty);
                let right_concrete = self.resolve_type_to_concrete(&right_ty);
                self.check_boolean_operand(&left_ty, &left_concrete, left.span, op_str);
                self.check_boolean_operand(&right_ty, &right_concrete, right.span, op_str);
                Type::Bool
            }
            BinOp::Add => {
                // Rule 12: String concat with + warning
                if matches!(left_ty, Type::String) || matches!(right_ty, Type::String) {
                    self.emit_warning_with_help(
                        "use template literal instead of `+` for string concatenation",
                        span,
                        ErrorCode::TodoPlaceholder,
                        "prefer template literal",
                        "use `\"${a}${b}\"` instead",
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
                self.emit_error(
                    format!("unknown type `{type_name}`"),
                    span,
                    ErrorCode::UndefinedName,
                    "not defined",
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
            let required_params = field_types.len();
            return Type::Function {
                params: field_types.clone(),
                return_type: Box::new(ty),
                required_params,
            };
        }

        // Rule 3: Opaque enforcement
        if let Some(ref info) = type_info
            && info.opaque
        {
            self.emit_error_with_help(
                format!("cannot construct opaque type `{type_name}` outside its defining module"),
                span,
                ErrorCode::OpaqueConstruction,
                "opaque type cannot be constructed directly",
                "use the module's exported constructor function instead",
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
                    self.emit_error_with_help(
                        format!("unknown field `{label}` on type `{type_name}`"),
                        span,
                        ErrorCode::UnknownField,
                        format!("`{label}` is not a field of `{type_name}`"),
                        format!(
                            "available fields: {}",
                            fields
                                .iter()
                                .map(|f| format!("`{f}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
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
                        self.emit_error(
                            format!(
                                "missing required field `{field}` in `{type_name}` constructor"
                            ),
                            span,
                            ErrorCode::DuplicateDefinition,
                            format!("field `{field}` is required"),
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
                        self.emit_warning_with_help(
                            format!("field `{label}` from spread is overwritten by explicit field"),
                            span,
                            ErrorCode::SpreadFieldOverwritten,
                            format!("`{label}` exists in the spread source"),
                            "the spread value will be replaced by the explicit field",
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

        // Build ordered field types for variant constructors (positional arg checking)
        let variant_field_types: Option<Vec<Type>> =
            self.env.lookup(type_name).cloned().and_then(|ty| {
                if let Type::Union { variants, .. } = ty {
                    variants
                        .into_iter()
                        .find(|(n, _)| n == type_name)
                        .map(|(_, types)| types)
                } else {
                    None
                }
            });

        let mut positional_index = 0;
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
                        if arg_ty.is_option()
                            && matches!(arg_ty.option_inner(), Some(Type::Unknown))
                            && expected_ty.is_option()
                        {
                            self.expr_types.insert(e.id, expected_ty.clone());
                        }
                    }
                    if let Some(ref field_types) = field_type_map
                        && let Some((_, expected_ty)) = field_types.iter().find(|(n, _)| n == label)
                        && !self.types_compatible(expected_ty, &arg_ty)
                        && !matches!(arg_ty, Type::Unknown | Type::Var(_))
                    {
                        self.emit_error(
                            format!(
                                "field `{label}`: expected `{}`, found `{}`",
                                expected_ty, arg_ty
                            ),
                            span,
                            ErrorCode::TypeMismatch,
                            format!("expected `{}`", expected_ty),
                        );
                    }
                }
                Arg::Positional(e) => {
                    let arg_ty = self.check_expr(e);
                    if let Some(ref field_types) = variant_field_types
                        && let Some(expected_ty) = field_types.get(positional_index)
                        && !self.types_compatible(expected_ty, &arg_ty)
                        && !matches!(arg_ty, Type::Unknown | Type::Var(_))
                    {
                        self.emit_error(
                            format!(
                                "argument {}: expected `{}`, found `{}`",
                                positional_index + 1,
                                expected_ty,
                                arg_ty
                            ),
                            span,
                            ErrorCode::TypeMismatch,
                            format!("expected `{}`", expected_ty),
                        );
                    }
                    positional_index += 1;
                }
            }
        }

        // Only check arg count when all args are positional (no named args mixing)
        if let Some(ref field_types) = variant_field_types
            && spread.is_none()
            && positional_index == args.len()
            && positional_index != field_types.len()
        {
            self.emit_error(
                format!(
                    "`{type_name}` expects {} argument{}, found {}",
                    field_types.len(),
                    if field_types.len() == 1 { "" } else { "s" },
                    positional_index
                ),
                span,
                ErrorCode::DuplicateDefinition,
                format!(
                    "expected {} argument{}",
                    field_types.len(),
                    if field_types.len() == 1 { "" } else { "s" }
                ),
            );
        }

        // Option<T>: infer T from the Some argument
        if type_name == crate::type_layout::VARIANT_SOME {
            let inner = args
                .iter()
                .find_map(|a| {
                    if let Arg::Positional(e) = a {
                        Some(self.expr_types.get(&e.id).cloned().unwrap_or(Type::Unknown))
                    } else {
                        None
                    }
                })
                .unwrap_or(Type::Unknown);
            return Type::option_of(inner);
        }

        // Result<T, E>: infer T/E from the Ok/Err argument + return type context
        if type_name == crate::type_layout::VARIANT_OK {
            let ok_ty = args
                .iter()
                .find_map(|a| {
                    if let Arg::Positional(e) = a {
                        Some(self.expr_types.get(&e.id).cloned().unwrap_or(Type::Unknown))
                    } else {
                        None
                    }
                })
                .unwrap_or(Type::Unknown);
            let err_ty = match &self.ctx.current_return_type {
                Some(rt) if rt.is_result() => rt.result_err().cloned().unwrap_or(Type::Unknown),
                _ => Type::Unknown,
            };
            return Type::result_of(ok_ty, err_ty);
        }
        if type_name == crate::type_layout::VARIANT_ERR {
            let err_ty = args
                .iter()
                .find_map(|a| {
                    if let Arg::Positional(e) = a {
                        Some(self.expr_types.get(&e.id).cloned().unwrap_or(Type::Unknown))
                    } else {
                        None
                    }
                })
                .unwrap_or(Type::Unknown);
            let ok_ty = match &self.ctx.current_return_type {
                Some(rt) if rt.is_result() => rt.result_ok().cloned().unwrap_or(Type::Unknown),
                _ => Type::Unknown,
            };
            return Type::result_of(ok_ty, err_ty);
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

        // Qualified for-block call: `row |> AccentRow.toModel` or `row |> AccentRow.toModel(args)`
        if let Some((type_name, func_name)) = member_info
            && let Some(overloads) = self.for_block_overloads.get(func_name)
            && let Some((_, fn_type)) = overloads.iter().find(|(tn, _)| tn == type_name)
        {
            self.unused.used_names.insert(func_name.to_string());
            return match fn_type {
                Type::Function { return_type, .. } => *return_type.clone(),
                _ => Type::Unknown,
            };
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
                    ..
                } => {
                    // Validate the piped value as the first (and only) argument
                    if let Some(first_param) = params.first()
                        && !self.types_compatible(first_param, left_ty)
                    {
                        self.emit_error(
                            format!(
                                "argument 1 to `{name}`: expected `{}`, found `{}`",
                                first_param, left_ty
                            ),
                            right.span,
                            ErrorCode::TypeMismatch,
                            format!("expected `{}`", first_param),
                        );
                    }
                    return *return_type;
                }
                // Unknown types: don't error (not enough info)
                Type::Unknown | Type::Var(_) => {}
                // Non-function types: error
                _ => {
                    self.emit_error(
                        format!(
                            "cannot pipe into `{name}`: expected a function, found `{}`",
                            right_ty
                        ),
                        right.span,
                        ErrorCode::TypeMismatch,
                        "not a function",
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
            self.emit_error(
                format!(
                    "argument 1 to `{display_name}`: expected `{}`, found `{}`",
                    first_param, left_ty
                ),
                right.span,
                ErrorCode::TypeMismatch,
                format!("expected `{}`", first_param),
            );
        }
        if let Type::Array(elem) = left_ty {
            self.ctx.lambda_param_hints = vec![(**elem).clone()];
        } else if let Some(inner) = left_ty.option_inner() {
            self.ctx.lambda_param_hints = vec![inner.clone()];
        }

        // Check lambda args and capture return type for generic inference
        let lambda_return = self.check_pipe_right_args_with_return(right);
        self.ctx.lambda_param_hints.clear();

        // Resolve return type: if the stdlib fn's return type uses a different
        // type var than the input (e.g. map: Array<T> -> Array<U>), infer U
        // from the lambda's actual return type.
        let infer_from_lambda = lambda_return.is_some()
            && match (&stdlib_fn.return_type, stdlib_fn.params.first()) {
                (Type::Array(ret_elem), Some(Type::Array(in_elem))) => ret_elem != in_elem,
                (ret, Some(inp)) if ret.is_option() && inp.is_option() => {
                    ret.option_inner() != inp.option_inner()
                }
                _ => false,
            };
        // Resolve return type by substituting type variables from the piped input.
        let mut var_bindings: HashMap<usize, Type> = HashMap::new();
        if let Some(first_param) = stdlib_fn.params.first() {
            Self::collect_type_var_bindings(first_param, left_ty, &mut var_bindings);
        }
        // If the return type uses a different type var (e.g. map: Array<T> -> Array<U>),
        // infer it from the lambda's actual return type.
        if infer_from_lambda {
            match &stdlib_fn.return_type {
                Type::Array(_) => Type::Array(Box::new(lambda_return.unwrap())),
                ret if ret.is_option() => Type::option_of(lambda_return.unwrap()),
                _ => Self::substitute_type_vars(&stdlib_fn.return_type, &var_bindings),
            }
        } else if !var_bindings.is_empty() {
            Self::substitute_type_vars(&stdlib_fn.return_type, &var_bindings)
        } else {
            // No type var bindings resolved — match concrete types directly
            match (&stdlib_fn.return_type, left_ty) {
                (Type::Array(_), Type::Array(elem)) => Type::Array(elem.clone()),
                (ret, actual) if ret.is_option() && actual.is_option() => actual.clone(),
                _ => stdlib_fn.return_type.clone(),
            }
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

    /// Unify a stdlib parameter type against an actual argument type to resolve
    /// `Type::Var(n)` bindings. E.g. `Option<Var(0)>` unified with `Option<IssueDto>`
    /// binds `Var(0) → IssueDto`.
    fn collect_type_var_bindings(
        param_ty: &Type,
        actual_ty: &Type,
        bindings: &mut HashMap<usize, Type>,
    ) {
        match (param_ty, actual_ty) {
            (Type::Var(n), _) if !matches!(actual_ty, Type::Unknown | Type::Var(_)) => {
                bindings.insert(*n, actual_ty.clone());
            }
            (Type::Array(p), Type::Array(a))
            | (Type::Promise(p), Type::Promise(a))
            | (Type::Settable(p), Type::Settable(a))
            | (Type::Set { element: p }, Type::Set { element: a }) => {
                Self::collect_type_var_bindings(p, a, bindings);
            }
            (Type::Map { key: pk, value: pv }, Type::Map { key: ak, value: av })
            | (Type::RecordMap { key: pk, value: pv }, Type::RecordMap { key: ak, value: av }) => {
                Self::collect_type_var_bindings(pk, ak, bindings);
                Self::collect_type_var_bindings(pv, av, bindings);
            }
            (Type::Tuple(ps), Type::Tuple(as_)) if ps.len() == as_.len() => {
                for (p, a) in ps.iter().zip(as_.iter()) {
                    Self::collect_type_var_bindings(p, a, bindings);
                }
            }
            (Type::Union { variants: pv, .. }, Type::Union { variants: av, .. }) => {
                for (pvar, avar) in pv.iter().zip(av.iter()) {
                    for (p, a) in pvar.1.iter().zip(avar.1.iter()) {
                        Self::collect_type_var_bindings(p, a, bindings);
                    }
                }
            }
            (
                Type::Function {
                    params: pp,
                    return_type: pr,
                    ..
                },
                Type::Function {
                    params: ap,
                    return_type: ar,
                    ..
                },
            ) => {
                for (p, a) in pp.iter().zip(ap.iter()) {
                    Self::collect_type_var_bindings(p, a, bindings);
                }
                Self::collect_type_var_bindings(pr, ar, bindings);
            }
            _ => {}
        }
    }

    /// Substitute `Type::Var(n)` with resolved types from the bindings map.
    fn substitute_type_vars(ty: &Type, bindings: &HashMap<usize, Type>) -> Type {
        match ty {
            Type::Var(n) => bindings.get(n).cloned().unwrap_or_else(|| ty.clone()),
            Type::Array(inner) => {
                Type::Array(Box::new(Self::substitute_type_vars(inner, bindings)))
            }
            Type::Promise(inner) => {
                Type::Promise(Box::new(Self::substitute_type_vars(inner, bindings)))
            }
            Type::Settable(inner) => {
                Type::Settable(Box::new(Self::substitute_type_vars(inner, bindings)))
            }
            Type::Set { element } => Type::Set {
                element: Box::new(Self::substitute_type_vars(element, bindings)),
            },
            Type::Map { key, value } => Type::Map {
                key: Box::new(Self::substitute_type_vars(key, bindings)),
                value: Box::new(Self::substitute_type_vars(value, bindings)),
            },
            Type::RecordMap { key, value } => Type::RecordMap {
                key: Box::new(Self::substitute_type_vars(key, bindings)),
                value: Box::new(Self::substitute_type_vars(value, bindings)),
            },
            Type::Tuple(types) => Type::Tuple(
                types
                    .iter()
                    .map(|t| Self::substitute_type_vars(t, bindings))
                    .collect(),
            ),
            Type::Union { name, variants } => Type::Union {
                name: name.clone(),
                variants: variants
                    .iter()
                    .map(|(n, ts)| {
                        (
                            n.clone(),
                            ts.iter()
                                .map(|t| Self::substitute_type_vars(t, bindings))
                                .collect(),
                        )
                    })
                    .collect(),
            },
            Type::Function {
                params,
                return_type,
                required_params,
            } => Type::Function {
                params: params
                    .iter()
                    .map(|t| Self::substitute_type_vars(t, bindings))
                    .collect(),
                return_type: Box::new(Self::substitute_type_vars(return_type, bindings)),
                required_params: *required_params,
            },
            other => other.clone(),
        }
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
            Type::Array(inner) => {
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
                ..
            } => {
                for p in params {
                    Self::collect_generic_params_from_type(p, names, seen);
                }
                Self::collect_generic_params_from_type(return_type, names, seen);
            }
            Type::Map { key, value } | Type::RecordMap { key, value } => {
                Self::collect_generic_params_from_type(key, names, seen);
                Self::collect_generic_params_from_type(value, names, seen);
            }
            Type::Set { element } => {
                Self::collect_generic_params_from_type(element, names, seen);
            }
            Type::Union { variants, .. } => {
                for (_, field_types) in variants {
                    for ft in field_types {
                        Self::collect_generic_params_from_type(ft, names, seen);
                    }
                }
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
            (
                Type::Map { key: pk, value: pv } | Type::RecordMap { key: pk, value: pv },
                Type::Map { key: ak, value: av } | Type::RecordMap { key: ak, value: av },
            ) => {
                Self::unify_for_inference(pk, ak, generics, subs);
                Self::unify_for_inference(pv, av, generics, subs);
            }
            (Type::Set { element: pe }, Type::Set { element: ae }) => {
                Self::unify_for_inference(pe, ae, generics, subs);
            }
            (p, a) if p.is_option() && a.is_option() => {
                if let (Some(pi), Some(ai)) = (p.option_inner(), a.option_inner()) {
                    Self::unify_for_inference(pi, ai, generics, subs);
                }
            }
            (
                Type::Union {
                    name: pn,
                    variants: pv,
                },
                Type::Union {
                    name: an,
                    variants: av,
                },
            ) if pn == an => {
                for (pvar, avar) in pv.iter().zip(av.iter()) {
                    for (pt, at) in pvar.1.iter().zip(avar.1.iter()) {
                        Self::unify_for_inference(pt, at, generics, subs);
                    }
                }
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
            Type::RecordMap { key, value } => Type::RecordMap {
                key: Box::new(Self::substitute_generics(key, subs)),
                value: Box::new(Self::substitute_generics(value, subs)),
            },
            Type::Set { element } => Type::Set {
                element: Box::new(Self::substitute_generics(element, subs)),
            },
            _ if ty.is_option() => {
                if let Some(inner) = ty.option_inner() {
                    Type::option_of(Self::substitute_generics(inner, subs))
                } else {
                    ty.clone()
                }
            }
            Type::Tuple(types) => Type::Tuple(
                types
                    .iter()
                    .map(|t| Self::substitute_generics(t, subs))
                    .collect(),
            ),
            Type::Function {
                params,
                return_type,
                required_params,
            } => Type::Function {
                params: params
                    .iter()
                    .map(|t| Self::substitute_generics(t, subs))
                    .collect(),
                return_type: Box::new(Self::substitute_generics(return_type, subs)),
                required_params: *required_params,
            },
            Type::Union { name, variants } => Type::Union {
                name: name.clone(),
                variants: variants
                    .iter()
                    .map(|(vname, fields)| {
                        (
                            vname.clone(),
                            fields
                                .iter()
                                .map(|f| Self::substitute_generics(f, subs))
                                .collect(),
                        )
                    })
                    .collect(),
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
            _ => &dispatch_ty.to_string(),
        };
        let (_, fn_type) = overloads
            .iter()
            .find(|(type_name, _)| type_name == dispatch_name)?;
        Some(fn_type.clone())
    }

    /// Look up a for-block method for member-access syntax (`obj.method()`).
    /// Strips the `self` parameter since the object provides it implicitly.
    fn resolve_for_block_method(&self, field: &str, obj_ty: &Type) -> Option<Type> {
        let type_name = match obj_ty {
            Type::Named(n) | Type::Foreign(n) => n.as_str(),
            _ => return None,
        };
        let overloads = self.for_block_overloads.get(field)?;
        let (_, fn_type) = overloads.iter().find(|(tn, _)| tn == type_name)?;
        if let Type::Function {
            params,
            return_type,
            required_params,
        } = fn_type
        {
            let new_params: Vec<_> = params.iter().skip(1).cloned().collect();
            let new_required = required_params.saturating_sub(1);
            Some(Type::Function {
                params: new_params,
                return_type: return_type.clone(),
                required_params: new_required,
            })
        } else {
            Some(fn_type.clone())
        }
    }

    /// Resolve the type of a member access (`obj_ty.field`), producing diagnostics for errors.
    fn resolve_member_type(&mut self, obj_ty: &Type, field: &str, span: Span) -> Type {
        // Rule 6: No property access on unnarrowed unions
        if obj_ty.is_result() {
            self.emit_error_with_help(
                format!("cannot access `.{field}` on `Result` - use `match` or `?` first"),
                span,
                ErrorCode::FieldAccessOnResult,
                "`Result` must be narrowed first",
                "use `match result { Ok(v) -> ..., Err(e) -> ... }`",
            );
            return Type::Unknown;
        }
        if let Type::Union { name, .. } = obj_ty {
            self.emit_error_with_help(
                format!("cannot access `.{field}` on union `{name}` - use `match` first"),
                span,
                ErrorCode::FieldAccessOnResult,
                "union must be narrowed first",
                "use `match` to narrow the union first",
            );
            return Type::Unknown;
        }

        // Error on member access on Promise — must await first
        if let Type::Promise(_) = obj_ty {
            self.emit_error(
                format!(
                    "cannot access `.{field}` on `{}` — use `await` first",
                    obj_ty
                ),
                span,
                ErrorCode::AccessOnPromise,
                "must `await` the Promise before accessing members",
            );
            return Type::Unknown;
        }

        // Error on member access on `unknown` — must narrow first
        if matches!(obj_ty, Type::Unknown) {
            self.emit_error_with_help(
                format!("cannot access `.{field}` on `unknown`"),
                span,
                ErrorCode::AccessOnUnknown,
                "`unknown` must be narrowed before member access",
                "use `match`, type validation (e.g. Zod), or pattern matching",
            );
            return Type::Unknown;
        }

        // Resolve Named types to their concrete definition
        let concrete = self.resolve_type_to_concrete(obj_ty);
        if let Type::Record(fields) = &concrete {
            if let Some((_, ty)) = fields.iter().find(|(n, _)| n == field) {
                return ty.clone();
            }
            // Check for-block methods before reporting missing field —
            // `row.toModel()` should resolve to the for-block method.
            if let Some(ty) = self.resolve_for_block_method(field, obj_ty) {
                return ty;
            }
            let type_name = if let Type::Named(name) = obj_ty {
                format!("`{name}`")
            } else {
                format!("`{}`", obj_ty)
            };
            self.emit_error_with_help(
                format!("type {type_name} has no field `{field}`"),
                span,
                ErrorCode::InvalidFieldAccess,
                "unknown field",
                format!(
                    "available fields: {}",
                    fields
                        .iter()
                        .map(|(n, _)| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
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
                .with_error_code(ErrorCode::InvalidTupleIndex),
            );
            return Type::Unknown;
        }

        // Error on member access on primitive types
        match obj_ty {
            Type::Number | Type::String | Type::Bool | Type::Unit => {
                self.emit_error(
                    format!("cannot access `.{field}` on type `{}`", obj_ty),
                    span,
                    ErrorCode::InvalidFieldAccess,
                    "not a record type",
                );
                return Type::Unknown;
            }
            _ => {}
        }

        // For-block methods: check before foreign/named fallback so that
        // `a.toModel()` resolves to the for-block method's type.
        if let Some(ty) = self.resolve_for_block_method(field, obj_ty) {
            return ty;
        }

        // Foreign types: allow member access, return Foreign for chained access.
        // (Foreign types with known Record definitions are resolved above via
        // resolve_type_to_concrete, so this only fires for truly opaque types.)
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
            self.emit_error_with_help(
                format!("cannot access `.{field}` on unresolved type `{name}`"),
                span,
                ErrorCode::AccessOnUnknown,
                "type definition not found",
                "ensure the type's source module has a .d.ts file or is a .fl file",
            );
            return Type::Unknown;
        }

        // Never is the bottom type — member access propagates Never
        if matches!(obj_ty, Type::Never) {
            return Type::Never;
        }

        // Unresolved type variables — can't diagnose yet, return Unknown
        if matches!(obj_ty, Type::Var(_)) {
            return Type::Unknown;
        }

        // Fallback: type doesn't support member access
        self.emit_error(
            format!("cannot access `.{field}` on type `{}`", obj_ty),
            span,
            ErrorCode::InvalidFieldAccess,
            "this type does not support member access",
        );
        Type::Unknown
    }

    /// Define bindings for a destructured parameter in the current scope.
    ///
    /// Resolves the parameter type to its concrete form and extracts field/element
    /// types. Emits errors when destructuring is applied to an incompatible concrete
    /// type. When the type is unresolved (TypeVar, Unknown), fields fall back to
    /// Unknown silently — arrow params may have their types inferred later.
    pub(crate) fn define_destructured_bindings(
        &mut self,
        destructure: &ParamDestructure,
        ty: &Type,
        span: Span,
    ) {
        let concrete_ty = self.resolve_type_to_concrete(ty);
        let type_is_unresolved = matches!(
            concrete_ty,
            Type::Unknown | Type::Var(_) | Type::Named(_) | Type::Foreign(_)
        );

        match destructure {
            ParamDestructure::Object(fields) => {
                if let Type::Record(ref rec_fields) = concrete_ty {
                    for f in fields {
                        let field_ty = rec_fields
                            .iter()
                            .find(|(n, _)| n == &f.field)
                            .map(|(_, t)| t.clone())
                            .unwrap_or_else(|| {
                                self.emit_error(
                                    format!("field `{}` does not exist on type `{}`", f.field, ty),
                                    span,
                                    ErrorCode::TypeMismatch,
                                    format!("`{}` has no field `{}`", ty, f.field),
                                );
                                Type::Unknown
                            });
                        self.env.define(f.bound_name(), field_ty);
                    }
                } else if type_is_unresolved {
                    // Type not yet resolved (e.g. untyped arrow param, foreign type).
                    // Infer Error for the "error" field since Floe's error-handling
                    // callbacks (use blocks, fallbackRender) destructure { error }.
                    for f in fields {
                        let field_ty = unresolved_field_heuristic_type(f.field.as_str());
                        self.env.define(f.bound_name(), field_ty);
                    }
                } else {
                    self.emit_error(
                        format!("cannot destructure parameter of type `{}`", ty),
                        span,
                        ErrorCode::TypeMismatch,
                        "destructuring requires a record type".to_string(),
                    );
                    for f in fields {
                        self.env.define(f.bound_name(), Type::Unknown);
                    }
                }
            }
            ParamDestructure::Array(fields) => {
                if let Type::Array(inner) = &concrete_ty {
                    for field in fields {
                        self.env.define(field, inner.as_ref().clone());
                    }
                } else if type_is_unresolved {
                    for field in fields {
                        let field_ty = unresolved_field_heuristic_type(field);
                        self.env.define(field, field_ty);
                    }
                } else {
                    self.emit_error(
                        format!("cannot array-destructure parameter of type `{}`", ty),
                        span,
                        ErrorCode::TypeMismatch,
                        "array destructuring requires an array type".to_string(),
                    );
                    for field in fields {
                        self.env.define(field, Type::Unknown);
                    }
                }
            }
        }
    }

    /// Resolve a type to its concrete definition, following Named type lookups.
    pub(crate) fn resolve_type_to_concrete(&mut self, ty: &Type) -> Type {
        let resolved = self.env.resolve_to_concrete(ty, &simple_resolve_type_expr);
        // If still Named or Foreign after type_defs resolution, check if it's a known
        // value (e.g. built-in Response, Error, or TS interface imported via DTS)
        let name = match &resolved {
            Type::Named(n) | Type::Foreign(n) => Some(n.as_str()),
            _ => None,
        };
        if let Some(name) = name {
            // Check env first (built-in types, previous imports)
            if let Some(val_ty) = self.env.lookup(name).cloned()
                && matches!(val_ty, Type::Record(_))
            {
                return val_ty;
            }
            // Check DTS imports for type/interface definitions (e.g. non-exported
            // interfaces like IssueFilters that are referenced in probe output).
            // Strip generic args for lookup: "DraggableLocation<TId>" → "DraggableLocation"
            let base_name = name.split('<').next().unwrap_or(name);
            for exports in self.dts_imports.values() {
                if let Some(export) = exports.iter().find(|e| e.name == base_name) {
                    let ty = crate::interop::wrap_boundary_type(&export.ts_type);
                    if matches!(ty, Type::Record(_)) {
                        return ty;
                    }
                }
            }
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
                Type::option_of(inner)
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
                Type::result_of(ok, err)
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
            let required_params = param_types.len();
            Type::Function {
                params: param_types,
                return_type: Box::new(ret),
                required_params,
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
