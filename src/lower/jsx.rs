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
        let self_closing = self.has_token(node, SyntaxKind::SLASH);

        let mut props = Vec::new();
        let mut children = Vec::new();

        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_PROP => {
                    if let Some(prop) = self.lower_jsx_prop(&child) {
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

        let value = self.lower_first_expr(node);

        Some(JsxProp { name, value, span })
    }
}
