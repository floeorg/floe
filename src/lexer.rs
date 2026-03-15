pub mod span;
pub mod token;

use span::Span;
use token::{Token, TokenKind};

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
            return self.make_token(TokenKind::Eof, self.pos);
        }

        // Check for trivia first
        match self.peek() {
            Some(b' ' | b'\t' | b'\r' | b'\n') => {
                let start = self.pos;
                while !self.is_at_end() && matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n'))
                {
                    self.advance();
                }
                return self.make_token(TokenKind::Whitespace, start);
            }
            Some(b'/') if self.peek_at(1) == Some(b'/') => {
                let start = self.pos;
                while !self.is_at_end() && self.peek() != Some(b'\n') {
                    self.advance();
                }
                return self.make_token(TokenKind::Comment, start);
            }
            Some(b'/') if self.peek_at(1) == Some(b'*') => {
                let start = self.pos;
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
            return self.make_token(TokenKind::Eof, self.pos);
        }

        self.scan_non_trivia_token()
    }

    /// Scan a non-trivia token. Assumes we are NOT at whitespace/comment/EOF.
    fn scan_non_trivia_token(&mut self) -> Token {
        let start = self.pos;
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

            // Dot or DotDot
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.advance();
                    TokenKind::DotDot
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

            // LessThan or LessEqual
            b'<' => {
                if self.peek() == Some(b'=') {
                    self.advance();
                    TokenKind::LessEqual
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

            // Pipe (|>) or PipePipe (||)
            b'|' => {
                if self.peek() == Some(b'>') {
                    self.advance();
                    TokenKind::Pipe
                } else if self.peek() == Some(b'|') {
                    self.advance();
                    TokenKind::PipePipe
                } else {
                    // Bare `|` — used in type union declarations and lambda delimiters
                    TokenKind::VerticalBar
                }
            }

            // AmpAmp (&&)
            b'&' => {
                if self.peek() == Some(b'&') {
                    self.advance();
                    TokenKind::AmpAmp
                } else {
                    // Single `&` is not used in Floe — treat as unknown
                    TokenKind::Identifier("&".to_string())
                }
            }

            // Slash (division) — comments are already handled in skip_whitespace_and_comments
            b'/' => TokenKind::Slash,

            // String literals
            b'"' => self.scan_string(start),

            // Template literals
            b'`' => self.scan_template_literal(start),

            // Numbers
            b'0'..=b'9' => self.scan_number(start),

            // Identifiers and keywords (including _ as standalone)
            b'_' if !self.peek_is_ident_char() => TokenKind::Underscore,
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => self.scan_identifier(start),

            // Non-ASCII — consume full UTF-8 character(s) as an identifier
            0x80..=0xFF => self.scan_unicode_text(start),

            other => {
                // Unknown ASCII character — emit as identifier for error recovery
                TokenKind::Identifier(String::from(other as char))
            }
        };

        self.make_token(kind, start)
    }

    // -- Scanning helpers --

    fn scan_string(&mut self, _start: usize) -> TokenKind {
        let mut value = String::new();
        while !self.is_at_end() && self.peek() != Some(b'"') {
            let ch = self.advance();
            if ch == b'\\' && !self.is_at_end() {
                let escaped = self.advance();
                match escaped {
                    b'n' => value.push('\n'),
                    b't' => value.push('\t'),
                    b'r' => value.push('\r'),
                    b'\\' => value.push('\\'),
                    b'"' => value.push('"'),
                    b'0' => value.push('\0'),
                    _ => {
                        value.push('\\');
                        value.push(escaped as char);
                    }
                }
            } else if ch >= 0x80 {
                // Multi-byte UTF-8: extract the full character from source
                let char_start = self.pos - 1;
                while !self.is_at_end()
                    && self.bytes[self.pos] >= 0x80
                    && self.bytes[self.pos] < 0xC0
                {
                    self.advance();
                }
                value.push_str(&self.source[char_start..self.pos]);
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

    fn scan_template_literal(&mut self, _start: usize) -> TokenKind {
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
                    match escaped {
                        b'n' => current_raw.push('\n'),
                        b't' => current_raw.push('\t'),
                        b'r' => current_raw.push('\r'),
                        b'\\' => current_raw.push('\\'),
                        b'`' => current_raw.push('`'),
                        b'$' => current_raw.push('$'),
                        _ => {
                            current_raw.push('\\');
                            current_raw.push(escaped as char);
                        }
                    }
                }
            } else {
                let ch = self.advance();
                if ch >= 0x80 {
                    let char_start = self.pos - 1;
                    while !self.is_at_end()
                        && self.bytes[self.pos] >= 0x80
                        && self.bytes[self.pos] < 0xC0
                    {
                        self.advance();
                    }
                    current_raw.push_str(&self.source[char_start..self.pos]);
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
                    return TokenKind::Number(self.source[start..self.pos].to_string());
                }
                Some(b'b' | b'B') => {
                    self.advance();
                    while !self.is_at_end() && matches!(self.peek(), Some(b'0' | b'1' | b'_')) {
                        self.advance();
                    }
                    return TokenKind::Number(self.source[start..self.pos].to_string());
                }
                Some(b'o' | b'O') => {
                    self.advance();
                    while !self.is_at_end() && matches!(self.peek(), Some(b'0'..=b'7' | b'_')) {
                        self.advance();
                    }
                    return TokenKind::Number(self.source[start..self.pos].to_string());
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

        TokenKind::Number(self.source[start..self.pos].to_string())
    }

    fn scan_identifier(&mut self, start: usize) -> TokenKind {
        while !self.is_at_end() && self.peek_is_ident_char() {
            self.advance();
        }
        let word = &self.source[start..self.pos];
        token::lookup_keyword(word).unwrap_or_else(|| TokenKind::Identifier(word.to_string()))
    }

    /// Consume a run of non-ASCII (UTF-8 multi-byte) characters as an identifier.
    /// This handles emoji, unicode symbols, and non-Latin text in JSX content.
    fn scan_unicode_text(&mut self, start: usize) -> TokenKind {
        while !self.is_at_end() && self.bytes[self.pos] >= 0x80 {
            self.advance();
        }
        let text = &self.source[start..self.pos];
        TokenKind::Identifier(text.to_string())
    }

    // -- Low-level helpers --

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => {
                    self.advance();
                }
                Some(b'\n') => {
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

    fn peek_is_ident_char(&self) -> bool {
        matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$')
        )
    }

    fn advance(&mut self) -> u8 {
        let ch = self.bytes[self.pos];
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn make_token(&self, kind: TokenKind, start: usize) -> Token {
        // Calculate the line/column of the start position by counting back
        let mut line = self.line;
        let mut col = self.column;

        // We need the line/col at `start`, not at `self.pos`.
        // Recompute from the source up to `start`.
        if start < self.pos {
            line = 1;
            col = 1;
            for &b in &self.bytes[..start] {
                if b == b'\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
            }
        }

        Token::new(kind, Span::new(start, self.pos, line, col))
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
            lex("|> -> => == != <= >= && || !"),
            vec![
                TokenKind::Pipe,
                TokenKind::ThinArrow,
                TokenKind::FatArrow,
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
            lex("const fn export import match type opaque return"),
            vec![
                TokenKind::Const,
                TokenKind::Fn,
                TokenKind::Export,
                TokenKind::Import,
                TokenKind::Match,
                TokenKind::Type,
                TokenKind::Opaque,
                TokenKind::Return,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn builtins() {
        assert_eq!(
            lex("Ok Err Some None true false"),
            vec![
                TokenKind::Ok,
                TokenKind::Err,
                TokenKind::Some,
                TokenKind::None,
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
        let tokens = lex("let class throw null undefined any as enum function if else");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Banned(BannedKeyword::Let),
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
                TokenKind::Number("1_000".to_string()),
                TokenKind::Eof,
            ]
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
            lex("const // this is a comment\nx"),
            vec![
                TokenKind::Const,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn block_comments_skipped() {
        assert_eq!(
            lex("const /* block */ x"),
            vec![
                TokenKind::Const,
                TokenKind::Identifier("x".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn nested_block_comments() {
        assert_eq!(
            lex("const /* outer /* inner */ still comment */ x"),
            vec![
                TokenKind::Const,
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
                TokenKind::Ok,
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
}
