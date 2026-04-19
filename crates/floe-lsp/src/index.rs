//! Symbol indexing driven off the typed AST.
//!
//! `SymbolIndex::build` walks a `TypedProgram` and records each
//! declaration, import, binding, and record field as a `Symbol`. The
//! checker's resolved `name_types` / `name_type_map` maps are folded in
//! at build time so `Symbol.detail` is the final hover string and
//! `first_param_type` is the resolved `Type`.

use std::collections::HashMap;
use std::sync::Arc;

use tower_lsp::lsp_types::*;

use floe_core::checker::{Type, simple_resolve_type_expr};
use floe_core::parser::ast::*;

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
    /// First parameter's resolved `Type`. Consulted by pipe-aware
    /// completion ranking to decide whether a function can sit on the
    /// right of `|>` given the piped expression's type.
    pub(super) first_param_type: Option<Arc<Type>>,
    /// For properties: the parent type name (e.g., "User" for User.name)
    pub(super) owner_type: Option<String>,
    /// For `ENUM_MEMBER` variants: shape of the declared field list. Drives
    /// match-arm completion to insert the right snippet — `Variant`, `Variant(..)`,
    /// or `Variant { .. }` — instead of always plain `Variant`.
    pub(super) variant_shape: Option<VariantShapeHint>,
}

/// Summary of a variant declaration's field list, used by LSP completion.
#[derive(Debug, Clone)]
pub(super) enum VariantShapeHint {
    /// Unit variant: no fields — `Variant`.
    Unit,
    /// Positional fields: `Variant(Type1, Type2)` — suggests `Variant($1, $2)`.
    Positional(usize),
    /// Named fields: `Variant { f1: Type1, f2: Type2 }` — suggests
    /// `Variant { f1, f2 }`.
    Named(Vec<String>),
}

/// Index of all symbols in a document.
#[derive(Debug, Clone, Default)]
pub(super) struct SymbolIndex {
    /// All defined/imported symbols
    pub(super) symbols: Vec<Symbol>,
}

impl SymbolIndex {
    /// Walk the typed program, then fold in the checker's resolved
    /// names so each `Symbol` carries its final hover string and
    /// pipe-compat `Type` without a second pass at query time.
    pub(super) fn build(
        program: &TypedProgram,
        name_types: &HashMap<String, String>,
        name_type_map: &HashMap<String, Arc<Type>>,
    ) -> Self {
        let mut symbols = Vec::new();
        collect_items(&program.items, &mut symbols);
        for sym in &mut symbols {
            enrich_symbol(sym, name_types, name_type_map);
        }
        Self { symbols }
    }

    /// Add symbols for imported for-block functions from resolved imports.
    /// These don't appear in the current file's AST but are defined via cross-file resolution.
    pub(super) fn add_imported_for_blocks(
        &mut self,
        resolved_imports: &HashMap<String, floe_core::resolve::ResolvedImports>,
    ) {
        for (source, resolved) in resolved_imports {
            for block in &resolved.for_blocks {
                for func in &block.functions {
                    self.symbols.push(for_block_function_symbol(
                        func,
                        &block.type_name,
                        0,
                        0,
                        Some(source.clone()),
                    ));
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

fn collect_items(items: &[TypedItem], symbols: &mut Vec<Symbol>) {
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
                        owner_type: None,
                        variant_shape: None,
                    });
                }
            }
            ItemKind::Const(decl) => {
                let name = match &decl.binding {
                    ConstBinding::Name(n) => n.clone(),
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
                    detail: format!("{vis}let {name}{type_ann}"),
                    first_param_type: None,
                    owner_type: None,
                    variant_shape: None,
                });

                // Also index destructured names
                match &decl.binding {
                    ConstBinding::Tuple(names) => {
                        for n in names {
                            symbols.push(Symbol {
                                name: n.clone(),
                                kind: SymbolKind::VARIABLE,
                                start: item.span.start,
                                end: item.span.end,
                                import_source: None,
                                detail: format!("let {{ {n} }}"),
                                first_param_type: None,
                                owner_type: None,
                                variant_shape: None,
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
                                detail: format!("let {{ {n} }}"),
                                first_param_type: None,
                                owner_type: None,
                                variant_shape: None,
                            });
                        }
                    }
                    ConstBinding::Name(_) => {}
                }

                collect_expr(&decl.value, symbols);
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

                let type_params = if decl.type_params.is_empty() {
                    String::new()
                } else {
                    let parts: Vec<String> = decl
                        .type_params
                        .iter()
                        .map(|tp| {
                            if tp.bounds.is_empty() {
                                tp.name.clone()
                            } else {
                                format!("{}: {}", tp.name, tp.bounds.join(" + "))
                            }
                        })
                        .collect();
                    format!("<{}>", parts.join(", "))
                };

                symbols.push(Symbol {
                    name: decl.name.clone(),
                    kind: SymbolKind::FUNCTION,
                    start: item.span.start,
                    end: item.span.end,
                    import_source: None,
                    detail: format!(
                        "{vis}{async_kw}let {}{type_params}({}){ret}",
                        decl.name,
                        params.join(", ")
                    ),
                    first_param_type: None,
                    owner_type: None,
                    variant_shape: None,
                });

                for param in &decl.params {
                    push_param_symbol(param, symbols);
                }

                // Recurse into function body
                collect_expr(&decl.body, symbols);
            }
            ItemKind::TypeDecl(decl) => {
                let vis = if decl.exported { "export " } else { "" };
                let opaque = if decl.opaque { "opaque " } else { "" };
                let type_params = if decl.type_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", decl.type_params.join(", "))
                };

                let (body, kind_tag) = match &decl.def {
                    TypeDef::Record(entries) => {
                        let members: Vec<String> = entries
                            .iter()
                            .map(|e| match e {
                                RecordEntry::Field(f) => {
                                    format!("    {}: {}", f.name, type_expr_to_string(&f.type_ann))
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
                        let body = if members.is_empty() {
                            " = {}".to_string()
                        } else {
                            format!(" = {{\n{},\n}}", members.join(",\n"))
                        };
                        (body, "[record — nominal]".to_string())
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
                        let body = format!(" =\n{}", vs.join("\n"));
                        let tag = if variants.len() == 1 && variants[0].name == decl.name {
                            "[newtype — nominal]".to_string()
                        } else {
                            format!("[sum — nominal, {} variants]", variants.len())
                        };
                        (body, tag)
                    }
                    TypeDef::Alias(ty) => {
                        let body = format!(" = {}", type_expr_to_string(ty));
                        (body, alias_kind_tag(ty))
                    }
                    TypeDef::StringLiteralUnion(variants) => {
                        let vs: Vec<String> =
                            variants.iter().map(|v| format!("\"{}\"", v)).collect();
                        let body = format!(" = {}", vs.join(" | "));
                        (body, "[union — structural]".to_string())
                    }
                };

                symbols.push(Symbol {
                    name: decl.name.clone(),
                    kind: SymbolKind::TYPE_PARAMETER,
                    start: item.span.start,
                    end: item.span.end,
                    import_source: None,
                    detail: format!(
                        "{vis}{opaque}type {}{type_params}{body}\n{kind_tag}",
                        decl.name
                    ),
                    first_param_type: None,
                    owner_type: None,
                    variant_shape: None,
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
                                owner_type: Some(decl.name.clone()),
                                variant_shape: None,
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
                            owner_type: None,
                            variant_shape: Some(variant_shape_from_decl(&variant.fields)),
                        });
                    }
                }
            }
            ItemKind::ForBlock(block) => {
                let type_str = type_expr_to_string(&block.type_name);
                for func in &block.functions {
                    symbols.push(for_block_function_symbol(
                        func,
                        &block.type_name,
                        block.span.start,
                        block.span.end,
                        None,
                    ));

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
                                owner_type: None,
                                variant_shape: None,
                            });
                        } else {
                            push_param_symbol(param, symbols);
                        }
                    }

                    collect_expr(&func.body, symbols);
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
                    owner_type: None,
                    variant_shape: None,
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
                            "{}.let {}({}){ret}",
                            decl.name,
                            method.name,
                            params.join(", ")
                        ),
                        first_param_type: None,
                        owner_type: None,
                        variant_shape: None,
                    });

                    // Recurse into default method bodies
                    if let Some(body) = &method.body {
                        collect_expr(body, symbols);
                    }
                }
            }
            ItemKind::ReExport(_) | ItemKind::TestBlock(_) => {
                // Re-exports and test blocks don't contribute symbols
            }
            ItemKind::Expr(expr) => {
                collect_expr(expr, symbols);
            }
        }
    }
}

/// Walk an expression tree to find symbols inside blocks, arrows, etc.
fn collect_expr(expr: &TypedExpr, symbols: &mut Vec<Symbol>) {
    match &expr.kind {
        ExprKind::Block(items) => {
            collect_items(items, symbols);
        }
        ExprKind::Arrow { params, body, .. } => {
            for param in params {
                push_param_symbol(param, symbols);
            }
            collect_expr(body, symbols);
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                collect_pattern_bindings(&arm.pattern, symbols);
                collect_expr(&arm.body, symbols);
            }
        }
        ExprKind::Call { callee, args, .. } => {
            collect_expr(callee, symbols);
            collect_args(args, symbols);
        }
        ExprKind::TaggedTemplate { tag, parts } => {
            collect_expr(tag, symbols);
            for part in parts {
                if let floe_core::parser::ast::TemplatePart::Expr(e) = part {
                    collect_expr(e, symbols);
                }
            }
        }
        ExprKind::Construct { args, .. }
        | ExprKind::Mock {
            overrides: args, ..
        } => {
            collect_args(args, symbols);
        }
        ExprKind::Pipe { left, right } | ExprKind::Binary { left, right, .. } => {
            collect_expr(left, symbols);
            collect_expr(right, symbols);
        }
        ExprKind::Grouped(inner)
        | ExprKind::Unwrap(inner)
        | ExprKind::Unary { operand: inner, .. }
        | ExprKind::Spread(inner)
        | ExprKind::Value(inner) => {
            collect_expr(inner, symbols);
        }
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_expr(item, symbols);
            }
        }
        ExprKind::Index { object, index } => {
            collect_expr(object, symbols);
            collect_expr(index, symbols);
        }
        ExprKind::Member { object, .. } => {
            collect_expr(object, symbols);
        }
        ExprKind::Collect(items) => {
            collect_items(items, symbols);
        }
        ExprKind::Jsx(element) => {
            collect_jsx(element, symbols);
        }
        _ => {}
    }
}

/// Walk match patterns to index binding names and literals for hover.
fn collect_pattern_bindings(pattern: &floe_core::parser::ast::Pattern, symbols: &mut Vec<Symbol>) {
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
                owner_type: None,
                variant_shape: None,
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
                owner_type: None,
                variant_shape: None,
            });
        }
        PatternKind::Variant { fields, .. } => {
            for field in fields.patterns() {
                collect_pattern_bindings(field, symbols);
            }
        }
        PatternKind::Record { fields, .. } => {
            for (_, field) in fields {
                collect_pattern_bindings(field, symbols);
            }
        }
        PatternKind::Array { elements, rest } => {
            for elem in elements {
                collect_pattern_bindings(elem, symbols);
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
                    owner_type: None,
                    variant_shape: None,
                });
            }
        }
        _ => {}
    }
}

fn collect_args(args: &[TypedArg], symbols: &mut Vec<Symbol>) {
    for arg in args {
        match arg {
            Arg::Positional(e) | Arg::Named { value: e, .. } => {
                collect_expr(e, symbols);
            }
        }
    }
}

fn collect_jsx(element: &TypedJsxElement, symbols: &mut Vec<Symbol>) {
    if let JsxElementKind::Element { props, .. } = &element.kind {
        for prop in props {
            match prop {
                JsxProp::Named { value: Some(e), .. } | JsxProp::Spread { expr: e, .. } => {
                    collect_expr(e, symbols);
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
            JsxChild::Expr(e) => collect_expr(e, symbols),
            JsxChild::Element(el) => collect_jsx(el, symbols),
            JsxChild::Text(_) => {}
        }
    }
}

fn push_param_symbol(param: &TypedParam, symbols: &mut Vec<Symbol>) {
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
        owner_type: None,
        variant_shape: None,
    });
}

/// Convert a simple expression to a short display string for default values.
fn expr_to_short_string(expr: &TypedExpr) -> String {
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
fn format_param(p: &TypedParam) -> String {
    let mut s = p.name.clone();
    if let Some(ty) = &p.type_ann {
        s.push_str(&format!(": {}", type_expr_to_string(ty)));
    }
    if let Some(default) = &p.default {
        s.push_str(&format!(" = {}", expr_to_short_string(default)));
    }
    s
}

pub(super) fn alias_kind_tag<T>(ty: &TypeExpr<T>) -> String {
    match &ty.kind {
        TypeExprKind::Named { name, .. } if name == floe_core::type_layout::TYPE_ONE_OF => {
            "[union — structural]".to_string()
        }
        TypeExprKind::Named { name, .. } if name == floe_core::type_layout::TYPE_INTERSECT => {
            "[intersection — structural]".to_string()
        }
        TypeExprKind::Function { .. } => "[function — structural]".to_string(),
        TypeExprKind::Intersection(_) => "[intersection — structural]".to_string(),
        _ => "[alias — structural]".to_string(),
    }
}

pub(super) fn type_expr_to_string<T>(ty: &TypeExpr<T>) -> String {
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

/// Build the function symbol for a for-block entry. Used both when
/// walking an in-file `for Type { ... }` block and when injecting
/// cross-file for-blocks via `add_imported_for_blocks` — both paths share
/// the same formatting and type-resolution logic.
fn for_block_function_symbol<T>(
    func: &FunctionDecl<T>,
    block_type: &TypeExpr<T>,
    start: usize,
    end: usize,
    import_source: Option<String>,
) -> Symbol {
    let block_type_str = type_expr_to_string(block_type);
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| {
            if p.name == "self" {
                format!("self: {block_type_str}")
            } else if let Some(ty) = &p.type_ann {
                format!("{}: {}", p.name, type_expr_to_string(ty))
            } else {
                p.name.clone()
            }
        })
        .collect();

    // Same-file for-blocks use `fn name(...) => R`; cross-file imports
    // render as `fn name(...): R (from "source")` to match the TS-import
    // style hover. The two sites chose different separators historically
    // — keep that contract rather than changing user-visible hover output.
    let (ret_sep, source_suffix) = match &import_source {
        Some(src) => (": ", format!(" (from \"{src}\")")),
        None => (" -> ", String::new()),
    };
    let ret = func
        .return_type
        .as_ref()
        .map(|t| format!("{ret_sep}{}", type_expr_to_string(t)))
        .unwrap_or_default();

    let first_param_type = if func.params.first().is_some_and(|p| p.name == "self") {
        Some(Arc::new(simple_resolve_type_expr(block_type)))
    } else {
        func.params
            .first()
            .and_then(|p| p.type_ann.as_ref())
            .map(|t| Arc::new(simple_resolve_type_expr(t)))
    };

    Symbol {
        name: func.name.clone(),
        kind: SymbolKind::FUNCTION,
        start,
        end,
        import_source,
        detail: format!(
            "let {}({}){ret}{source_suffix}",
            func.name,
            params.join(", ")
        ),
        first_param_type,
        owner_type: None,
        variant_shape: None,
    }
}

fn enrich_symbol(
    sym: &mut Symbol,
    name_types: &HashMap<String, String>,
    name_type_map: &HashMap<String, Arc<Type>>,
) {
    // Detail string: imports render from scratch with the resolved type,
    // consts/variables append their inferred type when annotation was
    // omitted, functions pick up the inferred return type similarly.
    if let Some(source) = sym.import_source.clone() {
        if let Some(inferred) = name_types.get(&sym.name)
            && !inferred.contains("?T")
            && inferred != "unknown"
            && inferred != &sym.name
        {
            sym.detail = format!("(import) {}: {inferred}\nfrom \"{source}\"", sym.name);
        } else {
            sym.detail = format!("(type) {}\nfrom \"{source}\"", sym.name);
        }
    } else if sym.kind == SymbolKind::CONSTANT || sym.kind == SymbolKind::VARIABLE {
        if let Some(inferred) = name_types.get(&sym.name)
            && !inferred.contains("?T")
            && !sym.detail.contains(':')
        {
            sym.detail = format!("{}: {inferred}", sym.detail);
        }
    } else if sym.kind == SymbolKind::FUNCTION
        && !sym.detail.contains("->")
        && let Some(inferred) = name_types.get(&sym.name)
        && let Some((_, ret)) = inferred
            .rsplit_once(" -> ")
            .or_else(|| inferred.rsplit_once(" -> "))
        && !ret.contains("?T")
    {
        sym.detail = format!("{} -> {ret}", sym.detail);
    }

    // Typed fields — only meaningful for functions, and only if the
    // checker saw the full signature (for-block / trait / cross-file
    // symbols are already populated by `for_block_function_symbol`).
    if sym.kind == SymbolKind::FUNCTION
        && sym.first_param_type.is_none()
        && let Some(ty) = name_type_map.get(&sym.name)
        && let Type::Function { params, .. } = ty.as_ref()
    {
        sym.first_param_type = params.first().cloned().map(Arc::new);
    }
}

/// Summarize a variant's declared fields into a shape hint. Empty field list
/// is a unit variant; all-named means a brace-form variant; otherwise
/// positional. The parser rejects mixed forms, so we don't have to handle
/// that case here.
fn variant_shape_from_decl<T>(fields: &[VariantField<T>]) -> VariantShapeHint {
    if fields.is_empty() {
        return VariantShapeHint::Unit;
    }
    let names: Vec<String> = fields.iter().filter_map(|f| f.name.clone()).collect();
    if names.len() == fields.len() {
        VariantShapeHint::Named(names)
    } else {
        VariantShapeHint::Positional(fields.len())
    }
}
