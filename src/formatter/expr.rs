use crate::syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, PipeSegment};

impl Formatter<'_> {
    pub(crate) fn fmt_block(&mut self, node: &SyntaxNode) {
        self.write("{");
        let children: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ITEM || c.kind() == SyntaxKind::EXPR_ITEM)
            .collect();

        if children.is_empty() {
            self.write("}");
            return;
        }

        self.indent += 1;
        for child in &children {
            self.newline();
            self.write_indent();
            self.fmt_node(child);
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    // ── Pipe ────────────────────────────────────────────────────

    pub(crate) fn fmt_pipe(&mut self, node: &SyntaxNode) {
        let mut segments = Vec::new();
        self.collect_pipe_segments(node, &mut segments);

        if segments.len() <= 3 {
            for (i, seg) in segments.iter().enumerate() {
                if i > 0 {
                    self.write(" |> ");
                }
                self.fmt_pipe_segment(seg);
            }
        } else {
            for (i, seg) in segments.iter().enumerate() {
                if i > 0 {
                    self.newline();
                    self.write_indent();
                    self.write("|> ");
                }
                self.fmt_pipe_segment(seg);
            }
        }
    }

    fn collect_pipe_segments(&self, node: &SyntaxNode, segments: &mut Vec<PipeSegment>) {
        if node.kind() != SyntaxKind::PIPE_EXPR {
            segments.push(PipeSegment::Node(node.clone()));
            return;
        }

        let mut left_nodes = Vec::new();
        let mut right_nodes = Vec::new();
        let mut past_pipe = false;

        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::PIPE {
                        past_pipe = true;
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
                        self.collect_pipe_segments(&child, segments);
                    } else {
                        left_nodes.push(PipeSegment::Node(child));
                    }
                }
            }
        }

        for ln in left_nodes {
            segments.push(ln);
        }
        for rn in right_nodes {
            segments.push(rn);
        }
    }

    fn fmt_pipe_segment(&mut self, seg: &PipeSegment) {
        match seg {
            PipeSegment::Node(node) => self.fmt_node(node),
            PipeSegment::Token(text) => self.write(text),
        }
    }

    // ── Match ───────────────────────────────────────────────────

    pub(crate) fn fmt_match(&mut self, node: &SyntaxNode) {
        self.write("match ");

        let mut wrote_subject = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::MATCH_ARM {
                break;
            }
            if !wrote_subject {
                self.fmt_node(&child);
                wrote_subject = true;
            }
        }
        if !wrote_subject {
            self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_MATCH);
        }

        self.write(" {");

        let arms: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::MATCH_ARM)
            .collect();

        self.indent += 1;
        for arm in &arms {
            self.newline();
            self.write_indent();
            self.fmt_match_arm(arm);
            self.write(",");
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    fn fmt_match_arm(&mut self, node: &SyntaxNode) {
        if let Some(pattern) = node.children().find(|c| c.kind() == SyntaxKind::PATTERN) {
            self.fmt_pattern(&pattern);
        }
        self.write(" -> ");

        // Body: expression after ->
        let mut past_arrow = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::THIN_ARROW {
                    past_arrow = true;
                    continue;
                }
                if past_arrow && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
            if let Some(child) = t.into_node()
                && past_arrow
            {
                self.fmt_node(&child);
                return;
            }
        }
    }

    pub(crate) fn fmt_pattern(&mut self, node: &SyntaxNode) {
        let sub_patterns: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PATTERN)
            .collect();

        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::UNDERSCORE => {
                        self.write("_");
                        return;
                    }
                    SyntaxKind::BOOL | SyntaxKind::STRING | SyntaxKind::NUMBER => {
                        self.write(tok.text());
                        if self.has_token(node, SyntaxKind::DOT_DOT) {
                            let numbers: Vec<_> = node
                                .children_with_tokens()
                                .filter_map(|t| t.into_token())
                                .filter(|t| t.kind() == SyntaxKind::NUMBER)
                                .collect();
                            if numbers.len() >= 2 {
                                let len = self.out.len() - tok.text().len();
                                self.out.truncate(len);
                                self.write(numbers[0].text());
                                self.write("..");
                                self.write(numbers[1].text());
                            }
                        }
                        return;
                    }
                    SyntaxKind::KW_NONE => {
                        self.write("None");
                        return;
                    }
                    SyntaxKind::KW_OK | SyntaxKind::KW_ERR | SyntaxKind::KW_SOME => {
                        self.write(tok.text());
                        if !sub_patterns.is_empty() {
                            self.write("(");
                            for (i, p) in sub_patterns.iter().enumerate() {
                                if i > 0 {
                                    self.write(", ");
                                }
                                self.fmt_pattern(p);
                            }
                            self.write(")");
                        }
                        return;
                    }
                    SyntaxKind::IDENT => {
                        let name = tok.text();
                        if name.starts_with(char::is_uppercase) {
                            self.write(name);
                            if !sub_patterns.is_empty() {
                                self.write("(");
                                for (i, p) in sub_patterns.iter().enumerate() {
                                    if i > 0 {
                                        self.write(", ");
                                    }
                                    self.fmt_pattern(p);
                                }
                                self.write(")");
                            }
                        } else {
                            self.write(name);
                        }
                        return;
                    }
                    SyntaxKind::L_BRACE => {
                        self.write("{ ");
                        let idents: Vec<_> = node
                            .children_with_tokens()
                            .filter_map(|t| t.into_token())
                            .filter(|t| t.kind() == SyntaxKind::IDENT)
                            .collect();
                        for (i, ident) in idents.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.write(ident.text());
                        }
                        self.write(" }");
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    // ── If ──────────────────────────────────────────────────────

    pub(crate) fn fmt_if(&mut self, node: &SyntaxNode) {
        self.write("if ");

        let children: Vec<_> = node.children().collect();
        let mut child_iter = children.iter();

        if let Some(cond) = child_iter.next() {
            if cond.kind() == SyntaxKind::BLOCK_EXPR {
                self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_IF);
                self.write(" ");
                self.fmt_block(cond);
            } else {
                self.fmt_node(cond);
                self.write(" ");
                if let Some(then_block) = child_iter.next() {
                    self.fmt_node(then_block);
                }
            }
        }

        let has_else = self.has_token(node, SyntaxKind::KW_ELSE);
        if has_else {
            self.write(" else ");
            if let Some(else_node) = child_iter.next() {
                self.fmt_node(else_node);
            }
        }
    }

    // ── Binary ──────────────────────────────────────────────────

    pub(crate) fn fmt_binary(&mut self, node: &SyntaxNode) {
        let mut phase = 0; // 0=left, 1=op found, 2=right
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    if phase == 0 {
                        self.fmt_node(&child);
                        phase = 1;
                    } else if phase >= 1 {
                        self.fmt_node(&child);
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
                        self.write(" ");
                        self.write(op);
                        self.write(" ");
                        phase = 2;
                    } else if phase == 0 {
                        self.write(tok.text());
                        phase = 1;
                    } else if phase >= 2 {
                        self.write(tok.text());
                        phase = 3;
                    }
                }
            }
        }
    }

    // ── Unary ───────────────────────────────────────────────────

    pub(crate) fn fmt_unary(&mut self, node: &SyntaxNode) {
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::BANG => {
                        self.write("!");
                        break;
                    }
                    SyntaxKind::MINUS => {
                        self.write("-");
                        break;
                    }
                    SyntaxKind::KW_AWAIT => {
                        self.write("await ");
                        break;
                    }
                    _ => {}
                }
            }
        }

        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_tokens_after_op(node);
        }
    }

    // ── Call ────────────────────────────────────────────────────

    pub(crate) fn fmt_call(&mut self, node: &SyntaxNode) {
        let mut wrote_callee = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::ARG {
                break;
            }
            self.fmt_node(&child);
            wrote_callee = true;
        }
        if !wrote_callee {
            self.fmt_token_callee(node);
        }

        self.write("(");
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.fmt_arg(arg);
        }
        self.write(")");
    }

    pub(crate) fn fmt_arg(&mut self, node: &SyntaxNode) {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            if let Some(name) = self.first_ident(node) {
                self.write(&name);
                self.write(": ");
            }
            let mut past_colon = false;
            for child_or_tok in node.children_with_tokens() {
                if let Some(tok) = child_or_tok.as_token() {
                    if tok.kind() == SyntaxKind::COLON {
                        past_colon = true;
                        continue;
                    }
                    if past_colon && !tok.kind().is_trivia() {
                        self.write(tok.text());
                        return;
                    }
                }
                if let Some(child) = child_or_tok.into_node()
                    && past_colon
                {
                    self.fmt_node(&child);
                    return;
                }
            }
        } else {
            if let Some(child) = node.children().next() {
                self.fmt_node(&child);
                return;
            }
            self.fmt_tokens_only(node);
        }
    }

    // ── Construct ───────────────────────────────────────────────

    pub(crate) fn fmt_construct(&mut self, node: &SyntaxNode) {
        // Collect idents before '(' to handle qualified variants: Route.Profile(...)
        let idents = self.collect_idents_before_lparen(node);
        if idents.is_empty() {
            if let Some(name) = self.first_ident(node) {
                self.write(&name);
            }
        } else {
            for (i, ident) in idents.iter().enumerate() {
                if i > 0 {
                    self.write(".");
                }
                self.write(ident);
            }
        }
        self.write("(");

        let spread = node
            .children()
            .find(|c| c.kind() == SyntaxKind::SPREAD_EXPR);
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();

        let mut first = true;
        if let Some(spread) = spread {
            self.write("..");
            for child in spread.children() {
                self.fmt_node(&child);
            }
            if spread.children().next().is_none() {
                self.fmt_tokens_only(&spread);
            }
            first = false;
        }

        for arg in &args {
            if !first {
                self.write(", ");
            }
            self.fmt_arg(arg);
            first = false;
        }

        self.write(")");
    }

    // ── Member / Index / Unwrap ─────────────────────────────────

    pub(crate) fn fmt_member(&mut self, node: &SyntaxNode) {
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_token_callee(node);
        }
        self.write(".");
        let idents = self.collect_idents(node);
        if let Some(field) = idents.last() {
            self.write(field);
        }
    }

    pub(crate) fn fmt_index(&mut self, node: &SyntaxNode) {
        let children: Vec<_> = node.children().collect();
        if let Some(obj) = children.first() {
            self.fmt_node(obj);
        }
        self.write("[");
        if children.len() >= 2 {
            self.fmt_node(&children[1]);
        } else {
            self.fmt_token_expr_inside_brackets(node);
        }
        self.write("]");
    }

    pub(crate) fn fmt_unwrap(&mut self, node: &SyntaxNode) {
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_tokens_only(node);
        }
        self.write("?");
    }

    // ── Arrow ───────────────────────────────────────────────────

    pub(crate) fn fmt_arrow(&mut self, node: &SyntaxNode) {
        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();

        // Check if this is a zero-arg lambda (has || token)
        let has_pipe_pipe = self.has_token(node, SyntaxKind::PIPE_PIPE);

        if has_pipe_pipe && params.is_empty() {
            self.write("|| ");
        } else {
            self.write("|");
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_param(param);
            }
            self.write("| ");
        }

        for child in node.children() {
            if child.kind() != SyntaxKind::PARAM {
                self.fmt_node(&child);
                return;
            }
        }
        self.fmt_token_expr_after_lambda_delim(node);
    }

    // ── Return ──────────────────────────────────────────────────

    pub(crate) fn fmt_return(&mut self, node: &SyntaxNode) {
        self.write("return");

        if let Some(child) = node.children().next() {
            self.write(" ");
            self.fmt_node(&child);
            return;
        }

        let has_value = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| !tok.kind().is_trivia() && tok.kind() != SyntaxKind::KW_RETURN)
        });
        if has_value {
            self.write(" ");
            self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_RETURN);
        }
    }

    // ── Grouped / Array / Wrapper ───────────────────────────────

    pub(crate) fn fmt_grouped(&mut self, node: &SyntaxNode) {
        self.write("(");
        for child in node.children() {
            self.fmt_node(&child);
        }
        if node.children().next().is_none() {
            self.fmt_tokens_inside_parens(node);
        }
        self.write(")");
    }

    pub(crate) fn fmt_array(&mut self, node: &SyntaxNode) {
        self.write("[");
        let mut first = true;
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    if !first {
                        self.write(", ");
                    }
                    self.fmt_node(&child);
                    first = false;
                }
                rowan::NodeOrToken::Token(tok) => match tok.kind() {
                    SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::BOOL
                    | SyntaxKind::IDENT
                    | SyntaxKind::UNDERSCORE
                    | SyntaxKind::KW_NONE => {
                        if !first {
                            self.write(", ");
                        }
                        self.write(tok.text());
                        first = false;
                    }
                    _ => {}
                },
            }
        }
        self.write("]");
    }

    pub(crate) fn fmt_wrapper_expr(&mut self, node: &SyntaxNode) {
        let keyword = match node.kind() {
            SyntaxKind::OK_EXPR => "Ok",
            SyntaxKind::ERR_EXPR => "Err",
            SyntaxKind::SOME_EXPR => "Some",
            _ => unreachable!(),
        };
        self.write(keyword);
        self.write("(");
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_tokens_inside_parens(node);
        }
        self.write(")");
    }
}
