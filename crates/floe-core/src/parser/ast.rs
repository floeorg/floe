use std::cell::Cell;

use crate::lexer::span::Span;

// ── ExprId ──────────────────────────────────────────────────────

/// A unique identifier for every `Expr` node in the AST.
/// Assigned during CST-to-AST lowering and used as a stable key
/// for the checker → codegen type map (replacing span-based keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

// ── Phase type aliases ──────────────────────────────────────────
//
// The AST is parametrized over a phase type `T` that represents the
// resolved type attached to each `Expr` node. Before type-checking,
// `T = ()` (zero-sized, no runtime cost) — this is `UntypedExpr`.
// After type-checking, `T = std::sync::Arc<crate::checker::Type>` —
// this is `TypedExpr`. Because the phase flows through every struct
// that transitively holds an `Expr`, codegen's signature (`&TypedExpr`)
// makes it structurally impossible to emit code for an unchecked tree.
//
// Pragmatically, parameter defaults keep existing code compiling
// unchanged: a plain `Expr` still means `Expr<()>`, and migration from
// untyped to typed happens one function boundary at a time.

/// An untyped expression tree (the output of `lower.rs`).
pub type UntypedExpr = Expr<()>;

/// An untyped program (the output of `lower.rs`).
pub type UntypedProgram = Program<()>;

/// A typed expression tree (the output of the checker).
pub type TypedExpr = Expr<std::sync::Arc<crate::checker::Type>>;

/// A typed program (the output of the checker, fed to codegen).
pub type TypedProgram = Program<std::sync::Arc<crate::checker::Type>>;

/// Aliases for every AST node in its typed form. Used throughout
/// codegen and desugar so signatures don't need
/// `&Thing<std::sync::Arc<crate::checker::Type>>` everywhere.
pub type TypedItem = Item<std::sync::Arc<crate::checker::Type>>;
pub type TypedItemKind = ItemKind<std::sync::Arc<crate::checker::Type>>;
pub type TypedExprKind = ExprKind<std::sync::Arc<crate::checker::Type>>;
pub type TypedConstDecl = ConstDecl<std::sync::Arc<crate::checker::Type>>;
pub type TypedFunctionDecl = FunctionDecl<std::sync::Arc<crate::checker::Type>>;
pub type TypedParam = Param<std::sync::Arc<crate::checker::Type>>;
pub type TypedTypeDecl = TypeDecl<std::sync::Arc<crate::checker::Type>>;
pub type TypedTypeDef = TypeDef<std::sync::Arc<crate::checker::Type>>;
pub type TypedTypeExpr = TypeExpr<std::sync::Arc<crate::checker::Type>>;
pub type TypedTypeExprKind = TypeExprKind<std::sync::Arc<crate::checker::Type>>;
pub type TypedRecordEntry = RecordEntry<std::sync::Arc<crate::checker::Type>>;
pub type TypedRecordField = RecordField<std::sync::Arc<crate::checker::Type>>;
pub type TypedRecordSpread = RecordSpread<std::sync::Arc<crate::checker::Type>>;
pub type TypedVariant = Variant<std::sync::Arc<crate::checker::Type>>;
pub type TypedVariantField = VariantField<std::sync::Arc<crate::checker::Type>>;
pub type TypedTraitDecl = TraitDecl<std::sync::Arc<crate::checker::Type>>;
pub type TypedTraitMethod = TraitMethod<std::sync::Arc<crate::checker::Type>>;
pub type TypedForBlock = ForBlock<std::sync::Arc<crate::checker::Type>>;
pub type TypedTestBlock = TestBlock<std::sync::Arc<crate::checker::Type>>;
pub type TypedTestStatement = TestStatement<std::sync::Arc<crate::checker::Type>>;
pub type TypedMatchArm = MatchArm<std::sync::Arc<crate::checker::Type>>;
pub type TypedArg = Arg<std::sync::Arc<crate::checker::Type>>;
pub type TypedTemplatePart = TemplatePart<std::sync::Arc<crate::checker::Type>>;
pub type TypedJsxElement = JsxElement<std::sync::Arc<crate::checker::Type>>;
pub type TypedJsxElementKind = JsxElementKind<std::sync::Arc<crate::checker::Type>>;
pub type TypedJsxProp = JsxProp<std::sync::Arc<crate::checker::Type>>;
pub type TypedJsxChild = JsxChild<std::sync::Arc<crate::checker::Type>>;

/// A complete Floe source file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Program<T = ()> {
    pub items: Vec<Item<T>>,
    pub span: Span,
}

/// Top-level items in a Floe file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item<T = ()> {
    pub kind: ItemKind<T>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ItemKind<T = ()> {
    /// `import { x, y } from "module"`
    Import(ImportDecl),
    /// `export { x, y } from "module"` — re-export without importing into scope
    ReExport(ReExportDecl),
    /// `const x = expr` or `export const x = expr`
    Const(ConstDecl<T>),
    /// `function f(...) { ... }` or `export function f(...) { ... }`
    Function(FunctionDecl<T>),
    /// `type T = ...` or `export type T = ...`
    TypeDecl(TypeDecl<T>),
    /// `for Type { fn ... }` — group functions under a type
    ForBlock(ForBlock<T>),
    /// `trait Name { fn ... }` — trait declaration
    TraitDecl(TraitDecl<T>),
    /// `test "name" { assert expr ... }` — inline test block
    TestBlock(TestBlock<T>),
    /// Expression statement (for REPL / scripts)
    Expr(Expr<T>),
}

// ── Imports ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImportSpecifier {
    pub name: String,
    pub alias: Option<String>,
    /// Whether this specific import is trusted: `import { trusted capitalize } from "..."`
    pub trusted: bool,
    pub span: Span,
}

/// `for Type` specifier in an import: `import { for User } from "./helpers"`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ForImportSpecifier {
    /// The type name (base type only, no type params): e.g., "User", "Array"
    pub type_name: String,
    pub span: Span,
}

// ── Re-export Declaration ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReExportDecl {
    pub specifiers: Vec<ReExportSpecifier>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReExportSpecifier {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

// ── Const Declaration ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConstDecl<T = ()> {
    pub exported: bool,
    pub binding: ConstBinding,
    pub type_ann: Option<TypeExpr<T>>,
    pub value: Expr<T>,
}

/// A field in an object destructuring pattern, optionally renamed.
/// `{ data }` → field="data", alias=None
/// `{ data: rows }` → field="data", alias=Some("rows")
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConstBinding {
    /// Simple name: `const x = ...`
    Name(String),
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
            ConstBinding::Tuple(names) => names.join("_"),
            ConstBinding::Object(fields) => fields
                .iter()
                .map(|f| f.bound_name())
                .collect::<Vec<_>>()
                .join("_"),
        }
    }
}

// ── Function Declaration ─────────────────────────────────────────

/// A generic type parameter with optional trait bounds: `R: SnippetRepository`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeParam {
    pub name: String,
    /// Trait names this parameter must implement (e.g. `["SnippetRepository"]`).
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionDecl<T = ()> {
    pub exported: bool,
    pub async_fn: bool,
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param<T>>,
    pub return_type: Option<TypeExpr<T>>,
    pub body: Box<Expr<T>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Param<T = ()> {
    pub name: String,
    pub type_ann: Option<TypeExpr<T>>,
    pub default: Option<Expr<T>>,
    /// Destructuring pattern for this parameter: `|{ x, y }| ...`
    /// When present, `name` is a generated identifier and this holds the field names.
    pub destructure: Option<ParamDestructure>,
    pub span: Span,
}

/// Returns `true` if the first element of `params` is named `self`.
pub fn params_have_self<T>(params: &[Param<T>]) -> bool {
    params.first().is_some_and(|p| p.name == "self")
}

/// Destructuring pattern for a function/lambda parameter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ParamDestructure {
    /// Object destructuring: `{ field1, field2 }` or `{ field1: alias1 }`
    Object(Vec<ObjectDestructureField>),
    /// Array destructuring: `[a, b]`
    Array(Vec<String>),
}

// ── Type Declarations ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeDecl<T = ()> {
    pub exported: bool,
    pub opaque: bool,
    pub name: String,
    pub type_params: Vec<String>,
    pub def: TypeDef<T>,
    /// `deriving (Display)` — auto-derive trait implementations for record types.
    pub deriving: Vec<String>,
}

/// The right-hand side of a type declaration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeDef<T = ()> {
    /// Record type: `{ field: Type, ...OtherType, ... }`
    Record(Vec<RecordEntry<T>>),
    /// Union type: `| Unit | Positional(Type) | Named { field: Type }`
    Union(Vec<Variant<T>>),
    /// Type alias: `type X = SomeOtherType`
    Alias(TypeExpr<T>),
    /// String literal union: `"GET" | "POST" | "PUT" | "DELETE"`
    StringLiteralUnion(Vec<String>),
}

/// An entry inside a record type definition — either a regular field or a spread.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RecordEntry<T = ()> {
    /// A regular field: `name: Type`
    Field(Box<RecordField<T>>),
    /// A spread: `...OtherType` — includes all fields from the referenced record type
    Spread(RecordSpread<T>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecordField<T = ()> {
    pub name: String,
    pub type_ann: TypeExpr<T>,
    pub default: Option<Expr<T>>,
    pub span: Span,
}

/// A spread entry in a record type: `...TypeName` or `...Generic<T>`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecordSpread<T = ()> {
    pub type_name: String,
    pub type_expr: Option<TypeExpr<T>>,
    pub span: Span,
}

impl<T> RecordEntry<T> {
    /// Returns the field if this is a `RecordEntry::Field`, otherwise `None`.
    pub fn as_field(&self) -> Option<&RecordField<T>> {
        match self {
            RecordEntry::Field(f) => Some(f),
            RecordEntry::Spread(_) => None,
        }
    }

    /// Returns the spread if this is a `RecordEntry::Spread`, otherwise `None`.
    pub fn as_spread(&self) -> Option<&RecordSpread<T>> {
        match self {
            RecordEntry::Spread(s) => Some(s),
            RecordEntry::Field(_) => None,
        }
    }
}

impl<T> TypeDef<T> {
    /// Returns only the direct fields (excluding spreads) from a record type definition.
    pub fn record_fields(&self) -> Vec<&RecordField<T>> {
        match self {
            TypeDef::Record(entries) => entries.iter().filter_map(RecordEntry::as_field).collect(),
            _ => Vec::new(),
        }
    }

    /// Returns the spread entries from a record type definition.
    pub fn record_spreads(&self) -> Vec<&RecordSpread<T>> {
        match self {
            TypeDef::Record(entries) => entries.iter().filter_map(RecordEntry::as_spread).collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Variant<T = ()> {
    pub name: String,
    pub fields: Vec<VariantField<T>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariantField<T = ()> {
    pub name: Option<String>,
    pub type_ann: TypeExpr<T>,
    pub span: Span,
}

// ── Trait Declarations ──────────────────────────────────────────

/// `trait Name { fn method(self) -> T ... }` — trait declaration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TraitDecl<T = ()> {
    pub exported: bool,
    pub name: String,
    /// Methods declared in the trait (signatures and optional default bodies).
    pub methods: Vec<TraitMethod<T>>,
    pub span: Span,
}

/// A method in a trait declaration. May have a default body.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TraitMethod<T = ()> {
    pub name: String,
    pub params: Vec<Param<T>>,
    pub return_type: Option<TypeExpr<T>>,
    /// If Some, this is a default implementation.
    pub body: Option<Expr<T>>,
    pub span: Span,
}

// ── For Blocks ──────────────────────────────────────────────────

/// `for Type { fn f(self) -> T { ... } }` — group functions under a type.
/// `for Type: Trait { fn f(self) -> T { ... } }` — implement a trait for a type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ForBlock<T = ()> {
    pub type_name: TypeExpr<T>,
    /// Optional trait bound: `for User: Display { ... }`
    pub trait_name: Option<String>,
    pub functions: Vec<FunctionDecl<T>>,
    pub span: Span,
}

// ── Test Blocks ─────────────────────────────────────────────────

/// `test "name" { assert expr ... }` — inline test block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TestBlock<T = ()> {
    pub name: String,
    pub body: Vec<TestStatement<T>>,
    pub span: Span,
}

/// A statement inside a test block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TestStatement<T = ()> {
    /// `assert expr` — asserts that the expression is truthy
    Assert(Expr<T>, Span),
    /// `let NAME = expr` — a local binding within the test
    Let(ConstDecl<T>),
    /// A regular expression statement (e.g., function calls)
    Expr(Expr<T>),
}

// ── Type Expressions ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeExpr<T = ()> {
    pub kind: TypeExprKind<T>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeExprKind<T = ()> {
    /// A named type: `string`, `number`, `User`, `Option<T>`
    Named {
        name: String,
        type_args: Vec<TypeExpr<T>>,
        /// Trait bounds on this type parameter: `T: Display + Eq`
        bounds: Vec<String>,
    },
    /// Record type inline: `{ name: string, age: number }`
    Record(Vec<RecordField<T>>),
    /// Function type: `(a: number, b: string) -> Result<T, E>`
    ///
    /// Each parameter carries an optional label. Labels are required in
    /// top-level type aliases (`type F = (x: T) -> U`) and on function-typed
    /// record fields, and remain optional inside higher-order parameter
    /// positions like `fn map(xs: Array<A>, f: (A) -> B)`. Labels are
    /// documentation only and never affect structural assignability.
    Function {
        params: Vec<FnTypeParam<T>>,
        return_type: Box<TypeExpr<T>>,
    },
    /// Array type: `Array<T>`
    Array(Box<TypeExpr<T>>),
    /// Tuple type: `[string, number]`
    Tuple(Vec<TypeExpr<T>>),
    /// `typeof <ident>` — extract the type of a value binding
    TypeOf(String),
    /// `A & B` — intersection type
    Intersection(Vec<TypeExpr<T>>),
    /// String literal type: `"div"`, `"button"` (for npm interop like `ComponentProps<"div">`)
    StringLiteral(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FnTypeParam<T = ()> {
    pub label: Option<String>,
    pub type_ann: TypeExpr<T>,
    pub span: Span,
}

// ── Expressions ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Expr<T = ()> {
    pub id: ExprId,
    pub kind: ExprKind<T>,
    /// Resolved type — unit `()` before type-checking (zero-sized),
    /// `Arc<Type>` after checking. Codegen takes `Expr<Arc<Type>>` and
    /// therefore cannot be called on an unchecked tree.
    pub ty: T,
    pub span: Span,
}

impl Expr<()> {
    /// Create a synthetic untyped `Expr` for codegen-internal use.
    /// Uses a sentinel ID — these are never looked up in the type map.
    pub fn synthetic(kind: ExprKind<()>, span: Span) -> Self {
        Self {
            id: ExprId::SYNTHETIC,
            kind,
            ty: (),
            span,
        }
    }
}

impl Expr<std::sync::Arc<crate::checker::Type>> {
    /// Create a synthetic typed `Expr` for codegen-internal use. Uses a
    /// sentinel ID and `Type::Unknown` for the type — these nodes are
    /// never looked up in the type map and never feed type-directed
    /// decisions downstream.
    pub fn synthetic_typed(
        kind: ExprKind<std::sync::Arc<crate::checker::Type>>,
        span: Span,
    ) -> Self {
        Self {
            id: ExprId::SYNTHETIC,
            kind,
            ty: std::sync::Arc::new(crate::checker::Type::Unknown),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ExprKind<T = ()> {
    // -- Literals --
    /// Number literal: `42`, `3.14`
    Number(String),
    /// String literal: `"hello"`
    String(String),
    /// Template literal: `` `hello ${name}` ``
    TemplateLiteral(Vec<TemplatePart<T>>),
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
        left: Box<Expr<T>>,
        op: BinOp,
        right: Box<Expr<T>>,
    },
    /// Unary operation: `!x`, `-x`
    Unary { op: UnaryOp, operand: Box<Expr<T>> },
    /// Pipe: `a |> f(b)`
    Pipe {
        left: Box<Expr<T>>,
        right: Box<Expr<T>>,
    },
    /// Unwrap: `expr?`
    Unwrap(Box<Expr<T>>),

    // -- Calls & Construction --
    /// Function call: `f(a, b, name: c)` or `f<T>(a, b)`
    Call {
        callee: Box<Expr<T>>,
        type_args: Vec<TypeExpr<T>>,
        args: Vec<Arg<T>>,
    },
    /// Tagged template literal: `` tag`a ${x} b` ``
    /// Emits the same tagged-template form in TypeScript so the target
    /// library receives the expected `TemplateStringsArray`.
    TaggedTemplate {
        tag: Box<Expr<T>>,
        parts: Vec<TemplatePart<T>>,
    },
    /// Type constructor: `User(name: "Ryan", email: e)` or `User(..existing, name: "New")`
    Construct {
        type_name: String,
        spread: Option<Box<Expr<T>>>,
        args: Vec<Arg<T>>,
    },
    /// Member access: `a.b`
    Member { object: Box<Expr<T>>, field: String },
    /// Index access: `a[0]`
    Index {
        object: Box<Expr<T>>,
        index: Box<Expr<T>>,
    },

    // -- Functions --
    /// Arrow function: `(a, b) => a + b`
    Arrow {
        async_fn: bool,
        params: Vec<Param<T>>,
        body: Box<Expr<T>>,
    },

    // -- Control flow --
    /// Match expression: `match x { Pat -> expr, ... }`
    Match {
        subject: Box<Expr<T>>,
        arms: Vec<MatchArm<T>>,
    },
    // -- Built-in constructors --
    /// `Value(expr)` — Settable value present
    Value(Box<Expr<T>>),
    /// `Clear` — Settable value explicitly null
    Clear,
    /// `Unchanged` — Settable value omitted
    Unchanged,
    /// `parse<T>(value)` — compiler built-in for runtime type validation
    Parse {
        type_arg: TypeExpr<T>,
        value: Box<Expr<T>>,
    },
    /// `mock<T>` — compiler built-in for auto-generating test data from types
    /// Optional overrides: `mock<User>(name: "Alice")`
    Mock {
        type_arg: TypeExpr<T>,
        overrides: Vec<Arg<T>>,
    },
    /// `todo` — placeholder that panics at runtime, type `never`
    Todo,
    /// `unreachable` — asserts unreachable code path, type `never`
    Unreachable,
    /// Unit value: `()`
    Unit,

    // -- JSX --
    /// JSX element: `<Component prop={value}>children</Component>`
    Jsx(JsxElement<T>),

    // -- Blocks --
    /// Block expression: `{ stmt1; stmt2; expr }`
    Block(Vec<Item<T>>),
    /// Collect block: `collect { ... }` — accumulates errors from `?` instead of short-circuiting
    Collect(Vec<Item<T>>),

    // -- Grouping --
    /// Parenthesized expression: `(a + b)`
    Grouped(Box<Expr<T>>),

    // -- Array --
    /// Array literal: `[1, 2, 3]`
    Array(Vec<Expr<T>>),

    /// Object literal: `{ name: "Alice", age: 30 }`
    /// Fields are (key, value) pairs. Shorthand `{ name }` desugars to `{ name: name }`.
    Object(Vec<(String, Expr<T>)>),

    /// Tuple literal: `(1, 2)`, `("key", 42, true)`
    Tuple(Vec<Expr<T>>),

    // -- Spread --
    /// Spread: `...expr`
    Spread(Box<Expr<T>>),

    // -- Dot shorthand --
    /// Dot shorthand: `.field` or `.field op expr` — creates an implicit lambda
    DotShorthand {
        /// The field name (e.g., `done` in `.done`)
        field: String,
        /// Optional operator and right-hand side (e.g., `== false` in `.done == false`)
        predicate: Option<(BinOp, Box<Expr<T>>)>,
    },

    // -- Error recovery --
    /// A subtree whose type checking failed. The error has already been
    /// reported via `Problems`; this node exists so codegen and
    /// downstream passes can skip the broken subtree without crashing.
    /// Only constructed by `attach_types` when the expression was
    /// flagged as invalid during checking.
    Invalid,
}

/// Template literal parts for the AST.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TemplatePart<T = ()> {
    /// Raw string segment.
    Raw(String),
    /// Interpolated expression.
    Expr(Expr<T>),
}

// ── Arguments ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Arg<T = ()> {
    /// Positional argument: `expr`
    Positional(Expr<T>),
    /// Named argument: `name: expr`
    Named { label: String, value: Expr<T> },
}

// ── Operators ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ── Match ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MatchArm<T = ()> {
    pub pattern: Pattern,
    pub guard: Option<Expr<T>>,
    pub body: Expr<T>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PatternKind {
    /// Literal pattern: `42`, `"hello"`, `true`
    Literal(LiteralPattern),
    /// Range pattern: `1..10`
    Range {
        start: LiteralPattern,
        end: LiteralPattern,
    },
    /// Variant/constructor pattern: `Ok(x)`, `Rectangle { width, height: h }`.
    /// The field list is either positional (parens) or named (braces) — it
    /// must match the shape of the variant's declaration.
    Variant {
        name: String,
        fields: VariantPatternFields,
    },
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

/// Field list shape for a variant pattern — mirrors the declaration.
/// Parser + checker enforce that `Positional` patterns only match variants
/// declared as `Variant(Type, ...)` and `Named` patterns only match variants
/// declared as `Variant { field: Type, ... }`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VariantPatternFields {
    /// `Ok(x)`, `Rect(w, h)` — positional field patterns.
    Positional(Vec<Pattern>),
    /// `Rectangle { width, height: h }` — named field patterns. Each entry is
    /// `(field_name, nested_pattern)`. Shorthand like `{ width }` lowers to
    /// `("width", PatternKind::Binding("width"))`.
    Named(Vec<(String, Pattern)>),
}

impl VariantPatternFields {
    /// Total number of fields in the pattern, regardless of shape.
    pub fn len(&self) -> usize {
        match self {
            VariantPatternFields::Positional(pats) => pats.len(),
            VariantPatternFields::Named(fields) => fields.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate over the nested patterns without the field name.
    pub fn patterns(&self) -> impl Iterator<Item = &Pattern> {
        VariantPatternIter::Patterns(self.entries())
    }

    /// Iterate over `(index, Option<field_name>, pattern)` entries. `Some` name
    /// is yielded for named-shape patterns, `None` for positional. Used by
    /// codegen to pick the runtime field accessor.
    pub fn entries(&self) -> VariantPatternEntries<'_> {
        match self {
            VariantPatternFields::Positional(pats) => VariantPatternEntries::Positional {
                inner: pats.iter().enumerate(),
            },
            VariantPatternFields::Named(fields) => VariantPatternEntries::Named {
                inner: fields.iter().enumerate(),
            },
        }
    }

    /// Human-readable name of the shape — for error messages.
    pub fn shape_name(&self) -> &'static str {
        match self {
            VariantPatternFields::Positional(_) => "positional",
            VariantPatternFields::Named(_) => "named",
        }
    }
}

/// Enum-based iterator over variant pattern fields. Avoids the heap
/// allocation a `Box<dyn Iterator>` would incur in codegen / LSP hot paths.
pub enum VariantPatternEntries<'a> {
    Positional {
        inner: std::iter::Enumerate<std::slice::Iter<'a, Pattern>>,
    },
    Named {
        inner: std::iter::Enumerate<std::slice::Iter<'a, (String, Pattern)>>,
    },
}

impl<'a> Iterator for VariantPatternEntries<'a> {
    type Item = (usize, Option<&'a str>, &'a Pattern);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            VariantPatternEntries::Positional { inner } => inner.next().map(|(i, p)| (i, None, p)),
            VariantPatternEntries::Named { inner } => inner
                .next()
                .map(|(i, (name, p))| (i, Some(name.as_str()), p)),
        }
    }
}

/// Bare-pattern iterator that drops the index and field name. Thin wrapper
/// around `VariantPatternEntries` so callers that only care about the nested
/// pattern don't need to destructure the tuple.
enum VariantPatternIter<'a> {
    Patterns(VariantPatternEntries<'a>),
}

impl<'a> Iterator for VariantPatternIter<'a> {
    type Item = &'a Pattern;
    fn next(&mut self) -> Option<Self::Item> {
        let VariantPatternIter::Patterns(entries) = self;
        entries.next().map(|(_, _, p)| p)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LiteralPattern {
    Number(String),
    String(String),
    Bool(bool),
}

/// A segment in a string pattern — either a literal part or a capture variable.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsxElement<T = ()> {
    pub kind: JsxElementKind<T>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JsxElementKind<T = ()> {
    /// `<Tag props...>children</Tag>` or `<Tag props... />`
    Element {
        name: String,
        props: Vec<JsxProp<T>>,
        children: Vec<JsxChild<T>>,
        self_closing: bool,
    },
    /// `<>children</>`
    Fragment { children: Vec<JsxChild<T>> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JsxProp<T = ()> {
    /// `name={value}` or `name="string"`
    Named {
        name: String,
        value: Option<Expr<T>>,
        span: Span,
    },
    /// `{...expr}`
    Spread { expr: Expr<T>, span: Span },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JsxChild<T = ()> {
    /// Raw text between tags
    Text(String),
    /// `{expression}`
    Expr(Expr<T>),
    /// Nested JSX element
    Element(JsxElement<T>),
}
