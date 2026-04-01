use std::collections::HashSet;

use super::*;

/// Represents a concrete value in a single slot of a tuple's product space.
enum TupleSlotValue {
    Bool(bool),
    Variant(String),
    StringLiteral(String),
}

/// Collect variant names covered by unguarded arms.
fn covered_variant_names(arms: &[MatchArm]) -> HashSet<&str> {
    arms.iter()
        .filter(|arm| arm.guard.is_none())
        .filter_map(|arm| match &arm.pattern.kind {
            PatternKind::Variant { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// Check that all `required` variant names are covered, emit E004 if not.
fn check_required_variants(
    checker: &mut Checker,
    type_name: &str,
    required: &[&str],
    arms: &[MatchArm],
    span: Span,
) {
    let covered = covered_variant_names(arms);
    let missing: Vec<&&str> = required.iter().filter(|r| !covered.contains(**r)).collect();
    if !missing.is_empty() {
        let missing_str = missing
            .iter()
            .map(|s| format!("`{s}`"))
            .collect::<Vec<_>>()
            .join(" and ");
        checker.emit_error_with_help(
            format!("non-exhaustive match on `{type_name}`: missing {missing_str}"),
            span,
            ErrorCode::NonExhaustiveMatch,
            "not all cases covered",
            "add match arms for the missing cases",
        );
    }
}

// ── Match Exhaustiveness ─────────────────────────────────────

impl Checker {
    pub(super) fn check_match_exhaustiveness(
        &mut self,
        subject_ty: &Type,
        arms: &[MatchArm],
        span: Span,
    ) {
        // Resolve Named types to their actual definitions.
        let resolved_ty;
        let subject_ty = match subject_ty {
            Type::Foreign(_) | Type::Promise(_) => subject_ty,
            Type::Named(type_name) => {
                if let Some(actual) = self.env.lookup(type_name) {
                    resolved_ty = actual.clone();
                    &resolved_ty
                } else {
                    subject_ty
                }
            }
            _ => subject_ty,
        };

        let has_catch_all = arms.iter().any(|arm| {
            arm.guard.is_none()
                && matches!(
                    arm.pattern.kind,
                    PatternKind::Wildcard | PatternKind::Binding(_)
                )
        });

        if has_catch_all {
            return;
        }

        // Union types: check all variants covered
        if let Type::Union { name, variants } = subject_ty {
            let variant_names: HashSet<&str> = variants.iter().map(|(n, _)| n.as_str()).collect();
            let covered = covered_variant_names(arms);
            let missing: Vec<_> = variant_names.difference(&covered).collect();
            if !missing.is_empty() {
                let missing_str = missing
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.emit_error_with_help(
                    format!("non-exhaustive match on `{name}`: missing {missing_str}"),
                    span,
                    ErrorCode::NonExhaustiveMatch,
                    "not all variants covered",
                    "add match arms for the missing variants, or add a `_ ->` catch-all",
                );
            }
        }

        // String literal unions: check all string variants covered
        if let Some(string_variants) = subject_ty.as_string_literal_variants() {
            let variant_set: HashSet<&str> = string_variants.into_iter().collect();
            let mut covered: HashSet<&str> = HashSet::new();
            for arm in arms {
                if arm.guard.is_none()
                    && let PatternKind::Literal(LiteralPattern::String(s)) = &arm.pattern.kind
                {
                    covered.insert(s.as_str());
                }
            }
            let missing: Vec<_> = variant_set.difference(&covered).collect();
            if !missing.is_empty() {
                let type_name = subject_ty.to_string();
                let missing_str = missing
                    .iter()
                    .map(|s| format!("`\"{s}\"`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.emit_error_with_help(
                    format!("non-exhaustive match on `{type_name}`: missing {missing_str}"),
                    span,
                    ErrorCode::NonExhaustiveMatch,
                    "not all variants covered",
                    "add match arms for the missing variants, or add a `_ ->` catch-all",
                );
            }
        }

        // Option: check Some and None covered
        if subject_ty.is_option() {
            check_required_variants(
                self,
                crate::type_layout::TYPE_OPTION,
                &[
                    crate::type_layout::VARIANT_SOME,
                    crate::type_layout::VARIANT_NONE,
                ],
                arms,
                span,
            );
        }

        // Settable: check Value, Clear, Unchanged covered
        if subject_ty.is_settable() {
            check_required_variants(
                self,
                "Settable",
                &["Value", "Clear", "Unchanged"],
                arms,
                span,
            );
        }

        // Array: check empty + non-empty covered
        if matches!(subject_ty, Type::Array(_)) {
            let mut has_empty = false;
            let mut has_nonempty_rest = false;
            for arm in arms {
                if arm.guard.is_some() {
                    continue;
                }
                if let PatternKind::Array { elements, rest } = &arm.pattern.kind {
                    if elements.is_empty() && rest.is_none() {
                        has_empty = true;
                    }
                    if rest.is_some() {
                        has_nonempty_rest = true;
                    }
                }
            }
            let has_any_array_pattern = arms
                .iter()
                .any(|a| matches!(a.pattern.kind, PatternKind::Array { .. }));
            if has_any_array_pattern && !(has_empty && has_nonempty_rest) {
                let missing = match (has_empty, has_nonempty_rest) {
                    (false, false) => "empty array `[]` and non-empty array `[_, .._]`",
                    (false, true) => "empty array `[]`",
                    (true, false) => "non-empty array `[_, .._]`",
                    _ => unreachable!(),
                };
                self.emit_error_with_help(
                    format!("non-exhaustive match on array: missing {missing}"),
                    span,
                    ErrorCode::NonExhaustiveMatch,
                    "not all cases covered",
                    "add match arms for both `[]` and `[_, ..rest]`, or add a `_ ->` catch-all",
                );
            }
        }

        // Bool: check true/false covered
        if matches!(subject_ty, Type::Bool) {
            let mut has_true = false;
            let mut has_false = false;
            for arm in arms {
                if arm.guard.is_none()
                    && let PatternKind::Literal(LiteralPattern::Bool(b)) = &arm.pattern.kind
                {
                    if *b {
                        has_true = true;
                    } else {
                        has_false = true;
                    }
                }
            }
            if !has_true || !has_false {
                self.emit_error_with_help(
                    "non-exhaustive match on `boolean`: missing a case",
                    span,
                    ErrorCode::NonExhaustiveMatch,
                    "not all cases covered",
                    "add match arms for both `true` and `false`",
                );
            }
        }

        // Number/String: require catch-all (infinite values)
        if matches!(subject_ty, Type::Number) {
            self.emit_error_with_help(
                "non-exhaustive match on `number`: cannot cover all values without a catch-all",
                span,
                ErrorCode::NonExhaustiveMatch,
                "number type has infinite values",
                "add a `_ ->` catch-all arm",
            );
        }
        if matches!(subject_ty, Type::String) {
            self.emit_error_with_help(
                "non-exhaustive match on `string`: cannot cover all values without a catch-all",
                span,
                ErrorCode::NonExhaustiveMatch,
                "string type has infinite values",
                "add a `_ ->` catch-all arm",
            );
        }

        // Tuple: check product space exhaustiveness
        if let Type::Tuple(elem_types) = subject_ty
            && !self.check_tuple_exhaustiveness(elem_types, arms)
        {
            self.emit_error_with_help(
                "non-exhaustive match on tuple: not all combinations are covered",
                span,
                ErrorCode::NonExhaustiveMatch,
                "not all cases covered",
                "add match arms for the missing combinations, or add a `_ ->` catch-all",
            );
        }
    }
    fn check_tuple_exhaustiveness(&self, elem_types: &[Type], arms: &[MatchArm]) -> bool {
        // If any arm is a top-level catch-all (wildcard, binding, or tuple of all wildcards/bindings),
        // the match is exhaustive regardless of element types.
        let has_catch_all = arms.iter().any(|arm| {
            if arm.guard.is_some() {
                return false;
            }
            match &arm.pattern.kind {
                PatternKind::Wildcard | PatternKind::Binding(_) => true,
                PatternKind::Tuple(patterns) => patterns
                    .iter()
                    .all(|p| matches!(p.kind, PatternKind::Wildcard | PatternKind::Binding(_))),
                _ => false,
            }
        });
        if has_catch_all {
            return true;
        }

        // Collect the possible values for each position
        let possible: Vec<Option<Vec<TupleSlotValue>>> = elem_types
            .iter()
            .map(|ty| self.finite_values_for_type(ty))
            .collect();

        // If any element type is unbounded, we can't prove exhaustiveness without a catch-all
        if possible.iter().any(|p| p.is_none()) {
            return false;
        }

        let possible = possible.into_iter().map(|p| p.unwrap()).collect::<Vec<_>>();

        // Generate all combinations (product space) and check each is covered
        let mut combo: Vec<usize> = vec![0; elem_types.len()];
        loop {
            // Check if this combination is covered by some arm
            let covered = arms.iter().any(|arm| {
                if arm.guard.is_some() {
                    return false;
                }
                match &arm.pattern.kind {
                    PatternKind::Tuple(patterns) if patterns.len() == elem_types.len() => patterns
                        .iter()
                        .enumerate()
                        .all(|(i, pat)| self.pattern_covers_value(pat, &possible[i][combo[i]])),
                    PatternKind::Wildcard | PatternKind::Binding(_) => true,
                    _ => false,
                }
            });
            if !covered {
                return false;
            }

            // Advance to next combination
            let mut pos = combo.len();
            loop {
                if pos == 0 {
                    return true; // all combinations checked
                }
                pos -= 1;
                combo[pos] += 1;
                if combo[pos] < possible[pos].len() {
                    break;
                }
                combo[pos] = 0;
                if pos == 0 {
                    return true; // wrapped around, done
                }
            }
        }
    }

    /// Returns the finite set of values for a type, or None if unbounded.
    fn finite_values_for_type(&self, ty: &Type) -> Option<Vec<TupleSlotValue>> {
        // Resolve named types
        let resolved;
        let ty = if let Type::Named(name) = ty {
            if let Some(actual) = self.env.lookup(name) {
                resolved = actual.clone();
                &resolved
            } else {
                return None;
            }
        } else {
            ty
        };

        match ty {
            Type::Bool => Some(vec![
                TupleSlotValue::Bool(true),
                TupleSlotValue::Bool(false),
            ]),
            Type::Union { variants, .. } => Some(
                variants
                    .iter()
                    .map(|(name, _)| TupleSlotValue::Variant(name.clone()))
                    .collect(),
            ),
            _ if ty.as_string_literal_variants().is_some() => {
                let variants = ty.as_string_literal_variants().unwrap();
                Some(
                    variants
                        .into_iter()
                        .map(|s| TupleSlotValue::StringLiteral(s.to_string()))
                        .collect(),
                )
            }
            _ => None, // number, string, etc. are unbounded
        }
    }

    /// Check if a pattern covers a specific value from the product space.
    fn pattern_covers_value(&self, pattern: &Pattern, value: &TupleSlotValue) -> bool {
        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Binding(_) => true,
            PatternKind::Literal(LiteralPattern::Bool(b)) => {
                matches!(value, TupleSlotValue::Bool(v) if v == b)
            }
            PatternKind::Literal(LiteralPattern::String(s)) => {
                matches!(value, TupleSlotValue::StringLiteral(v) if v == s)
            }
            PatternKind::Variant { name, .. } => {
                matches!(value, TupleSlotValue::Variant(v) if v == name)
            }
            _ => false,
        }
    }

    // ── Pattern Checking ─────────────────────────────────────────

    pub(super) fn check_pattern(&mut self, pattern: &Pattern, subject_ty: &Type) {
        // Resolve Named types to their actual definitions for pattern matching
        let resolved_ty;
        let subject_ty = if let Type::Named(type_name) = subject_ty {
            // Resolve Named types to their definitions, but keep Named if the
            // env value is Unknown (foreign npm types that have no definition)
            if let Some(actual) = self.env.lookup(type_name)
                && !matches!(actual, Type::Unknown)
            {
                resolved_ty = actual.clone();
                &resolved_ty
            } else {
                subject_ty
            }
        } else {
            subject_ty
        };

        match &pattern.kind {
            PatternKind::Literal(_) | PatternKind::Range { .. } | PatternKind::Wildcard => {}
            PatternKind::Variant { name, fields } => {
                let mut handled = false;
                if let Type::Union { variants, .. } = subject_ty
                    && let Some((_, field_types)) = variants.iter().find(|(n, _)| n == name)
                {
                    for (pat, ty) in fields.iter().zip(field_types.iter()) {
                        self.check_pattern(pat, ty);
                    }
                    handled = true;
                }
                if subject_ty.is_option()
                    && name == crate::type_layout::VARIANT_SOME
                    && let Some(pat) = fields.first()
                    && let Some(inner) = subject_ty.option_inner()
                {
                    self.check_pattern(pat, inner);
                    handled = true;
                }
                // Fallback: when subject type is Unknown (e.g. from npm imports),
                // still register bindings so they're available in the arm body
                if !handled {
                    for pat in fields {
                        self.check_pattern(pat, &Type::Unknown);
                    }
                }
            }
            PatternKind::Record { fields } => {
                for (_, pat) in fields {
                    self.check_pattern(pat, &Type::Unknown);
                }
            }
            PatternKind::StringPattern { segments } => {
                // String patterns require the subject to be a string type
                if !matches!(subject_ty, Type::String | Type::Unknown) {
                    self.emit_error(
                        format!("string pattern used on non-string type `{}`", subject_ty),
                        pattern.span,
                        ErrorCode::StringPatternOnNonString,
                        "expected string type",
                    );
                }
                // Bind all captured variables as string
                for segment in segments {
                    if let StringPatternSegment::Capture(name) = segment {
                        self.env.define(name, Type::String);
                        self.name_types.insert(name.clone(), "string".to_string());
                    }
                }
            }
            PatternKind::Binding(name) => {
                self.env.define(name, subject_ty.clone());
                self.name_types.insert(name.clone(), subject_ty.to_string());
            }
            PatternKind::Tuple(patterns) => {
                if let Type::Tuple(types) = subject_ty {
                    if patterns.len() != types.len() {
                        self.emit_error_with_help(
                            format!(
                                "tuple pattern has {} element(s), but the matched tuple has {}",
                                patterns.len(),
                                types.len()
                            ),
                            pattern.span,
                            ErrorCode::TuplePatternArity,
                            "wrong number of elements",
                            format!(
                                "adjust the pattern to match all {} elements of the tuple",
                                types.len()
                            ),
                        );
                    }
                    for (i, pat) in patterns.iter().enumerate() {
                        let ty = types.get(i).unwrap_or(&Type::Unknown);
                        self.check_pattern(pat, ty);
                    }
                } else {
                    for pat in patterns {
                        self.check_pattern(pat, &Type::Unknown);
                    }
                }
            }
            PatternKind::Array { elements, rest } => {
                // Determine element type from subject
                let elem_ty = if let Type::Array(inner) = subject_ty {
                    inner.as_ref().clone()
                } else {
                    Type::Unknown
                };

                // Bind each element pattern
                for pat in elements {
                    self.check_pattern(pat, &elem_ty);
                }

                // Bind rest as array of same element type
                if let Some(name) = rest
                    && name != "_"
                {
                    let rest_ty = if let Type::Array(_) = subject_ty {
                        subject_ty.clone()
                    } else {
                        Type::Array(Box::new(Type::Unknown))
                    };
                    self.env.define(name, rest_ty.clone());
                    self.name_types.insert(name.clone(), rest_ty.to_string());
                }
            }
        }
    }
}
