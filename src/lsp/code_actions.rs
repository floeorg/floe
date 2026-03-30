use std::collections::HashMap;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::{FloeLsp, offset_to_position, position_to_offset};

impl FloeLsp {
    pub(super) async fn handle_code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
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
