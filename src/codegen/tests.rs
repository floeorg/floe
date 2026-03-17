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

#[test]
fn template_literal_expression_interpolation() {
    assert_eq!(emit("`count: ${1 + 2}`"), "`count: ${1 + 2}`;");
}

#[test]
fn template_literal_pipe_match_interpolation() {
    assert_eq!(
        emit(r#"`${count |> match { 1 -> "one", _ -> "other" }}`"#),
        r#"`${count === 1 ? "one" : "other"}`;"#,
    );
}

#[test]
fn template_literal_multiple_interpolations() {
    assert_eq!(emit(r#"`${a} and ${b}`"#), "`${a} and ${b}`;",);
}

#[test]
fn template_literal_no_interpolation() {
    assert_eq!(emit("`hello world`"), "`hello world`;");
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

// ── Pipe into Match ─────────────────────────────────────────

#[test]
fn pipe_into_match_simple() {
    // x |> match { 1 -> true, _ -> false } -> same as match x { ... }
    let result = emit("x |> match { 1 -> true, _ -> false }");
    assert!(
        result.contains("=== 1"),
        "expected literal check, got: {result}"
    );
    assert!(
        result.contains("true"),
        "expected true branch, got: {result}"
    );
    assert!(
        result.contains("false"),
        "expected false branch, got: {result}"
    );
}

#[test]
fn pipe_chain_into_match() {
    // a |> f |> match { 1 -> true, _ -> false }
    // desugars to: match (f(a)) { 1 -> true, _ -> false }
    let result = emit("a |> f |> match { 1 -> true, _ -> false }");
    assert!(
        result.contains("f(a)"),
        "expected f(a) as match subject, got: {result}"
    );
    assert!(
        result.contains("=== 1"),
        "expected literal check, got: {result}"
    );
}

#[test]
fn pipe_into_match_with_guard() {
    let result = emit(r#"price |> match { _ when price < 10 -> "cheap", _ -> "expensive" }"#);
    assert!(
        result.contains("price < 10"),
        "expected guard condition, got: {result}"
    );
    assert!(
        result.contains("cheap"),
        "expected cheap branch, got: {result}"
    );
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

// ── Match Guards ─────────────────────────────────────────────

#[test]
fn match_guard_no_bindings() {
    let result = emit("match n { 1 -> true, _ when n > 10 -> true, _ -> false }");
    // Guard without bindings emits guard condition directly (no `true &&`)
    assert!(result.contains("n > 10"));
    assert!(!result.contains("true && n"));
}

#[test]
fn match_guard_with_binding() {
    let result = emit("match x { Ok(v) when v > 0 -> v, _ -> 0 }");
    // Guard with binding uses IIFE with if-check
    assert!(result.contains("if (v > 0)"));
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
    assert!(result.contains("__floeEq(a, b)"));
    let result = emit("a != b");
    assert!(result.contains("!__floeEq(a, b)"));
}

#[test]
fn floe_eq_helper_emitted_when_needed() {
    // File that uses == should have the __floeEq helper definition
    let result = emit("a == b");
    assert!(
        result.contains("function __floeEq(a: unknown, b: unknown): boolean"),
        "expected __floeEq helper to be emitted, got:\n{result}"
    );
}

#[test]
fn floe_eq_helper_not_emitted_when_not_needed() {
    // File that doesn't use == should NOT have the __floeEq helper
    let result = emit("const x = 1 + 2");
    assert!(
        !result.contains("__floeEq"),
        "expected no __floeEq helper, got:\n{result}"
    );
}

#[test]
fn floe_eq_helper_emitted_for_dot_shorthand_eq() {
    // Dot shorthand with == should emit the helper
    let result = emit("const active = todos |> Array.filter(.done == false)");
    assert!(
        result.contains("function __floeEq(a: unknown, b: unknown): boolean"),
        "expected __floeEq helper for dot shorthand ==, got:\n{result}"
    );
}

#[test]
fn floe_eq_helper_emitted_for_stdlib_contains() {
    // Array.contains uses __floeEq in its template
    let result = emit("Array.contains([1, 2], 2)");
    assert!(
        result.contains("function __floeEq(a: unknown, b: unknown): boolean"),
        "expected __floeEq helper for Array.contains, got:\n{result}"
    );
}

// ── Await ────────────────────────────────────────────────────

#[test]
fn await_expr() {
    assert_eq!(emit("await fetchData()"), "await fetchData();");
}

// ── Implicit Return ──────────────────────────────────────────

#[test]
fn implicit_return_single_expr() {
    let result = emit("fn f() -> number { 42 }");
    assert!(result.contains("return 42"));
}

#[test]
fn implicit_return_multi_statement() {
    let result = emit("fn f() -> number { const x = 1\nx + 1 }");
    assert!(result.contains("return x + 1"));
}

#[test]
fn unit_function_no_return() {
    let result = emit("fn f() -> () { Console.log(\"hi\") }");
    assert!(!result.contains("return"));
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
    assert!(result.contains("__floeEq"));
    assert!(result.contains(".some("));
}

#[test]
fn stdlib_array_any() {
    assert_eq!(
        emit("Array.any([1, 2, 3], |n| n > 2)"),
        "[1, 2, 3].some((n) => n > 2);"
    );
}

#[test]
fn stdlib_array_all() {
    assert_eq!(
        emit("Array.all([1, 2, 3], |n| n > 0)"),
        "[1, 2, 3].every((n) => n > 0);"
    );
}

#[test]
fn stdlib_array_sum() {
    assert_eq!(
        emit("Array.sum([1, 2, 3])"),
        "[1, 2, 3].reduce((a, b) => a + b, 0);"
    );
}

#[test]
fn stdlib_array_join() {
    assert_eq!(
        emit(r#"Array.join(["a", "b"], ", ")"#),
        r#"["a", "b"].join(", ");"#
    );
}

#[test]
fn stdlib_array_is_empty() {
    assert_eq!(emit("Array.isEmpty([])"), "[].length === 0;");
}

#[test]
fn stdlib_array_unique() {
    assert_eq!(emit("Array.unique([1, 2, 2])"), "[...new Set([1, 2, 2])];");
}

#[test]
fn stdlib_array_chunk() {
    let result = emit("Array.chunk([1, 2, 3, 4], 2)");
    assert!(result.contains("slice"));
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

// ── Type-directed pipe resolution ───────────────────────────

fn emit_with_types(input: &str) -> String {
    let program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let (_, expr_types) = crate::checker::Checker::new().check_full(&program);
    Codegen::with_expr_types(expr_types)
        .generate(&program)
        .code
        .trim()
        .to_string()
}

#[test]
fn type_directed_array_length() {
    let result = emit_with_types("const _x = [1, 2, 3] |> length");
    assert_eq!(result, "const _x = [1, 2, 3].length;");
}

#[test]
fn type_directed_string_length() {
    let result = emit_with_types(r#"const _x = "hello" |> length"#);
    assert_eq!(result, r#"const _x = "hello".length;"#);
}

#[test]
fn type_directed_array_filter() {
    let result = emit_with_types(r#"const _x = [1, 2, 3] |> filter(|x| x > 1)"#);
    assert_eq!(result, "const _x = [1, 2, 3].filter((x) => x > 1);");
}

#[test]
fn union_variant_dot_access() {
    let result = emit(
        r#"
type Filter = | All | Active | Completed
const _f = Filter.All
"#,
    );
    assert!(result.contains(r#"{ tag: "All" }"#));
}

#[test]
fn union_variant_dot_access_non_union_passthrough() {
    // Regular member access should still work normally
    let result = emit("const _x = foo.bar");
    assert!(result.contains("foo.bar"));
}

// ── Tuples ─────────────────────────────────────────────────

#[test]
fn tuple_construction() {
    assert_eq!(emit("(1, 2)"), "[1, 2] as const;");
}

#[test]
fn tuple_three_elements() {
    assert_eq!(emit(r#"(1, "two", true)"#), r#"[1, "two", true] as const;"#);
}

#[test]
fn tuple_destructuring() {
    let result = emit("const (x, y) = point");
    assert_eq!(result, "const [x, y] = point;");
}

#[test]
fn tuple_type_annotation() {
    let result = emit("const p: (number, string) = (1, \"a\")");
    assert!(result.contains("readonly [number, string]"));
    assert!(result.contains("[1, \"a\"] as const"));
}

#[test]
fn tuple_return_type() {
    let result = emit("fn f(a: number) -> (number, string) { (a, \"x\") }");
    assert!(result.contains("readonly [number, string]"));
}

#[test]
fn tuple_trailing_comma() {
    assert_eq!(emit("(1, 2,)"), "[1, 2] as const;");
}

// ── Pipe: tap ───────────────────────────────────────────────

#[test]
fn stdlib_pipe_tap_qualified() {
    let result = emit("[1, 2, 3] |> Pipe.tap(Console.log)");
    // Console.log gets its own codegen template, so it's expanded inside tap's IIFE
    assert!(result.contains("const _v"), "output: {result}");
    assert!(result.contains("return _v"), "output: {result}");
}

#[test]
fn stdlib_tap_direct_call() {
    let result = emit("Pipe.tap([1, 2, 3], Console.log)");
    assert!(result.contains("const _v"), "output: {result}");
    assert!(result.contains("return _v"), "output: {result}");
}

#[test]
fn stdlib_pipe_tap_with_lambda() {
    let result = emit("[1, 2, 3] |> Pipe.tap(|x| Console.log(x))");
    assert!(result.contains("const _v"), "output: {result}");
    assert!(result.contains("return _v"), "output: {result}");
}

// ── Http Stdlib ─────────────────────────────────────────────

#[test]
fn stdlib_http_get() {
    let result = emit(r#"Http.get("https://api.example.com")"#);
    assert!(
        result.contains("fetch(\"https://api.example.com\")"),
        "expected fetch call, got: {result}"
    );
    assert!(
        result.contains("async"),
        "expected async IIFE, got: {result}"
    );
    assert!(
        result.contains("ok: true as const"),
        "expected Result ok branch, got: {result}"
    );
    assert!(
        result.contains("ok: false as const"),
        "expected Result err branch, got: {result}"
    );
}

#[test]
fn stdlib_http_post() {
    let result = emit(r#"Http.post("https://api.example.com", data)"#);
    assert!(
        result.contains("\"POST\""),
        "expected POST method, got: {result}"
    );
    assert!(
        result.contains("JSON.stringify(data)"),
        "expected JSON.stringify body, got: {result}"
    );
    assert!(
        result.contains("Content-Type"),
        "expected Content-Type header, got: {result}"
    );
}

#[test]
fn stdlib_http_put() {
    let result = emit(r#"Http.put("https://api.example.com", data)"#);
    assert!(
        result.contains("\"PUT\""),
        "expected PUT method, got: {result}"
    );
    assert!(
        result.contains("JSON.stringify(data)"),
        "expected JSON.stringify body, got: {result}"
    );
}

#[test]
fn stdlib_http_delete() {
    let result = emit(r#"Http.delete("https://api.example.com")"#);
    assert!(
        result.contains("\"DELETE\""),
        "expected DELETE method, got: {result}"
    );
    assert!(
        result.contains("fetch(\"https://api.example.com\""),
        "expected fetch call, got: {result}"
    );
}

#[test]
fn stdlib_http_json() {
    let result = emit("Http.json(response)");
    assert!(
        result.contains("response.json()"),
        "expected .json() call, got: {result}"
    );
    assert!(
        result.contains("async"),
        "expected async IIFE, got: {result}"
    );
}

#[test]
fn stdlib_http_text() {
    let result = emit("Http.text(response)");
    assert!(
        result.contains("response.text()"),
        "expected .text() call, got: {result}"
    );
    assert!(
        result.contains("async"),
        "expected async IIFE, got: {result}"
    );
}

// ── Test Blocks ─────────────────────────────────────────────

fn emit_test_mode(input: &str) -> String {
    let program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let output = Codegen::new().with_test_mode().generate(&program);
    output.code.trim().to_string()
}

#[test]
fn test_block_stripped_in_production() {
    let result = emit(
        r#"
fn add(a: number, b: number) -> number { a + b }

test "addition" {
    assert add(1, 2) == 3
}
"#,
    );
    // In production mode (default), test blocks should not appear
    assert!(
        !result.contains("test"),
        "test block should be stripped in production mode"
    );
    assert!(result.contains("function add"));
}

#[test]
fn test_block_emitted_in_test_mode() {
    let result = emit_test_mode(
        r#"
test "math" {
    assert 1 == 1
}
"#,
    );
    // In test mode, test blocks should be emitted
    assert!(
        result.contains("__testName"),
        "test block should emit test runner code"
    );
    assert!(result.contains("math"), "test name should appear in output");
    assert!(result.contains("PASS"), "should have pass reporting");
    assert!(result.contains("FAIL"), "should have fail reporting");
}

// ── Inline For Declarations ─────────────────────────────────

#[test]
fn inline_for_emits_same_as_block() {
    let block_result = emit(
        r#"
type User = { name: string }
for User {
    fn display(self) -> string { self.name }
}
"#,
    );
    let inline_result = emit(
        r#"
type User = { name: string }
for User fn display(self) -> string { self.name }
"#,
    );
    assert_eq!(block_result, inline_result);
}

#[test]
fn inline_for_exported() {
    let result = emit(
        r#"
type User = { name: string }
export for User fn display(self) -> string { self.name }
"#,
    );
    // The type is emitted as the full record type
    assert!(
        result.contains("export function display(self: "),
        "expected export function display, got: {result}"
    );
}

#[test]
fn inline_for_multiple_separate() {
    let result = emit(
        r#"
type User = { name: string }
for User fn display(self) -> string { self.name }
export for User fn greet(self, greeting: string) -> string { greeting }
"#,
    );
    assert!(
        result.contains("function display(self: "),
        "expected function display, got: {result}"
    );
    assert!(
        result.contains("export function greet(self: "),
        "expected export function greet, got: {result}"
    );
}

// ── String Literal Unions ───────────────────────────────────

#[test]
fn string_literal_union_type() {
    let result = emit(r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#);
    assert_eq!(
        result,
        r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";"#
    );
}

#[test]
fn string_literal_union_match() {
    let result = emit(
        r#"
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

fn describe(method: HttpMethod) -> string {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
        "PUT" -> "updating",
        "DELETE" -> "removing",
    }
}
"#,
    );
    assert!(
        result.contains(r#"method === "GET""#),
        "expected string comparison, got: {result}"
    );
    assert!(
        result.contains(r#""fetching""#),
        "expected fetching branch, got: {result}"
    );
    assert!(
        result.contains(r#"method === "DELETE""#),
        "expected DELETE comparison, got: {result}"
    );
}

#[test]
fn string_literal_union_match_with_wildcard() {
    let result = emit(
        r#"
type Status = "ok" | "error"
fn handle(s: Status) -> number {
    match s {
        "ok" -> 1,
        _ -> 0,
    }
}
"#,
    );
    assert!(
        result.contains(r#"s === "ok""#),
        "expected string check, got: {result}"
    );
    assert!(result.contains("0"), "expected fallback, got: {result}");
}

#[test]
fn string_literal_union_exported() {
    let result = emit(r#"export type Direction = "north" | "south" | "east" | "west""#);
    assert!(result.starts_with("export type Direction = "));
    assert!(result.contains(r#""north" | "south" | "east" | "west""#));
}

// ── Array Pattern Matching ──────────────────────────────────

#[test]
fn match_array_empty() {
    let result = emit(r#"match items { [] -> "empty", _ -> "other" }"#);
    assert!(
        result.contains(".length === 0"),
        "expected empty array check, got: {result}"
    );
    assert!(
        result.contains("\"empty\""),
        "expected empty branch, got: {result}"
    );
}

#[test]
fn match_array_single() {
    let result = emit(r#"match items { [a] -> a, _ -> "none" }"#);
    assert!(
        result.contains(".length === 1"),
        "expected single element check, got: {result}"
    );
    assert!(
        result.contains("[0]"),
        "expected index access for binding, got: {result}"
    );
}

#[test]
fn match_array_two_elements() {
    let result = emit(r#"match items { [a, b] -> a, _ -> "none" }"#);
    assert!(
        result.contains(".length === 2"),
        "expected two element check, got: {result}"
    );
}

#[test]
fn match_array_rest() {
    let result = emit("match items { [first, ..rest] -> first, _ -> 0 }");
    assert!(
        result.contains(".length >= 1"),
        "expected length >= 1 check, got: {result}"
    );
    assert!(
        result.contains("[0]"),
        "expected index access for first, got: {result}"
    );
    assert!(
        result.contains(".slice(1)"),
        "expected slice for rest, got: {result}"
    );
}

#[test]
fn match_array_two_plus_rest() {
    let result = emit("match items { [a, b, ..rest] -> a, _ -> 0 }");
    assert!(
        result.contains(".length >= 2"),
        "expected length >= 2 check, got: {result}"
    );
    assert!(
        result.contains(".slice(2)"),
        "expected slice(2) for rest, got: {result}"
    );
}

#[test]
fn match_array_empty_and_rest_exhaustive() {
    // [] + [_, ..rest] covers all cases — should not add non-exhaustive throw
    let result = emit(r#"match items { [] -> "empty", [first, ..rest] -> first }"#);
    assert!(
        result.contains(".length === 0"),
        "expected empty check, got: {result}"
    );
    assert!(
        result.contains(".length >= 1"),
        "expected non-empty check, got: {result}"
    );
}

#[test]
fn match_array_wildcard_rest() {
    // [_, ..rest] with underscore as first element
    let result = emit("match items { [_, ..rest] -> rest, _ -> items }");
    assert!(
        result.contains(".length >= 1"),
        "expected length >= 1, got: {result}"
    );
    assert!(
        result.contains(".slice(1)"),
        "expected slice(1) for rest, got: {result}"
    );
}

#[test]
fn match_array_literal_element() {
    // Pattern with literal sub-pattern
    let result = emit(r#"match items { [1] -> "one", _ -> "other" }"#);
    assert!(
        result.contains(".length === 1"),
        "expected length check, got: {result}"
    );
    assert!(
        result.contains("[0] === 1"),
        "expected literal element check, got: {result}"
    );
}

// ── Collect Block ───────────────────────────────────────────

#[test]
fn collect_basic_structure() {
    let result = emit(
        r#"
fn validate(x: number) -> Result<number, string> { Ok(x) }
fn f() -> Result<number, Array<string>> {
    collect {
        const a = validate(1)?
        const b = validate(2)?
        a + b
    }
}
"#,
    );
    assert!(
        result.contains("__errors"),
        "expected error accumulator, got: {result}"
    );
    assert!(result.contains("(() => {"), "expected IIFE, got: {result}");
    assert!(
        result.contains("ok: true as const"),
        "expected ok result, got: {result}"
    );
    assert!(
        result.contains("ok: false as const"),
        "expected err result, got: {result}"
    );
}

#[test]
fn collect_no_unwrap() {
    // collect with no ? just wraps in Ok
    let result = emit(
        r#"
fn f() -> Result<number, Array<string>> {
    collect {
        42
    }
}
"#,
    );
    assert!(
        result.contains("ok: true as const, value: 42"),
        "expected Ok(42) result, got: {result}"
    );
}

// ── Deriving ────────────────────────────────────────────────

#[test]
fn deriving_display_generates_string() {
    let result = emit(
        r#"
type User = {
  name: string,
  age: number,
} deriving (Display)
"#,
    );
    assert!(
        result.contains("function display(self: User): string"),
        "should generate display function, got: {result}"
    );
    assert!(
        result.contains("User(name: ${self.name}, age: ${self.age})"),
        "should format all fields, got: {result}"
    );
}

// ── Parse<T> Built-in ────────────────────────────────────────

#[test]
fn parse_string_type() {
    let result = emit("parse<string>(x)");
    assert!(
        result.contains("typeof __v !== \"string\""),
        "should check typeof for string, got: {result}"
    );
    assert!(
        result.contains("ok: true as const"),
        "should return ok on success, got: {result}"
    );
    assert!(
        result.contains("ok: false as const"),
        "should return error on failure, got: {result}"
    );
}

#[test]
fn parse_number_type() {
    let result = emit("parse<number>(x)");
    assert!(
        result.contains("typeof __v !== \"number\""),
        "should check typeof for number, got: {result}"
    );
}

#[test]
fn parse_boolean_type() {
    let result = emit("parse<boolean>(x)");
    assert!(
        result.contains("typeof __v !== \"boolean\""),
        "should check typeof for boolean, got: {result}"
    );
}

#[test]
fn parse_record_type_codegen() {
    let result = emit("parse<{ name: string, age: number }>(data)");
    assert!(
        result.contains("typeof __v !== \"object\""),
        "should check for object, got: {result}"
    );
    assert!(
        result.contains("(__v as any).name"),
        "should check field 'name', got: {result}"
    );
    assert!(
        result.contains("(__v as any).age"),
        "should check field 'age', got: {result}"
    );
    assert!(
        result.contains("\"string\""),
        "should validate string field, got: {result}"
    );
    assert!(
        result.contains("\"number\""),
        "should validate number field, got: {result}"
    );
}

#[test]
fn parse_array_type_codegen() {
    let result = emit("parse<Array<number>>(items)");
    assert!(
        result.contains("Array.isArray"),
        "should check Array.isArray, got: {result}"
    );
    assert!(
        result.contains("typeof"),
        "should validate element types, got: {result}"
    );
}

#[test]
fn parse_in_pipe() {
    let result = emit("x |> parse<string>");
    assert!(
        result.contains("const __v = x"),
        "should use piped value, got: {result}"
    );
    assert!(
        result.contains("typeof __v !== \"string\""),
        "should validate type, got: {result}"
    );
}
