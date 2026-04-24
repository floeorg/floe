mod exprs;
mod items;
mod jsx;
mod types;

use crate::lexer::span::Span;
use crate::lexer::token::{Token, TokenKind};
use crate::syntax::{SyntaxKind, SyntaxNode, token_kind_to_syntax};
use rowan::GreenNode;

/// Result of CST parsing.
pub struct Parse {
    pub green_node: GreenNode,
    pub errors: Vec<CstError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green_node.clone())
    }
}

#[derive(Debug, Clone)]
pub struct CstError {
    pub message: String,
    pub span: Span,
    pub kind: CstErrorKind,
}

/// What kind of CST error, tagged at creation time so downstream
/// classification doesn't substring-match on the message string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CstErrorKind {
    /// A banned keyword was used (e.g. `let`, `var`).
    BannedKeyword,
    /// An expected token was missing.
    UnexpectedToken,
    /// A JSX closing tag did not match the opening tag.
    MismatchedTag,
    /// Anything else.
    General,
}

/// CST parser: builds a lossless green tree from a token stream (including trivia).
pub struct CstParser<'src> {
    source: &'src str,
    tokens: Vec<Token>,
    pos: usize,
    builder: rowan::GreenNodeBuilder<'static>,
    errors: Vec<CstError>,
    /// When set, `(T, U) => V` stops being parsed as a function-type at the
    /// top level of a type expression. Enabled while parsing the return
    /// type of a `let NAME = (...): RET => body` binding so the outer `=>`
    /// belongs to the let-body arrow, not the return type.
    suppress_function_type: bool,
}

impl<'src> CstParser<'src> {
    pub fn new(source: &'src str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            builder: rowan::GreenNodeBuilder::new(),
            errors: Vec::new(),
            suppress_function_type: false,
        }
    }

    pub fn parse(mut self) -> Parse {
        self.builder.start_node(SyntaxKind::PROGRAM.into());
        self.eat_trivia();

        while !self.at_end() {
            let prev_pos = self.pos;
            self.parse_item();
            self.eat_trivia();
            if self.pos == prev_pos && !self.at_end() {
                // Safety: if parse_item made no progress, skip the stuck token
                // to prevent an infinite loop.
                self.bump();
            }
        }

        // Eat any remaining trivia and EOF
        self.eat_trivia();
        if self.at_end() {
            self.bump();
        }

        self.builder.finish_node();
        Parse {
            green_node: self.builder.finish(),
            errors: self.errors,
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn current_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.pos).map(|t| t.kind.clone())
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(self.source.len(), self.source.len(), 1, 1))
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current_kind()
            .is_some_and(|k| std::mem::discriminant(&k) == std::mem::discriminant(&kind))
    }

    fn at_identifier(&self, name: &str) -> bool {
        matches!(self.current_kind(), Some(TokenKind::Identifier(n)) if n == name)
    }

    /// True when `use` at the current position opens a bind statement rather
    /// than an identifier expression. Distinguishes `use <ident>? <-`,
    /// `use ( ... ) <-`, `use { ... } <-` from `use(promise)` (React's hook).
    fn is_use_bind_start(&self) -> bool {
        if !self.at_identifier("use") {
            return false;
        }
        let mut i = self.pos + 1;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        match self.tokens.get(i).map(|t| &t.kind) {
            Some(TokenKind::LeftArrow) => return true,
            Some(TokenKind::Identifier(_)) => i += 1,
            Some(TokenKind::LeftParen) => {
                i = self.skip_balanced(i + 1, |k| match k {
                    TokenKind::LeftParen => 1,
                    TokenKind::RightParen => -1,
                    _ => 0,
                });
            }
            Some(TokenKind::LeftBrace) => {
                i = self.skip_balanced(i + 1, |k| match k {
                    TokenKind::LeftBrace => 1,
                    TokenKind::RightBrace => -1,
                    _ => 0,
                });
            }
            _ => return false,
        }
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::LeftArrow)
        )
    }

    /// Scan forward from `start` (just past an opening bracket) until the
    /// matching close token balances to depth 0. `delta` maps a token kind to
    /// `+1` for open, `-1` for close, `0` otherwise. Returns the index after
    /// the matching close (or `tokens.len()` on unbalanced input).
    fn skip_balanced(&self, start: usize, delta: impl Fn(&TokenKind) -> i32) -> usize {
        let mut depth = 1_i32;
        let mut i = start;
        while i < self.tokens.len() && depth > 0 {
            depth += delta(&self.tokens[i].kind);
            i += 1;
        }
        i
    }

    fn peek_is_string(&self) -> bool {
        // Look ahead past trivia to find the next non-trivia token
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            let kind = &self.tokens[i].kind;
            if matches!(
                kind,
                TokenKind::Whitespace | TokenKind::Comment | TokenKind::BlockComment
            ) {
                i += 1;
                continue;
            }
            return matches!(kind, TokenKind::String(_));
        }
        false
    }

    fn at_pipe_in_union(&self) -> bool {
        self.at(TokenKind::VerticalBar)
    }

    /// Check if we're at a string literal union: `"A" | "B" | ...`
    /// This is true when the current token is a string and the next non-trivia token is `|`.
    fn at_string_literal_union(&self) -> bool {
        self.at(TokenKind::String("".into()))
            && matches!(
                self.peek_nth_non_trivia_kind(1),
                Some(TokenKind::VerticalBar)
            )
    }

    fn is_ident(&self) -> bool {
        matches!(
            self.current_kind(),
            Some(TokenKind::Identifier(_) | TokenKind::Parse)
        )
    }

    /// Check if the current token is a keyword that could appear as a JSX prop name
    /// (e.g., `type`, `for`, `match`, `fn`, `const`, etc.).
    fn is_keyword(&self) -> bool {
        matches!(
            self.current_kind(),
            Some(
                TokenKind::Type
                    | TokenKind::For
                    | TokenKind::Match
                    | TokenKind::Fn
                    | TokenKind::Let
                    | TokenKind::Import
                    | TokenKind::Export
                    | TokenKind::Trait
            )
        )
    }

    /// Check if the current token maps to a SyntaxKind that is a valid member
    /// name (identifiers, keywords, numbers, etc.). Delegates to
    /// `SyntaxKind::is_member_name` via `token_kind_to_syntax`.
    fn is_member_name_token(&self) -> bool {
        self.current_kind()
            .is_some_and(|kind| crate::syntax::token_kind_to_syntax(&kind).is_member_name())
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.at(TokenKind::Eof)
    }

    /// Check if the previous trivia token contains a newline.
    /// Used to prevent `<` on a new line from being parsed as comparison.
    fn preceded_by_newline(&self) -> bool {
        if self.pos == 0 {
            return false;
        }
        // Look at the previous token(s) — if we see a whitespace token with \n, it's a newline
        let mut i = self.pos - 1;
        loop {
            if self.tokens[i].kind.is_trivia() {
                if let TokenKind::Whitespace = &self.tokens[i].kind {
                    let text = &self.tokens[i].span;
                    // Check if the whitespace span contains a newline
                    let ws_text = &self.source[text.start..text.end];
                    if ws_text.contains('\n') {
                        return true;
                    }
                }
                if i == 0 {
                    break;
                }
                i -= 1;
            } else {
                break;
            }
        }
        false
    }

    /// Check if the current `<` starts a generic call: `f<Type>(...)`.
    /// Looks ahead for balanced `<>` followed by `(`.
    fn is_generic_call(&self) -> bool {
        let mut depth = 0;
        let mut brace_depth = 0;
        let mut i = self.pos; // at `<`
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                // Inline object-type literals (e.g. `foo<{ k: T }>()`) are valid
                // type arguments. Track brace nesting so `<` / `>` inside braces
                // (nested generics like `foo<{ k: Map<K, V> }>()`) don't shift
                // the outer angle counter.
                TokenKind::LeftBrace => brace_depth += 1,
                TokenKind::RightBrace => {
                    if brace_depth == 0 {
                        return false;
                    }
                    brace_depth -= 1;
                }
                TokenKind::LessThan if brace_depth == 0 => depth += 1,
                TokenKind::GreaterThan if brace_depth == 0 => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if the next non-trivia token is `(`
                        i += 1;
                        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
                            i += 1;
                        }
                        return i < self.tokens.len()
                            && self.tokens[i].kind == TokenKind::LeftParen;
                    }
                }
                // Outside of braces, these tokens end any plausible type-arg list.
                TokenKind::Semicolon | TokenKind::Equal if brace_depth == 0 => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn peek_is_ident(&self) -> bool {
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return matches!(self.tokens[i].kind, TokenKind::Identifier(_));
            }
            i += 1;
        }
        false
    }

    fn peek_is(&self, kind: TokenKind) -> bool {
        // Skip trivia to find the next non-trivia token
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return std::mem::discriminant(&self.tokens[i].kind)
                    == std::mem::discriminant(&kind);
            }
            i += 1;
        }
        false
    }

    /// Get the nth non-trivia token kind after the current position (1-indexed).
    fn peek_nth_non_trivia_kind(&self, n: usize) -> Option<TokenKind> {
        let mut count = 0;
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                count += 1;
                if count == n {
                    return Some(self.tokens[i].kind.clone());
                }
            }
            i += 1;
        }
        None
    }

    fn next_non_trivia_kind(&self) -> Option<TokenKind> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return Some(self.tokens[i].kind.clone());
            }
            i += 1;
        }
        None
    }

    fn is_jsx_text_token(&self) -> bool {
        // In JSX children, almost everything is text EXCEPT:
        // - `<` starts a child element or closing tag
        // - `{` starts an expression
        // - `}` ends a parent expression (shouldn't happen in children)
        // - EOF
        !matches!(
            self.current_kind(),
            Some(TokenKind::LessThan)
                | Some(TokenKind::LeftBrace)
                | Some(TokenKind::RightBrace)
                | Some(TokenKind::Eof)
                | None
        )
    }

    fn is_uppercase_ident_at_checkpoint(&self) -> bool {
        // Walk backward through previously emitted tokens to find the last non-trivia
        // In practice, we need to check the expression that was just parsed.
        // The simplest heuristic: check if the previous non-trivia token was an uppercase ident.
        let mut i = self.pos.saturating_sub(1);
        loop {
            if i < self.tokens.len() && !self.tokens[i].kind.is_trivia() {
                return matches!(&self.tokens[i].kind, TokenKind::Identifier(name) if name.starts_with(char::is_uppercase));
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        false
    }

    /// Heuristic: is the current `(` a tuple type `(T, U)`?
    /// Has a comma at depth 1 and is NOT followed by `->`.
    fn is_paren_tuple_type(&self) -> bool {
        let mut depth = 0;
        let mut has_comma = false;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        if !has_comma {
                            return false;
                        }
                        // Find next non-trivia
                        let mut j = i + 1;
                        while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                            j += 1;
                        }
                        return !(j < self.tokens.len()
                            && self.tokens[j].kind == TokenKind::ThinArrow);
                    }
                }
                TokenKind::Comma if depth == 1 => has_comma = true,
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` in `const (a, b) = ...` a tuple destructuring?
    /// Check that `)` is followed by `=` or `:`.
    fn is_const_tuple_destructuring(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Find next non-trivia
                        let mut j = i + 1;
                        while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                            j += 1;
                        }
                        return j < self.tokens.len()
                            && matches!(self.tokens[j].kind, TokenKind::Equal | TokenKind::Colon);
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` the start of a function type `(T) -> U`?
    fn is_paren_function_type(&self) -> bool {
        if self.suppress_function_type {
            return false;
        }
        self.is_paren_followed_by(TokenKind::ThinArrow)
    }

    /// Heuristic: is the current `(` the start of an arrow closure
    /// `(params) -> body`?
    fn is_arrow_expr(&self) -> bool {
        if self.suppress_function_type {
            return false;
        }
        self.is_paren_followed_by(TokenKind::ThinArrow)
    }

    /// Check if the `(` at position `start` has a matching `)` followed by `kind`.
    fn is_paren_followed_by_at(&self, start: usize, kind: TokenKind) -> bool {
        let mut depth = 0;
        let mut i = start;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Found matching `)` — check next non-trivia token
                        i += 1;
                        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
                            i += 1;
                        }
                        return i < self.tokens.len() && self.tokens[i].kind == kind;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Check if the `(` at the current position has a matching `)` followed by `kind`.
    fn is_paren_followed_by(&self, kind: TokenKind) -> bool {
        self.is_paren_followed_by_at(self.pos, kind)
    }

    /// Heuristic: is the current `(` a tuple expression `(a, b)`?
    /// Scans to matching `)` and checks if there's a comma at depth 1.
    fn is_paren_tuple_expr(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return false; // no comma found
                    }
                }
                TokenKind::Comma if depth == 1 => return true,
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Consume the current token, adding it to the green tree.
    fn bump(&mut self) {
        if self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];
            let syntax_kind = token_kind_to_syntax(&token.kind);
            let text = &self.source[token.span.start..token.span.end];
            self.builder.token(syntax_kind.into(), text);
            self.pos += 1;
        }
    }

    /// Consume the current token, recording it in the green tree as
    /// `syntax_kind` regardless of its lexer kind. Used for contextual
    /// keywords like `use` that lex as identifiers.
    fn bump_remap(&mut self, syntax_kind: SyntaxKind) {
        debug_assert!(self.pos < self.tokens.len(), "bump_remap past EOF");
        let token = &self.tokens[self.pos];
        let text = &self.source[token.span.start..token.span.end];
        self.builder.token(syntax_kind.into(), text);
        self.pos += 1;
    }

    /// Consume trivia tokens (whitespace, comments).
    fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            self.bump();
        }
    }

    fn expect(&mut self, kind: TokenKind) {
        if self.at(kind.clone()) {
            self.bump();
        } else {
            self.error_kind(
                &format!("expected {:?}, found {:?}", kind, self.current_kind()),
                CstErrorKind::UnexpectedToken,
            );
        }
    }

    fn expect_kind(&mut self, kind: TokenKind) {
        if self.at(kind.clone()) {
            self.bump();
        } else {
            self.error_kind(
                &format!("expected {:?}, found {:?}", kind, self.current_kind()),
                CstErrorKind::UnexpectedToken,
            );
        }
    }

    fn expect_ident(&mut self) {
        if self.is_ident() {
            self.bump();
        } else {
            self.error_kind(
                &format!("expected identifier, found {:?}", self.current_kind()),
                CstErrorKind::UnexpectedToken,
            );
        }
    }

    fn expect_ident_item(&mut self) {
        self.expect_ident();
    }

    /// Parse a type parameter: `T` or `T: Trait` (with trait bound).
    fn parse_type_param(&mut self) {
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump(); // :
            self.eat_trivia();
            self.expect_ident(); // trait name
            self.eat_trivia();
        }
    }

    /// Parse a destructuring field: `ident` or `ident: ident` (with rename).
    fn parse_destructure_field(&mut self) {
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump(); // eat ':'
            self.eat_trivia();
            self.expect_ident(); // alias
        }
    }

    fn error(&mut self, message: &str) {
        self.error_kind(message, CstErrorKind::General);
    }

    fn error_kind(&mut self, message: &str, kind: CstErrorKind) {
        self.errors.push(CstError {
            message: message.to_string(),
            span: self.current_span(),
            kind,
        });
    }

    fn parse_comma_separated(&mut self, parse_fn: fn(&mut Self), closing: TokenKind) {
        if self.at(closing.clone()) {
            return;
        }

        parse_fn(self);
        self.eat_trivia();

        while self.at(TokenKind::Comma) {
            self.bump();
            self.eat_trivia();
            if self.at(closing.clone()) {
                break;
            }
            parse_fn(self);
            self.eat_trivia();
        }
    }
}

impl TokenKind {
    fn is_trivia(&self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::BlockComment
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::syntax::SyntaxKind;

    /// Helper: parse source through CstParser and return the Parse result.
    fn cst_parse(source: &str) -> Parse {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        CstParser::new(source, tokens).parse()
    }

    /// Helper: assert the CST text round-trips exactly.
    fn assert_lossless(source: &str) {
        let parse = cst_parse(source);
        assert_eq!(
            parse.syntax().text().to_string(),
            source,
            "CST text should match original source"
        );
    }

    /// Helper: assert no CST errors.
    fn assert_no_errors(source: &str) -> Parse {
        let parse = cst_parse(source);
        assert!(
            parse.errors.is_empty(),
            "unexpected CST errors: {:?}",
            parse.errors
        );
        parse
    }

    // ── Const declarations ────────────────────────────────────────

    #[test]
    fn const_simple() {
        assert_no_errors("let x = 42");
    }

    #[test]
    fn const_typed() {
        assert_no_errors("let x: number = 42");
    }

    #[test]
    fn const_exported() {
        assert_no_errors("export let name = \"hello\"");
    }

    #[test]
    fn const_string_value() {
        assert_no_errors("let greeting = \"world\"");
    }

    #[test]
    fn const_bool_value() {
        assert_no_errors("let flag = true");
    }

    // ── Function declarations ─────────────────────────────────────

    #[test]
    fn function_no_params() {
        assert_no_errors("let greet() = { 42 }");
    }

    #[test]
    fn function_with_params() {
        assert_no_errors("let add(a: number, b: number) -> number = { a + b }");
    }

    #[test]
    fn function_with_promise_return() {
        assert_no_errors("let fetch(url: string) -> Promise<string> = { url }");
    }

    #[test]
    fn function_exported() {
        assert_no_errors("export let hello() = { 1 }");
    }

    // ── Imports ───────────────────────────────────────────────────

    #[test]
    fn import_bare() {
        assert_no_errors("import \"./module\"");
    }

    #[test]
    fn import_with_specifiers() {
        assert_no_errors("import { foo, bar } from \"./module\"");
    }

    #[test]
    fn import_aliased() {
        // "as" is a banned keyword but allowed contextually in imports
        let parse = cst_parse("import { foo as f } from \"./module\"");
        // Should have at most an error for "as" being banned, but still parses
        let text = parse.syntax().text().to_string();
        assert_eq!(text, "import { foo as f } from \"./module\"");
    }

    // ── Exports ───────────────────────────────────────────────────

    #[test]
    fn export_function() {
        assert_no_errors("export let myFunc() = { 1 }");
    }

    #[test]
    fn export_type() {
        assert_no_errors("export type Color = | Red | Green | Blue");
    }

    // ── Type declarations ─────────────────────────────────────────

    #[test]
    fn type_record() {
        assert_no_errors("type User = { name: string, age: number }");
    }

    #[test]
    fn type_union() {
        assert_no_errors("type Color = | Red | Green | Blue");
    }

    #[test]
    fn type_string_literal_union() {
        assert_no_errors(r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#);
    }

    #[test]
    fn type_string_literal_union_two() {
        assert_no_errors(r#"type Status = "ok" | "error""#);
    }

    #[test]
    fn type_string_literal_union_rejected_in_braces() {
        // String literal unions are only valid in = type aliases (TS interop).
        // They must not be accepted inside { } type definitions.
        let parse = cst_parse(r#"type Method = | "GET" | "POST""#);
        assert!(
            !parse.errors.is_empty(),
            "string literal union in {{ }} should produce parse errors"
        );
    }

    #[test]
    fn type_alias() {
        assert_no_errors("typealias Name = string");
    }

    #[test]
    fn type_opaque() {
        assert_no_errors("opaque type Id = Id(string)");
    }

    #[test]
    fn type_generic() {
        assert_no_errors("type Box<T> = { value: T }");
    }

    #[test]
    fn type_exported() {
        assert_no_errors("export type Point = { x: number, y: number }");
    }

    // ── Expressions ───────────────────────────────────────────────

    #[test]
    fn binary_add() {
        assert_no_errors("1 + 2");
    }

    #[test]
    fn binary_comparison() {
        assert_no_errors("a == b");
    }

    #[test]
    fn unary_not() {
        assert_no_errors("!flag");
    }

    #[test]
    fn unary_neg() {
        assert_no_errors("-42");
    }

    #[test]
    fn call_expr() {
        assert_no_errors("f(a, b)");
    }

    #[test]
    fn member_access() {
        assert_no_errors("user.name");
    }

    #[test]
    fn constructor_simple() {
        assert_no_errors("User(name: \"Alice\")");
    }

    #[test]
    fn ok_expr() {
        assert_no_errors("Ok(42)");
    }

    #[test]
    fn err_expr() {
        assert_no_errors("Err(\"fail\")");
    }

    #[test]
    fn some_expr() {
        assert_no_errors("Some(1)");
    }

    #[test]
    fn none_expr() {
        assert_no_errors("None");
    }

    #[test]
    fn return_is_banned() {
        // `return` should produce a banned keyword error
        let parse = cst_parse("let f = () => { return 42 }");
        assert!(
            parse.errors.iter().any(|e| e.message.contains("banned")),
            "expected banned keyword error for return, got: {:?}",
            parse.errors
        );
    }

    #[test]
    fn array_literal() {
        assert_no_errors("[1, 2, 3]");
    }

    #[test]
    fn tuple_literal() {
        assert_no_errors("(1, 2)");
    }

    // ── Pipe expressions ──────────────────────────────────────────

    #[test]
    fn pipe_simple() {
        assert_no_errors("x |> f(y, _)");
    }

    #[test]
    fn pipe_chain() {
        assert_no_errors("data |> filter(.done) |> map(.name)");
    }

    // ── Match expressions ─────────────────────────────────────────

    #[test]
    fn match_basic() {
        assert_no_errors("match x { Ok(v) -> v, Err(e) -> e }");
    }

    #[test]
    fn match_wildcard() {
        assert_no_errors("match x { _ -> 0 }");
    }

    #[test]
    fn match_guard() {
        assert_no_errors("match x { n when n > 0 -> n, _ -> 0 }");
    }

    #[test]
    fn match_negative_number_pattern() {
        assert_no_errors("match x { -1 -> \"neg\", 0 -> \"zero\", _ -> \"pos\" }");
    }

    #[test]
    fn match_qualified_variant_pattern() {
        assert_no_errors("match s { Status.Active -> 1, Status.Inactive -> 0 }");
    }

    #[test]
    fn match_qualified_variant_with_payload() {
        assert_no_errors("match s { Shape.Circle(r) -> r, Shape.Rect(w, h) -> w }");
    }

    // ── JSX ───────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        assert_no_errors("<Input />");
    }

    #[test]
    fn jsx_with_children() {
        assert_no_errors("<div>hello</div>");
    }

    #[test]
    fn jsx_with_props() {
        assert_no_errors("<Button onClick={handler} />");
    }

    #[test]
    fn jsx_comment() {
        assert_no_errors("<div>{/* comment */}</div>");
    }

    #[test]
    fn jsx_comment_among_children() {
        assert_no_errors("<div>{/* comment */}<span>hello</span></div>");
    }

    #[test]
    fn lossless_jsx_comment() {
        assert_lossless("<div>{/* comment */}</div>");
    }

    // ── Lambda / arrow functions ──────────────────────────────────

    #[test]
    fn lambda_arrow_style() {
        assert_no_errors("(x) -> x + 1");
    }

    #[test]
    fn lambda_zero_arg() {
        assert_no_errors("() -> 42");
    }

    #[test]
    fn let_with_partial_application() {
        assert_no_errors(
            "let add(a: number, b: number) -> number = { a + b }\nlet inc = add(1, _)",
        );
    }

    // ── For blocks ────────────────────────────────────────────────

    #[test]
    fn for_block_basic() {
        assert_no_errors("for User { let greet(self) -> string = { self.name } }");
    }

    #[test]
    fn impl_block() {
        assert_no_errors("impl Display for User { let show(self) -> string = { self.name } }");
    }

    #[test]
    fn impl_block_empty_body() {
        assert_no_errors("impl Eq for User");
    }

    // ── Trait declarations ────────────────────────────────────────

    #[test]
    fn trait_basic() {
        assert_no_errors("trait Display { let show(self) -> string }");
    }

    // ── Test blocks ───────────────────────────────────────────────

    #[test]
    fn test_block_basic() {
        assert_no_errors("test \"my test\" { assert 1 == 1 }");
    }

    // ── Trivia preservation ───────────────────────────────────────

    #[test]
    fn trivia_comments_preserved() {
        assert_lossless("// comment\nconst x = 1");
    }

    #[test]
    fn trivia_whitespace_preserved() {
        assert_lossless("let  x  =  1");
    }

    #[test]
    fn trivia_block_comment_preserved() {
        assert_lossless("/* block */ let x = 1");
    }

    // ── Error recovery ────────────────────────────────────────────

    #[test]
    fn error_recovery_missing_equal() {
        // Should not panic, produces CST errors
        let parse = cst_parse("let x 42");
        assert!(!parse.errors.is_empty());
    }

    #[test]
    fn error_recovery_malformed_function() {
        // `fn` followed by something that's neither an identifier (declaration) nor `(` (lambda)
        let parse = cst_parse("fn { }");
        assert!(!parse.errors.is_empty());
    }

    #[test]
    fn error_recovery_empty_input() {
        let parse = cst_parse("");
        assert!(parse.errors.is_empty());
        assert_lossless("");
    }

    #[test]
    fn error_recovery_random_tokens() {
        // Should not panic regardless of input
        let _ = cst_parse("!@#$%^");
        let _ = cst_parse("}{)(][");
        let _ = cst_parse(";;; , , ,");
    }

    // ── Lossless round-trips ──────────────────────────────────────

    #[test]
    fn lossless_const() {
        assert_lossless("let x = 42");
    }

    #[test]
    fn lossless_function() {
        assert_lossless("let add(a: number, b: number) -> number = { a + b }");
    }

    #[test]
    fn lossless_import() {
        assert_lossless("import { foo, bar } from \"./module\"");
    }

    #[test]
    fn lossless_match() {
        assert_lossless("match x { Ok(v) -> v, _ -> 0 }");
    }

    #[test]
    fn lossless_jsx() {
        assert_lossless("<div>hello</div>");
    }

    #[test]
    fn lossless_pipe() {
        assert_lossless("x |> f(y, _)");
    }

    #[test]
    fn lossless_for_block() {
        assert_lossless("for User { fn greet(self) -> string { self.name } }");
    }

    // ── CST node kind checks ──────────────────────────────────────

    #[test]
    fn root_is_program() {
        let parse = cst_parse("let x = 1");
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
    }

    #[test]
    fn has_item_children() {
        let parse = cst_parse("let x = 1\nlet y = 2");
        let items: Vec<_> = parse
            .syntax()
            .children()
            .filter(|c| c.kind() == SyntaxKind::ITEM)
            .collect();
        assert_eq!(items.len(), 2);
    }
}
