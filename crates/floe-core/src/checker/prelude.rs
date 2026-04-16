//! Built-in type constructors and shared sentinels.
//!
//! Mirrors Gleam's `type_/prelude.rs`: a flat module of constructors
//! for the language's built-in types so callers can write
//! `prelude::int()` / `prelude::array_of(t)` / `prelude::option_of(t)`
//! instead of spelling out the full enum variants. The shared
//! `UNKNOWN` `Arc<Type>` sentinel also lives here because it's
//! conceptually a "built-in type value" rather than part of the
//! `Type` enum definition.
//!
//! Existing `impl Type { pub fn option_of, result_of, ... }` helpers
//! stay in `types.rs` for backwards compatibility; over time new
//! call sites should prefer `prelude::` for readability.

use std::sync::{Arc, LazyLock};

use super::types::Type;

/// Shared `Arc<Type::Unknown>` sentinel. Cloning bumps a refcount
/// instead of allocating — the fallback path in `attach_types` hits
/// this for every codegen-synthetic node and every post-error
/// subtree, so keeping it interned matters for compiles with many
/// errors. Moved here from `types.rs` as part of #1107 because it's
/// a prelude-level concern, not part of the `Type` data structure.
pub static UNKNOWN: LazyLock<Arc<Type>> = LazyLock::new(|| Arc::new(Type::Unknown));

// ── Primitive constructors ──────────────────────────────────────

#[inline]
pub fn number() -> Type {
    Type::Number
}

#[inline]
pub fn string() -> Type {
    Type::String
}

#[inline]
pub fn bool() -> Type {
    Type::Bool
}

#[inline]
pub fn unit() -> Type {
    Type::Unit
}

#[inline]
pub fn never() -> Type {
    Type::Never
}

#[inline]
pub fn unknown() -> Type {
    Type::Unknown
}

#[inline]
pub fn error() -> Type {
    Type::Error
}

#[inline]
pub fn undefined() -> Type {
    Type::Undefined
}

// ── Parametric constructors ─────────────────────────────────────

#[inline]
pub fn array_of(inner: Type) -> Type {
    Type::Array(Arc::new(inner))
}

#[inline]
pub fn promise_of(inner: Type) -> Type {
    Type::Promise(Arc::new(inner))
}

#[inline]
pub fn settable_of(inner: Type) -> Type {
    Type::Settable(Arc::new(inner))
}

#[inline]
pub fn option_of(inner: Type) -> Type {
    Type::option_of(inner)
}

#[inline]
pub fn result_of(ok: Type, err: Type) -> Type {
    Type::result_of(ok, err)
}

#[inline]
pub fn tuple_of(elems: Vec<Type>) -> Type {
    Type::Tuple(elems)
}

#[inline]
pub fn named(name: impl Into<String>) -> Type {
    Type::Named(name.into())
}

#[inline]
pub fn foreign(name: impl Into<String>) -> Type {
    Type::foreign(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_sentinel_is_interned() {
        let a = Arc::clone(&UNKNOWN);
        let b = Arc::clone(&UNKNOWN);
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn array_of_wraps() {
        assert!(matches!(array_of(number()), Type::Array(_)));
    }

    #[test]
    fn option_of_round_trips_inner() {
        let ty = option_of(string());
        assert!(ty.is_option());
        assert!(matches!(ty.option_inner(), Some(Type::String)));
    }

    #[test]
    fn result_of_round_trips_both_sides() {
        let ty = result_of(number(), string());
        assert!(ty.is_result());
        assert!(matches!(ty.result_ok(), Some(Type::Number)));
        assert!(matches!(ty.result_err(), Some(Type::String)));
    }
}
