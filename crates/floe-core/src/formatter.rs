mod expr;
mod items;
mod jsx;
#[cfg(test)]
mod tests;

use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::parse::extra::ModuleExtra;
use crate::pretty::{self, Document};
use crate::syntax::{SyntaxKind, SyntaxNode};

const MAX_WIDTH: usize = 100;

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

pub(crate) struct Formatter<'src> {
    source: &'src str,
    extra: ModuleExtra,
    comment_cursor: usize,
    doc_comment_cursor: usize,
    module_comment_cursor: usize,
}

impl<'src> Formatter<'src> {
    fn new(source: &'src str, extra: ModuleExtra) -> Self {
        Self {
            source,
            extra,
            comment_cursor: 0,
            doc_comment_cursor: 0,
            module_comment_cursor: 0,
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
            SyntaxKind::TYPE_DECL => self.fmt_type_decl(node),
            SyntaxKind::BLOCK_EXPR => self.fmt_block(node),
            SyntaxKind::PIPE_EXPR => self.fmt_pipe(node),
            SyntaxKind::MATCH_EXPR => self.fmt_match(node),
            SyntaxKind::BINARY_EXPR => self.fmt_binary(node),
            SyntaxKind::UNARY_EXPR => self.fmt_unary(node),
            SyntaxKind::CALL_EXPR => self.fmt_call(node),
            SyntaxKind::CONSTRUCT_EXPR => self.fmt_construct(node),
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
        let mut first = true;
        let mut prev_kind: Option<SyntaxKind> = None;
        let mut prev_was_comment = false;
        let mut prev_end: u32 = 0;

        for child in &children {
            let child_start: u32 = child.text_range().start().into();

            // Pop comments before this child from the side-channel
            let comments = self.pop_comments_before(child_start);
            for (c_start, c_end, comment_text) in &comments {
                let had_blank_before = self.has_empty_line_between(prev_end, *c_start);
                if !first && (!prev_was_comment || had_blank_before) {
                    docs.push(pretty::line());
                    docs.push(pretty::line());
                } else if prev_was_comment {
                    docs.push(pretty::line());
                }
                docs.push(pretty::str(comment_text.clone()));
                first = false;
                prev_was_comment = true;
                prev_end = *c_end;
            }

            let child_inner_kind = self.inner_decl_kind(child);

            if !first {
                if prev_was_comment {
                    docs.push(pretty::line());
                    docs.push(pretty::line());
                } else {
                    let is_import_like = |k: SyntaxKind| {
                        matches!(k, SyntaxKind::IMPORT_DECL | SyntaxKind::REEXPORT_DECL)
                    };
                    let want_blank = match (prev_kind, child_inner_kind) {
                        (Some(a), Some(b)) if is_import_like(a) && is_import_like(b) => false,
                        _ => true,
                    };
                    if want_blank {
                        docs.push(pretty::line());
                        docs.push(pretty::line());
                    } else {
                        docs.push(pretty::line());
                    }
                }
            }

            docs.push(self.fmt_node(child));

            prev_was_comment = false;
            prev_kind = child_inner_kind;
            prev_end = child.text_range().end().into();
            first = false;
        }

        // Pop any remaining comments after the last item
        let remaining = self.pop_comments_before(u32::MAX);
        for (c_start, c_end, comment_text) in &remaining {
            let had_blank_before = self.has_empty_line_between(prev_end, *c_start);
            if !first && (!prev_was_comment || had_blank_before) {
                docs.push(pretty::line());
                docs.push(pretty::line());
            } else if prev_was_comment {
                docs.push(pretty::line());
            }
            docs.push(pretty::str(comment_text.clone()));
            first = false;
            prev_was_comment = true;
            prev_end = *c_end;
        }

        pretty::concat(docs)
    }

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

    /// Pop all unconsumed comments whose start byte < `to`.
    /// Returns `(start, end, text)` tuples in source order. Advances cursors
    /// past everything consumed.
    pub(crate) fn pop_comments_before(&mut self, to: u32) -> Vec<(u32, u32, String)> {
        let mut results: Vec<(u32, u32, String)> = Vec::new();

        while self.comment_cursor < self.extra.comments.len() {
            let span = self.extra.comments[self.comment_cursor];
            if span.start >= to {
                break;
            }
            results.push((
                span.start,
                span.end,
                self.source[span.start as usize..span.end as usize].to_string(),
            ));
            self.comment_cursor += 1;
        }

        while self.doc_comment_cursor < self.extra.doc_comments.len() {
            let span = self.extra.doc_comments[self.doc_comment_cursor];
            if span.start >= to {
                break;
            }
            results.push((
                span.start,
                span.end,
                self.source[span.start as usize..span.end as usize].to_string(),
            ));
            self.doc_comment_cursor += 1;
        }

        while self.module_comment_cursor < self.extra.module_comments.len() {
            let span = self.extra.module_comments[self.module_comment_cursor];
            if span.start >= to {
                break;
            }
            results.push((
                span.start,
                span.end,
                self.source[span.start as usize..span.end as usize].to_string(),
            ));
            self.module_comment_cursor += 1;
        }

        results.sort_by_key(|(pos, _, _)| *pos);
        results
    }

    /// Check if the source has a blank line in `(from, to)` (exclusive both ends).
    pub(crate) fn has_empty_line_between(&self, from: u32, to: u32) -> bool {
        self.extra
            .empty_lines
            .iter()
            .any(|&off| off > from && off < to)
    }

    /// Pop comments whose start is in `[from, to)`, advancing cursors past
    /// `to`. Comments before `from` that haven't been consumed yet are skipped
    /// silently — they should have been emitted by enclosing context already.
    pub(crate) fn pop_comments_in_range(&mut self, from: u32, to: u32) -> Vec<String> {
        let mut results: Vec<(u32, String)> = Vec::new();

        while self.comment_cursor < self.extra.comments.len() {
            let span = self.extra.comments[self.comment_cursor];
            if span.start >= to {
                break;
            }
            if span.start >= from {
                results.push((
                    span.start,
                    self.source[span.start as usize..span.end as usize].to_string(),
                ));
            }
            self.comment_cursor += 1;
        }
        while self.doc_comment_cursor < self.extra.doc_comments.len() {
            let span = self.extra.doc_comments[self.doc_comment_cursor];
            if span.start >= to {
                break;
            }
            if span.start >= from {
                results.push((
                    span.start,
                    self.source[span.start as usize..span.end as usize].to_string(),
                ));
            }
            self.doc_comment_cursor += 1;
        }

        results.sort_by_key(|(p, _)| *p);
        results.into_iter().map(|(_, t)| t).collect()
    }

    /// Advance all comment cursors past `pos`. Used after recursing into a
    /// node that handles its own comments via CST traversal, so the program
    /// loop doesn't re-emit the same comments from the side-channel.
    pub(crate) fn advance_comment_cursor_to(&mut self, pos: u32) {
        while self.comment_cursor < self.extra.comments.len()
            && self.extra.comments[self.comment_cursor].start < pos
        {
            self.comment_cursor += 1;
        }
        while self.doc_comment_cursor < self.extra.doc_comments.len()
            && self.extra.doc_comments[self.doc_comment_cursor].start < pos
        {
            self.doc_comment_cursor += 1;
        }
        while self.module_comment_cursor < self.extra.module_comments.len()
            && self.extra.module_comments[self.module_comment_cursor].start < pos
        {
            self.module_comment_cursor += 1;
        }
    }

    // ── CST query helpers ───────────────────────────────────────

    pub(crate) fn has_token(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

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

    /// Format expression after `=>`
    pub(crate) fn fmt_expr_after_fat_arrow(&mut self, node: &SyntaxNode) -> Document {
        let mut found_arrow = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::FAT_ARROW {
                    found_arrow = true;
                    continue;
                }
                if found_arrow && !tok.kind().is_trivia() {
                    return pretty::str(tok.text());
                }
            }
            if let Some(child) = t.into_node()
                && found_arrow
                && child.kind() != SyntaxKind::PARAM
            {
                return self.fmt_node(&child);
            }
        }
        pretty::nil()
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
