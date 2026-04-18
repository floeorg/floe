use super::*;

impl<'src> CstParser<'src> {
    // ── Type Expressions ────────────────────────────────────────

    pub(super) fn parse_type_expr(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_EXPR.into());

        // Function type: (params) -> ReturnType or () -> ReturnType
        if self.at(TokenKind::LeftParen) && self.is_paren_function_type() {
            self.parse_function_type();
        }
        // Unit type: ()
        else if self.at(TokenKind::LeftParen) && self.peek_is(TokenKind::RightParen) {
            self.bump(); // (
            self.eat_trivia();
            self.bump(); // )
        }
        // Function type: fn(params) -> ReturnType (old syntax — error)
        else if self.at(TokenKind::Fn) && self.peek_is(TokenKind::LeftParen) {
            self.builder.start_node(SyntaxKind::ERROR.into());
            self.error("function types use arrow syntax: `(T) -> U` instead of `fn(T) -> U`");
            self.bump(); // fn
            self.builder.finish_node();
        }
        // Tuple type: (T, U) — paren with comma, no `->` after `)`
        else if self.at(TokenKind::LeftParen) && self.is_paren_tuple_type() {
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
        }
        // Tuple: [T, U]
        else if self.at(TokenKind::LeftBracket) {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightBracket);
            self.expect(TokenKind::RightBracket);
        }
        // Record type: { ... }
        else if self.at(TokenKind::LeftBrace) {
            self.parse_record_fields();
        }
        // String literal type (e.g. ComponentProps<"div">)
        else if self.at(TokenKind::String("".into())) {
            self.bump();
        }
        // typeof <ident> or typeof Module.value
        else if self.at(TokenKind::Typeof) {
            self.bump();
            self.eat_trivia();
            self.expect_ident();
            self.eat_trivia();
            while self.at(TokenKind::Dot) {
                self.bump();
                self.eat_trivia();
                self.expect_ident();
                self.eat_trivia();
            }
        }
        // Named type
        else {
            self.expect_ident();
            self.eat_trivia();

            // Dotted names (e.g. JSX.Element)
            while self.at(TokenKind::Dot) {
                self.bump();
                self.eat_trivia();
                self.expect_ident();
                self.eat_trivia();
            }

            // Type arguments: <T, U>
            if self.at(TokenKind::LessThan) {
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_type_expr, TokenKind::GreaterThan);
                self.expect(TokenKind::GreaterThan);
                self.eat_trivia();
            }
        }

        // Intersection: A & B & C — parse as flat list within this TYPE_EXPR node
        while self.at(TokenKind::Amp) {
            self.bump(); // &
            self.eat_trivia();
            self.parse_type_expr();
        }

        self.builder.finish_node();
    }

    fn parse_function_type(&mut self) {
        self.expect(TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightParen);
        self.expect(TokenKind::RightParen);
        self.eat_trivia();
        self.expect(TokenKind::FatArrow);
        self.eat_trivia();
        self.parse_type_expr();
    }
}
