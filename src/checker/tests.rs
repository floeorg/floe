use super::*;
use crate::diagnostic::Severity;
use crate::parser::Parser;

fn check(source: &str) -> Vec<Diagnostic> {
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    Checker::new().check(&program)
}

fn has_error(diagnostics: &[Diagnostic], code: &str) -> bool {
    diagnostics.iter().any(|d| d.code.as_deref() == Some(code))
}

fn has_error_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.message.contains(text))
}

fn has_warning_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains(text))
}

// ── Rule 1: Basic type checking ─────────────────────────────

#[test]
fn basic_const_number() {
    let diags = check("const x = 42");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn basic_const_string() {
    let diags = check("const x = \"hello\"");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn undeclared_variable() {
    let diags = check("const x = y");
    assert!(has_error_containing(&diags, "`y` is not defined"));
}

// ── Rule 2: Brand enforcement ───────────────────────────────

#[test]
fn brand_comparison_different_tags() {
    let diags = check(
        r#"
type UserId = Brand<string, UserId>
type Email = Brand<string, Email>
const a: UserId = UserId("abc")
const b: Email = Email("test@test.com")
const result = a == b
"#,
    );
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 4: Exhaustiveness checking ─────────────────────────

#[test]
fn exhaustive_match_with_wildcard() {
    let diags = check(
        r#"
const x = match 42 {
    1 -> "one",
    _ -> "other",
}
"#,
    );
    assert!(!has_error(&diags, "E004"));
}

#[test]
fn non_exhaustive_bool_match() {
    let diags = check(
        r#"
const x: boolean = true
const y = match x {
    true -> "yes",
}
"#,
    );
    assert!(has_error_containing(&diags, "non-exhaustive"));
}

// ── Rule 5: Result/Option ? tracking ────────────────────────

#[test]
fn unwrap_in_result_function() {
    let diags = check(
        r#"
fn tryFetch(url: string) -> Result<string, string> {
    const result = Ok("data")
    const value = result?
    return Ok(value)
}
"#,
    );
    let unwrap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("E005") && d.message.contains("? operator requires"))
        .collect();
    assert!(unwrap_errors.is_empty());
}

#[test]
fn unwrap_not_on_result_or_option() {
    let diags = check(
        r#"
fn process() -> Result<number, string> {
    const x = 42
    const y = x?
    return Ok(y)
}
"#,
    );
    assert!(has_error_containing(
        &diags,
        "? can only be used on Result or Option"
    ));
}

// ── Rule 6: No property access on unnarrowed unions ─────────

#[test]
fn property_access_on_result() {
    let diags = check(
        r#"
const result = Ok(42)
const x = result.value
"#,
    );
    assert!(has_error_containing(
        &diags,
        "cannot access `.value` on Result"
    ));
}

// ── Rule 8: Same-type equality ──────────────────────────────

#[test]
fn equality_same_types() {
    let diags = check("const x = 1 == 1");
    assert!(!has_error(&diags, "E008"));
}

#[test]
fn equality_different_types() {
    let diags = check(r#"const x = 1 == "hello""#);
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 9: Unused detection ────────────────────────────────

#[test]
fn unused_variable_warning() {
    let diags = check("const x = 42");
    assert!(has_warning_containing(&diags, "is never used"));
}

#[test]
fn underscore_prefix_suppresses_unused() {
    let diags = check("const _x = 42");
    assert!(!has_warning_containing(&diags, "is never used"));
}

#[test]
fn used_variable_no_warning() {
    let diags = check(
        r#"
const x = 42
const y = x
"#,
    );
    let unused_x: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning && d.message.contains("`x`"))
        .collect();
    assert!(unused_x.is_empty());
}

#[test]
fn unused_import_error() {
    let diags = check(r#"import { useState } from "react""#);
    assert!(has_error_containing(&diags, "is never used"));
}

// ── Rule 10: Exported function return types ─────────────────

#[test]
fn exported_function_needs_return_type() {
    let diags = check("export fn add(a: number, b: number) { return a }");
    assert!(has_error_containing(&diags, "must declare a return type"));
}

#[test]
fn exported_function_with_return_type_ok() {
    let diags = check("export fn add(a: number, b: number) -> number { return a }");
    assert!(!has_error(&diags, "E010"));
}

// ── Rule 12: String concat warning ──────────────────────────

#[test]
fn string_concat_warning() {
    let diags = check(r#"const x = "hello" + " world""#);
    assert!(has_warning_containing(&diags, "template literal"));
}

// ── OK/Err/Some/None types ──────────────────────────────────

#[test]
fn ok_creates_result() {
    let diags = check("const _x = Ok(42)");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn none_creates_option() {
    let diags = check("const _x = None");
    assert!(!has_error(&diags, "E001"));
}

// ── Array type checking ─────────────────────────────────────

#[test]
fn homogeneous_array() {
    let diags = check("const _x = [1, 2, 3]");
    assert!(!has_error(&diags, "E004"));
}

#[test]
fn mixed_array_error() {
    let diags = check(r#"const _x = [1, "two", 3]"#);
    assert!(has_error_containing(&diags, "mixed array"));
}

// ── Dead code detection ─────────────────────────────────────

#[test]
fn dead_code_after_return() {
    let diags = check(
        r#"
fn test() -> number {
    return 1
    const x = 2
}
"#,
    );
    assert!(has_error_containing(&diags, "unreachable code"));
}

// ── Opaque type enforcement ─────────────────────────────────

#[test]
fn opaque_type_cannot_be_constructed() {
    let diags = check(
        r#"
opaque type HashedPassword = string
const _x = HashedPassword("abc")
"#,
    );
    assert!(has_error_containing(&diags, "opaque type"));
}

// ── Unhandled Result ────────────────────────────────────────

#[test]
fn floating_result_error() {
    let diags = check("Ok(42)");
    assert!(has_error_containing(&diags, "unhandled Result"));
}

// ── Untrusted Import Enforcement ─────────────────────────────

#[test]
fn untrusted_import_requires_try() {
    let diags = check(
        r#"
import { fetchUser } from "some-lib"
const _x = fetchUser("id")
"#,
    );
    assert!(has_error(&diags, "E014"));
    assert!(has_error_containing(&diags, "untrusted import"));
}

#[test]
fn untrusted_import_ok_with_try() {
    let diags = check(
        r#"
import { fetchUser } from "some-lib"
const _x = try fetchUser("id")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn trusted_specifier_no_error() {
    let diags = check(
        r#"
import { trusted capitalize } from "some-lib"
const _x = capitalize("hello")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn trusted_module_no_error() {
    let diags = check(
        r#"
import trusted { capitalize, slugify } from "string-utils"
const _x = capitalize("hello")
const _y = slugify("hello world")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn mixed_trusted_untrusted() {
    let diags = check(
        r#"
import { trusted capitalize, fetchUser } from "some-lib"
const _x = capitalize("hello")
const _y = fetchUser("id")
"#,
    );
    // capitalize is trusted — no error
    assert!(!has_error_containing(&diags, "capitalize"));
    // fetchUser is untrusted — error
    assert!(has_error_containing(&diags, "fetchUser"));
}
