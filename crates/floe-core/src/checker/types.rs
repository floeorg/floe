//! The `Type` data structure and its inherent inspection/constructor
//! methods. Ancillary concerns that used to live here (the scope
//! stack, display formatting, prelude constructors) were split into
//! `environment.rs`, `printer.rs`, and `prelude.rs` as part of #1107.

use std::sync::Arc;

use super::printer::{TypeDisplay, TypeDisplayStyle};
use super::type_var::{self, TypeVarRef};

// ── Types ────────────────────────────────────────────────────────

/// Internal type representation used by the checker.
///
/// Sub-types are stored behind `Arc<Type>` for cheap cloning — type trees
/// are cloned frequently during inference. Arc refcount bumps replace the
/// deep copies that `Box<Type>` required.
#[derive(Debug, Clone)]
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
    /// but TypeScript validated it at the source. `untrusted` is set for
    /// imports from non-trusted packages: codegen wraps their calls in
    /// try/catch and the checker tracks their Result propagation so that
    /// callers are forced to handle thrown exceptions.
    Foreign {
        name: String,
        untrusted: bool,
    },
    /// Promise<T> — async return type, unwrapped by `Promise.await`
    Promise(Arc<Type>),
    /// Opaque type: only the defining module can construct/destructure
    Opaque {
        name: String,
        base: Arc<Type>,
    },
    // Result<T, E> is now represented as Type::Union { name: "Result", variants: [("Ok", [T]), ("Err", [E])] }
    // Use Type::result_of(ok, err) to construct, ty.is_result() / ty.result_ok() / ty.result_err() to inspect.
    // Option<T> is now represented as Type::Union { name: "Option", variants: [("Some", [T]), ("None", [])] }
    // Use Type::option_of(inner) to construct, ty.is_option() / ty.option_inner() to inspect.
    /// Settable<T> = Set(T) | Clear | Unchanged
    Settable(Arc<Type>),
    /// Function type. `required_params` is how many leading params the caller
    /// must provide; trailing params beyond that index have defaults and can be omitted.
    Function {
        params: Vec<Type>,
        required_params: usize,
        return_type: Arc<Type>,
    },
    /// Array type
    Array(Arc<Type>),
    /// Map type: Map<K, V> (JS Map at runtime)
    Map {
        key: Arc<Type>,
        value: Arc<Type>,
    },
    /// TS Record<K, V> — plain object at runtime, supports Map operations
    /// via bracket access codegen instead of JS Map API
    RecordMap {
        key: Arc<Type>,
        value: Arc<Type>,
    },
    /// Set type: Set<T>
    Set {
        element: Arc<Type>,
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
    /// Type variable for Hindley-Milner inference. Unbound vars get Linked to
    /// concrete types by `unify`; Generic vars are polymorphic markers that
    /// `instantiate` replaces with fresh Unbound vars at each call site.
    Var(TypeVarRef),
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
    /// Construct a fresh unbound type variable with the given id.
    pub fn unbound(id: u64) -> Type {
        Type::Var(type_var::unbound(id))
    }

    /// Construct a trusted foreign type (npm imports from trusted packages).
    pub fn foreign(name: impl Into<String>) -> Type {
        Type::Foreign {
            name: name.into(),
            untrusted: false,
        }
    }

    /// Construct an untrusted foreign type (npm imports from non-trusted
    /// packages whose calls must be wrapped in try/catch at the boundary).
    pub fn untrusted_foreign(name: impl Into<String>) -> Type {
        Type::Foreign {
            name: name.into(),
            untrusted: true,
        }
    }

    /// True if this resolves to any foreign type (trusted or untrusted).
    pub fn is_foreign(&self) -> bool {
        matches!(self.resolved(), Type::Foreign { .. })
    }

    /// True if this resolves to an untrusted foreign type.
    pub fn is_untrusted_foreign(&self) -> bool {
        matches!(
            self.resolved(),
            Type::Foreign {
                untrusted: true,
                ..
            }
        )
    }

    /// The foreign type's name if this resolves to a `Type::Foreign`.
    pub fn foreign_name(&self) -> Option<String> {
        match self.resolved() {
            Type::Foreign { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Construct a polymorphic (generalized) type variable with the given id.
    pub fn generic(id: u64) -> Type {
        Type::Var(type_var::generic(id))
    }

    /// Follow any `Link` chain on this type, returning the underlying type.
    /// Returns the original type when no links exist.
    pub fn resolved(&self) -> Type {
        type_var::resolve(self)
    }

    /// Recursively resolve all `Type::Var` `Link` chains in the tree — use
    /// when passing a type across a boundary (pattern binding, error message,
    /// stored type map) so downstream consumers don't walk past unresolved
    /// links.
    pub fn deep_resolved(&self) -> Type {
        type_var::deep_resolve(self)
    }

    /// Is this an unbound type variable after resolving links?
    pub fn is_unbound(&self) -> bool {
        type_var::is_unbound(&self.resolved())
    }

    /// Is this a generic (polymorphic) type variable after resolving links?
    pub fn is_generic_var(&self) -> bool {
        type_var::is_generic(&self.resolved())
    }

    /// The id of the underlying unbound or generic variable, if this is one.
    pub fn var_id(&self) -> Option<u64> {
        type_var::var_id(&self.resolved())
    }

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
        let r = self.resolved();
        matches!(r, Type::Unknown | Type::Error | Type::Var(_))
    }

    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self.resolved(), Type::Number)
    }

    pub(crate) fn is_boolean(&self) -> bool {
        matches!(self.resolved(), Type::Bool)
    }

    pub(crate) fn is_primitive(&self) -> bool {
        matches!(
            self.resolved(),
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

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        // Resolve both sides through any `Link` chains before comparing.
        let a = self.resolved();
        let b = other.resolved();
        match (&a, &b) {
            (Type::Number, Type::Number)
            | (Type::String, Type::String)
            | (Type::Bool, Type::Bool)
            | (Type::Undefined, Type::Undefined)
            | (Type::Unknown, Type::Unknown)
            | (Type::Error, Type::Error)
            | (Type::Unit, Type::Unit)
            | (Type::Never, Type::Never) => true,
            (Type::Named(a), Type::Named(b)) => a == b,
            (Type::Foreign { name: a, .. }, Type::Foreign { name: b, .. }) => a == b,
            (Type::Promise(a), Type::Promise(b)) => **a == **b,
            (Type::Opaque { name: na, base: ba }, Type::Opaque { name: nb, base: bb }) => {
                na == nb && **ba == **bb
            }
            (Type::Settable(a), Type::Settable(b)) => **a == **b,
            (
                Type::Function {
                    params: pa,
                    required_params: ra,
                    return_type: rta,
                },
                Type::Function {
                    params: pb,
                    required_params: rb,
                    return_type: rtb,
                },
            ) => pa == pb && ra == rb && **rta == **rtb,
            (Type::Array(a), Type::Array(b)) => **a == **b,
            (Type::Map { key: ka, value: va }, Type::Map { key: kb, value: vb })
            | (Type::RecordMap { key: ka, value: va }, Type::RecordMap { key: kb, value: vb }) => {
                **ka == **kb && **va == **vb
            }
            (Type::Set { element: a }, Type::Set { element: b }) => **a == **b,
            (Type::Tuple(a), Type::Tuple(b)) => a == b,
            (Type::Record(a), Type::Record(b)) => a == b,
            (
                Type::Union {
                    name: na,
                    variants: va,
                },
                Type::Union {
                    name: nb,
                    variants: vb,
                },
            ) => na == nb && va == vb,
            (Type::TsUnion(a), Type::TsUnion(b)) => a == b,
            (Type::StringLiteral(a), Type::StringLiteral(b)) => a == b,
            // Type variables compare by identity: same Arc means same variable.
            (Type::Var(a), Type::Var(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
