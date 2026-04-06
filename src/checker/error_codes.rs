use std::fmt;

/// Central registry of all checker error codes.
///
/// Each variant maps to a unique `ENNN` string code used in diagnostics.
/// Grouped by category for readability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // ── Type mismatches ──────────────────────────────────────────────
    /// Type mismatch between expected and actual types.
    TypeMismatch,

    // ── Name resolution ──────────────────────────────────────────────
    /// Name is not defined in scope (value or type).
    UndefinedName,
    /// Opaque type constructed outside its defining module.
    OpaqueConstruction,
    /// Non-exhaustive match expression.
    NonExhaustiveMatch,

    // ── Operator / expression errors ─────────────────────────────────
    /// `?` used on non-Result/Option, or outside a function returning Result/Option.
    InvalidTryOperator,
    /// Result value not handled (must use `?`, `match`, or assign to `_`).
    UnhandledResult,
    /// Cannot access field on Result - use `match` or `?` first.
    FieldAccessOnResult,
    /// Cannot compare incompatible types.
    InvalidComparison,
    /// Unused import.
    UnusedImport,
    /// Exported function must declare a return type.
    MissingReturnType,
    /// Function must return a value of the declared type.
    MissingReturnValue,

    // ── Import / module errors ───────────────────────────────────────
    /// Module file not found during import resolution.
    ModuleNotFound,
    /// npm package not found.
    PackageNotFound,
    /// Calling untrusted import requires `try`.
    UntrustedImport,
    /// Named export not found in module.
    ExportNotFound,

    // ── Field / property access ──────────────────────────────────────
    /// Unknown field on a type (record, union, etc.).
    UnknownField,
    /// Name already defined in the current scope.
    DuplicateDefinition,
    /// Ambiguous variant name - defined in multiple unions.
    AmbiguousVariant,
    /// Array index must be `number`.
    InvalidArrayIndex,
    /// Tuple index out of bounds or not a valid literal.
    InvalidTupleIndex,
    /// Cannot use bracket access on a type.
    InvalidBracketAccess,
    /// Cannot access field on this type.
    InvalidFieldAccess,

    // ── Trait / for-block errors ─────────────────────────────────────
    /// Unknown trait name.
    UnknownTrait,
    /// Missing required trait method in a for-block.
    MissingTraitMethod,
    /// Unsafe narrowing from `unknown` - use runtime validation.
    UnsafeNarrowing,
    /// Access on `unknown` type.
    AccessOnUnknown,
    /// Access on promise - use `Promise.await` first.
    AccessOnPromise,
    /// Only one `_` placeholder allowed per call.
    MultiplePlaceholders,

    // ── Type registration errors ─────────────────────────────────────
    /// Type name must start with uppercase letter.
    TypeNameCase,
    /// Enum variants cannot use record spread syntax.
    InvalidEnumSpread,
    /// Duplicate field in record type.
    DuplicateField,
    /// Spread field conflicts with existing field.
    SpreadFieldConflict,
    /// Cannot spread union type into record type.
    InvalidSpreadType,
    /// Trait cannot be derived for this type.
    InvalidDerive,
    /// Assert expression must be boolean.
    AssertNotBoolean,

    // ── Control flow ────────────────────────────────────────────────
    /// String pattern on non-string type in match.
    StringPatternOnNonString,
    /// Tuple pattern arity mismatch in match.
    TuplePatternArity,
    /// Variant pattern field count mismatch in match.
    VariantPatternArity,
    /// Literal pattern type mismatch (e.g. `true` on a string).
    LiteralPatternMismatch,

    // ── Warnings ─────────────────────────────────────────────────────
    /// `todo` placeholder will panic at runtime.
    TodoPlaceholder,
    /// Spread field overwritten by explicit field.
    SpreadFieldOverwritten,
    /// Callee has unknown type - arguments are not type-checked.
    UncheckedArguments,
    /// Binding pattern on a finite type (boolean, union) - likely a typo.
    SuspiciousBinding,
    /// Binding resolved to `unknown` type.
    UnknownBinding,
    /// `try` used on a Floe function (which never throws).
    TryOnFloeFunction,
    /// Function uses `await` but return type is not `Promise<T>`.
    MissingPromiseReturn,
    /// Bridge type syntax (`= ...`) used without referencing any TypeScript import.
    BridgeTypeWithoutImport,
    /// tsgo is required to resolve TypeScript imports but is not installed.
    TsgoNotFound,
    /// Wrong number of type arguments for a generic type.
    TypeArgumentArity,
    /// Type name used where a value is expected.
    TypeUsedAsValue,
}

impl ErrorCode {
    /// Returns the string error code (e.g. "E001").
    pub fn code(&self) -> &'static str {
        match self {
            Self::TypeMismatch => "E001",
            Self::UndefinedName => "E002",
            Self::OpaqueConstruction => "E003",
            Self::NonExhaustiveMatch => "E004",
            Self::InvalidTryOperator => "E005",
            Self::UnhandledResult => "E006",
            Self::FieldAccessOnResult => "E007",
            Self::InvalidComparison => "E008",
            Self::UnusedImport => "E009",
            Self::MissingReturnType => "E010",
            Self::MissingReturnValue => "E011",
            Self::ModuleNotFound => "E012",
            Self::PackageNotFound => "E013",
            Self::UntrustedImport => "E014",
            Self::UnknownField => "E015",
            Self::DuplicateDefinition => "E016",
            Self::AmbiguousVariant => "E017",
            Self::InvalidArrayIndex => "E018",
            Self::InvalidTupleIndex => "E019",
            Self::InvalidBracketAccess => "E020",
            Self::InvalidFieldAccess => "E021",
            Self::UnknownTrait => "E022",
            Self::MissingTraitMethod => "E023",
            Self::UnsafeNarrowing => "E024",
            Self::AccessOnUnknown => "E025",
            Self::AccessOnPromise => "E026",
            Self::MultiplePlaceholders => "E027",
            Self::TypeNameCase => "E028",
            Self::InvalidEnumSpread => "E029",
            Self::DuplicateField => "E030",
            Self::SpreadFieldConflict => "E031",
            Self::InvalidSpreadType => "E032",
            Self::InvalidDerive => "E033",
            Self::AssertNotBoolean => "E034",
            Self::StringPatternOnNonString => "E037",
            Self::TuplePatternArity => "E038",
            Self::VariantPatternArity => "E039",
            Self::LiteralPatternMismatch => "E040",
            Self::TodoPlaceholder => "W002",
            Self::SpreadFieldOverwritten => "W003",
            Self::UncheckedArguments => "W004",
            Self::SuspiciousBinding => "W005",
            Self::UnknownBinding => "W006",
            Self::TryOnFloeFunction => "W007",
            Self::MissingPromiseReturn => "E041",
            Self::BridgeTypeWithoutImport => "E042",
            Self::TsgoNotFound => "E043",
            Self::ExportNotFound => "E044",
            Self::TypeArgumentArity => "E045",
            Self::TypeUsedAsValue => "E046",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
