use super::*;

impl<'src> Lowerer<'src> {
    pub(super) fn lower_match_arm(&mut self, node: &SyntaxNode) -> Option<MatchArm> {
        let span = self.node_span(node);
        let mut pattern = None;
        let mut guard = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PATTERN => {
                    if pattern.is_none() {
                        pattern = self.lower_pattern(&child);
                    }
                }
                SyntaxKind::MATCH_GUARD => {
                    guard = self.lower_guard(&child);
                }
                _ => {
                    if body.is_none() {
                        body = self.lower_expr_node(&child);
                    }
                }
            }
        }

        // If body wasn't found in child nodes, check tokens after ->
        if body.is_none() {
            body = self.lower_token_expr_after_arrow(node);
        }

        Some(MatchArm {
            pattern: pattern?,
            guard,
            body: body?,
            span,
        })
    }

    fn lower_guard(&mut self, node: &SyntaxNode) -> Option<Expr> {
        // The guard node contains `when` keyword + expression
        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                return Some(expr);
            }
        }
        // Try token expression inside guard
        for child_or_token in node.children_with_tokens() {
            if let Some(token) = child_or_token.as_token() {
                if token.kind() == SyntaxKind::KW_WHEN {
                    continue;
                }
                if !token.kind().is_trivia()
                    && let Some(expr) = self.token_to_expr(token)
                {
                    return Some(expr);
                }
            }
        }
        None
    }

    pub(super) fn lower_token_expr_after_arrow(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_arrow = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::THIN_ARROW {
                        past_arrow = true;
                        continue;
                    }
                    if past_arrow && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_arrow {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }

    pub(super) fn lower_pattern(&mut self, node: &SyntaxNode) -> Option<Pattern> {
        let span = self.node_span(node);

        // Check tokens for simple patterns
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::UNDERSCORE => {
                        return Some(Pattern {
                            kind: PatternKind::Wildcard,
                            span,
                        });
                    }
                    SyntaxKind::BOOL => {
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::Bool(
                                token.text() == "true",
                            )),
                            span,
                        });
                    }
                    SyntaxKind::STRING => {
                        let s = self.unquote_string(token.text());
                        // Check for string patterns with captures like {id}
                        if let Some(segments) = parse_string_pattern_segments(&s) {
                            return Some(Pattern {
                                kind: PatternKind::StringPattern { segments },
                                span,
                            });
                        }
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::String(s)),
                            span,
                        });
                    }
                    SyntaxKind::MINUS => {
                        // Negative number pattern: find the number token after the minus
                        for next_token in node.children_with_tokens() {
                            if let Some(t) = next_token.as_token()
                                && t.kind() == SyntaxKind::NUMBER
                            {
                                return Some(Pattern {
                                    kind: PatternKind::Literal(LiteralPattern::Number(format!(
                                        "-{}",
                                        t.text()
                                    ))),
                                    span,
                                });
                            }
                        }
                        // Fallback: just a minus with no number (shouldn't happen)
                        return None;
                    }
                    SyntaxKind::NUMBER => {
                        // Check for range
                        if self.has_token(node, SyntaxKind::DOT_DOT) {
                            let numbers = self.collect_numbers(node);
                            if numbers.len() >= 2 {
                                return Some(Pattern {
                                    kind: PatternKind::Range {
                                        start: LiteralPattern::Number(numbers[0].clone()),
                                        end: LiteralPattern::Number(numbers[1].clone()),
                                    },
                                    span,
                                });
                            }
                        }
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::Number(
                                token.text().to_string(),
                            )),
                            span,
                        });
                    }
                    SyntaxKind::KW_NONE => {
                        return Some(Pattern {
                            kind: PatternKind::Variant {
                                name: "None".to_string(),
                                fields: Vec::new(),
                            },
                            span,
                        });
                    }
                    SyntaxKind::KW_OK | SyntaxKind::KW_ERR | SyntaxKind::KW_SOME => {
                        let name = token.text().to_string();
                        let fields: Vec<Pattern> = node
                            .children()
                            .filter(|c| c.kind() == SyntaxKind::PATTERN)
                            .filter_map(|c| self.lower_pattern(&c))
                            .collect();
                        return Some(Pattern {
                            kind: PatternKind::Variant { name, fields },
                            span,
                        });
                    }
                    SyntaxKind::IDENT => {
                        let name = token.text().to_string();
                        if name.starts_with(char::is_uppercase) {
                            // Check for qualified variant: Type.Variant
                            // If there's a DOT token, use the last IDENT as the variant name
                            let variant_name = if self.has_token(node, SyntaxKind::DOT) {
                                // Find all ident tokens; the last one after the dot is the variant name
                                let mut last_ident = name.clone();
                                let mut past_dot = false;
                                for child_or_token in node.children_with_tokens() {
                                    if let Some(t) = child_or_token.as_token() {
                                        if t.kind() == SyntaxKind::DOT {
                                            past_dot = true;
                                            continue;
                                        }
                                        if past_dot && !t.kind().is_trivia() {
                                            last_ident = t.text().to_string();
                                            break;
                                        }
                                    }
                                }
                                last_ident
                            } else {
                                name
                            };
                            let fields: Vec<Pattern> = node
                                .children()
                                .filter(|c| c.kind() == SyntaxKind::PATTERN)
                                .filter_map(|c| self.lower_pattern(&c))
                                .collect();
                            return Some(Pattern {
                                kind: PatternKind::Variant {
                                    name: variant_name,
                                    fields,
                                },
                                span,
                            });
                        } else {
                            return Some(Pattern {
                                kind: PatternKind::Binding(name),
                                span,
                            });
                        }
                    }
                    SyntaxKind::L_BRACE => {
                        // Record pattern
                        let fields = self.lower_record_pattern_fields(node);
                        return Some(Pattern {
                            kind: PatternKind::Record { fields },
                            span,
                        });
                    }
                    SyntaxKind::L_BRACKET => {
                        // Array pattern: [], [a, b], [first, ..rest]
                        let child_patterns: Vec<Pattern> = node
                            .children()
                            .filter(|c| c.kind() == SyntaxKind::PATTERN)
                            .filter_map(|c| self.lower_pattern(&c))
                            .collect();

                        // Check for rest pattern (..name) by looking for DOT_DOT token
                        let rest = if self.has_token(node, SyntaxKind::DOT_DOT) {
                            // Find the identifier after ..
                            let mut found_dotdot = false;
                            let mut rest_name = None;
                            for child_or_token in node.children_with_tokens() {
                                if let Some(token) = child_or_token.as_token() {
                                    if token.kind() == SyntaxKind::DOT_DOT {
                                        found_dotdot = true;
                                        continue;
                                    }
                                    if found_dotdot && !token.kind().is_trivia() {
                                        match token.kind() {
                                            SyntaxKind::IDENT => {
                                                rest_name = Some(token.text().to_string());
                                            }
                                            SyntaxKind::UNDERSCORE => {
                                                rest_name = Some("_".to_string());
                                            }
                                            _ => {}
                                        }
                                        break;
                                    }
                                }
                            }
                            rest_name
                        } else {
                            None
                        };

                        return Some(Pattern {
                            kind: PatternKind::Array {
                                elements: child_patterns,
                                rest,
                            },
                            span,
                        });
                    }
                    SyntaxKind::L_PAREN => {
                        // Tuple pattern: (x, y)
                        let patterns: Vec<Pattern> = node
                            .children()
                            .filter(|c| c.kind() == SyntaxKind::PATTERN)
                            .filter_map(|c| self.lower_pattern(&c))
                            .collect();
                        return Some(Pattern {
                            kind: PatternKind::Tuple(patterns),
                            span,
                        });
                    }
                    _ => {}
                }
            }
        }

        None
    }

    pub(super) fn lower_record_pattern_fields(
        &mut self,
        node: &SyntaxNode,
    ) -> Vec<(String, Pattern)> {
        let mut fields = Vec::new();
        let idents = self.collect_idents(node);

        // Simple approach: collect ident tokens and check for colon patterns
        // This needs more sophisticated handling for complex patterns
        for ident in &idents {
            // For now, assume shorthand: `{ x }` → `{ x: x }`
            fields.push((
                ident.clone(),
                Pattern {
                    kind: PatternKind::Binding(ident.clone()),
                    span: self.node_span(node),
                },
            ));
        }

        fields
    }
}
