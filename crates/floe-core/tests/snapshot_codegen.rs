//! Snapshot tests for Floe codegen: .fl fixtures -> TypeScript output.
//!
//! Each test reads a .fl fixture file, parses + codegen, and compares
//! against an insta snapshot. Run `cargo insta review` to accept new snapshots.

use floe_core::checker::{self, Checker};
use floe_core::codegen::Codegen;
use floe_core::desugar;
use floe_core::parser::Parser;

fn compile(source: &str) -> String {
    let mut program = Parser::new(source)
        .parse_program()
        .expect("fixture should parse");
    let (_, expr_types, _) = Checker::new().check_full(&program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed = checker::attach_types(program, &expr_types, &std::collections::HashSet::new());
    Codegen::new().generate(&typed).code
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
fn snapshot_match_option() {
    let output = compile_fixture("match_option");
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
fn snapshot_positional_variants() {
    let output = compile_fixture("positional_variants");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_jsx_component() {
    let output = compile_fixture("jsx_component");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_jsx_comment() {
    let output = compile_fixture("jsx_comment");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_jsx_member_expr() {
    let output = compile_fixture("jsx_member_expr");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_imports() {
    let output = compile_fixture("imports");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_reexport() {
    let output = compile_fixture("reexport");
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
fn snapshot_traits() {
    let output = compile_fixture("traits");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_trait_constrained_generics() {
    let output = compile(
        r#"
trait Repo {
  fn create(self, input: string) -> string
  fn findById(self, id: number) -> string
}

type DrizzleRepo {
  db: string,
}

for DrizzleRepo: Repo {
  export fn create(self, input: string) -> string {
    input
  }

  export fn findById(self, id: number) -> string {
    "found"
  }
}

fn doWork<R: Repo>(repo: R, input: string) -> string {
  repo |> create(input)
}
"#,
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_tuples() {
    let output = compile_fixture("tuples");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_string_patterns() {
    let output = compile_fixture("string_patterns");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_todo_unreachable() {
    let output = compile_fixture("todo_unreachable");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_record_spread() {
    let output = compile_fixture("record_spread");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_array_patterns() {
    let output = compile_fixture("array_patterns");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_collect() {
    let output = compile_fixture("collect");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_deriving() {
    let output = compile_fixture("deriving");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_pipe_unwrap() {
    let output = compile_fixture("pipe_unwrap");
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_unicode_escapes() {
    let output = compile_fixture("unicode_escapes");
    insta::assert_snapshot!(output);
}
