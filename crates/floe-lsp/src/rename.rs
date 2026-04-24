use std::collections::HashMap;

use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;

use floe_core::lexer::span::Span;
use floe_core::reference::ReferenceTracker;

use super::{
    Document, FloeLsp, offset_to_range, position_to_offset, word_at_offset, word_range_at_offset,
};

impl FloeLsp {
    pub(super) async fn handle_prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&params.text_document.uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, params.position);
        let word = word_at_offset(&doc.content, offset);
        if word.is_empty() {
            return Ok(None);
        }

        let Some((name_start, name_end)) = renameable_word_range(doc, offset, word) else {
            return Ok(None);
        };

        Ok(Some(PrepareRenameResponse::Range(offset_to_range(
            &doc.content,
            name_start,
            name_end,
        ))))
    }

    pub(super) async fn handle_rename(
        &self,
        params: RenameParams,
    ) -> Result<Option<WorkspaceEdit>> {
        if !is_valid_identifier(&params.new_name) {
            return Err(Error::invalid_params(format!(
                "`{}` is not a valid identifier",
                params.new_name
            )));
        }

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

        if resolve_def_span(&doc.references, offset, word).is_none() {
            return Ok(None);
        }

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for_each_symbol_site(&docs, word, true, |site_uri, source, start, end| {
            changes.entry(site_uri.clone()).or_default().push(TextEdit {
                range: offset_to_range(source, start, end),
                new_text: params.new_name.clone(),
            });
        });

        if changes.is_empty() {
            return Ok(None);
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }
}

/// Resolve the definition span the cursor binds to.
///
/// Two cases: the cursor is on a recorded reference (tracker maps it directly),
/// or the cursor is on the definition's own name. Definition spans cover the
/// whole declaration, so the second case also requires the cursor word to
/// match the registered name — otherwise any cursor inside an item body
/// would resolve to that item's def.
pub(super) fn resolve_def_span(refs: &ReferenceTracker, offset: usize, word: &str) -> Option<Span> {
    if let Some(def_span) = refs.definition_at_offset(offset) {
        return Some(def_span);
    }
    let def_span = refs.definition_for_name(word)?;
    (offset >= def_span.start && offset <= def_span.end).then_some(def_span)
}

/// Locate the name token within a definition's span.
///
/// Definition spans cover the entire item (e.g. `fn foo(...) = { ... }`),
/// not just the identifier. The name is the first occurrence of `name` inside
/// that span — for any well-formed declaration the name appears before the
/// body, so a recursive use like `fn foo() = foo()` still finds the def site.
fn name_range_in_def(source: &str, def_span: Span, name: &str) -> Option<(usize, usize)> {
    let end = def_span.end.min(source.len());
    if def_span.start >= end {
        return None;
    }
    let rel = source[def_span.start..end].find(name)?;
    let start = def_span.start + rel;
    Some((start, start + name.len()))
}

/// Visit every occurrence of `word` across open documents that the
/// reference tracker can resolve — definition name plus all uses, in
/// every doc that has a same-named registered symbol. Each importing
/// module rebinds the source symbol locally, so walking every doc's
/// tracker covers both intra-file and cross-file occurrences without a
/// separate import-resolution pass.
pub(super) fn for_each_symbol_site<F>(
    docs: &HashMap<Url, Document>,
    word: &str,
    include_decl: bool,
    mut visit: F,
) where
    F: FnMut(&Url, &str, usize, usize),
{
    for (other_uri, other_doc) in docs.iter() {
        let Some(other_def) = other_doc.references.definition_for_name(word) else {
            continue;
        };
        if include_decl && let Some((s, e)) = name_range_in_def(&other_doc.content, other_def, word)
        {
            visit(other_uri, &other_doc.content, s, e);
        }
        for ref_span in other_doc.references.find_references(other_def) {
            visit(other_uri, &other_doc.content, ref_span.start, ref_span.end);
        }
    }
}

fn renameable_word_range(doc: &Document, offset: usize, word: &str) -> Option<(usize, usize)> {
    let def_span = resolve_def_span(&doc.references, offset, word)?;
    if let Some(name_range) = name_range_in_def(&doc.content, def_span, word)
        && offset >= name_range.0
        && offset <= name_range.1
    {
        return Some(name_range);
    }
    word_range_at_offset(&doc.content, offset)
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(start: usize, end: usize) -> Span {
        Span::new(start, end, 1, start + 1)
    }

    #[test]
    fn name_range_in_def_finds_first_occurrence() {
        let src = "fn foo() = foo()";
        let def = sp(0, src.len());
        assert_eq!(name_range_in_def(src, def, "foo"), Some((3, 6)));
    }

    #[test]
    fn resolve_def_span_via_reference_tracker() {
        let mut refs = ReferenceTracker::new();
        let def = sp(0, 20);
        refs.register_definition("foo", def);
        refs.record(def, sp(30, 33));

        assert_eq!(resolve_def_span(&refs, 31, "foo"), Some(def));
    }

    #[test]
    fn resolve_def_span_via_cursor_on_definition() {
        let mut refs = ReferenceTracker::new();
        let def = sp(0, 20);
        refs.register_definition("foo", def);

        assert_eq!(resolve_def_span(&refs, 5, "foo"), Some(def));
    }

    #[test]
    fn resolve_def_span_rejects_word_mismatch() {
        let mut refs = ReferenceTracker::new();
        let def = sp(0, 20);
        refs.register_definition("foo", def);

        // Otherwise every identifier inside an item body would resolve to
        // that item's definition.
        assert_eq!(resolve_def_span(&refs, 10, "bar"), None);
    }

    #[test]
    fn valid_identifier_accepts_normal_names() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_foo"));
        assert!(is_valid_identifier("foo_bar2"));
    }

    #[test]
    fn valid_identifier_rejects_bad_input() {
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("2foo"));
        assert!(!is_valid_identifier("foo bar"));
        assert!(!is_valid_identifier("foo-bar"));
    }
}
