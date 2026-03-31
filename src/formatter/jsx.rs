use crate::syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, JsxChildInfo};

impl Formatter<'_> {
    pub(crate) fn fmt_jsx(&mut self, node: &SyntaxNode) {
        let tag_name = self.jsx_tag_name(node);
        let is_fragment = tag_name.is_none();
        let is_self_closing =
            self.has_token(node, SyntaxKind::SLASH) && !self.jsx_has_children(node);

        let props: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::JSX_PROP || c.kind() == SyntaxKind::JSX_SPREAD_PROP)
            .collect();

        let children = self.jsx_collect_children(node);

        if is_fragment {
            self.write("<>");
            if children.is_empty() {
                self.write("</>");
                return;
            }
            let frag_inline = children.len() == 1
                && match &children[0] {
                    JsxChildInfo::Text(_) | JsxChildInfo::Comment(_) => true,
                    JsxChildInfo::Expr(node) => !self.jsx_expr_is_multiline(node),
                    JsxChildInfo::Element(_) => false,
                };
            if frag_inline {
                self.fmt_jsx_children_inline(&children);
            } else {
                self.indent += 1;
                self.fmt_jsx_children(&children);
                self.indent -= 1;
                self.newline();
                self.write_indent();
            }
            self.write("</>");
            return;
        }

        let name = tag_name.unwrap();

        // Opening tag
        self.write("<");
        self.write(&name);

        // Props
        let multiline_props =
            !(props.is_empty() || props.len() <= 3 && self.jsx_props_short(&props));

        if !props.is_empty() {
            if !multiline_props {
                for prop in &props {
                    self.write(" ");
                    self.fmt_jsx_prop(prop);
                }
            } else {
                self.indent += 1;
                for prop in &props {
                    self.newline();
                    self.write_indent();
                    self.fmt_jsx_prop(prop);
                }
                self.indent -= 1;
                self.newline();
                self.write_indent();
            }
        }

        if is_self_closing {
            self.write(" />");
            return;
        }

        self.write(">");

        if children.is_empty() {
            self.write("</");
            self.write(&name);
            self.write(">");
            return;
        }

        // Single text, expr, or comment child → inline, unless:
        // - The opening tag has multi-line props
        // - The expr child contains multi-line content (e.g., match expressions)
        let inline = children.len() == 1
            && !multiline_props
            && match &children[0] {
                JsxChildInfo::Text(_) | JsxChildInfo::Comment(_) => true,
                JsxChildInfo::Expr(node) => !self.jsx_expr_is_multiline(node),
                JsxChildInfo::Element(_) => false,
            };

        if inline {
            self.fmt_jsx_children_inline(&children);
        } else {
            self.indent += 1;
            self.fmt_jsx_children(&children);
            self.indent -= 1;
            self.newline();
            self.write_indent();
        }

        self.write("</");
        self.write(&name);
        self.write(">");
    }

    fn fmt_jsx_prop(&mut self, node: &SyntaxNode) {
        // JSX spread prop: {...expr}
        if node.kind() == SyntaxKind::JSX_SPREAD_PROP {
            self.write("{...");
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
                            self.write(tok.text());
                        }
                    }
                    rowan::NodeOrToken::Node(child) => self.fmt_node(&child),
                }
            }
            self.write("}");
            return;
        }

        // JSX prop names can be identifiers or keywords (e.g., `type`, `for`)
        if let Some(name) = self.first_ident(node) {
            self.write(&name);
        } else {
            // Check for keyword tokens used as prop names
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token()
                    && !tok.kind().is_trivia()
                    && tok.kind() != SyntaxKind::EQUAL
                {
                    self.write(tok.text());
                    break;
                }
            }
        }

        let has_eq = self.has_token(node, SyntaxKind::EQUAL);
        if !has_eq {
            return;
        }

        self.write("=");

        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        if has_lbrace {
            self.write("{");
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
                            self.write(tok.text());
                        }
                    }
                    rowan::NodeOrToken::Node(child) => {
                        if inside {
                            self.fmt_node(&child);
                        }
                    }
                }
            }
            self.write("}");
        } else {
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token()
                    && tok.kind() == SyntaxKind::STRING
                {
                    self.write(tok.text());
                    break;
                }
            }
        }
    }

    fn fmt_jsx_children_inline(&mut self, children: &[JsxChildInfo]) {
        for child in children {
            match child {
                JsxChildInfo::Text(text) => {
                    self.write(text.trim());
                }
                JsxChildInfo::Expr(node) => {
                    self.fmt_jsx_expr_child(node);
                }
                JsxChildInfo::Element(node) => {
                    self.fmt_jsx(node);
                }
                JsxChildInfo::Comment(text) => {
                    self.write("{");
                    self.write(text);
                    self.write("}");
                }
            }
        }
    }

    fn fmt_jsx_children(&mut self, children: &[JsxChildInfo]) {
        for child in children {
            match child {
                JsxChildInfo::Text(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        self.newline();
                        self.write_indent();
                        self.write(trimmed);
                    }
                }
                JsxChildInfo::Expr(node) => {
                    self.newline();
                    self.write_indent();
                    self.fmt_jsx_expr_child(node);
                }
                JsxChildInfo::Element(node) => {
                    self.newline();
                    self.write_indent();
                    self.fmt_jsx(node);
                }
                JsxChildInfo::Comment(text) => {
                    self.newline();
                    self.write_indent();
                    self.write("{");
                    self.write(text);
                    self.write("}");
                }
            }
        }
    }

    fn fmt_jsx_expr_child(&mut self, node: &SyntaxNode) {
        self.write("{");
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
                        self.write(tok.text());
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if inside {
                        self.fmt_node(&child);
                    }
                }
            }
        }
        self.write("}");
    }

    fn jsx_tag_name(&self, node: &SyntaxNode) -> Option<String> {
        let mut past_lt = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::LESS_THAN {
                    past_lt = true;
                    continue;
                }
                if past_lt && tok.kind() == SyntaxKind::IDENT {
                    return Some(tok.text().to_string());
                }
                if past_lt && !tok.kind().is_trivia() {
                    return None;
                }
            }
        }
        None
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

    /// Check if a JSX_EXPR_CHILD contains multi-line content (e.g., a match expression,
    /// or a pipe/call with a multiline arrow body).
    fn jsx_expr_is_multiline(&self, node: &SyntaxNode) -> bool {
        let inline = self.try_inline(|f| f.fmt_jsx_expr_child(node));
        inline.contains('\n')
    }

    /// If a `JSX_EXPR_CHILD` contains only a block comment (no real expression),
    /// return the comment text (e.g. `/* Desktop */`).
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

    /// Check if a JSX element will be formatted with multi-line props.
    pub(crate) fn jsx_has_multiline_props(&self, node: &SyntaxNode) -> bool {
        let props: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::JSX_PROP || c.kind() == SyntaxKind::JSX_SPREAD_PROP)
            .collect();
        !(props.is_empty() || props.len() <= 3 && self.jsx_props_short(&props))
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
