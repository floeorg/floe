//! Snapshot tests for Floe codegen: .fl fixtures -> TypeScript output.
//!
//! Each test reads a .fl fixture file, parses + codegen, and compares
//! against an insta snapshot. Run `cargo insta review` to accept new snapshots.

use floe::checker::Checker;
use floe::codegen::Codegen;
use floe::parser::Parser;

fn compile(source: &str) -> String {
    let program = Parser::new(source)
        .parse_program()
        .expect("fixture should parse");
    let (_, expr_types) = Checker::new().check_full(&program);
    Codegen::with_expr_types(expr_types).generate(&program).code
}

fn compile_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}.fl");
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("failed to read fixture {path}"));
    compile(&source)
}

#[test]
fn snapshot_hello() {
    let output = compile_fixture("hello");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_pipes() {
    let output = compile_fixture("pipes");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_match_expr() {
    let output = compile_fixture("match_expr");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_result_option() {
    let output = compile_fixture("result_option");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_functions() {
    let output = compile_fixture("functions");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_types() {
    let output = compile_fixture("types");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_jsx_component() {
    let output = compile_fixture("jsx_component");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_imports() {
    let output = compile_fixture("imports");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_constructors() {
    let output = compile_fixture("constructors");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_partial_application() {
    let output = compile_fixture("partial_application");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_unit_type() {
    let output = compile_fixture("unit_type");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_structural_equality() {
    let output = compile_fixture("structural_equality");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_stdlib() {
    let output = compile_fixture("stdlib");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_dot_shorthand() {
    let output = compile_fixture("dot_shorthand");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_for_blocks() {
    let output = compile_fixture("for_blocks");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_try_expr() {
    let output = compile_fixture("try_expr");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_trusted_import() {
    let output = compile_fixture("trusted_import");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_string_patterns() {
    let output = compile_fixture("string_patterns");
    insta::assert_snapshot!(output);
}
