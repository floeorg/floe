use crate::lexer::token::TokenKind;

/// All syntax kinds for the Floe CST.
///
/// Token kinds (from the lexer) and composite node kinds (grammar productions)
/// share the same enum so rowan can use a single `u16` tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types)]
pub enum SyntaxKind {
    // ── Tokens (1:1 with TokenKind) ─────────────────────────────

    // Literals
    NUMBER = 0,
    STRING,
    TEMPLATE_LITERAL,
    BOOL,

    // Identifiers & keywords
    IDENT,
    KW_CONST,
    KW_FN,
    KW_EXPORT,
    KW_IMPORT,
    KW_FROM,
    KW_RETURN,
    KW_MATCH,
    KW_TYPE,
    KW_OPAQUE,
    KW_FOR,
    KW_SELF,
    KW_TRUSTED,
    KW_TRAIT,
    KW_ASSERT,
    KW_WHEN,
    KW_COLLECT,
    KW_DERIVING,
    KW_USE,
    KW_TYPEOF,

    // Built-in constructors
    KW_VALUE,
    KW_CLEAR,
    KW_UNCHANGED,

    // Built-in expressions
    KW_PARSE,
    KW_MOCK,
    KW_TODO,
    KW_UNREACHABLE,

    // Operators
    PIPE,          // |>
    THIN_ARROW,    // ->
    LEFT_ARROW,    // <-
    FAT_ARROW,     // =>
    VERT_BAR,      // |
    QUESTION,      // ?
    UNDERSCORE,    // _
    DOT_DOT,       // ..
    DOT_DOT_DOT,   // ...
    PLUS,          // +
    MINUS,         // -
    STAR,          // *
    SLASH,         // /
    PERCENT,       // %
    EQUAL_EQUAL,   // ==
    BANG_EQUAL,    // !=
    LESS_THAN,     // <
    GREATER_THAN,  // >
    LESS_EQUAL,    // <=
    GREATER_EQUAL, // >=
    AMP,           // &
    AMP_AMP,       // &&
    PIPE_PIPE,     // ||
    BANG,          // !
    EQUAL,         // =

    // Delimiters
    L_PAREN,   // (
    R_PAREN,   // )
    L_BRACE,   // {
    R_BRACE,   // }
    L_BRACKET, // [
    R_BRACKET, // ]

    // Punctuation
    COMMA,     // ,
    DOT,       // .
    COLON,     // :
    SEMICOLON, // ;

    // Special tokens
    EOF,
    BANNED,

    // Trivia
    WHITESPACE,
    COMMENT,
    BLOCK_COMMENT,

    // ── Composite nodes (grammar productions) ───────────────────
    PROGRAM,
    IMPORT_DECL,
    IMPORT_SPECIFIER,
    IMPORT_FOR_SPECIFIER,
    CONST_DECL,
    FUNCTION_DECL,
    TYPE_DECL,
    TYPE_DEF_RECORD,
    TYPE_DEF_UNION,
    TYPE_DEF_ALIAS,
    TYPE_DEF_STRING_UNION,
    FOR_BLOCK,
    DERIVING_CLAUSE,
    TRAIT_DECL,
    USE_DECL,
    TEST_BLOCK,
    ASSERT_EXPR,
    RECORD_FIELD,
    RECORD_SPREAD,
    VARIANT,
    VARIANT_FIELD,
    TYPE_EXPR,
    TYPE_EXPR_FUNCTION,
    TYPE_EXPR_RECORD,
    TYPE_EXPR_TUPLE,
    PARAM,
    PARAM_LIST,
    ARG_LIST,
    ARG,

    // Expressions
    BINARY_EXPR,
    UNARY_EXPR,
    PIPE_EXPR,
    CALL_EXPR,
    CONSTRUCT_EXPR,
    MEMBER_EXPR,
    INDEX_EXPR,
    ARROW_EXPR,
    MATCH_EXPR,
    MATCH_ARM,
    MATCH_GUARD,
    PATTERN,
    BLOCK_EXPR,
    RETURN_EXPR,
    UNWRAP_EXPR,
    GROUPED_EXPR,
    ARRAY_EXPR,
    SPREAD_EXPR,
    COLLECT_EXPR,
    TUPLE_EXPR,
    DOT_SHORTHAND,
    VALUE_EXPR,
    PARSE_EXPR,
    MOCK_EXPR,
    OBJECT_EXPR,
    OBJECT_FIELD,
    TODO_EXPR,
    UNREACHABLE_EXPR,

    // JSX
    JSX_ELEMENT,
    JSX_FRAGMENT,
    JSX_OPENING_TAG,
    JSX_CLOSING_TAG,
    JSX_SELF_CLOSING_TAG,
    JSX_PROP,
    JSX_SPREAD_PROP,
    JSX_EXPR_CHILD,
    JSX_TEXT,

    // Item wrapper
    ITEM,
    EXPR_ITEM,

    // Error recovery
    ERROR,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::WHITESPACE | Self::COMMENT | Self::BLOCK_COMMENT)
    }

    /// Whether this token kind can appear as a member name after `.` in a
    /// member expression (e.g. `Date.from`, `Number.parse`, `pair.0`).
    /// Must stay in sync with the parser's member-expression handling in
    /// `cst/exprs.rs`.
    pub fn is_member_name(self) -> bool {
        matches!(
            self,
            Self::IDENT
                | Self::NUMBER
                | Self::BANNED
                | Self::KW_PARSE
                | Self::KW_MATCH
                | Self::KW_FOR
                | Self::KW_FROM
                | Self::KW_TYPE
                | Self::KW_EXPORT
                | Self::KW_IMPORT
                | Self::KW_CONST
                | Self::KW_FN
                | Self::KW_TRAIT
                | Self::KW_COLLECT
                | Self::KW_DERIVING
                | Self::KW_WHEN
                | Self::KW_SELF
        )
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// The language tag for Floe's CST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloeLang {}

impl rowan::Language for FloeLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::ERROR as u16);
        // SAFETY: SyntaxKind is repr(u16) and we checked bounds
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Convenience type aliases.
pub type SyntaxNode = rowan::SyntaxNode<FloeLang>;
pub type SyntaxToken = rowan::SyntaxToken<FloeLang>;

/// Convert a lexer `TokenKind` to a `SyntaxKind`.
pub fn token_kind_to_syntax(kind: &TokenKind) -> SyntaxKind {
    match kind {
        TokenKind::Number(_) => SyntaxKind::NUMBER,
        TokenKind::String(_) => SyntaxKind::STRING,
        TokenKind::TemplateLiteral(_) => SyntaxKind::TEMPLATE_LITERAL,
        TokenKind::Bool(_) => SyntaxKind::BOOL,
        TokenKind::Identifier(_) => SyntaxKind::IDENT,
        TokenKind::Const => SyntaxKind::KW_CONST,
        TokenKind::Fn => SyntaxKind::KW_FN,
        TokenKind::Export => SyntaxKind::KW_EXPORT,
        TokenKind::Import => SyntaxKind::KW_IMPORT,
        TokenKind::From => SyntaxKind::KW_FROM,
        TokenKind::Match => SyntaxKind::KW_MATCH,
        TokenKind::Type => SyntaxKind::KW_TYPE,
        TokenKind::Opaque => SyntaxKind::KW_OPAQUE,
        TokenKind::For => SyntaxKind::KW_FOR,
        TokenKind::SelfKw => SyntaxKind::KW_SELF,
        TokenKind::Trusted => SyntaxKind::KW_TRUSTED,
        TokenKind::Trait => SyntaxKind::KW_TRAIT,
        TokenKind::Assert => SyntaxKind::KW_ASSERT,
        TokenKind::When => SyntaxKind::KW_WHEN,
        TokenKind::Collect => SyntaxKind::KW_COLLECT,
        TokenKind::Deriving => SyntaxKind::KW_DERIVING,
        TokenKind::Use => SyntaxKind::KW_USE,
        TokenKind::Typeof => SyntaxKind::KW_TYPEOF,
        TokenKind::Value => SyntaxKind::KW_VALUE,
        TokenKind::Clear => SyntaxKind::KW_CLEAR,
        TokenKind::Unchanged => SyntaxKind::KW_UNCHANGED,
        TokenKind::Parse => SyntaxKind::KW_PARSE,
        TokenKind::Mock => SyntaxKind::KW_MOCK,
        TokenKind::Todo => SyntaxKind::KW_TODO,
        TokenKind::Unreachable => SyntaxKind::KW_UNREACHABLE,
        TokenKind::Pipe => SyntaxKind::PIPE,
        TokenKind::ThinArrow => SyntaxKind::THIN_ARROW,
        TokenKind::LeftArrow => SyntaxKind::LEFT_ARROW,
        TokenKind::FatArrow => SyntaxKind::FAT_ARROW,
        TokenKind::VerticalBar => SyntaxKind::VERT_BAR,
        TokenKind::Question => SyntaxKind::QUESTION,
        TokenKind::Underscore => SyntaxKind::UNDERSCORE,
        TokenKind::DotDot => SyntaxKind::DOT_DOT,
        TokenKind::DotDotDot => SyntaxKind::DOT_DOT_DOT,
        TokenKind::Plus => SyntaxKind::PLUS,
        TokenKind::Minus => SyntaxKind::MINUS,
        TokenKind::Star => SyntaxKind::STAR,
        TokenKind::Slash => SyntaxKind::SLASH,
        TokenKind::Percent => SyntaxKind::PERCENT,
        TokenKind::EqualEqual => SyntaxKind::EQUAL_EQUAL,
        TokenKind::BangEqual => SyntaxKind::BANG_EQUAL,
        TokenKind::LessThan => SyntaxKind::LESS_THAN,
        TokenKind::GreaterThan => SyntaxKind::GREATER_THAN,
        TokenKind::LessEqual => SyntaxKind::LESS_EQUAL,
        TokenKind::GreaterEqual => SyntaxKind::GREATER_EQUAL,
        TokenKind::Amp => SyntaxKind::AMP,
        TokenKind::AmpAmp => SyntaxKind::AMP_AMP,
        TokenKind::PipePipe => SyntaxKind::PIPE_PIPE,
        TokenKind::Bang => SyntaxKind::BANG,
        TokenKind::Equal => SyntaxKind::EQUAL,
        TokenKind::LeftParen => SyntaxKind::L_PAREN,
        TokenKind::RightParen => SyntaxKind::R_PAREN,
        TokenKind::LeftBrace => SyntaxKind::L_BRACE,
        TokenKind::RightBrace => SyntaxKind::R_BRACE,
        TokenKind::LeftBracket => SyntaxKind::L_BRACKET,
        TokenKind::RightBracket => SyntaxKind::R_BRACKET,
        TokenKind::Comma => SyntaxKind::COMMA,
        TokenKind::Dot => SyntaxKind::DOT,
        TokenKind::Colon => SyntaxKind::COLON,
        TokenKind::Semicolon => SyntaxKind::SEMICOLON,
        TokenKind::Eof => SyntaxKind::EOF,
        TokenKind::Banned(_) => SyntaxKind::BANNED,
        TokenKind::Whitespace => SyntaxKind::WHITESPACE,
        TokenKind::Comment => SyntaxKind::COMMENT,
        TokenKind::BlockComment => SyntaxKind::BLOCK_COMMENT,
    }
}
