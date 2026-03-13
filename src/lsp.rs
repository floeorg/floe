use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::checker::Checker;
use crate::diagnostic::{self as zs_diag, Severity};
use crate::parser::Parser;

/// State for an open document.
#[derive(Debug, Clone)]
struct Document {
    content: String,
}

/// The ZenScript Language Server.
pub struct ZenScriptLsp {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl ZenScriptLsp {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse and type-check a document, publishing diagnostics to the client.
    async fn publish_diagnostics(&self, uri: Url, source: &str) {
        let _filename = uri
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("unknown.zs");

        let diagnostics = match Parser::new(source).parse_program() {
            Err(parse_errors) => {
                let zs_diags = zs_diag::from_parse_errors(&parse_errors);
                self.convert_diagnostics(source, &zs_diags)
            }
            Ok(program) => {
                let check_diags = Checker::new().check(&program);
                self.convert_diagnostics(source, &check_diags)
            }
        };

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
}

/// Convert byte offsets to an LSP Range (line/character positions).
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

#[tower_lsp::async_trait]
impl LanguageServer for ZenScriptLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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

        self.documents.write().await.insert(
            uri.clone(),
            Document {
                content: content.clone(),
            },
        );

        self.publish_diagnostics(uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full sync — take the last change
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.documents.write().await.insert(
                uri.clone(),
                Document {
                    content: change.text.clone(),
                },
            );

            self.publish_diagnostics(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        // Find the word at the cursor position
        let offset = position_to_offset(&doc.content, position);
        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            return Ok(None);
        }

        // Basic hover: show the word and any type info we can infer
        let hover_text = match word {
            "Ok" => "```zenscript\nOk(value: T) -> Result<T, E>\n```\nWrap a success value in a Result.".to_string(),
            "Err" => "```zenscript\nErr(error: E) -> Result<T, E>\n```\nWrap an error value in a Result.".to_string(),
            "Some" => "```zenscript\nSome(value: T) -> Option<T>\n```\nWrap a value in an Option.".to_string(),
            "None" => "```zenscript\nNone -> Option<T>\n```\nRepresents the absence of a value.".to_string(),
            "match" => "```zenscript\nmatch expr { pattern -> body, ... }\n```\nExhaustive pattern matching expression.".to_string(),
            _ => return Ok(None),
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text,
            }),
            range: None,
        }))
    }
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

    // Find word boundaries
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

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Start the LSP server on stdin/stdout.
pub async fn run_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(ZenScriptLsp::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

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
        // Position (1, 3) should be offset 9 ('l' in 'world')
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
    fn banned_keyword_produces_parse_error() {
        let source = "let x = 42";
        let parse_result = Parser::new(source).parse_program();
        assert!(parse_result.is_err());
        let errs = parse_result.unwrap_err();
        let zs_diags = zs_diag::from_parse_errors(&errs);
        assert!(!zs_diags.is_empty());
        assert_eq!(zs_diags[0].severity, Severity::Error);
    }
}
