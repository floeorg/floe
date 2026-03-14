use super::*;
use crate::parser::Parser;

fn emit(input: &str) -> String {
    let program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let output = Codegen::new().generate(&program);
    output.code.trim().to_string()
}

// ── Basic Expressions ────────────────────────────────────────

#[test]
fn number_literal() {
    assert_eq!(emit("42"), "42;");
}

#[test]
fn string_literal() {
    assert_eq!(emit(r#""hello""#), r#""hello";"#);
}

#[test]
fn bool_literal() {
    assert_eq!(emit("true"), "true;");
}

#[test]
fn binary_expr() {
    assert_eq!(emit("1 + 2"), "1 + 2;");
}

#[test]
fn unary_expr() {
    assert_eq!(emit("!x"), "!x;");
}

#[test]
fn member_access() {
    assert_eq!(emit("a.b.c"), "a.b.c;");
}

#[test]
fn function_call() {
    assert_eq!(emit("f(1, 2)"), "f(1, 2);");
}

#[test]
fn named_args_erased() {
    assert_eq!(emit("f(name: x, limit: 10)"), "f(x, 10);");
}

#[test]
fn named_arg_punning_erased() {
    assert_eq!(emit("f(name:, limit:)"), "f(name, limit);");
}

#[test]
fn template_literal() {
    assert_eq!(emit("`hello ${name}`"), "`hello ${name}`;");
}

// ── Declarations ─────────────────────────────────────────────

#[test]
fn const_decl() {
    assert_eq!(emit("const x = 42"), "const x = 42;");
}

#[test]
fn const_with_type() {
    assert_eq!(emit("const x: number = 42"), "const x: number = 42;");
}

#[test]
fn export_const() {
    assert_eq!(emit("export const x = 42"), "export const x = 42;");
}

#[test]
fn const_array_destructure() {
    assert_eq!(emit("const [a, b] = pair"), "const [a, b] = pair;");
}

#[test]
fn function_decl() {
    let result = emit("fn add(a: number, b: number) -> number { a + b }");
    assert_eq!(
        result,
        "function add(a: number, b: number): number {\n  return a + b;\n}"
    );
}

#[test]
fn export_function() {
    let result = emit("export fn greet() { \"hi\" }");
    assert!(result.starts_with("export function greet()"));
}

#[test]
fn async_function() {
    let result = emit("async fn fetch() { await getData() }");
    assert!(result.starts_with("async function fetch()"));
}

#[test]
fn function_with_defaults() {
    let result = emit("fn f(x: number = 10) { x }");
    assert!(result.contains("x: number = 10"));
}

// ── Imports ──────────────────────────────────────────────────

#[test]
fn import_named() {
    assert_eq!(
        emit(r#"import { useState, useEffect } from "react""#),
        r#"import { useState, useEffect } from "react";"#
    );
}

// ── Pipe Operator ────────────────────────────────────────────

#[test]
fn pipe_simple() {
    // x |> f -> f(x)
    assert_eq!(emit("x |> f"), "f(x);");
}

#[test]
fn pipe_with_args() {
    // x |> f(y) -> f(x, y)
    assert_eq!(emit("x |> f(y)"), "f(x, y);");
}

#[test]
fn pipe_with_placeholder() {
    // x |> f(y, _, z) -> f(y, x, z)
    assert_eq!(emit("x |> f(y, _, z)"), "f(y, x, z);");
}

#[test]
fn pipe_chained() {
    // a |> f |> g -> g(f(a))
    assert_eq!(emit("a |> f |> g"), "g(f(a));");
}

// ── Partial Application ──────────────────────────────────────

#[test]
fn partial_application() {
    // add(10, _) -> (_x) => add(10, _x)
    assert_eq!(emit("add(10, _)"), "(_x) => add(10, _x);");
}

// ── Result / Option ──────────────────────────────────────────

#[test]
fn ok_constructor() {
    assert_eq!(emit("Ok(42)"), "{ ok: true as const, value: 42 };");
}

#[test]
fn err_constructor() {
    assert_eq!(
        emit(r#"Err("not found")"#),
        r#"{ ok: false as const, error: "not found" };"#
    );
}

#[test]
fn some_constructor() {
    // Some(x) -> x
    assert_eq!(emit("Some(x)"), "x;");
}

#[test]
fn none_literal() {
    // None -> undefined
    assert_eq!(emit("None"), "undefined;");
}

// ── Constructors ─────────────────────────────────────────────

#[test]
fn constructor_named() {
    assert_eq!(
        emit(r#"User(name: "Ryan", email: e)"#),
        r#"{ name: "Ryan", email: e };"#
    );
}

#[test]
fn constructor_with_spread() {
    assert_eq!(
        emit(r#"User(..user, name: "New")"#),
        r#"{ ...user, name: "New" };"#
    );
}

// ── Match ────────────────────────────────────────────────────

#[test]
fn match_simple() {
    let result = emit("match x { Ok(v) -> v, Err(e) -> e }");
    assert!(result.contains(".tag === \"Ok\""));
    assert!(result.contains(".tag === \"Err\""));
}

#[test]
fn match_with_wildcard() {
    let result = emit("match x { Ok(v) -> v, _ -> 0 }");
    // Last arm is wildcard -> no condition needed
    assert!(result.contains(".tag === \"Ok\""));
    assert!(result.contains("0"));
}

#[test]
fn match_literal() {
    let result = emit("match n { 1 -> true, _ -> false }");
    assert!(result.contains("=== 1"));
}

#[test]
fn match_range() {
    let result = emit("match n { 1..10 -> true, _ -> false }");
    assert!(result.contains(">= 1"));
    assert!(result.contains("<= 10"));
}

// ── Type Declarations ────────────────────────────────────────

#[test]
fn type_record() {
    let result = emit("type User = { id: string, name: string }");
    assert_eq!(result, "type User = { id: string; name: string };");
}

#[test]
fn type_union() {
    let result = emit("type Route = | Home | Profile(id: string) | NotFound");
    assert!(result.contains("tag: \"Home\""));
    assert!(result.contains("tag: \"Profile\""));
    assert!(result.contains("tag: \"NotFound\""));
}

#[test]
fn type_alias() {
    assert_eq!(emit("type Name = string"), "type Name = string;");
}

#[test]
fn opaque_type_erased() {
    assert_eq!(
        emit("opaque type HashedPassword = string"),
        "type HashedPassword = string;"
    );
}

#[test]
fn brand_type_erased() {
    // Brand<string, "UserId"> -> string
    let result = emit("type UserId = Brand<string, UserId>");
    assert_eq!(result, "type UserId = string;");
}

#[test]
fn option_type() {
    let result = emit("const x: Option<string> = None");
    assert!(result.contains("string | undefined"));
}

#[test]
fn result_type() {
    let result = emit("type Res = Result<User, ApiError>");
    assert!(result.contains("ok: true"));
    assert!(result.contains("ok: false"));
}

// ── JSX ──────────────────────────────────────────────────────

#[test]
fn jsx_self_closing() {
    let result = emit("<Button />");
    assert_eq!(result, "<Button />;");
}

#[test]
fn jsx_with_props() {
    let result = emit(r#"<Button label="Save" onClick={handleSave} />"#);
    assert!(result.contains("label={\"Save\"}"));
    assert!(result.contains("onClick={handleSave}"));
}

#[test]
fn jsx_with_children() {
    let result = emit("<div>{x}</div>");
    assert_eq!(result, "<div>{x}</div>;");
}

#[test]
fn jsx_fragment() {
    let result = emit("<>{x}</>");
    assert_eq!(result, "<>{x}</>;");
}

#[test]
fn jsx_detection() {
    let program = Parser::new("<Button />").parse_program().unwrap();
    let output = Codegen::new().generate(&program);
    assert!(output.has_jsx);
}

#[test]
fn no_jsx_detection() {
    let program = Parser::new("const x = 42").parse_program().unwrap();
    let output = Codegen::new().generate(&program);
    assert!(!output.has_jsx);
}

// ── Pipe Lambdas ─────────────────────────────────────────────

#[test]
fn lambda_single_arg() {
    assert_eq!(emit("|x| x + 1"), "(x) => x + 1;");
}

#[test]
fn lambda_multi_arg() {
    assert_eq!(emit("|a, b| a + b"), "(a, b) => a + b;");
}

// ── Equality -> structural equality ──────────────────────────

#[test]
fn equality_becomes_structural() {
    let result = emit("a == b");
    assert!(result.contains("__zenEq(a, b)"));
    let result = emit("a != b");
    assert!(result.contains("!__zenEq(a, b)"));
}

// ── If/Else -> ternary ────────────────────────────────────────

#[test]
fn if_else() {
    assert_eq!(
        emit("if x { 1 } else { 2 }"),
        "x ? {\n  1;\n} : {\n  2;\n};"
    );
}

// ── Await ────────────────────────────────────────────────────

#[test]
fn await_expr() {
    assert_eq!(emit("await fetchData()"), "await fetchData();");
}

// ── Return ───────────────────────────────────────────────────

#[test]
fn return_expr() {
    let result = emit("fn f() { return 42 }");
    assert!(result.contains("return 42"));
}

// ── Array ────────────────────────────────────────────────────

#[test]
fn array_literal() {
    assert_eq!(emit("[1, 2, 3]"), "[1, 2, 3];");
}

// ── Stdlib: Array ────────────────────────────────────────────

#[test]
fn stdlib_array_sort() {
    assert_eq!(
        emit("Array.sort([3, 1, 2])"),
        "[...[3, 1, 2]].sort((a, b) => a - b);"
    );
}

#[test]
fn stdlib_array_map() {
    assert_eq!(
        emit("Array.map([1, 2], |n| n * 2)"),
        "[1, 2].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_array_filter() {
    assert_eq!(
        emit("Array.filter([1, 2, 3], |n| n > 1)"),
        "[1, 2, 3].filter((n) => n > 1);"
    );
}

#[test]
fn stdlib_array_head() {
    assert_eq!(emit("Array.head([1, 2, 3])"), "[1, 2, 3][0];");
}

#[test]
fn stdlib_array_last() {
    assert_eq!(
        emit("Array.last([1, 2, 3])"),
        "[1, 2, 3][[1, 2, 3].length - 1];"
    );
}

#[test]
fn stdlib_array_reverse() {
    assert_eq!(
        emit("Array.reverse([1, 2, 3])"),
        "[...[1, 2, 3]].reverse();"
    );
}

#[test]
fn stdlib_array_take() {
    assert_eq!(emit("Array.take([1, 2, 3], 2)"), "[1, 2, 3].slice(0, 2);");
}

#[test]
fn stdlib_array_drop() {
    assert_eq!(emit("Array.drop([1, 2, 3], 1)"), "[1, 2, 3].slice(1);");
}

#[test]
fn stdlib_array_length() {
    assert_eq!(emit("Array.length([1, 2])"), "[1, 2].length;");
}

#[test]
fn stdlib_array_contains() {
    let result = emit("Array.contains([1, 2], 2)");
    assert!(result.contains("__zenEq"));
    assert!(result.contains(".some("));
}

// ── Stdlib: Option ───────────────────────────────────────────

#[test]
fn stdlib_option_map() {
    let result = emit("Option.map(Some(1), |n| n * 2)");
    assert!(result.contains("!== undefined"));
}

#[test]
fn stdlib_option_unwrap_or() {
    let result = emit("Option.unwrapOr(None, 0)");
    assert!(result.contains("!== undefined"));
    assert!(result.contains(": 0"));
}

#[test]
fn stdlib_option_is_some() {
    assert_eq!(emit("Option.isSome(Some(1))"), "1 !== undefined;");
}

#[test]
fn stdlib_option_is_none() {
    assert_eq!(emit("Option.isNone(None)"), "undefined === undefined;");
}

// ── Stdlib: Result ───────────────────────────────────────────

#[test]
fn stdlib_result_is_ok() {
    let result = emit("Result.isOk(Ok(1))");
    assert!(result.contains(".ok;"));
}

#[test]
fn stdlib_result_is_err() {
    let result = emit(r#"Result.isErr(Err("fail"))"#);
    assert!(result.contains("!"));
    assert!(result.contains(".ok;"));
}

#[test]
fn stdlib_result_to_option() {
    let result = emit("Result.toOption(Ok(42))");
    assert!(result.contains(".ok ?"));
    assert!(result.contains("undefined"));
}

// ── Stdlib: String ───────────────────────────────────────────

#[test]
fn stdlib_string_trim() {
    assert_eq!(emit(r#"String.trim("  hi  ")"#), r#""  hi  ".trim();"#);
}

#[test]
fn stdlib_string_to_upper() {
    assert_eq!(
        emit(r#"String.toUpper("hello")"#),
        r#""hello".toUpperCase();"#
    );
}

#[test]
fn stdlib_string_contains() {
    assert_eq!(
        emit(r#"String.contains("hello", "el")"#),
        r#""hello".includes("el");"#
    );
}

#[test]
fn stdlib_string_split() {
    assert_eq!(emit(r#"String.split("a,b", ",")"#), r#""a,b".split(",");"#);
}

#[test]
fn stdlib_string_length() {
    assert_eq!(emit(r#"String.length("hi")"#), r#""hi".length;"#);
}

// ── Stdlib: Number ───────────────────────────────────────────

#[test]
fn stdlib_number_clamp() {
    assert_eq!(
        emit("Number.clamp(15, 0, 10)"),
        "Math.min(Math.max(15, 0), 10);"
    );
}

#[test]
fn stdlib_number_parse() {
    let result = emit(r#"Number.parse("42")"#);
    assert!(result.contains("Number.isNaN"));
    assert!(result.contains("ok: true"));
    assert!(result.contains("ok: false"));
}

#[test]
fn stdlib_number_is_finite() {
    assert_eq!(emit("Number.isFinite(42)"), "Number.isFinite(42);");
}

// ── Stdlib: Pipes ────────────────────────────────────────────

#[test]
fn stdlib_pipe_bare() {
    assert_eq!(
        emit("[3, 1, 2] |> Array.sort"),
        "[...[3, 1, 2]].sort((a, b) => a - b);"
    );
}

#[test]
fn stdlib_pipe_with_args() {
    assert_eq!(
        emit("[1, 2, 3] |> Array.map(|n| n * 2)"),
        "[1, 2, 3].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_pipe_chain() {
    let result = emit("[1, 2, 3] |> Array.filter(|n| n > 1) |> Array.reverse");
    assert!(result.contains(".filter("));
    assert!(result.contains(".reverse()"));
}

#[test]
fn stdlib_pipe_string() {
    assert_eq!(emit(r#""  hi  " |> String.trim"#), r#""  hi  ".trim();"#);
}
