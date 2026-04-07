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
fn snapshot_error_exported_missing_return_type() {
    let output = get_diagnostics("test.fl", "export function add(a: number, b: number) { a }");
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
fn snapshot_error_untrusted_result_used_as_value() {
    // Untrusted npm call result used directly where the unwrapped type is expected
    let output = get_diagnostics(
        "test.fl",
        r#"import { transform } from "some-lib"
fn process(x: string) -> string {
    const result = transform(x)
    result
}"#,
    );
    insta::assert_snapshot!(output);
}

// ── Trait Error Snapshots ─────────────────────────────────────

#[test]
fn snapshot_error_trait_method_param_type_mismatch() {
    let output = get_diagnostics(
        "test.fl",
        r#"
trait Repo {
  fn create(self, input: number) -> string
}

type MyRepo {}

for MyRepo: Repo {
  fn create(self, input: string) -> string {
    input
  }
}
"#,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_method_return_type_mismatch() {
    let output = get_diagnostics(
        "test.fl",
        r#"
trait Repo {
  fn create(self) -> number
}

type MyRepo {}

for MyRepo: Repo {
  fn create(self) -> string {
    "oops"
  }
}
"#,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_missing_method() {
    let output = error_fixture("trait_missing_method");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_unknown() {
    let output = error_fixture("trait_unknown");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_default_method_not_required() {
    // Default methods should not be required in implementations
    let source = r#"
trait Eq {
  fn eq(self, other: string) -> boolean
  fn neq(self, other: string) -> boolean {
    !(self |> eq(other))
  }
}

type User { name: string }

for User: Eq {
  export fn eq(self, other: string) -> boolean {
    self.name == other
  }
}
"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::new().check(&program);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == diagnostic::Severity::Error)
        .collect();
    // Should produce no errors - neq has a default implementation
    assert!(
        errors.is_empty(),
        "Expected no errors but got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn snapshot_error_unknown_named_argument() {
    let output = get_diagnostics(
        "test.fl",
        "fn add(a: number, b: number) -> number { a + b }\nconst _x = add(pooopy: 22)",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_todo_warning() {
    let output = get_diagnostics("test.fl", "fn process(x: number) -> number {\n  todo\n}");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_member_access_on_non_record_type() {
    let output = get_diagnostics(
        "test.fl",
        "fn check(items: [number]) -> number {\n  items.gibberish\n}",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_no_cascade_from_undefined_name() {
    // When a name is undefined, the error type (Type::Error) should suppress
    // cascading "type mismatch" errors on downstream uses.
    let output = get_diagnostics("test.fl", "fn check() -> number {\n  undefined_name + 1\n}");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_no_cascade_from_invalid_field_access() {
    // When field access fails, Type::Error suppresses cascading errors
    // on the result (e.g. no "found <error>" type mismatch message).
    let output = get_diagnostics(
        "test.fl",
        "type User { name: string }\nfn check(u: User) -> number {\n  u.missing_field\n}",
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_used_as_parameter_type() {
    let output = get_diagnostics(
        "test.fl",
        r#"
trait Repo {
  fn create(self) -> string
}

type MyRepo {}

for MyRepo: Repo {
  fn create(self) -> string { "ok" }
}

fn doThing(repo: Repo) -> string {
  "hi"
}
"#,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_used_as_return_type() {
    let output = get_diagnostics(
        "test.fl",
        r#"
trait Repo {
  fn create(self) -> string
}

fn getRepo() -> Repo {
  todo
}
"#,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_trait_used_in_const_annotation() {
    let output = get_diagnostics(
        "test.fl",
        r#"
trait Repo {
  fn create(self) -> string
}

const x: Repo = todo
"#,
    );
    insta::assert_snapshot!(output);
}
