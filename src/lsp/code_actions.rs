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
