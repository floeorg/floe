//! Snapshot tests for Floe error messages.
//!
//! Tests that parse errors and type checker diagnostics produce the expected
//! error output. Run `cargo insta review` to accept new snapshots.

use floe::checker::Checker;
use floe::diagnostic;
use floe::parser::Parser;

/// Compile a source string and return rendered diagnostics (parse errors or type errors).
fn get_diagnostics(filename: &str, source: &str) -> String {
    match Parser::new(source).parse_program() {
        Err(errs) => {
            let diags = diagnostic::from_parse_errors(&errs);
            diagnostic::render_diagnostics(filename, source, &diags)
        }
        Ok(program) => {
            let diags = Checker::new().check(&program);
            if diags.is_empty() {
                return String::new();
            }
            diagnostic::render_diagnostics(filename, source, &diags)
        }
    }
}

fn error_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/errors/{name}.fl");
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("failed to read fixture {path}"));
    get_diagnostics(&format!("{name}.fl"), &source)
}

// ── Parse Error Snapshots ───────────────────────────────────────

#[test]
fn snapshot_error_banned_let() {
    let output = error_fixture("banned_let");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_banned_class() {
    let output = error_fixture("banned_class");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_banned_null() {
    let output = error_fixture("banned_null");
    insta::assert_snapshot!(output);
}

// ── Type Checker Error Snapshots ────────────────────────────────

#[test]
fn snapshot_error_unused_import() {
    let output = get_diagnostics("test.fl", r#"import { useState } from "react""#);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_unused_variable() {
    let output = get_diagnostics("test.fl", "const x = 42");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_type_mismatch_comparison() {
    let output = get_diagnostics("test.fl", r#"const _x = 1 == "hello""#);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_mixed_array() {
    let output = get_diagnostics("test.fl", r#"const _x = [1, "two", 3]"#);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_exported_missing_return_type() {
    let output = get_diagnostics(
        "test.fl",
        "export function add(a: number, b: number) { return a }",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_unhandled_result() {
    let output = get_diagnostics("test.fl", "Ok(42)");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_string_concat() {
    let output = get_diagnostics("test.fl", r#"const _x = "a" + "b""#);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_banned_void() {
    let output = error_fixture("banned_void");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_missing_return() {
    let output = get_diagnostics(
        "test.fl",
        "function getName(_id: string): string {\n  const _x = 42\n}",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_untrusted_import() {
    let output = get_diagnostics(
        "test.fl",
        "import { fetchUser } from \"some-lib\"\nconst _x = fetchUser(\"123\")",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_todo_warning() {
    let output = get_diagnostics("test.fl", "fn process(x: number) -> number {\n  todo\n}");
    insta::assert_snapshot!(output);
}
