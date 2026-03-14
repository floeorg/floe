use std::collections::HashMap;

use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::completion::is_pipe_context;
use super::completion::resolve_piped_type;
use super::stdlib_hover;
use super::symbols::symbol_kind_to_completion;
use super::{
    BUILTINS, FloeLsp, KEYWORDS, is_word_char, offset_to_position, offset_to_range,
    position_to_offset, word_at_offset, word_prefix_at_offset,
};

#[tower_lsp::async_trait]
impl LanguageServer for FloeLsp {
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
                name: "floe-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Floe LSP initialized")
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
            let detail = enrich_hover_detail(sym, &doc.type_map);
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```floe\n{detail}\n```"),
                }),
                range: None,
            }));
        }

        // Check stdlib module names (Array, String, Option, etc.)
        if let Some(hover_text) = stdlib_hover::hover_stdlib_module(word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: hover_text,
                }),
                range: None,
            }));
        }

        // Check bare stdlib function names (for pipe context)
        if let Some(hover_text) = stdlib_hover::hover_stdlib_function(word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: hover_text,
                }),
                range: None,
            }));
        }

        // Fallback to builtin hover
        let hover_text = match word {
            "Ok" => "```floe\nOk(value: T) -> Result<T, E>\n```\nWrap a success value in a Result.",
            "Err" => {
                "```floe\nErr(error: E) -> Result<T, E>\n```\nWrap an error value in a Result."
            }
            "Some" => "```floe\nSome(value: T) -> Option<T>\n```\nWrap a value in an Option.",
            "None" => "```floe\nNone -> Option<T>\n```\nRepresents the absence of a value.",
            "match" => {
                "```floe\nmatch expr { pattern -> body, ... }\n```\nExhaustive pattern matching expression."
            }
            "|>" => {
                "```floe\nexpr |> function\n```\nPipe operator: passes left side as first argument to right side."
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

        // ── Match arm variant completions (#143) ─────────────────
        if let Some(variants) = detect_match_context(&doc.content, offset, &doc.index) {
            let items: Vec<CompletionItem> = variants
                .into_iter()
                .filter(|v| prefix.is_empty() || v.starts_with(&prefix))
                .map(|name| CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("match variant".to_string()),
                    insert_text: Some(format!("{name} -> $0,")),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                })
                .collect();
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // ── JSX attribute completions (#144) ─────────────────────
        if is_in_jsx_tag(&doc.content, offset) {
            let items = jsx_attribute_completions(&prefix);
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // ── Lambda event completions (#145) ──────────────────────
        if let Some(items) = lambda_event_completions(&doc.content, offset, &prefix)
            && !items.is_empty()
        {
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
                    .trim_end_matches(".fl");

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
            // If this symbol is an import, try to resolve to the source file
            // (even if cursor is on the import itself — that's the point)
            if let Some(source_spec) = &sym.import_source
                && let Some(location) = Self::resolve_import_location(&uri, source_spec, word)
            {
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
            }

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

            // E014: untrusted import — offer three quick fixes
            let is_e014 = diag
                .code
                .as_ref()
                .is_some_and(|c| matches!(c, NumberOrString::String(s) if s == "E014"));

            if is_e014 {
                // Extract function name from "calling untrusted import `X` requires `try`"
                if let Some(fn_name) = diag
                    .message
                    .strip_prefix("calling untrusted import `")
                    .and_then(|s| s.strip_suffix("` requires `try`"))
                {
                    let fn_name = fn_name.to_string();

                    // Quick fix 1: Wrap call with `try`
                    // Find the call expression start and insert `try ` before it
                    let call_start = diag.range.start;
                    let edit = TextEdit {
                        range: Range {
                            start: call_start,
                            end: call_start,
                        },
                        new_text: "try ".to_string(),
                    };
                    let mut changes = HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Wrap `{fn_name}(...)` with `try`"),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        is_preferred: Some(true),
                        ..Default::default()
                    }));

                    // Quick fix 2: Mark this specifier as trusted
                    // Find `import { ... fn_name ... } from` and insert `trusted ` before fn_name
                    if let Some(import_edit) =
                        find_import_specifier_edit(&doc.content, &fn_name, false)
                    {
                        let mut changes = HashMap::new();
                        changes.insert(uri.clone(), vec![import_edit]);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Mark `{fn_name}` as `trusted`"),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }

                    // Quick fix 3: Mark whole import as trusted
                    if let Some(import_edit) =
                        find_import_specifier_edit(&doc.content, &fn_name, true)
                    {
                        let mut changes = HashMap::new();
                        changes.insert(uri.clone(), vec![import_edit]);
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "Mark entire import as `trusted`".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }

                continue;
            }

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
                // Type map stores the function type like "(number, number) -> number"
                // Extract the return type after " -> "
                ty.rsplit_once(" -> ").map(|(_, ret)| ret.to_string())
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
                    new_text: format!("-> {return_type} "),
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

// ── Hover enrichment ─────────────────────────────────────────────

use super::symbols::Symbol;

/// Enrich a symbol's hover detail with the inferred type from the checker's
/// type_map when the symbol doesn't already have a type annotation.
pub(super) fn enrich_hover_detail(sym: &Symbol, type_map: &HashMap<String, String>) -> String {
    let detail = &sym.detail;

    // For consts/variables without explicit type annotations (no `:` in the detail),
    // append the inferred type from the checker if available.
    if (sym.kind == SymbolKind::CONSTANT || sym.kind == SymbolKind::VARIABLE)
        && !detail.contains(':')
        && sym.import_source.is_none()
        && let Some(inferred) = type_map.get(&sym.name)
        && !inferred.contains("?T")
    {
        return format!("{detail}: {inferred}");
    }

    // For functions without return type annotation, try to show the inferred return type
    if sym.kind == SymbolKind::FUNCTION
        && sym.import_source.is_none()
        && !detail.contains("->")
        && let Some(inferred) = type_map.get(&sym.name)
        && let Some((_, ret)) = inferred.rsplit_once(" -> ")
        && !ret.contains("?T")
    {
        return format!("{detail} -> {ret}");
    }

    detail.clone()
}

// ── Import quick-fix helpers ─────────────────────────────────────

/// Find the text edit to insert `trusted` for an import.
/// If `whole_module` is true, inserts `trusted ` after `import`.
/// If `whole_module` is false, inserts `trusted ` before the specifier name.
fn find_import_specifier_edit(source: &str, fn_name: &str, whole_module: bool) -> Option<TextEdit> {
    // Find the import line containing this function name
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("import") || !line.contains(fn_name) {
            continue;
        }

        if whole_module {
            // Insert `trusted ` after `import `
            let import_pos = line.find("import")?;
            let after_import = import_pos + "import".len();
            let pos = Position {
                line: line_idx as u32,
                character: after_import as u32,
            };
            return Some(TextEdit {
                range: Range {
                    start: pos,
                    end: pos,
                },
                new_text: " trusted".to_string(),
            });
        } else {
            // Insert `trusted ` before the function name inside { ... }
            let brace_start = line.find('{')?;
            let content_after_brace = &line[brace_start + 1..];
            // Find fn_name in the content after the brace, ensuring it's a word boundary
            let name_in_braces = content_after_brace.find(fn_name)?;
            let insert_col = brace_start + 1 + name_in_braces;
            let pos = Position {
                line: line_idx as u32,
                character: insert_col as u32,
            };
            return Some(TextEdit {
                range: Range {
                    start: pos,
                    end: pos,
                },
                new_text: "trusted ".to_string(),
            });
        }
    }

    None
}

// ── Completion heuristic helpers ─────────────────────────────────

use super::symbols::SymbolIndex;

/// Detect if cursor is inside a `match expr { ... }` block.
/// If so, look up the matched expression's type and return its variant names.
pub(super) fn detect_match_context(
    source: &str,
    offset: usize,
    index: &SymbolIndex,
) -> Option<Vec<String>> {
    let before = &source[..offset];

    // Find the innermost unclosed `match ... {` before the cursor
    // Scan backwards for `{` that belongs to a match expression
    let mut brace_depth: i32 = 0;
    let mut search_pos = before.len();

    while search_pos > 0 {
        search_pos -= 1;
        let ch = before.as_bytes()[search_pos];
        match ch {
            b'}' => brace_depth += 1,
            b'{' => {
                if brace_depth == 0 {
                    // This is an unmatched open brace — check if preceded by `match <expr>`
                    let before_brace = before[..search_pos].trim_end();
                    // Extract the expression between `match` and `{`
                    if let Some(match_pos) = before_brace.rfind("match ") {
                        let expr_text = before_brace[match_pos + 6..].trim();
                        // Look up the expression in the symbol index to find its type
                        let variants = find_variants_for_expr(expr_text, index);
                        if !variants.is_empty() {
                            return Some(variants);
                        }
                    }
                    // Not a match block, stop searching
                    return None;
                }
                brace_depth -= 1;
            }
            _ => {}
        }
    }

    None
}

/// Given a match expression text, try to find variant names for it.
/// Looks up the expression as a type name in the symbol index.
fn find_variants_for_expr(expr: &str, index: &SymbolIndex) -> Vec<String> {
    // The expr could be a variable name; look for a type with the same name
    // or look for a type whose variants are in the index
    let expr = expr.trim();

    // Strategy: check if expr matches a type name directly
    let type_syms = index.find_by_name(expr);
    for sym in &type_syms {
        if sym.kind == SymbolKind::TYPE_PARAMETER {
            // Found a type — collect its variants from the index
            let prefix = format!("{}.", expr);
            let variants: Vec<String> = index
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::ENUM_MEMBER && s.detail.starts_with(&prefix))
                .map(|s| s.name.clone())
                .collect();
            if !variants.is_empty() {
                return variants;
            }
        }
    }

    // Strategy 2: the expr might be a variable — look for all ENUM_MEMBER symbols
    // This is a best-effort fallback
    let all_variants: Vec<String> = index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::ENUM_MEMBER)
        .map(|s| s.name.clone())
        .collect();

    // Only return if there are some variants to suggest
    if !all_variants.is_empty() {
        return all_variants;
    }

    Vec::new()
}

/// Detect if cursor is inside a JSX opening tag (e.g., `<button on|`).
pub(super) fn is_in_jsx_tag(source: &str, offset: usize) -> bool {
    let before = &source[..offset];

    // Scan backwards for `<` that isn't closed by `>`
    let mut angle_depth: i32 = 0;
    for ch in before.chars().rev() {
        match ch {
            '>' => angle_depth += 1,
            '<' => {
                if angle_depth == 0 {
                    // Found an unclosed `<` — we're inside a tag
                    return true;
                }
                angle_depth -= 1;
            }
            _ => {}
        }
    }
    false
}

/// Generate JSX attribute completion items.
pub(super) fn jsx_attribute_completions(prefix: &str) -> Vec<CompletionItem> {
    let event_handlers = [
        "onClick",
        "onChange",
        "onKeyDown",
        "onSubmit",
        "onFocus",
        "onBlur",
        "onMouseEnter",
        "onMouseLeave",
        "onInput",
        "onKeyUp",
        "onKeyPress",
    ];

    let common_attrs = [
        "className",
        "id",
        "style",
        "key",
        "ref",
        "disabled",
        "type",
        "value",
        "placeholder",
        "href",
        "src",
        "alt",
        "title",
        "name",
        "role",
        "tabIndex",
        "autoFocus",
        "checked",
        "readOnly",
        "required",
        "hidden",
    ];

    let mut items = Vec::new();

    for attr in event_handlers.iter().chain(common_attrs.iter()) {
        if !prefix.is_empty() && !attr.starts_with(prefix) {
            continue;
        }
        let is_event = attr.starts_with("on");
        items.push(CompletionItem {
            label: attr.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(if is_event {
                "event handler".to_string()
            } else {
                "JSX attribute".to_string()
            }),
            insert_text: Some(format!("{attr}={{$1}}")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });
    }

    items
}

/// Detect if cursor is inside a lambda body used as an event handler callback,
/// and provide event-type completions (e.g., `e.target`, `e.preventDefault()`).
pub(super) fn lambda_event_completions(
    source: &str,
    offset: usize,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    let before = &source[..offset];

    // Check if we're typing after a `.` on an expression chain
    let dot_pos = before.rfind('.')?;
    let before_dot = before[..dot_pos].trim_end();

    // Extract the full dotted expression chain backwards (e.g., "e.target" or "e")
    // Find where the expression chain starts (first non-word, non-dot character)
    let chain_start = before_dot
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let chain = &before_dot[chain_start..];

    if chain.is_empty() {
        return None;
    }

    // Split the chain by dots to get root param and path
    let parts: Vec<&str> = chain.split('.').collect();
    let param_name = parts[0];

    if param_name.is_empty() {
        return None;
    }

    // Check if this param is a lambda parameter by scanning backwards for `|param|`
    let pre_chain = &before[..chain_start];
    let pipe_pattern = format!("|{param_name}|");
    if !pre_chain.contains(&pipe_pattern) {
        let alt_pattern = format!("|{param_name}");
        if !pre_chain.contains(&alt_pattern) {
            return None;
        }
    }

    // Now check if this lambda is used as an event handler callback
    let event_handler_attrs = [
        "onClick",
        "onChange",
        "onKeyDown",
        "onSubmit",
        "onFocus",
        "onBlur",
        "onMouseEnter",
        "onMouseLeave",
        "onInput",
        "onKeyUp",
        "onKeyPress",
    ];

    // Find the `={|` pattern before the lambda
    let lambda_start = before.rfind(&format!("|{param_name}|"))?;
    let before_lambda = before[..lambda_start].trim_end();
    let before_eq = before_lambda.strip_suffix('{')?;
    let before_eq = before_eq.trim_end().strip_suffix('=')?;
    let attr_name_end = before_eq.trim_end();

    // Extract the attribute name
    let attr_start = attr_name_end
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let attr_name = &attr_name_end[attr_start..];

    if !event_handler_attrs.contains(&attr_name) {
        return None;
    }

    // Determine completion level from the dot chain
    // parts[0] = param_name, parts[1..] = property path so far
    // dot_count = number of dots including the trailing one we're completing after
    let dot_count = parts.len(); // e.g., ["e"] = 1 dot (e.), ["e", "target"] = 2 dots (e.target.)

    let mut items = Vec::new();

    if dot_count == 1 {
        // First level: e.target, e.preventDefault(), etc.
        let event_props = [
            ("target", "EventTarget", false),
            ("currentTarget", "EventTarget", false),
            ("type", "string", false),
            ("preventDefault()", "void", true),
            ("stopPropagation()", "void", true),
            ("key", "string", false),
            ("bubbles", "boolean", false),
            ("defaultPrevented", "boolean", false),
            ("timeStamp", "number", false),
        ];

        for (name, ty, is_method) in &event_props {
            if !prefix.is_empty() && !name.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(if *is_method {
                    CompletionItemKind::METHOD
                } else {
                    CompletionItemKind::PROPERTY
                }),
                detail: Some(ty.to_string()),
                insert_text: Some(name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    } else if dot_count == 2 && parts.get(1) == Some(&"target") {
        // Second level: e.target.value, e.target.checked, etc.
        let target_props = [
            ("value", "string"),
            ("checked", "boolean"),
            ("name", "string"),
            ("id", "string"),
            ("tagName", "string"),
            ("className", "string"),
            ("textContent", "string"),
            ("innerHTML", "string"),
            ("disabled", "boolean"),
        ];

        for (name, ty) in &target_props {
            if !prefix.is_empty() && !name.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(ty.to_string()),
                insert_text: Some(name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    if items.is_empty() { None } else { Some(items) }
}
