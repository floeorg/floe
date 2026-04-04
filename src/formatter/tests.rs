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

// ── Comments in blocks ────────────────────────────────

#[test]
fn format_preserves_comment_in_block() {
    assert_fmt(
        "fn f() {\n    // hello\n    const x = 1\n\n    x\n}",
        "fn f() {\n    // hello\n    const x = 1\n\n    x\n}",
    );
}

#[test]
fn format_preserves_comment_between_statements() {
    assert_fmt(
        "fn f() {\n    const x = 1\n    // middle comment\n    const y = 2\n\n    x + y\n}",
        "fn f() {\n    const x = 1\n    // middle comment\n    const y = 2\n\n    x + y\n}",
    );
}

#[test]
fn format_preserves_comment_before_final_expr() {
    assert_fmt(
        "fn f() {\n    const x = 1\n    // result\n    x\n}",
        "fn f() {\n    const x = 1\n    // result\n\n    x\n}",
    );
}

#[test]
fn format_preserves_block_comment_in_block() {
    assert_fmt(
        "fn f() {\n    /* block comment */\n    const x = 1\n\n    x\n}",
        "fn f() {\n    /* block comment */\n    const x = 1\n\n    x\n}",
    );
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
fn format_jsx_hyphenated_prop() {
    assert_fmt(
        r#"<Input aria-label="Share link" data-testid="input" />"#,
        r#"<Input aria-label="Share link" data-testid="input" />"#,
    );
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
        "const data = Http.get(`https://example.com/very/long/url/that/exceeds/width`)|>Promise.await?|>Http.json|>Promise.await?",
        "const data = Http.get(`https://example.com/very/long/url/that/exceeds/width`)\n    |> Promise.await?\n    |> Http.json\n    |> Promise.await?",
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
    assert_fmt("const x = [1, 2, 3]", "const x = [1, 2, 3]");
}

#[test]
fn format_long_array_goes_multiline() {
    assert_fmt(
        r#"const navItems: Array<NavItem> = [NavItem(to: "/", label: "Dashboard", icon: Grid), NavItem(to: "/board", label: "Board", icon: Columns), NavItem(to: "/backlog", label: "Backlog", icon: List)]"#,
        "const navItems: Array<NavItem> = [\n    NavItem(to: \"/\", label: \"Dashboard\", icon: Grid),\n    NavItem(to: \"/board\", label: \"Board\", icon: Columns),\n    NavItem(to: \"/backlog\", label: \"Backlog\", icon: List),\n]",
    );
}

#[test]
fn idempotent_long_array_with_constructors() {
    assert_idempotent(
        r#"const navItems: Array<NavItem> = [NavItem(to: "/", label: "Dashboard", icon: Grid), NavItem(to: "/board", label: "Board", icon: Columns), NavItem(to: "/backlog", label: "Backlog", icon: List)]"#,
    );
}

// ── JSX multiline arrow body in expression child ──────────

#[test]
fn format_jsx_arrow_with_multiline_jsx_body_breaks_after_arrow() {
    assert_fmt(
        r#"<nav className="flex-1 p-2">{items |> map((item) => <NavLink key={item.to} to={item.to} className="flex items-center gap-3 px-3 py-2 rounded-md"><span>{item.icon}</span>{item.label}</NavLink>)}</nav>"#,
        r#"<nav className="flex-1 p-2">
    {items |> map((item) =>
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
        r#"<nav className="flex-1 p-2">{items |> map((item) => <NavLink key={item.to} to={item.to} className="flex items-center gap-3 px-3 py-2 rounded-md"><span>{item.icon}</span>{item.label}</NavLink>)}</nav>"#,
    );
}

#[test]
fn format_jsx_arrow_simple_jsx_stays_inline() {
    // Arrow with inline JSX body should not break
    assert_fmt(
        r#"<div>{items |> map((x) => <span>{x}</span>)}</div>"#,
        r#"<div>{items |> map((x) => <span>{x}</span>)}</div>"#,
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
        "const { data: issues, isLoading: loading } = hook()",
        "const { data: issues, isLoading: loading } = hook()",
    );
}

#[test]
fn format_object_destructure_without_rename() {
    assert_fmt(
        "const {  data ,  columns  } = hook()",
        "const { data, columns } = hook()",
    );
}

#[test]
fn format_object_destructure_mixed() {
    assert_fmt(
        "const { data: items, isLoading } = hook()",
        "const { data: items, isLoading } = hook()",
    );
}

// ── For Blocks ─────────────────────────────────────────────

#[test]
fn format_for_block_basic() {
    assert_fmt(
        "for User {\n  fn display(self) -> string {\n  `${self.name}`\n}\n}",
        "for User {\n    fn display(self) -> string {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_with_trait() {
    assert_fmt(
        "for  User :  Display  {\nfn display(self) -> string {\n`${self.name}`\n}\n}",
        "for User: Display {\n    fn display(self) -> string {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_with_export() {
    assert_fmt(
        "for User {\n  export fn display(self) -> string {\n  `${self.name}`\n}\n}",
        "for User {\n    export fn display(self) -> string {\n        `${self.name}`\n    }\n}",
    );
}

#[test]
fn format_for_block_multiple_methods() {
    assert_fmt(
        "for User {\nfn name(self) -> string { self.name }\nfn age(self) -> number { self.age }\n}",
        "for User {\n    fn name(self) -> string {\n        self.name\n    }\n\n    fn age(self) -> number {\n        self.age\n    }\n}",
    );
}

#[test]
fn format_for_block_generic_type() {
    assert_fmt(
        "for Array<Todo> {\nexport fn remaining(self) -> number {\nself |> filter(.done == false) |> length\n}\n}",
        "for Array<Todo> {\n    export fn remaining(self) -> number {\n        self |> filter(.done == false) |> length\n    }\n}",
    );
}

#[test]
fn format_trait_decl_basic() {
    assert_fmt(
        "trait Display {\nfn display(self) -> string\n}",
        "trait Display {\n    fn display(self) -> string\n}",
    );
}

#[test]
fn format_trait_decl_with_default_impl() {
    assert_fmt(
        "trait Eq {\nfn eq(self,other:Self) -> boolean\nfn neq(self,other:Self) -> boolean {\n!(self |> eq(other))\n}\n}",
        "trait Eq {\n    fn eq(self, other: Self) -> boolean\n\n    fn neq(self, other: Self) -> boolean {\n        !(self |> eq(other))\n    }\n}",
    );
}

#[test]
fn idempotent_for_block() {
    let formatted = "for User: Display {\n    export fn display(self) -> string {\n        `${self.name}`\n    }\n}";
    assert_fmt(formatted, formatted);
}

#[test]
fn idempotent_trait_decl() {
    let formatted = "trait Display {\n    fn display(self) -> string\n}";
    assert_fmt(formatted, formatted);
}
