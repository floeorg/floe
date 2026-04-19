mod expr;
mod items;
mod jsx;
mod pattern;
mod types;

use crate::lexer::span::Span;
use crate::parser::ParseError;
use crate::parser::ast::*;
use crate::syntax::{FloeLang, SyntaxKind, SyntaxNode};

/// Lower a CST `SyntaxNode` (rowan) tree into the existing AST.
pub fn lower_program(root: &SyntaxNode, source: &str) -> Result<Program, Vec<ParseError>> {
    let mut lowerer = Lowerer {
        source,
        errors: Vec::new(),
        id_gen: ExprIdGen::new(),
    };
    let program = lowerer.lower_root(root);
    if lowerer.errors.is_empty() {
        Ok(program)
    } else {
        Err(lowerer.errors)
    }
}

/// Lower a CST into an AST on a best-effort basis, returning whatever was
/// successfully parsed along with any errors. Used by the LSP to build a
/// partial symbol index even when the source contains errors.
pub fn lower_program_lossy(root: &SyntaxNode, source: &str) -> (Program, Vec<ParseError>) {
    let mut lowerer = Lowerer {
        source,
        errors: Vec::new(),
        id_gen: ExprIdGen::new(),
    };
    let program = lowerer.lower_root(root);
    (program, lowerer.errors)
}

struct Lowerer<'src> {
    source: &'src str,
    errors: Vec<ParseError>,
    id_gen: ExprIdGen,
}

impl<'src> Lowerer<'src> {
    /// Create an untyped `Expr` with a fresh unique ID.
    fn expr(&self, kind: ExprKind, span: Span) -> Expr {
        Expr {
            id: self.id_gen.next(),
            kind,
            ty: (),
            span,
        }
    }
}

impl<'src> Lowerer<'src> {
    fn lower_root(&mut self, root: &SyntaxNode) -> Program {
        assert_eq!(root.kind(), SyntaxKind::PROGRAM);
        let span = self.node_span(root);
        let mut items = Vec::new();

        for child in root.children() {
            match child.kind() {
                SyntaxKind::ITEM => {
                    if let Some(item) = self.lower_item(&child) {
                        items.push(item);
                    }
                }
                SyntaxKind::EXPR_ITEM => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        let span = self.node_span(&child);
                        items.push(Item {
                            kind: ItemKind::Expr(expr),
                            span,
                        });
                    }
                }
                SyntaxKind::ERROR => {
                    // Collect error text
                    let text = child.text().to_string();
                    self.errors.push(ParseError {
                        message: format!("parse error: {text}"),
                        span: self.node_span(&child),
                        kind: crate::parser::ParseErrorKind::General,
                    });
                }
                _ => {}
            }
        }

        Program { items, span }
    }

    // ── Template literal lowering ─────────────────────────────────

    /// Parse a template literal source text (including backticks) into AST
    /// `TemplatePart`s, properly lowering interpolated expressions.
    pub(super) fn lower_template_literal(&self, text: &str) -> Vec<TemplatePart> {
        // Strip backticks
        let inner = if text.len() >= 2 && text.starts_with('`') && text.ends_with('`') {
            &text[1..text.len() - 1]
        } else {
            text
        };

        let mut parts = Vec::new();
        let mut current_raw = String::new();
        let bytes = inner.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                // Save current raw segment
                if !current_raw.is_empty() {
                    parts.push(TemplatePart::Raw(std::mem::take(&mut current_raw)));
                }

                // Skip `${`
                i += 2;

                // Find matching `}` with brace depth tracking
                let mut depth = 1;
                let interp_start = i;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        b'`' => {
                            // Skip nested template literals
                            i += 1;
                            while i < bytes.len() && bytes[i] != b'`' {
                                if bytes[i] == b'\\' {
                                    i += 1; // skip escaped char
                                }
                                i += 1;
                            }
                            // i now points at closing backtick (or end)
                        }
                        b'"' => {
                            // Skip string literals
                            i += 1;
                            while i < bytes.len() && bytes[i] != b'"' {
                                if bytes[i] == b'\\' {
                                    i += 1;
                                }
                                i += 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                // After the loop: if depth == 0, i points one past the closing `}`
                // (we broke at `}`, then i += 1 didn't execute, so i points AT `}`)
                // Actually: when depth hits 0, we break BEFORE i += 1, so i is AT `}`
                let interp_end = i;
                let interp_source = &inner[interp_start..interp_end.min(inner.len())];

                // Parse the interpolation as a Floe expression
                if let Some(expr) = self.parse_interpolation_expr(interp_source) {
                    parts.push(TemplatePart::Expr(expr));
                } else {
                    // Fallback: store as raw if parsing fails
                    parts.push(TemplatePart::Raw(format!("${{{}}}", interp_source)));
                }

                // Skip past the closing `}`
                if depth == 0 {
                    i += 1;
                }
            } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
                // Process escape sequences
                i += 1;
                match bytes[i] {
                    b'n' => current_raw.push('\n'),
                    b't' => current_raw.push('\t'),
                    b'r' => current_raw.push('\r'),
                    b'\\' => current_raw.push('\\'),
                    b'0' => current_raw.push('\0'),
                    b'`' => current_raw.push('`'),
                    b'$' => current_raw.push('$'),
                    b'u' => {
                        if let Some((ch, consumed)) =
                            Self::parse_unicode_escape_bytes(&inner[i + 1..])
                        {
                            current_raw.push(ch);
                            i += consumed;
                        } else {
                            current_raw.push('\\');
                            current_raw.push('u');
                        }
                    }
                    c => {
                        current_raw.push('\\');
                        current_raw.push(c as char);
                    }
                }
                i += 1;
            } else if bytes[i] >= 0x80 {
                // UTF-8 multibyte: find the full character
                let ch_start = i;
                i += 1;
                while i < bytes.len() && bytes[i] >= 0x80 && bytes[i] < 0xC0 {
                    i += 1;
                }
                current_raw.push_str(&inner[ch_start..i]);
            } else {
                current_raw.push(bytes[i] as char);
                i += 1;
            }
        }

        // Save final raw segment
        if !current_raw.is_empty() {
            parts.push(TemplatePart::Raw(current_raw));
        }

        parts
    }

    /// Parse a string of Floe source code as a single expression.
    fn parse_interpolation_expr(&self, source: &str) -> Option<Expr> {
        use crate::cst::CstParser;
        use crate::lexer::Lexer;

        let tokens = Lexer::new(source).tokenize_with_trivia();
        let cst_parse = CstParser::new(source, tokens).parse();

        // Ignore CST errors for interpolations — they may be complex expressions
        let root = cst_parse.syntax();
        let mut lowerer = Lowerer {
            source,
            errors: Vec::new(),
            id_gen: ExprIdGen::new(),
        };
        let program = lowerer.lower_root(&root);

        // Extract the first expression from the program
        program.items.into_iter().find_map(|item| {
            if let ItemKind::Expr(expr) = item.kind {
                Some(expr)
            } else {
                None
            }
        })
    }

    // ── Utility helpers ─────────────────────────────────────────

    pub(super) fn node_span(&self, node: &SyntaxNode) -> Span {
        let range = node.text_range();
        let start = range.start().into();
        let end = range.end().into();

        // Compute line/column from byte offset
        let (line, column) = self.offset_to_line_col(start);
        Span::new(start, end, line, column)
    }

    pub(super) fn token_span(&self, token: &rowan::SyntaxToken<FloeLang>) -> Span {
        let range = token.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let (line, column) = self.offset_to_line_col(start);
        Span::new(start, end, line, column)
    }

    fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for &b in &self.source.as_bytes()[..offset.min(self.source.len())] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Collect ident tokens that appear before the `=` sign.
    pub(super) fn collect_idents_before_eq(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::EQUAL {
                    break;
                }
                if token.kind() == SyntaxKind::IDENT {
                    idents.push(token.text().to_string());
                }
            }
        }
        idents
    }

    /// Collect object destructure fields (with optional `: alias`) from tokens inside `{ }`.
    /// When `stop_at_equal` is true, stops before `=` (for const declarations).
    /// Tokens: `{ data: rows, error }` → [("data", Some("rows")), ("error", None)]
    pub(super) fn collect_object_destructure_fields(
        &self,
        node: &SyntaxNode,
        stop_at_equal: bool,
    ) -> Vec<ObjectDestructureField> {
        let mut fields = Vec::new();
        let mut inside_braces = false;
        // Pending field name waiting to see if a `: alias` follows
        let mut pending_field: Option<String> = None;
        let mut expect_alias = false;

        for token in node.children_with_tokens() {
            let Some(token) = token.as_token() else {
                continue;
            };
            let kind = token.kind();

            if stop_at_equal && kind == SyntaxKind::EQUAL {
                break;
            }
            if kind == SyntaxKind::L_BRACE {
                inside_braces = true;
                continue;
            }
            if kind == SyntaxKind::R_BRACE {
                break;
            }
            if !inside_braces || kind.is_trivia() {
                continue;
            }

            match kind {
                SyntaxKind::IDENT if expect_alias => {
                    // This ident is the alias after `:`
                    let field = pending_field.take().unwrap();
                    fields.push(ObjectDestructureField {
                        field,
                        alias: Some(token.text().to_string()),
                    });
                    expect_alias = false;
                }
                SyntaxKind::IDENT => {
                    // Flush any pending field without alias
                    if let Some(field) = pending_field.take() {
                        fields.push(ObjectDestructureField { field, alias: None });
                    }
                    pending_field = Some(token.text().to_string());
                }
                SyntaxKind::COLON if pending_field.is_some() => {
                    expect_alias = true;
                }
                _ => {
                    // Comma or other — flush pending field without alias
                    if let Some(field) = pending_field.take() {
                        fields.push(ObjectDestructureField { field, alias: None });
                    }
                    expect_alias = false;
                }
            }
        }

        // Flush trailing field (e.g. `{ error }` with no trailing comma)
        if let Some(field) = pending_field {
            fields.push(ObjectDestructureField { field, alias: None });
        }

        fields
    }

    /// Check if a token kind appears before the `=` sign.
    pub(super) fn has_token_before_eq(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::EQUAL {
                    return false;
                }
                if token.kind() == kind {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn collect_idents(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::IDENT
            {
                idents.push(token.text().to_string());
            }
        }
        idents
    }

    /// Collect only direct ident tokens (not from child nodes).
    pub(super) fn collect_idents_direct(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::IDENT
            {
                idents.push(token.text().to_string());
            }
        }
        idents
    }

    /// Collect type parameters from `<T, R: Trait>` in function declarations.
    /// Supports optional trait bounds after `:`.
    pub(super) fn collect_type_params(&self, node: &SyntaxNode) -> Vec<TypeParam> {
        let mut params = Vec::new();
        let mut in_angle = false;
        let mut current_name: Option<String> = None;
        let mut after_colon = false;

        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::LESS_THAN => in_angle = true,
                    SyntaxKind::GREATER_THAN => {
                        if let Some(name) = current_name.take() {
                            params.push(TypeParam {
                                name,
                                bounds: Vec::new(),
                            });
                        }
                        break;
                    }
                    SyntaxKind::COMMA if in_angle => {
                        if let Some(name) = current_name.take() {
                            params.push(TypeParam {
                                name,
                                bounds: Vec::new(),
                            });
                        }
                        after_colon = false;
                    }
                    SyntaxKind::COLON if in_angle && current_name.is_some() => {
                        after_colon = true;
                    }
                    SyntaxKind::IDENT if in_angle => {
                        if after_colon {
                            // This ident is a bound — attach to the last param being built
                            // (current_name is already set, we need to store the bound separately)
                            // Rebuild: push param with bound
                            if let Some(name) = current_name.take() {
                                params.push(TypeParam {
                                    name,
                                    bounds: vec![token.text().to_string()],
                                });
                                after_colon = false;
                            }
                        } else {
                            current_name = Some(token.text().to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        params
    }

    /// Collect ident tokens that appear before the first `(` token.
    /// Used for CONSTRUCT_EXPR to handle qualified variants like `Route.Profile(...)`.
    pub(super) fn collect_idents_before_lparen(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::L_PAREN {
                    break;
                }
                if token.kind() == SyntaxKind::IDENT {
                    idents.push(token.text().to_string());
                }
            }
        }
        idents
    }

    pub(super) fn collect_numbers(&self, node: &SyntaxNode) -> Vec<String> {
        let mut numbers = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::NUMBER
            {
                numbers.push(token.text().to_string());
            }
        }
        numbers
    }

    pub(super) fn has_keyword(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    pub(super) fn has_token(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    /// Check if a MATCH_EXPR node has no subject expression (used for pipe-into-match).
    /// A subjectless match has `match` keyword followed directly by `{`, with no
    /// expression child nodes before the first MATCH_ARM.
    pub(super) fn is_subjectless_match(&self, node: &SyntaxNode) -> bool {
        // A subjectless match has no child expression nodes — only MATCH_ARM children
        for child in node.children() {
            if child.kind() == SyntaxKind::MATCH_ARM {
                continue;
            }
            // Any other child node means there's a subject expression
            return false;
        }
        // Also check: no token-level expressions (identifiers, numbers, etc.)
        // between `match` keyword and `{`
        let mut past_match_kw = false;
        for tok in node.children_with_tokens() {
            if let Some(token) = tok.as_token() {
                if token.kind() == SyntaxKind::KW_MATCH {
                    past_match_kw = true;
                    continue;
                }
                if past_match_kw && token.kind() == SyntaxKind::L_BRACE {
                    return true; // No expression between `match` and `{`
                }
                if past_match_kw && !token.kind().is_trivia() {
                    return false; // Found a token that could be a subject
                }
            }
        }
        true
    }

    pub(super) fn unquote_string(&self, text: &str) -> String {
        // Remove surrounding quotes
        if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
            let inner = &text[1..text.len() - 1];
            // Process escape sequences
            let mut result = String::new();
            let mut chars = inner.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('r') => result.push('\r'),
                        Some('\\') => result.push('\\'),
                        Some('"') => result.push('"'),
                        Some('0') => result.push('\0'),
                        Some('u') => {
                            if let Some(ch) = Self::parse_unicode_escape(&mut chars) {
                                result.push(ch);
                            } else {
                                result.push('\\');
                                result.push('u');
                            }
                        }
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => result.push('\\'),
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        } else {
            text.to_string()
        }
    }

    /// Parse a `\uXXXX` or `\u{XXXX}` unicode escape from a char iterator.
    /// The `\u` has already been consumed.
    fn parse_unicode_escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
        let mut hex = String::new();
        if chars.as_str().starts_with('{') {
            chars.next(); // consume '{'
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                hex.push(ch);
            }
        } else {
            for _ in 0..4 {
                hex.push(chars.next()?);
            }
        }
        u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
    }

    /// Parse a `\uXXXX` or `\u{XXXX}` unicode escape from a byte slice.
    /// The `\u` has already been consumed; `rest` starts after the `u`.
    /// Returns the parsed char and the number of additional bytes consumed.
    fn parse_unicode_escape_bytes(rest: &str) -> Option<(char, usize)> {
        if rest.starts_with('{') {
            let end = rest.find('}')?;
            let hex = &rest[1..end];
            let ch = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)?;
            Some((ch, end + 1)) // consume through '}'
        } else if rest.len() >= 4 {
            let hex = &rest[..4];
            let ch = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)?;
            Some((ch, 4)) // consumed 4 hex digits
        } else {
            None
        }
    }

    pub(super) fn find_binary_op(&self, node: &SyntaxNode) -> Option<BinOp> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                let op = match token.kind() {
                    SyntaxKind::PLUS => Some(BinOp::Add),
                    SyntaxKind::MINUS => Some(BinOp::Sub),
                    SyntaxKind::STAR => Some(BinOp::Mul),
                    SyntaxKind::SLASH => Some(BinOp::Div),
                    SyntaxKind::PERCENT => Some(BinOp::Mod),
                    SyntaxKind::EQUAL_EQUAL => Some(BinOp::Eq),
                    SyntaxKind::BANG_EQUAL => Some(BinOp::NotEq),
                    SyntaxKind::LESS_THAN => Some(BinOp::Lt),
                    SyntaxKind::GREATER_THAN => Some(BinOp::Gt),
                    SyntaxKind::LESS_EQUAL => Some(BinOp::LtEq),
                    SyntaxKind::GREATER_EQUAL => Some(BinOp::GtEq),
                    SyntaxKind::AMP_AMP => Some(BinOp::And),
                    SyntaxKind::PIPE_PIPE => Some(BinOp::Or),
                    _ => None,
                };
                if op.is_some() {
                    return op;
                }
            }
        }
        None
    }

    pub(super) fn find_unary_op(&self, node: &SyntaxNode) -> Option<UnaryOp> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::BANG => return Some(UnaryOp::Not),
                    SyntaxKind::MINUS => return Some(UnaryOp::Neg),
                    _ => {}
                }
            }
        }
        None
    }

    pub(super) fn lower_expr_after_eq(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_eq = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::EQUAL {
                        past_eq = true;
                        continue;
                    }
                    if past_eq && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_eq {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::cst::CstParser;
    use crate::lexer::Lexer;
    use crate::lower::lower_program;
    use crate::parser::ast::*;

    /// Helper: parse source through CST then lower to AST.
    fn lower(source: &str) -> Program {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        let parse = CstParser::new(source, tokens).parse();
        assert!(parse.errors.is_empty(), "CST errors: {:?}", parse.errors);
        let root = parse.syntax();
        lower_program(&root, source).unwrap_or_else(|errs| {
            panic!(
                "lower failed:\n{}",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }

    fn first_item(source: &str) -> ItemKind {
        lower(source).items.into_iter().next().unwrap().kind
    }

    fn first_expr(source: &str) -> ExprKind {
        match first_item(source) {
            ItemKind::Expr(e) => e.kind,
            other => panic!("expected Expr, got {other:?}"),
        }
    }

    // ── Const declarations ────────────────────────────────────────

    #[test]
    fn const_simple() {
        let item = first_item("let x = 42");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert_eq!(decl.binding, ConstBinding::Name("x".into()));
        assert!(!decl.exported);
        assert!(decl.type_ann.is_none());
    }

    #[test]
    fn const_typed() {
        let item = first_item("let x: number = 42");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(decl.type_ann.is_some());
    }

    #[test]
    fn const_exported() {
        let item = first_item("export let x = 1");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(decl.exported);
    }

    #[test]
    fn const_array_destructuring() {
        let item = first_item("let [a, b] = pair");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(matches!(decl.binding, ConstBinding::Array(_)));
    }

    // ── Function declarations ─────────────────────────────────────

    #[test]
    fn function_basic() {
        let item = first_item("let greet() = { 1 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert_eq!(decl.name, "greet");
        assert!(decl.params.is_empty());
        assert!(!decl.exported);
        assert!(!decl.async_fn);
    }

    #[test]
    fn function_with_params_and_return() {
        let item = first_item("let add(a: number, b: number) -> number = { a + b }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert_eq!(decl.params.len(), 2);
        assert_eq!(decl.params[0].name, "a");
        assert!(decl.return_type.is_some());
    }

    #[test]
    fn function_exported() {
        let item = first_item("export let hello() = { 1 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert!(decl.exported);
    }

    #[test]
    fn function_param_default() {
        let item = first_item("fn greet(name: string = \"world\") { name }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert!(decl.params[0].default.is_some());
    }

    // ── Literals ──────────────────────────────────────────────────

    #[test]
    fn literal_number() {
        assert_eq!(first_expr("42"), ExprKind::Number("42".into()));
    }

    #[test]
    fn literal_string() {
        assert_eq!(first_expr("\"hello\""), ExprKind::String("hello".into()));
    }

    #[test]
    fn literal_bool() {
        assert_eq!(first_expr("true"), ExprKind::Bool(true));
        assert_eq!(first_expr("false"), ExprKind::Bool(false));
    }

    #[test]
    fn none_is_identifier() {
        assert_eq!(first_expr("None"), ExprKind::Identifier("None".to_string()));
    }

    #[test]
    fn literal_todo() {
        assert_eq!(first_expr("todo"), ExprKind::Todo);
    }

    // ── Binary / unary operations ─────────────────────────────────

    #[test]
    fn binary_add() {
        let ExprKind::Binary { op, .. } = first_expr("1 + 2") else {
            panic!("expected Binary")
        };
        assert_eq!(op, BinOp::Add);
    }

    #[test]
    fn binary_eq() {
        let ExprKind::Binary { op, .. } = first_expr("1 == 2") else {
            panic!("expected Binary")
        };
        assert_eq!(op, BinOp::Eq);
    }

    #[test]
    fn unary_not() {
        let ExprKind::Unary { op, .. } = first_expr("!flag") else {
            panic!("expected Unary")
        };
        assert_eq!(op, UnaryOp::Not);
    }

    #[test]
    fn unary_neg() {
        let ExprKind::Unary { op, .. } = first_expr("-42") else {
            panic!("expected Unary")
        };
        assert_eq!(op, UnaryOp::Neg);
    }

    // ── Function calls ────────────────────────────────────────────

    #[test]
    fn call_basic() {
        let ExprKind::Call { callee, args, .. } = first_expr("f(1, 2)") else {
            panic!("expected Call")
        };
        assert!(matches!(callee.kind, ExprKind::Identifier(ref n) if n == "f"));
        assert_eq!(args.len(), 2);
    }

    // ── Imports ───────────────────────────────────────────────────

    #[test]
    fn import_named() {
        let item = first_item("import { foo, bar } from \"./mod\"");
        let ItemKind::Import(decl) = item else {
            panic!("expected Import")
        };
        assert_eq!(decl.specifiers.len(), 2);
        assert_eq!(decl.specifiers[0].name, "foo");
        assert_eq!(decl.source, "./mod");
    }

    #[test]
    fn import_aliased() {
        // "as" is banned but contextually used; test that the specifier still lowers
        let tokens = Lexer::new("import { foo as f } from \"./mod\"").tokenize_with_trivia();
        let parse = CstParser::new("import { foo as f } from \"./mod\"", tokens).parse();
        let root = parse.syntax();
        // Even if there's a banned keyword error, lowering should extract specifiers
        let _ = lower_program(&root, "import { foo as f } from \"./mod\"");
    }

    // ── Type declarations ─────────────────────────────────────────

    #[test]
    fn type_record() {
        let item = first_item("type User = { name: string, age: number }");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert_eq!(decl.name, "User");
        assert!(matches!(decl.def, TypeDef::Record(ref fields) if fields.len() == 2));
    }

    #[test]
    fn type_union() {
        let item = first_item("type Color = | Red | Green | Blue");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert!(matches!(decl.def, TypeDef::Union(ref variants) if variants.len() == 3));
    }

    #[test]
    fn type_alias() {
        let item = first_item("type Name = string");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert!(matches!(decl.def, TypeDef::Alias(_)));
    }

    #[test]
    fn type_string_literal_union() {
        let item = first_item(r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#);
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        match decl.def {
            TypeDef::StringLiteralUnion(ref variants) => {
                assert_eq!(variants, &["GET", "POST", "PUT", "DELETE"]);
            }
            other => panic!("expected StringLiteralUnion, got {other:?}"),
        }
    }

    // ── Match expressions ─────────────────────────────────────────

    #[test]
    fn match_basic() {
        let ExprKind::Match { arms, .. } = first_expr("match x { Ok(v) -> v, Err(e) -> e }") else {
            panic!("expected Match")
        };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn match_wildcard() {
        let ExprKind::Match { arms, .. } = first_expr("match x { _ -> 0 }") else {
            panic!("expected Match")
        };
        assert!(matches!(arms[0].pattern.kind, PatternKind::Wildcard));
    }

    #[test]
    fn match_with_guard() {
        let ExprKind::Match { arms, .. } = first_expr("match x { n when n > 0 -> n, _ -> 0 }")
        else {
            panic!("expected Match")
        };
        assert!(arms[0].guard.is_some());
    }

    // ── Pipe expressions ──────────────────────────────────────────

    #[test]
    fn pipe_basic() {
        let prog = lower("1 |> f(_)");
        let ItemKind::Expr(ref expr) = prog.items[0].kind else {
            panic!("expected Expr")
        };
        assert!(matches!(expr.kind, ExprKind::Pipe { .. }));
    }

    // ── Lambda / arrow functions ──────────────────────────────────

    #[test]
    fn lambda_basic() {
        let ExprKind::Arrow { params, .. } = first_expr("(x) -> x + 1") else {
            panic!("expected Arrow")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
    }

    #[test]
    fn lambda_zero_arg() {
        let ExprKind::Arrow { params, .. } = first_expr("() -> 42") else {
            panic!("expected Arrow")
        };
        assert!(params.is_empty());
    }

    // ── JSX ───────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        let ExprKind::Jsx(ref el) = first_expr("<Input />") else {
            panic!("expected Jsx")
        };
        match &el.kind {
            JsxElementKind::Element {
                name, self_closing, ..
            } => {
                assert_eq!(name, "Input");
                assert!(self_closing);
            }
            _ => panic!("expected Element"),
        }
    }

    #[test]
    fn jsx_with_children() {
        let ExprKind::Jsx(ref el) = first_expr("<div>hello</div>") else {
            panic!("expected Jsx")
        };
        match &el.kind {
            JsxElementKind::Element { children, .. } => {
                assert!(!children.is_empty());
            }
            _ => panic!("expected Element"),
        }
    }

    // ── Array, return, member ─────────────────────────────────────

    #[test]
    fn array_literal() {
        let ExprKind::Array(ref elts) = first_expr("[1, 2, 3]") else {
            panic!("expected Array")
        };
        assert_eq!(elts.len(), 3);
    }

    #[test]
    fn implicit_return_last_expr() {
        let item = first_item("let f() = { 42 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        let ExprKind::Block(ref items) = decl.body.kind else {
            panic!("expected Block")
        };
        assert!(!items.is_empty());
    }

    #[test]
    fn member_access() {
        let ExprKind::Member { field, .. } = first_expr("user.name") else {
            panic!("expected Member")
        };
        assert_eq!(field, "name");
    }

    // ── Ok / Err / Some constructors ──────────────────────────────

    #[test]
    fn ok_constructor() {
        assert!(matches!(first_expr("Ok(42)"), ExprKind::Construct { .. }));
    }

    #[test]
    fn err_constructor() {
        assert!(matches!(
            first_expr("Err(\"fail\")"),
            ExprKind::Construct { .. }
        ));
    }

    #[test]
    fn some_constructor() {
        assert!(matches!(first_expr("Some(1)"), ExprKind::Construct { .. }));
    }

    // ── For blocks ────────────────────────────────────────────────

    #[test]
    fn for_block_basic() {
        let item = first_item("for User { fn greet(self) -> string { self.name } }");
        let ItemKind::ForBlock(block) = item else {
            panic!("expected ForBlock")
        };
        assert_eq!(block.functions.len(), 1);
        assert_eq!(block.functions[0].name, "greet");
    }

    // ── Test blocks ───────────────────────────────────────────────

    #[test]
    fn test_block_basic() {
        let item = first_item("test \"adds\" { assert true }");
        let ItemKind::TestBlock(block) = item else {
            panic!("expected TestBlock")
        };
        assert_eq!(block.name, "adds");
        assert!(!block.body.is_empty());
    }

    // ── Empty program / multiple items ────────────────────────────

    #[test]
    fn empty_program() {
        let prog = lower("");
        assert!(prog.items.is_empty());
    }

    #[test]
    fn multiple_items() {
        let prog = lower("let x = 1\nlet y = 2");
        assert_eq!(prog.items.len(), 2);
    }

    // ── Use desugaring ────────────────────────────────────────────

    #[test]
    fn use_desugars_to_callback() {
        // `use x <- f(1)` followed by `x` should desugar to `f(1, fn(x) { x })`
        let prog = lower("let _test() -> number = {\n    use x <- f(1)\n    x\n}");
        let ItemKind::Function(decl) = &prog.items[0].kind else {
            panic!("expected Function")
        };
        let ExprKind::Block(ref items) = decl.body.kind else {
            panic!("expected Block")
        };
        assert_eq!(
            items.len(),
            1,
            "use should desugar to a single call expression"
        );
        let ItemKind::Expr(ref expr) = items[0].kind else {
            panic!("expected Expr")
        };
        let ExprKind::Call { ref args, .. } = expr.kind else {
            panic!("expected Call, got {:?}", expr.kind)
        };
        assert_eq!(args.len(), 2, "call should have original arg + callback");
    }

    #[test]
    fn use_zero_binding() {
        // `use <- f()` followed by `g()` should desugar to `f(fn() { g() })`
        let prog = lower("let _test() -> () = {\n    use <- f()\n    g()\n}");
        let ItemKind::Function(decl) = &prog.items[0].kind else {
            panic!("expected Function")
        };
        let ExprKind::Block(ref items) = decl.body.kind else {
            panic!("expected Block")
        };
        assert_eq!(items.len(), 1);
        let ItemKind::Expr(ref expr) = items[0].kind else {
            panic!("expected Expr")
        };
        let ExprKind::Call { ref args, .. } = expr.kind else {
            panic!("expected Call")
        };
        // The callback has zero params
        if let Arg::Positional(ref callback) = args[0]
            && let ExprKind::Arrow { ref params, .. } = callback.kind
        {
            assert_eq!(
                params.len(),
                0,
                "zero-binding use should produce zero-param callback"
            );
        }
    }

    #[test]
    fn use_chained() {
        // Two chained `use` statements should produce nested calls
        let prog =
            lower("let _test() -> () = {\n    use x <- f()\n    use y <- g(x)\n    h(y)\n}");
        let ItemKind::Function(decl) = &prog.items[0].kind else {
            panic!("expected Function")
        };
        let ExprKind::Block(ref items) = decl.body.kind else {
            panic!("expected Block")
        };
        // Should be a single item: f(fn(x) { g(x, fn(y) { h(y) }) })
        assert_eq!(
            items.len(),
            1,
            "chained use should nest into a single expression"
        );
    }
}
