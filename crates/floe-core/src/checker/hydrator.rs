//! Hydration: translate user-written type names into the internal representation.
//!
//! When the parser sees a name like `a` or `T` in a function signature, it
//! shows up in the CST as `Type::Named("a")`. To participate in Hindley-Milner
//! inference, those names need to be mapped to actual type variables.
//!
//! The `Hydrator` is a small per-scope map from lowercase identifier (the
//! user-facing generic name) to a `Generic` type variable. The first time a
//! name is seen, a fresh `Generic` is minted. Subsequent occurrences reuse it,
//! so `fn id(x: a) -> a` binds both `a`s to the same variable.
//!
//! Tree-walking is delegated to `type_var::map_children` so the three surface
//! operations here — `instantiate`, `generalise`, `hydrate_single_letter_generics`
//! — are each a single leaf-replacement rule, not a 60-line match per walker.

use std::collections::HashMap;
use std::sync::Arc;

use super::type_var::{self, TypeVar, TypeVarRef, map_children};
use super::types::Type;

/// Maps user-written type-parameter names to `Generic` variables during
/// signature hydration.
#[derive(Default)]
pub struct Hydrator {
    by_name: HashMap<String, TypeVarRef>,
}

impl Hydrator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up or mint a fresh `Generic` variable for `name`. Uses `next_id`
    /// to allocate fresh ids (the caller threads in a global counter).
    pub fn generic_for(&mut self, name: &str, next_id: &mut u64) -> Type {
        if let Some(var) = self.by_name.get(name) {
            return Type::Var(Arc::clone(var));
        }
        let id = *next_id;
        *next_id += 1;
        let var = type_var::generic(id);
        self.by_name.insert(name.to_string(), Arc::clone(&var));
        Type::Var(var)
    }
}

/// Single uppercase ASCII letter (`T`, `U`, `E`, …) — the `.d.ts` convention
/// for generic parameter names.
pub fn is_single_uppercase(n: &str) -> bool {
    n.len() == 1 && n.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn fresh_unbound(next_id: &mut u64) -> TypeVarRef {
    let id = *next_id;
    *next_id += 1;
    type_var::unbound(id)
}

/// Walk `ty` and replace every `Generic` variable with a fresh `Unbound` one,
/// using a shared id-keyed mapping so the *same* generic (by id) becomes the
/// *same* unbound across the produced tree. This is the let-polymorphism step:
/// each call site gets its own copy of the signature's variables.
pub fn instantiate(ty: &Type, next_id: &mut u64) -> Type {
    let mut mapping: HashMap<u64, TypeVarRef> = HashMap::new();
    instantiate_with(ty, next_id, &mut mapping)
}

fn instantiate_with(ty: &Type, next_id: &mut u64, mapping: &mut HashMap<u64, TypeVarRef>) -> Type {
    map_children(ty, &mut |t| match t {
        Type::Var(var) => match type_var::snapshot(var) {
            TypeVar::Generic { id } => {
                let fresh = mapping
                    .entry(id)
                    .or_insert_with(|| fresh_unbound(next_id))
                    .clone();
                Some(Type::Var(fresh))
            }
            TypeVar::Link { type_ } => Some(instantiate_with(&type_, next_id, mapping)),
            TypeVar::Unbound { .. } => Some(t.clone()),
        },
        _ => None,
    })
}

/// Instantiate a whole function signature (params + return type) with a shared
/// mapping so each `Generic` id maps to the *same* fresh `Unbound` variable
/// across both the parameters and the return. Use this at call sites where
/// the params and return should share type-var bindings.
pub fn instantiate_signature(
    params: &[Type],
    return_type: &Type,
    next_id: &mut u64,
) -> (Vec<Type>, Type) {
    let mut mapping: HashMap<u64, TypeVarRef> = HashMap::new();
    let inst_params = params
        .iter()
        .map(|p| instantiate_with(p, next_id, &mut mapping))
        .collect();
    let inst_ret = instantiate_with(return_type, next_id, &mut mapping);
    (inst_params, inst_ret)
}

/// Walk `ty` and replace every `Unbound` variable with a `Generic` one,
/// reusing ids so shared variables remain shared. Used to freeze a function's
/// signature after its body has been checked.
pub fn generalise(ty: &Type) -> Type {
    let mut mapping: HashMap<u64, TypeVarRef> = HashMap::new();
    map_children(ty, &mut |t| match t {
        Type::Var(var) => match type_var::snapshot(var) {
            TypeVar::Unbound { id } => {
                let g = mapping
                    .entry(id)
                    .or_insert_with(|| type_var::generic(id))
                    .clone();
                Some(Type::Var(g))
            }
            TypeVar::Link { type_ } => Some(generalise(&type_)),
            TypeVar::Generic { .. } => Some(t.clone()),
        },
        _ => None,
    })
}

/// Walk `ty` replacing every `Type::Named(n)` where `n` is a single uppercase
/// ASCII letter (`T`, `U`, `E`, …) with a `Generic` type variable. Uppercase
/// single letters are the convention used by TypeScript `.d.ts` signatures for
/// generics — by hydrating them here, imported function signatures plug into
/// the same HM unification pipeline as native Floe functions. Same letters
/// share the same Generic id across the walk so `(T) -> T` stays tied.
pub fn hydrate_single_letter_generics(ty: &Type, next_id: &mut u64) -> Type {
    let mut mapping: HashMap<String, TypeVarRef> = HashMap::new();
    map_children(ty, &mut |t| match t {
        Type::Named(n) if is_single_uppercase(n) => {
            let var = mapping
                .entry(n.clone())
                .or_insert_with(|| {
                    let id = *next_id;
                    *next_id += 1;
                    type_var::generic(id)
                })
                .clone();
            Some(Type::Var(var))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydrator_reuses_same_variable_for_same_name() {
        let mut h = Hydrator::new();
        let mut next = 0u64;
        let a1 = h.generic_for("a", &mut next);
        let a2 = h.generic_for("a", &mut next);
        assert_eq!(a1.var_id(), a2.var_id());
        assert_eq!(next, 1);
    }

    #[test]
    fn hydrator_mints_new_variable_for_new_name() {
        let mut h = Hydrator::new();
        let mut next = 0u64;
        let a = h.generic_for("a", &mut next);
        let b = h.generic_for("b", &mut next);
        assert_ne!(a.var_id(), b.var_id());
    }

    #[test]
    fn instantiate_replaces_generics_with_fresh_unbounds() {
        let mut next = 10u64;
        let mut h = Hydrator::new();
        let a = h.generic_for("a", &mut next);
        let sig = Type::Function {
            params: vec![a.clone()],
            required_params: 1,
            return_type: Arc::new(a.clone()),
        };
        assert!(a.is_generic_var());

        let inst = instantiate(&sig, &mut next);
        if let Type::Function {
            params,
            return_type,
            ..
        } = &inst
        {
            assert!(params[0].is_unbound());
            assert!(return_type.is_unbound());
            // Both point to the same Unbound var.
            if let (Type::Var(p), Type::Var(r)) = (&params[0], return_type.as_ref()) {
                assert!(Arc::ptr_eq(p, r));
            } else {
                panic!("expected Type::Var on both sides");
            }
        } else {
            panic!("expected Type::Function");
        }
    }

    #[test]
    fn generalise_turns_unbound_back_to_generic() {
        let v = Type::unbound(7);
        let g = generalise(&v);
        assert!(g.is_generic_var());
    }
}
