use super::*;
use crate::type_names;

// ── Expression Checking ──────────────────────────────────────

impl Checker {
    pub(super) fn check_expr(&mut self, expr: &Expr) -> Type {
        let ty = self.check_expr_inner(expr);
        self.expr_types
            .insert((expr.span.start, expr.span.end), ty.clone());
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
                self.used_names.insert(name.clone());
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
                if !self.inside_collect {
                    match &self.current_return_type {
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
                        if self.inside_collect {
                            self.collect_err_type = Some(*err);
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
            } => {
                // Check for stdlib call: Array.sort(arr), Option.map(opt, fn), etc.
                if let ExprKind::Member { object, field } = &callee.kind
                    && let ExprKind::Identifier(module) = &object.kind
                    && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
                {
                    self.used_names.insert(module.clone());
                    let ret = stdlib_fn.return_type.clone();
                    for arg in args {
                        match arg {
                            Arg::Positional(e) | Arg::Named { value: e, .. } => {
                                self.check_expr(e);
                            }
                        }
                    }
                    return ret;
                }

                // Check for untrusted import call without try
                if let ExprKind::Identifier(name) = &callee.kind
                    && !self.inside_try
                    && self.untrusted_imports.contains(name)
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!("calling untrusted import `{name}` requires `try`"),
                            expr.span,
                        )
                        .with_label("untrusted import")
                        .with_help(format!(
                            "use `try {name}(...)` or mark the import as `trusted`"
                        ))
                        .with_code("E014"),
                    );
                }

                // Save pipe context before checking callee (which would consume it)
                let piped_ty = self.pipe_input_type.take();
                let piped_ty_was_none = piped_ty.is_none();

                // Infer lambda param type from piped array element type
                // e.g. [1,2,3] |> Array.map(|x| ...) → x: number
                if let Some(ref piped) = piped_ty
                    && let Type::Array(elem_ty) = piped
                {
                    self.lambda_param_hint = Some((**elem_ty).clone());
                }

                // Detect placeholder args for partial application
                let has_placeholder = args.iter().any(|a| match a {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        matches!(e.kind, ExprKind::Placeholder)
                    }
                });

                let callee_ty = self.check_expr(callee);
                let mut arg_types: Vec<Type> = args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => self.check_expr(e),
                    })
                    .collect();
                // Clear any unconsumed hint
                self.lambda_param_hint = None;

                // Handle piped value insertion:
                // - If the call has placeholders, the piped value replaces the `_`
                //   (e.g. `5 |> add(3, _)` → add(3, 5))
                // - Otherwise, the piped value is inserted as the first arg
                //   (e.g. `5 |> add(3)` → add(5, 3))
                if let Some(piped_ty) = piped_ty {
                    if has_placeholder {
                        // Replace placeholder types with the piped type
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

                match callee_ty {
                    Type::Function {
                        params,
                        return_type,
                    } => {
                        let callee_name = match &callee.kind {
                            ExprKind::Identifier(name) => name.as_str(),
                            _ => "<anonymous>",
                        };

                        // For partial application (non-pipe), the number of actual
                        // (non-placeholder) args must fit within the function's params,
                        // and the total arg count (including placeholders) must match.
                        if has_placeholder && piped_ty_was_none {
                            // Non-pipe partial application: `add(10, _)`
                            // Total args (with placeholders) should equal param count
                            let required_params = self
                                .fn_required_params
                                .get(callee_name)
                                .copied()
                                .unwrap_or(params.len());
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
                                        expr.span,
                                    )
                                    .with_label("wrong number of arguments")
                                    .with_code("E001"),
                                );
                            }

                            // Type-check only the non-placeholder arguments
                            for (i, (arg_ty, param_ty)) in
                                arg_types.iter().zip(params.iter()).enumerate()
                            {
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
                                            expr.span,
                                        )
                                        .with_label(format!(
                                            "expected `{}`",
                                            param_ty.display_name()
                                        ))
                                        .with_code("E001"),
                                    );
                                }
                            }

                            // Return a function type for the remaining (placeholder) params
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

                            // Substitute generics before building the partial type
                            let generic_params =
                                Self::collect_generic_params(&params, &return_type);
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

                            Type::Function {
                                params: placeholder_param_types,
                                return_type: Box::new(return_type),
                            }
                        } else {
                            // Normal call or pipe-with-placeholder (already resolved)
                            // Check argument count, accounting for default parameters
                            let required_params = self
                                .fn_required_params
                                .get(callee_name)
                                .copied()
                                .unwrap_or(params.len());
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
                                        expr.span,
                                    )
                                    .with_label("wrong number of arguments")
                                    .with_code("E001"),
                                );
                            }

                            // Substitute generic params if we can resolve them
                            let generic_params =
                                Self::collect_generic_params(&params, &return_type);
                            let return_type = if !generic_params.is_empty() {
                                let substitutions = if !type_args.is_empty() {
                                    // Explicit type args: useState<Array<Todo>>
                                    let resolved: Vec<Type> =
                                        type_args.iter().map(|t| self.resolve_type(t)).collect();
                                    generic_params.into_iter().zip(resolved).collect()
                                } else {
                                    // Infer from arguments: useState("") → S = string
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

                            // Check argument types (after substitution)
                            for (i, (arg_ty, param_ty)) in
                                arg_types.iter().zip(params.iter()).enumerate()
                            {
                                if !self.types_compatible(param_ty, arg_ty) {
                                    self.diagnostics.push(
                                        Diagnostic::error(
                                            format!(
                                                "argument {} to `{callee_name}`: expected `{}`, found `{}`",
                                                i + 1,
                                                param_ty.display_name(),
                                                arg_ty.display_name()
                                            ),
                                            expr.span,
                                        )
                                        .with_label(format!(
                                            "expected `{}`",
                                            param_ty.display_name()
                                        ))
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

            ExprKind::Construct {
                type_name,
                spread,
                args,
            } => {
                self.used_names.insert(type_name.clone());

                let type_info = self.env.lookup_type(type_name).cloned();
                if type_info.is_none() {
                    // Also accept union variant names as constructors
                    let is_variant = self
                        .env
                        .lookup(type_name)
                        .is_some_and(|ty| matches!(ty, Type::Union { .. }));
                    // Also accept known imported symbols (e.g. npm imports) used as constructors.
                    // When an uppercase import like `QueryClient` is called with named args,
                    // the parser produces a Construct node. If the name exists in the value
                    // environment, treat it as a function call rather than erroring.
                    let is_known_value = self.env.lookup(type_name).is_some();
                    if !is_variant && !is_known_value {
                        self.diagnostics.push(
                            Diagnostic::error(format!("unknown type `{type_name}`"), expr.span)
                                .with_label("not defined")
                                .with_code("E002"),
                        );
                    }
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
                            expr.span,
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
                    // For variant constructors, look up parent union's type info
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
                                variants.iter().find(|v| v.name == *type_name).map(|v| {
                                    v.fields.iter().filter_map(|f| f.name.clone()).collect()
                                })
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
                                    expr.span,
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
                                        expr.span,
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

                    // Rule: warn on overlapping spread keys
                    if let Type::Record(spread_fields) = &spread_type {
                        let spread_keys: Vec<&str> =
                            spread_fields.iter().map(|(k, _)| k.as_str()).collect();
                        for arg in args.iter() {
                            if let Arg::Named { label, .. } = arg
                                && spread_keys.contains(&label.as_str())
                            {
                                self.diagnostics.push(
                                    Diagnostic::warning(
                                        format!("field `{label}` from spread is overwritten by explicit field"),
                                        expr.span,
                                    )
                                    .with_label(format!("`{label}` exists in the spread source"))
                                    .with_help(
                                        "the spread value will be replaced by the explicit field",
                                    )
                                    .with_code("W003"),
                                );
                            }
                        }
                    }
                }

                // Build a map of field name -> expected type from the type definition
                let field_type_map: Option<Vec<(String, Type)>> = if let Some(ref info) = type_info
                {
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

                // Check each argument and validate types against declared fields
                for arg in args {
                    match arg {
                        Arg::Named {
                            label, value: e, ..
                        } => {
                            let arg_ty = self.check_expr(e);
                            // Validate type against declared field type
                            if let Some(ref field_types) = field_type_map
                                && let Some((_, expected_ty)) =
                                    field_types.iter().find(|(n, _)| n == label)
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
                                        expr.span,
                                    )
                                    .with_label(format!(
                                        "expected `{}`",
                                        expected_ty.display_name()
                                    ))
                                    .with_code("E001"),
                                );
                            }
                        }
                        Arg::Positional(e) => {
                            self.check_expr(e);
                        }
                    }
                }

                // If this is a variant constructor, return the parent union type
                // rather than Named(variant_name) so match arm types are consistent
                if let Some(ty) = self.env.lookup(type_name).cloned()
                    && let Type::Union { .. } = &ty
                {
                    return ty;
                }
                Type::Named(type_name.clone())
            }

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
                                if let Some(hint) = self.lambda_param_hint.take() {
                                    return hint;
                                }
                                // In event handler context, infer Event type for the parameter
                                if self.event_handler_context && p.destructure.is_none() {
                                    Type::Named("Event".to_string())
                                } else {
                                    self.fresh_type_var()
                                }
                            });
                        self.env.define(&p.name, ty.clone());
                        // For destructured params, also define the field names
                        if let Some(ref destructure) = p.destructure {
                            match destructure {
                                ParamDestructure::Object(fields)
                                | ParamDestructure::Array(fields) => {
                                    for field in fields {
                                        // Infer type for well-known field names
                                        let field_ty = match field.as_str() {
                                            "error" => Type::Named(type_names::ERROR.to_string()),
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
                        if !self.types_compatible(first_type, &arm_type)
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
                    } else {
                        result_type = Some(arm_type);
                    }
                }
                result_type.unwrap_or(Type::Unit)
            }

            ExprKind::Await(inner) => {
                let ty = self.check_expr(inner);
                // Unwrap Promise<T> to T
                if let Type::Named(name) = &ty
                    && let Some(inner_name) = name
                        .strip_prefix("Promise<")
                        .and_then(|s| s.strip_suffix('>'))
                {
                    return self.resolve_named_type(inner_name, &[], expr.span);
                }
                // If not a Promise, pass through (e.g. await on a non-async value)
                ty
            }

            ExprKind::Try(inner) => {
                let prev_inside_try = self.inside_try;
                self.inside_try = true;
                let inner_ty = self.check_expr(inner);
                self.inside_try = prev_inside_try;
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(Type::Named(type_names::ERROR.to_string())),
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
                    err: Box::new(Type::Named(type_names::ERROR.to_string())),
                }
            }

            ExprKind::Ok(inner) => {
                let inner_ty = self.check_expr(inner);
                // Infer error type from enclosing function's return type if available
                let err_ty = match &self.current_return_type {
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
                let ok_ty = match &self.current_return_type {
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
                let prev_inside_collect = self.inside_collect;
                self.inside_collect = true;
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
                    if let Some(ref et) = self.collect_err_type
                        && err_type.is_none()
                    {
                        err_type = Some(et.clone());
                    }
                }

                self.inside_collect = prev_inside_collect;
                self.collect_err_type = None;
                self.env.pop_scope();

                let e = err_type.unwrap_or(Type::Unknown);
                Type::Result {
                    ok: Box::new(last_type),
                    err: Box::new(Type::Array(Box::new(e))),
                }
            }

            ExprKind::Block(items) => {
                self.env.push_scope();
                let mut last_type = Type::Unit;
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last {
                        if let ItemKind::Expr(expr) = &item.kind {
                            // Check last expression once and use its type as block type
                            last_type = self.check_expr(expr);
                        } else {
                            self.check_item(item);
                        }
                    } else {
                        self.check_item(item);
                    }
                }
                self.env.pop_scope();
                last_type
            }

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
                // Rule 2: Brand enforcement
                if let (Type::Brand { tag: tag_l, .. }, Type::Brand { tag: tag_r, .. }) =
                    (&left_ty, &right_ty)
                    && tag_l != tag_r
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot compare branded type `{tag_l}` with `{tag_r}`"
                            ),
                            span,
                        )
                        .with_label("different branded types")
                        .with_help(format!(
                            "`{tag_l}` and `{tag_r}` are distinct types even though they share the same base type"
                        ))
                        .with_code("E002"),
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

    /// Check the right side of a pipe expression with type-directed resolution.
    /// When the right side uses a bare function name (not locally defined),
    /// resolve it against stdlib using the left side's type.
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
            self.used_names.insert(module.to_string());
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
            let module = Self::type_to_stdlib_module(left_ty);
            let fallback_matches = self.stdlib.lookup_by_name(name);

            if let Some(m) = module
                && let Some(stdlib_fn) = self.stdlib.lookup(m, name).cloned()
            {
                // Found via type-directed resolution
                self.used_names.insert(name.to_string());
                let display = format!("{m}.{name}");
                return self.validate_stdlib_pipe_call(&stdlib_fn, &display, left_ty, right);
            } else if !fallback_matches.is_empty() && self.env.lookup(name).is_none() {
                // Found via name-based fallback (only if not locally defined)
                let stdlib_fn = fallback_matches[0].clone();
                self.used_names.insert(name.to_string());
                return self.validate_stdlib_pipe_call(&stdlib_fn, name, left_ty, right);
            }
        }

        // Default: check normally, with pipe context for arg validation
        let prev_pipe = self.pipe_input_type.take();
        self.pipe_input_type = Some(left_ty.clone());
        let right_ty = self.check_expr(right);
        self.pipe_input_type = prev_pipe;

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
        let ret = match (&stdlib_fn.return_type, left_ty) {
            (Type::Array(_), Type::Array(elem)) => Type::Array(elem.clone()),
            _ => stdlib_fn.return_type.clone(),
        };
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
            self.lambda_param_hint = Some((**elem).clone());
        }
        self.check_pipe_right_args(right);
        self.lambda_param_hint = None;
        ret
    }

    /// Check arguments in the right side of a pipe without checking the callee identifier.
    fn check_pipe_right_args(&mut self, right: &Expr) {
        if let ExprKind::Call { args, .. } = &right.kind {
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        self.check_expr(e);
                    }
                }
            }
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
            Type::Named(n) if n.len() == 1 && n.chars().next().unwrap().is_ascii_uppercase() => {
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

    fn type_to_stdlib_module(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Array(_) => Some(type_names::MOD_ARRAY),
            Type::Map { .. } => Some(type_names::MOD_MAP),
            Type::Set { .. } => Some(type_names::MOD_SET),
            Type::String => Some(type_names::MOD_STRING),
            Type::Number => Some(type_names::MOD_NUMBER),
            Type::Option(_) => Some(type_names::MOD_OPTION),
            Type::Result { .. } => Some(type_names::MOD_RESULT),
            _ => None,
        }
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
        if let Type::Named(name) = obj_ty
            && name.starts_with("Promise<")
        {
            self.diagnostics.push(
                Diagnostic::error(
                    format!("cannot access `.{field}` on `{name}` — use `await` first"),
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
            type_names::NUMBER => Type::Number,
            type_names::STRING => Type::String,
            type_names::BOOLEAN => Type::Bool,
            type_names::UNIT => Type::Unit,
            type_names::UNDEFINED => Type::Undefined,
            type_names::ARRAY => {
                let inner = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Array(Box::new(inner))
            }
            type_names::OPTION => {
                let inner = type_args
                    .first()
                    .map(simple_resolve_type_expr)
                    .unwrap_or(Type::Unknown);
                Type::Option(Box::new(inner))
            }
            type_names::RESULT => {
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
    }
}
