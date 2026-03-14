use super::*;

// ── Expression Checking ──────────────────────────────────────

impl Checker {
    pub(super) fn check_expr(&mut self, expr: &Expr) -> Type {
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
                                    format!("cannot negate `{}`", ty.display_name()),
                                    expr.span,
                                )
                                .with_label("not a number")
                                .with_code("E001"),
                            );
                        }
                        Type::Number
                    }
                    UnaryOp::Not => Type::Bool,
                }
            }

            ExprKind::Pipe { left, right } => {
                let _left_ty = self.check_expr(left);
                self.check_expr(right)
            }

            ExprKind::Unwrap(inner) => {
                let ty = self.check_expr(inner);
                // Rule 5: ? only allowed in functions returning Result/Option
                match &self.current_return_type {
                    Some(ret) if ret.is_result() || ret.is_option() => {}
                    Some(_) => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "? operator requires function to return Result or Option",
                                expr.span,
                            )
                            .with_label("invalid ? usage")
                            .with_help("Change the function's return type to Result or Option")
                            .with_code("E005"),
                        );
                    }
                    None => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "? operator can only be used inside a function",
                                expr.span,
                            )
                            .with_label("not inside a function")
                            .with_code("E005"),
                        );
                    }
                }
                // Unwrap the inner type
                match ty {
                    Type::Result { ok, .. } => *ok,
                    Type::Option(inner) => *inner,
                    _ => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                format!(
                                    "? can only be used on Result or Option, found `{}`",
                                    ty.display_name()
                                ),
                                expr.span,
                            )
                            .with_label("not a Result or Option")
                            .with_code("E005"),
                        );
                        Type::Unknown
                    }
                }
            }

            ExprKind::Call { callee, args } => {
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
                        .with_label("untrusted TS import")
                        .with_help(format!(
                            "Use `try {name}(...)` or mark the import as `trusted`"
                        ))
                        .with_code("E014"),
                    );
                }

                let callee_ty = self.check_expr(callee);
                for arg in args {
                    match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => {
                            self.check_expr(e);
                        }
                    }
                }
                match callee_ty {
                    Type::Function { return_type, .. } => *return_type,
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
                    if !is_variant {
                        self.diagnostics.push(
                            Diagnostic::error(format!("unknown type `{type_name}`"), expr.span)
                                .with_label("not a known type")
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
                        .with_label("opaque type")
                        .with_help("Use the module's exported constructor function instead")
                        .with_code("E003"),
                    );
                }

                // Collect valid field names for this type
                let valid_fields: Option<Vec<String>> = if let Some(ref info) = type_info {
                    match &info.def {
                        TypeDef::Record(fields) => {
                            Some(fields.iter().map(|f| f.name.clone()).collect())
                        }
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
                                .with_help(format!("available fields: {}", fields.join(", ")))
                                .with_code("E015"),
                            );
                        }
                    }

                    // Check for missing required fields (only when no spread)
                    if spread.is_none() {
                        let has_defaults: Vec<String> = if let Some(ref info) = type_info {
                            if let TypeDef::Record(record_fields) = &info.def {
                                record_fields
                                    .iter()
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
                                            "missing field `{field}` in `{type_name}` constructor"
                                        ),
                                        expr.span,
                                    )
                                    .with_label(format!("`{field}` is required"))
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
                                        format!("field `{label}` from spread is overwritten"),
                                        expr.span,
                                    )
                                    .with_label(format!("`{label}` exists in the spread source"))
                                    .with_help(
                                        "The spread value will be replaced by the explicit field",
                                    )
                                    .with_code("W003"),
                                );
                            }
                        }
                    }
                }
                for arg in args {
                    match arg {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => {
                            self.check_expr(e);
                        }
                    }
                }

                Type::Named(type_name.clone())
            }

            ExprKind::Member { object, field } => {
                let obj_ty = self.check_expr(object);
                // Rule 6: No property access on unnarrowed unions
                if let Type::Result { .. } = obj_ty {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot access `.{field}` on Result - use `match` or `?` first"
                            ),
                            expr.span,
                        )
                        .with_label("Result not narrowed")
                        .with_help("Use `match result { Ok(v) -> ..., Err(e) -> ... }`")
                        .with_code("E006"),
                    );
                }
                if let Type::Union { name, .. } = &obj_ty {
                    self.diagnostics.push(
                        Diagnostic::error(
                            format!(
                                "cannot access `.{field}` on union `{name}` - use `match` first"
                            ),
                            expr.span,
                        )
                        .with_label("union not narrowed")
                        .with_help("Use `match` to narrow the union first")
                        .with_code("E006"),
                    );
                }
                if let Type::Record(fields) = &obj_ty
                    && let Some((_, ty)) = fields.iter().find(|(n, _)| n == field)
                {
                    return ty.clone();
                }
                Type::Unknown
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

            ExprKind::Arrow { params, body } => {
                self.env.push_scope();
                let param_types: Vec<_> = params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .type_ann
                            .as_ref()
                            .map(|t| self.resolve_type(t))
                            .unwrap_or_else(|| self.fresh_type_var());
                        self.env.define(&p.name, ty.clone());
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
                    let arm_type = self.check_expr(&arm.body);
                    self.env.pop_scope();

                    if result_type.is_none() {
                        result_type = Some(arm_type);
                    }
                }
                result_type.unwrap_or(Type::Unit)
            }

            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(condition);
                let then_ty = self.check_expr(then_branch);
                if let Some(else_expr) = else_branch {
                    self.check_expr(else_expr);
                }
                then_ty
            }

            ExprKind::Return(value) => {
                if let Some(e) = value {
                    self.check_expr(e)
                } else {
                    Type::Unit
                }
            }

            ExprKind::Await(inner) => self.check_expr(inner),

            ExprKind::Try(inner) => {
                let prev_inside_try = self.inside_try;
                self.inside_try = true;
                let inner_ty = self.check_expr(inner);
                self.inside_try = prev_inside_try;
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(Type::Named("Error".to_string())),
                }
            }

            ExprKind::Ok(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Result {
                    ok: Box::new(inner_ty),
                    err: Box::new(Type::Unknown),
                }
            }

            ExprKind::Err(inner) => {
                let err_ty = self.check_expr(inner);
                Type::Result {
                    ok: Box::new(Type::Unknown),
                    err: Box::new(err_ty),
                }
            }

            ExprKind::Some(inner) => {
                let inner_ty = self.check_expr(inner);
                Type::Option(Box::new(inner_ty))
            }

            ExprKind::None => Type::Option(Box::new(Type::Unknown)),

            ExprKind::Unit => Type::Unit,

            ExprKind::Jsx(element) => {
                self.check_jsx(element);
                Type::Named("JSX.Element".to_string())
            }

            ExprKind::Block(items) => {
                self.env.push_scope();
                let mut last_type = Type::Unit;
                let mut found_return = false;
                for (i, item) in items.iter().enumerate() {
                    if found_return {
                        // Rule 10: Dead code detection
                        self.diagnostics.push(
                            Diagnostic::error("unreachable code", item.span)
                                .with_label("this code is unreachable")
                                .with_help("Remove this code")
                                .with_code("E011"),
                        );
                        break;
                    }
                    let is_last = i == items.len() - 1;
                    if is_last {
                        if let ItemKind::Expr(expr) = &item.kind {
                            if matches!(expr.kind, ExprKind::Return(_)) {
                                found_return = true;
                            }
                            // Check last expression once and use its type as block type
                            last_type = self.check_expr(expr);
                        } else {
                            self.check_item(item);
                        }
                    } else {
                        self.check_item(item);
                        if let ItemKind::Expr(expr) = &item.kind
                            && matches!(expr.kind, ExprKind::Return(_))
                        {
                            found_return = true;
                        }
                    }
                }
                self.env.pop_scope();
                last_type
            }

            ExprKind::Grouped(inner) => self.check_expr(inner),

            ExprKind::Array(elements) => {
                let mut elem_type: Option<Type> = None;
                for el in elements {
                    let ty = self.check_expr(el);
                    if let Some(ref prev) = elem_type {
                        if !self.types_compatible(prev, &ty)
                            && !matches!(ty, Type::Unknown | Type::Var(_))
                            && !matches!(prev, Type::Unknown | Type::Var(_))
                        {
                            self.diagnostics.push(
                                Diagnostic::error(
                                    "mixed array needs explicit type annotation",
                                    el.span,
                                )
                                .with_label("mismatched element type")
                                .with_help("Add an explicit type annotation to the array")
                                .with_code("E004"),
                            );
                        }
                    } else {
                        elem_type = Some(ty);
                    }
                }
                Type::Array(Box::new(elem_type.unwrap_or(Type::Unknown)))
            }

            ExprKind::Spread(inner) => self.check_expr(inner),

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
                        .with_label("type mismatch in comparison")
                        .with_help("Convert one side to match the other")
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
                                "cannot compare `{tag_l}` with `{tag_r}` - different branded types"
                            ),
                            span,
                        )
                        .with_label("brand mismatch")
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
                        .with_label("string concat with +")
                        .with_help("Use `${a}${b}` instead")
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
}
