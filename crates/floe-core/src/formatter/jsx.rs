use crate::pretty::{self, Document};
use crate::syntax::{SyntaxKind, SyntaxNode};

use super::Formatter;

pub(crate) enum JsxChildInfo {
    Text(String),
    Expr(SyntaxNode),
    Element(SyntaxNode),
    Comment(String),
}

impl Formatter<'_> {
    pub(crate) fn fmt_jsx(&mut self, node: &SyntaxNode) -> Document {
        // JSX comments (`{/* ... */}`) live in the CST and are handled by JSX
        // formatting. Advance the side-channel cursor past this whole element
        // so the program loop doesn't re-emit them.
        let jsx_end: u32 = node.text_range().end().into();
        self.advance_comment_cursor_to(jsx_end);

        let tag_name = self.jsx_tag_name(node);
        let is_fragment = tag_name.is_none();
        let is_self_closing =
            self.has_token(node, SyntaxKind::SLASH) && !self.jsx_has_children(node);

        let props: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::JSX_PROP || c.kind() == SyntaxKind::JSX_SPREAD_PROP)
            .collect();

        let children = self.jsx_collect_children(node);
        let multiline_props =
            !(props.is_empty() || (props.len() <= 3 && self.jsx_props_short(&props)));

        if is_fragment {
            let mut parts = vec![pretty::str("<>")];
            if children.is_empty() {
                parts.push(pretty::str("</>"));
                return pretty::concat(parts);
            }
            let frag_inline = children.len() == 1
                && match &children[0] {
                    JsxChildInfo::Text(_) | JsxChildInfo::Comment(_) => true,
                    JsxChildInfo::Expr(node) => !self.jsx_expr_is_multiline_node(node),
                    JsxChildInfo::Element(_) => false,
                };
            if frag_inline {
                parts.push(self.fmt_jsx_children_inline(&children));
            } else {
                parts.push(pretty::nest(4, self.fmt_jsx_children_block(&children)));
                parts.push(pretty::line());
            }
            parts.push(pretty::str("</>"));
            return pretty::concat(parts);
        }

        let name = tag_name.unwrap();

        // Opening tag
        let mut parts = vec![pretty::str("<"), pretty::str(name.clone())];

        if !props.is_empty() {
            if !multiline_props {
                for prop in &props {
                    parts.push(pretty::str(" "));
                    parts.push(self.fmt_jsx_prop(prop));
                }
            } else {
                let mut prop_inner = Vec::new();
                for prop in &props {
                    prop_inner.push(pretty::line());
                    prop_inner.push(self.fmt_jsx_prop(prop));
                }
                parts.push(pretty::nest(4, pretty::concat(prop_inner)));
                parts.push(pretty::line());
            }
        }

        if is_self_closing {
            parts.push(pretty::str(" />"));
            return pretty::concat(parts);
        }

        parts.push(pretty::str(">"));

        if children.is_empty() {
            parts.push(pretty::str("</"));
            parts.push(pretty::str(name));
            parts.push(pretty::str(">"));
            return pretty::concat(parts);
        }

        let inline = children.len() == 1
            && !multiline_props
            && match &children[0] {
                JsxChildInfo::Text(_) | JsxChildInfo::Comment(_) => true,
                JsxChildInfo::Expr(n) => !self.jsx_expr_is_multiline_node(n),
                JsxChildInfo::Element(_) => false,
            };

        if inline {
            parts.push(self.fmt_jsx_children_inline(&children));
        } else {
            parts.push(pretty::nest(4, self.fmt_jsx_children_block(&children)));
            parts.push(pretty::line());
        }

        parts.push(pretty::str("</"));
        parts.push(pretty::str(name));
        parts.push(pretty::str(">"));

        pretty::concat(parts)
    }

    fn fmt_jsx_prop(&mut self, node: &SyntaxNode) -> Document {
        // JSX spread prop: {...expr}
        if node.kind() == SyntaxKind::JSX_SPREAD_PROP {
            let mut parts = vec![pretty::str("{...")];
            let mut past_dots = false;
            for child_or_tok in node.children_with_tokens() {
                if child_or_tok
                    .as_token()
                    .is_some_and(|t| t.kind() == SyntaxKind::DOT_DOT_DOT)
                {
                    past_dots = true;
                    continue;
                }
                if !past_dots {
                    continue;
                }
                match child_or_tok {
                    rowan::NodeOrToken::Token(tok) => {
                        if tok.kind() == SyntaxKind::R_BRACE {
                            break;
                        }
                        if !tok.kind().is_trivia() {
                            parts.push(pretty::str(tok.text()));
                        }
                    }
                    rowan::NodeOrToken::Node(child) => parts.push(self.fmt_node(&child)),
                }
            }
            parts.push(pretty::str("}"));
            return pretty::concat(parts);
        }

        // JSX prop name: identifier, keyword, or hyphenated (aria-label, data-testid)
        let mut parts = Vec::new();
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                let kind = tok.kind();
                if kind == SyntaxKind::EQUAL {
                    break;
                }
                if kind.is_trivia() {
                    continue;
                }
                if kind == SyntaxKind::MINUS || kind.is_member_name() {
                    parts.push(pretty::str(tok.text()));
                } else {
                    break;
                }
            }
        }

        let has_eq = self.has_token(node, SyntaxKind::EQUAL);
        if !has_eq {
            return pretty::concat(parts);
        }

        parts.push(pretty::str("="));

        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        if has_lbrace {
            parts.push(pretty::str("{"));
            let mut inside = false;
            for child_or_tok in node.children_with_tokens() {
                match child_or_tok {
                    rowan::NodeOrToken::Token(tok) => {
                        if tok.kind() == SyntaxKind::L_BRACE {
                            inside = true;
                            continue;
                        }
                        if tok.kind() == SyntaxKind::R_BRACE {
                            break;
                        }
                        if inside && !tok.kind().is_trivia() {
                            parts.push(pretty::str(tok.text()));
                        }
                    }
                    rowan::NodeOrToken::Node(child) => {
                        if inside {
                            parts.push(self.fmt_node(&child));
                        }
                    }
                }
            }
            parts.push(pretty::str("}"));
        } else {
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token()
                    && tok.kind() == SyntaxKind::STRING
                {
                    parts.push(pretty::str(tok.text()));
                    break;
                }
            }
        }
        pretty::concat(parts)
    }

    fn fmt_jsx_children_inline(&mut self, children: &[JsxChildInfo]) -> Document {
        let mut parts = Vec::new();
        for child in children {
            match child {
                JsxChildInfo::Text(text) => parts.push(pretty::str(text.trim())),
                JsxChildInfo::Expr(node) => parts.push(self.fmt_jsx_expr_child(node)),
                JsxChildInfo::Element(node) => parts.push(self.fmt_jsx(node)),
                JsxChildInfo::Comment(text) => {
                    parts.push(pretty::str("{"));
                    parts.push(pretty::str(text.clone()));
                    parts.push(pretty::str("}"));
                }
            }
        }
        pretty::concat(parts)
    }

    fn fmt_jsx_children_block(&mut self, children: &[JsxChildInfo]) -> Document {
        let mut parts = Vec::new();
        for child in children {
            match child {
                JsxChildInfo::Text(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(pretty::line());
                        parts.push(pretty::str(trimmed));
                    }
                }
                JsxChildInfo::Expr(node) => {
                    parts.push(pretty::line());
                    parts.push(self.fmt_jsx_expr_child(node));
                }
                JsxChildInfo::Element(node) => {
                    parts.push(pretty::line());
                    parts.push(self.fmt_jsx(node));
                }
                JsxChildInfo::Comment(text) => {
                    parts.push(pretty::line());
                    parts.push(pretty::str("{"));
                    parts.push(pretty::str(text.clone()));
                    parts.push(pretty::str("}"));
                }
            }
        }
        pretty::concat(parts)
    }

    fn fmt_jsx_expr_child(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("{")];
        let mut inside = false;
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::L_BRACE {
                        inside = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::R_BRACE {
                        break;
                    }
                    if inside && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if inside {
                        parts.push(self.fmt_node(&child));
                    }
                }
            }
        }
        parts.push(pretty::str("}"));
        pretty::concat(parts)
    }

    fn jsx_tag_name(&self, node: &SyntaxNode) -> Option<String> {
        crate::syntax::jsx_tag_name_from_node(node)
    }

    fn jsx_has_children(&self, node: &SyntaxNode) -> bool {
        node.children().any(|c| {
            matches!(
                c.kind(),
                SyntaxKind::JSX_ELEMENT | SyntaxKind::JSX_EXPR_CHILD | SyntaxKind::JSX_TEXT
            )
        })
    }

    fn jsx_collect_children(&self, node: &SyntaxNode) -> Vec<JsxChildInfo> {
        let mut children = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChildInfo::Text(text));
                    }
                }
                SyntaxKind::JSX_EXPR_CHILD => {
                    if let Some(comment) = self.jsx_expr_child_comment(&child) {
                        children.push(JsxChildInfo::Comment(comment));
                    } else {
                        children.push(JsxChildInfo::Expr(child));
                    }
                }
                SyntaxKind::JSX_ELEMENT => {
                    children.push(JsxChildInfo::Element(child));
                }
                _ => {}
            }
        }
        children
    }

    /// Heuristic: an expression child is multiline if it contains a match,
    /// a block, or a nested JSX element with multiline props.
    fn jsx_expr_is_multiline_node(&self, node: &SyntaxNode) -> bool {
        node.descendants().any(|d| match d.kind() {
            SyntaxKind::MATCH_EXPR | SyntaxKind::BLOCK_EXPR => true,
            SyntaxKind::JSX_ELEMENT => self.jsx_has_multiline_props(&d),
            _ => false,
        })
    }

    fn jsx_expr_child_comment(&self, node: &SyntaxNode) -> Option<String> {
        let mut comment = None;
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::BLOCK_COMMENT {
                        comment = Some(tok.text().to_string());
                    } else if !tok.kind().is_trivia()
                        && tok.kind() != SyntaxKind::L_BRACE
                        && tok.kind() != SyntaxKind::R_BRACE
                    {
                        return None;
                    }
                }
                rowan::NodeOrToken::Node(_) => return None,
            }
        }
        comment
    }

    pub(crate) fn jsx_has_multiline_props(&self, node: &SyntaxNode) -> bool {
        let props: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::JSX_PROP || c.kind() == SyntaxKind::JSX_SPREAD_PROP)
            .collect();
        !(props.is_empty() || (props.len() <= 3 && self.jsx_props_short(&props)))
    }

    fn jsx_props_short(&self, props: &[SyntaxNode]) -> bool {
        let total: usize = props
            .iter()
            .map(|p| {
                let range = p.text_range();
                let len: usize = (range.end() - range.start()).into();
                len
            })
            .sum();
        total < 60
    }
}
