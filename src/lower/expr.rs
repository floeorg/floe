use super::*;

impl<'src> Lowerer<'src> {
    pub(super) fn lower_expr_node(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let span = self.node_span(node);

        match node.kind() {
            SyntaxKind::PIPE_EXPR => {
                let exprs = self.lower_child_exprs(node);
                if exprs.len() >= 2 {
                    let mut iter = exprs.into_iter();
                    let left = iter.next()?;
                    let right = iter.next()?;
                    Some(Expr {
                        span: self.node_span(node),
                        kind: ExprKind::Pipe {
                            left: Box::new(left),
                            right: Box::new(right),
                        },
                    })
                } else {
                    exprs.into_iter().next()
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
                let idents = self.collect_idents(node);
                let field = idents.last()?.clone();
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

            SyntaxKind::IF_EXPR => {
                let mut exprs = Vec::new();
                for child in node.children() {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        exprs.push(expr);
                    }
                }

                // Also collect token-level expressions
                if exprs.is_empty()
                    && let Some(expr) = self.lower_token_expr(node)
                {
                    exprs.push(expr);
                }

                let condition = exprs.first().cloned()?;
                let then_branch = exprs.get(1).cloned()?;
                let else_branch = exprs.get(2).cloned().map(Box::new);

                Some(Expr {
                    span,
                    kind: ExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                })
            }

            SyntaxKind::BLOCK_EXPR => {
                let mut items = Vec::new();
                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::ITEM => {
                            if let Some(item) = self.lower_item(&child) {
                                items.push(item);
                            }
                        }
                        SyntaxKind::EXPR_ITEM => {
                            if let Some(expr) = self.lower_first_expr(&child) {
                                items.push(Item {
                                    kind: ItemKind::Expr(expr),
                                    span: self.node_span(&child),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Some(Expr {
                    span,
                    kind: ExprKind::Block(items),
                })
            }

            SyntaxKind::RETURN_EXPR => {
                let value = self.lower_child_exprs(node).into_iter().next();
                if value.is_none() {
                    // Try token expr
                    let tok_expr = self.lower_token_expr(node);
                    return Some(Expr {
                        span,
                        kind: ExprKind::Return(tok_expr.map(Box::new)),
                    });
                }
                Some(Expr {
                    span,
                    kind: ExprKind::Return(value.map(Box::new)),
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

            SyntaxKind::ARRAY_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                Some(Expr {
                    span,
                    kind: ExprKind::Array(elements),
                })
            }

            SyntaxKind::TUPLE_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                Some(Expr {
                    span,
                    kind: ExprKind::Tuple(elements),
                })
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
                    // Find the RHS expression (child node or token after the operator)
                    let rhs = self.lower_first_expr(node)?;
                    Some((op, Box::new(rhs)))
                });

                Some(Expr {
                    span,
                    kind: ExprKind::DotShorthand { field, predicate },
                })
            }

            SyntaxKind::ERROR => None,

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
        let mut exprs = Vec::new();
        let mut found_first_token_expr = false;

        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                exprs.push(expr);
            }
        }

        // If no child expr nodes, try token exprs
        if exprs.is_empty() {
            for token in node.children_with_tokens() {
                if let Some(token) = token.as_token()
                    && let Some(expr) = self.token_to_expr(token)
                    && (!found_first_token_expr || token.kind() != SyntaxKind::IDENT)
                {
                    exprs.push(expr);
                    found_first_token_expr = true;
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

    pub(super) fn lower_token_expr_after_lambda_delim(
        &mut self,
        node: &SyntaxNode,
    ) -> Option<Expr> {
        // For `|params| body` lambdas, find token expr after the second `|`.
        // For `|| body` zero-arg lambdas, find token expr after `||`.
        let mut pipe_count = 0;
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::VERT_BAR {
                    pipe_count += 1;
                    continue;
                }
                if token.kind() == SyntaxKind::PIPE_PIPE {
                    pipe_count = 2;
                    continue;
                }
                if pipe_count >= 2
                    && let Some(expr) = self.token_to_expr(token)
                {
                    return Some(expr);
                }
            }
        }
        None
    }

    pub(super) fn token_to_expr(&self, token: &rowan::SyntaxToken<ZenLang>) -> Option<Expr> {
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
                // Template literals are complex — for now, store as raw
                // The lowering for interpolations needs the original token parts
                // We'll handle this separately
                Some(Expr {
                    kind: ExprKind::TemplateLiteral(vec![TemplatePart::Raw(
                        text[1..text.len().saturating_sub(1)].to_string(),
                    )]),
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
            let value = self.lower_first_expr(node)?;
            Some(Arg::Named { label, value })
        } else {
            let expr = self.lower_first_expr(node)?;
            Some(Arg::Positional(expr))
        }
    }
}
