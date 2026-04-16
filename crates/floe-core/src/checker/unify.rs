//! Hindley-Milner unification.
//!
//! `unify(a, b)` makes two types equal by:
//! - Following `Link` chains on any `Type::Var` to their resolved type.
//! - When one side is an `Unbound` variable, destructively updating it to
//!   `Link { type_: other }` so every previous reference to that variable
//!   now sees the resolved type.
//! - Recursing through matching type constructors (`Array(a)` vs `Array(b)`,
//!   tuples, functions, records, …).
//!
//! The occurs check prevents constructing an infinite type. If `a` appears
//! inside `b`, `unify(a, b)` returns `UnifyError::InfiniteType` so a recursive
//! type like `fn bad(x) { [x, bad(x)] }` is rejected cleanly instead of
//! silently building an infinite tree.

use std::sync::Arc;

use super::type_var::{self, TypeVar};
use super::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub enum UnifyError {
    /// Occurs check failure: one variable appears inside the other side.
    InfiniteType,
    /// Two concrete types don't match.
    Mismatch { expected: Type, actual: Type },
    /// Function arities differ.
    FunctionArity { expected: usize, actual: usize },
    /// Tuple arities differ.
    TupleArity { expected: usize, actual: usize },
}

/// Unify two types, destructively updating any `Unbound` variables on either
/// side so they point to the other type. Returns `Ok(())` when the two types
/// are made equal, or a `UnifyError` when they cannot be reconciled.
pub fn unify(a: &Type, b: &Type) -> Result<(), UnifyError> {
    // Fast-path: follow `Link` chains so the outer match never sees them.
    let a = a.resolved();
    let b = b.resolved();

    // Errors suppress cascading diagnostics — unify with anything.
    if matches!(a, Type::Error) || matches!(b, Type::Error) {
        return Ok(());
    }

    // Unknown widens on either side (TypeScript-ish assignability).
    if matches!(a, Type::Unknown) || matches!(b, Type::Unknown) {
        return Ok(());
    }

    // Never is the bottom type and unifies with anything.
    if matches!(a, Type::Never) || matches!(b, Type::Never) {
        return Ok(());
    }

    match (&a, &b) {
        // Two vars: same allocation → already equal.
        (Type::Var(va), Type::Var(vb)) if Arc::ptr_eq(va, vb) => Ok(()),

        // One side is an unbound var — link it to the other side after occurs check.
        (Type::Var(var), other) | (other, Type::Var(var))
            if matches!(type_var::snapshot(var), TypeVar::Unbound { .. }) =>
        {
            if occurs_in(var, other) {
                return Err(UnifyError::InfiniteType);
            }
            type_var::set(
                var,
                TypeVar::Link {
                    type_: Arc::new(other.clone()),
                },
            );
            Ok(())
        }

        // Two Generic vars with the same id → equal. Different ids → mismatch.
        (Type::Var(va), Type::Var(vb)) => {
            let ida = type_var::snapshot(va).id();
            let idb = type_var::snapshot(vb).id();
            if ida.is_some() && ida == idb {
                Ok(())
            } else {
                Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    actual: b.clone(),
                })
            }
        }

        // Generic var on one side, concrete on the other: generics shouldn't reach
        // here after `instantiate`. Treat as a mismatch (the caller forgot to
        // instantiate) rather than silently binding.
        (Type::Var(_), _) | (_, Type::Var(_)) => Err(UnifyError::Mismatch {
            expected: a.clone(),
            actual: b.clone(),
        }),

        // Concrete types: match by constructor.
        (Type::Number, Type::Number)
        | (Type::Bool, Type::Bool)
        | (Type::String, Type::String)
        | (Type::Unit, Type::Unit)
        | (Type::Undefined, Type::Undefined) => Ok(()),

        (Type::StringLiteral(x), Type::StringLiteral(y)) if x == y => Ok(()),
        // A string literal unifies with `String` (widen): useful when a literal
        // flows into a param typed `String`.
        (Type::StringLiteral(_), Type::String) | (Type::String, Type::StringLiteral(_)) => Ok(()),

        (Type::Named(x), Type::Named(y)) if x == y => Ok(()),
        (Type::Foreign { .. }, _) | (_, Type::Foreign { .. }) => Ok(()),

        (Type::Promise(x), Type::Promise(y)) => unify(x, y),
        (Type::Array(x), Type::Array(y)) => unify(x, y),
        (Type::Settable(x), Type::Settable(y)) => unify(x, y),
        (Type::Set { element: x }, Type::Set { element: y }) => unify(x, y),

        (Type::Map { key: k1, value: v1 }, Type::Map { key: k2, value: v2 })
        | (Type::RecordMap { key: k1, value: v1 }, Type::RecordMap { key: k2, value: v2 })
        | (Type::Map { key: k1, value: v1 }, Type::RecordMap { key: k2, value: v2 })
        | (Type::RecordMap { key: k1, value: v1 }, Type::Map { key: k2, value: v2 }) => {
            unify(k1, k2)?;
            unify(v1, v2)
        }

        (Type::Tuple(xs), Type::Tuple(ys)) => {
            if xs.len() != ys.len() {
                return Err(UnifyError::TupleArity {
                    expected: xs.len(),
                    actual: ys.len(),
                });
            }
            for (x, y) in xs.iter().zip(ys.iter()) {
                unify(x, y)?;
            }
            Ok(())
        }

        (
            Type::Function {
                params: p1,
                return_type: r1,
                ..
            },
            Type::Function {
                params: p2,
                return_type: r2,
                ..
            },
        ) => {
            if p1.len() != p2.len() {
                return Err(UnifyError::FunctionArity {
                    expected: p1.len(),
                    actual: p2.len(),
                });
            }
            for (x, y) in p1.iter().zip(p2.iter()) {
                unify(x, y)?;
            }
            unify(r1, r2)
        }

        (Type::Record(fa), Type::Record(fb)) => {
            if fa.len() != fb.len() {
                return Err(UnifyError::Mismatch {
                    expected: a.clone(),
                    actual: b.clone(),
                });
            }
            for ((na, ta), (nb, tb)) in fa.iter().zip(fb.iter()) {
                if na != nb {
                    return Err(UnifyError::Mismatch {
                        expected: a.clone(),
                        actual: b.clone(),
                    });
                }
                unify(ta, tb)?;
            }
            Ok(())
        }

        (
            Type::Union {
                name: na,
                variants: vs_a,
            },
            Type::Union {
                name: nb,
                variants: vs_b,
            },
        ) if na == nb && vs_a.len() == vs_b.len() => {
            for ((n1, fs1), (n2, fs2)) in vs_a.iter().zip(vs_b.iter()) {
                if n1 != n2 || fs1.len() != fs2.len() {
                    return Err(UnifyError::Mismatch {
                        expected: a.clone(),
                        actual: b.clone(),
                    });
                }
                for (f1, f2) in fs1.iter().zip(fs2.iter()) {
                    unify(f1, f2)?;
                }
            }
            Ok(())
        }

        (Type::TsUnion(members), concrete) | (concrete, Type::TsUnion(members)) => {
            // TsUnion accepts if any member unifies. Note: this doesn't perform a
            // backtracking search through all members — it takes the first that
            // unifies, which is enough for the cases Floe uses TsUnion for
            // (string literal unions, `Date | number | string`).
            for m in members {
                if unify(m, concrete).is_ok() {
                    return Ok(());
                }
            }
            Err(UnifyError::Mismatch {
                expected: a.clone(),
                actual: b.clone(),
            })
        }

        (Type::Opaque { name: na, .. }, Type::Opaque { name: nb, .. }) if na == nb => Ok(()),

        _ => Err(UnifyError::Mismatch {
            expected: a.clone(),
            actual: b.clone(),
        }),
    }
}

/// Return `true` if the unbound variable `var` appears anywhere inside `ty`.
/// Used by unification to reject recursive bindings like `X = List(X)`.
fn occurs_in(var: &super::type_var::TypeVarRef, ty: &Type) -> bool {
    let resolved = ty.resolved();
    match &resolved {
        Type::Var(other) => {
            if Arc::ptr_eq(var, other) {
                return true;
            }
            // Only interesting vars are Unbound — Link was resolved above,
            // Generic can't contain anything.
            false
        }
        Type::Promise(inner)
        | Type::Array(inner)
        | Type::Settable(inner)
        | Type::Set { element: inner } => occurs_in(var, inner),
        Type::Map { key, value } | Type::RecordMap { key, value } => {
            occurs_in(var, key) || occurs_in(var, value)
        }
        Type::Tuple(items) => items.iter().any(|t| occurs_in(var, t)),
        Type::Record(fields) => fields.iter().any(|(_, t)| occurs_in(var, t)),
        Type::Union { variants, .. } => variants
            .iter()
            .any(|(_, fs)| fs.iter().any(|t| occurs_in(var, t))),
        Type::TsUnion(ms) => ms.iter().any(|t| occurs_in(var, t)),
        Type::Function {
            params,
            return_type,
            ..
        } => params.iter().any(|t| occurs_in(var, t)) || occurs_in(var, return_type),
        Type::Opaque { base, .. } => occurs_in(var, base),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unify_same_primitive() {
        assert_eq!(unify(&Type::Number, &Type::Number), Ok(()));
    }

    #[test]
    fn unify_different_primitives() {
        assert!(unify(&Type::Number, &Type::String).is_err());
    }

    #[test]
    fn unify_unbound_to_concrete() {
        let var = Type::unbound(0);
        assert_eq!(unify(&var, &Type::Number), Ok(()));
        // After unification, the var resolves to Number.
        assert_eq!(var.resolved(), Type::Number);
    }

    #[test]
    fn unify_concrete_to_unbound() {
        let var = Type::unbound(0);
        assert_eq!(unify(&Type::Number, &var), Ok(()));
        assert_eq!(var.resolved(), Type::Number);
    }

    #[test]
    fn unify_two_unbound_links_one_to_the_other() {
        let a = Type::unbound(0);
        let b = Type::unbound(1);
        assert_eq!(unify(&a, &b), Ok(()));
        // Now unifying either with Number resolves both.
        assert_eq!(unify(&a, &Type::Number), Ok(()));
        assert_eq!(a.resolved(), Type::Number);
        assert_eq!(b.resolved(), Type::Number);
    }

    #[test]
    fn unify_occurs_check_catches_infinite_type() {
        let v = Type::unbound(0);
        let list_of_v = Type::Array(Arc::new(v.clone()));
        assert_eq!(unify(&v, &list_of_v), Err(UnifyError::InfiniteType));
    }

    #[test]
    fn unify_recursive_through_tuple_is_rejected() {
        let v = Type::unbound(0);
        let tup = Type::Tuple(vec![Type::Number, v.clone()]);
        assert_eq!(unify(&v, &tup), Err(UnifyError::InfiniteType));
    }

    #[test]
    fn unify_arrays_recurses() {
        let v = Type::unbound(0);
        let a = Type::Array(Arc::new(v.clone()));
        let b = Type::Array(Arc::new(Type::Number));
        assert_eq!(unify(&a, &b), Ok(()));
        assert_eq!(v.resolved(), Type::Number);
    }

    #[test]
    fn unify_tuple_length_mismatch_is_error() {
        let a = Type::Tuple(vec![Type::Number]);
        let b = Type::Tuple(vec![Type::Number, Type::String]);
        assert!(matches!(unify(&a, &b), Err(UnifyError::TupleArity { .. })));
    }

    #[test]
    fn unify_functions_check_arity_and_recurse() {
        let r = Type::unbound(0);
        let f = Type::Function {
            params: vec![Type::Number],
            required_params: 1,
            return_type: Arc::new(r.clone()),
        };
        let g = Type::Function {
            params: vec![Type::Number],
            required_params: 1,
            return_type: Arc::new(Type::Bool),
        };
        assert_eq!(unify(&f, &g), Ok(()));
        assert_eq!(r.resolved(), Type::Bool);
    }
}
