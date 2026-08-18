pub mod span;
pub mod token;

use span::Span;
use token::{Token, TokenKind};

/// Bytes >= this value are non-ASCII (multi-byte UTF-8 lead or continuation bytes).
const UTF8_MULTIBYTE_FLAG: u8 = 0x80;
/// Minimum value for a UTF-8 lead byte (starts a new character).
const UTF8_LEAD_BYTE_MIN: u8 = 0xC0;

/// True when this byte continues a multi-byte UTF-8 character.
const fn is_utf8_continuation(byte: u8) -> bool {
    byte >= UTF8_MULTIBYTE_FLAG && byte < UTF8_LEAD_BYTE_MIN
}

/// True when this character may start a Floe name.
///
/// Floe emits TypeScript, so every Floe name must be a legal TypeScript name.
/// The rule is the ECMAScript one, which `oxc_syntax` implements: a character
/// with the Unicode `ID_Start` property, `$`, or `_`. Floe does not normalize
/// a name, because TypeScript does not.
///
/// This function is the source of the rule for the compiler. The lexer and
/// the language server both read it. The editor grammars listed in
/// `.claude/rules/syntax-sources.md` hold their own copy of the rule, so
/// change them in the same commit.
pub fn is_name_start(ch: char) -> bool {
    oxc_syntax::identifier::is_identifier_start(ch)
}

/// True when this character may continue a Floe name.
///
/// The rule is the ECMAScript one: a character with the Unicode `ID_Continue`
/// property, `$`, a zero width joiner, or a zero width non-joiner.
pub fn is_name_part(ch: char) -> bool {
    oxc_syntax::identifier::is_identifier_part(ch)
}

/// Where a token starts: the byte offset, plus the line and the character
/// column at that offset.
///
/// The lexer already tracks all three as it walks, so a token records them
/// when it starts instead of counting the prefix again when it ends. The
/// count-again form was O(n squared) over a file (#1576 review).
#[derive(Debug, Clone, Copy)]
struct Mark {
    /// Byte offset into the source.
    pos: usize,
    /// 1-based line number at `pos`.
    line: usize,
    /// 1-based column number at `pos`, counted in characters.
    column: usize,
}

/// The Floe lexer. Converts source text into a sequence of tokens.
pub struct Lexer<'src> {
    /// The full source text being lexed.
    source: &'src str,
    /// The remaining source bytes as a slice.
    bytes: &'src [u8],
    /// Current byte offset into the source.
    pos: usize,
    /// Current 1-based line number.
    line: usize,
    /// Current 1-based column number.
    column: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire source, returning all tokens including Eof.
    /// Trivia (whitespace, comments) is skipped.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    /// Tokenize the entire source, including trivia tokens (whitespace, comments).
    pub fn tokenize_with_trivia(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token_with_trivia();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    /// Advance to the next token, emitting trivia tokens for whitespace/comments.
    pub fn next_token_with_trivia(&mut self) -> Token {
        if self.is_at_end() {
            return self.make_token(TokenKind::Eof, self.mark());
        }

        // Check for trivia first
        match self.peek() {
            Some(b' ' | b'\t' | b'\r' | b'\n') => {
                let start = self.mark();
                while !self.is_at_end() && matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n'))
                {
                    self.advance();
                }
                return self.make_token(TokenKind::Whitespace, start);
            }
            Some(b'/') if self.peek_at(1) == Some(b'/') => {
                let start = self.mark();
                while !self.is_at_end() && self.peek() != Some(b'\n') {
                    self.advance();
                }
                return self.make_token(TokenKind::Comment, start);
            }
            Some(b'/') if self.peek_at(1) == Some(b'*') => {
                let start = self.mark();
                self.consume_block_comment();
                return self.make_token(TokenKind::BlockComment, start);
            }
            _ => {}
        }

        // Non-trivia token — delegate to the core scanning logic
        self.scan_non_trivia_token()
    }

    /// Advance to the next token (skipping trivia).
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        if self.is_at_end() {
            return self.make_token(TokenKind::Eof, self.mark());
        }

        self.scan_non_trivia_token()
    }

    /// Scan a non-trivia token. Assumes we are NOT at whitespace/comment/EOF.
    #[allow(clippy::too_many_lines)]
    fn scan_non_trivia_token(&mut self) -> Token {
        let start = self.mark();

        // A non-ASCII character decides between a name and JSX content, so read
        // the whole character before the byte match below splits it.
        if self.bytes[self.pos] >= UTF8_MULTIBYTE_FLAG {
            let kind = self.scan_non_ascii(start.pos);

            return self.make_token(kind, start);
        }

        let ch = self.advance();

        let kind = match ch {
            // Single-character tokens
            b'(' => TokenKind::LeftParen,
            b')' => TokenKind::RightParen,
            b'{' => TokenKind::LeftBrace,
            b'}' => TokenKind::RightBrace,
            b'[' => TokenKind::LeftBracket,
            b']' => TokenKind::RightBracket,
            b',' => TokenKind::Comma,
            b';' => TokenKind::Semicolon,
            b':' => TokenKind::Colon,
            b'?' => TokenKind::Question,
            b'+' => TokenKind::Plus,
            b'*' => TokenKind::Star,
            b'%' => TokenKind::Percent,

            // Dot, DotDot, or DotDotDot
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.advance();
                    if self.peek() == Some(b'.') {
                        self.advance();
                        TokenKind::DotDotDot
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }

            // Minus or ThinArrow
            b'-' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    TokenKind::ThinArrow
                } else {
                    TokenKind::Minus
                }
            }

            // Equal, FatArrow, or EqualEqual
            b'=' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    TokenKind::FatArrow
                } else if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::EqualEqual
                } else {
                    TokenKind::Equal
                }
            }

            // Bang or BangEqual
            b'!' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::BangEqual
                } else {
                    TokenKind::Bang
                }
            }

            // LessThan, LessEqual, or LeftArrow
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::LessEqual
                } else if self.peek() == Some(b'-') {
                    self.advance();
                    TokenKind::LeftArrow
                } else {
                    TokenKind::LessThan
                }
            }

            // GreaterThan or GreaterEqual
            b'>' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::GreaterThan
                }
            }

            // Pipe (|>), PipeUnwrap (|>?), or PipePipe (||)
            b'|' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    if self.peek() == Some(b'?') {
                        self.advance();
                        TokenKind::PipeUnwrap
                    } else {
                        TokenKind::Pipe
                    }
                } else if self.peek() == Some(b'|') {
                    self.advance();
                    TokenKind::PipePipe
                } else {
                    // Bare `|` — used in type union declarations and lambda delimiters
                    TokenKind::VerticalBar
                }
            }

            // Amp (&) or AmpAmp (&&)
            b'&' => {
                if self.peek() == Some(b'&') {
                    self.advance();
                    TokenKind::AmpAmp
                } else {
                    TokenKind::Amp
                }
            }

            // Slash (division) — comments are already handled in skip_whitespace_and_comments
            b'/' => TokenKind::Slash,

            // String literals
            b'"' => self.scan_string(),

            // Template literals
            b'`' => self.scan_template_literal(),

            // Numbers
            b'0'..=b'9' => self.scan_number(start.pos),

            // Identifiers and keywords (including _ as standalone)
            b'_' if !self.peek_is_ident_char() => TokenKind::Underscore,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => self.scan_identifier(start.pos),

            other => {
                // Unknown ASCII character — emit as identifier for error recovery
                TokenKind::Identifier(String::from(other as char))
            }
        };

        self.make_token(kind, start)
    }

    // -- Scanning helpers --

    fn scan_string(&mut self) -> TokenKind {
        let mut value = String::new();
        while !self.is_at_end() && self.peek() != Some(b'"') {
            let ch = self.advance();
            if ch == b'\\' && !self.is_at_end() {
                let escaped = self.advance();
                match self.process_escape(escaped) {
                    Some(c) => value.push(c),
                    // String-specific escape: \"
                    _ if escaped == b'"' => value.push('"'),
                    _ => {
                        value.push('\\');
                        value.push(escaped as char);
                    }
                }
            } else if ch >= UTF8_MULTIBYTE_FLAG {
                let char_start = self.pos - 1;
                value.push_str(self.consume_utf8_continuation_bytes(char_start));
            } else {
                value.push(ch as char);
            }
        }
        // Consume the closing quote
        if !self.is_at_end() {
            self.advance();
        }
        TokenKind::String(value)
    }

    fn scan_template_literal(&mut self) -> TokenKind {
        let mut parts = Vec::new();
        let mut current_raw = String::new();

        while !self.is_at_end() && self.peek() != Some(b'`') {
            if self.peek() == Some(b'$') && self.peek_at(1) == Some(b'{') {
                // Save current raw segment
                if !current_raw.is_empty() {
                    parts.push(token::TemplatePart::Raw(std::mem::take(&mut current_raw)));
                }

                // Skip `${`
                self.advance();
                self.advance();

                // Collect tokens until matching `}`
                let mut depth = 1;
                let mut interp_tokens = Vec::new();
                while !self.is_at_end() && depth > 0 {
                    if self.peek() == Some(b'{') {
                        depth += 1;
                    } else if self.peek() == Some(b'}') {
                        depth -= 1;
                        if depth == 0 {
                            self.advance(); // consume the closing `}`
                            break;
                        }
                    }
                    interp_tokens.push(self.next_token());
                }
                parts.push(token::TemplatePart::Interpolation(interp_tokens));
            } else if self.peek() == Some(b'\\') {
                self.advance();
                if !self.is_at_end() {
                    let escaped = self.advance();
                    match self.process_escape(escaped) {
                        Some(c) => current_raw.push(c),
                        // Template-specific escapes: backtick and $
                        _ if escaped == b'`' => current_raw.push('`'),
                        _ if escaped == b'$' => current_raw.push('$'),
                        _ => {
                            current_raw.push('\\');
                            current_raw.push(escaped as char);
                        }
                    }
                }
            } else {
                let ch = self.advance();
                if ch >= UTF8_MULTIBYTE_FLAG {
                    let char_start = self.pos - 1;
                    current_raw.push_str(self.consume_utf8_continuation_bytes(char_start));
                } else {
                    current_raw.push(ch as char);
                }
            }
        }

        // Save final raw segment
        if !current_raw.is_empty() {
            parts.push(token::TemplatePart::Raw(current_raw));
        }

        // Consume the closing backtick
        if !self.is_at_end() {
            self.advance();
        }

        TokenKind::TemplateLiteral(parts)
    }

    fn scan_number(&mut self, start: usize) -> TokenKind {
        // Check for hex, binary, octal prefixes
        if self.source.as_bytes().get(start) == Some(&b'0') {
            match self.peek() {
                Some(b'x' | b'X') => {
                    self.advance();
                    while !self.is_at_end()
                        && matches!(
                            self.peek(),
                            Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' | b'_')
                        )
                    {
                        self.advance();
                    }
                    return TokenKind::Number(Self::strip_underscores(
                        &self.source[start..self.pos],
                    ));
                }
                Some(b'b' | b'B') => {
                    self.advance();
                    while !self.is_at_end() && matches!(self.peek(), Some(b'0' | b'1' | b'_')) {
                        self.advance();
                    }
                    return TokenKind::Number(Self::strip_underscores(
                        &self.source[start..self.pos],
                    ));
                }
                Some(b'o' | b'O') => {
                    self.advance();
                    while !self.is_at_end() && matches!(self.peek(), Some(b'0'..=b'7' | b'_')) {
                        self.advance();
                    }
                    return TokenKind::Number(Self::strip_underscores(
                        &self.source[start..self.pos],
                    ));
                }
                _ => {}
            }
        }

        // Decimal digits
        while !self.is_at_end() && matches!(self.peek(), Some(b'0'..=b'9' | b'_')) {
            self.advance();
        }

        // Fractional part
        if self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            self.advance(); // consume `.`
            while !self.is_at_end() && matches!(self.peek(), Some(b'0'..=b'9' | b'_')) {
                self.advance();
            }
        }

        TokenKind::Number(Self::strip_underscores(&self.source[start..self.pos]))
    }

    /// Strip underscore separators from a number literal.
    /// `1_000` becomes `1000`, `0xFF_FF` becomes `0xFFFF`.
    fn strip_underscores(raw: &str) -> String {
        if raw.contains('_') {
            raw.chars().filter(|&c| c != '_').collect()
        } else {
            raw.to_string()
        }
    }

    fn scan_identifier(&mut self, start: usize) -> TokenKind {
        while self.peek_char().is_some_and(is_name_part) {
            self.advance_char();
        }
        let word = &self.source[start..self.pos];
        token::lookup_keyword(word).unwrap_or_else(|| TokenKind::Identifier(word.to_string()))
    }

    /// Scan a token that starts with a non-ASCII character.
    ///
    /// Floe names follow the TypeScript rule, because Floe emits TypeScript.
    /// A character that may start a TypeScript identifier starts a name here.
    /// Every other character, an emoji for example, is JSX content.
    fn scan_non_ascii(&mut self, start: usize) -> TokenKind {
        if self.peek_char().is_some_and(is_name_start) {
            return self.scan_identifier(start);
        }

        self.scan_unicode_text(start)
    }

    /// Consume a run of non-ASCII characters that cannot stand in a name.
    /// This carries emoji, symbols and punctuation inside JSX content.
    fn scan_unicode_text(&mut self, start: usize) -> TokenKind {
        while self
            .peek_char()
            .is_some_and(|ch| !ch.is_ascii() && !is_name_start(ch))
        {
            self.advance_char();
        }
        let text = &self.source[start..self.pos];
        TokenKind::UnicodeText(text.to_string())
    }

    // -- Extracted helpers --

    /// Consume a `/* ... */` block comment, supporting nesting.
    /// Assumes the lexer is positioned at the opening `/`.
    fn consume_block_comment(&mut self) {
        self.advance(); // /
        self.advance(); // *
        let mut depth = 1;
        while !self.is_at_end() && depth > 0 {
            if self.peek() == Some(b'*') && self.peek_at(1) == Some(b'/') {
                self.advance();
                self.advance();
                depth -= 1;
            } else if self.peek() == Some(b'/') && self.peek_at(1) == Some(b'*') {
                self.advance();
                self.advance();
                depth += 1;
            } else {
                self.advance();
            }
        }
    }

    /// Process a common escape sequence byte, returning the unescaped char.
    /// Returns `None` for context-specific escapes (e.g. `"`, `` ` ``, `$`),
    /// which must be handled at the call site.
    fn process_escape(&mut self, escaped: u8) -> Option<char> {
        match escaped {
            b'n' => Some('\n'),
            b't' => Some('\t'),
            b'r' => Some('\r'),
            b'\\' => Some('\\'),
            b'0' => Some('\0'),
            b'u' => self.process_unicode_escape(),
            _ => None,
        }
    }

    /// Process a `\uXXXX` or `\u{XXXX}` unicode escape sequence.
    /// The `u` has already been consumed. Returns `None` if the sequence is invalid.
    fn process_unicode_escape(&mut self) -> Option<char> {
        if self.peek() == Some(b'{') {
            // \u{XXXX} braced form — 1 to 6 hex digits
            self.advance(); // consume '{'
            let start = self.pos;
            while !self.is_at_end() && self.peek() != Some(b'}') {
                self.advance();
            }
            let hex = &self.source[start..self.pos];
            if !self.is_at_end() {
                self.advance(); // consume '}'
            }
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        } else {
            // \uXXXX fixed 4-digit form
            let start = self.pos;
            for _ in 0..4 {
                if self.is_at_end() {
                    return None;
                }
                self.advance();
            }
            let hex = &self.source[start..self.pos];
            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
        }
    }

    /// Consume UTF-8 continuation bytes starting from a position where the lead
    /// byte has already been advanced past. Returns the full character as a `&str`.
    fn consume_utf8_continuation_bytes(&mut self, start_pos: usize) -> &str {
        while !self.is_at_end()
            && self.bytes[self.pos] >= UTF8_MULTIBYTE_FLAG
            && self.bytes[self.pos] < UTF8_LEAD_BYTE_MIN
        {
            self.advance();
        }
        &self.source[start_pos..self.pos]
    }

    // -- Low-level helpers --

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => {
                    self.advance();
                }
                // Line comment
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    while !self.is_at_end() && self.peek() != Some(b'\n') {
                        self.advance();
                    }
                }
                // Block comment
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    self.consume_block_comment();
                }
                _ => break,
            }
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// True when a name may continue at the current position.
    fn peek_is_ident_char(&self) -> bool {
        self.peek_char().is_some_and(is_name_part)
    }

    /// The current position, for a token that starts here.
    const fn mark(&self) -> Mark {
        Mark {
            pos: self.pos,
            line: self.line,
            column: self.column,
        }
    }

    /// The whole character at the current position.
    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Consume the whole character at the current position.
    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(ch)
    }

    fn advance(&mut self) -> u8 {
        let ch = self.bytes[self.pos];
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.column = 1;
        } else if !is_utf8_continuation(ch) {
            // A column counts characters, so the trailing bytes of a
            // multi-byte character do not advance it.
            self.column += 1;
        }
        ch
    }

    /// Build a token that runs from `start` to the current position.
    ///
    /// `start` carries the line and the column, so this does no counting.
    /// `advance` and `advance_char` keep `self.line` and `self.column` on
    /// the character the lexer stands on, and `mark` copies them.
    fn make_token(&self, kind: TokenKind, start: Mark) -> Token {
        Token::new(
            kind,
            Span::new(start.pos, self.pos, start.line, start.column),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use token::{BannedKeyword, TemplatePart, TokenKind};

    fn lex(input: &str) -> Vec<TokenKind> {
        Lexer::new(input)
            .tokenize()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn empty_input() {
        assert_eq!(lex(""), vec![TokenKind::Eof]);
    }

    #[test]
    fn single_char_tokens() {
        assert_eq!(
            lex("( ) { } [ ] , ; : ?"),
            vec![
                TokenKind::LeftParen,
                TokenKind::RightParen,
                TokenKind::LeftBrace,
                TokenKind::RightBrace,
                TokenKind::LeftBracket,
                TokenKind::RightBracket,
                TokenKind::Comma,
                TokenKind::Semicolon,
                TokenKind::Colon,
                TokenKind::Question,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            lex("|> -> == != <= >= && || !"),
            vec![
                TokenKind::Pipe,
                TokenKind::ThinArrow,
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Bang,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn arithmetic() {
        assert_eq!(
            lex("+ - * / %"),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn dot_and_dotdot() {
        assert_eq!(
            lex(". .."),
            vec![TokenKind::Dot, TokenKind::DotDot, TokenKind::Eof]
        );
    }

    #[test]
    fn dot_dot_dot() {
        assert_eq!(lex("..."), vec![TokenKind::DotDotDot, TokenKind::Eof]);
        assert_eq!(
            lex(".. ."),
            vec![TokenKind::DotDot, TokenKind::Dot, TokenKind::Eof]
        );
    }

    #[test]
    fn underscore_standalone_vs_identifier() {
        assert_eq!(
            lex("_ _name"),
            vec![
                TokenKind::Underscore,
                TokenKind::Identifier("_name".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn keywords() {
        assert_eq!(
            lex("let fn export import match type typealias opaque"),
            vec![
                TokenKind::Let,
                TokenKind::Fn,
                TokenKind::Export,
                TokenKind::Import,
                TokenKind::Match,
                TokenKind::Type,
                TokenKind::Typealias,
                TokenKind::Opaque,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn builtins() {
        assert_eq!(
            lex("Ok Err Some None true false"),
            vec![
                TokenKind::Identifier("Ok".to_string()),
                TokenKind::Identifier("Err".to_string()),
                TokenKind::Identifier("Some".to_string()),
                TokenKind::Identifier("None".to_string()),
                TokenKind::Bool(true),
                TokenKind::Bool(false),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn todo_and_unreachable() {
        assert_eq!(
            lex("todo unreachable"),
            vec![TokenKind::Todo, TokenKind::Unreachable, TokenKind::Eof,]
        );
    }

    #[test]
    fn banned_keywords() {
        let tokens = lex("const class throw null undefined any as enum function if else return");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Banned(BannedKeyword::Const),
                TokenKind::Banned(BannedKeyword::Class),
                TokenKind::Banned(BannedKeyword::Throw),
                TokenKind::Banned(BannedKeyword::Null),
                TokenKind::Banned(BannedKeyword::Undefined),
                TokenKind::Banned(BannedKeyword::Any),
                TokenKind::Banned(BannedKeyword::As),
                TokenKind::Banned(BannedKeyword::Enum),
                TokenKind::Banned(BannedKeyword::Function),
                TokenKind::Banned(BannedKeyword::If),
                TokenKind::Banned(BannedKeyword::Else),
                TokenKind::Banned(BannedKeyword::Return),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn vertical_bar() {
        assert_eq!(
            lex("| || |>"),
            vec![
                TokenKind::VerticalBar,
                TokenKind::PipePipe,
                TokenKind::Pipe,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            lex("42 3.14 0xFF 0b1010 0o77 1_000"),
            vec![
                TokenKind::Number("42".to_string()),
                TokenKind::Number("3.14".to_string()),
                TokenKind::Number("0xFF".to_string()),
                TokenKind::Number("0b1010".to_string()),
                TokenKind::Number("0o77".to_string()),
                TokenKind::Number("1000".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn number_underscore_separators() {
        // Underscores are stripped from the token value
        assert_eq!(
            lex("1_000_000 3.141_592 0xFF_FF 0b1010_0101 0o77_77"),
            vec![
                TokenKind::Number("1000000".to_string()),
                TokenKind::Number("3.141592".to_string()),
                TokenKind::Number("0xFFFF".to_string()),
                TokenKind::Number("0b10100101".to_string()),
                TokenKind::Number("0o7777".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn number_without_underscores_unchanged() {
        assert_eq!(
            lex("42"),
            vec![TokenKind::Number("42".to_string()), TokenKind::Eof]
        );
    }

    #[test]
    fn string_literal() {
        assert_eq!(
            lex(r#""hello world""#),
            vec![TokenKind::String("hello world".to_string()), TokenKind::Eof,]
        );
    }

    #[test]
    fn string_escape_sequences() {
        assert_eq!(
            lex(r#""hello\nworld""#),
            vec![
                TokenKind::String("hello\nworld".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn template_literal_no_interpolation() {
        let tokens = lex("`hello world`");
        assert_eq!(
            tokens,
            vec![
                TokenKind::TemplateLiteral(vec![TemplatePart::Raw("hello world".to_string())]),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn template_literal_with_interpolation() {
        let tokens = lex("`hello ${name}`");
        assert_eq!(tokens.len(), 2); // TemplateLiteral + Eof
        match &tokens[0] {
            TokenKind::TemplateLiteral(parts) => {
                assert_eq!(parts.len(), 2);
                assert_eq!(parts[0], TemplatePart::Raw("hello ".to_string()));
                match &parts[1] {
                    TemplatePart::Interpolation(toks) => {
                        assert_eq!(toks.len(), 1);
                        assert_eq!(toks[0].kind, TokenKind::Identifier("name".to_string()));
                    }
                    _ => panic!("expected interpolation"),
                }
            }
            _ => panic!("expected template literal"),
        }
    }

    #[test]
    fn line_comments_skipped() {
        assert_eq!(
            lex("let // this is a comment\nx"),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn block_comments_skipped() {
        assert_eq!(
            lex("let /* block */ x"),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn nested_block_comments() {
        assert_eq!(
            lex("let /* outer /* inner */ still comment */ x"),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn span_tracking() {
        let tokens = Lexer::new("const x = 42").tokenize();
        // "const" starts at line 1, column 1
        assert_eq!(tokens[0].span.line, 1);
        assert_eq!(tokens[0].span.column, 1);
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 5);
    }

    #[test]
    fn multiline_span_tracking() {
        let tokens = Lexer::new("const x\nconst y").tokenize();
        // Second "const" should be line 2, column 1
        assert_eq!(tokens[2].span.line, 2);
        assert_eq!(tokens[2].span.column, 1);
    }

    #[test]
    fn pipe_expression() {
        assert_eq!(
            lex("x |> f(y, _)"),
            vec![
                TokenKind::Identifier("x".to_string()),
                TokenKind::Pipe,
                TokenKind::Identifier("f".to_string()),
                TokenKind::LeftParen,
                TokenKind::Identifier("y".to_string()),
                TokenKind::Comma,
                TokenKind::Underscore,
                TokenKind::RightParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn match_expression_tokens() {
        assert_eq!(
            lex("match x { Ok(v) -> v }"),
            vec![
                TokenKind::Match,
                TokenKind::Identifier("x".to_string()),
                TokenKind::LeftBrace,
                TokenKind::Identifier("Ok".to_string()),
                TokenKind::LeftParen,
                TokenKind::Identifier("v".to_string()),
                TokenKind::RightParen,
                TokenKind::ThinArrow,
                TokenKind::Identifier("v".to_string()),
                TokenKind::RightBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn question_operator() {
        assert_eq!(
            lex("fetchUser(id)?"),
            vec![
                TokenKind::Identifier("fetchUser".to_string()),
                TokenKind::LeftParen,
                TokenKind::Identifier("id".to_string()),
                TokenKind::RightParen,
                TokenKind::Question,
                TokenKind::Eof,
            ]
        );
    }

    // ── Unicode names (#1576) ────────────────────────────────────
    //
    // Floe emits TypeScript, so a Floe name follows the TypeScript rule.
    // A character with `ID_Start` starts a name, a character with
    // `ID_Continue` continues one, and `$`, `_`, a zero width joiner and a
    // zero width non-joiner stand as well.

    #[test]
    fn accented_letter_is_one_name() {
        assert_eq!(
            lex("let café = 1"),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("café".to_string()),
                TokenKind::Equal,
                TokenKind::Number("1".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn japanese_letters_are_one_name() {
        assert_eq!(
            lex("let 名前 = \"kotoko\""),
            vec![
                TokenKind::Let,
                TokenKind::Identifier("名前".to_string()),
                TokenKind::Equal,
                TokenKind::String("kotoko".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_name_mixes_ascii_and_unicode_letters() {
        assert_eq!(
            lex("caféLatte1"),
            vec![
                TokenKind::Identifier("caféLatte1".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_combining_mark_continues_a_name() {
        // "cafe" plus U+0301 COMBINING ACUTE ACCENT. The mark carries
        // `ID_Continue`, so it belongs to the name before it.
        assert_eq!(
            lex("cafe\u{301}"),
            vec![
                TokenKind::Identifier("cafe\u{301}".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn two_names_that_differ_only_by_normalization_are_two_names() {
        // Floe does not normalize a name, because TypeScript does not.
        let composed = lex("café");
        let decomposed = lex("cafe\u{301}");
        assert_ne!(composed, decomposed);
    }

    #[test]
    fn a_zero_width_joiner_continues_a_name() {
        assert_eq!(
            lex("a\u{200d}b"),
            vec![
                TokenKind::Identifier("a\u{200d}b".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn an_emoji_is_not_a_name() {
        // TypeScript rejects an emoji in a name, so Floe rejects it too.
        assert_eq!(
            lex("let 🎉 = 1"),
            vec![
                TokenKind::Let,
                TokenKind::UnicodeText("🎉".to_string()),
                TokenKind::Equal,
                TokenKind::Number("1".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn jsx_content_carries_emoji_and_japanese_text() {
        // The text splits into a name token and a symbol token, and the two
        // together hold the source text. The parser reads JSX text back from
        // the source span, so the split does not change the content.
        let source = "こんにちは 🎉 world";
        let kinds = lex(source);
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier("こんにちは".to_string()),
                TokenKind::UnicodeText("🎉".to_string()),
                TokenKind::Identifier("world".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_symbol_run_stops_at_the_next_name() {
        assert_eq!(
            lex("🎉✨名前"),
            vec![
                TokenKind::UnicodeText("🎉✨".to_string()),
                TokenKind::Identifier("名前".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn a_column_counts_characters_not_bytes() {
        // `café` is five characters and six bytes. The `=` after it stands
        // at character 11, and a byte count would report 12.
        let tokens = Lexer::new("let café = 1").tokenize();
        let equal = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Equal)
            .expect("the source holds an `=`");
        assert_eq!(equal.span.line, 1);
        assert_eq!(equal.span.column, 10);
    }

    #[test]
    fn a_column_counts_characters_on_a_later_line() {
        let tokens = Lexer::new("let 名前 = 1\nlet x = 2").tokenize();
        let last = tokens
            .iter()
            .find(|t| t.kind == TokenKind::Number("2".to_string()))
            .expect("the source holds a `2`");
        assert_eq!(last.span.line, 2);
        assert_eq!(last.span.column, 9);
    }

    /// The line and the column of every byte offset in `source`, counted
    /// the way the definition reads: a column counts characters, and a
    /// newline starts the next line.
    fn positions_of(source: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::with_capacity(source.len() + 1);
        let (mut line, mut col) = (1, 1);
        for ch in source.chars() {
            for _ in 0..ch.len_utf8() {
                out.push((line, col));
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        out.push((line, col));

        out
    }

    #[test]
    fn every_token_records_the_position_of_its_first_character() {
        // The lexer carries the line and the column forward as it walks,
        // rather than counting the prefix again for every token. This
        // holds that carried pair to the definition, over trivia, a
        // comment, a string, a template literal, JSX and Unicode names.
        let source = "// caf\u{e9} \u{1F389}\nlet \u{540D}\u{524D} = \"kotoko\"\n\nlet greet(\u{540D}: string) -> string = {\n    `\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}\u{3001}${\u{540D}}`\n}\n\nlet View() -> JSX.Element = {\n    <p>\u{3053}\u{3093}\u{306B}\u{3061}\u{306F} \u{1F389} world</p>\n}\n";
        let want = positions_of(source);

        for token in Lexer::new(source).tokenize_with_trivia() {
            assert_eq!(
                (token.span.line, token.span.column),
                want[token.span.start],
                "token {:?} at byte {} reported the wrong position",
                token.kind,
                token.span.start
            );
        }
    }

    #[test]
    fn a_span_stays_on_a_character_boundary() {
        let source = "let café = 1";
        for token in Lexer::new(source).tokenize() {
            assert!(
                source.is_char_boundary(token.span.start),
                "span start {} is not a character boundary",
                token.span.start
            );
            assert!(
                source.is_char_boundary(token.span.end),
                "span end {} is not a character boundary",
                token.span.end
            );
        }
    }
}
