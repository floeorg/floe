use super::*;

impl<'src> CstParser<'src> {
    // ── Expressions ─────────────────────────────────────────────

    pub(super) fn parse_expr(&mut self) {
        self.parse_or_expr();
    }

    fn parse_or_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_and_expr();

        while self.at(TokenKind::PipePipe) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_and_expr();
            self.builder.finish_node();
        }
    }

    fn parse_and_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_equality_expr();

        while self.at(TokenKind::AmpAmp) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_equality_expr();
            self.builder.finish_node();
        }
    }

    fn parse_equality_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_pipe_expr();

        while self.at(TokenKind::EqualEqual) || self.at(TokenKind::BangEqual) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_pipe_expr();
            self.builder.finish_node();
        }
    }

    fn parse_pipe_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_comparison_expr();

        while self.at(TokenKind::Pipe) || self.at(TokenKind::PipeUnwrap) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::PIPE_EXPR.into());
            self.bump(); // |> or |>?
            self.eat_trivia();

            // Pipe into match: `x |> match { ... }`
            if self.at(TokenKind::Match) {
                self.parse_subjectless_match_expr();
            } else {
                self.parse_comparison_expr();
            }

            self.builder.finish_node();
        }
    }

    fn parse_comparison_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_additive_expr();

        while (self.at(TokenKind::LessThan)
            || self.at(TokenKind::GreaterThan)
            || self.at(TokenKind::LessEqual)
            || self.at(TokenKind::GreaterEqual))
            && !self.preceded_by_newline()
        {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_additive_expr();
            self.builder.finish_node();
        }
    }

    fn parse_additive_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_multiplicative_expr();

        while self.at(TokenKind::Plus) || self.at(TokenKind::Minus) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_multiplicative_expr();
            self.builder.finish_node();
        }
    }

    fn parse_multiplicative_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_unary_expr();

        while self.at(TokenKind::Star) || self.at(TokenKind::Slash) || self.at(TokenKind::Percent) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_unary_expr();
            self.builder.finish_node();
        }
    }

    fn parse_unary_expr(&mut self) {
        match self.current_kind() {
            Some(TokenKind::Bang) | Some(TokenKind::Minus) => {
                self.builder.start_node(SyntaxKind::UNARY_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.parse_unary_expr();
                self.builder.finish_node();
            }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_postfix_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_primary_expr();

        loop {
            self.eat_trivia();
            match self.current_kind() {
                Some(TokenKind::Question) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::UNWRAP_EXPR.into());
                    self.bump();
                    self.builder.finish_node();
                }
                Some(TokenKind::Dot) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::MEMBER_EXPR.into());
                    self.bump();
                    self.eat_trivia();
                    // Accept identifiers, keywords, and numbers after `.`
                    // (must match SyntaxKind::is_member_name)
                    if self.is_ident()
                        || matches!(
                            self.current_kind(),
                            Some(TokenKind::Number(_))
                                | Some(TokenKind::Banned(_))
                                | Some(TokenKind::Parse)
                                | Some(TokenKind::Match)
                                | Some(TokenKind::For)
                                | Some(TokenKind::From)
                                | Some(TokenKind::Type)
                                | Some(TokenKind::Export)
                                | Some(TokenKind::Import)
                                | Some(TokenKind::Const)
                                | Some(TokenKind::Fn)
                                | Some(TokenKind::Trait)
                                | Some(TokenKind::Collect)
                                | Some(TokenKind::Deriving)
                                | Some(TokenKind::When)
                                | Some(TokenKind::SelfKw)
                                | Some(TokenKind::Value)
                                | Some(TokenKind::Clear)
                                | Some(TokenKind::Unchanged)
                                | Some(TokenKind::Todo)
                                | Some(TokenKind::Unreachable)
                                | Some(TokenKind::Mock)
                                | Some(TokenKind::Assert)
                                | Some(TokenKind::Use)
                                | Some(TokenKind::Typeof)
                                | Some(TokenKind::Opaque)
                                | Some(TokenKind::Trusted)
                        )
                    {
                        self.bump();
                    } else {
                        self.expect_ident();
                    }
                    self.builder.finish_node();
                }
                Some(TokenKind::LeftBracket) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::INDEX_EXPR.into());
                    self.bump();
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightBracket);
                    self.builder.finish_node();
                }
                Some(TokenKind::LessThan) if self.is_generic_call() => {
                    // Generic call: `f<T>(args)` or `f<T, U>(args)`
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::CALL_EXPR.into());
                    self.bump(); // <
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_type_expr, TokenKind::GreaterThan);
                    self.expect(TokenKind::GreaterThan);
                    self.eat_trivia();
                    self.expect(TokenKind::LeftParen);
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                }
                Some(TokenKind::LeftParen) => {
                    // Don't treat `(` on a new line as a call — it's a new expression
                    if self.preceded_by_newline() {
                        break;
                    }
                    // Check if it's a constructor (uppercase ident) — don't parse as call
                    if self.is_uppercase_ident_at_checkpoint() {
                        break;
                    }
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::CALL_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                }
                _ => break,
            }
        }
    }

    fn parse_primary_expr(&mut self) {
        match self.current_kind() {
            Some(TokenKind::Number(_)) => self.bump(),
            Some(TokenKind::String(_)) => self.bump(),
            Some(TokenKind::TemplateLiteral(_)) => self.bump(),
            Some(TokenKind::Bool(_)) => self.bump(),
            Some(TokenKind::Underscore) => self.bump(),
            Some(TokenKind::Clear) => self.bump(),
            Some(TokenKind::Unchanged) => self.bump(),
            Some(TokenKind::Todo) => self.bump(),
            Some(TokenKind::Unreachable) => self.bump(),

            Some(TokenKind::Value) => {
                self.builder.start_node(SyntaxKind::VALUE_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.expect(TokenKind::LeftParen);
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightParen);
                self.builder.finish_node();
            }

            Some(TokenKind::Parse) => {
                self.builder.start_node(SyntaxKind::PARSE_EXPR.into());
                self.bump(); // parse
                self.eat_trivia();
                // parse<T> — type argument
                self.expect(TokenKind::LessThan);
                self.eat_trivia();
                self.parse_type_expr();
                self.eat_trivia();
                self.expect(TokenKind::GreaterThan);
                self.eat_trivia();
                // Optional (value) — may be absent in pipe context
                if self.current_kind() == Some(TokenKind::LeftParen) {
                    self.bump();
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightParen);
                }
                self.builder.finish_node();
            }

            Some(TokenKind::Mock) => {
                self.builder.start_node(SyntaxKind::MOCK_EXPR.into());
                self.bump(); // mock
                self.eat_trivia();
                // mock<T> — type argument
                self.expect(TokenKind::LessThan);
                self.eat_trivia();
                self.parse_type_expr();
                self.eat_trivia();
                self.expect(TokenKind::GreaterThan);
                self.eat_trivia();
                // Optional (overrides) — named args for field overrides
                if self.current_kind() == Some(TokenKind::LeftParen) {
                    self.bump();
                    self.eat_trivia();
                    while self.current_kind() != Some(TokenKind::RightParen)
                        && self.current_kind().is_some()
                    {
                        self.parse_call_arg();
                        self.eat_trivia();
                        if self.current_kind() == Some(TokenKind::Comma) {
                            self.bump();
                            self.eat_trivia();
                        }
                    }
                    self.expect(TokenKind::RightParen);
                }
                self.builder.finish_node();
            }

            Some(TokenKind::Match) => self.parse_match_expr(),
            Some(TokenKind::Collect) => {
                self.builder.start_node(SyntaxKind::COLLECT_EXPR.into());
                self.bump(); // collect
                self.eat_trivia();
                self.parse_block_expr();
                self.builder.finish_node();
            }
            Some(TokenKind::LeftBrace) => {
                if self.is_object_literal() {
                    self.parse_object_literal();
                } else {
                    self.parse_block_expr();
                }
            }

            Some(TokenKind::LeftBracket) => {
                self.builder.start_node(SyntaxKind::ARRAY_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_expr, TokenKind::RightBracket);
                self.expect(TokenKind::RightBracket);
                self.builder.finish_node();
            }

            Some(TokenKind::LeftParen) => {
                if self.is_arrow_expr() {
                    // Arrow closure: `(params) => body`
                    self.parse_arrow_closure();
                } else if self.peek_is(TokenKind::RightParen) {
                    // Unit value: ()
                    self.builder.start_node(SyntaxKind::TUPLE_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.bump(); // )
                    self.builder.finish_node();
                } else if self.is_paren_tuple_expr() {
                    // Tuple: (expr, expr, ...)
                    self.builder.start_node(SyntaxKind::TUPLE_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_expr, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                } else {
                    self.builder.start_node(SyntaxKind::GROUPED_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                }
            }

            Some(TokenKind::LessThan) => self.parse_jsx_element(),

            Some(TokenKind::Dot) => {
                self.parse_dot_shorthand();
            }

            Some(TokenKind::Fn) if self.peek_is(TokenKind::LeftParen) => {
                // `fn(params) expr` is the old syntax — emit error pointing to =>
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.error("anonymous functions use arrow syntax: `(params) => body` instead of `fn(params) body`");
                self.bump(); // fn
                self.builder.finish_node();
            }

            // `self` keyword — treat as identifier in expression context
            Some(TokenKind::SelfKw) => {
                self.bump();
            }

            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();

                // Uppercase + ( → constructor
                if name.starts_with(char::is_uppercase) && self.peek_is(TokenKind::LeftParen) {
                    self.parse_construct_expr();
                    return;
                }

                // Qualified variant: `Filter.All` or `Route.Profile(id: "123")`
                if name.starts_with(char::is_uppercase)
                    && self.peek_is(TokenKind::Dot)
                    && let Some(TokenKind::Identifier(variant_name)) =
                        self.peek_nth_non_trivia_kind(2)
                    && variant_name.starts_with(char::is_uppercase)
                {
                    // Check if there's a `(` after the variant name (3rd non-trivia)
                    let has_args =
                        matches!(self.peek_nth_non_trivia_kind(3), Some(TokenKind::LeftParen));

                    // Emit as CONSTRUCT_EXPR for both unit and parameterized variants
                    self.builder.start_node(SyntaxKind::CONSTRUCT_EXPR.into());
                    self.bump(); // type name (Filter/Route)
                    self.eat_trivia();
                    self.bump(); // .
                    self.eat_trivia();
                    self.bump(); // variant name
                    self.eat_trivia();

                    if has_args {
                        self.expect(TokenKind::LeftParen);
                        self.eat_trivia();

                        // Check for spread
                        if self.at(TokenKind::DotDot) {
                            self.builder.start_node(SyntaxKind::SPREAD_EXPR.into());
                            self.bump();
                            self.eat_trivia();
                            self.parse_expr();
                            self.builder.finish_node();
                            self.eat_trivia();
                            if self.at(TokenKind::Comma) {
                                self.bump();
                                self.eat_trivia();
                            }
                        }

                        if !self.at(TokenKind::RightParen) {
                            self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
                        }

                        self.expect(TokenKind::RightParen);
                    }

                    self.builder.finish_node();
                    return;
                }

                self.bump();
            }

            Some(TokenKind::Banned(_)) => {
                self.builder.start_node(SyntaxKind::ERROR.into());
                let kind = self.current_kind().unwrap();
                if let TokenKind::Banned(banned) = kind {
                    self.error_kind(
                        &format!(
                            "banned keyword '{}': {}",
                            banned.as_str(),
                            banned.help_message()
                        ),
                        super::CstErrorKind::BannedKeyword,
                    );
                }
                self.bump();
                self.builder.finish_node();
            }

            _ => {
                self.builder.start_node(SyntaxKind::ERROR.into());
                if let Some(kind) = self.current_kind() {
                    self.error(&format!("unexpected token: {:?}", kind));
                    self.bump();
                }
                self.builder.finish_node();
            }
        }
    }

    // ── Constructors ─────────────────────────────────────────────

    fn parse_construct_expr(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRUCT_EXPR.into());
        self.bump(); // TypeName
        self.eat_trivia();
        self.expect(TokenKind::LeftParen);
        self.eat_trivia();

        // Check for spread: `..expr`
        if self.at(TokenKind::DotDot) {
            self.builder.start_node(SyntaxKind::SPREAD_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_expr();
            self.builder.finish_node();
            self.eat_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.eat_trivia();
            }
        }

        if !self.at(TokenKind::RightParen) {
            self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
        }

        self.expect(TokenKind::RightParen);
        self.builder.finish_node();
    }

    // ── Call Arguments ───────────────────────────────────────────

    fn parse_call_arg(&mut self) {
        self.builder.start_node(SyntaxKind::ARG.into());

        // Named arg: `label: expr` or punned `label:`
        if self.is_ident() && self.peek_is(TokenKind::Colon) {
            self.bump(); // label
            self.eat_trivia();
            self.bump(); // :

            // Punning: `label:` without a value — next non-trivia is `)` or `,`
            let next = self.next_non_trivia_kind();
            let is_pun = matches!(
                next,
                Some(TokenKind::RightParen) | Some(TokenKind::Comma) | None
            );
            if !is_pun {
                self.eat_trivia();
                self.parse_expr();
            }
        } else {
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── Dot Shorthand ────────────────────────────────────────────

    /// Parse `.field` or `.field op expr` dot shorthand expression.
    fn parse_dot_shorthand(&mut self) {
        self.builder.start_node(SyntaxKind::DOT_SHORTHAND.into());
        self.expect(TokenKind::Dot);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        // Check for optional binary operator predicate
        if self.at(TokenKind::EqualEqual)
            || self.at(TokenKind::BangEqual)
            || self.at(TokenKind::LessThan)
            || self.at(TokenKind::GreaterThan)
            || self.at(TokenKind::LessEqual)
            || self.at(TokenKind::GreaterEqual)
            || self.at(TokenKind::AmpAmp)
            || self.at(TokenKind::PipePipe)
            || self.at(TokenKind::Plus)
            || self.at(TokenKind::Minus)
            || self.at(TokenKind::Star)
            || self.at(TokenKind::Slash)
            || self.at(TokenKind::Percent)
        {
            self.bump(); // operator
            self.eat_trivia();
            self.parse_postfix_expr();
        }

        self.builder.finish_node();
    }

    // ── Fn Lambda ────────────────────────────────────────────────

    /// Parse `(params) => body` arrow closure.
    fn parse_arrow_closure(&mut self) {
        self.builder.start_node(SyntaxKind::ARROW_EXPR.into());

        self.expect(TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_param, TokenKind::RightParen);
        self.expect(TokenKind::RightParen);
        self.eat_trivia();
        self.expect(TokenKind::FatArrow);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }

    // ── Match Expression ─────────────────────────────────────────

    fn parse_match_expr(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_EXPR.into());
        self.expect(TokenKind::Match);
        self.eat_trivia();
        self.parse_expr();
        self.eat_trivia();
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            let prev_pos = self.pos;
            self.parse_match_arm();
            self.eat_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.eat_trivia();
            }
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
        }

        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }

    /// Parse `match { arms }` without a subject — used for `x |> match { ... }`.
    fn parse_subjectless_match_expr(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_EXPR.into());
        self.expect(TokenKind::Match);
        self.eat_trivia();
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            let prev_pos = self.pos;
            self.parse_match_arm();
            self.eat_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.eat_trivia();
            }
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
        }

        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }

    fn parse_match_arm(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_ARM.into());
        self.parse_pattern();
        self.eat_trivia();

        // Optional guard: `when expr`
        if self.at(TokenKind::When) {
            self.builder.start_node(SyntaxKind::MATCH_GUARD.into());
            self.bump(); // consume `when`
            self.eat_trivia();
            self.parse_expr();
            self.builder.finish_node();
            self.eat_trivia();
        }

        self.expect(TokenKind::ThinArrow);
        self.eat_trivia();
        self.parse_expr();
        self.builder.finish_node();
    }

    // ── Pattern ─────────────────────────────────────────────────

    fn parse_pattern(&mut self) {
        self.builder.start_node(SyntaxKind::PATTERN.into());

        match self.current_kind() {
            Some(TokenKind::Underscore) => {
                self.bump();
            }
            Some(TokenKind::Bool(_)) => {
                self.bump();
            }
            Some(TokenKind::String(_)) => {
                self.bump();
            }
            Some(TokenKind::Minus) => {
                // Negative number pattern: `-1`, `-3.14`
                self.bump(); // -
                self.eat_trivia();
                if matches!(self.current_kind(), Some(TokenKind::Number(_))) {
                    self.bump();
                } else {
                    self.error("expected number after '-' in pattern");
                }
            }
            Some(TokenKind::Number(_)) => {
                self.bump();
                self.eat_trivia();
                if self.at(TokenKind::DotDot) {
                    self.bump();
                    self.eat_trivia();
                    // Expect number after ..
                    if matches!(self.current_kind(), Some(TokenKind::Number(_))) {
                        self.bump();
                    } else {
                        self.error("expected number after '..' in range pattern");
                    }
                }
            }
            Some(TokenKind::LeftBrace) => {
                self.bump(); // {
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_record_pattern_field, TokenKind::RightBrace);
                self.expect(TokenKind::RightBrace);
            }
            Some(TokenKind::LeftBracket) => {
                // Array pattern: [], [a, b], [first, ..rest]
                self.bump(); // [
                self.eat_trivia();
                // Parse elements and optional rest pattern
                while !self.at(TokenKind::RightBracket) && !self.at_end() {
                    // Check for rest pattern: ..name
                    if self.at(TokenKind::DotDot) {
                        self.bump(); // ..
                        self.eat_trivia();
                        // Expect identifier or _ after ..
                        if matches!(
                            self.current_kind(),
                            Some(TokenKind::Identifier(_)) | Some(TokenKind::Underscore)
                        ) {
                            self.bump();
                        } else {
                            self.error("expected identifier after '..' in array pattern");
                        }
                        self.eat_trivia();
                        if self.at(TokenKind::Comma) {
                            self.bump();
                            self.eat_trivia();
                        }
                        break;
                    }
                    self.parse_pattern();
                    self.eat_trivia();
                    if self.at(TokenKind::Comma) {
                        self.bump();
                        self.eat_trivia();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RightBracket);
            }
            Some(TokenKind::LeftParen) => {
                // Tuple pattern: (x, y)
                self.bump(); // (
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                self.expect(TokenKind::RightParen);
            }
            // Implicit variant pattern: .Variant or .Variant(args)
            Some(TokenKind::Dot) if matches!(self.peek_nth_non_trivia_kind(1), Some(TokenKind::Identifier(n)) if n.starts_with(char::is_uppercase)) =>
            {
                self.bump(); // .
                self.eat_trivia();
                self.bump(); // Variant name
                self.eat_trivia();
                if self.at(TokenKind::LeftParen) {
                    self.bump();
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                }
            }
            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();
                if name.starts_with(char::is_uppercase) {
                    self.bump();
                    self.eat_trivia();
                    // Qualified variant pattern: `Type.Variant` or `Type.Variant(...)`
                    if self.at(TokenKind::Dot) {
                        self.bump(); // .
                        self.eat_trivia();
                        // Accept identifiers after dot
                        if self.is_ident() {
                            self.bump();
                        } else {
                            self.expect_ident();
                        }
                        self.eat_trivia();
                    }
                    if self.at(TokenKind::LeftParen) {
                        self.bump();
                        self.eat_trivia();
                        self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                        self.expect(TokenKind::RightParen);
                    } else if self.at(TokenKind::LeftBrace) {
                        // Named-field variant pattern: `Rectangle { width, height: h }`
                        self.bump(); // {
                        self.eat_trivia();
                        self.parse_comma_separated(
                            Self::parse_named_variant_pattern_field,
                            TokenKind::RightBrace,
                        );
                        self.expect(TokenKind::RightBrace);
                    }
                } else {
                    self.bump();
                }
            }
            _ => {
                self.error(&format!(
                    "unexpected token in pattern: {:?}",
                    self.current_kind()
                ));
                if !self.at_end() {
                    self.bump();
                }
            }
        }

        self.builder.finish_node();
    }

    fn parse_record_pattern_field(&mut self) {
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_pattern();
        }
    }

    /// Parse a named-field entry inside a brace-form variant pattern:
    /// `width` (shorthand binding) or `width: h` (rename) or `width: 0` (nested).
    fn parse_named_variant_pattern_field(&mut self) {
        self.builder
            .start_node(SyntaxKind::VARIANT_FIELD_PATTERN.into());
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_pattern();
        }
        self.builder.finish_node();
    }

    // ── Object Literal ───────────────────────────────────────────

    /// Check if the current `{` starts an object literal rather than a block.
    /// An object literal has the form `{ ident: expr, ... }` or `{ ident, ... }` (shorthand).
    fn is_object_literal(&self) -> bool {
        // Look ahead past trivia after `{`
        let mut i = self.pos + 1;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        if i >= self.tokens.len() {
            return false;
        }
        // Must be an identifier (not a keyword)
        if !matches!(self.tokens[i].kind, TokenKind::Identifier(_)) {
            return false;
        }
        // Next non-trivia token after the ident must be `:` (key: value) or `,` or `}` (shorthand)
        i += 1;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        if i >= self.tokens.len() {
            return false;
        }
        matches!(
            self.tokens[i].kind,
            TokenKind::Colon | TokenKind::Comma | TokenKind::RightBrace
        )
    }

    fn parse_object_literal(&mut self) {
        self.builder.start_node(SyntaxKind::OBJECT_EXPR.into());
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_object_field, TokenKind::RightBrace);
        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }

    fn parse_object_field(&mut self) {
        self.builder.start_node(SyntaxKind::OBJECT_FIELD.into());
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump(); // :
            self.eat_trivia();
            self.parse_expr();
        }
        // If no colon, it's shorthand: { name } means { name: name }
        self.builder.finish_node();
    }

    // ── Block Expression ─────────────────────────────────────────

    pub(super) fn parse_block_expr(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_EXPR.into());
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            let prev_pos = self.pos;
            self.parse_item();
            self.eat_trivia();
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
        }

        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }
}
