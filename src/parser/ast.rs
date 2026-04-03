use std::cell::Cell;

use crate::lexer::span::Span;

// ── ExprId ──────────────────────────────────────────────────────

/// A unique identifier for every `Expr` node in the AST.
/// Assigned during CST-to-AST lowering and used as a stable key
/// for the checker → codegen type map (replacing span-based keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

impl ExprId {
    /// Sentinel ID for synthetic expressions created by codegen.
    /// These are never looked up in the type map.
    pub const SYNTHETIC: Self = Self(u32::MAX);
}

/// Generator for unique `ExprId` values.
pub struct ExprIdGen(Cell<u32>);

impl ExprIdGen {
    pub fn new() -> Self {
        Self(Cell::new(0))
    }

    pub fn next(&self) -> ExprId {
        let id = self.0.get();
        self.0.set(id + 1);
        ExprId(id)
    }
}

impl Default for ExprIdGen {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// `trait Name { fn ... }` — trait declaration
    TraitDecl(TraitDecl),
    /// `test "name" { assert expr ... }` — inline test block
    TestBlock(TestBlock),
    /// Expression statement (for REPL / scripts)
    Expr(Expr),
}

// ── Imports ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// Whether the entire import is trusted: `import trusted { ... } from "..."`
    pub trusted: bool,
    /// Default import name: `import Markdown from "react-markdown"`
    pub default_import: Option<String>,
    pub specifiers: Vec<ImportSpecifier>,
    /// For-import specifiers: `import { for User, for Array } from "..."`
    pub for_specifiers: Vec<ForImportSpecifier>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportSpecifier {
    pub name: String,
    pub alias: Option<String>,
    /// Whether this specific import is trusted: `import { trusted capitalize } from "..."`
    pub trusted: bool,
    pub span: Span,
}

/// `for Type` specifier in an import: `import { for User } from "./helpers"`
#[derive(Debug, Clone, PartialEq)]
pub struct ForImportSpecifier {
    /// The type name (base type only, no type params): e.g., "User", "Array"
    pub type_name: String,
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

/// A field in an object destructuring pattern, optionally renamed.
/// `{ data }` → field="data", alias=None
/// `{ data: rows }` → field="data", alias=Some("rows")
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDestructureField {
    pub field: String,
    pub alias: Option<String>,
}

impl ObjectDestructureField {
    /// The name this field is bound to in scope — alias if present, otherwise the field name.
    pub fn bound_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.field)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstBinding {
    /// Simple name: `const x = ...`
    Name(String),
    /// Array destructuring: `const [a, b] = ...`
    Array(Vec<String>),
    /// Object destructuring: `const { a, b } = ...` or `const { a: x, b: y } = ...`
    Object(Vec<ObjectDestructureField>),
    /// Tuple destructuring: `const (a, b) = ...`
    Tuple(Vec<String>),
}

impl ConstBinding {
    /// A `_`-joined name for identification purposes (probe keys, etc.).
    pub fn binding_name(&self) -> String {
        match self {
            ConstBinding::Name(name) => name.clone(),
            ConstBinding::Array(names) | ConstBinding::Tuple(names) => names.join("_"),
            ConstBinding::Object(fields) => fields
                .iter()
                .map(|f| f.bound_name())
                .collect::<Vec<_>>()
                .join("_"),
        }
    }
}

// ── Function Declaration ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub exported: bool,
    pub async_fn: bool,
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeExpr>,
    pub default: Option<Expr>,
    /// Destructuring pattern for this parameter: `|{ x, y }| ...`
    /// When present, `name` is a generated identifier and this holds the field names.
    pub destructure: Option<ParamDestructure>,
    pub span: Span,
}

/// Destructuring pattern for a function/lambda parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamDestructure {
    /// Object destructuring: `{ field1, field2 }` or `{ field1: alias1 }`
    Object(Vec<ObjectDestructureField>),
    /// Array destructuring: `[a, b]`
    Array(Vec<String>),
}

// ── Type Declarations ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub exported: bool,
    pub opaque: bool,
    pub name: String,
    pub type_params: Vec<String>,
    pub def: TypeDef,
    /// `deriving (Display)` — auto-derive trait implementations for record types.
    pub deriving: Vec<String>,
}

/// The right-hand side of a type declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeDef {
    /// Record type: `{ field: Type, ...OtherType, ... }`
    Record(Vec<RecordEntry>),
    /// Union type: `| Variant1 | Variant2(field: Type)`
    Union(Vec<Variant>),
    /// Type alias: `type X = SomeOtherType`
    Alias(TypeExpr),
    /// String literal union: `"GET" | "POST" | "PUT" | "DELETE"`
    StringLiteralUnion(Vec<String>),
}

/// An entry inside a record type definition — either a regular field or a spread.
#[derive(Debug, Clone, PartialEq)]
pub enum RecordEntry {
    /// A regular field: `name: Type`
    Field(Box<RecordField>),
    /// A spread: `...OtherType` — includes all fields from the referenced record type
    Spread(RecordSpread),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub type_ann: TypeExpr,
    pub default: Option<Expr>,
    pub span: Span,
}

/// A spread entry in a record type: `...TypeName` or `...Generic<T>`
#[derive(Debug, Clone, PartialEq)]
pub struct RecordSpread {
    pub type_name: String,
    pub type_expr: Option<TypeExpr>,
    pub span: Span,
}

impl RecordEntry {
    /// Returns the field if this is a `RecordEntry::Field`, otherwise `None`.
    pub fn as_field(&self) -> Option<&RecordField> {
        match self {
            RecordEntry::Field(f) => Some(f),
            RecordEntry::Spread(_) => None,
        }
    }

    /// Returns the spread if this is a `RecordEntry::Spread`, otherwise `None`.
    pub fn as_spread(&self) -> Option<&RecordSpread> {
        match self {
            RecordEntry::Spread(s) => Some(s),
            RecordEntry::Field(_) => None,
        }
    }
}

impl TypeDef {
    /// Returns only the direct fields (excluding spreads) from a record type definition.
    pub fn record_fields(&self) -> Vec<&RecordField> {
        match self {
            TypeDef::Record(entries) => entries.iter().filter_map(RecordEntry::as_field).collect(),
            _ => Vec::new(),
        }
    }

    /// Returns the spread entries from a record type definition.
    pub fn record_spreads(&self) -> Vec<&RecordSpread> {
        match self {
            TypeDef::Record(entries) => entries.iter().filter_map(RecordEntry::as_spread).collect(),
            _ => Vec::new(),
        }
    }
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

// ── Trait Declarations ──────────────────────────────────────────

/// `trait Name { fn method(self) -> T ... }` — trait declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub exported: bool,
    pub name: String,
    /// Methods declared in the trait (signatures and optional default bodies).
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

/// A method in a trait declaration. May have a default body.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    /// If Some, this is a default implementation.
    pub body: Option<Expr>,
    pub span: Span,
}

// ── For Blocks ──────────────────────────────────────────────────

/// `for Type { fn f(self) -> T { ... } }` — group functions under a type.
/// `for Type: Trait { fn f(self) -> T { ... } }` — implement a trait for a type.
#[derive(Debug, Clone, PartialEq)]
pub struct ForBlock {
    pub type_name: TypeExpr,
    /// Optional trait bound: `for User: Display { ... }`
    pub trait_name: Option<String>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

// ── Test Blocks ─────────────────────────────────────────────────

/// `test "name" { assert expr ... }` — inline test block.
#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub name: String,
    pub body: Vec<TestStatement>,
    pub span: Span,
}

/// A statement inside a test block.
#[derive(Debug, Clone, PartialEq)]
pub enum TestStatement {
    /// `assert expr` — asserts that the expression is truthy
    Assert(Expr, Span),
    /// A regular expression statement (e.g., const bindings, function calls)
    Expr(Expr),
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
        /// Trait bounds on this type parameter: `T: Display + Eq`
        bounds: Vec<String>,
    },
    /// Record type inline: `{ name: string, age: number }`
    Record(Vec<RecordField>),
    /// Function type: `(a: number, b: string) -> Result<T, E>`
    Function {
        params: Vec<TypeExpr>,
        return_type: Box<TypeExpr>,
    },
    /// Array type: `Array<T>`
    Array(Box<TypeExpr>),
    /// Tuple type: `[string, number]`
    Tuple(Vec<TypeExpr>),
    /// `typeof <ident>` — extract the type of a value binding
    TypeOf(String),
    /// `A & B` — intersection type
    Intersection(Vec<TypeExpr>),
    /// String literal type: `"div"`, `"button"` (for npm interop like `ComponentProps<"div">`)
    StringLiteral(String),
}

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub id: ExprId,
    pub kind: ExprKind,
    /// Resolved type — `Unknown` before type-checking, filled in after.
    pub ty: crate::checker::Type,
    pub span: Span,
}

impl Expr {
    /// Create a synthetic `Expr` for codegen-internal use (not from source).
    /// Uses a sentinel ID — these are never looked up in the type map.
    pub fn synthetic(kind: ExprKind, span: Span) -> Self {
        Self {
            id: ExprId::SYNTHETIC,
            kind,
            ty: crate::checker::Type::Unknown,
            span,
        }
    }
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
    Arrow {
        async_fn: bool,
        params: Vec<Param>,
        body: Box<Expr>,
    },

    // -- Control flow --
    /// Match expression: `match x { Pat -> expr, ... }`
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    // -- Built-in constructors --
    /// `Value(expr)` — Settable value present
    Value(Box<Expr>),
    /// `Clear` — Settable value explicitly null
    Clear,
    /// `Unchanged` — Settable value omitted
    Unchanged,
    /// `parse<T>(value)` — compiler built-in for runtime type validation
    Parse {
        type_arg: TypeExpr,
        value: Box<Expr>,
    },
    /// `mock<T>` — compiler built-in for auto-generating test data from types
    /// Optional overrides: `mock<User>(name: "Alice")`
    Mock {
        type_arg: TypeExpr,
        overrides: Vec<Arg>,
    },
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
    /// Collect block: `collect { ... }` — accumulates errors from `?` instead of short-circuiting
    Collect(Vec<Item>),

    // -- Grouping --
    /// Parenthesized expression: `(a + b)`
    Grouped(Box<Expr>),

    // -- Array --
    /// Array literal: `[1, 2, 3]`
    Array(Vec<Expr>),

    /// Object literal: `{ name: "Alice", age: 30 }`
    /// Fields are (key, value) pairs. Shorthand `{ name }` desugars to `{ name: name }`.
    Object(Vec<(String, Expr)>),

    /// Tuple literal: `(1, 2)`, `("key", 42, true)`
    Tuple(Vec<Expr>),

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
    pub guard: Option<Expr>,
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
    /// Tuple pattern: `(x, y)`, `(_, 0)`
    Tuple(Vec<Pattern>),
    /// Array pattern: `[]`, `[a]`, `[a, b]`, `[first, ..rest]`
    Array {
        /// Fixed element patterns (before any rest pattern)
        elements: Vec<Pattern>,
        /// Optional rest binding: `..rest` captures the remaining tail
        rest: Option<String>,
    },
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
pub enum JsxProp {
    /// `name={value}` or `name="string"`
    Named {
        name: String,
        value: Option<Expr>,
        span: Span,
    },
    /// `{...expr}`
    Spread { expr: Expr, span: Span },
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
