use super::format;

fn assert_fmt(input: &str, expected: &str) {
    let result = format(input).expect("format should succeed (no parse errors)");
    assert_eq!(
        result.trim(),
        expected.trim(),
        "\n--- input ---\n{input}\n--- got ---\n{result}\n--- expected ---\n{expected}"
    );
}

// ── Literals & Declarations ─────────────────────────────────

#[test]
fn format_const() {
    assert_fmt("let   x   =   42", "let x = 42");
}

#[test]
fn format_const_typed() {
    assert_fmt("let x:number = 42", "let x: number = 42");
}

#[test]
fn format_function() {
    assert_fmt(
        "let add( a:number,b:number ) -> number = {a+b}",
        "let add(a: number, b: number) -> number = {\n    a + b\n}",
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
        "export let add(a:number,b:number) -> number = {a+b}",
        "export let add(a: number, b: number) -> number = {\n    a + b\n}",
    );
}

// ── Types ───────────────────────────────────────────────────

#[test]
fn format_type_record() {
    assert_fmt(
        "type User = {id:string,name:string}",
        "type User = {\n    id: string,\n    name: string,\n}",
    );
}

#[test]
fn format_short_union_stays_on_one_line() {
    assert_fmt(
        "type Route = |Home|Profile{id:string}|NotFound",
        "type Route = Home | Profile { id: string } | NotFound",
    );
}

#[test]
fn format_single_variant_newtype_stays_on_one_line() {
    assert_fmt(
        "export type SnippetCode =\n    | SnippetCode(string)",
        "export type SnippetCode = SnippetCode(string)",
    );
}

#[test]
fn format_enum_like_union_stays_on_one_line() {
    assert_fmt(
        "export type ExpiryOption = ONE_HOUR | ONE_DAY | ONE_WEEK",
        "export type ExpiryOption = ONE_HOUR | ONE_DAY | ONE_WEEK",
    );
}

#[test]
fn format_long_union_splits_to_one_variant_per_line() {
    // Over the 100-column threshold: every variant on its own `|` line.
    let input = "type Shape = Circle(number) | Rectangle(number, number) | \
                 Triangle(number, number, number) | Trapezoid(number, number, number, number)";
    let expected = "type Shape =\n    \
                    | Circle(number)\n    \
                    | Rectangle(number, number)\n    \
                    | Triangle(number, number, number)\n    \
                    | Trapezoid(number, number, number, number)";
    assert_fmt(input, expected);
}

#[test]
fn format_union_exactly_at_column_boundary_stays_inline() {
    // `type Roo = A | B | ... | W` is exactly 100 chars. At the boundary
    // the width check uses `<=`, so it should stay on one line.
    let input = "type Roo = A | B | C | D | E | F | G | H | I | J | K | L | M | \
                 N | O | P | Q | R | S | T | U | V | W";
    let expected = "type Roo = A | B | C | D | E | F | G | H | I | J | K | L | M | \
                    N | O | P | Q | R | S | T | U | V | W";
    assert_eq!(input.len(), 100);
    assert_fmt(input, expected);
}

#[test]
fn format_union_one_char_over_boundary_splits() {
    // Same variants, name padded by one character — now 101 chars total,
    // one over the budget, so the whole declaration splits.
    let input = "type Root = A | B | C | D | E | F | G | H | I | J | K | L | M | \
                 N | O | P | Q | R | S | T | U | V | W";
    assert_eq!(input.len(), 101);
    let expected = "type Root =\n    \
                    | A\n    | B\n    | C\n    | D\n    | E\n    | F\n    | G\n    \
                    | H\n    | I\n    | J\n    | K\n    | L\n    | M\n    | N\n    \
                    | O\n    | P\n    | Q\n    | R\n    | S\n    | T\n    | U\n    \
                    | V\n    | W";
    assert_fmt(input, expected);
}

#[test]
fn format_type_alias() {
    assert_fmt(
        "typealias StringAlias = string",
        "typealias StringAlias = string",
    );
}

#[test]
fn format_string_union_type() {
    assert_fmt(
        r#"export type ExpiryOption = "1h" | "1d" | "1w""#,
        r#"export type ExpiryOption = "1h" | "1d" | "1w""#,
    );
}

#[test]
fn format_string_union_type_normalizes_spacing() {
    assert_fmt(
        r#"type Status = "active"|"inactive"  |  "pending""#,
        r#"type Status = "active" | "inactive" | "pending""#,
    );
}

// ── Expressions ─────────────────────────────────────────────

#[test]
fn format_match() {
    assert_fmt(
        "let x = match route {Home -> \"home\",NotFound -> \"404\"}",
        "let x = match route {\n    Home -> \"home\",\n    NotFound -> \"404\",\n}",
    );
}

#[test]
fn format_pipe() {
    assert_fmt(
        "let _r = data|>transform|>format",
        "let _r = data |> transform |> format",
    );
}

#[test]
fn format_arrow() {
    assert_fmt("let f(x) = x + 1", "let f(x) = x + 1");
}

#[test]
fn format_blank_lines_between_items() {
    assert_fmt("let x = 1\nlet y = 2", "let x = 1\n\nlet y = 2");
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

#[test]
fn format_jsx_comment() {
    assert_fmt(
        "<div>{/* comment */}<span>hi</span></div>",
        "<div>\n    {/* comment */}\n    <span>hi</span>\n</div>",
    );
}

#[test]
fn format_jsx_comment_only_child() {
    assert_fmt("<div>{/* comment */}</div>", "<div>{/* comment */}</div>");
}

// ── Blank line before final expression ──────────────────────

#[test]
fn format_blank_line_before_final_expr_in_multi_stmt_fn() {
    assert_fmt(
        "let load(id: string) -> number = {\n    let x = fetch(id)\n    let y = process(x)\n    x + y\n}",
        "let load(id: string) -> number = {\n    let x = fetch(id)\n    let y = process(x)\n\n    x + y\n}",
    );
}

#[test]
fn format_single_expr_fn_no_blank_line() {
    assert_fmt(
        "let add(a: number, b: number) -> number = { a + b }",
        "let add(a: number, b: number) -> number = {\n    a + b\n}",
    );
}

#[test]
fn format_already_has_blank_line_no_double() {
    // Even if the input doesn't have one, the formatter always produces
    // the canonical output with exactly one blank line before the last expr
    assert_fmt(
        "let f() -> number = {\n    let x = 1\n\n    x\n}",
        "let f() -> number = {\n    let x = 1\n\n    x\n}",
    );
}

#[test]
fn format_two_statement_block_gets_blank_line() {
    assert_fmt(
        "let f() -> number = {\n    let x = 1\n    x\n}",
        "let f() -> number = {\n    let x = 1\n\n    x\n}",
    );
}

#[test]
fn format_match_arm_block_body_blank_line() {
    assert_fmt(
        "let r = match x {\n    Some(v) -> {\n        let y = v + 1\n        y\n    },\n    None -> 0,\n}",
        "let r = match x {\n    Some(v) -> {\n        let y = v + 1\n\n        y\n    },\n    None -> 0,\n}",
    );
}

#[test]
fn format_lambda_block_body_blank_line() {
    assert_fmt(
        "let f(x) = {\n    let y = x + 1\n    y\n}",
        "let f(x) = {\n    let y = x + 1\n\n    y\n}",
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
        "let f() -> Result<(string, number), Error> = {}",
        "let f() -> Result<(string, number), Error> = {}",
    );
}

#[test]
fn format_unit_type() {
    assert_fmt("let f() -> () = {}", "let f() -> () = {}");
}

// ── Tuple expressions ──────────────────────────────────

#[test]
fn format_tuple_expr() {
    assert_fmt("let x = (a, b)", "let x = (a, b)");
}

#[test]
fn format_tuple_expr_in_ok() {
    assert_fmt("Ok((product, reviews))", "Ok((product, reviews))");
}

// ── Tuple patterns ─────────────────────────────────────

#[test]
fn format_match_tuple_pattern() {
    assert_fmt(
        r#"let x = match point { (0, 0) -> "origin", (x, y) -> "other" }"#,
        "let x = match point {\n    (0, 0) -> \"origin\",\n    (x, y) -> \"other\",\n}",
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
        r#"let x = value |> match { 1 -> "one", _ -> "other" }"#,
        "let x = value |> match {\n    1 -> \"one\",\n    _ -> \"other\",\n}",
    );
}

// ── Generic call expressions ───────────────────────────

#[test]
fn format_call_with_type_args() {
    assert_fmt("let x = Array<Todo>([])", "let x = Array<Todo>([])");
}

// ── Const tuple destructuring ──────────────────────────

#[test]
fn format_const_tuple_destructure() {
    assert_fmt("let (a, b) = getPoint()", "let (a, b) = getPoint()");
}

// ── Comments ───────────────────────────────────────────

#[test]
fn format_preserves_top_level_comments() {
    assert_fmt(
        "// section header\nlet x = 1",
        "// section header\n\nlet x = 1",
    );
}

#[test]
fn format_preserves_consecutive_comments() {
    assert_fmt(
        "// line 1\n// line 2\nlet x = 1",
        "// line 1\n// line 2\n\nlet x = 1",
    );
}

// ── Comments in blocks ────────────────────────────────

#[test]
fn format_preserves_comment_in_block() {
    assert_fmt(
        "let f() = {\n    // hello\n    let x = 1\n\n    x\n}",
        "let f() = {\n    // hello\n    let x = 1\n\n    x\n}",
    );
}

#[test]
fn format_preserves_comment_between_statements() {
    assert_fmt(
        "let f() = {\n    let x = 1\n    // middle comment\n    let y = 2\n\n    x + y\n}",
        "let f() = {\n    let x = 1\n    // middle comment\n    let y = 2\n\n    x + y\n}",
    );
}

#[test]
fn format_preserves_comment_before_final_expr() {
    assert_fmt(
        "let f() = {\n    let x = 1\n    // result\n    x\n}",
        "let f() = {\n    let x = 1\n    // result\n\n    x\n}",
    );
}

#[test]
fn format_preserves_block_comment_in_block() {
    assert_fmt(
        "let f() = {\n    /* block comment */\n    let x = 1\n\n    x\n}",
        "let f() = {\n    /* block comment */\n    let x = 1\n\n    x\n}",
    );
}

// ── Comments inside parameter / arg / element lists (#1088) ─

#[test]
fn format_preserves_comment_between_call_args() {
    assert_fmt(
        "f(a, /* middle */ b)",
        "f(\n    a,\n    /* middle */\n    b,\n)",
    );
}

#[test]
fn format_preserves_line_comment_between_call_args() {
    assert_fmt(
        "f(\n    a,\n    // explain b\n    b,\n)",
        "f(\n    a,\n    // explain b\n    b,\n)",
    );
}

#[test]
fn format_preserves_comment_between_construct_args() {
    assert_fmt(
        "User(\n    name: \"a\",\n    // their age\n    age: 30,\n)",
        "User(\n    name: \"a\",\n    // their age\n    age: 30,\n)",
    );
}

#[test]
fn format_preserves_comment_between_array_elements() {
    assert_fmt(
        "let xs = [1, /* skip */ 2, 3]",
        "let xs = [\n    1,\n    /* skip */\n    2,\n    3,\n]",
    );
}

#[test]
fn format_preserves_comment_between_record_fields() {
    assert_fmt(
        "type User = {\n    id: string,\n    // a person's name\n    name: string,\n}",
        "type User = {\n    id: string,\n    // a person's name\n    name: string,\n}",
    );
}

#[test]
fn format_preserves_comment_between_function_params() {
    assert_fmt(
        "let add(\n    a: number,\n    // second operand\n    b: number,\n) -> number = {\n    a + b\n}",
        "let add(\n    a: number,\n    // second operand\n    b: number,\n) -> number = {\n    a + b\n}",
    );
}

#[test]
fn format_preserves_doc_comment_before_definition() {
    assert_fmt(
        "/// the global counter\nlet count = 0",
        "/// the global counter\n\nlet count = 0",
    );
}

#[test]
fn idempotent_comment_between_call_args() {
    assert_idempotent("f(a, /* middle */ b)");
}

#[test]
fn idempotent_comment_between_record_fields() {
    assert_idempotent("type User = {\n    id: string,\n    // name\n    name: string,\n}");
}

// ── Tagged template literals ───────────────────────────

#[test]
fn format_tagged_template_simple() {
    assert_fmt("let q = sql`select 1`", "let q = sql`select 1`");
}

#[test]
fn format_tagged_template_with_interpolation() {
    assert_fmt(
        "let q = sql`${col} + ${delta}`",
        "let q = sql`${col} + ${delta}`",
    );
}

#[test]
fn format_tagged_template_member_tag() {
    assert_fmt("let q = db.sql`select 1`", "let q = db.sql`select 1`");
}

// ── Idempotency ────────────────────────────────────────

fn assert_idempotent(input: &str) {
    let first = format(input).expect("first format should succeed");
    let second = format(&first).expect("second format should succeed");
    assert_eq!(
        first, second,
        "\nFormatter is not idempotent!\n--- 1st ---\n{first}\n--- 2nd ---\n{second}"
    );
}

#[test]
fn idempotent_tuple_type_in_result() {
    assert_idempotent("let f(id: Id) -> Result<(Product, Array<Review>), Error> = { Ok((p, r)) }");
}

#[test]
fn idempotent_piped_match_with_tuple_patterns() {
    assert_idempotent(
        r#"let url = (cat, search) |> match { ("", "") -> "a", (c, "") -> "b", (_, q) -> "c" }"#,
    );
}

#[test]
fn idempotent_generic_call() {
    assert_idempotent("let (items, setItems) = Array<Todo>([])");
}

// ── Record spread ──────────────────────────────────────

#[test]
fn format_record_spread() {
    assert_fmt(
        "type A = { x: number, ...B, y: string }",
        "type A = {\n    x: number,\n    ...B,\n    y: string,\n}",
    );
}

#[test]
fn format_spread_in_construct() {
    assert_fmt(
        "let x = Todo(done: true, ..t)",
        "let x = Todo(done: true, ..t)",
    );
}

#[test]
fn format_jsx_keyword_prop() {
    assert_fmt(r#"<input type="text" />"#, r#"<input type="text" />"#);
}

#[test]
fn format_jsx_hyphenated_prop() {
    assert_fmt(
        r#"<Input aria-label="Share link" data-testid="input" />"#,
        r#"<Input aria-label="Share link" data-testid="input" />"#,
    );
}

#[test]
fn format_trailing_comments_between_items() {
    assert_fmt(
        "let x = 1\n// section\nlet y = 2",
        "let x = 1\n\n// section\n\nlet y = 2",
    );
}

// ── Line width wrapping ────────────────────────────────

#[test]
fn format_long_pipe_goes_vertical() {
    assert_fmt(
        "let data = Http.get(`https://example.com/very/long/url/that/exceeds/width`)|>Promise.await?|>Http.json|>Promise.await?",
        "let data = Http.get(`https://example.com/very/long/url/that/exceeds/width`)\n    |> Promise.await?\n    |> Http.json\n    |> Promise.await?",
    );
}

#[test]
fn format_short_pipe_stays_inline() {
    assert_fmt(
        "let _r = data|>transform|>format",
        "let _r = data |> transform |> format",
    );
}

#[test]
fn format_long_fn_params_go_multiline() {
    assert_fmt(
        "let fetchProducts(category: string = \"\", search: string = \"\", limit: number = 20, skip: number = 0) -> Result<number, Error> = {}",
        "let fetchProducts(\n    category: string = \"\",\n    search: string = \"\",\n    limit: number = 20,\n    skip: number = 0,\n) -> Result<number, Error> = {}",
    );
}

#[test]
fn format_short_fn_params_stay_inline() {
    assert_fmt(
        "let add(a: number, b: number) -> number = { a + b }",
        "let add(a: number, b: number) -> number = {\n    a + b\n}",
    );
}

#[test]
fn format_long_call_args_go_multiline() {
    assert_fmt(
        "let p = Product(id: ProductId(data.id), title: data.title, description: data.description, category: data.category, price: data.price)",
        "let p = Product(\n    id: ProductId(data.id),\n    title: data.title,\n    description: data.description,\n    category: data.category,\n    price: data.price,\n)",
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
        "let f() = {\n    let a = 1\n\n    let b = 2\n\n    a + b\n}",
        "let f() = {\n    let a = 1\n\n    let b = 2\n\n    a + b\n}",
    );
}

#[test]
fn format_no_blank_line_when_source_has_none() {
    assert_fmt(
        "let f() = {\n    let a = 1\n    let b = 2\n\n    a + b\n}",
        "let f() = {\n    let a = 1\n    let b = 2\n\n    a + b\n}",
    );
}

#[test]
fn format_preserves_blank_line_after_match_block() {
    let src = "let f() = {\n    let url = x |> match {\n        1 -> \"a\",\n    }\n\n    let data = y\n\n    Ok(data)\n}";
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
        "let greet({name,age}:User) = {\n    name\n}",
        "let greet({ name, age }: User) = {\n    name\n}",
    );
}

#[test]
fn format_destructured_arrow_param() {
    assert_fmt("let f({x,y}) = x + y", "let f({ x, y }) = x + y");
}

#[test]
fn format_underscore_param() {
    assert_fmt(
        "let f(_:number) -> number = {\n    42\n}",
        "let f(_: number) -> number = {\n    42\n}",
    );
}

// ── Tuple index access ─────────────────────────────────

#[test]
fn format_tuple_index_access() {
    assert_fmt("let x = pair.0", "let x = pair.0");
}

#[test]
fn format_tuple_index_access_1() {
    assert_fmt("let x = pair.1", "let x = pair.1");
}

// ── JSX multi-line children ───────────────────────────────

#[test]
fn format_jsx_match_child_gets_own_lines() {
    assert_fmt(
        r#"<button>{match menuOpen { true -> <X size={24} />, false -> <Menu size={24} /> }}</button>"#,
        "<button>\n    {match menuOpen {\n        true -> <X size={24} />,\n        false -> <Menu size={24} />,\n    }}\n</button>",
    );
}

#[test]
fn format_jsx_sibling_expr_gets_newline() {
    assert_fmt(
        r#"<div><span>text</span>{match x { true -> "a", false -> "b" }}</div>"#,
        "<div>\n    <span>text</span>\n    {match x {\n        true -> \"a\",\n        false -> \"b\",\n    }}\n</div>",
    );
}

#[test]
fn format_jsx_multiline_tag_children_on_own_lines() {
    assert_fmt(
        "<Link to=\"/search\" className=\"text-2xl font-bold\" title=\"Home\" target=\"_blank\">京阪アクセント辞典</Link>",
        "<Link\n    to=\"/search\"\n    className=\"text-2xl font-bold\"\n    title=\"Home\"\n    target=\"_blank\"\n>\n    京阪アクセント辞典\n</Link>",
    );
}

#[test]
fn format_jsx_match_in_link_gets_own_lines() {
    assert_fmt(
        r#"<Link to="/login">{match session { Some(_) -> "account", None -> "login" }}</Link>"#,
        "<Link to=\"/login\">\n    {match session {\n        Some(_) -> \"account\",\n        None -> \"login\",\n    }}\n</Link>",
    );
}

#[test]
fn format_jsx_simple_expr_stays_inline() {
    // Simple (non-multiline) single expr child should stay inline
    assert_fmt("<span>{count}</span>", "<span>{count}</span>");
}

#[test]
fn idempotent_jsx_match_child() {
    assert_idempotent(
        r#"<button>{match menuOpen { true -> <X size={24} />, false -> <Menu size={24} /> }}</button>"#,
    );
}

#[test]
fn idempotent_jsx_multiline_tag_with_text() {
    assert_idempotent(
        "<Link to=\"/search\" className=\"text-2xl font-bold\" title=\"Home\" target=\"_blank\">京阪アクセント辞典</Link>",
    );
}

#[test]
fn format_match_arm_jsx_multiline_props_breaks_after_arrow() {
    assert_fmt(
        r#"match menuOpen { true -> <div className="absolute left-0 top-20 z-50 w-full border-b border-[var(--line)] bg-[var(--bg)] px-4 pb-4 sm:hidden"><ul>items</ul></div>, false -> <></> }"#,
        "match menuOpen {\n    true ->\n        <div\n            className=\"absolute left-0 top-20 z-50 w-full border-b border-[var(--line)] bg-[var(--bg)] px-4 pb-4 sm:hidden\"\n        >\n            <ul>items</ul>\n        </div>,\n    false -> <></>,\n}",
    );
}

#[test]
fn format_match_arm_jsx_short_props_stays_inline() {
    // JSX with few short props should stay on the same line as ->
    assert_fmt(
        r#"match x { true -> <X size={24} />, false -> <Y /> }"#,
        "match x {\n    true -> <X size={24} />,\n    false -> <Y />,\n}",
    );
}

#[test]
fn idempotent_match_arm_jsx_multiline_props() {
    assert_idempotent(
        r#"match menuOpen { true -> <div className="absolute left-0 top-20 z-50 w-full border-b border-[var(--line)] bg-[var(--bg)] px-4 pb-4 sm:hidden"><ul>items</ul></div>, false -> <></> }"#,
    );
}

// ── Array line breaking ──────────────────────────────────

#[test]
fn format_short_array_stays_inline() {
    assert_fmt("let x = [1, 2, 3]", "let x = [1, 2, 3]");
}

#[test]
fn format_long_array_goes_multiline() {
    assert_fmt(
        r#"let navItems: Array<NavItem> = [NavItem(to: "/", label: "Dashboard", icon: Grid), NavItem(to: "/board", label: "Board", icon: Columns), NavItem(to: "/backlog", label: "Backlog", icon: List)]"#,
        "let navItems: Array<NavItem> = [\n    NavItem(to: \"/\", label: \"Dashboard\", icon: Grid),\n    NavItem(to: \"/board\", label: \"Board\", icon: Columns),\n    NavItem(to: \"/backlog\", label: \"Backlog\", icon: List),\n]",
    );
}

#[test]
fn idempotent_long_array_with_constructors() {
    assert_idempotent(
        r#"let navItems: Array<NavItem> = [NavItem(to: "/", label: "Dashboard", icon: Grid), NavItem(to: "/board", label: "Board", icon: Columns), NavItem(to: "/backlog", label: "Backlog", icon: List)]"#,
    );
}

// ── JSX multiline arrow body in expression child ──────────

#[test]
fn format_jsx_arrow_with_multiline_jsx_body_breaks_after_arrow() {
    assert_fmt(
        r#"<nav className="flex-1 p-2">{items |> map((item) -> <NavLink key={item.to} to={item.to} className="flex items-center gap-3 px-3 py-2 rounded-md"><span>{item.icon}</span>{item.label}</NavLink>)}</nav>"#,
        r#"<nav className="flex-1 p-2">
    {items |> map((item) ->
        <NavLink
            key={item.to}
            to={item.to}
            className="flex items-center gap-3 px-3 py-2 rounded-md"
        >
            <span>{item.icon}</span>
            {item.label}
        </NavLink>
    )}
</nav>"#,
    );
}

#[test]
fn idempotent_jsx_arrow_with_multiline_jsx_body() {
    assert_idempotent(
        r#"<nav className="flex-1 p-2">{items |> map((item) -> <NavLink key={item.to} to={item.to} className="flex items-center gap-3 px-3 py-2 rounded-md"><span>{item.icon}</span>{item.label}</NavLink>)}</nav>"#,
    );
}

#[test]
fn format_jsx_arrow_simple_jsx_stays_inline() {
    // Arrow with inline JSX body should not break
    assert_fmt(
        r#"<div>{items |> map((x) -> <span>{x}</span>)}</div>"#,
        r#"<div>{items |> map((x) -> <span>{x}</span>)}</div>"#,
    );
}

// ── Member expressions with keyword names ──────────────────

#[test]
fn format_member_keyword_names() {
    assert_fmt("Date.from(x)", "Date.from(x)");
    assert_fmt("Number.parse(x)", "Number.parse(x)");
    assert_fmt("Router.match(path)", "Router.match(path)");
    assert_fmt("Array.for(items)", "Array.for(items)");
    assert_fmt("Schema.type(x)", "Schema.type(x)");
}

// ── Object destructuring ────────────────────────────────────

#[test]
fn format_object_destructure_preserves_rename() {
    assert_fmt(
        "let { data: issues, isLoading: loading } = hook()",
        "let { data: issues, isLoading: loading } = hook()",
    );
}

#[test]
fn format_object_destructure_without_rename() {
    assert_fmt(
        "let {  data ,  columns  } = hook()",
        "let { data, columns } = hook()",
    );
}

#[test]
fn format_object_destructure_mixed() {
    assert_fmt(
        "let { data: items, isLoading } = hook()",
        "let { data: items, isLoading } = hook()",
    );
}

// ── For Blocks ─────────────────────────────────────────────

#[test]
fn format_for_block_basic() {
    assert_fmt(
        "for User {\n  let display(self) -> string = {\n  `${self.name}`\n}\n}",
        "for User {\n    let display(self) -> string = {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_with_trait() {
    assert_fmt(
        "impl Display for User {\nlet display(self) -> string = {\n`${self.name}`\n}\n}",
        "impl Display for User {\n    let display(self) -> string = {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_with_export() {
    assert_fmt(
        "for User {\n  export let display(self) -> string = {\n  `${self.name}`\n}\n}",
        "for User {\n    export let display(self) -> string = {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_multiple_methods() {
    assert_fmt(
        "for User {\nlet name(self) -> string = { self.name }\nlet age(self) -> number = { self.age }\n}",
        "for User {\n    let name(self) -> string = {\n        self.name\n    }\n\n    let age(self) -> number = {\n        self.age\n    }\n}",
    );
}

#[test]
fn format_for_block_generic_type() {
    assert_fmt(
        "for Array<Todo> {\nexport let remaining(self) -> number = {\nself |> filter(.done == false) |> length\n}\n}",
        "for Array<Todo> {\n    export let remaining(self) -> number = {\n        self |> filter(.done == false) |> length\n    }\n}",
    );
}

#[test]
fn format_trait_decl_basic() {
    assert_fmt(
        "trait Display {\nlet display(self) -> string\n}",
        "trait Display {\n    let display(self) -> string\n}",
    );
}

#[test]
fn format_trait_decl_with_default_impl() {
    assert_fmt(
        "trait Eq {\nlet eq(self,other:Self) -> boolean\nlet neq(self,other:Self) -> boolean = {\n!(self |> eq(other))\n}\n}",
        "trait Eq {\n    let eq(self, other: Self) -> boolean\n\n    let neq(self, other: Self) -> boolean = {\n        !(self |> eq(other))\n    }\n}",
    );
}

#[test]
fn idempotent_for_block() {
    let formatted = "impl Display for User {\n    export let display(self) -> string = {\n        `${self.name}`\n    }\n}";
    assert_fmt(formatted, formatted);
}

#[test]
fn idempotent_trait_decl() {
    let formatted = "trait Display {\n    let display(self) -> string\n}";
    assert_fmt(formatted, formatted);
}

// ── Comment idempotence ────────────────────────────────

#[test]
fn idempotent_block_comment_between_record_fields() {
    assert_idempotent(
        "type User = {\n    id: string,\n    /* the person's name */\n    name: string,\n}",
    );
}

#[test]
fn idempotent_line_comment_between_array_elements() {
    assert_idempotent("let xs = [\n    1,\n    // skip\n    2,\n    3,\n]");
}

#[test]
fn idempotent_block_comment_between_array_elements() {
    assert_idempotent("let xs = [\n    1,\n    /* skip */\n    2,\n    3,\n]");
}

#[test]
fn idempotent_line_comment_between_tuple_elements() {
    assert_idempotent("let pair = (\n    1,\n    // second\n    2,\n)");
}

#[test]
fn idempotent_block_comment_inside_construct_args() {
    assert_idempotent("User(\n    name: \"a\",\n    /* their age */\n    age: 30,\n)");
}

#[test]
fn idempotent_trailing_comment_at_end_of_function() {
    assert_idempotent("let f() -> number = {\n    let x = 1\n\n    // return the answer\n    x\n}");
}

#[test]
fn idempotent_doc_comment_before_const() {
    assert_idempotent("/// the global counter\nlet count = 0");
}

#[test]
fn idempotent_doc_comment_before_function() {
    assert_idempotent(
        "/// adds two numbers\nlet add(a: number, b: number) -> number = {\n    a + b\n}",
    );
}

#[test]
fn idempotent_doc_comment_before_type() {
    assert_idempotent(
        "/// a registered user\ntype User = {\n    id: string,\n    name: string,\n}",
    );
}

#[test]
fn idempotent_module_doc_comment_at_top() {
    assert_idempotent("//// Todo domain module\n\nlet version = 1");
}

#[test]
fn idempotent_module_doc_then_doc_then_plain() {
    assert_idempotent("//// module header\n/// item doc\n// plain\nlet x = 1");
}

#[test]
fn idempotent_blank_lines_between_definitions() {
    assert_idempotent("let a = 1\n\nlet b = 2\n\nlet c = 3");
}

#[test]
fn idempotent_blank_lines_between_functions() {
    assert_idempotent("let one() -> number = {\n    1\n}\n\nlet two() -> number = {\n    2\n}");
}

#[test]
fn idempotent_mixed_comment_styles_before_imports() {
    assert_idempotent(
        r#"//// module header

// runtime deps
import { useState } from "react"
// dom helpers
import { render } from "react-dom""#,
    );
}

#[test]
fn idempotent_block_comment_between_call_args() {
    assert_idempotent("f(\n    a,\n    /* middle */\n    b,\n)");
}

#[test]
fn idempotent_line_comment_between_function_params() {
    assert_idempotent(
        "let add(\n    a: number,\n    // second operand\n    b: number,\n) -> number = {\n    a + b\n}",
    );
}

// ── Real-world fixture ────────────────────────────────

#[test]
fn idempotent_todo_app_todo_fl() {
    let src = include_str!("../../../../examples/todo-app/src/todo.fl");
    assert_idempotent(src);
}

// ── Single-expression closures keep their body (#1275) ──────

#[test]
fn format_zero_arg_closure_string_body() {
    assert_fmt(r#"let f = () -> "poop""#, r#"let f = () -> "poop""#);
}

#[test]
fn format_zero_arg_closure_with_alias_annotation() {
    assert_fmt(
        "typealias F = () -> string\nlet f: F = () -> \"poop\"",
        "typealias F = () -> string\n\nlet f: F = () -> \"poop\"",
    );
}

#[test]
fn format_one_arg_closure_single_expression_body() {
    assert_fmt("let f = (x) -> x + 1", "let f = (x) -> x + 1");
}

#[test]
fn format_zero_arg_closure_identifier_body() {
    assert_fmt("let f = () -> x", "let f = () -> x");
}

#[test]
fn format_zero_arg_closure_block_body() {
    assert_fmt(r#"let f = () -> { "x" }"#, "let f = () -> {\n    \"x\"\n}");
}

#[test]
fn idempotent_zero_arg_closure_with_alias() {
    assert_idempotent("typealias F = () -> string\n\nlet f: F = () -> \"poop\"\n");
}

#[test]
fn idempotent_one_arg_closure_single_expression() {
    assert_idempotent("let f = (x) -> x + 1\n");
}

// ── use declarations ────────────────────────────────────────

#[test]
fn use_decl_reformats_pipe_rhs() {
    // Input pipe is long enough to force a break; arms use three different
    // source indentations (7, 8, 1 spaces) to prove they are all rewritten.
    assert_fmt(
        concat!(
            "let f() = {\n",
            "    use body <-\n",
            "       c.req.json()\n",
            "        |> await\n",
            "        |> parse<CreateBody>\n",
            " |> Result.guard((_) -> c.json({ error: \"invalid body\" }, 400))\n",
            "    c.json(body, 200)\n",
            "}",
        ),
        concat!(
            "let f() = {\n",
            "    use body <- c.req.json()\n",
            "        |> await\n",
            "        |> parse<CreateBody>\n",
            "        |> Result.guard((_) -> c.json({ error: \"invalid body\" }, 400))\n",
            "\n",
            "    c.json(body, 200)\n",
            "}",
        ),
    );
}

#[test]
fn use_decl_single_ident_idempotent() {
    assert_idempotent("let f() = {\n    use x <- Option.guard(a, b)\n\n    x\n}\n");
}

#[test]
fn use_decl_paren_destructure() {
    assert_fmt(
        "let f() = {\n    use (a,b) <- pair\n    a\n}",
        "let f() = {\n    use (a, b) <- pair\n\n    a\n}",
    );
}

#[test]
fn use_decl_brace_destructure() {
    assert_fmt(
        "let f() = {\n    use {a,b} <- record\n    a\n}",
        "let f() = {\n    use { a, b } <- record\n\n    a\n}",
    );
}

#[test]
fn use_decl_brace_destructure_with_rename() {
    assert_fmt(
        "let f() = {\n    use {a:x,b} <- record\n    x\n}",
        "let f() = {\n    use { a: x, b } <- record\n\n    x\n}",
    );
}

#[test]
fn use_decl_no_binding() {
    assert_fmt(
        "let f() = {\n    use <- wrap()\n    done\n}",
        "let f() = {\n    use <- wrap()\n\n    done\n}",
    );
}
