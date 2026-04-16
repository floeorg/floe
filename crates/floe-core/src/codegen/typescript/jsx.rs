use crate::parser::ast::*;
use crate::pretty::{self, Document};

use super::generator::TypeScriptGenerator;

impl<'a> TypeScriptGenerator<'a> {
    // ── JSX ──────────────────────────────────────────────────────

    pub(super) fn emit_jsx(&mut self, element: &TypedJsxElement) -> Document {
        match &element.kind {
            JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            } => {
                let mut docs = vec![pretty::str(format!("<{name}"))];
                for prop in props {
                    docs.push(pretty::str(" "));
                    match prop {
                        JsxProp::Named { name, value, .. } => {
                            docs.push(pretty::str(name));
                            if let Some(value) = value {
                                docs.push(pretty::str("={"));
                                docs.push(self.emit_expr(value));
                                docs.push(pretty::str("}"));
                            }
                        }
                        JsxProp::Spread { expr, .. } => {
                            docs.push(pretty::str("{..."));
                            docs.push(self.emit_expr(expr));
                            docs.push(pretty::str("}"));
                        }
                    }
                }
                if *self_closing {
                    docs.push(pretty::str(" />"));
                } else {
                    docs.push(pretty::str(">"));
                    docs.push(self.emit_jsx_children(children));
                    docs.push(pretty::str(format!("</{name}>")));
                }
                pretty::concat(docs)
            }
            JsxElementKind::Fragment { children } => pretty::concat([
                pretty::str("<>"),
                self.emit_jsx_children(children),
                pretty::str("</>"),
            ]),
        }
    }

    fn emit_jsx_children(&mut self, children: &[TypedJsxChild]) -> Document {
        let mut docs = Vec::new();
        for child in children {
            match child {
                JsxChild::Text(text) => docs.push(pretty::str(text)),
                JsxChild::Expr(expr) => {
                    docs.push(pretty::str("{"));
                    docs.push(self.emit_expr(expr));
                    docs.push(pretty::str("}"));
                }
                JsxChild::Element(element) => docs.push(self.emit_jsx(element)),
            }
        }
        pretty::concat(docs)
    }
}
