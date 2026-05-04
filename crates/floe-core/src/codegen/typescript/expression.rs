use crate::parser::ast::{
    Arg, BinOp, ConstBinding, ExprKind, ItemKind, TemplatePart, TypedArg, TypedExpr, TypedItem,
    TypedTemplatePart,
};
use crate::pretty::{self, Document};
use crate::type_layout::{ERROR_FIELD, OK_FIELD, TAG_FIELD, VALUE_FIELD};

use super::super::{DEEP_EQUAL_FN, binop_str, escape_string, has_placeholder_arg, unaryop_str};
use super::generator::{THROW_NOT_IMPLEMENTED, THROW_UNREACHABLE, TypeScriptGenerator};

/// A single step in a flattened pipe+unwrap chain.
pub(super) struct PipeStep {
    pub expr: TypedExpr,
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
            ExprKind::Identifier(name) => {
                if self.ctx.unit_variants.contains(name.as_str()) {
                    pretty::str(format!("{{ {TAG_FIELD}: \"{name}\" }}"))
                } else if let Some(field_names) = self
                    .ctx
                    .variant_info
                    .get(name.as_str())
                    .filter(|(_, f)| !f.is_empty())
                    .map(|(_, f)| f.clone())
                {
                    self.emit_variant_constructor_fn(name, &field_names)
                } else if let Some(mangled) = self
                    .ctx
                    .lookup_for_block_fn_by_name(name, &self.import_aliases)
                {
                    pretty::str(mangled)
                } else {
                    pretty::str(name)
                }
            }
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
                let inner_doc = self.emit_expr(inner);
                pretty::concat([
                    pretty::str("(() => { const __r = "),
                    inner_doc,
                    pretty::str(
                        "; if (typeof __r === 'object' && __r !== null && 'ok' in __r && typeof __r.ok === 'boolean') { if (!__r.ok) throw __r.error; return __r.value; } return __r; })()",
                    ),
                ])
            }

            ExprKind::Call { callee, args, .. } => {
                if self.is_untrusted_call(callee) {
                    let is_async = matches!(&*expr.ty, crate::checker::Type::Promise(_));
                    let mut docs = Vec::new();
                    if is_async {
                        docs.push(pretty::str(format!(
                            "(async () => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: await "
                        )));
                    } else {
                        docs.push(pretty::str(format!(
                            "(() => {{ try {{ return {{ {OK_FIELD}: true as const, {VALUE_FIELD}: "
                        )));
                    }
                    docs.push(self.emit_expr(callee));
                    docs.push(pretty::str("("));
                    docs.push(self.emit_args(args));
                    docs.push(pretty::str(")"));
                    docs.push(pretty::str(format!(
                        " }}; }} catch (_e) {{ return {{ {OK_FIELD}: false as const, {ERROR_FIELD}: _e instanceof Error ? _e : new Error(String(_e)) }}; }} }})()"
                    )));
                    pretty::concat(docs)
                } else if let Some(output) = self.try_emit_stdlib_call(callee, args) {
                    pretty::str(output)
                } else if has_placeholder_arg(args) {
                    self.emit_partial_application(callee, args)
                } else {
                    pretty::concat([
                        self.emit_expr(callee),
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

            ExprKind::Member { object, field } => self.emit_member(object, field),

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
                    let needs_parens = matches!(
                        body.kind,
                        ExprKind::Construct { .. } | ExprKind::Object(_)
                    );
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

            ExprKind::Parse { type_arg, value } => self.emit_parse(type_arg, value),

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

            ExprKind::Invalid => pretty::str("undefined /* type error */"),
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

    // ── Member Access ───────────────────────────────────────────

    fn emit_member(&mut self, object: &TypedExpr, field: &str) -> Document {
        // For-block function: Entry.toModel → Entry__toModel
        if let ExprKind::Identifier(type_name) = &object.kind
            && let Some(mangled) = self
                .ctx
                .for_block_fns
                .get(&(type_name.clone(), field.to_string()))
        {
            let name = self
                .import_aliases
                .get(mangled)
                .cloned()
                .unwrap_or_else(|| mangled.clone());
            return pretty::str(name);
        }
        // Union variant access: Filter.All → { tag: "All" }
        if let ExprKind::Identifier(type_name) = &object.kind
            && self
                .ctx
                .variant_info
                .get(field)
                .is_some_and(|(union_name, _)| union_name == type_name)
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
        let has_await = items.iter().any(|item| match &item.kind {
            ItemKind::Expr(e) => expr_contains_await(e),
            ItemKind::Const(c) => expr_contains_await(&c.value),
            _ => false,
        });

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
                        unwrap: true,
                        is_pipe: true,
                    });
                }
                _ => {
                    steps.push(PipeStep {
                        expr: (**inner).clone(),
                        unwrap: true,
                        is_pipe: false,
                    });
                }
            },
            ExprKind::Pipe { left, right } => {
                Self::collect_pipe_steps(left, steps);
                steps.push(PipeStep {
                    expr: (**right).clone(),
                    unwrap: false,
                    is_pipe: true,
                });
            }
            _ => {
                steps.push(PipeStep {
                    expr: expr.clone(),
                    unwrap: false,
                    is_pipe: false,
                });
            }
        }
    }
}

/// Check if an expression tree contains a Promise.await stdlib call.
pub(super) fn expr_contains_await(expr: &TypedExpr) -> bool {
    match &expr.kind {
        ExprKind::Member { object, field }
            if field == "await"
                && matches!(&object.kind, ExprKind::Identifier(m) if m == "Promise") =>
        {
            true
        }
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
