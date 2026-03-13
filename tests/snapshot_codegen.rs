//! Snapshot tests for ZenScript codegen: .zs fixtures -> TypeScript output.
//!
//! Each test reads a .zs fixture file, parses + codegen, and compares
//! against an insta snapshot. Run `cargo insta review` to accept new snapshots.

use zenscript::codegen::Codegen;
use zenscript::parser::Parser;

fn compile(source: &str) -> String {
    let program = Parser::new(source)
        .parse_program()
        .expect("fixture should parse");
    Codegen::new().generate(&program).code
}

fn compile_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}.zs");
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
