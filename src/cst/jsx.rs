use super::*;

impl<'src> CstParser<'src> {
    // ── JSX ──────────────────────────────────────────────────────

    pub(super) fn parse_jsx_element(&mut self) {
        self.builder.start_node(SyntaxKind::JSX_ELEMENT.into());
        self.expect(TokenKind::LessThan);
        self.eat_trivia();

        // Fragment: <>
        if self.at(TokenKind::GreaterThan) {
            self.bump(); // >
            self.parse_jsx_children();
            // Expect </>
            self.expect(TokenKind::LessThan);
            self.expect(TokenKind::Slash);
            self.expect(TokenKind::GreaterThan);
            self.builder.finish_node();
            return;
        }

        self.expect_ident();
        self.parse_jsx_member_segments();
        self.eat_trivia();

        // Props
        while !self.at(TokenKind::GreaterThan) && !self.at(TokenKind::Slash) && !self.at_end() {
            let prev_pos = self.pos;
            self.parse_jsx_prop();
            self.eat_trivia();
            if self.pos == prev_pos && !self.at_end() {
                // Safety: skip stuck token to prevent infinite loop.
                self.error(&format!(
                    "unexpected token in JSX element: {:?}",
                    self.current_kind()
                ));
                self.bump();
            }
        }

        // Self-closing: />
        if self.at(TokenKind::Slash) {
            self.bump();
            self.expect(TokenKind::GreaterThan);
            self.builder.finish_node();
            return;
        }

        self.expect(TokenKind::GreaterThan);
        self.parse_jsx_children();

        // Closing tag: </Tag> or </Tag.Member>
        self.expect(TokenKind::LessThan);
        self.expect(TokenKind::Slash);
        self.eat_trivia();
        self.expect_ident();
        self.parse_jsx_member_segments();
        self.expect(TokenKind::GreaterThan);

        self.builder.finish_node();
    }

    /// Parse `.Member` segments after a JSX tag name (e.g., `.Trigger` in `Select.Trigger`).
    fn parse_jsx_member_segments(&mut self) {
        while self.at(TokenKind::Dot) {
            self.bump();
            if self.is_member_name_token() {
                self.bump();
            } else {
                self.expect_ident();
            }
        }
    }

    fn parse_jsx_prop(&mut self) {
        // JSX spread: {...expr}
        if self.at(TokenKind::LeftBrace) && self.peek_is(TokenKind::DotDotDot) {
            self.builder.start_node(SyntaxKind::JSX_SPREAD_PROP.into());
            self.bump(); // {
            self.eat_trivia();
            self.bump(); // ...
            self.eat_trivia();
            self.parse_expr();
            self.eat_trivia();
            self.expect(TokenKind::RightBrace);
            self.builder.finish_node();
            return;
        }

        self.builder.start_node(SyntaxKind::JSX_PROP.into());
        // Accept identifiers and keywords as JSX prop names (e.g., type="text", for="id")
        // Also support hyphenated names like aria-label, data-testid
        if self.is_ident() || self.is_keyword() {
            self.bump();
        } else {
            self.expect_ident();
        }
        // Continue consuming -ident sequences for hyphenated attribute names
        while self.at(TokenKind::Minus) {
            self.bump(); // -
            if self.is_ident() || self.is_keyword() {
                self.bump();
            } else {
                self.expect_ident();
                break;
            }
        }
        self.eat_trivia();

        if self.at(TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            if self.at(TokenKind::LeftBrace) {
                self.bump();
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightBrace);
            } else if matches!(self.current_kind(), Some(TokenKind::String(_))) {
                self.bump();
            } else {
                self.error("expected '{' or string after '=' in JSX prop");
            }
        }

        self.builder.finish_node();
    }

    fn parse_jsx_children(&mut self) {
        loop {
            // Check for closing tag
            if self.at(TokenKind::LessThan) && self.peek_is(TokenKind::Slash) {
                break;
            }
            if self.at_end() {
                break;
            }

            let prev_pos = self.pos;
            match self.current_kind() {
                Some(TokenKind::LeftBrace) => {
                    self.builder.start_node(SyntaxKind::JSX_EXPR_CHILD.into());
                    self.bump();
                    self.eat_trivia();
                    // {/* comment */} — block comment already eaten as trivia,
                    // so if the next token is `}` there's no expr to parse.
                    if !self.at(TokenKind::RightBrace) {
                        self.parse_expr();
                        self.eat_trivia();
                    }
                    self.expect(TokenKind::RightBrace);
                    self.builder.finish_node();
                }
                Some(TokenKind::LessThan) => {
                    self.parse_jsx_element();
                }
                _ => {
                    if self.is_jsx_text_token() {
                        self.builder.start_node(SyntaxKind::JSX_TEXT.into());
                        self.bump();
                        while !self.at_end()
                            && !self.at(TokenKind::LeftBrace)
                            && !self.at(TokenKind::LessThan)
                            && self.is_jsx_text_token()
                        {
                            self.bump();
                        }
                        self.builder.finish_node();
                    } else {
                        break;
                    }
                }
            }
            // Safety: if no progress was made, skip the stuck token.
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
        }
    }
}
