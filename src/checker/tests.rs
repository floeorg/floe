use super::*;
use crate::diagnostic::Severity;
use crate::parser::Parser;

fn check(source: &str) -> Vec<Diagnostic> {
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    Checker::new().check(&program)
}

fn has_error(diagnostics: &[Diagnostic], code: &str) -> bool {
    diagnostics.iter().any(|d| d.code.as_deref() == Some(code))
}

fn has_error_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.message.contains(text))
}

fn has_warning_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains(text))
}

// ── Rule 1: Basic type checking ─────────────────────────────

#[test]
fn basic_const_number() {
    let diags = check("const x = 42");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn basic_const_string() {
    let diags = check("const x = \"hello\"");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn undeclared_variable() {
    let diags = check("const x = y");
    assert!(has_error_containing(&diags, "is not defined"));
}

// ── Rule 2: Brand enforcement ───────────────────────────────

#[test]
fn brand_comparison_different_tags() {
    let diags = check(
        r#"
type UserId = Brand<string, UserId>
type Email = Brand<string, Email>
const a: UserId = UserId("abc")
const b: Email = Email("test@test.com")
const result = a == b
"#,
    );
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 4: Exhaustiveness checking ─────────────────────────

#[test]
fn exhaustive_match_with_wildcard() {
    let diags = check(
        r#"
const x = match 42 {
    1 -> "one",
    _ -> "other",
}
"#,
    );
    assert!(!has_error(&diags, "E004"));
}

#[test]
fn non_exhaustive_bool_match() {
    let diags = check(
        r#"
const x: boolean = true
const y = match x {
    true -> "yes",
}
"#,
    );
    assert!(has_error_containing(&diags, "non-exhaustive"));
}

// ── Rule 5: Result/Option ? tracking ────────────────────────

#[test]
fn unwrap_in_result_function() {
    let diags = check(
        r#"
fn tryFetch(url: string) -> Result<string, string> {
    const result = Ok("data")
    const value = result?
    Ok(value)
}
"#,
    );
    let unwrap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("E005") && d.message.contains("operator requires"))
        .collect();
    assert!(unwrap_errors.is_empty());
}

#[test]
fn unwrap_not_on_result_or_option() {
    let diags = check(
        r#"
fn process() -> Result<number, string> {
    const x = 42
    const y = x?
    Ok(y)
}
"#,
    );
    assert!(has_error_containing(
        &diags,
        "`?` can only be used on `Result` or `Option`"
    ));
}

// ── Rule 6: No property access on unnarrowed unions ─────────

#[test]
fn property_access_on_result() {
    let diags = check(
        r#"
const result = Ok(42)
const x = result.value
"#,
    );
    assert!(has_error_containing(
        &diags,
        "cannot access `.value` on `Result`"
    ));
}

// ── Rule 8: Same-type equality ──────────────────────────────

#[test]
fn equality_same_types() {
    let diags = check("const x = 1 == 1");
    assert!(!has_error(&diags, "E008"));
}

#[test]
fn equality_different_types() {
    let diags = check(r#"const x = 1 == "hello""#);
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 9: Unused detection ────────────────────────────────

#[test]
fn unused_variable_warning() {
    let diags = check("const x = 42");
    assert!(has_warning_containing(&diags, "unused variable"));
}

#[test]
fn underscore_prefix_suppresses_unused() {
    let diags = check("const _x = 42");
    assert!(!has_warning_containing(&diags, "is never used"));
}

#[test]
fn used_variable_no_warning() {
    let diags = check(
        r#"
const x = 42
const y = x
"#,
    );
    let unused_x: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning && d.message.contains("`x`"))
        .collect();
    assert!(unused_x.is_empty());
}

#[test]
fn unused_import_error() {
    let diags = check(r#"import { useState } from "react""#);
    assert!(has_error_containing(&diags, "unused import"));
}

// ── Rule 10: Exported function return types ─────────────────

#[test]
fn exported_function_needs_return_type() {
    let diags = check("export fn add(a: number, b: number) { a }");
    assert!(has_error_containing(&diags, "must declare a return type"));
}

#[test]
fn exported_function_with_return_type_ok() {
    let diags = check("export fn add(a: number, b: number) -> number { a }");
    assert!(!has_error(&diags, "E010"));
}

// ── Return type mismatch ─────────────────────────────────────

#[test]
fn return_type_mismatch_errors() {
    let diags = check(
        r#"
fn greet() -> string { 42 }
"#,
    );
    assert!(
        has_error_containing(&diags, "expected return type"),
        "should error when body returns number but declared string, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn return_type_match_ok() {
    let diags = check(
        r#"
fn greet() -> string { "hello" }
"#,
    );
    assert!(!has_error_containing(&diags, "expected return type"),);
}

#[test]
fn non_exported_function_return_type_not_required() {
    // Non-exported functions can omit -> return type
    let diags = check(
        r#"
fn helper(x: number) { x * 2 }
"#,
    );
    assert!(!has_error(&diags, "E010"));
}

// ── Rule 12: String concat warning ──────────────────────────

#[test]
fn string_concat_warning() {
    let diags = check(r#"const x = "hello" + " world""#);
    assert!(has_warning_containing(&diags, "template literal"));
}

// ── OK/Err/Some/None types ──────────────────────────────────

#[test]
fn ok_creates_result() {
    let diags = check("const _x = Ok(42)");
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn none_creates_option() {
    let diags = check("const _x = None");
    assert!(!has_error(&diags, "E001"));
}

// ── Array type checking ─────────────────────────────────────

#[test]
fn homogeneous_array() {
    let diags = check("const _x = [1, 2, 3]");
    assert!(!has_error(&diags, "E004"));
}

#[test]
fn mixed_array_inferred_as_unknown() {
    // Mixed-type arrays should be allowed and inferred as Array<unknown>
    let diags = check(r#"const _x = [1, "two", 3]"#);
    assert!(!has_error(&diags, "E004"));
    assert!(!has_error_containing(&diags, "mixed types"));
}

#[test]
fn mixed_array_string_and_number() {
    // e.g. TanStack Query's queryKey: ["user", props.userId]
    let diags = check(r#"const _x = ["user", 42]"#);
    assert!(!has_error(&diags, "E004"));
}

// ── Opaque type enforcement ─────────────────────────────────

#[test]
fn opaque_type_cannot_be_constructed() {
    let diags = check(
        r#"
opaque type HashedPassword = string
const _x = HashedPassword("abc")
"#,
    );
    assert!(has_error_containing(&diags, "opaque type"));
}

// ── Unhandled Result ────────────────────────────────────────

#[test]
fn floating_result_error() {
    let diags = check("Ok(42)");
    assert!(has_error_containing(&diags, "unhandled `Result`"));
}

// ── For Blocks ─────────────────────────────────────────────

#[test]
fn for_block_registers_function() {
    let diags = check(
        r#"
type User = { name: string }
for User {
    fn display(self) -> string { self.name }
}
const _x = display(User(name: "Ryan"))
"#,
    );
    // display should be defined and callable
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn for_block_self_gets_type() {
    let diags = check(
        r#"
type User = { name: string }
for User {
    fn getName(self) -> string { self.name }
}
"#,
    );
    // self.name should resolve since self is typed as User
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn for_block_multiple_params() {
    let diags = check(
        r#"
type User = { name: string }
for User {
    fn greet(self, greeting: string) -> string { greeting }
}
const _x = greet(User(name: "Ryan"), "Hello")
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn call_site_type_args_infer_return() {
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { useState } from "react"
type Todo = { text: string }
const [todos, _setTodos] = useState<Array<Todo>>([])
const _x = todos
"#,
    )
    .parse_program()
    .expect("should parse");

    // Provide a mock useState type: <S>(initialState: S) => [S, (S) => void]
    let use_state_export = DtsExport {
        name: "useState".to_string(),
        ts_type: TsType::Function {
            params: vec![TsType::Named("S".to_string())],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Function {
                    params: vec![TsType::Named("S".to_string())],
                    return_type: Box::new(TsType::Primitive("void".to_string())),
                },
            ])),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, types) = checker.check_with_types(&program);

    assert!(
        !has_error_containing(&diags, "not defined"),
        "unexpected errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    // todos should be Array<Todo> (first element of the substituted tuple)
    if let Some(ty) = types.get("todos") {
        assert!(ty.contains("Array"), "expected Array type, got: {ty}");
    }
    // _setTodos should be a function (second element of the substituted tuple)
    if let Some(ty) = types.get("_setTodos") {
        assert!(
            ty.contains("->"),
            "expected function type for setter, got: {ty}"
        );
    }
}

#[test]
fn for_block_with_pipe() {
    let diags = check(
        r#"
type User = { name: string }
for User {
    fn display(self) -> string { self.name }
}
const _user = User(name: "Ryan")
const _x = _user |> display
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

// ── Inline For Declarations ─────────────────────────────────

#[test]
fn inline_for_registers_function() {
    let diags = check(
        r#"
type User = { name: string }
for User fn display(self) -> string { self.name }
const _x = display(User(name: "Ryan"))
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn inline_for_exported_registers_function() {
    let diags = check(
        r#"
type User = { name: string }
export for User fn display(self) -> string { self.name }
const _x = display(User(name: "Ryan"))
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn inline_for_self_gets_type() {
    let diags = check(
        r#"
type User = { name: string }
for User fn getName(self) -> string { self.name }
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

#[test]
fn inline_for_with_pipe() {
    let diags = check(
        r#"
type User = { name: string }
for User fn display(self) -> string { self.name }
const _user = User(name: "Ryan")
const _x = _user |> display
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

// ── Untrusted Import Enforcement ─────────────────────────────

#[test]
fn untrusted_import_requires_try() {
    let diags = check(
        r#"
import { fetchUser } from "some-lib"
const _x = fetchUser("id")
"#,
    );
    assert!(has_error(&diags, "E014"));
    assert!(has_error_containing(&diags, "untrusted import"));
}

#[test]
fn untrusted_import_ok_with_try() {
    let diags = check(
        r#"
import { fetchUser } from "some-lib"
const _x = try fetchUser("id")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn trusted_specifier_no_error() {
    let diags = check(
        r#"
import { trusted capitalize } from "some-lib"
const _x = capitalize("hello")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn trusted_module_no_error() {
    let diags = check(
        r#"
import trusted { capitalize, slugify } from "string-utils"
const _x = capitalize("hello")
const _y = slugify("hello world")
"#,
    );
    assert!(!has_error(&diags, "E014"));
}

#[test]
fn mixed_trusted_untrusted() {
    let diags = check(
        r#"
import { trusted capitalize, fetchUser } from "some-lib"
const _x = capitalize("hello")
const _y = fetchUser("id")
"#,
    );
    // capitalize is trusted — no error
    assert!(!has_error_containing(&diags, "capitalize"));
    // fetchUser is untrusted — error
    assert!(has_error_containing(&diags, "fetchUser"));
}

// ── Constructor field validation ────────────────────────────

#[test]
fn constructor_unknown_field_error() {
    let diags = check(
        r#"
type Todo = {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", textt: "hello", done: false)
"#,
    );
    assert!(has_error(&diags, "E015"));
    assert!(has_error_containing(&diags, "unknown field `textt`"));
}

#[test]
fn constructor_valid_fields_no_error() {
    let diags = check(
        r#"
type Todo = {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", text: "hello", done: false)
"#,
    );
    assert!(!has_error(&diags, "E015"));
    assert!(!has_error(&diags, "E016"));
}

#[test]
fn constructor_missing_required_field() {
    let diags = check(
        r#"
type Todo = {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", text: "hello")
"#,
    );
    assert!(has_error(&diags, "E016"));
    assert!(has_error_containing(
        &diags,
        "missing required field `done`"
    ));
}

#[test]
fn constructor_missing_field_with_default_ok() {
    let diags = check(
        r#"
type Config = {
    host: string,
    port: number = 3000,
}
const _c = Config(host: "localhost")
"#,
    );
    assert!(!has_error(&diags, "E016"));
}

#[test]
fn constructor_spread_skips_missing_check() {
    let diags = check(
        r#"
type Todo = {
    id: string,
    text: string,
    done: bool,
}
const original = Todo(id: "1", text: "hello", done: false)
const _t = Todo(..original, text: "updated")
"#,
    );
    assert!(!has_error(&diags, "E016"));
}

#[test]
fn union_variant_unknown_field_error() {
    let diags = check(
        r#"
type Validation =
    | Valid(text: string)
    | TooShort
    | Empty

const _v = Valid(texxt: "hello")
"#,
    );
    assert!(has_error(&diags, "E015"));
    assert!(has_error_containing(&diags, "unknown field `texxt`"));
}

#[test]
fn union_variant_valid_field_no_error() {
    let diags = check(
        r#"
type Validation =
    | Valid(text: string)
    | TooShort
    | Empty

const _v = Valid(text: "hello")
"#,
    );
    assert!(!has_error(&diags, "E015"));
}

// ── Unknown type errors ────────────────────────────────────

#[test]
fn unknown_type_in_record_field() {
    let diags = check(
        r#"
type Todo = {
    id: string,
    text: string,
    done: asojSIDJA,
}
"#,
    );
    assert!(has_error_containing(&diags, "unknown type `asojSIDJA`"));
}

#[test]
fn unknown_type_in_const_annotation() {
    let diags = check("const x: Nonexistent = 42");
    assert!(has_error_containing(&diags, "unknown type `Nonexistent`"));
}

#[test]
fn unknown_type_in_function_param() {
    let diags = check("fn foo(x: BadType) -> () {}");
    assert!(has_error_containing(&diags, "unknown type `BadType`"));
}

#[test]
fn unknown_type_in_function_return() {
    let diags = check("fn foo() -> BadReturn { 42 }");
    assert!(has_error_containing(&diags, "unknown type `BadReturn`"));
}

#[test]
fn known_type_no_error() {
    let diags = check(
        r#"
type User = { name: string }
const _u: User = User(name: "Alice")
"#,
    );
    assert!(!has_error_containing(&diags, "unknown type"));
}

#[test]
fn builtin_types_no_error() {
    let diags = check(
        r#"
const _a: number = 42
const _b: string = "hi"
const _c: boolean = true
"#,
    );
    assert!(!has_error_containing(&diags, "unknown type"));
}

#[test]
fn forward_reference_in_union_no_error() {
    let diags = check(
        r#"
type Container = { item: Item }
type Item = { name: string }
"#,
    );
    assert!(!has_error_containing(&diags, "unknown type `Item`"));
}

// ── Function argument type validation ─────────────────────

#[test]
fn function_call_wrong_arg_type() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _r = add("hello", true)
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `string`"
    ));
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `boolean`"
    ));
}

#[test]
fn function_call_correct_types_no_error() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _r = add(1, 2)
"#,
    );
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn function_call_wrong_arg_count() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _r = add(1)
"#,
    );
    assert!(has_error_containing(&diags, "expects 2 arguments, found 1"));
}

#[test]
fn function_call_too_many_args() {
    let diags = check(
        r#"
fn greet(name: string) -> string { name }
const _r = greet("Alice", "Bob")
"#,
    );
    assert!(has_error_containing(&diags, "expects 1 argument, found 2"));
}

#[test]
fn pipe_call_accounts_for_implicit_arg() {
    let diags = check(
        r#"
fn double(x: number) -> number { x + x }
const _r = 5 |> double
"#,
    );
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn pipe_call_with_extra_args_no_false_positive() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _r = 5 |> add(3)
"#,
    );
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn pipe_call_wrong_type() {
    let diags = check(
        r#"
fn double(x: number) -> number { x + x }
const _r = "hello" |> double
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `string`"
    ));
}

#[test]
fn pipe_stdlib_wrong_type_via_type_directed() {
    // `5 |> trim` should error: trim expects string, got number
    // This goes through type-directed resolution (Number -> Number module, no trim found)
    // then falls back to name-based lookup (finds String.trim)
    let diags = check(
        r#"
const _r = 5 |> trim
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `string`, found `number`"
    ));
}

#[test]
fn pipe_stdlib_wrong_type_number_to_sort() {
    // `5 |> sort` should error: sort expects Array<T>, got number
    let diags = check(
        r#"
const _r = 5 |> sort
"#,
    );
    assert!(has_error_containing(&diags, "found `number`"));
}

#[test]
fn pipe_stdlib_correct_type() {
    // `"hello" |> trim` should NOT error
    let diags = check(
        r#"
const _r = "hello" |> trim
"#,
    );
    assert!(!has_error(&diags, "E001"));
}

#[test]
fn pipe_stdlib_correct_array_type() {
    // `[1, 2, 3] |> sort` should NOT error
    let diags = check(
        r#"
const _r = [1, 2, 3] |> sort
"#,
    );
    assert!(!has_error(&diags, "E001"));
}

// ── Variable shadowing tests (#189) ─────────────────────────

#[test]
fn shadow_const_redefinition_errors() {
    // Defining the same const name twice in the same scope should error
    let diags = check(
        r#"
const x = 5
const x = 10
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_const_shadows_function_errors() {
    // A const shadowing a function name should error
    let diags = check(
        r#"
fn double(x: number) -> number { x * 2 }
const double = 42
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_const_shadows_for_block_fn_errors() {
    // A const shadowing a for-block function should error
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
const remaining = 5
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_function_redefinition_errors() {
    // Defining two functions with the same name should error
    let diags = check(
        r#"
fn foo() -> number { 1 }
fn foo() -> string { "hi" }
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_not_allowed_in_inner_scope() {
    // No shadowing ever — even function params can't shadow outer names
    let diags = check(
        r#"
const x = 5
fn double(x: number) -> number { x * 2 }
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_inner_scope_const_shadows_for_block_fn() {
    // A const INSIDE a function body shadowing a for-block function should error
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
fn test() -> number {
    const remaining = 5
    remaining
}
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "inner-scope const should not shadow for-block fn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_inner_scope_const_shadows_outer_const() {
    // A const inside a function body shadowing an outer const should error
    let diags = check(
        r#"
const x = 5
fn test() -> number {
    const x = 10
    x
}
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "inner-scope const should not shadow outer const, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn for_block_pipe_then_shadow_errors() {
    // Real-world case: piping into for-block fn then shadowing its name
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
fn test() -> number {
    const _todos: Array<Todo> = []
    const remaining = _todos |> remaining
    remaining
}
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "should error on shadowing for-block fn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #192: Shadowing error context ───────────────────────

#[test]
fn shadow_error_includes_source_const() {
    let diags = check(
        r#"
const x = 5
const x = 10
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined (const)"),
        "shadow error should mention source 'const', got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_error_includes_source_function() {
    let diags = check(
        r#"
fn double(x: number) -> number { x * 2 }
const double = 42
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined (function)"),
        "shadow error should mention source 'function', got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_error_includes_source_for_block() {
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
const remaining = 5
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined (for-block function)"),
        "shadow error should mention source 'for-block function', got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #192: Pipe into non-function ────────────────────────

#[test]
fn pipe_into_non_function_errors() {
    let diags = check(
        r#"
const items = [1, 2, 3]
const target = "hello"
const _x = items |> target
"#,
    );
    assert!(
        has_error_containing(&diags, "cannot pipe into `target`"),
        "should error on piping into non-function, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_into_number_errors() {
    let diags = check(
        r#"
const items = [1, 2, 3]
const count = 42
const _x = items |> count
"#,
    );
    assert!(
        has_error_containing(&diags, "cannot pipe into `count`"),
        "should error on piping into number, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_into_function_ok() {
    let diags = check(
        r#"
fn double(x: number) -> number { x * 2 }
const _r = 5 |> double
"#,
    );
    assert!(
        !has_error_containing(&diags, "cannot pipe into"),
        "piping into function should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Phase 1: Type Resolution Foundation ─────────────────────

// ── 2. Member access on Named types ────────────────────────

#[test]
fn member_access_on_record_type_resolves_field() {
    let diags = check(
        r#"
type User = { name: string, age: number }
const u = User(name: "hi", age: 21)
const _n = u.name
"#,
    );
    assert!(
        !has_error_containing(&diags, "Unknown"),
        "u.name should resolve to string, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    // Verify no errors at all (field access should succeed)
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "member access on record type should not produce errors, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn member_access_unknown_field_errors() {
    let diags = check(
        r#"
type User = { name: string }
const u = User(name: "hi")
const _n = u.nonexistent
"#,
    );
    assert!(
        has_error_containing(&diags, "has no field `nonexistent`"),
        "should error on unknown field, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn member_access_on_non_record_errors() {
    let diags = check(
        r#"
const x = 5
const _n = x.name
"#,
    );
    assert!(
        has_error_containing(&diags, "cannot access"),
        "should error on member access on number, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── 3. Constructor field type validation ───────────────────

#[test]
fn constructor_wrong_field_type_errors() {
    let diags = check(
        r#"
type User = { name: string, age: number }
const _u = User(name: 42, age: "old")
"#,
    );
    assert!(
        has_error_containing(&diags, "expected `string`, found `number`"),
        "should error on wrong field type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn constructor_correct_types_ok() {
    let diags = check(
        r#"
type User = { name: string, age: number }
const _u = User(name: "hi", age: 21)
"#,
    );
    let type_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error && d.message.contains("expected"))
        .collect();
    assert!(
        type_errors.is_empty(),
        "correct constructor types should not error, got: {:?}",
        type_errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn constructor_missing_field_errors_phase1() {
    // This test verifies missing field detection (already exists as constructor_missing_required_field
    // but let's add one that specifically tests the two-field case)
    let diags = check(
        r#"
type User = { name: string, age: number }
const _u = User(name: "hi")
"#,
    );
    assert!(
        has_error_containing(&diags, "missing required field `age`"),
        "should error on missing required field, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── 4. Match arm type consistency ──────────────────────────

#[test]
fn match_arms_incompatible_types_errors() {
    let diags = check(
        r#"
const x = 1
const _y = match x {
    1 -> "hi",
    _ -> 42,
}
"#,
    );
    assert!(
        has_error_containing(&diags, "match arms have incompatible types"),
        "should error on incompatible match arm types, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn match_arms_compatible_types_ok() {
    let diags = check(
        r#"
const x = 1
const _y = match x {
    1 -> "hi",
    _ -> "bye",
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "match arms have incompatible types"),
        "compatible match arms should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── 5. If/else is banned (parse-level) ────────────────────

#[test]
fn if_else_is_banned() {
    let result = Parser::new("const _x = if true { 1 } else { 2 }").parse_program();
    assert!(
        result.is_err(),
        "if/else should be banned at the parse level"
    );
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("banned keyword")),
        "expected banned keyword error for `if`, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// ── 6. Object destructuring ───────────────────────────────

#[test]
fn unit_type_from_void_match() {
    // A match where all arms return unit should infer () not unknown
    let program = crate::parser::Parser::new(
        r#"
fn log(msg: string) { Console.log(msg) }
const _hello = match true {
    true -> log("hi"),
    false -> log("bye"),
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types) = Checker::new().check_with_types(&program);
    if let Some(ty) = types.get("_hello") {
        assert_eq!(ty, "()", "void match should infer (), got: {ty}");
    } else {
        panic!("_hello should be in type map");
    }
}

#[test]
fn unit_type_from_void_function_call() {
    // Calling a function that returns nothing should give ()
    let program = crate::parser::Parser::new(
        r#"
fn log(msg: string) { Console.log(msg) }
const _result = log("test")
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types) = Checker::new().check_with_types(&program);
    if let Some(ty) = types.get("_result") {
        assert_eq!(ty, "()", "void function call should give (), got: {ty}");
    } else {
        panic!("_result should be in type map");
    }
}

#[test]
fn calling_named_function_type_returns_its_return_type() {
    // Dispatch<SetStateAction<T>> is a function type alias from React.
    // When we call setTodos(...), the checker sees Named("Dispatch<...>")
    // and returns Unknown. It should return the function's return type.
    //
    // Simulate: setTodos has type (Array<Todo>) -> ()
    // (which is what Dispatch<SetStateAction<Array<Todo>>> resolves to)
    let program = crate::parser::Parser::new(
        r#"
type Todo = { text: string }
fn setTodos(value: Array<Todo>) -> () { () }
fn handler() {
    setTodos([])
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types) = Checker::new().check_with_types(&program);
    eprintln!("types: {:?}", types);
    // handler calls setTodos which returns () — handler should infer ()
    if let Some(ty) = types.get("handler") {
        assert!(
            ty.contains("()"),
            "handler should infer () from calling void function, got: {ty}"
        );
    } else {
        panic!("handler should be in types");
    }
}

#[test]
fn dispatch_generic_converts_to_function() {
    // The REAL tsgo output: Dispatch<SetStateAction<Todo[]>> should become a function type
    use crate::interop::{DtsExport, TsType};

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
type Todo = { text: string }
const [todos, setTodos] = useState<Array<Todo>>([])
fn handler() {
    setTodos([])
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate what tsgo ACTUALLY returns: Dispatch<SetStateAction<Todo[]>>
    let probe_export = DtsExport {
        name: "__probe_todos_setTodos".to_string(),
        ts_type: TsType::Tuple(vec![
            TsType::Array(Box::new(TsType::Named("Todo".to_string()))),
            TsType::Generic {
                name: "Dispatch".to_string(),
                args: vec![TsType::Generic {
                    name: "SetStateAction".to_string(),
                    args: vec![TsType::Array(Box::new(TsType::Named("Todo".to_string())))],
                }],
            },
        ]),
    };
    let use_state_export = DtsExport {
        name: "useState".to_string(),
        ts_type: TsType::Function {
            params: vec![TsType::Named("S".to_string())],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Named("S".to_string()),
            ])),
        },
    };
    let mut dts_imports = std::collections::HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export, probe_export]);

    let checker = Checker::with_all_imports(std::collections::HashMap::new(), dts_imports);
    let (_diags, types) = checker.check_with_types(&program);
    eprintln!("types (real dispatch): {:?}", types);

    // setTodos should be a function, NOT Named("Dispatch<...>")
    if let Some(ty) = types.get("setTodos") {
        assert!(
            ty.contains("->"),
            "setTodos with Dispatch<SetStateAction> should be a function, got: {ty}"
        );
    } else {
        panic!("setTodos should be in types");
    }

    // handler should infer () because setTodos returns void
    if let Some(ty) = types.get("handler") {
        assert!(
            ty.contains("()"),
            "handler calling dispatch setter should infer (), got: {ty}"
        );
    } else {
        panic!("handler should be in types");
    }
}

#[test]
fn calling_dispatch_type_is_callable() {
    // The REAL problem: setTodos has type Named("Dispatch<SetStateAction<...>>")
    // which is NOT Type::Function. Calling it returns Unknown.
    // This test demonstrates the gap.
    use crate::interop::DtsExport;
    use crate::interop::TsType;

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
type Todo = { text: string }
const [todos, setTodos] = useState<Array<Todo>>([])
fn handler() {
    setTodos([])
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate tsgo giving us the probe result
    let probe_export = DtsExport {
        name: "__probe_todos_setTodos".to_string(),
        ts_type: TsType::Tuple(vec![
            TsType::Array(Box::new(TsType::Named("Todo".to_string()))),
            TsType::Function {
                params: vec![TsType::Named("Todo[]".to_string())],
                return_type: Box::new(TsType::Primitive("void".to_string())),
            },
        ]),
    };
    let use_state_export = DtsExport {
        name: "useState".to_string(),
        ts_type: TsType::Function {
            params: vec![TsType::Named("S".to_string())],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Function {
                    params: vec![TsType::Named("S".to_string())],
                    return_type: Box::new(TsType::Primitive("void".to_string())),
                },
            ])),
        },
    };
    let mut dts_imports = std::collections::HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export, probe_export]);

    let checker = Checker::with_all_imports(std::collections::HashMap::new(), dts_imports);
    let (_diags, types) = checker.check_with_types(&program);
    eprintln!("types with dts: {:?}", types);

    // setTodos should be a function type, not Named("Dispatch<...>")
    if let Some(ty) = types.get("setTodos") {
        eprintln!("setTodos type: {ty}");
        assert!(
            !ty.contains("unknown"),
            "setTodos should not be unknown, got: {ty}"
        );
    }

    // handler should infer () because setTodos returns void
    if let Some(ty) = types.get("handler") {
        assert!(
            ty.contains("()"),
            "handler should infer () when calling void setTodos, got: {ty}"
        );
    } else {
        panic!("handler should be in types");
    }
}

#[test]
fn inner_function_infers_unit_return() {
    let program = crate::parser::Parser::new(
        r#"
fn outer() {
    fn inner() {
        Console.log("hi")
    }
    inner()
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types) = Checker::new().check_with_types(&program);
    eprintln!("types: {:?}", types);
    if let Some(ty) = types.get("inner") {
        assert!(
            ty.contains("()"),
            "inner function should infer () return, got: {ty}"
        );
    }
    if let Some(ty) = types.get("outer") {
        assert!(
            ty.contains("()"),
            "outer function should infer () return, got: {ty}"
        );
    }
}

#[test]
fn object_destructuring_gets_field_types() {
    let program = crate::parser::Parser::new(
        r#"
type User = { name: string, age: number }
const user = User(name: "hi", age: 21)
const { name, age } = user
const _x = name
const _y = age
"#,
    )
    .parse_program()
    .expect("should parse");
    let (diags, types) = Checker::new().check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "destructuring should not produce errors, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // name should be string, age should be number
    if let Some(name_ty) = types.get("name") {
        assert_eq!(name_ty, "string", "name should be string, got: {name_ty}");
    }
    if let Some(age_ty) = types.get("age") {
        assert_eq!(age_ty, "number", "age should be number, got: {age_ty}");
    }
}

// ── Tuple Types ─────────────────────────────────────────────

#[test]
fn tuple_construction_infers_type() {
    let diags = check("const _p = (1, 2)");
    assert!(
        diags.is_empty(),
        "tuple construction should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_with_type_annotation() {
    let diags = check("const _p: (number, number) = (1, 2)");
    assert!(
        diags.is_empty(),
        "tuple with type annotation should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_type_mismatch() {
    let diags = check(r#"const _p: (number, number) = ("a", "b")"#);
    assert!(
        has_error(&diags, "E001"),
        "tuple type mismatch should produce E001, got: {diags:?}"
    );
}

#[test]
fn tuple_destructuring_infers_types() {
    let source = r#"
        const _pair = (10, "hello")
        const (_x, _y) = _pair
        const _z = _x + 1
    "#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "tuple destructuring should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_in_function_return() {
    let source = r#"
        export fn divmod(a: number, b: number) -> (number, number) {
            (a / b, a % b)
        }
    "#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "tuple return type should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_three_elements() {
    let diags = check(r#"const _t = (1, "two", true)"#);
    assert!(
        diags.is_empty(),
        "3-element tuple should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_return_from_block_inline() {
    // Tuples work inline with function params
    let source = r#"
        export fn test(a: number, b: number) -> (number, number) {
            (a + 1, b + 1)
        }
    "#;
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "tuple return inline should not produce errors: {diags:?}"
    );
}

// ── Pipe: tap ───────────────────────────────────────────────

#[test]
fn pipe_tap_no_errors() {
    // tap with a function should type-check without errors
    let diags = check(
        r#"
const _x = [1, 2, 3] |> tap(Console.log)
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
}

#[test]
fn pipe_tap_qualified_no_errors() {
    // Pipe.tap should also work when fully qualified
    let diags = check(
        r#"
const _x = [1, 2, 3] |> Pipe.tap(Console.log)
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
}

// ── Trait declarations ──────────────────────────────────────────

#[test]
fn trait_basic_definition() {
    let diags = check(
        r#"
trait Display {
  fn display(self) -> string
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "trait definition should not produce errors: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_impl_valid() {
    let diags = check(
        r#"
trait Display {
  fn display(self) -> string
}
type User = { name: string }
for User: Display {
  fn display(self) -> string {
    self.name
  }
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "valid trait impl should not produce errors: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_impl_missing_method() {
    let diags = check(
        r#"
trait Display {
  fn display(self) -> string
}
type User = { name: string }
for User: Display {
  fn toString(self) -> string {
    "wrong"
  }
}
"#,
    );
    assert!(
        has_error(&diags, "E018"),
        "should error on missing required method"
    );
    assert!(has_error_containing(&diags, "requires method `display`"));
}

#[test]
fn trait_unknown_trait() {
    let diags = check(
        r#"
type User = { name: string }
for User: NonExistent {
  fn display(self) -> string {
    self.name
  }
}
"#,
    );
    assert!(has_error(&diags, "E017"), "should error on unknown trait");
    assert!(has_error_containing(&diags, "unknown trait"));
}

// ── Test Blocks ──────────────────────────────────────────────

#[test]
fn test_block_type_checks_body() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }

test "addition" {
    assert add(1, 2) == 3
}
"#,
    );
    // Should produce no errors
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
}

#[test]
fn test_block_assert_requires_boolean() {
    let diags = check(
        r#"
test "bad assert" {
    assert 42
}
"#,
    );
    assert!(
        has_error_containing(&diags, "assert expression must be boolean"),
        "expected boolean error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_default_method_not_required() {
    let diags = check(
        r#"
trait Eq {
  fn eq(self, other: string) -> boolean
  fn neq(self, other: string) -> boolean {
    !(self |> eq(other))
  }
}
type User = { name: string }
for User: Eq {
  fn eq(self, other: string) -> boolean {
    self.name == other
  }
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "default methods should not be required: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_for_block_without_trait_still_works() {
    let diags = check(
        r#"
type User = { name: string }
for User {
  fn greet(self) -> string {
    self.name
  }
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "regular for block should still work: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_impl_all_required_methods() {
    let diags = check(
        r#"
trait Printable {
  fn print(self) -> string
  fn prettyPrint(self) -> string
}
type User = { name: string }
for User: Printable {
  fn print(self) -> string {
    self.name
  }
  fn prettyPrint(self) -> string {
    self.name
  }
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "all methods implemented: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_impl_missing_one_of_two() {
    let diags = check(
        r#"
trait Printable {
  fn print(self) -> string
  fn prettyPrint(self) -> string
}
type User = { name: string }
for User: Printable {
  fn print(self) -> string {
    self.name
  }
}
"#,
    );
    assert!(
        has_error(&diags, "E018"),
        "should error on missing prettyPrint"
    );
    assert!(has_error_containing(&diags, "prettyPrint"));
}

// ── Bug: Cross-file trait resolution ────────────────────────
// Traits imported from another file should be recognized by the checker

#[test]
fn cross_file_trait_resolution() {
    use crate::lexer::span::Span;
    use crate::parser::ast::*;
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let dummy_span = Span::new(0, 0, 0, 0);

    // Simulate a resolved import that exports a trait `Display`
    let mut imports = HashMap::new();
    let mut resolved = ResolvedImports::default();
    resolved.trait_decls.push(TraitDecl {
        exported: true,
        name: "Display".to_string(),
        methods: vec![TraitMethod {
            name: "display".to_string(),
            params: vec![Param {
                name: "self".to_string(),
                type_ann: None,
                default: None,
                destructure: None,
                span: dummy_span,
            }],
            return_type: Some(TypeExpr {
                kind: TypeExprKind::Named {
                    name: "string".to_string(),
                    type_args: vec![],
                    bounds: vec![],
                },
                span: dummy_span,
            }),
            body: None,
            span: dummy_span,
        }],
        span: dummy_span,
    });
    // Also need to export the type
    resolved.type_decls.push(TypeDecl {
        exported: true,
        opaque: false,
        name: "User".to_string(),
        type_params: vec![],
        def: TypeDef::Record(vec![RecordEntry::Field(Box::new(RecordField {
            name: "name".to_string(),
            type_ann: TypeExpr {
                kind: TypeExprKind::Named {
                    name: "string".to_string(),
                    type_args: vec![],
                    bounds: vec![],
                },
                span: dummy_span,
            },
            default: None,
            span: dummy_span,
        }))]),
        deriving: vec![],
    });
    imports.insert("./types".to_string(), resolved);

    let source = r#"
import { User, Display } from "./types"

for User: Display {
    fn display(self) -> string {
        self.name
    }
}
"#;

    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::with_imports(imports).check(&program);
    assert!(
        !has_error_containing(&diags, "unknown trait"),
        "imported trait Display should be recognized, but got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug: Pipe with stdlib member access returns Unknown ─────
// `x |> String.length` should infer as number, not unknown

#[test]
fn pipe_stdlib_member_returns_correct_type() {
    let source = r#"
const len = "hello" |> String.length
const doubled = len + 1
"#;
    let diags = check(source);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "pipe with String.length should infer number, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug: npm imports used as constructors ───────────────────
// When an uppercase import (e.g. QueryClient) is called with named args,
// the parser produces a Construct node. The checker should recognize it
// as a known import and not emit "unknown type".

#[test]
fn npm_import_used_as_constructor_no_error() {
    let diags = check(
        r#"
import trusted { QueryClient } from "@tanstack/react-query"
const _qc = QueryClient(defaultOptions: {})
"#,
    );
    assert!(
        !has_error_containing(&diags, "unknown type"),
        "npm import used as constructor should not error, but got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Browser globals ────────────────────────────────────────

#[test]
fn fetch_is_recognized_as_global() {
    let diags = check("const result = fetch(\"https://example.com\")");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "fetch should be a recognized browser global, but got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn browser_globals_are_recognized() {
    let globals = vec![
        "const w = window",
        "const d = document",
        "const j = JSON.parse(\"{}\")",
    ];
    for src in globals {
        let diags = check(src);
        assert!(
            !has_error_containing(&diags, "is not defined"),
            "{src} should not produce 'not defined' error, but got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn timer_globals_are_recognized() {
    let globals = vec![
        "const a = setTimeout",
        "const b = setInterval",
        "const c = clearTimeout",
        "const d = clearInterval",
    ];
    for src in globals {
        let diags = check(src);
        assert!(
            !has_error_containing(&diags, "is not defined"),
            "{src} should not produce 'not defined' error, but got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

// ── Unsafe narrowing from unknown ───────────────────────────

#[test]
fn narrowing_unknown_to_concrete_type_is_error() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
const data = getData()
const x: number = data
"#,
    );
    assert!(
        has_error(&diags, "E019"),
        "narrowing unknown to a concrete type should be an error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn unknown_to_unknown_annotation_is_ok() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
const data = getData()
const x: unknown = data
"#,
    );
    assert!(
        !has_error(&diags, "E019"),
        "annotating unknown as unknown should be fine"
    );
}

// ── fetch requires try ──────────────────────────────────────

#[test]
fn fetch_requires_try() {
    let diags = check(r#"const res = fetch("https://example.com")"#);
    assert!(
        has_error(&diags, "E014"),
        "calling fetch without try should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn fetch_with_try_is_ok() {
    let diags = check(r#"const res = try fetch("https://example.com")"#);
    assert!(
        !has_error(&diags, "E014"),
        "calling fetch with try should be fine"
    );
}

// ── Member access on unknown ────────────────────────────────

#[test]
fn member_access_on_unknown_is_error() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
const data = getData()
const x = data.name
"#,
    );
    assert!(
        has_error(&diags, "E020"),
        "member access on unknown should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn method_call_on_unknown_is_error() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
const data = getData()
const x = data.toJSON()
"#,
    );
    assert!(
        has_error(&diags, "E020"),
        "method call on unknown should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_member_access_still_works() {
    let diags = check(r#"const x = "hello" |> String.length"#);
    assert!(
        !has_error(&diags, "E020"),
        "stdlib member access should not error"
    );
}

// ── Promise / await ─────────────────────────────────────────

#[test]
fn fetch_returns_promise_response() {
    // fetch returns Promise<Response>, not Response directly
    // So without await, you can't access .json()
    let diags = check(
        r#"
fn test() -> Result<string, Error> {
    const res = try fetch("url")?
    const j = res.json()
    Ok("done")
}
"#,
    );
    // res should be Promise<Response>, so .json() should error
    // (need await to unwrap Promise first)
    assert!(
        has_error_containing(&diags, "Promise"),
        "fetch without await should give Promise<Response>, accessing .json() should error about Promise, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn await_unwraps_promise() {
    // await fetch() should give Response, so .json() works
    let diags = check(
        r#"
fn test() -> Result<string, Error> {
    const res = try await fetch("url")?
    const j = res.json()
    Ok("done")
}
"#,
    );
    assert!(
        !has_error(&diags, "E020"),
        "await should unwrap Promise, allowing .json() access, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn try_without_unwrap_gives_result() {
    // try fetch() without ? should give Result<Promise<Response>, Error>
    let diags = check(
        r#"
fn test() -> Result<string, Error> {
    const res = try fetch("url")
    const val = match res {
        Ok(promise) -> "got promise",
        Err(e) -> e.message,
    }
    Ok(val)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "not defined"),
        "try without ? should give Result, matching on it should work, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

// ── String Literal Unions ───────────────────────────────────

#[test]
fn string_literal_union_exhaustive_match() {
    let diags = check(
        r#"
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

fn _describe(method: HttpMethod) -> string {
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
        !has_error(&diags, "E004"),
        "exhaustive match should not produce error, got: {:?}",
        diags
    );
}

#[test]
fn string_literal_union_missing_variant() {
    let diags = check(
        r#"
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

fn _describe(method: HttpMethod) -> string {
    match method {
        "GET" -> "fetching",
        "POST" -> "creating",
    }
}
"#,
    );
    assert!(
        has_error(&diags, "E004"),
        "missing variants should produce exhaustiveness error, got: {:?}",
        diags
    );
}

#[test]
fn string_literal_union_with_wildcard() {
    let diags = check(
        r#"
type Status = "ok" | "error" | "pending"

fn _handle(s: Status) -> number {
    match s {
        "ok" -> 1,
        _ -> 0,
    }
}
"#,
    );
    assert!(
        !has_error(&diags, "E004"),
        "wildcard should satisfy exhaustiveness, got: {:?}",
        diags
    );
}

// ── Record type composition with spread ──────────────────────

#[test]
fn record_spread_basic() {
    let diags = check(
        r#"
type BaseProps = {
    className: string,
    disabled: boolean,
}

type ButtonProps = {
    ...BaseProps,
    onClick: () -> (),
    label: string,
}

const btn = ButtonProps(className: "btn", disabled: false, onClick: || (), label: "Click")
"#,
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "expected no errors, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_multiple() {
    let diags = check(
        r#"
type A = {
    x: number,
}

type B = {
    y: string,
}

type C = {
    ...A,
    ...B,
    z: boolean,
}

const c = C(x: 1, y: "hello", z: true)
"#,
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "expected no errors, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_conflict_error() {
    let diags = check(
        r#"
type A = {
    name: string,
}

type B = {
    ...A,
    name: number,
}
"#,
    );
    assert!(
        has_error(&diags, "E030"),
        "expected duplicate field error E030, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_union_error() {
    let diags = check(
        r#"
type Status = | Active | Inactive

type Bad = {
    ...Status,
    extra: string,
}
"#,
    );
    assert!(
        has_error(&diags, "E032"),
        "expected spread-of-non-record error E032, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_nested() {
    let diags = check(
        r#"
type A = {
    x: number,
}

type B = {
    ...A,
    y: string,
}

type C = {
    ...B,
    z: boolean,
}

const c = C(x: 1, y: "hello", z: true)
"#,
    );
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "expected no errors for nested spread, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_conflict_between_spreads() {
    let diags = check(
        r#"
type A = {
    name: string,
}

type B = {
    name: string,
}

type C = {
    ...A,
    ...B,
}
"#,
    );
    assert!(
        has_error(&diags, "E031"),
        "expected conflict error E031 between spreads, got: {:?}",
        diags
    );
}

// ── Cross-file spread resolution ────────────────────────────

#[test]
fn cross_file_spread_resolved_via_imports() {
    // Simulate importing a type whose spread was flattened by the resolver.
    // Product originally had `...WithRating`, but the resolver should have
    // flattened it to `{ rating: number, title: string }`.
    use crate::lexer::span::Span;
    use crate::parser::ast::*;
    use crate::resolve::ResolvedImports;

    let dummy_span = Span::new(0, 0, 1, 1);

    // Build a pre-flattened Product type (as the resolver would produce)
    let product_decl = TypeDecl {
        name: "Product".to_string(),
        def: TypeDef::Record(vec![
            RecordEntry::Field(Box::new(RecordField {
                name: "rating".to_string(),
                type_ann: TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "number".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: dummy_span,
                },
                default: None,
                span: dummy_span,
            })),
            RecordEntry::Field(Box::new(RecordField {
                name: "title".to_string(),
                type_ann: TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "string".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: dummy_span,
                },
                default: None,
                span: dummy_span,
            })),
        ]),
        exported: true,
        opaque: false,
        type_params: vec![],
        deriving: vec![],
    };

    let mut imports = std::collections::HashMap::new();
    imports.insert(
        "./types".to_string(),
        ResolvedImports {
            type_decls: vec![product_decl],
            ..ResolvedImports::default()
        },
    );

    let source = r#"
import { Product } from "./types"

const p = Product(rating: 5, title: "Widget")
"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::with_imports(imports).check(&program);
    assert!(
        !diags.iter().any(|d| d.severity == Severity::Error),
        "expected no errors for cross-file spread import, got: {:?}",
        diags
    );
}

// ── Collect Block ───────────────────────────────────────────

#[test]
fn collect_allows_question_without_result_return() {
    // ? inside collect doesn't require the enclosing function to return Result
    let diags = check(
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
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected no errors in collect block, got: {errors:?}"
    );
}

#[test]
fn collect_question_on_non_result_still_errors() {
    let diags = check(
        r#"
fn f() -> Result<number, Array<string>> {
    collect {
        const a = (42)?
        a
    }
}
"#,
    );
    assert!(
        has_error(&diags, "E005"),
        "expected E005 for ? on non-Result, got: {diags:?}"
    );
}

// ── Deriving ────────────────────────────────────────────────

#[test]
fn deriving_eq_is_error() {
    let diags = check(
        r#"
type Point = {
  x: number,
  y: number,
} deriving (Eq)
"#,
    );
    assert!(
        has_error_containing(&diags, "structural equality is built-in"),
        "deriving Eq should error: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn deriving_display_on_record_type() {
    let diags = check(
        r#"
type User = {
  name: string,
  age: number,
} deriving (Display)
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "deriving Display on record should not error: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn deriving_eq_and_display_errors_on_eq() {
    let diags = check(
        r#"
type User = {
  name: string,
  age: number,
} deriving (Eq, Display)
"#,
    );
    assert!(
        has_error_containing(&diags, "structural equality is built-in"),
        "deriving Eq should error even when combined with Display: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn deriving_on_union_type_is_error() {
    let diags = check(
        r#"
type Shape = | Circle(radius: number) | Square(side: number) deriving (Display)
"#,
    );
    assert!(
        has_error_containing(&diags, "can only be used on record types"),
        "deriving on union should error: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn deriving_unknown_trait_is_error() {
    let diags = check(
        r#"
type Point = {
  x: number,
  y: number,
} deriving (Hash)
"#,
    );
    assert!(
        has_error_containing(&diags, "cannot be derived"),
        "deriving unknown trait should error: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Single-variant union newtypes ───────────────────────────

#[test]
fn newtype_with_number() {
    let diags = check("type ProductId = ProductId(number)");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "ProductId(number) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_string() {
    let diags = check("type Email = Email(string)");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "Email(string) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_boolean() {
    let diags = check("type Flag = Flag(boolean)");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "Flag(boolean) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_named_field() {
    let diags = check("type UserId = UserId(value: number)");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "UserId(value: number) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_coexists_with_regular_unions() {
    let diags = check(
        r#"
type ProductId = ProductId(number)
type Route =
  | Home
  | Profile(id: string)
  | NotFound
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "newtypes and regular unions should coexist, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn type_alias_still_works() {
    let diags = check("type Name = string");
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "type aliases should still work, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
