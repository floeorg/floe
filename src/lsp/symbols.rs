use tower_lsp::lsp_types::*;

use crate::parser::ast::*;

pub(super) fn symbol_kind_to_completion(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
        SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
        SymbolKind::VARIABLE => CompletionItemKind::VARIABLE,
        SymbolKind::TYPE_PARAMETER => CompletionItemKind::CLASS,
        SymbolKind::ENUM_MEMBER => CompletionItemKind::ENUM_MEMBER,
        SymbolKind::INTERFACE => CompletionItemKind::INTERFACE,
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
    /// For properties: the parent type name (e.g., "User" for User.name)
    pub(super) owner_type: Option<String>,
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
                            owner_type: None,
                        });
                    }
                }
                ItemKind::Const(decl) => {
                    let name = match &decl.binding {
                        ConstBinding::Name(n) => n.clone(),
                        ConstBinding::Array(names) => format!("[{}]", names.join(", ")),
                        ConstBinding::Object(fields) => format!(
                            "{{ {} }}",
                            fields
                                .iter()
                                .map(|f| {
                                    match &f.alias {
                                        Some(a) => format!("{}: {}", f.field, a),
                                        None => f.field.clone(),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
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
                        owner_type: None,
                    });

                    // Also index destructured names
                    match &decl.binding {
                        ConstBinding::Array(names) | ConstBinding::Tuple(names) => {
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
                                    owner_type: None,
                                });
                            }
                        }
                        ConstBinding::Object(fields) => {
                            for f in fields {
                                let n = f.bound_name();
                                symbols.push(Symbol {
                                    name: n.to_string(),
                                    kind: SymbolKind::VARIABLE,
                                    start: item.span.start,
                                    end: item.span.end,
                                    import_source: None,
                                    detail: format!("const {{ {n} }}"),
                                    first_param_type: None,
                                    return_type_str: None,
                                    owner_type: None,
                                });
                            }
                        }
                        ConstBinding::Name(_) => {}
                    }

                    Self::collect_expr(&decl.value, symbols);
                }
                ItemKind::Function(decl) => {
                    let vis = if decl.exported { "export " } else { "" };
                    let async_kw = if decl.async_fn { "async " } else { "" };
                    let params: Vec<String> = decl.params.iter().map(format_param).collect();
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

                    let type_params = if decl.type_params.is_empty() {
                        String::new()
                    } else {
                        format!("<{}>", decl.type_params.join(", "))
                    };

                    symbols.push(Symbol {
                        name: decl.name.clone(),
                        kind: SymbolKind::FUNCTION,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!(
                            "{vis}{async_kw}fn {}{type_params}({}){ret}",
                            decl.name,
                            params.join(", ")
                        ),
                        first_param_type,
                        return_type_str,
                        owner_type: None,
                    });

                    for param in &decl.params {
                        Self::push_param_symbol(param, symbols);
                    }

                    // Recurse into function body
                    Self::collect_expr(&decl.body, symbols);
                }
                ItemKind::TypeDecl(decl) => {
                    let vis = if decl.exported { "export " } else { "" };
                    let opaque = if decl.opaque { "opaque " } else { "" };
                    let type_params = if decl.type_params.is_empty() {
                        String::new()
                    } else {
                        format!("<{}>", decl.type_params.join(", "))
                    };

                    // Build detailed type body for hover
                    let body = match &decl.def {
                        TypeDef::Record(entries) => {
                            let members: Vec<String> = entries
                                .iter()
                                .map(|e| match e {
                                    RecordEntry::Field(f) => {
                                        format!(
                                            "    {}: {}",
                                            f.name,
                                            type_expr_to_string(&f.type_ann)
                                        )
                                    }
                                    RecordEntry::Spread(s) => {
                                        if let Some(ref type_expr) = s.type_expr {
                                            format!("    ...{}", type_expr_to_string(type_expr))
                                        } else {
                                            format!("    ...{}", s.type_name)
                                        }
                                    }
                                })
                                .collect();
                            if members.is_empty() {
                                " {}".to_string()
                            } else {
                                format!(" {{\n{},\n}}", members.join(",\n"))
                            }
                        }
                        TypeDef::Union(variants) => {
                            let vs: Vec<String> = variants
                                .iter()
                                .map(|v| {
                                    if v.fields.is_empty() {
                                        format!("    | {}", v.name)
                                    } else {
                                        let fs: Vec<String> = v
                                            .fields
                                            .iter()
                                            .map(|f| {
                                                if let Some(name) = &f.name {
                                                    format!(
                                                        "{}: {}",
                                                        name,
                                                        type_expr_to_string(&f.type_ann)
                                                    )
                                                } else {
                                                    type_expr_to_string(&f.type_ann)
                                                }
                                            })
                                            .collect();
                                        format!("    | {}({})", v.name, fs.join(", "))
                                    }
                                })
                                .collect();
                            format!("\n{}", vs.join("\n"))
                        }
                        TypeDef::Alias(ty) => {
                            format!(" = {}", type_expr_to_string(ty))
                        }
                        TypeDef::StringLiteralUnion(variants) => {
                            let vs: Vec<String> =
                                variants.iter().map(|v| format!("\"{}\"", v)).collect();
                            format!(" = {}", vs.join(" | "))
                        }
                    };

                    symbols.push(Symbol {
                        name: decl.name.clone(),
                        kind: SymbolKind::TYPE_PARAMETER,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!("{vis}{opaque}type {}{type_params}{body}", decl.name),
                        first_param_type: None,
                        return_type_str: None,
                        owner_type: None,
                    });

                    // Index record fields
                    if let TypeDef::Record(entries) = &decl.def {
                        for entry in entries {
                            if let Some(field) = entry.as_field() {
                                symbols.push(Symbol {
                                    name: field.name.clone(),
                                    kind: SymbolKind::PROPERTY,
                                    start: field.span.start,
                                    end: field.span.end,
                                    import_source: None,
                                    detail: format!(
                                        "(property) {}: {}",
                                        field.name,
                                        type_expr_to_string(&field.type_ann)
                                    ),
                                    first_param_type: None,
                                    return_type_str: None,
                                    owner_type: Some(decl.name.clone()),
                                });
                            }
                        }
                    }

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
                                owner_type: None,
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
                            owner_type: None,
                        });

                        // Index `self` parameter so hover works on it
                        for param in &func.params {
                            if param.name == "self" {
                                symbols.push(Symbol {
                                    name: "self".to_string(),
                                    kind: SymbolKind::VARIABLE,
                                    start: param.span.start,
                                    end: param.span.end,
                                    import_source: None,
                                    detail: format!("self: {type_str}"),
                                    first_param_type: None,
                                    return_type_str: None,
                                    owner_type: None,
                                });
                            } else {
                                Self::push_param_symbol(param, symbols);
                            }
                        }

                        Self::collect_expr(&func.body, symbols);
                    }
                }
                ItemKind::TraitDecl(decl) => {
                    let vis = if decl.exported { "export " } else { "" };
                    symbols.push(Symbol {
                        name: decl.name.clone(),
                        kind: SymbolKind::INTERFACE,
                        start: item.span.start,
                        end: item.span.end,
                        import_source: None,
                        detail: format!("{vis}trait {}", decl.name),
                        first_param_type: None,
                        return_type_str: None,
                        owner_type: None,
                    });

                    // Index trait methods
                    for method in &decl.methods {
                        let params: Vec<String> = method
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
                        let ret = method
                            .return_type
                            .as_ref()
                            .map(|t| format!(" -> {}", type_expr_to_string(t)))
                            .unwrap_or_default();

                        symbols.push(Symbol {
                            name: method.name.clone(),
                            kind: SymbolKind::FUNCTION,
                            start: method.span.start,
                            end: method.span.end,
                            import_source: None,
                            detail: format!(
                                "{}.fn {}({}){ret}",
                                decl.name,
                                method.name,
                                params.join(", ")
                            ),
                            first_param_type: None,
                            return_type_str: method.return_type.as_ref().map(type_expr_to_string),
                            owner_type: None,
                        });

                        // Recurse into default method bodies
                        if let Some(body) = &method.body {
                            Self::collect_expr(body, symbols);
                        }
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
            ExprKind::Arrow { params, body, .. } => {
                for param in params {
                    Self::push_param_symbol(param, symbols);
                }
                Self::collect_expr(body, symbols);
            }
            ExprKind::Match { arms, .. } => {
                for arm in arms {
                    Self::collect_pattern_bindings(&arm.pattern, symbols);
                    Self::collect_expr(&arm.body, symbols);
                }
            }
            ExprKind::Call { callee, args, .. } => {
                Self::collect_expr(callee, symbols);
                Self::collect_args(args, symbols);
            }
            ExprKind::Construct { args, .. }
            | ExprKind::Mock {
                overrides: args, ..
            } => {
                Self::collect_args(args, symbols);
            }
            ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
                Self::collect_expr(left, symbols);
                Self::collect_expr(right, symbols);
            }
            ExprKind::Grouped(inner)
            | ExprKind::Unwrap(inner)
            | ExprKind::Unary { operand: inner, .. }
            | ExprKind::Spread(inner)
            | ExprKind::Value(inner) => {
                Self::collect_expr(inner, symbols);
            }
            ExprKind::Array(items) | ExprKind::Tuple(items) => {
                for item in items {
                    Self::collect_expr(item, symbols);
                }
            }
            ExprKind::Index { object, index } => {
                Self::collect_expr(object, symbols);
                Self::collect_expr(index, symbols);
            }
            ExprKind::Member { object, .. } => {
                Self::collect_expr(object, symbols);
            }
            ExprKind::Collect(items) => {
                Self::collect_items(items, symbols);
            }
            ExprKind::Jsx(element) => {
                Self::collect_jsx(element, symbols);
            }
            _ => {}
        }
    }

    /// Walk match patterns to index binding names and literals for hover.
    fn collect_pattern_bindings(pattern: &crate::parser::ast::Pattern, symbols: &mut Vec<Symbol>) {
        match &pattern.kind {
            PatternKind::Literal(lit) => {
                let (name, ty) = match lit {
                    LiteralPattern::Bool(b) => (b.to_string(), "boolean"),
                    LiteralPattern::Number(n) => (n.clone(), "number"),
                    LiteralPattern::String(s) => (format!("\"{s}\""), "string"),
                };
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::CONSTANT,
                    start: pattern.span.start,
                    end: pattern.span.end,
                    import_source: None,
                    detail: format!("(pattern) {ty}"),
                    first_param_type: None,
                    return_type_str: None,
                    owner_type: None,
                });
            }
            PatternKind::Binding(name) if name != "_" => {
                symbols.push(Symbol {
                    name: name.clone(),
                    kind: SymbolKind::VARIABLE,
                    start: pattern.span.start,
                    end: pattern.span.end,
                    import_source: None,
                    detail: format!("binding {name}"),
                    first_param_type: None,
                    return_type_str: None,
                    owner_type: None,
                });
            }
            PatternKind::Variant { fields, .. } => {
                for field in fields {
                    Self::collect_pattern_bindings(field, symbols);
                }
            }
            PatternKind::Record { fields, .. } => {
                for (_, field) in fields {
                    Self::collect_pattern_bindings(field, symbols);
                }
            }
            PatternKind::Array { elements, rest } => {
                for elem in elements {
                    Self::collect_pattern_bindings(elem, symbols);
                }
                if let Some(rest_name) = rest {
                    symbols.push(Symbol {
                        name: rest_name.clone(),
                        kind: SymbolKind::VARIABLE,
                        start: pattern.span.start,
                        end: pattern.span.end,
                        import_source: None,
                        detail: format!("binding {rest_name}"),
                        first_param_type: None,
                        return_type_str: None,
                        owner_type: None,
                    });
                }
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
                        owner_type: None,
                    });
                }
            }
        }
    }

    fn collect_args(args: &[Arg], symbols: &mut Vec<Symbol>) {
        for arg in args {
            match arg {
                Arg::Positional(e) | Arg::Named { value: e, .. } => {
                    Self::collect_expr(e, symbols);
                }
            }
        }
    }

    fn collect_jsx(element: &JsxElement, symbols: &mut Vec<Symbol>) {
        if let JsxElementKind::Element { props, .. } = &element.kind {
            for prop in props {
                match prop {
                    JsxProp::Named { value: Some(e), .. } | JsxProp::Spread { expr: e, .. } => {
                        Self::collect_expr(e, symbols);
                    }
                    _ => {}
                }
            }
        }
        let children = match &element.kind {
            JsxElementKind::Element { children, .. } | JsxElementKind::Fragment { children } => {
                children
            }
        };
        for child in children {
            match child {
                JsxChild::Expr(e) => Self::collect_expr(e, symbols),
                JsxChild::Element(el) => Self::collect_jsx(el, symbols),
                JsxChild::Text(_) => {}
            }
        }
    }

    fn push_param_symbol(param: &Param, symbols: &mut Vec<Symbol>) {
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
            owner_type: None,
        });
    }

    pub(super) fn find_by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    pub(super) fn all_completions(&self) -> Vec<&Symbol> {
        self.symbols.iter().collect()
    }
}

/// Convert a simple expression to a short display string for default values.
fn expr_to_short_string(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Number(n) => n.clone(),
        ExprKind::String(s) => format!("\"{}\"", s),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::Array(items) if items.is_empty() => "[]".to_string(),
        ExprKind::Unary { op, operand } => {
            let op_str = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
            };
            format!("{}{}", op_str, expr_to_short_string(operand))
        }
        _ => "...".to_string(),
    }
}

/// Format a function parameter including type annotation and default value.
fn format_param(p: &Param) -> String {
    let mut s = p.name.clone();
    if let Some(ty) = &p.type_ann {
        s.push_str(&format!(": {}", type_expr_to_string(ty)));
    }
    if let Some(default) = &p.default {
        s.push_str(&format!(" = {}", expr_to_short_string(default)));
    }
    s
}

pub(super) fn type_expr_to_string(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
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
        TypeExprKind::TypeOf(name) => format!("typeof {name}"),
        TypeExprKind::Intersection(types) => {
            let parts: Vec<String> = types.iter().map(type_expr_to_string).collect();
            parts.join(" & ")
        }
        TypeExprKind::StringLiteral(value) => format!("\"{value}\""),
    }
}
