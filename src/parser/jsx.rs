use crate::lexer::token::TokenKind;

use super::ast::*;
use super::{ParseError, Parser};

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
            return Err(self.error(&format!(
                "mismatched closing tag: expected </{name}>, found </{closing_name}>"
            )));
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
    /// In JSX children, almost everything that isn't `{`, `<`, or `</` is text.
    pub(super) fn token_as_jsx_text(&self) -> Option<String> {
        match &self.tokens[self.pos].kind {
            // These end text content — never treat as text
            TokenKind::LeftBrace | TokenKind::LessThan | TokenKind::Eof => None,

            // Identifiers and literals
            TokenKind::Identifier(s) | TokenKind::Number(s) | TokenKind::String(s) => {
                Some(s.clone())
            }
            TokenKind::Bool(b) => Some(b.to_string()),

            // Keywords — valid as JSX text
            TokenKind::Const => Some("const".into()),
            TokenKind::Fn => Some("fn".into()),
            TokenKind::Export => Some("export".into()),
            TokenKind::Import => Some("import".into()),
            TokenKind::From => Some("from".into()),
            TokenKind::Return => Some("return".into()),
            TokenKind::Match => Some("match".into()),
            TokenKind::Type => Some("type".into()),
            TokenKind::Opaque => Some("opaque".into()),
            TokenKind::Async => Some("async".into()),
            TokenKind::Await => Some("await".into()),
            TokenKind::For => Some("for".into()),
            TokenKind::SelfKw => Some("self".into()),
            TokenKind::Try => Some("try".into()),
            TokenKind::Ok => Some("Ok".into()),
            TokenKind::Err => Some("Err".into()),
            TokenKind::Some => Some("Some".into()),
            TokenKind::None => Some("None".into()),
            TokenKind::Todo => Some("todo".into()),
            TokenKind::Unreachable => Some("unreachable".into()),

            // Punctuation — valid in JSX text
            TokenKind::Comma => Some(",".into()),
            TokenKind::Colon => Some(":".into()),
            TokenKind::Dot => Some(".".into()),
            TokenKind::Plus => Some("+".into()),
            TokenKind::Minus => Some("-".into()),
            TokenKind::Star => Some("*".into()),
            TokenKind::Slash => Some("/".into()),
            TokenKind::Percent => Some("%".into()),
            TokenKind::EqualEqual => Some("==".into()),
            TokenKind::BangEqual => Some("!=".into()),
            TokenKind::Bang => Some("!".into()),
            TokenKind::Equal => Some("=".into()),
            TokenKind::GreaterThan => Some(">".into()),
            TokenKind::GreaterEqual => Some(">=".into()),
            TokenKind::LessEqual => Some("<=".into()),
            TokenKind::Pipe => Some("|>".into()),
            TokenKind::VerticalBar => Some("|".into()),
            TokenKind::ThinArrow => Some("->".into()),
            TokenKind::FatArrow => Some("=>".into()),
            TokenKind::Question => Some("?".into()),
            TokenKind::Underscore => Some("_".into()),
            TokenKind::DotDot => Some("..".into()),
            TokenKind::AmpAmp => Some("&&".into()),
            TokenKind::PipePipe => Some("||".into()),
            TokenKind::Semicolon => Some(";".into()),

            // Parens/brackets in text
            TokenKind::LeftParen => Some("(".into()),
            TokenKind::RightParen => Some(")".into()),
            TokenKind::LeftBracket => Some("[".into()),
            TokenKind::RightBracket => Some("]".into()),
            TokenKind::RightBrace => Some("}".into()),

            // Banned keywords — still valid as text content
            TokenKind::Banned(b) => Some(format!("{b:?}").to_lowercase()),

            // Template literals in JSX text — skip
            TokenKind::TemplateLiteral(_) => None,

            // Trivia tokens — skip (shouldn't appear in normal token stream)
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::BlockComment => None,
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
            TokenKind::Return => {
                self.advance();
                Ok("return".to_string())
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
