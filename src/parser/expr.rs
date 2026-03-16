use crate::lexer::token::TokenKind;

use super::ast::*;
use super::{ParseError, Parser};

impl Parser {
    // ── Expression Parsing (Pratt parser) ────────────────────────

    pub(super) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or_expr()
    }

    /// Logical OR: `a || b` (lowest precedence)
    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;

        while matches!(self.current_kind(), TokenKind::PipePipe) {
            self.advance();
            let right = self.parse_and_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: BinOp::Or,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Logical AND: `a && b`
    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;

        while matches!(self.current_kind(), TokenKind::AmpAmp) {
            self.advance();
            let right = self.parse_equality_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: BinOp::And,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Equality: `a == b`, `a != b`
    fn parse_equality_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pipe_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::EqualEqual => BinOp::Eq,
                TokenKind::BangEqual => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Pipe: `a |> f |> g` — binds tighter than `==` but looser than `<`/`>`
    fn parse_pipe_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_expr()?;

        while matches!(self.current_kind(), TokenKind::Pipe) {
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Pipe {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Comparison: `a < b`, `a > b`, `a <= b`, `a >= b`
    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::LessThan => BinOp::Lt,
                TokenKind::GreaterThan => BinOp::Gt,
                TokenKind::LessEqual => BinOp::LtEq,
                TokenKind::GreaterEqual => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Additive: `a + b`, `a - b`
    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Multiplicative: `a * b`, `a / b`, `a % b`
    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Unary: `!a`, `-a`
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_unary_expr_inner(true)
    }

    /// Unified unary parser. When `allow_unwrap` is false, postfix `?` is not
    /// consumed — used by `try` so that `try fetch()?` parses as `(try fetch())?`.
    fn parse_unary_expr_inner(&mut self, allow_unwrap: bool) -> Result<Expr, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            TokenKind::Bang => {
                self.advance();
                let operand = self.parse_unary_expr_inner(allow_unwrap)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span: self.merge_spans(start_span, end_span),
                })
            }
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary_expr_inner(allow_unwrap)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span: self.merge_spans(start_span, end_span),
                })
            }
            TokenKind::Await => {
                self.advance();
                let operand = self.parse_unary_expr_inner(allow_unwrap)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    span: self.merge_spans(start_span, end_span),
                })
            }
            TokenKind::Try => {
                self.advance();
                // Parse inner without consuming `?` — so `try fetch()?` parses as
                // `(try fetch())?`, not `try (fetch()?)`.
                let operand = self.parse_unary_expr_inner(false)?;
                let end_span = self.previous_span();
                let mut expr = Expr {
                    kind: ExprKind::Try(Box::new(operand)),
                    span: self.merge_spans(start_span, end_span),
                };
                // Now consume trailing `?` which applies to the try-wrapped Result
                if self.check(&TokenKind::Question) {
                    self.advance();
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Unwrap(Box::new(expr)),
                        span,
                    };
                }
                Ok(expr)
            }
            _ => self.parse_postfix_expr_inner(allow_unwrap),
        }
    }

    /// Postfix: `expr?`, `expr.field`, `expr[index]`, `expr(args)`
    /// When `allow_unwrap` is false, `?` is not consumed (used by `try`).
    fn parse_postfix_expr_inner(&mut self, allow_unwrap: bool) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            match self.current_kind() {
                // Unwrap: `expr?`
                TokenKind::Question => {
                    if !allow_unwrap {
                        break;
                    }
                    self.advance();
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Unwrap(Box::new(expr)),
                        span,
                    };
                }
                // Member access: `expr.field`
                // Banned keywords are allowed as field names (e.g. Array.any)
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_identifier_or_keyword()?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Member {
                            object: Box::new(expr),
                            field,
                        },
                        span,
                    };
                }
                // Index: `expr[index]`
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&TokenKind::RightBracket)?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Index {
                            object: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                // Generic call: `f<T>(args)` — type arguments before call
                TokenKind::LessThan
                    if allow_unwrap
                        && matches!(
                            &expr.kind,
                            ExprKind::Identifier(_) | ExprKind::Member { .. }
                        )
                        && self.is_generic_call() =>
                {
                    self.advance(); // consume `<`
                    let type_args = self.parse_comma_separated(|p| p.parse_type_expr())?;
                    self.expect(&TokenKind::GreaterThan)?;
                    self.expect(&TokenKind::LeftParen)?;
                    let args = self.parse_call_args()?;
                    self.expect(&TokenKind::RightParen)?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            type_args,
                            args,
                        },
                        span,
                    };
                }
                // Call: `expr(args)` — but only for lowercase identifiers or member exprs
                // Uppercase identifiers with `(` are constructors, handled in primary
                TokenKind::LeftParen => {
                    // If the callee is an uppercase identifier, it was already
                    // parsed as a Construct in primary. Only parse calls for
                    // lowercase identifiers, members, etc.
                    if matches!(&expr.kind, ExprKind::Identifier(name) if name.starts_with(char::is_uppercase))
                    {
                        break;
                    }
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(&TokenKind::RightParen)?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            type_args: Vec::new(),
                            args,
                        },
                        span,
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Primary expressions: literals, identifiers, constructors, match, etc.
    pub(super) fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            // Number literal
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Number(n),
                    span: start_span,
                })
            }

            // String literal
            TokenKind::String(s) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::String(s),
                    span: start_span,
                })
            }

            // Template literal
            TokenKind::TemplateLiteral(parts) => {
                let ast_parts = self.convert_template_parts(parts)?;
                self.advance();
                Ok(Expr {
                    kind: ExprKind::TemplateLiteral(ast_parts),
                    span: start_span,
                })
            }

            // Boolean literal
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(b),
                    span: start_span,
                })
            }

            // Placeholder
            TokenKind::Underscore => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Placeholder,
                    span: start_span,
                })
            }

            // None
            TokenKind::None => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::None,
                    span: start_span,
                })
            }

            // Todo
            TokenKind::Todo => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Todo,
                    span: start_span,
                })
            }

            // Unreachable
            TokenKind::Unreachable => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Unreachable,
                    span: start_span,
                })
            }

            // Ok(expr)
            TokenKind::Ok => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Ok(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Err(expr)
            TokenKind::Err => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Err(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Some(expr)
            TokenKind::Some => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Some(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Match expression
            TokenKind::Match => self.parse_match_expr(),

            // Return
            TokenKind::Return => {
                self.advance();
                let value = if self.is_at_end()
                    || self.check(&TokenKind::RightBrace)
                    || self.check(&TokenKind::Semicolon)
                {
                    Option::None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Return(value),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Object literal `{ key: value }` or block expression `{ ... }`
            TokenKind::LeftBrace => {
                if self.is_object_literal() {
                    self.parse_object_literal()
                } else {
                    self.parse_block_expr()
                }
            }

            // Array literal: `[1, 2, 3]`
            TokenKind::LeftBracket => {
                self.advance();
                let elements = self.parse_comma_separated(|p| p.parse_expr())?;
                self.expect(&TokenKind::RightBracket)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Array(elements),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Parenthesized expression, unit value (), or tuple (a, b)
            TokenKind::LeftParen => {
                if self.peek_kind() == Some(&TokenKind::RightParen) {
                    // Unit value: ()
                    self.advance(); // (
                    self.advance(); // )
                    let end_span = self.previous_span();
                    Ok(Expr {
                        kind: ExprKind::Unit,
                        span: self.merge_spans(start_span, end_span),
                    })
                } else {
                    self.advance();
                    let first = self.parse_expr()?;
                    if self.check(&TokenKind::Comma) {
                        // Tuple: (expr, expr, ...)
                        self.advance(); // consume first comma
                        let mut elements = vec![first];
                        // Allow trailing comma after first element
                        if !self.check(&TokenKind::RightParen) {
                            elements.push(self.parse_expr()?);
                            while self.check(&TokenKind::Comma) {
                                self.advance();
                                if self.check(&TokenKind::RightParen) {
                                    break; // trailing comma
                                }
                                elements.push(self.parse_expr()?);
                            }
                        }
                        self.expect(&TokenKind::RightParen)?;
                        let end_span = self.previous_span();
                        Ok(Expr {
                            kind: ExprKind::Tuple(elements),
                            span: self.merge_spans(start_span, end_span),
                        })
                    } else {
                        // Grouped expression: (expr)
                        self.expect(&TokenKind::RightParen)?;
                        let end_span = self.previous_span();
                        Ok(Expr {
                            kind: ExprKind::Grouped(Box::new(first)),
                            span: self.merge_spans(start_span, end_span),
                        })
                    }
                }
            }

            // Dot shorthand: `.field` or `.field op expr`
            TokenKind::Dot => self.parse_dot_shorthand(),

            // Async lambda: `async || body` or `async |params| body`
            TokenKind::Async
                if matches!(
                    self.peek_kind(),
                    Some(TokenKind::VerticalBar | TokenKind::PipePipe)
                ) =>
            {
                self.advance(); // consume `async`
                if self.check(&TokenKind::PipePipe) {
                    self.advance(); // consume `||`
                    let body = self.parse_expr()?;
                    let end_span = self.previous_span();
                    Ok(Expr {
                        kind: ExprKind::Arrow {
                            async_fn: true,
                            params: vec![],
                            body: Box::new(body),
                        },
                        span: self.merge_spans(start_span, end_span),
                    })
                } else {
                    self.expect(&TokenKind::VerticalBar)?;
                    let params = self.parse_lambda_params()?;
                    self.expect(&TokenKind::VerticalBar)?;
                    let body = self.parse_expr()?;
                    let end_span = self.previous_span();
                    Ok(Expr {
                        kind: ExprKind::Arrow {
                            async_fn: true,
                            params,
                            body: Box::new(body),
                        },
                        span: self.merge_spans(start_span, end_span),
                    })
                }
            }

            // Pipe lambda: `|params| body` or `|| body` (zero-arg)
            TokenKind::VerticalBar => self.parse_pipe_lambda(),

            // Zero-arg lambda: `|| expr`
            TokenKind::PipePipe => {
                self.advance();
                let body = self.parse_expr()?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Arrow {
                        async_fn: false,
                        params: vec![],
                        body: Box::new(body),
                    },
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // JSX: `<Component ...>` or `<>`
            TokenKind::LessThan => self.parse_jsx_expr(),

            // Identifier — could be a constructor (uppercase) or variable (lowercase)
            TokenKind::Identifier(name) => {
                // Uppercase identifier followed by `(` is a constructor
                if name.starts_with(char::is_uppercase)
                    && self.peek_kind() == Some(&TokenKind::LeftParen)
                {
                    return self.parse_construct_expr();
                }

                // Qualified variant: `Filter.All` or `Route.Profile(id: "123")`
                // Uppercase.Uppercase pattern
                if name.starts_with(char::is_uppercase)
                    && self.peek_kind() == Some(&TokenKind::Dot)
                    && let Some(after_dot) = self.peek_nth_kind(2)
                    && matches!(after_dot, TokenKind::Identifier(n) if n.starts_with(char::is_uppercase))
                {
                    self.advance(); // consume type name
                    self.advance(); // consume '.'
                    let variant_name = self.expect_identifier()?;

                    // If followed by `(`, it's a qualified constructor: Route.Profile(id: "123")
                    if self.check(&TokenKind::LeftParen) {
                        self.advance(); // consume '('

                        // Check for spread: `..expr`
                        let spread = if self.check(&TokenKind::DotDot) {
                            self.advance();
                            let spread_expr = self.parse_expr()?;
                            if self.check(&TokenKind::Comma) {
                                self.advance();
                            }
                            Some(Box::new(spread_expr))
                        } else {
                            Option::None
                        };

                        let args = if !self.check(&TokenKind::RightParen) {
                            self.parse_call_args()?
                        } else {
                            Vec::new()
                        };

                        self.expect(&TokenKind::RightParen)?;
                        let end_span = self.previous_span();

                        return Ok(Expr {
                            kind: ExprKind::Construct {
                                type_name: variant_name,
                                spread,
                                args,
                            },
                            span: self.merge_spans(start_span, end_span),
                        });
                    }

                    // Unit variant: Filter.All → Identifier("All")
                    let end_span = self.previous_span();
                    return Ok(Expr {
                        kind: ExprKind::Identifier(variant_name),
                        span: self.merge_spans(start_span, end_span),
                    });
                }

                self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier(name),
                    span: start_span,
                })
            }

            // `self` keyword — treat as identifier in expression context
            TokenKind::SelfKw => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier("self".to_string()),
                    span: start_span,
                })
            }

            // Banned keyword — report it
            TokenKind::Banned(banned) => {
                let msg = format!(
                    "banned keyword '{}': {}",
                    banned.as_str(),
                    banned.help_message()
                );
                Err(self.error_with_kind(&msg, super::ParseErrorKind::BannedKeyword))
            }

            _ => Err(self.error(&format!("unexpected token: {:?}", self.current_kind()))),
        }
    }

    // ── Pipe Lambda ─────────────────────────────────────────────

    /// Parse `|params| body` lambda expression.
    fn parse_pipe_lambda(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::VerticalBar)?;

        let params = self.parse_lambda_params()?;

        self.expect(&TokenKind::VerticalBar)?;

        let body = self.parse_expr()?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Arrow {
                async_fn: false,
                params,
                body: Box::new(body),
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    /// Parse comma-separated params terminated by `|`.
    /// Supports destructuring patterns: `|{ x, y }| ...`
    fn parse_lambda_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();

        if self.check(&TokenKind::VerticalBar) {
            return Ok(params);
        }

        params.push(self.parse_lambda_param()?);

        while self.check(&TokenKind::Comma) {
            self.advance();
            if self.check(&TokenKind::VerticalBar) {
                break;
            }
            params.push(self.parse_lambda_param()?);
        }

        Ok(params)
    }

    /// Parse a single lambda parameter, which can be a plain identifier or a destructuring pattern.
    fn parse_lambda_param(&mut self) -> Result<Param, ParseError> {
        self.parse_param_in_context(super::ParamContext::Lambda)
    }

    // ── Dot Shorthand ────────────────────────────────────────────

    /// Parse `.field` or `.field op expr` dot shorthand expression.
    fn parse_dot_shorthand(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::Dot)?;
        let field = self.expect_identifier()?;

        // Check for optional binary operator predicate
        let predicate = match self.current_kind() {
            TokenKind::EqualEqual => Some(BinOp::Eq),
            TokenKind::BangEqual => Some(BinOp::NotEq),
            TokenKind::LessThan => Some(BinOp::Lt),
            TokenKind::GreaterThan => Some(BinOp::Gt),
            TokenKind::LessEqual => Some(BinOp::LtEq),
            TokenKind::GreaterEqual => Some(BinOp::GtEq),
            TokenKind::AmpAmp => Some(BinOp::And),
            TokenKind::PipePipe => Some(BinOp::Or),
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            _ => Option::None,
        };

        let predicate = if let Some(op) = predicate {
            self.advance(); // consume the operator
            let rhs = self.parse_primary_expr()?;
            Some((op, Box::new(rhs)))
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(Expr {
            kind: ExprKind::DotShorthand { field, predicate },
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Constructors ─────────────────────────────────────────────

    fn parse_construct_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        let type_name = self.expect_identifier()?;
        self.expect(&TokenKind::LeftParen)?;

        // Check for spread: `..expr`
        let spread = if self.check(&TokenKind::DotDot) {
            self.advance();
            let spread_expr = self.parse_expr()?;
            // Expect comma after spread if there are more args
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            Some(Box::new(spread_expr))
        } else {
            Option::None
        };

        let args = if !self.check(&TokenKind::RightParen) {
            self.parse_call_args()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::RightParen)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Construct {
                type_name,
                spread,
                args,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Call Arguments ───────────────────────────────────────────

    fn parse_call_args(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.parse_comma_separated(|p| p.parse_call_arg())
    }

    fn parse_call_arg(&mut self) -> Result<Arg, ParseError> {
        // Check for named argument: `label: expr` or punned `label:`
        if self.is_identifier() && self.peek_kind() == Some(&TokenKind::Colon) {
            let span = self.current_span();
            let label = self.expect_identifier()?;
            self.advance(); // consume ':'

            // Punning: `label:` without a value (next token is `)` or `,`)
            if matches!(
                self.current_kind(),
                TokenKind::RightParen | TokenKind::Comma
            ) {
                let value = Expr {
                    kind: ExprKind::Identifier(label.clone()),
                    span,
                };
                return Ok(Arg::Named { label, value });
            }

            let value = self.parse_expr()?;
            return Ok(Arg::Named { label, value });
        }

        let expr = self.parse_expr()?;
        Ok(Arg::Positional(expr))
    }

    // ── Object Literals ──────────────────────────────────────────

    /// Check if `{ ... }` is an object literal rather than a block.
    /// Lookahead: `{ identifier : ...` or `{ identifier , ...` or `{ identifier }`
    fn is_object_literal(&self) -> bool {
        // Current token is `{`, peek at what follows
        match self.peek_kind() {
            // `{ }` — empty object
            Some(TokenKind::RightBrace) => true,
            // `{ identifier ... }`
            Some(TokenKind::Identifier(_)) => {
                matches!(
                    self.peek_nth_kind(2),
                    Some(TokenKind::Colon | TokenKind::Comma | TokenKind::RightBrace)
                )
            }
            _ => false,
        }
    }

    /// Parse an object literal: `{ key: value, key2: value2 }` or `{ key }` (shorthand)
    fn parse_object_literal(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;

        let mut fields = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let key = self.expect_identifier()?;

            let value = if self.check(&TokenKind::Colon) {
                self.advance(); // consume `:`
                self.parse_expr()?
            } else {
                // Shorthand: `{ name }` means `{ name: name }`
                Expr {
                    kind: ExprKind::Identifier(key.clone()),
                    span: self.previous_span(),
                }
            };

            fields.push((key, value));

            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume `,`
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Object(fields),
            span: self.merge_spans(start_span, end_span),
        })
    }
}
