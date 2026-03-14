use std::collections::HashMap;

use tower_lsp::LanguageServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::completion::is_pipe_context;
use super::completion::resolve_piped_type;
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
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```floe\n{}\n```", sym.detail),
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
