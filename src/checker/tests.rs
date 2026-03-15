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
    assert!(has_error_containing(&diags, "`y` is not defined"));
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
    return Ok(value)
}
"#,
    );
    let unwrap_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("E005") && d.message.contains("? operator requires"))
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
    return Ok(y)
}
"#,
    );
    assert!(has_error_containing(
        &diags,
        "? can only be used on Result or Option"
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
        "cannot access `.value` on Result"
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
    assert!(has_warning_containing(&diags, "is never used"));
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
    assert!(has_error_containing(&diags, "is never used"));
}

// ── Rule 10: Exported function return types ─────────────────

#[test]
fn exported_function_needs_return_type() {
    let diags = check("export fn add(a: number, b: number) { return a }");
    assert!(has_error_containing(&diags, "must declare a return type"));
}

#[test]
fn exported_function_with_return_type_ok() {
    let diags = check("export fn add(a: number, b: number) -> number { return a }");
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
fn mixed_array_error() {
    let diags = check(r#"const _x = [1, "two", 3]"#);
    assert!(has_error_containing(&diags, "mixed array"));
}

// ── Dead code detection ─────────────────────────────────────

#[test]
fn dead_code_after_return() {
    let diags = check(
        r#"
fn test() -> number {
    return 1
    const x = 2
}
"#,
    );
    assert!(has_error_containing(&diags, "unreachable code"));
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
    assert!(has_error_containing(&diags, "unhandled Result"));
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
    assert!(has_error_containing(&diags, "missing field `done`"));
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
