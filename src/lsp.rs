mod completion;
mod handlers;
mod resolution;
mod stdlib_hover;
mod symbols;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LspService, Server};

use crate::checker::Checker;
use crate::diagnostic::{self as zs_diag, Severity};
use crate::parser::Parser;

use completion::is_pipe_compatible;
use resolution::enrich_from_imports;
use symbols::SymbolIndex;

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
/// Prioritizes finding `node_modules` over `package.json` to handle
/// pnpm workspaces where node_modules is hoisted to the workspace root.
pub(super) fn find_project_dir(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    let mut package_json_dir: Option<PathBuf> = None;
    loop {
        if dir.join("node_modules").is_dir() {
            return dir;
        }
        if package_json_dir.is_none() && dir.join("package.json").is_file() {
            package_json_dir = Some(dir.clone());
        }
        if !dir.pop() {
            return package_json_dir.unwrap_or_else(|| start.to_path_buf());
        }
    }
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

// ── The Floe Language Server ────────────────────────────────────

/// The Floe Language Server.
pub struct FloeLsp {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
    /// Cache of resolved .d.ts exports per module specifier
    dts_cache: Arc<RwLock<HashMap<String, Vec<crate::interop::DtsExport>>>>,
}

impl FloeLsp {
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

                // Resolve .fl imports for cross-file type checking
                let resolved_imports = if let Ok(source_path) = uri.to_file_path() {
                    crate::resolve::resolve_imports(&source_path, &program)
                } else {
                    Default::default()
                };

                // Resolve .d.ts imports BEFORE the checker so it gets npm type info
                let mut dts_map = HashMap::new();
                let mut import_diags_early = Vec::new();
                if let Ok(source_path) = uri.to_file_path() {
                    let source_dir = source_path.parent().unwrap_or(Path::new("."));
                    let project_dir = find_project_dir(source_dir);
                    let cache = self.dts_cache.read().await.clone();
                    let (import_diags, new_cache) =
                        enrich_from_imports(&program, &project_dir, source_dir, &mut index, &cache);
                    import_diags_early = import_diags;
                    // Build dts_map for the checker
                    for (specifier, exports) in &new_cache {
                        dts_map.insert(specifier.clone(), exports.clone());
                    }
                    // Also include cached entries
                    for (specifier, exports) in &cache {
                        dts_map
                            .entry(specifier.clone())
                            .or_insert_with(|| exports.clone());
                    }
                    if !new_cache.is_empty() {
                        let mut cache_write = self.dts_cache.write().await;
                        cache_write.extend(new_cache);
                    }
                }

                let checker = if resolved_imports.is_empty() && dts_map.is_empty() {
                    Checker::new()
                } else if dts_map.is_empty() {
                    Checker::with_imports(resolved_imports)
                } else {
                    Checker::with_all_imports(resolved_imports, dts_map)
                };
                let (mut check_diags, type_map) = checker.check_with_types(&program);
                check_diags.extend(import_diags_early);

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

    /// Convert Floe diagnostics to LSP diagnostics.
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

        matched.extend(unmatched);
        matched
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

        let is_relative = specifier.starts_with("./") || specifier.starts_with("../");

        let resolved_path = if is_relative {
            resolution::resolve_relative_import(specifier, source_dir)?
        } else {
            let project_dir = find_project_dir(source_dir);
            resolution::resolve_npm_dts(specifier, &project_dir)?
        };

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
}

/// Start the LSP server on stdin/stdout.
pub async fn run_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(FloeLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
