//! Type variables for Hindley-Milner inference.
//!
//! A `TypeVar` is held behind an `Arc<Mutex<_>>` so unification can
//! destructively update it without needing a separate substitution map.
//! Three variants model the life-cycle of a type variable:
//!
//! - `Unbound { id }` — fresh inference variable, not yet resolved.
//! - `Link { type_ }` — resolved: the real type lives on the far side of the link.
//! - `Generic { id }` — the marker form used for generalized type parameters in
//!   polymorphic signatures. At each call site, `instantiate` replaces every
//!   `Generic(id)` with a fresh `Unbound` so inference can specialize them.
//!
//! Identity across a `Type` tree is done with `Arc::ptr_eq` — two vars with the
//! same `id` but different allocations are *different* variables.
//!
//! The `Mutex` (over `RefCell`) is what keeps the rest of the codebase's
//! `Sync` statics (`UNKNOWN`, `EMPTY_TYPES`) happy — the checker is single-
//! threaded so contention never happens in practice.

use std::sync::{Arc, Mutex};

use super::types::Type;

#[derive(Debug, Clone)]
pub enum TypeVar {
    Unbound { id: u64 },
    Link { type_: Arc<Type> },
    Generic { id: u64 },
}

impl TypeVar {
    pub fn id(&self) -> Option<u64> {
        match self {
            TypeVar::Unbound { id } | TypeVar::Generic { id } => Some(*id),
            TypeVar::Link { .. } => None,
        }
    }
}

pub type TypeVarRef = Arc<Mutex<TypeVar>>;

/// Construct a fresh `Unbound` type variable wrapped in an `Arc<Mutex<_>>`.
pub fn unbound(id: u64) -> TypeVarRef {
    Arc::new(Mutex::new(TypeVar::Unbound { id }))
}

/// Construct a fresh `Generic` type variable wrapped in an `Arc<Mutex<_>>`.
pub fn generic(id: u64) -> TypeVarRef {
    Arc::new(Mutex::new(TypeVar::Generic { id }))
}

/// Snapshot the inner `TypeVar` by cloning it out of the mutex. Prefer this
/// over repeatedly calling `lock()` — poisoned mutexes are recovered silently
/// since the checker never panics while holding a var lock.
pub fn snapshot(var: &TypeVarRef) -> TypeVar {
    match var.lock() {
        Ok(g) => g.clone(),
        Err(poison) => poison.into_inner().clone(),
    }
}

/// Set the inner `TypeVar` — used by unification when linking an unbound
/// variable to a concrete type.
pub fn set(var: &TypeVarRef, value: TypeVar) {
    match var.lock() {
        Ok(mut g) => *g = value,
        Err(poison) => *poison.into_inner() = value,
    }
}

/// Follow `Link` chains to the underlying type. Returns the original type when
/// it is not a `Type::Var`, or a `Type::Var` holding an `Unbound` / `Generic`
/// variable when the chain terminates at an unbound variable.
pub fn resolve(ty: &Type) -> Type {
    match ty {
        Type::Var(var) => match snapshot(var) {
            TypeVar::Link { type_ } => resolve(&type_),
            _ => ty.clone(),
        },
        _ => ty.clone(),
    }
}

/// Rebuild a `Type` tree by recursing into every compound constructor,
/// applying `f` at each node. `f` returns `Some(replacement)` to short-
/// circuit the recursion at that node, or `None` to let `map_children`
/// recurse into the children.
///
/// Every structural walker in the checker (instantiate, generalise,
/// deep-resolve, hydrate, occurs-in) collapses to "at each Type node,
/// decide whether to replace or descend". Centralising the recursion here
/// means new `Type` variants only need one update instead of five.
pub fn map_children<F>(ty: &Type, f: &mut F) -> Type
where
    F: FnMut(&Type) -> Option<Type>,
{
    use std::sync::Arc;
    if let Some(replacement) = f(ty) {
        return replacement;
    }
    match ty {
        Type::Promise(inner) => Type::Promise(Arc::new(map_children(inner, f))),
        Type::Array(inner) => Type::Array(Arc::new(map_children(inner, f))),
        Type::Settable(inner) => Type::Settable(Arc::new(map_children(inner, f))),
        Type::Set { element } => Type::Set {
            element: Arc::new(map_children(element, f)),
        },
        Type::Map { key, value } => Type::Map {
            key: Arc::new(map_children(key, f)),
            value: Arc::new(map_children(value, f)),
        },
        Type::RecordMap { key, value } => Type::RecordMap {
            key: Arc::new(map_children(key, f)),
            value: Arc::new(map_children(value, f)),
        },
        Type::Tuple(items) => Type::Tuple(items.iter().map(|t| map_children(t, f)).collect()),
        Type::Record(fields) => Type::Record(
            fields
                .iter()
                .map(|(n, t)| (n.clone(), map_children(t, f)))
                .collect(),
        ),
        Type::Union { name, variants } => Type::Union {
            name: name.clone(),
            variants: variants
                .iter()
                .map(|(n, fs)| (n.clone(), fs.iter().map(|t| map_children(t, f)).collect()))
                .collect(),
        },
        Type::TsUnion(ms) => Type::TsUnion(ms.iter().map(|t| map_children(t, f)).collect()),
        Type::Function {
            params,
            required_params,
            return_type,
        } => Type::Function {
            params: params.iter().map(|t| map_children(t, f)).collect(),
            required_params: *required_params,
            return_type: Arc::new(map_children(return_type, f)),
        },
        Type::Opaque { name, base } => Type::Opaque {
            name: name.clone(),
            base: Arc::new(map_children(base, f)),
        },
        _ => ty.clone(),
    }
}

/// True if `pred` holds at any node in `ty`'s tree (short-circuits). The
/// structural walk mirrors `map_children` so new `Type` variants only need
/// one update here, not at every call site that wants to ask "does this
/// tree contain X?".
pub fn any_nested<F>(ty: &Type, pred: &F) -> bool
where
    F: Fn(&Type) -> bool,
{
    if pred(ty) {
        return true;
    }
    match ty {
        Type::Promise(inner)
        | Type::Array(inner)
        | Type::Settable(inner)
        | Type::Opaque { base: inner, .. } => any_nested(inner, pred),
        Type::Set { element } => any_nested(element, pred),
        Type::Map { key, value } | Type::RecordMap { key, value } => {
            any_nested(key, pred) || any_nested(value, pred)
        }
        Type::Tuple(items) | Type::TsUnion(items) => items.iter().any(|t| any_nested(t, pred)),
        Type::Record(fields) => fields.iter().any(|(_, t)| any_nested(t, pred)),
        Type::Function {
            params,
            return_type,
            ..
        } => params.iter().any(|t| any_nested(t, pred)) || any_nested(return_type, pred),
        Type::Union { variants, .. } => variants
            .iter()
            .any(|(_, fs)| fs.iter().any(|t| any_nested(t, pred))),
        _ => false,
    }
}

/// Recursively resolve all `Type::Var` `Link` chains in a type tree. Unlike
/// `resolve`, which only follows the outermost link, `deep_resolve` rebuilds
/// the entire tree so nested unbound-then-linked vars (like `Array<Unbound>`
/// where `Unbound → CartItem`) surface as `Array<CartItem>` to downstream
/// consumers that walk the tree structurally (pattern matching, codegen,
/// hover).
pub fn deep_resolve(ty: &Type) -> Type {
    map_children(ty, &mut |t| match t {
        Type::Var(var) => match snapshot(var) {
            TypeVar::Link { type_ } => Some(deep_resolve(&type_)),
            _ => Some(t.clone()),
        },
        _ => None,
    })
}

/// Return `true` if `ty` resolves to an `Unbound` type variable.
pub fn is_unbound(ty: &Type) -> bool {
    match ty {
        Type::Var(var) => matches!(snapshot(var), TypeVar::Unbound { .. }),
        _ => false,
    }
}

/// Return `true` if `ty` resolves to a `Generic` type variable.
pub fn is_generic(ty: &Type) -> bool {
    match ty {
        Type::Var(var) => matches!(snapshot(var), TypeVar::Generic { .. }),
        _ => false,
    }
}

/// Return the variable's id if `ty` is a `Type::Var` holding an `Unbound` or
/// `Generic` variable. Returns `None` for `Link` chains or non-vars.
pub fn var_id(ty: &Type) -> Option<u64> {
    if let Type::Var(var) = ty {
        snapshot(var).id()
    } else {
        None
    }
}
