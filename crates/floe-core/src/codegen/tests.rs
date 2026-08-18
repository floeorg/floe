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
        &std::collections::HashMap::new(),
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
    // Production order: the checker reads the tree lower.rs produced,
    // then desugar rewrites it. Desugaring first hands the checker the
    // synthetic ids desugar stamps on a spliced default (#1533), and the
    // checker would key one type map entry for all of them.
    let (_diags, expr_types, invalid_exprs, shadowed) =
        crate::checker::Checker::new().check_full(&program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed = crate::checker::attach_types(program, &expr_types, &invalid_exprs, &shadowed);
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
let safeDivide(a: number, b: number) -> number = { a / b }
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
let safeDivide(a: number, b: number) -> number = { a / b }
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
let f(a: number, b: number, c: number) -> number = { a + b + c }
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
let f(a: number, b: number, c: number) -> number = { a + b + c }
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
let g(a: number, b: number = 2, c: number = 3, d: number) -> number = { a + b + c + d }
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
let greet(name: string, greeting: string = "hello") -> string = { greeting }
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
let f(a: number) -> number = { a }
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
    assert_eq!(emit("let x = 42"), "const x = 42;");
}

#[test]
fn const_with_type() {
    assert_eq!(emit("let x: number = 42"), "const x: number = 42;");
}

#[test]
fn export_const() {
    assert_eq!(emit("export let x = 42"), "export const x = 42;");
}

#[test]
fn default_export_emits_named_reexport() {
    let out = emit("let app = 1\nexport default app");
    assert!(out.contains("const app = 1;"), "got: {out}");
    assert!(out.contains("export { app as default };"), "got: {out}");
}

#[test]
fn default_export_preserves_outer_export() {
    let out = emit("export let app = 1\nexport default app");
    assert!(out.contains("export const app = 1;"), "got: {out}");
    assert!(out.contains("export { app as default };"), "got: {out}");
}

#[test]
fn function_decl() {
    let result = emit("let add(a: number, b: number) -> number = { a + b }");
    assert_eq!(
        result,
        "function add(a: number, b: number): number {\n  return a + b;\n}"
    );
}

#[test]
fn export_function() {
    let result = emit("export let greet() = { \"hi\" }");
    assert!(result.starts_with("export function greet()"));
}

#[test]
fn promise_await_emits_async_function() {
    let result = emit_with_types("let fetch() -> Promise<string> = { getData() |> Promise.await }");
    assert!(result.starts_with("async function fetch()"));
    assert!(result.contains("await getData()"));
}

#[test]
fn async_fn_sugar_wraps_return_type_in_promise() {
    // `async fn f() -> T` should emit `async function f(): Promise<T>`
    let result = emit_with_types("async let fetch() -> string = { \"hi\" }");
    assert!(
        result.starts_with("async function fetch(): Promise<string>"),
        "expected async + Promise wrap, got: {result}"
    );
}

#[test]
fn async_fn_sugar_with_await_body() {
    let result =
        emit_with_types("async let fetch() -> string = { let x = getData() |> Promise.await\n x }");
    assert!(
        result.starts_with("async function fetch(): Promise<string>"),
        "expected async + Promise wrap, got: {result}"
    );
    assert!(result.contains("await getData()"));
}

#[test]
fn function_with_defaults() {
    let result = emit("let f(x: number = 10) = { x }");
    assert!(result.contains("x: number = 10"));
}

// ── Imports ──────────────────────────────────────────────────

#[test]
fn import_named() {
    // Both names used in value positions → regular import
    assert_eq!(
        emit(
            r#"import trusted { useState, useEffect } from "react"
let x = useState(0)
let y = useEffect"#
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
let x: Option<Session> = None"#
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
let map(arr: Array<number>, f: (number) -> number) -> Array<number> = { arr }
let _items = [1, 2, 3] |> map((x) -> x + 1)
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
let get(r: Router, path: string) -> Router = { Router(path: path) }
let _r = Router(path: "/") |> get("/hello")
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

#[test]
fn partial_application_multiple_placeholders() {
    // add3(_, 5, _) -> (_x0, _x1) => add3(_x0, 5, _x1)
    assert_eq!(emit("add3(_, 5, _)"), "(_x0, _x1) => add3(_x0, 5, _x1);");
}

#[test]
fn partial_application_three_placeholders() {
    assert_eq!(
        emit("add4(_, _, 10, _)"),
        "(_x0, _x1, _x2) => add4(_x0, _x1, 10, _x2);"
    );
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
        emit(r#"User(name: "New", ..user)"#),
        r#"{ ...user, name: "New" };"#
    );
}

#[test]
fn constructor_with_defaults_omitted() {
    let result = emit(
        r#"
        type Config = { baseUrl: string, timeout: number = 5000, retries: number = 3 }
        let c = Config(baseUrl: "https://api.com")
        "#,
    );
    assert!(result.contains(r#"baseUrl: "https://api.com", timeout: 5000, retries: 3"#));
}

#[test]
fn constructor_with_defaults_overridden() {
    let result = emit(
        r#"
        type Config = { baseUrl: string, timeout: number = 5000, retries: number = 3 }
        let c = Config(baseUrl: "https://api.com", timeout: 10000)
        "#,
    );
    assert!(result.contains(r#"baseUrl: "https://api.com", timeout: 10000, retries: 3"#));
}

#[test]
fn constructor_all_defaults() {
    let result = emit(
        r#"
        type Options = { timeout: number = 5000, retries: number = 3 }
        let o = Options()
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
        let c = Config(baseUrl: "https://api.com")
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
        let d = Dto(name: Value("Ryan"))
        "#,
    );
    assert!(result.contains(r#"name: "Ryan""#));
}

#[test]
fn settable_clear_emits_null() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged }
        let d = Dto(name: Clear)
        "#,
    );
    assert!(result.contains("name: null"));
}

#[test]
fn settable_unchanged_omits_field() {
    let result = emit(
        r#"
        type Dto = { name: Settable<string> = Unchanged, age: Settable<number> = Unchanged }
        let d = Dto(name: Value("Ryan"))
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
        let d = Dto()
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
    assert_eq!(emit("typealias Name = string"), "type Name = string;");
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
    let result = emit("let x: Option<string> = None");
    assert!(result.contains("string | null | undefined"));
}

#[test]
fn result_type() {
    let result = emit("typealias Res = Result<User, ApiError>");
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
        &std::collections::HashMap::new(),
    );
    let output = Codegen::new().generate(&typed);
    assert!(output.has_jsx);
}

#[test]
fn no_jsx_detection() {
    let program = Parser::new("let x = 42").parse_program().unwrap();
    let typed = crate::checker::attach_types(
        program,
        &crate::checker::ExprTypeMap::new(),
        &std::collections::HashSet::new(),
        &std::collections::HashMap::new(),
    );
    let output = Codegen::new().generate(&typed);
    assert!(!output.has_jsx);
}

// ── JSX namespace import (#1498) ─────────────────────────────

/// Compile a source and return the emitted `.ts` and `.d.ts`.
fn emit_ts_and_dts(input: &str) -> (String, String) {
    let mut program = Parser::new(input).parse_program().unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let (_diags, expr_types, invalid_exprs, shadowed) =
        crate::checker::Checker::new().check_full(&program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed = crate::checker::attach_types(program, &expr_types, &invalid_exprs, &shadowed);
    let output = Codegen::new().generate(&typed);

    (output.code, output.dts)
}

#[test]
fn jsx_element_return_type_imports_the_namespace() {
    let (code, _dts) = emit_ts_and_dts("let Badge() -> JSX.Element = { <span /> }");
    assert!(
        code.starts_with(crate::type_layout::JSX_TYPE_IMPORT),
        "the emitted file should import the JSX namespace, got: {code}"
    );
}

#[test]
fn jsx_element_in_dts_imports_the_namespace() {
    let (_code, dts) = emit_ts_and_dts("export let Badge() -> JSX.Element = { <span /> }");
    assert!(
        dts.starts_with(crate::type_layout::JSX_TYPE_IMPORT),
        "the emitted declarations should import the JSX namespace, got: {dts}"
    );
}

/// A component the file keeps to itself declares no `.d.ts` entry, so the
/// declarations name no `JSX.Element` and take no import.
#[test]
fn unexported_component_leaves_the_dts_import_out() {
    let (code, dts) = emit_ts_and_dts("let Badge() -> JSX.Element = { <span /> }");
    assert!(code.contains(crate::type_layout::JSX_TYPE_IMPORT));
    assert!(
        !dts.contains(crate::type_layout::JSX_TYPE_IMPORT),
        "the declarations name no JSX.Element, got: {dts}"
    );
}

/// A file that imports `JSX` itself already binds the name. A second
/// import would be a duplicate identifier, so the written one wins.
#[test]
fn written_jsx_import_wins_over_the_emitted_one() {
    let (code, _dts) = emit_ts_and_dts(
        "import trusted { JSX } from \"react\"\n\nlet Badge() -> JSX.Element = { <span /> }",
    );
    assert!(
        !code.contains(crate::type_layout::JSX_TYPE_IMPORT),
        "codegen should not add a second JSX import, got: {code}"
    );
    assert!(code.contains("JSX.Element"), "got: {code}");
}

/// A file-scope value named `JSX` binds no namespace, so it must not
/// suppress the import. The first guard read the value scope, so it did,
/// and the emitted file kept the `TS2503` this fix removes.
#[test]
fn a_value_named_jsx_still_gets_the_import() {
    let (code, _dts) =
        emit_ts_and_dts("let JSX() = { 1 }\n\nlet Badge() -> JSX.Element = { <span /> }");
    assert!(
        code.starts_with(crate::type_layout::JSX_TYPE_IMPORT),
        "a value named JSX binds no namespace, so it must not suppress the import, got: {code}"
    );
}

/// A local `type JSX` does not suppress the import either. TypeScript
/// holds an imported namespace and a local type alias in separate
/// declaration spaces, so the two stand together, and only the import
/// makes `JSX.Element` resolve.
#[test]
fn a_type_named_jsx_still_gets_the_import() {
    let (code, _dts) = emit_ts_and_dts(
        "type JSX = { Element: unknown }\n\nexport let take(a: JSX.Element) -> number = { 1 }",
    );
    assert!(
        code.starts_with(crate::type_layout::JSX_TYPE_IMPORT),
        "a type named JSX must not suppress the import, got: {code}"
    );
}

/// A file that names no `JSX.Element` gets no import, even when it holds
/// JSX. The type is what needs the namespace, not the syntax.
#[test]
fn a_file_without_the_type_gets_no_import() {
    let (code, _dts) = emit_ts_and_dts("let render() = { <span /> }");
    assert!(!code.contains("from \"react\""), "got: {code}");
}

// ── Generic Functions ─────────────────────────────────────────

#[test]
fn generic_function_codegen() {
    assert_eq!(
        emit("let identity<T>(x: T) -> T = { x }"),
        "function identity<T>(x: T): T {\n  return x;\n}"
    );
}

#[test]
fn generic_function_multi_params_codegen() {
    assert_eq!(
        emit("let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }"),
        "function pair<A, B>(a: A, b: B): readonly [A, B] {\n  return [a, b];\n}"
    );
}

// ── Explicit Type Arguments at a Call Site ────────────────────

#[test]
fn call_with_an_explicit_type_argument_keeps_it() {
    let result = emit_typed("let identity<T>(x: T) -> T = { x }\nlet a = identity<string>(\"hi\")");
    assert!(
        result.contains("const a = identity<string>(\"hi\");"),
        "the call must carry the type argument the user wrote, got: {result}"
    );
}

#[test]
fn tuple_destructure_keeps_the_type_arguments() {
    let result = emit_typed(
        "let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }\nlet (p, q) = pair<string, number>(\"a\", 1)",
    );
    assert!(
        result.contains("const [p, q] = pair<string, number>(\"a\", 1);"),
        "a tuple destructure must carry both type arguments, got: {result}"
    );
}

#[test]
fn call_in_expression_position_keeps_the_type_argument() {
    let result = emit_typed("let identity<T>(x: T) -> T = { x }\nlet b = identity<number>(1) + 2");
    assert!(
        result.contains("const b = identity<number>(1) + 2;"),
        "a call inside an expression must carry the type argument, got: {result}"
    );
}

#[test]
fn call_inside_a_pipe_keeps_the_type_argument() {
    let result =
        emit_typed("let identity<T>(x: T) -> T = { x }\nlet c = \"x\" |> identity<string>()");
    assert!(
        result.contains("const c = identity<string>(\"x\");"),
        "a piped call must carry the type argument, got: {result}"
    );
}

#[test]
fn partial_application_keeps_the_type_arguments() {
    let result =
        emit_typed("let take<A, B>(a: A, b: B) -> A = { a }\nlet f = take<string, number>(_, 1)");
    assert!(
        result.contains("take<string, number>(_x, 1)"),
        "a partial application must carry the type arguments, got: {result}"
    );
}

#[test]
fn a_piped_placeholder_call_keeps_the_type_arguments() {
    let result = emit_typed(
        "let take<A, B>(a: A, b: B) -> A = { a }\nlet g = \"z\" |> take<string, number>(_, 1)",
    );
    assert!(
        result.contains("take<string, number>(\"z\", 1)"),
        "a piped placeholder call must carry the type arguments, got: {result}"
    );
}

#[test]
fn an_array_type_argument_emits_its_typescript_form() {
    let result = emit_typed(
        "type Todo = { id: string }\nlet identity<T>(x: T) -> T = { x }\nlet e = identity<Array<Todo>>([])",
    );
    assert!(
        result.contains("const e = identity<Array<Todo>>([]);"),
        "an `Array<T>` type argument must emit as an array type, got: {result}"
    );
}

#[test]
fn a_call_without_type_arguments_emits_none() {
    let result = emit_typed("let identity<T>(x: T) -> T = { x }\nlet d = identity(\"hi\")");
    assert!(
        result.contains("const d = identity(\"hi\");"),
        "a call the user left to inference must stay bare, got: {result}"
    );
    assert!(
        !result.contains("identity<string>"),
        "codegen must not invent a type argument the user did not write, got: {result}"
    );
}

// ── Pipe Lambdas ─────────────────────────────────────────────

#[test]
fn lambda_single_arg() {
    assert_eq!(emit("(x) -> x + 1"), "(x) => x + 1;");
}

#[test]
fn lambda_multi_arg() {
    assert_eq!(emit("(a, b) -> a + b"), "(a, b) => a + b;");
}

// ── Derived function binding ─────────────────────────────────

#[test]
fn fn_binding_partial_application() {
    assert_eq!(
        emit("let add(a: number, b: number) -> number = { a + b }\nlet inc = add(1, _)"),
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
    let result = emit("let x = 1 + 2");
    assert!(
        !result.contains("__floeEq"),
        "expected no __floeEq helper, got:\n{result}"
    );
}

#[test]
fn floe_eq_helper_emitted_for_dot_shorthand_eq() {
    // Dot shorthand with == should emit the helper
    let result = emit("let active = todos |> Array.filter(.done == false)");
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
        "let _x: Option<Array<number>> = None\nlet _y = _x |> Option.unwrapOr([]) |> filter((n) -> n > 0)",
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
    let result = emit("let _x: Option<number> = None\nlet _y = _x |> Option.map((n) -> n + 1)");
    assert!(
        result.contains("!= null") && !result.contains("!== undefined"),
        "Option.map should use != null, not !== undefined, got: {result}"
    );
}

// ── Promise.await ───────────────────────────────────────────

#[test]
fn promise_await_pipe() {
    let result = emit_with_types("let _x = fetchData() |> Promise.await");
    assert!(result.contains("await fetchData()"));
}

#[test]
fn bare_await_shorthand_emits_async_function() {
    let result = emit_with_types("let fetch() -> Promise<string> = { getData() |> await }");
    assert!(
        result.starts_with("async function fetch()"),
        "bare `|> await` should infer async on enclosing function, got: {result}"
    );
    assert!(result.contains("await getData()"));
}

#[test]
fn bare_await_shorthand_pipe() {
    let result = emit_with_types("let _x = fetchData() |> await");
    assert!(result.contains("await fetchData()"));
}

#[test]
fn nested_fn_with_promise_await_emits_async() {
    let result =
        emit_with_types("let outer() = { let inner() = { getData() |> Promise.await } inner() }");
    assert!(
        result.contains("async function inner()"),
        "nested fn with Promise.await should be async, got: {result}"
    );
}

#[test]
fn nested_fn_with_bare_await_emits_async() {
    let result = emit_with_types("let outer() = { let inner() = { getData() |> await } inner() }");
    assert!(
        result.contains("async function inner()"),
        "nested fn with bare await should be async, got: {result}"
    );
}

#[test]
fn await_before_member_access_is_parenthesized() {
    let result = emit_with_types(
        "type R = Ok(number) | Err(string)\n\
         let f() -> Promise<number> = { match getResult() |> await { Ok(v) -> v, Err(_) -> 0 } }",
    );
    assert!(
        result.contains("(await getResult())"),
        "`await` feeding member access must parenthesize — JS parses `await X.Y` as `await (X.Y)` — got: {result}"
    );
    assert!(
        !result.contains(" await getResult().tag"),
        "`await X.tag` must not appear — JS would await the tag of the unawaited Promise, got: {result}"
    );
    assert!(
        !result.contains(" await getResult().value"),
        "`await X.value` must not appear — JS would await the value of the unawaited Promise, got: {result}"
    );
}

#[test]
fn match_on_comparison_wraps_subject_in_parens() {
    let result = emit("let _x = match 5 > 0 { true -> \"yes\", false -> \"no\" }");
    assert!(
        result.contains("(5 > 0) === true"),
        "match on comparison should wrap subject in parens, got: {result}"
    );
}

#[test]
fn match_arm_block_iife_returns_last_expr() {
    let result = emit("let _x = match true { true -> { let a = 1\na + 2 }, false -> 0 }");
    assert!(
        result.contains("return a + 2"),
        "match arm block IIFE should return last expression, got: {result}"
    );
}

#[test]
fn match_with_awaited_subject_and_bindings_emits_async_iife() {
    let result = emit_with_types(
        "type R = Ok(number) | Err(string)\n\
         let f() -> Promise<number> = { match fetchResult() |> await { Ok(value) -> value, Err(message) -> 0 } }",
    );
    assert!(
        result.contains("await (async () => {"),
        "match arms with awaited subject must wrap bindings in async IIFE, got: {result}"
    );
}

#[test]
fn match_without_await_keeps_sync_iife() {
    let result = emit_with_types(
        "type R = Ok(number) | Err(string)\n\
         let f() -> number = { match getResult() { Ok(value) -> value, Err(message) -> 0 } }",
    );
    assert!(
        result.contains("(() => {"),
        "match without await should stay sync, got: {result}"
    );
    assert!(
        !result.contains("async () =>"),
        "match without await must not produce async IIFE, got: {result}"
    );
}

// ── Implicit Return ──────────────────────────────────────────

#[test]
fn implicit_return_single_expr() {
    let result = emit("let f() -> number = { 42 }");
    assert!(result.contains("return 42"));
}

#[test]
fn implicit_return_multi_statement() {
    let result = emit("let f() -> number = { let x = 1\nx + 1 }");
    assert!(result.contains("return x + 1"));
}

#[test]
fn unit_function_no_return() {
    let result = emit("let f() -> () = { Console.log(\"hi\") }");
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
fn stdlib_array_sort_with_calls_the_comparator() {
    assert_eq!(
        emit("Array.sortWith([3, 1, 2], (a, b) -> a - b)"),
        "[...[3, 1, 2]].sort((a, b) => a - b);"
    );
}

#[test]
fn stdlib_array_map() {
    assert_eq!(
        emit("Array.map([1, 2], (n) -> n * 2)"),
        "[1, 2].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_array_filter() {
    assert_eq!(
        emit("Array.filter([1, 2, 3], (n) -> n > 1)"),
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
        emit("Array.any([1, 2, 3], (n) -> n > 2)"),
        "[1, 2, 3].some((n) => n > 2);"
    );
}

#[test]
fn stdlib_array_all() {
    assert_eq!(
        emit("Array.all([1, 2, 3], (n) -> n > 0)"),
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
    let result = emit("Option.map(Some(1), (n) -> n * 2)");
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

// glb #1492. The pipe form is what the store example uses, and it is the
// form that dropped the comparator. Pin it so the argument reaches `sort`.
#[test]
fn stdlib_pipe_array_sort_with_keeps_the_comparator() {
    assert_eq!(
        emit("[3, 1, 2] |> Array.sortWith((a, b) -> b - a)"),
        "[...[3, 1, 2]].sort((a, b) => b - a);"
    );
}

#[test]
fn stdlib_pipe_with_args() {
    assert_eq!(
        emit("[1, 2, 3] |> Array.map((n) -> n * 2)"),
        "[1, 2, 3].map((n) => n * 2);"
    );
}

#[test]
fn stdlib_pipe_chain() {
    let result = emit("[1, 2, 3] |> Array.filter((n) -> n > 1) |> Array.reverse");
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
    let (_, expr_types, _, shadowed) = crate::checker::Checker::new().check_full(&program);
    let mut program = program;
    crate::checker::mark_async_functions(&mut program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed = crate::checker::attach_types(
        program,
        &expr_types,
        &std::collections::HashSet::new(),
        &shadowed,
    );
    Codegen::new().generate(&typed).code.trim().to_string()
}

#[test]
fn type_directed_array_length() {
    let result = emit_with_types("let _x = [1, 2, 3] |> length");
    assert_eq!(result, "const _x = [1, 2, 3].length;");
}

#[test]
fn type_directed_string_length() {
    let result = emit_with_types(r#"let _x = "hello" |> length"#);
    assert_eq!(result, r#"const _x = "hello".length;"#);
}

#[test]
fn type_directed_array_filter() {
    let result = emit_with_types(r#"let _x = [1, 2, 3] |> filter((x) -> x > 1)"#);
    assert_eq!(result, "const _x = [1, 2, 3].filter((x) => x > 1);");
}

#[test]
fn union_variant_dot_access() {
    let result = emit(
        r#"
type Filter = | All | Active | Completed
let _f = Filter.All
"#,
    );
    assert!(result.contains(r#"{ __tag: "All" }"#));
}

#[test]
fn union_variant_dot_access_non_union_passthrough() {
    // Regular member access should still work normally
    let result = emit("let _x = foo.bar");
    assert!(result.contains("foo.bar"));
}

// ── Variant constructors as functions ──────────────────────

#[test]
fn non_unit_variant_as_function() {
    let result = emit(
        r#"
type SaveError = | Validation { errors: Array<string> }
    | Api { message: string }

let _f = Validation
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
let _f = All
"#,
    );
    assert!(result.contains(r#"{ __tag: "All" }"#), "got: {result}");
    assert!(
        !result.contains("->"),
        "should not emit arrow function, got: {result}"
    );
}

#[test]
fn qualified_non_unit_variant_as_function() {
    let result = emit(
        r#"
type SaveError = | Validation { errors: Array<string> }
    | Api { message: string }

let _f = SaveError.Validation
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

let _e = Validation(message: "bad")
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

let _f = Rect
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
    let result = emit("let (x, y) = point");
    assert_eq!(result, "const [x, y] = point;");
}

#[test]
fn tuple_type_annotation() {
    let result = emit("let p: (number, string) = (1, \"a\")");
    assert!(result.contains("readonly [number, string]"));
    assert!(result.contains("[1, \"a\"]"));
}

#[test]
fn tuple_return_type() {
    let result = emit("let f(a: number) -> (number, string) = { (a, \"x\") }");
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
    let result = emit("[1, 2, 3] |> Pipe.tap((x) -> Console.log(x))");
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
        &std::collections::HashMap::new(),
    );
    let output = Codegen::new().with_test_mode().generate(&typed);
    output.code.trim().to_string()
}

#[test]
fn test_block_stripped_in_production() {
    let result = emit(
        r#"
let add(a: number, b: number) -> number = { a + b }

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

// ── Contextual keyword shadowing ────────────────────────────

#[test]
fn bare_todo_without_shadow_still_panics() {
    let result = emit_typed("todo");
    assert!(
        result.contains("throw new Error"),
        "expected throw, got: {result}"
    );
}

#[test]
fn local_todo_binding_shadows_keyword() {
    let result = emit_typed(
        r#"let todo = 5
let used = todo"#,
    );
    assert!(
        !result.contains("not yet implemented"),
        "shadowed todo should not emit panic, got: {result}"
    );
    assert!(
        result.contains("const used = todo"),
        "shadowed todo should compile to identifier read, got: {result}"
    );
}

#[test]
fn local_unreachable_binding_shadows_keyword() {
    let result = emit_typed(
        r#"let unreachable = 42
let out = unreachable"#,
    );
    assert!(
        result.contains("const out = unreachable"),
        "shadowed unreachable should compile to identifier read, got: {result}"
    );
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

let describe(method: HttpMethod) -> string = {
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
let handle(s: Status) -> number = {
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
let validate(x: number) -> Result<number, string> = { Ok(x) }
let f() -> Result<number, Array<string>> = {
    collect {
        let a = validate(1)?
        let b = validate(2)?
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
let f() -> Result<number, Array<string>> = {
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

#[test]
fn collect_awaiting_inside_an_array_emits_async_iife() {
    // The await sits inside an array literal. Codegen used to carry its own
    // walk, and that walk never entered an array, so it emitted `await`
    // inside a plain `(() => {` arrow while the checker read the same body
    // as async. Both passes now read `body_has_promise_await` (glb #1516).
    let result = emit_with_types(
        r#"
async let g() -> number = { 1 }
async let f() -> Result<number, Array<Error>> = {
    collect {
        let xs = [g() |> await]
        xs |> length
    }
}
"#,
    );
    assert!(
        result.contains("(async () => {"),
        "collect block that awaits inside an array must emit an async IIFE, got: {result}"
    );
    assert!(
        result.contains("await g()"),
        "should still emit the inner await, got: {result}"
    );
}

// ── Parse<T> Built-in ────────────────────────────────────────

#[test]
fn parse_string_type() {
    let result = emit_with_types("parse<string>(x)");
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
    let result = emit_with_types("parse<number>(x)");
    assert!(
        result.contains("typeof __v !== \"number\""),
        "should check typeof for number, got: {result}"
    );
}

#[test]
fn parse_boolean_type() {
    let result = emit_with_types("parse<boolean>(x)");
    assert!(
        result.contains("typeof __v !== \"boolean\""),
        "should check typeof for boolean, got: {result}"
    );
}

#[test]
fn parse_record_type_codegen() {
    let result = emit_with_types("parse<{ name: string, age: number }>(data)");
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
    let result = emit_with_types("parse<Array<number>>(items)");
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
    let result = emit_with_types("x |> parse<string>");
    assert!(
        result.contains("const __v = x"),
        "should use piped value, got: {result}"
    );
    assert!(
        result.contains("typeof __v !== \"string\""),
        "should validate type, got: {result}"
    );
}

#[test]
fn parse_with_awaited_value_emits_async_iife() {
    let result =
        emit_with_types("let f() -> Promise<unknown> = { fetchData() |> await |> parse<string> }");
    assert!(
        result.contains("await (async () => {"),
        "parse with awaited value must wrap in async IIFE that is awaited, got: {result}"
    );
    assert!(
        result.contains("await fetchData()"),
        "should still emit the inner await, got: {result}"
    );
}

#[test]
fn parse_without_await_keeps_sync_iife() {
    let result = emit_with_types("x |> parse<string>");
    assert!(
        result.contains("(() => {"),
        "parse without await should stay sync, got: {result}"
    );
    assert!(
        !result.contains("async () =>"),
        "parse without await must not produce async IIFE, got: {result}"
    );
}

// ── Codegen reads the resolved type, not the annotation (#1521) ──

#[test]
fn parse_validates_the_type_an_alias_resolves_to() {
    let result = emit_with_types(
        "typealias Id = string\nlet f(raw: unknown) -> Result<Id, Error> = { parse<Id>(raw) }",
    );
    assert!(
        result.contains("typeof __v !== \"string\""),
        "parse<Id> must validate the string the alias resolves to, got: {result}"
    );
    assert!(
        !result.contains("expected object"),
        "parse<Id> must not validate an alias of string as an object, got: {result}"
    );
}

#[test]
fn parse_in_a_pipe_validates_the_type_an_alias_resolves_to() {
    let result = emit_with_types(
        "typealias Id = string\nlet f(raw: unknown) -> Result<Id, Error> = { raw |> parse<Id> }",
    );
    assert!(
        result.contains("typeof __v !== \"string\""),
        "the pipe form must resolve the alias too, got: {result}"
    );
}

#[test]
fn parse_validates_a_tuple_as_an_array_of_a_fixed_length() {
    let result = emit_with_types(
        "typealias Pair = (number, number)\n\
         let f(raw: unknown) -> Result<Pair, Error> = { parse<Pair>(raw) }",
    );
    assert!(
        result.contains("!Array.isArray(__v)"),
        "a tuple is an array at run time, got: {result}"
    );
    assert!(
        result.contains("__v.length !== 2"),
        "a tuple carries a fixed number of elements, got: {result}"
    );
    assert!(
        result.contains("typeof __v[0] !== \"number\""),
        "each element carries its own type, got: {result}"
    );
    assert!(
        result.contains("typeof __v[1] !== \"number\""),
        "each element carries its own type, got: {result}"
    );
}

#[test]
fn parse_validates_a_function_type_with_typeof() {
    let result = emit_with_types(
        "typealias Handler = (a: number) -> number\n\
         let f(raw: unknown) -> Result<Handler, Error> = { parse<Handler>(raw) }",
    );
    assert!(
        result.contains("typeof __v !== \"function\""),
        "a function type checks as a function, got: {result}"
    );
}

#[test]
fn parse_and_mock_agree_on_an_alias() {
    let source = "typealias Id = string\n\
                  let f(raw: unknown) -> Result<Id, Error> = { parse<Id>(raw) }\n\
                  let g() -> Id = { mock<Id>() }";
    let result = emit_with_types(source);
    assert!(
        result.contains("typeof __v !== \"string\""),
        "parse<Id> must read the alias as a string, got: {result}"
    );
    assert!(
        result.contains("\"mock-Id-1\""),
        "mock<Id> must read the alias as a string too, got: {result}"
    );
}

#[test]
fn bare_option_emits_what_option_unknown_means() {
    let result = emit_with_types("let f(x: Option) -> number = { 1 }");
    assert!(
        result.contains("x: unknown | null | undefined"),
        "the checker reads a bare `Option` as `Option<Unknown>`, got: {result}"
    );
    assert!(
        !result.contains("x: Option"),
        "`Option` is a name TypeScript does not declare, got: {result}"
    );
}

#[test]
fn bare_settable_emits_what_settable_unknown_means() {
    let result = emit_with_types("let f(x: Settable) -> number = { 1 }");
    assert!(
        result.contains("x: unknown | null | undefined"),
        "the checker reads a bare `Settable` as `Settable<Unknown>`, got: {result}"
    );
}

#[test]
fn bare_result_emits_what_result_unknown_unknown_means() {
    let result = emit_with_types("let f(x: Result) -> number = { 1 }");
    assert!(
        result.contains("{ ok: true; value: unknown } | { ok: false; error: unknown }"),
        "the checker reads a bare `Result` as `Result<Unknown, Unknown>`, got: {result}"
    );
    assert!(
        !result.contains("x: Result"),
        "`Result` is a name TypeScript does not declare, got: {result}"
    );
}

// ── Use keyword (callback flattening) ────────────────────────

#[test]
fn use_basic() {
    let result = emit(
        r#"let _test() -> string = {
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
        r#"let _test() -> () = {
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
        r#"let _test() -> string = {
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
fn use_piped_call_appends_continuation_to_inner_call() {
    let result = emit(
        r#"let _test() -> number = {
    use x <- 1 |> doSomething(42)
    x
}"#,
    );
    assert!(
        result.contains("doSomething(1, 42, (x)"),
        "piped use should expand into the piped-into call, got: {result}"
    );
}

#[test]
fn use_piped_bare_identifier_appends_continuation() {
    let result = emit(
        r#"let _test() -> number = {
    use x <- 1 |> doSomething
    x
}"#,
    );
    assert!(
        result.contains("doSomething(1, (x)"),
        "piped-into bare identifier should receive the continuation, got: {result}"
    );
}

#[test]
fn use_chained_pipe_threads_continuation_to_final_call() {
    let result = emit(
        r#"let _test() -> number = {
    use x <- 1 |> stepA |> stepB(42)
    x
}"#,
    );
    assert!(
        result.contains("stepB(stepA(1), 42, (x)"),
        "continuation should reach the final piped-into call, got: {result}"
    );
}

#[test]
fn use_callback_block_returns_last_expr() {
    let result = emit(
        r#"let _test() -> number = {
    use x <- doSomething(42)
    let y = x + 1
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
        r#"let _test(promise: Promise<number>) -> number = {
    let value = use(promise)
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
        r#"let _test(m: { use: string }) -> string = {
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
        r#"let _test(promise: Promise<number>) -> number = {
    use x <- doSomething(42)
    let fromHook = use(promise)
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
        r#"let _test() -> number = {
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
        r#"let _test() -> number = {
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
let u = mock<User>",
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
let u = mock<User>(name: \"Alice\")",
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
let s = mock<Status>",
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
        "let greet(name: string) -> string = { `Hello, ${name}!` }
typealias Greeter = typeof greet",
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
let config = Config(baseUrl: \"https://api.com\")
typealias MyConfig = typeof config",
    );
    assert!(
        result.contains("type MyConfig = typeof config;"),
        "should emit typeof for let binding, got: {result}"
    );
}

// ── intersection types ──────────────────────────────────────

#[test]
fn intersection_two_types() {
    let result = emit(
        "type A = { x: number }
type B = { y: string }
typealias C = A & B",
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
typealias D = A & B & { z: boolean }",
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
typealias C = Array<A> & B",
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
    let result = emit("typealias A = Array<\"div\">");
    assert!(
        result.contains("type A = Array<\"div\">;"),
        "should emit string literal type arg, got: {result}"
    );
}

#[test]
fn jsx_spread_prop() {
    let result = emit(
        "type Props = { x: number }
let _test(props: Props) -> JSX.Element = {
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
    let toChar(self) -> string = {
        match self { Grid -> "G", Columns -> "C" }
    }
}

let _x = Grid |> toChar
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
    let toChar(self) -> string = {
        match self { Grid -> "G", Columns -> "C" }
    }
}

let _f = toChar
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

export let describe(b: Bag) -> string = {
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
export let describe(r: Result<number, string>) -> string = {
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

let btn = Button(tag: "nav-button", label: "Home")
let r = Home
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
let half(n: number) -> Option<number> = {
    match n % 2 {
        0 -> Some(n / 2),
        _ -> None,
    }
}

let run() -> Option<number> = {
    let x = 10 |>? half
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

export let wrap() -> Result<number, Error> = {
    someFn()
}
"#,
    );
    assert!(
        result.contains("try {") && result.contains("catch"),
        "untrusted call should be wrapped in try/catch, got: {result}"
    );
}

// ── Brace-form record construction (#1409) ───────────────────────

#[test]
fn brace_construct_emits_object_literal() {
    let output = emit_typed(
        r#"
type User = { id: string, name: string }
let u = User { id: "1", name: "Ryan" }
"#,
    );
    // Should emit a plain object literal with named fields.
    assert!(
        output.contains(r#"{ id: "1", name: "Ryan" }"#),
        "expected object literal, got:\n{output}"
    );
}

#[test]
fn brace_construct_with_spread_emits_object_spread() {
    let output = emit_typed(
        r#"
type User = { id: string, name: string }
let base = User { id: "1", name: "Ryan" }
let updated = User { name: "Sky", ..base }
"#,
    );
    assert!(
        output.contains("...base"),
        "expected JS spread, got:\n{output}"
    );
}

#[test]
fn brace_construct_with_punning_emits_inferred_field() {
    let output = emit_typed(
        r#"
type User = { id: string, name: string }
let id = "1"
let name = "Ryan"
let u = User { id, name }
"#,
    );
    assert!(
        output.contains("id: id") && output.contains("name: name"),
        "expected punning to expand to `id: id`, got:\n{output}"
    );
}

// ── URL Stdlib (#1426) ───────────────────────────────────────

#[test]
fn stdlib_url_parse() {
    let result = emit(r#"URL.parse("http://example.com")"#);
    assert!(
        result.contains("new URL(\"http://example.com\")"),
        "expected `new URL(...)`, got: {result}"
    );
    assert!(
        result.contains("try") && result.contains("catch"),
        "expected try/catch wrapper, got: {result}"
    );
    assert!(
        result.contains("ok: true as const") && result.contains("ok: false as const"),
        "expected Result literal, got: {result}"
    );
}

#[test]
fn stdlib_url_field_accessors_pipe() {
    let result = emit_typed(
        r#"
let host = match URL.parse("http://example.com") {
    Ok(u) -> u |> URL.host,
    Err(_) -> "",
}
"#,
    );
    assert!(
        result.contains("u.host"),
        "expected `u.host` for `URL.host` accessor, got: {result}"
    );
}

#[test]
fn stdlib_url_to_string() {
    let result = emit_typed(
        r#"
let s = match URL.parse("http://example.com") {
    Ok(u) -> u |> URL.toString,
    Err(_) -> "",
}
"#,
    );
    assert!(
        result.contains("u.toString()"),
        "expected `u.toString()`, got: {result}"
    );
}

// ── URLSearchParams Stdlib (#1426) ───────────────────────────

#[test]
fn stdlib_url_search_params_parse() {
    let result = emit(r#"URLSearchParams.parse("a=1&b=2")"#);
    assert!(
        result.contains("new URLSearchParams(\"a=1&b=2\")"),
        "expected `new URLSearchParams(...)`, got: {result}"
    );
}

#[test]
fn stdlib_url_search_params_get_coerces_null() {
    let result = emit_typed(
        r#"
let p = URLSearchParams.parse("a=1")
let v = p |> URLSearchParams.get("a")
"#,
    );
    assert!(
        result.contains("?? undefined"),
        "expected `?? undefined` null coercion, got: {result}"
    );
}

#[test]
fn stdlib_url_searchparams_from_url() {
    let result = emit_typed(
        r#"
let p = match URL.parse("http://x.com?a=1") {
    Ok(u) -> u |> URL.searchParams,
    Err(_) -> URLSearchParams.parse(""),
}
"#,
    );
    assert!(
        result.contains("u.searchParams"),
        "expected `u.searchParams` accessor, got: {result}"
    );
}

// ── RegExp Stdlib (#1426) ────────────────────────────────────

#[test]
fn stdlib_regexp_compile() {
    let result = emit(r#"RegExp.compile("^foo", "i")"#);
    assert!(
        result.contains("new RegExp(\"^foo\", \"i\")"),
        "expected `new RegExp(pattern, flags)`, got: {result}"
    );
    assert!(
        result.contains("try") && result.contains("catch"),
        "expected try/catch wrapper, got: {result}"
    );
}

#[test]
fn stdlib_regexp_test() {
    let result = emit_typed(
        r#"
let isMatch = match RegExp.compile("^[a-z]", "") {
    Ok(r) -> r |> RegExp.test("foo"),
    Err(_) -> false,
}
"#,
    );
    assert!(
        result.contains("r.test(\"foo\")"),
        "expected `r.test(\"foo\")`, got: {result}"
    );
}

#[test]
fn stdlib_regexp_exec_coerces_null() {
    let result = emit_typed(
        r#"
let captures = match RegExp.compile("(\\d+)", "") {
    Ok(r) -> r |> RegExp.exec("abc 123"),
    Err(_) -> None,
}
"#,
    );
    assert!(
        result.contains("?? undefined"),
        "expected `?? undefined` null coercion, got: {result}"
    );
    assert!(
        result.contains("r.exec(\"abc 123\")"),
        "expected `r.exec(...)`, got: {result}"
    );
}

// ── The file-global name maps are guarded on the resolved type (#1520) ──
//
// Codegen holds every unit variant, every variant constructor and every
// for-block function in a file-global map with no scope in it. The checker
// resolves a name with a scoped lookup. A local binding that shadows one of
// these names parted the two passes: the checker read the local, codegen
// read the map, and the program checked clean and emitted something else.
// Every emission below now reads `expr.ty` first.

#[test]
fn a_parameter_shadows_a_unit_variant() {
    let result = emit_typed(
        r#"
type Color = Red | Green
export let f(Red: number) -> number = { Red + 1 }
"#,
    );
    assert!(
        result.contains("return Red + 1;"),
        "the parameter shadows the variant, so the parameter must be emitted, got: {result}"
    );
    assert!(
        !result.contains("__tag: \"Red\" } + 1"),
        "the variant must not be emitted where the checker read the parameter, got: {result}"
    );
}

#[test]
fn a_unit_variant_the_checker_resolved_still_emits_its_tag() {
    let result = emit_typed(
        r#"
type Color = Red | Green
export let f() -> Color = { Red }
"#,
    );
    assert!(
        result.contains("{ __tag: \"Red\" }"),
        "an unshadowed unit variant must still emit its tag, got: {result}"
    );
}

#[test]
fn a_parameter_shadows_a_variant_constructor() {
    let result = emit_typed(
        r#"
type Route = | Home | Profile(string)
export let f(Profile: number) -> number = { Profile + 1 }
"#,
    );
    assert!(
        result.contains("return Profile + 1;"),
        "the parameter shadows the constructor, so the parameter must be emitted, got: {result}"
    );
    assert!(
        !result.contains("=> ({ __tag: \"Profile\""),
        "the constructor must not be emitted where the checker read the parameter, got: {result}"
    );
}

#[test]
fn a_variant_constructor_the_checker_resolved_still_emits_its_function() {
    let result = emit_typed(
        r#"
type Route = | Home | Profile(string)
export let f() -> (a: string) -> Route = { Profile }
"#,
    );
    assert!(
        result.contains("=> ({ __tag: \"Profile\""),
        "an unshadowed constructor must still emit its function, got: {result}"
    );
}

#[test]
fn a_parameter_shadows_a_for_block_function() {
    let result = emit_typed(
        r#"
type Entry = { n: number }
for Entry { export let double(self) -> number = { self.n * 2 } }
export let g(double: (a: number) -> number) -> number = { double(4) }
"#,
    );
    assert!(
        result.contains("return double(4);"),
        "the parameter shadows the for-block function, so the parameter must be called, got: {result}"
    );
    assert!(
        !result.contains("return Entry__double(4);"),
        "the for-block function must not be called where the checker read the parameter, got: {result}"
    );
}

#[test]
fn a_parameter_shadows_a_for_block_function_in_a_pipe() {
    let result = emit_typed(
        r#"
type Entry = { n: number }
for Entry { export let double(self) -> number = { self.n * 2 } }
export let g(double: (a: number) -> number) -> number = { 4 |> double() }
"#,
    );
    assert!(
        result.contains("return double(4);"),
        "the parameter shadows the for-block function, so the parameter must be called, got: {result}"
    );
    assert!(
        !result.contains("return Entry__double(4);"),
        "the for-block function must not be called where the checker read the parameter, got: {result}"
    );
}

#[test]
fn a_for_block_function_the_checker_resolved_still_uses_its_mangled_name() {
    let result = emit_typed(
        r#"
type Entry = { n: number }
for Entry { export let double(self) -> number = { self.n * 2 } }
export let g(e: Entry) -> number = { e |> double() }
"#,
    );
    assert!(
        result.contains("Entry__double(e)"),
        "an unshadowed for-block call must still use the mangled name, got: {result}"
    );
}

#[test]
fn a_for_block_on_a_stdlib_type_does_not_answer_for_a_stdlib_member() {
    // The checker reads `Array.sum` as the stdlib function, because a
    // for-block never enters the stdlib module namespace. Codegen read its
    // own for-block map and emitted a different function.
    //
    // The guard refuses the map here because the checker types an uncalled
    // stdlib member as that function's return type, so `expr.ty` is
    // `number` and no function can match it. That is not a signature
    // comparison, and it would not survive the checker giving the member
    // its own function type: `Array.sum` and this for-block declare the
    // same signature, so no reading of `expr.ty` separates them. What
    // codegen should emit for a stdlib function used as a bare value is
    // #1525, and this test does not fix the emission.
    let result = emit_typed(
        r#"
for Array<number> { export let sum(self) -> number = { 1 } }
export let f() -> number = { Array.sum }
"#,
    );
    assert!(
        !result.contains("return Array_number__sum"),
        "the for-block function must not be emitted where the checker read the stdlib, got: {result}"
    );
}

#[test]
fn two_for_blocks_sharing_a_method_name_each_get_their_own() {
    // The language supports one method name on two types, and the checker
    // picks between them by the receiver. Codegen kept only the first
    // registration under the bare name, so every call emitted `A__show`.
    let result = emit_typed(
        r#"
type A = { n: number }
type B = { s: string }
for A { export let show(self) -> string = { "a" } }
for B { export let show(self) -> string = { "b" } }
export let ga(x: A) -> string = { show(x) }
export let gb(x: B) -> string = { show(x) }
export let pa(x: A) -> string = { x |> show() }
export let pb(x: B) -> string = { x |> show() }
"#,
    );
    for (caller, callee) in [
        ("ga", "A__show(x)"),
        ("gb", "B__show(x)"),
        ("pa", "A__show(x)"),
        ("pb", "B__show(x)"),
    ] {
        assert!(
            result.contains(&format!("return {callee};")),
            "`{caller}` must call `{callee}`, got: {result}"
        );
    }
}

#[test]
fn a_parameter_shadows_a_for_block_function_that_takes_no_self() {
    let result = emit_typed(
        r#"
type Entry = { n: number }
for Entry { export let make(n: number) -> Entry = { Entry { n: n } } }
export let g(make: (a: number) -> number) -> number = { make(4) }
"#,
    );
    assert!(
        result.contains("return make(4);"),
        "the parameter shadows the for-block function, so the parameter must be called, got: {result}"
    );
    assert!(
        !result.contains("return Entry__make(4);"),
        "the for-block function must not be called where the checker read the parameter, got: {result}"
    );
}

#[test]
fn a_for_block_function_that_takes_no_self_still_uses_its_mangled_name() {
    let result = emit_typed(
        r#"
type Entry = { n: number }
for Entry { export let make(n: number) -> Entry = { Entry { n: n } } }
export let g() -> Entry = { make(4) }
"#,
    );
    assert!(
        result.contains("return Entry__make(4);"),
        "an unshadowed for-block call must still use the mangled name, got: {result}"
    );
}

#[test]
fn a_template_interpolation_does_not_retype_an_earlier_variant() {
    // Every expression inside `${...}` used to restart its id at 0, so the
    // checker's type map wrote the interpolation's types onto the file's
    // first expressions. `Red` reached codegen carrying `string`, and the
    // guard, reading that type, refused to emit the variant. See #1530.
    let result = emit_typed(
        r#"
type Color = Red | Green
export let f() -> Color = { Red }
export let name(s: string) -> string = { `hi ${s}` }
"#,
    );
    assert!(
        result.contains("return { __tag: \"Red\" };"),
        "the variant must keep its own type across a later interpolation, got: {result}"
    );
}

// ── A for-block on a foreign npm type (#1520) ────────────────────

/// Compile source against one synthetic `.d.ts`, then emit.
///
/// The guard compares the for-block header's written type name against the
/// name the checker's resolved receiver prints. A foreign type prints the
/// name the header wrote, with its type arguments encoded into it, so this
/// proves the two agree without a network or a tsgo probe.
fn emit_with_dts(dts_source: &str, specifier: &str, source: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("index.d.ts");
    std::fs::write(&path, dts_source).expect("write .d.ts");
    let exports = crate::interop::parse_dts_exports(&path).expect("parse .d.ts");
    let mut dts_imports = std::collections::HashMap::new();
    dts_imports.insert(specifier.to_string(), exports);
    let analysed = crate::analyse::analyse_module(
        source,
        crate::analyse::ModuleInputs {
            externs: crate::analyse::ExternTypes {
                dts_imports,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let errors: Vec<_> = analysed
        .diagnostics
        .iter()
        .filter(|d| d.severity == crate::diagnostic::Severity::Error)
        .map(|d| d.message.clone())
        .collect();
    assert!(errors.is_empty(), "fixture should check clean: {errors:?}");

    Codegen::new()
        .generate(&analysed.program)
        .code
        .trim()
        .to_string()
}

#[test]
fn a_for_block_on_a_foreign_npm_type_keeps_its_mangled_name() {
    let result = emit_with_dts(
        "export declare class Router { path: string }\n",
        "router",
        r#"
import trusted { Router } from "router"

for Router {
    export let describe(self) -> string = { "a router" }
}

export let g(r: Router) -> string = { r |> describe() }
"#,
    );
    assert!(
        result.contains("Router__describe(r)"),
        "a for-block on a foreign type must still use the mangled name, got: {result}"
    );
}

#[test]
fn a_parameter_shadows_a_for_block_function_on_a_foreign_npm_type() {
    let result = emit_with_dts(
        "export declare class Router { path: string }\n",
        "router",
        r#"
import trusted { Router } from "router"

for Router {
    export let describe(self) -> string = { "a router" }
}

export let g(describe: (a: number) -> string) -> string = { describe(4) }
"#,
    );
    assert!(
        result.contains("return describe(4);"),
        "the parameter shadows the for-block function, so the parameter must be called, got: {result}"
    );
    assert!(
        !result.contains("Router__describe(4)"),
        "the for-block function must not be called where the checker read the parameter, got: {result}"
    );
}

// ── Codegen must not apply a rule the checker never applies (glb #1522) ──

#[test]
fn receiver_style_emits_the_field_the_checker_resolved() {
    // The checker has no receiver-style rule. It resolves `x.month` to the
    // record field, so codegen must emit that call. It used to dispatch on
    // the receiver's type name and emit `(x.getMonth() + 1)` against a record
    // with no such method.
    let result = emit_typed(
        "type Date = { month: () -> number }\n\
         export let f(x: Date) -> number = { x.month() }",
    );
    assert!(
        result.contains("x.month()"),
        "expected the resolved field call `x.month()`, got: {result}"
    );
    assert!(
        !result.contains("getMonth"),
        "codegen must not route receiver style through the stdlib, got: {result}"
    );
}

#[test]
fn receiver_style_on_a_stdlib_type_keeps_every_argument() {
    // `Date` names a stdlib module, so this path dropped the argument and
    // emitted `d.getFullYear()`. The checker counts no arguments here either
    // (glb #1514), but the argument now reaches the output.
    let result = emit_typed("let d = Date.now()\nlet _y = d.year(1)");
    assert!(
        result.contains("d.year(1)"),
        "expected the plain member call `d.year(1)`, got: {result}"
    );
    assert!(
        !result.contains("getFullYear"),
        "codegen must not route receiver style through the stdlib, got: {result}"
    );
}

#[test]
fn unwrap_chain_step_keeps_a_promise_its_type_declares() {
    // `Http.get` emits an async IIFE and its type is `Promise<...>`, so the
    // const must bind the promise. The old string test saw `(async ` and
    // wrote `await`, inside a function codegen never marked `async`.
    let result = emit_with_types(
        "export let f(x: Result<string, Error>) -> Result<Promise<Result<Response, Error>>, Error> = {\n\
         \x20   let r = x? |> Http.get\n\
         \x20   Ok(r)\n\
         }",
    );
    assert!(
        result.contains("const _r1 = (async () => {"),
        "a `Promise` step must stay a promise, got: {result}"
    );
    assert!(
        !result.contains("_r1 = await"),
        "`f` is not async, so the step must not await, got: {result}"
    );
}

#[test]
fn unwrap_chain_awaits_an_async_collect_block() {
    // A `collect { ... }` block with an await inside emits an async IIFE and
    // keeps its `Result` type, so the type cannot report the promise. This is
    // the one step the temp must await.
    let result = emit_with_types(
        "export async let f(u: string) -> Result<string, Error> = {\n\
         \x20   let a = collect {\n\
         \x20       let b = Http.get(u) |> Promise.await? |> Http.text |> Promise.await?\n\
         \x20       b\n\
         \x20   }?\n\
         \x20   Ok(a)\n\
         }",
    );
    assert!(
        result.contains("= await (async () => {"),
        "an async collect block must be awaited before `.ok` is read, got: {result}"
    );
}

#[test]
fn unwrap_chain_awaits_a_parenthesised_async_collect_block() {
    // `emit_expr` reaches `emit_collect_block` through `Grouped` too, so a
    // pair of parentheses builds the same async IIFE. The step rule matched
    // `Collect` at the top of the step only, so the temp bound the promise,
    // `_r0.ok` read `undefined`, the guard always returned, and every later
    // step was skipped without a word (glb #1522).
    let result = emit_with_types(
        "export let shout(s: string) -> string = { `${s}!` }\n\
         export async let f(u: string) -> Result<string, Error> = {\n\
         \x20   let a = (collect {\n\
         \x20       let b = Http.get(u) |> Promise.await? |> Http.text |> Promise.await?\n\
         \x20       b\n\
         \x20   })? |> shout\n\
         \x20   Ok(a)\n\
         }",
    );
    assert!(
        result.contains("const _r0 = await ((async () => {"),
        "a parenthesised async collect block must be awaited before `.ok` is read, got: {result}"
    );
    assert!(
        result.contains("const _r1 = shout(_r0.value);"),
        "the step after the collect block must still run, got: {result}"
    );
}

// ── `await` inside an emitted wrapper (#1499) ────────────────────
//
// Codegen wraps several shapes in an arrow it calls at once. Whenever the
// wrapped code awaits, the arrow has to be `async` and the call site has to
// await it back. A plain arrow with an `await` in it is not TypeScript, and
// tsc reads the `await` as a name: `TS2311: Cannot find name 'await'`.

#[test]
fn piped_await_unwrap_emits_an_async_arrow() {
    // The shape from #1499: two `Promise.await?` steps in one tail
    // expression. Each `?` builds an arrow around an `await`.
    let result = emit_typed(
        r#"
type Post = { id: number, title: string }

export async let fetchPost(url: string) -> Result<Post, Error> = {
    Http.get(url) |> Promise.await? |> Http.json |> Promise.await? |> parse<Post>
}
"#,
    );
    assert!(
        result.contains("(await (async () => { const __r = "),
        "each unwrap wrapper around an await must be an awaited async arrow, got: {result}"
    );
    assert!(
        !result.contains("(() => { const __r = "),
        "no unwrap wrapper in this chain may stay a plain arrow, got: {result}"
    );
    assert_eq!(
        result.matches("(await (async () => { const __r = ").count(),
        2,
        "both unwrap steps must be marked, got: {result}"
    );
}

#[test]
fn unwrap_without_an_await_keeps_the_plain_arrow() {
    // The single-step chain with no await emits what it emitted before.
    let result = emit_typed(
        r#"
let validate(s: string) -> Result<string, string> = { Ok(s) }

export let run() -> Result<string, string> = {
    "hello" |> validate? |> validate
}
"#,
    );
    assert!(
        result.contains("(() => { const __r = "),
        "an unwrap with no await stays a plain arrow, got: {result}"
    );
    assert!(
        !result.contains("async"),
        "an unwrap with no await emits no async marker, got: {result}"
    );
}

#[test]
fn guarded_match_arm_awaiting_emits_an_async_arrow() {
    // The guard-arm wrapper holds the bindings, the guard and the body.
    let result = emit_typed(
        r#"
async let g() -> number = { 1 }

export async let pick(x: Option<number>) -> number = {
    match x {
        Some(n) when n > 0 -> n + (g() |> Promise.await),
        _ -> 0,
    }
}
"#,
    );
    assert!(
        result.contains("await (async () => {"),
        "a guard arm that awaits must emit an awaited async arrow, got: {result}"
    );
    assert!(
        !result.contains("? (() => {"),
        "the guard arm must not stay a plain arrow, got: {result}"
    );
}

#[test]
fn guarded_match_arm_awaiting_in_a_later_arm_emits_an_async_arrow() {
    // The wrapper copies every later arm into its own arrow, so an await
    // in a later arm decides the marker too.
    let result = emit_typed(
        r#"
async let g() -> number = { 1 }

export async let pick(x: Option<number>) -> number = {
    match x {
        Some(n) when n > 0 -> n,
        _ -> g() |> Promise.await,
    }
}
"#,
    );
    assert!(
        result.contains("await (async () => {"),
        "an await in a later arm must mark the guard arm's arrow, got: {result}"
    );
}

#[test]
fn string_pattern_match_arm_awaiting_emits_an_async_arrow() {
    let result = emit_typed(
        r#"
async let g() -> number = { 1 }

export async let route(s: string) -> number = {
    match s {
        "user/${id}" -> {
            let y = g() |> Promise.await
            y
        },
        _ -> 0,
    }
}
"#,
    );
    assert!(
        result.contains("await (async () => { const _m = "),
        "a string-pattern arm that awaits must emit an awaited async arrow, got: {result}"
    );
    assert!(
        !result.contains("(() => { const _m = "),
        "the string-pattern arm must not stay a plain arrow, got: {result}"
    );
}

#[test]
fn untrusted_call_with_an_awaiting_argument_emits_an_async_arrow() {
    // The try/catch wrapper evaluates the arguments inside its own arrow.
    // The call returns no promise, so the wrapper is awaited back and the
    // value stays the `Result` the checker typed.
    let result = emit_typed(
        r#"
import { someFn } from "untrusted-pkg"

async let g() -> number = { 1 }

export async let wrap() -> Result<number, Error> = {
    someFn(g() |> Promise.await)
}
"#,
    );
    assert!(
        result.contains("(await (async () => { try {"),
        "an awaiting argument must mark the untrusted-call wrapper, got: {result}"
    );
    assert!(
        !result.contains("value: await someFn"),
        "the call itself returns no promise, so its value must not be awaited, got: {result}"
    );
}

// ── Codegen reports what it cannot emit (#1493) ────────────────

/// Run the production pipeline and hand back the whole codegen result,
/// so a test can read the emitted code and the diagnostics together.
fn emit_with_diagnostics(input: &str) -> CodegenOutput {
    let mut program = Parser::new(input).parse_program().expect("parse");
    let (_diags, expr_types, invalid_exprs, shadowed) =
        crate::checker::Checker::new().check_full(&program);
    desugar::desugar_program(&mut program, &std::collections::HashMap::new());
    let typed = crate::checker::attach_types(program, &expr_types, &invalid_exprs, &shadowed);

    Codegen::new().generate(&typed)
}

/// How many E059 diagnostics the run reported.
fn count_unemittable(output: &CodegenOutput) -> usize {
    output
        .diagnostics
        .iter()
        .filter(|d| {
            d.code.as_deref()
                == Some(crate::checker::error_codes::ErrorCode::UnemittableExpression.code())
        })
        .count()
}

/// Codegen used to write the string `undefined /* type error */` into
/// the file for an expression the checker had rejected. A comment in
/// the output is not a diagnostic: nothing reads it, and the build
/// stayed green. Codegen now reports E059 instead.
#[test]
fn an_expression_codegen_cannot_emit_reports_a_diagnostic() {
    let output = emit_with_diagnostics("export let main() -> number = { bogusName(1) }");

    assert!(
        !output.code.contains("/* type error */"),
        "no marker may reach the output, got: {}",
        output.code
    );
    assert_eq!(
        count_unemittable(&output),
        1,
        "codegen must report E059 once, got: {:?}",
        output
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

/// One expression is one message. Three broken expressions must give
/// three, so a reader can find each one. The count is the point: a
/// dedup that collapsed them all would still satisfy an `any` check.
#[test]
fn three_expressions_codegen_cannot_emit_report_three_diagnostics() {
    let output = emit_with_diagnostics(
        "export let a() -> number = { bogusOne(1) }\n\
         export let b() -> number = { bogusTwo(2) }\n\
         export let c() -> number = { bogusThree(3) }\n",
    );

    assert_eq!(
        count_unemittable(&output),
        3,
        "each broken expression must report once, got: {:?}",
        output
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

/// The other half of the rule. Codegen emits a match scrutinee once per
/// arm, so one broken scrutinee reaches `ExprKind::Invalid` more than
/// once. It is still one expression, so it is one message.
#[test]
fn a_scrutinee_codegen_emits_twice_reports_one_diagnostic() {
    let output = emit_with_diagnostics(
        "type Color = Red | Green\n\
         export let pick() -> string = {\n\
           match bogusScrutinee(1) {\n\
             Red -> \"r\",\n\
             Green -> \"g\",\n\
           }\n\
         }\n",
    );

    assert!(
        output.code.matches("undefined.__tag").count() > 1,
        "this test is only meaningful while codegen emits the scrutinee more than once, got: {}",
        output.code
    );
    assert_eq!(
        count_unemittable(&output),
        1,
        "one expression is one message, however many times codegen emits it, got: {:?}",
        output
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}
