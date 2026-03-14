//! Tests for the interop module.

use super::*;

// ── Type Parsing ────────────────────────────────────────────

#[test]
fn parse_primitive_string() {
    assert_eq!(
        parse_type_str("string"),
        TsType::Primitive("string".to_string())
    );
}

#[test]
fn parse_primitive_number() {
    assert_eq!(
        parse_type_str("number"),
        TsType::Primitive("number".to_string())
    );
}

#[test]
fn parse_null() {
    assert_eq!(parse_type_str("null"), TsType::Null);
}

#[test]
fn parse_undefined() {
    assert_eq!(parse_type_str("undefined"), TsType::Undefined);
}

#[test]
fn parse_any() {
    assert_eq!(parse_type_str("any"), TsType::Any);
}

#[test]
fn parse_named() {
    assert_eq!(
        parse_type_str("Element"),
        TsType::Named("Element".to_string())
    );
}

#[test]
fn parse_union() {
    let ty = parse_type_str("string | null");
    assert_eq!(
        ty,
        TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null,])
    );
}

#[test]
fn parse_union_three() {
    let ty = parse_type_str("string | null | undefined");
    assert_eq!(
        ty,
        TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Null,
            TsType::Undefined,
        ])
    );
}

#[test]
fn parse_array_shorthand() {
    let ty = parse_type_str("string[]");
    assert_eq!(
        ty,
        TsType::Array(Box::new(TsType::Primitive("string".to_string())))
    );
}

#[test]
fn parse_generic_array() {
    let ty = parse_type_str("Array<string>");
    assert_eq!(
        ty,
        TsType::Array(Box::new(TsType::Primitive("string".to_string())))
    );
}

#[test]
fn parse_generic_promise() {
    let ty = parse_type_str("Promise<string>");
    assert_eq!(
        ty,
        TsType::Generic {
            name: "Promise".to_string(),
            args: vec![TsType::Primitive("string".to_string())],
        }
    );
}

#[test]
fn parse_tuple() {
    let ty = parse_type_str("[string, number]");
    assert_eq!(
        ty,
        TsType::Tuple(vec![
            TsType::Primitive("string".to_string()),
            TsType::Primitive("number".to_string()),
        ])
    );
}

#[test]
fn parse_function_type() {
    let ty = parse_type_str("(x: string) => void");
    assert_eq!(
        ty,
        TsType::Function {
            params: vec![TsType::Primitive("string".to_string())],
            return_type: Box::new(TsType::Primitive("void".to_string())),
        }
    );
}

// ── Boundary Wrapping ───────────────────────────────────────

#[test]
fn wrap_string_stays_string() {
    let ty = wrap_boundary_type(&TsType::Primitive("string".to_string()));
    assert_eq!(ty, Type::String);
}

#[test]
fn wrap_number_stays_number() {
    let ty = wrap_boundary_type(&TsType::Primitive("number".to_string()));
    assert_eq!(ty, Type::Number);
}

#[test]
fn wrap_boolean_becomes_bool() {
    let ty = wrap_boundary_type(&TsType::Primitive("boolean".to_string()));
    assert_eq!(ty, Type::Bool);
}

#[test]
fn wrap_any_becomes_unknown() {
    let ty = wrap_boundary_type(&TsType::Any);
    assert_eq!(ty, Type::Unknown);
}

#[test]
fn wrap_null_union_becomes_option() {
    // string | null -> Option<String>
    let ts = TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::Option(Box::new(Type::String)));
}

#[test]
fn wrap_undefined_union_becomes_option() {
    // number | undefined -> Option<Number>
    let ts = TsType::Union(vec![
        TsType::Primitive("number".to_string()),
        TsType::Undefined,
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::Option(Box::new(Type::Number)));
}

#[test]
fn wrap_null_undefined_union_becomes_option() {
    // string | null | undefined -> Option<String>
    let ts = TsType::Union(vec![
        TsType::Primitive("string".to_string()),
        TsType::Null,
        TsType::Undefined,
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::Option(Box::new(Type::String)));
}

#[test]
fn wrap_plain_union_stays_non_option() {
    // string | number -> Unknown (multi-type union without null)
    let ts = TsType::Union(vec![
        TsType::Primitive("string".to_string()),
        TsType::Primitive("number".to_string()),
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::Unknown);
}

#[test]
fn wrap_function_wraps_params_and_return() {
    // (x: string | null) => any
    let ts = TsType::Function {
        params: vec![TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Null,
        ])],
        return_type: Box::new(TsType::Any),
    };
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Function {
            params: vec![Type::Option(Box::new(Type::String))],
            return_type: Box::new(Type::Unknown),
        }
    );
}

#[test]
fn wrap_array_wraps_inner() {
    // (string | null)[] -> Array<Option<String>>
    let ts = TsType::Array(Box::new(TsType::Union(vec![
        TsType::Primitive("string".to_string()),
        TsType::Null,
    ])));
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Array(Box::new(Type::Option(Box::new(Type::String))))
    );
}

#[test]
fn wrap_object_wraps_fields() {
    let ts = TsType::Object(vec![
        ("name".to_string(), TsType::Primitive("string".to_string())),
        (
            "value".to_string(),
            TsType::Union(vec![TsType::Primitive("number".to_string()), TsType::Null]),
        ),
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Record(vec![
            ("name".to_string(), Type::String),
            ("value".to_string(), Type::Option(Box::new(Type::Number))),
        ])
    );
}

// ── .d.ts Parsing ───────────────────────────────────────────

#[test]
fn parse_dts_function_export() {
    let export = parse_function_export("findElement(id: string): Element | null;");
    let export = export.unwrap();
    assert_eq!(export.name, "findElement");
    assert_eq!(
        export.ts_type,
        TsType::Function {
            params: vec![TsType::Primitive("string".to_string())],
            return_type: Box::new(TsType::Union(vec![
                TsType::Named("Element".to_string()),
                TsType::Null,
            ])),
        }
    );
}

#[test]
fn parse_dts_const_export() {
    let export = parse_const_export("VERSION: string;");
    let export = export.unwrap();
    assert_eq!(export.name, "VERSION");
    assert_eq!(export.ts_type, TsType::Primitive("string".to_string()));
}

#[test]
fn parse_dts_type_export() {
    let export = parse_type_export("Config = { debug: boolean; port: number };");
    let export = export.unwrap();
    assert_eq!(export.name, "Config");
    assert_eq!(
        export.ts_type,
        TsType::Object(vec![
            (
                "debug".to_string(),
                TsType::Primitive("boolean".to_string())
            ),
            ("port".to_string(), TsType::Primitive("number".to_string())),
        ])
    );
}

#[test]
fn parse_function_nullable_return_wraps_to_option() {
    let export = parse_function_export("findElement(id: string): Element | null;").unwrap();
    let wrapped = wrap_boundary_type(&export.ts_type);
    assert_eq!(
        wrapped,
        Type::Function {
            params: vec![Type::String],
            return_type: Box::new(Type::Option(Box::new(Type::Named("Element".to_string())))),
        }
    );
}

#[test]
fn parse_function_any_param_wraps_to_unknown() {
    let export = parse_function_export("process(data: any): void;").unwrap();
    let wrapped = wrap_boundary_type(&export.ts_type);
    assert_eq!(
        wrapped,
        Type::Function {
            params: vec![Type::Unknown],
            return_type: Box::new(Type::Unit),
        }
    );
}

// ── Helper tests ────────────────────────────────────────────

#[test]
fn split_simple() {
    let parts = split_at_top_level("a | b | c", '|');
    assert_eq!(parts, vec!["a ", " b ", " c"]);
}

#[test]
fn split_nested_generics() {
    let parts = split_at_top_level("Map<string, number> | null", '|');
    assert_eq!(parts, vec!["Map<string, number> ", " null"]);
}

#[test]
fn find_paren() {
    assert_eq!(find_matching_paren("(a, b)"), Some(5));
    assert_eq!(find_matching_paren("((a))"), Some(4));
    assert_eq!(find_matching_paren("(a, (b, c), d)"), Some(13));
}

#[test]
fn tsconfig_not_found() {
    let result = find_tsconfig(Path::new("/nonexistent/path"));
    assert!(result.is_none());
}
