use crate::lexer::span::Span;

/// A complete Floe source file.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top-level items in a Floe file.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// `import { x, y } from "module"`
    Import(ImportDecl),
    /// `const x = expr` or `export const x = expr`
    Const(ConstDecl),
    /// `function f(...) { ... }` or `export function f(...) { ... }`
    Function(FunctionDecl),
    /// `type T = ...` or `export type T = ...`
    TypeDecl(TypeDecl),
    /// `for Type { fn ... }` — group functions under a type
    ForBlock(ForBlock),
    /// Expression statement (for REPL / scripts)
    Expr(Expr),
}

// ── Imports ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// Whether the entire import is trusted: `import trusted { ... } from "..."`
    pub trusted: bool,
    pub specifiers: Vec<ImportSpecifier>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportSpecifier {
    pub name: String,
    pub alias: Option<String>,
    /// Whether this specific import is trusted: `import { trusted foo } from "..."`
    pub trusted: bool,
    pub span: Span,
}

// ── Const Declaration ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub exported: bool,
    pub binding: ConstBinding,
    pub type_ann: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstBinding {
    /// Simple name: `const x = ...`
    Name(String),
    /// Array destructuring: `const [a, b] = ...`
    Array(Vec<String>),
    /// Object destructuring: `const { a, b } = ...`
    Object(Vec<String>),
}

// ── Function Declaration ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub exported: bool,
    pub async_fn: bool,
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

// ── Type Declarations ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub exported: bool,
    pub opaque: bool,
    pub name: String,
    pub type_params: Vec<String>,
    pub def: TypeDef,
}

/// The right-hand side of a type declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDef {
    /// Record type: `{ field: Type, ... }`
    Record(Vec<RecordField>),
    /// Union type: `| Variant1 | Variant2(field: Type)`
    Union(Vec<Variant>),
    /// Type alias: `type X = SomeOtherType`
    Alias(TypeExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub type_ann: TypeExpr,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<VariantField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantField {
    pub name: Option<String>,
    pub type_ann: TypeExpr,
    pub span: Span,
}

// ── For Blocks ──────────────────────────────────────────────────

/// `for Type { fn f(self) -> T { ... } }` — group functions under a type.
#[derive(Debug, Clone, PartialEq)]
pub struct ForBlock {
    pub type_name: TypeExpr,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

// ── Type Expressions ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    /// A named type: `string`, `number`, `User`, `Option<T>`
    Named {
        name: String,
        type_args: Vec<TypeExpr>,
    },
    /// Record type inline: `{ name: string, age: number }`
    Record(Vec<RecordField>),
    /// Function type: `(a: number, b: string) => Result<T, E>`
    Function {
        params: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
    },
    /// Array type: `Array<T>`
    Array(Box<TypeExpr>),
    /// Tuple type: `[string, number]`
    Tuple(Vec<TypeExpr>),
}

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // -- Literals --
    /// Number literal: `42`, `3.14`
    Number(String),
    /// String literal: `"hello"`
    String(String),
    /// Template literal: `` `hello ${name}` ``
    TemplateLiteral(Vec<TemplatePart>),
    /// Boolean literal: `true`, `false`
    Bool(bool),

    // -- Identifiers --
    /// Variable/function reference: `x`, `myFunc`
    Identifier(String),
    /// Placeholder for partial application: `_`
    Placeholder,

    // -- Operators --
    /// Binary operation: `a + b`, `a == b`, `a && b`
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// Unary operation: `!x`, `-x`
    Unary { op: UnaryOp, operand: Box<Expr> },
    /// Pipe: `a |> f(b)`
    Pipe { left: Box<Expr>, right: Box<Expr> },
    /// Unwrap: `expr?`
    Unwrap(Box<Expr>),

    // -- Calls & Construction --
    /// Function call: `f(a, b, name: c)` or `f<T>(a, b)`
    Call {
        callee: Box<Expr>,
        type_args: Vec<TypeExpr>,
        args: Vec<Arg>,
    },
    /// Type constructor: `User(name: "Ryan", email: e)` or `User(..existing, name: "New")`
    Construct {
        type_name: String,
        spread: Option<Box<Expr>>,
        args: Vec<Arg>,
    },
    /// Member access: `a.b`
    Member { object: Box<Expr>, field: String },
    /// Index access: `a[0]`
    Index { object: Box<Expr>, index: Box<Expr> },

    // -- Functions --
    /// Arrow function: `(a, b) => a + b`
    Arrow { params: Vec<Param>, body: Box<Expr> },

    // -- Control flow --
    /// Match expression: `match x { Pat -> expr, ... }`
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// Return: `return expr`
    Return(Option<Box<Expr>>),
    /// Await: `await expr`
    Await(Box<Expr>),
    /// Try: `try expr` — wraps a throwing expression in Result
    Try(Box<Expr>),

    // -- Built-in constructors --
    /// `Ok(expr)`
    Ok(Box<Expr>),
    /// `Err(expr)`
    Err(Box<Expr>),
    /// `Some(expr)`
    Some(Box<Expr>),
    /// `None`
    None,
    /// `todo` — placeholder that panics at runtime, type `never`
    Todo,
    /// `unreachable` — asserts unreachable code path, type `never`
    Unreachable,
    /// Unit value: `()`
    Unit,

    // -- JSX --
    /// JSX element: `<Component prop={value}>children</Component>`
    Jsx(JsxElement),

    // -- Blocks --
    /// Block expression: `{ stmt1; stmt2; expr }`
    Block(Vec<Item>),

    // -- Grouping --
    /// Parenthesized expression: `(a + b)`
    Grouped(Box<Expr>),

    // -- Array --
    /// Array literal: `[1, 2, 3]`
    Array(Vec<Expr>),

    // -- Spread --
    /// Spread: `...expr`
    Spread(Box<Expr>),

    // -- Dot shorthand --
    /// Dot shorthand: `.field` or `.field op expr` — creates an implicit lambda
    DotShorthand {
        /// The field name (e.g., `done` in `.done`)
        field: String,
        /// Optional operator and right-hand side (e.g., `== false` in `.done == false`)
        predicate: Option<(BinOp, Box<Expr>)>,
    },
}

/// Template literal parts for the AST.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    /// Raw string segment.
    Raw(String),
    /// Interpolated expression.
    Expr(Expr),
}

// ── Arguments ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// Positional argument: `expr`
    Positional(Expr),
    /// Named argument: `name: expr`
    Named { label: String, value: Expr },
}

// ── Operators ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ── Match ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// Literal pattern: `42`, `"hello"`, `true`
    Literal(LiteralPattern),
    /// Range pattern: `1..10`
    Range {
        start: LiteralPattern,
        end: LiteralPattern,
    },
    /// Variant/constructor pattern: `Ok(x)`, `Network(Timeout(ms))`
    Variant { name: String, fields: Vec<Pattern> },
    /// Record destructuring pattern: `{ x, y }` or `{ ctrl: true }`
    Record { fields: Vec<(String, Pattern)> },
    /// String pattern with captures: `"/users/{id}"` or `"/users/{id}/posts"`
    StringPattern {
        /// The segments of the string pattern (literal parts and capture names)
        segments: Vec<StringPatternSegment>,
    },
    /// Binding pattern (identifier): `x`, `msg`
    Binding(String),
    /// Wildcard pattern: `_`
    Wildcard,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiteralPattern {
    Number(String),
    String(String),
    Bool(bool),
}

/// A segment in a string pattern — either a literal part or a capture variable.
#[derive(Debug, Clone, PartialEq)]
pub enum StringPatternSegment {
    /// A literal string segment: `"/users/"` in `"/users/{id}"`
    Literal(String),
    /// A capture variable: `id` in `"/users/{id}"`
    Capture(String),
}

/// Parse a string value for `{name}` capture segments.
/// Returns `Some(segments)` if the string contains at least one capture,
/// or `None` if it's a plain string literal (no captures).
pub fn parse_string_pattern_segments(s: &str) -> Option<Vec<StringPatternSegment>> {
    // Quick check: does the string contain any `{...}` patterns?
    if !s.contains('{') {
        return None;
    }

    let mut segments = Vec::new();
    let mut current_literal = String::new();
    let mut chars = s.chars().peekable();
    let mut has_capture = false;

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Collect the capture name
            let mut name = String::new();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                name.push(ch);
            }

            // Validate: capture name must be a valid identifier (non-empty, alphanumeric + _)
            if !name.is_empty()
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                && name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            {
                // Push any preceding literal
                if !current_literal.is_empty() {
                    segments.push(StringPatternSegment::Literal(std::mem::take(
                        &mut current_literal,
                    )));
                }
                segments.push(StringPatternSegment::Capture(name));
                has_capture = true;
            } else {
                // Not a valid capture — treat as literal text
                current_literal.push('{');
                current_literal.push_str(&name);
                current_literal.push('}');
            }
        } else {
            current_literal.push(ch);
        }
    }

    if !current_literal.is_empty() {
        segments.push(StringPatternSegment::Literal(current_literal));
    }

    if has_capture { Some(segments) } else { None }
}

// ── JSX ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct JsxElement {
    pub kind: JsxElementKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxElementKind {
    /// `<Tag props...>children</Tag>` or `<Tag props... />`
    Element {
        name: String,
        props: Vec<JsxProp>,
        children: Vec<JsxChild>,
        self_closing: bool,
    },
    /// `<>children</>`
    Fragment { children: Vec<JsxChild> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsxProp {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JsxChild {
    /// Raw text between tags
    Text(String),
    /// `{expression}`
    Expr(Expr),
    /// Nested JSX element
    Element(JsxElement),
}
