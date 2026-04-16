use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::{FloeLsp, is_cursor_on_def_name, offset_to_range, position_to_offset, word_at_offset};

impl FloeLsp {
    pub(super) async fn handle_goto_definition(
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

        // Check if cursor is on an import path string — go-to-def opens the target file
        if let Some(import_path) = import_path_at_offset(&doc.content, offset)
            && let Some(location) = Self::resolve_import_path_location(&uri, &import_path)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
        }

        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            return Ok(None);
        }

        // Precise reference-tracker lookup: if the checker recorded the
        // identifier at this offset as a reference to a known definition,
        // jump straight there. Falls through to the name-based index
        // below for cases the tracker doesn't cover yet (imports, member
        // accesses).
        if let Some(def_span) = doc.references.definition_at_offset(offset) {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: offset_to_range(&doc.content, def_span.start, def_span.end),
            })));
        }

        // Search current document
        for sym in doc.index.find_by_name(word) {
            // If this symbol is an import, resolve to the source file.
            // Never return the import's own location — that would jump within
            // the current file instead of going to the definition.
            if let Some(source_spec) = &sym.import_source {
                if let Some(location) = Self::resolve_import_location(&uri, source_spec, word) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                }
                // Resolution failed — skip this symbol rather than returning
                // the import declaration's position in the current file.
                continue;
            }

            // Skip only when the cursor is on the definition name itself
            // (not anywhere in the item body). Find the name's position within
            // the declaration to do a precise check.
            if is_cursor_on_def_name(&doc.content, offset, sym) {
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
}

/// If the cursor offset is inside a string literal on an import line,
/// return the import path string (without quotes).
///
/// Matches lines like:
///   import { Foo } from "../types"
///   import { Bar } from "./bar"
pub(super) fn import_path_at_offset(source: &str, offset: usize) -> Option<String> {
    // Find the line containing the offset
    let before = &source[..offset];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = source[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(source.len());
    let line = &source[line_start..line_end];

    // Must be an import line
    let trimmed = line.trim();
    if !trimmed.starts_with("import") {
        return None;
    }

    // Find the string literal — after "from" if present, otherwise after "import"
    let search_after = if let Some(from_pos) = line.find("from") {
        from_pos + 4
    } else {
        // Bare import: `import "../todo"` — search after "import"
        line.find("import").unwrap_or(0) + 6
    };
    let after_keyword = &line[search_after..];

    // Find opening quote
    let quote_char;
    let quote_start;
    if let Some(pos) = after_keyword.find('"') {
        quote_char = '"';
        quote_start = search_after + pos;
    } else if let Some(pos) = after_keyword.find('\'') {
        quote_char = '\'';
        quote_start = search_after + pos;
    } else {
        return None;
    }

    // Find closing quote
    let after_open = &line[quote_start + 1..];
    let quote_end = after_open.find(quote_char)?;
    let string_content = &after_open[..quote_end];

    // Check that the cursor offset is within the string (including quotes)
    let abs_string_start = line_start + quote_start;
    let abs_string_end = line_start + quote_start + 1 + quote_end + 1; // inclusive of closing quote

    if offset >= abs_string_start && offset <= abs_string_end {
        Some(string_content.to_string())
    } else {
        None
    }
}
