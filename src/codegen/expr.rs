use super::*;

impl Codegen {
    /// Check if a callee targets an untrusted import.
    fn is_untrusted_call(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Identifier(name) => self.untrusted_imports.contains(name.as_str()),
            ExprKind::Member { object, .. } => {
                if let ExprKind::Identifier(obj_name) = &object.kind {
                    self.untrusted_imports.contains(obj_name.as_str())
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    // ── Expressions ──────────────────────────────────────────────

    pub(super) fn emit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Number(n) => self.push(n),
            ExprKind::String(s) => self.push(&format!("\"{}\"", escape_string(s))),
            ExprKind::TemplateLiteral(parts) => {
                self.push("`");
                for part in parts {
                    match part {
                        TemplatePart::Raw(s) => self.push(s),
                        TemplatePart::Expr(e) => {
                            self.push("${");
                            self.emit_expr(e);
                            self.push("}");
                        }
                    }
                }
                self.push("`");
            }
            ExprKind::Bool(b) => self.push(if *b { "true" } else { "false" }),
            ExprKind::Identifier(name) => {
                if self.unit_variants.contains(name.as_str()) {
                    // Zero-arg union variant: `All` → `{ tag: "All" }`
                    self.push(&format!("{{ {TAG_FIELD}: \""));
                    self.push(name);
                    self.push("\" }");
                } else if let Some(field_names) = self
                    .variant_info
                    .get(name.as_str())
                    .filter(|(_, f)| !f.is_empty())
                    .map(|(_, f)| f.clone())
                {
                    // Non-unit variant as function value:
                    // `Validation` → `(value) => ({ tag: "Validation", value })`
                    self.emit_variant_constructor_fn(name, &field_names);
                } else if let Some(mangled) = self.lookup_for_block_fn_by_name(name) {
                    self.push(&mangled);
                } else {
                    self.push(name);
                }
            }
            ExprKind::Placeholder => self.push("_"),

            ExprKind::Binary { left, op, right } => match op {
                BinOp::Eq => {
                    self.needs_deep_equal = true;
                    self.push(&format!("{DEEP_EQUAL_FN}("));
                    self.emit_expr(left);
                    self.push(", ");
                    self.emit_expr(right);
                    self.push(")");
                }
                BinOp::NotEq => {
                    self.needs_deep_equal = true;
                    self.push(&format!("!{DEEP_EQUAL_FN}("));
                    self.emit_expr(left);
                    self.push(", ");
                    self.emit_expr(right);
                    self.push(")");
                }
                _ => {
                    self.emit_expr(left);
                    self.push(&format!(" {} ", binop_str(*op)));
                    self.emit_expr(right);
                }
            },

            ExprKind::Unary { op, operand } => {
                self.push(unaryop_str(*op));
                self.emit_expr(operand);
            }

            // Pipe: `a |> f(b, c)` → `f(a, b, c)`
            // Pipe with placeholder: `a |> f(b, _, c)` → `f(b, a, c)`
            ExprKind::Pipe { left, right } => {
                self.emit_pipe(left, right);
            }

            // Unwrap: `expr?` → inline Result unwrap via IIFE
            ExprKind::Unwrap(inner) => {
                // Emit as IIFE that checks .ok and either returns value or throws
                // Use 'ok' in __r check to distinguish Floe Results from HTTP Response
                self.push("(() => { const __r = ");
                self.emit_expr(inner);
                self.push(
                    "; if (typeof __r === 'object' && __r !== null && 'ok' in __r && typeof __r.ok === 'boolean') { if (!__r.ok) throw __r.error; return __r.value; } return __r; })()",
                );
            }

            ExprKind::Call { callee, args, .. } => {
                // Auto-wrap untrusted import calls in try/catch IIFE
                if self.is_untrusted_call(callee) {
                    self.push(&format!(
                        "await (async () => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: await "
                    ));
                    self.emit_expr(callee);
                    self.push("(");
                    self.emit_args(args);
                    self.push(")");
                    self.push(&format!(
                        " }}; }} catch (_e) {{ return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: _e instanceof Error ? _e : new Error(String(_e)) }}; }} }})()"
                    ));
                } else if let Some(output) = self.try_emit_stdlib_call(callee, args) {
                    // Check for stdlib call: Array.sort(arr), Option.map(opt, fn), etc.
                    self.push(&output);
                } else if has_placeholder_arg(args) {
                    // Check if this is a partial application (has placeholder args)
                    self.emit_partial_application(callee, args);
                } else {
                    self.emit_expr(callee);
                    self.push("(");
                    self.emit_args(args);
                    self.push(")");
                }
            }

            // Constructor: `User(name: "Ry", email: e)` → `{ name: "Ry", email: e }`
            // Union variant: `Valid(text)` → `{ tag: "Valid", text: text }`
            // npm constructor: `QueryClient({...})` → `new QueryClient({...})`
            ExprKind::Construct {
                type_name,
                spread,
                args,
            } => {
                // Ok(value) → { ok: true as const, value: value }
                if type_name == "Ok" && args.len() == 1 && spread.is_none() {
                    self.push(&format!("{{ {OK_FIELD}: true as const, {VALUE_FIELD}: "));
                    match &args[0] {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => self.emit_expr(e),
                    }
                    self.push(" }");
                    return;
                }
                // Err(error) → { ok: false as const, error: error }
                if type_name == "Err" && args.len() == 1 && spread.is_none() {
                    self.push(&format!("{{ {OK_FIELD}: false as const, {ERROR_FIELD}: "));
                    match &args[0] {
                        Arg::Positional(e) | Arg::Named { value: e, .. } => self.emit_expr(e),
                    }
                    self.push(" }");
                    return;
                }
                // Qualified non-unit variant with no args → function value
                // `SaveError.Validation` → `(value) => ({ tag: "Validation", value })`
                if args.is_empty()
                    && spread.is_none()
                    && let Some(field_names) = self
                        .variant_info
                        .get(type_name.as_str())
                        .filter(|(_, f)| !f.is_empty())
                        .map(|(_, f)| f.clone())
                {
                    self.emit_variant_constructor_fn(type_name, &field_names);
                    return;
                }

                let variant_field_names = self
                    .variant_info
                    .get(type_name.as_str())
                    .map(|(_, fields)| fields.clone());
                let is_variant = variant_field_names.is_some();

                // Floe constructors use named args: User(name: "x", age: 30)
                // npm constructor calls use positional args: QueryClient({...})
                // If all args are positional (no named args) and it's not a known Floe type,
                // emit as `new Name(args)`
                let has_named_args = args.iter().any(|a| matches!(a, Arg::Named { .. }));
                let is_known_type = self.type_defs.contains_key(type_name.as_str());
                if !is_variant && !has_named_args && !is_known_type && spread.is_none() {
                    self.push("new ");
                    self.push(type_name);
                    self.push("(");
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        if let Arg::Positional(e) = arg {
                            self.emit_expr(e);
                        }
                    }
                    self.push(")");
                    return;
                }

                self.push("{ ");
                if is_variant {
                    self.push(&format!("{TAG_FIELD}: \""));
                    self.push(type_name);
                    self.push("\"");
                    if !args.is_empty() || spread.is_some() {
                        self.push(", ");
                    }
                }
                if let Some(spread_expr) = spread {
                    self.push("...");
                    self.emit_expr(spread_expr);
                    if !args.is_empty() {
                        self.push(", ");
                    }
                }
                // For variant constructors with positional args, use field names
                if let Some(ref field_names) = variant_field_names {
                    self.emit_construct_fields(args, field_names);
                } else {
                    self.emit_named_fields(args);
                }
                self.push(" }");
            }

            ExprKind::Member { object, field } => {
                // Check for for-block function: `Entry.toModel` → `Entry__toModel`
                if let ExprKind::Identifier(type_name) = &object.kind
                    && let Some(mangled) =
                        self.for_block_fns.get(&(type_name.clone(), field.clone()))
                {
                    let name = self
                        .import_aliases
                        .get(mangled)
                        .cloned()
                        .unwrap_or_else(|| mangled.clone());
                    self.push(&name);
                }
                // Check for union variant access: `Filter.All` → `{ tag: "All" }`
                else if let ExprKind::Identifier(type_name) = &object.kind
                    && self
                        .variant_info
                        .get(field.as_str())
                        .is_some_and(|(union_name, _)| union_name == type_name)
                {
                    self.push(&format!("{{ {TAG_FIELD}: \""));
                    self.push(field);
                    self.push("\" }");
                } else if field.chars().all(|c| c.is_ascii_digit()) {
                    // Tuple index access: pair.0 → pair[0]
                    self.emit_expr(object);
                    self.push("[");
                    self.push(field);
                    self.push("]");
                } else {
                    self.emit_expr(object);
                    self.push(".");
                    self.push(field);
                }
            }

            ExprKind::Index { object, index } => {
                self.emit_expr(object);
                self.push("[");
                self.emit_expr(index);
                self.push("]");
            }

            ExprKind::Arrow {
                async_fn,
                params,
                body,
            } => {
                if *async_fn {
                    self.push("async ");
                }
                if params.len() == 1 && params[0].type_ann.is_none() {
                    self.push("(");
                    self.emit_param(&params[0]);
                    self.push(")");
                } else {
                    self.push("(");
                    self.emit_params(params);
                    self.push(")");
                }
                self.push(" => ");
                // Wrap object-like bodies in parens to avoid block statement ambiguity
                // e.g. (p) => ({ id: p.id }) not (p) => { id: p.id }
                let needs_parens =
                    matches!(body.kind, ExprKind::Construct { .. } | ExprKind::Object(_));
                if needs_parens {
                    self.push("(");
                }
                self.emit_expr(body);
                if needs_parens {
                    self.push(")");
                }
            }

            // Match: `match x { A -> ..., B -> ... }` → ternary chain
            ExprKind::Match { subject, arms } => {
                self.emit_match(subject, arms);
            }

            // parse<T>(value) → validation IIFE
            ExprKind::Parse { type_arg, value } => {
                self.emit_parse(type_arg, value);
            }

            // mock<T> → object literal with generated test data
            ExprKind::Mock {
                type_arg,
                overrides,
            } => {
                self.emit_mock(type_arg, overrides, &mut 0);
            }

            // Ok/Err/Some/None are desugared before codegen or handled in Construct

            // Value(x) → x (after desugar, shouldn't reach here normally)
            ExprKind::Value(inner) => {
                self.emit_expr(inner);
            }

            // Clear → null
            ExprKind::Clear => {
                self.push("null");
            }

            // Unchanged → should only appear inside Construct args (filtered out)
            ExprKind::Unchanged => {
                self.push("undefined");
            }

            // todo → throw new Error("not implemented")
            ExprKind::Todo => {
                self.push(THROW_NOT_IMPLEMENTED);
            }

            // unreachable → throw new Error("unreachable")
            ExprKind::Unreachable => {
                self.push(THROW_UNREACHABLE);
            }

            ExprKind::Unit => {
                self.push("undefined");
            }

            ExprKind::Jsx(element) => {
                self.has_jsx = true;
                self.emit_jsx(element);
            }

            ExprKind::Collect(items) => {
                self.emit_collect_block(items);
            }

            ExprKind::Block(items) => {
                self.emit_block_items(items);
            }

            ExprKind::Grouped(inner) => {
                self.push("(");
                self.emit_expr(inner);
                self.push(")");
            }

            ExprKind::Array(elements) => {
                self.push("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_expr(elem);
                }
                self.push("]");
            }

            ExprKind::Tuple(elements) => {
                // Tuple: (a, b) → [a, b] as const
                self.push("[");
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_expr(elem);
                }
                self.push("]");
            }

            ExprKind::Spread(inner) => {
                self.push("...");
                self.emit_expr(inner);
            }

            ExprKind::Object(fields) => {
                self.push("{ ");
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(key);
                    self.push(": ");
                    self.emit_expr(value);
                }
                self.push(" }");
            }

            ExprKind::DotShorthand { field, predicate } => {
                match predicate {
                    Some((op, rhs)) => match op {
                        BinOp::Eq => {
                            self.needs_deep_equal = true;
                            self.push(&format!("(_x) => {DEEP_EQUAL_FN}(_x."));
                            self.push(field);
                            self.push(", ");
                            self.emit_expr(rhs);
                            self.push(")");
                        }
                        BinOp::NotEq => {
                            self.needs_deep_equal = true;
                            self.push(&format!("(_x) => !{DEEP_EQUAL_FN}(_x."));
                            self.push(field);
                            self.push(", ");
                            self.emit_expr(rhs);
                            self.push(")");
                        }
                        _ => {
                            self.push("(_x) => _x.");
                            self.push(field);
                            self.push(&format!(" {} ", binop_str(*op)));
                            self.emit_expr(rhs);
                        }
                    },
                    None => {
                        // `.field` → `(_x) => _x.field`
                        self.push("(_x) => _x.");
                        self.push(field);
                    }
                }
            }
        }
    }

    // ── Stdlib Helpers ─────────────────────────────────────────

    /// Emit each argument via a sub-codegen, propagating `needs_deep_equal`, and collect output strings.
    pub(super) fn emit_arg_strings(&mut self, args: &[Arg]) -> Vec<String> {
        let mut arg_strings = Vec::new();
        for arg in args {
            let mut sub = self.sub_codegen();
            match arg {
                Arg::Positional(e) => sub.emit_expr(e),
                Arg::Named { value, .. } => sub.emit_expr(value),
            }
            if sub.needs_deep_equal {
                self.needs_deep_equal = true;
            }
            if sub.has_jsx {
                self.has_jsx = true;
            }
            arg_strings.push(sub.output);
        }
        arg_strings
    }

    /// Emit a single expression via a sub-codegen, propagating flags.
    pub(super) fn emit_expr_string(&mut self, expr: &Expr) -> String {
        let mut sub = self.sub_codegen();
        sub.emit_expr(expr);
        if sub.needs_deep_equal {
            self.needs_deep_equal = true;
        }
        if sub.has_jsx {
            self.has_jsx = true;
        }
        sub.output
    }

    /// Check a stdlib template for deep-equal usage and expand it with the given arg strings.
    pub(super) fn apply_stdlib_template(
        &mut self,
        template: &str,
        arg_strings: &[String],
    ) -> String {
        if template.contains(DEEP_EQUAL_FN) {
            self.needs_deep_equal = true;
        }
        expand_codegen_template(template, arg_strings)
    }

    // ── Constructor → Object Literal ─────────────────────────────

    /// Emit a variant constructor as an arrow function.
    /// `Validation` → `(value) => ({ tag: "Validation", value })`
    fn emit_variant_constructor_fn(&mut self, variant_name: &str, field_names: &[String]) {
        self.push("(");
        for (i, fname) in field_names.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(fname);
        }
        self.push(&format!(") => ({{ {TAG_FIELD}: \""));
        self.push(variant_name);
        self.push("\"");
        for fname in field_names {
            self.push(", ");
            self.push(fname);
        }
        self.push(" })");
    }

    /// Emit construct fields, mapping positional args to field names from the type definition.
    fn emit_construct_fields(&mut self, args: &[Arg], field_names: &[String]) {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            match arg {
                Arg::Named { label, value } => {
                    self.push(label);
                    self.push(": ");
                    self.emit_expr(value);
                }
                Arg::Positional(expr) => {
                    // Map positional args to field names
                    if let Some(name) = field_names.get(i) {
                        self.push(name);
                        self.push(": ");
                    }
                    self.emit_expr(expr);
                }
            }
        }
    }

    fn emit_named_fields(&mut self, args: &[Arg]) {
        let mut first = true;
        for arg in args {
            // Skip Unchanged args — they should not appear in the output
            if matches!(arg, Arg::Named { value, .. } if matches!(value.kind, ExprKind::Unchanged))
            {
                continue;
            }
            if !first {
                self.push(", ");
            }
            first = false;
            match arg {
                Arg::Named { label, value } => {
                    self.push(label);
                    self.push(": ");
                    self.emit_expr(value);
                }
                Arg::Positional(expr) => {
                    self.emit_expr(expr);
                }
            }
        }
    }

    // ── Arguments (labels erased) ────────────────────────────────

    pub(super) fn emit_args(&mut self, args: &[Arg]) {
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            match arg {
                Arg::Positional(expr) => self.emit_expr(expr),
                // Named args: labels are erased in function calls
                Arg::Named { value, .. } => self.emit_expr(value),
            }
        }
    }

    // ── Block ────────────────────────────────────────────────────

    /// Like emit_block_expr but adds implicit return to the last expression.
    pub(super) fn emit_block_expr_with_return(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Block(items) => {
                self.push("{");
                self.newline();
                self.indent += 1;
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last && matches!(item.kind, ItemKind::Expr(_)) {
                        self.emit_indent();
                        self.push("return ");
                        if let ItemKind::Expr(e) = &item.kind {
                            self.emit_expr(e);
                        }
                        self.push(";");
                    } else {
                        self.emit_item(item);
                    }
                    self.newline();
                }
                self.indent -= 1;
                self.emit_indent();
                self.push("}");
            }
            _ => {
                self.push("{");
                self.newline();
                self.indent += 1;
                self.emit_indent();
                self.push("return ");
                self.emit_expr(expr);
                self.push(";");
                self.newline();
                self.indent -= 1;
                self.emit_indent();
                self.push("}");
            }
        }
    }

    pub(super) fn emit_block_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Block(items) => {
                self.emit_block_items(items);
            }
            _ => {
                self.push("{");
                self.newline();
                self.indent += 1;
                self.emit_indent();
                self.emit_expr(expr);
                self.push(";");
                self.newline();
                self.indent -= 1;
                self.emit_indent();
                self.push("}");
            }
        }
    }

    fn emit_block_items(&mut self, items: &[Item]) {
        self.push("{");
        self.newline();
        self.indent += 1;
        for item in items {
            self.emit_item(item);
            self.newline();
        }
        self.indent -= 1;
        self.emit_indent();
        self.push("}");
    }

    // ── Collect Block ───────────────────────────────────────────

    /// Emit a collect block as an IIFE that accumulates errors from `?`.
    ///
    /// ```typescript
    /// (() => {
    ///     const __errors: Array<E> = [];
    ///     const _r0 = validateName(input.name);
    ///     if (!_r0.ok) __errors.push(_r0.error);
    ///     const name = _r0.ok ? _r0.value : undefined as any;
    ///     ...
    ///     if (__errors.length > 0) return { ok: false, error: __errors };
    ///     return { ok: true, value: <last_expr> };
    /// })()
    /// ```
    fn emit_collect_block(&mut self, items: &[Item]) {
        // Check if any item contains await — if so, emit async IIFE
        let has_await = items.iter().any(|item| match &item.kind {
            ItemKind::Expr(e) => expr_contains_await(e),
            ItemKind::Const(c) => expr_contains_await(&c.value),
            _ => false,
        });
        if has_await {
            self.push("(async () => {");
        } else {
            self.push("(() => {");
        }
        self.newline();
        self.indent += 1;

        // Emit error accumulator
        self.emit_indent();
        self.push("const __errors: Array<any> = [];");
        self.newline();

        let mut result_counter = 0;

        for (i, item) in items.iter().enumerate() {
            let is_last = i == items.len() - 1;
            if is_last {
                if let ItemKind::Expr(expr) = &item.kind {
                    // Check for errors before returning
                    self.emit_indent();
                    self.push(
                        "if (__errors.length > 0) return { ok: false as const, error: __errors };",
                    );
                    self.newline();
                    self.emit_indent();
                    self.push("return { ok: true as const, value: ");
                    self.emit_expr(expr);
                    self.push(" };");
                    self.newline();
                } else {
                    self.emit_collect_item(item, &mut result_counter);
                    self.emit_indent();
                    self.push(
                        "if (__errors.length > 0) return { ok: false as const, error: __errors };",
                    );
                    self.newline();
                    self.emit_indent();
                    self.push("return { ok: true as const, value: undefined };");
                    self.newline();
                }
            } else {
                self.emit_collect_item(item, &mut result_counter);
            }
        }

        self.indent -= 1;
        self.emit_indent();
        self.push("})()");
    }

    /// Emit an item inside a collect block.
    /// Const declarations with `?` get special treatment:
    /// instead of short-circuiting, we accumulate the error.
    fn emit_collect_item(&mut self, item: &Item, result_counter: &mut usize) {
        match &item.kind {
            ItemKind::Const(decl) => {
                if let Some(unwrap_inner) = Self::find_unwrap_in_expr(&decl.value) {
                    let idx = *result_counter;
                    *result_counter += 1;
                    let temp = format!("_r{idx}");

                    // const _rN = <inner expression before ?>
                    self.emit_indent();
                    self.push(&format!("const {temp} = "));
                    self.emit_expr(unwrap_inner);
                    self.push(";");
                    self.newline();

                    // if (!_rN.ok) __errors.push(_rN.error);
                    self.emit_indent();
                    self.push(&format!("if (!{temp}.ok) __errors.push({temp}.error);"));
                    self.newline();

                    // const <binding> = _rN.ok ? _rN.value : undefined as any;
                    self.emit_indent();
                    match &decl.binding {
                        ConstBinding::Name(name) => {
                            self.push(&format!(
                                "const {name} = {temp}.ok ? {temp}.value : undefined as any;"
                            ));
                        }
                        _ => {
                            // For destructured bindings, fall back to normal emit
                            self.push(&format!(
                                "const __v{idx} = {temp}.ok ? {temp}.value : undefined as any;"
                            ));
                        }
                    }
                    self.newline();
                } else {
                    // No unwrap — emit normally
                    self.emit_item(item);
                    self.newline();
                }
            }
            ItemKind::Expr(expr) => {
                // Check if the expression itself is an unwrap
                if let ExprKind::Unwrap(inner) = &expr.kind {
                    let idx = *result_counter;
                    *result_counter += 1;
                    let temp = format!("_r{idx}");

                    self.emit_indent();
                    self.push(&format!("const {temp} = "));
                    self.emit_expr(inner);
                    self.push(";");
                    self.newline();

                    self.emit_indent();
                    self.push(&format!("if (!{temp}.ok) __errors.push({temp}.error);"));
                    self.newline();
                } else {
                    self.emit_indent();
                    self.emit_expr(expr);
                    self.push(";");
                    self.newline();
                }
            }
            _ => {
                self.emit_item(item);
                self.newline();
            }
        }
    }

    /// Find the inner expression of the outermost `?` in an expression.
    /// For example, in `input.name |> validateName?`, the parser produces
    /// `Unwrap(Pipe { ... })`, and this returns the `Pipe` expression.
    pub fn find_unwrap_in_expr(expr: &Expr) -> Option<&Expr> {
        match &expr.kind {
            ExprKind::Unwrap(inner) => Some(inner),
            _ => None,
        }
    }
}

/// Check if an expression tree contains a Promise.await stdlib call.
/// Detects `expr |> Promise.await`, `Promise.await(expr)`, and bare `|> await` patterns.
pub(super) fn expr_contains_await(expr: &Expr) -> bool {
    match &expr.kind {
        // Direct member access: Promise.await (in pipe target position)
        ExprKind::Member { object, field }
            if field == "await"
                && matches!(&object.kind, ExprKind::Identifier(m) if m == "Promise") =>
        {
            true
        }
        // Bare shorthand: `|> await`
        ExprKind::Identifier(name) if name == "await" => true,
        ExprKind::Call { callee, args, .. } => {
            expr_contains_await(callee)
                || args.iter().any(|a| match a {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => expr_contains_await(e),
                })
        }
        ExprKind::Member { object, .. } => expr_contains_await(object),
        ExprKind::Pipe { left, right } => expr_contains_await(left) || expr_contains_await(right),
        ExprKind::Binary { left, right, .. } => {
            expr_contains_await(left) || expr_contains_await(right)
        }
        ExprKind::Unary { operand, .. }
        | ExprKind::Grouped(operand)
        | ExprKind::Unwrap(operand)
        | ExprKind::Spread(operand) => expr_contains_await(operand),
        ExprKind::Match { subject, arms } => {
            expr_contains_await(subject) || arms.iter().any(|a| expr_contains_await(&a.body))
        }
        ExprKind::Collect(items) | ExprKind::Block(items) => {
            items.iter().any(|item| match &item.kind {
                ItemKind::Expr(e) => expr_contains_await(e),
                ItemKind::Const(c) => expr_contains_await(&c.value),
                _ => false,
            })
        }
        _ => false,
    }
}
