//! The `Type` data structure and its inherent inspection/constructor
//! methods. Ancillary concerns that used to live here (the scope
//! stack, display formatting, prelude constructors) were split into
//! `environment.rs`, `printer.rs`, and `prelude.rs` as part of #1107.

use super::printer::{TypeDisplay, TypeDisplayStyle};

// ── Types ────────────────────────────────────────────────────────

/// Internal type representation used by the checker.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Primitive types: number, string, boolean
    Number,
    String,
    Bool,
    /// The undefined type (used for None)
    Undefined,
    /// A named/user-defined type (locally defined in Floe)
    Named(String),
    /// A foreign type from npm imports — structure unknown to Floe,
    /// but TypeScript validated it at the source
    Foreign(String),
    /// Promise<T> — async return type, unwrapped by `Promise.await`
    Promise(Box<Type>),
    /// Opaque type: only the defining module can construct/destructure
    Opaque {
        name: String,
        base: Box<Type>,
    },
    // Result<T, E> is now represented as Type::Union { name: "Result", variants: [("Ok", [T]), ("Err", [E])] }
    // Use Type::result_of(ok, err) to construct, ty.is_result() / ty.result_ok() / ty.result_err() to inspect.
    // Option<T> is now represented as Type::Union { name: "Option", variants: [("Some", [T]), ("None", [])] }
    // Use Type::option_of(inner) to construct, ty.is_option() / ty.option_inner() to inspect.
    /// Settable<T> = Set(T) | Clear | Unchanged
    Settable(Box<Type>),
    /// Function type. `required_params` is how many leading params the caller
    /// must provide; trailing params beyond that index have defaults and can be omitted.
    Function {
        params: Vec<Type>,
        required_params: usize,
        return_type: Box<Type>,
    },
    /// Array type
    Array(Box<Type>),
    /// Map type: Map<K, V> (JS Map at runtime)
    Map {
        key: Box<Type>,
        value: Box<Type>,
    },
    /// TS Record<K, V> — plain object at runtime, supports Map operations
    /// via bracket access codegen instead of JS Map API
    RecordMap {
        key: Box<Type>,
        value: Box<Type>,
    },
    /// Set type: Set<T>
    Set {
        element: Box<Type>,
    },
    /// Tuple type
    Tuple(Vec<Type>),
    /// Record/struct type
    Record(Vec<(String, Type)>),
    /// Union (tagged discriminated union)
    Union {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
    /// Untagged union: `Date | number | string` or `"GET" | "POST"`
    /// Compatible if the value matches any member type.
    TsUnion(Vec<Type>),
    /// String literal type: `"GET"`, `"POST"`, etc.
    StringLiteral(String),
    /// Type variable (for inference)
    Var(usize),
    /// The unknown/any escape hatch — used for genuinely unknown external types
    /// (e.g. npm imports without type probes). Compatible with everything as expected,
    /// but not as actual (same as TypeScript's `unknown`).
    Unknown,
    /// Error sentinel — type resolution failed and an error was already emitted.
    /// Compatible with all types in both directions to suppress cascading errors.
    /// Should never reach codegen; if it does, that indicates a checker bug.
    Error,
    /// Unit type () — replaces void, a real value usable in generics
    Unit,
    /// The never type — used for `todo` and `unreachable`, compatible with any type
    Never,
}

impl Type {
    /// Construct an Option<T> as a Union type.
    pub fn option_of(inner: Type) -> Type {
        Type::Union {
            name: crate::type_layout::TYPE_OPTION.to_string(),
            variants: vec![
                (crate::type_layout::VARIANT_SOME.to_string(), vec![inner]),
                (crate::type_layout::VARIANT_NONE.to_string(), vec![]),
            ],
        }
    }

    /// Construct a Result<T, E> as a Union type.
    pub fn result_of(ok: Type, err: Type) -> Type {
        Type::Union {
            name: crate::type_layout::TYPE_RESULT.to_string(),
            variants: vec![
                (crate::type_layout::VARIANT_OK.to_string(), vec![ok]),
                (crate::type_layout::VARIANT_ERR.to_string(), vec![err]),
            ],
        }
    }

    pub(crate) fn is_result(&self) -> bool {
        matches!(
            self,
            Type::Union { name, .. } if name == crate::type_layout::TYPE_RESULT
        )
    }

    /// Extract T from Result<T, E> (the Union representation). Returns None if not a Result.
    pub fn result_ok(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_RESULT
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_OK)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    /// Extract E from Result<T, E> (the Union representation). Returns None if not a Result.
    pub fn result_err(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_RESULT
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_ERR)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    pub(crate) fn is_option(&self) -> bool {
        matches!(
            self,
            Type::Union { name, .. } if name == crate::type_layout::TYPE_OPTION
        )
    }

    pub(crate) fn is_settable(&self) -> bool {
        matches!(self, Type::Settable(_))
    }

    /// Extract T from Option<T> (the Union representation). Returns None if not an Option.
    pub fn option_inner(&self) -> Option<&Type> {
        if let Type::Union { name, variants } = self
            && name == crate::type_layout::TYPE_OPTION
        {
            variants
                .iter()
                .find(|(n, _)| n == crate::type_layout::VARIANT_SOME)
                .and_then(|(_, fields)| fields.first())
        } else {
            None
        }
    }

    /// Unwrap Option<T> → T. If not an Option, return self.
    pub fn unwrap_option(self) -> Type {
        if let Type::Union { name, variants } = self {
            if name == crate::type_layout::TYPE_OPTION {
                return variants
                    .into_iter()
                    .find(|(n, _)| n == crate::type_layout::VARIANT_SOME)
                    .and_then(|(_, mut fields)| fields.pop())
                    .unwrap_or(Type::Unknown);
            }
            Type::Union { name, variants }
        } else {
            self
        }
    }

    /// Returns true if the type is still being resolved (Unknown, Error, or a type
    /// variable). Guards on this pattern prevent emitting cascading diagnostics when
    /// there is not yet enough type information to report a meaningful error.
    pub(crate) fn is_undetermined(&self) -> bool {
        matches!(self, Type::Unknown | Type::Error | Type::Var(_))
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Type::Number)
    }

    pub(crate) fn is_boolean(&self) -> bool {
        matches!(self, Type::Bool)
    }

    pub(crate) fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Number
                | Type::String
                | Type::Bool
                | Type::Unit
                | Type::Undefined
                | Type::StringLiteral(_)
        )
    }

    /// Check if this is a TsUnion of all StringLiterals (non-allocating).
    pub(crate) fn is_string_literal_union(&self) -> bool {
        if let Type::TsUnion(members) = self {
            !members.is_empty() && members.iter().all(|m| matches!(m, Type::StringLiteral(_)))
        } else {
            false
        }
    }

    /// If this is a TsUnion of all StringLiterals, return the string values.
    pub(crate) fn as_string_literal_variants(&self) -> Option<Vec<&str>> {
        if let Type::TsUnion(members) = self {
            let strings: Vec<&str> = members
                .iter()
                .filter_map(|m| {
                    if let Type::StringLiteral(s) = m {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if strings.len() == members.len() && !strings.is_empty() {
                return Some(strings);
            }
        }
        None
    }

    /// Return a display wrapper that formats type variables as readable letters
    /// (T, U, V, ...) instead of internal IDs. Used for stdlib hover/completion display.
    pub fn display_for_stdlib(&self) -> TypeDisplay<'_> {
        TypeDisplay {
            ty: self,
            style: TypeDisplayStyle::Stdlib,
        }
    }
}
