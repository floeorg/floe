use super::format;

fn assert_fmt(input: &str, expected: &str) {
    let result = format(input);
    assert_eq!(
        result.trim(),
        expected.trim(),
        "\n--- input ---\n{input}\n--- got ---\n{result}\n--- expected ---\n{expected}"
    );
}

// ── Literals & Declarations ─────────────────────────────────

#[test]
fn format_const() {
    assert_fmt("const   x   =   42", "const x = 42");
}

#[test]
fn format_const_typed() {
    assert_fmt("const x:number = 42", "const x: number = 42");
}

#[test]
fn format_function() {
    assert_fmt(
        "fn  add( a:number,b:number ) -> number{a+b}",
        "fn add(a: number, b: number) -> number {\n    a + b\n}",
    );
}

#[test]
fn format_import() {
    assert_fmt(
        r#"import {useState,useEffect} from "react""#,
        r#"import { useState, useEffect } from "react""#,
    );
}

#[test]
fn format_export() {
    assert_fmt(
        "export fn add(a:number,b:number) -> number{a+b}",
        "export fn add(a: number, b: number) -> number {\n    a + b\n}",
    );
}

// ── Types ───────────────────────────────────────────────────

#[test]
fn format_type_record() {
    assert_fmt(
        "type User {id:string,name:string}",
        "type User {\n    id: string,\n    name: string,\n}",
    );
}

#[test]
fn format_type_union() {
    assert_fmt(
        "type Route {|Home|Profile{id:string}|NotFound}",
        "type Route {\n    | Home\n    | Profile { id: string }\n    | NotFound\n}",
    );
}

#[test]
fn format_type_alias() {
    assert_fmt("type StringAlias = string", "type StringAlias = string");
}

// ── Expressions ─────────────────────────────────────────────

#[test]
fn format_match() {
    assert_fmt(
        "const x = match route {Home -> \"home\",NotFound -> \"404\"}",
        "const x = match route {\n    Home -> \"home\",\n    NotFound -> \"404\",\n}",
    );
}

#[test]
fn format_pipe() {
    assert_fmt(
        "const _r = data|>transform|>format",
        "const _r = data |> transform |> format",
    );
}

#[test]
fn format_arrow() {
    assert_fmt("const f = (x) => x + 1", "const f = (x) => x + 1");
}

#[test]
fn format_blank_lines_between_items() {
    assert_fmt("const x = 1\nconst y = 2", "const x = 1\n\nconst y = 2");
}

// ── JSX ─────────────────────────────────────────────────────

#[test]
fn format_jsx_self_closing() {
    assert_fmt("<Button />", "<Button />");
}

#[test]
fn format_jsx_self_closing_with_props() {
    assert_fmt(
        r#"<Button label="Save" onClick={handleSave} />"#,
        r#"<Button label="Save" onClick={handleSave} />"#,
    );
}

#[test]
fn format_jsx_with_expr_child() {
    assert_fmt("<div>{x}</div>", "<div>{x}</div>");
}

#[test]
fn format_jsx_with_nested_elements() {
    assert_fmt(
        "<div><h1>Title</h1><p>Body</p></div>",
        "<div>\n    <h1>Title</h1>\n    <p>Body</p>\n</div>",
    );
}

#[test]
fn format_jsx_fragment() {
    assert_fmt("<>{x}</>", "<>{x}</>");
}

// ── Blank line before final expression ──────────────────────

#[test]
fn format_blank_line_before_final_expr_in_multi_stmt_fn() {
    assert_fmt(
        "fn load(id: string) -> number {\n    const x = fetch(id)\n    const y = process(x)\n    x + y\n}",
        "fn load(id: string) -> number {\n    const x = fetch(id)\n    const y = process(x)\n\n    x + y\n}",
    );
}

#[test]
fn format_single_expr_fn_no_blank_line() {
    assert_fmt(
        "fn add(a: number, b: number) -> number { a + b }",
        "fn add(a: number, b: number) -> number {\n    a + b\n}",
    );
}

#[test]
fn format_already_has_blank_line_no_double() {
    // Even if the input doesn't have one, the formatter always produces
    // the canonical output with exactly one blank line before the last expr
    assert_fmt(
        "fn f() -> number {\n    const x = 1\n\n    x\n}",
        "fn f() -> number {\n    const x = 1\n\n    x\n}",
    );
}

#[test]
fn format_two_statement_block_gets_blank_line() {
    assert_fmt(
        "fn f() -> number {\n    const x = 1\n    x\n}",
        "fn f() -> number {\n    const x = 1\n\n    x\n}",
    );
}

#[test]
fn format_match_arm_block_body_blank_line() {
    assert_fmt(
        "const r = match x {\n    Some(v) -> {\n        const y = v + 1\n        y\n    },\n    None -> 0,\n}",
        "const r = match x {\n    Some(v) -> {\n        const y = v + 1\n\n        y\n    },\n    None -> 0,\n}",
    );
}

#[test]
fn format_lambda_block_body_blank_line() {
    assert_fmt(
        "const f = (x) => {\n    const y = x + 1\n    y\n}",
        "const f = (x) => {\n    const y = x + 1\n\n    y\n}",
    );
}

// ── Named arg punning ──────────────────────────────────────

#[test]
fn format_named_arg_punning() {
    assert_fmt("f(name: name, limit: 10)", "f(name:, limit: 10)");
}

#[test]
fn format_named_arg_no_pun_when_different() {
    assert_fmt("f(name: other)", "f(name: other)");
}

#[test]
fn format_named_arg_punning_already_punned() {
    assert_fmt("f(name:, limit:)", "f(name:, limit:)");
}

// ── Tuple types ────────────────────────────────────────

#[test]
fn format_tuple_type() {
    assert_fmt(
        "fn f() -> Result<(string, number), Error> {}",
        "fn f() -> Result<(string, number), Error> {}",
    );
}

#[test]
fn format_unit_type() {
    assert_fmt("fn f() -> () {}", "fn f() -> () {}");
}

// ── Tuple expressions ──────────────────────────────────

#[test]
fn format_tuple_expr() {
    assert_fmt("const x = (a, b)", "const x = (a, b)");
}

#[test]
fn format_tuple_expr_in_ok() {
    assert_fmt("Ok((product, reviews))", "Ok((product, reviews))");
}

// ── Tuple patterns ─────────────────────────────────────

#[test]
fn format_match_tuple_pattern() {
    assert_fmt(
        r#"const x = match point { (0, 0) -> "origin", (x, y) -> "other" }"#,
        "const x = match point {\n    (0, 0) -> \"origin\",\n    (x, y) -> \"other\",\n}",
    );
}

// ── Array patterns ─────────────────────────────────────

#[test]
fn format_match_array_pattern() {
    assert_fmt(
        r#"match items { [] -> "empty", [first, ..rest] -> first }"#,
        "match items {\n    [] -> \"empty\",\n    [first, ..rest] -> first,\n}",
    );
}

#[test]
fn format_match_array_pattern_with_wildcard_rest() {
    assert_fmt(
        r#"match items { [x, .._] -> x }"#,
        "match items {\n    [x, .._] -> x,\n}",
    );
}

// ── Subjectless (piped) match ──────────────────────────

#[test]
fn format_piped_match() {
    assert_fmt(
        r#"const x = value |> match { 1 -> "one", _ -> "other" }"#,
        "const x = value |> match {\n    1 -> \"one\",\n    _ -> \"other\",\n}",
    );
}

// ── Generic call expressions ───────────────────────────

#[test]
fn format_call_with_type_args() {
    assert_fmt("const x = Array<Todo>([])", "const x = Array<Todo>([])");
}

// ── Const tuple destructuring ──────────────────────────

#[test]
fn format_const_tuple_destructure() {
    assert_fmt("const (a, b) = getPoint()", "const (a, b) = getPoint()");
}

// ── Comments ───────────────────────────────────────────

#[test]
fn format_preserves_top_level_comments() {
    assert_fmt(
        "// section header\nconst x = 1",
        "// section header\n\nconst x = 1",
    );
}

#[test]
fn format_preserves_consecutive_comments() {
    assert_fmt(
        "// line 1\n// line 2\nconst x = 1",
        "// line 1\n// line 2\n\nconst x = 1",
    );
}

// ── Idempotency ────────────────────────────────────────

fn assert_idempotent(input: &str) {
    let first = format(input);
    let second = format(&first);
    assert_eq!(
        first, second,
        "\nFormatter is not idempotent!\n--- 1st ---\n{first}\n--- 2nd ---\n{second}"
    );
}

#[test]
fn idempotent_tuple_type_in_result() {
    assert_idempotent("fn f(id: Id) -> Result<(Product, Array<Review>), Error> { Ok((p, r)) }");
}

#[test]
fn idempotent_piped_match_with_tuple_patterns() {
    assert_idempotent(
        r#"const url = (cat, search) |> match { ("", "") -> "a", (c, "") -> "b", (_, q) -> "c" }"#,
    );
}

#[test]
fn idempotent_generic_call() {
    assert_idempotent("const [items, setItems] = Array<Todo>([])");
}

// ── Record spread ──────────────────────────────────────

#[test]
fn format_record_spread() {
    assert_fmt(
        "type A { x: number, ...B, y: string }",
        "type A {\n    x: number,\n    ...B,\n    y: string,\n}",
    );
}

#[test]
fn format_spread_in_construct() {
    assert_fmt(
        "const x = Todo(..t, done: true)",
        "const x = Todo(..t, done: true)",
    );
}

#[test]
fn format_jsx_keyword_prop() {
    assert_fmt(r#"<input type="text" />"#, r#"<input type="text" />"#);
}

#[test]
fn format_trailing_comments_between_items() {
    assert_fmt(
        "const x = 1\n// section\nconst y = 2",
        "const x = 1\n\n// section\n\nconst y = 2",
    );
}

// ── Line width wrapping ────────────────────────────────

#[test]
fn format_long_pipe_goes_vertical() {
    assert_fmt(
        "const data = await Http.get(`https://example.com/very/long/url/that/exceeds/width`)?|>Http.json?|>parse<Response>?",
        "const data = await Http.get(`https://example.com/very/long/url/that/exceeds/width`)?\n    |> Http.json?\n    |> parse<Response>?",
    );
}

#[test]
fn format_short_pipe_stays_inline() {
    assert_fmt(
        "const _r = data|>transform|>format",
        "const _r = data |> transform |> format",
    );
}

#[test]
fn format_long_fn_params_go_multiline() {
    assert_fmt(
        "fn fetchProducts(category: string = \"\", search: string = \"\", limit: number = 20, skip: number = 0) -> Result<number, Error> {}",
        "fn fetchProducts(\n    category: string = \"\",\n    search: string = \"\",\n    limit: number = 20,\n    skip: number = 0,\n) -> Result<number, Error> {}",
    );
}

#[test]
fn format_short_fn_params_stay_inline() {
    assert_fmt(
        "fn add(a: number, b: number) -> number { a + b }",
        "fn add(a: number, b: number) -> number {\n    a + b\n}",
    );
}

#[test]
fn format_long_call_args_go_multiline() {
    assert_fmt(
        "const p = Product(id: ProductId(data.id), title: data.title, description: data.description, category: data.category, price: data.price)",
        "const p = Product(\n    id: ProductId(data.id),\n    title: data.title,\n    description: data.description,\n    category: data.category,\n    price: data.price,\n)",
    );
}

#[test]
fn format_short_call_args_stay_inline() {
    assert_fmt("f(a, b, c)", "f(a, b, c)");
}

// ── Blank line preservation ────────────────────────────

#[test]
fn format_preserves_blank_lines_between_statements() {
    assert_fmt(
        "fn f() {\n    const a = 1\n\n    const b = 2\n\n    a + b\n}",
        "fn f() {\n    const a = 1\n\n    const b = 2\n\n    a + b\n}",
    );
}

#[test]
fn format_no_blank_line_when_source_has_none() {
    assert_fmt(
        "fn f() {\n    const a = 1\n    const b = 2\n\n    a + b\n}",
        "fn f() {\n    const a = 1\n    const b = 2\n\n    a + b\n}",
    );
}

#[test]
fn format_preserves_blank_line_after_match_block() {
    let src = "fn f() {\n    const url = x |> match {\n        1 -> \"a\",\n    }\n\n    const data = y\n\n    Ok(data)\n}";
    assert_fmt(src, src);
}

// ── Import trusted ─────────────────────────────────────────

#[test]
fn format_import_trusted_module() {
    assert_fmt(
        r#"import trusted {useState,Suspense} from "react""#,
        r#"import trusted { useState, Suspense } from "react""#,
    );
}

#[test]
fn format_import_trusted_specifier() {
    assert_fmt(
        r#"import {trusted capitalize,fetchUser} from "some-lib""#,
        r#"import { trusted capitalize, fetchUser } from "some-lib""#,
    );
}

#[test]
fn format_import_trusted_roundtrip() {
    let src = r#"import trusted { useState, useEffect } from "react""#;
    assert_fmt(src, src);
}

// ── Destructured params ────────────────────────────────────

#[test]
fn format_destructured_param() {
    assert_fmt(
        "fn greet({name,age}:User) {\n    name\n}",
        "fn greet({ name, age }: User) {\n    name\n}",
    );
}

#[test]
fn format_destructured_arrow_param() {
    assert_fmt(
        "const f = ({x,y}) => x + y",
        "const f = ({ x, y }) => x + y",
    );
}

#[test]
fn format_underscore_param() {
    assert_fmt(
        "fn f(_:number) -> number {\n    42\n}",
        "fn f(_: number) -> number {\n    42\n}",
    );
}

// ── Tuple index access ─────────────────────────────────

#[test]
fn format_tuple_index_access() {
    assert_fmt("const x = pair.0", "const x = pair.0");
}

#[test]
fn format_tuple_index_access_1() {
    assert_fmt("const x = pair.1", "const x = pair.1");
}
