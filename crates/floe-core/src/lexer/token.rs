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

    // Floe keywords
    Const,
    /// `fn` — function declaration keyword
    Fn,
    Export,
    Import,
    From,
    Match,
    Type,
    Opaque,
    /// `for` — for block keyword (grouping functions under a type)
    For,
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
    /// `deriving` — auto-derive trait implementations for record types
    Deriving,
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
    Let,
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
            Self::Let => "Use `const` - all bindings are immutable in Floe",
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
            Self::Let => "let",
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
        "const" => Some(TokenKind::Const),
        "fn" => Some(TokenKind::Fn),
        "export" => Some(TokenKind::Export),
        "import" => Some(TokenKind::Import),
        "from" => Some(TokenKind::From),
        "match" => Some(TokenKind::Match),
        "type" => Some(TokenKind::Type),
        "opaque" => Some(TokenKind::Opaque),
        "for" => Some(TokenKind::For),
        "self" => Some(TokenKind::SelfKw),
        "trusted" => Some(TokenKind::Trusted),
        "trait" => Some(TokenKind::Trait),
        "assert" => Some(TokenKind::Assert),
        "when" => Some(TokenKind::When),
        "collect" => Some(TokenKind::Collect),
        "deriving" => Some(TokenKind::Deriving),
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
        "let" => Some(TokenKind::Banned(BannedKeyword::Let)),
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
        assert_eq!(lookup_keyword("const"), Some(TokenKind::Const));
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
        assert_eq!(lookup_keyword("self"), Some(TokenKind::SelfKw));
        assert_eq!(lookup_keyword("when"), Some(TokenKind::When));
        assert_eq!(lookup_keyword("collect"), Some(TokenKind::Collect));
        assert_eq!(lookup_keyword("true"), Some(TokenKind::Bool(true)));
        assert_eq!(lookup_keyword("false"), Some(TokenKind::Bool(false)));
    }

    #[test]
    fn lookup_banned_keywords() {
        assert_eq!(
            lookup_keyword("let"),
            Some(TokenKind::Banned(BannedKeyword::Let))
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

    #[test]
    fn banned_keyword_help_messages() {
        assert!(BannedKeyword::Let.help_message().contains("const"));
        assert!(BannedKeyword::Throw.help_message().contains("Result"));
        assert!(BannedKeyword::Null.help_message().contains("Option"));
        assert!(BannedKeyword::Enum.help_message().contains("type"));
    }
}
