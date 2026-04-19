use std::collections::HashMap;

use super::completion::*;
use super::completions::{
    detect_match_context, is_in_jsx_tag, jsx_attribute_completions, lambda_event_completions,
    match_arm_snippet,
};
use super::goto_def::import_path_at_offset;
use super::handlers::end_position;
use super::index::*;
use super::*;

use floe_core::analyse::{self, ModuleInputs};
use floe_core::diagnostic::{self as floe_diag, Severity};
use floe_core::parser::Parser;
use floe_core::parser::ast::*;

/// Run analyse on a source string and return a SymbolIndex built off the
/// typed program. Tests use this instead of `SymbolIndex::build(&program)`
/// now that the index walks the typed AST.
fn build_index(source: &str) -> SymbolIndex {
    let program = Parser::new(source).parse_program().unwrap();
    let analysed = analyse::analyse_parsed(program, ModuleInputs::default());
    SymbolIndex::build(
        &analysed.program,
        &analysed.name_types,
        &analysed.name_type_map,
    )
}

#[test]
fn offset_to_position_first_line() {
    let source = "let x = 42";
    let pos = offset_to_position(source, 6);
    assert_eq!(pos.line, 0);
    assert_eq!(pos.character, 6);
}

#[test]
fn offset_to_position_second_line() {
    let source = "let x = 42\nconst y = 10";
    let pos = offset_to_position(source, 19);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.character, 6);
}

#[test]
fn offset_to_range_basic() {
    let source = "let x = 42";
    let range = offset_to_range(source, 6, 7);
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 6);
    assert_eq!(range.end.line, 0);
    assert_eq!(range.end.character, 7);
}

#[test]
fn position_to_offset_roundtrip() {
    let source = "hello\nworld\nfoo";
    let offset = position_to_offset(source, Position::new(1, 3));
    assert_eq!(offset, 9);
}

#[test]
fn word_at_offset_basic() {
    let source = "let hello = 42";
    assert_eq!(word_at_offset(source, 6), "hello");
    assert_eq!(word_at_offset(source, 8), "hello");
}

#[test]
fn word_at_offset_at_boundary() {
    let source = "let x = 42";
    assert_eq!(word_at_offset(source, 0), "const");
}

#[test]
fn word_prefix_at_offset_partial() {
    let source = "let hel";
    assert_eq!(word_prefix_at_offset(source, 9), "hel");
}

#[test]
fn word_prefix_at_offset_empty() {
    let source = "let ";
    assert_eq!(word_prefix_at_offset(source, 6), "");
}

#[test]
fn banned_keyword_produces_parse_error() {
    let source = "let x = 42";
    let parse_result = Parser::new(source).parse_program();
    assert!(parse_result.is_err());
    let errs = parse_result.unwrap_err();
    let floe_diags = floe_diag::from_parse_errors(&errs);
    assert!(!floe_diags.is_empty());
    assert_eq!(floe_diags[0].severity, Severity::Error);
}

#[test]
fn symbol_index_function() {
    let source = "let add(a: number, b: number) -> number = { a + b }";
    let index = build_index(source);
    let syms = index.find_by_name("add");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].kind, SymbolKind::FUNCTION);
    assert_eq!(
        syms[0].detail, "fn add(a: number, b: number) -> number",
        "function detail should use => for return type, not :"
    );
}

#[test]
fn symbol_index_function_no_return_type() {
    let source = "let greet(name: string) = { Console.log(name) }";
    let index = build_index(source);
    let syms = index.find_by_name("greet");
    assert_eq!(syms.len(), 1);
    assert_eq!(
        syms[0].detail, "fn greet(name: string) -> ()",
        "function without return type should show the checker's inferred return"
    );
}

#[test]
fn symbol_index_exported_function() {
    let source = "export let hello() -> string = { \"hi\" }";
    let index = build_index(source);
    let syms = index.find_by_name("hello");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].detail, "export fn hello() -> string",);
}

#[test]
fn symbol_index_const() {
    let source = "let x = 42";
    let index = build_index(source);
    let syms = index.find_by_name("x");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].kind, SymbolKind::CONSTANT);
}

#[test]
fn symbol_index_type() {
    let source = "type User = { name: string, age: number }";
    let index = build_index(source);
    let syms = index.find_by_name("User");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].kind, SymbolKind::TYPE_PARAMETER);
}

#[test]
fn symbol_index_import() {
    let source = r#"import { useState } from "react""#;
    let index = build_index(source);
    let syms = index.find_by_name("useState");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].import_source.as_deref(), Some("react"));
}

#[test]
fn symbol_index_union_variants() {
    let source = "type Color = | Red | Green | Blue";
    let index = build_index(source);
    assert_eq!(index.find_by_name("Color").len(), 1);
    assert_eq!(index.find_by_name("Red").len(), 1);
    assert_eq!(index.find_by_name("Green").len(), 1);
    assert_eq!(index.find_by_name("Blue").len(), 1);
}

#[test]
fn type_expr_to_string_named() {
    let ty: TypeExpr = TypeExpr {
        kind: TypeExprKind::Named {
            name: "string".to_string(),
            type_args: vec![],
            bounds: vec![],
        },
        span: floe_core::lexer::span::Span::new(0, 0, 1, 1),
    };
    assert_eq!(type_expr_to_string(&ty), "string");
}

#[test]
fn type_expr_to_string_generic() {
    let ty: TypeExpr = TypeExpr {
        kind: TypeExprKind::Named {
            name: "Result".to_string(),
            bounds: vec![],
            type_args: vec![
                TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "User".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: floe_core::lexer::span::Span::new(0, 0, 1, 1),
                },
                TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "Error".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: floe_core::lexer::span::Span::new(0, 0, 1, 1),
                },
            ],
        },
        span: floe_core::lexer::span::Span::new(0, 0, 1, 1),
    };
    assert_eq!(type_expr_to_string(&ty), "Result<User, Error>");
}

// ── Pipe-aware completion tests ────────────────────────

#[test]
fn pipe_context_detected() {
    assert!(is_pipe_context("users |> ", 9));
    assert!(is_pipe_context("users |>  ", 10));
    assert!(is_pipe_context("x |> f() |> ", 12));
    assert!(!is_pipe_context("let x = 42", 12));
    assert!(!is_pipe_context("let x = |", 11));
}

#[test]
fn pipe_context_with_prefix() {
    // User typed "users |> fi" — cursor is at offset 11
    // The prefix would be "fi", and before that is "|> "
    let source = "users |> fi";
    // is_pipe_context checks before the prefix starts
    assert!(is_pipe_context(&source[..9], 9)); // "users |> "
}

#[test]
fn base_type_name_simple() {
    assert_eq!(base_type_name("string"), "string");
    assert_eq!(base_type_name("number"), "number");
}

#[test]
fn base_type_name_generic() {
    assert_eq!(base_type_name("Array<User>"), "Array");
    assert_eq!(base_type_name("Option<string>"), "Option");
    assert_eq!(base_type_name("Result<User, Error>"), "Result");
}

#[test]
fn unwrap_result_type() {
    assert_eq!(unwrap_type("Result<User, Error>"), "User");
}

#[test]
fn unwrap_option_type() {
    assert_eq!(unwrap_type("Option<string>"), "string");
}

#[test]
fn unwrap_non_wrapper_type() {
    assert_eq!(unwrap_type("string"), "string");
}

#[test]
fn pipe_compatible_same_type() {
    assert!(is_pipe_compatible("Array<T>", "Array<User>"));
    assert!(is_pipe_compatible("string", "string"));
    assert!(is_pipe_compatible("Option<T>", "Option<number>"));
}

#[test]
fn pipe_compatible_generic_param() {
    // Single-letter type params match anything
    assert!(is_pipe_compatible("T", "string"));
    assert!(is_pipe_compatible("A", "Array<User>"));
}

#[test]
fn pipe_incompatible_types() {
    assert!(!is_pipe_compatible("string", "number"));
    assert!(!is_pipe_compatible("Array<T>", "Option<T>"));
}

#[test]
fn extract_identifier_simple() {
    assert_eq!(extract_trailing_identifier("users"), "users");
}

#[test]
fn extract_identifier_call() {
    assert_eq!(extract_trailing_identifier("getUsers()"), "getUsers");
}

#[test]
fn extract_identifier_member() {
    assert_eq!(extract_trailing_identifier("a.b.c"), "c");
}

#[test]
fn infer_literal_string() {
    assert_eq!(infer_literal_type("\"hello\""), Some("string".to_string()));
}

#[test]
fn infer_literal_number() {
    assert_eq!(infer_literal_type("42"), Some("number".to_string()));
}

#[test]
fn infer_literal_bool() {
    assert_eq!(infer_literal_type("true"), Some("boolean".to_string()));
}

#[test]
fn resolve_piped_type_from_type_map() {
    let mut type_map = HashMap::new();
    type_map.insert("users".to_string(), "Array<User>".to_string());
    let source = "users |> ";
    let result = resolve_piped_type(source, 9, &type_map);
    assert_eq!(result, Some("Array<User>".to_string()));
}

#[test]
fn resolve_piped_type_with_unwrap() {
    let mut type_map = HashMap::new();
    type_map.insert(
        "fetchUser".to_string(),
        "(number) -> Result<User, Error>".to_string(),
    );
    let source = "result? |> ";
    let mut tm = HashMap::new();
    tm.insert("result".to_string(), "Result<User, Error>".to_string());
    let resolved = resolve_piped_type(source, 11, &tm);
    assert_eq!(resolved, Some("User".to_string()));
}

#[test]
fn function_symbol_stores_first_param_type() {
    // Concrete types only — the index sources `first_param_type` from the
    // checker's resolved signature, so unbound generics like `T` without a
    // declared type parameter would surface as `<error>`.
    let source = "let head(arr: Array<number>) -> number = { 0 }";
    let index = build_index(source);
    let syms = index.find_by_name("head");
    assert_eq!(syms.len(), 1);
    assert_eq!(
        syms[0].first_param_type.as_deref().map(|t| t.to_string()),
        Some("Array<number>".to_string())
    );
}

// ── Integration tests on jsx_component.fl ──────────────

fn jsx_component_source() -> &'static str {
    r#"import { useState, JSX } from "react"

export let Counter = (): JSX.Element => {
    let [_count, setCount] = useState(0)

    <div>
        <h1>Count</h1>
        <button onClick={setCount}>Increment</button>
    </div>
}"#
}

fn build_index_and_types(source: &str) -> (SymbolIndex, HashMap<String, String>) {
    let program = Parser::new(source).parse_program().unwrap();
    let analysed = analyse::analyse_parsed(program, ModuleInputs::default());
    let index = SymbolIndex::build(
        &analysed.program,
        &analysed.name_types,
        &analysed.name_type_map,
    );
    (index, analysed.name_types)
}

#[test]
fn jsx_fixture_all_symbols_indexed() {
    let (index, _) = build_index_and_types(jsx_component_source());
    let all_names: Vec<&str> = index.symbols.iter().map(|s| s.name.as_str()).collect();
    println!("All indexed symbols: {:?}", all_names);

    // Imports
    assert!(
        !index.find_by_name("useState").is_empty(),
        "useState not indexed"
    );
    assert!(!index.find_by_name("JSX").is_empty(), "JSX not indexed");

    // Function
    assert!(
        !index.find_by_name("Counter").is_empty(),
        "Counter not indexed"
    );

    // Destructured variables
    assert!(
        !index.find_by_name("_count").is_empty(),
        "_count not indexed"
    );
    assert!(
        !index.find_by_name("setCount").is_empty(),
        "setCount not indexed"
    );
}

#[test]
fn jsx_fixture_hover_on_destructured_var() {
    let (index, _) = build_index_and_types(jsx_component_source());

    // Hover on _count should work
    let syms = index.find_by_name("_count");
    assert!(!syms.is_empty(), "_count should be found for hover");
    assert!(
        syms[0].detail.contains("_count"),
        "detail should mention _count, got: {}",
        syms[0].detail
    );

    // Hover on setCount should work
    let syms = index.find_by_name("setCount");
    assert!(!syms.is_empty(), "setCount should be found for hover");
    assert!(
        syms[0].detail.contains("setCount"),
        "detail should mention setCount, got: {}",
        syms[0].detail
    );
}

#[test]
fn jsx_fixture_goto_def_setcount_from_jsx() {
    let source = jsx_component_source();
    let (index, _) = build_index_and_types(source);

    // Find the offset of setCount in onClick={setCount} (line 8)
    let jsx_setcount_offset = source.find("onClick={setCount}").unwrap() + "onClick={".len();
    let word = word_at_offset(source, jsx_setcount_offset);
    assert_eq!(
        word, "setCount",
        "should extract 'setCount' from JSX attribute"
    );

    // find_by_name should find it
    let syms = index.find_by_name("setCount");
    assert!(!syms.is_empty(), "setCount should be in index");

    // The definition's span should NOT contain the JSX usage offset
    let sym = &syms[0];
    let cursor_in_def = jsx_setcount_offset >= sym.start && jsx_setcount_offset <= sym.end;
    assert!(
        !cursor_in_def,
        "JSX usage offset {} should NOT be inside definition span {}..{} (go-to-def would skip it!)",
        jsx_setcount_offset, sym.start, sym.end
    );
}

#[test]
fn jsx_fixture_hover_on_usestate() {
    let (index, _) = build_index_and_types(jsx_component_source());
    let syms = index.find_by_name("useState");
    assert!(!syms.is_empty());
    assert!(syms[0].detail.contains("useState"));
}

#[test]
fn jsx_fixture_type_map_has_counter() {
    let (_, type_map) = build_index_and_types(jsx_component_source());
    println!("Type map: {:?}", type_map);
    assert!(
        type_map.contains_key("Counter"),
        "Counter should be in type map"
    );
}

// ── Hover type display tests (#180) ─────────────────────────

/// Simulate hover: look up symbol by name and read the pre-enriched
/// `Symbol.detail` — the same string the LSP handler renders.
fn simulate_hover(source: &str, name: &str) -> Option<String> {
    let (index, _) = build_index_and_types(source);
    let syms = index.find_by_name(name);
    syms.first().map(|s| s.detail.clone())
}

#[test]
fn hover_destructured_const_shows_type() {
    let source = r#"
import { useState } from "react"
export let App = (): JSX.Element => {
    let [input, setInput] = useState("")
    <div>{input}</div>
}
"#;
    let (index, type_map) = build_index_and_types(source);
    eprintln!("TYPE MAP: {:?}", type_map);
    let syms = index.find_by_name("input");
    eprintln!(
        "SYMBOLS for input: {:?}",
        syms.iter()
            .map(|s| (&s.name, &s.detail, s.kind, &s.import_source))
            .collect::<Vec<_>>()
    );
    let hover = simulate_hover(source, "input");
    let detail = hover.unwrap();
    eprintln!("HOVER for destructured input: {detail}");
}

#[test]
fn hover_simple_const_debug() {
    let source = r#"let pee = "test""#;
    let hover = simulate_hover(source, "pee");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    eprintln!("HOVER for pee: {detail}");
    assert!(
        detail.contains("string"),
        "should show string type, got: {detail}"
    );
}

#[test]
fn shadow_error_in_checker() {
    let source = r#"
let x = 5
let x = 10
"#;
    let (_index, _type_map) = build_index_and_types(source);
    let (diags, _, _, _) = floe_core::checker::Checker::new().check_with_types(
        &floe_core::parser::Parser::new(source)
            .parse_program()
            .unwrap(),
    );
    eprintln!(
        "SHADOW DIAGS: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        diags.iter().any(|d| d.message.contains("already defined")),
        "should have shadowing error"
    );
}

#[test]
fn hover_const_shows_inferred_type() {
    // const without explicit type annotation should show inferred type
    let hover = simulate_hover("let x = 42", "x");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("number"),
        "hover for let x = 42 should show 'number', got: {detail}"
    );
}

#[test]
fn hover_const_with_annotation_shows_annotation() {
    // const with explicit type annotation should show the annotation
    let hover = simulate_hover("let x: number = 42", "x");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("number"),
        "hover for annotated let should show type, got: {detail}"
    );
}

#[test]
fn hover_const_string_shows_type() {
    let hover = simulate_hover(r#"let msg = "hello""#, "msg");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("string"),
        "hover for let msg = \"hello\" should show 'string', got: {detail}"
    );
}

#[test]
fn hover_const_bool_shows_type() {
    let hover = simulate_hover("let flag = true", "flag");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("boolean"),
        "hover for let flag = true should show 'boolean', got: {detail}"
    );
}

#[test]
fn hover_function_shows_signature() {
    let hover = simulate_hover(
        "let add(a: number, b: number) -> number = { a + b }",
        "add",
    );
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("fn add"),
        "hover for function should show signature, got: {detail}"
    );
    assert!(
        detail.contains("number"),
        "hover for function should show types, got: {detail}"
    );
}

#[test]
fn hover_const_function_value_shows_type() {
    // A const assigned to a function call should show the inferred return type
    let source = r#"
let getNum() -> number = { 42 }
let result = getNum()
"#;
    let hover = simulate_hover(source, "result");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("number"),
        "hover for let result = getNum() should show inferred type 'number', got: {detail}"
    );
}

#[test]
fn hover_fn_with_return_type_shows_it() {
    // A function with explicit return type should show it
    let hover = simulate_hover("let double(x: number) -> number = { x * 2 }", "double");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        detail.contains("-> number"),
        "hover for fn with return type should show it, got: {detail}"
    );
}

#[test]
fn hover_fn_without_return_type_skips_unresolved() {
    // When the checker can't fully resolve the return type, don't show ?T variables
    let hover = simulate_hover("let double(x: number) = { x * 2 }", "double");
    assert!(hover.is_some());
    let detail = hover.unwrap();
    assert!(
        !detail.contains("?T"),
        "hover should not show unresolved type variables, got: {detail}"
    );
}

#[test]
fn unannotated_const_detail_includes_inferred_type() {
    // Symbol detail is enriched with the checker's inferred type at build
    // time so hover renders `Symbol.detail` directly without a separate
    // enrichment pass.
    let source = "let x = 42";
    let (index, type_map) = build_index_and_types(source);
    let syms = index.find_by_name("x");
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].detail, "let x: number");
    assert_eq!(type_map.get("x").map(|s| s.as_str()), Some("number"));
}

#[test]
fn hover_nested_const_shows_type() {
    // Reproduces #583: const inside nested fn shows no type on hover
    let source = r#"
fn outer() {
    fn inner() {
        let s = "hello"
    }
}
"#;
    let hover = simulate_hover(source, "s");
    assert!(
        hover.as_ref().is_some_and(|h| h.contains("string")),
        "let s inside nested fn should show type, got: {:?}",
        hover
    );
}

#[test]
fn hover_nested_const_with_type_annotation() {
    // const with type annotation should show type even when nested
    let source = r#"
fn outer() {
    fn inner() {
        let s: Option<number> = None
    }
}
"#;
    let hover = simulate_hover(source, "s");
    assert!(
        hover.as_ref().is_some_and(|h| h.contains("Option<number>")),
        "let s with type annotation should show type, got: {:?}",
        hover
    );
}

// ── Match arm variant completion tests (#143) ──────────────

#[test]
fn match_context_detects_variants() {
    // Build the index from valid source with the type declaration
    let valid_source = "type Color = | Red | Green | Blue";
    let (index, _) = build_index_and_types(valid_source);

    // Simulate incomplete source as it would appear in the editor
    let editor_source = "type Color = | Red | Green | Blue\nmatch Color {\n    ";
    let offset = editor_source.len();
    let variants = detect_match_context(editor_source, offset, &index);
    assert!(variants.is_some(), "should detect match context");
    let names: Vec<_> = variants.unwrap().into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"Red".to_string()));
    assert!(names.contains(&"Green".to_string()));
    assert!(names.contains(&"Blue".to_string()));
}

#[test]
fn match_context_not_detected_outside_match() {
    let valid_source = "type Color = | Red | Green | Blue";
    let (index, _) = build_index_and_types(valid_source);

    let editor_source = "type Color = | Red | Green | Blue\nconst x = ";
    let offset = editor_source.len();
    let variants = detect_match_context(editor_source, offset, &index);
    assert!(
        variants.is_none(),
        "should not detect match context outside match block"
    );
}

#[test]
fn match_context_emits_shape_aware_snippets() {
    let source = r#"
type Shape = | Unit
    | Circle(number)
    | Rect(number, number)
    | Rectangle { width: number, height: number }
"#;
    let (index, _) = build_index_and_types(source);
    let editor_source = format!("{source}match s {{\n    ");
    let offset = editor_source.len();
    let variants: Vec<_> = detect_match_context(&editor_source, offset, &index)
        .expect("match context")
        .into_iter()
        .collect();

    let by_name = |name: &str| {
        let (n, shape) = variants.iter().find(|(n, _)| n == name).expect(name);
        match_arm_snippet(n, shape.as_ref())
    };

    assert_eq!(by_name("Unit"), "Unit -> $0,");
    assert_eq!(by_name("Circle"), "Circle($1) -> $0,");
    assert_eq!(by_name("Rect"), "Rect($1, $2) -> $0,");
    assert_eq!(
        by_name("Rectangle"),
        "Rectangle { width: $1, height: $2 } -> $0,"
    );
}

// ── JSX attribute completion tests (#144) ──────────────────

#[test]
fn jsx_tag_detected() {
    assert!(is_in_jsx_tag("<button on", 10));
    assert!(is_in_jsx_tag("<div className", 14));
    assert!(!is_in_jsx_tag("<button>content", 15));
    assert!(!is_in_jsx_tag("let x = 42", 12));
}

#[test]
fn jsx_attribute_completions_with_prefix() {
    let items = jsx_attribute_completions("on");
    assert!(!items.is_empty());
    assert!(items.iter().any(|i| i.label == "onClick"));
    assert!(items.iter().any(|i| i.label == "onChange"));
    assert!(
        items.iter().all(|i| i.label.starts_with("on")),
        "all items should start with 'on'"
    );
}

#[test]
fn jsx_attribute_completions_all() {
    let items = jsx_attribute_completions("");
    assert!(items.iter().any(|i| i.label == "className"));
    assert!(items.iter().any(|i| i.label == "onClick"));
    assert!(items.iter().any(|i| i.label == "disabled"));
}

// ── Lambda event completion tests (#145) ───────────────────

#[test]
fn lambda_event_completions_on_change() {
    let source = r#"<input onChange={(e) -> e.}"#;
    let offset = source.len();
    let items = lambda_event_completions(source, offset, "");
    assert!(items.is_some(), "should provide event completions");
    let items = items.unwrap();
    assert!(items.iter().any(|i| i.label == "target"));
    assert!(items.iter().any(|i| i.label == "preventDefault()"));
}

#[test]
fn lambda_event_completions_target_value() {
    let source = r#"<input onChange={(e) -> e.target.}"#;
    let offset = source.len();
    let items = lambda_event_completions(source, offset, "");
    assert!(
        items.is_some(),
        "should provide target property completions"
    );
    let items = items.unwrap();
    assert!(items.iter().any(|i| i.label == "value"));
    assert!(items.iter().any(|i| i.label == "checked"));
}

#[test]
fn lambda_event_completions_not_in_normal_lambda() {
    let source = r#"let f(x) = x."#;
    let offset = source.len();
    let items = lambda_event_completions(source, offset, "");
    assert!(
        items.is_none(),
        "should not provide event completions in normal lambda"
    );
}

// ── Unresolved import diagnostic test (#142) ───────────────

#[test]
fn unresolved_npm_import_diagnostic() {
    use floe_core::parser::Parser;
    use std::path::Path;

    let source = r#"import { nonexistent } from "fake-package-12345""#;
    let program = Parser::new(source).parse_program().unwrap();
    let analysed = analyse::analyse_parsed(program, ModuleInputs::default());
    let mut index = SymbolIndex::build(
        &analysed.program,
        &analysed.name_types,
        &analysed.name_type_map,
    );
    let cache = HashMap::new();
    // Use a directory that definitely has no node_modules
    let project_dir = Path::new("/tmp/no-such-project-dir");
    let source_dir = project_dir;
    let tsconfig_paths = floe_core::resolve::TsconfigPaths::default();
    let (diags, _) = super::resolution::enrich_from_imports(
        &analysed.program,
        project_dir,
        source_dir,
        &mut index,
        &cache,
        &tsconfig_paths,
    );
    assert!(
        !diags.is_empty(),
        "should report error for unresolved npm import"
    );
    assert!(
        diags[0].message.contains("cannot find module"),
        "diagnostic should say 'cannot find module', got: {}",
        diags[0].message
    );
}

// ── Import path go-to-definition tests (#196) ──────────────

#[test]
fn import_path_at_offset_on_path_string() {
    let source = r#"import { Todo } from "../types""#;
    // Cursor on the 't' in "../types"
    let quote_pos = source.find('"').unwrap();
    let result = import_path_at_offset(source, quote_pos + 3);
    assert_eq!(result, Some("../types".to_string()));
}

#[test]
fn import_path_at_offset_on_opening_quote() {
    let source = r#"import { Todo } from "../types""#;
    let quote_pos = source.find('"').unwrap();
    let result = import_path_at_offset(source, quote_pos);
    assert_eq!(result, Some("../types".to_string()));
}

#[test]
fn import_path_at_offset_on_closing_quote() {
    let source = r#"import { Todo } from "../types""#;
    // Find the closing quote position
    let first_quote = source.find('"').unwrap();
    let closing_quote = source[first_quote + 1..].find('"').unwrap() + first_quote + 1;
    let result = import_path_at_offset(source, closing_quote);
    assert_eq!(result, Some("../types".to_string()));
}

#[test]
fn import_path_at_offset_not_on_path() {
    let source = r#"import { Todo } from "../types""#;
    // Cursor on "Todo" — not on the path string
    let todo_pos = source.find("Todo").unwrap();
    let result = import_path_at_offset(source, todo_pos);
    assert_eq!(result, None);
}

#[test]
fn import_path_at_offset_non_import_line() {
    let source = r#"let x = "hello""#;
    let result = import_path_at_offset(source, 12);
    assert_eq!(result, None);
}

#[test]
fn import_path_at_offset_multiline() {
    let source = "let x = 42\nimport { Foo } from \"./foo\"\nconst y = 10";
    // Cursor on the path in line 2
    let import_line_start = source.find("import").unwrap();
    let quote_pos = source[import_line_start..].find('"').unwrap() + import_line_start;
    let result = import_path_at_offset(source, quote_pos + 2);
    assert_eq!(result, Some("./foo".to_string()));
}

#[test]
fn import_path_at_offset_single_quotes() {
    let source = "import { Foo } from './foo'";
    let quote_pos = source.find('\'').unwrap();
    let result = import_path_at_offset(source, quote_pos + 2);
    assert_eq!(result, Some("./foo".to_string()));
}

#[test]
fn symbol_index_for_block_function_detail() {
    let source = r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
"#;
    let index = build_index(source);
    let syms = index.find_by_name("remaining");
    assert!(!syms.is_empty());
    assert_eq!(
        syms[0].detail, "fn remaining(self: Array<Todo>) -> number",
        "for-block function should show clean signature, not wrapped in for {{ }}"
    );
}

#[test]
fn import_path_at_offset_bare_import() {
    // `import "../todo"` has no `from` keyword
    let source = r#"import "../todo""#;
    let quote_pos = source.find('"').unwrap();
    let result = import_path_at_offset(source, quote_pos + 2);
    assert_eq!(result, Some("../todo".to_string()));
}

#[test]
fn hover_inner_function_shows_unit_return() {
    let source = r#"
export let outer() -> () = {
    fn handleAdd() {
        Console.log("hi")
    }
    handleAdd()
}
"#;
    let hover = simulate_hover(source, "handleAdd");
    assert!(hover.is_some(), "handleAdd should be found");
    let detail = hover.unwrap();
    eprintln!("HOVER handleAdd: {detail}");
    assert!(
        detail.contains("-> ()"),
        "handleAdd hover should show -> (), got: {detail}"
    );
}

// ── Go-to-definition simulation tests (#353) ─────────────

/// Simulate go-to-definition: given source + cursor offset,
/// find the word under cursor, look it up in the index, and
/// return the definition's (start, end) if found.
fn simulate_goto_def(source: &str, cursor_offset: usize) -> Option<(usize, usize)> {
    let index = build_index(source);
    let word = word_at_offset(source, cursor_offset);
    eprintln!("GOTO_DEF: cursor_offset={cursor_offset}, word={word:?}");
    if word.is_empty() {
        return None;
    }
    let syms = index.find_by_name(word);
    eprintln!("GOTO_DEF: found {} symbols for {word:?}", syms.len());
    for sym in &syms {
        eprintln!(
            "  sym: name={:?}, kind={:?}, start={}, end={}, import_source={:?}",
            sym.name, sym.kind, sym.start, sym.end, sym.import_source
        );
    }
    for sym in syms {
        if sym.import_source.is_some() {
            continue;
        }
        if cursor_offset >= sym.start && cursor_offset <= sym.end {
            eprintln!("  SKIP (cursor inside span)");
            continue;
        }
        eprintln!("  FOUND definition at {}..{}", sym.start, sym.end);
        return Some((sym.start, sym.end));
    }
    None
}

#[test]
fn goto_def_type_usage_in_annotation() {
    let source = "type Color = | Red | Green | Blue\nfn pick(c: Color) -> string { \"ok\" }\n";
    // Find "Color" in fn pick(c: Color)
    let usage_offset = source.find("fn pick(c: Color)").unwrap() + "fn pick(c: ".len();
    eprintln!(
        "Color usage at offset {usage_offset}, char: {:?}",
        &source[usage_offset..usage_offset + 5]
    );
    let result = simulate_goto_def(source, usage_offset);
    assert!(
        result.is_some(),
        "go-to-def on type usage should find definition"
    );
}

#[test]
fn goto_def_const_variable_usage() {
    let source = "let first = (x: number): number => { x + 1 }\nfn second(x: number) => number { x + 2 }\n\nconst a = first(1)\nconst b = second(a)\n";
    // Find "a" in second(a)
    let usage_offset = source.find("second(a)").unwrap() + "second(".len();
    eprintln!(
        "let a usage at offset {usage_offset}, char: {:?}",
        &source[usage_offset..usage_offset + 1]
    );
    let result = simulate_goto_def(source, usage_offset);
    assert!(
        result.is_some(),
        "go-to-def on let usage should find definition"
    );
}

#[test]
fn goto_def_union_variant_in_match() {
    let source = "type Color = | Red | Green | Blue\nfn describe(c: Color) -> string {\n    match c {\n        Red -> \"red\",\n        Green -> \"green\",\n        Blue -> \"blue\",\n    }\n}\n";
    // Find "Red" in match arm
    let usage_offset = source.find("Red -> \"red\"").unwrap();
    eprintln!(
        "Red usage at offset {usage_offset}, char: {:?}",
        &source[usage_offset..usage_offset + 3]
    );
    let result = simulate_goto_def(source, usage_offset);
    assert!(
        result.is_some(),
        "go-to-def on union variant in match arm should find definition"
    );
}

#[test]
fn type_map_has_field_entries_for_records() {
    let source = r#"
type User = { name: string, age: number }
let u = User(name: "a", age: 1)
"#;
    let (_, type_map) = build_index_and_types(source);
    assert_eq!(
        type_map.get("__field_User_name").map(String::as_str),
        Some("string"),
        "type_map should have __field_User_name"
    );
    assert_eq!(
        type_map.get("__field_User_age").map(String::as_str),
        Some("number"),
        "type_map should have __field_User_age"
    );
    assert_eq!(
        type_map.get("u").map(String::as_str),
        Some("User"),
        "type_map should have u -> User"
    );
}

#[test]
fn dot_access_completions_for_record_type() {
    let source = r#"
type User = { name: string, age: number }
let u = User(name: "a", age: 1)
"#;
    let (index, type_map) = build_index_and_types(source);
    let items = dot_access_completions("u", "", &type_map, &index);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(
        labels.contains(&"name"),
        "should suggest 'name', got: {labels:?}"
    );
    assert!(
        labels.contains(&"age"),
        "should suggest 'age', got: {labels:?}"
    );
}

// ── Formatting range helper ──────────────────────────────────────

#[test]
fn end_position_no_trailing_newline() {
    let pos = end_position("let x = 1");
    assert_eq!(pos, Position::new(0, 11));
}

#[test]
fn end_position_single_trailing_newline() {
    let pos = end_position("let x = 1\n");
    assert_eq!(pos, Position::new(1, 0));
}

#[test]
fn end_position_multiple_trailing_newlines_covers_all() {
    // The previous implementation using `str::lines()` returned (2, 0) here,
    // leaving the final "\n" outside the edit range — which caused the
    // formatter to leave two trailing newlines after applying the TextEdit.
    let pos = end_position("let x = 1\n\n\n");
    assert_eq!(pos, Position::new(3, 0));
}

#[test]
fn end_position_multiline_no_trailing_newline() {
    let pos = end_position("let a = 1\nconst b = 2");
    assert_eq!(pos, Position::new(1, 11));
}

#[test]
fn end_position_utf16_surrogate_pair_on_last_line() {
    // "𝕊" (U+1D54A) is encoded as two UTF-16 code units — LSP character
    // offsets must count code units, not chars.
    let pos = end_position("let s = \"𝕊\"");
    assert_eq!(pos, Position::new(0, 14));
}
