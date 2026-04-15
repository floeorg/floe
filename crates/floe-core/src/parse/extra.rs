//! Storing comments as byte spans in a side channel (rather than attaching
//! them to AST nodes) is what lets the formatter walk the tree without any
//! risk of filtering comments out.

use crate::lexer::token::{Token, TokenKind};

/// A byte-only source span. Intentionally narrower than [`crate::lexer::span::Span`] —
/// it carries just the start/end byte offsets because `ModuleExtra` only needs
/// to point at ranges of source text. Line/column resolution is done on demand
/// through [`crate::line_numbers::LineNumbers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SrcSpan {
    pub start: u32,
    pub end: u32,
}

impl SrcSpan {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Returns `true` if `offset` is within `[start, end)`.
    pub fn contains(&self, offset: u32) -> bool {
        offset >= self.start && offset < self.end
    }
}

/// Parse-time side-channel for a single module.
///
/// All vectors are kept in ascending order of `start`, which lets callers use
/// binary search to answer "what appears between two AST nodes?" queries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleExtra {
    /// Regular comments: `// ...` and `/* ... */`.
    pub comments: Vec<SrcSpan>,
    /// Doc comments: `/// ...`. Attach to the following declaration.
    pub doc_comments: Vec<SrcSpan>,
    /// Module-level doc comments: `//// ...`. Attach to the whole module.
    pub module_comments: Vec<SrcSpan>,
    /// Byte offsets of the terminating newline of each blank line. A blank
    /// line is a line whose content between its start and its trailing `\n`
    /// is entirely whitespace. Recording the terminating newline (rather than
    /// the one that opens the line) gives each blank line a unique offset and
    /// lets "is there a blank line between X and Y?" be a simple range query.
    pub empty_lines: Vec<u32>,
    /// Byte offsets of every `\n` in the source, in ascending order. Used by
    /// the formatter to decide whether two spans straddle a line boundary.
    pub new_lines: Vec<u32>,
}

impl ModuleExtra {
    pub fn new() -> Self {
        Self::default()
    }

    /// `tokens` must include trivia — the ones from `Lexer::tokenize_with_trivia`.
    /// Newlines and empty lines are collected by walking the source bytes
    /// directly rather than the whitespace tokens, so interior newlines of
    /// multi-line strings and templates are still tracked.
    pub fn from_tokens(source: &str, tokens: &[Token]) -> Self {
        let mut extra = Self::new();
        collect_newlines(source, &mut extra);
        classify_comments(source, tokens, &mut extra);
        extra
    }

    /// Returns the earliest comment whose `start` lies in `[start, end)`.
    /// The range is half-open so a comment starting exactly at `end` is
    /// *not* included.
    pub fn first_comment_between(&self, start: u32, end: u32) -> Option<SrcSpan> {
        [&self.comments, &self.doc_comments, &self.module_comments]
            .into_iter()
            .filter_map(|list| first_starting_in(list, start, end))
            .min_by_key(|s| s.start)
    }
}

/// Binary-search for the first span in `list` whose `start` is in `[start, end)`.
fn first_starting_in(list: &[SrcSpan], start: u32, end: u32) -> Option<SrcSpan> {
    let idx = list.partition_point(|s| s.start < start);
    list.get(idx).copied().filter(|s| s.start < end)
}

fn collect_newlines(source: &str, extra: &mut ModuleExtra) {
    let bytes = source.as_bytes();
    // Rough heuristic: ~40 bytes per line. Avoids repeated Vec reallocations on
    // files with thousands of lines. Same rule used by LineNumbers::new.
    let capacity = bytes.len() / 40 + 1;
    extra.new_lines.reserve(capacity);

    // Single pass: for each `\n`, record it and check whether the segment
    // since the previous newline (or BOF) was entirely whitespace. If so the
    // current newline terminates a blank line.
    let mut segment_blank = true;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            if segment_blank {
                extra.empty_lines.push(i as u32);
            }
            extra.new_lines.push(i as u32);
            segment_blank = true;
        } else if !b.is_ascii_whitespace() {
            segment_blank = false;
        }
    }
}

fn classify_comments(source: &str, tokens: &[Token], extra: &mut ModuleExtra) {
    for token in tokens {
        let span = SrcSpan::new(token.span.start as u32, token.span.end as u32);
        match token.kind {
            TokenKind::Comment => {
                let text = &source[token.span.start..token.span.end];
                if text.starts_with("////") {
                    extra.module_comments.push(span);
                } else if text.starts_with("///") {
                    extra.doc_comments.push(span);
                } else {
                    extra.comments.push(span);
                }
            }
            TokenKind::BlockComment => {
                extra.comments.push(span);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn build(source: &str) -> ModuleExtra {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        ModuleExtra::from_tokens(source, &tokens)
    }

    fn span(start: u32, end: u32) -> SrcSpan {
        SrcSpan::new(start, end)
    }

    #[test]
    fn src_span_basics() {
        let s = span(3, 7);
        assert_eq!(s.len(), 4);
        assert!(!s.is_empty());
        assert!(s.contains(3));
        assert!(s.contains(6));
        assert!(!s.contains(7));
        assert!(!s.contains(2));

        let empty = span(5, 5);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn empty_source_produces_empty_extra() {
        let extra = build("");
        assert!(extra.comments.is_empty());
        assert!(extra.doc_comments.is_empty());
        assert!(extra.module_comments.is_empty());
        assert!(extra.empty_lines.is_empty());
        assert!(extra.new_lines.is_empty());
    }

    #[test]
    fn source_without_comments_has_empty_comment_lists() {
        let extra = build("const x = 1\nconst y = 2\n");
        assert!(extra.comments.is_empty());
        assert!(extra.doc_comments.is_empty());
        assert!(extra.module_comments.is_empty());
    }

    #[test]
    fn classifies_line_doc_and_module_comments() {
        //                              1111111111222222222233333333334444444444
        //                    0123456789012345678901234567890123456789012345678
        let source = "//// module doc\n/// item doc\n// regular\nconst x = 1\n";

        let extra = build(source);

        assert_eq!(extra.module_comments.len(), 1);
        assert_eq!(extra.doc_comments.len(), 1);
        assert_eq!(extra.comments.len(), 1);

        assert_eq!(extra.module_comments[0].start, 0);
        assert_eq!(extra.module_comments[0].end, 15);
        assert!(source[..extra.module_comments[0].end as usize].starts_with("////"));

        assert_eq!(extra.doc_comments[0].start, 16);
        assert_eq!(extra.doc_comments[0].end, 28);

        assert_eq!(extra.comments[0].start, 29);
        assert_eq!(extra.comments[0].end, 39);
    }

    #[test]
    fn block_comment_classified_as_regular() {
        let source = "/* a block comment */ const x = 1";
        let extra = build(source);
        assert_eq!(extra.comments.len(), 1);
        assert!(extra.doc_comments.is_empty());
        assert!(extra.module_comments.is_empty());
        assert_eq!(extra.comments[0].start, 0);
        assert_eq!(extra.comments[0].end, 21);
    }

    #[test]
    fn comments_are_recorded_in_source_order() {
        let source = "// one\n// two\n// three\nconst x = 1";
        let extra = build(source);
        assert_eq!(extra.comments.len(), 3);
        let starts: Vec<u32> = extra.comments.iter().map(|s| s.start).collect();
        assert_eq!(starts, vec![0, 7, 14]);
        for window in extra.comments.windows(2) {
            assert!(window[0].start < window[1].start);
        }
    }

    #[test]
    fn tracks_every_newline() {
        let source = "a\nb\nc\n";
        let extra = build(source);
        assert_eq!(extra.new_lines, vec![1, 3, 5]);
    }

    #[test]
    fn empty_lines_between_statements_are_preserved() {
        // "a\n\nb\n" — one blank line between "a" and "b".
        //     ^ pos 1 is the newline that ends line 1
        //      ^ pos 2 is the terminating newline of the blank line
        let source = "a\n\nb\n";
        let extra = build(source);
        assert_eq!(extra.new_lines, vec![1, 2, 4]);
        assert_eq!(extra.empty_lines, vec![2]);
    }

    #[test]
    fn multiple_consecutive_empty_lines() {
        // Three newlines in a row = two blank lines, with terminators at
        // positions 2 and 3.
        let source = "a\n\n\nb";
        let extra = build(source);
        assert_eq!(extra.new_lines, vec![1, 2, 3]);
        assert_eq!(extra.empty_lines, vec![2, 3]);
    }

    #[test]
    fn blank_line_with_whitespace_still_counts() {
        let source = "a\n   \nb";
        let extra = build(source);
        // The " " run is tokenized as Whitespace, but we drive empty-line
        // detection from raw bytes so it's still found.
        assert_eq!(extra.new_lines, vec![1, 5]);
        assert_eq!(extra.empty_lines, vec![5]);
    }

    #[test]
    fn leading_blank_lines_are_recorded() {
        let source = "\n\nconst x = 1";
        let extra = build(source);
        assert_eq!(extra.new_lines, vec![0, 1]);
        // Both lines before the const are blank, so we record both terminators.
        assert_eq!(extra.empty_lines, vec![0, 1]);
    }

    #[test]
    fn first_comment_between_returns_none_when_empty() {
        let extra = ModuleExtra::new();
        assert_eq!(extra.first_comment_between(0, 100), None);
    }

    #[test]
    fn first_comment_between_finds_earliest_across_categories() {
        let mut extra = ModuleExtra::new();
        extra.comments.push(span(20, 25));
        extra.comments.push(span(60, 65));
        extra.doc_comments.push(span(10, 18));
        extra.doc_comments.push(span(50, 58));
        extra.module_comments.push(span(0, 8));

        // Whole range: earliest is the module comment at 0.
        assert_eq!(extra.first_comment_between(0, 100), Some(span(0, 8)));

        // Range that excludes the module comment: earliest is the doc at 10.
        assert_eq!(extra.first_comment_between(9, 100), Some(span(10, 18)));

        // Range that contains only the late regular comment.
        assert_eq!(extra.first_comment_between(59, 100), Some(span(60, 65)));

        // Empty range → nothing.
        assert_eq!(extra.first_comment_between(26, 49), None);

        // Exclusive end: a comment at exactly `end` does not match.
        assert_eq!(extra.first_comment_between(10, 10), None);
        // Inclusive start: a comment at exactly `start` does match.
        assert_eq!(extra.first_comment_between(10, 11), Some(span(10, 18)));
    }

    #[test]
    fn first_comment_between_uses_binary_search_correctness() {
        let mut extra = ModuleExtra::new();
        for i in 0..100u32 {
            extra.comments.push(span(i * 10, i * 10 + 5));
        }

        // Search in the middle of a densely packed range.
        assert_eq!(extra.first_comment_between(500, 600), Some(span(500, 505)));
        assert_eq!(extra.first_comment_between(501, 600), Some(span(510, 515)));
        // No match when the range falls between two comments.
        assert_eq!(extra.first_comment_between(505, 510), None);
    }

    #[test]
    fn comments_inside_strings_are_not_captured() {
        // The `//` inside a string literal must NOT be reported as a comment —
        // the lexer handles escaping, and we walk its tokens.
        let source = r#"const url = "https://floe.dev""#;
        let extra = build(source);
        assert!(extra.comments.is_empty());
    }
}
