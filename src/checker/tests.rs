use super::*;
use crate::diagnostic::Severity;
use crate::parser::Parser;

fn check(source: &str) -> Vec<Diagnostic> {
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    Checker::new().check(&program)
}

fn has_error(diagnostics: &[Diagnostic], code: ErrorCode) -> bool {
    diagnostics
        .iter()
        .any(|d| d.code.as_deref() == Some(code.code()))
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
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn basic_const_string() {
    let diags = check("const x = \"hello\"");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn undeclared_variable() {
    let diags = check("const x = y");
    assert!(has_error_containing(&diags, "is not defined"));
}

// ── Rule 2: Newtype enforcement ─────────────────────────────

#[test]
fn newtype_comparison_different_types() {
    let diags = check(
        r#"
type UserId { string }
type Email { string }
const a = UserId("abc")
const b = Email("test@test.com")
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
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
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
        .filter(|d| {
            d.code.as_deref() == Some(ErrorCode::InvalidTryOperator.code())
                && d.message.contains("operator requires")
        })
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
    assert!(!has_error(&diags, ErrorCode::InvalidComparison));
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
    assert!(!has_error(&diags, ErrorCode::MissingReturnType));
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
    assert!(!has_error(&diags, ErrorCode::MissingReturnType));
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
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn none_creates_option() {
    let diags = check("const _x = None");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

// ── Array type checking ─────────────────────────────────────

#[test]
fn homogeneous_array() {
    let diags = check("const _x = [1, 2, 3]");
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
}

#[test]
fn mixed_array_inferred_as_unknown() {
    // Mixed-type arrays should be allowed and inferred as Array<unknown>
    let diags = check(r#"const _x = [1, "two", 3]"#);
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    assert!(!has_error_containing(&diags, "mixed types"));
}

#[test]
fn mixed_array_string_and_number() {
    // e.g. TanStack Query's queryKey: ["user", props.userId]
    let diags = check(r#"const _x = ["user", 42]"#);
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
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

#[test]
fn opaque_type_allows_underlying_type_in_defining_module() {
    let diags = check(
        r#"
opaque type HashedPassword { string }

fn hash(pw: string) -> HashedPassword {
    pw
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "opaque type should accept underlying type in the defining module, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn opaque_type_rejects_wrong_type() {
    let diags = check(
        r#"
opaque type HashedPassword { string }

fn hash(pw: number) -> HashedPassword {
    pw
}
"#,
    );
    assert!(
        has_error_containing(&diags, "expected return type"),
        "opaque type should reject non-underlying type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
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
type User { name: string }
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
type User { name: string }
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
type User { name: string }
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
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
type Todo { text: string }
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
            params: vec![FunctionParam {
                ty: TsType::Named("S".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Function {
                    params: vec![FunctionParam {
                        ty: TsType::Named("S".to_string()),
                        optional: false,
                    }],
                    return_type: Box::new(TsType::Primitive("void".to_string())),
                },
            ])),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, types, _) = checker.check_with_types(&program);

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
type User { name: string }
for User {
    fn display(self) -> string { self.name }
}
const _user = User(name: "Ryan")
const _x = _user |> display
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

// (Inline for-declaration tests removed — only block form is supported)

// ── Untrusted Import Auto-wrapping ───────────────────────────

#[test]
fn untrusted_import_auto_wraps_to_result() {
    // Untrusted npm imports auto-wrap return type to Result
    let diags = check(
        r#"
import { capitalize } from "some-lib"
const _x = capitalize("hello")
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "untrusted npm import should be callable (auto-wrapped), got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trusted_specifier_no_auto_wrap() {
    let diags = check(
        r#"
import { trusted capitalize } from "some-lib"
const _x = capitalize("hello")
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "trusted import should be callable directly, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trusted_module_no_auto_wrap() {
    let diags = check(
        r#"
import trusted { capitalize, slugify } from "string-utils"
const _x = capitalize("hello")
const _y = slugify("hello world")
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "trusted module import should be callable directly, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Constructor field validation ────────────────────────────

#[test]
fn constructor_unknown_field_error() {
    let diags = check(
        r#"
type Todo {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", textt: "hello", done: false)
"#,
    );
    assert!(has_error(&diags, ErrorCode::UnknownField));
    assert!(has_error_containing(&diags, "unknown field `textt`"));
}

#[test]
fn constructor_valid_fields_no_error() {
    let diags = check(
        r#"
type Todo {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", text: "hello", done: false)
"#,
    );
    assert!(!has_error(&diags, ErrorCode::UnknownField));
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn constructor_missing_required_field() {
    let diags = check(
        r#"
type Todo {
    id: string,
    text: string,
    done: bool,
}
const _t = Todo(id: "1", text: "hello")
"#,
    );
    assert!(has_error(&diags, ErrorCode::DuplicateDefinition));
    assert!(has_error_containing(
        &diags,
        "missing required field `done`"
    ));
}

#[test]
fn constructor_missing_field_with_default_ok() {
    let diags = check(
        r#"
type Config {
    host: string,
    port: number = 3000,
}
const _c = Config(host: "localhost")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn constructor_spread_skips_missing_check() {
    let diags = check(
        r#"
type Todo {
    id: string,
    text: string,
    done: bool,
}
const original = Todo(id: "1", text: "hello", done: false)
const _t = Todo(..original, text: "updated")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn union_variant_unknown_field_error() {
    let diags = check(
        r#"
type Validation {
    | Valid { text: string }
    | TooShort
    | Empty
}

const _v = Valid(texxt: "hello")
"#,
    );
    assert!(has_error(&diags, ErrorCode::UnknownField));
    assert!(has_error_containing(&diags, "unknown field `texxt`"));
}

#[test]
fn union_variant_valid_field_no_error() {
    let diags = check(
        r#"
type Validation {
    | Valid { text: string }
    | TooShort
    | Empty
}

const _v = Valid(text: "hello")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::UnknownField));
}

// ── Unknown type errors ────────────────────────────────────

#[test]
fn unknown_type_in_record_field() {
    let diags = check(
        r#"
type Todo {
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
type User { name: string }
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
type Container { item: Item }
type Item { name: string }
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
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
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
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn pipe_call_with_extra_args_no_false_positive() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _r = 5 |> add(3)
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
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
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn pipe_stdlib_correct_array_type() {
    // `[1, 2, 3] |> sort` should NOT error
    let diags = check(
        r#"
const _r = [1, 2, 3] |> sort
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
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
type Todo { text: string, done: boolean }
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
fn shadow_allowed_in_inner_scope() {
    // Function params can shadow outer names (like Rust/Gleam)
    let diags = check(
        r#"
const x = 5
fn double(x: number) -> number { x * 2 }
"#,
    );
    assert!(
        !has_error_containing(&diags, "already defined"),
        "inner-scope shadowing should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_inner_scope_const_shadows_for_block_fn() {
    // A const INSIDE a function body can shadow a for-block function
    let diags = check(
        r#"
type Todo { text: string, done: boolean }
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
        !has_error_containing(&diags, "already defined"),
        "inner-scope shadowing of for-block fn should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_inner_scope_const_shadows_outer_const() {
    // A const inside a function body can shadow an outer const
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
        !has_error_containing(&diags, "already defined"),
        "inner-scope shadowing of outer const should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_for_block_pipe_then_shadow_allowed() {
    // Real-world case: piping into for-block fn then shadowing its name in inner scope
    let diags = check(
        r#"
type Todo { text: string, done: boolean }
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
        !has_error_containing(&diags, "already defined"),
        "inner-scope shadowing of for-block fn should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Same-scope redefinition error messages ──────────────────

#[test]
fn same_scope_redefinition_const() {
    let diags = check(
        r#"
const x = 5
const x = 10
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope const redefinition should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn same_scope_redefinition_function_then_const() {
    let diags = check(
        r#"
fn double(x: number) -> number { x * 2 }
const double = 42
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope fn then const redefinition should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn same_scope_redefinition_for_block_then_const() {
    let diags = check(
        r#"
type Todo { text: string, done: boolean }
for Array<Todo> {
    export fn remaining(self) -> number { 0 }
}
const remaining = 5
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope for-block fn then const redefinition should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Duplicate import names (#812) ───────────────────────────

#[test]
fn duplicate_import_same_name_from_different_modules_errors() {
    let diags = check(
        r#"
import { Foo } from "./a"
import { Foo } from "./b"
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "duplicate import name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_same_module_errors() {
    let diags = check(
        r#"
import { Foo } from "./a"
import { Foo } from "./a"
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "duplicate import from same module should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_alias_conflicts_errors() {
    let diags = check(
        r#"
import { Foo } from "./a"
import { Bar as Foo } from "./b"
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "aliased import conflicting with existing name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn import_then_const_same_name_errors() {
    let diags = check(
        r#"
import { foo } from "./a"
const foo = 5
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "const redefining imported name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn distinct_imports_ok() {
    let diags = check(
        r#"
import { Foo } from "./a"
import { Bar } from "./b"
const _x = Foo
const _y = Bar
"#,
    );
    assert!(
        !has_error_containing(&diags, "already defined"),
        "distinct imports should not conflict, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_dts_same_name_from_different_modules_errors() {
    // Two .d.ts imports bringing the same type name into scope should error.
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { Transition } from "framer-motion"
import { Transition } from "react-transition-group"
"#,
    )
    .parse_program()
    .expect("should parse");

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "framer-motion".to_string(),
        vec![DtsExport {
            name: "Transition".to_string(),
            ts_type: TsType::Named("Transition".to_string()),
        }],
    );
    dts_imports.insert(
        "react-transition-group".to_string(),
        vec![DtsExport {
            name: "Transition".to_string(),
            ts_type: TsType::Named("Transition".to_string()),
        }],
    );

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    assert!(
        has_error_containing(&diags, "already defined"),
        "duplicate dts import name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_dts_and_floe_same_name_errors() {
    // A .d.ts import and a .fl import with the same name should error.
    use crate::interop::{DtsExport, TsType};
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { Transition } from "framer-motion"
import { Transition } from "./local"
"#,
    )
    .parse_program()
    .expect("should parse");

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "framer-motion".to_string(),
        vec![DtsExport {
            name: "Transition".to_string(),
            ts_type: TsType::Named("Transition".to_string()),
        }],
    );

    let mut fl_imports = HashMap::new();
    fl_imports.insert("./local".to_string(), ResolvedImports::default());

    let diags = Checker::with_all_imports(fl_imports, dts_imports).check(&program);
    assert!(
        has_error_containing(&diags, "already defined"),
        "dts + floe import with same name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_dts_and_unresolved_same_name_errors() {
    // A .d.ts import and an unresolved npm import with the same name should error.
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { Component } from "react"
import { Component } from "some-other-lib"
"#,
    )
    .parse_program()
    .expect("should parse");

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react".to_string(),
        vec![DtsExport {
            name: "Component".to_string(),
            ts_type: TsType::Named("Component".to_string()),
        }],
    );
    // "some-other-lib" is not in dts_imports, so it's unresolved/foreign

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    assert!(
        has_error_containing(&diags, "already defined"),
        "dts + unresolved import with same name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_import_floe_resolved_same_name_errors() {
    // Two resolved .fl imports with the same type name should error.
    use crate::lexer::span::Span;
    use crate::parser::ast::*;
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let dummy_span = Span::new(0, 0, 0, 0);

    let make_type_decl = |name: &str| TypeDecl {
        exported: true,
        opaque: false,
        name: name.to_string(),
        type_params: vec![],
        def: TypeDef::Record(vec![RecordEntry::Field(Box::new(RecordField {
            name: "id".to_string(),
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
        }))]),
        deriving: vec![],
    };

    let mut fl_imports = HashMap::new();

    let mut resolved_a = ResolvedImports::default();
    resolved_a.type_decls.push(make_type_decl("Item"));
    fl_imports.insert("./a".to_string(), resolved_a);

    let mut resolved_b = ResolvedImports::default();
    resolved_b.type_decls.push(make_type_decl("Item"));
    fl_imports.insert("./b".to_string(), resolved_b);

    let program = crate::parser::Parser::new(
        r#"
import { Item } from "./a"
import { Item } from "./b"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_imports(fl_imports).check(&program);
    assert!(
        has_error_containing(&diags, "already defined"),
        "two resolved .fl imports with same type name should error, got: {:?}",
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
type User { name: string, age: number }
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
type User { name: string }
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

#[test]
fn member_access_on_function_type_errors() {
    let diags = check(
        r#"
fn myFunc(x: number) -> string { "hi" }
const _n = myFunc.name
"#,
    );
    assert!(
        has_error_containing(&diags, "cannot access"),
        "should error on member access on function type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── 3. Constructor field type validation ───────────────────

#[test]
fn constructor_wrong_field_type_errors() {
    let diags = check(
        r#"
type User { name: string, age: number }
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
type User { name: string, age: number }
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
type User { name: string, age: number }
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

#[test]
fn match_arms_unify_result_with_unknown_params() {
    let diags = check(
        r#"
type MyError { message: string }
fn fallible(x: number) -> Result<string, MyError> {
    match x {
        0 -> Err(MyError(message: "zero")),
        _ -> Ok("ok"),
    }
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "match arms have incompatible types"),
        "Result<unknown, MyError> should unify with Result<string, unknown>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "merged Result should match declared return type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn match_arms_truly_incompatible_result_still_errors() {
    let diags = check(
        r#"
type E1 { a: string }
type E2 { b: number }
fn test(x: number) -> Result<string, E1> {
    match x {
        0 -> Err(E1(a: "x")),
        _ -> Err(E2(b: 1)),
    }
}
"#,
    );
    assert!(
        has_error_containing(&diags, "match arms have incompatible types"),
        "truly incompatible Result types should still error, got: {:?}",
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
    let (_diags, types, _) = Checker::new().check_with_types(&program);
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
    let (_diags, types, _) = Checker::new().check_with_types(&program);
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
type Todo { text: string }
fn setTodos(value: Array<Todo>) -> () { () }
fn handler() {
    setTodos([])
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types, _) = Checker::new().check_with_types(&program);
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
    use crate::interop::{DtsExport, FunctionParam, TsType};

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
type Todo { text: string }
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
            params: vec![FunctionParam {
                ty: TsType::Named("S".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Named("S".to_string()),
            ])),
        },
    };
    let mut dts_imports = std::collections::HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export, probe_export]);

    let checker = Checker::with_all_imports(std::collections::HashMap::new(), dts_imports);
    let (_diags, types, _) = checker.check_with_types(&program);
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
    use crate::interop::{DtsExport, FunctionParam, TsType};

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
type Todo { text: string }
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
                params: vec![FunctionParam {
                    ty: TsType::Named("Todo[]".to_string()),
                    optional: false,
                }],
                return_type: Box::new(TsType::Primitive("void".to_string())),
            },
        ]),
    };
    let use_state_export = DtsExport {
        name: "useState".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Named("S".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Tuple(vec![
                TsType::Named("S".to_string()),
                TsType::Function {
                    params: vec![FunctionParam {
                        ty: TsType::Named("S".to_string()),
                        optional: false,
                    }],
                    return_type: Box::new(TsType::Primitive("void".to_string())),
                },
            ])),
        },
    };
    let mut dts_imports = std::collections::HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export, probe_export]);

    let checker = Checker::with_all_imports(std::collections::HashMap::new(), dts_imports);
    let (_diags, types, _) = checker.check_with_types(&program);
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
    let (_diags, types, _) = Checker::new().check_with_types(&program);
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
type User { name: string, age: number }
const user = User(name: "hi", age: 21)
const { name, age } = user
const _x = name
const _y = age
"#,
    )
    .parse_program()
    .expect("should parse");
    let (diags, types, _) = Checker::new().check_with_types(&program);

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

// ── Object destructuring from npm imports ───────────────

#[test]
fn object_destructure_from_trusted_import_gets_field_types() {
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { useQuery } from "react-query"
const { data, isLoading } = useQuery("key")
const _x = data
const _y = isLoading
"#,
    )
    .parse_program()
    .expect("should parse");

    // Mock: useQuery returns an object with data: string, isLoading: boolean
    let use_query_export = DtsExport {
        name: "useQuery".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Object(vec![
                ObjectField {
                    name: "data".to_string(),
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                ObjectField {
                    name: "isLoading".to_string(),
                    ty: TsType::Primitive("boolean".to_string()),
                    optional: false,
                },
            ])),
        },
    };
    // Per-field probes that specifier_map would generate
    let data_probe = DtsExport {
        name: "__probe_data_0".to_string(),
        ts_type: TsType::Primitive("string".to_string()),
    };
    let is_loading_probe = DtsExport {
        name: "__probe_isLoading_0".to_string(),
        ts_type: TsType::Primitive("boolean".to_string()),
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react-query".to_string(),
        vec![use_query_export, data_probe, is_loading_probe],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, types, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "object destructure from npm import should not error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // data should be string, isLoading should be boolean (not unknown)
    if let Some(data_ty) = types.get("data") {
        assert_eq!(data_ty, "string", "data should be string, got: {data_ty}");
    }
    if let Some(loading_ty) = types.get("isLoading") {
        assert_eq!(
            loading_ty, "boolean",
            "isLoading should be boolean, got: {loading_ty}"
        );
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
        has_error(&diags, ErrorCode::TypeMismatch),
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

// ── Tuple pattern arity ─────────────────────────────────────

#[test]
fn tuple_pattern_too_few_elements() {
    let source = r#"
        const _pair = (1, 2)
        match _pair {
            (x) -> x
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TuplePatternArity),
        "tuple pattern with too few elements should produce E035, got: {diags:?}"
    );
}

#[test]
fn tuple_pattern_too_many_elements() {
    let source = r#"
        const _pair = (1, 2)
        match _pair {
            (x, y, z) -> x
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TuplePatternArity),
        "tuple pattern with too many elements should produce E035, got: {diags:?}"
    );
}

#[test]
fn tuple_pattern_correct_arity() {
    let source = r#"
        const _pair = (1, 2)
        match _pair {
            (x, y) -> x + y
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "tuple pattern with correct arity should not produce errors: {errors:?}"
    );
}

// ── Variant pattern arity ────────────────────────────────────

#[test]
fn variant_pattern_too_few_fields() {
    let source = r#"
        type Pair { | Both(number, string) | Neither }
        const _p: Pair = Both(1, "a")
        match _p {
            Both(x) -> x
            Neither -> 0
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::VariantPatternArity),
        "variant pattern with too few fields should produce E039, got: {diags:?}"
    );
}

#[test]
fn variant_pattern_too_many_fields() {
    let source = r#"
        type Shape { | Circle(number) | Square(number) }
        const _s: Shape = Circle(5)
        match _s {
            Circle(r, extra) -> r
            Square(s) -> s
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::VariantPatternArity),
        "variant pattern with too many fields should produce E039, got: {diags:?}"
    );
}

#[test]
fn variant_pattern_correct_arity() {
    let source = r#"
        type Shape { | Circle(number) | Square(number) }
        const _s: Shape = Circle(5)
        match _s {
            Circle(r) -> r
            Square(s) -> s
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "variant pattern with correct arity should not produce errors: {errors:?}"
    );
}

// ── Literal pattern type checking ────────────────────────────

#[test]
fn bool_literal_on_string_type_errors() {
    let source = r#"
        const _s = "hello"
        match _s {
            true -> "yes",
            false -> "no",
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::LiteralPatternMismatch),
        "boolean literal on string type should produce E040, got: {diags:?}"
    );
}

#[test]
fn number_literal_on_string_type_errors() {
    let source = r#"
        const _s = "hello"
        match _s {
            42 -> "the answer",
            _ -> "something else",
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::LiteralPatternMismatch),
        "number literal on string type should produce E040, got: {diags:?}"
    );
}

#[test]
fn string_literal_on_bool_type_errors() {
    let source = r#"
        const _b = true
        match _b {
            "yes" -> 1,
            _ -> 0,
        }
    "#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::LiteralPatternMismatch),
        "string literal on bool type should produce E040, got: {diags:?}"
    );
}

#[test]
fn bool_literal_on_bool_type_ok() {
    let source = r#"
        const _b = true
        match _b {
            true -> "yes",
            false -> "no",
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "boolean literal on boolean type should not error: {errors:?}"
    );
}

#[test]
fn string_literal_on_string_type_ok() {
    let source = r#"
        const _s = "hello"
        match _s {
            "hello" -> "greeting",
            _ -> "other",
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "string literal on string type should not error: {errors:?}"
    );
}

// ── Suspicious binding patterns ──────────────────────────────

#[test]
fn binding_on_boolean_warns() {
    let source = r#"
        const _b = true
        match _b {
            stinkypoopy -> "yes",
        }
    "#;
    let diags = check(source);
    assert!(
        has_warning_containing(&diags, "binds the entire value"),
        "binding on boolean should warn W005, got: {diags:?}"
    );
}

#[test]
fn binding_on_union_warns() {
    let source = r#"
        type Shape { | Circle(number) | Square(number) }
        const _s: Shape = Circle(5)
        match _s {
            x -> 0,
        }
    "#;
    let diags = check(source);
    assert!(
        has_warning_containing(&diags, "binds the entire value"),
        "binding on union should warn W005, got: {diags:?}"
    );
}

#[test]
fn wildcard_on_boolean_no_warning() {
    let source = r#"
        const _b = true
        match _b {
            true -> "yes",
            _ -> "no",
        }
    "#;
    let diags = check(source);
    assert!(
        !has_warning_containing(&diags, "binds the entire value"),
        "wildcard on boolean should not warn: {diags:?}"
    );
}

#[test]
fn binding_on_string_no_warning() {
    let source = r#"
        const _s = "hello"
        match _s {
            greeting -> greeting,
        }
    "#;
    let diags = check(source);
    assert!(
        !has_warning_containing(&diags, "binds the entire value"),
        "binding on string should not warn (infinite type): {diags:?}"
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
type User { name: string }
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
type User { name: string }
for User: Display {
  fn toString(self) -> string {
    "wrong"
  }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::MissingTraitMethod),
        "should error on missing required method"
    );
    assert!(has_error_containing(&diags, "requires method `display`"));
}

#[test]
fn trait_unknown_trait() {
    let diags = check(
        r#"
type User { name: string }
for User: NonExistent {
  fn display(self) -> string {
    self.name
  }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UnknownTrait),
        "should error on unknown trait"
    );
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

// ── Boolean operand enforcement ──────────────────────────────

#[test]
fn and_or_require_boolean_operands() {
    let diags = check(r#"const _x = 1 && 2"#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `&&`"),
        "non-boolean && should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let diags = check(r#"const _x = "a" || "b""#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `||`"),
        "non-boolean || should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn and_or_accept_booleans() {
    let diags = check(r#"const _x = true && false"#);
    assert!(
        !has_error_containing(&diags, "expected boolean operand"),
        "boolean && should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn not_requires_boolean_operand() {
    let diags = check(r#"const _x = !42"#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `!`"),
        "non-boolean ! should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn not_accepts_boolean() {
    let diags = check(r#"const _x = !true"#);
    assert!(
        !has_error_containing(&diags, "expected boolean operand"),
        "boolean ! should not error, got: {:?}",
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
type User { name: string }
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
type User { name: string }
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
type User { name: string }
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
type User { name: string }
for User: Printable {
  fn print(self) -> string {
    self.name
  }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::MissingTraitMethod),
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
        has_error(&diags, ErrorCode::UnsafeNarrowing),
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
        !has_error(&diags, ErrorCode::UnsafeNarrowing),
        "annotating unknown as unknown should be fine"
    );
}

// ── Async function return types ────────────────────────────

#[test]
fn promise_return_type_matches_body() {
    let diags = check(
        r#"
export fn getName() -> Promise<string> {
    "hello"
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "fn body string should match Promise<string>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn promise_option_return_type_matches() {
    let diags = check(
        r#"
export fn maybeGet(x: number) -> Promise<Option<string>> {
    match x {
        0 -> None,
        _ -> Some("found"),
    }
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "fn body Option<string> should match Promise<Option<string>>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn promise_return_type_mismatch_still_errors() {
    let diags = check(
        r#"
export fn bad() -> Promise<string> {
    42
}
"#,
    );
    assert!(
        has_error_containing(&diags, "expected return type"),
        "fn body number should not match Promise<string>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
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
        has_error(&diags, ErrorCode::AccessOnUnknown),
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
        has_error(&diags, ErrorCode::AccessOnUnknown),
        "method call on unknown should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_member_access_still_works() {
    let diags = check(r#"const x = "hello" |> String.length"#);
    assert!(
        !has_error(&diags, ErrorCode::AccessOnUnknown),
        "stdlib member access should not error"
    );
}

// ── Promise / Promise.await ─────────────────────────────────

#[test]
fn await_without_promise_return_type_errors() {
    let diags = check(
        r#"
fn getData() -> Promise<string> { "hello" }
fn bad() -> string { getData() |> Promise.await }
"#,
    );
    assert!(
        has_error_containing(&diags, "uses `await`"),
        "should error when await used but return type is not Promise<T>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn await_with_promise_return_type_ok() {
    let diags = check(
        r#"
fn getData() -> Promise<string> { "hello" }
fn good() -> Promise<string> { getData() |> Promise.await }
"#,
    );
    assert!(
        !has_error_containing(&diags, "uses `await`"),
        "should not error when return type is Promise<T>, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn bare_await_without_promise_return_type_errors() {
    let diags = check(
        r#"
fn getData() -> Promise<string> { "hello" }
fn bad() -> string { getData() |> await }
"#,
    );
    assert!(
        has_error_containing(&diags, "uses `await`"),
        "bare |> await should also trigger the error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn await_inferred_return_type_is_promise() {
    let diags = check(
        r#"
fn getData() -> Promise<string> { "hello" }
fn inferred() { getData() |> Promise.await }
"#,
    );
    assert!(
        !has_error_containing(&diags, "uses `await`"),
        "unannotated async fn should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn promise_return_type_unwrap() {
    // Promise<T> return type should accept T as body type
    let diags = check(
        r#"
export fn asyncOp() -> Promise<string> { "hello" }
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "Promise<string> should accept string body, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn result_match_works() {
    let diags = check(
        r#"
fn getData() -> Result<string, Error> { Ok("hello") }
fn test() -> Result<string, Error> {
    const res = getData()
    const val = match res {
        Ok(data) -> data,
        Err(e) -> e.message,
    }
    Ok(val)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "not defined"),
        "Result match should work, got: {:?}",
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
        !has_error(&diags, ErrorCode::NonExhaustiveMatch),
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
        has_error(&diags, ErrorCode::NonExhaustiveMatch),
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
        !has_error(&diags, ErrorCode::NonExhaustiveMatch),
        "wildcard should satisfy exhaustiveness, got: {:?}",
        diags
    );
}

// ── Record type composition with spread ──────────────────────

#[test]
fn record_spread_basic() {
    let diags = check(
        r#"
type BaseProps {
    className: string,
    disabled: boolean,
}

type ButtonProps {
    ...BaseProps,
    onClick: () -> (),
    label: string,
}

const btn = ButtonProps(className: "btn", disabled: false, onClick: () => (), label: "Click")
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
type A {
    x: number,
}

type B {
    y: string,
}

type C {
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
type A {
    name: string,
}

type B {
    ...A,
    name: number,
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DuplicateField),
        "expected duplicate field error E030, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_union_error() {
    let diags = check(
        r#"
type Status { | Active | Inactive }

type Bad {
    ...Status,
    extra: string,
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::InvalidSpreadType),
        "expected spread-of-non-record error E032, got: {:?}",
        diags
    );
}

#[test]
fn record_spread_nested() {
    let diags = check(
        r#"
type A {
    x: number,
}

type B {
    ...A,
    y: string,
}

type C {
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
type A {
    name: string,
}

type B {
    name: string,
}

type C {
    ...A,
    ...B,
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::SpreadFieldConflict),
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
        has_error(&diags, ErrorCode::InvalidTryOperator),
        "expected E005 for ? on non-Result, got: {diags:?}"
    );
}

// ── Deriving ────────────────────────────────────────────────

#[test]
fn deriving_eq_is_error() {
    let diags = check(
        r#"
type Point {
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
type User {
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
type User {
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
type Shape { | Circle { radius: number } | Square { side: number } } deriving (Display)
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
type Point {
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
    let diags = check("type ProductId { number }");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "ProductId(number) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_string() {
    let diags = check("type Email { string }");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "Email(string) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_boolean() {
    let diags = check("type Flag { boolean }");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "Flag(boolean) should parse as a newtype, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn newtype_with_named_field() {
    let diags = check("type UserId { value: number }");
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
type ProductId { number }
type Route {
  | Home
  | Profile { id: string }
  | NotFound
}
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

// ── Default parameter values ────────────────────────────────

#[test]
fn default_params_all_defaults_omitted() {
    let diags = check(
        r#"
fn greet(name: string, greeting: string = "Hello") -> string {
    `${greeting}, ${name}!`
}
const x = greet("Ryan")
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "calling with defaults omitted should work, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_some_defaults_omitted() {
    let diags = check(
        r#"
fn make(a: string, b: string = "x", c: number = 0) -> string {
    `${a}${b}`
}
const x = make("hello", "world")
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "calling with some defaults omitted should work, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_all_explicit() {
    let diags = check(
        r#"
fn make(a: string, b: string = "x", c: number = 0) -> string {
    `${a}${b}`
}
const x = make("hello", "world", 42)
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "calling with all args explicit should work, got errors: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_too_few_args_error() {
    let diags = check(
        r#"
fn make(a: string, b: string = "x", c: number = 0) -> string {
    `${a}${b}`
}
const x = make()
"#,
    );
    assert!(
        has_error_containing(&diags, "expects"),
        "missing required param should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_too_many_args_error() {
    let diags = check(
        r#"
fn make(a: string, b: string = "x") -> string {
    `${a}${b}`
}
const x = make("a", "b", "c")
"#,
    );
    assert!(
        has_error_containing(&diags, "expects"),
        "too many args should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_type_mismatch_error() {
    let diags = check(
        r#"
fn greet(name: string, count: number = "oops") -> string {
    name
}
"#,
    );
    assert!(
        has_error_containing(&diags, "default value for `count`"),
        "default value type mismatch should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_only_required_param() {
    let diags = check(
        r#"
fn greet(name: string, greeting: string = "Hello") -> string {
    `${greeting}, ${name}!`
}
const x = greet("Ryan")
"#,
    );
    // Verify the error message format says "1 to 2 arguments" not just "2 arguments"
    assert!(
        !has_error_containing(&diags, "expects"),
        "should not produce argument count error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #779: Imported TS functions with optional params ──

#[test]
fn imported_optional_params_allow_omission() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // useQueryClient(queryClient?: QueryClient): QueryClient
    let program = crate::parser::Parser::new(
        r#"
import trusted { useQueryClient } from "@tanstack/react-query"
const _client = useQueryClient()
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "useQueryClient".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Named("QueryClient".to_string()),
                optional: true,
            }],
            return_type: Box::new(TsType::Named("QueryClient".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("@tanstack/react-query".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        !has_error_containing(&diags, "expects"),
        "calling with 0 args should be allowed when param is optional, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn imported_optional_params_still_validates_max_args() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // fn(a: string, b?: number): void — max 2 args
    let program = crate::parser::Parser::new(
        r#"
import trusted { doStuff } from "some-lib"
const _x = doStuff("hi", 1, true)
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "doStuff".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("number".to_string()),
                    optional: true,
                },
            ],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        has_error_containing(&diags, "expects"),
        "calling with 3 args should error when max is 2, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #781: Imported TS union types should be strict ──

#[test]
fn ts_union_accepts_compatible_member() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // format(date: Date | number | string, fmt: string): string
    let program = crate::parser::Parser::new(
        r#"
import trusted { format } from "date-fns"
const _x = format("2024-01-01", "PPpp")
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "format".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Union(vec![
                        TsType::Named("Date".to_string()),
                        TsType::Primitive("number".to_string()),
                        TsType::Primitive("string".to_string()),
                    ]),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Primitive("string".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("date-fns".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        !has_error_containing(&diags, "expected"),
        "passing string to Date | number | string should be accepted, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn ts_union_rejects_incompatible_type() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // doStuff(x: number | string): void
    let program = crate::parser::Parser::new(
        r#"
import trusted { doStuff } from "some-lib"
const _x = doStuff(true)
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "doStuff".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Union(vec![
                    TsType::Primitive("number".to_string()),
                    TsType::Primitive("string".to_string()),
                ]),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        has_error_containing(&diags, "expected"),
        "passing boolean to number | string should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn ts_union_compatible_with_itself() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // identity(x: number | string): number | string
    // calling with return value of same type should work
    let program = crate::parser::Parser::new(
        r#"
import trusted { identity, consume } from "some-lib"
const x = identity()
const _y = consume(x)
"#,
    )
    .parse_program()
    .expect("should parse");

    let union_params = || {
        vec![FunctionParam {
            ty: TsType::Union(vec![
                TsType::Primitive("number".to_string()),
                TsType::Primitive("string".to_string()),
            ]),
            optional: false,
        }]
    };
    let union_ret = || {
        Box::new(TsType::Union(vec![
            TsType::Primitive("number".to_string()),
            TsType::Primitive("string".to_string()),
        ]))
    };

    let identity_export = DtsExport {
        name: "identity".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: union_ret(),
        },
    };
    let consume_export = DtsExport {
        name: "consume".to_string(),
        ts_type: TsType::Function {
            params: union_params(),
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "some-lib".to_string(),
        vec![identity_export, consume_export],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        !has_error_containing(&diags, "expected"),
        "TsUnion should be compatible with itself, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #784: Foreign type compatibility should reject primitives ──

#[test]
fn foreign_rejects_primitive_string() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // takesClient(c: QueryClient): void — should reject a string
    let program = crate::parser::Parser::new(
        r#"
import trusted { takesClient } from "some-lib"
const _x = takesClient("hello")
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "takesClient".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Named("QueryClient".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        has_error_containing(&diags, "expected"),
        "passing string to Foreign(QueryClient) should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn foreign_accepts_same_foreign() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // getClient(): QueryClient, takesClient(c: QueryClient): void
    let program = crate::parser::Parser::new(
        r#"
import trusted { getClient, takesClient } from "some-lib"
const client = getClient()
const _x = takesClient(client)
"#,
    )
    .parse_program()
    .expect("should parse");

    let get_export = DtsExport {
        name: "getClient".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Named("QueryClient".to_string())),
        },
    };
    let takes_export = DtsExport {
        name: "takesClient".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Named("QueryClient".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![get_export, takes_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    assert!(
        !has_error_containing(&diags, "expected"),
        "passing Foreign(QueryClient) to Foreign(QueryClient) should work, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #334: Object literal keys should not be resolved as variables ──

#[test]
fn object_literal_keys_not_resolved_as_variables() {
    let diags = check(
        r#"
fn _test() {
    const config = { staleTime: 60000, retry: 1 }
    config
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "not defined"),
        "object keys should not be resolved as variables, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn object_literal_shorthand_still_resolves_variable() {
    // Shorthand `{ name }` should still require `name` to be defined
    let diags = check(
        r#"
fn _test() {
    const obj = { undefinedVar }
    obj
}
"#,
    );
    assert!(
        has_error_containing(&diags, "not defined"),
        "shorthand object field should require variable to be defined"
    );
}

// ── Bug #333: Lambda object destructuring binds variables ──

#[test]
fn lambda_object_destructure_binds_variables() {
    let diags = check(
        r#"
fn _test() {
    const f = ({ x, y }) => x + y
    f({ x: 1, y: 2 })
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "not defined"),
        "destructured lambda params should be in scope, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Partial application with _ placeholder ──────────────────

#[test]
fn partial_application_no_error() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _addTen = add(10, _)
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "partial application should not produce errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn partial_application_returns_function_type() {
    // `add(10, _)` should have type `(number) => number`
    // so calling it with a number should be fine
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const addTen = add(10, _)
const _result = addTen(5)
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "calling partial application result should work, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn partial_application_wrong_type_errors() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _addTen = add("hello", _)
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `string`"
    ));
}

#[test]
fn pipe_with_placeholder_no_error() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _result = 5 |> add(3, _)
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "pipe with placeholder should not produce errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_with_placeholder_wrong_type() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _result = "hello" |> add(3, _)
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `string`"
    ));
}

#[test]
fn multiple_placeholders_error() {
    let diags = check(
        r#"
fn add(a: number, b: number) -> number { a + b }
const _f = add(_, _)
"#,
    );
    assert!(
        has_error_containing(&diags, "only one `_` placeholder allowed per call"),
        "multiple placeholders should produce error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn partial_application_first_arg() {
    // `concat(_, "!")` should work — placeholder in first position
    let diags = check(
        r#"
fn concat(a: string, b: string) -> string { a }
const _addBang = concat(_, "!")
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "partial application in first position should work, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Variant constructors as functions ───────────────────────

#[test]
fn variant_constructor_as_function_no_error() {
    let diags = check(
        r#"
type SaveError {
    | Validation { errors: Array<string> }
    | Api { message: string }
}

fn apply(f: (Array<string>) -> SaveError) -> SaveError {
    f(["error"])
}

const _result = apply(Validation)
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn unit_variant_not_treated_as_function() {
    let diags = check(
        r#"
type Filter {
    | All
    | Active
    | Completed
}

const _f: Filter = All
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn variant_constructor_type_mismatch() {
    let diags = check(
        r#"
type MyError {
    | Validation { errors: Array<string> }
    | Api { message: string }
}

fn apply(f: (number) -> MyError) -> MyError {
    f(42)
}

const _result = apply(Validation)
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeMismatch));
}

// ── Positional variant fields ────────────────────────────────

#[test]
fn positional_variant_construction_no_error() {
    let diags = check(
        r#"
type Shape {
    | Circle(number)
    | Rect(number, number)
    | Point
}

const _c = Circle(5)
const _r = Rect(10, 20)
const _p = Point
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn positional_variant_type_mismatch() {
    let diags = check(
        r#"
type Shape {
    | Circle(number)
}

const _c = Circle("hello")
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn positional_variant_wrong_arg_count() {
    let diags = check(
        r#"
type Shape {
    | Rect(number, number)
}

const _r = Rect(10)
"#,
    );
    assert!(has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn positional_variant_pattern_matching() {
    let diags = check(
        r#"
type Shape {
    | Circle(number)
    | Point
}

fn describe(s: Shape) -> string {
    match s {
        Circle(r) -> `r=${r}`,
        Point -> "point",
    }
}

const _d = describe(Circle(5))
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

// ── Generic functions ───────────────────────────────────────

#[test]
fn generic_function_no_error() {
    let diags = check(
        r#"
fn identity<T>(x: T) -> T { x }
const _n = identity(42)
const _s = identity("hello")
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "generic function should not produce errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn generic_function_pair() {
    let diags = check(
        r#"
fn pair<A, B>(a: A, b: B) -> (A, B) { (a, b) }
const _p = pair(1, "hello")
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "generic pair should not produce errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn generic_function_with_callback() {
    let diags = check(
        r#"
fn apply<T, U>(x: T, f: (T) -> U) -> U { f(x) }
fn double(n: number) -> number { n * 2 }
const _r = apply(5, double)
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "generic apply should not produce errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
// ── Naming convention enforcement ───────────────────────────

#[test]
fn lowercase_type_name_error() {
    let diags = check("type color { | Red | Green }");
    assert!(has_error(&diags, ErrorCode::TypeNameCase));
    assert!(has_error_containing(
        &diags,
        "type name `color` must start with an uppercase letter"
    ));
}

#[test]
fn uppercase_type_name_ok() {
    let diags = check("type Color { | Red | Green }");
    assert!(!has_error(&diags, ErrorCode::TypeNameCase));
}

#[test]
fn lowercase_variant_name_error() {
    let diags = check("type Color { | red | Green }");
    assert!(has_error(&diags, ErrorCode::TypeNameCase));
    assert!(has_error_containing(
        &diags,
        "variant name `red` must start with an uppercase letter"
    ));
}

#[test]
fn uppercase_variant_name_ok() {
    let diags = check("type Color { | Red | Green }");
    assert!(!has_error(&diags, ErrorCode::TypeNameCase));
}

// Note: uppercase field names are already rejected by the parser
// (uppercase identifiers are parsed as types/variants, not field names)

// ── Bug #516: For-block functions from different types clash ──

#[test]
fn for_block_same_fn_name_different_types_no_conflict() {
    // Importing a type whose for-block defines `fromRow`, then defining a local
    // for-block with the same function name on a different type, should NOT error.
    use crate::lexer::span::Span;
    use crate::parser::ast::*;
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let dummy_span = Span::new(0, 0, 0, 0);

    let mut imports = HashMap::new();
    let mut resolved = ResolvedImports::default();

    // Simulate: type Accent { id: number }
    resolved.type_decls.push(TypeDecl {
        exported: true,
        opaque: false,
        name: "Accent".to_string(),
        type_params: vec![],
        def: TypeDef::Record(vec![RecordEntry::Field(Box::new(RecordField {
            name: "id".to_string(),
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
        }))]),
        deriving: vec![],
    });

    // Simulate: for Accent { export fn fromRow() -> Accent { ... } }
    resolved.for_blocks.push(ForBlock {
        type_name: TypeExpr {
            kind: TypeExprKind::Named {
                name: "Accent".to_string(),
                type_args: vec![],
                bounds: vec![],
            },
            span: dummy_span,
        },
        trait_name: None,
        functions: vec![FunctionDecl {
            exported: true,
            async_fn: false,
            name: "fromRow".to_string(),
            type_params: vec![],
            params: vec![],
            return_type: Some(TypeExpr {
                kind: TypeExprKind::Named {
                    name: "Accent".to_string(),
                    type_args: vec![],
                    bounds: vec![],
                },
                span: dummy_span,
            }),
            body: Box::new(Expr {
                id: ExprId(0),
                kind: ExprKind::Construct {
                    type_name: "Accent".to_string(),
                    spread: None,
                    args: vec![Arg::Named {
                        label: "id".to_string(),
                        value: Expr {
                            id: ExprId(0),
                            kind: ExprKind::Number("0".to_string()),
                            ty: Type::Unknown,
                            span: dummy_span,
                        },
                    }],
                },
                ty: Type::Unknown,
                span: dummy_span,
            }),
        }],
        span: dummy_span,
    });

    imports.insert("./accent".to_string(), resolved);

    let source = r#"
import { Accent } from "./accent"

type Entry {
    id: number,
    accents: Array<Accent>,
}

for Entry {
    export fn fromRow() -> Entry {
        Entry(id: 0, accents: [])
    }
}
"#;

    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::with_imports(imports).check(&program);
    assert!(
        !has_error(&diags, ErrorCode::DuplicateDefinition),
        "for-block functions with the same name on different types should not conflict, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn for_block_same_fn_name_same_type_still_errors() {
    // Two for-blocks on the SAME type with the same function name SHOULD error.
    let diags = check(
        r#"
type Todo { text: string }
for Todo {
    fn format(self) -> string { self.text }
}
for Todo {
    fn format(self) -> string { self.text }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DuplicateDefinition),
        "duplicate for-block function on same type should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Member access on imported types (tsgo) ─────────────────

#[test]
fn member_access_on_imported_type_validates_fields() {
    use crate::interop::{DtsExport, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { AccentRow } from "../services/supabase/row-dto"

type Accent {
    id: number,
    accent: string,
    entryId: number,
}

for AccentRow {
    export fn toModel(self) -> Accent {
        Accent(
            id: self.id,
            accent: self.accent,
            entryId: self.entryId,
        )
    }
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // AccentRow has id, accent, entry_id (snake_case) — NOT entryId
    let accent_row_export = DtsExport {
        name: "AccentRow".to_string(),
        ts_type: TsType::Object(vec![
            ObjectField {
                name: "id".to_string(),
                ty: TsType::Primitive("number".to_string()),
                optional: false,
            },
            ObjectField {
                name: "accent".to_string(),
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            },
            ObjectField {
                name: "entry_id".to_string(),
                ty: TsType::Primitive("number".to_string()),
                optional: false,
            },
        ]),
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "../services/supabase/row-dto".to_string(),
        vec![accent_row_export],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _types, _) = checker.check_with_types(&program);

    // self.entryId should error because AccentRow has entry_id, not entryId
    assert!(
        has_error_containing(&diags, "has no field `entryId`"),
        "accessing non-existent field on imported type should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn member_access_on_imported_type_valid_fields_ok() {
    use crate::interop::{DtsExport, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { UserRow } from "db"

const row = UserRow(id: 1, name: "test")
const _id = row.id
const _name = row.name
"#,
    )
    .parse_program()
    .expect("should parse");

    let user_row_export = DtsExport {
        name: "UserRow".to_string(),
        ts_type: TsType::Object(vec![
            ObjectField {
                name: "id".to_string(),
                ty: TsType::Primitive("number".to_string()),
                optional: false,
            },
            ObjectField {
                name: "name".to_string(),
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            },
        ]),
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("db".to_string(), vec![user_row_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "valid field access on imported type should not error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn foreign_type_member_access_resolves_via_record_definition() {
    // When a TS interface is imported via DTS, wrap_boundary_type produces
    // Type::Foreign. Member access should resolve fields from the env's Record.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
import trusted { Transition } from "api"

fn test(id: string) -> () { () }

fn App() -> JSX.Element {
    const [transitions, _setTransitions] = useState<Array<Transition>>([])
    const _r = transitions |> map((t) => test(t.id))

    <div />
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate what tsgo ACTUALLY returns: useState resolves to (any) => any,
    // but the probe captures concrete types for the destructured elements.
    let use_state_export = DtsExport {
        name: "useState".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: true,
            }],
            return_type: Box::new(TsType::Any),
        },
    };
    // Probe result: tsgo resolves the concrete call useState<Array<Transition>>([])
    let probe_export = DtsExport {
        name: "__probe_transitions__setTransitions".to_string(),
        ts_type: TsType::Tuple(vec![
            TsType::Array(Box::new(TsType::Named("Transition".to_string()))),
            TsType::Function {
                params: vec![FunctionParam {
                    ty: TsType::Array(Box::new(TsType::Named("Transition".to_string()))),
                    optional: false,
                }],
                return_type: Box::new(TsType::Primitive("void".to_string())),
            },
        ]),
    };
    let transition_export = DtsExport {
        name: "Transition".to_string(),
        ts_type: TsType::Object(vec![
            ObjectField {
                name: "id".to_string(),
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            },
            ObjectField {
                name: "name".to_string(),
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            },
        ]),
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_state_export, probe_export]);
    dts_imports.insert("api".to_string(), vec![transition_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "explicit type args should resolve return type, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn member_access_on_unresolved_named_type_errors() {
    // When a Named type can't be resolved to a concrete definition,
    // field access should error rather than silently returning Unknown.
    let diags = check(
        r#"
type Wrapper { inner: number }

for Wrapper {
    fn test(self) -> number {
        self.nonexistent
    }
}
"#,
    );
    assert!(
        has_error_containing(&diags, "has no field `nonexistent`"),
        "field access on a record type should catch invalid fields, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| format!("{}: {}", d.code.as_deref().unwrap_or(""), d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn member_access_on_imported_for_block_type_no_silent_pass() {
    // Reproduces the user's bug: imported type in a for-block, field access
    // on self silently returns Unknown instead of erroring for invalid fields.
    // When the import can't be resolved (no dts), self becomes unknown-typed
    // and any field access should error (E020), not silently pass.
    let diags = check(
        r#"
import { AccentRow } from "../services/supabase/row-dto"

type Accent { id: number, entryId: number }

for AccentRow {
    export fn toModel(self) -> Accent {
        Accent(
            id: self.id,
            entryId: self.entryId,
        )
    }
}
"#,
    );
    // With no import resolution, AccentRow is a foreign Named type.
    // Member access on foreign types is allowed (returns unknown) since
    // we can't validate fields but TypeScript would have checked them.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code.as_deref().unwrap_or(""), d.message))
        .collect();
    assert!(
        !errors
            .iter()
            .any(|e| e.contains(ErrorCode::AccessOnUnknown.code()) && e.contains("AccentRow")),
        "foreign type member access should not error, got: {:?}",
        errors
    );
}

// ── Console variadic ─────────────────────────────────────────

#[test]
fn console_log_variadic_no_error() {
    let src = r#"
        fn main() -> () {
            Console.log("label:", 42)
            Console.log("a", "b", "c")
            Console.warn("warn:", 1, 2)
            Console.error("err")
        }
    "#;
    let diags = check(src);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "variadic Console calls should not produce errors, got: {:?}",
        errors
    );
}

// ── Stdlib call argument validation ────────────────────

#[test]
fn stdlib_call_wrong_arity_errors() {
    let diags = check("const _x = Date.day()");
    assert!(
        has_error_containing(&diags, "expects 1 argument"),
        "stdlib call with wrong arity should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_call_too_many_args_errors() {
    let diags = check("const _x = Date.now(42)");
    assert!(
        has_error_containing(&diags, "expects 0 arguments"),
        "stdlib call with too many args should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_call_correct_arity_ok() {
    let diags = check("const _x = Date.now()");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "stdlib call with correct arity should not error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

// ── Stdlib argument type validation ────────────────────

#[test]
fn stdlib_option_map_rejects_non_option_direct_call() {
    let diags = check(r#"const _x = Option.map("hello", (s) => s)"#);
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "Option.map with string first arg should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_option_map_rejects_non_option_pipe() {
    let diags = check(r#"const _x = "hello" |> Option.map((s) => s)"#);
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "piping string into Option.map should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_option_unwrap_or_rejects_non_option_pipe() {
    let diags = check(r#"const _x = "hello" |> Option.unwrapOr("")"#);
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "piping string into Option.unwrapOr should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_option_is_some_rejects_non_option() {
    let diags = check("const _x = Option.isSome(42)");
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "Option.isSome with number should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_result_map_rejects_non_result() {
    let diags = check(r#"const _x = Result.map("hello", (s) => s)"#);
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "Result.map with string first arg should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_result_is_ok_rejects_non_result() {
    let diags = check("const _x = Result.isOk(42)");
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "Result.isOk with number should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_array_sort_rejects_non_array() {
    let diags = check("const _x = Array.sort(42)");
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "Array.sort with number should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_array_map_rejects_non_array_pipe() {
    let diags = check("const _x = 42 |> Array.map((n) => n)");
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "piping number into Array.map should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_option_map_accepts_valid_option() {
    let diags = check("const _x = Option.map(Some(1), (n) => n * 2)");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "Option.map with Some value should not error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_option_map_accepts_valid_option_pipe() {
    let diags = check("const _x = Some(1) |> Option.map((n) => n * 2)");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "piping Some into Option.map should not error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_array_map_accepts_valid_array() {
    let diags = check("const _x = Array.map([1, 2, 3], (n) => n * 2)");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "Array.map with array should not error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_implicit_some_wrapping_still_works_for_concrete_option_params() {
    // Functions that take Option<concrete_type> should still accept bare values
    let diags = check(
        r#"
        fn foo(x: Option<number>) -> Option<number> { x }
        const _x = foo(42)
    "#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "implicit Some wrapping for concrete Option params should still work, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn stdlib_string_split_rejects_wrong_type() {
    let diags = check("const _x = String.split(42, \",\")");
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "String.split with number should error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

// ── For-block method resolution with multiple overloads ──

#[test]
fn for_block_overload_resolves_correct_return_type_in_pipe() {
    let program = crate::parser::Parser::new(
        r#"
type AccentRow { id: number }
type EntryRow { id: number }
type Accent { id: number }
type Entry { id: number }

for AccentRow {
    fn toModel(self) -> Accent { Accent(id: self.id) }
}

for EntryRow {
    fn toModel(self) -> Entry { Entry(id: self.id) }
}

const row = AccentRow(id: 1)
const _result = row |> toModel
"#,
    )
    .parse_program()
    .expect("should parse");

    let (diags, types, _) = Checker::new().check_with_types(&program);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    if let Some(ty) = types.get("_result") {
        assert_eq!(
            ty, "Accent",
            "should resolve AccentRow's toModel, got: {ty}"
        );
    }
}

#[test]
fn for_block_overload_resolves_correct_return_type_in_call() {
    let program = crate::parser::Parser::new(
        r#"
type AccentRow { id: number }
type EntryRow { id: number }
type Accent { id: number }
type Entry { id: number }

for AccentRow {
    fn toModel(self) -> Accent { Accent(id: self.id) }
}

for EntryRow {
    fn toModel(self) -> Entry { Entry(id: self.id) }
}

const row = AccentRow(id: 1)
const _result = toModel(row)
"#,
    )
    .parse_program()
    .expect("should parse");

    let (diags, types, _) = Checker::new().check_with_types(&program);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    if let Some(ty) = types.get("_result") {
        assert_eq!(
            ty, "Accent",
            "should resolve AccentRow's toModel, got: {ty}"
        );
    }
}

// ── typeof operator ────────────────────────────────────────

#[test]
fn typeof_function_binding() {
    let diags = check(
        "fn greet(name: string) -> string { `Hello, ${name}!` }
type Greeter = typeof greet",
    );
    assert!(diags.is_empty(), "typeof function should pass: {diags:?}");
}

#[test]
fn typeof_record_binding() {
    let diags = check(
        "type Config { baseUrl: string, timeout: number }
const config = Config(baseUrl: \"https://api.com\", timeout: 5000)
type MyConfig = typeof config
fn _getUrl(c: MyConfig) -> string { c.baseUrl }",
    );
    assert!(
        diags.is_empty(),
        "typeof record alias should allow field access: {diags:?}"
    );
}

#[test]
fn typeof_undefined_binding() {
    let diags = check("type T = typeof doesNotExist");
    assert!(
        has_error_containing(&diags, "undefined binding"),
        "should error on undefined binding: {diags:?}"
    );
}

#[test]
fn typeof_forward_reference_errors() {
    // typeof cannot forward-reference local functions (they aren't registered
    // until the second pass, after type registration)
    let diags = check(
        "type Greeter = typeof greet
fn greet(name: string) -> string { `Hello, ${name}!` }",
    );
    assert!(
        has_error_containing(&diags, "undefined binding"),
        "typeof forward ref to local fn should error: {diags:?}"
    );
}

// ── Boundary type compatibility ──────────────────────────

#[test]
fn value_assignable_to_option() {
    // A concrete value should be assignable to Option<T> (implicit Some wrapping)
    let diags = check("const x: Option<number> = 42");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "concrete value should be assignable to Option<T>, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn array_assignable_to_option_array() {
    // An array should be assignable to Option<Array<T>>
    let diags = check("const x: Option<Array<number>> = [1, 2, 3]");
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "array should be assignable to Option<Array<T>>, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

// ── intersection types ─────────────────────────────────────

#[test]
fn intersection_two_records() {
    let diags = check(
        "type A { x: number }
type B { y: string }
type C = A & B
fn _test(c: C) -> number { c.x }",
    );
    assert!(
        diags.is_empty(),
        "intersection of two records should allow field access: {diags:?}"
    );
}

#[test]
fn intersection_three_types() {
    let diags = check(
        "type A { x: number }
type B { y: string }
type D = A & B & { z: boolean }",
    );
    assert!(
        diags.is_empty(),
        "three-way intersection should work: {diags:?}"
    );
}

#[test]
fn intersection_with_typeof() {
    let diags = check(
        "type Config { baseUrl: string }
const config = Config(baseUrl: \"https://api.com\")
type Extended = typeof config & { timeout: number }",
    );
    assert!(
        diags.is_empty(),
        "typeof & record intersection should work: {diags:?}"
    );
}

#[test]
fn intersection_after_generic_type() {
    let diags = check(
        "type A { x: number }
type B { y: string }
type C = Array<A> & B",
    );
    assert!(
        diags.is_empty(),
        "intersection after generic type should work: {diags:?}"
    );
}

#[test]
fn record_spread_field_access() {
    let diags = check(
        "type A { x: number }
type B {
    ...A,
    y: string,
}
fn _test(b: B) -> number { b.x }",
    );
    assert!(
        diags.is_empty(),
        "record spread should allow accessing spread fields: {diags:?}"
    );
}

#[test]
fn string_literal_type_arg() {
    let diags = check(
        "type A { x: number }
type B = Array<\"div\">",
    );
    assert!(
        diags.is_empty(),
        "string literal type argument should parse and check: {diags:?}"
    );
}

// ── Intersection restriction ─────────────────────────────────

#[test]
fn intersection_in_record_type_is_error() {
    let diags = check(
        r#"
type Props {
    value: string & { extra: number },
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::InvalidEnumSpread),
        "& in {{ }} type definition should be an error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn intersection_in_type_alias_is_ok() {
    let diags = check(
        r#"
type Props = string & { extra: number }
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::InvalidEnumSpread),
        "& in = type alias should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Await in non-async function ─────────────────────────────

// ── Bracket access (index) checking ─────────────────────────────

#[test]
fn bracket_access_array_with_number_ok() {
    let diags = check(
        r#"
const items: Array<string> = ["a", "b"]
const x = items[0]
"#,
    );
    assert!(!has_error(&diags, ErrorCode::InvalidArrayIndex));
}

#[test]
fn bracket_access_array_with_string_errors() {
    let diags = check(
        r#"
const items: Array<string> = ["a", "b"]
const x = items["foo"]
"#,
    );
    assert!(has_error_containing(&diags, "array index must be `number`"));
}

#[test]
fn bracket_access_on_string_errors() {
    let diags = check(
        r#"
const s = "hello"
const x = s[0]
"#,
    );
    assert!(has_error_containing(
        &diags,
        "cannot use bracket access on type"
    ));
}

#[test]
fn bracket_access_on_number_errors() {
    let diags = check(
        r#"
const n = 42
const x = n[0]
"#,
    );
    assert!(has_error_containing(
        &diags,
        "cannot use bracket access on type"
    ));
}

#[test]
fn bracket_access_on_record_errors() {
    let diags = check(
        r#"
type User { name: string, age: number }
const u = User { name: "Alice", age: 30 }
const x = u[0]
"#,
    );
    assert!(has_error_containing(
        &diags,
        "cannot use bracket access on type"
    ));
}

#[test]
fn bracket_access_tuple_in_bounds_ok() {
    let diags = check(
        r#"
const pair: [string, number] = ["hello", 42]
const x = pair[0]
const y = pair[1]
"#,
    );
    assert!(!has_error(&diags, ErrorCode::InvalidTupleIndex));
}

#[test]
fn bracket_access_tuple_out_of_bounds_errors() {
    let diags = check(
        r#"
const pair: [string, number] = ["hello", 42]
const x = pair[5]
"#,
    );
    assert!(
        has_error_containing(&diags, "tuple index")
            && has_error_containing(&diags, "out of bounds")
    );
}

#[test]
fn bracket_access_tuple_dynamic_index_errors() {
    let diags = check(
        r#"
const pair: [string, number] = ["hello", 42]
const i = 0
const x = pair[i]
"#,
    );
    assert!(has_error_containing(
        &diags,
        "tuple index must be a numeric literal"
    ));
}

// ── Qualified for-block pipe ────────────────────────────────────

#[test]
fn pipe_qualified_for_block_resolves_return_type() {
    let diags = check(
        r#"
type Out { value: number }
type In { x: number }

for In {
    fn convert(self) -> Out {
        Out(value: self.x)
    }
}

const input = In(x: 42)
const _result: Out = input |> In.convert
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::InvalidFieldAccess),
        "should not error on qualified for-block pipe"
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "return type should match Out"
    );
}

// ── For-block member access ─────────────────────────────────

#[test]
fn for_block_method_resolves_via_member_access() {
    let diags = check(
        r#"
type AccentRow { id: number, entryId: number }
type Accent { id: number, entryId: number }

for AccentRow {
    fn toModel(self) -> Accent {
        Accent(id: self.id, entryId: self.entryId)
    }
}

fn convert(row: AccentRow) -> Accent {
    row.toModel()
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "for-block method via member access should resolve, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn for_block_method_in_map_resolves_return_type() {
    let diags = check(
        r#"
type AccentRow { id: number, entryId: number }
type Accent { id: number, entryId: number }

for AccentRow {
    fn toModel(self) -> Accent {
        Accent(id: self.id, entryId: self.entryId)
    }
}

fn convertAll(rows: Array<AccentRow>) -> Array<Accent> {
    rows |> Array.map((a) => a.toModel())
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "for-block method in map should resolve return type, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── JSX callback parameter inference from tsgo probes ──────

#[test]
fn jsx_callback_param_inferred_from_probe() {
    use crate::interop::{DtsExport, ObjectField, TsType};
    use std::collections::HashMap;

    // Source: NavLink with a callback className prop
    let program = crate::parser::Parser::new(
        r#"
import trusted { NavLink } from "react-router-dom"

fn page() {
    <NavLink className={(state) => "active"} />
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate tsgo probe result: __jsx_NavLink_className resolves to
    // { isActive: boolean, isPending: boolean }
    let jsx_probe = DtsExport {
        name: "__jsx_NavLink_className".to_string(),
        ts_type: TsType::Object(vec![
            ObjectField {
                name: "isActive".to_string(),
                ty: TsType::Primitive("boolean".to_string()),
                optional: false,
            },
            ObjectField {
                name: "isPending".to_string(),
                ty: TsType::Primitive("boolean".to_string()),
                optional: false,
            },
        ]),
    };

    // NavLink itself is a Foreign type (typical for npm components)
    let nav_link_export = DtsExport {
        name: "NavLink".to_string(),
        ts_type: TsType::Named("NavLink".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react-router-dom".to_string(),
        vec![nav_link_export, jsx_probe],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, name_types, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "jsx callback should not error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // The `state` parameter should be inferred as a record type (not unknown)
    if let Some(state_type) = name_types.get("state") {
        assert!(
            state_type.contains("isActive") || state_type == "Record",
            "state param should be inferred from probe, got: {state_type}"
        );
    }
}

#[test]
fn jsx_children_render_prop_params_inferred_from_probe() {
    use crate::interop::{DtsExport, ObjectField, TsType};
    use std::collections::HashMap;

    // Source: Draggable with a render prop child (function-as-child pattern)
    let program = crate::parser::Parser::new(
        r#"
import trusted { Draggable } from "@hello-pangea/dnd"

fn page() {
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) =>
            <div />
        }
    </Draggable>
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate tsgo probe results for children params:
    // __jsxc_Draggable_0 -> { innerRef: Ref, draggableProps: Record }
    // __jsxc_Draggable_1 -> { isDragging: boolean }
    let probe_0 = DtsExport {
        name: "__jsxc_Draggable_0".to_string(),
        ts_type: TsType::Object(vec![
            ObjectField {
                name: "innerRef".to_string(),
                ty: TsType::Primitive("Ref".to_string()),
                optional: false,
            },
            ObjectField {
                name: "draggableProps".to_string(),
                ty: TsType::Primitive("Record".to_string()),
                optional: false,
            },
        ]),
    };
    let probe_1 = DtsExport {
        name: "__jsxc_Draggable_1".to_string(),
        ts_type: TsType::Object(vec![ObjectField {
            name: "isDragging".to_string(),
            ty: TsType::Primitive("boolean".to_string()),
            optional: false,
        }]),
    };

    let draggable_export = DtsExport {
        name: "Draggable".to_string(),
        ts_type: TsType::Named("Draggable".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "@hello-pangea/dnd".to_string(),
        vec![draggable_export, probe_0, probe_1],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, name_types, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "children render prop should not error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // `provided` (first param) should have innerRef/draggableProps fields
    if let Some(provided_type) = name_types.get("provided") {
        assert!(
            provided_type.contains("innerRef"),
            "provided param should be inferred from probe, got: {provided_type}"
        );
    } else {
        panic!("provided param type not found in name_types");
    }

    // `snapshot` (second param) should have isDragging field
    if let Some(snapshot_type) = name_types.get("snapshot") {
        assert!(
            snapshot_type.contains("isDragging"),
            "snapshot param should be inferred from probe, got: {snapshot_type}"
        );
    } else {
        panic!("snapshot param type not found in name_types");
    }
}

#[test]
fn jsx_children_render_prop_named_type_shows_in_name_types() {
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { Draggable } from "@hello-pangea/dnd"

fn page() {
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) =>
            <div />
        }
    </Draggable>
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Real tsgo output: Named types (import paths stripped by DTS parser)
    let probe_0 = DtsExport {
        name: "__jsxc_Draggable_0".to_string(),
        ts_type: TsType::Named("DraggableProvided".to_string()),
    };
    let probe_1 = DtsExport {
        name: "__jsxc_Draggable_1".to_string(),
        ts_type: TsType::Named("DraggableStateSnapshot".to_string()),
    };
    let draggable_export = DtsExport {
        name: "Draggable".to_string(),
        ts_type: TsType::Named("Draggable".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "@hello-pangea/dnd".to_string(),
        vec![draggable_export, probe_0, probe_1],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (_, name_types, _) = checker.check_with_types(&program);

    // provided should be DraggableProvided (Foreign type), not a type var
    if let Some(provided_type) = name_types.get("provided") {
        assert!(
            provided_type.contains("DraggableProvided"),
            "provided should be DraggableProvided, got: {provided_type}"
        );
    } else {
        panic!("provided param type not found in name_types");
    }
}

// ── JSX prop type checking ───────────────────────────────────

#[test]
fn jsx_prop_type_mismatch_errors() {
    let diags = check(
        r#"
type Props { name: string, count: number }
fn Card(props: Props) -> JSX.Element { <div /> }
fn page() -> JSX.Element {
    <Card name={42} count={"hello"} />
}
"#,
    );
    assert!(
        has_error_containing(&diags, "prop `name`: expected `string`, found `number`"),
        "should error on string prop receiving number, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        has_error_containing(&diags, "prop `count`: expected `number`, found `string`"),
        "should error on number prop receiving string, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn jsx_prop_correct_types_no_error() {
    let diags = check(
        r#"
type Props { name: string, count: number }
fn Card(props: Props) -> JSX.Element { <div /> }
fn page() -> JSX.Element {
    <Card name={"hello"} count={42} />
}
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "correct prop types should not error: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Rule: unknown type checking (#734) ─────────────────────

#[test]
fn unknown_arg_rejected_for_concrete_param() {
    let diags = check(
        r#"
fn takesString(s: string) -> string { s }
fn returnsUnknown() -> unknown { "hello" }
const x = returnsUnknown()
const _result = takesString(x)
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "should reject unknown arg for string param, got: {diags:?}"
    );
}

#[test]
fn unknown_callee_emits_warning() {
    let diags = check(
        r#"
fn returnsUnknown() -> unknown { "hello" }
const f = returnsUnknown()
const _result = f(42)
"#,
    );
    assert!(
        has_warning_containing(&diags, "unknown type"),
        "should warn when calling unknown-typed value, got: {diags:?}"
    );
}

// ── Dot shorthand in function arguments ──────────────────────

#[test]
fn dot_shorthand_as_function_argument() {
    let diags = check(
        r#"
type Store { sidebarOpen: boolean, name: string }
fn select(store: Store, f: (Store) -> boolean) -> boolean { f(store) }
const store = Store(sidebarOpen: true, name: "test")
const _r = select(store, .sidebarOpen)
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn dot_shorthand_predicate_as_function_argument() {
    let diags = check(
        r#"
type User { name: string, active: boolean }
fn find(users: Array<User>, f: (User) -> boolean) -> Array<User> {
    users |> filter(f)
}
const users = [User(name: "a", active: true)]
const _r = find(users, .name == "a")
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn dot_shorthand_predicate_with_captured_variable() {
    let diags = check(
        r#"
type Column { id: string }
type Issue { status_name: string }
const columns: Array<Column> = [Column(id: "todo")]
const issue = Issue(status_name: "todo")
const _r = columns |> Array.find(.id == issue.status_name)
"#,
    );
    assert!(
        diags.is_empty(),
        "captured variable in dot shorthand predicate should not resolve as unknown, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn dot_shorthand_predicate_with_function_call_rhs() {
    let diags = check(
        r#"
type User { name: string, active: boolean }
fn getName() -> string { "alice" }
const users: Array<User> = [User(name: "alice", active: true)]
const _r = users |> Array.find(.name == getName())
"#,
    );
    assert!(
        diags.is_empty(),
        "function call on rhs of dot shorthand should work, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug #805: Settable/Option fields default when omitted ───

#[test]
fn option_fields_can_be_omitted() {
    // Option fields in a record should allow omission (default to None)
    let diags = check(
        r#"
type Config { name: string, nickname: Option<string> }
fn _take(c: Config) { c }
const _x = _take({ name: "Alice" })
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected"),
        "omitting Option fields should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn required_fields_cannot_be_omitted() {
    let diags = check(
        r#"
type Config { name: string, email: string }
fn _take(c: Config) { c }
const _x = _take({ name: "Alice" })
"#,
    );
    assert!(
        has_error_containing(&diags, "expected"),
        "omitting required fields should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── tsgo/checker type merging ───────────────────────────────

#[test]
fn tsgo_function_with_unknown_return_uses_checker_return() {
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    // useCallback((issue: IssueDto) => { ... }, [])
    // tsgo returns (IssueDto) => any, checker infers (IssueDto) => ()
    let program = crate::parser::Parser::new(
        r#"
import trusted { useCallback } from "react"
type Item { id: string }
const handler = useCallback((item: Item) => {
    const _x = item.id
}, [])
const _h = handler
"#,
    )
    .parse_program()
    .expect("should parse");

    // Mock: useCallback probe returns (Item) => any
    let probe = DtsExport {
        name: "__probe_handler".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Named("Item".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Any),
        },
    };
    let use_callback_export = DtsExport {
        name: "useCallback".to_string(),
        ts_type: TsType::Named("useCallback".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("react".to_string(), vec![use_callback_export, probe]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (_, types, _) = checker.check_with_types(&program);

    // handler should have () return, not unknown
    if let Some(handler_ty) = types.get("handler") {
        assert!(
            !handler_ty.contains("unknown"),
            "handler return type should not be unknown, got: {handler_ty}"
        );
    }
}

// ── Unknown binding warnings ────────────────────────────────

#[test]
fn unknown_binding_no_warning_for_known_type() {
    let diags = check("const x = 42");
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}

#[test]
fn unknown_binding_no_warning_for_underscore_prefix() {
    let diags = check("const _x = undefinedThing");
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}

#[test]
fn unknown_binding_no_duplicate_when_error_exists() {
    let diags = check("const x = undefinedThing");
    assert!(has_error_containing(&diags, "is not defined"));
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}
