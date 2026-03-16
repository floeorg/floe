use super::*;
use super::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

const DEEP_EQUAL_FN: &str = "__zenEq";
const THROW_NOT_IMPLEMENTED: &str = "(() => { throw new Error(\"not implemented\"); })()";
const THROW_UNREACHABLE: &str = "(() => { throw new Error(\"unreachable\"); })()";

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

            // Unwrap: `expr?` → early return pattern
            // In expression context, we emit as inline (the statement-level
            // version with temp vars is handled at block level)
            ExprKind::Unwrap(inner) => {
                // Simple inline unwrap — the full temp var version needs
                // statement context. For now emit as-is for nested expressions.
                self.emit_expr(inner);
                self.push("!");
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
                let variant_field_names = self
                    .variant_info
                    .get(type_name.as_str())
                    .map(|(_, fields)| fields.clone());
                let is_variant = variant_field_names.is_some();

                // Floe constructors use named args: User(name: "x", age: 30)
                // npm constructor calls use positional args: QueryClient({...})
                // If all args are positional (no named args), emit as `new Name(args)`
                let has_named_args = args.iter().any(|a| matches!(a, Arg::Named { .. }));
                if !is_variant && !has_named_args && spread.is_none() {
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
                // Check for union variant access: `Filter.All` → `{ tag: "All" }`
                if let ExprKind::Identifier(type_name) = &object.kind
                    && self
                        .variant_info
                        .get(field.as_str())
                        .is_some_and(|(union_name, _)| union_name == type_name)
                {
                    self.push(&format!("{{ {TAG_FIELD}: \""));
                    self.push(field);
                    self.push("\" }");
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
                    self.push(&params[0].name);
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
                self.push("] as const");
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
            let template = stdlib_fn.codegen.to_string();
            let arg_strings = self.emit_arg_strings(args);
            Some(self.apply_stdlib_template(&template, &arg_strings))
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
            let template = stdlib_fn.codegen.to_string();
            let left_str = self.emit_expr_string(left);
            let mut arg_strings = vec![left_str];
            arg_strings.extend(self.emit_arg_strings(extra_args));
            Some(self.apply_stdlib_template(&template, &arg_strings))
        } else {
            None
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
            let stdlib_fn = match self
                .expr_types
                .get(&(left.span.start, left.span.end))
                .and_then(|ty| Self::type_to_stdlib_module(ty))
            {
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

    /// Map a checker Type to the corresponding stdlib module name.
    fn type_to_stdlib_module(ty: &crate::checker::Type) -> Option<&'static str> {
        use crate::checker::Type;
        match ty {
            Type::Array(_) => Some("Array"),
            Type::String => Some("String"),
            Type::Number => Some("Number"),
            Type::Option(_) => Some("Option"),
            Type::Result { .. } => Some("Result"),
            _ => None,
        }
    }

    fn emit_pipe(&mut self, left: &Expr, right: &Expr) {
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
                // Fall through to normal call handling below
                // Use aliased import name if available (avoids TDZ conflicts)
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
            ExprKind::Member { .. } => {
                // Bare stdlib: `arr |> Array.sort` (no args)
                if let Some(output) = self.try_emit_stdlib_pipe(left, right, &[]) {
                    self.push(&output);
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
                    // Positional args in constructors become value_0, value_1 etc
                    // In practice, constructors should use named args
                    self.emit_expr(expr);
                }
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
        _ => false,
    }
}
