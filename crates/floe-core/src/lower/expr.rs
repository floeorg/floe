use super::{
    Arg, Expr, ExprKind, FloeLang, Item, ItemKind, Lowerer, Param, ParamDestructure, Span,
    SyntaxKind, SyntaxNode, TypeExpr,
};

impl<'src> Lowerer<'src> {
    #[allow(clippy::cognitive_complexity)]
    #[allow(clippy::too_many_lines)]
    pub(super) fn lower_expr_node(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let span = self.node_span(node);

        match node.kind() {
            SyntaxKind::PIPE_EXPR => {
                // Check for pipe-into-match: `x |> match { ... }`
                // If the last child node is a MATCH_EXPR without a subject, desugar
                // to `match x { ... }` by using the left side as the match subject.
                let last_child_node = node.children().last();
                let has_pipe_match = last_child_node.as_ref().is_some_and(|c| {
                    c.kind() == SyntaxKind::MATCH_EXPR && self.is_subjectless_match(c)
                });

                if has_pipe_match {
                    // Lower the left side: collect all expressions except the match
                    let match_node = last_child_node?;
                    let mut left_exprs = Vec::new();
                    for child_or_token in node.children_with_tokens() {
                        match child_or_token {
                            rowan::NodeOrToken::Node(ref child) => {
                                if child.kind() != SyntaxKind::MATCH_EXPR
                                    && let Some(expr) = self.lower_expr_node(child)
                                {
                                    left_exprs.push(expr);
                                }
                            }
                            rowan::NodeOrToken::Token(ref token) => {
                                if !matches!(
                                    token.kind(),
                                    SyntaxKind::PIPE | SyntaxKind::PIPE_UNWRAP
                                ) && let Some(expr) = self.token_to_expr(token)
                                {
                                    left_exprs.push(expr);
                                }
                            }
                        }
                    }
                    let left = left_exprs.into_iter().next()?;

                    // Lower the match arms from the MATCH_EXPR node
                    let mut arms = Vec::new();
                    for child in match_node.children() {
                        if child.kind() == SyntaxKind::MATCH_ARM
                            && let Some(arm) = self.lower_match_arm(&child)
                        {
                            arms.push(arm);
                        }
                    }

                    Some(self.expr(
                        ExprKind::Match {
                            subject: Box::new(left),
                            arms,
                        },
                        self.node_span(node),
                    ))
                } else {
                    let exprs = self.lower_child_exprs(node);
                    if exprs.len() >= 2 {
                        let mut iter = exprs.into_iter();
                        let left = iter.next()?;
                        let right = iter.next()?;

                        // `|>?` operator: wrap the pipe in Unwrap
                        let is_pipe_unwrap = node.children_with_tokens().any(|ct| {
                            ct.as_token()
                                .is_some_and(|t| t.kind() == SyntaxKind::PIPE_UNWRAP)
                        });
                        let pipe_span = self.node_span(node);

                        if is_pipe_unwrap {
                            Some(self.expr(
                                ExprKind::Unwrap(Box::new(self.expr(
                                    ExprKind::Pipe {
                                        left: Box::new(left),
                                        right: Box::new(right),
                                    },
                                    pipe_span,
                                ))),
                                pipe_span,
                            ))
                        } else if let ExprKind::Unwrap(inner) = right.kind {
                            // `a |> f?` restructure: lift `?` above the pipe → `(a |> f)?`
                            Some(self.expr(
                                ExprKind::Unwrap(Box::new(self.expr(
                                    ExprKind::Pipe {
                                        left: Box::new(left),
                                        right: inner,
                                    },
                                    pipe_span,
                                ))),
                                pipe_span,
                            ))
                        } else {
                            Some(self.expr(
                                ExprKind::Pipe {
                                    left: Box::new(left),
                                    right: Box::new(right),
                                },
                                pipe_span,
                            ))
                        }
                    } else {
                        exprs.into_iter().next()
                    }
                }
            }

            SyntaxKind::BINARY_EXPR => {
                let op = self.find_binary_op(node)?;
                let exprs = self.lower_child_exprs(node);
                if exprs.len() >= 2 {
                    let mut iter = exprs.into_iter();
                    let left = iter.next()?;
                    let right = iter.next()?;
                    Some(self.expr(
                        ExprKind::Binary {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        },
                        span,
                    ))
                } else {
                    None
                }
            }

            SyntaxKind::UNARY_EXPR => {
                let op = self.find_unary_op(node)?;
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(self.expr(
                    ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                    span,
                ))
            }

            SyntaxKind::UNWRAP_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(self.expr(ExprKind::Unwrap(Box::new(operand)), span))
            }

            SyntaxKind::MEMBER_EXPR => {
                let exprs = self.lower_child_exprs(node);
                let object = exprs.into_iter().next()?;
                // Find the field name: the token AFTER the DOT
                let mut found_dot = false;
                let mut field = String::new();
                for token in node.children_with_tokens() {
                    if let Some(token) = token.as_token() {
                        if token.kind() == SyntaxKind::DOT {
                            found_dot = true;
                        } else if found_dot
                            && !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
                        {
                            field = token.text().to_string();
                            break;
                        }
                    }
                }
                if field.is_empty() {
                    // Fallback to last ident
                    let idents = self.collect_idents(node);
                    field = idents.last()?.clone();
                }
                Some(self.expr(
                    ExprKind::Member {
                        object: Box::new(object),
                        field,
                    },
                    span,
                ))
            }

            SyntaxKind::INDEX_EXPR => {
                let exprs = self.lower_child_exprs(node);
                let mut iter = exprs.into_iter();
                let object = iter.next()?;
                let index = iter.next()?;
                Some(self.expr(
                    ExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                    span,
                ))
            }

            SyntaxKind::CALL_EXPR => {
                let mut child_exprs = Vec::new();
                let mut args = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::ARG => {
                            if let Some(arg) = self.lower_arg(&child) {
                                args.push(arg);
                            }
                        }
                        _ => {
                            if let Some(expr) = self.lower_expr_node(&child) {
                                child_exprs.push(expr);
                            }
                        }
                    }
                }

                // Also check for token-level expressions (ident, number, etc.)
                if child_exprs.is_empty()
                    && let Some(expr) = self.lower_token_expr(node)
                {
                    child_exprs.push(expr);
                }

                let callee = child_exprs.into_iter().next()?;

                // Collect type arguments from TYPE_EXPR children that appear
                // between the callee and the args (for generic calls like f<T>(x))
                let type_args: Vec<TypeExpr> = node
                    .children()
                    .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                    .filter_map(|c| self.lower_type_expr(&c))
                    .collect();

                Some(self.expr(
                    ExprKind::Call {
                        callee: Box::new(callee),
                        type_args,
                        args,
                    },
                    span,
                ))
            }

            SyntaxKind::TAGGED_TEMPLATE_EXPR => {
                let tag = self.lower_first_expr(node)?;
                let template = node
                    .children_with_tokens()
                    .filter_map(|ct| ct.into_token())
                    .find(|t| t.kind() == SyntaxKind::TEMPLATE_LITERAL)?;
                let template_start: usize = template.text_range().start().into();
                let parts = self.lower_template_literal(template.text(), template_start);
                Some(self.expr(
                    ExprKind::TaggedTemplate {
                        tag: Box::new(tag),
                        parts,
                    },
                    span,
                ))
            }

            SyntaxKind::CONSTRUCT_EXPR => {
                // For qualified variants like Route.Profile(...), there are multiple
                // idents before '('. We want the last one (the variant name).
                let idents = self.collect_idents_before_lparen(node);
                let type_name = idents.last()?.clone();

                let mut spread = None;
                let mut args = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::SPREAD_EXPR => {
                            let inner = self.lower_child_exprs(&child).into_iter().next()?;
                            spread = Some(Box::new(inner));
                        }
                        SyntaxKind::ARG => {
                            if let Some(arg) = self.lower_arg(&child) {
                                args.push(arg);
                            }
                        }
                        _ => {}
                    }
                }

                Some(self.expr(
                    ExprKind::Construct {
                        type_name,
                        spread,
                        args,
                    },
                    span,
                ))
            }

            SyntaxKind::BRACE_CONSTRUCT_EXPR => {
                let mut type_name = String::new();
                for tok in node.children_with_tokens() {
                    if let Some(tok) = tok.as_token() {
                        if tok.kind() == SyntaxKind::L_BRACE {
                            break;
                        }
                        if tok.kind() == SyntaxKind::IDENT {
                            type_name = tok.text().to_string();
                        }
                    }
                }

                let mut spread = None;
                let mut fields = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::SPREAD_EXPR => {
                            let inner = self.lower_child_exprs(&child).into_iter().next()?;
                            spread = Some(Box::new(inner));
                        }
                        SyntaxKind::BRACE_CONSTRUCT_FIELD => {
                            if let Some(field) = self.lower_brace_construct_field(&child) {
                                fields.push(field);
                            }
                        }
                        _ => {}
                    }
                }

                Some(self.expr(
                    ExprKind::BraceConstruct {
                        type_name,
                        spread,
                        fields,
                    },
                    span,
                ))
            }

            SyntaxKind::ARROW_EXPR => {
                let mut params = Vec::new();
                let mut body = None;

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::PARAM => {
                            if let Some(param) = self.lower_param(&child) {
                                params.push(param);
                            }
                        }
                        _ => {
                            if body.is_none() {
                                body = self.lower_expr_node(&child);
                            }
                        }
                    }
                }

                // If no child expression nodes, try token expr
                if body.is_none() {
                    body = self.lower_token_expr_after_lambda_delim(node);
                }

                Some(self.expr(
                    ExprKind::Arrow {
                        async_fn: false,
                        params,
                        body: Box::new(body?),
                    },
                    span,
                ))
            }

            SyntaxKind::MATCH_EXPR => {
                let mut subject = None;
                let mut arms = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::MATCH_ARM => {
                            if let Some(arm) = self.lower_match_arm(&child) {
                                arms.push(arm);
                            }
                        }
                        _ => {
                            if subject.is_none() {
                                subject = self.lower_expr_node(&child);
                                if subject.is_none() {
                                    subject = self.lower_token_expr_in_node(&child);
                                }
                            }
                        }
                    }
                }

                // If subject wasn't a child node, try as token in match expr node directly
                if subject.is_none() {
                    subject = self.lower_token_expr(node);
                }

                Some(self.expr(
                    ExprKind::Match {
                        subject: Box::new(subject?),
                        arms,
                    },
                    span,
                ))
            }

            SyntaxKind::COLLECT_EXPR => {
                // collect { ... } — the child is a BLOCK_EXPR
                let mut items = Vec::new();
                for child in node.children() {
                    if child.kind() == SyntaxKind::BLOCK_EXPR {
                        for block_child in child.children() {
                            match block_child.kind() {
                                SyntaxKind::ITEM => {
                                    if let Some(item) = self.lower_item(&block_child) {
                                        items.push(item);
                                    }
                                }
                                SyntaxKind::EXPR_ITEM => {
                                    if let Some(expr) = self.lower_first_expr(&block_child) {
                                        items.push(Item {
                                            kind: ItemKind::Expr(expr),
                                            span: self.node_span(&block_child),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Some(self.expr(ExprKind::Collect(items), span))
            }

            SyntaxKind::BLOCK_EXPR => {
                // Collect all child nodes first so we can look ahead for `use`
                let children: Vec<_> = node.children().collect();
                let items = self.lower_block_children(&children, span);
                Some(self.expr(ExprKind::Block(items), span))
            }

            SyntaxKind::PARSE_EXPR => {
                // parse<T>(value) or parse<T> (pipe context, placeholder)
                let type_arg = node
                    .children()
                    .find(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                    .and_then(|c| self.lower_type_expr(&c))?;

                let value = self
                    .lower_child_exprs(node)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| self.expr(ExprKind::Placeholder, span));

                Some(self.expr(
                    ExprKind::Parse {
                        type_arg,
                        value: Box::new(value),
                    },
                    span,
                ))
            }

            SyntaxKind::MOCK_EXPR => {
                // mock<T> or mock<T>(overrides...)
                let type_arg = node
                    .children()
                    .find(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                    .and_then(|c| self.lower_type_expr(&c))?;

                let mut overrides = Vec::new();
                for child in node.children() {
                    if child.kind() == SyntaxKind::ARG
                        && let Some(arg) = self.lower_arg(&child)
                    {
                        overrides.push(arg);
                    }
                }

                Some(self.expr(
                    ExprKind::Mock {
                        type_arg,
                        overrides,
                    },
                    span,
                ))
            }

            SyntaxKind::VALUE_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(self.expr(ExprKind::Value(Box::new(inner)), span))
            }

            SyntaxKind::GROUPED_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(self.expr(ExprKind::Grouped(Box::new(inner)), span))
            }

            SyntaxKind::OBJECT_EXPR => {
                let mut fields = Vec::new();
                for child in node.children() {
                    if child.kind() == SyntaxKind::OBJECT_FIELD {
                        let idents = self.collect_idents(&child);
                        if let Some(key) = idents.first() {
                            let value =
                                self.lower_object_field_value(&child).unwrap_or_else(|| {
                                    // Shorthand: { name } means { name: name }
                                    self.expr(
                                        ExprKind::Identifier(key.clone()),
                                        self.node_span(&child),
                                    )
                                });
                            fields.push((key.clone(), value));
                        }
                    }
                }
                Some(self.expr(ExprKind::Object(fields), span))
            }

            SyntaxKind::ARRAY_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                Some(self.expr(ExprKind::Array(elements), span))
            }

            SyntaxKind::TUPLE_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                if elements.is_empty() {
                    // Empty tuple: () → Unit
                    Some(self.expr(ExprKind::Unit, span))
                } else {
                    Some(self.expr(ExprKind::Tuple(elements), span))
                }
            }

            SyntaxKind::JSX_ELEMENT => {
                let element = self.lower_jsx_element(node);
                Some(self.expr(ExprKind::Jsx(element), span))
            }

            SyntaxKind::SPREAD_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(self.expr(ExprKind::Spread(Box::new(inner)), span))
            }

            SyntaxKind::DOT_SHORTHAND => {
                let idents = self.collect_idents_direct(node);
                let field = idents.first()?.clone();

                // Check for binary op and RHS expression
                let predicate = self.find_binary_op(node).and_then(|op| {
                    // Find the RHS expression after the binary operator
                    let rhs = self.lower_expr_after_binary_op(node)?;
                    Some((op, Box::new(rhs)))
                });

                Some(self.expr(ExprKind::DotShorthand { field, predicate }, span))
            }

            // Error nodes, type expressions, patterns, and other non-expression nodes
            SyntaxKind::ERROR
            | SyntaxKind::TYPE_EXPR
            | SyntaxKind::TYPE_EXPR_FUNCTION
            | SyntaxKind::TYPE_EXPR_RECORD
            | SyntaxKind::TYPE_EXPR_TUPLE
            | SyntaxKind::TYPE_DEF_RECORD
            | SyntaxKind::TYPE_DEF_UNION
            | SyntaxKind::TYPE_DEF_ALIAS
            | SyntaxKind::TYPE_DEF_STRING_UNION
            | SyntaxKind::PARAM
            | SyntaxKind::PARAM_LIST
            | SyntaxKind::IMPORT_DECL
            | SyntaxKind::IMPORT_SPECIFIER
            | SyntaxKind::IMPORT_FOR_SPECIFIER
            | SyntaxKind::RECORD_FIELD
            | SyntaxKind::VARIANT
            | SyntaxKind::VARIANT_FIELD
            | SyntaxKind::MATCH_GUARD
            | SyntaxKind::OBJECT_FIELD => None,

            // For other kinds, try to extract token-level expressions
            _ => self.lower_token_expr_in_node(node),
        }
    }

    pub(super) fn lower_first_expr(&mut self, node: &SyntaxNode) -> Option<Expr> {
        // First try child nodes
        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                return Some(expr);
            }
        }
        // Then try tokens
        self.lower_token_expr(node)
    }

    pub(super) fn lower_first_expr_in(&mut self, node: &SyntaxNode) -> Option<Expr> {
        self.lower_first_expr(node)
    }

    pub(super) fn lower_child_exprs(&mut self, node: &SyntaxNode) -> Vec<Expr> {
        // Walk children_with_tokens so we pick up both child nodes (like CALL_EXPR)
        // and bare tokens (like NUMBER, IDENT) that represent expression operands.
        let mut exprs = Vec::new();

        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Node(child) => {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        exprs.push(expr);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr) = self.token_to_expr(&token) {
                        exprs.push(expr);
                    }
                }
            }
        }

        exprs
    }

    pub(super) fn lower_child_exprs_and_tokens(&mut self, node: &SyntaxNode) -> Vec<Expr> {
        let mut exprs = Vec::new();

        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Node(child) => {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        exprs.push(expr);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr) = self.token_to_expr(&token) {
                        exprs.push(expr);
                    }
                }
            }
        }

        exprs
    }

    pub(super) fn lower_token_expr(&mut self, node: &SyntaxNode) -> Option<Expr> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && let Some(expr) = self.token_to_expr(token)
            {
                return Some(expr);
            }
        }
        None
    }

    pub(super) fn lower_token_expr_in_node(&mut self, node: &SyntaxNode) -> Option<Expr> {
        self.lower_token_expr(node)
    }

    /// Lower the value expression from an OBJECT_FIELD node.
    /// An OBJECT_FIELD contains: IDENT COLON expr (or just IDENT for shorthand).
    /// We must skip the key IDENT and COLON tokens to find the value expression,
    /// otherwise `lower_first_expr` would pick up the key IDENT as the value.
    fn lower_object_field_value(&mut self, node: &SyntaxNode) -> Option<Expr> {
        // Check if there's a colon — if not, it's shorthand (no value to lower)
        let has_colon = node
            .children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == SyntaxKind::COLON));

        if !has_colon {
            // Shorthand: { name } — caller will create Identifier expr
            return None;
        }

        // First, try child expression nodes (these are unambiguous — they're always the value)
        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                return Some(expr);
            }
        }

        // Then try tokens, but only those after the colon
        let mut saw_colon = false;
        for child_or_token in node.children_with_tokens() {
            if let Some(token) = child_or_token.as_token() {
                if token.kind() == SyntaxKind::COLON {
                    saw_colon = true;
                    continue;
                }
                if saw_colon && let Some(expr) = self.token_to_expr(token) {
                    return Some(expr);
                }
            }
        }
        None
    }

    pub(super) fn lower_token_expr_after_lambda_delim(
        &mut self,
        node: &SyntaxNode,
    ) -> Option<Expr> {
        // For `(params) -> body` arrows, find token expr after the `->`.
        let mut found_arrow = false;
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::THIN_ARROW {
                    found_arrow = true;
                    continue;
                }
                if found_arrow && let Some(expr) = self.token_to_expr(token) {
                    return Some(expr);
                }
            }
        }
        None
    }

    #[allow(clippy::unused_self)]
    pub(super) fn token_to_expr(&self, token: &rowan::SyntaxToken<FloeLang>) -> Option<Expr> {
        let span = self.token_span(token);
        let text = token.text();

        match token.kind() {
            SyntaxKind::NUMBER => Some(self.expr(ExprKind::Number(text.to_string()), span)),
            SyntaxKind::STRING => {
                Some(self.expr(ExprKind::String(self.unquote_string(text)), span))
            }
            SyntaxKind::TEMPLATE_LITERAL => {
                let parts = self.lower_template_literal(text, span.start);
                Some(self.expr(ExprKind::TemplateLiteral(parts), span))
            }
            SyntaxKind::BOOL => Some(self.expr(ExprKind::Bool(text == "true"), span)),
            SyntaxKind::IDENT => Some(self.expr(ExprKind::Identifier(text.to_string()), span)),
            SyntaxKind::UNDERSCORE => Some(self.expr(ExprKind::Placeholder, span)),
            SyntaxKind::KW_CLEAR => Some(self.expr(ExprKind::Clear, span)),
            SyntaxKind::KW_UNCHANGED => Some(self.expr(ExprKind::Unchanged, span)),
            SyntaxKind::KW_TODO => Some(self.expr(ExprKind::Todo, span)),
            SyntaxKind::KW_UNREACHABLE => Some(self.expr(ExprKind::Unreachable, span)),
            SyntaxKind::KW_SELF => Some(self.expr(ExprKind::Identifier("self".to_string()), span)),
            _ => None,
        }
    }

    /// Lower a single `BRACE_CONSTRUCT_FIELD` node (`name: expr` or punned `name`).
    pub(super) fn lower_brace_construct_field(
        &mut self,
        node: &SyntaxNode,
    ) -> Option<crate::parser::ast::BraceField> {
        let span = self.node_span(node);
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();
        let value = if self.has_token(node, SyntaxKind::COLON) {
            self.lower_expr_after_colon(node).unwrap_or_else(|| {
                // Punning shorthand `{ name }` desugars to `{ name: name }`.
                self.expr(ExprKind::Identifier(name.clone()), span)
            })
        } else {
            self.expr(ExprKind::Identifier(name.clone()), span)
        };
        Some(crate::parser::ast::BraceField { name, value, span })
    }

    pub(super) fn lower_arg(&mut self, node: &SyntaxNode) -> Option<Arg> {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let idents = self.collect_idents_direct(node);
            let label = idents.first()?.clone();
            // Find the expression after the colon
            let value = self.lower_expr_after_colon(node).unwrap_or_else(|| {
                // Punning: `label:` without value → `label: label`
                self.expr(ExprKind::Identifier(label.clone()), self.node_span(node))
            });
            Some(Arg::Named { label, value })
        } else {
            let expr = self.lower_first_expr(node)?;
            Some(Arg::Positional(expr))
        }
    }

    fn lower_expr_after_binary_op(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_op = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    let is_binop = matches!(
                        token.kind(),
                        SyntaxKind::EQUAL_EQUAL
                            | SyntaxKind::BANG_EQUAL
                            | SyntaxKind::LESS_THAN
                            | SyntaxKind::GREATER_THAN
                            | SyntaxKind::LESS_EQUAL
                            | SyntaxKind::GREATER_EQUAL
                            | SyntaxKind::PLUS
                            | SyntaxKind::MINUS
                            | SyntaxKind::STAR
                            | SyntaxKind::SLASH
                            | SyntaxKind::PERCENT
                    );
                    if is_binop {
                        past_op = true;
                        continue;
                    }
                    if past_op && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_op {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }

    fn lower_expr_after_colon(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_colon = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::COLON {
                        past_colon = true;
                        continue;
                    }
                    if past_colon && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_colon {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }

    // ── Use desugaring ──────────────────────────────────────────

    /// Lower block children, desugaring any `use` declarations.
    /// When a `use` is encountered, all remaining children become the callback body.
    pub(super) fn lower_block_children(
        &mut self,
        children: &[SyntaxNode],
        block_span: Span,
    ) -> Vec<Item> {
        let mut items = Vec::new();
        let mut i = 0;

        while i < children.len() {
            let child = &children[i];

            // Check if this ITEM contains a USE_DECL
            if child.kind() == SyntaxKind::ITEM && self.item_contains_use(child) {
                // Desugar: use binding <- call becomes call(fn(binding) { rest... })
                if let Some(expr) = self.desugar_use(child, &children[i + 1..], block_span) {
                    items.push(Item {
                        kind: ItemKind::Expr(expr),
                        span: block_span,
                    });
                }
                // All remaining children are consumed by the use desugaring
                break;
            }

            match child.kind() {
                SyntaxKind::ITEM => {
                    if let Some(item) = self.lower_item(child) {
                        items.push(item);
                    }
                }
                SyntaxKind::EXPR_ITEM => {
                    if let Some(expr) = self.lower_first_expr(child) {
                        items.push(Item {
                            kind: ItemKind::Expr(expr),
                            span: self.node_span(child),
                        });
                    }
                }
                _ => {}
            }
            i += 1;
        }

        items
    }

    /// Check if an ITEM node contains a USE_DECL child.
    #[allow(clippy::unused_self)]
    fn item_contains_use(&self, node: &SyntaxNode) -> bool {
        node.children()
            .any(|child| child.kind() == SyntaxKind::USE_DECL)
    }

    /// Desugar a `use` declaration into a function call with callback.
    ///
    /// `use x <- doSomething(arg)` with remaining items becomes:
    /// `doSomething(arg, fn(x) { remaining items... })`
    fn desugar_use(
        &mut self,
        use_item: &SyntaxNode,
        remaining: &[SyntaxNode],
        block_span: Span,
    ) -> Option<Expr> {
        let use_node = use_item
            .children()
            .find(|c| c.kind() == SyntaxKind::USE_DECL)?;

        let use_span = self.node_span(&use_node);

        // Walk the tokens between `use` and `<-` once, gathering both the
        // shape (object destructure vs bare idents) and the identifiers.
        let mut found_arrow = false;
        let mut has_lbrace = false;
        let mut bindings: Vec<String> = Vec::new();
        for token in use_node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
        {
            match token.kind() {
                SyntaxKind::LEFT_ARROW => {
                    found_arrow = true;
                    break;
                }
                SyntaxKind::L_BRACE => has_lbrace = true,
                SyntaxKind::IDENT if !has_lbrace => {
                    bindings.push(token.text().to_string());
                }
                _ => {}
            }
        }

        if !found_arrow {
            return None;
        }

        let call_expr = self.lower_use_call_expr(&use_node)?;
        let body_items = self.lower_block_children(remaining, block_span);
        let body = self.expr(ExprKind::Block(body_items), block_span);

        let params: Vec<Param> = if has_lbrace {
            let fields = self.collect_object_destructure_fields(&use_node, false);
            let synthetic_name = format!(
                "_{}",
                fields
                    .iter()
                    .map(|f| f.bound_name())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            vec![Param {
                name: synthetic_name,
                type_ann: None,
                default: None,
                destructure: Some(ParamDestructure::Object(fields)),
                span: use_span,
            }]
        } else {
            bindings
                .into_iter()
                .map(|name| Param {
                    name,
                    type_ann: None,
                    default: None,
                    destructure: None,
                    span: use_span,
                })
                .collect()
        };

        let lambda = self.expr(
            ExprKind::Arrow {
                async_fn: false,
                params,
                body: Box::new(body),
            },
            use_span,
        );

        // Append the lambda as the last argument to the call
        Some(self.append_use_continuation(call_expr, lambda, use_span))
    }

    /// Thread the `use`-generated continuation into the RHS of `<-`.
    ///
    /// For pipes, the continuation must land on the piped-into function so the
    /// checker's lambda-param hints (derived from that function's signature)
    /// flow into the continuation. Wrapping the whole pipe as a callee —
    /// `(a |> f(x))(continuation)` — strands the continuation outside the
    /// hint-bearing call and the parameter types come back as `unknown`.
    fn append_use_continuation(&self, call_expr: Expr, lambda: Expr, use_span: Span) -> Expr {
        match call_expr.kind {
            ExprKind::Call {
                callee,
                type_args,
                mut args,
            } => {
                args.push(Arg::Positional(lambda));
                self.expr(
                    ExprKind::Call {
                        callee,
                        type_args,
                        args,
                    },
                    use_span,
                )
            }
            ExprKind::Pipe { left, right } => {
                let pipe_span = call_expr.span;
                let new_right = self.append_use_continuation(*right, lambda, use_span);
                self.expr(
                    ExprKind::Pipe {
                        left,
                        right: Box::new(new_right),
                    },
                    pipe_span,
                )
            }
            // Bare identifier / member / other: wrap it as a call with the lambda.
            _ => self.expr(
                ExprKind::Call {
                    callee: Box::new(call_expr),
                    type_args: Vec::new(),
                    args: vec![Arg::Positional(lambda)],
                },
                use_span,
            ),
        }
    }

    /// Lower the call expression from a USE_DECL node (everything after `<-`).
    fn lower_use_call_expr(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut after_arrow = false;
        for child_or_token in node.children_with_tokens() {
            if let Some(token) = child_or_token.as_token() {
                if token.kind() == SyntaxKind::LEFT_ARROW {
                    after_arrow = true;
                    continue;
                }
                if after_arrow && !token.kind().is_trivia() {
                    return self.token_to_expr(token);
                }
            } else if after_arrow && let Some(child) = child_or_token.into_node() {
                return self.lower_expr_node(&child);
            }
        }
        None
    }
}
