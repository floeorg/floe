//! Match exhaustiveness checking.
//!
//! Operates on typed match expressions as a post-type-check pass.
//! Takes a subject type and match arms, returns diagnostics for
//! missing patterns. Does not modify the AST or environment.

use std::collections::HashSet;

use crate::checker::Type;
use crate::checker::error_codes::ErrorCode;
use crate::diagnostic::Diagnostic;
use crate::lexer::span::Span;
use crate::parser::ast::{LiteralPattern, MatchArm, Pattern, PatternKind};
use crate::type_layout;

/// Represents a concrete value in a single slot of a tuple's product space.
enum TupleSlotValue {
    Bool(bool),
    Variant(String),
    StringLiteral(String),
}

/// Collect variant names covered by unguarded arms.
fn covered_variant_names<T>(arms: &[MatchArm<T>]) -> HashSet<&str> {
    arms.iter()
        .filter(|arm| arm.guard.is_none())
        .filter_map(|arm| match &arm.pattern.kind {
            PatternKind::Variant { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// Check that all `required` variant names are covered, returning a diagnostic if not.
fn check_required_variants<T>(
    type_name: &str,
    required: &[&str],
    arms: &[MatchArm<T>],
    span: Span,
) -> Option<Diagnostic> {
    let covered = covered_variant_names(arms);
    let missing: Vec<&&str> = required.iter().filter(|r| !covered.contains(**r)).collect();
    if missing.is_empty() {
        return None;
    }
    let missing_str = missing
        .iter()
        .map(|s| format!("`{s}`"))
        .collect::<Vec<_>>()
        .join(" and ");
    Some(
        Diagnostic::error(
            format!("non-exhaustive match on `{type_name}`: missing {missing_str}"),
            span,
        )
        .with_label("not all cases covered")
        .with_help("add match arms for the missing cases")
        .with_error_code(ErrorCode::NonExhaustiveMatch),
    )
}

/// Check match exhaustiveness for a given subject type and arms.
///
/// `resolve_named` resolves `Type::Named` to its concrete definition.
/// Returns diagnostics for any missing patterns.
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::too_many_lines)]
pub fn check_match_exhaustiveness<T>(
    subject_ty: &Type,
    arms: &[MatchArm<T>],
    span: Span,
    resolve_named: &dyn Fn(&Type) -> Type,
) -> Vec<Diagnostic> {
    // Resolve Named types to their actual definitions.
    let resolved_ty;
    let subject_ty = match subject_ty {
        Type::Named(_) => {
            resolved_ty = resolve_named(subject_ty);
            &resolved_ty
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
        return vec![];
    }

    let mut diagnostics = Vec::new();

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
            diagnostics.push(
                Diagnostic::error(
                    format!("non-exhaustive match on `{name}`: missing {missing_str}"),
                    span,
                )
                .with_label("not all variants covered")
                .with_help("add match arms for the missing variants, or add a `_ ->` catch-all")
                .with_error_code(ErrorCode::NonExhaustiveMatch),
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
            diagnostics.push(
                Diagnostic::error(
                    format!("non-exhaustive match on `{type_name}`: missing {missing_str}"),
                    span,
                )
                .with_label("not all variants covered")
                .with_help("add match arms for the missing variants, or add a `_ ->` catch-all")
                .with_error_code(ErrorCode::NonExhaustiveMatch),
            );
        }
    }

    // Option: check Some and None covered
    if subject_ty.is_option()
        && let Some(diag) = check_required_variants(
            type_layout::TYPE_OPTION,
            &[type_layout::VARIANT_SOME, type_layout::VARIANT_NONE],
            arms,
            span,
        )
    {
        diagnostics.push(diag);
    }

    // Settable: check Value, Clear, Unchanged covered
    if subject_ty.is_settable()
        && let Some(diag) =
            check_required_variants("Settable", &["Value", "Clear", "Unchanged"], arms, span)
    {
        diagnostics.push(diag);
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
            diagnostics.push(
                Diagnostic::error(
                    format!("non-exhaustive match on array: missing {missing}"),
                    span,
                )
                .with_label("not all cases covered")
                .with_help(
                    "add match arms for both `[]` and `[_, ..rest]`, or add a `_ ->` catch-all",
                )
                .with_error_code(ErrorCode::NonExhaustiveMatch),
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
            diagnostics.push(
                Diagnostic::error("non-exhaustive match on `boolean`: missing a case", span)
                    .with_label("not all cases covered")
                    .with_help("add match arms for both `true` and `false`")
                    .with_error_code(ErrorCode::NonExhaustiveMatch),
            );
        }
    }

    // Number/String: require catch-all (infinite values)
    if matches!(subject_ty, Type::Number) {
        diagnostics.push(
            Diagnostic::error(
                "non-exhaustive match on `number`: cannot cover all values without a catch-all",
                span,
            )
            .with_label("number type has infinite values")
            .with_help("add a `_ ->` catch-all arm")
            .with_error_code(ErrorCode::NonExhaustiveMatch),
        );
    }
    if matches!(subject_ty, Type::String) {
        diagnostics.push(
            Diagnostic::error(
                "non-exhaustive match on `string`: cannot cover all values without a catch-all",
                span,
            )
            .with_label("string type has infinite values")
            .with_help("add a `_ ->` catch-all arm")
            .with_error_code(ErrorCode::NonExhaustiveMatch),
        );
    }

    // Tuple: check product space exhaustiveness
    if let Type::Tuple(elem_types) = subject_ty
        && !check_tuple_exhaustiveness(elem_types, arms, resolve_named)
    {
        diagnostics.push(
            Diagnostic::error(
                "non-exhaustive match on tuple: not all combinations are covered",
                span,
            )
            .with_label("not all cases covered")
            .with_help("add match arms for the missing combinations, or add a `_ ->` catch-all")
            .with_error_code(ErrorCode::NonExhaustiveMatch),
        );
    }

    diagnostics
}

fn check_tuple_exhaustiveness<T>(
    elem_types: &[Type],
    arms: &[MatchArm<T>],
    resolve_named: &dyn Fn(&Type) -> Type,
) -> bool {
    // Check for tuple-of-all-wildcards catch-all (top-level wildcard/binding
    // already handled by the caller's early return).
    let has_tuple_catchall = arms.iter().any(|arm| {
        if arm.guard.is_some() {
            return false;
        }
        matches!(&arm.pattern.kind, PatternKind::Tuple(patterns)
            if patterns.iter().all(|p| matches!(p.kind, PatternKind::Wildcard | PatternKind::Binding(_))))
    });
    if has_tuple_catchall {
        return true;
    }

    // Collect the possible values for each position; return false if any is unbounded
    let possible: Vec<Vec<TupleSlotValue>> = match elem_types
        .iter()
        .map(|ty| finite_values_for_type(ty, resolve_named))
        .collect::<Option<Vec<_>>>()
    {
        Some(p) => p,
        None => return false,
    };

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
                    .all(|(i, pat)| pattern_covers_value(pat, &possible[i][combo[i]])),
                PatternKind::Wildcard | PatternKind::Binding(_) => true,
                _ => false,
            }
        });
        if !covered {
            return false;
        }

        // Advance to next combination
        let mut pos = elem_types.len();
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
        }
    }
}

/// Returns the finite set of values for a type, or None if unbounded.
fn finite_values_for_type(
    ty: &Type,
    resolve_named: &dyn Fn(&Type) -> Type,
) -> Option<Vec<TupleSlotValue>> {
    // Resolve named types
    let resolved;
    let ty = if matches!(ty, Type::Named(_)) {
        resolved = resolve_named(ty);
        &resolved
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
        _ if ty.is_string_literal_union() => {
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
fn pattern_covers_value(pattern: &Pattern, value: &TupleSlotValue) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::Checker;
    use crate::parser::Parser;

    fn check(source: &str) -> Vec<Diagnostic> {
        let program = Parser::new(source)
            .parse_program()
            .expect("parse should succeed");
        Checker::new().check(&program)
    }

    fn has_error(diagnostics: &[Diagnostic], code: ErrorCode) -> bool {
        diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some(code.code()))
    }

    fn has_error_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
        diagnostics
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error && d.message.contains(text))
    }

    // ── Exhaustive matches (no errors) ─────────────────────────

    #[test]
    fn wildcard_is_exhaustive() {
        let diags = check(
            r#"
let x = match 42 {
    1 -> "one",
    _ -> "other",
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn binding_is_exhaustive() {
        let diags = check(
            r#"
let x = match 42 {
    n -> n + 1,
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn bool_both_branches_exhaustive() {
        let diags = check(
            r#"
let x: boolean = true
let y = match x {
    true -> "yes",
    false -> "no",
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn union_all_variants_exhaustive() {
        let diags = check(
            r#"
type Color = | Red | Green | Blue
let _f(c: Color) -> string = {
    match c {
        Red -> "r",
        Green -> "g",
        Blue -> "b",
    }
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn option_both_variants_exhaustive() {
        let diags = check(
            r#"
let _f(x: Option<number>) -> number = {
    match x {
        Some(n) -> n,
        None -> 0,
    }
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn string_literal_union_all_variants() {
        let diags = check(
            r#"
type Method = "GET" | "POST" | "PUT"
let _f(m: Method) -> string = {
    match m {
        "GET" -> "get",
        "POST" -> "post",
        "PUT" -> "put",
    }
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn array_empty_and_nonempty_exhaustive() {
        let diags = check(
            r#"
let _f(xs: Array<number>) -> number = {
    match xs {
        [] -> 0,
        [first, ..rest] -> first,
    }
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    // ── Non-exhaustive matches (errors expected) ───────────────

    #[test]
    fn bool_missing_false() {
        let diags = check(
            r#"
let x: boolean = true
let y = match x {
    true -> "yes",
}
"#,
        );
        assert!(has_error_containing(&diags, "non-exhaustive"));
    }

    #[test]
    fn union_missing_variant() {
        let diags = check(
            r#"
type Color = | Red | Green | Blue
let _f(c: Color) -> string = {
    match c {
        Red -> "r",
        Green -> "g",
    }
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn number_without_catchall() {
        let diags = check(
            r#"
let x = match 42 {
    1 -> "one",
    2 -> "two",
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn string_without_catchall() {
        let diags = check(
            r#"
let x: string = "hello"
let y = match x {
    "hello" -> 1,
    "world" -> 2,
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn string_literal_union_missing_variant() {
        let diags = check(
            r#"
type Method = "GET" | "POST" | "PUT" | "DELETE"
let _f(m: Method) -> string = {
    match m {
        "GET" -> "get",
        "POST" -> "post",
    }
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn string_literal_union_with_wildcard() {
        let diags = check(
            r#"
type Status = "ok" | "error" | "pending"
let _f(s: Status) -> number = {
    match s {
        "ok" -> 1,
        _ -> 0,
    }
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn array_missing_empty_case() {
        let diags = check(
            r#"
let _f(xs: Array<number>) -> number = {
    match xs {
        [first, ..rest] -> first,
    }
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn array_missing_nonempty_case() {
        let diags = check(
            r#"
let _f(xs: Array<number>) -> number = {
    match xs {
        [] -> 0,
    }
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    // ── Tuple exhaustiveness ───────────────────────────────────

    #[test]
    fn tuple_bool_bool_exhaustive() {
        let diags = check(
            r#"
let x: (boolean, boolean) = (true, false)
let y = match x {
    (true, true) -> 1,
    (true, false) -> 2,
    (false, true) -> 3,
    (false, false) -> 4,
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn tuple_bool_bool_missing_combination() {
        let diags = check(
            r#"
let x: (boolean, boolean) = (true, false)
let y = match x {
    (true, true) -> 1,
    (true, false) -> 2,
    (false, true) -> 3,
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn tuple_with_unbounded_needs_catchall() {
        let diags = check(
            r#"
let x: (number, boolean) = (1, true)
let y = match x {
    (1, true) -> 1,
    (2, false) -> 2,
}
"#,
        );
        assert!(has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }

    #[test]
    fn tuple_wildcard_exhaustive() {
        let diags = check(
            r#"
let x: (number, boolean) = (1, true)
let y = match x {
    _ -> 0,
}
"#,
        );
        assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    }
}
