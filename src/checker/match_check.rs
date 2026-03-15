use std::collections::HashSet;

use super::*;

// ── Match Exhaustiveness ─────────────────────────────────────

impl Checker {
    pub(super) fn check_match_exhaustiveness(
        &mut self,
        subject_ty: &Type,
        arms: &[MatchArm],
        span: Span,
    ) {
        // Resolve Named types to their actual definitions
        let resolved_ty;
        let subject_ty = if let Type::Named(type_name) = subject_ty {
            if let Some(actual) = self.env.lookup(type_name) {
                resolved_ty = actual.clone();
                &resolved_ty
            } else {
                subject_ty
            }
        } else {
            subject_ty
        };

        let has_catch_all = arms.iter().any(|arm| {
            matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            )
        });

        if has_catch_all {
            return;
        }

        // For union types, check that all variants are covered
        if let Type::Union { name, variants } = subject_ty {
            let variant_names: HashSet<&str> = variants.iter().map(|(n, _)| n.as_str()).collect();
            let mut covered: HashSet<&str> = HashSet::new();

            for arm in arms {
                if let PatternKind::Variant { name, .. } = &arm.pattern.kind {
                    covered.insert(name.as_str());
                }
            }

            let missing: Vec<_> = variant_names.difference(&covered).collect();
            if !missing.is_empty() {
                let missing_str = missing
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on `{name}`: missing {missing_str}"),
                        span,
                    )
                    .with_label("not all variants covered")
                    .with_help("add match arms for the missing variants, or add a `_ ->` catch-all")
                    .with_code("E004"),
                );
            }
        }

        // For Result types, check Ok and Err are covered
        if subject_ty.is_result() {
            let mut has_ok = false;
            let mut has_err = false;
            for arm in arms {
                if let PatternKind::Variant { name, .. } = &arm.pattern.kind {
                    match name.as_str() {
                        "Ok" => has_ok = true,
                        "Err" => has_err = true,
                        _ => {}
                    }
                }
            }
            if !has_ok || !has_err {
                let missing = match (has_ok, has_err) {
                    (false, false) => "`Ok` and `Err`",
                    (false, true) => "`Ok`",
                    (true, false) => "`Err`",
                    _ => unreachable!(),
                };
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on `Result`: missing {missing}"),
                        span,
                    )
                    .with_label("not all cases covered")
                    .with_help("add match arms for the missing cases")
                    .with_code("E004"),
                );
            }
        }

        // For Option types, check Some and None are covered
        if subject_ty.is_option() {
            let mut has_some = false;
            let mut has_none = false;
            for arm in arms {
                match &arm.pattern.kind {
                    PatternKind::Variant { name, .. } if name == "Some" => has_some = true,
                    PatternKind::Variant { name, .. } if name == "None" => has_none = true,
                    _ => {}
                }
            }
            if !has_some || !has_none {
                let missing = match (has_some, has_none) {
                    (false, false) => "`Some` and `None`",
                    (false, true) => "`Some`",
                    (true, false) => "`None`",
                    _ => unreachable!(),
                };
                self.diagnostics.push(
                    Diagnostic::error(
                        format!("non-exhaustive match on `Option`: missing {missing}"),
                        span,
                    )
                    .with_label("not all cases covered")
                    .with_help("add match arms for the missing cases")
                    .with_code("E004"),
                );
            }
        }

        // For bool, check true/false covered
        if matches!(subject_ty, Type::Bool) {
            let mut has_true = false;
            let mut has_false = false;
            for arm in arms {
                if let PatternKind::Literal(LiteralPattern::Bool(b)) = &arm.pattern.kind {
                    if *b {
                        has_true = true;
                    } else {
                        has_false = true;
                    }
                }
            }
            if !has_true || !has_false {
                self.diagnostics.push(
                    Diagnostic::error("non-exhaustive match on `boolean`: missing a case", span)
                        .with_label("not all cases covered")
                        .with_help("add match arms for both `true` and `false`")
                        .with_code("E004"),
                );
            }
        }
    }

    // ── Pattern Checking ─────────────────────────────────────────

    pub(super) fn check_pattern(&mut self, pattern: &Pattern, subject_ty: &Type) {
        // Resolve Named types to their actual definitions for pattern matching
        let resolved_ty;
        let subject_ty = if let Type::Named(type_name) = subject_ty {
            if let Some(actual) = self.env.lookup(type_name) {
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
                if let Type::Union { variants, .. } = subject_ty
                    && let Some((_, field_types)) = variants.iter().find(|(n, _)| n == name)
                {
                    for (pat, ty) in fields.iter().zip(field_types.iter()) {
                        self.check_pattern(pat, ty);
                    }
                }
                if let Type::Result { ok, err } = subject_ty {
                    match name.as_str() {
                        "Ok" => {
                            if let Some(pat) = fields.first() {
                                self.check_pattern(pat, ok);
                            }
                        }
                        "Err" => {
                            if let Some(pat) = fields.first() {
                                self.check_pattern(pat, err);
                            }
                        }
                        _ => {}
                    }
                }
                if let Type::Option(inner) = subject_ty
                    && name == "Some"
                    && let Some(pat) = fields.first()
                {
                    self.check_pattern(pat, inner);
                }
            }
            PatternKind::Record { fields } => {
                for (_, pat) in fields {
                    self.check_pattern(pat, &Type::Unknown);
                }
            }
            PatternKind::Binding(name) => {
                self.env.define(name, subject_ty.clone());
            }
        }
    }
}
