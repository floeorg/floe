mod code_actions;
mod completion;
mod completions;
mod goto_def;
mod handlers;
mod hover;
mod resolution;
mod stdlib_hover;
mod symbols;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LspService, Server};

use floe_core::analyse::{self, ExternTypes, ModuleInputs};
use floe_core::checker::Type;
use floe_core::diagnostic::{self as floe_diag, Severity};
use floe_core::parser::Parser;
use floe_core::parser::ast::TypedProgram;
use floe_core::reference::ReferenceTracker;

use completion::is_pipe_compatible;
use resolution::enrich_from_imports;
use symbols::SymbolIndex;

/// Find the resolved type and span width of the innermost expression at a byte offset.
/// Returns (span_width, type) of the tightest non-Unknown expression containing the offset.
fn find_expr_type_at_offset(program: &TypedProgram, offset: usize) -> Option<(usize, Type)> {
    use floe_core::parser::ast::TypedExpr;

    let mut best: Option<(usize, Type)> = None;

    let mut check = |expr: &TypedExpr| {
        if offset >= expr.span.start
            && offset <= expr.span.end
            && !matches!(&*expr.ty, Type::Unknown)
        {
            let width = expr.span.end - expr.span.start;
            if best.as_ref().is_none_or(|(w, _)| width < *w) {
                best = Some((width, (*expr.ty).clone()));
            }
        }
    };

    floe_core::walk::walk_program(program, &mut check);
    best
}

/// Find the type of the left-hand side of a pipe expression at the given offset.
/// Used for hover on `|>` to show what value is being piped.
fn find_pipe_input_type_at_offset(program: &TypedProgram, offset: usize) -> Option<Type> {
    use floe_core::parser::ast::{ExprKind, TypedExpr};

    let mut best: Option<(usize, Type)> = None;

    let mut check = |expr: &TypedExpr| {
        if let ExprKind::Pipe { left, .. } = &expr.kind
            && offset >= expr.span.start
            && offset <= expr.span.end
            && !matches!(&*left.ty, Type::Unknown)
        {
            let width = expr.span.end - expr.span.start;
            if best.as_ref().is_none_or(|(w, _)| width < *w) {
                best = Some((width, (*left.ty).clone()));
            }
        }
    };

    floe_core::walk::walk_program(program, &mut check);
    best.map(|(_, ty)| ty)
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

/// Check if the cursor is on the definition name itself (not in the body).
///
/// The symbol's start..end covers the entire declaration (e.g. the whole
/// function including its body). We only want to skip goto-def when the
/// cursor is literally on the name token at the definition site, not when
/// it's on a usage of that name inside the declaration body.
fn is_cursor_on_def_name(source: &str, cursor_offset: usize, sym: &symbols::Symbol) -> bool {
    // The name must appear somewhere near the start of the declaration.
    // Search for the first occurrence of the name within the item span.
    let end = sym.end.min(source.len());
    if sym.start >= end {
        return false;
    }
    let span_slice = &source[sym.start..end];
    if let Some(rel_pos) = span_slice.find(&sym.name) {
        let name_start = sym.start + rel_pos;
        let name_end = name_start + sym.name.len();
        cursor_offset >= name_start && cursor_offset < name_end
    } else {
        false
    }
}

use floe_core::find_project_dir;

// ── Document State ──────────────────────────────────────────────

/// State for an open document.
#[derive(Debug, Clone)]
struct Document {
    content: String,
    index: SymbolIndex,
    /// Type map from the checker: variable/function name -> inferred type display name.
    /// Used for completions, dot-access, and pipe type resolution.
    type_map: HashMap<String, String>,
    /// Typed AST — every Expr has its resolved `Arc<Type>` in `expr.ty`.
    /// Used as the single source of truth for hover on expressions.
    typed_program: Option<TypedProgram>,
    /// Per-module reference tracker built by `analyse`. Goto-definition
    /// consults it for precise definition spans so intra-module jumps
    /// don't rely on name-based index lookups.
    references: ReferenceTracker,
}

// ── LSP Protocol Constants ──────────────────────────────────────

/// Floe keywords and builtins for completion.
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
    ("opaque", "opaque type ${1:Name} = ${0:BaseType}"),
];

const BUILTINS: &[(&str, &str, &str)] = &[
    ("Ok", "Ok(${0:value})", "Ok(value: T) -> Result<T, E>"),
    ("Err", "Err(${0:error})", "Err(error: E) -> Result<T, E>"),
    ("Some", "Some(${0:value})", "Some(value: T) -> Option<T>"),
    ("None", "None", "None -> Option<T>"),
    (
        "parse",
        "parse<${1:Type}>(${0:value})",
        "parse<T>(value) -> Result<T, Error>",
    ),
    (
        "mock",
        "mock<${0:Type}>",
        "mock<T> -> T (compiler-generated test data)",
    ),
    ("true", "true", "bool literal"),
    ("false", "false", "bool literal"),
];

// ── The Floe Language Server ────────────────────────────────────

/// The Floe Language Server.
pub struct FloeLsp {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
    /// Cache of resolved .d.ts exports per module specifier
    dts_cache: Arc<RwLock<HashMap<String, Vec<floe_core::interop::DtsExport>>>>,
    /// Project directories we've already logged startup info for
    logged_projects: Arc<RwLock<HashSet<PathBuf>>>,
    /// Per-file cache of .fl import resolution. Keyed by the
    /// dep file's canonical path; stores (source_hash, exports).
    /// When the dep's source matches the cached hash, its exports
    /// are reused without re-parsing. Avoids re-walking every
    /// imported `.fl` module on each keystroke.
    resolve_cache: Arc<RwLock<HashMap<PathBuf, (u64, floe_core::resolve::ResolvedImports)>>>,
}

impl FloeLsp {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
            dts_cache: Arc::new(RwLock::new(HashMap::new())),
            logged_projects: Arc::new(RwLock::new(HashSet::new())),
            resolve_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Log project directory, tsconfig, and path alias info (once per project).
    async fn log_project_info(
        &self,
        project_dir: &Path,
        tsconfig_paths: &floe_core::resolve::TsconfigPaths,
    ) {
        let canonical = project_dir.to_path_buf();
        {
            let logged = self.logged_projects.read().await;
            if logged.contains(&canonical) {
                return;
            }
        }
        self.logged_projects.write().await.insert(canonical);

        self.client
            .log_message(
                MessageType::INFO,
                format!("Project directory: {}", project_dir.display()),
            )
            .await;

        let parsed = floe_core::resolve::ParsedTsconfig::from_project_dir(project_dir);
        match parsed {
            Some(ref ts) => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("tsconfig.json: {}", ts.tsconfig_path.display()),
                    )
                    .await;
            }
            None => {
                self.client
                    .log_message(MessageType::INFO, "tsconfig.json: not found")
                    .await;
            }
        }

        let alias_count = tsconfig_paths.mappings.len();
        self.client
            .log_message(
                MessageType::INFO,
                format!("Path alias mappings: {alias_count}"),
            )
            .await;
    }

    /// Resolve imports with per-dep caching. Unchanged deps skip
    /// re-parsing — only deps whose source hash changed are re-walked.
    async fn resolve_imports_cached(
        &self,
        source_path: &Path,
        program: &floe_core::parser::ast::Program,
        tsconfig_paths: &floe_core::resolve::TsconfigPaths,
    ) -> HashMap<String, floe_core::resolve::ResolvedImports> {
        let mut cache = self.resolve_cache.write().await;
        let (resolved, _dep_paths) = floe_core::resolve::resolve_imports_cached(
            source_path,
            program,
            tsconfig_paths,
            &mut cache,
        );
        resolved
    }

    /// Parse and type-check a document, update symbol index, publish diagnostics.
    async fn update_document(&self, uri: Url, source: &str) {
        let (diagnostics, index, type_map, typed_program, references) = match Parser::new(source)
            .parse_program()
        {
            Err(_) => {
                // Full parse failed — use lossy parse to get a partial AST so
                // we can still build a symbol index for completions/hover.
                let (program, parse_errors) = Parser::parse_lossy(source);
                let floe_diags = floe_diag::from_parse_errors(&parse_errors);
                let index = SymbolIndex::build(&program);
                let analysed = analyse::analyse_parsed(program, ModuleInputs::default());
                let mut combined = floe_diags;
                combined.extend(analysed.diagnostics);
                (
                    self.convert_diagnostics(source, &combined),
                    index,
                    analysed.name_types,
                    Some(analysed.program),
                    analysed.references,
                )
            }
            Ok(program) => {
                let mut index = SymbolIndex::build(&program);

                // Resolve .fl imports for cross-file type checking.
                let (resolved_imports, project_dir, tsconfig_paths) = if let Ok(source_path) =
                    uri.to_file_path()
                {
                    let source_dir = source_path.parent().unwrap_or(Path::new(".")).to_path_buf();
                    let project_dir = find_project_dir(&source_dir);
                    let paths = floe_core::resolve::TsconfigPaths::from_project_dir(&project_dir);
                    self.log_project_info(&project_dir, &paths).await;
                    let resolved = self
                        .resolve_imports_cached(&source_path, &program, &paths)
                        .await;
                    (resolved, Some(project_dir), paths)
                } else {
                    (Default::default(), None, Default::default())
                };

                // Resolve .d.ts imports BEFORE analyse so it gets npm type info.
                let mut import_diags_early = Vec::new();
                let (dts_map, ts_imports_missing_tsgo, ambient) =
                    if let (Ok(source_path), Some(project_dir)) =
                        (uri.to_file_path(), project_dir.as_ref())
                    {
                        let source_dir = source_path.parent().unwrap_or(Path::new("."));
                        let cache = self.dts_cache.read().await.clone();
                        let (import_diags, new_cache) = enrich_from_imports(
                            &program,
                            project_dir,
                            source_dir,
                            &mut index,
                            &cache,
                            &tsconfig_paths,
                        );
                        import_diags_early = import_diags;
                        let mut tsgo_resolver = floe_core::interop::TsgoResolver::new(project_dir);
                        let tsgo_result = tsgo_resolver.resolve_imports(
                            &program,
                            &resolved_imports,
                            source_dir,
                            &tsconfig_paths,
                        );
                        if !new_cache.is_empty() {
                            let mut cache_write = self.dts_cache.write().await;
                            cache_write.extend(new_cache);
                        }
                        let ambient = floe_core::interop::ambient::load_ambient_types(project_dir);
                        (
                            tsgo_result.exports,
                            tsgo_result.ts_imports_missing_tsgo,
                            ambient,
                        )
                    } else {
                        (HashMap::new(), HashSet::new(), None)
                    };

                // Add imported for-block functions to the symbol index.
                index.add_imported_for_blocks(&resolved_imports);

                let analysed = analyse::analyse_parsed(
                    program,
                    ModuleInputs {
                        resolved_imports,
                        externs: ExternTypes {
                            dts_imports: dts_map,
                            ambient,
                            ts_imports_missing_tsgo,
                        },
                    },
                );
                let mut check_diags = analysed.diagnostics;
                check_diags.extend(import_diags_early);

                let mut typed_program = analysed.program;
                floe_core::checker::mark_async_functions(&mut typed_program);

                (
                    self.convert_diagnostics(source, &check_diags),
                    index,
                    analysed.name_types,
                    Some(typed_program),
                    analysed.references,
                )
            }
        };

        self.documents.write().await.insert(
            uri.clone(),
            Document {
                content: source.to_string(),
                index,
                type_map,
                typed_program,
                references,
            },
        );

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    /// Convert Floe diagnostics to LSP diagnostics.
    fn convert_diagnostics(
        &self,
        source: &str,
        floe_diagnostics: &[floe_diag::Diagnostic],
    ) -> Vec<Diagnostic> {
        floe_diagnostics
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
                    source: Some("floe".to_string()),
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
                        .trim_end_matches(".fl");

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

        // Add stdlib functions to pipe completions using bare names
        // (pipes use type-directed resolution: `|> map(...)` not `|> Array.map(...)`)
        let stdlib = floe_core::stdlib::StdlibRegistry::new();
        for f in stdlib.all_functions() {
            if !prefix.is_empty() && !f.name.starts_with(prefix) {
                continue;
            }
            // Skip if a user-defined function with the same name is already listed
            if matched
                .iter()
                .chain(unmatched.iter())
                .any(|i| i.label == f.name)
            {
                continue;
            }
            let first_param_str = f.params.first().map(stdlib_hover::format_type);
            let compatible = piped_type
                .zip(first_param_str.as_deref())
                .is_some_and(|(pt, fpt)| completion::is_pipe_compatible(fpt, pt));

            let sort_prefix = if compatible { "0" } else { "1" };
            let ret = stdlib_hover::format_type(&f.return_type);
            let detail = format!(
                "(for {}) ({}) -> {}",
                f.module,
                stdlib_hover::format_params(f),
                ret
            );
            let name = f.name.to_string();

            let item = CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail),
                insert_text: Some(name),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                sort_text: Some(format!("{sort_prefix}2{}", f.name)),
                ..Default::default()
            };

            if compatible {
                matched.push(item);
            } else {
                unmatched.push(item);
            }
        }

        matched.extend(unmatched);
        matched
    }

    /// Resolve an import specifier to a file path.
    /// Handles relative imports, tsconfig path aliases, and npm packages.
    fn resolve_specifier_to_path(specifier: &str, source_dir: &Path) -> Option<PathBuf> {
        let is_relative = specifier.starts_with("./") || specifier.starts_with("../");
        if is_relative {
            return resolution::resolve_relative_import(specifier, source_dir);
        }
        let project_dir = find_project_dir(source_dir);
        let tsconfig_paths = floe_core::resolve::TsconfigPaths::from_project_dir(&project_dir);
        if let Some(resolved) = tsconfig_paths.resolve(specifier) {
            return Some(resolved);
        }
        resolution::resolve_npm_dts(specifier, &project_dir)
    }

    /// Resolve an import specifier to a Location in the source file (.d.ts or .fl).
    /// For `.d.ts` files, finds the line where the symbol is exported.
    /// For relative imports, finds the file and looks for the symbol definition.
    fn resolve_import_location(
        source_uri: &Url,
        specifier: &str,
        symbol_name: &str,
    ) -> Option<Location> {
        let source_path = source_uri.to_file_path().ok()?;
        let source_dir = source_path.parent()?;

        let resolved_path = Self::resolve_specifier_to_path(specifier, source_dir)?;

        let file_content = std::fs::read_to_string(&resolved_path).ok()?;
        let target_uri = Url::from_file_path(&resolved_path).ok()?;

        // Search for the export line containing the symbol name
        for (line_num, line) in file_content.lines().enumerate() {
            let trimmed = line.trim();
            // Match patterns like: export function symbolName, export const symbolName,
            // export type symbolName, export interface symbolName, export declare ...
            let is_export_of_symbol = trimmed.contains("export")
                && (trimmed.contains(&format!("function {symbol_name}"))
                    || trimmed.contains(&format!("const {symbol_name}"))
                    || trimmed.contains(&format!("type {symbol_name}"))
                    || trimmed.contains(&format!("interface {symbol_name}"))
                    || trimmed.contains(&format!("fn {symbol_name}")));

            if is_export_of_symbol {
                // Find the column where the symbol name starts on this line
                let col = line.find(symbol_name).unwrap_or(0) as u32;
                let pos = Position::new(line_num as u32, col);
                let end_pos = Position::new(line_num as u32, col + symbol_name.len() as u32);
                return Some(Location {
                    uri: target_uri,
                    range: Range {
                        start: pos,
                        end: end_pos,
                    },
                });
            }
        }

        // Fallback: jump to the top of the resolved file
        Some(Location {
            uri: target_uri,
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
        })
    }

    /// Resolve an import path string to a Location at line 1 of the target file.
    /// Used when the cursor is on the path string itself (e.g., `"../types"`).
    fn resolve_import_path_location(source_uri: &Url, specifier: &str) -> Option<Location> {
        let source_path = source_uri.to_file_path().ok()?;
        let source_dir = source_path.parent()?;

        let resolved_path = Self::resolve_specifier_to_path(specifier, source_dir)?;

        let target_uri = Url::from_file_path(&resolved_path).ok()?;

        // Jump to the start of the file
        Some(Location {
            uri: target_uri,
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            },
        })
    }
}

/// Start the LSP server on stdin/stdout.
pub async fn run_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(FloeLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
