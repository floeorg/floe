use std::sync::Arc;

use super::*;

// ── Match Exhaustiveness (delegated to exhaustiveness module) ────

impl Checker {
    pub(super) fn check_match_exhaustiveness(
        &mut self,
        subject_ty: &Type,
        arms: &[MatchArm],
        span: Span,
    ) {
        let resolve_named = |ty: &Type| {
            self.env
                .resolve_to_concrete(ty, &expr::simple_resolve_type_expr)
        };
        let diagnostics = crate::exhaustiveness::check_match_exhaustiveness(
            subject_ty,
            arms,
            span,
            &resolve_named,
        );
        for diag in diagnostics {
            self.problems.push(diag);
        }
    }

    // ── Pattern Checking ─────────────────────────────────────────

    pub(super) fn check_pattern(&mut self, pattern: &Pattern, subject_ty: &Type) {
        // Resolve Named types to their actual definitions via the type namespace for
        // structural checking (variant matching, field access, etc.). Keep the original
        // Named type for bindings so hover shows the declared type name, not expanded fields.
        // Keep Named if the resolved type is Unknown (foreign npm types with no definition).
        let resolved_ty;
        let binding_ty = subject_ty; // original type, used for env binding
        let subject_ty = if let Type::Named(_) = subject_ty {
            let concrete = self
                .env
                .resolve_to_concrete(subject_ty, &expr::simple_resolve_type_expr);
            if matches!(concrete, Type::Unknown) {
                subject_ty
            } else {
                resolved_ty = concrete;
                &resolved_ty
            }
        } else {
            subject_ty
        };

        match &pattern.kind {
            PatternKind::Wildcard | PatternKind::Range { .. } => {}
            PatternKind::Literal(lit) => {
                if !matches!(subject_ty, Type::Unknown | Type::Foreign(_)) {
                    let compatible = match lit {
                        LiteralPattern::Bool(_) => matches!(subject_ty, Type::Bool),
                        LiteralPattern::Number(_) => matches!(subject_ty, Type::Number),
                        LiteralPattern::String(_) => {
                            matches!(subject_ty, Type::String | Type::StringLiteral(_))
                                || subject_ty.as_string_literal_variants().is_some()
                        }
                    };
                    if !compatible {
                        let lit_desc = match lit {
                            LiteralPattern::Bool(_) => "boolean",
                            LiteralPattern::Number(_) => "number",
                            LiteralPattern::String(_) => "string",
                        };
                        self.emit_error_with_help(
                            format!("{lit_desc} literal pattern used on type `{subject_ty}`",),
                            pattern.span,
                            ErrorCode::LiteralPatternMismatch,
                            format!("expected `{subject_ty}`, found {lit_desc}"),
                            format!("use a `{subject_ty}` pattern or a `_` catch-all"),
                        );
                    }
                }
            }
            PatternKind::Variant { name, fields } => {
                let mut handled = false;
                if let Type::Union { variants, .. } = subject_ty
                    && let Some((_, field_types)) = variants.iter().find(|(n, _)| n == name)
                {
                    if fields.len() != field_types.len() {
                        self.emit_error_with_help(
                            format!(
                                "variant `{name}` pattern has {} field(s), but the variant has {}",
                                fields.len(),
                                field_types.len()
                            ),
                            pattern.span,
                            ErrorCode::VariantPatternArity,
                            "wrong number of fields",
                            format!(
                                "adjust the pattern to match all {} field(s) of `{name}`",
                                field_types.len()
                            ),
                        );
                    }
                    for (i, pat) in fields.iter().enumerate() {
                        let ty = field_types.get(i).unwrap_or(&Type::Unknown);
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
                if name != "_" && !matches!(subject_ty, Type::Unknown) {
                    let hint = match subject_ty {
                        Type::Bool => Some("use `true`, `false`, or `_` to match booleans"),
                        _ if subject_ty.is_option() => {
                            Some("use `Some(...)`, `None`, or `_` to match options")
                        }
                        Type::Settable(_) => Some(
                            "use `Value(...)`, `Clear`, `Unchanged`, or `_` to match settable types",
                        ),
                        Type::Union { .. } => Some("use variant names or `_` to match union types"),
                        _ if subject_ty.is_string_literal_union() => {
                            Some("use string literals or `_` to match string unions")
                        }
                        _ => None,
                    };
                    if let Some(help) = hint {
                        self.emit_warning_with_help(
                            format!(
                                "`{name}` binds the entire value as a catch-all on type `{subject_ty}`",
                            ),
                            pattern.span,
                            ErrorCode::SuspiciousBinding,
                            "this name captures the matched value, it doesn't check it",
                            help,
                        );
                    }
                }
                self.env.define(name, binding_ty.clone());
                self.name_types.insert(name.clone(), binding_ty.to_string());
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
                        Type::Array(Arc::new(Type::Unknown))
                    };
                    self.env.define(name, rest_ty.clone());
                    self.name_types.insert(name.clone(), rest_ty.to_string());
                }
            }
        }
    }
}
