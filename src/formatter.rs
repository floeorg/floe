use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::syntax::{SyntaxKind, SyntaxNode};

/// Format ZenScript source code.
pub fn format(source: &str) -> String {
    let tokens = Lexer::new(source).tokenize_with_trivia();
    let parse = CstParser::new(source, tokens).parse();
    let root = parse.syntax();
    let mut formatter = Formatter::new(source);
    formatter.fmt_node(&root);
    formatter.finish()
}

enum JsxChildInfo {
    Text(String),
    Expr(SyntaxNode),
    Element(SyntaxNode),
}

enum PipeSegment {
    Node(SyntaxNode),
    Token(String),
}

struct Formatter<'src> {
    source: &'src str,
    out: String,
    indent: usize,
    /// True if we just wrote a newline (and possibly indent), so we know
    /// not to double-space.
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
        // Ensure trailing newline
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        // Remove trailing blank lines (keep exactly one \n)
        while self.out.ends_with("\n\n") {
            self.out.pop();
        }
        self.out
    }

    fn fmt_node(&mut self, node: &SyntaxNode) {
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
            SyntaxKind::IF_EXPR => self.fmt_if(node),
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
            SyntaxKind::JSX_ELEMENT => self.fmt_jsx(node),
            SyntaxKind::TYPE_DEF_UNION => self.fmt_union(node),
            SyntaxKind::TYPE_DEF_RECORD => self.fmt_record_def(node),
            SyntaxKind::TYPE_DEF_ALIAS => self.fmt_type_alias_def(node),
            SyntaxKind::TYPE_EXPR => self.fmt_type_expr(node),
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
                // Blank line between different kinds of top-level items,
                // or between any non-import items
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

    // ── Items ───────────────────────────────────────────────────

    fn fmt_item(&mut self, node: &SyntaxNode) {
        // ITEM wraps export? + decl
        let has_export = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|t| t.kind() == SyntaxKind::KW_EXPORT)
        });

        if has_export {
            self.write("export ");
        }

        for child in node.children() {
            self.fmt_node(&child);
        }
    }

    fn fmt_expr_item(&mut self, node: &SyntaxNode) {
        // Expression statement
        for child in node.children() {
            self.fmt_node(&child);
        }
        // Also handle bare token exprs
        if node.children().next().is_none() {
            self.fmt_tokens_only(node);
        }
    }

    // ── Import ──────────────────────────────────────────────────

    fn fmt_import(&mut self, node: &SyntaxNode) {
        self.write("import ");

        let specifiers: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::IMPORT_SPECIFIER)
            .collect();

        if !specifiers.is_empty() {
            self.write("{ ");
            for (i, spec) in specifiers.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_import_specifier(spec);
            }
            self.write(" } ");
        }

        self.write("from ");

        // Find the string token
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                self.write(tok.text());
            }
        }
    }

    fn fmt_import_specifier(&mut self, node: &SyntaxNode) {
        let idents: Vec<_> = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::BANNED)
            .collect();

        if let Some(name) = idents.first() {
            self.write(name.text());
        }
        if idents.len() > 1 {
            // `as alias` — the "as" might be BANNED token
            self.write(" as ");
            if let Some(alias) = idents.last() {
                self.write(alias.text());
            }
        }
    }

    // ── Const ───────────────────────────────────────────────────

    fn fmt_const(&mut self, node: &SyntaxNode) {
        self.write("const ");

        // Collect tokens to figure out binding shape
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lbrace_before_eq = self.has_brace_destructuring(node);

        if has_lbracket {
            self.write("[");
            let idents = self.collect_idents(node);
            self.write(&idents.join(", "));
            self.write("]");
        } else if has_lbrace_before_eq {
            self.write("{ ");
            let idents = self.collect_idents_before_eq(node);
            self.write(&idents.join(", "));
            self.write(" }");
        } else {
            let idents = self.collect_idents_before_colon_or_eq(node);
            if let Some(name) = idents.first() {
                self.write(name);
            }
        }

        // Type annotation
        let type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();
        if let Some(type_expr) = type_exprs.first() {
            self.write(": ");
            self.fmt_type_expr(type_expr);
        }

        self.write(" = ");

        // Value: find expression after =
        let expr = self.find_expr_after_eq(node);
        if let Some(expr) = expr {
            self.fmt_node(&expr);
        } else {
            // Token-level expression
            self.fmt_token_expr_after_eq(node);
        }
    }

    // ── Function ────────────────────────────────────────────────

    fn fmt_function(&mut self, node: &SyntaxNode) {
        let has_async = self.has_token(node, SyntaxKind::KW_ASYNC);
        if has_async {
            self.write("async ");
        }
        self.write("function ");

        // Name
        let name = self.first_ident(node);
        if let Some(name) = name {
            self.write(&name);
        }

        // Params
        self.write("(");
        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.fmt_param(param);
        }
        self.write(")");

        // Return type — find TYPE_EXPR that's NOT inside a PARAM
        let return_type = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR);
        if let Some(rt) = return_type {
            self.write(": ");
            self.fmt_type_expr(&rt);
        }

        self.write(" ");

        // Body
        if let Some(block) = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR) {
            self.fmt_block(&block);
        }
    }

    fn fmt_param(&mut self, node: &SyntaxNode) {
        let name = self.first_ident(node);
        if let Some(name) = name {
            self.write(&name);
        }

        // Type annotation
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.write(": ");
            self.fmt_type_expr(&type_expr);
        }

        // Default value
        if self.has_token(node, SyntaxKind::EQUAL) {
            self.write(" = ");
            self.fmt_token_expr_after_eq(node);
        }
    }

    // ── Type Declaration ────────────────────────────────────────

    fn fmt_type_decl(&mut self, node: &SyntaxNode) {
        let has_opaque = self.has_token(node, SyntaxKind::KW_OPAQUE);
        if has_opaque {
            self.write("opaque ");
        }
        self.write("type ");

        let idents = self.collect_idents_direct(node);
        if let Some(name) = idents.first() {
            self.write(name);
        }

        // Type params
        if idents.len() > 1 {
            self.write("<");
            self.write(&idents[1..].join(", "));
            self.write(">");
        }

        self.write(" =");

        // Type def
        for child in node.children() {
            match child.kind() {
                SyntaxKind::TYPE_DEF_UNION => {
                    self.fmt_union(&child);
                }
                SyntaxKind::TYPE_DEF_RECORD => {
                    self.write(" ");
                    self.fmt_record_def(&child);
                }
                SyntaxKind::TYPE_DEF_ALIAS => {
                    self.write(" ");
                    self.fmt_type_alias_def(&child);
                }
                _ => {}
            }
        }
    }

    fn fmt_union(&mut self, node: &SyntaxNode) {
        let variants: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::VARIANT)
            .collect();

        self.indent += 1;
        for variant in &variants {
            self.newline();
            self.write_indent();
            self.write("| ");
            self.fmt_variant(variant);
        }
        self.indent -= 1;
    }

    fn fmt_variant(&mut self, node: &SyntaxNode) {
        // Skip the "|" ident — it's the union separator, not the variant name
        let name = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT && t.text() != "|")
            .map(|t| t.text().to_string());
        if let Some(name) = name {
            self.write(&name);
        }

        let fields: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::VARIANT_FIELD)
            .collect();

        if !fields.is_empty() {
            self.write("(");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_variant_field(field);
            }
            self.write(")");
        }
    }

    fn fmt_variant_field(&mut self, node: &SyntaxNode) {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        let idents = self.collect_idents(node);

        if has_colon && let Some(name) = idents.first() {
            self.write(name);
            self.write(": ");
        }

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }
    }

    fn fmt_record_def(&mut self, node: &SyntaxNode) {
        let fields: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
            .collect();

        self.write("{");
        if fields.is_empty() {
            self.write("}");
            return;
        }

        self.indent += 1;
        for (i, field) in fields.iter().enumerate() {
            self.newline();
            self.write_indent();
            self.fmt_record_field(field);
            self.write(",");
            let _ = i;
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    fn fmt_record_field(&mut self, node: &SyntaxNode) {
        let name = self.first_ident(node);
        if let Some(name) = name {
            self.write(&name);
        }
        self.write(": ");
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }

        if self.has_token(node, SyntaxKind::EQUAL) {
            self.write(" = ");
            self.fmt_token_expr_after_eq(node);
        }
    }

    fn fmt_type_alias_def(&mut self, node: &SyntaxNode) {
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }
    }

    // ── Type Expressions ────────────────────────────────────────

    fn fmt_type_expr(&mut self, node: &SyntaxNode) {
        let idents = self.collect_idents(node);
        let has_fat_arrow = self.has_token(node, SyntaxKind::FAT_ARROW);
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);
        let has_record_fields = node
            .children()
            .any(|c| c.kind() == SyntaxKind::RECORD_FIELD);
        let child_type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();

        // Unit type: ()
        if has_lparen && idents.is_empty() && !has_fat_arrow && child_type_exprs.is_empty() {
            self.write("()");
            return;
        }

        // Function type: (params) => ReturnType
        if has_fat_arrow {
            self.write("(");
            let param_count = child_type_exprs.len().saturating_sub(1);
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i == param_count {
                    break;
                }
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write(") => ");
            if let Some(ret) = child_type_exprs.last() {
                self.fmt_type_expr(ret);
            }
            return;
        }

        // Tuple: [T, U]
        if has_lbracket {
            self.write("[");
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write("]");
            return;
        }

        // Record type
        if has_record_fields {
            let fields: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
                .collect();
            self.write("{ ");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_record_field(field);
            }
            self.write(" }");
            return;
        }

        // Named type with dots
        let has_dot = self.has_token(node, SyntaxKind::DOT);
        if has_dot {
            self.write(&idents.join("."));
        } else if let Some(name) = idents.first() {
            self.write(name);
        }

        // Type args
        if !child_type_exprs.is_empty() {
            self.write("<");
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write(">");
        }
    }

    // ── Expressions ─────────────────────────────────────────────

    fn fmt_block(&mut self, node: &SyntaxNode) {
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

    fn fmt_pipe(&mut self, node: &SyntaxNode) {
        // Collect all segments of the pipe chain (tokens + nodes), flattened
        let mut segments: Vec<PipeSegment> = Vec::new();
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

        // Walk children_with_tokens to find left, |>, right
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
                        // Recursively flatten the left pipe chain
                        self.collect_pipe_segments(&child, segments);
                    } else {
                        left_nodes.push(PipeSegment::Node(child));
                    }
                }
            }
        }

        // If left_nodes weren't already added via recursion, add them
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

    fn fmt_match(&mut self, node: &SyntaxNode) {
        self.write("match ");

        // Subject: first non-MATCH_ARM child expression
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
            // Token-level subject
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
        // Pattern -> Expr
        if let Some(pattern) = node.children().find(|c| c.kind() == SyntaxKind::PATTERN) {
            self.fmt_pattern(&pattern);
        }
        self.write(" -> ");

        // Body: expression after ->
        let mut past_arrow = false;
        for child in node.children() {
            if child.kind() == SyntaxKind::PATTERN {
                continue;
            }
            if past_arrow || child.kind() != SyntaxKind::PATTERN {
                self.fmt_node(&child);
                return;
            }
        }

        // Token-level body after ->
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

    fn fmt_pattern(&mut self, node: &SyntaxNode) {
        let sub_patterns: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PATTERN)
            .collect();

        // Check tokens
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                match tok.kind() {
                    SyntaxKind::UNDERSCORE => {
                        self.write("_");
                        return;
                    }
                    SyntaxKind::BOOL | SyntaxKind::STRING | SyntaxKind::NUMBER => {
                        self.write(tok.text());
                        // Check for range
                        if self.has_token(node, SyntaxKind::DOT_DOT) {
                            let numbers: Vec<_> = node
                                .children_with_tokens()
                                .filter_map(|t| t.into_token())
                                .filter(|t| t.kind() == SyntaxKind::NUMBER)
                                .collect();
                            if numbers.len() >= 2 {
                                // Clear what we wrote and redo
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
                        // Record pattern: { x, y } or { x: pattern }
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

    fn fmt_if(&mut self, node: &SyntaxNode) {
        self.write("if ");

        let children: Vec<_> = node.children().collect();
        let mut child_iter = children.iter();

        // Condition
        if let Some(cond) = child_iter.next() {
            if cond.kind() == SyntaxKind::BLOCK_EXPR {
                // Condition was a token expr, need to find it
                self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_IF);
                self.write(" ");
                self.fmt_block(cond);
            } else {
                self.fmt_node(cond);
                self.write(" ");
                // Then block
                if let Some(then_block) = child_iter.next() {
                    self.fmt_node(then_block);
                }
            }
        }

        // Else
        let has_else = self.has_token(node, SyntaxKind::KW_ELSE);
        if has_else {
            self.write(" else ");
            // Else branch — either if-else or block
            if let Some(else_node) = child_iter.next() {
                self.fmt_node(else_node);
            }
        }
    }

    fn fmt_binary(&mut self, node: &SyntaxNode) {
        // Walk children_with_tokens to handle both node and token operands
        let mut phase = 0; // 0=left, 1=op, 2=right
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
                        // Left operand is a token
                        self.write(tok.text());
                        phase = 1;
                    } else if phase >= 2 {
                        // Right operand is a token
                        self.write(tok.text());
                        phase = 3;
                    }
                }
            }
        }
    }

    fn fmt_unary(&mut self, node: &SyntaxNode) {
        // Find the operator
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

        // Operand
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_tokens_after_op(node);
        }
    }

    fn fmt_call(&mut self, node: &SyntaxNode) {
        // Callee
        let children_iter = node.children().peekable();
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
        let _ = children_iter;
    }

    fn fmt_arg(&mut self, node: &SyntaxNode) {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let name = self.first_ident(node);
            if let Some(name) = name {
                self.write(&name);
                self.write(": ");
            }
            // Value after colon
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
            // Positional arg
            if let Some(child) = node.children().next() {
                self.fmt_node(&child);
                return;
            }
            self.fmt_tokens_only(node);
        }
    }

    fn fmt_construct(&mut self, node: &SyntaxNode) {
        let name = self.first_ident(node);
        if let Some(name) = name {
            self.write(&name);
        }
        self.write("(");

        // Spread
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

    fn fmt_member(&mut self, node: &SyntaxNode) {
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_token_callee(node);
        }
        self.write(".");
        // Field name: last ident
        let idents = self.collect_idents(node);
        if let Some(field) = idents.last() {
            self.write(field);
        }
    }

    fn fmt_index(&mut self, node: &SyntaxNode) {
        let children: Vec<_> = node.children().collect();
        if let Some(obj) = children.first() {
            self.fmt_node(obj);
        }
        self.write("[");
        if children.len() >= 2 {
            self.fmt_node(&children[1]);
        } else {
            // Token index
            self.fmt_token_expr_inside_brackets(node);
        }
        self.write("]");
    }

    fn fmt_unwrap(&mut self, node: &SyntaxNode) {
        if let Some(child) = node.children().next() {
            self.fmt_node(&child);
        } else {
            self.fmt_tokens_only(node);
        }
        self.write("?");
    }

    fn fmt_arrow(&mut self, node: &SyntaxNode) {
        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();

        if params.len() == 1 && !self.param_has_type(&params[0]) {
            // Single untyped param: `x => expr`
            let name = self.first_ident(&params[0]);
            if let Some(name) = name {
                self.write(&name);
            }
        } else {
            self.write("(");
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_param(param);
            }
            self.write(")");
        }

        self.write(" => ");

        // Body: first non-PARAM child
        for child in node.children() {
            if child.kind() != SyntaxKind::PARAM {
                self.fmt_node(&child);
                return;
            }
        }
        // Token body
        self.fmt_token_expr_after_fat_arrow(node);
    }

    fn fmt_return(&mut self, node: &SyntaxNode) {
        self.write("return");

        // Value
        if let Some(child) = node.children().next() {
            self.write(" ");
            self.fmt_node(&child);
            return;
        }
        // Token value
        let has_value = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| !tok.kind().is_trivia() && tok.kind() != SyntaxKind::KW_RETURN)
        });
        if has_value {
            self.write(" ");
            self.fmt_token_expr_after_keyword(node, SyntaxKind::KW_RETURN);
        }
    }

    fn fmt_grouped(&mut self, node: &SyntaxNode) {
        self.write("(");
        for child in node.children() {
            self.fmt_node(&child);
        }
        if node.children().next().is_none() {
            self.fmt_tokens_inside_parens(node);
        }
        self.write(")");
    }

    fn fmt_array(&mut self, node: &SyntaxNode) {
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

    fn fmt_wrapper_expr(&mut self, node: &SyntaxNode) {
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
            self.write(")");
            return;
        }
        // Token inner
        self.fmt_tokens_inside_parens(node);
        self.write(")");
    }

    fn fmt_jsx(&mut self, node: &SyntaxNode) {
        // Detect fragment vs element by looking for tag name
        let tag_name = self.jsx_tag_name(node);
        let is_fragment = tag_name.is_none();
        let is_self_closing =
            self.has_token(node, SyntaxKind::SLASH) && !self.jsx_has_children(node);

        let props: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::JSX_PROP)
            .collect();

        let children = self.jsx_collect_children(node);

        if is_fragment {
            // <>children</>
            self.write("<>");
            if children.is_empty() {
                self.write("</>");
                return;
            }
            self.fmt_jsx_children(&children);
            self.write("</>");
            return;
        }

        let name = tag_name.unwrap();

        // Opening tag
        self.write("<");
        self.write(&name);

        // Props
        if !props.is_empty() {
            if props.len() <= 3 && self.jsx_props_short(&props) {
                // Inline props
                for prop in &props {
                    self.write(" ");
                    self.fmt_jsx_prop(prop);
                }
            } else {
                // Multi-line props
                self.indent += 1;
                for prop in &props {
                    self.newline();
                    self.write_indent();
                    self.fmt_jsx_prop(prop);
                }
                self.indent -= 1;
                self.newline();
                self.write_indent();
            }
        }

        if is_self_closing {
            self.write(" />");
            return;
        }

        self.write(">");

        if children.is_empty() {
            self.write("</");
            self.write(&name);
            self.write(">");
            return;
        }

        // Single text or single expr child → inline
        let inline = children.len() == 1
            && matches!(&children[0], JsxChildInfo::Text(_) | JsxChildInfo::Expr(_));

        if inline {
            self.fmt_jsx_children_inline(&children);
        } else {
            // Multi-line children
            self.indent += 1;
            self.fmt_jsx_children(&children);
            self.indent -= 1;
            self.newline();
            self.write_indent();
        }

        self.write("</");
        self.write(&name);
        self.write(">");
    }

    fn fmt_jsx_prop(&mut self, node: &SyntaxNode) {
        let name = self.first_ident(node);
        if let Some(name) = name {
            self.write(&name);
        }

        // Check for value
        let has_eq = self.has_token(node, SyntaxKind::EQUAL);
        if !has_eq {
            return; // Boolean prop
        }

        self.write("=");

        // Find value: string literal or {expr}
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        if has_lbrace {
            self.write("{");
            // Find the expression inside braces
            let mut inside = false;
            for child_or_tok in node.children_with_tokens() {
                match child_or_tok {
                    rowan::NodeOrToken::Token(tok) => {
                        if tok.kind() == SyntaxKind::L_BRACE {
                            inside = true;
                            continue;
                        }
                        if tok.kind() == SyntaxKind::R_BRACE {
                            break;
                        }
                        if inside && !tok.kind().is_trivia() {
                            self.write(tok.text());
                        }
                    }
                    rowan::NodeOrToken::Node(child) => {
                        if inside {
                            self.fmt_node(&child);
                        }
                    }
                }
            }
            self.write("}");
        } else {
            // String value
            for t in node.children_with_tokens() {
                if let Some(tok) = t.as_token()
                    && tok.kind() == SyntaxKind::STRING
                {
                    self.write(tok.text());
                    break;
                }
            }
        }
    }

    fn fmt_jsx_children_inline(&mut self, children: &[JsxChildInfo]) {
        for child in children {
            match child {
                JsxChildInfo::Text(text) => {
                    self.write(text.trim());
                }
                JsxChildInfo::Expr(node) => {
                    self.write("{");
                    let mut inside = false;
                    for child_or_tok in node.children_with_tokens() {
                        match child_or_tok {
                            rowan::NodeOrToken::Token(tok) => {
                                if tok.kind() == SyntaxKind::L_BRACE {
                                    inside = true;
                                    continue;
                                }
                                if tok.kind() == SyntaxKind::R_BRACE {
                                    break;
                                }
                                if inside && !tok.kind().is_trivia() {
                                    self.write(tok.text());
                                }
                            }
                            rowan::NodeOrToken::Node(child) => {
                                if inside {
                                    self.fmt_node(&child);
                                }
                            }
                        }
                    }
                    self.write("}");
                }
                JsxChildInfo::Element(node) => {
                    self.fmt_jsx(node);
                }
            }
        }
    }

    fn fmt_jsx_children(&mut self, children: &[JsxChildInfo]) {
        for child in children {
            match child {
                JsxChildInfo::Text(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        self.newline();
                        self.write_indent();
                        self.write(trimmed);
                    }
                }
                JsxChildInfo::Expr(node) => {
                    self.write("{");
                    // Find the expression inside the JSX_EXPR_CHILD
                    let mut inside = false;
                    for child_or_tok in node.children_with_tokens() {
                        match child_or_tok {
                            rowan::NodeOrToken::Token(tok) => {
                                if tok.kind() == SyntaxKind::L_BRACE {
                                    inside = true;
                                    continue;
                                }
                                if tok.kind() == SyntaxKind::R_BRACE {
                                    break;
                                }
                                if inside && !tok.kind().is_trivia() {
                                    self.write(tok.text());
                                }
                            }
                            rowan::NodeOrToken::Node(child) => {
                                if inside {
                                    self.fmt_node(&child);
                                }
                            }
                        }
                    }
                    self.write("}");
                }
                JsxChildInfo::Element(node) => {
                    self.newline();
                    self.write_indent();
                    self.fmt_jsx(node);
                }
            }
        }
    }

    fn jsx_tag_name(&self, node: &SyntaxNode) -> Option<String> {
        // The tag name is the first IDENT token (after <)
        let mut past_lt = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::LESS_THAN {
                    past_lt = true;
                    continue;
                }
                if past_lt && tok.kind() == SyntaxKind::IDENT {
                    return Some(tok.text().to_string());
                }
                if past_lt && !tok.kind().is_trivia() {
                    // Hit > or / before finding ident — fragment
                    return None;
                }
            }
        }
        None
    }

    fn jsx_has_children(&self, node: &SyntaxNode) -> bool {
        node.children().any(|c| {
            matches!(
                c.kind(),
                SyntaxKind::JSX_ELEMENT | SyntaxKind::JSX_EXPR_CHILD | SyntaxKind::JSX_TEXT
            )
        })
    }

    fn jsx_collect_children(&self, node: &SyntaxNode) -> Vec<JsxChildInfo> {
        let mut children = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChildInfo::Text(text));
                    }
                }
                SyntaxKind::JSX_EXPR_CHILD => {
                    children.push(JsxChildInfo::Expr(child));
                }
                SyntaxKind::JSX_ELEMENT => {
                    children.push(JsxChildInfo::Element(child));
                }
                _ => {}
            }
        }
        children
    }

    fn jsx_props_short(&self, props: &[SyntaxNode]) -> bool {
        // Estimate total prop text length
        let total: usize = props
            .iter()
            .map(|p| {
                let range = p.text_range();
                let len: usize = (range.end() - range.start()).into();
                len
            })
            .sum();
        total < 60
    }

    // ── Verbatim fallback ───────────────────────────────────────

    fn fmt_verbatim(&mut self, node: &SyntaxNode) {
        let range = node.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let text = self.source[start..end].trim();
        self.write(text);
    }

    // ── Low-level helpers ───────────────────────────────────────

    fn write(&mut self, s: &str) {
        self.out.push_str(s);
        self.at_line_start = s.ends_with('\n');
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.at_line_start = false;
    }

    fn has_token(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    fn first_ident(&self, node: &SyntaxNode) -> Option<String> {
        node.children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
    }

    fn collect_idents(&self, node: &SyntaxNode) -> Vec<String> {
        node.children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
            .collect()
    }

    fn collect_idents_direct(&self, node: &SyntaxNode) -> Vec<String> {
        self.collect_idents(node)
    }

    fn collect_idents_before_eq(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::EQUAL {
                    break;
                }
                if tok.kind() == SyntaxKind::IDENT {
                    idents.push(tok.text().to_string());
                }
            }
        }
        idents
    }

    fn collect_idents_before_colon_or_eq(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::EQUAL || tok.kind() == SyntaxKind::COLON {
                    break;
                }
                if tok.kind() == SyntaxKind::IDENT {
                    idents.push(tok.text().to_string());
                }
            }
        }
        idents
    }

    fn has_brace_destructuring(&self, node: &SyntaxNode) -> bool {
        // Check if { appears before = (destructuring, not type annotation)
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

    fn find_expr_after_eq(&self, node: &SyntaxNode) -> Option<SyntaxNode> {
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

    fn fmt_token_expr_after_eq(&mut self, node: &SyntaxNode) {
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

    fn fmt_tokens_only(&mut self, node: &SyntaxNode) {
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

    fn fmt_token_expr_after_keyword(&mut self, node: &SyntaxNode, keyword: SyntaxKind) {
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

    fn fmt_token_expr_after_fat_arrow(&mut self, node: &SyntaxNode) {
        let mut past = false;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token() {
                if tok.kind() == SyntaxKind::FAT_ARROW {
                    past = true;
                    continue;
                }
                if past && !tok.kind().is_trivia() {
                    self.write(tok.text());
                    return;
                }
            }
            if let Some(child) = t.into_node()
                && past
                && child.kind() != SyntaxKind::PARAM
            {
                self.fmt_node(&child);
                return;
            }
        }
    }

    fn fmt_tokens_after_op(&mut self, node: &SyntaxNode) {
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

    fn fmt_token_callee(&mut self, node: &SyntaxNode) {
        // Write the first non-trivia, non-paren token
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

    fn fmt_tokens_inside_parens(&mut self, node: &SyntaxNode) {
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

    fn fmt_token_expr_inside_brackets(&mut self, node: &SyntaxNode) {
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

    fn param_has_type(&self, node: &SyntaxNode) -> bool {
        node.children().any(|c| c.kind() == SyntaxKind::TYPE_EXPR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fmt(input: &str, expected: &str) {
        let result = format(input);
        assert_eq!(
            result.trim(),
            expected.trim(),
            "\n--- input ---\n{input}\n--- got ---\n{result}\n--- expected ---\n{expected}"
        );
    }

    #[test]
    fn format_const() {
        assert_fmt("const   x   =   42", "const x = 42");
    }

    #[test]
    fn format_const_typed() {
        assert_fmt("const x:number = 42", "const x: number = 42");
    }

    #[test]
    fn format_function() {
        assert_fmt(
            "function  add( a:number,b:number ):number{a+b}",
            "function add(a: number, b: number): number {\n    a + b\n}",
        );
    }

    #[test]
    fn format_import() {
        assert_fmt(
            r#"import {useState,useEffect} from "react""#,
            r#"import { useState, useEffect } from "react""#,
        );
    }

    #[test]
    fn format_type_record() {
        assert_fmt(
            "type User = {id:string,name:string}",
            "type User = {\n    id: string,\n    name: string,\n}",
        );
    }

    #[test]
    fn format_type_union() {
        assert_fmt(
            "type Route = |Home|Profile(id:string)|NotFound",
            "type Route =\n    | Home\n    | Profile(id: string)\n    | NotFound",
        );
    }

    #[test]
    fn format_match() {
        assert_fmt(
            "const x = match route {Home -> \"home\",NotFound -> \"404\"}",
            "const x = match route {\n    Home -> \"home\",\n    NotFound -> \"404\",\n}",
        );
    }

    #[test]
    fn format_pipe() {
        assert_fmt(
            "const _r = data|>transform|>format",
            "const _r = data |> transform |> format",
        );
    }

    #[test]
    fn format_arrow() {
        assert_fmt("const f = x=>x+1", "const f = x => x + 1");
    }

    #[test]
    fn format_blank_lines_between_items() {
        assert_fmt("const x = 1\nconst y = 2", "const x = 1\n\nconst y = 2");
    }

    #[test]
    fn format_export() {
        assert_fmt(
            "export function add(a:number,b:number):number{a+b}",
            "export function add(a: number, b: number): number {\n    a + b\n}",
        );
    }

    #[test]
    fn format_type_alias() {
        assert_fmt(
            "type UserId = Brand<string,UserId>",
            "type UserId = Brand<string, UserId>",
        );
    }

    // ── JSX ─────────────────────────────────────────────────────

    #[test]
    fn format_jsx_self_closing() {
        assert_fmt("<Button />", "<Button />");
    }

    #[test]
    fn format_jsx_self_closing_with_props() {
        assert_fmt(
            r#"<Button label="Save" onClick={handleSave} />"#,
            r#"<Button label="Save" onClick={handleSave} />"#,
        );
    }

    #[test]
    fn format_jsx_with_expr_child() {
        assert_fmt("<div>{x}</div>", "<div>{x}</div>");
    }

    #[test]
    fn format_jsx_with_nested_elements() {
        assert_fmt(
            "<div><h1>Title</h1><p>Body</p></div>",
            "<div>\n    <h1>Title</h1>\n    <p>Body</p>\n</div>",
        );
    }

    #[test]
    fn format_jsx_fragment() {
        assert_fmt("<>{x}</>", "<>{x}</>");
    }
}
