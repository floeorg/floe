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
    assert_fmt(
        "type UserId = Brand<string,UserId>",
        "type UserId = Brand<string, UserId>",
    );
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
    assert_fmt("const f = fn(x) x + 1", "const f = fn(x) x + 1");
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
        "const f = fn(x) {\n    const y = x + 1\n    y\n}",
        "const f = fn(x) {\n    const y = x + 1\n\n    y\n}",
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
