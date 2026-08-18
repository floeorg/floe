use super::span::Span;

/// A token produced by the lexer, pairing a token kind with its source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// All possible token types in Floe.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // -- Literals --
    /// Integer or float literal: `42`, `3.14`, `0xFF`, `0b1010`
    /// Underscore separators (e.g. `1_000`) are stripped during lexing.
    Number(String),
    /// Double-quoted string literal: `"hello"`
    String(String),
    /// Template literal: `` `hello ${name}` `` — stored as parts between interpolations
    TemplateLiteral(Vec<TemplatePart>),
    /// `true` or `false`
    Bool(bool),

    // -- Identifiers & Keywords --
    /// Any identifier: variable names, type names, etc.
    Identifier(String),
    /// A run of non-ASCII characters that cannot stand in a name: emoji,
    /// symbols and punctuation. Floe carries this text inside JSX content.
    /// It never names anything, because TypeScript rejects such a name.
    UnicodeText(String),

    // Floe keywords
    /// `let` — universal binding keyword (values and functions)
    Let,
    /// `fn` — retained only for methods inside `for { ... }` blocks.
    /// Top-level function declarations use `let` + arrow syntax.
    Fn,
    Export,
    Import,
    From,
    Match,
    Type,
    Typealias,
    Opaque,
    /// `for` — inherent block keyword (grouping pipe-functions under a local type)
    For,
    /// `impl` — trait impl keyword (`impl Trait for Type { ... }`)
    Impl,
    /// `self` — explicit receiver parameter in for blocks
    SelfKw,
    /// `trusted` — marks an import as safe to call without Result wrapping
    Trusted,
    /// `trait` — trait declaration keyword
    Trait,
    /// `assert` — assertion (only valid inside test blocks)
    Assert,
    /// `when` — match arm guard
    When,
    /// `collect` — error accumulation block
    Collect,
    /// `typeof` — type-level operator to extract the type of a value binding
    Typeof,
    /// `async` — marks a function as async (return type is implicitly wrapped in `Promise<T>`)
    Async,

    // Built-in type constructors
    Value,
    Clear,
    Unchanged,

    // Built-in expressions
    /// `parse` — compiler built-in for runtime type validation
    Parse,
    /// `mock` — compiler built-in for auto-generating test data from types
    Mock,
    /// `todo` — placeholder that panics at runtime, type `never`
    Todo,
    /// `unreachable` — asserts unreachable code path, type `never`
    Unreachable,

    // -- Operators --
    /// `|>` — pipe operator
    Pipe,
    /// `|>?` — pipe-unwrap operator (pipe, then unwrap the result)
    PipeUnwrap,
    /// `->` — match arm arrow
    ThinArrow,
    /// `<-` — use binding arrow
    LeftArrow,
    /// `=>` — fat arrow (banned, kept for error reporting)
    FatArrow,
    /// `|` — vertical bar (union types)
    VerticalBar,
    /// `?` — Result/Option unwrap
    Question,
    /// `_` — placeholder / wildcard
    Underscore,
    /// `..` — spread in constructors
    DotDot,
    /// `...` — spread in type definitions (record type composition)
    DotDotDot,

    // Arithmetic
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,

    // Comparison
    /// `==`
    EqualEqual,
    /// `!=`
    BangEqual,
    /// `<`
    LessThan,
    /// `>`
    GreaterThan,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,

    // Logical / Type operators
    /// `&` — intersection type operator
    Amp,
    /// `&&`
    AmpAmp,
    /// `||`
    PipePipe,
    /// `!`
    Bang,

    // Assignment
    /// `=`
    Equal,

    // -- Delimiters --
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,

    // -- Punctuation --
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `:`
    Colon,
    /// `;`
    Semicolon,

    // -- JSX --
    /// `<` in JSX context (reuses LessThan in non-JSX)
    /// `/` in `</` or `/>` is handled by the parser via Slash

    // -- Special --
    /// End of file
    Eof,

    // -- Banned tokens (produce compile errors) --
    /// A banned keyword was used — carries the keyword and a help message.
    Banned(BannedKeyword),

    // -- Trivia --
    /// Whitespace (spaces, tabs, newlines)
    Whitespace,
    /// Line comment: `// ...`
    Comment,
    /// Block comment: `/* ... */`
    BlockComment,
}

/// What a word may do where an identifier could stand.
///
/// `TokenKind::ident_role` is the single table that answers this. Every
/// "may this word stand here" list in the lexer, the parser and the CST
/// derives from that table, so a new keyword costs one arm and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Every word may name a property. JavaScript accepts any word as a property
/// name, and a property position in Floe is always followed by `:`, `=` or a
/// closing delimiter, so no word is ambiguous there. The roles differ in one
/// thing only: whether the word may name a value.
pub enum IdentRole {
    /// A plain identifier, or a Floe keyword that JavaScript does not
    /// reserve. It may name a value, a parameter and a property.
    Binding,
    /// A word that JavaScript reserves. It may name a property, and it may
    /// never name a value or a parameter, because the emitted TypeScript
    /// would not compile.
    PropertyOnly,
    /// A Floe keyword. It may name a property, and it may never name a
    /// value or a parameter, because Floe needs the word in its own
    /// syntactic position.
    Keyword,
}

impl TokenKind {
    /// The single table: what this word may do where an identifier could
    /// stand. Returns `None` when the token is not a word.
    pub fn ident_role(&self) -> Option<IdentRole> {
        match self {
            // A plain identifier binds, and so does a Floe keyword that
            // JavaScript does not reserve. Each of those keywords is a keyword
            // only in its own syntactic position, and an identifier everywhere
            // else.
            Self::Identifier(_)
            | Self::Type
            | Self::Opaque
            | Self::Trusted
            | Self::Collect
            | Self::Parse
            | Self::Mock
            | Self::Todo
            | Self::Unreachable
            | Self::Clear
            | Self::Unchanged => Some(IdentRole::Binding),

            // Words that JavaScript reserves. `for` is a Floe keyword as
            // well, at item start, in `import { for Type }` and in
            // `impl Trait for Type`.
            Self::For | Self::Banned(_) => Some(IdentRole::PropertyOnly),

            // Floe keywords. Floe needs each of these in its own position,
            // so none of them names a value.
            Self::Let
            | Self::Fn
            | Self::Export
            | Self::Import
            | Self::From
            | Self::Match
            | Self::Typealias
            | Self::Impl
            | Self::SelfKw
            | Self::Trait
            | Self::Assert
            | Self::When
            | Self::Typeof
            | Self::Async
            | Self::Value => Some(IdentRole::Keyword),

            _ => None,
        }
    }

    /// True when this word may name a value: a `let` binding, a parameter, a
    /// destructured name, or a shorthand that reads a value back.
    pub fn can_bind(&self) -> bool {
        self.ident_role() == Some(IdentRole::Binding)
    }

    /// True when this word may name a property: a record field, an
    /// object-literal key, a member, a named argument, or a JSX attribute.
    /// Every word may, for the reason on `IdentRole`.
    ///
    /// `true` and `false` are the exception, because Floe lexes them as
    /// literals rather than as words. `{ true: 1 }` is legal JavaScript and
    /// is not legal Floe.
    pub fn can_name_property(&self) -> bool {
        self.ident_role().is_some()
    }

    /// True when this token may follow a `.`. Every word may, and so may a
    /// number, for tuple element access (`pair.0`).
    pub fn can_name_member(&self) -> bool {
        self.can_name_property() || matches!(self, Self::Number(_))
    }

    /// True when `{ word }` reads as a shorthand field rather than as a
    /// block. A shorthand is the one property position that also reads a
    /// value, so it is not open to every word.
    ///
    /// A word that binds reads the value of that name. A word that
    /// JavaScript reserves cannot bind, but it cannot stand alone as an
    /// expression either, so the shorthand reading is the only one left and
    /// the parser can report the reserved word instead of a cascade. A Floe
    /// keyword stays on the block side, because `{ self }` is a block that
    /// returns `self`.
    pub fn starts_shorthand_field(&self) -> bool {
        matches!(
            self.ident_role(),
            Some(IdentRole::Binding | IdentRole::PropertyOnly)
        )
    }
}

/// Template literal parts: either a raw string segment or an interpolation hole.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    /// Raw string segment between interpolations.
    Raw(String),
    /// The tokens inside a `${...}` interpolation.
    Interpolation(Vec<Token>),
}

/// Banned keywords that produce immediate compile errors with helpful messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannedKeyword {
    Const,
    Class,
    Throw,
    Null,
    Undefined,
    Any,
    As,
    Enum,
    Void,
    Function,
    If,
    Else,
    Return,
}

impl BannedKeyword {
    /// Returns a human-readable error message explaining why this keyword is banned
    /// and what to use instead.
    pub fn help_message(&self) -> &'static str {
        match self {
            Self::Const => "Use `let` — the single binding keyword",
            Self::Class => "Use functions and types instead of classes",
            Self::Throw => "Return a `Result<T, E>` instead of throwing",
            Self::Null => "Use `Option<T>` with `Some`/`None` instead of null",
            Self::Undefined => "Use `Option<T>` with `Some`/`None` instead of undefined",
            Self::Any => "Use a concrete type, generic, or `unknown` with narrowing",
            Self::As => "Use a type guard or `match` expression instead of type assertions",
            Self::Enum => "Use `type` with `|` variants instead of enum",
            Self::Void => "Use the unit type `()` instead of `void`",
            Self::Function => "Use `fn` instead of `function`",
            Self::If => "Use `match` instead of `if`",
            Self::Else => "Use `match` instead of `else`",
            Self::Return => {
                "Floe uses implicit returns — the last expression in a block is the return value"
            }
        }
    }

    /// Returns the keyword as it would appear in source code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Const => "const",
            Self::Class => "class",
            Self::Throw => "throw",
            Self::Null => "null",
            Self::Undefined => "undefined",
            Self::Any => "any",
            Self::As => "as",
            Self::Enum => "enum",
            Self::Void => "void",
            Self::Function => "function",
            Self::If => "if",
            Self::Else => "else",
            Self::Return => "return",
        }
    }
}

/// Maps a string to a keyword token kind, or returns None for identifiers.
pub fn lookup_keyword(word: &str) -> Option<TokenKind> {
    match word {
        // Floe keywords
        "let" => Some(TokenKind::Let),
        "fn" => Some(TokenKind::Fn),
        "export" => Some(TokenKind::Export),
        "import" => Some(TokenKind::Import),
        "from" => Some(TokenKind::From),
        "match" => Some(TokenKind::Match),
        "type" => Some(TokenKind::Type),
        "typealias" => Some(TokenKind::Typealias),
        "opaque" => Some(TokenKind::Opaque),
        "for" => Some(TokenKind::For),
        "impl" => Some(TokenKind::Impl),
        "self" => Some(TokenKind::SelfKw),
        "trusted" => Some(TokenKind::Trusted),
        "trait" => Some(TokenKind::Trait),
        "assert" => Some(TokenKind::Assert),
        "when" => Some(TokenKind::When),
        "collect" => Some(TokenKind::Collect),
        "typeof" => Some(TokenKind::Typeof),
        "async" => Some(TokenKind::Async),
        "true" => Some(TokenKind::Bool(true)),
        "false" => Some(TokenKind::Bool(false)),

        // Built-in constructors
        "Value" => Some(TokenKind::Value),
        "Clear" => Some(TokenKind::Clear),
        "Unchanged" => Some(TokenKind::Unchanged),

        // Built-in expressions
        "parse" => Some(TokenKind::Parse),
        "mock" => Some(TokenKind::Mock),
        "todo" => Some(TokenKind::Todo),
        "unreachable" => Some(TokenKind::Unreachable),

        // Banned keywords
        "const" => Some(TokenKind::Banned(BannedKeyword::Const)),
        "class" => Some(TokenKind::Banned(BannedKeyword::Class)),
        "throw" => Some(TokenKind::Banned(BannedKeyword::Throw)),
        "null" => Some(TokenKind::Banned(BannedKeyword::Null)),
        "undefined" => Some(TokenKind::Banned(BannedKeyword::Undefined)),
        "any" => Some(TokenKind::Banned(BannedKeyword::Any)),
        "as" => Some(TokenKind::Banned(BannedKeyword::As)),
        "enum" => Some(TokenKind::Banned(BannedKeyword::Enum)),
        "void" => Some(TokenKind::Banned(BannedKeyword::Void)),
        "function" => Some(TokenKind::Banned(BannedKeyword::Function)),
        "if" => Some(TokenKind::Banned(BannedKeyword::If)),
        "else" => Some(TokenKind::Banned(BannedKeyword::Else)),
        "return" => Some(TokenKind::Banned(BannedKeyword::Return)),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_floe_keywords() {
        assert_eq!(lookup_keyword("let"), Some(TokenKind::Let));
        assert_eq!(lookup_keyword("fn"), Some(TokenKind::Fn));
        assert_eq!(lookup_keyword("match"), Some(TokenKind::Match));
        assert_eq!(lookup_keyword("opaque"), Some(TokenKind::Opaque));
        assert_eq!(lookup_keyword("trusted"), Some(TokenKind::Trusted));
        assert_eq!(lookup_keyword("trait"), Some(TokenKind::Trait));
        assert_eq!(lookup_keyword("Ok"), None);
        assert_eq!(lookup_keyword("Err"), None);
        assert_eq!(lookup_keyword("Some"), None);
        assert_eq!(lookup_keyword("None"), None);
        assert_eq!(lookup_keyword("for"), Some(TokenKind::For));
        assert_eq!(lookup_keyword("impl"), Some(TokenKind::Impl));
        assert_eq!(lookup_keyword("self"), Some(TokenKind::SelfKw));
        assert_eq!(lookup_keyword("when"), Some(TokenKind::When));
        assert_eq!(lookup_keyword("collect"), Some(TokenKind::Collect));
        assert_eq!(lookup_keyword("true"), Some(TokenKind::Bool(true)));
        assert_eq!(lookup_keyword("false"), Some(TokenKind::Bool(false)));
    }

    #[test]
    fn lookup_banned_keywords() {
        assert_eq!(
            lookup_keyword("const"),
            Some(TokenKind::Banned(BannedKeyword::Const))
        );
        assert_eq!(
            lookup_keyword("class"),
            Some(TokenKind::Banned(BannedKeyword::Class))
        );
        assert_eq!(
            lookup_keyword("null"),
            Some(TokenKind::Banned(BannedKeyword::Null))
        );
        assert_eq!(
            lookup_keyword("enum"),
            Some(TokenKind::Banned(BannedKeyword::Enum))
        );
    }

    #[test]
    fn lookup_todo_unreachable() {
        assert_eq!(lookup_keyword("todo"), Some(TokenKind::Todo));
        assert_eq!(lookup_keyword("unreachable"), Some(TokenKind::Unreachable));
    }

    #[test]
    fn lookup_identifiers_return_none() {
        assert_eq!(lookup_keyword("myVar"), None);
        assert_eq!(lookup_keyword("Component"), None);
        assert_eq!(lookup_keyword("fetch"), None);
    }

    // ── Identifier roles ─────────────────────────────────────

    #[test]
    fn plain_identifier_binds() {
        let word = TokenKind::Identifier("total".to_string());
        assert_eq!(word.ident_role(), Some(IdentRole::Binding));
        assert!(word.can_bind());
        assert!(word.can_name_property());
        assert!(word.can_name_member());
    }

    #[test]
    fn contextual_floe_keyword_binds() {
        for word in [
            TokenKind::Type,
            TokenKind::Opaque,
            TokenKind::Trusted,
            TokenKind::Collect,
            TokenKind::Parse,
            TokenKind::Mock,
            TokenKind::Todo,
            TokenKind::Unreachable,
            TokenKind::Clear,
            TokenKind::Unchanged,
        ] {
            assert_eq!(word.ident_role(), Some(IdentRole::Binding), "{word:?}");
            assert!(word.can_bind(), "{word:?}");
        }
    }

    #[test]
    fn for_names_a_property_only() {
        assert_eq!(TokenKind::For.ident_role(), Some(IdentRole::PropertyOnly));
        assert!(!TokenKind::For.can_bind());
        assert!(TokenKind::For.can_name_property());
        assert!(TokenKind::For.can_name_member());
    }

    #[test]
    fn every_banned_keyword_names_a_property_only() {
        for keyword in [
            BannedKeyword::Const,
            BannedKeyword::Class,
            BannedKeyword::Throw,
            BannedKeyword::Null,
            BannedKeyword::Undefined,
            BannedKeyword::Any,
            BannedKeyword::As,
            BannedKeyword::Enum,
            BannedKeyword::Void,
            BannedKeyword::Function,
            BannedKeyword::If,
            BannedKeyword::Else,
            BannedKeyword::Return,
        ] {
            let word = TokenKind::Banned(keyword.clone());
            assert_eq!(
                word.ident_role(),
                Some(IdentRole::PropertyOnly),
                "{keyword:?}"
            );
            assert!(!word.can_bind(), "{keyword:?}");
            assert!(word.can_name_property(), "{keyword:?}");
        }
    }

    #[test]
    fn floe_keyword_names_a_property_but_binds_nothing() {
        for word in [
            TokenKind::Let,
            TokenKind::Fn,
            TokenKind::Match,
            TokenKind::Impl,
            TokenKind::When,
            TokenKind::From,
            TokenKind::SelfKw,
        ] {
            assert_eq!(word.ident_role(), Some(IdentRole::Keyword), "{word:?}");
            assert!(!word.can_bind(), "{word:?}");
            assert!(word.can_name_property(), "{word:?}");
            assert!(word.can_name_member(), "{word:?}");
        }
    }

    #[test]
    fn every_word_names_a_property() {
        // The one rule the table encodes: a word may always name a property,
        // and the role decides only whether it may name a value.
        for word in [
            TokenKind::Identifier("total".to_string()),
            TokenKind::Type,
            TokenKind::For,
            TokenKind::Banned(BannedKeyword::Class),
            TokenKind::Match,
            TokenKind::Let,
        ] {
            assert!(word.can_name_property(), "{word:?}");
            assert!(word.can_name_member(), "{word:?}");
        }
    }

    #[test]
    fn a_number_names_a_member_for_tuple_access() {
        let number = TokenKind::Number("0".to_string());
        assert_eq!(number.ident_role(), None);
        assert!(number.can_name_member());
        assert!(!number.can_name_property());
    }

    #[test]
    fn punctuation_has_no_identifier_role() {
        for token in [TokenKind::Comma, TokenKind::LeftBrace, TokenKind::Equal] {
            assert_eq!(token.ident_role(), None, "{token:?}");
            assert!(!token.can_name_member(), "{token:?}");
        }
    }

    #[test]
    fn every_keyword_the_lexer_makes_has_a_role() {
        // The table must cover every word `lookup_keyword` produces, so a new
        // keyword cannot slip in without an answer. `true` and `false` are
        // literals, not words that could stand for an identifier.
        for word in [
            "let",
            "fn",
            "export",
            "import",
            "from",
            "match",
            "type",
            "typealias",
            "opaque",
            "for",
            "impl",
            "self",
            "trusted",
            "trait",
            "assert",
            "when",
            "collect",
            "typeof",
            "async",
            "Value",
            "Clear",
            "Unchanged",
            "parse",
            "mock",
            "todo",
            "unreachable",
            "const",
            "class",
            "throw",
            "null",
            "undefined",
            "any",
            "as",
            "enum",
            "void",
            "function",
            "if",
            "else",
            "return",
        ] {
            let token =
                lookup_keyword(word).unwrap_or_else(|| panic!("{word} should lex as a keyword"));
            assert!(token.ident_role().is_some(), "{word} has no role");
        }
    }

    #[test]
    fn banned_keyword_help_messages() {
        assert!(BannedKeyword::Const.help_message().contains("let"));
        assert!(BannedKeyword::Throw.help_message().contains("Result"));
        assert!(BannedKeyword::Null.help_message().contains("Option"));
        assert!(BannedKeyword::Enum.help_message().contains("type"));
    }
}
