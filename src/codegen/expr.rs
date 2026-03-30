use super::*;

const DEEP_EQUAL_FN: &str = "__floeEq";
const THROW_NOT_IMPLEMENTED: &str = "(() => { throw new Error(\"not implemented\"); })()";
const THROW_UNREACHABLE: &str = "(() => { throw new Error(\"unreachable\"); })()";

/// Alternative codegen templates for Map operations on plain-object Records.
/// TS Record<K, V> maps to Floe Map<K, V>, but at runtime it's a plain object,
/// not a JS Map, so we use bracket access instead of Map API methods.
fn record_map_template(method: &str) -> Option<&'static str> {
    match method {
        "get" => Some("$0[$1]"),
        "has" => Some("($1 in $0)"),
        "keys" => Some("Object.keys($0)"),
        "values" => Some("Object.values($0)"),
        "entries" => Some("Object.entries($0)"),
        "size" => Some("Object.keys($0).length"),
        "isEmpty" => Some("Object.keys($0).length === 0"),
        _ => None,
    }
}

impl Codegen {
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
                // Check for stdlib call: Array.sort(arr), Option.map(opt, fn), etc.
                if let Some(output) = self.try_emit_stdlib_call(callee, args) {
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

            ExprKind::Await(inner) => {
                self.push("await ");
                self.emit_expr(inner);
            }

            // Try: `try expr` → IIFE with try/catch wrapping in Result
            // Non-Error throws are coerced to Error for consistent typing
            ExprKind::Try(inner) => {
                let has_await = expr_contains_await(inner);
                if has_await {
                    self.push(&format!("await (async () => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: "));
                } else {
                    self.push(&format!(
                        "(() => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: "
                    ));
                }
                self.emit_expr(inner);
                self.push(&format!(" }}; }} catch (_e) {{ return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: _e instanceof Error ? _e : new Error(String(_e)) }}; }} }})()"));
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

            // Ok(value) → { ok: true, value: value }
            ExprKind::Ok(inner) => {
                self.push(&format!("{{ {OK_FIELD}: true as const, {VALUE_FIELD}: "));
                self.emit_expr(inner);
                self.push(" }");
            }

            // Err(error) → { ok: false, error: error }
            ExprKind::Err(inner) => {
                self.push(&format!("{{ {OK_FIELD}: false as const, {ERROR_FIELD}: "));
                self.emit_expr(inner);
                self.push(" }");
            }

            // Some(value) → value
            ExprKind::Some(inner) => {
                self.emit_expr(inner);
            }

            // None → undefined
            ExprKind::None => {
                self.push("undefined");
            }

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
    fn emit_arg_strings(&mut self, args: &[Arg]) -> Vec<String> {
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
            arg_strings.push(sub.output);
        }
        arg_strings
    }

    /// Emit a single expression via a sub-codegen, propagating `needs_deep_equal`.
    fn emit_expr_string(&mut self, expr: &Expr) -> String {
        let mut sub = self.sub_codegen();
        sub.emit_expr(expr);
        if sub.needs_deep_equal {
            self.needs_deep_equal = true;
        }
        sub.output
    }

    /// Check a stdlib template for deep-equal usage and expand it with the given arg strings.
    fn apply_stdlib_template(&mut self, template: &str, arg_strings: &[String]) -> String {
        if template.contains(DEEP_EQUAL_FN) {
            self.needs_deep_equal = true;
        }
        expand_codegen_template(template, arg_strings)
    }

    // ── Pipe Lowering ────────────────────────────────────────────

    /// Try to emit a stdlib call. Returns Some(output) if the callee is a stdlib function.
    fn try_emit_stdlib_call(&mut self, callee: &Expr, args: &[Arg]) -> Option<String> {
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
        {
            // For Map.get(obj, key) etc., check if the first arg is a Record-backed Map
            let template = if module == "Map"
                && let Some(first_arg) = args.first()
                && let Arg::Positional(first_expr) = first_arg
                && self.is_record_map(&first_expr.ty)
            {
                record_map_template(field).unwrap_or(stdlib_fn.codegen)
            } else {
                stdlib_fn.codegen
            };
            let arg_strings = self.emit_arg_strings(args);
            Some(self.apply_stdlib_template(template, &arg_strings))
        } else {
            None
        }
    }

    /// Check if an expression's type is a Map that came from a TS Record
    /// (plain object at runtime, not a JS Map).
    fn is_record_map(&self, ty: &crate::checker::Type) -> bool {
        matches!(ty, crate::checker::Type::RecordMap { .. })
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
            let template = if module == "Map" && self.is_record_map(&left.ty) {
                record_map_template(field).unwrap_or(stdlib_fn.codegen)
            } else {
                stdlib_fn.codegen
            };
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

    fn emit_partial_application(&mut self, callee: &Expr, args: &[Arg]) {
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

    // ── Parse<T> Validation Codegen ─────────────────────────────

    fn emit_parse(&mut self, type_arg: &TypeExpr, value: &Expr) {
        // Generate: (() => { const __v = <value>; <checks>; return { ok: true, value: __v as T }; })()
        self.push("(() => { const __v = ");
        self.emit_expr(value);
        self.push("; ");
        self.emit_parse_checks("__v", type_arg, "");
        self.push(&format!(
            "return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: __v as "
        ));
        self.emit_type_expr(type_arg);
        self.push(" }; })()");
    }

    /// Emit validation checks for a given accessor path against a type expression.
    /// `accessor` is the JS expression to check (e.g., "__v", "(__v as any).name").
    /// `path` is a human-readable path for error messages (e.g., "", "field 'name'").
    fn emit_parse_checks(&mut self, accessor: &str, type_expr: &TypeExpr, path: &str) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => {
                match name.as_str() {
                    "string" => {
                        self.emit_typeof_check(accessor, "string", path);
                    }
                    "number" => {
                        self.emit_typeof_check(accessor, "number", path);
                    }
                    "boolean" => {
                        self.emit_typeof_check(accessor, "boolean", path);
                    }
                    "Array" => {
                        // Array.isArray check + element validation
                        let err_prefix = if path.is_empty() {
                            String::new()
                        } else {
                            format!("{path}: ")
                        };
                        self.push(&format!(
                            "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
                        ));
                        if let Some(elem_type) = type_args.first() {
                            let idx_var = format!("__i{}", accessor.len());
                            let elem_accessor = format!("{accessor}[{idx_var}]");
                            let elem_path = if path.is_empty() {
                                format!("element [\" + {idx_var} + \"]")
                            } else {
                                format!("{path} element [\" + {idx_var} + \"]")
                            };
                            self.push(&format!(
                                "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                            ));
                            self.emit_parse_checks(&elem_accessor, elem_type, &elem_path);
                            self.push("} ");
                        }
                    }
                    "Option" => {
                        // Allow undefined or validate inner type
                        if let Some(inner_type) = type_args.first() {
                            self.push(&format!("if ({accessor} !== undefined) {{ "));
                            self.emit_parse_checks(accessor, inner_type, path);
                            self.push("} ");
                        }
                    }
                    _ => {
                        // Named type — look up in expr_types to find if it's a known record.
                        // For now, just check it's an object (non-null).
                        let err_prefix = if path.is_empty() {
                            String::new()
                        } else {
                            format!("{path}: ")
                        };
                        self.push(&format!(
                            "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                        ));
                    }
                }
            }
            TypeExprKind::Record(fields) => {
                // Check it's an object
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                self.push(&format!(
                    "if (typeof {accessor} !== \"object\" || {accessor} === null) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected object, got \" + typeof {accessor}) }}; "
                ));
                // Check each field
                for field in fields {
                    let field_accessor = format!("({accessor} as any).{}", field.name);
                    let field_path = if path.is_empty() {
                        format!("field '{}'", field.name)
                    } else {
                        format!("{path}.{}", field.name)
                    };
                    self.emit_parse_checks(&field_accessor, &field.type_ann, &field_path);
                }
            }
            TypeExprKind::Array(inner) => {
                let err_prefix = if path.is_empty() {
                    String::new()
                } else {
                    format!("{path}: ")
                };
                self.push(&format!(
                    "if (!Array.isArray({accessor})) return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected array, got \" + typeof {accessor}) }}; "
                ));
                let idx_var = format!("__i{}", accessor.len());
                let elem_accessor = format!("{accessor}[{idx_var}]");
                let elem_path = if path.is_empty() {
                    format!("element [\" + {idx_var} + \"]")
                } else {
                    format!("{path} element [\" + {idx_var} + \"]")
                };
                self.push(&format!(
                    "for (let {idx_var} = 0; {idx_var} < {accessor}.length; {idx_var}++) {{ "
                ));
                self.emit_parse_checks(&elem_accessor, inner, &elem_path);
                self.push("} ");
            }
            TypeExprKind::StringLiteral(_)
            | TypeExprKind::Function { .. }
            | TypeExprKind::Tuple(_)
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => {
                // Can't validate string literals, functions, tuples, typeof, or intersections at runtime — skip
            }
        }
    }

    fn emit_typeof_check(&mut self, accessor: &str, expected: &str, path: &str) {
        let err_prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}: ")
        };
        self.push(&format!(
            "if (typeof {accessor} !== \"{expected}\") return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: new Error(\"{err_prefix}expected {expected}, got \" + typeof {accessor}) }}; "
        ));
    }

    // ── Mock codegen ─────────────────────────────────────────────

    /// Emit a mock value for the given type expression.
    /// `counter` is used to generate unique sequential values.
    /// `overrides` provides named field overrides from `mock<T>(field: value)`.
    fn emit_mock(&mut self, type_arg: &TypeExpr, overrides: &[Arg], counter: &mut usize) {
        self.emit_mock_for_type(type_arg, overrides, counter, "");
    }

    fn emit_mock_for_type(
        &mut self,
        type_expr: &TypeExpr,
        overrides: &[Arg],
        counter: &mut usize,
        field_name: &str,
    ) {
        match &type_expr.kind {
            TypeExprKind::Named {
                name, type_args, ..
            } => match name.as_str() {
                "string" => {
                    *counter += 1;
                    let label = if field_name.is_empty() {
                        "string"
                    } else {
                        field_name
                    };
                    self.push(&format!("\"mock-{label}-{}\"", counter));
                }
                "number" => {
                    *counter += 1;
                    self.push(&format!("{}", counter));
                }
                "boolean" => {
                    // Alternate true/false based on counter
                    self.push(if (*counter).is_multiple_of(2) {
                        "true"
                    } else {
                        "false"
                    });
                    *counter += 1;
                }
                "Array" => {
                    if let Some(elem_type) = type_args.first() {
                        self.push("[");
                        self.emit_mock_for_type(elem_type, &[], counter, field_name);
                        self.push("]");
                    } else {
                        self.push("[]");
                    }
                }
                "Option" => {
                    // Option<T> → Some(mock<T>) — emit the inner value
                    if let Some(inner_type) = type_args.first() {
                        self.emit_mock_for_type(inner_type, &[], counter, field_name);
                    } else {
                        self.push("undefined");
                    }
                }
                _ => {
                    // Named user type — look up in type_defs
                    if let Some(type_def) = self.type_defs.get(name).cloned() {
                        self.emit_mock_for_typedef(&type_def, name, overrides, counter);
                    } else {
                        // Unknown type — emit empty object
                        self.push("{}");
                    }
                }
            },
            TypeExprKind::Record(fields) => {
                self.push("{ ");
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    // Check if this field has an override
                    let has_override = overrides.iter().find(|arg| {
                        if let Arg::Named { label, .. } = arg {
                            label == &field.name
                        } else {
                            false
                        }
                    });
                    self.push(&format!("{}: ", field.name));
                    if let Some(Arg::Named { value, .. }) = has_override {
                        self.emit_expr(value);
                    } else {
                        self.emit_mock_for_type(&field.type_ann, &[], counter, &field.name);
                    }
                }
                self.push(" }");
            }
            TypeExprKind::Array(inner) => {
                self.push("[");
                self.emit_mock_for_type(inner, &[], counter, field_name);
                self.push("]");
            }
            TypeExprKind::Tuple(types) => {
                self.push("[");
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_mock_for_type(ty, &[], counter, "");
                }
                self.push("]");
            }
            TypeExprKind::StringLiteral(value) => {
                self.push(&format!("\"{value}\""));
            }
            TypeExprKind::Function { .. }
            | TypeExprKind::TypeOf(_)
            | TypeExprKind::Intersection(_) => {
                self.push("(() => { throw new Error(\"mock function\"); })");
            }
        }
    }

    fn emit_mock_for_typedef(
        &mut self,
        type_def: &TypeDef,
        type_name: &str,
        overrides: &[Arg],
        counter: &mut usize,
    ) {
        match type_def {
            TypeDef::Record(entries) => {
                self.push("{ ");
                let mut first = true;
                for entry in entries {
                    match entry {
                        RecordEntry::Field(field) => {
                            if !first {
                                self.push(", ");
                            }
                            first = false;
                            let has_override = overrides.iter().find(|arg| {
                                if let Arg::Named { label, .. } = arg {
                                    label == &field.name
                                } else {
                                    false
                                }
                            });
                            self.push(&format!("{}: ", field.name));
                            if let Some(Arg::Named { value, .. }) = has_override {
                                self.emit_expr(value);
                            } else {
                                self.emit_mock_for_type(&field.type_ann, &[], counter, &field.name);
                            }
                        }
                        RecordEntry::Spread(spread) => {
                            // Spread in mock: recursively mock the spread type
                            if let Some(TypeDef::Record(spread_entries)) =
                                self.type_defs.get(&spread.type_name).cloned()
                            {
                                for spread_entry in &spread_entries {
                                    if let RecordEntry::Field(field) = spread_entry {
                                        if !first {
                                            self.push(", ");
                                        }
                                        first = false;
                                        let has_override = overrides.iter().find(|arg| {
                                            if let Arg::Named { label, .. } = arg {
                                                label == &field.name
                                            } else {
                                                false
                                            }
                                        });
                                        self.push(&format!("{}: ", field.name));
                                        if let Some(Arg::Named { value, .. }) = has_override {
                                            self.emit_expr(value);
                                        } else {
                                            self.emit_mock_for_type(
                                                &field.type_ann,
                                                &[],
                                                counter,
                                                &field.name,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.push(" }");
            }
            TypeDef::Union(variants) => {
                // Pick first variant
                if let Some(variant) = variants.first() {
                    if variant.fields.is_empty() {
                        // Unit variant
                        self.push(&format!("{{ {TAG_FIELD}: \"{}\" as const }}", variant.name));
                    } else {
                        self.push(&format!("{{ {TAG_FIELD}: \"{}\" as const", variant.name));
                        for field in &variant.fields {
                            let fname = field.name.clone().unwrap_or_else(|| "value".to_string());
                            self.push(&format!(", {fname}: "));
                            self.emit_mock_for_type(&field.type_ann, &[], counter, &fname);
                        }
                        self.push(" }");
                    }
                } else {
                    self.push("{}");
                }
            }
            TypeDef::StringLiteralUnion(variants) => {
                // Pick first variant
                if let Some(first) = variants.first() {
                    self.push(&format!("\"{first}\""));
                } else {
                    self.push("\"\"");
                }
            }
            TypeDef::Alias(type_expr) => {
                // Newtype: mock the inner type
                self.emit_mock_for_type(type_expr, overrides, counter, type_name);
            }
        }
    }

    // ── Arguments (labels erased) ────────────────────────────────

    fn emit_args(&mut self, args: &[Arg]) {
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

/// Check if an expression tree contains an Await node.
fn expr_contains_await(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Await(_) => true,
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
        | ExprKind::Try(operand)
        | ExprKind::Spread(operand) => expr_contains_await(operand),
        ExprKind::Collect(items) | ExprKind::Block(items) => items.iter().any(|item| {
            if let ItemKind::Expr(e) = &item.kind {
                expr_contains_await(e)
            } else {
                false
            }
        }),
        _ => false,
    }
}
