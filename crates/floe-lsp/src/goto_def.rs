use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Url};

use super::{
    Document, FloeLsp, is_cursor_on_def_name, offset_to_range, position_to_offset, word_at_offset,
};

/// Search a single document's symbol index for `word`. When `cursor_offset`
/// is `Some`, the search is for the document the cursor is in: imports are
/// resolved to their source files, and the symbol is skipped if the cursor
/// sits on its own definition name. When `cursor_offset` is `None`, imports
/// are skipped entirely (we never want to land on someone else's import
/// rebinding when searching across files).
fn find_def_in_doc(
    doc: &Document,
    doc_uri: &Url,
    word: &str,
    cursor_offset: Option<usize>,
) -> Option<Location> {
    for sym in doc.index.find_by_name(word) {
        if let Some(source_spec) = &sym.import_source {
            if cursor_offset.is_some()
                && let Some(location) = FloeLsp::resolve_import_location(doc_uri, source_spec, word)
            {
                return Some(location);
            }
            continue;
        }

        if let Some(offset) = cursor_offset
            && is_cursor_on_def_name(&doc.content, offset, sym)
        {
            continue;
        }

        return Some(Location {
            uri: doc_uri.clone(),
            range: offset_to_range(&doc.content, sym.start, sym.end),
        });
    }
    None
}

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

        // Imports resolve through to the source file via the index path below;
        // a tracker hit on an import would land on the local rebinding.
        if let Some(def_span) = doc.references.definition_at_offset(offset)
            && !doc.index.covers_import(def_span.start, def_span.end)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: offset_to_range(&doc.content, def_span.start, def_span.end),
            })));
        }

        if let Some(location) = find_def_in_doc(doc, &uri, word, Some(offset)) {
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
        }

        for (other_uri, other_doc) in docs.iter() {
            if other_uri == &uri {
                continue;
            }
            if let Some(location) = find_def_in_doc(other_doc, other_uri, word, None) {
                return Ok(Some(GotoDefinitionResponse::Scalar(location)));
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
