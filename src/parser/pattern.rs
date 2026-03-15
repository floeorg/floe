use crate::lexer::token::TokenKind;

use super::ast::*;
use super::{ParseError, Parser};

impl Parser {
    // ── Match Expression ─────────────────────────────────────────

    pub(super) fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::Match)?;
        let subject = self.parse_expr()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let arm = self.parse_match_arm()?;
            arms.push(arm);
            // Optional comma between arms
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Match {
                subject: Box::new(subject),
                arms,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start_span = self.current_span();
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::ThinArrow)?;
        let body = self.parse_expr()?;
        let end_span = self.previous_span();

        Ok(MatchArm {
            pattern,
            body,
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            // Wildcard: `_`
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Wildcard,
                    span: start_span,
                })
            }

            // Boolean literal pattern
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Literal(LiteralPattern::Bool(b)),
                    span: start_span,
                })
            }

            // String literal pattern — may contain `{name}` captures
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();

                // Check if the string contains capture patterns like {id}
                if let Some(segments) = parse_string_pattern_segments(&s) {
                    Ok(Pattern {
                        kind: PatternKind::StringPattern { segments },
                        span: start_span,
                    })
                } else {
                    Ok(Pattern {
                        kind: PatternKind::Literal(LiteralPattern::String(s)),
                        span: start_span,
                    })
                }
            }

            // Number literal pattern (possibly a range)
            TokenKind::Number(n) => {
                let n = n.clone();
                self.advance();

                // Check for range: `1..10`
                if self.check(&TokenKind::DotDot) {
                    self.advance();
                    let end_num = match self.current_kind() {
                        TokenKind::Number(e) => {
                            let e = e.clone();
                            self.advance();
                            e
                        }
                        _ => return Err(self.error("expected number after '..' in range pattern")),
                    };
                    let end_span = self.previous_span();
                    return Ok(Pattern {
                        kind: PatternKind::Range {
                            start: LiteralPattern::Number(n),
                            end: LiteralPattern::Number(end_num),
                        },
                        span: self.merge_spans(start_span, end_span),
                    });
                }

                Ok(Pattern {
                    kind: PatternKind::Literal(LiteralPattern::Number(n)),
                    span: start_span,
                })
            }

            // Record pattern: `{ x, y }` or `{ ctrl: true }`
            TokenKind::LeftBrace => {
                self.advance();
                let fields = self.parse_comma_separated(|p| p.parse_record_pattern_field())?;
                self.expect(&TokenKind::RightBrace)?;
                let end_span = self.previous_span();
                Ok(Pattern {
                    kind: PatternKind::Record { fields },
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // None pattern
            TokenKind::None => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Variant {
                        name: "None".to_string(),
                        fields: Vec::new(),
                    },
                    span: start_span,
                })
            }

            // Ok/Err/Some variant patterns
            TokenKind::Ok | TokenKind::Err | TokenKind::Some => {
                let name = match self.current_kind() {
                    TokenKind::Ok => "Ok".to_string(),
                    TokenKind::Err => "Err".to_string(),
                    TokenKind::Some => "Some".to_string(),
                    _ => unreachable!(),
                };
                self.advance();

                let fields = if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let f = self.parse_comma_separated(|p| p.parse_pattern())?;
                    self.expect(&TokenKind::RightParen)?;
                    f
                } else {
                    Vec::new()
                };

                let end_span = self.previous_span();
                Ok(Pattern {
                    kind: PatternKind::Variant { name, fields },
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Tuple pattern: `(x, y)` or `(_, 0)`
            TokenKind::LeftParen => {
                self.advance(); // (
                let patterns = self.parse_comma_separated(|p| p.parse_pattern())?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Pattern {
                    kind: PatternKind::Tuple(patterns),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Identifier — could be a variant pattern or a binding
            TokenKind::Identifier(name) => {
                let name = name.clone();

                // Uppercase + `(` → variant pattern: `Click(el, { x, y })`
                if name.starts_with(char::is_uppercase) {
                    self.advance();
                    let fields = if self.check(&TokenKind::LeftParen) {
                        self.advance();
                        let f = self.parse_comma_separated(|p| p.parse_pattern())?;
                        self.expect(&TokenKind::RightParen)?;
                        f
                    } else {
                        Vec::new()
                    };
                    let end_span = self.previous_span();
                    return Ok(Pattern {
                        kind: PatternKind::Variant { name, fields },
                        span: self.merge_spans(start_span, end_span),
                    });
                }

                // Lowercase → binding: `x`, `msg`
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Binding(name),
                    span: start_span,
                })
            }

            _ => Err(self.error(&format!(
                "unexpected token in pattern: {:?}",
                self.current_kind()
            ))),
        }
    }

    fn parse_record_pattern_field(&mut self) -> Result<(String, Pattern), ParseError> {
        let name = self.expect_identifier()?;

        // `field: pattern` or just `field` (shorthand for `field: field`)
        let pattern = if self.check(&TokenKind::Colon) {
            self.advance();
            self.parse_pattern()?
        } else {
            // Shorthand: `{ x }` means `{ x: x }`
            Pattern {
                kind: PatternKind::Binding(name.clone()),
                span: self.previous_span(),
            }
        };

        Ok((name, pattern))
    }
}
