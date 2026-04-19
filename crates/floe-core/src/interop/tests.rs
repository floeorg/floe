//! Tests for the interop module.
use std::sync::Arc;

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
    let ty = parse_type_str("(x: string) -> void");
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
            return_type: Arc::new(Type::Unknown),
            required_params: 1,
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
            return_type: Arc::new(Type::Unit),
            required_params: 1,
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
        Type::Array(Arc::new(Type::option_of(Type::String)))
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
            Type::Settable(Arc::new(Type::String))
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
            return_type: Arc::new(Type::option_of(Type::foreign("Element".to_string()))),
            required_params: 1,
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
            return_type: Arc::new(Type::Unit),
            required_params: 1,
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

#[test]
fn intersection_merges_object_fields() {
    let dts = "export type T = { a: number } & { b: string };";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let t = exports.iter().find(|e| e.name == "T").unwrap();
    match &t.ts_type {
        TsType::Object(fields) => {
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            assert!(names.contains(&"a"), "missing `a`, got {names:?}");
            assert!(names.contains(&"b"), "missing `b`, got {names:?}");
        }
        other => panic!("expected Object, got {other:?}"),
    }
}

#[test]
fn string_literal_union_preserves_discriminators() {
    let dts = r#"export type Dir = "up" | "down";"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let dir = exports.iter().find(|e| e.name == "Dir").unwrap();
    match &dir.ts_type {
        TsType::Union(members) => {
            assert!(matches!(members[0], TsType::StringLiteral(ref s) if s == "up"));
            assert!(matches!(members[1], TsType::StringLiteral(ref s) if s == "down"));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn exported_class_surfaces_methods_and_constructor() {
    let dts = r#"export declare class Foo {
        constructor(x: number);
        bar(): void;
        baz: string;
    }"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let foo = exports.iter().find(|e| e.name == "Foo").unwrap();
    match &foo.ts_type {
        TsType::Object(fields) => {
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            assert!(names.contains(&"bar"), "missing method `bar` in {names:?}");
            assert!(names.contains(&"baz"), "missing field `baz` in {names:?}");
            assert!(
                names.contains(&"constructor"),
                "missing synthetic constructor in {names:?}"
            );
        }
        other => panic!("expected Object, got {other:?}"),
    }
}

#[test]
fn string_enum_exports_as_literal_union() {
    let dts = r#"export enum Color { Red = "r", Green = "g" }"#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let color = exports.iter().find(|e| e.name == "Color").unwrap();
    match &color.ts_type {
        TsType::Union(members) => {
            assert!(matches!(members[0], TsType::StringLiteral(ref s) if s == "r"));
            assert!(matches!(members[1], TsType::StringLiteral(ref s) if s == "g"));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn numeric_enum_widens_to_number() {
    let dts = "export enum N { A = 1, B = 2 }";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let n = exports.iter().find(|e| e.name == "N").unwrap();
    assert!(matches!(&n.ts_type, TsType::Primitive(p) if p == "number"));
}

#[test]
fn export_default_identifier_resolves_against_local_declaration() {
    let dts = r#"
        export interface Config { host: string }
        declare const config: Config;
        export default config;
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "default"),
        "missing `default` export"
    );
}

#[test]
fn export_alias_resolves_for_all_declaration_kinds() {
    let dts = r#"
        declare function impl(x: number): string;
        type Internal = { kind: "x" };
        export { impl as handler, Internal as Public };
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"handler"),
        "missing aliased fn export, got {names:?}"
    );
    assert!(
        names.contains(&"Public"),
        "missing aliased type export, got {names:?}"
    );
}

#[test]
fn this_return_inside_interface_resolves_to_interface_name() {
    let dts = r#"
        export interface Builder {
            add(x: number): this;
        }
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let builder = exports.iter().find(|e| e.name == "Builder").unwrap();
    if let TsType::Object(fields) = &builder.ts_type {
        let add = fields.iter().find(|f| f.name == "add").unwrap();
        if let TsType::Function { return_type, .. } = &add.ty {
            assert!(
                matches!(return_type.as_ref(), TsType::Named(n) if n == "Builder"),
                "expected Named(Builder), got {:?}",
                return_type
            );
            return;
        }
    }
    panic!("expected Builder to be an Object with `add` method");
}

#[test]
fn index_signature_only_produces_record() {
    let dts = "export type Dict = { [k: string]: number };";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let dict = exports.iter().find(|e| e.name == "Dict").unwrap();
    match &dict.ts_type {
        TsType::Generic { name, args } if name == "Record" && args.len() == 2 => {}
        other => panic!("expected Record<K, V> generic, got {other:?}"),
    }
}

#[test]
fn keyof_of_concrete_object_yields_string_literal_union() {
    let dts = "export type Keys = keyof { a: number; b: number };";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let keys = exports.iter().find(|e| e.name == "Keys").unwrap();
    match &keys.ts_type {
        TsType::Union(parts) => {
            assert_eq!(parts.len(), 2);
            assert!(matches!(&parts[0], TsType::StringLiteral(s) if s == "a"));
            assert!(matches!(&parts[1], TsType::StringLiteral(s) if s == "b"));
        }
        other => panic!("expected Union, got {other:?}"),
    }
}

#[test]
fn qualified_typeof_keeps_full_path() {
    let dts = r#"
        declare const React: { Component: any };
        export type C = typeof React.Component;
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let c = exports.iter().find(|e| e.name == "C").unwrap();
    assert!(
        matches!(&c.ts_type, TsType::Named(n) if n == "typeof React.Component"),
        "got {:?}",
        c.ts_type
    );
}

#[test]
fn construct_signature_surfaces_as_function() {
    let dts = "export type Ctor = new (x: number) => string;";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let ctor = exports.iter().find(|e| e.name == "Ctor").unwrap();
    assert!(
        matches!(&ctor.ts_type, TsType::Function { .. }),
        "got {:?}",
        ctor.ts_type
    );
}

#[test]
fn setter_only_property_is_surfaced() {
    let dts = r#"
        export interface Thing {
            set name(value: string);
        }
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let thing = exports.iter().find(|e| e.name == "Thing").unwrap();
    if let TsType::Object(fields) = &thing.ts_type {
        assert!(
            fields.iter().any(|f| f.name == "name"),
            "setter-only field was dropped"
        );
        return;
    }
    panic!("expected Object");
}

#[test]
fn nested_namespace_exports_surface() {
    let dts = r#"
        declare namespace Outer {
            namespace Inner {
                export function deep(x: number): string;
            }
        }
        export = Outer;
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    assert!(
        exports.iter().any(|e| e.name == "deep"),
        "nested namespace export missing, got {:?}",
        exports.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}

#[test]
fn bigint_literal_narrows_to_bigint_primitive() {
    let dts = "export type B = 100n;";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let b = exports.iter().find(|e| e.name == "B").unwrap();
    assert!(matches!(&b.ts_type, TsType::Primitive(p) if p == "bigint"));
}

#[test]
fn conditional_type_approximates_as_union_of_branches() {
    let dts = "export type X<T> = T extends string ? number : boolean;";
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let x = exports.iter().find(|e| e.name == "X").unwrap();
    match &x.ts_type {
        TsType::Union(parts) => {
            assert_eq!(parts.len(), 2);
            assert!(matches!(&parts[0], TsType::Primitive(p) if p == "number"));
            assert!(matches!(&parts[1], TsType::Primitive(p) if p == "boolean"));
        }
        other => panic!("expected Union of branches, got {other:?}"),
    }
}

#[test]
fn triple_slash_reference_is_extracted() {
    let dts = "/// <reference path=\"./other.d.ts\" />\n";
    let refs = super::dts::extract_triple_slash_references(dts);
    assert_eq!(refs, vec!["./other.d.ts".to_string()]);
}

#[test]
fn triple_slash_scan_stops_at_first_non_header() {
    let dts = "/// <reference path=\"./a.d.ts\" />\nexport const x: number;\n/// <reference path=\"./b.d.ts\" />\n";
    let refs = super::dts::extract_triple_slash_references(dts);
    assert_eq!(refs, vec!["./a.d.ts".to_string()]);
}

// ── Cross-module alias expansion (#1234) ─────────────────────

#[test]
fn dts_import_type_encodes_module_sentinel() {
    // tsgo emits cross-module type references as `import("pkg").Foo<args>`.
    // The tsgo probe parser preserves the module source in the name so a
    // later pass can look the alias up in its owning .d.ts. The default
    // `parse_dts_exports_from_str` entry strips the sentinel — this test
    // uses the sentinel-preserving variant explicitly.
    let dts = r#"
        export declare let _r: <E>(handler: import("@floeorg/hono").Handler<E>) => void;
    "#;
    let exports = super::dts::parse_dts_exports_with_import_sources(dts).unwrap();
    let r = exports.iter().find(|e| e.name == "_r").unwrap();
    let handler_param = match &r.ts_type {
        TsType::Function { params, .. } => &params[0].ty,
        other => panic!("expected function, got {other:?}"),
    };
    let encoded_name = match handler_param {
        TsType::Generic { name, .. } => name.as_str(),
        other => panic!("expected Generic, got {other:?}"),
    };
    let (module, alias) = super::dts::decode_import_source(encoded_name)
        .expect("name should carry the module sentinel");
    assert_eq!(module, "@floeorg/hono");
    assert_eq!(alias, "Handler");
}

#[test]
fn parse_dts_exports_from_str_strips_import_sentinels() {
    // Non-tsgo callers of `parse_dts_exports_from_str` (e.g. direct .d.ts
    // parsing via `enhance_import_types`) must receive clean names — the
    // sentinel encoding is an internal tsgo-probe optimisation and would
    // break helpers like `unwrap_set_state_action` that match on `name`.
    let dts = r#"
        export declare let _r: <T>(setter: import("react").Dispatch<import("react").SetStateAction<T>>) => void;
    "#;
    let exports = parse_dts_exports_from_str(dts).unwrap();
    let r = exports.iter().find(|e| e.name == "_r").unwrap();
    let mut names = Vec::new();
    fn collect(ty: &TsType, out: &mut Vec<String>) {
        match ty {
            TsType::Named(n) => out.push(n.clone()),
            TsType::Generic { name, args } => {
                out.push(name.clone());
                for a in args {
                    collect(a, out);
                }
            }
            TsType::Function {
                params,
                return_type,
            } => {
                for p in params {
                    collect(&p.ty, out);
                }
                collect(return_type, out);
            }
            _ => {}
        }
    }
    collect(&r.ts_type, &mut names);
    for n in &names {
        assert!(
            !n.contains('\x1F'),
            "name `{n}` still carries an unstripped sentinel"
        );
    }
    assert!(
        names.iter().any(|n| n == "Dispatch"),
        "expected clean `Dispatch` name, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "SetStateAction"),
        "expected clean `SetStateAction` name, got {names:?}"
    );
}

#[test]
fn expand_cross_module_aliases_substitutes_generic_args() {
    // Simulate what the tsgo runner does: given a function whose parameter
    // references an external alias via the sentinel encoding, expand the
    // alias using the owning module's definitions and substitute type args.
    use super::dts::{TypeAliasDef, expand_cross_module_aliases};

    // Parse the alias-owning module to get its function-shaped aliases.
    let foreign_dts = r#"
        export type Handler<E> = (bindings: E) => string;
    "#;
    let foreign_aliases = {
        let mut aliases: std::collections::HashMap<String, TypeAliasDef> =
            std::collections::HashMap::new();
        for export in parse_dts_exports_from_str(foreign_dts).unwrap() {
            if let TsType::Function { .. } = &export.ts_type {
                // Simulate what collect_function_alias_bodies does for type alias decls.
                // Since `export type` comes through as DtsExport, we reuse the body.
                aliases.insert(
                    export.name.clone(),
                    TypeAliasDef {
                        params: vec!["E".to_string()],
                        body: export.ts_type.clone(),
                    },
                );
            }
        }
        aliases
    };

    // Build the sentinel-encoded reference that tsgo would produce for
    // `handler: import("foreign-lib").Handler<{ port: number }>`.
    let mut referencing = TsType::Function {
        params: vec![super::FunctionParam {
            ty: TsType::Generic {
                name: super::dts::encode_import_source("foreign-lib", "Handler"),
                args: vec![TsType::Object(vec![super::ObjectField {
                    name: "port".to_string(),
                    ty: TsType::Primitive("number".to_string()),
                    optional: false,
                }])],
            },
            optional: false,
        }],
        return_type: Box::new(TsType::Primitive("void".to_string())),
    };

    let mut by_module = std::collections::HashMap::new();
    by_module.insert("foreign-lib".to_string(), foreign_aliases);
    expand_cross_module_aliases(&mut referencing, &by_module, 0);

    // After expansion the handler param should BE a function (c: { port: number }) -> string,
    // not a Generic alias reference.
    match &referencing {
        TsType::Function { params, .. } => match &params[0].ty {
            TsType::Function {
                params: inner_params,
                return_type,
            } => {
                match &inner_params[0].ty {
                    TsType::Object(fields) => {
                        assert_eq!(fields[0].name, "port");
                        assert!(matches!(&fields[0].ty, TsType::Primitive(p) if p == "number"));
                    }
                    other => panic!("expected expanded object param, got {other:?}"),
                }
                assert!(matches!(return_type.as_ref(), TsType::Primitive(p) if p == "string"));
            }
            other => panic!("expected handler to be expanded to Function, got {other:?}"),
        },
        other => panic!("outer function disappeared: {other:?}"),
    }
}

#[test]
fn strip_import_sentinels_cleans_surviving_names() {
    // Names that survive expansion (because the alias is missing or the
    // module's .d.ts couldn't be resolved) must still have their sentinels
    // stripped so the boundary wrapper sees clean identifiers.
    use super::dts::{encode_import_source, strip_import_sentinels};
    let mut ty = TsType::Generic {
        name: encode_import_source("pkg", "Unknown"),
        args: vec![TsType::Primitive("string".to_string())],
    };
    strip_import_sentinels(&mut ty);
    match ty {
        TsType::Generic { name, .. } => assert_eq!(name, "Unknown"),
        other => panic!("expected Generic, got {other:?}"),
    }
}
