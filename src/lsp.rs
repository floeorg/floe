use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::checker::Checker;
use crate::diagnostic::{self as zs_diag, Severity};
use crate::interop;
use crate::parser::Parser;
use crate::parser::ast::*;

// ── Helpers ─────────────────────────────────────────────────────

fn symbol_kind_to_completion(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::FUNCTION => CompletionItemKind::FUNCTION,
        SymbolKind::CONSTANT => CompletionItemKind::CONSTANT,
        SymbolKind::VARIABLE => CompletionItemKind::VARIABLE,
        SymbolKind::TYPE_PARAMETER => CompletionItemKind::CLASS,
        SymbolKind::ENUM_MEMBER => CompletionItemKind::ENUM_MEMBER,
        _ => CompletionItemKind::TEXT,
    }
}

// ── Symbol Index ────────────────────────────────────────────────

/// A symbol defined in a document.
#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    kind: SymbolKind,
    /// Byte offset range in the source
    start: usize,
    end: usize,
    /// The source module for imported symbols
    import_source: Option<String>,
    /// Type signature for hover
    detail: String,
    /// For functions: the type of the first parameter (for pipe-aware completions)
    first_param_type: Option<String>,
    /// For functions: the return type (for pipe chain type inference)
    #[allow(dead_code)]
    return_type_str: Option<String>,
}

/// Index of all symbols in a document.
#[derive(Debug, Clone, Default)]
struct SymbolIndex {
    /// All defined/imported symbols
    symbols: Vec<Symbol>,
}

impl SymbolIndex {
    fn build(program: &Program) -> Self {
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
                        ConstBinding::Array(names) | ConstBinding::Object(names) => {
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
                        .map(|t| format!(": {}", type_expr_to_string(t)))
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
                            "{vis}{async_kw}function {}({}){ret}",
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
            ExprKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                Self::collect_expr(then_branch, symbols);
                if let Some(eb) = else_branch {
                    Self::collect_expr(eb, symbols);
                }
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

    fn find_by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    fn all_completions(&self) -> Vec<&Symbol> {
        self.symbols.iter().collect()
    }
}

fn type_expr_to_string(ty: &TypeExpr) -> String {
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
                "({}) => {}",
                ps.join(", "),
                type_expr_to_string(return_type)
            )
        }
        TypeExprKind::Array(inner) => format!("Array<{}>", type_expr_to_string(inner)),
        TypeExprKind::Tuple(parts) => {
            let ps: Vec<String> = parts.iter().map(type_expr_to_string).collect();
            format!("[{}]", ps.join(", "))
        }
    }
}

// ── Pipe-Aware Completions ──────────────────────────────────────

/// Detect if the cursor is in a pipe context (after `|>`).
/// Returns true if we find `|>` before the cursor (ignoring whitespace).
fn is_pipe_context(source: &str, offset: usize) -> bool {
    let before = &source[..offset];
    let trimmed = before.trim_end();
    trimmed.ends_with("|>")
}

/// Extract the base type name from a full type string.
/// e.g., "Array<User>" -> "Array", "Option<string>" -> "Option", "string" -> "string"
fn base_type_name(type_str: &str) -> &str {
    match type_str.find('<') {
        Some(pos) => &type_str[..pos],
        None => type_str,
    }
}

/// Try to resolve the type of the expression being piped.
/// Looks at the text before `|>` and tries to determine its type using the type map.
fn resolve_piped_type(
    source: &str,
    offset: usize,
    type_map: &HashMap<String, String>,
) -> Option<String> {
    let before = &source[..offset];
    let trimmed = before.trim_end();
    // Strip the `|>` suffix
    let before_pipe = trimmed.strip_suffix("|>")?;
    let before_pipe = before_pipe.trim_end();

    // Check for `?` unwrap at the end
    let (expr_text, unwrap) = if let Some(inner) = before_pipe.strip_suffix('?') {
        (inner.trim_end(), true)
    } else {
        (before_pipe, false)
    };

    // Try to find the last identifier or call expression
    let ident = extract_trailing_identifier(expr_text);

    if ident.is_empty() {
        // Try literal type inference
        return infer_literal_type(expr_text);
    }

    // Look up the identifier in the type map
    let type_str = type_map.get(ident)?;
    let resolved = if unwrap {
        unwrap_type(type_str)
    } else {
        type_str.clone()
    };
    Some(resolved)
}

/// Extract the trailing identifier from an expression string.
/// e.g., "users" -> "users", "getUsers()" -> "getUsers", "a.b.c" -> "c"
fn extract_trailing_identifier(s: &str) -> &str {
    let s = s.trim_end();
    // Strip trailing () for function calls
    let s = if s.ends_with(')') {
        // Find matching open paren
        let mut depth = 0;
        let mut paren_start = s.len();
        for (i, c) in s.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        paren_start = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        &s[..paren_start]
    } else {
        s
    };

    // Extract last identifier (after `.` or standalone)
    let start = s
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    &s[start..]
}

/// Infer the type of a literal expression.
fn infer_literal_type(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('"') || s.starts_with('`') {
        Some("string".to_string())
    } else if s == "true" || s == "false" {
        Some("bool".to_string())
    } else if s.starts_with('[') {
        Some("Array".to_string())
    } else if s.parse::<f64>().is_ok() {
        Some("number".to_string())
    } else {
        None
    }
}

/// Unwrap a Result or Option type: Result<T, E> -> T, Option<T> -> T
fn unwrap_type(type_str: &str) -> String {
    if let Some(inner) = type_str.strip_prefix("Result<") {
        // Result<T, E> -> T (first type arg)
        if let Some(comma_pos) = find_top_level_comma(inner) {
            return inner[..comma_pos].to_string();
        }
    }
    if let Some(inner) = type_str.strip_prefix("Option<") {
        // Option<T> -> T
        if let Some(end) = inner.strip_suffix('>') {
            return end.to_string();
        }
    }
    type_str.to_string()
}

/// Find the position of the first top-level comma (not inside angle brackets).
fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Check if a function's first parameter type is compatible with the piped type.
/// Uses base type name matching: "Array<User>" matches "Array<T>", etc.
fn is_pipe_compatible(fn_first_param: &str, piped_type: &str) -> bool {
    let fn_base = base_type_name(fn_first_param);
    let piped_base = base_type_name(piped_type);

    // Exact base type match
    if fn_base == piped_base {
        return true;
    }

    // Generic type parameter (single uppercase letter like T, U, A) matches anything
    if fn_base.len() == 1
        && fn_base
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
    {
        return true;
    }

    false
}

// ── Module Resolution (lightweight, no tsc) ────────────────────

/// Resolve an npm package specifier to its .d.ts file path.
/// Walks node_modules looking for package.json types/typings field.
fn resolve_npm_dts(specifier: &str, project_dir: &Path) -> Option<PathBuf> {
    // Walk up directories looking for node_modules
    let mut dir = project_dir.to_path_buf();
    loop {
        let pkg_dir = dir.join("node_modules").join(specifier);
        if pkg_dir.is_dir() {
            // Check package.json for types/typings field
            let pkg_json = pkg_dir.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_json)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
            {
                // Try "types", then "typings"
                for field in &["types", "typings"] {
                    if let Some(types_path) = json.get(field).and_then(|v| v.as_str()) {
                        let full = pkg_dir.join(types_path);
                        if full.exists() {
                            return Some(full);
                        }
                    }
                }
            }
            // Fallback: index.d.ts
            let index_dts = pkg_dir.join("index.d.ts");
            if index_dts.exists() {
                return Some(index_dts);
            }
        }

        // Also check @types/<pkg>
        let at_types = dir.join("node_modules").join("@types").join(specifier);
        if at_types.is_dir() {
            let index_dts = at_types.join("index.d.ts");
            if index_dts.exists() {
                return Some(index_dts);
            }
        }

        if !dir.pop() {
            break;
        }
    }
    None
}

/// Resolve a relative import to an actual file path.
/// Checks .zs, .ts, .tsx extensions and /index variants.
fn resolve_relative_import(specifier: &str, source_dir: &Path) -> Option<PathBuf> {
    let base = source_dir.join(specifier);
    for ext in &[".zs", ".ts", ".tsx", "/index.zs", "/index.ts", "/index.tsx"] {
        let path = PathBuf::from(format!("{}{}", base.display(), ext));
        if path.exists() {
            return Some(path);
        }
    }
    // Maybe it already has an extension
    if base.exists() && base.is_file() {
        return Some(base);
    }
    None
}

/// Enrich a symbol index with type info from resolved .d.ts files.
/// Also returns diagnostics for unresolvable relative imports.
fn enrich_from_imports(
    program: &Program,
    project_dir: &Path,
    source_dir: &Path,
    index: &mut SymbolIndex,
    dts_cache: &HashMap<String, Vec<interop::DtsExport>>,
) -> (
    Vec<zs_diag::Diagnostic>,
    HashMap<String, Vec<interop::DtsExport>>,
) {
    let mut import_diags = Vec::new();
    let mut new_cache = HashMap::new();

    for item in &program.items {
        let ItemKind::Import(decl) = &item.kind else {
            continue;
        };

        let specifier = &decl.source;
        let is_relative = specifier.starts_with("./") || specifier.starts_with("../");

        if is_relative {
            // Validate relative imports exist
            if resolve_relative_import(specifier, source_dir).is_none() {
                import_diags.push(
                    zs_diag::Diagnostic::error(
                        format!("cannot find module \"{}\"", specifier),
                        item.span,
                    )
                    .with_label("module not found")
                    .with_help("Check the file path and extension")
                    .with_code("E012"),
                );
            }
            continue;
        }

        // npm package — try to resolve .d.ts
        let exports = if let Some(cached) = dts_cache.get(specifier) {
            cached.clone()
        } else if let Some(dts_path) = resolve_npm_dts(specifier, project_dir) {
            match interop::parse_dts_exports(&dts_path) {
                Ok(exports) => exports,
                Err(_) => continue,
            }
        } else {
            continue;
        };

        new_cache.insert(specifier.clone(), exports.clone());

        // Enrich imported symbols with type info from .d.ts
        for sym in &mut index.symbols {
            if sym.import_source.as_deref() != Some(specifier) {
                continue;
            }
            // Find matching export
            if let Some(dts_export) = exports.iter().find(|e| e.name == sym.name) {
                let type_str = interop::ts_type_to_string(&dts_export.ts_type);
                sym.detail = format!("{} (from \"{}\")", type_str, specifier);

                // If it's a function export, extract first param and return type
                if let interop::TsType::Function {
                    params,
                    return_type,
                } = &dts_export.ts_type
                {
                    sym.kind = SymbolKind::FUNCTION;
                    sym.first_param_type = params.first().map(interop::ts_type_to_string);
                    sym.return_type_str = Some(interop::ts_type_to_string(return_type));
                }
            }
        }
    }

    (import_diags, new_cache)
}

// ── Document State ──────────────────────────────────────────────

/// State for an open document.
#[derive(Debug, Clone)]
struct Document {
    content: String,
    index: SymbolIndex,
    /// Type map from the checker: variable/function name -> inferred type display name
    type_map: HashMap<String, String>,
}

/// The ZenScript Language Server.
pub struct ZenScriptLsp {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
    /// Cache of resolved .d.ts exports per module specifier
    dts_cache: Arc<RwLock<HashMap<String, Vec<interop::DtsExport>>>>,
}

impl ZenScriptLsp {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            dts_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse and type-check a document, update symbol index, publish diagnostics.
    async fn update_document(&self, uri: Url, source: &str) {
        let (diagnostics, index, type_map) = match Parser::new(source).parse_program() {
            Err(parse_errors) => {
                let zs_diags = zs_diag::from_parse_errors(&parse_errors);
                (
                    self.convert_diagnostics(source, &zs_diags),
                    SymbolIndex::default(),
                    HashMap::new(),
                )
            }
            Ok(program) => {
                let mut index = SymbolIndex::build(&program);
                let (mut check_diags, type_map) = Checker::new().check_with_types(&program);

                // Resolve imports: enrich symbols from .d.ts, validate relative paths
                if let Ok(source_path) = uri.to_file_path() {
                    let source_dir = source_path.parent().unwrap_or(Path::new("."));
                    // Walk up to find project root (where node_modules lives)
                    let project_dir = find_project_dir(source_dir);
                    let cache = self.dts_cache.read().await.clone();
                    let (import_diags, new_cache) =
                        enrich_from_imports(&program, &project_dir, source_dir, &mut index, &cache);
                    check_diags.extend(import_diags);
                    // Update cache with newly resolved modules
                    if !new_cache.is_empty() {
                        let mut cache_write = self.dts_cache.write().await;
                        cache_write.extend(new_cache);
                    }
                }

                (
                    self.convert_diagnostics(source, &check_diags),
                    index,
                    type_map,
                )
            }
        };

        self.documents.write().await.insert(
            uri.clone(),
            Document {
                content: source.to_string(),
                index,
                type_map,
            },
        );

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Convert ZenScript diagnostics to LSP diagnostics.
    fn convert_diagnostics(
        &self,
        source: &str,
        zs_diagnostics: &[zs_diag::Diagnostic],
    ) -> Vec<Diagnostic> {
        zs_diagnostics
            .iter()
            .map(|d| {
                let severity = match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Help => DiagnosticSeverity::HINT,
                };

                let range = offset_to_range(source, d.span.start, d.span.end);

                Diagnostic {
                    range,
                    severity: Some(severity),
                    code: d.code.as_ref().map(|c| NumberOrString::String(c.clone())),
                    source: Some("zenscript".to_string()),
                    message: d.message.clone(),
                    related_information: None,
                    tags: None,
                    code_description: None,
                    data: None,
                }
            })
            .collect()
    }

    /// Generate pipe-aware completions.
    /// Only shows functions (not keywords/types/consts), ranked by first-param compatibility.
    fn pipe_completions(
        &self,
        docs: &HashMap<Url, Document>,
        current_uri: &Url,
        prefix: &str,
        piped_type: Option<&str>,
    ) -> Vec<CompletionItem> {
        let mut matched: Vec<CompletionItem> = Vec::new();
        let mut unmatched: Vec<CompletionItem> = Vec::new();

        // Collect functions from all open documents
        for (doc_uri, doc) in docs.iter() {
            let is_current = doc_uri == current_uri;

            for sym in &doc.index.symbols {
                // Only suggest functions in pipe context
                if sym.kind != SymbolKind::FUNCTION {
                    continue;
                }
                // Must have at least one parameter to be pipe-compatible
                if sym.first_param_type.is_none() {
                    continue;
                }
                // Filter by prefix
                if !prefix.is_empty() && !sym.name.starts_with(prefix) {
                    continue;
                }
                // Skip re-exports
                if !is_current && sym.import_source.is_some() {
                    continue;
                }

                let compatible = piped_type
                    .zip(sym.first_param_type.as_deref())
                    .is_some_and(|(pt, fpt)| is_pipe_compatible(fpt, pt));

                let sort_prefix = if compatible { "0" } else { "1" };

                let mut item = CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some(sym.detail.clone()),
                    insert_text: Some(sym.name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    sort_text: Some(format!("{sort_prefix}{}", sym.name)),
                    ..Default::default()
                };

                // Add auto-import for cross-file functions
                if !is_current {
                    let relative_path = doc_uri
                        .path_segments()
                        .and_then(|mut s| s.next_back())
                        .unwrap_or("unknown")
                        .trim_end_matches(".zs");

                    item.detail = Some(format!(
                        "{} (auto-import from {})",
                        sym.detail, relative_path
                    ));
                    item.additional_text_edits = Some(vec![TextEdit {
                        range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(0, 0),
                        },
                        new_text: format!(
                            "import {{ {} }} from \"./{}\"\n",
                            sym.name, relative_path
                        ),
                    }]);
                    // Cross-file sorts after same-file
                    item.sort_text = Some(format!("{sort_prefix}1{}", sym.name));
                }

                if compatible {
                    matched.push(item);
                } else {
                    unmatched.push(item);
                }
            }
        }

        matched.extend(unmatched);
        matched
    }
}

// ── LSP Protocol ────────────────────────────────────────────────

/// ZenScript keywords and builtins for completion.
const KEYWORDS: &[(&str, &str)] = &[
    ("const", "const ${1:name} = ${0:value}"),
    (
        "function",
        "function ${1:name}(${2:params}): ${3:ReturnType} {\n\t$0\n}",
    ),
    ("export", "export "),
    ("import", "import { ${1:name} } from \"${0:module}\""),
    (
        "match",
        "match ${1:expr} {\n\t${2:pattern} -> ${3:body},\n\t_ -> ${0:default},\n}",
    ),
    ("type", "type ${1:Name} = {\n\t${0:field}: ${2:Type},\n}"),
    ("return", "return ${0:expr}"),
    ("if", "if ${1:condition} {\n\t${0:body}\n}"),
    ("async", "async "),
    ("await", "await ${0:expr}"),
    ("opaque", "opaque type ${1:Name} = ${0:BaseType}"),
];

const BUILTINS: &[(&str, &str, &str)] = &[
    ("Ok", "Ok(${0:value})", "Ok(value: T) -> Result<T, E>"),
    ("Err", "Err(${0:error})", "Err(error: E) -> Result<T, E>"),
    ("Some", "Some(${0:value})", "Some(value: T) -> Option<T>"),
    ("None", "None", "None -> Option<T>"),
    ("true", "true", "bool literal"),
    ("false", "false", "bool literal"),
];

#[tower_lsp::async_trait]
impl LanguageServer for ZenScriptLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "|".to_string(),
                        ">".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "zenscript-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "ZenScript LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        self.update_document(uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.update_document(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
    }

    // ── Hover ───────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            return Ok(None);
        }

        // Check symbol index first
        let symbols = doc.index.find_by_name(word);
        if let Some(sym) = symbols.first() {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```zenscript\n{}\n```", sym.detail),
                }),
                range: None,
            }));
        }

        // Fallback to builtin hover
        let hover_text = match word {
            "Ok" => {
                "```zenscript\nOk(value: T) -> Result<T, E>\n```\nWrap a success value in a Result."
            }
            "Err" => {
                "```zenscript\nErr(error: E) -> Result<T, E>\n```\nWrap an error value in a Result."
            }
            "Some" => "```zenscript\nSome(value: T) -> Option<T>\n```\nWrap a value in an Option.",
            "None" => "```zenscript\nNone -> Option<T>\n```\nRepresents the absence of a value.",
            "match" => {
                "```zenscript\nmatch expr { pattern -> body, ... }\n```\nExhaustive pattern matching expression."
            }
            "|>" => {
                "```zenscript\nexpr |> function\n```\nPipe operator: passes left side as first argument to right side."
            }
            _ => return Ok(None),
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text.to_string(),
            }),
            range: None,
        }))
    }

    // ── Completion ──────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let prefix = word_prefix_at_offset(&doc.content, offset);

        // ── Pipe-aware completions ──────────────────────────────
        if is_pipe_context(&doc.content, offset) {
            let piped_type = resolve_piped_type(&doc.content, offset, &doc.type_map);
            let items = self.pipe_completions(&docs, &uri, &prefix, piped_type.as_deref());
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // ── Normal completions ──────────────────────────────────
        let mut items = Vec::new();

        // Symbols from the current document
        for sym in doc.index.all_completions() {
            if !prefix.is_empty() && !sym.name.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: sym.name.clone(),
                kind: Some(symbol_kind_to_completion(sym.kind)),
                detail: Some(sym.detail.clone()),
                insert_text: Some(sym.name.clone()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }

        // Symbols from other open documents (cross-file completions + auto-import)
        for (other_uri, other_doc) in docs.iter() {
            if other_uri == &uri {
                continue;
            }
            for sym in &other_doc.index.symbols {
                if sym.import_source.is_some() {
                    continue;
                }
                if !prefix.is_empty() && !sym.name.starts_with(&prefix) {
                    continue;
                }
                let relative_path = other_uri
                    .path_segments()
                    .and_then(|mut s| s.next_back())
                    .unwrap_or("unknown")
                    .trim_end_matches(".zs");

                let import_edit =
                    format!("import {{ {} }} from \"./{}\"\n", sym.name, relative_path);

                items.push(CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(symbol_kind_to_completion(sym.kind)),
                    detail: Some(format!(
                        "{} (auto-import from {})",
                        sym.detail, relative_path
                    )),
                    insert_text: Some(sym.name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    additional_text_edits: Some(vec![TextEdit {
                        range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(0, 0),
                        },
                        new_text: import_edit,
                    }]),
                    ..Default::default()
                });
            }
        }

        // Keywords
        for (kw, snippet) in KEYWORDS {
            if !prefix.is_empty() && !kw.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Builtins
        for (name, snippet, detail) in BUILTINS {
            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(detail.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    // ── Go to Definition ────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            return Ok(None);
        }

        // Search current document
        for sym in doc.index.find_by_name(word) {
            // Skip the symbol at the cursor position itself (don't jump to yourself)
            if offset >= sym.start && offset <= sym.end {
                continue;
            }
            let range = offset_to_range(&doc.content, sym.start, sym.end);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range,
            })));
        }

        // Search other open documents
        for (other_uri, other_doc) in docs.iter() {
            if other_uri == &uri {
                continue;
            }
            for sym in other_doc.index.find_by_name(word) {
                if sym.import_source.is_some() {
                    continue;
                }
                let range = offset_to_range(&other_doc.content, sym.start, sym.end);
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: other_uri.clone(),
                    range,
                })));
            }
        }

        Ok(None)
    }

    // ── Find References ─────────────────────────────────────────

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            return Ok(None);
        }

        let mut locations = Vec::new();

        // Find all occurrences in all open documents
        for (doc_uri, doc) in docs.iter() {
            let source = &doc.content;
            let mut search_from = 0;
            while let Some(pos) = source[search_from..].find(word) {
                let abs_pos = search_from + pos;
                let end_pos = abs_pos + word.len();

                // Check it's a whole word match
                let before_ok = abs_pos == 0 || !is_word_char(source.as_bytes()[abs_pos - 1]);
                let after_ok = end_pos >= source.len() || !is_word_char(source.as_bytes()[end_pos]);

                if before_ok && after_ok {
                    let range = offset_to_range(source, abs_pos, end_pos);
                    locations.push(Location {
                        uri: doc_uri.clone(),
                        range,
                    });
                }

                search_from = abs_pos + 1;
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    // ── Document Symbols ────────────────────────────────────────

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        #[allow(deprecated)]
        let symbols: Vec<SymbolInformation> = doc
            .index
            .symbols
            .iter()
            .filter(|s| s.import_source.is_none()) // Skip imports in outline
            .map(|s| {
                let range = offset_to_range(&doc.content, s.start, s.end);
                SymbolInformation {
                    name: s.name.clone(),
                    kind: s.kind,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range,
                    },
                    container_name: None,
                }
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Flat(symbols)))
    }

    // ── Code Actions ─────────────────────────────────────────

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let mut actions = Vec::new();

        for diag in &params.context.diagnostics {
            // E010: exported function missing return type — offer to insert inferred type
            let is_e010 = diag
                .code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "E010"));

            if !is_e010 {
                continue;
            }

            // Find the function name from the diagnostic message
            let fn_name = diag
                .message
                .strip_prefix("exported function `")
                .and_then(|s| s.strip_suffix("` must declare a return type"));

            let Some(fn_name) = fn_name else {
                continue;
            };

            // Look up the inferred return type from the checker's type map
            let inferred = doc.type_map.get(fn_name).and_then(|ty| {
                // Type map stores the function type like "(number, number) => number"
                // Extract the return type after " => "
                ty.rsplit_once(" => ").map(|(_, ret)| ret.to_string())
            });

            let return_type = inferred.unwrap_or_else(|| "unknown".to_string());

            // Find the `) {` or `)  {` in the function signature to insert before `{`
            let start_offset = position_to_offset(&doc.content, diag.range.start);
            let end_offset = position_to_offset(&doc.content, diag.range.end);
            let fn_text = &doc.content[start_offset..end_offset];

            // Find the closing paren before the opening brace
            if let Some(brace_pos) = fn_text.find('{') {
                let insert_offset = start_offset + brace_pos;
                let insert_pos = offset_to_position(&doc.content, insert_offset);

                let edit = TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text: format!(": {return_type} "),
                };

                let mut changes = HashMap::new();
                changes.insert(uri.clone(), vec![edit]);

                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Add return type `: {return_type}`"),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    is_preferred: Some(true),
                    ..Default::default()
                }));
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn offset_to_range(source: &str, start: usize, end: usize) -> Range {
    let start_pos = offset_to_position(source, start);
    let end_pos = offset_to_position(source, end);
    Range {
        start: start_pos,
        end: end_pos,
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}

fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == position.line && col == position.character {
            return i;
        }
        if ch == '\n' {
            if line == position.line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    source.len()
}

fn word_at_offset(source: &str, offset: usize) -> &str {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return "";
    }

    let mut start = offset;
    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }

    let mut end = offset;
    while end < bytes.len() && is_word_char(bytes[end]) {
        end += 1;
    }

    &source[start..end]
}

/// Get the word prefix before the cursor (for completion filtering).
fn word_prefix_at_offset(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }
    source[start..offset].to_string()
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Find the project root directory (where node_modules lives).
fn find_project_dir(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("node_modules").is_dir() || dir.join("package.json").is_file() {
            return dir;
        }
        if !dir.pop() {
            return start.to_path_buf();
        }
    }
}

/// Start the LSP server on stdin/stdout.
pub async fn run_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(ZenScriptLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_to_position_first_line() {
        let source = "const x = 42";
        let pos = offset_to_position(source, 6);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 6);
    }

    #[test]
    fn offset_to_position_second_line() {
        let source = "const x = 42\nconst y = 10";
        let pos = offset_to_position(source, 19);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 6);
    }

    #[test]
    fn offset_to_range_basic() {
        let source = "const x = 42";
        let range = offset_to_range(source, 6, 7);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 7);
    }

    #[test]
    fn position_to_offset_roundtrip() {
        let source = "hello\nworld\nfoo";
        let offset = position_to_offset(source, Position::new(1, 3));
        assert_eq!(offset, 9);
    }

    #[test]
    fn word_at_offset_basic() {
        let source = "const hello = 42";
        assert_eq!(word_at_offset(source, 6), "hello");
        assert_eq!(word_at_offset(source, 8), "hello");
    }

    #[test]
    fn word_at_offset_at_boundary() {
        let source = "const x = 42";
        assert_eq!(word_at_offset(source, 0), "const");
    }

    #[test]
    fn word_prefix_at_offset_partial() {
        let source = "const hel";
        assert_eq!(word_prefix_at_offset(source, 9), "hel");
    }

    #[test]
    fn word_prefix_at_offset_empty() {
        let source = "const ";
        assert_eq!(word_prefix_at_offset(source, 6), "");
    }

    #[test]
    fn banned_keyword_produces_parse_error() {
        let source = "let x = 42";
        let parse_result = Parser::new(source).parse_program();
        assert!(parse_result.is_err());
        let errs = parse_result.unwrap_err();
        let zs_diags = zs_diag::from_parse_errors(&errs);
        assert!(!zs_diags.is_empty());
        assert_eq!(zs_diags[0].severity, Severity::Error);
    }

    #[test]
    fn symbol_index_function() {
        let source = "function add(a: number, b: number): number { a + b }";
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let syms = index.find_by_name("add");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::FUNCTION);
        assert!(syms[0].detail.contains("function add"));
    }

    #[test]
    fn symbol_index_const() {
        let source = "const x = 42";
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let syms = index.find_by_name("x");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::CONSTANT);
    }

    #[test]
    fn symbol_index_type() {
        let source = "type User = { name: string, age: number }";
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let syms = index.find_by_name("User");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].kind, SymbolKind::TYPE_PARAMETER);
    }

    #[test]
    fn symbol_index_import() {
        let source = r#"import { useState } from "react""#;
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let syms = index.find_by_name("useState");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].import_source.as_deref(), Some("react"));
    }

    #[test]
    fn symbol_index_union_variants() {
        let source = "type Color = | Red | Green | Blue";
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        assert_eq!(index.find_by_name("Color").len(), 1);
        assert_eq!(index.find_by_name("Red").len(), 1);
        assert_eq!(index.find_by_name("Green").len(), 1);
        assert_eq!(index.find_by_name("Blue").len(), 1);
    }

    #[test]
    fn type_expr_to_string_named() {
        let ty = TypeExpr {
            kind: TypeExprKind::Named {
                name: "string".to_string(),
                type_args: vec![],
            },
            span: crate::lexer::span::Span::new(0, 0, 1, 1),
        };
        assert_eq!(type_expr_to_string(&ty), "string");
    }

    #[test]
    fn type_expr_to_string_generic() {
        let ty = TypeExpr {
            kind: TypeExprKind::Named {
                name: "Result".to_string(),
                type_args: vec![
                    TypeExpr {
                        kind: TypeExprKind::Named {
                            name: "User".to_string(),
                            type_args: vec![],
                        },
                        span: crate::lexer::span::Span::new(0, 0, 1, 1),
                    },
                    TypeExpr {
                        kind: TypeExprKind::Named {
                            name: "Error".to_string(),
                            type_args: vec![],
                        },
                        span: crate::lexer::span::Span::new(0, 0, 1, 1),
                    },
                ],
            },
            span: crate::lexer::span::Span::new(0, 0, 1, 1),
        };
        assert_eq!(type_expr_to_string(&ty), "Result<User, Error>");
    }

    // ── Pipe-aware completion tests ────────────────────────

    #[test]
    fn pipe_context_detected() {
        assert!(is_pipe_context("users |> ", 9));
        assert!(is_pipe_context("users |>  ", 10));
        assert!(is_pipe_context("x |> f() |> ", 12));
        assert!(!is_pipe_context("const x = 42", 12));
        assert!(!is_pipe_context("const x = |", 11));
    }

    #[test]
    fn pipe_context_with_prefix() {
        // User typed "users |> fi" — cursor is at offset 11
        // The prefix would be "fi", and before that is "|> "
        let source = "users |> fi";
        // is_pipe_context checks before the prefix starts
        assert!(is_pipe_context(&source[..9], 9)); // "users |> "
    }

    #[test]
    fn base_type_name_simple() {
        assert_eq!(base_type_name("string"), "string");
        assert_eq!(base_type_name("number"), "number");
    }

    #[test]
    fn base_type_name_generic() {
        assert_eq!(base_type_name("Array<User>"), "Array");
        assert_eq!(base_type_name("Option<string>"), "Option");
        assert_eq!(base_type_name("Result<User, Error>"), "Result");
    }

    #[test]
    fn unwrap_result_type() {
        assert_eq!(unwrap_type("Result<User, Error>"), "User");
    }

    #[test]
    fn unwrap_option_type() {
        assert_eq!(unwrap_type("Option<string>"), "string");
    }

    #[test]
    fn unwrap_non_wrapper_type() {
        assert_eq!(unwrap_type("string"), "string");
    }

    #[test]
    fn pipe_compatible_same_type() {
        assert!(is_pipe_compatible("Array<T>", "Array<User>"));
        assert!(is_pipe_compatible("string", "string"));
        assert!(is_pipe_compatible("Option<T>", "Option<number>"));
    }

    #[test]
    fn pipe_compatible_generic_param() {
        // Single-letter type params match anything
        assert!(is_pipe_compatible("T", "string"));
        assert!(is_pipe_compatible("A", "Array<User>"));
    }

    #[test]
    fn pipe_incompatible_types() {
        assert!(!is_pipe_compatible("string", "number"));
        assert!(!is_pipe_compatible("Array<T>", "Option<T>"));
    }

    #[test]
    fn extract_identifier_simple() {
        assert_eq!(extract_trailing_identifier("users"), "users");
    }

    #[test]
    fn extract_identifier_call() {
        assert_eq!(extract_trailing_identifier("getUsers()"), "getUsers");
    }

    #[test]
    fn extract_identifier_member() {
        assert_eq!(extract_trailing_identifier("a.b.c"), "c");
    }

    #[test]
    fn infer_literal_string() {
        assert_eq!(infer_literal_type("\"hello\""), Some("string".to_string()));
    }

    #[test]
    fn infer_literal_number() {
        assert_eq!(infer_literal_type("42"), Some("number".to_string()));
    }

    #[test]
    fn infer_literal_bool() {
        assert_eq!(infer_literal_type("true"), Some("bool".to_string()));
    }

    #[test]
    fn resolve_piped_type_from_type_map() {
        let mut type_map = HashMap::new();
        type_map.insert("users".to_string(), "Array<User>".to_string());
        let source = "users |> ";
        let result = resolve_piped_type(source, 9, &type_map);
        assert_eq!(result, Some("Array<User>".to_string()));
    }

    #[test]
    fn resolve_piped_type_with_unwrap() {
        let mut type_map = HashMap::new();
        type_map.insert(
            "fetchUser".to_string(),
            "(number) => Result<User, Error>".to_string(),
        );
        let source = "result? |> ";
        let mut tm = HashMap::new();
        tm.insert("result".to_string(), "Result<User, Error>".to_string());
        let resolved = resolve_piped_type(source, 11, &tm);
        assert_eq!(resolved, Some("User".to_string()));
    }

    #[test]
    fn function_symbol_stores_first_param_type() {
        let source = "function filter(arr: Array<T>, pred: (T) => bool): Array<T> { arr }";
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let syms = index.find_by_name("filter");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].first_param_type.as_deref(), Some("Array<T>"));
        assert_eq!(syms[0].return_type_str.as_deref(), Some("Array<T>"));
    }

    // ── Integration tests on jsx_component.zs ──────────────

    fn jsx_component_source() -> &'static str {
        r#"import { useState, JSX } from "react"

export function Counter(): JSX.Element {
    const [_count, setCount] = useState(0)

    return <div>
        <h1>Count</h1>
        <button onClick={setCount}>Increment</button>
    </div>
}"#
    }

    fn build_index_and_types(source: &str) -> (SymbolIndex, HashMap<String, String>) {
        let program = Parser::new(source).parse_program().unwrap();
        let index = SymbolIndex::build(&program);
        let (_, type_map) = crate::checker::Checker::new().check_with_types(&program);
        (index, type_map)
    }

    #[test]
    fn jsx_fixture_all_symbols_indexed() {
        let (index, _) = build_index_and_types(jsx_component_source());
        let all_names: Vec<&str> = index.symbols.iter().map(|s| s.name.as_str()).collect();
        println!("All indexed symbols: {:?}", all_names);

        // Imports
        assert!(
            !index.find_by_name("useState").is_empty(),
            "useState not indexed"
        );
        assert!(!index.find_by_name("JSX").is_empty(), "JSX not indexed");

        // Function
        assert!(
            !index.find_by_name("Counter").is_empty(),
            "Counter not indexed"
        );

        // Destructured variables
        assert!(
            !index.find_by_name("_count").is_empty(),
            "_count not indexed"
        );
        assert!(
            !index.find_by_name("setCount").is_empty(),
            "setCount not indexed"
        );
    }

    #[test]
    fn jsx_fixture_hover_on_destructured_var() {
        let (index, _) = build_index_and_types(jsx_component_source());

        // Hover on _count should work
        let syms = index.find_by_name("_count");
        assert!(!syms.is_empty(), "_count should be found for hover");
        assert!(
            syms[0].detail.contains("_count"),
            "detail should mention _count, got: {}",
            syms[0].detail
        );

        // Hover on setCount should work
        let syms = index.find_by_name("setCount");
        assert!(!syms.is_empty(), "setCount should be found for hover");
        assert!(
            syms[0].detail.contains("setCount"),
            "detail should mention setCount, got: {}",
            syms[0].detail
        );
    }

    #[test]
    fn jsx_fixture_goto_def_setcount_from_jsx() {
        let source = jsx_component_source();
        let (index, _) = build_index_and_types(source);

        // Find the offset of setCount in onClick={setCount} (line 8)
        let jsx_setcount_offset = source.find("onClick={setCount}").unwrap() + "onClick={".len();
        let word = word_at_offset(source, jsx_setcount_offset);
        assert_eq!(
            word, "setCount",
            "should extract 'setCount' from JSX attribute"
        );

        // find_by_name should find it
        let syms = index.find_by_name("setCount");
        assert!(!syms.is_empty(), "setCount should be in index");

        // The definition's span should NOT contain the JSX usage offset
        let sym = &syms[0];
        let cursor_in_def = jsx_setcount_offset >= sym.start && jsx_setcount_offset <= sym.end;
        assert!(
            !cursor_in_def,
            "JSX usage offset {} should NOT be inside definition span {}..{} (go-to-def would skip it!)",
            jsx_setcount_offset, sym.start, sym.end
        );
    }

    #[test]
    fn jsx_fixture_hover_on_usestate() {
        let (index, _) = build_index_and_types(jsx_component_source());
        let syms = index.find_by_name("useState");
        assert!(!syms.is_empty());
        assert!(syms[0].detail.contains("useState"));
    }

    #[test]
    fn jsx_fixture_type_map_has_counter() {
        let (_, type_map) = build_index_and_types(jsx_component_source());
        println!("Type map: {:?}", type_map);
        assert!(
            type_map.contains_key("Counter"),
            "Counter should be in type map"
        );
    }
}
