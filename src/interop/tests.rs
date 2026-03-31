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
            params: vec![FunctionParam {
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            }],
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
    assert_eq!(wrapped, Type::option_of(Type::String));
}

#[test]
fn wrap_undefined_union_becomes_option() {
    // number | undefined -> Option<Number>
    let ts = TsType::Union(vec![
        TsType::Primitive("number".to_string()),
        TsType::Undefined,
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::option_of(Type::Number));
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
    assert_eq!(wrapped, Type::option_of(Type::String));
}

#[test]
fn wrap_plain_union_becomes_ts_union() {
    // string | number -> TsUnion([String, Number])
    let ts = TsType::Union(vec![
        TsType::Primitive("string".to_string()),
        TsType::Primitive("number".to_string()),
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(wrapped, Type::TsUnion(vec![Type::String, Type::Number]));
}

#[test]
fn wrap_function_wraps_params_and_return() {
    // (x: string | null) => any
    let ts = TsType::Function {
        params: vec![FunctionParam {
            ty: TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null]),
            optional: false,
        }],
        return_type: Box::new(TsType::Any),
    };
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Function {
            params: vec![Type::option_of(Type::String)],
            return_type: Box::new(Type::Unknown),
        }
    );
}

#[test]
fn wrap_function_optional_params_become_option() {
    // (x: string, y?: number) => void
    let ts = TsType::Function {
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
    };
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Function {
            params: vec![Type::String, Type::option_of(Type::Number)],
            return_type: Box::new(Type::Unit),
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
        Type::Array(Box::new(Type::option_of(Type::String)))
    );
}

#[test]
fn wrap_object_wraps_fields() {
    let ts = TsType::Object(vec![
        ObjectField {
            name: "name".to_string(),
            ty: TsType::Primitive("string".to_string()),
            optional: false,
        },
        ObjectField {
            name: "value".to_string(),
            ty: TsType::Union(vec![TsType::Primitive("number".to_string()), TsType::Null]),
            optional: false,
        },
    ]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Record(vec![
            ("name".to_string(), Type::String),
            ("value".to_string(), Type::option_of(Type::Number)),
        ])
    );
}

#[test]
fn wrap_optional_nullable_becomes_settable() {
    // x?: string | null → Settable<string>
    let ts = TsType::Object(vec![ObjectField {
        name: "email".to_string(),
        ty: TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null]),
        optional: true,
    }]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Record(vec![(
            "email".to_string(),
            Type::Settable(Box::new(Type::String))
        ),])
    );
}

#[test]
fn wrap_optional_non_nullable_becomes_option() {
    // x?: string → Option<string>
    let ts = TsType::Object(vec![ObjectField {
        name: "nickname".to_string(),
        ty: TsType::Primitive("string".to_string()),
        optional: true,
    }]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Record(vec![(
            "nickname".to_string(),
            Type::option_of(Type::String),
        )])
    );
}

#[test]
fn wrap_required_nullable_stays_option() {
    // x: string | null → Option<string> (not Settable)
    let ts = TsType::Object(vec![ObjectField {
        name: "deletedAt".to_string(),
        ty: TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null]),
        optional: false,
    }]);
    let wrapped = wrap_boundary_type(&ts);
    assert_eq!(
        wrapped,
        Type::Record(vec![(
            "deletedAt".to_string(),
            Type::option_of(Type::String),
        )])
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
            params: vec![FunctionParam {
                ty: TsType::Primitive("string".to_string()),
                optional: false,
            }],
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
            ObjectField {
                name: "debug".to_string(),
                ty: TsType::Primitive("boolean".to_string()),
                optional: false,
            },
            ObjectField {
                name: "port".to_string(),
                ty: TsType::Primitive("number".to_string()),
                optional: false,
            },
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
            return_type: Box::new(Type::option_of(Type::Foreign("Element".to_string()))),
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
    let result = crate::resolve::find_tsconfig_from(Path::new("/nonexistent/path"));
    assert!(result.is_none());
}

// ── Namespace + export = parsing (oxc_parser) ──────────────

#[test]
fn parse_dts_namespace_with_export_assignment() {
    // React-like pattern: export = React; declare namespace React { function useState<S>(...): ...; }
    let dts = r#"
export = React;
declare namespace React {
    function useState<S>(initialState: S | (() => S)): [S, Dispatch<SetStateAction<S>>];
    function useEffect(effect: () => void, deps?: any[]): void;
    function useRef<T>(initialValue: T): MutableRefObject<T>;
}
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();

    assert_eq!(exports.len(), 3);

    // useState
    let use_state = exports.iter().find(|e| e.name == "useState").unwrap();
    match &use_state.ts_type {
        TsType::Function {
            params,
            return_type,
        } => {
            // Should have 1 param (the initialState union)
            assert_eq!(params.len(), 1);
            // Return type should be a tuple [S, Dispatch<SetStateAction<S>>]
            assert!(matches!(return_type.as_ref(), TsType::Tuple(_)));
        }
        other => panic!("expected Function, got {other:?}"),
    }

    // useEffect
    let use_effect = exports.iter().find(|e| e.name == "useEffect").unwrap();
    match &use_effect.ts_type {
        TsType::Function {
            params,
            return_type,
        } => {
            assert_eq!(params.len(), 2);
            assert_eq!(return_type.as_ref(), &TsType::Primitive("void".to_string()));
        }
        other => panic!("expected Function, got {other:?}"),
    }

    // useRef
    let use_ref = exports.iter().find(|e| e.name == "useRef").unwrap();
    match &use_ref.ts_type {
        TsType::Function {
            params,
            return_type,
        } => {
            assert_eq!(params.len(), 1);
            match return_type.as_ref() {
                TsType::Generic { name, args } => {
                    assert_eq!(name, "MutableRefObject");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected Generic return, got {other:?}"),
            }
        }
        other => panic!("expected Function, got {other:?}"),
    }
}

#[test]
fn parse_dts_direct_export_function() {
    let dts = r#"
export declare function createElement(tag: string, props: any): Element;
export declare const version: string;
export declare type ID = string | number;
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();

    assert_eq!(exports.len(), 3);

    let create_element = exports.iter().find(|e| e.name == "createElement").unwrap();
    match &create_element.ts_type {
        TsType::Function { params, .. } => assert_eq!(params.len(), 2),
        other => panic!("expected Function, got {other:?}"),
    }

    let version = exports.iter().find(|e| e.name == "version").unwrap();
    assert_eq!(version.ts_type, TsType::Primitive("string".to_string()));

    let id = exports.iter().find(|e| e.name == "ID").unwrap();
    match &id.ts_type {
        TsType::Union(parts) => assert_eq!(parts.len(), 2),
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn parse_dts_export_interface() {
    let dts = r#"
export interface Config {
    debug: boolean;
    port: number;
    host: string;
}
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    assert_eq!(exports.len(), 1);

    let config = &exports[0];
    assert_eq!(config.name, "Config");
    match &config.ts_type {
        TsType::Object(fields) => {
            assert_eq!(fields.len(), 3);
            assert_eq!(fields[0].name, "debug");
            assert_eq!(fields[0].ty, TsType::Primitive("boolean".to_string()));
            assert_eq!(fields[1].name, "port");
            assert_eq!(fields[1].ty, TsType::Primitive("number".to_string()));
            assert_eq!(fields[2].name, "host");
            assert_eq!(fields[2].ty, TsType::Primitive("string".to_string()));
        }
        other => panic!("expected Object, got {other:?}"),
    }
}

#[test]
fn parse_dts_overloaded_functions_use_first() {
    // Overloaded functions: should use the first declaration
    let dts = r#"
export = MyModule;
declare namespace MyModule {
    function parse(text: string): object;
    function parse(text: string, reviver: (key: string, value: any) => any): object;
}
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();

    // Should only have one "parse" entry (the first overload)
    let parse_exports: Vec<_> = exports.iter().filter(|e| e.name == "parse").collect();
    assert_eq!(parse_exports.len(), 1);

    match &parse_exports[0].ts_type {
        TsType::Function { params, .. } => {
            // First overload has 1 param
            assert_eq!(params.len(), 1);
        }
        other => panic!("expected Function, got {other:?}"),
    }
}

#[test]
fn parse_dts_namespace_without_export_assignment() {
    // If there's no `export = X`, namespace members should NOT be exported
    let dts = r#"
declare namespace Internal {
    function helper(): void;
}
export declare function publicFn(): string;
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();

    // Only publicFn should be exported, not helper
    assert_eq!(exports.len(), 1);
    assert_eq!(exports[0].name, "publicFn");
}

#[test]
fn parse_dts_namespace_const_and_type() {
    let dts = r#"
export = Lib;
declare namespace Lib {
    const VERSION: string;
    type Options = { verbose: boolean; timeout: number };
    interface Result {
        success: boolean;
        data: any;
    }
}
"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();

    assert_eq!(exports.len(), 3);

    let version = exports.iter().find(|e| e.name == "VERSION").unwrap();
    assert_eq!(version.ts_type, TsType::Primitive("string".to_string()));

    let options = exports.iter().find(|e| e.name == "Options").unwrap();
    match &options.ts_type {
        TsType::Object(fields) => {
            assert_eq!(fields.len(), 2);
        }
        other => panic!("expected Object, got {other:?}"),
    }

    let result = exports.iter().find(|e| e.name == "Result").unwrap();
    match &result.ts_type {
        TsType::Object(fields) => {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "success");
            assert_eq!(fields[1].name, "data");
        }
        other => panic!("expected Object, got {other:?}"),
    }
}

// ── Result union round-trip ─────────────────────────────────

#[test]
fn result_union_round_trip_via_oxc() {
    let dts = r#"export declare const _r0: { ok: true; value: { name: string; }; } | { ok: false; error: Error; };"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    assert_eq!(exports.len(), 1);
    let wrapped = crate::interop::wrap_boundary_type(&exports[0].ts_type);
    assert!(wrapped.is_result(), "expected Result, got {:?}", wrapped);
}
