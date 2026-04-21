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
    let diags = check("let x = 42");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn basic_const_string() {
    let diags = check("let x = \"hello\"");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn undeclared_variable() {
    let diags = check("let x = y");
    assert!(has_error_containing(&diags, "is not defined"));
}

// ── Rule 2: Newtype enforcement ─────────────────────────────

#[test]
fn newtype_comparison_different_types() {
    let diags = check(
        r#"
type UserId = UserId(string)
type Email = Email(string)
let a = UserId("abc")
let b = Email("test@test.com")
let result = a == b
"#,
    );
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 4: Exhaustiveness checking ─────────────────────────

#[test]
fn exhaustive_match_with_wildcard() {
    let diags = check(
        r#"
let x = match 42 {
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
let x: boolean = true
let y = match x {
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
let tryFetch(url: string) -> Result<string, string> = {
    let result = Ok("data")
    let value = result?
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
let process() -> Result<number, string> = {
    let x = 42
    let y = x?
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
let result = Ok(42)
let x = result.value
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
    let diags = check("let x = 1 == 1");
    assert!(!has_error(&diags, ErrorCode::InvalidComparison));
}

#[test]
fn equality_different_types() {
    let diags = check(r#"let x = 1 == "hello""#);
    assert!(has_error_containing(&diags, "cannot compare"));
}

// ── Rule 9: Unused detection ────────────────────────────────

#[test]
fn unused_variable_warning() {
    let diags = check("let x = 42");
    assert!(has_warning_containing(&diags, "unused variable"));
}

#[test]
fn underscore_prefix_suppresses_unused() {
    let diags = check("let _x = 42");
    assert!(!has_warning_containing(&diags, "is never used"));
}

#[test]
fn used_variable_no_warning() {
    let diags = check(
        r#"
let x = 42
let y = x
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
    let diags = check("export let add(a: number, b: number) = { a }");
    assert!(has_error_containing(&diags, "must declare a return type"));
}

#[test]
fn exported_function_with_return_type_ok() {
    let diags = check("export let add(a: number, b: number) -> number = { a }");
    assert!(!has_error(&diags, ErrorCode::MissingReturnType));
}

// ── Return type mismatch ─────────────────────────────────────

#[test]
fn return_type_mismatch_errors() {
    let diags = check(
        r#"
let greet() -> string = { 42 }
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
let greet() -> string = { "hello" }
"#,
    );
    assert!(!has_error_containing(&diags, "expected return type"),);
}

#[test]
fn non_exported_function_return_type_not_required() {
    // Non-exported functions can omit -> return type
    let diags = check(
        r#"
let helper(x: number) = { x * 2 }
"#,
    );
    assert!(!has_error(&diags, ErrorCode::MissingReturnType));
}

// ── Rule 12: String concat warning ──────────────────────────

#[test]
fn string_concat_warning() {
    let diags = check(r#"let x = "hello" + " world""#);
    assert!(has_warning_containing(&diags, "template literal"));
}

// ── OK/Err/Some/None types ──────────────────────────────────

#[test]
fn ok_creates_result() {
    let diags = check("let _x = Ok(42)");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn none_creates_option() {
    let diags = check("let _x = None");
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

// ── Array type checking ─────────────────────────────────────

#[test]
fn homogeneous_array() {
    let diags = check("let _x = [1, 2, 3]");
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
}

#[test]
fn mixed_array_inferred_as_unknown() {
    // Mixed-type arrays should be allowed and inferred as Array<unknown>
    let diags = check(r#"let _x = [1, "two", 3]"#);
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
    assert!(!has_error_containing(&diags, "mixed types"));
}

#[test]
fn mixed_array_string_and_number() {
    // e.g. TanStack Query's queryKey: ["user", props.userId]
    let diags = check(r#"let _x = ["user", 42]"#);
    assert!(!has_error(&diags, ErrorCode::NonExhaustiveMatch));
}

// ── Opaque type enforcement ─────────────────────────────────

#[test]
fn opaque_type_cannot_be_constructed() {
    let diags = check(
        r#"
opaque type HashedPassword = string
let _x = HashedPassword("abc")
"#,
    );
    assert!(has_error_containing(&diags, "opaque type"));
}

#[test]
fn opaque_type_allows_underlying_type_in_defining_module() {
    let diags = check(
        r#"
opaque type HashedPassword = HashedPassword(string)

let hash(pw: string) -> HashedPassword = {
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
opaque type HashedPassword = HashedPassword(string)

let hash(pw: number) -> HashedPassword = {
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
type User = { name: string }
for User {
    let display(self) -> string = { self.name }
}
let _x = display(User(name: "Ryan"))
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
    let getName(self) -> string = { self.name }
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
    let greet(self, greeting: string) -> string = { greeting }
}
let _x = greet(User(name: "Ryan"), "Hello")
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
type Todo = { text: string }
let (todos, _setTodos) = useState<Array<Todo>>([])
let _x = todos
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
    let (diags, types, _, _) = checker.check_with_types(&program);

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
    let display(self) -> string = { self.name }
}
let _user = User(name: "Ryan")
let _x = _user |> display
"#,
    );
    assert!(!has_error_containing(&diags, "not defined"));
}

// (Inline for-declaration tests removed — only block form is supported)

// ── Untrusted Import Auto-wrapping ───────────────────────────

#[test]
fn untrusted_import_without_types_warns() {
    let diags = check(
        r#"
import { capitalize } from "some-lib"
let _x = capitalize("hello")
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "calling untyped foreign import should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "foreign callee should produce warning not error, got: {:?}",
        diags
            .iter()
            .map(|d| (&d.severity, &d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn trusted_import_without_types_warns() {
    let diags = check(
        r#"
import { trusted capitalize } from "some-lib"
let _x = capitalize("hello")
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "calling untyped trusted import should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trusted_module_without_types_warns() {
    let diags = check(
        r#"
import trusted { capitalize, slugify } from "string-utils"
let _x = capitalize("hello")
let _y = slugify("hello world")
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "calling untyped trusted module import should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
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
let _t = Todo(id: "1", textt: "hello", done: false)
"#,
    );
    assert!(has_error(&diags, ErrorCode::UnknownField));
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
let _t = Todo(id: "1", text: "hello", done: false)
"#,
    );
    assert!(!has_error(&diags, ErrorCode::UnknownField));
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
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
let _t = Todo(id: "1", text: "hello")
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
type Config = {
    host: string,
    port: number = 3000,
}
let _c = Config(host: "localhost")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
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
let original = Todo(id: "1", text: "hello", done: false)
let _t = Todo(..original, text: "updated")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn union_variant_unknown_field_error() {
    let diags = check(
        r#"
type Validation = | Valid { text: string }
    | TooShort
    | Empty

let _v = Valid(texxt: "hello")
"#,
    );
    assert!(has_error(&diags, ErrorCode::UnknownField));
    assert!(has_error_containing(&diags, "unknown field `texxt`"));
}

#[test]
fn union_variant_valid_field_no_error() {
    let diags = check(
        r#"
type Validation = | Valid { text: string }
    | TooShort
    | Empty

let _v = Valid(text: "hello")
"#,
    );
    assert!(!has_error(&diags, ErrorCode::UnknownField));
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
    let diags = check("let x: Nonexistent = 42");
    assert!(has_error_containing(&diags, "unknown type `Nonexistent`"));
}

#[test]
fn unknown_type_in_function_param() {
    let diags = check("let foo(x: BadType) -> () = {}");
    assert!(has_error_containing(&diags, "unknown type `BadType`"));
}

#[test]
fn unknown_type_in_function_return() {
    let diags = check("let foo() -> BadReturn = { 42 }");
    assert!(has_error_containing(&diags, "unknown type `BadReturn`"));
}

#[test]
fn known_type_no_error() {
    let diags = check(
        r#"
type User = { name: string }
let _u: User = User(name: "Alice")
"#,
    );
    assert!(!has_error_containing(&diags, "unknown type"));
}

#[test]
fn builtin_types_no_error() {
    let diags = check(
        r#"
let _a: number = 42
let _b: string = "hi"
let _c: boolean = true
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
let add(a: number, b: number) -> number = { a + b }
let _r = add("hello", true)
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
let add(a: number, b: number) -> number = { a + b }
let _r = add(1, 2)
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn function_call_wrong_arg_count() {
    let diags = check(
        r#"
let add(a: number, b: number) -> number = { a + b }
let _r = add(1)
"#,
    );
    assert!(
        has_error_containing(&diags, "missing required argument `b`"),
        "expected missing-required diagnostic, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn function_call_too_many_args() {
    let diags = check(
        r#"
let greet(name: string) -> string = { name }
let _r = greet("Alice", "Bob")
"#,
    );
    assert!(
        has_error_containing(&diags, "at most 1 argument, found 2"),
        "expected too-many-args diagnostic, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn duplicate_named_argument_errors() {
    let diags = check(
        r#"
let f(a: number, b: number) -> number = { a + b }
let _r = f(a: 1, a: 2)
"#,
    );
    assert!(
        has_error_containing(&diags, "already provided"),
        "duplicate label should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn positional_and_named_cover_same_slot_errors() {
    // `f(1, a: 2)` covers slot `a` both positionally and by name.
    let diags = check(
        r#"
let f(a: number, b: number) -> number = { a + b }
let _r = f(1, a: 2)
"#,
    );
    assert!(
        has_error_containing(&diags, "already provided"),
        "slot covered twice should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn positional_after_named_errors() {
    let diags = check(
        r#"
let f(a: number, b: number) -> number = { a + b }
let _r = f(a: 1, 2)
"#,
    );
    assert!(
        has_error_containing(&diags, "positional argument after named"),
        "positional-after-named should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn missing_required_slot_via_default_adjacent_named_errors() {
    // `f(1, c: 3)` with `[a, b required, c default]` looks fine by arity
    // (2 args provided, 2 required) but `b` is unprovided. Without the
    // slot-coverage check the `3` would silently land in b's slot.
    let diags = check(
        r#"
let f(a: number, b: number, c: number = 0) -> number = { a + b + c }
let _r = f(1, c: 3)
"#,
    );
    assert!(
        has_error_containing(&diags, "missing required argument `b`"),
        "missing required slot should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn positional_for_defaulted_slot_errors() {
    // A defaulted parameter must be passed by name so skipping earlier
    // defaults can't silently land a value in the wrong slot.
    let diags = check(
        r#"
let send(to: string, body: string, subject: string = "no subject") -> string = { body }
let _r = send("a@b", "body", "override")
"#,
    );
    assert!(
        has_error_containing(&diags, "defaulted parameter `subject`"),
        "positional for defaulted slot should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_call_accounts_for_implicit_arg() {
    let diags = check(
        r#"
let double(x: number) -> number = { x + x }
let _r = 5 |> double
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn pipe_call_with_extra_args_no_false_positive() {
    let diags = check(
        r#"
let add(a: number, b: number) -> number = { a + b }
let _r = 5 |> add(3)
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn pipe_call_wrong_type() {
    let diags = check(
        r#"
let double(x: number) -> number = { x + x }
let _r = "hello" |> double
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
let _r = 5 |> trim
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
let _r = 5 |> sort
"#,
    );
    assert!(has_error_containing(&diags, "found `number`"));
}

#[test]
fn pipe_stdlib_correct_type() {
    // `"hello" |> trim` should NOT error
    let diags = check(
        r#"
let _r = "hello" |> trim
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn pipe_stdlib_correct_array_type() {
    // `[1, 2, 3] |> sort` should NOT error
    let diags = check(
        r#"
let _r = [1, 2, 3] |> sort
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
let x = 5
let x = 10
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_const_shadows_function_errors() {
    // A const shadowing a function name should error
    let diags = check(
        r#"
let double(x: number) -> number = { x * 2 }
let double = 42
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
    export let remaining(self) -> number = { 0 }
}
let remaining = 5
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_function_redefinition_errors() {
    // Defining two functions with the same name should error
    let diags = check(
        r#"
let foo() -> number = { 1 }
let foo() -> string = { "hi" }
"#,
    );
    assert!(has_error_containing(&diags, "already defined"));
}

#[test]
fn shadow_allowed_in_inner_scope() {
    // Function params can shadow outer names (like Rust/Gleam)
    let diags = check(
        r#"
let x = 5
let double(x: number) -> number = { x * 2 }
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
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export let remaining(self) -> number = { 0 }
}
let test() -> number = {
    let remaining = 5
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
let x = 5
let test() -> number = {
    let x = 10
    x
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "already defined"),
        "inner-scope shadowing of outer let should be allowed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn shadow_for_block_pipe_then_shadow_allowed() {
    // Real-world case: piping into for-block fn then shadowing its name in inner scope
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export let remaining(self) -> number = { 0 }
}
let test() -> number = {
    let _todos: Array<Todo> = []
    let remaining = _todos |> remaining
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
let x = 5
let x = 10
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope let redefinition should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn same_scope_redefinition_function_then_const() {
    let diags = check(
        r#"
let double(x: number) -> number = { x * 2 }
let double = 42
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope fn then let redefinition should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn same_scope_redefinition_for_block_then_const() {
    let diags = check(
        r#"
type Todo = { text: string, done: boolean }
for Array<Todo> {
    export let remaining(self) -> number = { 0 }
}
let remaining = 5
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined in this scope"),
        "same-scope for-block fn then let redefinition should error, got: {:?}",
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
let foo = 5
"#,
    );
    assert!(
        has_error_containing(&diags, "already defined"),
        "let redefining imported name should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn distinct_imports_ok() {
    let diags = check(
        r#"
import { Foo } from "./a"
import { Bar } from "./b"
let _x = Foo
let _y = Bar
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
let items = [1, 2, 3]
let target = "hello"
let _x = items |> target
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
let items = [1, 2, 3]
let count = 42
let _x = items |> count
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
let double(x: number) -> number = { x * 2 }
let _r = 5 |> double
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
let u = User(name: "hi", age: 21)
let _n = u.name
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
let u = User(name: "hi")
let _n = u.nonexistent
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
let x = 5
let _n = x.name
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
let myFunc(x: number) -> string = { "hi" }
let _n = myFunc.name
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
type User = { name: string, age: number }
let _u = User(name: 42, age: "old")
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
let _u = User(name: "hi", age: 21)
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
let _u = User(name: "hi")
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
let x = 1
let _y = match x {
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
let x = 1
let _y = match x {
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
type MyError = { message: string }
let fallible(x: number) -> Result<string, MyError> = {
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
type E1 = { a: string }
type E2 = { b: number }
let test(x: number) -> Result<string, E1> = {
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

// ── 4b. Bidirectional inference for Ok/Err ────────────────

#[test]
fn ok_infers_err_type_from_const_annotation() {
    let diags = check(
        r#"
let test() -> () = {
    let _r: Result<number, string> = Ok(42)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected"),
        "Ok should infer err type from let annotation, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn err_infers_ok_type_from_const_annotation() {
    let diags = check(
        r#"
type MyError = | NotFound
let test() -> () = {
    let _r: Result<number, MyError> = Err(NotFound)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected"),
        "Err should infer ok type from let annotation, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ok_infers_err_type_from_function_return() {
    let diags = check(
        r#"
let test() -> Result<number, string> = {
    Ok(42)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "Ok should infer err type from function return, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn err_infers_ok_type_from_function_return() {
    let diags = check(
        r#"
let test() -> Result<number, string> = {
    Err("bad")
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected return type"),
        "Err should infer ok type from function return, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ok_err_in_match_without_return_context_unify() {
    let diags = check(
        r#"
let test() -> () = {
    let _r = match true {
        true -> Ok(42),
        false -> Err("bad"),
    }
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "incompatible"),
        "Ok and Err in match should unify via merge_types, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn const_annotation_preferred_over_function_return_for_ok_err() {
    // When both exist, const annotation should take precedence
    let diags = check(
        r#"
let test() -> () = {
    let _r: Result<number, string> = Ok(42)
    let _s: Result<string, number> = Err(99)
}
"#,
    );
    assert!(
        !has_error_containing(&diags, "expected"),
        "let annotation should provide expected type for Ok/Err, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

#[test]
fn ok_err_mismatch_with_annotation_still_errors() {
    // Ok(42) produces Result<number, _> but annotation expects Result<string, _>
    let diags = check(
        r#"
let test() -> () = {
    let _r: Result<string, string> = Ok(42)
}
"#,
    );
    assert!(
        has_error_containing(&diags, "expected"),
        "type mismatch in Ok value should still error, got: {:?}",
        diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );
}

// ── 5. If/else is banned (parse-level) ────────────────────

#[test]
fn if_else_is_banned() {
    let result = Parser::new("let _x = if true { 1 } else { 2 }").parse_program();
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
let log(msg: string) = { Console.log(msg) }
let _hello = match true {
    true -> log("hi"),
    false -> log("bye"),
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types, _, _) = Checker::new().check_with_types(&program);
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
let log(msg: string) = { Console.log(msg) }
let _result = log("test")
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types, _, _) = Checker::new().check_with_types(&program);
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
let setTodos(value: Array<Todo>) -> () = { () }
let handler() ={
    setTodos([])
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types, _, _) = Checker::new().check_with_types(&program);
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
type Todo = { text: string }
let (todos, setTodos) = useState<Array<Todo>>([])
let handler() ={
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
    let (_diags, types, _, _) = checker.check_with_types(&program);
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
type Todo = { text: string }
let (todos, setTodos) = useState<Array<Todo>>([])
let handler() ={
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
    let (_diags, types, _, _) = checker.check_with_types(&program);
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
let outer() ={
    let inner() = {
        Console.log("hi")
    }
    inner()
}
"#,
    )
    .parse_program()
    .expect("should parse");
    let (_diags, types, _, _) = Checker::new().check_with_types(&program);
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
let user = User(name: "hi", age: 21)
let { name, age } = user
let _x = name
let _y = age
"#,
    )
    .parse_program()
    .expect("should parse");
    let (diags, types, _, _) = Checker::new().check_with_types(&program);

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
let { data, isLoading } = useQuery("key")
let _x = data
let _y = isLoading
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
    let (diags, types, _, _) = checker.check_with_types(&program);

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
    let diags = check("let _p = (1, 2)");
    assert!(
        diags.is_empty(),
        "tuple construction should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_with_type_annotation() {
    let diags = check("let _p: (number, number) = (1, 2)");
    assert!(
        diags.is_empty(),
        "tuple with type annotation should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_type_mismatch() {
    let diags = check(r#"let _p: (number, number) = ("a", "b")"#);
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "tuple type mismatch should produce E001, got: {diags:?}"
    );
}

#[test]
fn tuple_destructuring_infers_types() {
    let source = r#"
        let _pair = (10, "hello")
        let (_x, _y) = _pair
        let _z = _x + 1
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
        export let divmod(a: number, b: number) -> (number, number) = {
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
    let diags = check(r#"let _t = (1, "two", true)"#);
    assert!(
        diags.is_empty(),
        "3-element tuple should not produce errors: {diags:?}"
    );
}

#[test]
fn tuple_return_from_block_inline() {
    // Tuples work inline with function params
    let source = r#"
        export let test(a: number, b: number) -> (number, number) = {
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
        let _pair = (1, 2)
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
        let _pair = (1, 2)
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
        let _pair = (1, 2)
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
        type Pair = | Both(number, string) | Neither
        let _p: Pair = Both(1, "a")
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
        type Shape = | Circle(number) | Square(number)
        let _s: Shape = Circle(5)
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
        type Shape = | Circle(number) | Square(number)
        let _s: Shape = Circle(5)
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

// ── Variant pattern shape (positional vs named) ──────────────

#[test]
fn named_variant_rejects_positional_pattern() {
    let source = r#"
        type Shape = | Rectangle { width: number, height: number }
        let _r: Shape = Rectangle(width: 1, height: 2)
        match _r {
            Rectangle(w, h) -> w + h,
        }
    "#;
    let diags = check(source);
    assert!(
        has_error_containing(&diags, "has named fields"),
        "positional pattern on named variant should produce a shape mismatch error, got: {diags:?}"
    );
}

#[test]
fn positional_variant_rejects_named_pattern() {
    let source = r#"
        type Shape = | Circle(number)
        let _c: Shape = Circle(5)
        match _c {
            Circle { value: r } -> r,
        }
    "#;
    let diags = check(source);
    assert!(
        has_error_containing(&diags, "has positional fields"),
        "named pattern on positional variant should produce a shape mismatch error, got: {diags:?}"
    );
}

#[test]
fn named_variant_accepts_named_pattern() {
    let source = r#"
        type Shape = | Rectangle { width: number, height: number }
        let _r: Shape = Rectangle(width: 1, height: 2)
        let _area = match _r {
            Rectangle { width, height } -> width * height,
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "named pattern on named variant should type-check cleanly: {errors:?}"
    );
}

#[test]
fn named_variant_accepts_named_pattern_with_rename() {
    let source = r#"
        type Shape = | Rectangle { width: number, height: number }
        let _r: Shape = Rectangle(width: 1, height: 2)
        let _area = match _r {
            Rectangle { width: w, height: h } -> w * h,
        }
    "#;
    let diags = check(source);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "named pattern with rename should type-check cleanly: {errors:?}"
    );
}

// ── Literal pattern type checking ────────────────────────────

#[test]
fn bool_literal_on_string_type_errors() {
    let source = r#"
        let _s = "hello"
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
        let _s = "hello"
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
        let _b = true
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
        let _b = true
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
        let _s = "hello"
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
        let _b = true
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
        type Shape = | Circle(number) | Square(number)
        let _s: Shape = Circle(5)
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
        let _b = true
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
        let _s = "hello"
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
let _x = [1, 2, 3] |> tap(Console.log)
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
let _x = [1, 2, 3] |> Pipe.tap(Console.log)
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
  let display(self) -> string
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
  let display(self) -> string
}
type User = { name: string }
for User: Display {
  let display(self) -> string = {
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
  let display(self) -> string
}
type User = { name: string }
for User: Display {
  let toString(self) -> string = {
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
type User = { name: string }
for User: NonExistent {
  let display(self) -> string = {
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
let add(a: number, b: number) -> number = { a + b }

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
    let diags = check(r#"let _x = 1 && 2"#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `&&`"),
        "non-boolean && should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let diags = check(r#"let _x = "a" || "b""#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `||`"),
        "non-boolean || should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn and_or_accept_booleans() {
    let diags = check(r#"let _x = true && false"#);
    assert!(
        !has_error_containing(&diags, "expected boolean operand"),
        "boolean && should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn not_requires_boolean_operand() {
    let diags = check(r#"let _x = !42"#);
    assert!(
        has_error_containing(&diags, "expected boolean operand for `!`"),
        "non-boolean ! should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn not_accepts_boolean() {
    let diags = check(r#"let _x = !true"#);
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
  let eq(self, other: string) -> boolean
  let neq(self, other: string) -> boolean = {
    !(self |> eq(other))
  }
}
type User = { name: string }
for User: Eq {
  let eq(self, other: string) -> boolean = {
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
  let greet(self) -> string = {
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
  let print(self) -> string
  let prettyPrint(self) -> string
}
type User = { name: string }
for User: Printable {
  let print(self) -> string = {
    self.name
  }
  let prettyPrint(self) -> string = {
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
  let print(self) -> string
  let prettyPrint(self) -> string
}
type User = { name: string }
for User: Printable {
  let print(self) -> string = {
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

#[test]
fn trait_impl_missing_self_when_trait_requires_it() {
    let diags = check(
        r#"
trait Display {
  let display(self) -> string
}
type User = { name: string }
for User: Display {
  let display() -> string = {
    "hello"
  }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TraitMethodSignatureMismatch),
        "should error when impl omits self but trait requires it: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(has_error_containing(&diags, "missing `self`"));
}

#[test]
fn trait_impl_has_self_when_trait_does_not() {
    let diags = check(
        r#"
trait Greet {
  let greet(name: string) -> string
}
type User = {}
for User: Greet {
  let greet(self, name: string) -> string = {
    name
  }
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TraitMethodSignatureMismatch),
        "should error when impl adds self but trait does not have it: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(has_error_containing(&diags, "has `self`"));
}

// ── Traits imported from another file (cross-file + #1090) ────

fn resolved_module_with_display_trait() -> ResolvedImports {
    use crate::lexer::span::Span;
    use crate::parser::ast::*;

    let dummy_span = Span::new(0, 0, 0, 0);
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
    resolved
}

#[test]
fn trait_imported_without_for_errors() {
    use std::collections::HashMap;

    let mut imports = HashMap::new();
    imports.insert("./types".to_string(), resolved_module_with_display_trait());

    let source = r#"
import { User, Display } from "./types"

for User: Display {
    let display(self) -> string = {
        self.name
    }
}
"#;

    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::with_imports(imports).check(&program);
    assert!(
        has_error(&diags, ErrorCode::TraitImportWithoutFor),
        "expected TraitImportWithoutFor, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(has_error_containing(
        &diags,
        "trait `Display` must be imported with `import { for Display }`"
    ));
}

#[test]
fn trait_imported_with_for_accepted() {
    use std::collections::HashMap;

    let mut imports = HashMap::new();
    imports.insert("./types".to_string(), resolved_module_with_display_trait());

    let source = r#"
import { User, for Display } from "./types"

for User: Display {
    let display(self) -> string = {
        self.name
    }
}
"#;

    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let diags = Checker::with_imports(imports).check(&program);
    assert!(
        !has_error(&diags, ErrorCode::TraitImportWithoutFor),
        "should not error on trait imported via `for`: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error(&diags, ErrorCode::UnknownTrait),
        "trait should be registered after `for` import: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Bug: Pipe with stdlib member access returns Unknown ─────
// `x |> String.length` should infer as number, not unknown

#[test]
fn pipe_stdlib_member_returns_correct_type() {
    let source = r#"
let len = "hello" |> String.length
let doubled = len + 1
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
let _qc = QueryClient(defaultOptions: {})
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
    let diags = check("let result = fetch(\"https://example.com\")");
    assert!(
        !has_error_containing(&diags, "is not defined"),
        "fetch should be a recognized browser global, but got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn browser_globals_are_recognized() {
    let globals = vec![
        "let w = window",
        "let d = document",
        "let j = JSON.parse(\"{}\")",
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
        "let a = setTimeout",
        "let b = setInterval",
        "let c = clearTimeout",
        "let d = clearInterval",
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
fn narrowing_foreign_call_result_does_not_cascade() {
    // Calling through a foreign callee returns Type::Error (the warning was
    // already emitted at the call site). Assigning the Error-typed result
    // to a `number`-annotated binding should NOT produce a second
    // "unsafe narrowing" error — that would be cascade.
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
let data = getData()
let x: number = data
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "call through foreign callee should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error(&diags, ErrorCode::UnsafeNarrowing),
        "should NOT cascade a narrowing error after the call-site warning"
    );
}

#[test]
fn unknown_to_unknown_annotation_is_ok() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
let data = getData()
let x: unknown = data
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
export let getName() -> Promise<string> = {
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
export let maybeGet(x: number) -> Promise<Option<string>> = {
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
export let bad() -> Promise<string> = {
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
fn member_access_on_foreign_call_result_does_not_cascade() {
    // Calling getData() returns Type::Error (warning emitted at call site).
    // Member access on Error silently propagates Error instead of cascading
    // a second AccessOnUnknown diagnostic.
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
let data = getData()
let x = data.name
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "call through foreign callee should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error(&diags, ErrorCode::AccessOnUnknown),
        "should NOT cascade an access-on-unknown error after the call-site warning"
    );
}

#[test]
fn method_call_on_foreign_call_result_does_not_cascade() {
    let diags = check(
        r#"
import trusted { getData } from "some-lib"
let data = getData()
let x = data.toJSON()
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedForeignArguments),
        "call through foreign callee should warn, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error(&diags, ErrorCode::AccessOnUnknown),
        "should NOT cascade an access-on-unknown error after the call-site warning"
    );
}

#[test]
fn stdlib_member_access_still_works() {
    let diags = check(r#"let x = "hello" |> String.length"#);
    assert!(
        !has_error(&diags, ErrorCode::AccessOnUnknown),
        "stdlib member access should not error"
    );
}

// ── NotCallable ─────────────────────────────────────────────

#[test]
fn calling_non_function_is_error() {
    let diags = check(
        r#"
        let n = 42
        let x = n()
    "#,
    );
    assert!(
        has_error(&diags, ErrorCode::NotCallable),
        "calling a number should error E047, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn calling_record_is_error() {
    let diags = check(
        r#"
        type User = { name: string }
        let u = User(name: "Alice")
        let x = u()
    "#,
    );
    assert!(
        has_error(&diags, ErrorCode::NotCallable),
        "calling a record value should error E047, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn tagged_template_with_non_callable_is_error() {
    let diags = check(
        r#"
        let n = 42
        let x = n`hello`
    "#,
    );
    assert!(
        has_error(&diags, ErrorCode::NotCallable),
        "tagging a non-function should error E047, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn tagged_template_with_function_tag_is_ok() {
    let diags = check(
        r#"
        let tag(strings: Array<string>, values: Array<string>) -> string = { "" }
        let x = tag`hello ${name}`
    "#,
    );
    assert!(
        !has_error(&diags, ErrorCode::NotCallable),
        "tagging a function should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn calling_function_alias_works() {
    let diags = check(
        r#"
        let greet(name: string) -> string = { "hello " + name }
        let f: (string) -> string = greet
        let x = f("Alice")
    "#,
    );
    assert!(
        !has_error(&diags, ErrorCode::NotCallable),
        "calling through a function type alias should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Promise / Promise.await ─────────────────────────────────

#[test]
fn await_without_promise_return_type_errors() {
    let diags = check(
        r#"
let getData() -> Promise<string> = { "hello" }
let bad() -> string = { getData() |> Promise.await }
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
let getData() -> Promise<string> = { "hello" }
let good() -> Promise<string> = { getData() |> Promise.await }
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
let getData() -> Promise<string> = { "hello" }
let bad() -> string = { getData() |> await }
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
let getData() -> Promise<string> = { "hello" }
let inferred() ={ getData() |> Promise.await }
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
export let asyncOp() -> Promise<string> = { "hello" }
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
let getData() -> Result<string, Error> = { Ok("hello") }
let test() -> Result<string, Error> = {
    let res = getData()
    let val = match res {
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

let _describe(method: HttpMethod) -> string = {
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

let _describe(method: HttpMethod) -> string = {
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

let _handle(s: Status) -> number = {
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
type BaseProps = {
    className: string,
    disabled: boolean,
}

type ButtonProps = {
    ...BaseProps,
    onClick: () -> (),
    label: string,
}

let btn = ButtonProps(className: "btn", disabled: false, onClick: () -> (), label: "Click")
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

let c = C(x: 1, y: "hello", z: true)
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
        has_error(&diags, ErrorCode::DuplicateField),
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
        has_error(&diags, ErrorCode::InvalidSpreadType),
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

let c = C(x: 1, y: "hello", z: true)
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

let p = Product(rating: 5, title: "Widget")
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
let f() -> Result<number, Array<string>> = {
    collect {
        let a = (42)?
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
type Shape = | Circle { radius: number } | Square { side: number } deriving (Display)
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
    let diags = check("type UserId = { value: number }");
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
type Route = | Home
  | Profile { id: string }
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
fn type_alias_without_ts_import_is_error() {
    let diags = check("type Name = string");
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "type alias without TS import should error:, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

// ── Default parameter values ────────────────────────────────

#[test]
fn default_params_all_defaults_omitted() {
    let diags = check(
        r#"
let greet(name: string, greeting: string = "Hello") -> string = {
    `${greeting}, ${name}!`
}
let x = greet("Ryan")
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
let make(a: string, b: string = "x", c: number = 0) -> string = {
    `${a}${b}`
}
let x = make("hello", b: "world")
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
let make(a: string, b: string = "x", c: number = 0) -> string = {
    `${a}${b}`
}
let x = make("hello", b: "world", c: 42)
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
let make(a: string, b: string = "x", c: number = 0) -> string = {
    `${a}${b}`
}
let x = make()
"#,
    );
    assert!(
        has_error_containing(&diags, "missing required argument `a`"),
        "missing required param should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn default_params_too_many_args_error() {
    let diags = check(
        r#"
let make(a: string, b: string = "x") -> string = {
    `${a}${b}`
}
let x = make("a", "b", "c")
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
let greet(name: string, count: number = "oops") -> string = {
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
let greet(name: string, greeting: string = "Hello") -> string = {
    `${greeting}, ${name}!`
}
let x = greet("Ryan")
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
let _client = useQueryClient()
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let _x = doStuff("hi", 1, true)
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let _x = format("2024-01-01", "PPpp")
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let _x = doStuff(true)
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let x = identity()
let _y = consume(x)
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let _x = takesClient("hello")
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let client = getClient()
let _x = takesClient(client)
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
let _test() ={
    let config = { staleTime: 60000, retry: 1 }
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
let _test() ={
    let obj = { undefinedVar }
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
let _test() ={
    let f({ x, y }) = x + y
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
let add(a: number, b: number) -> number = { a + b }
let _addTen = add(10, _)
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
let add(a: number, b: number) -> number = { a + b }
let addTen = add(10, _)
let _result = addTen(5)
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
let add(a: number, b: number) -> number = { a + b }
let _addTen = add("hello", _)
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
let add(a: number, b: number) -> number = { a + b }
let _result = 5 |> add(3, _)
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
let add(a: number, b: number) -> number = { a + b }
let _result = "hello" |> add(3, _)
"#,
    );
    assert!(has_error_containing(
        &diags,
        "expected `number`, found `string`"
    ));
}

#[test]
fn multiple_placeholders_build_multi_arg_function() {
    // Regression for #1217. `add3(_, 5, _)` partially applies the middle
    // slot, leaving two open params whose types come from the function's
    // signature. The result is a two-arg function taking (number, number).
    let (diags, types, _, _) = Checker::new().check_with_types(
        &crate::parser::Parser::new(
            r#"
let add3(a: number, b: number, c: number) -> number = { a + b + c }
let _f = add3(_, 5, _)
"#,
        )
        .parse_program()
        .expect("should parse"),
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "multi-placeholder partial application should type-check, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(
        types.get("_f").map(String::as_str),
        Some("(number, number) -> number"),
        "expected two-arg partial, got {:?}",
        types.get("_f")
    );
}

#[test]
fn four_arg_partial_with_three_placeholders() {
    let (diags, types, _, _) = Checker::new().check_with_types(
        &crate::parser::Parser::new(
            r#"
let add4(a: number, b: number, c: number, d: number) -> number = { a + b + c + d }
let _g = add4(_, _, 10, _)
"#,
        )
        .parse_program()
        .expect("should parse"),
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "three-placeholder partial should type-check, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert_eq!(
        types.get("_g").map(String::as_str),
        Some("(number, number, number) -> number"),
    );
}

#[test]
fn partial_application_first_arg() {
    // `concat(_, "!")` should work — placeholder in first position
    let diags = check(
        r#"
let concat(a: string, b: string) -> string = { a }
let _addBang = concat(_, "!")
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
type SaveError = | Validation { errors: Array<string> }
    | Api { message: string }

let apply(f: (Array<string>) -> SaveError) -> SaveError = {
    f(["error"])
}

let _result = apply(Validation)
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn unit_variant_not_treated_as_function() {
    let diags = check(
        r#"
type Filter = | All
    | Active
    | Completed

let _f: Filter = All
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn variant_constructor_type_mismatch() {
    let diags = check(
        r#"
type MyError = | Validation { errors: Array<string> }
    | Api { message: string }

let apply(f: (number) -> MyError) -> MyError = {
    f(42)
}

let _result = apply(Validation)
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeMismatch));
}

// ── Positional variant fields ────────────────────────────────

#[test]
fn positional_variant_construction_no_error() {
    let diags = check(
        r#"
type Shape = | Circle(number)
    | Rect(number, number)
    | Point

let _c = Circle(5)
let _r = Rect(10, 20)
let _p = Point
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn positional_variant_type_mismatch() {
    let diags = check(
        r#"
type Shape = | Circle(number)

let _c = Circle("hello")
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeMismatch));
}

#[test]
fn positional_variant_wrong_arg_count() {
    let diags = check(
        r#"
type Shape = | Rect(number, number)

let _r = Rect(10)
"#,
    );
    assert!(has_error(&diags, ErrorCode::DuplicateDefinition));
}

#[test]
fn positional_variant_pattern_matching() {
    let diags = check(
        r#"
type Shape = | Circle(number)
    | Point

let describe(s: Shape) -> string = {
    match s {
        Circle(r) -> `r=${r}`,
        Point -> "point",
    }
}

let _d = describe(Circle(5))
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

// ── Generic functions ───────────────────────────────────────

#[test]
fn generic_function_no_error() {
    let diags = check(
        r#"
let identity<T>(x: T) -> T = { x }
let _n = identity(42)
let _s = identity("hello")
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
let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }
let _p = pair(1, "hello")
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
let apply<T, U>(x: T, f: (T) -> U) -> U = { f(x) }
let double(n: number) -> number = { n * 2 }
let _r = apply(5, double)
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
    let diags = check("type color = Red | Green");
    assert!(has_error(&diags, ErrorCode::TypeNameCase));
    assert!(has_error_containing(
        &diags,
        "type name `color` must start with an uppercase letter"
    ));
}

#[test]
fn uppercase_type_name_ok() {
    let diags = check("type Color = | Red | Green");
    assert!(!has_error(&diags, ErrorCode::TypeNameCase));
}

#[test]
fn lowercase_variant_name_error() {
    let diags = check("type Color = | red | Green");
    assert!(has_error(&diags, ErrorCode::TypeNameCase));
    assert!(has_error_containing(
        &diags,
        "variant name `red` must start with an uppercase letter"
    ));
}

#[test]
fn uppercase_variant_name_ok() {
    let diags = check("type Color = | Red | Green");
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
        trait_name_span: None,
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
                            ty: (),
                            span: dummy_span,
                        },
                    }],
                },
                ty: (),
                span: dummy_span,
            }),
        }],
        span: dummy_span,
    });

    imports.insert("./accent".to_string(), resolved);

    let source = r#"
import { Accent } from "./accent"

type Entry = {
    id: number,
    accents: Array<Accent>,
}

for Entry {
    export let fromRow() -> Entry = {
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
type Todo = { text: string }
for Todo {
    let format(self) -> string = { self.text }
}
for Todo {
    let format(self) -> string = { self.text }
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

type Accent = {
    id: number,
    accent: string,
    entryId: number,
}

for AccentRow {
    export let toModel(self) -> Accent = {
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
    let (diags, _types, _, _) = checker.check_with_types(&program);

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

let row = UserRow(id: 1, name: "test")
let _id = row.id
let _name = row.name
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
    let (diags, _, _, _) = checker.check_with_types(&program);

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

let test(id: string) -> () = { () }

let App() -> JSX.Element = {
    let (transitions, _setTransitions) = useState<Array<Transition>>([])
    let _r = transitions |> map((t) -> test(t.id))

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
    let (diags, _, _, _) = checker.check_with_types(&program);

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
type Wrapper = { inner: number }

for Wrapper {
    let test(self) -> number = {
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

type Accent = { id: number, entryId: number }

for AccentRow {
    export let toModel(self) -> Accent = {
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
        let main() -> () = {
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
    let diags = check("let _x = Date.day()");
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
    let diags = check("let _x = Date.now(42)");
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
    let diags = check("let _x = Date.now()");
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
    let diags = check(r#"let _x = Option.map("hello", (s) -> s)"#);
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
    let diags = check(r#"let _x = "hello" |> Option.map((s) -> s)"#);
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
    let diags = check(r#"let _x = "hello" |> Option.unwrapOr("")"#);
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
    let diags = check("let _x = Option.isSome(42)");
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
    let diags = check(r#"let _x = Result.map("hello", (s) -> s)"#);
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
    let diags = check("let _x = Result.isOk(42)");
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
    let diags = check("let _x = Array.sort(42)");
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
    let diags = check("let _x = 42 |> Array.map((n) -> n)");
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
    let diags = check("let _x = Option.map(Some(1), (n) -> n * 2)");
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
    let diags = check("let _x = Some(1) |> Option.map((n) -> n * 2)");
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
    let diags = check("let _x = Array.map([1, 2, 3], (n) -> n * 2)");
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
        let foo(x: Option<number>) -> Option<number> = { x }
        let _x = foo(42)
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
    let diags = check("let _x = String.split(42, \",\")");
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
type AccentRow = { id: number }
type EntryRow = { id: number }
type Accent = { id: number }
type Entry = { id: number }

for AccentRow {
    let toModel(self) -> Accent = { Accent(id: self.id) }
}

for EntryRow {
    let toModel(self) -> Entry = { Entry(id: self.id) }
}

let row = AccentRow(id: 1)
let _result = row |> toModel
"#,
    )
    .parse_program()
    .expect("should parse");

    let (diags, types, _, _) = Checker::new().check_with_types(&program);
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
type AccentRow = { id: number }
type EntryRow = { id: number }
type Accent = { id: number }
type Entry = { id: number }

for AccentRow {
    let toModel(self) -> Accent = { Accent(id: self.id) }
}

for EntryRow {
    let toModel(self) -> Entry = { Entry(id: self.id) }
}

let row = AccentRow(id: 1)
let _result = toModel(row)
"#,
    )
    .parse_program()
    .expect("should parse");

    let (diags, types, _, _) = Checker::new().check_with_types(&program);
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
fn typeof_local_binding_is_ok() {
    let diags = check(
        "let greet(name: string) -> string = { `Hello, ${name}!` }
type Greeter = typeof greet",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "typeof on local binding should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn typeof_local_record_binding_is_ok() {
    let diags = check(
        "type Config = { baseUrl: string, timeout: number }
let config = Config(baseUrl: \"https://api.com\", timeout: 5000)
type MyConfig = typeof config",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "typeof on local record binding should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
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
let greet(name: string) -> string = { `Hello, ${name}!` }",
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
    let diags = check("let x: Option<number> = 42");
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
    let diags = check("let x: Option<Array<number>> = [1, 2, 3]");
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
fn intersection_local_types_is_ok() {
    let diags = check(
        "type A = { x: number }
type B = { y: string }
type C = A & B",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "intersection of local types should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn intersection_three_local_types_is_ok() {
    let diags = check(
        "type A = { x: number }
type B = { y: string }
type D = A & B & { z: boolean }",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "three-way local intersection should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn intersection_with_local_typeof_is_ok() {
    let diags = check(
        "type Config = { baseUrl: string }
let config = Config(baseUrl: \"https://api.com\")
type Extended = typeof config & { timeout: number }",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "typeof & local record intersection should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn intersection_local_generic_is_ok() {
    let diags = check(
        "type A = { x: number }
type B = { y: string }
type C = Array<A> & B",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "intersection with local generic should be a valid alias, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn record_spread_field_access() {
    let diags = check(
        "type A = { x: number }
type B = {
    ...A,
    y: string,
}
let _test(b: B) -> number = { b.x }",
    );
    assert!(
        diags.is_empty(),
        "record spread should allow accessing spread fields: {diags:?}"
    );
}

#[test]
fn alias_with_local_type_is_ok() {
    let diags = check(
        "type A = { x: number }
type B = Array<\"div\">",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "alias referencing only local types should be accepted, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

// ── Alias with npm import (positive) ──────────────────

#[test]
fn alias_with_npm_import_is_ok() {
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { ComponentProps } from "react"
type DivProps = ComponentProps<"div">
"#,
    )
    .parse_program()
    .expect("should parse");

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react".to_string(),
        vec![DtsExport {
            name: "ComponentProps".to_string(),
            ts_type: TsType::Named("ComponentProps".to_string()),
        }],
    );

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "alias referencing npm import should not error, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn intersection_with_npm_import_is_ok() {
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { VariantProps } from "tailwind-variants"
type CardProps = VariantProps & { className: string }
"#,
    )
    .parse_program()
    .expect("should parse");

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "tailwind-variants".to_string(),
        vec![DtsExport {
            name: "VariantProps".to_string(),
            ts_type: TsType::Named("VariantProps".to_string()),
        }],
    );

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "intersection with npm import should not error: {diags:?}, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn string_literal_union_errors_with_oneof_fix_it() {
    let diags = check(r#"type ButtonVariant = "outline" | "ghost""#);
    assert!(
        has_error(&diags, ErrorCode::BareStringLiteralUnion),
        "bare string-literal union should fire E201: {diags:?}"
    );
    let help = diags
        .iter()
        .find_map(|d| d.help.clone())
        .unwrap_or_default();
    assert!(
        help.contains(r#"OneOf<"outline", "ghost">"#),
        "E201 help should suggest OneOf<>, got: {help}"
    );
}

// ── TypeScript utility types ─────────────────────────────────

fn assert_utility_type_accepted(src: &str) {
    let diags = check(src);
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "utility-type alias rejected: {diags:?}, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn alias_with_return_type_and_typeof_local_is_ok() {
    assert_utility_type_accepted(
        "let createDb(id: string) -> string = { id }
type Database = ReturnType<typeof createDb>",
    );
}

#[test]
fn alias_with_parameters_is_ok() {
    assert_utility_type_accepted(
        "let createDb(id: string) -> string = { id }
type DbArgs = Parameters<typeof createDb>",
    );
}

#[test]
fn alias_with_partial_over_local_record_is_ok() {
    assert_utility_type_accepted(
        "type User = { name: string, email: string, age: number }
type PartialUser = Partial<User>",
    );
}

#[test]
fn alias_with_readonly_and_non_nullable_is_ok() {
    assert_utility_type_accepted(
        "type User = { name: string }
type ReadOnlyUser = Readonly<User>
type NonNullableUser = NonNullable<User>",
    );
}

#[test]
fn bare_typeof_local_still_errors() {
    let diags = check(
        "let createDb(id: string) -> string = { id }
type Identity = typeof createDb",
    );
    {
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{diags:?}, got: {:?}",
            errors.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn unknown_utility_like_name_still_errors() {
    let diags = check(
        "type User = { name: string }
type Wat = MyCustomUtility<User>",
    );
    assert!(has_error(&diags, ErrorCode::UndefinedName), "{diags:?}");
}

// ── Intersection restriction ─────────────────────────────────

#[test]
fn intersection_in_record_type_is_error() {
    let diags = check(
        r#"
type Props = {
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
let items: Array<string> = ["a", "b"]
let x = items[0]
"#,
    );
    assert!(!has_error(&diags, ErrorCode::InvalidArrayIndex));
}

#[test]
fn bracket_access_array_with_string_errors() {
    let diags = check(
        r#"
let items: Array<string> = ["a", "b"]
let x = items["foo"]
"#,
    );
    assert!(has_error_containing(&diags, "array index must be `number`"));
}

#[test]
fn bracket_access_on_string_errors() {
    let diags = check(
        r#"
let s = "hello"
let x = s[0]
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
let n = 42
let x = n[0]
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
type User = { name: string, age: number }
let u = User(name: "Alice", age: 30)
let x = u[0]
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
let pair: [string, number] = ["hello", 42]
let x = pair[0]
let y = pair[1]
"#,
    );
    assert!(!has_error(&diags, ErrorCode::InvalidTupleIndex));
}

#[test]
fn bracket_access_tuple_out_of_bounds_errors() {
    let diags = check(
        r#"
let pair: [string, number] = ["hello", 42]
let x = pair[5]
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
let pair: [string, number] = ["hello", 42]
let i = 0
let x = pair[i]
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
type Out = { value: number }
type In = { x: number }

for In {
    let convert(self) -> Out = {
        Out(value: self.x)
    }
}

let input = In(x: 42)
let _result: Out = input |> In.convert
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

// ── For-block dot-call errors ────────────────────────────────

#[test]
fn dot_call_on_for_block_method_errors() {
    let diags = check(
        r#"
type User = { name: string }

for User {
    let greet(self) -> string = { `Hello, ${self.name}` }
}

let u = User(name: "Ryan")
let _g = u.greet()
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "dot-call on for-block method should error"
    );
}

#[test]
fn dot_call_on_for_block_method_via_record_field_errors() {
    let diags = check(
        r#"
type AccentRow = { id: number, entryId: number }
type Accent = { id: number, entryId: number }

for AccentRow {
    let toModel(self) -> Accent = {
        Accent(id: self.id, entryId: self.entryId)
    }
}

let convert(row: AccentRow) -> Accent = {
    row.toModel()
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "dot-call on for-block method should error; use pipe syntax instead"
    );
}

#[test]
fn dot_call_on_for_block_method_in_closure_errors() {
    let diags = check(
        r#"
type AccentRow = { id: number, entryId: number }
type Accent = { id: number, entryId: number }

for AccentRow {
    let toModel(self) -> Accent = {
        Accent(id: self.id, entryId: self.entryId)
    }
}

let convertAll(rows: Array<AccentRow>) -> Array<Accent> = {
    rows |> Array.map((a) -> a.toModel())
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "dot-call on for-block method inside closure should error"
    );
}

#[test]
fn pipe_call_on_for_block_method_allowed() {
    let diags = check(
        r#"
type User = { name: string }

for User {
    let greet(self) -> string = { `Hello, ${self.name}` }
}

let u = User(name: "Ryan")
let _g: string = u |> greet
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "pipe syntax for for-block method should not error"
    );
}

#[test]
fn dot_call_on_trait_method_via_generic_bound_errors() {
    // #1169: A trait method reached through a trait-bounded type parameter
    // must still be called via pipe syntax. Previously dot-access slipped
    // through because the receiver's type (`Type::Named("R")`) wasn't
    // recognised by `resolve_for_block_method` and the member access fell
    // through to the foreign/unknown branch.
    let diags = check(
        r#"
trait Repo {
    let create(self, value: string) -> string
}

let use_repo<R: Repo>(r: R, v: string) -> string = {
    r.create(v)
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "dot-call on trait method via generic bound should error; got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_call_on_trait_method_via_generic_bound_allowed() {
    let diags = check(
        r#"
trait Repo {
    let create(self, value: string) -> string
}

let use_repo<R: Repo>(r: R, v: string) -> string = {
    r |> create(v)
}
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::DotCallOnForBlockMethod),
        "pipe syntax for trait method via generic bound should not error; got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── JSX member expressions ──────────────────────────────────

#[test]
fn jsx_member_expression_no_error() {
    let diags = check(
        r#"
import trusted { JSX } from "react"

let Select(_props: { children: JSX.Element }) -> JSX.Element = { <div /> }

let App() -> JSX.Element = {
    <div>
        <Select.Trigger>Open</Select.Trigger>
        <Select.Value />
    </div>
}
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::UndefinedName),
        "JSX member expressions should not produce undefined name errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn jsx_member_expression_marks_root_used() {
    let diags = check(
        r#"
import trusted { JSX } from "react"
import trusted { Select } from "ui"

let App() -> JSX.Element = {
    <Select.Trigger />
}
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::UnusedImport),
        "Select should be marked as used via <Select.Trigger />, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
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

let page() ={
    <NavLink className={(state) -> "active"} />
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
    let (diags, name_types, _, _) = checker.check_with_types(&program);

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

let page() ={
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) ->
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
    let (diags, name_types, _, _) = checker.check_with_types(&program);

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

let page() ={
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) ->
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
    let (_, name_types, _, _) = checker.check_with_types(&program);

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
type Props = { name: string, count: number }
let Card(props: Props) -> JSX.Element = { <div /> }
let page() -> JSX.Element = {
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
type Props = { name: string, count: number }
let Card(props: Props) -> JSX.Element = { <div /> }
let page() -> JSX.Element = {
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
let takesString(s: string) -> string = { s }
let returnsUnknown() -> unknown = { "hello" }
let x = returnsUnknown()
let _result = takesString(x)
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "should reject unknown arg for string param, got: {diags:?}"
    );
}

// ── Unknown callee errors (#1115) ────────────────────────────

#[test]
fn calling_unknown_typed_value_is_error_not_warning() {
    let diags = check(
        r#"
let returnsUnknown() -> unknown = { "hello" }
let f = returnsUnknown()
let _result = f("hello", 42)
"#,
    );
    let unchecked_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.code.as_deref() == Some("E051"))
        .collect();
    assert!(
        !unchecked_diags.is_empty(),
        "calling unknown-typed value should produce error (E051), got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        unchecked_diags
            .iter()
            .all(|d| d.severity == Severity::Error),
        "UncheckedArguments should be an error, not a warning"
    );
}

#[test]
fn narrowing_unknown_call_result_does_not_cascade() {
    let diags = check(
        r#"
let returnsUnknown() -> unknown = { "hello" }
let f = returnsUnknown()
let _result = f()
let x: number = _result
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::UncheckedArguments),
        "call through unknown callee should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        !has_error(&diags, ErrorCode::UnsafeNarrowing),
        "should NOT cascade a narrowing error after the call-site error"
    );
}

// ── Dot shorthand in function arguments ──────────────────────

#[test]
fn dot_shorthand_as_function_argument() {
    let diags = check(
        r#"
type Store = { sidebarOpen: boolean, name: string }
let select(store: Store, f: (Store) -> boolean) -> boolean = { f(store) }
let store = Store(sidebarOpen: true, name: "test")
let _r = select(store, .sidebarOpen)
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn dot_shorthand_predicate_as_function_argument() {
    let diags = check(
        r#"
type User = { name: string, active: boolean }
let find(users: Array<User>, f: (User) -> boolean) -> Array<User> = {
    users |> filter(f)
}
let users = [User(name: "a", active: true)]
let _r = find(users, .name == "a")
"#,
    );
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn dot_shorthand_predicate_with_captured_variable() {
    let diags = check(
        r#"
type Column = { id: string }
type Issue = { status_name: string }
let columns: Array<Column> = [Column(id: "todo")]
let issue = Issue(status_name: "todo")
let _r = columns |> Array.find(.id == issue.status_name)
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
type User = { name: string, active: boolean }
let getName() -> string = { "alice" }
let users: Array<User> = [User(name: "alice", active: true)]
let _r = users |> Array.find(.name == getName())
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
    // Option fields in a record constructor should allow omission (default to None)
    let diags = check(
        r#"
type Config = { name: string, nickname: Option<string> }
let _take(c: Config) = { c }
let _x = _take(Config(name: "Alice"))
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
type Config = { name: string, email: string }
let _take(c: Config) = { c }
let _x = _take({ name: "Alice" })
"#,
    );
    assert!(
        has_error_containing(&diags, "expected"),
        "omitting required fields should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── nominal typing ──────────────────────────────────────────

#[test]
fn different_named_types_with_same_shape_are_incompatible() {
    // Two Floe types with identical fields must not be interchangeable (nominal)
    let diags = check(
        r#"
type Point = { x: number, y: number }
type Vec2 = { x: number, y: number }
let _move(v: Vec2) -> Vec2 = { v }
let p = Point(x: 1, y: 2)
let _r = _move(p)
"#,
    );
    assert!(
        has_error_containing(&diags, "expected `Vec2`, found `Point`"),
        "different named types with same shape should be incompatible, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn inline_object_literal_is_not_assignable_to_named_type() {
    // An inline record literal must not satisfy a Floe Named type (use the constructor)
    let diags = check(
        r#"
type Vec2 = { x: number, y: number }
let _move(v: Vec2) -> Vec2 = { v }
let _r = _move({ x: 1, y: 2 })
"#,
    );
    assert!(
        has_error_containing(&diags, "expected"),
        "inline object literal should not satisfy a named type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn inline_record_annotation_in_signature_errors() {
    let diags = check(
        r#"
type Point = { x: number, y: number }
let _draw(p: { x: number, y: number }) -> () = { () }
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::InlineRecordTypeInSignature),
        "inline record type in signature should fire E202, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn named_type_satisfies_foreign_object_param() {
    // A Floe Named type can be passed to a foreign function expecting an object type.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    // insert(row: { code: string, content: string }): void
    let program = crate::parser::Parser::new(
        r#"
import trusted { insert } from "some-db"
type Row = { code: string, content: string }
let _r = insert(Row(code: "abc", content: "hello"))
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "insert".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Object(vec![
                    ObjectField {
                        name: "code".to_string(),
                        ty: TsType::Primitive("string".to_string()),
                        optional: false,
                    },
                    ObjectField {
                        name: "content".to_string(),
                        ty: TsType::Primitive("string".to_string()),
                        optional: false,
                    },
                ]),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-db".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    assert!(
        diags.is_empty(),
        "Named type should satisfy a foreign object param structurally, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn foreign_record_return_is_not_auto_coerced_to_named_type() {
    // A foreign function returning an object type does NOT auto-satisfy a Floe Named type.
    // The user must explicitly construct the Floe type from the foreign data.
    use crate::interop::{DtsExport, ObjectField, TsType};
    use std::collections::HashMap;

    // getRow(): { code: string, content: string }
    // const r: Row = getRow()  -- should error (foreign record ≠ Floe Row)
    let program = crate::parser::Parser::new(
        r#"
import trusted { getRow } from "some-db"
type Row = { code: string, content: string }
let _take(r: Row) -> () = { () }
let _r = _take(getRow())
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "getRow".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Object(vec![
                ObjectField {
                    name: "code".to_string(),
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                ObjectField {
                    name: "content".to_string(),
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
            ])),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-db".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    assert!(
        has_error_containing(&diags, "expected"),
        "foreign record return should not auto-satisfy a Floe Named type, got: {:?}",
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
type Item = { id: string }
let handler = useCallback((item: Item) -> {
    let _x = item.id
}, [])
let _h = handler
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
    let (_, types, _, _) = checker.check_with_types(&program);

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
    let diags = check("let x = 42");
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}

#[test]
fn unknown_binding_no_warning_for_underscore_prefix() {
    let diags = check("let _x = undefinedThing");
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}

#[test]
fn unknown_binding_no_duplicate_when_error_exists() {
    let diags = check("let x = undefinedThing");
    assert!(has_error_containing(&diags, "is not defined"));
    assert!(!has_warning_containing(&diags, "has type `unknown`"));
}

// ── tsgo required for TS imports ────────────────────────────

#[test]
fn tsgo_missing_emits_error_for_ts_import() {
    let source = r#"import { useJiraStore } from "../../stores/jira-store"
let x = useJiraStore()"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let checker = Checker::from_context(
        HashMap::new(),
        HashMap::new(),
        None,
        HashSet::from(["../../stores/jira-store".to_string()]),
    );
    let diags = checker.check(&program);
    assert!(
        has_error(&diags, ErrorCode::TsgoNotFound),
        "should emit TsgoNotFound error, got: {diags:?}"
    );
}

#[test]
fn tsgo_missing_no_error_for_npm_import() {
    let source = r#"import { useState } from "react""#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    // "react" is not in ts_imports_missing_tsgo, so no error
    let checker = Checker::from_context(
        HashMap::new(),
        HashMap::new(),
        None,
        HashSet::from(["../../stores/jira-store".to_string()]),
    );
    let diags = checker.check(&program);
    assert!(
        !has_error(&diags, ErrorCode::TsgoNotFound),
        "should not emit TsgoNotFound for npm imports, got: {diags:?}"
    );
}

// ── Export not found (#976) ─────────────────────────────────

#[test]
fn named_import_not_in_resolved_fl_module_errors() {
    use crate::lexer::span::Span;
    use crate::parser::ast::*;
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let dummy_span = Span::new(0, 0, 0, 0);

    // Module "./utils" exports type `User` but not `Admin`
    let mut resolved = ResolvedImports::default();
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

    let mut fl_imports = HashMap::new();
    fl_imports.insert("./utils".to_string(), resolved);

    let program = crate::parser::Parser::new(
        r#"
import { User, Admin } from "./utils"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_imports(fl_imports).check(&program);
    assert!(
        has_error(&diags, ErrorCode::ExportNotFound),
        "importing non-existent export from .fl module should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(has_error_containing(&diags, "has no export named `Admin`"));
}

#[test]
fn named_import_found_in_resolved_fl_module_ok() {
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let mut resolved = ResolvedImports::default();
    resolved.const_names.push("API_URL".to_string());

    let mut fl_imports = HashMap::new();
    fl_imports.insert("./config".to_string(), resolved);

    let program = crate::parser::Parser::new(
        r#"
import { API_URL } from "./config"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_imports(fl_imports).check(&program);
    assert!(
        !has_error(&diags, ErrorCode::ExportNotFound),
        "importing existing let should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn named_import_not_in_dts_exports_errors() {
    use crate::interop;
    use std::collections::HashMap;

    // "react-markdown" only has a default export, no named "Markdown" export
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react-markdown".to_string(),
        vec![interop::DtsExport {
            name: "default".to_string(),
            ts_type: interop::TsType::Any,
        }],
    );

    let program = crate::parser::Parser::new(
        r#"
import { Markdown } from "react-markdown"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    assert!(
        has_error(&diags, ErrorCode::ExportNotFound),
        "importing non-existent named export from npm package should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(has_error_containing(
        &diags,
        "has no export named `Markdown`"
    ));
}

#[test]
fn named_import_found_in_dts_exports_ok() {
    use crate::interop;
    use std::collections::HashMap;

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "react".to_string(),
        vec![interop::DtsExport {
            name: "useState".to_string(),
            ts_type: interop::TsType::Function {
                params: vec![],
                return_type: Box::new(interop::TsType::Any),
            },
        }],
    );

    let program = crate::parser::Parser::new(
        r#"
import trusted { useState } from "react"
let _x = useState()
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_all_imports(HashMap::new(), dts_imports).check(&program);
    assert!(
        !has_error(&diags, ErrorCode::ExportNotFound),
        "importing existing named export from npm should not error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn unresolved_npm_import_no_false_positive() {
    // When neither .fl nor .d.ts resolution is available, we can't verify
    // exports — should NOT error (fallback to Foreign type).
    let program = crate::parser::Parser::new(
        r#"
import { Something } from "unknown-package"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::new().check(&program);
    assert!(
        !has_error(&diags, ErrorCode::ExportNotFound),
        "unresolved npm import should not produce ExportNotFound, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Chained method calls on foreign types ──────────────────

#[test]
fn chained_method_calls_on_foreign_type_propagate_without_error() {
    // Simulates: db.insert(snippets).values({ code: "abc" }).returning()
    // When db.insert resolves to a function returning a Foreign type,
    // subsequent .values() and .returning() should NOT produce E025.
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { db } from "drizzle"
import trusted { snippets } from "./schema"

let example() -> () = {
    let _result = db.insert(snippets).values(snippets).returning()
    ()
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // db.insert resolves to a function returning a Foreign type
    let db_export = DtsExport {
        name: "db".to_string(),
        ts_type: TsType::Named("BetterSQLite3Database".to_string()),
    };
    let member_probe = DtsExport {
        name: "__member_db_insert".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("SQLiteInsertBase".to_string())),
        },
    };
    let snippets_export = DtsExport {
        name: "snippets".to_string(),
        ts_type: TsType::Named("SQLiteTable".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("drizzle".to_string(), vec![db_export, member_probe]);
    dts_imports.insert("./schema".to_string(), vec![snippets_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "chained method calls on foreign types should not produce errors, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn calling_foreign_type_returns_foreign_not_unknown() {
    // When a Foreign type is called (e.g. a builder method returned from npm),
    // the result should stay Foreign so subsequent member access works.
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { createBuilder } from "some-lib"

let test() -> () = {
    let builder = createBuilder()
    let _result = builder.step1().step2().finish()
    ()
}
"#,
    )
    .parse_program()
    .expect("should parse");

    let export = DtsExport {
        name: "createBuilder".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Named("Builder".to_string())),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "calling a Foreign type should return Foreign (not unknown), got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn chain_probe_resolves_real_types_for_builder_pattern() {
    // When chain probes (__chain_db$insert$values) are present in DTS imports,
    // the checker should resolve real types at each step of the chain instead
    // of falling back to Foreign.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { db } from "drizzle"
import trusted { snippets } from "./schema"

let example() -> () = {
    let _result = db.insert(snippets).values(snippets).returning()
    ()
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // db is a Foreign type, db.insert is a function via __member_ probe
    let db_export = DtsExport {
        name: "db".to_string(),
        ts_type: TsType::Named("BetterSQLite3Database".to_string()),
    };
    let member_probe = DtsExport {
        name: "__member_db_insert".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("SQLiteInsertBase".to_string())),
        },
    };
    // Chain probe: .values on the return of db.insert() is a function returning InsertWithValues
    let chain_values = DtsExport {
        name: "__chain_db$insert$values".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Object(vec![ObjectField {
                    name: "code".to_string(),
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                }]),
                optional: false,
            }],
            return_type: Box::new(TsType::Named("InsertWithValues".to_string())),
        },
    };
    // Chain probe: .returning on the return of .values() is a function returning Promise
    let chain_returning = DtsExport {
        name: "__chain_db$insert$values$returning".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Promise".to_string(),
                args: vec![TsType::Any],
            }),
        },
    };
    let snippets_export = DtsExport {
        name: "snippets".to_string(),
        ts_type: TsType::Named("SQLiteTable".to_string()),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "drizzle".to_string(),
        vec![db_export, member_probe, chain_values, chain_returning],
    );
    dts_imports.insert("./schema".to_string(), vec![snippets_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "chain probes should resolve real types for builder pattern, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn chain_probe_deep_chain_resolves() {
    // Test that chain probes work for 4+ levels of chaining.
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { query } from "query-builder"

let example() -> () = {
    let _result = query.select(query).from(query).where(query).limit(query)
    ()
}
"#,
    )
    .parse_program()
    .expect("should parse");

    let query_export = DtsExport {
        name: "query".to_string(),
        ts_type: TsType::Named("QueryBuilder".to_string()),
    };
    let member_select = DtsExport {
        name: "__member_query_select".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("QB".to_string())),
        },
    };
    let chain_from = DtsExport {
        name: "__chain_query$select$from".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("QB".to_string())),
        },
    };
    let chain_where = DtsExport {
        name: "__chain_query$select$from$where".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("QB".to_string())),
        },
    };
    let chain_limit = DtsExport {
        name: "__chain_query$select$from$where$limit".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("QB".to_string())),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "query-builder".to_string(),
        vec![
            query_export,
            member_select,
            chain_from,
            chain_where,
            chain_limit,
        ],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "deep chain probes (4+ levels) should resolve without errors, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn chain_call_probe_resolves_overloaded_method_return_type() {
    // When a chainable method has overloaded signatures (e.g. Hono's
    // `HonoRequest.param`), the raw `__chain_{key}` probe exposes every
    // overload as an object with multiple call signatures — and Floe's DTS
    // parser keeps only the first, picking the wrong one. A separate
    // `__chain_call_{key}` probe captures the CALL RESULT with `null! as any`
    // as the argument, letting tsgo pick the matching overload, and the
    // checker prefers it in `check_call`.
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { Context } from "hono"

let needsString(s: string) -> string = { s }

export let handler(c: Context<unknown>) -> string = {
    match c.req.param("code") {
        None -> "missing",
        Some(v) -> needsString(v),
    }
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Simulate tsgo emitting a `__chain_call_Context$req$param` probe that
    // captures the result of calling `.param(null! as any)` — tsgo picks
    // overload 2 (`(key: string): string | undefined`) and Floe wraps the
    // resulting `string | undefined` to `Option<string>` at the boundary.
    let context_export = DtsExport {
        name: "Context".to_string(),
        ts_type: TsType::Any,
    };
    let chain_call_probe = DtsExport {
        name: "__chain_call_Context$req$param".to_string(),
        ts_type: TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Undefined,
        ]),
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("hono".to_string(), vec![context_export, chain_call_probe]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "overloaded chain call should resolve via __chain_call_ probe, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn chain_probe_resolves_for_fl_alias_type_param() {
    // When a function parameter is typed as a Floe type alias (e.g. db: Database
    // where `type Database = DrizzleType`), chain probes keyed by type name should resolve
    // even though the checker knows the type definition.
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use crate::lexer::span::Span;
    use crate::parser::ast::{TypeDecl, TypeDef, TypeExpr, TypeExprKind};
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    let dummy_span = Span::new(0, 0, 0, 0);

    let program = crate::parser::Parser::new(
        r#"
import { Database } from "./db"
import { snippetsTable } from "./schema"

type CreateItemInput = {
    content: string,
}

let createItem(db: Database, input: CreateItemInput) -> Promise<Array<string>> = {
    let rows = db.insert(snippetsTable).values(snippetsTable).returning() |> await
    rows
}
"#,
    )
    .parse_program()
    .expect("should parse");

    // Database is defined as a type alias in a .fl module
    let database_type_decl = TypeDecl {
        exported: true,
        opaque: false,
        name: "Database".to_string(),
        type_params: vec![],
        def: TypeDef::Alias(TypeExpr {
            kind: TypeExprKind::Named {
                name: "BetterSQLite3Database".to_string(),
                type_args: vec![],
                bounds: vec![],
            },
            span: dummy_span,
        }),
        deriving: vec![],
    };
    let mut fl_imports = HashMap::new();
    let mut resolved_db = ResolvedImports::default();
    resolved_db.type_decls.push(database_type_decl);
    fl_imports.insert("./db".to_string(), resolved_db);

    let mut resolved_schema = ResolvedImports::default();
    resolved_schema
        .const_names
        .push("snippetsTable".to_string());
    fl_imports.insert("./schema".to_string(), resolved_schema);

    // Chain probes keyed by the type name (Database) not the variable name (db)
    let chain_values = DtsExport {
        name: "__chain_Database$insert$values".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Any,
                optional: false,
            }],
            return_type: Box::new(TsType::Named("InsertWithValues".to_string())),
        },
    };
    let chain_returning = DtsExport {
        name: "__chain_Database$insert$values$returning".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Promise".to_string(),
                args: vec![TsType::Generic {
                    name: "Array".to_string(),
                    args: vec![TsType::Primitive("string".to_string())],
                }],
            }),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("./db".to_string(), vec![chain_values, chain_returning]);

    let checker = Checker::with_all_imports(fl_imports, dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "chain probes should resolve for fl alias type parameters, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn named_import_const_not_in_resolved_fl_module_errors() {
    use crate::resolve::ResolvedImports;
    use std::collections::HashMap;

    // Module "./config" exports nothing
    let mut fl_imports = HashMap::new();
    fl_imports.insert("./config".to_string(), ResolvedImports::default());

    let program = crate::parser::Parser::new(
        r#"
import { API_URL } from "./config"
"#,
    )
    .parse_program()
    .expect("should parse");

    let diags = Checker::with_imports(fl_imports).check(&program);
    assert!(
        has_error(&diags, ErrorCode::ExportNotFound),
        "importing non-existent let from empty .fl module should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Generic type argument arity ─────────────────────────────

#[test]
fn option_with_too_many_type_args_errors() {
    let source = r#"
let x: Option<string, number> = None
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Option<string, number> should error, got: {diags:?}"
    );
}

#[test]
fn result_with_too_many_type_args_errors() {
    let source = r#"
let x: Result<string, number, boolean> = Ok("hi")
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Result<string, number, boolean> should error, got: {diags:?}"
    );
}

#[test]
fn result_with_too_few_type_args_errors() {
    let source = r#"
let x: Result<string> = Ok("hi")
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Result<string> should error, got: {diags:?}"
    );
}

#[test]
fn promise_with_too_many_type_args_errors() {
    let source = r#"
let foo() -> Promise<string, number> = {
    "hi"
}
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Promise<string, number> should error, got: {diags:?}"
    );
}

#[test]
fn array_with_too_many_type_args_errors() {
    let source = r#"
let x: Array<string, number> = []
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Array<string, number> should error, got: {diags:?}"
    );
}

#[test]
fn settable_with_too_many_type_args_errors() {
    let source = r#"
let x: Settable<string, number> = Settable("hi")
"#;
    let diags = check(source);
    assert!(
        has_error(&diags, ErrorCode::TypeArgumentArity),
        "Settable<string, number> should error, got: {diags:?}"
    );
}

#[test]
fn correct_type_arg_arity_no_error() {
    let source = r#"
let a: Option<string> = None
let b: Result<string, number> = Ok("hi")
let c: Array<number> = []
let foo() -> Promise<string> = { "hi" }
"#;
    let diags = check(source);
    assert!(
        !has_error(&diags, ErrorCode::TypeArgumentArity),
        "correct type arg arities should not error, got: {diags:?}"
    );
}

#[test]
fn option_without_type_args_no_arity_error() {
    let source = r#"
let x: Option = None
"#;
    let diags = check(source);
    assert!(
        !has_error(&diags, ErrorCode::TypeArgumentArity),
        "Option without type args should not produce arity error, got: {diags:?}"
    );
}

// ── Untrusted import: create-snippet scenario ───────────────

#[test]
fn untrusted_import_const_gets_result_type() {
    // Simulates: import { addSeconds } from "date-fns"
    // const expiresAt = addSeconds(now, 5)
    // Hover on expiresAt should show Result<Date, Error>, not ()
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { addSeconds } from "date-fns"
let now = Date.now()
let expiresAt = addSeconds(now, 5)
let _x = expiresAt
"#,
    )
    .parse_program()
    .expect("should parse");

    // addSeconds(date: Date, amount: number) -> Date
    let add_seconds_export = DtsExport {
        name: "addSeconds".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Primitive("Date".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("number".to_string()),
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Primitive("Date".to_string())),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("date-fns".to_string(), vec![add_seconds_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (_diags, name_types, _, _) = checker.check_with_types(&program);

    // expiresAt should be Result<Date, Error> — untrusted call wraps the return type
    let expires_type = name_types
        .get("expiresAt")
        .expect("expiresAt should be in name_types");
    assert!(
        expires_type.contains("Result"),
        "expiresAt should be Result<Date, Error> from untrusted call, got: {}",
        expires_type
    );
    assert!(
        !expires_type.contains("()"),
        "expiresAt should NOT be () or Result<(), Error>, got: {}",
        expires_type
    );
}

#[test]
fn untrusted_import_field_mismatch_shows_hint() {
    // When an untrusted Result is passed as a record field where the unwrapped
    // type is expected, the error should mention "untrusted" and suggest ? or trusted
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { transform } from "some-lib"
import trusted { insert } from "db-lib"

let test() -> () = {
    let name = transform("hello")
    insert({ name: name })
    ()
}
"#,
    )
    .parse_program()
    .expect("should parse");

    let transform_export = DtsExport {
        name: "transform".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("string".to_string())),
        },
    };
    let insert_export = DtsExport {
        name: "insert".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Object(vec![ObjectField {
                    name: "name".to_string(),
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                }]),
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![transform_export]);
    dts_imports.insert("db-lib".to_string(), vec![insert_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, name_types, _, _) = checker.check_with_types(&program);

    // name should be Result<string, Error>
    let name_type = name_types
        .get("name")
        .expect("name should be in name_types");
    assert!(
        name_type.contains("Result"),
        "name should be Result<string, Error>, got: {}",
        name_type
    );

    // The field mismatch error should mention untrusted
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "should have a type error for passing Result<string, Error> where string is expected"
    );
    let error_msg = &errors[0].message;
    assert!(
        error_msg.contains("untrusted") || error_msg.contains("Result"),
        "error should mention untrusted or Result, got: {}",
        error_msg
    );
}

#[test]
fn untrusted_import_without_probe_gets_result() {
    // When there's no tsgo probe (or the probe resolves correctly),
    // the checker's untrusted wrapping should be preserved.
    use crate::interop::{DtsExport, FunctionParam, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import { addSeconds } from "date-fns"
let now = Date.now()
let expiresAt = addSeconds(now, 5)
let _x = expiresAt
"#,
    )
    .parse_program()
    .expect("should parse");

    // Only the function export — no probe override
    let fn_export = DtsExport {
        name: "addSeconds".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Primitive("Date".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("number".to_string()),
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Primitive("Date".to_string())),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("date-fns".to_string(), vec![fn_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (_diags, name_types, _, _) = checker.check_with_types(&program);

    let expires_type = name_types
        .get("expiresAt")
        .expect("expiresAt should be in name_types");

    // Without a probe, the checker's untrusted Result wrapping is preserved
    assert!(
        expires_type.contains("Result"),
        "expiresAt should be Result<unknown, Error> from untrusted call, got: {}",
        expires_type
    );
}

#[test]
fn record_type_name_as_bare_value_errors() {
    let diags = check(
        r#"
type Foo = { a: string }
export let bad() -> number = {
    let x = Foo
    42
}
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeUsedAsValue));
    assert!(has_error_containing(&diags, "`Foo` is a type, not a value"));
}

#[test]
fn union_type_name_as_bare_value_errors() {
    let diags = check(
        r#"
type Route = | Home | Profile(string)
export let bad() -> Route = {
    Route
}
"#,
    );
    assert!(has_error(&diags, ErrorCode::TypeUsedAsValue));
}

#[test]
fn qualified_variant_access_still_works() {
    let diags = check(
        r#"
type Route = | Home | Profile(string)
export let ok() -> Route = {
    Route.Home
}
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeUsedAsValue));
}

#[test]
fn bare_nullary_variant_still_works() {
    let diags = check(
        r#"
type Route = | Home | Profile(string)
export let ok() -> Route = {
    Home
}
"#,
    );
    assert!(!has_error(&diags, ErrorCode::TypeUsedAsValue));
}

// ── Hindley-Milner inference ────────────────────────────────

#[test]
fn identity_without_annotations_infers_polymorphic_type() {
    let diags = check(
        r#"
let id(x) = { x }
let _n = id(42)
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "expected no errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn identity_let_polymorphism_allows_different_types() {
    let diags = check(
        r#"
let id(x) = { x }
let _n = id(42)
let _s = id("hello")
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "expected let-polymorphism, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn recursive_infinite_type_is_rejected_by_occurs_check() {
    let diags = check(
        r#"
let bad(x) = { [x, bad(x)] }
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "expected occurs-check failure, got no errors"
    );
}

#[test]
fn deep_resolve_follows_links_through_arrays() {
    use super::types::Type;
    use std::sync::Arc;

    let v = Type::unbound(0);
    let arr = Type::Array(Arc::new(v.clone()));
    super::unify::unify(&v, &Type::Number).unwrap();
    match &arr.deep_resolved() {
        Type::Array(inner) => assert_eq!(**inner, Type::Number),
        other => panic!("expected Array<Number>, got {:?}", other),
    }
    assert_eq!(v.resolved(), Type::Number);
}

#[test]
fn annotated_generic_fn_matches_inferred_generic_fn() {
    let diags_annotated = check(
        r#"
let id<T>(x: T) -> T = { x }
let _n = id(1)
let _s = id("x")
"#,
    );
    let diags_inferred = check(
        r#"
let id(x) = { x }
let _n = id(1)
let _s = id("x")
"#,
    );
    assert!(!has_error(&diags_annotated, ErrorCode::TypeMismatch));
    assert!(!has_error(&diags_inferred, ErrorCode::TypeMismatch));
}

#[test]
fn inferred_return_type_flows_to_caller() {
    // After inferring `id : (a) -> a`, `id(42) + 1` must typecheck: the
    // return type has to resolve to `number` at the call site, not stay
    // an unbound var that gets accepted permissively.
    let diags = check(
        r#"
let id(x) = { x }
let _n = id(42) + 1
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "call-site return type failed to resolve, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn polymorphic_return_rejects_concrete_mismatch() {
    // `id(42)` must resolve to `number` at the call site. Passing that
    // to a `string`-typed param has to error — the old wildcard hack
    // swallowed this because Var(_) compared as compatible with anything.
    let diags = check(
        r#"
let id(x) = { x }
let takesString(s: string) -> string = { s }
let _x = takesString(id(42))
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "polymorphic return type didn't propagate — the old wildcard path is still live"
    );
}

#[test]
fn occurs_check_fires_inside_tuple() {
    // Occurs check must catch the cycle through any container, not only
    // arrays.
    let diags = check(
        r#"
let bad(x) = { (x, bad(x)) }
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "occurs-check through tuple not caught"
    );
}

#[test]
fn stdlib_let_polymorphism_across_element_types() {
    // `Array.map` in one program used with `number -> string` at one call
    // site and `string -> number` at another — each call has to get its
    // own fresh instantiation.
    let diags = check(
        r#"
let _ns = [1, 2, 3] |> Array.map((n) -> Number.toString(n))
let _ls = ["a", "bb", "ccc"] |> Array.map((s) -> String.length(s))
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "stdlib let-polymorphism failed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Reference tracking ──────────────────────────────────────

#[test]
fn reference_tracker_records_every_use_of_a_user_function() {
    let source = r#"
let greet(n: string) -> string = { n }
let _a = greet("a")
let _b = greet("b")
let _c = greet("c")
"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let (_diags, refs) = Checker::new().check_with_references(&program);
    let refs = &refs;
    let def_span = refs
        .definition_for_name("greet")
        .expect("greet definition should be registered");
    let uses = refs.find_references(def_span);
    assert_eq!(
        uses.len(),
        3,
        "expected 3 references to greet, found {}: {:?}",
        uses.len(),
        uses,
    );
}

#[test]
fn reference_tracker_back_resolves_use_site_to_definition() {
    let source = r#"
let x = 42
let _y = x
"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let (_diags, refs) = Checker::new().check_with_references(&program);
    let refs = &refs;
    let def = refs
        .definition_for_name("x")
        .expect("x definition registered");
    let uses = refs.find_references(def);
    assert_eq!(uses.len(), 1);
    // The reverse lookup should land back on the definition.
    assert_eq!(refs.definition_at(uses[0]), Some(def));
}

#[test]
fn reference_tracker_ignores_unused_definitions() {
    let source = r#"
let unused() -> number = { 1 }
"#;
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    let (_diags, refs) = Checker::new().check_with_references(&program);
    let refs = &refs;
    let def = refs
        .definition_for_name("unused")
        .expect("unused definition registered");
    assert!(refs.find_references(def).is_empty());
}

#[test]
fn tuple_destructure_on_tuple_value_succeeds() {
    let diags = check(
        r#"
let t = (1, 2)
let (x, y) = t
"#,
    );
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "tuple destructure on tuple value should succeed, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Trait / for-block validation (#1085, #1086, #1087) ────────────

#[test]
fn trait_name_used_as_value_errors() {
    // #1085: `TraitName.method(...)` calls a trait in value position —
    // should error. Traits are contracts, not callable modules.
    let diags = check(
        r#"
trait SnippetRepository {
    let create(self, input: string) -> string
}

let _r = SnippetRepository.create("hello")
"#,
    );
    assert!(
        has_error_containing(&diags, "trait")
            && diags
                .iter()
                .any(|d| d.message.contains("SnippetRepository")),
        "trait-as-value should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_method_untyped_param_errors() {
    // #1086: trait method params (other than `self`) must have explicit types.
    let diags = check(
        r#"
trait Repo {
    let create(self, shit, snippet: string) -> string
}
"#,
    );
    assert!(
        has_error_containing(&diags, "type annotation")
            || has_error_containing(&diags, "must have a type"),
        "untyped trait method param should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn for_block_fn_untyped_param_errors() {
    // #1086: for-block method params (other than `self`) must have explicit types.
    let diags = check(
        r#"
type MyRepo = {}

for MyRepo {
    let create(self, shit) -> string = {
        "hi"
    }
}
"#,
    );
    assert!(
        has_error_containing(&diags, "type annotation")
            || has_error_containing(&diags, "must have a type"),
        "untyped for-block param should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_method_without_self_errors() {
    // #1087: trait methods must have `self` as the first parameter.
    let diags = check(
        r#"
trait Repo {
    let create(input: string) -> string
}
"#,
    );
    assert!(
        has_error_containing(&diags, "self"),
        "trait method without `self` should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn trait_method_self_not_first_errors() {
    // #1087: `self` in a non-first position is also rejected.
    let diags = check(
        r#"
trait Repo {
    let create(input: string, self) -> string
}
"#,
    );
    assert!(
        has_error_containing(&diags, "self"),
        "trait method with `self` not first should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_into_stdlib_method_does_not_silently_unwrap_result() {
    // #1168: piping `Result<T, E>` into a stdlib method expecting `T` must
    // error — the caller is responsible for unwrapping with `?` or `match`.
    let diags = check(
        r#"
let arr() -> Result<Array<number>, Error> = { Ok([1, 2, 3]) }
export let main() -> string = {
    let r = arr() |> Array.at(0)
    let x: string = r
    x
}
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "piping Result<Array<T>, E> into Array.at should error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("Result") && d.message.contains("Array.at")),
        "error should flag the Result argument to Array.at, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn pipe_unwrap_then_stdlib_preserves_element_type() {
    // Once the Result is unwrapped via `?`, the pipe should bind the
    // element type correctly: Array<number> |> Array.at(0) → Option<number>.
    let diags = check(
        r#"
let arr() -> Result<Array<number>, Error> = { Ok([1, 2, 3]) }
export let main() -> Result<Option<number>, Error> = {
    let a = arr()?
    let r = a |> Array.at(0)
    Ok(r)
}
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "unwrapped Array<number> |> Array.at(0) should type-check, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn foreign_generic_args_distinguish_parameterizations() {
    // Floe signatures referencing a Foreign type with generic args like
    // `Router<Alpha>` used to collapse to bare `Foreign("Router")`, so
    // `Router<Alpha>` and `Router<Beta>` were indistinguishable and a
    // mismatched call silently succeeded. The resolver now encodes the
    // args into the Foreign name, and `types_compatible` rejects
    // same-base-name Foreigns whose full names differ — except when either
    // side carries a single-letter type-parameter placeholder (covered by
    // the sibling test below).
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { Router } from "@floeorg/hono"

type Alpha = { foo: string }
type Beta = { bar: number }

let takesAlpha(r: Router<Alpha>) -> Router<Alpha> = { r }
let takesBeta(r: Router<Beta>) -> Router<Beta> = { r }

let mkAlpha() -> Router<Alpha> = { takesAlpha(mkAlpha()) }

let _bad = takesBeta(mkAlpha())
"#,
    )
    .parse_program()
    .expect("should parse");

    let router_export = DtsExport {
        name: "Router".to_string(),
        ts_type: TsType::Any,
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("@floeorg/hono".to_string(), vec![router_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Router<Alpha>") && d.message.contains("Router<Beta>")),
        "expected a Router<Alpha> vs Router<Beta> mismatch error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn foreign_with_type_param_arg_stays_permissive() {
    // Imported TS generic signatures like `get<E>(...)->Router<E>` wrap to
    // `Foreign("Router<E>")` with `E` as an unresolved type parameter
    // placeholder. Until inference substitutes `E` through the call chain
    // (tracked separately as #1209), `Router<E>` must stay compatible with
    // `Router<Env>` / `Router<Bindings>` / etc., or the pre-#1211
    // user-level behavior regresses.
    use crate::interop::{DtsExport, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { Router } from "@floeorg/hono"

type Env = { greeting: string }

// Simulate a hono-style signature whose body just returns a fresh value:
// the point is that `handle`'s annotated Router<Env> input must accept
// `Router<E>` produced by imported generic functions.
let handle(_r: Router<Env>) -> Response = { Response("ok") }
"#,
    )
    .parse_program()
    .expect("should parse");

    let router_export = DtsExport {
        name: "Router".to_string(),
        ts_type: TsType::Any,
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("@floeorg/hono".to_string(), vec![router_export]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Router<Env> declaration should not error, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn bare_identifier_pipe_solves_generic_from_piped_type() {
    // `a |> f` where `f` is a generic bare function used to return a raw
    // unsolved type variable because `check_pipe_right` read `return_type`
    // straight off the Function without instantiating generics or unifying
    // with the piped-in type. The stdlib pipe path already did both —
    // the non-stdlib bare-identifier path now mirrors it.
    let program = crate::parser::Parser::new(
        r#"
type Bindings = { name: string }

let identity<T>(x: T) -> T = { x }
let tap<T>(x: T) -> T = { x }
let chain<T>(x: T, _extra: string) -> T = { x }

let _bindings = Bindings(name: "x")
let _direct = identity<Bindings>(_bindings)
let _piped_bare = identity<Bindings>(_bindings) |> tap
let _piped_call = identity<Bindings>(_bindings) |> chain("extra")
let _piped_chain = identity<Bindings>(_bindings) |> chain("a") |> chain("b")
"#,
    )
    .parse_program()
    .expect("should parse");
    let (diags, types, _, _) = Checker::new().check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "generic pipe should type-check, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    let direct = types.get("_direct").map(String::as_str);
    let piped_bare = types.get("_piped_bare").map(String::as_str);
    let piped_call = types.get("_piped_call").map(String::as_str);
    let piped_chain = types.get("_piped_chain").map(String::as_str);

    assert_eq!(direct, Some("Bindings"), "direct call baseline");
    assert_eq!(
        piped_bare,
        Some("Bindings"),
        "bare-identifier pipe must resolve T to Bindings"
    );
    assert_eq!(piped_call, Some("Bindings"), "call-form pipe baseline");
    assert_eq!(
        piped_chain,
        Some("Bindings"),
        "chained call-form pipe baseline"
    );
}

#[test]
fn destructured_lambda_param_infers_body_return_type() {
    // Lambda with object-destructured param used to lose body return-type
    // inference because arrow args were checked before the callee's
    // expected param types were available as hints — destructured bindings
    // fell through to `Type::Unknown`, so `a + b` returned `unknown`. The
    // main call path now defers arrow args and re-checks them with hints.
    let program = crate::parser::Parser::new(
        r#"
let call(cb: ({ a: number, b: number }) -> number) -> number = {
    cb({ a: 1, b: 2 })
}

let _plain = call(({ a, b }) -> a + b)
let _alias = call(({ a: x, b: y }) -> x + y)
let _ident = call((o) -> o.a + o.b)
"#,
    )
    .parse_program()
    .expect("should parse");
    let (diags, _, _, _) = Checker::new().check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "destructured-lambda call should type-check, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn lambda_param_inferred_through_generic_trusted_fn() {
    // The real-world Hono case: `post<E>(r: Router<E>, path: string,
    // handler: (c: Context<{ Bindings: E }>) => Response)` — the lambda's
    // `c` param should resolve to `Context<{ Bindings: <concrete E> }>`
    // after `E` is unified from the first argument's type.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { router, post } from "some-router"
type Bindings = { db: string }
export let app = router<Bindings>() |> post("/", (c) -> c.path)
"#,
    )
    .parse_program()
    .expect("should parse");

    // Router<E> has an opaque inner (we only care that it's a named wrapper
    // so post's signature can correlate its `E` with the first arg).
    let router_fn = DtsExport {
        name: "router".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    // post(r: Router<E>, path: string, handler: (c: Ctx<E>) -> string): Router<E>
    // Ctx<E> modeled as an object with a `path: string` field so we can
    // observe propagation by checking `c.path` in the lambda body.
    let ctx_obj = TsType::Object(vec![ObjectField {
        name: "path".to_string(),
        ty: TsType::Primitive("string".to_string()),
        optional: false,
    }]);
    let post_fn = DtsExport {
        name: "post".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Generic {
                        name: "Router".to_string(),
                        args: vec![TsType::Named("E".to_string())],
                    },
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Function {
                        params: vec![FunctionParam {
                            ty: ctx_obj,
                            optional: false,
                        }],
                        return_type: Box::new(TsType::Primitive("string".to_string())),
                    },
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-router".to_string(), vec![router_fn, post_fn]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "generic trusted higher-order fn should type-check inline lambda, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn named_handler_with_nested_type_param_matches_generic_callback() {
    // Regression for #1263. A named handler with an explicit
    // `Context<{ Bindings: Bindings }>` annotation should pass to a
    // generic `post<E>` expecting `(c: Context<{ Bindings: E }>) -> ...`
    // once `E` is inferred to `Bindings` from the first arg. Previously
    // the Foreign name was eagerly encoded as `Context<{ Bindings: E }>`
    // (because `E` is nested inside a record arg, not at the top level),
    // so the string-based compat check saw `Context<{ Bindings: E }>` vs
    // `Context<{ Bindings: Bindings }>` and rejected the handler.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { router, post, Context } from "some-router"
type Bindings = { DB: string }

let handleCreate(_c: Context<{ Bindings: Bindings }>) -> string = { "ok" }

export let app = router<Bindings>() |> post("/", handleCreate)
"#,
    )
    .parse_program()
    .expect("should parse");

    let context_ty = DtsExport {
        name: "Context".to_string(),
        ts_type: TsType::Any,
    };
    let router_fn = DtsExport {
        name: "router".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    let ctx_generic = TsType::Generic {
        name: "Context".to_string(),
        args: vec![TsType::Object(vec![ObjectField {
            name: "Bindings".to_string(),
            ty: TsType::Named("E".to_string()),
            optional: false,
        }])],
    };
    let post_fn = DtsExport {
        name: "post".to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Generic {
                        name: "Router".to_string(),
                        args: vec![TsType::Named("E".to_string())],
                    },
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Function {
                        params: vec![FunctionParam {
                            ty: ctx_generic,
                            optional: false,
                        }],
                        return_type: Box::new(TsType::Primitive("string".to_string())),
                    },
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "some-router".to_string(),
        vec![context_ty, router_fn, post_fn],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "named handler with Context<{{ Bindings: Bindings }}> should type-check against generic post<E>, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn lambda_param_hint_not_clobbered_by_adjacent_trusted_calls_in_pipe_chain() {
    // Realistic shape of a Hono routing pipeline: a `router<E>()` returning a
    // wrapper, piped through multiple `post(path, (c) -> ...)` calls. Each
    // lambda must pick up its own Context type from the expected handler
    // param, and the hint from one call must not leak into or be overwritten
    // by the next. Regression guard for #1234 once the fix lands.
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { router, post, get } from "some-router"
type Bindings = { DB: string }
export let app = router<Bindings>()
    |> get("/", (c) -> c.path)
    |> post("/snippets", (c) -> c.path)
"#,
    )
    .parse_program()
    .expect("should parse");

    let router_fn = DtsExport {
        name: "router".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    let handler_fn_type = TsType::Function {
        params: vec![FunctionParam {
            ty: TsType::Object(vec![ObjectField {
                name: "path".to_string(),
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            }]),
            optional: false,
        }],
        return_type: Box::new(TsType::Primitive("string".to_string())),
    };
    let make_route_fn = |name: &str| DtsExport {
        name: name.to_string(),
        ts_type: TsType::Function {
            params: vec![
                FunctionParam {
                    ty: TsType::Generic {
                        name: "Router".to_string(),
                        args: vec![TsType::Named("E".to_string())],
                    },
                    optional: false,
                },
                FunctionParam {
                    ty: TsType::Primitive("string".to_string()),
                    optional: false,
                },
                FunctionParam {
                    ty: handler_fn_type.clone(),
                    optional: false,
                },
            ],
            return_type: Box::new(TsType::Generic {
                name: "Router".to_string(),
                args: vec![TsType::Named("E".to_string())],
            }),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert(
        "some-router".to_string(),
        vec![router_fn, make_route_fn("get"), make_route_fn("post")],
    );

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "pipe-chained trusted higher-order calls should each receive their lambda hint, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn lambda_param_inferred_from_trusted_imported_higher_order_fn() {
    // A trusted import that takes a callback should propagate the expected
    // callback parameter type into an inline lambda. Previously the lambda
    // param came back as `unknown`, forcing users to re-annotate (e.g.
    // Hono's `post(path, (c: Context<...>) -> ...)`).
    use crate::interop::{DtsExport, FunctionParam, ObjectField, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { withUser } from "some-lib"
let _name = withUser((u) -> u.name)
"#,
    )
    .parse_program()
    .expect("should parse");

    // Mock: withUser(cb: (u: { id: number, name: string }) => string): string
    let user_obj = TsType::Object(vec![
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
    ]);
    let with_user = DtsExport {
        name: "withUser".to_string(),
        ts_type: TsType::Function {
            params: vec![FunctionParam {
                ty: TsType::Function {
                    params: vec![FunctionParam {
                        ty: user_obj,
                        optional: false,
                    }],
                    return_type: Box::new(TsType::Primitive("string".to_string())),
                },
                optional: false,
            }],
            return_type: Box::new(TsType::Primitive("string".to_string())),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![with_user]);

    let checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "inline lambda to trusted higher-order fn should type-check, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── #1264 default type parameters ──────────────────────────────

#[test]
fn dts_generic_default_fills_missing_type_arg() {
    // Regression for #1264. When a .d.ts generic declares defaults like
    // `interface Foo<A = string, B = number>`, a user's 1-arg reference
    // `Foo<boolean>` must resolve to the same Foreign as a 2-arg
    // `Foo<boolean, number>` produced elsewhere in the same program.
    use crate::interop::{DtsExport, GenericParamInfo, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { Foo, pair } from "some-lib"

let expectPadded(_x: Foo<boolean, number>) -> number = { 0 }
let _usePartial = expectPadded(pair())
"#,
    )
    .parse_program()
    .expect("should parse");

    let foo_export = DtsExport {
        name: "Foo".to_string(),
        ts_type: TsType::Any,
    };
    // pair(): Foo<boolean>  — library function returning a 1-arg form.
    // Defaults should pad the second slot with `number`, making the
    // return type equivalent to `Foo<boolean, number>`.
    let pair_fn = DtsExport {
        name: "pair".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Foo".to_string(),
                args: vec![TsType::Primitive("boolean".to_string())],
            }),
        },
    };

    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![foo_export, pair_fn]);

    let mut generic_params = HashMap::new();
    generic_params.insert(
        "Foo".to_string(),
        vec![
            GenericParamInfo {
                name: "A".to_string(),
                default: Some(TsType::Primitive("string".to_string())),
            },
            GenericParamInfo {
                name: "B".to_string(),
                default: Some(TsType::Primitive("number".to_string())),
            },
        ],
    );

    let mut checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    checker.set_dts_generic_params(generic_params);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Foo<boolean> (defaulted to Foo<boolean, number>) should unify with Foo<boolean, number>, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn dts_generic_default_stops_at_first_missing_default() {
    // If a parameter without a default sits before one with a default,
    // padding cannot continue past it. `Foo<A, B = number>` called as
    // `Foo<>` should leave A unresolved; we don't invent a value.
    use crate::interop::{DtsExport, GenericParamInfo, TsType};
    use std::collections::HashMap;

    let program = crate::parser::Parser::new(
        r#"
import trusted { twoArgs } from "some-lib"
let _x = twoArgs()
"#,
    )
    .parse_program()
    .expect("should parse");

    let two_args_fn = DtsExport {
        name: "twoArgs".to_string(),
        ts_type: TsType::Function {
            params: vec![],
            return_type: Box::new(TsType::Generic {
                name: "Foo".to_string(),
                args: vec![
                    TsType::Primitive("boolean".to_string()),
                    TsType::Primitive("string".to_string()),
                ],
            }),
        },
    };
    let mut dts_imports = HashMap::new();
    dts_imports.insert("some-lib".to_string(), vec![two_args_fn]);

    let mut generic_params = HashMap::new();
    generic_params.insert(
        "Foo".to_string(),
        vec![
            GenericParamInfo {
                name: "A".to_string(),
                default: None,
            },
            GenericParamInfo {
                name: "B".to_string(),
                default: Some(TsType::Primitive("number".to_string())),
            },
        ],
    );

    let mut checker = Checker::with_all_imports(HashMap::new(), dts_imports);
    checker.set_dts_generic_params(generic_params);
    let (diags, _, _, _) = checker.check_with_types(&program);

    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "missing-default gap should not block legitimate usage, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn fn_type_alias_accepts_labelled_and_unlabelled_params() {
    // Labels are documentation only — both forms parse and check cleanly.
    let diags = check(
        r#"
type Handler = (req: number) -> number
type Predicate = (number) -> boolean
let _h(f: Handler, x: number) -> number = { f(x) }
let _p(f: Predicate, x: number) -> boolean = { f(x) }
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "labelled and unlabelled fn-type aliases should both check cleanly, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn fn_type_param_label_does_not_collide_with_call_site_arguments() {
    // Labels are documentation only — call sites use positional args without
    // having to refer to the label.
    let diags = check(
        r#"
type Apply = (n: number) -> number
let _twice(f: Apply, x: number) -> number = { f(f(x)) }
"#,
    );
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "labelled fn type should accept positional calls, got: {:?}",
        errors.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ── Function-type aliases are structural (#1274) ─────────────

#[test]
fn function_type_alias_accepts_zero_arg_closure() {
    let diags = check(
        r#"
type CreatePoop = () -> string
export let poop: CreatePoop = () -> "poop"
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "closure should satisfy structural function alias, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn function_type_alias_accepts_one_arg_closure() {
    let diags = check(
        r#"
type F = (number) -> number
export let f: F = (x) -> x + 1
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "one-arg closure should satisfy structural function alias, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn function_type_alias_call_site_returns_unwrapped_type() {
    let diags = check(
        r#"
type F = () -> string
let f: F = () -> "x"
export let s: string = f()
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "calling an alias-typed value should return the alias's return type, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn function_type_alias_as_parameter_accepts_closure_literal() {
    let diags = check(
        r#"
type F = () -> string
let run(g: F) -> string = { g() }
export let r: string = run(() -> "z")
"#,
    );
    assert!(
        !has_error(&diags, ErrorCode::TypeMismatch),
        "passing a closure to an alias-typed parameter should check, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn nominal_newtype_still_rejects_raw_payload() {
    let diags = check(
        r#"
type OrderId = OrderId(number)
export let id: OrderId = 42
"#,
    );
    assert!(
        has_error(&diags, ErrorCode::TypeMismatch),
        "nominal newtype must keep rejecting raw payload values"
    );
}
