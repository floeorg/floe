use super::*;

impl Codegen {
    // ── Pipe Lowering ────────────────────────────────────────────

    /// Try to emit a stdlib call. Returns Some(output) if the callee is a stdlib function.
    pub(super) fn try_emit_stdlib_call(&mut self, callee: &Expr, args: &[Arg]) -> Option<String> {
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
        {
            let template = stdlib_fn.codegen;
            let arg_strings = self.emit_arg_strings(args);
            Some(self.apply_stdlib_template(template, &arg_strings))
        } else {
            None
        }
    }

    /// Try to emit a stdlib call in pipe context (piped value is first arg).
    fn try_emit_stdlib_pipe(
        &mut self,
        left: &Expr,
        callee: &Expr,
        extra_args: &[Arg],
    ) -> Option<String> {
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
        {
            let template = stdlib_fn.codegen;
            let left_str = self.emit_expr_string(left);
            let mut arg_strings = vec![left_str];
            arg_strings.extend(self.emit_arg_strings(extra_args));
            Some(self.apply_stdlib_template(template, &arg_strings))
        } else {
            None
        }
    }

    /// Try to emit a qualified for-block call in pipe context.
    /// `row |> AccentRow.toModel(args)` → `AccentRow__toModel(row, args)`
    fn try_emit_for_block_pipe(
        &mut self,
        left: &Expr,
        type_name: &str,
        field: &str,
        args: &[Arg],
    ) -> bool {
        if let Some(mangled) = self
            .for_block_fns
            .get(&(type_name.to_string(), field.to_string()))
        {
            let name = self
                .import_aliases
                .get(mangled)
                .cloned()
                .unwrap_or_else(|| mangled.clone());
            self.push(&name);
            self.push("(");
            self.emit_expr(left);
            if !args.is_empty() {
                self.push(", ");
                self.emit_args(args);
            }
            self.push(")");
            true
        } else {
            false
        }
    }

    /// Try to resolve a bare function name in pipe context via type-directed stdlib lookup.
    /// Uses the checker's type map to determine which stdlib module to use.
    /// e.g., `arr |> length` → left is Array → use Array.length template.
    fn try_emit_bare_stdlib_pipe(
        &mut self,
        left: &Expr,
        callee: &Expr,
        extra_args: &[Arg],
    ) -> Option<String> {
        if let ExprKind::Identifier(name) = &callee.kind {
            // Don't shadow locally defined functions, unless the name
            // is also a stdlib function (stdlib takes priority in pipes)
            if self.local_names.contains(name.as_str())
                && self.stdlib.lookup_by_name(name).is_empty()
            {
                return None;
            }

            // Resolve stdlib module from the left-hand type.
            // 1. Known type → type-directed (disambiguates Array.length vs String.length)
            // 2. Unknown/Var type or no entry → name-based fallback
            let stdlib_fn = match crate::type_layout::type_to_stdlib_module(&left.ty) {
                Some(module) => self
                    .stdlib
                    .lookup(module, name)
                    // Fallback: name might be in a different module (e.g. tap is in Pipe, not Array)
                    .or_else(|| self.stdlib.lookup_by_name(name).into_iter().next()),
                None => self.stdlib.lookup_by_name(name).into_iter().next(),
            }?;

            let template = stdlib_fn.codegen.to_string();
            let left_str = self.emit_expr_string(left);
            let mut arg_strings = vec![left_str];
            arg_strings.extend(self.emit_arg_strings(extra_args));
            return Some(self.apply_stdlib_template(&template, &arg_strings));
        }
        None
    }

    pub(super) fn emit_pipe(&mut self, left: &Expr, right: &Expr) {
        match &right.kind {
            // Stdlib pipe: `arr |> Array.sort` or `arr |> Array.map(fn)`
            // Also handles type-directed resolution: `arr |> map(fn)` → stdlib lookup by name
            ExprKind::Call { callee, args, .. } if !has_placeholder_arg(args) => {
                if let Some(output) = self.try_emit_stdlib_pipe(left, callee, args) {
                    self.push(&output);
                    return;
                }
                // Type-directed resolution: bare function name → check stdlib
                if let Some(output) = self.try_emit_bare_stdlib_pipe(left, callee, args) {
                    self.push(&output);
                    return;
                }
                // Qualified for-block: `row |> AccentRow.toModel(args)` → `AccentRow__toModel(row, args)`
                if let ExprKind::Member { object, field } = &callee.kind
                    && let ExprKind::Identifier(type_name) = &object.kind
                    && self.try_emit_for_block_pipe(left, type_name, field, args)
                {
                    return;
                }
                // Bare for-block: `x |> toChar(args)` → `Icon__toChar(x, args)`
                if let ExprKind::Identifier(name) = &callee.kind
                    && let Some(mangled) = self.lookup_for_block_fn_by_name(name)
                {
                    self.push(&mangled);
                    self.push("(");
                    self.emit_expr(left);
                    if !args.is_empty() {
                        self.push(", ");
                        self.emit_args(args);
                    }
                    self.push(")");
                    return;
                }
                // Fall through to normal call handling below
                let callee_alias = if let ExprKind::Identifier(name) = &callee.kind {
                    self.import_aliases.get(name.as_str()).cloned()
                } else {
                    None
                };
                if let Some(alias) = callee_alias {
                    self.push(&alias);
                } else {
                    self.emit_expr(callee);
                }
                self.push("(");
                self.emit_expr(left);
                if !args.is_empty() {
                    self.push(", ");
                    self.emit_args(args);
                }
                self.push(")");
            }
            ExprKind::Member { object, field } => {
                // Bare stdlib: `arr |> Array.sort` (no args)
                if let Some(output) = self.try_emit_stdlib_pipe(left, right, &[]) {
                    self.push(&output);
                    return;
                }
                // Qualified for-block: `row |> AccentRow.toModel` → `AccentRow__toModel(row)`
                if let ExprKind::Identifier(type_name) = &object.kind
                    && self.try_emit_for_block_pipe(left, type_name, field, &[])
                {
                    return;
                }
                // Fallback: treat as function call
                self.emit_expr(right);
                self.push("(");
                self.emit_expr(left);
                self.push(")");
            }
            // `a |> f(b, _, c)` → `f(b, a, c)` — placeholder replacement
            ExprKind::Call { callee, args, .. } if has_placeholder_arg(args) => {
                self.emit_expr(callee);
                self.push("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    match arg {
                        Arg::Positional(expr) if matches!(expr.kind, ExprKind::Placeholder) => {
                            self.emit_expr(left);
                        }
                        Arg::Positional(expr) => self.emit_expr(expr),
                        Arg::Named { label, value } => {
                            // Named args stay as-is in TS (but we erase labels in calls)
                            if matches!(value.kind, ExprKind::Placeholder) {
                                self.emit_expr(left);
                            } else {
                                let _ = label;
                                self.emit_expr(value);
                            }
                        }
                    }
                }
                self.push(")");
            }
            // `a |> parse<T>` — substitute piped value into parse
            ExprKind::Parse { type_arg, value } if matches!(value.kind, ExprKind::Placeholder) => {
                let substituted = Expr::synthetic(
                    ExprKind::Parse {
                        type_arg: type_arg.clone(),
                        value: Box::new(left.clone()),
                    },
                    right.span,
                );
                self.emit_expr(&substituted);
            }
            // `a |> f` → `f(a)` — bare function (also check stdlib)
            ExprKind::Identifier(name) => {
                if let Some(output) = self.try_emit_bare_stdlib_pipe(left, right, &[]) {
                    self.push(&output);
                    return;
                }
                // For-block function: `item.icon |> toChar` → `Icon__toChar(item.icon)`
                if let Some(mangled) = self.lookup_for_block_fn_by_name(name) {
                    self.push(&mangled);
                    self.push("(");
                    self.emit_expr(left);
                    self.push(")");
                    return;
                }
                // Use aliased import name if available (avoids TDZ conflicts)
                let alias = self.import_aliases.get(name.as_str()).cloned();
                if let Some(alias) = alias {
                    self.push(&alias);
                } else {
                    self.emit_expr(right);
                }
                self.push("(");
                self.emit_expr(left);
                self.push(")");
            }
            // Fallback: treat as function call
            _ => {
                self.emit_expr(right);
                self.push("(");
                self.emit_expr(left);
                self.push(")");
            }
        }
    }

    // ── Partial Application ──────────────────────────────────────

    pub(super) fn emit_partial_application(&mut self, callee: &Expr, args: &[Arg]) {
        // `add(10, _)` → `(_x) => add(10, _x)`
        let param_name = "_x";
        self.push(&format!("({param_name}) => "));
        self.emit_expr(callee);
        self.push("(");
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            match arg {
                Arg::Positional(expr) if matches!(expr.kind, ExprKind::Placeholder) => {
                    self.push(param_name);
                }
                Arg::Positional(expr) => self.emit_expr(expr),
                Arg::Named { value, .. } => {
                    if matches!(value.kind, ExprKind::Placeholder) {
                        self.push(param_name);
                    } else {
                        self.emit_expr(value);
                    }
                }
            }
        }
        self.push(")");
    }
}
