use super::*;

impl<'src> Lowerer<'src> {
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
                                if token.kind() != SyntaxKind::PIPE
                                    && let Some(expr) = self.token_to_expr(token)
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

                    Some(Expr {
                        span: self.node_span(node),
                        kind: ExprKind::Match {
                            subject: Box::new(left),
                            arms,
                        },
                    })
                } else {
                    let exprs = self.lower_child_exprs(node);
                    if exprs.len() >= 2 {
                        let mut iter = exprs.into_iter();
                        let left = iter.next()?;
                        let right = iter.next()?;

                        // `a |> f?` restructure: lift `?` above the pipe → `(a |> f)?`
                        if let ExprKind::Unwrap(inner) = right.kind {
                            let pipe_span = self.node_span(node);
                            Some(Expr {
                                span: pipe_span,
                                kind: ExprKind::Unwrap(Box::new(Expr {
                                    span: pipe_span,
                                    kind: ExprKind::Pipe {
                                        left: Box::new(left),
                                        right: inner,
                                    },
                                })),
                            })
                        } else {
                            Some(Expr {
                                span: self.node_span(node),
                                kind: ExprKind::Pipe {
                                    left: Box::new(left),
                                    right: Box::new(right),
                                },
                            })
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
                    Some(Expr {
                        span,
                        kind: ExprKind::Binary {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        },
                    })
                } else {
                    None
                }
            }

            SyntaxKind::UNARY_EXPR => {
                let op = self.find_unary_op(node)?;
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                })
            }

            SyntaxKind::TRY_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Try(Box::new(operand)),
                })
            }

            SyntaxKind::AWAIT_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Await(Box::new(operand)),
                })
            }

            SyntaxKind::UNWRAP_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Unwrap(Box::new(operand)),
                })
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
                Some(Expr {
                    span,
                    kind: ExprKind::Member {
                        object: Box::new(object),
                        field,
                    },
                })
            }

            SyntaxKind::INDEX_EXPR => {
                let exprs = self.lower_child_exprs(node);
                let mut iter = exprs.into_iter();
                let object = iter.next()?;
                let index = iter.next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                })
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

                Some(Expr {
                    span,
                    kind: ExprKind::Call {
                        callee: Box::new(callee),
                        type_args,
                        args,
                    },
                })
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

                Some(Expr {
                    span,
                    kind: ExprKind::Construct {
                        type_name,
                        spread,
                        args,
                    },
                })
            }

            SyntaxKind::ARROW_EXPR => {
                let mut params = Vec::new();
                let mut body = None;
                let async_fn = self.has_keyword(node, SyntaxKind::KW_ASYNC);

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

                Some(Expr {
                    span,
                    kind: ExprKind::Arrow {
                        async_fn,
                        params,
                        body: Box::new(body?),
                    },
                })
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

                Some(Expr {
                    span,
                    kind: ExprKind::Match {
                        subject: Box::new(subject?),
                        arms,
                    },
                })
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
                Some(Expr {
                    span,
                    kind: ExprKind::Collect(items),
                })
            }

            SyntaxKind::BLOCK_EXPR => {
                // Collect all child nodes first so we can look ahead for `use`
                let children: Vec<_> = node.children().collect();
                let items = self.lower_block_children(&children, span);
                Some(Expr {
                    span,
                    kind: ExprKind::Block(items),
                })
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
                    .unwrap_or(Expr {
                        span,
                        kind: ExprKind::Placeholder,
                    });

                Some(Expr {
                    span,
                    kind: ExprKind::Parse {
                        type_arg,
                        value: Box::new(value),
                    },
                })
            }

            SyntaxKind::OK_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Ok(Box::new(inner)),
                })
            }

            SyntaxKind::ERR_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Err(Box::new(inner)),
                })
            }

            SyntaxKind::SOME_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Some(Box::new(inner)),
                })
            }

            SyntaxKind::GROUPED_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Grouped(Box::new(inner)),
                })
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
                                    Expr {
                                        span: self.node_span(&child),
                                        kind: ExprKind::Identifier(key.clone()),
                                    }
                                });
                            fields.push((key.clone(), value));
                        }
                    }
                }
                Some(Expr {
                    span,
                    kind: ExprKind::Object(fields),
                })
            }

            SyntaxKind::ARRAY_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                Some(Expr {
                    span,
                    kind: ExprKind::Array(elements),
                })
            }

            SyntaxKind::TUPLE_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                if elements.is_empty() {
                    // Empty tuple: () → Unit
                    Some(Expr {
                        span,
                        kind: ExprKind::Unit,
                    })
                } else {
                    Some(Expr {
                        span,
                        kind: ExprKind::Tuple(elements),
                    })
                }
            }

            SyntaxKind::JSX_ELEMENT => {
                let element = self.lower_jsx_element(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Jsx(element),
                })
            }

            SyntaxKind::SPREAD_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Spread(Box::new(inner)),
                })
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

                Some(Expr {
                    span,
                    kind: ExprKind::DotShorthand { field, predicate },
                })
            }

            SyntaxKind::ERROR => None,

            // Type expressions, patterns, and other non-expression nodes
            SyntaxKind::TYPE_EXPR
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
            | SyntaxKind::DERIVING_CLAUSE
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
        // For `fn(params) body` lambdas, find token expr after the closing `)`.
        let mut found_rparen = false;
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::R_PAREN {
                    found_rparen = true;
                    continue;
                }
                if found_rparen && let Some(expr) = self.token_to_expr(token) {
                    return Some(expr);
                }
            }
        }
        None
    }

    pub(super) fn token_to_expr(&self, token: &rowan::SyntaxToken<FloeLang>) -> Option<Expr> {
        let span = self.token_span(token);
        let text = token.text();

        match token.kind() {
            SyntaxKind::NUMBER => Some(Expr {
                kind: ExprKind::Number(text.to_string()),
                span,
            }),
            SyntaxKind::STRING => Some(Expr {
                kind: ExprKind::String(self.unquote_string(text)),
                span,
            }),
            SyntaxKind::TEMPLATE_LITERAL => {
                let parts = self.lower_template_literal(text);
                Some(Expr {
                    kind: ExprKind::TemplateLiteral(parts),
                    span,
                })
            }
            SyntaxKind::BOOL => Some(Expr {
                kind: ExprKind::Bool(text == "true"),
                span,
            }),
            SyntaxKind::IDENT => Some(Expr {
                kind: ExprKind::Identifier(text.to_string()),
                span,
            }),
            SyntaxKind::UNDERSCORE => Some(Expr {
                kind: ExprKind::Placeholder,
                span,
            }),
            SyntaxKind::KW_NONE => Some(Expr {
                kind: ExprKind::None,
                span,
            }),
            SyntaxKind::KW_TODO => Some(Expr {
                kind: ExprKind::Todo,
                span,
            }),
            SyntaxKind::KW_UNREACHABLE => Some(Expr {
                kind: ExprKind::Unreachable,
                span,
            }),
            SyntaxKind::KW_SELF => Some(Expr {
                kind: ExprKind::Identifier("self".to_string()),
                span,
            }),
            _ => None,
        }
    }

    pub(super) fn lower_arg(&mut self, node: &SyntaxNode) -> Option<Arg> {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let idents = self.collect_idents_direct(node);
            let label = idents.first()?.clone();
            // Find the expression after the colon
            let value = self.lower_expr_after_colon(node).unwrap_or_else(|| {
                // Punning: `label:` without value → `label: label`
                Expr {
                    kind: ExprKind::Identifier(label.clone()),
                    span: self.node_span(node),
                }
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

        // Extract binding names (identifiers before `<-`)
        let mut bindings: Vec<String> = Vec::new();
        let mut found_arrow = false;
        for token in use_node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::LEFT_ARROW => {
                        found_arrow = true;
                        break;
                    }
                    SyntaxKind::IDENT => {
                        bindings.push(token.text().to_string());
                    }
                    _ => {}
                }
            }
        }

        if !found_arrow {
            return None;
        }

        // Extract the call expression (everything after `<-`)
        let call_expr = self.lower_use_call_expr(&use_node)?;

        // Build the callback body from remaining items
        let body_items = self.lower_block_children(remaining, block_span);
        let body = Expr {
            span: block_span,
            kind: ExprKind::Block(body_items),
        };

        // Build lambda params from bindings
        let params: Vec<Param> = bindings
            .into_iter()
            .map(|name| Param {
                name,
                type_ann: None,
                default: None,
                destructure: None,
                span: use_span,
            })
            .collect();

        let lambda = Expr {
            span: use_span,
            kind: ExprKind::Arrow {
                async_fn: false,
                params,
                body: Box::new(body),
            },
        };

        // Append the lambda as the last argument to the call
        match call_expr.kind {
            ExprKind::Call {
                callee,
                type_args,
                mut args,
            } => {
                args.push(Arg::Positional(lambda));
                Some(Expr {
                    span: use_span,
                    kind: ExprKind::Call {
                        callee,
                        type_args,
                        args,
                    },
                })
            }
            // If it's not a call (e.g. just an identifier), wrap it as a call with the lambda
            _ => Some(Expr {
                span: use_span,
                kind: ExprKind::Call {
                    callee: Box::new(call_expr),
                    type_args: Vec::new(),
                    args: vec![Arg::Positional(lambda)],
                },
            }),
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
