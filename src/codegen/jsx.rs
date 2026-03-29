use crate::parser::ast::*;

use super::Codegen;

impl Codegen {
    // ── JSX ──────────────────────────────────────────────────────

    pub(super) fn emit_jsx(&mut self, element: &JsxElement) {
        match &element.kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            } => {
                self.push(&format!("<{name}"));
                for prop in props {
                    self.push(" ");
                    match prop {
                        JsxProp::Named { name, value, .. } => {
                            self.push(name);
                            if let Some(value) = value {
                                self.push("={");
                                self.emit_expr(value);
                                self.push("}");
                            }
                        }
                        JsxProp::Spread { expr, .. } => {
                            self.push("{...");
                            self.emit_expr(expr);
                            self.push("}");
                        }
                    }
                }
                if *self_closing {
                    self.push(" />");
                } else {
                    self.push(">");
                    self.emit_jsx_children(children);
                    self.push(&format!("</{name}>"));
                }
            }
            JsxElementKind::Fragment { children } => {
                self.push("<>");
                self.emit_jsx_children(children);
                self.push("</>");
            }
        }
    }

    fn emit_jsx_children(&mut self, children: &[JsxChild]) {
        for child in children {
            match child {
                JsxChild::Text(text) => self.push(text),
                JsxChild::Expr(expr) => {
                    self.push("{");
                    self.emit_expr(expr);
                    self.push("}");
                }
                JsxChild::Element(element) => self.emit_jsx(element),
            }
        }
    }
}
