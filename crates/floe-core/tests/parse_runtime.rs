//! Behaviour cover for the validation that `parse<T>` emits.
//!
//! These tests run the emitted TypeScript and read what it answers. A
//! shape test cannot see the bug in #1521: the emitted text looked
//! reasonable, and `parse<Id>` on `typealias Id = string` rejected every
//! valid string at run time, because codegen validated the written name
//! instead of the type the checker resolved.
//!
//! The tests need Node 22.6 or newer on PATH, for its TypeScript type
//! stripping.

use floe_core::checker::{self, Checker};
use floe_core::codegen::Codegen;
use floe_core::parser::Parser;
use std::collections::HashMap;
use std::process::Command;

/// Compile Floe source to TypeScript through the same pipeline the CLI
/// runs, so every expression carries the type the checker gave it.
///
/// `lower_to_typed` is the whole post-check pipeline in one call. Calling
/// `desugar_program` and `attach_types` by hand here would skip
/// `mark_async_functions`, and the harness would emit `export function f`
/// where the compiler emits `export async function f`. A test of an async
/// `parse` would then fail as a broken program rather than show the bug.
fn compile(source: &str) -> String {
    let program = Parser::new(source)
        .parse_program()
        .expect("the source should parse");
    let (diagnostics, expr_types, invalid_exprs, shadowed) = Checker::new().check_full(&program);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == floe_core::diagnostic::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "the source should check: {errors:?}");
    let typed = checker::lower_to_typed(
        program,
        &expr_types,
        &invalid_exprs,
        &shadowed,
        &HashMap::new(),
    );

    Codegen::new().generate(&typed).code
}

/// Run one TypeScript program with Node and return its standard output.
fn run(source: &str) -> String {
    let dir = tempfile::tempdir().expect("failed to create a temp directory");
    let file = dir.path().join("main.ts");
    std::fs::write(&file, source).expect("failed to write the program");

    let output = Command::new("node")
        .arg("--experimental-strip-types")
        .arg(&file)
        .output()
        .expect("failed to run node. These tests need Node 22.6 or newer on PATH.");
    assert!(
        output.status.success(),
        "node rejected the emitted program: {}\n\n{source}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("node wrote output that is not UTF-8")
}

/// Compile the Floe source, append the TypeScript driver, and run both.
fn compile_and_run(source: &str, driver: &str) -> String {
    let mut program = compile(source);
    program.push('\n');
    program.push_str(driver);

    run(&program)
}

#[test]
fn parse_accepts_a_valid_value_through_a_type_alias() {
    let output = compile_and_run(
        "typealias Id = string\n\
         \n\
         export let parseId(raw: unknown) -> Result<Id, Error> = { parse<Id>(raw) }\n",
        "const good = parseId(\"hello\");\n\
         console.log(good.ok);\n\
         console.log(parseId(42).ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\n",
        "parse<Id> must accept a string and refuse a number, got: {output}"
    );
}

#[test]
fn parse_accepts_a_valid_value_through_an_alias_chain() {
    let output = compile_and_run(
        "typealias Name = string\n\
         typealias Label = Name\n\
         \n\
         export let parseLabel(raw: unknown) -> Result<Label, Error> = { parse<Label>(raw) }\n",
        "console.log(parseLabel(\"hello\").ok);\n\
         console.log(parseLabel(42).ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\n",
        "parse<Label> must follow the whole alias chain, got: {output}"
    );
}

#[test]
fn parse_accepts_a_valid_value_through_an_opaque_type() {
    let output = compile_and_run(
        "opaque type HashedPw = string\n\
         \n\
         export let parsePw(raw: unknown) -> Result<HashedPw, Error> = { parse<HashedPw>(raw) }\n",
        "console.log(parsePw(\"hunter2\").ok);\n\
         console.log(parsePw(42).ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\n",
        "parse<HashedPw> must validate the type the opaque type wraps, got: {output}"
    );
}

#[test]
fn parse_accepts_a_valid_array_of_an_aliased_element() {
    let output = compile_and_run(
        "typealias Id = string\n\
         \n\
         export let parseIds(raw: unknown) -> Result<Array<Id>, Error> = { parse<Array<Id>>(raw) }\n",
        "console.log(parseIds([\"a\", \"b\"]).ok);\n\
         console.log(parseIds([1]).ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\n",
        "parse<Array<Id>> must validate each element as a string, got: {output}"
    );
}

#[test]
fn parse_refuses_a_value_that_is_not_a_tuple() {
    let output = compile_and_run(
        "typealias Pair = (number, number)\n\
         \n\
         export let parsePair(raw: unknown) -> Result<Pair, Error> = { parse<Pair>(raw) }\n",
        "console.log(parsePair([1, 2]).ok);\n\
         console.log(parsePair(\"nope\").ok);\n\
         console.log(parsePair([1]).ok);\n\
         console.log(parsePair([1, \"two\"]).ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\nfalse\nfalse\n",
        "parse<Pair> must check the array, the length and each element, got: {output}"
    );
}

#[test]
fn parse_refuses_a_value_that_is_not_a_function() {
    let output = compile_and_run(
        "typealias Handler = (a: number) -> number\n\
         \n\
         export let parseHandler(raw: unknown) -> Result<Handler, Error> = { parse<Handler>(raw) }\n",
        "console.log(parseHandler((a: number) => a).ok);\n\
         console.log(parseHandler(\"nope\").ok);\n",
    );

    assert_eq!(
        output, "true\nfalse\n",
        "parse<Handler> must accept a function and refuse a string, got: {output}"
    );
}

#[test]
fn parse_accepts_anything_for_unknown() {
    let output = compile_and_run(
        "export let parseAny(raw: unknown) -> Result<unknown, Error> = { parse<unknown>(raw) }\n",
        "console.log(parseAny(42).ok);\n\
         console.log(parseAny(\"text\").ok);\n",
    );

    assert_eq!(
        output, "true\ntrue\n",
        "parse<unknown> validates nothing, so it must accept every value, got: {output}"
    );
}
