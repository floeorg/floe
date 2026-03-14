//! .d.ts export parsing: reads declaration files and extracts exports using oxc_parser.

use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, ExportNamedDeclaration, FormalParameters, PropertyKey, Statement,
    TSModuleDeclarationBody, TSModuleDeclarationName, TSSignature, TSTupleElement,
    TSType as OxcTSType, TSTypeName, VariableDeclarator,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

use super::*;

/// An export entry from a .d.ts file.
#[derive(Debug, Clone)]
pub struct DtsExport {
    pub name: String,
    pub ts_type: TsType,
}

/// Reads a .d.ts file and extracts its named exports.
///
/// Uses oxc_parser to parse the declaration file AST and extract exports.
/// Handles:
/// - `export function/const/type/interface`
/// - `export declare function/const/type/interface`
/// - `declare namespace X { ... }` blocks (when combined with `export = X`)
/// - `export = X` re-export patterns
/// - Overloaded function declarations (uses first signature)
pub fn parse_dts_exports(dts_path: &Path) -> Result<Vec<DtsExport>, String> {
    let content = std::fs::read_to_string(dts_path)
        .map_err(|e| format!("failed to read {}: {e}", dts_path.display()))?;

    parse_dts_exports_from_str(&content)
}

/// Parse .d.ts exports from a string (used by tests and the file-based entry point).
pub(super) fn parse_dts_exports_from_str(content: &str) -> Result<Vec<DtsExport>, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::d_ts();
    let ret = Parser::new(&allocator, content, source_type).parse();

    if ret.panicked {
        return Err("failed to parse .d.ts file".to_string());
    }

    let mut exports = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut export_assignment_name: Option<String> = None;
    let mut namespace_exports: HashMap<String, Vec<DtsExport>> = HashMap::new();

    // First pass: collect all info
    for stmt in &ret.program.body {
        // `export = X;` — remember the namespace name for later
        if let Statement::TSExportAssignment(assign) = stmt
            && let oxc_ast::ast::Expression::Identifier(ident) = &assign.expression
        {
            export_assignment_name = Some(ident.name.to_string());
        }

        // `export function/const/type/interface ...`
        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            extract_from_export_named(export_decl, &mut exports, &mut seen_names);
        }

        // `declare namespace X { ... }` (top-level)
        if let Statement::TSModuleDeclaration(ns_decl) = stmt {
            let ns_name = match &ns_decl.id {
                TSModuleDeclarationName::Identifier(ident) => ident.name.to_string(),
                TSModuleDeclarationName::StringLiteral(lit) => lit.value.to_string(),
            };
            let ns_exports = extract_from_namespace_body(&ns_decl.body);
            namespace_exports
                .entry(ns_name)
                .or_default()
                .extend(ns_exports);
        }
    }

    // If there's an `export = X` and a matching `declare namespace X`,
    // treat all namespace members as module exports
    if let Some(ref ns_name) = export_assignment_name
        && let Some(ns_exports) = namespace_exports.remove(ns_name)
    {
        for export in ns_exports {
            if seen_names.insert(export.name.clone()) {
                exports.push(export);
            }
        }
    }

    Ok(exports)
}

/// Extract exports from an `export` declaration (export function/const/type/interface).
fn extract_from_export_named(
    export_decl: &ExportNamedDeclaration<'_>,
    exports: &mut Vec<DtsExport>,
    seen_names: &mut HashSet<String>,
) {
    let Some(ref decl) = export_decl.declaration else {
        return;
    };

    match decl {
        Declaration::FunctionDeclaration(func) => {
            if let Some(ref id) = func.id {
                let name = id.name.to_string();
                if seen_names.insert(name.clone()) {
                    let ts_type = convert_function(&func.params, &func.return_type);
                    exports.push(DtsExport { name, ts_type });
                }
                // Skip overloads (same name already seen)
            }
        }
        Declaration::VariableDeclaration(var_decl) => {
            for declarator in &var_decl.declarations {
                if let Some(export) = convert_variable_declarator(declarator)
                    && seen_names.insert(export.name.clone())
                {
                    exports.push(export);
                }
            }
        }
        Declaration::TSTypeAliasDeclaration(type_decl) => {
            let name = type_decl.id.name.to_string();
            if seen_names.insert(name.clone()) {
                let ts_type = convert_oxc_type(&type_decl.type_annotation);
                exports.push(DtsExport { name, ts_type });
            }
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            let name = iface.id.name.to_string();
            if seen_names.insert(name.clone()) {
                let ts_type = convert_interface_body(&iface.body.body);
                exports.push(DtsExport { name, ts_type });
            }
        }
        _ => {}
    }
}

/// Extract function/const/type/interface declarations from inside a namespace body.
fn extract_from_namespace_body(body: &Option<TSModuleDeclarationBody<'_>>) -> Vec<DtsExport> {
    let mut exports = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    let Some(body) = body else { return exports };

    let block = match body {
        TSModuleDeclarationBody::TSModuleBlock(block) => block,
        TSModuleDeclarationBody::TSModuleDeclaration(_nested) => {
            // Nested namespace like `declare namespace A.B { ... }` — skip for now
            return exports;
        }
    };

    for stmt in &block.body {
        match stmt {
            // Function declarations inside namespace
            Statement::FunctionDeclaration(func) => {
                if let Some(ref id) = func.id {
                    let name = id.name.to_string();
                    if seen_names.insert(name.clone()) {
                        let ts_type = convert_function(&func.params, &func.return_type);
                        exports.push(DtsExport { name, ts_type });
                    }
                }
            }
            // Variable declarations inside namespace
            Statement::VariableDeclaration(var_decl) => {
                for declarator in &var_decl.declarations {
                    if let Some(export) = convert_variable_declarator(declarator)
                        && seen_names.insert(export.name.clone())
                    {
                        exports.push(export);
                    }
                }
            }
            // Type aliases inside namespace
            Statement::TSTypeAliasDeclaration(type_decl) => {
                let name = type_decl.id.name.to_string();
                if seen_names.insert(name.clone()) {
                    let ts_type = convert_oxc_type(&type_decl.type_annotation);
                    exports.push(DtsExport { name, ts_type });
                }
            }
            // Interfaces inside namespace
            Statement::TSInterfaceDeclaration(iface) => {
                let name = iface.id.name.to_string();
                if seen_names.insert(name.clone()) {
                    let ts_type = convert_interface_body(&iface.body.body);
                    exports.push(DtsExport { name, ts_type });
                }
            }
            // Exported declarations inside namespace
            Statement::ExportNamedDeclaration(export_decl) => {
                extract_from_export_named(export_decl, &mut exports, &mut seen_names);
            }
            _ => {}
        }
    }

    exports
}

// ── Type conversion helpers ─────────────────────────────────────

/// Convert an oxc function declaration to our TsType::Function.
fn convert_function(
    params: &FormalParameters<'_>,
    return_type: &Option<oxc_allocator::Box<'_, oxc_ast::ast::TSTypeAnnotation<'_>>>,
) -> TsType {
    let param_types: Vec<TsType> = params
        .items
        .iter()
        .map(|p| {
            p.type_annotation
                .as_ref()
                .map(|ta| convert_oxc_type(&ta.type_annotation))
                .unwrap_or(TsType::Any)
        })
        .collect();

    let ret = return_type
        .as_ref()
        .map(|ta| convert_oxc_type(&ta.type_annotation))
        .unwrap_or(TsType::Primitive("void".to_string()));

    TsType::Function {
        params: param_types,
        return_type: Box::new(ret),
    }
}

/// Convert an oxc variable declarator to a DtsExport (for const declarations).
fn convert_variable_declarator(declarator: &VariableDeclarator<'_>) -> Option<DtsExport> {
    let name = match &declarator.id {
        oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => ident.name.to_string(),
        _ => return None,
    };
    let ts_type = declarator
        .type_annotation
        .as_ref()
        .map(|ta| convert_oxc_type(&ta.type_annotation))
        .unwrap_or(TsType::Any);

    Some(DtsExport { name, ts_type })
}

/// Convert interface body members to TsType::Object.
fn convert_interface_body(members: &[TSSignature<'_>]) -> TsType {
    let fields: Vec<(String, TsType)> = members
        .iter()
        .filter_map(|sig| match sig {
            TSSignature::TSPropertySignature(prop) => {
                let name = property_key_name(&prop.key)?;
                let ty = prop
                    .type_annotation
                    .as_ref()
                    .map(|ta| convert_oxc_type(&ta.type_annotation))
                    .unwrap_or(TsType::Any);
                Some((name, ty))
            }
            _ => None,
        })
        .collect();
    TsType::Object(fields)
}

/// Extract a name from a PropertyKey.
fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    key.name().map(|n| n.to_string())
}

/// Convert an oxc TSType to our TsType enum.
fn convert_oxc_type(ty: &OxcTSType<'_>) -> TsType {
    match ty {
        // Keywords
        OxcTSType::TSStringKeyword(_) => TsType::Primitive("string".to_string()),
        OxcTSType::TSNumberKeyword(_) => TsType::Primitive("number".to_string()),
        OxcTSType::TSBooleanKeyword(_) => TsType::Primitive("boolean".to_string()),
        OxcTSType::TSVoidKeyword(_) => TsType::Primitive("void".to_string()),
        OxcTSType::TSNeverKeyword(_) => TsType::Primitive("never".to_string()),
        OxcTSType::TSBigIntKeyword(_) => TsType::Primitive("bigint".to_string()),
        OxcTSType::TSSymbolKeyword(_) => TsType::Primitive("symbol".to_string()),
        OxcTSType::TSNullKeyword(_) => TsType::Null,
        OxcTSType::TSUndefinedKeyword(_) => TsType::Undefined,
        OxcTSType::TSAnyKeyword(_) => TsType::Any,
        OxcTSType::TSUnknownKeyword(_) => TsType::Unknown,

        // Union: T | U | V
        OxcTSType::TSUnionType(union) => {
            let parts: Vec<TsType> = union.types.iter().map(|t| convert_oxc_type(t)).collect();
            TsType::Union(parts)
        }

        // Array shorthand: T[]
        OxcTSType::TSArrayType(arr) => TsType::Array(Box::new(convert_oxc_type(&arr.element_type))),

        // Tuple: [T, U]
        OxcTSType::TSTupleType(tuple) => {
            let parts: Vec<TsType> = tuple
                .element_types
                .iter()
                .map(|el| convert_tuple_element(el))
                .collect();
            TsType::Tuple(parts)
        }

        // Function type: (params) => ReturnType
        OxcTSType::TSFunctionType(func) => {
            let param_types: Vec<TsType> = func
                .params
                .items
                .iter()
                .map(|p| {
                    p.type_annotation
                        .as_ref()
                        .map(|ta| convert_oxc_type(&ta.type_annotation))
                        .unwrap_or(TsType::Any)
                })
                .collect();
            let ret = convert_oxc_type(&func.return_type.type_annotation);
            TsType::Function {
                params: param_types,
                return_type: Box::new(ret),
            }
        }

        // Type reference: named type or generic
        OxcTSType::TSTypeReference(type_ref) => {
            let name = ts_type_name_to_string(&type_ref.type_name);

            if let Some(ref type_args) = type_ref.type_arguments {
                let args: Vec<TsType> = type_args
                    .params
                    .iter()
                    .map(|t| convert_oxc_type(t))
                    .collect();

                // Normalize Array<T> to TsType::Array
                if name == "Array" && args.len() == 1 {
                    return TsType::Array(Box::new(args.into_iter().next().unwrap()));
                }

                TsType::Generic { name, args }
            } else {
                TsType::Named(name)
            }
        }

        // Object literal type: { key: Type; ... }
        OxcTSType::TSTypeLiteral(lit) => {
            let fields: Vec<(String, TsType)> = lit
                .members
                .iter()
                .filter_map(|sig| match sig {
                    TSSignature::TSPropertySignature(prop) => {
                        let name = property_key_name(&prop.key)?;
                        let ty = prop
                            .type_annotation
                            .as_ref()
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any);
                        Some((name, ty))
                    }
                    _ => None,
                })
                .collect();
            TsType::Object(fields)
        }

        // Parenthesized type: (T)
        OxcTSType::TSParenthesizedType(paren) => convert_oxc_type(&paren.type_annotation),

        // Intersection, conditional, mapped, etc. — fall back to Named/Unknown
        OxcTSType::TSIntersectionType(_) => TsType::Named("intersection".to_string()),

        // Literal types (string/number/boolean literals)
        OxcTSType::TSLiteralType(lit) => match &lit.literal {
            oxc_ast::ast::TSLiteral::StringLiteral(_) => TsType::Primitive("string".to_string()),
            oxc_ast::ast::TSLiteral::NumericLiteral(_) => TsType::Primitive("number".to_string()),
            oxc_ast::ast::TSLiteral::BooleanLiteral(_) => TsType::Primitive("boolean".to_string()),
            _ => TsType::Named("literal".to_string()),
        },

        // import("module").Name or import("module").Name<Args>
        OxcTSType::TSImportType(import_ty) => {
            if let Some(ref qualifier) = import_ty.qualifier {
                let name = import_qualifier_to_string(qualifier);
                if let Some(ref type_args) = import_ty.type_arguments {
                    let args: Vec<TsType> = type_args
                        .params
                        .iter()
                        .map(|t| convert_oxc_type(t))
                        .collect();
                    TsType::Generic { name, args }
                } else {
                    TsType::Named(name)
                }
            } else {
                TsType::Named("unknown".to_string())
            }
        }

        // typeof expression: typeof useState
        OxcTSType::TSTypeQuery(query) => {
            let name = match &query.expr_name {
                oxc_ast::ast::TSTypeQueryExprName::IdentifierReference(ident) => {
                    ident.name.to_string()
                }
                _ => "unknown".to_string(),
            };
            TsType::Named(format!("typeof {name}"))
        }

        // Everything else
        _ => TsType::Named("unknown".to_string()),
    }
}

/// Convert a TSTypeName to a string like "Foo" or "React.FC".
fn ts_type_name_to_string(name: &TSTypeName<'_>) -> String {
    match name {
        TSTypeName::IdentifierReference(ident) => ident.name.to_string(),
        TSTypeName::QualifiedName(qn) => {
            let left = ts_type_name_to_string(&qn.left);
            format!("{}.{}", left, qn.right.name)
        }
        TSTypeName::ThisExpression(_) => "this".to_string(),
    }
}

/// Convert a TSImportTypeQualifier to a string.
fn import_qualifier_to_string(q: &oxc_ast::ast::TSImportTypeQualifier<'_>) -> String {
    match q {
        oxc_ast::ast::TSImportTypeQualifier::Identifier(ident) => ident.name.to_string(),
        oxc_ast::ast::TSImportTypeQualifier::QualifiedName(qn) => {
            format!("{}.{}", import_qualifier_to_string(&qn.left), qn.right.name)
        }
    }
}

/// Convert a TSTupleElement to TsType.
fn convert_tuple_element(el: &TSTupleElement<'_>) -> TsType {
    match el {
        TSTupleElement::TSOptionalType(opt) => convert_oxc_type(&opt.type_annotation),
        TSTupleElement::TSRestType(rest) => convert_oxc_type(&rest.type_annotation),
        // TSNamedTupleMember inherits into TSTupleElement from TSType
        TSTupleElement::TSNamedTupleMember(member) => convert_tuple_element(&member.element_type),
        // All TSType variants are inherited — handle keywords directly
        TSTupleElement::TSStringKeyword(_) => TsType::Primitive("string".to_string()),
        TSTupleElement::TSNumberKeyword(_) => TsType::Primitive("number".to_string()),
        TSTupleElement::TSBooleanKeyword(_) => TsType::Primitive("boolean".to_string()),
        TSTupleElement::TSVoidKeyword(_) => TsType::Primitive("void".to_string()),
        TSTupleElement::TSNeverKeyword(_) => TsType::Primitive("never".to_string()),
        TSTupleElement::TSBigIntKeyword(_) => TsType::Primitive("bigint".to_string()),
        TSTupleElement::TSSymbolKeyword(_) => TsType::Primitive("symbol".to_string()),
        TSTupleElement::TSNullKeyword(_) => TsType::Null,
        TSTupleElement::TSUndefinedKeyword(_) => TsType::Undefined,
        TSTupleElement::TSAnyKeyword(_) => TsType::Any,
        TSTupleElement::TSUnknownKeyword(_) => TsType::Unknown,
        TSTupleElement::TSUnionType(union) => {
            TsType::Union(union.types.iter().map(|t| convert_oxc_type(t)).collect())
        }
        TSTupleElement::TSArrayType(arr) => {
            TsType::Array(Box::new(convert_oxc_type(&arr.element_type)))
        }
        TSTupleElement::TSTupleType(tuple) => TsType::Tuple(
            tuple
                .element_types
                .iter()
                .map(|e| convert_tuple_element(e))
                .collect(),
        ),
        TSTupleElement::TSFunctionType(func) => {
            let param_types: Vec<TsType> = func
                .params
                .items
                .iter()
                .map(|p| {
                    p.type_annotation
                        .as_ref()
                        .map(|ta| convert_oxc_type(&ta.type_annotation))
                        .unwrap_or(TsType::Any)
                })
                .collect();
            let ret = convert_oxc_type(&func.return_type.type_annotation);
            TsType::Function {
                params: param_types,
                return_type: Box::new(ret),
            }
        }
        TSTupleElement::TSTypeReference(type_ref) => {
            let name = ts_type_name_to_string(&type_ref.type_name);
            if let Some(ref type_args) = type_ref.type_arguments {
                let args: Vec<TsType> = type_args
                    .params
                    .iter()
                    .map(|t| convert_oxc_type(t))
                    .collect();
                if name == "Array" && args.len() == 1 {
                    return TsType::Array(Box::new(args.into_iter().next().unwrap()));
                }
                TsType::Generic { name, args }
            } else {
                TsType::Named(name)
            }
        }
        TSTupleElement::TSTypeLiteral(lit) => {
            let fields: Vec<(String, TsType)> = lit
                .members
                .iter()
                .filter_map(|sig| match sig {
                    TSSignature::TSPropertySignature(prop) => {
                        let name = property_key_name(&prop.key)?;
                        let ty = prop
                            .type_annotation
                            .as_ref()
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any);
                        Some((name, ty))
                    }
                    _ => None,
                })
                .collect();
            TsType::Object(fields)
        }
        TSTupleElement::TSParenthesizedType(paren) => convert_oxc_type(&paren.type_annotation),
        TSTupleElement::TSImportType(import_ty) => {
            if let Some(ref qualifier) = import_ty.qualifier {
                let name = import_qualifier_to_string(qualifier);
                if let Some(ref type_args) = import_ty.type_arguments {
                    let args: Vec<TsType> = type_args
                        .params
                        .iter()
                        .map(|t| convert_oxc_type(t))
                        .collect();
                    TsType::Generic { name, args }
                } else {
                    TsType::Named(name)
                }
            } else {
                TsType::Named("unknown".to_string())
            }
        }
        TSTupleElement::TSTypeQuery(query) => {
            let name = match &query.expr_name {
                oxc_ast::ast::TSTypeQueryExprName::IdentifierReference(ident) => {
                    ident.name.to_string()
                }
                _ => "unknown".to_string(),
            };
            TsType::Named(format!("typeof {name}"))
        }
        TSTupleElement::TSLiteralType(lit) => match &lit.literal {
            oxc_ast::ast::TSLiteral::StringLiteral(_) => TsType::Primitive("string".to_string()),
            oxc_ast::ast::TSLiteral::NumericLiteral(_) => TsType::Primitive("number".to_string()),
            oxc_ast::ast::TSLiteral::BooleanLiteral(_) => TsType::Primitive("boolean".to_string()),
            _ => TsType::Named("literal".to_string()),
        },
        _ => TsType::Named("unknown".to_string()),
    }
}

// ── Legacy helper functions (kept for backward compat with tests) ───

#[cfg(test)]
pub(super) fn parse_function_export(rest: &str) -> Option<DtsExport> {
    // name(params): ReturnType;
    let paren = rest.find('(')?;
    let name = rest[..paren].trim().to_string();

    // Strip generic type params from name if present (e.g., "useState<S>")
    let name = if let Some(angle) = name.find('<') {
        name[..angle].trim().to_string()
    } else {
        name
    };

    // Find matching close paren (handle nested parens)
    let after_name = &rest[paren..];
    let close = find_matching_paren(after_name)?;
    let params_str = &after_name[1..close];
    let after_params = after_name[close + 1..].trim();

    let params = parse_param_types(params_str);

    let return_type = if let Some(ret_str) = after_params.strip_prefix(':') {
        let ret_str = ret_str.trim().trim_end_matches(';').trim();
        parse_type_str(ret_str)
    } else {
        TsType::Primitive("void".to_string())
    };

    Some(DtsExport {
        name,
        ts_type: TsType::Function {
            params,
            return_type: Box::new(return_type),
        },
    })
}

#[cfg(test)]
pub(super) fn parse_const_export(rest: &str) -> Option<DtsExport> {
    // name: Type;
    let colon = rest.find(':')?;
    let name = rest[..colon].trim().to_string();
    let type_str = rest[colon + 1..].trim().trim_end_matches(';').trim();
    let ts_type = parse_type_str(type_str);

    Some(DtsExport { name, ts_type })
}

#[cfg(test)]
pub(super) fn parse_type_export(rest: &str) -> Option<DtsExport> {
    // Name = Type;
    let eq = rest.find('=')?;
    let name = rest[..eq].trim().to_string();
    // Strip generic params from name if present
    let name = if let Some(angle) = name.find('<') {
        name[..angle].trim().to_string()
    } else {
        name
    };
    let type_str = rest[eq + 1..].trim().trim_end_matches(';').trim();
    let ts_type = parse_type_str(type_str);

    Some(DtsExport { name, ts_type })
}

#[cfg(test)]
#[allow(dead_code)]
pub(super) fn parse_interface_export(
    rest: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'_>>,
) -> Option<DtsExport> {
    // Name { ... } or Name extends ... { ... }
    let name_end = rest
        .find('{')
        .or_else(|| rest.find("extends"))
        .unwrap_or(rest.len());
    let name = rest[..name_end].trim().to_string();
    // Strip generic params
    let name = if let Some(angle) = name.find('<') {
        name[..angle].trim().to_string()
    } else {
        name
    };

    // Collect interface body fields
    let mut fields = Vec::new();
    let mut brace_depth: i32 = if rest.contains('{') { 1 } else { 0 };

    // If opening brace wasn't on this line, skip to it
    if brace_depth == 0 {
        for line in lines.by_ref() {
            if line.contains('{') {
                brace_depth = 1;
                break;
            }
        }
    }

    while brace_depth > 0 {
        if let Some(line) = lines.next() {
            let trimmed = line.trim();
            brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;

            if brace_depth > 0 {
                // Parse field: name: Type; or name?: Type;
                if let Some(colon) = trimmed.find(':') {
                    let field_name = trimmed[..colon]
                        .trim()
                        .trim_end_matches('?')
                        .trim_start_matches("readonly ")
                        .trim()
                        .to_string();
                    let type_str = trimmed[colon + 1..].trim().trim_end_matches(';').trim();
                    if !field_name.is_empty() && !field_name.starts_with('[') {
                        fields.push((field_name, parse_type_str(type_str)));
                    }
                }
            }
        } else {
            break;
        }
    }

    Some(DtsExport {
        name,
        ts_type: TsType::Object(fields),
    })
}
