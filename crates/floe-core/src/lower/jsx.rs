use super::{JsxChild, JsxElement, JsxElementKind, JsxProp, Lowerer, SyntaxKind, SyntaxNode};

impl<'src> Lowerer<'src> {
    pub(super) fn lower_jsx_element(&mut self, node: &SyntaxNode) -> JsxElement {
        let span = self.node_span(node);

        let name = crate::syntax::jsx_tag_name_from_node(node);

        if name.is_none() {
            let children = self.lower_jsx_children(node);
            return JsxElement {
                kind: JsxElementKind::Fragment { children },
                span,
            };
        }

        let name = name.unwrap();
        // Self-closing: SLASH appears right before GREATER_THAN (not after LESS_THAN)
        let self_closing = {
            let mut prev_was_slash = false;
            let mut found = false;
            for token in node.children_with_tokens() {
                if let Some(token) = token.as_token() {
                    if token.kind() == SyntaxKind::SLASH {
                        prev_was_slash = true;
                    } else if token.kind() == SyntaxKind::GREATER_THAN && prev_was_slash {
                        found = true;
                        break;
                    } else if !token.kind().is_trivia() {
                        prev_was_slash = false;
                    }
                }
            }
            // Only truly self-closing if there are no children (JSX_EXPR_CHILD, JSX_TEXT, JSX_ELEMENT)
            found
                && !node.children().any(|c| {
                    matches!(
                        c.kind(),
                        SyntaxKind::JSX_EXPR_CHILD | SyntaxKind::JSX_TEXT | SyntaxKind::JSX_ELEMENT
                    )
                })
        };

        let mut props = Vec::new();
        let mut children = Vec::new();

        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_PROP => {
                    if let Some(prop) = self.lower_jsx_prop(&child) {
                        props.push(prop);
                    }
                }
                SyntaxKind::JSX_SPREAD_PROP => {
                    if let Some(prop) = self.lower_jsx_spread_prop(&child) {
                        props.push(prop);
                    }
                }
                SyntaxKind::JSX_EXPR_CHILD => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        children.push(JsxChild::Expr(expr));
                    }
                }
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChild::Text(text.trim().to_string()));
                    }
                }
                SyntaxKind::JSX_ELEMENT => {
                    let element = self.lower_jsx_element(&child);
                    children.push(JsxChild::Element(element));
                }
                _ => {}
            }
        }

        JsxElement {
            kind: JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            },
            span,
        }
    }

    pub(super) fn lower_jsx_children(&mut self, node: &SyntaxNode) -> Vec<JsxChild> {
        let mut children = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_EXPR_CHILD => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        children.push(JsxChild::Expr(expr));
                    }
                }
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChild::Text(text.trim().to_string()));
                    }
                }
                SyntaxKind::JSX_ELEMENT => {
                    let element = self.lower_jsx_element(&child);
                    children.push(JsxChild::Element(element));
                }
                _ => {}
            }
        }
        children
    }

    pub(super) fn lower_jsx_prop(&mut self, node: &SyntaxNode) -> Option<JsxProp> {
        let span = self.node_span(node);
        // Reconstruct the full prop name including hyphens (e.g., aria-label, data-testid)
        let name = self.collect_jsx_prop_name(node)?;

        let value = if self.has_token(node, SyntaxKind::EQUAL) {
            self.lower_expr_after_eq(node)
        } else {
            None
        };

        Some(JsxProp::Named { name, value, span })
    }

    /// Collect the full JSX prop name, joining ident/keyword tokens with hyphens.
    /// e.g., `aria-label` → "aria-label", `data-testid` → "data-testid"
    #[allow(clippy::unused_self)]
    fn collect_jsx_prop_name(&self, node: &SyntaxNode) -> Option<String> {
        let mut name = String::new();
        for child in node.children_with_tokens() {
            if let Some(tok) = child.as_token() {
                let kind = tok.kind();
                if kind == SyntaxKind::EQUAL {
                    break;
                }
                if kind.is_trivia() {
                    continue;
                }
                if kind == SyntaxKind::MINUS || kind.is_member_name() {
                    name.push_str(tok.text());
                } else {
                    break;
                }
            }
        }
        if name.is_empty() { None } else { Some(name) }
    }

    pub(super) fn lower_jsx_spread_prop(&mut self, node: &SyntaxNode) -> Option<JsxProp> {
        let span = self.node_span(node);
        let expr = self.lower_first_expr(node)?;

        Some(JsxProp::Spread { expr, span })
    }
}
