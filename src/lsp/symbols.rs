use tower_lsp::lsp_types::*;

use crate::parser::ast::*;

pub(super) fn symbol_kind_to_completion(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
        SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
        SymbolKind::VARIABLE => CompletionItemKind::VARIABLE,
        SymbolKind::TYPE_PARAMETER => CompletionItemKind::CLASS,
        SymbolKind::ENUM_MEMBER => CompletionItemKind::ENUM_MEMBER,
        _ => CompletionItemKind::TEXT,
    }
}

/// A symbol defined in a document.
#[derive(Debug, Clone)]
pub(super) struct Symbol {
    pub(super) name: String,
    pub(super) kind: SymbolKind,
    /// Byte offset range in the source
    pub(super) start: usize,
    pub(super) end: usize,
    /// The source module for imported symbols
    pub(super) import_source: Option<String>,
    /// Type signature for hover
    pub(super) detail: String,
    /// For functions: the type of the first parameter (for pipe-aware completions)
    pub(super) first_param_type: Option<String>,
    /// For functions: the return type (for pipe chain type inference)
    #[allow(dead_code)]
    pub(super) return_type_str: Option<String>,
}

/// Index of all symbols in a document.
#[derive(Debug, Clone, Default)]
pub(super) struct SymbolIndex {
    /// All defined/imported symbols
    pub(super) symbols: Vec<Symbol>,
}

impl SymbolIndex {
    pub(super) fn build(program: &Program) -> Self {
        let mut symbols = Vec::new();
        Self::collect_items(&program.items, &mut symbols);
        Self { symbols }
    }

    fn collect_items(items: &[Item], symbols: &mut Vec<Symbol>) {
        for item in items {
            match &item.kind {
                ItemKind::Import(decl) => {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_ref().unwrap_or(&spec.name);
                        symbols.push(Symbol {
                            name: name.clone(),
                            kind: SymbolKind::VARIABLE,
                            start: spec.span.start,
                            end: spec.span.end,
                            import_source: Some(decl.source.clone()),
                            detail: format!("import {{ {} }} from \"{}\"", spec.name, decl.source),
                            first_param_type: None,
                            return_type_str: None,
                        });
                    }
                }
                ItemKind::Const(decl) => {
                    let name = match &decl.binding {
                        ConstBinding::Name(n) => n.clone(),
                        ConstBinding::Array(names) => format!("[{}]", names.join(", ")),
                        ConstBinding::Object(names) => format!("{{ {} }}", names.join(", ")),
                        ConstBinding::Tuple(names) => format!("({})", names.join(", ")),
                    };
                    let vis = if decl.exported { "export " } else { "" };
                    let type_ann = decl
                        .type_ann
                        .as_ref()
                        .map(|t| format!(": {}", type_expr_to_string(t)))
                        .unwrap_or_default();
                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::CONSTANT,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!("{vis}const {name}{type_ann}"),
                        first_param_type: None,
                        return_type_str: None,
                    });

                    // Also index destructured names
                    match &decl.binding {
                        ConstBinding::Array(names)
                        | ConstBinding::Object(names)
                        | ConstBinding::Tuple(names) => {
                            for n in names {
                                symbols.push(Symbol {
                                    name: n.clone(),
                                    kind: SymbolKind::VARIABLE,
                                    start: item.span.start,
                                    end: item.span.end,
                                    import_source: None,
                                    detail: format!("const {{ {n} }}"),
                                    first_param_type: None,
                                    return_type_str: None,
                                });
                            }
                        }
                        ConstBinding::Name(_) => {}
                    }
                }
                ItemKind::Function(decl) => {
                    let vis = if decl.exported { "export " } else { "" };
                    let async_kw = if decl.async_fn { "async " } else { "" };
                    let params: Vec<String> = decl
                        .params
                        .iter()
                        .map(|p| {
                            if let Some(ty) = &p.type_ann {
                                format!("{}: {}", p.name, type_expr_to_string(ty))
                            } else {
                                p.name.clone()
                            }
                        })
                        .collect();
                    let ret = decl
                        .return_type
                        .as_ref()
                        .map(|t| format!(" -> {}", type_expr_to_string(t)))
                        .unwrap_or_default();

                    // Extract first param type for pipe-aware completions
                    let first_param_type = decl
                        .params
                        .first()
                        .and_then(|p| p.type_ann.as_ref())
                        .map(type_expr_to_string);

                    let return_type_str = decl.return_type.as_ref().map(type_expr_to_string);

                    symbols.push(Symbol {
                        name: decl.name.clone(),
                        kind: SymbolKind::FUNCTION,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!(
                            "{vis}{async_kw}fn {}({}){ret}",
                            decl.name,
                            params.join(", ")
                        ),
                        first_param_type,
                        return_type_str,
                    });

                    // Index parameters
                    for param in &decl.params {
                        let type_ann = param
                            .type_ann
                            .as_ref()
                            .map(|t| format!(": {}", type_expr_to_string(t)))
                            .unwrap_or_default();
                        symbols.push(Symbol {
                            name: param.name.clone(),
                            kind: SymbolKind::VARIABLE,
                            start: param.span.start,
                            end: param.span.end,
                            import_source: None,
                            detail: format!("parameter {}{type_ann}", param.name),
                            first_param_type: None,
                            return_type_str: None,
                        });
                    }

                    // Recurse into function body
                    Self::collect_expr(&decl.body, symbols);
                }
                ItemKind::TypeDecl(decl) => {
                    let vis = if decl.exported { "export " } else { "" };
                    let opaque = if decl.opaque { "opaque " } else { "" };
                    symbols.push(Symbol {
                        name: decl.name.clone(),
                        kind: SymbolKind::TYPE_PARAMETER,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!("{vis}{opaque}type {}", decl.name),
                        first_param_type: None,
                        return_type_str: None,
                    });

                    // Index union variants
                    if let TypeDef::Union(variants) = &decl.def {
                        for variant in variants {
                            symbols.push(Symbol {
                                name: variant.name.clone(),
                                kind: SymbolKind::ENUM_MEMBER,
                                start: variant.span.start,
                                end: variant.span.end,
                                import_source: None,
                                detail: format!("{}.{}", decl.name, variant.name),
                                first_param_type: None,
                                return_type_str: None,
                            });
                        }
                    }
                }
                ItemKind::ForBlock(block) => {
                    let type_str = type_expr_to_string(&block.type_name);
                    for func in &block.functions {
                        let params: Vec<String> = func
                            .params
                            .iter()
                            .map(|p| {
                                if p.name == "self" {
                                    format!("self: {type_str}")
                                } else if let Some(ty) = &p.type_ann {
                                    format!("{}: {}", p.name, type_expr_to_string(ty))
                                } else {
                                    p.name.clone()
                                }
                            })
                            .collect();
                        let ret = func
                            .return_type
                            .as_ref()
                            .map(|t| format!(" -> {}", type_expr_to_string(t)))
                            .unwrap_or_default();

                        // First param type is the for block's type (for self params)
                        let first_param_type =
                            if func.params.first().is_some_and(|p| p.name == "self") {
                                Some(type_str.clone())
                            } else {
                                func.params
                                    .first()
                                    .and_then(|p| p.type_ann.as_ref())
                                    .map(type_expr_to_string)
                            };

                        let return_type_str = func.return_type.as_ref().map(type_expr_to_string);

                        symbols.push(Symbol {
                            name: func.name.clone(),
                            kind: SymbolKind::FUNCTION,
                            start: block.span.start,
                            end: block.span.end,
                            import_source: None,
                            detail: format!("fn {}({}){ret}", func.name, params.join(", "),),
                            first_param_type,
                            return_type_str,
                        });

                        Self::collect_expr(&func.body, symbols);
                    }
                }
                ItemKind::TestBlock(_) => {
                    // Test blocks don't contribute symbols
                }
                ItemKind::Expr(expr) => {
                    Self::collect_expr(expr, symbols);
                }
            }
        }
    }

    /// Walk an expression tree to find symbols inside blocks, arrows, etc.
    fn collect_expr(expr: &Expr, symbols: &mut Vec<Symbol>) {
        match &expr.kind {
            ExprKind::Block(items) => {
                Self::collect_items(items, symbols);
            }
            ExprKind::Arrow { body, .. } => {
                Self::collect_expr(body, symbols);
            }
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    Self::collect_expr(&arm.body, symbols);
                }
            }
            ExprKind::Return(Some(inner)) | ExprKind::Await(inner) | ExprKind::Grouped(inner) => {
                Self::collect_expr(inner, symbols);
            }
            _ => {}
        }
    }

    /// Add symbols for imported for-block functions from resolved imports.
    /// These don't appear in the current file's AST but are defined via cross-file resolution.
    pub(super) fn add_imported_for_blocks(
        &mut self,
        resolved_imports: &std::collections::HashMap<String, crate::resolve::ResolvedImports>,
    ) {
        for (source, resolved) in resolved_imports {
            for block in &resolved.for_blocks {
                let type_str = type_expr_to_string(&block.type_name);
                for func in &block.functions {
                    let params: Vec<String> = func
                        .params
                        .iter()
                        .map(|p| {
                            if p.name == "self" {
                                format!("self: {type_str}")
                            } else if let Some(ty) = &p.type_ann {
                                format!("{}: {}", p.name, type_expr_to_string(ty))
                            } else {
                                p.name.clone()
                            }
                        })
                        .collect();
                    let ret = func
                        .return_type
                        .as_ref()
                        .map(|t| format!(": {}", type_expr_to_string(t)))
                        .unwrap_or_default();

                    let first_param_type = if func.params.first().is_some_and(|p| p.name == "self")
                    {
                        Some(type_str.clone())
                    } else {
                        func.params
                            .first()
                            .and_then(|p| p.type_ann.as_ref())
                            .map(type_expr_to_string)
                    };

                    let return_type_str = func.return_type.as_ref().map(type_expr_to_string);

                    self.symbols.push(Symbol {
                        name: func.name.clone(),
                        kind: SymbolKind::FUNCTION,
                        start: 0,
                        end: 0,
                        import_source: Some(source.clone()),
                        detail: format!(
                            "fn {}({}){} (from \"{}\")",
                            func.name,
                            params.join(", "),
                            ret,
                            source
                        ),
                        first_param_type,
                        return_type_str,
                    });
                }
            }
        }
    }

    pub(super) fn find_by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    pub(super) fn all_completions(&self) -> Vec<&Symbol> {
        self.symbols.iter().collect()
    }
}

pub(super) fn type_expr_to_string(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named { name, type_args } => {
            if type_args.is_empty() {
                name.clone()
            } else {
                let args: Vec<String> = type_args.iter().map(type_expr_to_string).collect();
                format!("{}<{}>", name, args.join(", "))
            }
        }
        TypeExprKind::Record(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_expr_to_string(&f.type_ann)))
                .collect();
            format!("{{ {} }}", fs.join(", "))
        }
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            let ps: Vec<String> = params.iter().map(type_expr_to_string).collect();
            format!(
                "({}) -> {}",
                ps.join(", "),
                type_expr_to_string(return_type)
            )
        }
        TypeExprKind::Array(inner) => format!("Array<{}>", type_expr_to_string(inner)),
        TypeExprKind::Tuple(parts) => {
            let ps: Vec<String> = parts.iter().map(type_expr_to_string).collect();
            format!("({})", ps.join(", "))
        }
    }
}
