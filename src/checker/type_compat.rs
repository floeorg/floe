use super::*;

impl Checker {
    /// Resolve a `Type::Named` to its concrete underlying type, if possible.
    /// Returns `Some(concrete)` if the type was resolved, `None` if not a Named type.
    pub(crate) fn resolve_named_to_concrete(&self, ty: &Type) -> Option<Type> {
        if let Type::Named(name) = ty {
            let resolved = self
                .env
                .resolve_to_concrete(ty, &expr::simple_resolve_type_expr);
            if &resolved != ty {
                Some(resolved)
            } else {
                self.env.lookup(name).cloned()
            }
        } else {
            None
        }
    }

    /// Like `types_compatible` but treats `unknown` as a wildcard on BOTH sides.
    /// Used for match arm unification where `Result<unknown, E>` should unify
    /// with `Result<T, unknown>`.
    pub(crate) fn types_unifiable(&self, a: &Type, b: &Type) -> bool {
        if matches!(a, Type::Unknown | Type::Var(_)) || matches!(b, Type::Unknown | Type::Var(_)) {
            return true;
        }
        match (a, b) {
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => {
                self.types_unifiable(o1, o2) && self.types_unifiable(e1, e2)
            }
            (a, b) if a.is_option() && b.is_option() => {
                match (a.option_inner(), b.option_inner()) {
                    (Some(ai), Some(bi)) => self.types_unifiable(ai, bi),
                    _ => true,
                }
            }
            (Type::Promise(a), Type::Promise(b)) => self.types_unifiable(a, b),
            (Type::Array(a), Type::Array(b)) => self.types_unifiable(a, b),
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| self.types_unifiable(x, y))
            }
            _ => self.types_compatible(a, b),
        }
    }

    /// Merge two types by filling `unknown` holes from the other side.
    /// `Result<unknown, AuthError>` + `Result<Option<Session>, unknown>` →
    /// `Result<Option<Session>, AuthError>`
    pub(crate) fn merge_types(a: &Type, b: &Type) -> Type {
        match (a, b) {
            (Type::Unknown, _) => b.clone(),
            (_, Type::Unknown) => a.clone(),
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => Type::Result {
                ok: Box::new(Self::merge_types(o1, o2)),
                err: Box::new(Self::merge_types(e1, e2)),
            },
            (a, b) if a.is_option() && b.is_option() => {
                let a_inner = a.option_inner().cloned().unwrap_or(Type::Unknown);
                let b_inner = b.option_inner().cloned().unwrap_or(Type::Unknown);
                Type::option_of(Self::merge_types(&a_inner, &b_inner))
            }
            (Type::Promise(a_inner), Type::Promise(b_inner)) => {
                Type::Promise(Box::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Array(a_inner), Type::Array(b_inner)) => {
                Type::Array(Box::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Tuple(a_elems), Type::Tuple(b_elems)) if a_elems.len() == b_elems.len() => {
                Type::Tuple(
                    a_elems
                        .iter()
                        .zip(b_elems.iter())
                        .map(|(x, y)| Self::merge_types(x, y))
                        .collect(),
                )
            }
            _ => a.clone(),
        }
    }

    pub(crate) fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        // Unknown/Var as EXPECTED: anything can be assigned to unknown (widening)
        if matches!(expected, Type::Unknown | Type::Var(_)) {
            return true;
        }
        // Var as ACTUAL: type variables are still being inferred, allow them
        if matches!(actual, Type::Var(_)) {
            return true;
        }
        // Unknown as ACTUAL with concrete expected: NOT compatible.
        // Must narrow unknown before assigning to a concrete type.
        // (This is the key strictness rule — same as TypeScript's unknown.)

        // `never` is compatible with any type (it means "this code never returns")
        if matches!(actual, Type::Never) || matches!(expected, Type::Never) {
            return true;
        }

        // Generic type parameters (single uppercase letter like T, U, E, S)
        // are wildcards that match any type — used in stdlib function signatures
        if let Type::Named(n) = expected
            && n.len() == 1
            && n.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return true;
        }
        if let Type::Named(n) = actual
            && n.len() == 1
            && n.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        {
            return true;
        }

        // Foreign types (from npm imports) are assumed compatible since we can't
        // fully resolve type aliases across the npm boundary.
        if matches!(expected, Type::Foreign(_)) && !matches!(actual, Type::Unknown) {
            return true;
        }
        if matches!(actual, Type::Foreign(_)) && !matches!(expected, Type::Unknown) {
            return true;
        }

        // Opaque type: within the defining module, the underlying type
        // is assignable to the opaque type (e.g. returning `string` as `HashedPassword`).
        // Currently all code lives in a single file, so same-file = defining module.
        // Supports both `opaque type X { T }` (newtype) and `opaque type X = T` (alias).
        if let Type::Named(name) = expected
            && let Some(info) = self.env.lookup_type(name)
            && info.opaque
        {
            let underlying = match &info.def {
                crate::parser::ast::TypeDef::Alias(type_expr) => {
                    Some(expr::simple_resolve_type_expr(type_expr))
                }
                crate::parser::ast::TypeDef::Union(variants)
                    if variants.len() == 1
                        && variants[0].fields.len() == 1
                        && variants[0].fields[0].name.is_none() =>
                {
                    Some(expr::simple_resolve_type_expr(
                        &variants[0].fields[0].type_ann,
                    ))
                }
                _ => None,
            };
            if let Some(underlying) = underlying
                && self.types_compatible(&underlying, actual)
            {
                return true;
            }
        }

        // Resolve Named types to concrete for structural comparison
        let expected_concrete = self.resolve_named_to_concrete(expected);
        let actual_concrete = self.resolve_named_to_concrete(actual);

        // Named<->Record structural comparison
        if let Some(Type::Record(ref exp_fields)) = expected_concrete
            && let Type::Record(act_fields) = actual
        {
            return exp_fields.len() == act_fields.len()
                && exp_fields.iter().all(|(name, ty)| {
                    act_fields
                        .iter()
                        .any(|(n, t)| n == name && self.types_compatible(ty, t))
                });
        }
        if let Some(Type::Record(ref act_fields)) = actual_concrete
            && let Type::Record(exp_fields) = expected
        {
            return exp_fields.len() == act_fields.len()
                && exp_fields.iter().all(|(name, ty)| {
                    act_fields
                        .iter()
                        .any(|(n, t)| n == name && self.types_compatible(ty, t))
                });
        }

        match (expected, actual) {
            (Type::Number, Type::Number)
            | (Type::String, Type::String)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit)
            | (Type::Undefined, Type::Undefined) => true,
            (Type::Named(a), Type::Named(b)) => a == b,
            (Type::Named(a), Type::Union { name: b, .. })
            | (Type::Union { name: a, .. }, Type::Named(b)) => a == b,
            (Type::Union { name: a, .. }, Type::Union { name: b, .. }) => a == b,
            (Type::Result { ok: o1, err: e1 }, Type::Result { ok: o2, err: e2 }) => {
                self.types_compatible(o1, o2) && self.types_compatible(e1, e2)
            }
            (expected, actual) if expected.is_option() && actual.is_option() => {
                // None (Option<Unknown>) is compatible with any Option<T>
                if matches!(actual.option_inner(), Some(Type::Unknown)) {
                    return true;
                }
                match (expected.option_inner(), actual.option_inner()) {
                    (Some(e), Some(a)) => self.types_compatible(e, a),
                    _ => true,
                }
            }
            // A concrete value T is assignable to Option<T> (implicit Some wrapping)
            (expected, actual) if expected.is_option() => {
                if let Some(inner) = expected.option_inner() {
                    self.types_compatible(inner, actual)
                } else {
                    true
                }
            }
            (Type::Promise(a), Type::Promise(b)) => self.types_compatible(a, b),
            (Type::Settable(_), Type::Settable(b)) if matches!(**b, Type::Unknown) => {
                true // Clear/Unchanged (Settable<Unknown>) is compatible with any Settable<T>
            }
            (Type::Settable(a), Type::Settable(b)) => self.types_compatible(a, b),
            (Type::Array(_), Type::Array(b)) if matches!(**b, Type::Unknown) => {
                true // empty array [] is compatible with any Array<T>
            }
            (Type::Array(a), Type::Array(b)) => self.types_compatible(a, b),
            (
                Type::Map { key: k1, value: v1 } | Type::RecordMap { key: k1, value: v1 },
                Type::Map { key: k2, value: v2 } | Type::RecordMap { key: k2, value: v2 },
            ) => self.types_compatible(k1, k2) && self.types_compatible(v1, v2),
            (Type::Set { element: e1 }, Type::Set { element: e2 }) => self.types_compatible(e1, e2),
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| self.types_compatible(x, y))
            }
            (
                Type::Function {
                    params: p1,
                    return_type: r1,
                },
                Type::Function {
                    params: p2,
                    return_type: r2,
                },
            ) => {
                p1.len() == p2.len()
                    && p1
                        .iter()
                        .zip(p2.iter())
                        .all(|(x, y)| self.types_compatible(x, y))
                    && self.types_compatible(r1, r2)
            }
            // TsUnion vs TsUnion: every actual member must match at least one expected member
            (Type::TsUnion(exp_members), Type::TsUnion(act_members)) => act_members
                .iter()
                .all(|a| exp_members.iter().any(|e| self.types_compatible(e, a))),
            // TsUnion as expected: actual must match at least one member
            (Type::TsUnion(members), _) => members.iter().any(|m| self.types_compatible(m, actual)),
            // TsUnion as actual: every member must be compatible with expected
            (_, Type::TsUnion(members)) => {
                members.iter().all(|m| self.types_compatible(expected, m))
            }
            // Structural record compatibility: { a: T, b: U } matches { a: T, b: U }
            (Type::Record(fields_a), Type::Record(fields_b)) => {
                fields_a.len() == fields_b.len()
                    && fields_a.iter().all(|(name_a, ty_a)| {
                        fields_b.iter().any(|(name_b, ty_b)| {
                            name_a == name_b && self.types_compatible(ty_a, ty_b)
                        })
                    })
            }
            _ => false,
        }
    }
}
