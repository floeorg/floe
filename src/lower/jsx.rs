use super::*;

impl<'src> Lowerer<'src> {
    pub(super) fn lower_jsx_element(&mut self, node: &SyntaxNode) -> Option<JsxElement> {
        let span = self.node_span(node);

        // Detect fragment: no tag name idents
        let idents = self.collect_idents_direct(node);

        if idents.is_empty() {
            // Fragment
            let children = self.lower_jsx_children(node);
            return Some(JsxElement {
                kind: JsxElementKind::Fragment { children },
                span,
            });
        }

        let name = idents.first()?.clone();
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
                    if let Some(element) = self.lower_jsx_element(&child) {
                        children.push(JsxChild::Element(element));
                    }
                }
                _ => {}
            }
        }

        Some(JsxElement {
            kind: JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            },
            span,
        })
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
                    if let Some(element) = self.lower_jsx_element(&child) {
                        children.push(JsxChild::Element(element));
                    }
                }
                _ => {}
            }
        }
        children
    }

    pub(super) fn lower_jsx_prop(&mut self, node: &SyntaxNode) -> Option<JsxProp> {
        let span = self.node_span(node);
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let value = if self.has_token(node, SyntaxKind::EQUAL) {
            self.lower_expr_after_eq(node)
        } else {
            None
        };

        Some(JsxProp::Named { name, value, span })
    }

    pub(super) fn lower_jsx_spread_prop(&mut self, node: &SyntaxNode) -> Option<JsxProp> {
        let span = self.node_span(node);
        let expr = self.lower_first_expr(node)?;

        Some(JsxProp::Spread { expr, span })
    }
}
