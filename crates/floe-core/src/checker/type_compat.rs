use std::sync::Arc;

use super::*;

/// Split a Foreign type name like `Foo<a, b<c>>` into a (base, args) pair
/// where args are the top-level comma-separated segments. Returns `None`
/// when the name carries no generic arguments.
fn split_foreign_name(foreign_name: &str) -> Option<(&str, Vec<&str>)> {
    let open = foreign_name.find('<')?;
    let base = &foreign_name[..open];
    let inner = foreign_name.get(open + 1..foreign_name.len() - 1)?;
    let mut depth = 0;
    let mut start = 0;
    let mut args: Vec<&str> = Vec::new();
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some((base, args))
}

/// True if a Foreign type name like `Router<E>` contains an arg that looks
/// like an unresolved type parameter (single uppercase letter). Used to stay
/// permissive for same-base-name Foreigns when the checker hasn't yet
/// substituted a type parameter through the call chain.
fn has_type_param_arg(foreign_name: &str) -> bool {
    let Some((_, args)) = split_foreign_name(foreign_name) else {
        return false;
    };
    args.iter()
        .any(|a| a.len() == 1 && a.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

impl Checker {
    /// Pad a Foreign type name with .d.ts-declared default type parameters.
    /// Returns `None` when no padding applied (either the name isn't in the
    /// registry, its arg count already matches the declaration, or there
    /// are no trailing defaults to supply). Callers should fall back to
    /// the original string when `None` is returned.
    fn normalize_foreign_name(&self, name: &str) -> Option<String> {
        let (base, args) = split_foreign_name(name)?;
        let param_infos = self.dts_generic_params.get(base)?;
        if args.len() >= param_infos.len() {
            return None;
        }
        let mut padded: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        for info in &param_infos[args.len()..] {
            let Some(default_ts) = &info.default else {
                break;
            };
            padded.push(crate::interop::wrap_boundary_type(default_ts).to_string());
        }
        if padded.len() == args.len() {
            return None;
        }
        Some(format!("{}<{}>", base, padded.join(", ")))
    }

    /// Unfold a `Type::Named` that resolves to a `TypeDef::Alias` (structural
    /// alias). Nominal defs (record / union / newtype / opaque) and names that
    /// don't refer to a user-declared type return `None`.
    pub(crate) fn unfold_structural_alias(&self, ty: &Type) -> Option<Type> {
        let Type::Named(name) = ty else {
            return None;
        };
        let info = self.env.lookup_type(name)?;
        if info.opaque {
            return None;
        }
        if !matches!(info.def, crate::parser::ast::TypeDef::Alias(_)) {
            return None;
        }
        Some(
            self.env
                .resolve_to_concrete(ty, &expr::simple_resolve_type_expr),
        )
    }

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
        if a.is_undetermined() || b.is_undetermined() {
            return true;
        }
        match (a, b) {
            (a, b) if a.is_result() && b.is_result() => {
                match (a.result_ok(), a.result_err(), b.result_ok(), b.result_err()) {
                    (Some(o1), Some(e1), Some(o2), Some(e2)) => {
                        self.types_unifiable(o1, o2) && self.types_unifiable(e1, e2)
                    }
                    _ => true,
                }
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
            (Type::Unknown | Type::Error, _) => b.clone(),
            (_, Type::Unknown | Type::Error) => a.clone(),
            (a, b) if a.is_result() && b.is_result() => {
                let ok = Self::merge_types(
                    a.result_ok().unwrap_or(&Type::Unknown),
                    b.result_ok().unwrap_or(&Type::Unknown),
                );
                let err = Self::merge_types(
                    a.result_err().unwrap_or(&Type::Unknown),
                    b.result_err().unwrap_or(&Type::Unknown),
                );
                Type::result_of(ok, err)
            }
            (a, b) if a.is_option() && b.is_option() => {
                let a_inner = a.option_inner().cloned().unwrap_or(Type::Unknown);
                let b_inner = b.option_inner().cloned().unwrap_or(Type::Unknown);
                Type::option_of(Self::merge_types(&a_inner, &b_inner))
            }
            (Type::Promise(a_inner), Type::Promise(b_inner)) => {
                Type::Promise(Arc::new(Self::merge_types(a_inner, b_inner)))
            }
            (Type::Array(a_inner), Type::Array(b_inner)) => {
                Type::Array(Arc::new(Self::merge_types(a_inner, b_inner)))
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

    /// Check if an actual record is compatible with an expected record.
    /// Fields with Settable or Option types can be omitted (default to Unchanged/None).
    fn records_compatible(&self, expected: &[(String, Type)], actual: &[(String, Type)]) -> bool {
        // Empty expected record (from unresolved generics in npm types) accepts any record
        if expected.is_empty() && !actual.is_empty() {
            return true;
        }
        // Every expected field must either match an actual field or be omittable
        expected.iter().all(|(name, ty)| {
            if let Some((_, act_ty)) = actual.iter().find(|(n, _)| n == name) {
                if self.types_compatible(ty, act_ty) {
                    true
                } else if let Type::Settable(inner) = ty {
                    // Settable<T> accepts T directly (user provides value, not Settable wrapper)
                    self.types_compatible(inner, act_ty)
                } else {
                    false
                }
            } else {
                // Field omitted — OK if it's Settable or Option
                ty.is_settable() || ty.is_option()
            }
        })
        // No extra fields in actual that aren't in expected
        && actual.iter().all(|(name, _)| {
            expected.iter().any(|(n, _)| n == name)
        })
    }

    pub(crate) fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        // Error in either position: suppress cascading errors by accepting any type
        if matches!(expected, Type::Error) || matches!(actual, Type::Error) {
            return true;
        }
        // Unknown/Var as EXPECTED: anything can be assigned to unknown (widening)
        if matches!(expected, Type::Unknown | Type::Var(_)) {
            return true;
        }
        // Var as ACTUAL: type variables are still being inferred, allow them
        if matches!(actual, Type::Var(_)) {
            return true;
        }
        // Unknown as actual with concrete expected: not compatible (same as TypeScript's unknown)
        if matches!(actual, Type::Unknown) {
            return false;
        }

        // `never` is compatible with any type (it means "this code never returns")
        if matches!(actual, Type::Never) || matches!(expected, Type::Never) {
            return true;
        }

        // Structural alias unfolding: `type F = () -> string` is a structural
        // alias per docs/design.md, so `Named("F")` must unfold to its RHS
        // before comparison. Nominal types (record defs, tagged unions,
        // newtypes, opaque) stay nominal — only `TypeDef::Alias` unfolds.
        if let Some(unfolded) = self.unfold_structural_alias(expected) {
            return self.types_compatible(&unfolded, actual);
        }
        if let Some(unfolded) = self.unfold_structural_alias(actual) {
            return self.types_compatible(expected, &unfolded);
        }

        // Result<T, E> is never compatible with a non-Result expected type.
        // Users must unwrap with `?` or handle with `match`.
        if actual.is_result() && !expected.is_result() {
            return false;
        }

        // Option<T> is never compatible with a non-Option expected type.
        // Users must unwrap with `match` or pattern binding. TsUnion
        // delegates to the per-member recursion further down.
        if actual.is_option() && !expected.is_option() && !matches!(expected, Type::TsUnion(_)) {
            return false;
        }

        // Foreign types: reject primitives, permissive otherwise.
        // Foreign-vs-Foreign is permissive because npm types often have subtype
        // relationships (e.g. SQLiteColumn extends SQLWrapper) that Floe can't verify —
        // EXCEPT when both sides share a base name with fully-resolved (non
        // type-parameter) generic args: `Router<A>` and `Router<B>` are the
        // same class with different instantiations and must not be freely
        // interchangeable. If either side still contains an unresolved
        // type-parameter placeholder (single uppercase letter like `E`, `T`,
        // a leftover from a pipe/generic-inference gap), fall back to
        // permissive because we can't yet decide equivalence. Bare-vs-encoded
        // (e.g. `Context` vs `Context<{ Bindings: Bindings }>`) is the same
        // situation one step earlier: bare Foreigns only arise when the
        // resolver stripped args containing an active type parameter, so the
        // real question is whether `Bindings` would unify with that stripped
        // param — which can't be answered at compat time from a name string.
        if let (Type::Foreign { name: e_name, .. }, Type::Foreign { name: a_name, .. }) =
            (expected, actual)
        {
            // Normalize both sides against the .d.ts default-type-parameter
            // registry so a user's 1-arg `Foo<A>` can match a library's
            // 3-arg `Foo<A, any, {}>` when the declaration supplies the
            // missing defaults.
            let e_normalized = self.normalize_foreign_name(e_name);
            let a_normalized = self.normalize_foreign_name(a_name);
            let e_ref = e_normalized.as_deref().unwrap_or(e_name);
            let a_ref = a_normalized.as_deref().unwrap_or(a_name);
            let e_base = e_ref.split('<').next().unwrap_or(e_ref);
            let a_base = a_ref.split('<').next().unwrap_or(a_ref);
            if e_base == a_base {
                let e_has_args = e_ref.contains('<');
                let a_has_args = a_ref.contains('<');
                if e_has_args != a_has_args {
                    return true;
                }
                if !has_type_param_arg(e_ref) && !has_type_param_arg(a_ref) {
                    return e_ref == a_ref;
                }
            }
            return true;
        }
        if let Type::Foreign { .. } = expected {
            return !actual.is_primitive();
        }
        if let Type::Foreign { .. } = actual {
            return !expected.is_primitive();
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

        // Nominal: a Floe Named type is only compatible with the same Named type.
        // Structural matching only applies when the *expected* side is an anonymous
        // Record (inline type annotation or foreign .d.ts object type) — a Floe Named
        // type can satisfy it by shape.
        let actual_concrete = self.resolve_named_to_concrete(actual);

        if let Some(Type::Record(ref act_fields)) = actual_concrete
            && let Type::Record(exp_fields) = expected
        {
            return self.records_compatible(exp_fields, act_fields);
        }

        // TsUnion as actual: every member must be compatible with expected.
        // Pulled out as an early exit so the match below doesn't need to repeat this in every arm.
        if let Type::TsUnion(members) = actual {
            return members.iter().all(|m| self.types_compatible(expected, m));
        }

        // Match on `expected` explicitly so that adding a new Type variant causes a compile
        // error here, forcing the developer to decide how it interacts with other types.
        // Variants already handled by early-return guards above are marked unreachable.
        match expected {
            // Caught by early guards above — cannot reach here
            Type::Error | Type::Unknown | Type::Var(_) | Type::Never | Type::Foreign { .. } => {
                unreachable!("handled by early guards in types_compatible")
            }

            Type::Number => matches!(actual, Type::Number),
            Type::String => matches!(actual, Type::String | Type::StringLiteral(_)),
            Type::Bool => matches!(actual, Type::Bool),
            Type::Unit => matches!(actual, Type::Unit),
            Type::Undefined => matches!(actual, Type::Undefined),
            Type::StringLiteral(s) => matches!(actual, Type::StringLiteral(t) if t == s),

            Type::Named(exp_name) => match actual {
                Type::Named(act_name) => act_name == exp_name,
                Type::Union { name, .. } => name == exp_name,
                _ => false,
            },

            Type::Union { name: exp_name, .. } if expected.is_result() => {
                if !actual.is_result() {
                    return false; // already enforced by the early guard above
                }
                let ok_compat = match (expected.result_ok(), actual.result_ok()) {
                    (Some(e), Some(a)) => self.types_compatible(e, a),
                    _ => true,
                };
                let err_compat = match (expected.result_err(), actual.result_err()) {
                    (Some(e), Some(a)) => self.types_compatible(e, a),
                    _ => true,
                };
                ok_compat && err_compat
            }
            Type::Union { .. } if expected.is_option() && actual.is_option() => {
                // None (Option<Unknown>) is compatible with any Option<T>
                if matches!(actual.option_inner(), Some(Type::Unknown)) {
                    return true;
                }
                match (expected.option_inner(), actual.option_inner()) {
                    (Some(e), Some(a)) => self.types_compatible(e, a),
                    _ => true,
                }
            }
            Type::Union { name: exp_name, .. } => match actual {
                Type::Named(n) => n == exp_name,
                Type::Union { name: n, .. } => n == exp_name,
                _ => false,
            },

            Type::Promise(a) => matches!(actual, Type::Promise(b) if self.types_compatible(a, b)),

            Type::Settable(a) => match actual {
                // Clear/Unchanged (Settable<Unknown>) is compatible with any Settable<T>
                Type::Settable(b) if matches!(**b, Type::Unknown) => true,
                Type::Settable(b) => self.types_compatible(a, b),
                _ => false,
            },

            Type::Array(a) => match actual {
                // Empty array [] (Array<Unknown>) is compatible with any Array<T>
                Type::Array(b) if matches!(**b, Type::Unknown) => true,
                Type::Array(b) => self.types_compatible(a, b),
                _ => false,
            },

            Type::Map { key: k1, value: v1 } | Type::RecordMap { key: k1, value: v1 } => {
                match actual {
                    Type::Map { key: k2, value: v2 } | Type::RecordMap { key: k2, value: v2 } => {
                        self.types_compatible(k1, k2) && self.types_compatible(v1, v2)
                    }
                    _ => false,
                }
            }

            Type::Set { element: e1 } => match actual {
                Type::Set { element: e2 } => self.types_compatible(e1, e2),
                _ => false,
            },

            Type::Tuple(a) => match actual {
                Type::Tuple(b) => {
                    a.len() == b.len()
                        && a.iter()
                            .zip(b.iter())
                            .all(|(x, y)| self.types_compatible(x, y))
                }
                _ => false,
            },

            Type::Function {
                params: p1,
                return_type: r1,
                ..
            } => match actual {
                Type::Function {
                    params: p2,
                    return_type: r2,
                    ..
                } => {
                    p1.len() == p2.len()
                        && p1
                            .iter()
                            .zip(p2.iter())
                            .all(|(x, y)| self.types_compatible(x, y))
                        && self.types_compatible(r1, r2)
                }
                _ => false,
            },

            // TsUnion as expected: actual must match at least one member.
            // TsUnion as actual is handled by the early exit above.
            Type::TsUnion(exp_members) => match actual {
                Type::TsUnion(act_members) => act_members
                    .iter()
                    .all(|a| exp_members.iter().any(|e| self.types_compatible(e, a))),
                _ => exp_members.iter().any(|m| self.types_compatible(m, actual)),
            },

            Type::Record(fields_a) => match actual {
                Type::Record(fields_b) => self.records_compatible(fields_a, fields_b),
                _ => false,
            },

            // Opaque is never constructed by the checker (opaque types use Type::Named
            // with the `info.opaque` flag). Listed explicitly to enforce exhaustiveness.
            Type::Opaque { name: exp_name, .. } => {
                matches!(actual, Type::Opaque { name: act_name, .. } if act_name == exp_name)
            }
        }
    }
}
