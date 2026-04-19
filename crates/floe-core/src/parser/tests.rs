use super::*;

fn parse(input: &str) -> Result<Program, Vec<ParseError>> {
    Parser::new(input).parse_program()
}

fn parse_ok(input: &str) -> Program {
    parse(input).unwrap_or_else(|errs| {
        panic!(
            "parse failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

fn first_item(input: &str) -> ItemKind {
    parse_ok(input).items.into_iter().next().unwrap().kind
}

fn first_expr(input: &str) -> ExprKind {
    match first_item(input) {
        ItemKind::Expr(e) => e.kind,
        other => panic!("expected expression item, got {other:?}"),
    }
}

// ── Literals ─────────────────────────────────────────────────

#[test]
fn number_literal() {
    assert_eq!(first_expr("42"), ExprKind::Number("42".to_string()));
}

#[test]
fn string_literal() {
    assert_eq!(
        first_expr(r#""hello""#),
        ExprKind::String("hello".to_string())
    );
}

#[test]
fn bool_literal() {
    assert_eq!(first_expr("true"), ExprKind::Bool(true));
    assert_eq!(first_expr("false"), ExprKind::Bool(false));
}

#[test]
fn none_is_identifier() {
    assert_eq!(first_expr("None"), ExprKind::Identifier("None".to_string()));
}

#[test]
fn todo_expr() {
    assert_eq!(first_expr("todo"), ExprKind::Todo);
}

#[test]
fn unreachable_expr() {
    assert_eq!(first_expr("unreachable"), ExprKind::Unreachable);
}

#[test]
fn placeholder() {
    assert_eq!(first_expr("_"), ExprKind::Placeholder);
}

// ── Tagged template literals ─────────────────────────────────

#[test]
fn tagged_template_no_interpolation() {
    let expr = first_expr("tag`hello`");
    let ExprKind::TaggedTemplate { tag, parts } = expr else {
        panic!("expected tagged template, got {expr:?}");
    };
    assert_eq!(tag.kind, ExprKind::Identifier("tag".to_string()));
    assert_eq!(parts, vec![TemplatePart::Raw("hello".to_string())]);
}

#[test]
fn tagged_template_with_interpolation() {
    let expr = first_expr("tag`a ${x} b`");
    let ExprKind::TaggedTemplate { tag, parts } = expr else {
        panic!("expected tagged template, got {expr:?}");
    };
    assert_eq!(tag.kind, ExprKind::Identifier("tag".to_string()));
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0], TemplatePart::Raw("a ".to_string()));
    assert!(
        matches!(&parts[1], TemplatePart::Expr(e) if matches!(&e.kind, ExprKind::Identifier(n) if n == "x"))
    );
    assert_eq!(parts[2], TemplatePart::Raw(" b".to_string()));
}

#[test]
fn tagged_template_nested_template_in_interpolation() {
    let expr = first_expr("tag`outer ${`inner ${x}`} end`");
    let ExprKind::TaggedTemplate { parts, .. } = expr else {
        panic!("expected tagged template");
    };
    assert_eq!(parts.len(), 3);
    let TemplatePart::Expr(inner) = &parts[1] else {
        panic!("expected interpolation")
    };
    assert!(matches!(&inner.kind, ExprKind::TemplateLiteral(_)));
}

#[test]
fn tagged_template_member_tag() {
    let expr = first_expr("db.sql`select 1`");
    let ExprKind::TaggedTemplate { tag, .. } = expr else {
        panic!("expected tagged template, got {expr:?}");
    };
    assert!(matches!(&tag.kind, ExprKind::Member { field, .. } if field == "sql"));
}

#[test]
fn plain_template_literal_still_works() {
    assert_eq!(
        first_expr("`hello`"),
        ExprKind::TemplateLiteral(vec![TemplatePart::Raw("hello".to_string())])
    );
}

#[test]
fn template_on_new_line_is_not_tag() {
    // A template literal on a new line after an identifier must be a
    // separate expression, not a tagged-template for the preceding ident.
    let program = parse_ok("tag\n`hello`");
    assert_eq!(program.items.len(), 2);
}

// ── Identifiers ──────────────────────────────────────────────

#[test]
fn identifier() {
    assert_eq!(
        first_expr("myVar"),
        ExprKind::Identifier("myVar".to_string())
    );
}

// ── Binary Operators ─────────────────────────────────────────

#[test]
fn binary_add() {
    let expr = first_expr("1 + 2");
    assert!(matches!(expr, ExprKind::Binary { op: BinOp::Add, .. }));
}

#[test]
fn binary_precedence() {
    // 1 + 2 * 3 should parse as 1 + (2 * 3)
    let expr = first_expr("1 + 2 * 3");
    match expr {
        ExprKind::Binary {
            op: BinOp::Add,
            right,
            ..
        } => {
            assert!(matches!(
                right.kind,
                ExprKind::Binary { op: BinOp::Mul, .. }
            ));
        }
        _ => panic!("expected binary add"),
    }
}

#[test]
fn comparison() {
    let expr = first_expr("a == b");
    assert!(matches!(expr, ExprKind::Binary { op: BinOp::Eq, .. }));
}

#[test]
fn logical_and_or() {
    // a || b && c should parse as a || (b && c)
    let expr = first_expr("a || b && c");
    match expr {
        ExprKind::Binary {
            op: BinOp::Or,
            right,
            ..
        } => {
            assert!(matches!(
                right.kind,
                ExprKind::Binary { op: BinOp::And, .. }
            ));
        }
        _ => panic!("expected binary or"),
    }
}

// ── Unary Operators ──────────────────────────────────────────

#[test]
fn unary_not() {
    let expr = first_expr("!x");
    assert!(matches!(
        expr,
        ExprKind::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn unary_neg() {
    let expr = first_expr("-42");
    assert!(matches!(
        expr,
        ExprKind::Unary {
            op: UnaryOp::Neg,
            ..
        }
    ));
}

// ── Pipe Operator ────────────────────────────────────────────

#[test]
fn pipe_simple() {
    let expr = first_expr("x |> f(y)");
    assert!(matches!(expr, ExprKind::Pipe { .. }));
}

#[test]
fn pipe_chained() {
    let expr = first_expr("x |> f |> g");
    match expr {
        ExprKind::Pipe { left, .. } => {
            assert!(matches!(left.kind, ExprKind::Pipe { .. }));
        }
        _ => panic!("expected chained pipe"),
    }
}

// ── Unwrap ───────────────────────────────────────────────────

#[test]
fn unwrap_operator() {
    let expr = first_expr("fetchUser(id)?");
    assert!(matches!(expr, ExprKind::Unwrap(_)));
}

#[test]
fn pipe_then_unwrap() {
    // `x |> f?` should parse as `(x |> f)?`, not `x |> (f?)`
    let expr = first_expr("x |> f?");
    match &expr {
        ExprKind::Unwrap(inner) => {
            assert!(
                matches!(inner.kind, ExprKind::Pipe { .. }),
                "expected Unwrap(Pipe), got Unwrap({:?})",
                inner.kind
            );
        }
        _ => panic!("expected Unwrap, got {expr:?}"),
    }
}

#[test]
fn pipe_chain_then_unwrap() {
    // `x |> f |> g?` should parse as `(x |> f |> g)?`
    let expr = first_expr("x |> f |> g?");
    match &expr {
        ExprKind::Unwrap(inner) => {
            assert!(
                matches!(inner.kind, ExprKind::Pipe { .. }),
                "expected Unwrap(Pipe), got Unwrap({:?})",
                inner.kind
            );
            // The inner pipe's left should also be a pipe (x |> f)
            if let ExprKind::Pipe { left, .. } = &inner.kind {
                assert!(
                    matches!(left.kind, ExprKind::Pipe { .. }),
                    "expected chained pipe"
                );
            }
        }
        _ => panic!("expected Unwrap, got {expr:?}"),
    }
}

#[test]
fn pipe_unwrap_operator_parses_as_unwrap_of_pipe() {
    // `x |>? f` is sugar for `(x |> f)?`: pipe into f, then unwrap.
    let expr = first_expr("x |>? f");
    match &expr {
        ExprKind::Unwrap(inner) => {
            assert!(
                matches!(inner.kind, ExprKind::Pipe { .. }),
                "expected Unwrap(Pipe), got Unwrap({:?})",
                inner.kind
            );
        }
        _ => panic!("expected Unwrap, got {expr:?}"),
    }
}

// ── Function Calls ───────────────────────────────────────────

#[test]
fn function_call() {
    let expr = first_expr("f(1, 2)");
    match expr {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 2);
        }
        _ => panic!("expected call"),
    }
}

#[test]
fn named_args() {
    let expr = first_expr("f(name: x, limit: 10)");
    match expr {
        ExprKind::Call { args, .. } => {
            assert!(matches!(&args[0], Arg::Named { label, .. } if label == "name"));
            assert!(matches!(&args[1], Arg::Named { label, .. } if label == "limit"));
        }
        _ => panic!("expected call"),
    }
}

#[test]
fn named_arg_punning() {
    let expr = first_expr("f(name:, limit:)");
    match expr {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(
                matches!(&args[0], Arg::Named { label, value } if label == "name" && matches!(&value.kind, ExprKind::Identifier(n) if n == "name"))
            );
            assert!(
                matches!(&args[1], Arg::Named { label, value } if label == "limit" && matches!(&value.kind, ExprKind::Identifier(n) if n == "limit"))
            );
        }
        _ => panic!("expected call"),
    }
}

#[test]
fn named_arg_punning_mixed() {
    let expr = first_expr(r#"f("pos", name:, limit: 10)"#);
    match expr {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 3);
            assert!(matches!(&args[0], Arg::Positional(_)));
            assert!(
                matches!(&args[1], Arg::Named { label, value } if label == "name" && matches!(&value.kind, ExprKind::Identifier(n) if n == "name"))
            );
            assert!(matches!(&args[2], Arg::Named { label, .. } if label == "limit"));
        }
        _ => panic!("expected call"),
    }
}

// ── Constructors ─────────────────────────────────────────────

#[test]
fn constructor() {
    let expr = first_expr(r#"User(name: "Ryan", email: e)"#);
    match expr {
        ExprKind::Construct {
            type_name, args, ..
        } => {
            assert_eq!(type_name, "User");
            assert_eq!(args.len(), 2);
        }
        _ => panic!("expected construct"),
    }
}

#[test]
fn constructor_with_spread() {
    let expr = first_expr(r#"User(..user, name: "New")"#);
    match expr {
        ExprKind::Construct { spread, args, .. } => {
            assert!(spread.is_some());
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected construct"),
    }
}

// ── Result/Option Constructors ───────────────────────────────

#[test]
fn ok_constructor() {
    let expr = first_expr("Ok(42)");
    assert!(matches!(expr, ExprKind::Construct { .. }));
}

#[test]
fn err_constructor() {
    let expr = first_expr(r#"Err("not found")"#);
    assert!(matches!(expr, ExprKind::Construct { .. }));
}

#[test]
fn some_constructor() {
    let expr = first_expr("Some(x)");
    assert!(matches!(expr, ExprKind::Construct { .. }));
}

// ── Parse Built-in ───────────────────────────────────────────

#[test]
fn parse_with_value() {
    let expr = first_expr("parse<string>(x)");
    assert!(matches!(expr, ExprKind::Parse { .. }));
}

#[test]
fn parse_without_parens() {
    // In pipe context: `json |> parse<User>`
    let expr = first_expr("parse<User>");
    match expr {
        ExprKind::Parse { type_arg, value } => {
            assert!(matches!(value.kind, ExprKind::Placeholder));
            assert!(matches!(type_arg.kind, TypeExprKind::Named { .. }));
        }
        other => panic!("expected Parse, got {other:?}"),
    }
}

#[test]
fn parse_record_type() {
    let expr = first_expr("parse<{ name: string, age: number }>(data)");
    assert!(matches!(expr, ExprKind::Parse { .. }));
}

#[test]
fn parse_array_type() {
    let expr = first_expr("parse<Array<number>>(items)");
    assert!(matches!(expr, ExprKind::Parse { .. }));
}

// ── Mock Built-in ───────────────────────────────────────────

#[test]
fn mock_basic() {
    let expr = first_expr("mock<string>");
    assert!(matches!(expr, ExprKind::Mock { .. }));
}

#[test]
fn mock_named_type() {
    let expr = first_expr("mock<User>");
    match expr {
        ExprKind::Mock {
            type_arg,
            overrides,
        } => {
            assert!(matches!(type_arg.kind, TypeExprKind::Named { .. }));
            assert!(overrides.is_empty());
        }
        other => panic!("expected Mock, got {other:?}"),
    }
}

#[test]
fn mock_with_overrides() {
    let expr = first_expr("mock<User>(name: \"Alice\")");
    match expr {
        ExprKind::Mock {
            type_arg,
            overrides,
        } => {
            assert!(matches!(type_arg.kind, TypeExprKind::Named { .. }));
            assert_eq!(overrides.len(), 1);
            assert!(matches!(&overrides[0], Arg::Named { label, .. } if label == "name"));
        }
        other => panic!("expected Mock, got {other:?}"),
    }
}

// ── Pipe Lambdas ─────────────────────────────────────────────

#[test]
fn pipe_lambda_multi_arg() {
    let expr = first_expr("(a, b) -> a + b");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "a");
            assert_eq!(params[1].name, "b");
        }
        _ => panic!("expected arrow"),
    }
}

#[test]
fn pipe_lambda_single_arg() {
    let expr = first_expr("(x) -> x + 1");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
        }
        _ => panic!("expected arrow"),
    }
}

#[test]
fn pipe_lambda_typed() {
    let expr = first_expr("(x: number) -> x + 1");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert!(params[0].type_ann.is_some());
        }
        _ => panic!("expected arrow"),
    }
}

#[test]
fn zero_arg_lambda() {
    let expr = first_expr("() -> 42");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 0);
        }
        _ => panic!("expected arrow"),
    }
}

// ── Match Expressions ────────────────────────────────────────

#[test]
fn match_simple() {
    let expr = first_expr("match x { Ok(v) -> v, Err(e) -> e }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_wildcard() {
    let expr = first_expr("match x { _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert!(matches!(arms[0].pattern.kind, PatternKind::Wildcard));
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_nested_variant() {
    let expr = first_expr("match err { Network(Timeout(ms)) -> ms, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
            PatternKind::Variant { name, fields } => {
                assert_eq!(name, "Network");
                let pats: Vec<&Pattern> = fields.patterns().collect();
                assert_eq!(pats.len(), 1);
                assert!(
                    matches!(&pats[0].kind, PatternKind::Variant { name, .. } if name == "Timeout")
                );
            }
            _ => panic!("expected variant pattern"),
        },
        _ => panic!("expected match"),
    }
}

#[test]
fn match_range() {
    let expr = first_expr("match n { 1..10 -> true, _ -> false }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert!(matches!(arms[0].pattern.kind, PatternKind::Range { .. }));
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_record_destructure() {
    let expr = first_expr(r#"match action { Click(el, { x, y }) -> handle(el, x, y) }"#);
    match expr {
        ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
            PatternKind::Variant { fields, .. } => {
                let pats: Vec<&Pattern> = fields.patterns().collect();
                assert_eq!(pats.len(), 2);
                assert!(matches!(&pats[1].kind, PatternKind::Record { .. }));
            }
            _ => panic!("expected variant"),
        },
        _ => panic!("expected match"),
    }
}

// ── Match Guards ─────────────────────────────────────────────

#[test]
fn match_guard_simple() {
    let expr = first_expr("match x { Ok(v) when v > 0 -> v, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(arms[0].guard.is_some());
            assert!(arms[1].guard.is_none());
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_guard_wildcard() {
    let expr = first_expr("match x { _ when x > 10 -> true, _ -> false }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(arms[0].guard.is_some());
            assert!(matches!(arms[0].pattern.kind, PatternKind::Wildcard));
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_guard_no_guard() {
    let expr = first_expr("match x { Ok(v) -> v, Err(e) -> e }");
    match expr {
        ExprKind::Match { arms, .. } => {
            assert!(arms[0].guard.is_none());
            assert!(arms[1].guard.is_none());
        }
        _ => panic!("expected match"),
    }
}

// ── Array Pattern Matching ───────────────────────────────────

#[test]
fn match_array_empty() {
    let expr = first_expr(r#"match items { [] -> "empty", _ -> "other" }"#);
    match expr {
        ExprKind::Match { arms, .. } => {
            assert!(matches!(
                arms[0].pattern.kind,
                PatternKind::Array {
                    ref elements,
                    ref rest
                } if elements.is_empty() && rest.is_none()
            ));
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_array_single_binding() {
    let expr = first_expr("match items { [a] -> a, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            if let PatternKind::Array { elements, rest } = &arms[0].pattern.kind {
                assert_eq!(elements.len(), 1);
                assert!(matches!(elements[0].kind, PatternKind::Binding(ref n) if n == "a"));
                assert!(rest.is_none());
            } else {
                panic!("expected array pattern");
            }
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_array_rest_pattern() {
    let expr = first_expr("match items { [first, ..rest] -> first, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            if let PatternKind::Array { elements, rest } = &arms[0].pattern.kind {
                assert_eq!(elements.len(), 1);
                assert!(matches!(elements[0].kind, PatternKind::Binding(ref n) if n == "first"));
                assert_eq!(rest.as_deref(), Some("rest"));
            } else {
                panic!("expected array pattern");
            }
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_array_two_plus_rest() {
    let expr = first_expr("match items { [a, b, ..rest] -> a, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            if let PatternKind::Array { elements, rest } = &arms[0].pattern.kind {
                assert_eq!(elements.len(), 2);
                assert_eq!(rest.as_deref(), Some("rest"));
            } else {
                panic!("expected array pattern");
            }
        }
        _ => panic!("expected match"),
    }
}

#[test]
fn match_array_wildcard_rest() {
    let expr = first_expr("match items { [_, .._] -> 1, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => {
            if let PatternKind::Array { elements, rest } = &arms[0].pattern.kind {
                assert_eq!(elements.len(), 1);
                assert!(matches!(elements[0].kind, PatternKind::Wildcard));
                assert_eq!(rest.as_deref(), Some("_"));
            } else {
                panic!("expected array pattern");
            }
        }
        _ => panic!("expected match"),
    }
}

// ── Const Declaration ────────────────────────────────────────

#[test]
fn const_decl() {
    match first_item("let x = 42") {
        ItemKind::Const(decl) => {
            assert_eq!(decl.binding, ConstBinding::Name("x".to_string()));
            assert!(!decl.exported);
        }
        other => panic!("expected const, got {other:?}"),
    }
}

#[test]
fn const_decl_typed() {
    match first_item("let x: number = 42") {
        ItemKind::Const(decl) => {
            assert!(decl.type_ann.is_some());
        }
        other => panic!("expected const, got {other:?}"),
    }
}

#[test]
fn export_const() {
    match first_item("export let x = 42") {
        ItemKind::Const(decl) => {
            assert!(decl.exported);
        }
        other => panic!("expected const, got {other:?}"),
    }
}

// ── Function Declaration ─────────────────────────────────────

#[test]
fn function_decl() {
    match first_item("let add(a: number, b: number) -> number = { a + b }") {
        ItemKind::Function(decl) => {
            assert_eq!(decl.name, "add");
            assert_eq!(decl.params.len(), 2);
            assert!(decl.return_type.is_some());
            assert!(!decl.async_fn);
            assert!(decl.type_params.is_empty());
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn generic_function_decl() {
    match first_item("let identity<T>(x: T) -> T = { x }") {
        ItemKind::Function(decl) => {
            assert_eq!(decl.name, "identity");
            assert_eq!(decl.type_params.len(), 1);
            assert_eq!(decl.type_params[0].name, "T");
            assert!(decl.type_params[0].bounds.is_empty());
            assert_eq!(decl.params.len(), 1);
            assert!(decl.return_type.is_some());
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn generic_function_multi_params() {
    match first_item("let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }") {
        ItemKind::Function(decl) => {
            assert_eq!(decl.name, "pair");
            assert_eq!(decl.type_params.len(), 2);
            assert_eq!(decl.type_params[0].name, "A");
            assert_eq!(decl.type_params[1].name, "B");
            assert_eq!(decl.params.len(), 2);
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn generic_function_with_trait_bound() {
    match first_item("let process<R: Display>(repo: R) -> string = { todo }") {
        ItemKind::Function(decl) => {
            assert_eq!(decl.name, "process");
            assert_eq!(decl.type_params.len(), 1);
            assert_eq!(decl.type_params[0].name, "R");
            assert_eq!(decl.type_params[0].bounds, vec!["Display"]);
            assert_eq!(decl.params.len(), 1);
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn partial_application_binds_as_const() {
    // `let inc = add(1, _)` is a value binding whose RHS is a
    // partial-application expression. It's not a lambda literal, so the
    // parser keeps it as a `ConstDecl` — the checker produces the function
    // value via placeholder rewriting downstream.
    let program =
        parse_ok("let add(a: number, b: number) -> number = { a + b }\nlet inc = add(1, _)");
    assert_eq!(program.items.len(), 2);
    match &program.items[1].kind {
        ItemKind::Const(decl) => {
            assert!(matches!(&decl.binding, ConstBinding::Name(n) if n == "inc"));
        }
        other => panic!("expected const binding, got {other:?}"),
    }
}

#[test]
fn promise_return_type_function() {
    match first_item("let fetchUser(id: string) -> Promise<Result<User, ApiError>> = { Ok(user) }")
    {
        ItemKind::Function(decl) => {
            assert!(!decl.async_fn); // async_fn is set by mark_async_functions, not parser
            assert_eq!(decl.name, "fetchUser");
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn async_fn_declaration_sets_async_fn_flag() {
    match first_item("async let fetchUser(id: string) -> Result<User, Error> = { Ok(user) }") {
        ItemKind::Function(decl) => {
            assert!(
                decl.async_fn,
                "parser should set async_fn=true for `async fn`"
            );
            assert_eq!(decl.name, "fetchUser");
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn exported_async_fn_declaration() {
    match first_item("export async let fetchUser(id: string) -> Result<User, Error> = { Ok(user) }")
    {
        ItemKind::Function(decl) => {
            assert!(decl.async_fn);
            assert!(decl.exported);
            assert_eq!(decl.name, "fetchUser");
        }
        other => panic!("expected function, got {other:?}"),
    }
}

#[test]
fn function_with_defaults() {
    match first_item("let f(x: number = 10) = { x }") {
        ItemKind::Function(decl) => {
            assert!(decl.params[0].default.is_some());
        }
        other => panic!("expected function, got {other:?}"),
    }
}

// ── Import ───────────────────────────────────────────────────

#[test]
fn import_named() {
    match first_item(r#"import { useState, useEffect } from "react""#) {
        ItemKind::Import(decl) => {
            assert_eq!(decl.specifiers.len(), 2);
            assert_eq!(decl.specifiers[0].name, "useState");
            assert_eq!(decl.specifiers[1].name, "useEffect");
            assert_eq!(decl.source, "react");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

#[test]
fn import_trusted_all() {
    match first_item(r#"import trusted { capitalize, slugify } from "string-utils""#) {
        ItemKind::Import(decl) => {
            assert!(decl.trusted);
            assert_eq!(decl.specifiers.len(), 2);
            assert_eq!(decl.specifiers[0].name, "capitalize");
            assert_eq!(decl.specifiers[1].name, "slugify");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

#[test]
fn import_trusted_per_specifier() {
    match first_item(r#"import { trusted capitalize, fetchUser } from "some-lib""#) {
        ItemKind::Import(decl) => {
            assert!(!decl.trusted);
            assert_eq!(decl.specifiers.len(), 2);
            assert!(decl.specifiers[0].trusted);
            assert_eq!(decl.specifiers[0].name, "capitalize");
            assert!(!decl.specifiers[1].trusted);
            assert_eq!(decl.specifiers[1].name, "fetchUser");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

// ── Type Declarations ────────────────────────────────────────

#[test]
fn type_alias() {
    match first_item("type StringAlias = string") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "StringAlias");
            assert!(matches!(decl.def, TypeDef::Alias(_)));
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_record() {
    match first_item("type User = { id: UserId, name: string }") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "User");
            match decl.def {
                TypeDef::Record(ref entries) => assert_eq!(entries.len(), 2),
                ref other => panic!("expected record, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_record_pascal_case_fields() {
    match first_item("type HonoEnv = { Bindings: Env, Variables: Vars }") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "HonoEnv");
            match decl.def {
                TypeDef::Record(ref entries) => {
                    assert_eq!(entries.len(), 2);
                    assert_eq!(entries[0].as_field().unwrap().name, "Bindings");
                    assert_eq!(entries[1].as_field().unwrap().name, "Variables");
                }
                ref other => panic!("expected record, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_record_mixed_case_fields() {
    match first_item("type X = { DB: string, fooBar: number, BAZ: boolean }") {
        ItemKind::TypeDecl(decl) => match decl.def {
            TypeDef::Record(ref entries) => {
                assert_eq!(entries.len(), 3);
                assert_eq!(entries[0].as_field().unwrap().name, "DB");
                assert_eq!(entries[1].as_field().unwrap().name, "fooBar");
                assert_eq!(entries[2].as_field().unwrap().name, "BAZ");
            }
            ref other => panic!("expected record, got {other:?}"),
        },
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_newtype_with_pascal_wrapper_parses_as_union() {
    match first_item("type OrderId = OrderId(Number)") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "OrderId");
            assert!(
                matches!(decl.def, TypeDef::Union(_)),
                "newtype should lower to a single-variant union, got {:?}",
                decl.def
            );
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_record_with_spread() {
    match first_item("type B = { ...A, extra: string }") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "B");
            match decl.def {
                TypeDef::Record(ref entries) => {
                    assert_eq!(entries.len(), 2);
                    assert!(entries[0].as_spread().is_some());
                    assert_eq!(entries[0].as_spread().unwrap().type_name, "A");
                    assert!(entries[1].as_field().is_some());
                    assert_eq!(entries[1].as_field().unwrap().name, "extra");
                }
                ref other => panic!("expected record, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_record_with_multiple_spreads() {
    match first_item("type C = { ...A, ...B, extra: string }") {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "C");
            match decl.def {
                TypeDef::Record(ref entries) => {
                    assert_eq!(entries.len(), 3);
                    assert_eq!(entries[0].as_spread().unwrap().type_name, "A");
                    assert_eq!(entries[1].as_spread().unwrap().type_name, "B");
                    assert_eq!(entries[2].as_field().unwrap().name, "extra");
                }
                ref other => panic!("expected record, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_union() {
    let input = r#"type Route = | Home | Profile { id: string } | NotFound"#;
    match first_item(input) {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "Route");
            match decl.def {
                TypeDef::Union(variants) => {
                    assert_eq!(variants.len(), 3);
                    assert_eq!(variants[0].name, "Home");
                    assert!(variants[0].fields.is_empty());
                    assert_eq!(variants[1].name, "Profile");
                    assert_eq!(variants[1].fields.len(), 1);
                    assert_eq!(variants[2].name, "NotFound");
                }
                other => panic!("expected union, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn type_union_positional_fields() {
    let input = r#"type Route = | Home | Profile(string) | Error(number, string) | Settings { tab: string }"#;
    match first_item(input) {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "Route");
            match decl.def {
                TypeDef::Union(variants) => {
                    assert_eq!(variants.len(), 4);

                    // Home — unit variant
                    assert_eq!(variants[0].name, "Home");
                    assert!(variants[0].fields.is_empty());

                    // Profile(string) — single positional field
                    assert_eq!(variants[1].name, "Profile");
                    assert_eq!(variants[1].fields.len(), 1);
                    assert!(variants[1].fields[0].name.is_none());

                    // Error(number, string) — multiple positional fields
                    assert_eq!(variants[2].name, "Error");
                    assert_eq!(variants[2].fields.len(), 2);
                    assert!(variants[2].fields[0].name.is_none());
                    assert!(variants[2].fields[1].name.is_none());

                    // Settings { tab: string } — named field (existing syntax)
                    assert_eq!(variants[3].name, "Settings");
                    assert_eq!(variants[3].fields.len(), 1);
                    assert_eq!(variants[3].fields[0].name.as_deref(), Some("tab"));
                }
                other => panic!("expected union, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn opaque_type() {
    match first_item("opaque type HashedPassword = string") {
        ItemKind::TypeDecl(decl) => {
            assert!(decl.opaque);
            assert_eq!(decl.name, "HashedPassword");
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn variant_named_field_in_parens_is_error() {
    let errs = parse("type Shape = | Circle(radius: number)").unwrap_err();
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("named fields are not allowed in `(...)` variants")),
        "expected targeted error about named-in-parens, got: {errs:?}"
    );
}

#[test]
fn variant_positional_field_in_braces_is_error() {
    let errs = parse("type Shape = | Rectangle { number, number }").unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.message.contains("`{...}` variants require named fields")),
        "expected targeted error about positional-in-braces, got: {errs:?}"
    );
}

#[test]
fn newtype_paren_rejects_named_field() {
    let errs = parse("type UserId = UserId(id: string)").unwrap_err();
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("named fields are not allowed in `(...)` variants")),
        "expected targeted error for named-in-parens newtype, got: {errs:?}"
    );
}

#[test]
fn named_variant_pattern_parses() {
    let expr = first_expr("match r { Rect { width, height: h } -> width + h, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
            PatternKind::Variant { name, fields } => {
                assert_eq!(name, "Rect");
                match fields {
                    VariantPatternFields::Named(named) => {
                        assert_eq!(named.len(), 2);
                        assert_eq!(named[0].0, "width");
                        assert!(
                            matches!(&named[0].1.kind, PatternKind::Binding(n) if n == "width")
                        );
                        assert_eq!(named[1].0, "height");
                        assert!(matches!(&named[1].1.kind, PatternKind::Binding(n) if n == "h"));
                    }
                    other => panic!("expected named fields, got {other:?}"),
                }
            }
            other => panic!("expected variant pattern, got {other:?}"),
        },
        other => panic!("expected match, got {other:?}"),
    }
}

#[test]
fn positional_variant_pattern_parses() {
    let expr = first_expr("match c { Circle(r) -> r, _ -> 0 }");
    match expr {
        ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
            PatternKind::Variant { name, fields } => {
                assert_eq!(name, "Circle");
                assert!(matches!(fields, VariantPatternFields::Positional(_)));
            }
            other => panic!("expected variant pattern, got {other:?}"),
        },
        other => panic!("expected match, got {other:?}"),
    }
}

// ── Member Access ────────────────────────────────────────────

#[test]
fn member_access() {
    let expr = first_expr("a.b.c");
    match expr {
        ExprKind::Member { object, field } => {
            assert_eq!(field, "c");
            assert!(matches!(object.kind, ExprKind::Member { field: ref f, .. } if f == "b"));
        }
        _ => panic!("expected member access"),
    }
}

// ── Array Literal ────────────────────────────────────────────

#[test]
fn array_literal() {
    let expr = first_expr("[1, 2, 3]");
    match expr {
        ExprKind::Array(elements) => {
            assert_eq!(elements.len(), 3);
        }
        _ => panic!("expected array"),
    }
}

// ── Index Access ─────────────────────────────────────────────

#[test]
fn index_access() {
    let expr = first_expr("arr[0]");
    assert!(matches!(expr, ExprKind::Index { .. }));
}

// ── JSX ──────────────────────────────────────────────────────

#[test]
fn jsx_self_closing() {
    let expr = first_expr("<Button />");
    match expr {
        ExprKind::Jsx(JsxElement {
            kind: JsxElementKind::Element {
                name, self_closing, ..
            },
            ..
        }) => {
            assert_eq!(name, "Button");
            assert!(self_closing);
        }
        _ => panic!("expected jsx element"),
    }
}

#[test]
fn jsx_with_props() {
    let expr = first_expr(r#"<Button label="Save" onClick={handleSave} />"#);
    match expr {
        ExprKind::Jsx(JsxElement {
            kind: JsxElementKind::Element { props, .. },
            ..
        }) => {
            assert_eq!(props.len(), 2);
            assert!(matches!(&props[0], JsxProp::Named { name, .. } if name == "label"));
            assert!(matches!(&props[1], JsxProp::Named { name, .. } if name == "onClick"));
        }
        _ => panic!("expected jsx element"),
    }
}

#[test]
fn jsx_hyphenated_prop_names() {
    let expr = first_expr(r#"<Input aria-label="Share link" data-testid="input" />"#);
    match expr {
        ExprKind::Jsx(JsxElement {
            kind: JsxElementKind::Element { props, .. },
            ..
        }) => {
            assert_eq!(props.len(), 2);
            assert!(matches!(&props[0], JsxProp::Named { name, .. } if name == "aria-label"));
            assert!(matches!(&props[1], JsxProp::Named { name, .. } if name == "data-testid"));
        }
        _ => panic!("expected jsx element"),
    }
}

#[test]
fn jsx_with_children() {
    let expr = first_expr("<div>{x}</div>");
    match expr {
        ExprKind::Jsx(JsxElement {
            kind: JsxElementKind::Element { children, .. },
            ..
        }) => {
            assert_eq!(children.len(), 1);
            assert!(matches!(&children[0], JsxChild::Expr(_)));
        }
        _ => panic!("expected jsx element"),
    }
}

#[test]
fn jsx_fragment() {
    let expr = first_expr("<>{x}</>");
    match expr {
        ExprKind::Jsx(JsxElement {
            kind: JsxElementKind::Fragment { children },
            ..
        }) => {
            assert_eq!(children.len(), 1);
        }
        _ => panic!("expected fragment"),
    }
}

// ── Banned Keywords ──────────────────────────────────────────

#[test]
fn banned_keyword_error() {
    let result = parse("const x = 5");
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors[0].message.contains("banned keyword"));
}

// ── Block & Implicit Return ──────────────────────────────────

#[test]
fn block_with_implicit_return() {
    match first_item("let f() = { let x = 1\nx }") {
        ItemKind::Function(decl) => match decl.body.kind {
            ExprKind::Block(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("expected block"),
        },
        other => panic!("expected function, got {other:?}"),
    }
}

// ── Pipe with placeholder ────────────────────────────────────

#[test]
fn pipe_with_placeholder() {
    let expr = first_expr("x |> f(y, _, z)");
    match expr {
        ExprKind::Pipe { right, .. } => match right.kind {
            ExprKind::Call { args, .. } => {
                assert_eq!(args.len(), 3);
                assert!(
                    matches!(&args[1], Arg::Positional(e) if matches!(e.kind, ExprKind::Placeholder))
                );
            }
            _ => panic!("expected call in pipe rhs"),
        },
        _ => panic!("expected pipe"),
    }
}

// ── Promise.await (stdlib, no keyword) ──────────────────────

#[test]
fn promise_await_is_member_access() {
    // `Promise.await` is now a stdlib function, not a keyword.
    // It parses as a regular member access expression.
    let expr = first_expr("Promise.await");
    assert!(matches!(expr, ExprKind::Member { .. }));
}

// ── If/Else is Banned ────────────────────────────────────────

#[test]
fn if_is_banned() {
    let result = parse("if x { 1 } else { 2 }");
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("banned keyword")),
        "expected banned keyword error for `if`, got: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// ── Grouped Expression ───────────────────────────────────────

#[test]
fn grouped() {
    let expr = first_expr("(1 + 2)");
    assert!(matches!(expr, ExprKind::Grouped(_)));
}

// ── Type Expression ──────────────────────────────────────────

#[test]
fn generic_type() {
    match first_item("let x: Result<User, ApiError> = Ok(user)") {
        ItemKind::Const(decl) => {
            let type_ann = decl.type_ann.unwrap();
            match type_ann.kind {
                TypeExprKind::Named {
                    name, type_args, ..
                } => {
                    assert_eq!(name, "Result");
                    assert_eq!(type_args.len(), 2);
                }
                _ => panic!("expected named type"),
            }
        }
        other => panic!("expected const, got {other:?}"),
    }
}

// ── Generic Call: object-type literal as type argument ──────

#[test]
fn generic_call_with_object_type_literal() {
    let expr = first_expr("foo<{ bindings: Env }>()");
    match expr {
        ExprKind::Call {
            type_args, args, ..
        } => {
            assert_eq!(type_args.len(), 1);
            assert!(
                matches!(type_args[0].kind, TypeExprKind::Record { .. }),
                "expected record type arg, got {:?}",
                type_args[0].kind,
            );
            assert!(args.is_empty());
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn generic_call_with_nested_object_type_literal() {
    let expr = first_expr("foo<{ outer: { inner: number } }>()");
    assert!(matches!(expr, ExprKind::Call { .. }));
}

#[test]
fn generic_call_with_multiple_type_args_including_object() {
    let expr = first_expr(r#"foo<string, { k: number }>()"#);
    match expr {
        ExprKind::Call { type_args, .. } => {
            assert_eq!(type_args.len(), 2);
            assert!(matches!(type_args[1].kind, TypeExprKind::Record { .. }));
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn generic_call_with_named_type_arg() {
    let expr = first_expr("foo<Env>()");
    match expr {
        ExprKind::Call { type_args, .. } => {
            assert_eq!(type_args.len(), 1);
            assert!(matches!(type_args[0].kind, TypeExprKind::Named { .. }));
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn comparison_followed_by_block_is_not_generic_call() {
    // `f < x > { ... }` is not a generic call — no `(` after the `>`.
    // The `{` after `>` disambiguates.
    let input = r#"
let check() -> boolean = {
    let r = f < x
    r
}
"#;
    parse_ok(input);
}

// ── Full program ─────────────────────────────────────────────

#[test]
fn full_program() {
    let input = r#"
import { useState } from "react"

type Todo = { id: string, text: string, done: boolean }

export let TodoApp() = {
    let (todos, setTodos) = useState([])
    <div>{todos |> map((t) -> <li>{t.text}</li>)}</div>
}
"#;
    let program = parse_ok(input);
    assert_eq!(program.items.len(), 3);
}

// ── For Blocks ──────────────────────────────────────────────

#[test]
fn for_block_basic() {
    let input = r#"
type User = { name: string }
for User {
    let display(self) -> string = {
        self.name
    }
}
"#;
    let program = parse_ok(input);
    assert_eq!(program.items.len(), 2);
    match &program.items[1].kind {
        ItemKind::ForBlock(block) => {
            assert_eq!(block.functions.len(), 1);
            assert_eq!(block.functions[0].name, "display");
            assert_eq!(block.functions[0].params.len(), 1);
            assert_eq!(block.functions[0].params[0].name, "self");
            assert!(block.functions[0].params[0].type_ann.is_none());
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn for_block_multiple_functions() {
    let input = r#"
type User = { name: string, age: number }
for User {
    let display(self) -> string = { self.name }
    let isAdult(self) -> bool = { self.age >= 18 }
    let greet(self, greeting: string) -> string = { `${greeting}` }
}
"#;
    let program = parse_ok(input);
    match &program.items[1].kind {
        ItemKind::ForBlock(block) => {
            assert_eq!(block.functions.len(), 3);
            assert_eq!(block.functions[0].name, "display");
            assert_eq!(block.functions[1].name, "isAdult");
            assert_eq!(block.functions[2].name, "greet");
            assert_eq!(block.functions[2].params.len(), 2);
            assert_eq!(block.functions[2].params[0].name, "self");
            assert_eq!(block.functions[2].params[1].name, "greeting");
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn for_block_generic_type() {
    let input = r#"
for Array<User> {
    let adults(self) -> Array<User> = { self }
}
"#;
    let program = parse_ok(input);
    match &program.items[0].kind {
        ItemKind::ForBlock(block) => {
            match &block.type_name.kind {
                TypeExprKind::Named {
                    name, type_args, ..
                } => {
                    assert_eq!(name, "Array");
                    assert_eq!(type_args.len(), 1);
                }
                other => panic!("expected Named type, got {other:?}"),
            }
            assert_eq!(block.functions.len(), 1);
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn self_as_expression() {
    let input = r#"
type User = { name: string }
for User {
    let getName(self) -> string = { self.name }
}
"#;
    let program = parse_ok(input);
    match &program.items[1].kind {
        ItemKind::ForBlock(block) => {
            // The body should contain self.name as a member expression
            let body = &block.functions[0].body;
            match &body.kind {
                ExprKind::Block(items) => match &items[0].kind {
                    ItemKind::Expr(expr) => {
                        assert!(matches!(&expr.kind, ExprKind::Member { .. }));
                    }
                    other => panic!("expected Expr item, got {other:?}"),
                },
                other => panic!("expected Block, got {other:?}"),
            }
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn for_block_error_non_fn() {
    let result = parse("for User { let x = 1 }");
    assert!(result.is_err());
}

// ── Block-level export on for-blocks (#1089) ─────────────────

#[test]
fn export_for_block_marks_all_methods_exported() {
    let input = r#"
type User = { name: string }
export for User {
    let display(self) -> string = { self.name }
    let shout(self) -> string = { self.name }
}
"#;
    let program = parse_ok(input);
    match &program.items[1].kind {
        ItemKind::ForBlock(block) => {
            assert_eq!(block.functions.len(), 2);
            assert!(
                block.functions.iter().all(|f| f.exported),
                "all methods in `export for` block should be exported",
            );
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn export_for_trait_impl_marks_all_methods_exported() {
    let input = r#"
type User = { name: string }
trait Display { let display(self) -> string }
export for User: Display {
    let display(self) -> string = { self.name }
}
"#;
    let program = parse_ok(input);
    match &program.items[2].kind {
        ItemKind::ForBlock(block) => {
            assert_eq!(block.trait_name.as_deref(), Some("Display"));
            assert!(block.functions.iter().all(|f| f.exported));
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

#[test]
fn per_method_export_still_works_on_for_block() {
    let input = r#"
type User = { name: string }
for User {
    export let display(self) -> string = { self.name }
    let helper(self) -> string = { self.name }
}
"#;
    let program = parse_ok(input);
    match &program.items[1].kind {
        ItemKind::ForBlock(block) => {
            assert!(block.functions[0].exported);
            assert!(!block.functions[1].exported);
        }
        other => panic!("expected ForBlock, got {other:?}"),
    }
}

// (Inline for-declaration tests removed — only block form is supported)

// ── Import { for Type } ────────────────────────────────────

#[test]
fn import_for_type() {
    let input = r#"import { for User } from "./helpers""#;
    match first_item(input) {
        ItemKind::Import(decl) => {
            assert!(decl.specifiers.is_empty());
            assert_eq!(decl.for_specifiers.len(), 1);
            assert_eq!(decl.for_specifiers[0].type_name, "User");
            assert_eq!(decl.source, "./helpers");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

#[test]
fn import_multiple_for_types() {
    let input = r#"import { for Array, for Map } from "./todo""#;
    match first_item(input) {
        ItemKind::Import(decl) => {
            assert!(decl.specifiers.is_empty());
            assert_eq!(decl.for_specifiers.len(), 2);
            assert_eq!(decl.for_specifiers[0].type_name, "Array");
            assert_eq!(decl.for_specifiers[1].type_name, "Map");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

#[test]
fn import_mixed_names_and_for_types() {
    let input = r#"import { Todo, Filter, for Array, for string } from "./todo""#;
    match first_item(input) {
        ItemKind::Import(decl) => {
            assert_eq!(decl.specifiers.len(), 2);
            assert_eq!(decl.specifiers[0].name, "Todo");
            assert_eq!(decl.specifiers[1].name, "Filter");
            assert_eq!(decl.for_specifiers.len(), 2);
            assert_eq!(decl.for_specifiers[0].type_name, "Array");
            assert_eq!(decl.for_specifiers[1].type_name, "string");
        }
        other => panic!("expected import, got {other:?}"),
    }
}

// ── Re-exports ──────────────────────────────────────────────

#[test]
fn reexport_named() {
    let input = r#"export { Card, CardContent } from "@heroui/react""#;
    match first_item(input) {
        ItemKind::ReExport(decl) => {
            assert_eq!(decl.specifiers.len(), 2);
            assert_eq!(decl.specifiers[0].name, "Card");
            assert_eq!(decl.specifiers[1].name, "CardContent");
            assert_eq!(decl.source, "@heroui/react");
        }
        other => panic!("expected re-export, got {other:?}"),
    }
}

#[test]
fn reexport_with_alias() {
    let input = r#"export { Todo as TodoItem } from "./types""#;
    match first_item(input) {
        ItemKind::ReExport(decl) => {
            assert_eq!(decl.specifiers.len(), 1);
            assert_eq!(decl.specifiers[0].name, "Todo");
            assert_eq!(decl.specifiers[0].alias, Some("TodoItem".to_string()));
            assert_eq!(decl.source, "./types");
        }
        other => panic!("expected re-export, got {other:?}"),
    }
}

// ── Test Blocks ─────────────────────────────────────────────

#[test]
fn test_block_basic() {
    let program = parse_ok(
        r#"
test "addition" {
    assert 1 == 1
}
"#,
    );
    match &program.items[0].kind {
        ItemKind::TestBlock(block) => {
            assert_eq!(block.name, "addition");
            assert_eq!(block.body.len(), 1);
            assert!(matches!(block.body[0], TestStatement::Assert(_, _)));
        }
        other => panic!("expected TestBlock, got {other:?}"),
    }
}

// ── Tuple Expressions ──────────────────────────────────────

#[test]
fn tuple_two_elements() {
    match first_expr("(1, 2)") {
        ExprKind::Tuple(elements) => {
            assert_eq!(elements.len(), 2);
            assert!(matches!(&elements[0].kind, ExprKind::Number(n) if n == "1"));
            assert!(matches!(&elements[1].kind, ExprKind::Number(n) if n == "2"));
        }
        other => panic!("expected tuple, got {other:?}"),
    }
}

#[test]
fn test_block_multiple_asserts() {
    let program = parse_ok(
        r#"
test "math" {
    assert 1 + 1 == 2
    assert 3 > 2
    assert true
}
"#,
    );
    match &program.items[0].kind {
        ItemKind::TestBlock(block) => {
            assert_eq!(block.name, "math");
            assert_eq!(block.body.len(), 3);
        }
        other => panic!("expected TestBlock, got {other:?}"),
    }
}

#[test]
fn tuple_three_elements() {
    match first_expr(r#"("a", 1, true)"#) {
        ExprKind::Tuple(elements) => {
            assert_eq!(elements.len(), 3);
        }
        other => panic!("expected tuple, got {other:?}"),
    }
}

#[test]
fn test_as_identifier() {
    // `test` should still work as a regular identifier (function name, variable, etc.)
    let program = parse_ok(
        r#"
let test() -> number = { 1 }
"#,
    );
    match &program.items[0].kind {
        ItemKind::Function(decl) => {
            assert_eq!(decl.name, "test");
        }
        other => panic!("expected Function, got {other:?}"),
    }
}

#[test]
fn test_block_with_function_calls() {
    let program = parse_ok(
        r#"
let add(a: number, b: number) -> number = { a + b }

test "add function" {
    assert add(1, 2) == 3
    assert add(0, 0) == 0
}
"#,
    );
    assert_eq!(program.items.len(), 2);
    assert!(matches!(program.items[0].kind, ItemKind::Function(_)));
    assert!(matches!(program.items[1].kind, ItemKind::TestBlock(_)));
}

#[test]
fn tuple_trailing_comma() {
    match first_expr("(1, 2,)") {
        ExprKind::Tuple(elements) => {
            assert_eq!(elements.len(), 2);
        }
        other => panic!("expected tuple, got {other:?}"),
    }
}

#[test]
fn grouped_not_tuple() {
    // Single element without comma is grouped, not tuple
    match first_expr("(42)") {
        ExprKind::Grouped(_) => {}
        other => panic!("expected grouped, got {other:?}"),
    }
}

#[test]
fn unit_not_tuple() {
    // Empty parens is unit, not tuple
    match first_expr("()") {
        ExprKind::Unit => {}
        other => panic!("expected unit, got {other:?}"),
    }
}

#[test]
fn tuple_destructuring() {
    match first_item("let (x, y) = point") {
        ItemKind::Const(decl) => {
            assert_eq!(
                decl.binding,
                ConstBinding::Tuple(vec!["x".to_string(), "y".to_string()])
            );
        }
        other => panic!("expected const, got {other:?}"),
    }
}

#[test]
fn array_destructure_in_const_is_parse_error() {
    let errs = parse("let [a, b] = pair").expect_err("array destructure should not parse");
    assert!(
        errs.iter().any(|e| e.to_string().contains("identifier")),
        "expected an 'expected identifier' style parse error, got: {:?}",
        errs.iter().map(|e| e.to_string()).collect::<Vec<_>>()
    );
}

// ── Tuple Patterns ──────────────────────────────────────────

#[test]
fn tuple_pattern_in_match() {
    let program = parse_ok(
        r#"
        match point {
            (0, 0) -> "origin",
            (x, y) -> "other",
        }
    "#,
    );
    match &program.items[0].kind {
        ItemKind::Expr(e) => match &e.kind {
            ExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                match &arms[0].pattern.kind {
                    PatternKind::Tuple(patterns) => {
                        assert_eq!(patterns.len(), 2);
                        assert!(
                            matches!(&patterns[0].kind, PatternKind::Literal(LiteralPattern::Number(n)) if n == "0")
                        );
                    }
                    other => panic!("expected tuple pattern, got {other:?}"),
                }
                match &arms[1].pattern.kind {
                    PatternKind::Tuple(patterns) => {
                        assert_eq!(patterns.len(), 2);
                        assert!(matches!(&patterns[0].kind, PatternKind::Binding(n) if n == "x"));
                    }
                    other => panic!("expected tuple pattern, got {other:?}"),
                }
            }
            other => panic!("expected match, got {other:?}"),
        },
        other => panic!("expected expr, got {other:?}"),
    }
}

// ── Tuple Type Expressions ──────────────────────────────────

#[test]
fn tuple_type_annotation() {
    match first_item("let p: (number, string) = (1, \"a\")") {
        ItemKind::Const(decl) => {
            let type_ann = decl.type_ann.unwrap();
            match &type_ann.kind {
                TypeExprKind::Tuple(types) => {
                    assert_eq!(types.len(), 2);
                }
                other => panic!("expected tuple type, got {other:?}"),
            }
        }
        other => panic!("expected const, got {other:?}"),
    }
}

#[test]
fn tuple_return_type() {
    match first_item("let f(a: number) -> (number, string) = { (a, \"x\") }") {
        ItemKind::Function(decl) => {
            let ret = decl.return_type.unwrap();
            match &ret.kind {
                TypeExprKind::Tuple(types) => {
                    assert_eq!(types.len(), 2);
                }
                other => panic!("expected tuple type, got {other:?}"),
            }
        }
        other => panic!("expected function, got {other:?}"),
    }
}

// ── Bug: Pipe precedence vs equality ────────────────────────
// `x |> f == y` should parse as `(x |> f) == y`, not `x |> (f == y)`

#[test]
fn pipe_binds_tighter_than_equality() {
    let expr = first_expr(r#""" |> validate == Empty"#);
    // Should be: Binary { left: Pipe { ... }, op: Eq, right: Identifier("Empty") }
    match expr {
        ExprKind::Binary {
            op: BinOp::Eq,
            left,
            right,
            ..
        } => {
            assert!(
                matches!(left.kind, ExprKind::Pipe { .. }),
                "left side of == should be a pipe, got {:?}",
                left.kind
            );
            assert!(
                matches!(right.kind, ExprKind::Identifier(ref name) if name == "Empty"),
                "right side of == should be Empty, got {:?}",
                right.kind
            );
        }
        other => panic!("expected binary ==, got {other:?}"),
    }
}

#[test]
fn pipe_binds_tighter_than_not_equal() {
    let expr = first_expr("x |> f != y");
    match expr {
        ExprKind::Binary {
            op: BinOp::NotEq,
            left,
            ..
        } => {
            assert!(matches!(left.kind, ExprKind::Pipe { .. }));
        }
        other => panic!("expected binary !=, got {other:?}"),
    }
}

#[test]
fn pipe_binds_tighter_than_logical_and() {
    let expr = first_expr("x |> f && y |> g");
    match expr {
        ExprKind::Binary {
            op: BinOp::And,
            left,
            right,
            ..
        } => {
            assert!(matches!(left.kind, ExprKind::Pipe { .. }));
            assert!(matches!(right.kind, ExprKind::Pipe { .. }));
        }
        other => panic!("expected binary &&, got {other:?}"),
    }
}

// ── Bug: Object literal syntax ──────────────────────────────
// `{ key: value, key2: value2 }` should parse as an object literal

#[test]
fn object_literal_basic() {
    let expr = first_expr(r#"{ name: "Alice", age: 30 }"#);
    match expr {
        ExprKind::Object(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "name");
            assert_eq!(fields[1].0, "age");
        }
        other => panic!("expected object literal, got {other:?}"),
    }
}

#[test]
fn object_literal_nested() {
    let expr = first_expr(r#"{ queries: { staleTime: 60000 } }"#);
    match expr {
        ExprKind::Object(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0, "queries");
            assert!(matches!(fields[0].1.kind, ExprKind::Object(_)));
        }
        other => panic!("expected object literal, got {other:?}"),
    }
}

#[test]
fn object_literal_in_call() {
    let expr = first_expr(r#"f({ key: "value" })"#);
    match expr {
        ExprKind::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            match &args[0] {
                Arg::Positional(e) => {
                    assert!(
                        matches!(e.kind, ExprKind::Object(_)),
                        "expected object literal in call arg, got {:?}",
                        e.kind
                    );
                }
                other => panic!("expected positional arg, got {other:?}"),
            }
        }
        other => panic!("expected call, got {other:?}"),
    }
}

#[test]
fn object_literal_shorthand() {
    // { name } should be shorthand for { name: name }
    let expr = first_expr("{ name, age }");
    match expr {
        ExprKind::Object(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "name");
            assert!(matches!(fields[0].1.kind, ExprKind::Identifier(ref n) if n == "name"));
            assert_eq!(fields[1].0, "age");
            assert!(matches!(fields[1].1.kind, ExprKind::Identifier(ref n) if n == "age"));
        }
        other => panic!("expected object literal, got {other:?}"),
    }
}

// ── Bug #334: Object literal keys should not be treated as variable refs ──

#[test]
fn object_literal_value_is_number_not_key() {
    // `{ staleTime: 60000 }` — the value should be Number("60000"), not Identifier("staleTime")
    let expr = first_expr("{ staleTime: 60000, retry: 1 }");
    match expr {
        ExprKind::Object(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "staleTime");
            assert!(
                matches!(fields[0].1.kind, ExprKind::Number(ref n) if n == "60000"),
                "expected Number(60000), got {:?}",
                fields[0].1.kind
            );
            assert_eq!(fields[1].0, "retry");
            assert!(
                matches!(fields[1].1.kind, ExprKind::Number(ref n) if n == "1"),
                "expected Number(1), got {:?}",
                fields[1].1.kind
            );
        }
        other => panic!("expected object literal, got {other:?}"),
    }
}

#[test]
fn object_literal_value_is_string_not_key() {
    let expr = first_expr(r#"{ name: "Alice" }"#);
    match expr {
        ExprKind::Object(fields) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0, "name");
            assert!(
                matches!(fields[0].1.kind, ExprKind::String(ref s) if s == "Alice"),
                "expected String(Alice), got {:?}",
                fields[0].1.kind
            );
        }
        other => panic!("expected object literal, got {other:?}"),
    }
}

// ── Bug #333: Lambda with object destructured params ────────

#[test]
fn lambda_object_destructure_binds_variables() {
    // `|{ x, y }| x + y` — x and y should be bound by the destructure
    let expr = first_expr("({ x, y }) -> x + y");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 1);
            let param = &params[0];
            match &param.destructure {
                Some(ParamDestructure::Object(fields)) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].field, "x");
                    assert_eq!(fields[0].alias, None);
                    assert_eq!(fields[1].field, "y");
                    assert_eq!(fields[1].alias, None);
                }
                other => panic!("expected Object destructure, got {other:?}"),
            }
        }
        other => panic!("expected arrow, got {other:?}"),
    }
}

// ── Destructuring rename ─────────────────────────────────────

#[test]
fn const_object_destructure_with_rename() {
    let program = parse_ok("let { data: rows, error: err } = response");
    let item = &program.items[0];
    match &item.kind {
        ItemKind::Const(decl) => match &decl.binding {
            ConstBinding::Object(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field, "data");
                assert_eq!(fields[0].alias, Some("rows".to_string()));
                assert_eq!(fields[1].field, "error");
                assert_eq!(fields[1].alias, Some("err".to_string()));
            }
            other => panic!("expected Object binding, got {other:?}"),
        },
        other => panic!("expected Const, got {other:?}"),
    }
}

#[test]
fn const_object_destructure_mixed_rename_and_plain() {
    let program = parse_ok("let { data: rows, error } = response");
    let item = &program.items[0];
    match &item.kind {
        ItemKind::Const(decl) => match &decl.binding {
            ConstBinding::Object(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field, "data");
                assert_eq!(fields[0].alias, Some("rows".to_string()));
                assert_eq!(fields[1].field, "error");
                assert_eq!(fields[1].alias, None);
            }
            other => panic!("expected Object binding, got {other:?}"),
        },
        other => panic!("expected Const, got {other:?}"),
    }
}

#[test]
fn lambda_object_destructure_with_rename() {
    let expr = first_expr("({ data: d, error: e }) -> d");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 1);
            match &params[0].destructure {
                Some(ParamDestructure::Object(fields)) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].field, "data");
                    assert_eq!(fields[0].alias, Some("d".to_string()));
                    assert_eq!(fields[1].field, "error");
                    assert_eq!(fields[1].alias, Some("e".to_string()));
                }
                other => panic!("expected Object destructure, got {other:?}"),
            }
        }
        other => panic!("expected arrow, got {other:?}"),
    }
}

// ── Bug: Lambda parameter destructuring ─────────────────────
// `|{ x, y }| expr` should parse with destructured params

#[test]
fn non_async_lambda_is_not_async() {
    let expr = first_expr("() -> 42");
    match expr {
        ExprKind::Arrow { async_fn, .. } => {
            assert!(
                !async_fn,
                "parser always sets async_fn=false, mark_async_functions infers it later"
            );
        }
        other => panic!("expected arrow, got {other:?}"),
    }
}

#[test]
fn lambda_destructured_param() {
    let expr = first_expr("({ name, age }) -> name");
    match expr {
        ExprKind::Arrow { params, .. } => {
            assert_eq!(params.len(), 1);
            assert!(
                params[0].destructure.is_some(),
                "expected destructured param, got plain param: {:?}",
                params[0]
            );
        }
        other => panic!("expected arrow, got {other:?}"),
    }
}

// ── Pipe into Match ────────────────────────────────────────────

#[test]
fn pipe_into_match_simple() {
    // `x |> match { _ -> 1 }` should desugar to `match x { _ -> 1 }`
    let expr = first_expr("x |> match {\n    _ -> 1,\n}");
    match expr {
        ExprKind::Match { subject, arms } => {
            assert!(
                matches!(subject.kind, ExprKind::Identifier(ref name) if name == "x"),
                "expected subject to be 'x', got {:?}",
                subject.kind
            );
            assert_eq!(arms.len(), 1);
        }
        other => panic!("expected match, got {other:?}"),
    }
}

#[test]
fn pipe_into_match_multiple_arms() {
    let expr = first_expr(
        r#"price |> match {
        _ when _ < 10 -> "cheap",
        _ when _ < 100 -> "moderate",
        _ -> "expensive",
    }"#,
    );
    match expr {
        ExprKind::Match { subject, arms } => {
            assert!(
                matches!(subject.kind, ExprKind::Identifier(ref name) if name == "price"),
                "expected subject to be 'price', got {:?}",
                subject.kind
            );
            assert_eq!(arms.len(), 3);
        }
        other => panic!("expected match, got {other:?}"),
    }
}

#[test]
fn pipe_chain_into_match() {
    // `x |> f |> match { _ -> 1 }` should parse as `match (x |> f) { _ -> 1 }`
    let expr = first_expr("x |> f |> match {\n    _ -> 1,\n}");
    match expr {
        ExprKind::Match { subject, arms } => {
            assert!(
                matches!(subject.kind, ExprKind::Pipe { .. }),
                "expected subject to be a pipe, got {:?}",
                subject.kind
            );
            assert_eq!(arms.len(), 1);
        }
        other => panic!("expected match, got {other:?}"),
    }
}

// ── String Literal Unions ───────────────────────────────────

#[test]
fn string_literal_union() {
    let input = r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#;
    match first_item(input) {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "HttpMethod");
            match decl.def {
                TypeDef::StringLiteralUnion(variants) => {
                    assert_eq!(variants, vec!["GET", "POST", "PUT", "DELETE"]);
                }
                other => panic!("expected string literal union, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn string_literal_union_two_variants() {
    let input = r#"type Bool = "true" | "false""#;
    match first_item(input) {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "Bool");
            match decl.def {
                TypeDef::StringLiteralUnion(variants) => {
                    assert_eq!(variants, vec!["true", "false"]);
                }
                other => panic!("expected string literal union, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

#[test]
fn string_literal_union_exported() {
    let input = r#"export type Status = "ok" | "error""#;
    match first_item(input) {
        ItemKind::TypeDecl(decl) => {
            assert!(decl.exported);
            assert_eq!(decl.name, "Status");
            match decl.def {
                TypeDef::StringLiteralUnion(variants) => {
                    assert_eq!(variants, vec!["ok", "error"]);
                }
                other => panic!("expected string literal union, got {other:?}"),
            }
        }
        other => panic!("expected type decl, got {other:?}"),
    }
}

// ── Collect Block ───────────────────────────────────────────

#[test]
fn collect_block_basic() {
    let expr = first_expr("collect { 42 }");
    match expr {
        ExprKind::Collect(items) => {
            assert_eq!(items.len(), 1);
        }
        other => panic!("expected Collect, got {other:?}"),
    }
}

#[test]
fn collect_block_with_const() {
    let expr = first_expr(
        r#"collect {
        let a = validate(1)?
        a
    }"#,
    );
    match expr {
        ExprKind::Collect(items) => {
            assert_eq!(items.len(), 2);
        }
        other => panic!("expected Collect, got {other:?}"),
    }
}

// ── Single-variant union newtypes ───────────────────────────

#[test]
fn newtype_parses_as_single_variant_union() {
    let item = first_item("type ProductId = ProductId(number)");
    match item {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "ProductId");
            match &decl.def {
                TypeDef::Union(variants) => {
                    assert_eq!(variants.len(), 1);
                    assert_eq!(variants[0].name, "ProductId");
                    assert_eq!(variants[0].fields.len(), 1);
                    assert!(variants[0].fields[0].name.is_none());
                    match &variants[0].fields[0].type_ann.kind {
                        TypeExprKind::Named { name, .. } => assert_eq!(name, "number"),
                        other => panic!("expected Named type, got {other:?}"),
                    }
                }
                other => panic!("expected Union, got {other:?}"),
            }
        }
        other => panic!("expected TypeDecl, got {other:?}"),
    }
}

#[test]
fn newtype_paren_syntax() {
    let item = first_item("type UserId = UserId(string)");
    match item {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "UserId");
            match &decl.def {
                TypeDef::Union(variants) => {
                    assert_eq!(variants.len(), 1);
                    assert_eq!(variants[0].name, "UserId");
                    assert_eq!(variants[0].fields.len(), 1);
                    assert!(variants[0].fields[0].name.is_none());
                    match &variants[0].fields[0].type_ann.kind {
                        TypeExprKind::Named { name, .. } => assert_eq!(name, "string"),
                        other => panic!("expected Named type, got {other:?}"),
                    }
                }
                other => panic!("expected Union, got {other:?}"),
            }
        }
        other => panic!("expected TypeDecl, got {other:?}"),
    }
}

#[test]
fn newtype_with_named_field_is_record() {
    // With new syntax, `{ value: number }` is a record, not a newtype
    let item = first_item("type UserId = { value: number }");
    match item {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "UserId");
            assert!(
                matches!(decl.def, TypeDef::Record(ref fields) if fields.len() == 1),
                "expected Record, got {:?}",
                decl.def
            );
        }
        other => panic!("expected TypeDecl, got {other:?}"),
    }
}

#[test]
fn type_alias_still_parses_as_alias() {
    let item = first_item("type Name = string");
    match item {
        ItemKind::TypeDecl(decl) => {
            assert_eq!(decl.name, "Name");
            assert!(matches!(&decl.def, TypeDef::Alias(_)));
        }
        other => panic!("expected TypeDecl, got {other:?}"),
    }
}

#[test]
fn parse_module_classifies_comment_kinds() {
    let source = "\
//// module header
/// item doc
// plain
let x = 1
";
    let (program, extra) = Parser::parse_module(source).unwrap();
    assert_eq!(program.items.len(), 1);
    assert_eq!(extra.module_comments.len(), 1);
    assert_eq!(extra.doc_comments.len(), 1);
    assert_eq!(extra.comments.len(), 1);

    let module_text =
        &source[extra.module_comments[0].start as usize..extra.module_comments[0].end as usize];
    assert!(module_text.starts_with("////"));

    let doc_text =
        &source[extra.doc_comments[0].start as usize..extra.doc_comments[0].end as usize];
    assert!(doc_text.starts_with("///") && !doc_text.starts_with("////"));

    let comment_text = &source[extra.comments[0].start as usize..extra.comments[0].end as usize];
    assert!(comment_text.starts_with("//") && !comment_text.starts_with("///"));
}

#[test]
fn parse_module_records_empty_lines_between_statements() {
    let source = "let a = 1\n\nlet b = 2\n";
    let (_, extra) = Parser::parse_module(source).unwrap();
    assert_eq!(extra.empty_lines.len(), 1);
    let offset = extra.empty_lines[0] as usize;
    assert_eq!(source.as_bytes()[offset], b'\n');
    assert!(source[..offset].ends_with("let a = 1\n"));
    assert!(source[offset + 1..].starts_with("let b = 2"));
}

#[test]
fn parse_lossy_module_exposes_extra_even_when_errors() {
    let source = "// leading\nconst = 1\n";
    let (_, extra, errors) = Parser::parse_lossy_module(source);
    assert!(!errors.is_empty(), "expected parse errors");
    assert_eq!(extra.comments.len(), 1);
    assert!(extra.new_lines.contains(&10));
}
