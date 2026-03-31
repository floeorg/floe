use crate::syntax::{SyntaxKind, SyntaxNode};

use super::{Formatter, PipeSegment};

enum NamedArgValue {
    Ident(String),
    Other,
    None,
}

impl Formatter<'_> {
    pub(crate) fn fmt_block(&mut self, node: &SyntaxNode) {
        self.write("{");

        // Block entries: either items/exprs or standalone comments.
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
                    let trailing_comments = self.trailing_comments_in(&child);
                    let trailing_blank = self.has_trailing_blank_line(&child);
                    entries.push(BlockEntry::Item(child, saw_blank));
                    saw_blank = trailing_blank;
                    for comment in trailing_comments {
                        entries.push(BlockEntry::Comment(comment, false));
                    }
                }
                _ => {}
            }
        }

        if entries.is_empty() {
            self.write("}");
            return;
        }

        // Count only items (not comments) for final-expr blank line logic
        let item_count = entries
            .iter()
            .filter(|e| matches!(e, BlockEntry::Item(..)))
            .count();
        let mut item_index = 0;

        self.indent += 1;
        for (i, entry) in entries.iter().enumerate() {
            match entry {
                BlockEntry::Item(child, had_blank) => {
                    if i > 0 {
                        let is_final_expr = item_count >= 2 && item_index == item_count - 1;
                        if *had_blank || is_final_expr {
                            self.newline();
                        }
                    }
                    self.newline();
                    self.write_indent();
                    self.fmt_node(child);
                    item_index += 1;
                }
                BlockEntry::Comment(text, had_blank) => {
                    if i > 0 && *had_blank {
                        self.newline();
                    }
                    self.newline();
                    self.write_indent();
                    self.write(text);
                }
            }
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

        // Try inline first
        let inline = self.try_inline(|f| {
            for (i, seg) in segments.iter().enumerate() {
                if i > 0 {
                    f.write(" |> ");
                }
                f.fmt_pipe_segment(seg);
            }
        });

        if self.fits_inline(&inline) {
            self.write(&inline);
        } else {
            // Vertical: first segment on current line, rest indented with |>
            for (i, seg) in segments.iter().enumerate() {
                if i > 0 {
                    self.newline();
                    self.write_indent();
                    self.write("    |> ");
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
        self.write("match");

        let mut wrote_subject = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::MATCH_ARM {
                break;
            }
            if !wrote_subject {
                self.write(" ");
                self.fmt_node(&child);
                wrote_subject = true;
            }
        }
        if !wrote_subject {
            // Check if this is a subjectless match (piped): `|> match { ... }`
            // The first non-trivia token after `match` keyword is `{`, so don't emit it as a subject.
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
                self.write(" ");
                self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_MATCH);
            }
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

        // Guard: `when expr`
        if let Some(guard) = node
            .children()
            .find(|c| c.kind() == SyntaxKind::MATCH_GUARD)
        {
            self.write(" when ");
            // Format the guard expression (skip the `when` keyword token)
            for t in guard.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::KW_WHEN || tok.kind().is_trivia() {
                        continue;
                    }
                    self.write(tok.text());
                    break;
                }
                if let Some(child) = t.into_node() {
                    self.fmt_node(&child);
                    break;
                }
            }
        }

        // Body: expression after ->
        // Check if the body is a JSX element with multiline props — if so, break
        // after `->` and indent so the JSX gets its own clean indentation context.
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
            self.write(" ->");
            self.indent += 1;
            self.newline();
            self.write_indent();
            self.fmt_node(body_node.as_ref().unwrap());
            self.indent -= 1;
            return;
        }

        self.write(" -> ");

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
                    SyntaxKind::L_PAREN => {
                        self.write("(");
                        for (i, p) in sub_patterns.iter().enumerate() {
                            if i > 0 {
                                self.write(", ");
                            }
                            self.fmt_pattern(p);
                        }
                        self.write(")");
                        return;
                    }
                    SyntaxKind::L_BRACKET => {
                        self.write("[");
                        let mut first_elem = true;
                        let mut saw_dotdot = false;
                        for inner in node.children_with_tokens() {
                            match &inner {
                                rowan::NodeOrToken::Token(t) => match t.kind() {
                                    SyntaxKind::DOT_DOT => {
                                        if !first_elem {
                                            self.write(", ");
                                        }
                                        self.write("..");
                                        saw_dotdot = true;
                                        first_elem = false;
                                    }
                                    SyntaxKind::IDENT if saw_dotdot => {
                                        self.write(t.text());
                                        saw_dotdot = false;
                                    }
                                    SyntaxKind::UNDERSCORE if saw_dotdot => {
                                        self.write("_");
                                        saw_dotdot = false;
                                    }
                                    _ => {}
                                },
                                rowan::NodeOrToken::Node(child)
                                    if child.kind() == SyntaxKind::PATTERN =>
                                {
                                    if !first_elem {
                                        self.write(", ");
                                    }
                                    self.fmt_pattern(child);
                                    first_elem = false;
                                }
                                _ => {}
                            }
                        }
                        self.write("]");
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
        // Format callee, including generic type args like `Array<Todo>(...)`
        // CST structure: IDENT LESS_THAN TYPE_EXPR* GREATER_THAN L_PAREN ARG* R_PAREN
        let mut wrote_callee = false;
        let mut in_type_args = false;
        let mut first_type_arg = true;

        for child_or_tok in node.children_with_tokens() {
            match &child_or_tok {
                rowan::NodeOrToken::Token(tok) => {
                    if tok.kind() == SyntaxKind::L_PAREN {
                        break; // Done with callee, start args
                    }
                    if tok.kind().is_trivia() {
                        continue;
                    }
                    if tok.kind() == SyntaxKind::LESS_THAN {
                        self.write("<");
                        in_type_args = true;
                        first_type_arg = true;
                        wrote_callee = true;
                        continue;
                    }
                    if tok.kind() == SyntaxKind::GREATER_THAN {
                        self.write(">");
                        in_type_args = false;
                        continue;
                    }
                    if !wrote_callee {
                        self.write(tok.text());
                        wrote_callee = true;
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if child.kind() == SyntaxKind::ARG {
                        break; // Done with callee
                    }
                    if in_type_args && child.kind() == SyntaxKind::TYPE_EXPR {
                        if !first_type_arg {
                            self.write(", ");
                        }
                        self.fmt_type_expr(child);
                        first_type_arg = false;
                    } else if !wrote_callee || !in_type_args {
                        self.fmt_node(child);
                        wrote_callee = true;
                    }
                }
            }
        }
        if !wrote_callee {
            self.fmt_token_callee(node);
        }

        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();

        // Try inline args
        let inline = self.try_inline(|f| {
            f.write("(");
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    f.write(", ");
                }
                f.fmt_arg(arg);
            }
            f.write(")");
        });

        if !inline.contains('\n') && self.fits_inline(&inline) {
            self.write(&inline);
        } else if args.len() == 1
            && inline.contains('\n')
            && self.fits_inline(&inline)
            && self.arg_has_jsx_arrow_body(&args[0])
        {
            // Single arg with arrow returning multiline JSX: hug format
            // map((item) =>
            //     <Body />
            // )
            self.write("(");
            self.fmt_arg(&args[0]);
            self.newline();
            self.write_indent();
            self.write(")");
        } else if self.fits_inline(&inline) {
            // Multiline content but not JSX arrow: write as-is (e.g. match in arrow)
            self.write(&inline);
        } else {
            // Multi-line args
            self.write("(");
            self.indent += 1;
            for arg in &args {
                self.newline();
                self.write_indent();
                self.fmt_arg(arg);
                self.write(",");
            }
            self.indent -= 1;
            self.newline();
            self.write_indent();
            self.write(")");
        }
    }

    /// Check if an ARG node contains an arrow expression whose body is a multiline JSX element.
    /// Only checks direct children of the arg, not deeply nested arrows.
    fn arg_has_jsx_arrow_body(&self, arg: &SyntaxNode) -> bool {
        arg.children().any(|c| {
            c.kind() == SyntaxKind::ARROW_EXPR
                && c.children().any(|body| self.is_multiline_jsx(&body))
        })
    }

    pub(crate) fn fmt_arg(&mut self, node: &SyntaxNode) {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let name = self.first_ident(node);
            let value_kind = self.named_arg_value_kind(node);

            // Pun: emit `name:` when value is same identifier as label, or no value at all
            if let Some(ref label) = name {
                match &value_kind {
                    NamedArgValue::Ident(val) if label == val => {
                        self.write(label);
                        self.write(":");
                        return;
                    }
                    NamedArgValue::None => {
                        self.write(label);
                        self.write(":");
                        return;
                    }
                    _ => {}
                }
            }

            if let Some(name) = name {
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

    /// Classify the value part of a named arg (after the colon).
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

        let spread = node
            .children()
            .find(|c| c.kind() == SyntaxKind::SPREAD_EXPR);
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();

        // Try inline
        let inline = self.try_inline(|f| {
            f.write("(");
            let mut first = true;
            if let Some(spread) = &spread {
                f.fmt_spread(spread);
                first = false;
            }
            for arg in &args {
                if !first {
                    f.write(", ");
                }
                f.fmt_arg(arg);
                first = false;
            }
            f.write(")");
        });

        if self.fits_inline(&inline) {
            self.write(&inline);
        } else {
            // Multi-line construct
            self.write("(");
            self.indent += 1;
            let mut first = true;
            if let Some(spread) = &spread {
                self.newline();
                self.write_indent();
                self.fmt_spread(spread);
                self.write(",");
                first = false;
            }
            for arg in &args {
                if !first {
                    // Already wrote comma after previous item
                }
                self.newline();
                self.write_indent();
                self.fmt_arg(arg);
                self.write(",");
                first = false;
            }
            self.indent -= 1;
            self.newline();
            self.write_indent();
            self.write(")");
        }
    }

    fn fmt_spread(&mut self, spread: &SyntaxNode) {
        self.write("..");
        if let Some(child) = spread.children().next() {
            self.fmt_node(&child);
        } else {
            // No child node — find ident/token after DOT_DOT
            let mut past_dots = false;
            for t in spread.children_with_tokens() {
                if let Some(tok) = t.as_token() {
                    if tok.kind() == SyntaxKind::DOT_DOT {
                        past_dots = true;
                        continue;
                    }
                    if past_dots && !tok.kind().is_trivia() {
                        self.write(tok.text());
                        break;
                    }
                }
            }
        }
    }

    // ── Member / Index / Unwrap ─────────────────────────────────

    pub(crate) fn fmt_member(&mut self, node: &SyntaxNode) {
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_token_callee(node);
        }
        self.write(".");

        // Find the field name or tuple index after the dot
        let mut found_dot = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::DOT {
                    found_dot = true;
                } else if found_dot && tok.kind().is_member_name() {
                    self.write(tok.text());
                    return;
                }
            }
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

        // Check if this is an async lambda
        let is_async = self.has_token(node, SyntaxKind::KW_ASYNC);
        if is_async {
            self.write("async ");
        }

        self.write("(");
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.fmt_param(param);
        }
        self.write(")");

        // Find the body node
        let body = node.children().find(|c| c.kind() != SyntaxKind::PARAM);

        if let Some(body) = body {
            // Only break after => for JSX elements that format as multiline.
            // Blocks and match expressions should stay on the same line as =>.
            if self.is_multiline_jsx(&body) {
                self.write(" =>");
                self.indent += 1;
                self.newline();
                self.write_indent();
                self.fmt_node(&body);
                self.indent -= 1;
            } else {
                self.write(" => ");
                self.fmt_node(&body);
            }
        } else {
            self.write(" => ");
            self.fmt_token_expr_after_lambda_delim(node);
        }
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

    pub(crate) fn fmt_tuple(&mut self, node: &SyntaxNode) {
        self.write("(");
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
                    | SyntaxKind::UNDERSCORE => {
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
        self.write(")");
    }

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
        let mut elems = Vec::new();
        for child_or_tok in node.children_with_tokens() {
            match child_or_tok {
                rowan::NodeOrToken::Node(child) => {
                    elems.push(PipeSegment::Node(child));
                }
                rowan::NodeOrToken::Token(tok) => match tok.kind() {
                    SyntaxKind::NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::BOOL
                    | SyntaxKind::IDENT
                    | SyntaxKind::UNDERSCORE => {
                        elems.push(PipeSegment::Token(tok.text().to_string()));
                    }
                    _ => {}
                },
            }
        }

        let inline = self.try_inline(|f| {
            f.write("[");
            for (i, elem) in elems.iter().enumerate() {
                if i > 0 {
                    f.write(", ");
                }
                f.fmt_pipe_segment(elem);
            }
            f.write("]");
        });

        // Unlike fmt_call/fmt_construct, arrays need the contains('\n') guard
        // because a nested element (e.g. a constructor) may break internally
        // while the first line stays short enough to pass fits_inline.
        if !inline.contains('\n') && self.fits_inline(&inline) {
            self.write(&inline);
        } else {
            self.write("[");
            self.indent += 1;
            for elem in &elems {
                self.newline();
                self.write_indent();
                self.fmt_pipe_segment(elem);
                self.write(",");
            }
            self.indent -= 1;
            self.newline();
            self.write_indent();
            self.write("]");
        }
    }

    pub(crate) fn fmt_parse_expr(&mut self, node: &SyntaxNode) {
        self.write("parse<");
        // Find and format the TYPE_EXPR child
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                self.fmt_node(&child);
                break;
            }
        }
        self.write(">");
        // Check if there's a value expression (non-TYPE_EXPR child)
        let value_child = node.children().find(|c| c.kind() != SyntaxKind::TYPE_EXPR);
        if let Some(value) = value_child {
            self.write("(");
            self.fmt_node(&value);
            self.write(")");
        }
    }

    pub(crate) fn fmt_mock_expr(&mut self, node: &SyntaxNode) {
        self.write("mock<");
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                self.fmt_node(&child);
                break;
            }
        }
        self.write(">");
        // Check if there are ARG children (overrides)
        let args: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::ARG)
            .collect();
        if !args.is_empty() {
            self.write("(");
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_node(arg);
            }
            self.write(")");
        }
    }

    pub(crate) fn fmt_wrapper_expr(&mut self, node: &SyntaxNode) {
        let keyword = match node.kind() {
            SyntaxKind::VALUE_EXPR => "Value",
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
