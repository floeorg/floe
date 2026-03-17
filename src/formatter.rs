mod expr;
mod items;
mod jsx;
#[cfg(test)]
mod tests;

use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::syntax::{SyntaxKind, SyntaxNode};

/// Format Floe source code.
pub fn format(source: &str) -> String {
    let tokens = Lexer::new(source).tokenize_with_trivia();
    let parse = CstParser::new(source, tokens).parse();
    let root = parse.syntax();
    let mut formatter = Formatter::new(source);
    formatter.fmt_node(&root);
    formatter.finish()
}

pub(crate) enum JsxChildInfo {
    Text(String),
    Expr(SyntaxNode),
    Element(SyntaxNode),
}

pub(crate) enum PipeSegment {
    Node(SyntaxNode),
    Token(String),
}

pub(crate) struct Formatter<'src> {
    source: &'src str,
    out: String,
    pub(crate) indent: usize,
    at_line_start: bool,
}

impl<'src> Formatter<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            out: String::with_capacity(source.len()),
            indent: 0,
            at_line_start: true,
        }
    }

    fn finish(mut self) -> String {
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        while self.out.ends_with("\n\n") {
            self.out.pop();
        }
        self.out
    }

    pub(crate) fn fmt_node(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SyntaxKind::PROGRAM => self.fmt_program(node),
            SyntaxKind::ITEM => self.fmt_item(node),
            SyntaxKind::EXPR_ITEM => self.fmt_expr_item(node),
            SyntaxKind::IMPORT_DECL => self.fmt_import(node),
            SyntaxKind::CONST_DECL => self.fmt_const(node),
            SyntaxKind::FUNCTION_DECL => self.fmt_function(node),
            SyntaxKind::TYPE_DECL => self.fmt_type_decl(node),
            SyntaxKind::BLOCK_EXPR => self.fmt_block(node),
            SyntaxKind::PIPE_EXPR => self.fmt_pipe(node),
            SyntaxKind::MATCH_EXPR => self.fmt_match(node),
            SyntaxKind::BINARY_EXPR => self.fmt_binary(node),
            SyntaxKind::UNARY_EXPR | SyntaxKind::AWAIT_EXPR => self.fmt_unary(node),
            SyntaxKind::CALL_EXPR => self.fmt_call(node),
            SyntaxKind::CONSTRUCT_EXPR => self.fmt_construct(node),
            SyntaxKind::MEMBER_EXPR => self.fmt_member(node),
            SyntaxKind::INDEX_EXPR => self.fmt_index(node),
            SyntaxKind::UNWRAP_EXPR => self.fmt_unwrap(node),
            SyntaxKind::ARROW_EXPR => self.fmt_arrow(node),
            SyntaxKind::RETURN_EXPR => self.fmt_return(node),
            SyntaxKind::GROUPED_EXPR => self.fmt_grouped(node),
            SyntaxKind::ARRAY_EXPR => self.fmt_array(node),
            SyntaxKind::OK_EXPR | SyntaxKind::ERR_EXPR | SyntaxKind::SOME_EXPR => {
                self.fmt_wrapper_expr(node)
            }
            SyntaxKind::PARSE_EXPR => self.fmt_parse_expr(node),
            SyntaxKind::JSX_ELEMENT => self.fmt_jsx(node),
            SyntaxKind::TYPE_DEF_UNION => self.fmt_union(node),
            SyntaxKind::TYPE_DEF_RECORD => self.fmt_record_def(node),
            SyntaxKind::TYPE_DEF_ALIAS => self.fmt_type_alias_def(node),
            SyntaxKind::TYPE_EXPR => self.fmt_type_expr(node),
            SyntaxKind::COLLECT_EXPR => self.fmt_verbatim(node),
            SyntaxKind::TEST_BLOCK | SyntaxKind::ASSERT_EXPR => self.fmt_verbatim(node),
            _ => self.fmt_verbatim(node),
        }
    }

    // ── Program ─────────────────────────────────────────────────

    fn fmt_program(&mut self, node: &SyntaxNode) {
        let mut first = true;
        let mut prev_kind: Option<SyntaxKind> = None;

        for child in node.children() {
            let child_inner_kind = self.inner_decl_kind(&child);

            if !first {
                let want_blank = match (prev_kind, child_inner_kind) {
                    (Some(a), Some(b)) if a != b => true,
                    (Some(SyntaxKind::IMPORT_DECL), Some(SyntaxKind::IMPORT_DECL)) => false,
                    _ => true,
                };
                if want_blank {
                    self.newline();
                    self.newline();
                } else {
                    self.newline();
                }
            }

            self.fmt_node(&child);
            first = false;
            prev_kind = child_inner_kind;
        }
    }

    fn inner_decl_kind(&self, node: &SyntaxNode) -> Option<SyntaxKind> {
        match node.kind() {
            SyntaxKind::ITEM => node.children().next().map(|c| c.kind()),
            SyntaxKind::EXPR_ITEM => Some(SyntaxKind::EXPR_ITEM),
            other => Some(other),
        }
    }

    fn fmt_verbatim(&mut self, node: &SyntaxNode) {
        let range = node.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let text = self.source[start..end].trim();
        self.write(text);
    }

    // ── Output helpers ──────────────────────────────────────────

    pub(crate) fn write(&mut self, s: &str) {
        self.out.push_str(s);
        self.at_line_start = s.ends_with('\n');
    }

    pub(crate) fn newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    pub(crate) fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.at_line_start = false;
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

    pub(crate) fn collect_idents_direct(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents(node)
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

    /// Collect IDENT tokens from direct children, stopping when `stop` returns true for a token kind.
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

    pub(crate) fn fmt_token_expr_after_eq(&mut self, node: &SyntaxNode) {
        let mut past_eq = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::EQUAL {
                    past_eq = true;
                    continue;
                }
                if past_eq && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
            if let Some(child) = t.into_node()
                && past_eq
            {
                self.fmt_node(&child);
                return;
            }
        }
    }

    pub(crate) fn fmt_tokens_only(&mut self, node: &SyntaxNode) {
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && !tok.kind().is_trivia()
                && tok.kind() != SyntaxKind::L_PAREN
                && tok.kind() != SyntaxKind::R_PAREN
                && tok.kind() != SyntaxKind::L_BRACKET
                && tok.kind() != SyntaxKind::R_BRACKET
                && tok.kind() != SyntaxKind::L_BRACE
                && tok.kind() != SyntaxKind::R_BRACE
                && tok.kind() != SyntaxKind::COMMA
            {
                self.write(tok.text());
                return;
            }
        }
    }

    pub(crate) fn fmt_token_expr_after_keyword(&mut self, node: &SyntaxNode, keyword: SyntaxKind) {
        let mut past_kw = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == keyword {
                    past_kw = true;
                    continue;
                }
                if past_kw && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
        }
    }

    pub(crate) fn fmt_token_expr_after_lambda_delim(&mut self, node: &SyntaxNode) {
        // For `fn(params) body`, find token expr after the closing `)`.
        let mut found_rparen = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::R_PAREN {
                    found_rparen = true;
                    continue;
                }
                if found_rparen && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
            if let Some(child) = t.into_node()
                && found_rparen
                && child.kind() != SyntaxKind::PARAM
            {
                self.fmt_node(&child);
                return;
            }
        }
    }

    pub(crate) fn fmt_tokens_after_op(&mut self, node: &SyntaxNode) {
        let mut past_op = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if matches!(
                    tok.kind(),
                    SyntaxKind::BANG | SyntaxKind::MINUS | SyntaxKind::KW_AWAIT
                ) {
                    past_op = true;
                    continue;
                }
                if past_op && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
        }
    }

    pub(crate) fn fmt_token_callee(&mut self, node: &SyntaxNode) {
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && !tok.kind().is_trivia()
                && tok.kind() != SyntaxKind::L_PAREN
            {
                self.write(tok.text());
                return;
            }
        }
    }

    pub(crate) fn fmt_tokens_inside_parens(&mut self, node: &SyntaxNode) {
        let mut inside = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::L_PAREN {
                    inside = true;
                    continue;
                }
                if tok.kind() == SyntaxKind::R_PAREN {
                    return;
                }
                if inside && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
        }
    }

    pub(crate) fn fmt_token_expr_inside_brackets(&mut self, node: &SyntaxNode) {
        let mut inside = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::L_BRACKET {
                    inside = true;
                    continue;
                }
                if tok.kind() == SyntaxKind::R_BRACKET {
                    return;
                }
                if inside && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
        }
    }
}
