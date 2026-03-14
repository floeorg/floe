use crate::parser::ast::*;
use crate::stdlib::StdlibRegistry;

/// Code generation result: the emitted TypeScript source and whether it contains JSX.
pub struct CodegenOutput {
    pub code: String,
    pub has_jsx: bool,
}

/// The ZenScript code generator. Emits clean, readable TypeScript / TSX.
pub struct Codegen {
    output: String,
    indent: usize,
    has_jsx: bool,
    needs_deep_equal: bool,
    stdlib: StdlibRegistry,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent: 0,
            has_jsx: false,
            needs_deep_equal: false,
            stdlib: StdlibRegistry::new(),
        }
    }

    /// Generate TypeScript from a ZenScript program.
    pub fn generate(mut self, program: &Program) -> CodegenOutput {
        for (i, item) in program.items.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.emit_item(item);
            self.newline();
        }

        // Prepend structural equality helper if any == or != was used
        if self.needs_deep_equal {
            let helper = concat!(
                "function __zenEq(a: unknown, b: unknown): boolean {\n",
                "  if (a === b) return true;\n",
                "  if (a == null || b == null) return false;\n",
                "  if (typeof a !== \"object\" || typeof b !== \"object\") return false;\n",
                "  const ka = Object.keys(a as object);\n",
                "  const kb = Object.keys(b as object);\n",
                "  if (ka.length !== kb.length) return false;\n",
                "  return ka.every((k) => __zenEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));\n",
                "}\n\n",
            );
            self.output = format!("{helper}{}", self.output);
        }

        CodegenOutput {
            code: self.output,
            has_jsx: self.has_jsx,
        }
    }

    // ── Items ────────────────────────────────────────────────────

    fn emit_item(&mut self, item: &Item) {
        match &item.kind {
            ItemKind::Import(decl) => self.emit_import(decl),
            ItemKind::Const(decl) => self.emit_const(decl),
            ItemKind::Function(decl) => self.emit_function(decl),
            ItemKind::TypeDecl(decl) => self.emit_type_decl(decl),
            ItemKind::Expr(expr) => {
                self.emit_indent();
                self.emit_expr(expr);
                self.push(";");
            }
        }
    }

    // ── Import ───────────────────────────────────────────────────

    fn emit_import(&mut self, decl: &ImportDecl) {
        self.emit_indent();
        if decl.specifiers.is_empty() {
            self.push(&format!("import \"{}\";", decl.source));
        } else {
            self.push("import { ");
            for (i, spec) in decl.specifiers.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&spec.name);
                if let Some(alias) = &spec.alias {
                    self.push(" as ");
                    self.push(alias);
                }
            }
            self.push(&format!(" }} from \"{}\";", decl.source));
        }
    }

    // ── Const ────────────────────────────────────────────────────

    fn emit_const(&mut self, decl: &ConstDecl) {
        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        self.push("const ");

        match &decl.binding {
            ConstBinding::Name(name) => self.push(name),
            ConstBinding::Array(names) => {
                self.push("[");
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(name);
                }
                self.push("]");
            }
            ConstBinding::Object(names) => {
                self.push("{ ");
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(name);
                }
                self.push(" }");
            }
        }

        if let Some(type_ann) = &decl.type_ann {
            self.push(": ");
            self.emit_type_expr(type_ann);
        }

        self.push(" = ");
        self.emit_expr(&decl.value);
        self.push(";");
    }

    // ── Function ─────────────────────────────────────────────────

    fn emit_function(&mut self, decl: &FunctionDecl) {
        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        if decl.async_fn {
            self.push("async ");
        }
        self.push("function ");
        self.push(&decl.name);
        self.push("(");
        self.emit_params(&decl.params);
        self.push(")");

        if let Some(ret) = &decl.return_type {
            self.push(": ");
            self.emit_type_expr(ret);
        }

        self.push(" ");
        self.emit_block_expr(&decl.body);
    }

    fn emit_params(&mut self, params: &[Param]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&param.name);
            if let Some(type_ann) = &param.type_ann {
                self.push(": ");
                self.emit_type_expr(type_ann);
            }
            if let Some(default) = &param.default {
                self.push(" = ");
                self.emit_expr(default);
            }
        }
    }

    // ── Type Declarations ────────────────────────────────────────

    fn emit_type_decl(&mut self, decl: &TypeDecl) {
        self.emit_indent();
        if decl.exported {
            self.push("export ");
        }
        self.push("type ");
        self.push(&decl.name);

        if !decl.type_params.is_empty() {
            self.push("<");
            for (i, tp) in decl.type_params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(tp);
            }
            self.push(">");
        }

        self.push(" = ");

        match &decl.def {
            TypeDef::Record(fields) => {
                self.emit_record_type(fields);
            }
            TypeDef::Union(variants) => {
                self.emit_union_type(&decl.name, variants);
            }
            TypeDef::Alias(type_expr) => {
                // Brand and opaque types erase to their underlying type
                self.emit_type_expr(type_expr);
            }
        }

        self.push(";");
    }

    fn emit_record_type(&mut self, fields: &[RecordField]) {
        self.push("{ ");
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.push("; ");
            }
            self.push(&field.name);
            self.push(": ");
            self.emit_type_expr(&field.type_ann);
        }
        self.push(" }");
    }

    fn emit_union_type(&mut self, _parent_name: &str, variants: &[Variant]) {
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }

            if variant.fields.is_empty() {
                // Simple variant: `{ tag: "Home" }`
                self.push(&format!("{{ tag: \"{}\" }}", variant.name));
            } else {
                // Variant with fields: `{ tag: "Profile"; id: string }`
                self.push(&format!("{{ tag: \"{}\"", variant.name));
                for field in &variant.fields {
                    self.push("; ");
                    if let Some(name) = &field.name {
                        self.push(name);
                    } else {
                        self.push("value");
                    }
                    self.push(": ");
                    self.emit_type_expr(&field.type_ann);
                }
                self.push(" }");
            }
        }
    }

    // ── Type Expressions ─────────────────────────────────────────

    fn emit_type_expr(&mut self, type_expr: &TypeExpr) {
        match &type_expr.kind {
            TypeExprKind::Named { name, type_args } => {
                // Brand<T, "Name"> erases to T
                if name == "Brand" && type_args.len() == 2 {
                    self.emit_type_expr(&type_args[0]);
                    return;
                }
                // Option<T> becomes T | undefined
                if name == "Option" && type_args.len() == 1 {
                    self.emit_type_expr(&type_args[0]);
                    self.push(" | undefined");
                    return;
                }
                // Result<T, E> becomes { ok: true; value: T } | { ok: false; error: E }
                if name == "Result" && type_args.len() == 2 {
                    self.push("{ ok: true; value: ");
                    self.emit_type_expr(&type_args[0]);
                    self.push(" } | { ok: false; error: ");
                    self.emit_type_expr(&type_args[1]);
                    self.push(" }");
                    return;
                }

                // Unit type () becomes void in TypeScript
                if name == "()" {
                    self.push("void");
                    return;
                }

                self.push(name);
                if !type_args.is_empty() {
                    self.push("<");
                    for (i, arg) in type_args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.emit_type_expr(arg);
                    }
                    self.push(">");
                }
            }
            TypeExprKind::Record(fields) => {
                self.emit_record_type(fields);
            }
            TypeExprKind::Function {
                params,
                return_type,
            } => {
                self.push("(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(&format!("_p{i}: "));
                    self.emit_type_expr(param);
                }
                self.push(") => ");
                self.emit_type_expr(return_type);
            }
            TypeExprKind::Array(inner) => {
                self.emit_type_expr(inner);
                self.push("[]");
            }
            TypeExprKind::Tuple(types) => {
                self.push("[");
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.emit_type_expr(t);
                }
                self.push("]");
            }
        }
    }

    // ── Expressions ──────────────────────────────────────────────

    fn emit_expr(&mut self, expr: &Expr) {
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
            ExprKind::Identifier(name) => self.push(name),
            ExprKind::Placeholder => self.push("_"),

            ExprKind::Binary { left, op, right } => match op {
                BinOp::Eq => {
                    self.needs_deep_equal = true;
                    self.push("__zenEq(");
                    self.emit_expr(left);
                    self.push(", ");
                    self.emit_expr(right);
                    self.push(")");
                }
                BinOp::NotEq => {
                    self.needs_deep_equal = true;
                    self.push("!__zenEq(");
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

            ExprKind::Call { callee, args } => {
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
            ExprKind::Construct {
                type_name: _,
                spread,
                args,
            } => {
                self.push("{ ");
                if let Some(spread_expr) = spread {
                    self.push("...");
                    self.emit_expr(spread_expr);
                    if !args.is_empty() {
                        self.push(", ");
                    }
                }
                self.emit_named_fields(args);
                self.push(" }");
            }

            ExprKind::Member { object, field } => {
                self.emit_expr(object);
                self.push(".");
                self.push(field);
            }

            ExprKind::Index { object, index } => {
                self.emit_expr(object);
                self.push("[");
                self.emit_expr(index);
                self.push("]");
            }

            ExprKind::Arrow { params, body } => {
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
                self.emit_expr(body);
            }

            // Match: `match x { A -> ..., B -> ... }` → ternary chain
            ExprKind::Match { subject, arms } => {
                self.emit_match(subject, arms);
            }

            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.emit_expr(condition);
                self.push(" ? ");
                self.emit_expr(then_branch);
                self.push(" : ");
                if let Some(else_expr) = else_branch {
                    self.emit_expr(else_expr);
                } else {
                    self.push("undefined");
                }
            }

            ExprKind::Return(value) => {
                self.push("return");
                if let Some(v) = value {
                    self.push(" ");
                    self.emit_expr(v);
                }
            }

            ExprKind::Await(inner) => {
                self.push("await ");
                self.emit_expr(inner);
            }

            // Ok(value) → { ok: true, value: value }
            ExprKind::Ok(inner) => {
                self.push("{ ok: true as const, value: ");
                self.emit_expr(inner);
                self.push(" }");
            }

            // Err(error) → { ok: false, error: error }
            ExprKind::Err(inner) => {
                self.push("{ ok: false as const, error: ");
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

            ExprKind::Spread(inner) => {
                self.push("...");
                self.emit_expr(inner);
            }
        }
    }

    // ── Pipe Lowering ────────────────────────────────────────────

    /// Try to emit a stdlib call. Returns Some(output) if the callee is a stdlib function.
    fn try_emit_stdlib_call(&mut self, callee: &Expr, args: &[Arg]) -> Option<String> {
        if let ExprKind::Member { object, field } = &callee.kind
            && let ExprKind::Identifier(module) = &object.kind
            && let Some(stdlib_fn) = self.stdlib.lookup(module, field)
        {
            // Collect emitted args
            let arg_strings: Vec<String> = args
                .iter()
                .map(|arg| {
                    let mut sub = Codegen::new();
                    match arg {
                        Arg::Positional(e) => sub.emit_expr(e),
                        Arg::Named { value, .. } => sub.emit_expr(value),
                    }
                    sub.output
                })
                .collect();

            if stdlib_fn.codegen.contains("__zenEq") {
                self.needs_deep_equal = true;
            }

            Some(expand_codegen_template(stdlib_fn.codegen, &arg_strings))
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
            // First arg is the piped value
            let mut sub = Codegen::new();
            sub.emit_expr(left);
            let mut arg_strings = vec![sub.output];

            // Remaining args
            for arg in extra_args {
                let mut sub = Codegen::new();
                match arg {
                    Arg::Positional(e) => sub.emit_expr(e),
                    Arg::Named { value, .. } => sub.emit_expr(value),
                }
                arg_strings.push(sub.output);
            }

            if stdlib_fn.codegen.contains("__zenEq") {
                self.needs_deep_equal = true;
            }

            Some(expand_codegen_template(stdlib_fn.codegen, &arg_strings))
        } else {
            None
        }
    }

    fn emit_pipe(&mut self, left: &Expr, right: &Expr) {
        match &right.kind {
            // Stdlib pipe: `arr |> Array.sort` or `arr |> Array.map(fn)`
            ExprKind::Call { callee, args } if !has_placeholder_arg(args) => {
                if let Some(output) = self.try_emit_stdlib_pipe(left, callee, args) {
                    self.push(&output);
                    return;
                }
                // Fall through to normal call handling below
                self.emit_expr(callee);
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
            ExprKind::Call { callee, args } if has_placeholder_arg(args) => {
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
            // `a |> f` → `f(a)` — bare function
            ExprKind::Identifier(_) => {
                self.emit_expr(right);
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

    // ── Match Lowering ───────────────────────────────────────────

    fn emit_match(&mut self, subject: &Expr, arms: &[MatchArm]) {
        // Emit as nested ternary: `subject.tag === "A" ? ... : subject.tag === "B" ? ... : unreachable()`
        self.emit_match_arms(subject, arms, 0);
    }

    fn emit_match_arms(&mut self, subject: &Expr, arms: &[MatchArm], index: usize) {
        if index >= arms.len() {
            // Should be unreachable if match is exhaustive
            self.push("(() => { throw new Error(\"non-exhaustive match\"); })()");
            return;
        }

        let arm = &arms[index];
        let is_last = index == arms.len() - 1;

        // Wildcard or binding at the end → just emit the body
        if is_last
            && matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            )
        {
            self.emit_match_body(subject, &arm.pattern, &arm.body);
            return;
        }

        self.emit_pattern_condition(subject, &arm.pattern);
        self.push(" ? ");
        self.emit_match_body(subject, &arm.pattern, &arm.body);
        self.push(" : ");

        if is_last {
            self.push("(() => { throw new Error(\"non-exhaustive match\"); })()");
        } else {
            self.emit_match_arms(subject, arms, index + 1);
        }
    }

    fn emit_pattern_condition(&mut self, subject: &Expr, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Literal(lit) => {
                self.emit_expr(subject);
                self.push(" === ");
                self.emit_literal_pattern(lit);
            }
            PatternKind::Range { start, end } => {
                self.push("(");
                self.emit_expr(subject);
                self.push(" >= ");
                self.emit_literal_pattern(start);
                self.push(" && ");
                self.emit_expr(subject);
                self.push(" <= ");
                self.emit_literal_pattern(end);
                self.push(")");
            }
            PatternKind::Variant { name, fields } => {
                // Check tag
                self.emit_expr(subject);
                self.push(&format!(".tag === \"{}\"", name));

                // Nested conditions for sub-patterns
                for (i, field_pat) in fields.iter().enumerate() {
                    if !matches!(
                        field_pat.kind,
                        PatternKind::Wildcard | PatternKind::Binding(_)
                    ) {
                        self.push(" && ");
                        // Access the field — for single-field variants use .value
                        let field_access = if fields.len() == 1 {
                            format!("{}.value", self.expr_to_string(subject))
                        } else {
                            format!("{}._{i}", self.expr_to_string(subject))
                        };
                        let field_expr = Expr {
                            kind: ExprKind::Identifier(field_access),
                            span: subject.span,
                        };
                        self.emit_pattern_condition(&field_expr, field_pat);
                    }
                }
            }
            PatternKind::Record { fields } => {
                let mut first = true;
                for (name, pat) in fields {
                    if matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                        continue;
                    }
                    if !first {
                        self.push(" && ");
                    }
                    first = false;
                    let field_expr = Expr {
                        kind: ExprKind::Identifier(format!(
                            "{}.{}",
                            self.expr_to_string(subject),
                            name
                        )),
                        span: subject.span,
                    };
                    self.emit_pattern_condition(&field_expr, pat);
                }
                if first {
                    // All fields are bindings/wildcards — always true
                    self.push("true");
                }
            }
            PatternKind::Binding(_) | PatternKind::Wildcard => {
                self.push("true");
            }
        }
    }

    fn emit_match_body(&mut self, subject: &Expr, pattern: &Pattern, body: &Expr) {
        // For patterns with bindings, wrap in an IIFE to introduce variables
        let bindings = collect_bindings(subject, pattern, &|s| self.expr_to_string(s));
        if bindings.is_empty() {
            self.emit_expr(body);
        } else {
            self.push("(() => { ");
            for (name, access) in &bindings {
                self.push(&format!("const {name} = {access}; "));
            }
            self.push("return ");
            self.emit_expr(body);
            self.push("; })()");
        }
    }

    fn emit_literal_pattern(&mut self, lit: &LiteralPattern) {
        match lit {
            LiteralPattern::Number(n) => self.push(n),
            LiteralPattern::String(s) => self.push(&format!("\"{}\"", escape_string(s))),
            LiteralPattern::Bool(b) => self.push(if *b { "true" } else { "false" }),
        }
    }

    // ── Constructor → Object Literal ─────────────────────────────

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

    fn emit_block_expr(&mut self, expr: &Expr) {
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

    // ── JSX ──────────────────────────────────────────────────────

    fn emit_jsx(&mut self, element: &JsxElement) {
        match &element.kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            } => {
                self.push(&format!("<{name}"));
                for prop in props {
                    self.push(" ");
                    self.push(&prop.name);
                    if let Some(value) = &prop.value {
                        self.push("={");
                        self.emit_expr(value);
                        self.push("}");
                    }
                }
                if *self_closing {
                    self.push(" />");
                } else {
                    self.push(">");
                    self.emit_jsx_children(children);
                    self.push(&format!("</{name}>"));
                }
            }
            JsxElementKind::Fragment { children } => {
                self.push("<>");
                self.emit_jsx_children(children);
                self.push("</>");
            }
        }
    }

    fn emit_jsx_children(&mut self, children: &[JsxChild]) {
        for child in children {
            match child {
                JsxChild::Text(text) => self.push(text),
                JsxChild::Expr(expr) => {
                    self.push("{");
                    self.emit_expr(expr);
                    self.push("}");
                }
                JsxChild::Element(element) => self.emit_jsx(element),
            }
        }
    }

    // ── Output helpers ───────────────────────────────────────────

    fn push(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }

    fn emit_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
    }

    fn expr_to_string(&self, expr: &Expr) -> String {
        let mut cg = Codegen::new();
        cg.emit_expr(expr);
        cg.output
    }
}

impl Default for Codegen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ──────────────────────────────────────────────────────

/// Expand a codegen template like `$0.map($1)` with actual arg strings.
fn expand_codegen_template(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();
    // Replace in reverse order so $10 doesn't get matched by $1
    for (i, arg) in args.iter().enumerate().rev() {
        result = result.replace(&format!("${i}"), arg);
    }
    result
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "===",
        BinOp::NotEq => "!==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn unaryop_str(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "!",
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn has_placeholder_arg(args: &[Arg]) -> bool {
    args.iter().any(|a| match a {
        Arg::Positional(expr) => matches!(expr.kind, ExprKind::Placeholder),
        Arg::Named { value, .. } => matches!(value.kind, ExprKind::Placeholder),
    })
}

/// Collect variable bindings from a match pattern.
fn collect_bindings(
    subject: &Expr,
    pattern: &Pattern,
    expr_to_str: &dyn Fn(&Expr) -> String,
) -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    collect_bindings_inner(subject, pattern, expr_to_str, &mut bindings);
    bindings
}

fn collect_bindings_inner(
    subject: &Expr,
    pattern: &Pattern,
    expr_to_str: &dyn Fn(&Expr) -> String,
    bindings: &mut Vec<(String, String)>,
) {
    match &pattern.kind {
        PatternKind::Binding(name) => {
            bindings.push((name.clone(), expr_to_str(subject)));
        }
        PatternKind::Variant { fields, .. } => {
            for (i, field_pat) in fields.iter().enumerate() {
                let field_access = if fields.len() == 1 {
                    format!("{}.value", expr_to_str(subject))
                } else {
                    format!("{}._{i}", expr_to_str(subject))
                };
                let field_expr = Expr {
                    kind: ExprKind::Identifier(field_access.clone()),
                    span: subject.span,
                };
                collect_bindings_inner(&field_expr, field_pat, expr_to_str, bindings);
            }
        }
        PatternKind::Record { fields } => {
            for (name, pat) in fields {
                let field_access = format!("{}.{}", expr_to_str(subject), name);
                let field_expr = Expr {
                    kind: ExprKind::Identifier(field_access.clone()),
                    span: subject.span,
                };
                collect_bindings_inner(&field_expr, pat, expr_to_str, bindings);
            }
        }
        PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Range { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn emit(input: &str) -> String {
        let program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
            panic!(
                "parse failed:\n{}",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        let output = Codegen::new().generate(&program);
        output.code.trim().to_string()
    }

    // ── Basic Expressions ────────────────────────────────────────

    #[test]
    fn number_literal() {
        assert_eq!(emit("42"), "42;");
    }

    #[test]
    fn string_literal() {
        assert_eq!(emit(r#""hello""#), r#""hello";"#);
    }

    #[test]
    fn bool_literal() {
        assert_eq!(emit("true"), "true;");
    }

    #[test]
    fn binary_expr() {
        assert_eq!(emit("1 + 2"), "1 + 2;");
    }

    #[test]
    fn unary_expr() {
        assert_eq!(emit("!x"), "!x;");
    }

    #[test]
    fn member_access() {
        assert_eq!(emit("a.b.c"), "a.b.c;");
    }

    #[test]
    fn function_call() {
        assert_eq!(emit("f(1, 2)"), "f(1, 2);");
    }

    #[test]
    fn named_args_erased() {
        assert_eq!(emit("f(name: x, limit: 10)"), "f(x, 10);");
    }

    #[test]
    fn template_literal() {
        assert_eq!(emit("`hello ${name}`"), "`hello ${name}`;");
    }

    // ── Declarations ─────────────────────────────────────────────

    #[test]
    fn const_decl() {
        assert_eq!(emit("const x = 42"), "const x = 42;");
    }

    #[test]
    fn const_with_type() {
        assert_eq!(emit("const x: number = 42"), "const x: number = 42;");
    }

    #[test]
    fn export_const() {
        assert_eq!(emit("export const x = 42"), "export const x = 42;");
    }

    #[test]
    fn const_array_destructure() {
        assert_eq!(emit("const [a, b] = pair"), "const [a, b] = pair;");
    }

    #[test]
    fn function_decl() {
        let result = emit("function add(a: number, b: number): number { a + b }");
        assert_eq!(
            result,
            "function add(a: number, b: number): number {\n  a + b;\n}"
        );
    }

    #[test]
    fn export_function() {
        let result = emit("export function greet() { \"hi\" }");
        assert!(result.starts_with("export function greet()"));
    }

    #[test]
    fn async_function() {
        let result = emit("async function fetch() { await getData() }");
        assert!(result.starts_with("async function fetch()"));
    }

    #[test]
    fn function_with_defaults() {
        let result = emit("function f(x: number = 10) { x }");
        assert!(result.contains("x: number = 10"));
    }

    // ── Imports ──────────────────────────────────────────────────

    #[test]
    fn import_named() {
        assert_eq!(
            emit(r#"import { useState, useEffect } from "react""#),
            r#"import { useState, useEffect } from "react";"#
        );
    }

    // ── Pipe Operator ────────────────────────────────────────────

    #[test]
    fn pipe_simple() {
        // x |> f → f(x)
        assert_eq!(emit("x |> f"), "f(x);");
    }

    #[test]
    fn pipe_with_args() {
        // x |> f(y) → f(x, y)
        assert_eq!(emit("x |> f(y)"), "f(x, y);");
    }

    #[test]
    fn pipe_with_placeholder() {
        // x |> f(y, _, z) → f(y, x, z)
        assert_eq!(emit("x |> f(y, _, z)"), "f(y, x, z);");
    }

    #[test]
    fn pipe_chained() {
        // a |> f |> g → g(f(a))
        assert_eq!(emit("a |> f |> g"), "g(f(a));");
    }

    // ── Partial Application ──────────────────────────────────────

    #[test]
    fn partial_application() {
        // add(10, _) → (_x) => add(10, _x)
        assert_eq!(emit("add(10, _)"), "(_x) => add(10, _x);");
    }

    // ── Result / Option ──────────────────────────────────────────

    #[test]
    fn ok_constructor() {
        assert_eq!(emit("Ok(42)"), "{ ok: true as const, value: 42 };");
    }

    #[test]
    fn err_constructor() {
        assert_eq!(
            emit(r#"Err("not found")"#),
            r#"{ ok: false as const, error: "not found" };"#
        );
    }

    #[test]
    fn some_constructor() {
        // Some(x) → x
        assert_eq!(emit("Some(x)"), "x;");
    }

    #[test]
    fn none_literal() {
        // None → undefined
        assert_eq!(emit("None"), "undefined;");
    }

    // ── Constructors ─────────────────────────────────────────────

    #[test]
    fn constructor_named() {
        assert_eq!(
            emit(r#"User(name: "Ryan", email: e)"#),
            r#"{ name: "Ryan", email: e };"#
        );
    }

    #[test]
    fn constructor_with_spread() {
        assert_eq!(
            emit(r#"User(..user, name: "New")"#),
            r#"{ ...user, name: "New" };"#
        );
    }

    // ── Match ────────────────────────────────────────────────────

    #[test]
    fn match_simple() {
        let result = emit("match x { Ok(v) -> v, Err(e) -> e }");
        assert!(result.contains(".tag === \"Ok\""));
        assert!(result.contains(".tag === \"Err\""));
    }

    #[test]
    fn match_with_wildcard() {
        let result = emit("match x { Ok(v) -> v, _ -> 0 }");
        // Last arm is wildcard → no condition needed
        assert!(result.contains(".tag === \"Ok\""));
        assert!(result.contains("0"));
    }

    #[test]
    fn match_literal() {
        let result = emit("match n { 1 -> true, _ -> false }");
        assert!(result.contains("=== 1"));
    }

    #[test]
    fn match_range() {
        let result = emit("match n { 1..10 -> true, _ -> false }");
        assert!(result.contains(">= 1"));
        assert!(result.contains("<= 10"));
    }

    // ── Type Declarations ────────────────────────────────────────

    #[test]
    fn type_record() {
        let result = emit("type User = { id: string, name: string }");
        assert_eq!(result, "type User = { id: string; name: string };");
    }

    #[test]
    fn type_union() {
        let result = emit("type Route = | Home | Profile(id: string) | NotFound");
        assert!(result.contains("tag: \"Home\""));
        assert!(result.contains("tag: \"Profile\""));
        assert!(result.contains("tag: \"NotFound\""));
    }

    #[test]
    fn type_alias() {
        assert_eq!(emit("type Name = string"), "type Name = string;");
    }

    #[test]
    fn opaque_type_erased() {
        assert_eq!(
            emit("opaque type HashedPassword = string"),
            "type HashedPassword = string;"
        );
    }

    #[test]
    fn brand_type_erased() {
        // Brand<string, "UserId"> → string
        let result = emit("type UserId = Brand<string, UserId>");
        assert_eq!(result, "type UserId = string;");
    }

    #[test]
    fn option_type() {
        let result = emit("const x: Option<string> = None");
        assert!(result.contains("string | undefined"));
    }

    #[test]
    fn result_type() {
        let result = emit("type Res = Result<User, ApiError>");
        assert!(result.contains("ok: true"));
        assert!(result.contains("ok: false"));
    }

    // ── JSX ──────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        let result = emit("<Button />");
        assert_eq!(result, "<Button />;");
    }

    #[test]
    fn jsx_with_props() {
        let result = emit(r#"<Button label="Save" onClick={handleSave} />"#);
        assert!(result.contains("label={\"Save\"}"));
        assert!(result.contains("onClick={handleSave}"));
    }

    #[test]
    fn jsx_with_children() {
        let result = emit("<div>{x}</div>");
        assert_eq!(result, "<div>{x}</div>;");
    }

    #[test]
    fn jsx_fragment() {
        let result = emit("<>{x}</>");
        assert_eq!(result, "<>{x}</>;");
    }

    #[test]
    fn jsx_detection() {
        let program = Parser::new("<Button />").parse_program().unwrap();
        let output = Codegen::new().generate(&program);
        assert!(output.has_jsx);
    }

    #[test]
    fn no_jsx_detection() {
        let program = Parser::new("const x = 42").parse_program().unwrap();
        let output = Codegen::new().generate(&program);
        assert!(!output.has_jsx);
    }

    // ── Arrow Functions ──────────────────────────────────────────

    #[test]
    fn arrow_single_arg() {
        assert_eq!(emit("x => x + 1"), "(x) => x + 1;");
    }

    #[test]
    fn arrow_multi_arg() {
        assert_eq!(emit("(a, b) => a + b"), "(a, b) => a + b;");
    }

    // ── Equality → structural equality ──────────────────────────

    #[test]
    fn equality_becomes_structural() {
        let result = emit("a == b");
        assert!(result.contains("__zenEq(a, b)"));
        let result = emit("a != b");
        assert!(result.contains("!__zenEq(a, b)"));
    }

    // ── If/Else → ternary ────────────────────────────────────────

    #[test]
    fn if_else() {
        assert_eq!(
            emit("if x { 1 } else { 2 }"),
            "x ? {\n  1;\n} : {\n  2;\n};"
        );
    }

    // ── Await ────────────────────────────────────────────────────

    #[test]
    fn await_expr() {
        assert_eq!(emit("await fetchData()"), "await fetchData();");
    }

    // ── Return ───────────────────────────────────────────────────

    #[test]
    fn return_expr() {
        let result = emit("function f() { return 42 }");
        assert!(result.contains("return 42"));
    }

    // ── Array ────────────────────────────────────────────────────

    #[test]
    fn array_literal() {
        assert_eq!(emit("[1, 2, 3]"), "[1, 2, 3];");
    }
}
