mod expr;
mod items;
mod jsx;
#[cfg(test)]
mod tests;

use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::parse::extra::{ModuleExtra, SrcSpan};
use crate::pretty::{self, Document};
use crate::syntax::{SyntaxKind, SyntaxNode};

/// Column budget the formatter targets. Exposed so other crates (the LSP
/// hover renderer in particular) can decide inline-vs-split with the same
/// threshold, instead of keeping a parallel number in sync by convention.
pub const MAX_WIDTH: usize = 100;

/// Format Floe source code. Returns `None` if the file has parse errors.
pub fn format(source: &str) -> Option<String> {
    let tokens = Lexer::new(source).tokenize_with_trivia();
    let extra = ModuleExtra::from_tokens(source, &tokens);
    let parse = CstParser::new(source, tokens).parse();
    if !parse.errors.is_empty() {
        return None;
    }
    let root = parse.syntax();
    let mut formatter = Formatter::new(source, extra);
    let doc = formatter.fmt_node(&root);
    let rendered = doc.pretty_print(MAX_WIDTH);
    let mut result = strip_trailing_whitespace(&rendered);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    while result.ends_with("\n\n") {
        result.pop();
    }
    Some(result)
}

fn strip_trailing_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, line) in s.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end());
    }
    out
}

/// A comment popped from the side-channel.
pub(crate) struct Comment {
    pub start: u32,
    pub end: u32,
    pub text: String,
}

struct ProgramState {
    first: bool,
    prev_kind: Option<SyntaxKind>,
    prev_was_comment: bool,
    prev_end: u32,
}

impl ProgramState {
    fn new() -> Self {
        Self {
            first: true,
            prev_kind: None,
            prev_was_comment: false,
            prev_end: 0,
        }
    }
}

pub(crate) struct Formatter<'src> {
    source: &'src str,
    /// Three category lists from `ModuleExtra` merged into one vector sorted
    /// by `start` so the formatter only has to advance a single cursor.
    comments: Vec<SrcSpan>,
    cursor: usize,
    empty_lines: Vec<u32>,
}

impl<'src> Formatter<'src> {
    fn new(source: &'src str, extra: ModuleExtra) -> Self {
        let mut comments = extra.comments;
        comments.extend(extra.doc_comments);
        comments.extend(extra.module_comments);
        comments.sort_by_key(|s| s.start);
        Self {
            source,
            comments,
            cursor: 0,
            empty_lines: extra.empty_lines,
        }
    }

    pub(crate) fn fmt_node(&mut self, node: &SyntaxNode) -> Document {
        match node.kind() {
            SyntaxKind::PROGRAM => self.fmt_program(node),
            SyntaxKind::ITEM => self.fmt_item(node),
            SyntaxKind::EXPR_ITEM => self.fmt_expr_item(node),
            SyntaxKind::IMPORT_DECL => self.fmt_import(node),
            SyntaxKind::REEXPORT_DECL => self.fmt_reexport(node),
            SyntaxKind::CONST_DECL => self.fmt_const(node),
            SyntaxKind::FUNCTION_DECL => self.fmt_function(node),
            SyntaxKind::USE_DECL => self.fmt_use_decl(node),
            SyntaxKind::TYPE_DECL => self.fmt_type_decl(node),
            SyntaxKind::BLOCK_EXPR => self.fmt_block(node),
            SyntaxKind::PIPE_EXPR => self.fmt_pipe(node),
            SyntaxKind::MATCH_EXPR => self.fmt_match(node),
            SyntaxKind::BINARY_EXPR => self.fmt_binary(node),
            SyntaxKind::UNARY_EXPR => self.fmt_unary(node),
            SyntaxKind::CALL_EXPR => self.fmt_call(node),
            SyntaxKind::TAGGED_TEMPLATE_EXPR => self.fmt_tagged_template(node),
            SyntaxKind::CONSTRUCT_EXPR => self.fmt_construct(node),
            SyntaxKind::BRACE_CONSTRUCT_EXPR => self.fmt_brace_construct(node),
            SyntaxKind::MEMBER_EXPR => self.fmt_member(node),
            SyntaxKind::INDEX_EXPR => self.fmt_index(node),
            SyntaxKind::UNWRAP_EXPR => self.fmt_unwrap(node),
            SyntaxKind::ARROW_EXPR => self.fmt_arrow(node),
            SyntaxKind::RETURN_EXPR => self.fmt_return(node),
            SyntaxKind::GROUPED_EXPR => self.fmt_grouped(node),
            SyntaxKind::TUPLE_EXPR => self.fmt_tuple(node),
            SyntaxKind::ARRAY_EXPR => self.fmt_array(node),
            SyntaxKind::VALUE_EXPR => self.fmt_wrapper_expr(node),
            SyntaxKind::PARSE_EXPR => self.fmt_parse_expr(node),
            SyntaxKind::MOCK_EXPR => self.fmt_mock_expr(node),
            SyntaxKind::JSX_ELEMENT => self.fmt_jsx(node),
            SyntaxKind::TYPE_DEF_UNION => self.fmt_union(node),
            SyntaxKind::TYPE_DEF_RECORD => self.fmt_record_def(node),
            SyntaxKind::TYPE_DEF_ALIAS => self.fmt_type_alias_def(node),
            SyntaxKind::TYPE_EXPR => self.fmt_type_expr(node),
            SyntaxKind::FOR_BLOCK => self.fmt_for_block(node),
            SyntaxKind::IMPL_BLOCK => self.fmt_impl_block(node),
            SyntaxKind::TRAIT_DECL => self.fmt_trait_decl(node),
            SyntaxKind::COLLECT_EXPR | SyntaxKind::TEST_BLOCK | SyntaxKind::ASSERT_EXPR => {
                self.fmt_verbatim(node)
            }
            _ => self.fmt_verbatim(node),
        }
    }

    // ── Program ─────────────────────────────────────────────────

    fn fmt_program(&mut self, node: &SyntaxNode) -> Document {
        let children: Vec<_> = node.children().collect();
        let mut docs = Vec::new();
        let mut state = ProgramState::new();

        for child in &children {
            let child_start: u32 = child.text_range().start().into();
            let comments = self.pop_comments_before(child_start);
            self.emit_program_comments(&mut docs, comments, &mut state);

            let child_inner_kind = self.inner_decl_kind(child);

            if !state.first {
                if state.prev_was_comment {
                    docs.push(pretty::line());
                    docs.push(pretty::line());
                } else {
                    let is_import_like = |k: SyntaxKind| {
                        matches!(k, SyntaxKind::IMPORT_DECL | SyntaxKind::REEXPORT_DECL)
                    };
                    let want_blank = !matches!(
                        (state.prev_kind, child_inner_kind),
                        (Some(a), Some(b)) if is_import_like(a) && is_import_like(b)
                    );
                    docs.push(pretty::line());
                    if want_blank {
                        docs.push(pretty::line());
                    }
                }
            }

            docs.push(self.fmt_node(child));

            state.prev_was_comment = false;
            state.prev_kind = child_inner_kind;
            state.prev_end = child.text_range().end().into();
            state.first = false;
        }

        let remaining = self.pop_comments_before(u32::MAX);
        self.emit_program_comments(&mut docs, remaining, &mut state);

        pretty::concat(docs)
    }

    fn emit_program_comments(
        &self,
        docs: &mut Vec<Document>,
        comments: Vec<Comment>,
        state: &mut ProgramState,
    ) {
        for c in comments {
            let had_blank_before = self.has_empty_line_between(state.prev_end, c.start);
            if !state.first && (!state.prev_was_comment || had_blank_before) {
                docs.push(pretty::line());
                docs.push(pretty::line());
            } else if state.prev_was_comment {
                docs.push(pretty::line());
            }
            docs.push(pretty::str(c.text));
            state.first = false;
            state.prev_was_comment = true;
            state.prev_end = c.end;
        }
    }

    #[allow(clippy::unused_self)]
    fn inner_decl_kind(&self, node: &SyntaxNode) -> Option<SyntaxKind> {
        match node.kind() {
            SyntaxKind::ITEM => node.children().next().map(|c| c.kind()),
            SyntaxKind::EXPR_ITEM => Some(SyntaxKind::EXPR_ITEM),
            other => Some(other),
        }
    }

    pub(crate) fn fmt_verbatim(&self, node: &SyntaxNode) -> Document {
        let range = node.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let text = self.source[start..end].trim();
        pretty::str(text)
    }

    // ── Comment handling ────────────────────────────────────────

    /// Drain comments whose `start` lies in `[from, to)`, advancing the
    /// cursor past `to`. Comments before `from` that the cursor still points
    /// at are skipped silently — they should have been emitted by an
    /// enclosing context already (or are inside a node that handles its own
    /// comments via CST traversal).
    fn drain_comments(&mut self, from: u32, to: u32) -> Vec<Comment> {
        let mut results = Vec::new();
        while self.cursor < self.comments.len() {
            let span = self.comments[self.cursor];
            if span.start >= to {
                break;
            }
            if span.start >= from {
                results.push(Comment {
                    start: span.start,
                    end: span.end,
                    text: self.source[span.start as usize..span.end as usize].to_string(),
                });
            }
            self.cursor += 1;
        }
        results
    }

    pub(crate) fn pop_comments_before(&mut self, to: u32) -> Vec<Comment> {
        self.drain_comments(0, to)
    }

    pub(crate) fn pop_comments_in_range(&mut self, from: u32, to: u32) -> Vec<Comment> {
        self.drain_comments(from, to)
    }

    pub(crate) fn advance_comment_cursor_to(&mut self, pos: u32) {
        let _ = self.drain_comments(0, pos);
    }

    /// Check if the source has a blank line in `(from, to)` (exclusive both ends).
    pub(crate) fn has_empty_line_between(&self, from: u32, to: u32) -> bool {
        let idx = self.empty_lines.partition_point(|&off| off <= from);
        idx < self.empty_lines.len() && self.empty_lines[idx] < to
    }

    // ── CST query helpers ───────────────────────────────────────

    #[allow(clippy::unused_self)]
    pub(crate) fn has_token(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn first_ident(&self, node: &SyntaxNode) -> Option<String> {
        node.children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
    }

    pub(crate) fn collect_idents(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents_until(node, |_| false)
    }

    pub(crate) fn collect_idents_before_lparen(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents_until(node, |k| k == SyntaxKind::L_PAREN)
    }

    pub(crate) fn collect_idents_before_eq(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents_until(node, |k| k == SyntaxKind::EQUAL)
    }

    pub(crate) fn collect_idents_before_colon_or_eq(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents_until(node, |k| k == SyntaxKind::EQUAL || k == SyntaxKind::COLON)
    }

    #[allow(clippy::unused_self)]
    fn collect_idents_until(
        &self,
        node: &SyntaxNode,
        stop: impl Fn(SyntaxKind) -> bool,
    ) -> Vec<String> {
        let mut idents = Vec::new();
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if stop(tok.kind()) {
                    break;
                }
                if tok.kind() == SyntaxKind::IDENT {
                    idents.push(tok.text().to_string());
                }
            }
        }
        idents
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn collect_destructure_fields(&self, node: &SyntaxNode) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current_field: Option<String> = None;
        let mut saw_colon = false;

        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::EQUAL => break,
                    SyntaxKind::IDENT => {
                        if saw_colon {
                            if let Some(ref mut field) = current_field {
                                field.push_str(": ");
                                field.push_str(tok.text());
                            }
                            saw_colon = false;
                        } else {
                            if let Some(field) = current_field.take() {
                                fields.push(field);
                            }
                            current_field = Some(tok.text().to_string());
                        }
                    }
                    SyntaxKind::COLON => saw_colon = true,
                    SyntaxKind::COMMA => {
                        if let Some(field) = current_field.take() {
                            fields.push(field);
                        }
                        saw_colon = false;
                    }
                    _ => {}
                }
            }
        }
        if let Some(field) = current_field {
            fields.push(field);
        }
        fields
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn has_paren_destructuring(&self, node: &SyntaxNode) -> bool {
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::L_PAREN {
                    return true;
                }
                if tok.kind() == SyntaxKind::EQUAL {
                    return false;
                }
            }
        }
        false
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn has_brace_destructuring(&self, node: &SyntaxNode) -> bool {
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::L_BRACE {
                    return true;
                }
                if tok.kind() == SyntaxKind::EQUAL {
                    return false;
                }
            }
        }
        false
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn find_expr_after_eq(&self, node: &SyntaxNode) -> Option<SyntaxNode> {
        let mut past_eq = false;
        for child_or_tok in node.children_with_tokens() {
            if let Some(tok) = child_or_tok.as_token()
                && tok.kind() == SyntaxKind::EQUAL
            {
                past_eq = true;
            }
            if past_eq
                && let Some(child) = child_or_tok.into_node()
                && child.kind() != SyntaxKind::TYPE_EXPR
            {
                return Some(child);
            }
        }
        None
    }

    /// Format the expression after `=`
    pub(crate) fn fmt_expr_after_eq(&mut self, node: &SyntaxNode) -> Document {
        if let Some(expr) = self.find_expr_after_eq(node) {
            return self.fmt_node(&expr);
        }
        // Fall back to token expression
        let mut past_eq = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::EQUAL {
                    past_eq = true;
                    continue;
                }
                if past_eq && !tok.kind().is_trivia() {
                    return pretty::str(tok.text());
                }
            }
            if let Some(child) = t.into_node()
                && past_eq
            {
                return self.fmt_node(&child);
            }
        }
        pretty::nil()
    }

    /// Format expression after a keyword
    pub(crate) fn fmt_expr_after_keyword(
        &mut self,
        node: &SyntaxNode,
        keyword: SyntaxKind,
    ) -> Document {
        let mut past_kw = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == keyword {
                    past_kw = true;
                    continue;
                }
                if past_kw && !tok.kind().is_trivia() {
                    return pretty::str(tok.text());
                }
            }
            if let Some(child) = t.into_node()
                && past_kw
            {
                return self.fmt_node(&child);
            }
        }
        pretty::nil()
    }

    /// Get the first non-trivia, non-delimiter token text
    #[allow(clippy::unused_self)]
    pub(crate) fn first_content_token(&self, node: &SyntaxNode) -> Option<String> {
        node.children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| {
                !t.kind().is_trivia()
                    && !matches!(
                        t.kind(),
                        SyntaxKind::L_PAREN
                            | SyntaxKind::R_PAREN
                            | SyntaxKind::L_BRACKET
                            | SyntaxKind::R_BRACKET
                            | SyntaxKind::L_BRACE
                            | SyntaxKind::R_BRACE
                            | SyntaxKind::COMMA
                    )
            })
            .map(|t| t.text().to_string())
    }

    /// Check if a JSX element will format as multiline (heuristic).
    pub(crate) fn is_multiline_jsx(&self, node: &SyntaxNode) -> bool {
        if node.kind() != SyntaxKind::JSX_ELEMENT {
            return false;
        }
        if self.jsx_has_multiline_props(node) {
            return true;
        }
        let has_element_child = node.children().any(|c| c.kind() == SyntaxKind::JSX_ELEMENT);
        if has_element_child {
            return true;
        }
        let meaningful_children: Vec<_> = node
            .children()
            .filter(|c| {
                matches!(
                    c.kind(),
                    SyntaxKind::JSX_ELEMENT | SyntaxKind::JSX_EXPR_CHILD | SyntaxKind::JSX_TEXT
                ) && !(c.kind() == SyntaxKind::JSX_TEXT && c.text().to_string().trim().is_empty())
            })
            .collect();
        if meaningful_children.len() > 1 {
            return true;
        }
        if meaningful_children.len() == 1
            && meaningful_children[0].kind() == SyntaxKind::JSX_EXPR_CHILD
        {
            return self.jsx_expr_is_multiline_heuristic(&meaningful_children[0]);
        }
        false
    }

    /// Check if a JSX expression child contains multiline constructs.
    fn jsx_expr_is_multiline_heuristic(&self, node: &SyntaxNode) -> bool {
        node.descendants().any(|d| match d.kind() {
            SyntaxKind::MATCH_EXPR | SyntaxKind::BLOCK_EXPR => true,
            SyntaxKind::JSX_ELEMENT => self.jsx_has_multiline_props(&d),
            _ => false,
        })
    }
}
