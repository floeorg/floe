use crate::lexer::token::TokenKind;

use super::ast::*;
use super::{ParseError, Parser};

impl TokenKind {
    /// Map a keyword or operator token to its source text representation.
    /// Used by `token_as_jsx_text` to avoid a large allowlist.
    fn as_jsx_text_str(&self) -> Option<&'static str> {
        match self {
            // Keywords
            TokenKind::Const => Some("const"),
            TokenKind::Fn => Some("fn"),
            TokenKind::Export => Some("export"),
            TokenKind::Import => Some("import"),
            TokenKind::From => Some("from"),
            TokenKind::Match => Some("match"),
            TokenKind::Type => Some("type"),
            TokenKind::Opaque => Some("opaque"),
            TokenKind::Async => Some("async"),
            TokenKind::Await => Some("await"),
            TokenKind::For => Some("for"),
            TokenKind::SelfKw => Some("self"),
            TokenKind::Try => Some("try"),
            TokenKind::Trait => Some("trait"),
            TokenKind::When => Some("when"),
            TokenKind::Ok => Some("Ok"),
            TokenKind::Err => Some("Err"),
            TokenKind::Some => Some("Some"),
            TokenKind::None => Some("None"),
            TokenKind::Todo => Some("todo"),
            TokenKind::Unreachable => Some("unreachable"),
            TokenKind::Assert => Some("assert"),
            // Operators and punctuation
            TokenKind::Comma => Some(","),
            TokenKind::Colon => Some(":"),
            TokenKind::Dot => Some("."),
            TokenKind::Plus => Some("+"),
            TokenKind::Minus => Some("-"),
            TokenKind::Star => Some("*"),
            TokenKind::Slash => Some("/"),
            TokenKind::Percent => Some("%"),
            TokenKind::EqualEqual => Some("=="),
            TokenKind::BangEqual => Some("!="),
            TokenKind::Bang => Some("!"),
            TokenKind::Equal => Some("="),
            TokenKind::GreaterThan => Some(">"),
            TokenKind::GreaterEqual => Some(">="),
            TokenKind::LessEqual => Some("<="),
            TokenKind::Pipe => Some("|>"),
            TokenKind::VerticalBar => Some("|"),
            TokenKind::ThinArrow => Some("->"),
            TokenKind::FatArrow => Some("=>"),
            TokenKind::Question => Some("?"),
            TokenKind::Underscore => Some("_"),
            TokenKind::DotDot => Some(".."),
            TokenKind::AmpAmp => Some("&&"),
            TokenKind::PipePipe => Some("||"),
            TokenKind::Semicolon => Some(";"),
            // Delimiters
            TokenKind::LeftParen => Some("("),
            TokenKind::RightParen => Some(")"),
            TokenKind::LeftBracket => Some("["),
            TokenKind::RightBracket => Some("]"),
            TokenKind::RightBrace => Some("}"),
            _ => Option::None,
        }
    }
}

impl Parser {
    // ── JSX ──────────────────────────────────────────────────────

    pub(super) fn parse_jsx_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        let element = self.parse_jsx_element()?;
        let span = self.merge_spans(start_span, element.span);
        Ok(Expr {
            kind: ExprKind::Jsx(element),
            span,
        })
    }

    fn parse_jsx_element(&mut self) -> Result<JsxElement, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LessThan)?;

        // Fragment: `<>`
        if self.check(&TokenKind::GreaterThan) {
            self.advance();
            let children = self.parse_jsx_children(Option::None)?;
            // Expect `</>`
            self.expect(&TokenKind::LessThan)?;
            self.expect(&TokenKind::Slash)?;
            self.expect(&TokenKind::GreaterThan)?;
            let end_span = self.previous_span();
            return Ok(JsxElement {
                kind: JsxElementKind::Fragment { children },
                span: self.merge_spans(start_span, end_span),
            });
        }

        let name = self.expect_identifier()?;

        // Parse props
        let mut props = Vec::new();
        while !self.check(&TokenKind::GreaterThan)
            && !self.check(&TokenKind::Slash)
            && !self.is_at_end()
        {
            let prop = self.parse_jsx_prop()?;
            props.push(prop);
        }

        // Self-closing: `<Tag ... />`
        if self.check(&TokenKind::Slash) {
            self.advance();
            self.expect(&TokenKind::GreaterThan)?;
            let end_span = self.previous_span();
            return Ok(JsxElement {
                kind: JsxElementKind::Element {
                    name,
                    props,
                    children: Vec::new(),
                    self_closing: true,
                },
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Opening tag close
        self.expect(&TokenKind::GreaterThan)?;

        // Children
        let children = self.parse_jsx_children(Some(&name))?;

        // Closing tag: `</Tag>`
        self.expect(&TokenKind::LessThan)?;
        self.expect(&TokenKind::Slash)?;
        let closing_name = self.expect_identifier()?;
        if closing_name != name {
            return Err(self.error_with_kind(
                &format!("mismatched closing tag: expected </{name}>, found </{closing_name}>"),
                super::ParseErrorKind::MismatchedTag,
            ));
        }
        self.expect(&TokenKind::GreaterThan)?;
        let end_span = self.previous_span();

        Ok(JsxElement {
            kind: JsxElementKind::Element {
                name,
                props,
                children,
                self_closing: false,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_jsx_prop(&mut self) -> Result<JsxProp, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_jsx_attr_name()?;

        let value = if self.check(&TokenKind::Equal) {
            self.advance();
            if self.check(&TokenKind::LeftBrace) {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                Some(expr)
            } else if matches!(self.current_kind(), TokenKind::String(_)) {
                Some(self.parse_primary_expr()?)
            } else {
                return Err(self.error("expected '{' or string after '=' in JSX prop"));
            }
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(JsxProp {
            name,
            value,
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_jsx_children(
        &mut self,
        _parent_name: Option<&str>,
    ) -> Result<Vec<JsxChild>, ParseError> {
        let mut children = Vec::new();

        loop {
            // Check for closing tag `</`
            if self.check(&TokenKind::LessThan) && self.peek_kind() == Some(&TokenKind::Slash) {
                break;
            }

            if self.is_at_end() {
                break;
            }

            match self.current_kind() {
                // Expression child: `{expr}`
                TokenKind::LeftBrace => {
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    children.push(JsxChild::Expr(expr));
                }
                // Nested JSX element
                TokenKind::LessThan => {
                    let element = self.parse_jsx_element()?;
                    children.push(JsxChild::Element(element));
                }
                // Text content — collect text-like tokens
                _ => {
                    if let Some(text) = self.token_as_jsx_text() {
                        let mut text_buf = text;
                        self.advance();
                        while !self.is_at_end()
                            && !self.check(&TokenKind::LeftBrace)
                            && !self.check(&TokenKind::LessThan)
                        {
                            if let Some(t) = self.token_as_jsx_text() {
                                text_buf.push(' ');
                                text_buf.push_str(&t);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        children.push(JsxChild::Text(text_buf));
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(children)
    }

    /// Try to interpret the current token as JSX text content.
    /// In JSX children, almost everything that isn't `{`, `<`, or EOF is text.
    /// Uses a small denylist instead of listing every allowed token.
    pub(super) fn token_as_jsx_text(&self) -> Option<String> {
        let kind = &self.tokens[self.pos].kind;
        match kind {
            // Denylist: these tokens are never JSX text
            TokenKind::LeftBrace
            | TokenKind::LessThan
            | TokenKind::Eof
            | TokenKind::Whitespace
            | TokenKind::Comment
            | TokenKind::BlockComment
            | TokenKind::TemplateLiteral(_) => None,

            // Identifiers and literals carry their own text
            TokenKind::Identifier(s) | TokenKind::Number(s) | TokenKind::String(s) => {
                Some(s.clone())
            }
            TokenKind::Bool(b) => Some(b.to_string()),

            // Banned keywords — format as lowercase text
            TokenKind::Banned(b) => Some(b.as_str().to_string()),

            // Everything else: use the static text mapping
            other => other.as_jsx_text_str().map(String::from),
        }
    }

    /// Like `expect_identifier` but also accepts keywords (e.g. `type`, `match`)
    /// since JSX attribute names can be any valid HTML attribute.
    pub(super) fn expect_jsx_attr_name(&mut self) -> Result<String, ParseError> {
        match self.current_kind() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            // Allow keywords as JSX attribute names (e.g. <input type="text" />)
            TokenKind::Type => {
                self.advance();
                Ok("type".to_string())
            }
            TokenKind::Match => {
                self.advance();
                Ok("match".to_string())
            }
            TokenKind::Async => {
                self.advance();
                Ok("async".to_string())
            }
            TokenKind::Export => {
                self.advance();
                Ok("export".to_string())
            }
            TokenKind::Import => {
                self.advance();
                Ok("import".to_string())
            }
            TokenKind::From => {
                self.advance();
                Ok("from".to_string())
            }
            TokenKind::Const => {
                self.advance();
                Ok("const".to_string())
            }
            TokenKind::Fn => {
                self.advance();
                Ok("fn".to_string())
            }
            _ => Err(self.error(&format!(
                "expected attribute name, found {:?}",
                self.current_kind()
            ))),
        }
    }
}
