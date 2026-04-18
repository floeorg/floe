use crate::pretty::{self, Document};
use crate::syntax::{SyntaxKind, SyntaxNode};

use super::Formatter;

enum NamedArgValue {
    Ident(String),
    Other,
    None,
}

pub(crate) enum PipeSegment {
    Node(SyntaxNode),
    Token(String),
}

impl Formatter<'_> {
    // ── Block ───────────────────────────────────────────────────

    pub(crate) fn fmt_block(&mut self, node: &SyntaxNode) -> Document {
        // Collect block entries: items/expressions and standalone comments.
        // We use the CST's whitespace tokens to detect blank lines (more reliable
        // than ModuleExtra here because we want to know about blank lines _between_
        // child nodes, not absolute positions).
        enum BlockEntry {
            Item(SyntaxNode, bool), // (node, had_blank_before)
            Comment(String, bool),  // (text, had_blank_before)
        }

        let mut entries: Vec<BlockEntry> = Vec::new();
        let mut saw_blank = false;

        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::WHITESPACE
                        && tok.text().chars().filter(|&c| c == '\n').count() >= 2
                    {
                        saw_blank = true;
                    } else if tok.kind() == SyntaxKind::COMMENT
                        || tok.kind() == SyntaxKind::BLOCK_COMMENT
                    {
                        entries.push(BlockEntry::Comment(tok.text().to_string(), saw_blank));
                        saw_blank = false;
                    }
                }
                rowan::NodeOrToken::Node(child)
                    if child.kind() == SyntaxKind::ITEM
                        || child.kind() == SyntaxKind::EXPR_ITEM =>
                {
                    let (trailing_comments, trailing_blank) = self.trailing_trivia(&child);
                    entries.push(BlockEntry::Item(child, saw_blank));
                    saw_blank = trailing_blank;
                    for comment in trailing_comments {
                        entries.push(BlockEntry::Comment(comment, false));
                    }
                }
                _ => {}
            }
        }

        // Advance the comment cursor past everything inside this block; we
        // handle internal comments ourselves via CST traversal.
        let block_end: u32 = node.text_range().end().into();
        self.advance_comment_cursor_to(block_end);

        if entries.is_empty() {
            return pretty::str("{}");
        }

        let item_count = entries
            .iter()
            .filter(|e| matches!(e, BlockEntry::Item(..)))
            .count();
        let mut item_index = 0;

        let mut inner = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            match entry {
                BlockEntry::Item(child, had_blank) => {
                    if i > 0 {
                        let is_final_expr = item_count >= 2 && item_index == item_count - 1;
                        if *had_blank || is_final_expr {
                            inner.push(pretty::line());
                        }
                    }
                    inner.push(pretty::line());
                    inner.push(self.fmt_node(child));
                    item_index += 1;
                }
                BlockEntry::Comment(text, had_blank) => {
                    if i > 0 && *had_blank {
                        inner.push(pretty::line());
                    }
                    inner.push(pretty::line());
                    inner.push(pretty::str(text));
                }
            }
        }

        pretty::concat(vec![
            pretty::str("{"),
            pretty::nest(4, pretty::concat(inner)),
            pretty::line(),
            pretty::str("}"),
        ])
    }

    /// Single reverse-walk over descendants to extract both trailing comments
    /// and whether there was a blank line in the trailing trivia. Replaces
    /// separate `trailing_comments_in` + `has_trailing_blank_line`.
    fn trailing_trivia(&self, node: &SyntaxNode) -> (Vec<String>, bool) {
        let mut comments = Vec::new();
        let mut has_blank = false;

        for tok in node
            .descendants_with_tokens()
            .filter_map(|t| t.into_token())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            match tok.kind() {
                SyntaxKind::WHITESPACE => {
                    if tok.text().chars().filter(|&c| c == '\n').count() >= 2 {
                        has_blank = true;
                    }
                }
                SyntaxKind::COMMENT | SyntaxKind::BLOCK_COMMENT => {
                    comments.push(tok.text().to_string());
                }
                k if k.is_trivia() => continue,
                _ => break,
            }
        }
        comments.reverse();
        (comments, has_blank)
    }

    // ── Pipe ────────────────────────────────────────────────────

    pub(crate) fn fmt_pipe(&mut self, node: &SyntaxNode) -> Document {
        let mut segments = Vec::new();
        let mut ops: Vec<&'static str> = Vec::new();
        self.collect_pipe_segments(node, &mut segments, &mut ops);

        if segments.is_empty() {
            return pretty::nil();
        }

        // Nest only the break + operator, NOT the segment content.
        // This way, forced breaks inside a segment (e.g. a match) see only
        // their own indent stack, not the pipe's continuation indent.
        let first = self.fmt_pipe_segment(&segments[0]);
        let mut docs = vec![first];
        for i in 1..segments.len() {
            let op_with_space = if ops[i - 1] == "|>" { "|> " } else { "|>? " };
            docs.push(pretty::nest(
                4,
                pretty::concat(vec![pretty::break_("", " "), pretty::str(op_with_space)]),
            ));
            docs.push(self.fmt_pipe_segment(&segments[i]));
        }

        pretty::group(pretty::concat(docs))
    }

    fn collect_pipe_segments(
        &self,
        node: &SyntaxNode,
        segments: &mut Vec<PipeSegment>,
        ops: &mut Vec<&'static str>,
    ) {
        if node.kind() != SyntaxKind::PIPE_EXPR {
            segments.push(PipeSegment::Node(node.clone()));
            return;
        }

        let mut op: &'static str = "|>";
        let mut left_nodes = Vec::new();
        let mut right_nodes = Vec::new();
        let mut past_pipe = false;

        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::PIPE {
                        past_pipe = true;
                        op = "|>";
                    } else if tok.kind() == SyntaxKind::PIPE_UNWRAP {
                        past_pipe = true;
                        op = "|>?";
                    } else if !tok.kind().is_trivia() {
                        if past_pipe {
                            right_nodes.push(PipeSegment::Token(tok.text().to_string()));
                        } else {
                            left_nodes.push(PipeSegment::Token(tok.text().to_string()));
                        }
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_pipe {
                        right_nodes.push(PipeSegment::Node(child));
                    } else if child.kind() == SyntaxKind::PIPE_EXPR {
                        self.collect_pipe_segments(&child, segments, ops);
                    } else {
                        left_nodes.push(PipeSegment::Node(child));
                    }
                }
            }
        }

        for ln in left_nodes {
            segments.push(ln);
        }
        ops.push(op);
        for rn in right_nodes {
            segments.push(rn);
        }
    }

    fn fmt_pipe_segment(&mut self, seg: &PipeSegment) -> Document {
        match seg {
            PipeSegment::Node(node) => self.fmt_node(node),
            PipeSegment::Token(text) => pretty::str(text.clone()),
        }
    }

    // ── Match ───────────────────────────────────────────────────

    pub(crate) fn fmt_match(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("match")];

        let mut wrote_subject = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::MATCH_ARM {
                break;
            }
            if !wrote_subject {
                parts.push(pretty::str(" "));
                parts.push(self.fmt_node(&child));
                wrote_subject = true;
            }
        }
        if !wrote_subject {
            // Subjectless match (piped): the first non-trivia token after `match` is `{`
            let is_subjectless = {
                let mut past_kw = false;
                let mut result = false;
                for t in node.children_with_tokens() {
                    if let Some(tok) = t.as_token() {
                        if tok.kind() == SyntaxKind::KW_MATCH {
                            past_kw = true;
                            continue;
                        }
                        if past_kw && !tok.kind().is_trivia() {
                            result = tok.kind() == SyntaxKind::L_BRACE;
                            break;
                        }
                    }
                }
                result
            };
            if !is_subjectless {
                parts.push(pretty::str(" "));
                parts.push(self.fmt_expr_after_keyword(node, SyntaxKind::KW_MATCH));
            }
        }

        parts.push(pretty::str(" {"));

        let arms: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MATCH_ARM)
            .collect();

        let mut arm_docs = Vec::new();
        for arm in &arms {
            arm_docs.push(pretty::line());
            arm_docs.push(self.fmt_match_arm(arm));
            arm_docs.push(pretty::str(","));
        }

        parts.push(pretty::nest(4, pretty::concat(arm_docs)));
        parts.push(pretty::line());
        parts.push(pretty::str("}"));

        pretty::concat(parts)
    }

    fn fmt_match_arm(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        if let Some(pattern) = node.children().find(|c| c.kind() == SyntaxKind::PATTERN) {
            parts.push(self.fmt_pattern(&pattern));
        }

        // Guard: when expr
        if let Some(guard) = node
            .children()
            .find(|c| c.kind() == SyntaxKind::MATCH_GUARD)
        {
            parts.push(pretty::str(" when "));
            for t in guard.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::KW_WHEN || tok.kind().is_trivia() {
                        continue;
                    }
                    parts.push(pretty::str(tok.text()));
                    break;
                }
                if let Some(child) = t.into_node() {
                    parts.push(self.fmt_node(&child));
                    break;
                }
            }
        }

        // Body: expression after ->
        let body_node = {
            let mut past_arrow = false;
            let mut found = None;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::THIN_ARROW {
                        past_arrow = true;
                    }
                } else if let Some(child) = t.into_node()
                    && past_arrow
                {
                    found = Some(child);
                    break;
                }
            }
            found
        };

        let break_after_arrow = body_node.as_ref().is_some_and(|n| {
            n.kind() == SyntaxKind::JSX_ELEMENT && self.jsx_has_multiline_props(n)
        });

        if break_after_arrow {
            parts.push(pretty::str(" ->"));
            parts.push(pretty::nest(
                4,
                pretty::concat(vec![
                    pretty::line(),
                    self.fmt_node(body_node.as_ref().unwrap()),
                ]),
            ));
            return pretty::concat(parts);
        }

        parts.push(pretty::str(" -> "));

        if let Some(body) = body_node {
            parts.push(self.fmt_node(&body));
        } else {
            // Token-only body
            let mut past_arrow = false;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::THIN_ARROW {
                        past_arrow = true;
                        continue;
                    }
                    if past_arrow && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }

        pretty::concat(parts)
    }

    pub(crate) fn fmt_pattern(&mut self, node: &SyntaxNode) -> Document {
        let sub_patterns: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PATTERN)
            .collect();

        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::UNDERSCORE => return pretty::str("_"),
                    SyntaxKind::BOOL | SyntaxKind::STRING | SyntaxKind::NUMBER => {
                        if self.has_token(node, SyntaxKind::DOT_DOT) {
                            let numbers: Vec<_> = node
                                .children_with_tokens()
                                .filter_map(|t| t.into_token())
                                .filter(|t| t.kind() == SyntaxKind::NUMBER)
                                .collect();
                            if numbers.len() >= 2 {
                                return pretty::concat(vec![
                                    pretty::str(numbers[0].text()),
                                    pretty::str(".."),
                                    pretty::str(numbers[1].text()),
                                ]);
                            }
                        }
                        return pretty::str(tok.text());
                    }
                    SyntaxKind::IDENT => {
                        let name = tok.text().to_string();
                        if name.starts_with(char::is_uppercase) {
                            let mut parts = vec![pretty::str(name)];
                            let named_fields: Vec<_> = node
                                .children()
                                .filter(|c| c.kind() == SyntaxKind::VARIANT_FIELD_PATTERN)
                                .collect();
                            if !named_fields.is_empty() {
                                parts.push(pretty::str(" { "));
                                for (i, field) in named_fields.iter().enumerate() {
                                    if i > 0 {
                                        parts.push(pretty::str(", "));
                                    }
                                    parts.push(self.fmt_variant_field_pattern(field));
                                }
                                parts.push(pretty::str(" }"));
                            } else if !sub_patterns.is_empty() {
                                parts.push(pretty::str("("));
                                for (i, p) in sub_patterns.iter().enumerate() {
                                    if i > 0 {
                                        parts.push(pretty::str(", "));
                                    }
                                    parts.push(self.fmt_pattern(p));
                                }
                                parts.push(pretty::str(")"));
                            }
                            return pretty::concat(parts);
                        }
                        return pretty::str(name);
                    }
                    SyntaxKind::L_PAREN => {
                        let mut parts = vec![pretty::str("(")];
                        for (i, p) in sub_patterns.iter().enumerate() {
                            if i > 0 {
                                parts.push(pretty::str(", "));
                            }
                            parts.push(self.fmt_pattern(p));
                        }
                        parts.push(pretty::str(")"));
                        return pretty::concat(parts);
                    }
                    SyntaxKind::L_BRACKET => {
                        let mut parts = vec![pretty::str("[")];
                        let mut first_elem = true;
                        let mut saw_dotdot = false;
                        for inner in node.children_with_tokens() {
                            match &inner {
                                rowan::NodeOrToken::Token(t) => match t.kind() {
                                    SyntaxKind::DOT_DOT => {
                                        if !first_elem {
                                            parts.push(pretty::str(", "));
                                        }
                                        parts.push(pretty::str(".."));
                                        saw_dotdot = true;
                                        first_elem = false;
                                    }
                                    SyntaxKind::IDENT if saw_dotdot => {
                                        parts.push(pretty::str(t.text()));
                                        saw_dotdot = false;
                                    }
                                    SyntaxKind::UNDERSCORE if saw_dotdot => {
                                        parts.push(pretty::str("_"));
                                        saw_dotdot = false;
                                    }
                                    _ => {}
                                },
                                rowan::NodeOrToken::Node(child)
                                    if child.kind() == SyntaxKind::PATTERN =>
                                {
                                    if !first_elem {
                                        parts.push(pretty::str(", "));
                                    }
                                    parts.push(self.fmt_pattern(child));
                                    first_elem = false;
                                }
                                _ => {}
                            }
                        }
                        parts.push(pretty::str("]"));
                        return pretty::concat(parts);
                    }
                    SyntaxKind::L_BRACE => {
                        let mut parts = vec![pretty::str("{ ")];
                        let idents: Vec<_> = node
                            .children_with_tokens()
                            .filter_map(|t| t.into_token())
                            .filter(|t| t.kind() == SyntaxKind::IDENT)
                            .collect();
                        for (i, ident) in idents.iter().enumerate() {
                            if i > 0 {
                                parts.push(pretty::str(", "));
                            }
                            parts.push(pretty::str(ident.text()));
                        }
                        parts.push(pretty::str(" }"));
                        return pretty::concat(parts);
                    }
                    _ => {}
                }
            }
        }
        pretty::nil()
    }

    /// Format one field in a brace-form variant pattern: either shorthand
    /// (`width`) or a rename/nested pattern (`width: pat`).
    fn fmt_variant_field_pattern(&mut self, node: &SyntaxNode) -> Document {
        let name = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .unwrap_or_default();
        let nested = node.children().find(|c| c.kind() == SyntaxKind::PATTERN);
        match nested {
            Some(p) => pretty::concat(vec![
                pretty::str(name),
                pretty::str(": "),
                self.fmt_pattern(&p),
            ]),
            None => pretty::str(name),
        }
    }

    // ── Binary / Unary ──────────────────────────────────────────

    pub(crate) fn fmt_binary(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        let mut phase = 0;

        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    if phase == 0 {
                        parts.push(self.fmt_node(&child));
                        phase = 1;
                    } else {
                        parts.push(self.fmt_node(&child));
                        phase = 3;
                    }
                }
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind().is_trivia() {
                        continue;
                    }
                    let op_str = match tok.kind() {
                        SyntaxKind::PLUS => Some("+"),
                        SyntaxKind::MINUS => Some("-"),
                        SyntaxKind::STAR => Some("*"),
                        SyntaxKind::SLASH => Some("/"),
                        SyntaxKind::PERCENT => Some("%"),
                        SyntaxKind::EQUAL_EQUAL => Some("=="),
                        SyntaxKind::BANG_EQUAL => Some("!="),
                        SyntaxKind::LESS_THAN => Some("<"),
                        SyntaxKind::GREATER_THAN => Some(">"),
                        SyntaxKind::LESS_EQUAL => Some("<="),
                        SyntaxKind::GREATER_EQUAL => Some(">="),
                        SyntaxKind::AMP_AMP => Some("&&"),
                        SyntaxKind::PIPE_PIPE => Some("||"),
                        _ => None,
                    };
                    if let Some(op) = op_str {
                        parts.push(pretty::str(" "));
                        parts.push(pretty::str(op));
                        parts.push(pretty::str(" "));
                        phase = 2;
                    } else if phase == 0 {
                        parts.push(pretty::str(tok.text()));
                        phase = 1;
                    } else if phase >= 2 {
                        parts.push(pretty::str(tok.text()));
                        phase = 3;
                    }
                }
            }
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_unary(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::BANG => {
                        parts.push(pretty::str("!"));
                        break;
                    }
                    SyntaxKind::MINUS => {
                        parts.push(pretty::str("-"));
                        break;
                    }
                    _ => {}
                }
            }
        }

        if let Some(child) = node.children().next() {
            parts.push(self.fmt_node(&child));
        } else {
            // Token-only operand
            let mut past_op = false;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if matches!(tok.kind(), SyntaxKind::BANG | SyntaxKind::MINUS) {
                        past_op = true;
                        continue;
                    }
                    if past_op && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }
        pretty::concat(parts)
    }

    // ── Call ────────────────────────────────────────────────────

    pub(crate) fn fmt_call(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        let mut wrote_callee = false;
        let mut in_type_args = false;
        let mut first_type_arg = true;

        for child_or_tok in node.children_with_tokens() {
            match &child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::L_PAREN {
                        break;
                    }
                    if tok.kind().is_trivia() {
                        continue;
                    }
                    if tok.kind() == SyntaxKind::LESS_THAN {
                        parts.push(pretty::str("<"));
                        in_type_args = true;
                        first_type_arg = true;
                        wrote_callee = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::GREATER_THAN {
                        parts.push(pretty::str(">"));
                        in_type_args = false;
                        continue;
                    }
                    if !wrote_callee {
                        parts.push(pretty::str(tok.text()));
                        wrote_callee = true;
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if child.kind() == SyntaxKind::ARG {
                        break;
                    }
                    if in_type_args && child.kind() == SyntaxKind::TYPE_EXPR {
                        if !first_type_arg {
                            parts.push(pretty::str(", "));
                        }
                        parts.push(self.fmt_type_expr(child));
                        first_type_arg = false;
                    } else if !wrote_callee || !in_type_args {
                        parts.push(self.fmt_node(child));
                        wrote_callee = true;
                    }
                }
            }
        }
        if !wrote_callee && let Some(text) = self.first_content_token(node) {
            parts.push(pretty::str(text));
        }

        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();

        // Special case: single arg with multiline JSX arrow body — hug format
        if args.len() == 1 && self.arg_has_jsx_arrow_body(&args[0]) {
            parts.push(pretty::str("("));
            parts.push(self.fmt_arg(&args[0]));
            parts.push(pretty::line());
            parts.push(pretty::str(")"));
            return pretty::concat(parts);
        }

        parts.push(self.comma_list("(", ")", &args, |s, n| s.fmt_arg(n)));
        pretty::concat(parts)
    }

    /// Build a comma-separated list document with inline-or-broken layout.
    /// The trailing comma+newline is OUTSIDE the nest so that the closing
    /// delimiter sits at the base indent.
    ///
    /// Comments that appear in the source between items are pulled from the
    /// `ModuleExtra` side-channel and interleaved into the document. When any
    /// such comment exists, the group is forced broken — comments only make
    /// sense in vertical layout.
    fn comma_list<F>(
        &mut self,
        open: &str,
        close: &str,
        items: &[SyntaxNode],
        mut fmt: F,
    ) -> Document
    where
        F: FnMut(&mut Self, &SyntaxNode) -> Document,
    {
        if items.is_empty() {
            return pretty::concat(vec![
                pretty::str(open.to_string()),
                pretty::str(close.to_string()),
            ]);
        }

        let mut has_comment = false;
        let mut inner = vec![pretty::break_("", "")];
        let mut prev_end: Option<u32> = None;

        for (i, item) in items.iter().enumerate() {
            let item_start: u32 = item.text_range().start().into();

            if i > 0 {
                inner.push(pretty::break_(",", ", "));
            }

            if let Some(prev) = prev_end {
                for c in self.pop_comments_in_range(prev, item_start) {
                    inner.push(pretty::str(c.text));
                    inner.push(pretty::line());
                    has_comment = true;
                }
            }

            inner.push(fmt(self, item));
            prev_end = Some(item.text_range().end().into());
        }

        let mut docs = vec![pretty::str(open)];
        if has_comment {
            docs.push(pretty::force_break());
        }
        docs.push(pretty::nest(4, pretty::concat(inner)));
        docs.push(pretty::break_(",", ""));
        docs.push(pretty::str(close));
        pretty::group(pretty::concat(docs))
    }

    fn arg_has_jsx_arrow_body(&self, arg: &SyntaxNode) -> bool {
        arg.children().any(|c| {
            c.kind() == SyntaxKind::ARROW_EXPR
                && c.children().any(|body| self.is_multiline_jsx(&body))
        })
    }

    pub(crate) fn fmt_arg(&mut self, node: &SyntaxNode) -> Document {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let name = self.first_ident(node);
            let value_kind = self.named_arg_value_kind(node);

            // Pun: emit `name:` when value is same identifier as label, or no value
            if let Some(ref label) = name {
                match &value_kind {
                    NamedArgValue::Ident(val) if label == val => {
                        return pretty::concat(vec![pretty::str(label.clone()), pretty::str(":")]);
                    }
                    NamedArgValue::None => {
                        return pretty::concat(vec![pretty::str(label.clone()), pretty::str(":")]);
                    }
                    _ => {}
                }
            }

            let mut parts = Vec::new();
            if let Some(name) = name {
                parts.push(pretty::str(name));
                parts.push(pretty::str(": "));
            }

            let mut past_colon = false;
            for child_or_tok in node.children_with_tokens() {
                if let Some(tok) = child_or_tok.as_token() {
                    if tok.kind() == SyntaxKind::COLON {
                        past_colon = true;
                        continue;
                    }
                    if past_colon && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        return pretty::concat(parts);
                    }
                }
                if let Some(child) = child_or_tok.into_node()
                    && past_colon
                {
                    parts.push(self.fmt_node(&child));
                    return pretty::concat(parts);
                }
            }
            pretty::concat(parts)
        } else {
            if let Some(child) = node.children().next() {
                return self.fmt_node(&child);
            }
            if let Some(text) = self.first_content_token(node) {
                return pretty::str(text);
            }
            pretty::nil()
        }
    }

    fn named_arg_value_kind(&self, node: &SyntaxNode) -> NamedArgValue {
        let mut past_colon = false;
        for child_or_tok in node.children_with_tokens() {
            if let Some(tok) = child_or_tok.as_token() {
                if tok.kind() == SyntaxKind::COLON {
                    past_colon = true;
                    continue;
                }
                if past_colon && !tok.kind().is_trivia() {
                    if tok.kind() == SyntaxKind::IDENT {
                        return NamedArgValue::Ident(tok.text().to_string());
                    }
                    return NamedArgValue::Other;
                }
            }
            if child_or_tok.as_node().is_some() && past_colon {
                return NamedArgValue::Other;
            }
        }
        NamedArgValue::None
    }

    // ── Construct ───────────────────────────────────────────────

    pub(crate) fn fmt_construct(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();

        let idents = self.collect_idents_before_lparen(node);
        if idents.is_empty() {
            if let Some(name) = self.first_ident(node) {
                parts.push(pretty::str(name));
            }
        } else {
            for (i, ident) in idents.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str("."));
                }
                parts.push(pretty::str(ident.clone()));
            }
        }

        let spread = node
            .children()
            .find(|c| c.kind() == SyntaxKind::SPREAD_EXPR);
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();

        // Comma-separated list with optional leading spread
        let has_items = spread.is_some() || !args.is_empty();
        if !has_items {
            parts.push(pretty::str("()"));
            return pretty::concat(parts);
        }

        let mut has_comment = false;
        let mut inner = vec![pretty::break_("", "")];
        let mut first = true;
        let mut prev_end: Option<u32> = None;
        if let Some(spread) = &spread {
            inner.push(self.fmt_spread(spread));
            prev_end = Some(spread.text_range().end().into());
            first = false;
        }
        for arg in &args {
            let arg_start: u32 = arg.text_range().start().into();
            if !first {
                inner.push(pretty::break_(",", ", "));
            }
            if let Some(prev) = prev_end {
                for c in self.pop_comments_in_range(prev, arg_start) {
                    inner.push(pretty::str(c.text));
                    inner.push(pretty::line());
                    has_comment = true;
                }
            }
            inner.push(self.fmt_arg(arg));
            prev_end = Some(arg.text_range().end().into());
            first = false;
        }

        let mut group_parts = vec![pretty::str("(")];
        if has_comment {
            group_parts.push(pretty::force_break());
        }
        group_parts.push(pretty::nest(4, pretty::concat(inner)));
        group_parts.push(pretty::break_(",", ""));
        group_parts.push(pretty::str(")"));
        parts.push(pretty::group(pretty::concat(group_parts)));

        pretty::concat(parts)
    }

    fn fmt_spread(&mut self, spread: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("..")];
        if let Some(child) = spread.children().next() {
            parts.push(self.fmt_node(&child));
        } else {
            let mut past_dots = false;
            for t in spread.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::DOT_DOT {
                        past_dots = true;
                        continue;
                    }
                    if past_dots && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }
        pretty::concat(parts)
    }

    // ── Member / Index / Unwrap ─────────────────────────────────

    pub(crate) fn fmt_member(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        if let Some(child) = node.children().next() {
            parts.push(self.fmt_node(&child));
        } else if let Some(text) = self.first_content_token(node) {
            parts.push(pretty::str(text));
        }
        parts.push(pretty::str("."));

        let mut found_dot = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::DOT {
                    found_dot = true;
                } else if found_dot && tok.kind().is_member_name() {
                    parts.push(pretty::str(tok.text()));
                    break;
                }
            }
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_index(&mut self, node: &SyntaxNode) -> Document {
        let children: Vec<_> = node.children().collect();
        let mut parts = Vec::new();
        if let Some(obj) = children.first() {
            parts.push(self.fmt_node(obj));
        }
        parts.push(pretty::str("["));
        if children.len() >= 2 {
            parts.push(self.fmt_node(&children[1]));
        } else {
            // Token-only index
            let mut inside = false;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::L_BRACKET {
                        inside = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::R_BRACKET {
                        break;
                    }
                    if inside && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }
        parts.push(pretty::str("]"));
        pretty::concat(parts)
    }

    pub(crate) fn fmt_tagged_template(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) if parts.is_empty() => {
                    parts.push(self.fmt_node(&child));
                }
                rowan::NodeOrToken::Token(tok) => match tok.kind() {
                    SyntaxKind::TEMPLATE_LITERAL => {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                    k if parts.is_empty() && !k.is_trivia() => {
                        parts.push(pretty::str(tok.text()));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_unwrap(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        if let Some(child) = node.children().next() {
            parts.push(self.fmt_node(&child));
        } else if let Some(text) = self.first_content_token(node) {
            parts.push(pretty::str(text));
        }
        parts.push(pretty::str("?"));
        pretty::concat(parts)
    }

    // ── Arrow ───────────────────────────────────────────────────

    pub(crate) fn fmt_arrow(&mut self, node: &SyntaxNode) -> Document {
        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();

        let mut parts = vec![pretty::str("(")];
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                parts.push(pretty::str(", "));
            }
            parts.push(self.fmt_param(param));
        }
        parts.push(pretty::str(")"));

        let body = node.children().find(|c| c.kind() != SyntaxKind::PARAM);

        if let Some(body) = body {
            if self.is_multiline_jsx(&body) {
                parts.push(pretty::str(" =>"));
                parts.push(pretty::nest(
                    4,
                    pretty::concat(vec![pretty::line(), self.fmt_node(&body)]),
                ));
            } else {
                parts.push(pretty::str(" => "));
                parts.push(self.fmt_node(&body));
            }
        } else {
            parts.push(pretty::str(" => "));
            parts.push(self.fmt_expr_after_fat_arrow(node));
        }

        pretty::concat(parts)
    }

    // ── Return ──────────────────────────────────────────────────

    pub(crate) fn fmt_return(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("return")];

        if let Some(child) = node.children().next() {
            parts.push(pretty::str(" "));
            parts.push(self.fmt_node(&child));
            return pretty::concat(parts);
        }

        let has_value = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| !tok.kind().is_trivia() && tok.kind() != SyntaxKind::KW_RETURN)
        });
        if has_value {
            parts.push(pretty::str(" "));
            parts.push(self.fmt_expr_after_keyword(node, SyntaxKind::KW_RETURN));
        }
        pretty::concat(parts)
    }

    // ── Grouped / Tuple / Array / Wrapper ───────────────────────

    pub(crate) fn fmt_tuple(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("(")];
        let mut first = true;
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    if !first {
                        parts.push(pretty::str(", "));
                    }
                    parts.push(self.fmt_node(&child));
                    first = false;
                }
                rowan::NodeOrToken::Token(tok) => match tok.kind() {
                    SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::BOOL
                    | SyntaxKind::IDENT
                    | SyntaxKind::UNDERSCORE => {
                        if !first {
                            parts.push(pretty::str(", "));
                        }
                        parts.push(pretty::str(tok.text()));
                        first = false;
                    }
                    _ => {}
                },
            }
        }
        parts.push(pretty::str(")"));
        pretty::concat(parts)
    }

    pub(crate) fn fmt_grouped(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("(")];
        let mut had_child = false;
        for child in node.children() {
            parts.push(self.fmt_node(&child));
            had_child = true;
        }
        if !had_child {
            // Token-only inner content
            let mut inside = false;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::L_PAREN {
                        inside = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::R_PAREN {
                        break;
                    }
                    if inside && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }
        parts.push(pretty::str(")"));
        pretty::concat(parts)
    }

    pub(crate) fn fmt_array(&mut self, node: &SyntaxNode) -> Document {
        // Collect element ranges + documents. Track byte ranges so we can pop
        // any inter-element comments from the side-channel.
        struct Elem {
            start: u32,
            end: u32,
            doc: Document,
        }
        let mut elems: Vec<Elem> = Vec::new();
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    let r = child.text_range();
                    let start: u32 = r.start().into();
                    let end: u32 = r.end().into();
                    elems.push(Elem {
                        start,
                        end,
                        doc: self.fmt_node(&child),
                    });
                }
                rowan::NodeOrToken::Token(tok) => match tok.kind() {
                    SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::BOOL
                    | SyntaxKind::IDENT
                    | SyntaxKind::UNDERSCORE => {
                        let r = tok.text_range();
                        let start: u32 = r.start().into();
                        let end: u32 = r.end().into();
                        elems.push(Elem {
                            start,
                            end,
                            doc: pretty::str(tok.text()),
                        });
                    }
                    _ => {}
                },
            }
        }

        if elems.is_empty() {
            return pretty::str("[]");
        }

        let mut has_comment = false;
        let mut inner = vec![pretty::break_("", "")];
        let mut prev_end: Option<u32> = None;
        for (i, elem) in elems.into_iter().enumerate() {
            if i > 0 {
                inner.push(pretty::break_(",", ", "));
            }
            if let Some(prev) = prev_end {
                for c in self.pop_comments_in_range(prev, elem.start) {
                    inner.push(pretty::str(c.text));
                    inner.push(pretty::line());
                    has_comment = true;
                }
            }
            inner.push(elem.doc);
            prev_end = Some(elem.end);
        }

        let mut group_parts = vec![pretty::str("[")];
        if has_comment {
            group_parts.push(pretty::force_break());
        }
        group_parts.push(pretty::nest(4, pretty::concat(inner)));
        group_parts.push(pretty::break_(",", ""));
        group_parts.push(pretty::str("]"));
        pretty::group(pretty::concat(group_parts))
    }

    pub(crate) fn fmt_parse_expr(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("parse<")];
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                parts.push(self.fmt_type_expr(&child));
                break;
            }
        }
        parts.push(pretty::str(">"));
        let value_child = node.children().find(|c| c.kind() != SyntaxKind::TYPE_EXPR);
        if let Some(value) = value_child {
            parts.push(pretty::str("("));
            parts.push(self.fmt_node(&value));
            parts.push(pretty::str(")"));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_mock_expr(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("mock<")];
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                parts.push(self.fmt_type_expr(&child));
                break;
            }
        }
        parts.push(pretty::str(">"));
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();
        if !args.is_empty() {
            parts.push(pretty::str("("));
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_arg(arg));
            }
            parts.push(pretty::str(")"));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_wrapper_expr(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("Value"), pretty::str("(")];
        if let Some(child) = node.children().next() {
            parts.push(self.fmt_node(&child));
        } else {
            // Token-only inside parens
            let mut inside = false;
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::L_PAREN {
                        inside = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::R_PAREN {
                        break;
                    }
                    if inside && !tok.kind().is_trivia() {
                        parts.push(pretty::str(tok.text()));
                        break;
                    }
                }
            }
        }
        parts.push(pretty::str(")"));
        pretty::concat(parts)
    }
}
