use crate::parser::ast::{
    Arg, BinOp, ConstBinding, ExprKind, ItemKind, TemplatePart, TypedArg, TypedExpr, TypedItem,
    TypedTemplatePart, TypedTypeExpr,
};
use crate::pretty::{self, Document};
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

use super::super::{
    DEEP_EQUAL_FN, GlobalName, binop_str, checker_agrees, escape_string, has_placeholder_arg,
    unaryop_str,
};
use super::generator::{THROW_NOT_IMPLEMENTED, THROW_UNREACHABLE, TypeScriptGenerator};

/// A single step in a flattened pipe+unwrap chain.
pub(super) struct PipeStep {
    pub expr: TypedExpr,
    /// The checker's type for the value this step produces. For a pipe step
    /// that is the enclosing `Pipe` node, not `expr`: `expr` holds only the
    /// right side, and the step emits the whole pipe. `emit_const_unwrap`
    /// reads this to decide whether the step needs `await`.
    pub ty: std::sync::Arc<crate::checker::Type>,
    pub unwrap: bool,
    pub is_pipe: bool,
}

impl<'a> TypeScriptGenerator<'a> {
    // ── Expressions ──────────────────────────────────────────────

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    pub(super) fn emit_expr(&mut self, expr: &TypedExpr) -> Document {
        match &expr.kind {
            ExprKind::Number(n) => pretty::str(n),
            ExprKind::String(s) => pretty::str(format!("\"{}\"", escape_string(s))),
            ExprKind::TemplateLiteral(parts) => self.emit_template(None, parts),
            ExprKind::TaggedTemplate { tag, parts } => self.emit_template(Some(tag), parts),
            ExprKind::Bool(b) => pretty::str(if *b { "true" } else { "false" }),
            ExprKind::Identifier(name) => self.emit_identifier(name, &expr.ty),
            ExprKind::Placeholder => pretty::str("_"),

            ExprKind::Binary { left, op, right } => match op {
                BinOp::Eq => {
                    self.needs_deep_equal = true;
                    pretty::concat([
                        pretty::str(format!("{DEEP_EQUAL_FN}(")),
                        self.emit_expr(left),
                        pretty::str(", "),
                        self.emit_expr(right),
                        pretty::str(")"),
                    ])
                }
                BinOp::NotEq => {
                    self.needs_deep_equal = true;
                    pretty::concat([
                        pretty::str(format!("!{DEEP_EQUAL_FN}(")),
                        self.emit_expr(left),
                        pretty::str(", "),
                        self.emit_expr(right),
                        pretty::str(")"),
                    ])
                }
                _ => pretty::concat([
                    self.emit_expr(left),
                    pretty::str(format!(" {} ", binop_str(*op))),
                    self.emit_expr(right),
                ]),
            },

            ExprKind::Unary { op, operand } => {
                pretty::concat([pretty::str(unaryop_str(*op)), self.emit_expr(operand)])
            }

            ExprKind::Pipe { left, right } => self.emit_pipe(left, right),

            ExprKind::Unwrap(inner) => {
                // The wrapper runs the inner expression inside its own arrow,
                // so an `await` in that expression lands in the arrow and not
                // in the enclosing function. The arrow was plain every time,
                // and `Http.get(url) |> Promise.await?` emitted
                // `(() => { const __r = (await fetch(url)); ... })()`. tsc read
                // that as `TS2311: Cannot find name 'await'` (glb #1499).
                //
                // `expr_contains_await` is the checker's own
                // `body_has_promise_await`, and the checker marks the enclosing
                // function `async` from the same answer, so the added `await`
                // always has an async function to sit in. `emit_parse` and the
                // two match wrappers build this same shape.
                let is_async = expr_contains_await(inner);
                let inner_doc = self.emit_expr(inner);
                let (open, close) = if is_async {
                    ("(await (async () => { const __r = ", " })())")
                } else {
                    ("(() => { const __r = ", " })()")
                };
                pretty::concat([
                    pretty::str(open),
                    inner_doc,
                    pretty::str(
                        "; if (typeof __r === 'object' && __r !== null && 'ok' in __r && typeof __r.ok === 'boolean') { if (!__r.ok) throw __r.error; return __r.value; } return __r;",
                    ),
                    pretty::str(close),
                ])
            }

            ExprKind::Call {
                callee,
                type_args,
                args,
            } => {
                if self.is_untrusted_call(callee) {
                    let returns_promise = matches!(&*expr.ty, crate::checker::Type::Promise(_));
                    // The wrapper evaluates the callee and every argument
                    // inside its own arrow. An `await` in one of them lands in
                    // that arrow, so the arrow has to be `async` even when the
                    // call itself hands back no promise. The wrapper's value
                    // stays the `Result` the checker typed, so this shape
                    // awaits the arrow back at the call site (glb #1499).
                    let inner_awaits = expr_contains_await(callee)
                        || args.iter().any(|arg| match arg {
                            Arg::Positional(value) | Arg::Named { value, .. } => {
                                expr_contains_await(value)
                            }
                        });
                    let await_wrapper = !returns_promise && inner_awaits;
                    let mut docs = Vec::new();
                    if returns_promise {
                        docs.push(pretty::str(format!(
                            "(async () => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: await "
                        )));
                    } else if await_wrapper {
                        docs.push(pretty::str(format!(
                            "(await (async () => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: "
                        )));
                    } else {
                        docs.push(pretty::str(format!(
                            "(() => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: "
                        )));
                    }
                    docs.push(self.emit_expr(callee));
                    docs.push(self.emit_type_args(type_args));
                    docs.push(pretty::str("("));
                    docs.push(self.emit_args(args));
                    docs.push(pretty::str(")"));
                    let mut close = format!(
                        " }}; }} catch (_e) {{ return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: _e instanceof Error ? _e : new Error(String(_e)) }}; }} }})()"
                    );
                    if await_wrapper {
                        close.push(')');
                    }
                    docs.push(pretty::str(close));
                    pretty::concat(docs)
                } else if let Some(output) = self.try_emit_stdlib_call(callee, args) {
                    pretty::str(output)
                } else if has_placeholder_arg(args) {
                    self.emit_partial_application(callee, type_args, args)
                } else {
                    pretty::concat([
                        self.emit_expr(callee),
                        self.emit_type_args(type_args),
                        pretty::str("("),
                        self.emit_args(args),
                        pretty::str(")"),
                    ])
                }
            }

            ExprKind::Construct {
                type_name,
                spread,
                args,
                ..
            } => self.emit_construct(type_name, spread.as_deref(), args),

            ExprKind::Member { object, field } => self.emit_member(object, field, &expr.ty),

            ExprKind::Index { object, index } => pretty::concat([
                self.emit_expr(object),
                pretty::str("["),
                self.emit_expr(index),
                pretty::str("]"),
            ]),

            ExprKind::Arrow {
                async_fn,
                params,
                body,
            } => {
                let mut docs = Vec::new();
                if *async_fn {
                    docs.push(pretty::str("async "));
                }
                docs.push(pretty::str("("));
                docs.push(self.emit_params(params));
                docs.push(pretty::str(") => "));
                if matches!(body.kind, ExprKind::Block(_)) {
                    docs.push(self.emit_block_expr_with_return(body));
                } else {
                    let needs_parens =
                        matches!(body.kind, ExprKind::Construct { .. } | ExprKind::Object(_));
                    if needs_parens {
                        docs.push(pretty::str("("));
                    }
                    docs.push(self.emit_expr(body));
                    if needs_parens {
                        docs.push(pretty::str(")"));
                    }
                }
                pretty::concat(docs)
            }

            ExprKind::Match { subject, arms } => self.emit_match(subject, arms),

            ExprKind::Parse { type_arg, value } => self.emit_parse(type_arg, value, &expr.ty),

            ExprKind::Mock {
                type_arg,
                overrides,
            } => self.emit_mock(type_arg, overrides, &mut 0),

            ExprKind::Value(inner) => self.emit_expr(inner),
            ExprKind::Clear => pretty::str("null"),
            ExprKind::Unchanged | ExprKind::Unit => pretty::str("undefined"),
            ExprKind::Todo => pretty::str(THROW_NOT_IMPLEMENTED),
            ExprKind::Unreachable => pretty::str(THROW_UNREACHABLE),

            ExprKind::Jsx(element) => {
                self.has_jsx = true;
                self.emit_jsx(element)
            }

            ExprKind::Collect(items) => self.emit_collect_block(items),
            ExprKind::Block(items) => self.emit_block_items(items),

            ExprKind::Grouped(inner) => {
                pretty::concat([pretty::str("("), self.emit_expr(inner), pretty::str(")")])
            }

            ExprKind::Array(elements) | ExprKind::Tuple(elements) => {
                let mut docs = vec![pretty::str("[")];
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    docs.push(self.emit_expr(elem));
                }
                docs.push(pretty::str("]"));
                pretty::concat(docs)
            }

            ExprKind::Spread(inner) => pretty::concat([pretty::str("..."), self.emit_expr(inner)]),

            ExprKind::Object(fields) => {
                let mut docs = vec![pretty::str("{ ")];
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        docs.push(pretty::str(", "));
                    }
                    docs.push(pretty::str(key));
                    docs.push(pretty::str(": "));
                    docs.push(self.emit_expr(value));
                }
                docs.push(pretty::str(" }"));
                pretty::concat(docs)
            }

            ExprKind::DotShorthand { field, predicate } => match predicate {
                Some((op, rhs)) => match op {
                    BinOp::Eq => {
                        self.needs_deep_equal = true;
                        pretty::concat([
                            pretty::str(format!("(_x) => {DEEP_EQUAL_FN}(_x.")),
                            pretty::str(field),
                            pretty::str(", "),
                            self.emit_expr(rhs),
                            pretty::str(")"),
                        ])
                    }
                    BinOp::NotEq => {
                        self.needs_deep_equal = true;
                        pretty::concat([
                            pretty::str(format!("(_x) => !{DEEP_EQUAL_FN}(_x.")),
                            pretty::str(field),
                            pretty::str(", "),
                            self.emit_expr(rhs),
                            pretty::str(")"),
                        ])
                    }
                    _ => pretty::concat([
                        pretty::str("(_x) => _x."),
                        pretty::str(field),
                        pretty::str(format!(" {} ", binop_str(*op))),
                        self.emit_expr(rhs),
                    ]),
                },
                None => pretty::concat([pretty::str("(_x) => _x."), pretty::str(field)]),
            },

            // The checker rejected this expression, so `attach_types`
            // replaced the subtree with `Invalid`. There is no
            // TypeScript that stands for it. Say so as a diagnostic:
            // a marker in the output is not one, and the file used to
            // ship with `undefined` in it and a green check (#1493).
            ExprKind::Invalid => {
                self.report_unemittable(expr.id, expr.span);

                pretty::str("undefined")
            }
        }
    }

    // ── Construct ────────────────────────────────────────────────

    fn emit_construct(
        &mut self,
        type_name: &str,
        spread: Option<&TypedExpr>,
        args: &[TypedArg],
    ) -> Document {
        // Ok(value)
        if type_name == "Ok" && args.len() == 1 && spread.is_none() {
            let val = match &args[0] {
                Arg::Positional(e) | Arg::Named { value: e, .. } => self.emit_expr(e),
            };
            return pretty::concat([
                pretty::str(format!("{{ {OK_FIELD}: true as const, {VALUE_FIELD}: ")),
                val,
                pretty::str(" }"),
            ]);
        }
        // Err(error)
        if type_name == "Err" && args.len() == 1 && spread.is_none() {
            let val = match &args[0] {
                Arg::Positional(e) | Arg::Named { value: e, .. } => self.emit_expr(e),
            };
            return pretty::concat([
                pretty::str(format!("{{ {OK_FIELD}: false as const, {ERROR_FIELD}: ")),
                val,
                pretty::str(" }"),
            ]);
        }
        // Qualified non-unit variant with no args → function value
        if args.is_empty()
            && spread.is_none()
            && let Some(field_names) = self
                .ctx
                .variant_info
                .get(type_name)
                .filter(|(_, f)| !f.is_empty())
                .map(|(_, f)| f.clone())
        {
            return self.emit_variant_constructor_fn(type_name, &field_names);
        }

        let variant_field_names = self
            .ctx
            .variant_info
            .get(type_name)
            .map(|(_, fields)| fields.clone());
        let is_variant = variant_field_names.is_some();

        // npm constructor: positional args, unknown type → new Name(args)
        let has_named_args = args.iter().any(|a| matches!(a, Arg::Named { .. }));
        let is_known_type = self.ctx.type_defs.contains_key(type_name);
        if !is_variant && !has_named_args && !is_known_type && spread.is_none() {
            let mut docs = vec![
                pretty::str("new "),
                pretty::str(type_name),
                pretty::str("("),
            ];
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    docs.push(pretty::str(", "));
                }
                if let Arg::Positional(e) = arg {
                    docs.push(self.emit_expr(e));
                }
            }
            docs.push(pretty::str(")"));
            return pretty::concat(docs);
        }

        // Types with trait impls use a factory function
        if !is_variant && self.ctx.type_trait_impls.contains_key(type_name) {
            let mut docs = vec![pretty::str(format!("{type_name}__make({{ "))];
            if let Some(spread_expr) = spread {
                docs.push(pretty::str("..."));
                docs.push(self.emit_expr(spread_expr));
                if !args.is_empty() {
                    docs.push(pretty::str(", "));
                }
            }
            docs.push(self.emit_named_fields(args));
            docs.push(pretty::str(" })"));
            return pretty::concat(docs);
        }

        let mut docs = vec![pretty::str("{ ")];
        if is_variant {
            docs.push(pretty::str(format!("{TAG_FIELD}: \"{type_name}\"")));
            if !args.is_empty() || spread.is_some() {
                docs.push(pretty::str(", "));
            }
        }
        if let Some(spread_expr) = spread {
            docs.push(pretty::str("..."));
            docs.push(self.emit_expr(spread_expr));
            if !args.is_empty() {
                docs.push(pretty::str(", "));
            }
        }
        if let Some(ref field_names) = variant_field_names {
            docs.push(self.emit_construct_fields(args, field_names));
        } else {
            docs.push(self.emit_named_fields(args));
        }
        docs.push(pretty::str(" }"));
        pretty::concat(docs)
    }

    // ── Identifiers ─────────────────────────────────────────────

    /// Emit a bare identifier.
    ///
    /// Three file-global maps claim bare names: unit variants, variant
    /// constructors and for-block functions. None of them carries a scope,
    /// so each answer is guarded on `ty`, the type the checker resolved for
    /// this expression. A local binding that shadows one of these names
    /// resolves to its own type, the guard fails, and codegen emits the
    /// plain name the checker validated.
    fn emit_identifier(&mut self, name: &str, ty: &crate::checker::Type) -> Document {
        let ctx = self.ctx;
        if let Some((union, field_names)) = ctx.variant_info.get(name) {
            if field_names.is_empty() {
                if checker_agrees(ty, &GlobalName::Variant { union }) {
                    return pretty::str(format!("{{ {TAG_FIELD}: \"{name}\" }}"));
                }
            } else if checker_agrees(ty, &GlobalName::Constructor { union }) {
                return self.emit_variant_constructor_fn(name, field_names);
            }
        }
        if let Some(entry) = ctx.resolved_for_block_fn(name, ty) {
            return pretty::str(entry.emitted_name(&self.import_aliases));
        }

        pretty::str(name)
    }

    // ── Member Access ───────────────────────────────────────────

    fn emit_member(
        &mut self,
        object: &TypedExpr,
        field: &str,
        ty: &crate::checker::Type,
    ) -> Document {
        // For-block function: Entry.toModel → Entry__toModel
        if let ExprKind::Identifier(type_name) = &object.kind
            && let Some(entry) = self
                .ctx
                .for_block_fns
                .get(&(type_name.clone(), field.to_string()))
            && checker_agrees(
                ty,
                &GlobalName::ForBlockFn {
                    shape: &entry.shape,
                },
            )
        {
            return pretty::str(entry.emitted_name(&self.import_aliases));
        }
        // Union variant access: Filter.All → { tag: "All" }
        if let ExprKind::Identifier(type_name) = &object.kind
            && self
                .ctx
                .variant_info
                .get(field)
                .is_some_and(|(union_name, _)| union_name == type_name)
            && checker_agrees(ty, &GlobalName::Variant { union: type_name })
        {
            return pretty::str(format!("{{ {TAG_FIELD}: \"{field}\" }}"));
        }
        // Tuple index: pair.0 → pair[0]
        if field.chars().all(|c| c.is_ascii_digit()) {
            return pretty::concat([
                self.emit_expr(object),
                pretty::str("["),
                pretty::str(field),
                pretty::str("]"),
            ]);
        }
        pretty::concat([self.emit_expr(object), pretty::str("."), pretty::str(field)])
    }

    // ── Untrusted Call Check ────────────────────────────────────

    #[allow(clippy::unused_self)]
    fn is_untrusted_call(&self, callee: &TypedExpr) -> bool {
        callee.ty.is_untrusted_foreign()
    }

    // ── Stdlib Helpers ─────────────────────────────────────────

    pub(super) fn emit_arg_strings(&mut self, args: &[TypedArg]) -> Vec<String> {
        args.iter()
            .map(|arg| {
                let doc = match arg {
                    Arg::Positional(e) => self.emit_expr(e),
                    Arg::Named { value, .. } => self.emit_expr(value),
                };
                Self::doc_to_string(&doc)
            })
            .collect()
    }

    pub(super) fn emit_expr_string(&mut self, expr: &TypedExpr) -> String {
        let doc = self.emit_expr(expr);
        Self::doc_to_string(&doc)
    }

    fn emit_template(&mut self, tag: Option<&TypedExpr>, parts: &[TypedTemplatePart]) -> Document {
        let mut docs = Vec::with_capacity(parts.len() * 3 + 3);
        if let Some(tag) = tag {
            docs.push(self.emit_expr(tag));
        }
        docs.push(pretty::str("`"));
        for part in parts {
            match part {
                TemplatePart::Raw(s) => docs.push(pretty::str(s)),
                TemplatePart::Expr(e) => {
                    docs.push(pretty::str("${"));
                    docs.push(self.emit_expr(e));
                    docs.push(pretty::str("}"));
                }
            }
        }
        docs.push(pretty::str("`"));
        pretty::concat(docs)
    }

    pub(super) fn apply_stdlib_template(
        &mut self,
        template: &str,
        arg_strings: &[String],
    ) -> String {
        if template.contains(DEEP_EQUAL_FN) {
            self.needs_deep_equal = true;
        }
        super::super::expand_codegen_template(template, arg_strings)
    }

    // ── Constructor Helpers ─────────────────────────────────────

    #[allow(clippy::unused_self)]
    fn emit_variant_constructor_fn(&self, variant_name: &str, field_names: &[String]) -> Document {
        let params = field_names.join(", ");
        let fields = field_names
            .iter()
            .map(|f| format!(", {f}"))
            .collect::<String>();
        pretty::str(format!(
            "({params}) => ({{ {TAG_FIELD}: \"{variant_name}\"{fields} }})"
        ))
    }

    fn emit_construct_fields(&mut self, args: &[TypedArg], field_names: &[String]) -> Document {
        let mut docs = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(", "));
            }
            match arg {
                Arg::Named { label, value } => {
                    docs.push(pretty::str(label));
                    docs.push(pretty::str(": "));
                    docs.push(self.emit_expr(value));
                }
                Arg::Positional(expr) => {
                    if let Some(name) = field_names.get(i) {
                        docs.push(pretty::str(name));
                        docs.push(pretty::str(": "));
                    }
                    docs.push(self.emit_expr(expr));
                }
            }
        }
        pretty::concat(docs)
    }

    pub(super) fn emit_named_fields(&mut self, args: &[TypedArg]) -> Document {
        let mut docs = Vec::new();
        let mut first = true;
        for arg in args {
            if matches!(arg, Arg::Named { value, .. } if matches!(value.kind, ExprKind::Unchanged))
            {
                continue;
            }
            if !first {
                docs.push(pretty::str(", "));
            }
            first = false;
            match arg {
                Arg::Named { label, value } => {
                    docs.push(pretty::str(label));
                    docs.push(pretty::str(": "));
                    docs.push(self.emit_expr(value));
                }
                Arg::Positional(expr) => {
                    docs.push(self.emit_expr(expr));
                }
            }
        }
        pretty::concat(docs)
    }

    // ── Arguments (labels erased) ────────────────────────────────

    pub(super) fn emit_args(&mut self, args: &[TypedArg]) -> Document {
        let mut docs = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(", "));
            }
            match arg {
                Arg::Positional(expr) => docs.push(self.emit_expr(expr)),
                Arg::Named { value, .. } => docs.push(self.emit_expr(value)),
            }
        }
        pretty::concat(docs)
    }

    // ── Block ────────────────────────────────────────────────────

    pub(super) fn emit_block_expr_with_return(&mut self, expr: &TypedExpr) -> Document {
        match &expr.kind {
            ExprKind::Block(items) => {
                let mut inner = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    inner.push(pretty::line());
                    if is_last && matches!(item.kind, ItemKind::Expr(_)) {
                        if let ItemKind::Expr(e) = &item.kind {
                            inner.push(pretty::str("return "));
                            inner.push(self.emit_expr(e));
                            inner.push(pretty::str(";"));
                        }
                    } else {
                        inner.push(self.emit_item(item));
                    }
                }
                pretty::concat([
                    pretty::str("{"),
                    pretty::nest(2, pretty::concat(inner)),
                    pretty::line(),
                    pretty::str("}"),
                ])
            }
            _ => pretty::concat([
                pretty::str("{"),
                pretty::nest(
                    2,
                    pretty::concat([
                        pretty::line(),
                        pretty::str("return "),
                        self.emit_expr(expr),
                        pretty::str(";"),
                    ]),
                ),
                pretty::line(),
                pretty::str("}"),
            ]),
        }
    }

    pub(super) fn emit_block_expr(&mut self, expr: &TypedExpr) -> Document {
        match &expr.kind {
            ExprKind::Block(items) => self.emit_block_items(items),
            _ => pretty::concat([
                pretty::str("{"),
                pretty::nest(
                    2,
                    pretty::concat([pretty::line(), self.emit_expr(expr), pretty::str(";")]),
                ),
                pretty::line(),
                pretty::str("}"),
            ]),
        }
    }

    pub(super) fn emit_block_items(&mut self, items: &[TypedItem]) -> Document {
        let mut inner = Vec::new();
        for item in items {
            inner.push(pretty::line());
            inner.push(self.emit_item(item));
        }
        pretty::concat([
            pretty::str("{"),
            pretty::nest(2, pretty::concat(inner)),
            pretty::line(),
            pretty::str("}"),
        ])
    }

    // ── Collect Block ───────────────────────────────────────────

    fn emit_collect_block(&mut self, items: &[TypedItem]) -> Document {
        let has_await = collect_block_is_async(items);

        let mut inner = Vec::new();
        inner.push(pretty::line());
        inner.push(pretty::str("const __errors: Array<any> = [];"));

        let mut result_counter = 0;

        for (i, item) in items.iter().enumerate() {
            let is_last = i == items.len() - 1;
            if is_last {
                if let ItemKind::Expr(expr) = &item.kind {
                    inner.push(pretty::line());
                    inner.push(pretty::str(
                        "if (__errors.length > 0) return { ok: false as const, error: __errors };",
                    ));
                    inner.push(pretty::line());
                    let expr_doc = self.emit_expr(expr);
                    inner.push(pretty::concat([
                        pretty::str("return { ok: true as const, value: "),
                        expr_doc,
                        pretty::str(" };"),
                    ]));
                } else {
                    inner.extend(self.emit_collect_item(item, &mut result_counter));
                    inner.push(pretty::line());
                    inner.push(pretty::str(
                        "if (__errors.length > 0) return { ok: false as const, error: __errors };",
                    ));
                    inner.push(pretty::line());
                    inner.push(pretty::str(
                        "return { ok: true as const, value: undefined };",
                    ));
                }
            } else {
                inner.extend(self.emit_collect_item(item, &mut result_counter));
            }
        }

        let prefix = if has_await {
            "(async () => {"
        } else {
            "(() => {"
        };

        pretty::concat([
            pretty::str(prefix),
            pretty::nest(2, pretty::concat(inner)),
            pretty::line(),
            pretty::str("})()"),
        ])
    }

    fn emit_collect_item(&mut self, item: &TypedItem, result_counter: &mut usize) -> Vec<Document> {
        let mut docs = Vec::new();
        match &item.kind {
            ItemKind::Const(decl) => {
                if let Some(unwrap_inner) = Self::find_unwrap_in_expr(&decl.value) {
                    let idx = *result_counter;
                    *result_counter += 1;
                    let temp = format!("_r{idx}");

                    docs.push(pretty::line());
                    let inner_doc = self.emit_expr(unwrap_inner);
                    docs.push(pretty::concat([
                        pretty::str(format!("const {temp} = ")),
                        inner_doc,
                        pretty::str(";"),
                    ]));

                    docs.push(pretty::line());
                    docs.push(pretty::str(format!(
                        "if (!{temp}.ok) __errors.push({temp}.error);"
                    )));

                    docs.push(pretty::line());
                    match &decl.binding {
                        ConstBinding::Name(name) => {
                            docs.push(pretty::str(format!(
                                "const {name} = {temp}.ok ? {temp}.value : undefined as any;"
                            )));
                        }
                        _ => {
                            docs.push(pretty::str(format!(
                                "const __v{idx} = {temp}.ok ? {temp}.value : undefined as any;"
                            )));
                        }
                    }
                } else {
                    docs.push(pretty::line());
                    docs.push(self.emit_item(item));
                }
            }
            ItemKind::Expr(expr) => {
                if let ExprKind::Unwrap(inner) = &expr.kind {
                    let idx = *result_counter;
                    *result_counter += 1;
                    let temp = format!("_r{idx}");

                    docs.push(pretty::line());
                    let inner_doc = self.emit_expr(inner);
                    docs.push(pretty::concat([
                        pretty::str(format!("const {temp} = ")),
                        inner_doc,
                        pretty::str(";"),
                    ]));

                    docs.push(pretty::line());
                    docs.push(pretty::str(format!(
                        "if (!{temp}.ok) __errors.push({temp}.error);"
                    )));
                } else {
                    docs.push(pretty::line());
                    let expr_doc = self.emit_expr(expr);
                    docs.push(pretty::concat([expr_doc, pretty::str(";")]));
                }
            }
            _ => {
                docs.push(pretty::line());
                docs.push(self.emit_item(item));
            }
        }
        docs
    }

    pub fn find_unwrap_in_expr(expr: &TypedExpr) -> Option<&TypedExpr> {
        match &expr.kind {
            ExprKind::Unwrap(inner) => Some(inner),
            _ => None,
        }
    }

    // ── Partial Application ──────────────────────────────────────

    pub(super) fn emit_partial_application(
        &mut self,
        callee: &TypedExpr,
        type_args: &[TypedTypeExpr],
        args: &[TypedArg],
    ) -> Document {
        // Each `_` placeholder becomes a distinct arrow parameter. A single
        // placeholder keeps the historical `_x` name for compact output;
        // two or more use indexed names `_x0, _x1, …` in left-to-right
        // source order.
        let placeholder_count = args
            .iter()
            .filter(|a| match a {
                Arg::Positional(e) | Arg::Named { value: e, .. } => {
                    matches!(e.kind, ExprKind::Placeholder)
                }
            })
            .count();
        let name_for = |idx: usize| {
            if placeholder_count == 1 {
                "_x".to_string()
            } else {
                format!("_x{idx}")
            }
        };

        let param_list = (0..placeholder_count)
            .map(name_for)
            .collect::<Vec<_>>()
            .join(", ");

        let mut docs = vec![
            pretty::str(format!("({param_list}) => ")),
            self.emit_expr(callee),
            self.emit_type_args(type_args),
            pretty::str("("),
        ];
        let mut placeholder_idx = 0;
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                docs.push(pretty::str(", "));
            }
            let value_expr = match arg {
                Arg::Positional(expr) => expr,
                Arg::Named { value, .. } => value,
            };
            if matches!(value_expr.kind, ExprKind::Placeholder) {
                docs.push(pretty::str(name_for(placeholder_idx)));
                placeholder_idx += 1;
            } else {
                docs.push(self.emit_expr(value_expr));
            }
        }
        docs.push(pretty::str(")"));
        pretty::concat(docs)
    }

    // ── Pipe-Unwrap Chain Helpers ────────────────────────────────

    pub(super) fn expr_has_unwrap(expr: &TypedExpr) -> bool {
        match &expr.kind {
            ExprKind::Unwrap(_) => true,
            ExprKind::Pipe { left, right } => {
                Self::expr_has_unwrap(left) || Self::expr_has_unwrap(right)
            }
            _ => false,
        }
    }

    pub(super) fn flatten_pipe_unwrap_chain(expr: &TypedExpr) -> Vec<PipeStep> {
        let mut steps = Vec::new();
        Self::collect_pipe_steps(expr, &mut steps);
        steps
    }

    fn collect_pipe_steps(expr: &TypedExpr, steps: &mut Vec<PipeStep>) {
        match &expr.kind {
            ExprKind::Unwrap(inner) => match &inner.kind {
                ExprKind::Pipe { left, right } => {
                    Self::collect_pipe_steps(left, steps);
                    steps.push(PipeStep {
                        expr: (**right).clone(),
                        ty: inner.ty.clone(),
                        unwrap: true,
                        is_pipe: true,
                    });
                }
                _ => {
                    steps.push(PipeStep {
                        expr: (**inner).clone(),
                        ty: inner.ty.clone(),
                        unwrap: true,
                        is_pipe: false,
                    });
                }
            },
            ExprKind::Pipe { left, right } => {
                Self::collect_pipe_steps(left, steps);
                steps.push(PipeStep {
                    expr: (**right).clone(),
                    ty: expr.ty.clone(),
                    unwrap: false,
                    is_pipe: true,
                });
            }
            _ => {
                steps.push(PipeStep {
                    expr: expr.clone(),
                    ty: expr.ty.clone(),
                    unwrap: false,
                    is_pipe: false,
                });
            }
        }
    }
}

/// Check whether `emit_collect_block` wraps these items in an async IIFE.
///
/// A `collect { ... }` block that awaits inside emits `(async () => { ... })()`,
/// so the emitted expression is a promise. The block's own type stays the
/// `Result` it collects, so no type read can see that promise. This is the one
/// async IIFE codegen builds for a reason the checker's type does not record,
/// and `emit_const_unwrap` reads this same function rather than the emitted
/// text (glb #1522).
///
/// The per-item walk below is the checker's own: each item goes to
/// [`expr_contains_await`], which is `body_has_promise_await`. So this is not a
/// third answer to "does this await", it is the checker's answer asked once per
/// item (glb #1516).
pub(super) fn collect_block_is_async(items: &[TypedItem]) -> bool {
    items.iter().any(|item| match &item.kind {
        ItemKind::Expr(e) => expr_contains_await(e),
        ItemKind::Const(c) => expr_contains_await(&c.value),
        _ => false,
    })
}

/// True when an expression tree awaits, so the wrapper codegen emits for it
/// must be `async`.
///
/// The checker asks the same question in `body_has_promise_await`, and its
/// answer decides whether the enclosing function is `async`. Codegen carried
/// a second copy of the walk, and the copy was narrower: it never descended
/// into an array or a tuple, so `collect { let xs = [g() |> await] }` typed
/// as async and emitted `await` inside a plain `(() => {` arrow (glb #1516).
/// One function, two readers, so the two passes cannot part again.
pub(super) fn expr_contains_await(expr: &TypedExpr) -> bool {
    crate::checker::body_has_promise_await(expr)
}
