use super::*;
use crate::desugar;
use crate::parser::Parser;

fn emit(input: &str) -> String {
    let mut program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    // Tests that only exercise codegen structure don't need inferred
    // types — `attach_types` fills every expression's type with
    // `Arc<Type::Unknown>` when the map is empty, which codegen tolerates
    // for structural emission paths.
    let typed = crate::checker::attach_types(
        program,
        &crate::checker::ExprTypeMap::new(),
        &std::collections::HashSet::new(),
    );
    let output = Codegen::new().generate(&typed);
    output.code.trim().to_string()
}

/// Run the full pipeline — parse, desugar, check, attach types, codegen —
/// so `expr.ty` is populated with real inferred types at every node. Use
/// this for tests that exercise type-directed dispatch.
fn emit_typed(input: &str) -> String {
    let mut program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let (_diags, expr_types, invalid_exprs) = crate::checker::Checker::new().check_full(&program);
    let typed = crate::checker::attach_types(program, &expr_types, &invalid_exprs);
    let output = Codegen::new().generate(&typed);
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
fn named_args_reorder_to_declared_order() {
    // Bug #1134: named args must be reordered to match the declared
    // parameter order before labels are erased. Without the fix, the
    // emitted call has values in source order which silently swaps
    // arguments at runtime.
    let source = r#"
fn safeDivide(a: number, b: number) => number { a / b }
safeDivide(b: 1, a: 2)
"#;
    let output = emit_typed(source);
    assert!(
        output.contains("safeDivide(2, 1)"),
        "named args should reorder to declared order (a=2, b=1); got:\n{output}"
    );
}

#[test]
fn named_args_in_declared_order_unchanged() {
    let source = r#"
fn safeDivide(a: number, b: number) => number { a / b }
safeDivide(a: 2, b: 1)
"#;
    let output = emit_typed(source);
    assert!(
        output.contains("safeDivide(2, 1)"),
        "named args already in declared order should stay; got:\n{output}"
    );
}

#[test]
fn mixed_positional_and_named_args_reorder() {
    let source = r#"
fn f(a: number, b: number, c: number) => number { a + b + c }
f(10, c: 30, b: 20)
"#;
    let output = emit_typed(source);
    assert!(
        output.contains("f(10, 20, 30)"),
        "positional fills leading slot, named reorder to declared; got:\n{output}"
    );
}

#[test]
fn named_args_fully_reversed_three_params() {
    let source = r#"
fn f(a: number, b: number, c: number) => number { a + b + c }
f(c: 30, b: 20, a: 10)
"#;
    let output = emit_typed(source);
    assert!(
        output.contains("f(10, 20, 30)"),
        "fully reversed 3-arg named call should reorder; got:\n{output}"
    );
}

#[test]
fn named_args_splice_multiple_defaults() {
    let source = r#"
fn g(a: number, b: number = 2, c: number = 3, d: number) => number { a + b + c + d }
g(d: 40, a: 10)
"#;
    let output = emit_typed(source);
    assert!(
        output.contains("g(10, 2, 3, 40)"),
        "two defaults spliced between named args; got:\n{output}"
    );
}

#[test]
fn named_args_default_spliced_in_missing_slot() {
    // A named call that omits a defaulted parameter gets the default
    // spliced into the reordered slot so codegen emits it positionally.
    let source = r#"
fn greet(name: string, greeting: string = "hello") => string { greeting }
greet(name: "world")
"#;
    let output = emit_typed(source);
    assert!(
        output.contains(r#"greet("world", "hello")"#),
        "missing default param should splice default into slot; got:\n{output}"
    );
}

#[test]
fn named_args_unknown_label_emits_error() {
    use crate::diagnostic::Severity;
    let source = r#"
fn f(a: number) => number { a }
f(nonexistent: 1)
"#;
    let program = crate::parser::Parser::new(source).parse_program().unwrap();
    let (diags, _, _, _) = crate::checker::Checker::new().check_with_types(&program);
    assert!(
        diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("nonexistent")),
        "unknown named label should surface as a checker error; got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
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

#[test]
fn tagged_template_no_interpolation() {
    assert_eq!(emit("tag`hello`"), "tag`hello`;");
}

#[test]
fn tagged_template_with_interpolation() {
    assert_eq!(emit("tag`a ${x} b`"), "tag`a ${x} b`;");
}

#[test]
fn tagged_template_multiple_interpolations() {
    assert_eq!(emit("sql`${col} + ${delta}`"), "sql`${col} + ${delta}`;");
}

#[test]
fn tagged_template_member_tag() {
    assert_eq!(emit("db.sql`select 1`"), "db.sql`select 1`;");
}

#[test]
fn tagged_template_nested_template_in_interpolation() {
    assert_eq!(
        emit("tag`outer ${`inner ${x}`} end`"),
        "tag`outer ${`inner ${x}`} end`;",
    );
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
    let result = emit("fn add(a: number, b: number) => number { a + b }");
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
fn promise_await_emits_async_function() {
    let result = emit_with_types("fn fetch() => Promise<string> { getData() |> Promise.await }");
    assert!(result.starts_with("async function fetch()"));
    assert!(result.contains("await getData()"));
}

#[test]
fn async_fn_sugar_wraps_return_type_in_promise() {
    // `async fn f() -> T` should emit `async function f(): Promise<T>`
    let result = emit_with_types("async fn fetch() => string { \"hi\" }");
    assert!(
        result.starts_with("async function fetch(): Promise<string>"),
        "expected async + Promise wrap, got: {result}"
    );
}

#[test]
fn async_fn_sugar_with_await_body() {
    let result =
        emit_with_types("async fn fetch() => string { const x = getData() |> Promise.await\n x }");
    assert!(
        result.starts_with("async function fetch(): Promise<string>"),
        "expected async + Promise wrap, got: {result}"
    );
    assert!(result.contains("await getData()"));
}

#[test]
fn function_with_defaults() {
    let result = emit("fn f(x: number = 10) { x }");
    assert!(result.contains("x: number = 10"));
}

// ── Imports ──────────────────────────────────────────────────

#[test]
fn import_named() {
    // Both names used in value positions → regular import
    assert_eq!(
        emit(
            r#"import trusted { useState, useEffect } from "react"
const x = useState(0)
const y = useEffect"#
        ),
        "import { useState, useEffect } from \"react\";\n\nconst x = useState(0);\n\nconst y = useEffect;"
    );
}

#[test]
fn import_type_only_specifier() {
    // Session only used as a type → import type
    assert_eq!(
        emit(
            r#"import { Session } from "@supabase/supabase-js"
const x: Option<Session> = None"#
        ),
        "import { type Session } from \"@supabase/supabase-js\";\n\nconst x: Session | null | undefined = undefined;"
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

#[test]
fn pipe_local_fn_shadows_stdlib_template() {
    // A locally defined `map` must win over the Array.map stdlib template.
    // Imports feed the same `local_names` set, so this also covers trusted
    // imports — the unit test avoids the npm resolver round-trip.
    let src = r#"
fn map(arr: Array<number>, f: (number) => number) => Array<number> { arr }
const _items = [1, 2, 3] |> map((x) => x + 1)
"#;
    let out = emit_typed(src);
    assert!(
        out.contains("map([1, 2, 3]"),
        "expected local `map` call, got:\n{out}"
    );
    assert!(
        !out.contains("[1, 2, 3].map"),
        "local `map` should not be routed through Array.map template:\n{out}"
    );
}

#[test]
fn pipe_local_fn_named_get_shadows_record_get() {
    // `get` is one of the stdlib names most likely to collide with
    // imports (Record.get / Map.get / Http.get). A local definition must
    // still win.
    let src = r#"
type Router = { path: string }
fn get(r: Router, path: string) => Router { Router(path: path) }
const _r = Router(path: "/") |> get("/hello")
"#;
    let out = emit_typed(src);
    assert!(
        out.contains("get({"),
        "expected local `get` call, got:\n{out}"
    );
    assert!(
        !out.contains(".has("),
        "local `get` should not be routed through Record.get template:\n{out}"
    );
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

#[test]
fn constructor_with_defaults_omitted() {
    let result = emit(
        r#"
        type Config = { baseUrl: string, timeout: number = 5000, retries: number = 3 }
        const c = Config(baseUrl: "https://api.com")
        "#,
    );
    assert!(result.contains(r#"baseUrl: "https://api.com", timeout: 5000, retries: 3"#));
}

#[test]
fn constructor_with_defaults_overridden() {
    let result = emit(
        r#"
        type Config = { baseUrl: string, timeout: number = 5000, retries: number = 3 }
        const c = Config(baseUrl: "https://api.com", timeout: 10000)
        "#,
    );
    assert!(result.contains(r#"baseUrl: "https://api.com", timeout: 10000, retries: 3"#));
}

#[test]
fn constructor_all_defaults() {
    let result = emit(
        r#"
        type Options = { timeout: number = 5000, retries: number = 3 }
        const o = Options()
        "#,
    );
    assert!(result.contains("timeout: 5000, retries: 3"));
}

// ── Default field optionality in type definitions ───────────

#[test]
fn record_type_default_fields_are_optional() {
    let result = emit(
        r#"
        type Config = { baseUrl: string, timeout: number = 5000, retries: number = 3 }
        const c = Config(baseUrl: "https://api.com")
        "#,
    );
    // Fields with defaults should be optional in the type definition
    assert!(
        result.contains("timeout?:") && result.contains("retries?:"),
        "default fields should be optional in type, got: {result}"
    );
    // Fields without defaults should remain required
    assert!(
        !result.contains("baseUrl?:"),
        "required field should not be optional, got: {result}"
    );
}

// ── Settable ────────────────────────────────────────────────

#[test]
fn settable_value_emits_value() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged }
        const d = Dto(name: Value("Ryan"))
        "#,
    );
    assert!(result.contains(r#"name: "Ryan""#));
}

#[test]
fn settable_clear_emits_null() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged }
        const d = Dto(name: Clear)
        "#,
    );
    assert!(result.contains("name: null"));
}

#[test]
fn settable_unchanged_omits_field() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged, age: Settable<number> = Unchanged }
        const d = Dto(name: Value("Ryan"))
        "#,
    );
    // Constructor line should have name but not age
    let const_line = result.lines().find(|l| l.starts_with("const d")).unwrap();
    assert!(const_line.contains(r#"name: "Ryan""#));
    assert!(!const_line.contains("age"));
}

#[test]
fn settable_all_unchanged_empty_object() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged }
        const d = Dto()
        "#,
    );
    assert!(result.contains("{  }"));
}

#[test]
fn settable_type_emits_nullable() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged }
        "#,
    );
    assert!(result.contains("string | null | undefined"));
}

// ── Match ────────────────────────────────────────────────────

#[test]
fn match_simple() {
    let result = emit("match x { Ok(v) -> v, Err(e) -> e }");
    assert!(result.contains(".ok === true"));
    assert!(result.contains(".ok === false"));
}

#[test]
fn match_with_wildcard() {
    let result = emit("match x { Ok(v) -> v, _ -> 0 }");
    // Last arm is wildcard -> no condition needed
    assert!(result.contains(".ok === true"));
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
    let result = emit("type Route = | Home | Profile { id: string } | NotFound");
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
fn newtype_erased() {
    // type UserId { string } -> erased at runtime
    let result = emit("type UserId = UserId(string)");
    assert!(result.contains("UserId"));
}

#[test]
fn option_type() {
    let result = emit("const x: Option<string> = None");
    assert!(result.contains("string | null | undefined"));
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
fn jsx_hyphenated_props() {
    let result = emit(r#"<Input aria-label="Share link" data-testid="input" />"#);
    assert!(result.contains("aria-label={\"Share link\"}"));
    assert!(result.contains("data-testid={\"input\"}"));
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
    let typed = crate::checker::attach_types(
        program,
        &crate::checker::ExprTypeMap::new(),
        &std::collections::HashSet::new(),
    );
    let output = Codegen::new().generate(&typed);
    assert!(output.has_jsx);
}

#[test]
fn no_jsx_detection() {
    let program = Parser::new("const x = 42").parse_program().unwrap();
    let typed = crate::checker::attach_types(
        program,
        &crate::checker::ExprTypeMap::new(),
        &std::collections::HashSet::new(),
    );
    let output = Codegen::new().generate(&typed);
    assert!(!output.has_jsx);
}

// ── Generic Functions ─────────────────────────────────────────

#[test]
fn generic_function_codegen() {
    assert_eq!(
        emit("fn identity<T>(x: T) => T { x }"),
        "function identity<T>(x: T): T {\n  return x;\n}"
    );
}

#[test]
fn generic_function_multi_params_codegen() {
    assert_eq!(
        emit("fn pair<A, B>(a: A, b: B) => (A, B) { (a, b) }"),
        "function pair<A, B>(a: A, b: B): readonly [A, B] {\n  return [a, b];\n}"
    );
}

// ── Pipe Lambdas ─────────────────────────────────────────────

#[test]
fn lambda_single_arg() {
    assert_eq!(emit("(x) => x + 1"), "(x) => x + 1;");
}

#[test]
fn lambda_multi_arg() {
    assert_eq!(emit("(a, b) => a + b"), "(a, b) => a + b;");
}

// ── Derived function binding ─────────────────────────────────

#[test]
fn fn_binding_partial_application() {
    assert_eq!(
        emit("fn add(a: number, b: number) => number { a + b }\nfn inc = add(1, _)"),
        "function add(a: number, b: number): number {\n  return a + b;\n}\n\nconst inc = (_x) => add(1, _x);"
    );
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

// ── Option.unwrapOr chained pipe ────────────────────────────

#[test]
fn option_unwrap_or_chained_with_pipe() {
    let result = emit(
        "const _x: Option<Array<number>> = None\nconst _y = _x |> Option.unwrapOr([]) |> filter((n) => n > 0)",
    );
    // The ternary from unwrapOr must be parenthesized so .filter binds to the result, not to []
    assert!(
        !result.contains(": [].filter(") && !result.contains("[].filter("),
        "Option.unwrapOr([]) piped into filter should parenthesize the ternary, got: {result}"
    );
}

#[test]
fn option_stdlib_uses_null_check_not_undefined() {
    // Option functions must use != null (catches both null and undefined)
    // not !== undefined (misses null from serde/JSON)
    let result = emit("const _x: Option<number> = None\nconst _y = _x |> Option.map((n) => n + 1)");
    assert!(
        result.contains("!= null") && !result.contains("!== undefined"),
        "Option.map should use != null, not !== undefined, got: {result}"
    );
}

// ── Promise.await ───────────────────────────────────────────

#[test]
fn promise_await_pipe() {
    let result = emit_with_types("const _x = fetchData() |> Promise.await");
    assert!(result.contains("await fetchData()"));
}

#[test]
fn bare_await_shorthand_emits_async_function() {
    let result = emit_with_types("fn fetch() => Promise<string> { getData() |> await }");
    assert!(
        result.starts_with("async function fetch()"),
        "bare `|> await` should infer async on enclosing function, got: {result}"
    );
    assert!(result.contains("await getData()"));
}

#[test]
fn bare_await_shorthand_pipe() {
    let result = emit_with_types("const _x = fetchData() |> await");
    assert!(result.contains("await fetchData()"));
}

#[test]
fn nested_fn_with_promise_await_emits_async() {
    let result =
        emit_with_types("fn outer() { fn inner() { getData() |> Promise.await } inner() }");
    assert!(
        result.contains("async function inner()"),
        "nested fn with Promise.await should be async, got: {result}"
    );
}

#[test]
fn nested_fn_with_bare_await_emits_async() {
    let result = emit_with_types("fn outer() { fn inner() { getData() |> await } inner() }");
    assert!(
        result.contains("async function inner()"),
        "nested fn with bare await should be async, got: {result}"
    );
}

#[test]
fn match_on_comparison_wraps_subject_in_parens() {
    let result = emit("const _x = match 5 > 0 { true -> \"yes\", false -> \"no\" }");
    assert!(
        result.contains("(5 > 0) === true"),
        "match on comparison should wrap subject in parens, got: {result}"
    );
}

#[test]
fn match_arm_block_iife_returns_last_expr() {
    let result = emit("const _x = match true { true -> { const a = 1\na + 2 }, false -> 0 }");
    assert!(
        result.contains("return a + 2"),
        "match arm block IIFE should return last expression, got: {result}"
    );
}

// ── Implicit Return ──────────────────────────────────────────

#[test]
fn implicit_return_single_expr() {
    let result = emit("fn f() => number { 42 }");
    assert!(result.contains("return 42"));
}

#[test]
fn implicit_return_multi_statement() {
    let result = emit("fn f() => number { const x = 1\nx + 1 }");
    assert!(result.contains("return x + 1"));
}

#[test]
fn unit_function_no_return() {
    let result = emit("fn f() => () { Console.log(\"hi\") }");
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
        emit("Array.map([1, 2], (n) => n * 2)"),
        "[1, 2].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_array_filter() {
    assert_eq!(
        emit("Array.filter([1, 2, 3], (n) => n > 1)"),
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
        emit("Array.any([1, 2, 3], (n) => n > 2)"),
        "[1, 2, 3].some((n) => n > 2);"
    );
}

#[test]
fn stdlib_array_all() {
    assert_eq!(
        emit("Array.all([1, 2, 3], (n) => n > 0)"),
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
    let result = emit("Option.map(Some(1), (n) => n * 2)");
    assert!(result.contains("!= null"));
}

#[test]
fn stdlib_option_unwrap_or() {
    let result = emit("Option.unwrapOr(None, 0)");
    assert!(result.contains("!= null"));
    assert!(result.contains(": 0"));
}

#[test]
fn stdlib_option_is_some() {
    assert_eq!(emit("Option.isSome(Some(1))"), "1 != null;");
}

#[test]
fn stdlib_option_is_none() {
    assert_eq!(emit("Option.isNone(None)"), "undefined == null;");
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
        emit(r#"String.toUpperCase("hello")"#),
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

// ── Stdlib: Console ─────────────────────────────────────────

#[test]
fn stdlib_console_log_single() {
    assert_eq!(emit("Console.log(\"hi\")"), "console.log(\"hi\");");
}

#[test]
fn stdlib_console_log_variadic() {
    assert_eq!(
        emit("Console.log(\"label:\", 42)"),
        "console.log(\"label:\", 42);"
    );
}

#[test]
fn stdlib_console_log_three_args() {
    assert_eq!(
        emit("Console.log(\"a\", \"b\", \"c\")"),
        "console.log(\"a\", \"b\", \"c\");"
    );
}

#[test]
fn stdlib_console_warn_variadic() {
    assert_eq!(
        emit("Console.warn(\"warn:\", 1)"),
        "console.warn(\"warn:\", 1);"
    );
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
        emit("[1, 2, 3] |> Array.map((n) => n * 2)"),
        "[1, 2, 3].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_pipe_chain() {
    let result = emit("[1, 2, 3] |> Array.filter((n) => n > 1) |> Array.reverse");
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
    let (_, expr_types, _) = crate::checker::Checker::new().check_full(&program);
    let mut program = program;
    crate::checker::mark_async_functions(&mut program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed =
        crate::checker::attach_types(program, &expr_types, &std::collections::HashSet::new());
    Codegen::new().generate(&typed).code.trim().to_string()
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
    let result = emit_with_types(r#"const _x = [1, 2, 3] |> filter((x) => x > 1)"#);
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
    assert!(result.contains(r#"{ __tag: "All" }"#));
}

#[test]
fn union_variant_dot_access_non_union_passthrough() {
    // Regular member access should still work normally
    let result = emit("const _x = foo.bar");
    assert!(result.contains("foo.bar"));
}

// ── Variant constructors as functions ──────────────────────

#[test]
fn non_unit_variant_as_function() {
    let result = emit(
        r#"
type SaveError = | Validation { errors: Array<string> }
    | Api { message: string }

const _f = Validation
"#,
    );
    assert!(
        result.contains(r#"(errors) => ({ __tag: "Validation", errors })"#),
        "got: {result}"
    );
}

#[test]
fn unit_variant_unchanged_as_value() {
    let result = emit(
        r#"
type Filter = | All | Active | Completed
const _f = All
"#,
    );
    assert!(result.contains(r#"{ __tag: "All" }"#), "got: {result}");
    assert!(
        !result.contains("=>"),
        "should not emit arrow function, got: {result}"
    );
}

#[test]
fn qualified_non_unit_variant_as_function() {
    let result = emit(
        r#"
type SaveError = | Validation { errors: Array<string> }
    | Api { message: string }

const _f = SaveError.Validation
"#,
    );
    assert!(
        result.contains(r#"(errors) => ({ __tag: "Validation", errors })"#),
        "got: {result}"
    );
}

#[test]
fn variant_construct_with_args_unchanged() {
    let result = emit(
        r#"
type MyError = | Validation { message: string }
    | NotFound

const _e = Validation(message: "bad")
"#,
    );
    assert!(
        result.contains(r#"{ __tag: "Validation", message: "bad" }"#),
        "got: {result}"
    );
}

#[test]
fn multi_field_variant_as_function() {
    let result = emit(
        r#"
type Shape = | Circle { radius: number }
    | Rect { width: number, height: number }

const _f = Rect
"#,
    );
    assert!(
        result.contains(r#"(width, height) => ({ __tag: "Rect", width, height })"#),
        "got: {result}"
    );
}

// ── Tuples ─────────────────────────────────────────────────

#[test]
fn tuple_construction() {
    assert_eq!(emit("(1, 2)"), "[1, 2];");
}

#[test]
fn tuple_three_elements() {
    assert_eq!(emit(r#"(1, "two", true)"#), r#"[1, "two", true];"#);
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
    assert!(result.contains("[1, \"a\"]"));
}

#[test]
fn tuple_return_type() {
    let result = emit("fn f(a: number) => (number, string) { (a, \"x\") }");
    assert!(result.contains("readonly [number, string]"));
}

#[test]
fn tuple_trailing_comma() {
    assert_eq!(emit("(1, 2,)"), "[1, 2];");
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
    let result = emit("[1, 2, 3] |> Pipe.tap((x) => Console.log(x))");
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
    let typed = crate::checker::attach_types(
        program,
        &crate::checker::ExprTypeMap::new(),
        &std::collections::HashSet::new(),
    );
    let output = Codegen::new().with_test_mode().generate(&typed);
    output.code.trim().to_string()
}

#[test]
fn test_block_stripped_in_production() {
    let result = emit(
        r#"
fn add(a: number, b: number) => number { a + b }

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

// (Inline for-declaration tests removed — only block form is supported)

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

fn describe(method: HttpMethod) => string {
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
fn handle(s: Status) => number {
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
fn validate(x: number) => Result<number, string> { Ok(x) }
fn f() => Result<number, Array<string>> {
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
fn f() => Result<number, Array<string>> {
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

// ── Use keyword (callback flattening) ────────────────────────

#[test]
fn use_basic() {
    let result = emit(
        r#"fn _test() => string {
    use x <- doSomething(42)
    x
}"#,
    );
    assert!(
        result.contains("doSomething(42, (x)"),
        "use should desugar to callback, got: {result}"
    );
}

#[test]
fn use_zero_binding() {
    let result = emit(
        r#"fn _test() => () {
    use <- delay(1000)
    Console.log("done")
}"#,
    );
    assert!(
        result.contains("delay(1000, ()"),
        "zero-binding use should produce no-param callback, got: {result}"
    );
}

#[test]
fn use_chained() {
    let result = emit(
        r#"fn _test() => string {
    use a <- first(1)
    use b <- second(a)
    b
}"#,
    );
    assert!(
        result.contains("first(1, (a)"),
        "first use should desugar, got: {result}"
    );
    assert!(
        result.contains("second(a, (b)"),
        "second use should nest inside first callback, got: {result}"
    );
}

#[test]
fn use_callback_block_returns_last_expr() {
    let result = emit(
        r#"fn _test() => number {
    use x <- doSomething(42)
    const y = x + 1
    y + 2
}"#,
    );
    assert!(
        result.contains("return y + 2"),
        "use callback block body should return last expression, got: {result}"
    );
}

#[test]
fn use_as_function_call_identifier() {
    let result = emit(
        r#"fn _test(promise: Promise<number>) => number {
    const value = use(promise)
    value
}"#,
    );
    assert!(
        result.contains("use(promise)"),
        "`use(...)` in expression position should parse as a function call, got: {result}"
    );
}

#[test]
fn use_as_member_access_identifier() {
    let result = emit(
        r#"fn _test(m: { use: string }) => string {
    m.use
}"#,
    );
    assert!(
        result.contains("m.use"),
        "`.use` in member position should parse as a field access, got: {result}"
    );
}

#[test]
fn use_bind_adjacent_to_use_call() {
    let result = emit(
        r#"fn _test(promise: Promise<number>) => number {
    use x <- doSomething(42)
    const fromHook = use(promise)
    x + fromHook
}"#,
    );
    assert!(
        result.contains("doSomething(42, (x)"),
        "use-bind should still desugar alongside a use() call, got: {result}"
    );
    assert!(
        result.contains("use(promise)"),
        "use() call should remain a plain call, got: {result}"
    );
}

#[test]
fn use_bind_object_destructure() {
    let result = emit(
        r#"fn _test() => number {
    use { a, b } <- provideValues()
    a + b
}"#,
    );
    assert!(
        result.contains("provideValues((") && result.contains("{ a, b }"),
        "object-destructured use should emit a single destructured callback param, got: {result}"
    );
}

#[test]
fn use_bind_object_destructure_with_rename() {
    let result = emit(
        r#"fn _test() => number {
    use { a: x, b: y } <- provideValues()
    x + y
}"#,
    );
    assert!(
        result.contains("a: x") && result.contains("b: y"),
        "renamed fields should appear in the destructure pattern, got: {result}"
    );
}

// ── Mock Built-in ────────────────────────────────────────────

#[test]
fn mock_string() {
    let result = emit("mock<string>");
    assert!(
        result.contains("\"mock-string-1\""),
        "should generate mock string, got: {result}"
    );
}

#[test]
fn mock_number() {
    let result = emit("mock<number>");
    assert!(
        result.contains('1'),
        "should generate mock number, got: {result}"
    );
}

#[test]
fn mock_boolean() {
    let result = emit("mock<boolean>");
    assert!(
        result.contains("true") || result.contains("false"),
        "should generate mock boolean, got: {result}"
    );
}

#[test]
fn mock_record_type() {
    let result = emit("mock<{ name: string, age: number }>");
    assert!(
        result.contains("name: \"mock-name-"),
        "should generate mock name field, got: {result}"
    );
    assert!(
        result.contains("age: "),
        "should generate mock age field, got: {result}"
    );
}

#[test]
fn mock_named_record() {
    let result = emit(
        "type User = { name: string, age: number }
const u = mock<User>",
    );
    assert!(
        result.contains("name: \"mock-name-"),
        "should generate mock name field, got: {result}"
    );
    assert!(
        result.contains("age: "),
        "should generate mock age field, got: {result}"
    );
}

#[test]
fn mock_with_override() {
    let result = emit(
        "type User = { name: string, age: number }
const u = mock<User>(name: \"Alice\")",
    );
    assert!(
        result.contains("name: \"Alice\""),
        "override should use provided value, got: {result}"
    );
    assert!(
        result.contains("age: "),
        "non-overridden field should be mocked, got: {result}"
    );
}

#[test]
fn mock_array_type() {
    let result = emit("mock<Array<number>>");
    assert!(
        result.contains('[') && result.contains(']'),
        "should generate mock array, got: {result}"
    );
}

#[test]
fn mock_union_type() {
    let result = emit(
        "type Status = | Active | Inactive
const s = mock<Status>",
    );
    assert!(
        result.contains("tag: \"Active\""),
        "should pick first variant, got: {result}"
    );
}

// ── typeof ──────────────────────────────────────────────────

#[test]
fn typeof_function_alias() {
    let result = emit(
        "fn greet(name: string) => string { `Hello, ${name}!` }
type Greeter = typeof greet",
    );
    assert!(
        result.contains("type Greeter = typeof greet;"),
        "should emit typeof in type alias, got: {result}"
    );
}

#[test]
fn typeof_const_alias() {
    let result = emit(
        "type Config = { baseUrl: string }
const config = Config(baseUrl: \"https://api.com\")
type MyConfig = typeof config",
    );
    assert!(
        result.contains("type MyConfig = typeof config;"),
        "should emit typeof for const binding, got: {result}"
    );
}

// ── intersection types ──────────────────────────────────────

#[test]
fn intersection_two_types() {
    let result = emit(
        "type A = { x: number }
type B = { y: string }
type C = A & B",
    );
    assert!(
        result.contains("type C = A & B;"),
        "should emit intersection type, got: {result}"
    );
}

#[test]
fn intersection_three_types() {
    let result = emit(
        "type A = { x: number }
type B = { y: string }
type D = A & B & { z: boolean }",
    );
    assert!(
        result.contains("A & B & { z: boolean }"),
        "should emit three-way intersection, got: {result}"
    );
}

#[test]
fn intersection_after_generic_type() {
    let result = emit(
        "type A = { x: number }
type B = { y: string }
type C = Array<A> & B",
    );
    assert!(
        result.contains("type C = Array<A> & B;"),
        "should emit intersection after generic type, got: {result}"
    );
}

#[test]
fn record_spread_emits_intersection() {
    let result = emit(
        "type A = { x: number }
type B = {
    ...A,
    y: string,
}",
    );
    assert!(
        result.contains("type B = A & { y: string }"),
        "record spread should emit as intersection, got: {result}"
    );
}

#[test]
fn string_literal_type_arg() {
    let result = emit("type A = Array<\"div\">");
    assert!(
        result.contains("type A = Array<\"div\">;"),
        "should emit string literal type arg, got: {result}"
    );
}

#[test]
fn jsx_spread_prop() {
    let result = emit(
        "type Props = { x: number }
fn _test(props: Props) => JSX.Element {
    <div {...props} />
}",
    );
    assert!(
        result.contains("{...props}"),
        "should emit JSX spread prop, got: {result}"
    );
}

// ── For-block function call namespacing ────────────────────

#[test]
fn for_block_bare_pipe_uses_mangled_name() {
    let result = emit(
        r#"
type Icon = | Grid | Columns

for Icon {
    fn toChar(self) => string {
        match self { Grid -> "G", Columns -> "C" }
    }
}

const _x = Grid |> toChar
"#,
    );
    assert!(
        result.contains("Icon__toChar("),
        "bare pipe call should use mangled name, got: {result}"
    );
    assert!(
        !result.replace("Icon__toChar(", "").contains("toChar("),
        "should not emit bare toChar call, got: {result}"
    );
}

#[test]
fn for_block_bare_identifier_uses_mangled_name() {
    let result = emit(
        r#"
type Icon = | Grid | Columns

for Icon {
    fn toChar(self) => string {
        match self { Grid -> "G", Columns -> "C" }
    }
}

const _f = toChar
"#,
    );
    assert!(
        result.contains("Icon__toChar"),
        "bare identifier should use mangled name, got: {result}"
    );
}

// ── Type-directed dispatch ────────────────────────────────

#[test]
fn user_union_named_ok_does_not_inherit_result_dispatch() {
    // A user-defined union whose variant happens to be called `Ok` must
    // use tagged (`.kind === "Ok"`) dispatch — not Result's `.ok === true`
    // — because the subject's type is not `Result`.
    let result = emit_typed(
        r#"
type Bag = | Ok(number) | Missing

export fn describe(b: Bag) => string {
    match b {
        Ok(n) -> "ok",
        Missing -> "missing",
    }
}
"#,
    );
    assert!(
        !result.contains(".ok === true"),
        "user-defined `Ok` variant must not use Result-style dispatch, got: {result}"
    );
    assert!(
        result.contains(r#".__tag === "Ok""#),
        "expected tagged-union dispatch, got: {result}"
    );
}

#[test]
fn real_result_match_uses_ok_field_discriminator() {
    let result = emit_typed(
        r#"
export fn describe(r: Result<number, string>) => string {
    match r {
        Ok(n) -> "ok",
        Err(e) -> "err",
    }
}
"#,
    );
    assert!(
        result.contains(".ok === true"),
        "Result match should use `.ok === true`, got: {result}"
    );
}

#[test]
fn user_record_tag_field_does_not_collide_with_union_discriminator() {
    // The discriminator is `__tag` so user records can keep a `tag`
    // field (HTML attributes, git tag IDs, etc.) without colliding with
    // the compiler's emitted union shape.
    let result = emit_typed(
        r#"
type Button = { tag: string, label: string }
type Route = | Home | Profile { id: string }

const btn = Button(tag: "nav-button", label: "Home")
const r = Home
"#,
    );
    // User's `tag` field survives as-is.
    assert!(
        result.contains(r#"tag: "nav-button""#),
        "user-defined `tag` should still appear, got:\n{result}"
    );
    // Discriminator is emitted as `__tag`.
    assert!(
        result.contains(r#"__tag: "Home""#),
        "union discriminator should use `__tag`, got:\n{result}"
    );
    // And they don't collide — the Button literal shouldn't sprout a `__tag`.
    assert!(
        !result.contains(r#"{ __tag: "nav-button""#),
        "user record should not get a discriminator, got:\n{result}"
    );
}

#[test]
fn pipe_unwrap_emits_early_return_on_none() {
    // `x |>? f` pipes into `f`, then early-returns on `None`/`Err` the
    // same way `(x |> f)?` does — identical runtime semantics.
    let result = emit_typed(
        r#"
fn half(n: number) => Option<number> {
    match n % 2 {
        0 -> Some(n / 2),
        _ -> None,
    }
}

fn run() => Option<number> {
    const x = 10 |>? half
    Some(x + 1)
}
"#,
    );
    assert!(
        result.contains("half("),
        "pipe target should be called, got:\n{result}"
    );
    assert!(
        result.contains("return") && result.contains(".ok"),
        "pipe-unwrap should emit an early-return check, got:\n{result}"
    );
    assert!(
        result.contains("x + 1"),
        "body after the unwrap should use the unwrapped value, got:\n{result}"
    );
}

#[test]
fn untrusted_call_detection_reads_callee_type() {
    // A call to an untrusted foreign fn must emit the try/catch boundary
    // wrapper — driven by `callee.ty.is_untrusted_foreign()`, not by a
    // parallel `untrusted_imports` side-table.
    let result = emit_typed(
        r#"
import { someFn } from "untrusted-pkg"

export fn wrap() => Result<number, Error> {
    someFn()
}
"#,
    );
    assert!(
        result.contains("try {") && result.contains("catch"),
        "untrusted call should be wrapped in try/catch, got: {result}"
    );
}
