mod exprs;
mod items;
mod jsx;
mod types;

use crate::lexer::span::Span;
use crate::lexer::token::{IdentRole, Token, TokenKind};
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
    /// A word that JavaScript reserves was used where a value name belongs.
    ReservedWord,
    /// An expected token was missing.
    UnexpectedToken,
    /// A JSX closing tag did not match the opening tag.
    MismatchedTag,
    /// Text that cannot name anything stood where a name belongs.
    InvalidName,
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
    /// When set, `Foo { ... }` is NOT parsed as a brace-form record
    /// construction. Used inside the subject of `match` so the trailing
    /// `{ arms }` is the match block, not part of the subject expression.
    no_struct_literal: bool,
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
            no_struct_literal: false,
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

    fn at(&self, kind: &TokenKind) -> bool {
        self.current_kind()
            .is_some_and(|k| std::mem::discriminant(&k) == std::mem::discriminant(kind))
    }

    /// `at` for a set of alternatives. Cleaner than chained `at(...) || at(...)`
    /// when matching multi-token operator classes (binary operators, keyword
    /// alternatives, etc.).
    fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.iter().any(|k| self.at(k))
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
        self.at(&TokenKind::VerticalBar)
    }

    /// Check if we're at a string literal union: `"A" | "B" | ...`
    /// This is true when the current token is a string and the next non-trivia token is `|`.
    fn at_string_literal_union(&self) -> bool {
        self.at(&TokenKind::String(String::new()))
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

    /// True when the current word may name a property: a record field, an
    /// object-literal key, a member, a named argument label, or a JSX
    /// attribute.
    fn at_property_name(&self) -> bool {
        self.current_kind()
            .as_ref()
            .is_some_and(TokenKind::can_name_property)
    }

    /// The source text of the current token. Tied to the source lifetime, so
    /// a caller may hold it across a `&mut self` call.
    fn current_text(&self) -> &'src str {
        match self.tokens.get(self.pos) {
            Some(token) => &self.source[token.span.start..token.span.end],
            None => "",
        }
    }

    /// The spelling of the current word when that word cannot name a value,
    /// paired with why. `None` for an identifier and for a non-word token.
    fn word_that_cannot_bind(&self) -> Option<(&'src str, &'static str)> {
        match self.current_kind()?.ident_role()? {
            IdentRole::Binding => None,
            IdentRole::PropertyOnly => {
                Some((self.current_text(), "is a reserved word in JavaScript"))
            }
            IdentRole::Keyword => Some((self.current_text(), "is a keyword in Floe")),
        }
    }

    /// True when the current token may follow a `.`.
    fn at_member_name(&self) -> bool {
        self.current_kind()
            .as_ref()
            .is_some_and(TokenKind::can_name_member)
    }

    /// Expect a name that binds a value: a `let` binding, a parameter, a
    /// destructured name, or a shorthand that reads the value back.
    ///
    /// A word that JavaScript reserves is rejected here, because the emitted
    /// TypeScript would not compile. The diagnostic names the word.
    fn expect_binding_name(&mut self) {
        if let Some(text) = self.text_that_cannot_name() {
            self.error_not_a_name(&text);

            return;
        }

        let kind = self.current_kind();

        if matches!(kind, Some(TokenKind::Identifier(_))) {
            self.bump();
            return;
        }

        if kind.as_ref().is_some_and(TokenKind::can_bind) {
            self.bump_remap(SyntaxKind::IDENT);
            return;
        }

        if let Some((word, why)) = self.word_that_cannot_bind() {
            self.error_kind(
                &format!(
                    "`{word}` {why}, so it cannot name a value. Rename the value. \
                     Floe accepts `{word}` as a field name, a member name, a named argument and a JSX attribute."
                ),
                CstErrorKind::ReservedWord,
            );
            self.bump();
            return;
        }

        self.error_kind(
            &format!("expected identifier, found {:?}", self.current_kind()),
            CstErrorKind::UnexpectedToken,
        );
    }

    fn expect_binding_name_item(&mut self) {
        self.expect_binding_name();
    }

    /// Expect the name in a shorthand field: `{ name }`, `Foo { name }` and
    /// `Foo { name: }` all read back as `name: name`. The name is a property,
    /// and the shorthand also reads a value of that name, so a word that
    /// cannot name a value is rejected with the punning advice.
    fn expect_shorthand_name(&mut self) {
        if let Some((word, why)) = self.word_that_cannot_bind() {
            self.error_reserved_pun(word, why);
            self.bump_remap(SyntaxKind::IDENT);
            return;
        }

        self.expect_binding_name();
    }

    /// Expect a name that names a property: a record field, an object-literal
    /// key, a member, a named argument label, or a JSX attribute. Every word
    /// stands here, for the reason on `IdentRole`.
    fn expect_property_name(&mut self) {
        if let Some(text) = self.text_that_cannot_name() {
            self.error_not_a_name(&text);

            return;
        }

        match self.current_kind() {
            Some(TokenKind::Identifier(_)) => self.bump(),
            Some(k) if k.can_name_property() => self.bump_remap(SyntaxKind::IDENT),
            _ => self.error_kind(
                &format!("expected a field name, found {:?}", self.current_kind()),
                CstErrorKind::UnexpectedToken,
            ),
        }
    }

    /// The current token when it holds non-ASCII text that cannot name
    /// anything: an emoji, a symbol or punctuation.
    fn text_that_cannot_name(&self) -> Option<String> {
        match self.current_kind() {
            Some(TokenKind::UnicodeText(text)) => Some(text),
            _ => None,
        }
    }

    /// Write the diagnostic for text that cannot name anything, then step
    /// over it so one bad character does not cascade.
    ///
    /// Floe emits TypeScript, so a Floe name follows the TypeScript rule.
    fn error_not_a_name(&mut self, text: &str) {
        self.error_kind(
            &format!(
                "`{text}` cannot name anything. A Floe name starts with a Unicode letter, \
                 `$` or `_`, and continues with a letter, a digit, `$` or `_`. \
                 An emoji is not a letter, and TypeScript rejects it in a name as well."
            ),
            CstErrorKind::InvalidName,
        );
        self.bump();
    }

    /// Write the diagnostic for a punned field (`{ for }` and `Foo { for: }`
    /// both read back as `for: for`). The name is a property, but the pun
    /// also reads a value of the same name, and this word can never name one.
    fn error_reserved_pun(&mut self, word: &str, why: &str) {
        self.error_kind(
            &format!(
                "`{word}` {why}, so it cannot name a value. \
                 Write the field out as `{word}: ...` instead of punning it."
            ),
            CstErrorKind::ReservedWord,
        );
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.at(&TokenKind::Eof)
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

    fn peek_is(&self, kind: &TokenKind) -> bool {
        // Skip trivia to find the next non-trivia token
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return std::mem::discriminant(&self.tokens[i].kind)
                    == std::mem::discriminant(kind);
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
            Some(
                TokenKind::LessThan | TokenKind::LeftBrace | TokenKind::RightBrace | TokenKind::Eof
            ) | None
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
        self.is_paren_followed_by(&TokenKind::ThinArrow)
    }

    /// Heuristic: is the current `(` the start of an arrow closure
    /// `(params) -> body`?
    fn is_arrow_expr(&self) -> bool {
        if self.suppress_function_type {
            return false;
        }
        self.is_paren_followed_by(&TokenKind::ThinArrow)
    }

    /// Check if the `(` at position `start` has a matching `)` followed by `kind`.
    fn is_paren_followed_by_at(&self, start: usize, kind: &TokenKind) -> bool {
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
                        return i < self.tokens.len() && self.tokens[i].kind == *kind;
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
    fn is_paren_followed_by(&self, kind: &TokenKind) -> bool {
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

    fn expect(&mut self, kind: &TokenKind) {
        if self.at(kind) {
            self.bump();
        } else {
            self.error_kind(
                &format!("expected {:?}, found {:?}", kind, self.current_kind()),
                CstErrorKind::UnexpectedToken,
            );
        }
    }

    fn expect_ident(&mut self) {
        if let Some(text) = self.text_that_cannot_name() {
            self.error_not_a_name(&text);

            return;
        }

        match self.current_kind() {
            Some(TokenKind::Identifier(_)) => self.bump(),
            Some(TokenKind::Parse) => self.bump_remap(SyntaxKind::IDENT),
            _ => self.error_kind(
                &format!("expected identifier, found {:?}", self.current_kind()),
                CstErrorKind::UnexpectedToken,
            ),
        }
    }

    fn expect_ident_item(&mut self) {
        self.expect_ident();
    }

    /// Parse a type parameter: `T` or `T: Trait` (with trait bound).
    fn parse_type_param(&mut self) {
        self.expect_ident();
        self.eat_trivia();
        if self.at(&TokenKind::Colon) {
            self.bump(); // :
            self.eat_trivia();
            self.expect_ident(); // trait name
            self.eat_trivia();
        }
    }

    /// Parse a destructuring field: `name` or `field: name` (with rename).
    /// `field` reads a property, so a reserved word is fine there. `name`
    /// binds, so a reserved word is rejected.
    fn parse_destructure_field(&mut self) {
        if self.peek_is(&TokenKind::Colon) {
            self.expect_property_name();
            self.eat_trivia();
            self.bump(); // eat ':'
            self.eat_trivia();
            self.expect_binding_name(); // alias
            return;
        }

        // `{ name }` binds `name` and reads the property of the same name.
        self.expect_shorthand_name();
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

    fn parse_comma_separated(&mut self, parse_fn: fn(&mut Self), closing: &TokenKind) {
        if self.at(closing) {
            return;
        }

        parse_fn(self);
        self.eat_trivia();

        while self.at(&TokenKind::Comma) {
            self.bump();
            self.eat_trivia();
            if self.at(closing) {
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

    // ── Reserved words as names ───────────────────────────────────

    /// Assert the parse reports exactly one error, that error names the
    /// word, and it carries the given advice.
    fn assert_one_reserved_word_error_with(source: &str, word: &str, advice: &str) {
        let parse = cst_parse(source);
        let reserved: Vec<_> = parse
            .errors
            .iter()
            .filter(|e| e.kind == CstErrorKind::ReservedWord)
            .collect();
        assert_eq!(
            reserved.len(),
            1,
            "expected one reserved-word error for {source:?}, got: {:?}",
            parse.errors
        );
        assert!(
            reserved[0].message.contains(word),
            "error should name `{word}`, got: {}",
            reserved[0].message
        );
        assert!(
            reserved[0].message.contains(advice),
            "error should advise {advice:?}, got: {}",
            reserved[0].message
        );
    }

    /// Assert the parse reports exactly one error, and that error names the
    /// reserved word.
    fn assert_one_reserved_word_error(source: &str, word: &str) {
        let parse = cst_parse(source);
        let reserved: Vec<_> = parse
            .errors
            .iter()
            .filter(|e| e.kind == CstErrorKind::ReservedWord)
            .collect();
        assert_eq!(
            reserved.len(),
            1,
            "expected one reserved-word error for {source:?}, got: {:?}",
            parse.errors
        );
        assert!(
            reserved[0].message.contains(word),
            "error should name `{word}`, got: {}",
            reserved[0].message
        );
        assert!(
            reserved[0].message.contains("JavaScript"),
            "error should say JavaScript reserves the word, got: {}",
            reserved[0].message
        );
    }

    #[test]
    fn record_type_field_named_for() {
        assert_no_errors("type Form = { for: string }");
    }

    #[test]
    fn record_type_fields_named_after_javascript_keywords() {
        assert_no_errors(
            "type Payload = { for: string, class: string, function: string, if: string }",
        );
    }

    #[test]
    fn record_type_field_named_for_is_lossless() {
        assert_lossless("type Form = { for: string }");
    }

    #[test]
    fn object_literal_key_named_for() {
        assert_no_errors(r#"let row = { for: "name", class: "row" }"#);
    }

    #[test]
    fn brace_construction_field_named_for() {
        assert_no_errors(r#"let row = Form { for: "name", class: "row" }"#);
    }

    #[test]
    fn named_argument_labelled_for() {
        assert_no_errors(r#"let row = label(for: "name")"#);
    }

    #[test]
    fn member_named_for() {
        assert_no_errors("let name = f.for");
    }

    #[test]
    fn member_named_class() {
        assert_no_errors("let name = f.class");
    }

    #[test]
    fn jsx_prop_named_for() {
        assert_no_errors(r#"let view = <label for="name" />"#);
    }

    #[test]
    fn jsx_prop_named_class() {
        assert_no_errors(r#"let view = <label class="row" />"#);
    }

    #[test]
    fn destructure_renames_a_reserved_field() {
        assert_no_errors("let { for: htmlFor } = props");
    }

    #[test]
    fn record_pattern_renames_a_reserved_field() {
        assert_no_errors("let name = match row {\n    Form { for: target } -> target,\n}");
    }

    #[test]
    fn let_binding_named_for_is_rejected() {
        assert_one_reserved_word_error("let for = 1", "for");
    }

    #[test]
    fn let_binding_named_class_is_rejected() {
        assert_one_reserved_word_error("let class = 1", "class");
    }

    #[test]
    fn parameter_named_for_is_rejected() {
        assert_one_reserved_word_error(r#"let f(for: string) -> string = { "x" }"#, "for");
    }

    #[test]
    fn function_binding_named_for_is_rejected() {
        assert_one_reserved_word_error(r#"let for(x: string) -> string = { x }"#, "for");
    }

    #[test]
    fn destructured_binding_named_for_is_rejected() {
        assert_one_reserved_word_error("let { for } = props", "for");
    }

    #[test]
    fn object_shorthand_named_for_is_rejected() {
        assert_one_reserved_word_error("let row = { for, name }", "for");
    }

    #[test]
    fn punned_field_named_for_is_rejected() {
        assert_one_reserved_word_error("let row = Form { for: }", "for");
    }

    // ── Every word names a property ───────────────────────────────

    #[test]
    fn jsx_prop_named_after_a_floe_keyword() {
        // Regression: `is_keyword` used to accept these six, and the first
        // cut of the role table dropped them.
        for prop in ["match", "fn", "let", "import", "export", "trait"] {
            assert_no_errors(&format!(r#"let v = <label {prop}="x" />"#));
        }
    }

    #[test]
    fn record_type_field_named_after_a_floe_keyword() {
        assert_no_errors("type T = { match: string, self: string, let: string }");
    }

    #[test]
    fn member_named_after_a_floe_keyword() {
        assert_no_errors("let x = row.match");
    }

    #[test]
    fn named_argument_labelled_after_a_floe_keyword() {
        assert_no_errors(r#"let x = render(match: "a")"#);
    }

    #[test]
    fn dot_shorthand_named_for() {
        assert_no_errors("let names = rows |> map(.for)");
    }

    #[test]
    fn named_variant_field_named_for() {
        assert_no_errors("type T = A { for: string } | B");
    }

    #[test]
    fn function_type_param_labelled_for() {
        assert_no_errors("typealias L = (for: string) -> string");
    }

    #[test]
    fn for_block_parameter_named_for_is_rejected() {
        assert_one_reserved_word_error(
            "for User {\n    let label(self, for: string) -> string = { self.id }\n}",
            "for",
        );
    }

    #[test]
    fn a_floe_keyword_cannot_bind() {
        assert_one_reserved_word_error_with("let match = 1", "match", "keyword in Floe");
    }

    #[test]
    fn a_floe_keyword_parameter_is_rejected() {
        assert_one_reserved_word_error_with(
            r#"let f(match: string) -> string = { "x" }"#,
            "match",
            "keyword in Floe",
        );
    }

    // ── A shorthand reads a value, so it takes the pun advice ─────

    #[test]
    fn object_shorthand_named_for_takes_the_pun_advice() {
        assert_one_reserved_word_error_with("let row = { for, name }", "for", "punning");
    }

    #[test]
    fn brace_construct_shorthand_named_for_takes_the_pun_advice() {
        assert_one_reserved_word_error_with("let row = Form { for }", "for", "punning");
    }

    #[test]
    fn destructure_shorthand_named_for_takes_the_pun_advice() {
        assert_one_reserved_word_error_with("let { for } = props", "for", "punning");
    }

    #[test]
    fn punned_field_with_colon_takes_the_pun_advice() {
        assert_one_reserved_word_error_with("let row = Form { for: }", "for", "punning");
    }

    #[test]
    fn a_binding_takes_the_rename_advice() {
        assert_one_reserved_word_error_with("let for = 1", "for", "Rename the value");
    }

    // ── A shorthand of a Floe keyword stays a block ───────────────

    #[test]
    fn lone_self_in_braces_is_a_block() {
        assert_no_errors("for User {\n    let me(self) -> User = { self }\n}");
    }

    #[test]
    fn match_in_braces_is_a_block() {
        assert_no_errors(r#"let f(x: number) -> string = { match x { 1 -> "a", _ -> "b" } }"#);
    }

    // ── `for` stays a keyword in its own three positions ──────────

    #[test]
    fn for_block_at_item_start_still_parses() {
        assert_no_errors("for User {\n    let name(self) -> string = { self.id }\n}");
    }

    #[test]
    fn import_for_specifier_still_parses() {
        assert_no_errors(r#"import { for User } from "./user""#);
    }

    #[test]
    fn impl_trait_for_type_still_parses() {
        assert_no_errors("impl Show for User {\n    let show(self) -> string = { self.id }\n}");
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

    // ── Unicode names (#1576) ─────────────────────────────────────

    #[test]
    fn an_accented_name_parses() {
        assert_no_errors("let café = 1");
    }

    #[test]
    fn a_japanese_name_parses() {
        assert_no_errors("let 名前 = \"kotoko\"");
    }

    #[test]
    fn a_unicode_name_round_trips() {
        assert_lossless("let café = 1\nlet 名前 = café\n");
    }

    #[test]
    fn an_emoji_name_reports_the_rule() {
        let parse = cst_parse("let 🎉 = 1");
        let error = parse
            .errors
            .first()
            .expect("an emoji cannot name a value, so the parser reports it");
        assert_eq!(error.kind, CstErrorKind::InvalidName);
        assert!(
            error.message.contains("cannot name anything"),
            "the message must say what is wrong: {}",
            error.message
        );
        assert!(
            error.message.contains("emoji"),
            "the message must name the emoji rule: {}",
            error.message
        );
    }

    #[test]
    fn an_emoji_name_reports_once() {
        // The parser steps over the bad text, so one emoji does not produce
        // a cascade of follow-on errors.
        let parse = cst_parse("let 🎉 = 1");
        assert_eq!(parse.errors.len(), 1, "got: {:?}", parse.errors);
    }

    #[test]
    fn jsx_content_carries_emoji_and_japanese_text() {
        let source = "let View() -> JSX.Element = {\n    <p>こんにちは 🎉 world</p>\n}";
        assert_no_errors(source);
        assert_lossless(source);
    }
}
