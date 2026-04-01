//! .d.ts export parsing: reads declaration files and extracts exports using oxc_parser.

use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, ExportNamedDeclaration, FormalParameters, PropertyKey, Statement,
    TSModuleDeclarationBody, TSModuleDeclarationName, TSPropertySignature, TSSignature,
    TSTupleElement, TSType as OxcTSType, TSTypeName, VariableDeclarator,
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
/// - `export * from "./X"` re-exports (follows relative paths)
/// - Overloaded function declarations (uses first signature)
pub fn parse_dts_exports(dts_path: &Path) -> Result<Vec<DtsExport>, String> {
    let mut visited = HashSet::new();
    parse_dts_exports_recursive(dts_path, &mut visited)
}

fn parse_dts_exports_recursive(
    dts_path: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<Vec<DtsExport>, String> {
    let canonical = dts_path
        .canonicalize()
        .unwrap_or_else(|_| dts_path.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(dts_path)
        .map_err(|e| format!("failed to read {}: {e}", dts_path.display()))?;

    let result = parse_dts_content(&content)?;
    let mut exports = result.exports;
    let mut seen_names: HashSet<String> = exports.iter().map(|e| e.name.clone()).collect();

    // Follow `export *` re-exports (relative paths only)
    let parent_dir = dts_path.parent().unwrap_or(Path::new("."));
    for source in &result.reexport_sources {
        if let Some(resolved) = resolve_dts_source(parent_dir, source)
            && let Ok(reexported) = parse_dts_exports_recursive(&resolved, visited)
        {
            for export in reexported {
                if seen_names.insert(export.name.clone()) {
                    exports.push(export);
                }
            }
        }
    }

    Ok(exports)
}

/// Resolve a relative source path (e.g. `"./getYear.js"`) to a .d.ts file.
fn resolve_dts_source(parent_dir: &Path, source: &str) -> Option<PathBuf> {
    // Only follow relative paths
    if !source.starts_with("./") && !source.starts_with("../") {
        return None;
    }

    let base = parent_dir.join(source);

    // Try replacing .js/.mjs/.cjs extension with corresponding .d.ts
    if let Some(base_str) = base.to_str() {
        for (ext, dts_ext) in &[
            (".js", ".d.ts"),
            (".mjs", ".d.mts"),
            (".cjs", ".d.cts"),
            (".jsx", ".d.ts"),
        ] {
            if let Some(stripped) = base_str.strip_suffix(ext) {
                let dts_path = PathBuf::from(format!("{stripped}{dts_ext}"));
                if dts_path.exists() {
                    return Some(dts_path);
                }
            }
        }
    }

    // Try adding .d.ts directly
    let with_dts = parent_dir.join(format!("{source}.d.ts"));
    if with_dts.exists() {
        return Some(with_dts);
    }

    // Try as directory with index.d.ts
    if base.is_dir() {
        let index = base.join("index.d.ts");
        if index.exists() {
            return Some(index);
        }
    }

    None
}

/// Internal parse result including re-export sources.
struct ParseResult {
    exports: Vec<DtsExport>,
    reexport_sources: Vec<String>,
}

fn parse_dts_content(content: &str) -> Result<ParseResult, String> {
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
    let mut reexport_sources = Vec::new();
    // Collect all function declarations for resolving `typeof X` references
    let mut fn_declarations: HashMap<String, TsType> = HashMap::new();

    // First pass: collect all info
    for stmt in &ret.program.body {
        // `export = X;` - remember the namespace name for later
        if let Statement::TSExportAssignment(assign) = stmt
            && let oxc_ast::ast::Expression::Identifier(ident) = &assign.expression
        {
            export_assignment_name = Some(ident.name.to_string());
        }

        // `export function/const/type/interface ...`
        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            extract_from_export_named(export_decl, &mut exports, &mut seen_names);

            // Also record function declarations for typeof resolution
            if let Some(ref decl) = export_decl.declaration
                && let Declaration::FunctionDeclaration(func) = decl
                && let Some(ref id) = func.id
            {
                let ts_type = convert_function(&func.params, &func.return_type);
                fn_declarations.insert(id.name.to_string(), ts_type);
            }
        }

        // Non-exported function declarations (for typeof resolution)
        if let Statement::FunctionDeclaration(func) = stmt
            && let Some(ref id) = func.id
        {
            let ts_type = convert_function(&func.params, &func.return_type);
            fn_declarations.insert(id.name.to_string(), ts_type);
        }

        // `export * from "./X.js"`
        if let Statement::ExportAllDeclaration(export_all) = stmt {
            reexport_sources.push(export_all.source.value.to_string());
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

    // Second pass: resolve `typeof X` references against local declarations
    for export in &mut exports {
        if let TsType::Named(ref s) = export.ts_type
            && let Some(ref_name) = s.strip_prefix("typeof ")
            && let Some(resolved_type) = fn_declarations.get(ref_name)
        {
            export.ts_type = resolved_type.clone();
        }
    }

    // Third pass: resolve interface `extends` by collecting all interface
    // definitions and merging parent fields into children.
    let mut interface_bodies: HashMap<String, Vec<ObjectField>> = HashMap::new();
    let mut interface_extends: HashMap<String, Vec<String>> = HashMap::new();

    // Collect interfaces from all sources (exports, namespace exports)
    for stmt in &ret.program.body {
        collect_interface_info(stmt, &mut interface_bodies, &mut interface_extends);
    }

    // Resolve extends chains: merge parent fields into each interface
    let resolved_names: Vec<String> = interface_extends.keys().cloned().collect();
    for name in &resolved_names {
        let fields = resolve_interface_fields(name, &interface_bodies, &interface_extends);
        interface_bodies.insert(name.clone(), fields);
    }

    // Update exports that have interface types with resolved fields
    for export in &mut exports {
        if let TsType::Object(ref fields) = export.ts_type
            && let Some(resolved_fields) = interface_bodies.get(&export.name)
            && resolved_fields.len() > fields.len()
        {
            export.ts_type = TsType::Object(resolved_fields.clone());
        }
    }

    // Add interfaces referenced by `export { type X }` specifiers that
    // aren't already in exports (e.g. non-exported interfaces re-exported
    // via `export { type DropResult }`)
    for stmt in &ret.program.body {
        if let Statement::ExportNamedDeclaration(export_decl) = stmt
            && export_decl.declaration.is_none()
        {
            for spec in &export_decl.specifiers {
                let name = spec.exported.name().to_string();
                if !name.is_empty()
                    && !seen_names.contains(&name)
                    && let Some(fields) = interface_bodies.get(&name)
                {
                    seen_names.insert(name.clone());
                    exports.push(DtsExport {
                        name,
                        ts_type: TsType::Object(fields.clone()),
                    });
                }
            }
        }
    }

    Ok(ParseResult {
        exports,
        reexport_sources,
    })
}

/// Parse .d.ts exports from a string (used by tests and the file-based entry point).
pub(super) fn parse_dts_exports_from_str(content: &str) -> Result<Vec<DtsExport>, String> {
    parse_dts_content(content).map(|r| r.exports)
}

/// Parse ALL type/interface declarations from a source file, including non-exported ones.
/// Used to resolve type references like `IssueFilters` that appear in probe output
/// but aren't exported from the source module.
pub(super) fn parse_all_types_from_str(content: &str) -> Result<Vec<DtsExport>, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::tsx();
    let ret = Parser::new(&allocator, content, source_type).parse();

    if ret.panicked {
        return Err("failed to parse file".to_string());
    }

    let mut types = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for stmt in &ret.program.body {
        // Interface declarations (exported or not)
        if let Statement::TSInterfaceDeclaration(iface) = stmt {
            let name = iface.id.name.to_string();
            if seen_names.insert(name.clone()) {
                let ts_type = convert_interface_body(&iface.body.body);
                types.push(DtsExport { name, ts_type });
            }
        }
        // Type alias declarations (exported or not)
        if let Statement::TSTypeAliasDeclaration(type_decl) = stmt {
            let name = type_decl.id.name.to_string();
            if seen_names.insert(name.clone()) {
                let ts_type = convert_oxc_type(&type_decl.type_annotation);
                types.push(DtsExport { name, ts_type });
            }
        }
        // Also check inside export declarations
        if let Statement::ExportNamedDeclaration(export_decl) = stmt
            && let Some(ref decl) = export_decl.declaration
        {
            if let Declaration::TSInterfaceDeclaration(iface) = decl {
                let name = iface.id.name.to_string();
                if seen_names.insert(name.clone()) {
                    let ts_type = convert_interface_body(&iface.body.body);
                    types.push(DtsExport { name, ts_type });
                }
            }
            if let Declaration::TSTypeAliasDeclaration(type_decl) = decl {
                let name = type_decl.id.name.to_string();
                if seen_names.insert(name.clone()) {
                    let ts_type = convert_oxc_type(&type_decl.type_annotation);
                    types.push(DtsExport { name, ts_type });
                }
            }
        }
    }

    Ok(types)
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

/// Convert oxc formal parameters to our FunctionParam list.
fn convert_formal_params(params: &FormalParameters<'_>) -> Vec<FunctionParam> {
    params
        .items
        .iter()
        .map(|p| {
            let ty = p
                .type_annotation
                .as_ref()
                .map(|ta| convert_oxc_type(&ta.type_annotation))
                .unwrap_or(TsType::Any);
            FunctionParam {
                ty,
                optional: p.optional,
            }
        })
        .collect()
}

/// Convert an oxc function declaration to our TsType::Function.
fn convert_function(
    params: &FormalParameters<'_>,
    return_type: &Option<oxc_allocator::Box<'_, oxc_ast::ast::TSTypeAnnotation<'_>>>,
) -> TsType {
    let ret = return_type
        .as_ref()
        .map(|ta| convert_oxc_type(&ta.type_annotation))
        .unwrap_or(TsType::Primitive("void".to_string()));

    TsType::Function {
        params: convert_formal_params(params),
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

/// Convert a single TSPropertySignature to an ObjectField.
fn convert_property_signature(prop: &TSPropertySignature<'_>) -> Option<ObjectField> {
    let name = property_key_name(&prop.key)?;
    let ty = prop
        .type_annotation
        .as_ref()
        .map(|ta| convert_oxc_type(&ta.type_annotation))
        .unwrap_or(TsType::Any);
    Some(ObjectField {
        name,
        ty,
        optional: prop.optional,
    })
}

/// Convert interface body members to TsType::Object.
fn convert_interface_body(members: &[TSSignature<'_>]) -> TsType {
    let fields: Vec<ObjectField> = members
        .iter()
        .filter_map(|sig| match sig {
            TSSignature::TSPropertySignature(prop) => convert_property_signature(prop),
            _ => None,
        })
        .collect();
    TsType::Object(fields)
}

/// Extract a name from a PropertyKey.
fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    key.name().map(|n| n.to_string())
}

/// Shared match arms for converting oxc type variants to `TsType`.
///
/// Both `OxcTSType` and `TSTupleElement` share the same inherited TSType variants
/// (via oxc's `@inherit TSType` macro). This macro generates the identical match
/// arms for both enum types, eliminating ~100 lines of duplication.
macro_rules! convert_shared_type_arms {
    ($prefix:ident, $value:expr) => {
        match $value {
            // Keywords
            $prefix::TSStringKeyword(_) => TsType::Primitive("string".to_string()),
            $prefix::TSNumberKeyword(_) => TsType::Primitive("number".to_string()),
            $prefix::TSBooleanKeyword(_) => TsType::Primitive("boolean".to_string()),
            $prefix::TSVoidKeyword(_) => TsType::Primitive("void".to_string()),
            $prefix::TSNeverKeyword(_) => TsType::Primitive("never".to_string()),
            $prefix::TSBigIntKeyword(_) => TsType::Primitive("bigint".to_string()),
            $prefix::TSSymbolKeyword(_) => TsType::Primitive("symbol".to_string()),
            $prefix::TSNullKeyword(_) => TsType::Null,
            $prefix::TSUndefinedKeyword(_) => TsType::Undefined,
            $prefix::TSAnyKeyword(_) => TsType::Any,
            $prefix::TSUnknownKeyword(_) => TsType::Unknown,

            // Union: T | U | V
            $prefix::TSUnionType(union) => {
                TsType::Union(union.types.iter().map(|t| convert_oxc_type(t)).collect())
            }

            // Array shorthand: T[]
            $prefix::TSArrayType(arr) => {
                TsType::Array(Box::new(convert_oxc_type(&arr.element_type)))
            }

            // Tuple: [T, U]
            $prefix::TSTupleType(tuple) => TsType::Tuple(
                tuple
                    .element_types
                    .iter()
                    .map(|e| convert_tuple_element(e))
                    .collect(),
            ),

            // Function type: (params) => ReturnType
            $prefix::TSFunctionType(func) => {
                let ret = convert_oxc_type(&func.return_type.type_annotation);
                TsType::Function {
                    params: convert_formal_params(&func.params),
                    return_type: Box::new(ret),
                }
            }

            // Type reference: named type or generic
            $prefix::TSTypeReference(type_ref) => {
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
            $prefix::TSTypeLiteral(lit) => {
                let fields: Vec<ObjectField> = lit
                    .members
                    .iter()
                    .filter_map(|sig| match sig {
                        TSSignature::TSPropertySignature(prop) => convert_property_signature(prop),
                        _ => None,
                    })
                    .collect();
                TsType::Object(fields)
            }

            // Parenthesized type: (T)
            $prefix::TSParenthesizedType(paren) => convert_oxc_type(&paren.type_annotation),

            // Intersection: T & U — use the first non-empty-object type.
            // The common TS pattern `T & {}` is used for type inference hints and is a no-op.
            $prefix::TSIntersectionType(inter) => {
                let meaningful: Vec<TsType> = inter
                    .types
                    .iter()
                    .map(|t| convert_oxc_type(t))
                    .filter(|t| !matches!(t, TsType::Object(fields) if fields.is_empty()))
                    .collect();
                match meaningful.len() {
                    0 => TsType::Object(Vec::new()),
                    _ => meaningful.into_iter().next().unwrap(),
                }
            }

            // Literal types (string/number/boolean literals)
            $prefix::TSLiteralType(lit) => match &lit.literal {
                oxc_ast::ast::TSLiteral::StringLiteral(_) => {
                    TsType::Primitive("string".to_string())
                }
                oxc_ast::ast::TSLiteral::NumericLiteral(_) => {
                    TsType::Primitive("number".to_string())
                }
                oxc_ast::ast::TSLiteral::BooleanLiteral(_) => {
                    TsType::Primitive("boolean".to_string())
                }
                _ => TsType::Named("literal".to_string()),
            },

            // import("module").Name or import("module").Name<Args>
            $prefix::TSImportType(import_ty) => {
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

            // Type operator: readonly T, keyof T, unique T
            $prefix::TSTypeOperatorType(op) => convert_oxc_type(&op.type_annotation),

            // typeof expression: typeof useState
            $prefix::TSTypeQuery(query) => {
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
    };
}

/// Convert an oxc TSType to our TsType enum.
fn convert_oxc_type(ty: &OxcTSType<'_>) -> TsType {
    convert_shared_type_arms!(OxcTSType, ty)
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
///
/// Handles tuple-specific variants (optional, rest, named members) then
/// delegates inherited TSType variants to the shared macro.
fn convert_tuple_element(el: &TSTupleElement<'_>) -> TsType {
    match el {
        TSTupleElement::TSOptionalType(opt) => convert_oxc_type(&opt.type_annotation),
        TSTupleElement::TSRestType(rest) => convert_oxc_type(&rest.type_annotation),
        TSTupleElement::TSNamedTupleMember(member) => convert_tuple_element(&member.element_type),
        // All other variants are inherited from TSType
        _ => convert_shared_type_arms!(TSTupleElement, el),
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
            params: params
                .into_iter()
                .map(|(ty, optional)| FunctionParam { ty, optional })
                .collect(),
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
                    let raw_name = trimmed[..colon]
                        .trim()
                        .trim_start_matches("readonly ")
                        .trim();
                    let optional = raw_name.ends_with('?');
                    let field_name = raw_name.trim_end_matches('?').to_string();
                    let type_str = trimmed[colon + 1..].trim().trim_end_matches(';').trim();
                    if !field_name.is_empty() && !field_name.starts_with('[') {
                        fields.push(ObjectField {
                            name: field_name,
                            ty: parse_type_str(type_str),
                            optional,
                        });
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

// ── Interface extends resolution ───────────────────────────────

/// Collect interface body fields and extends names from a statement.
fn collect_interface_info(
    stmt: &Statement<'_>,
    bodies: &mut HashMap<String, Vec<ObjectField>>,
    extends: &mut HashMap<String, Vec<String>>,
) {
    let collect_from_iface =
        |iface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
         bodies: &mut HashMap<String, Vec<ObjectField>>,
         extends: &mut HashMap<String, Vec<String>>| {
            let name = iface.id.name.to_string();
            if let TsType::Object(fields) = convert_interface_body(&iface.body.body) {
                bodies.entry(name.clone()).or_insert(fields);
            }

            if !iface.extends.is_empty() {
                let parent_names: Vec<String> = iface
                    .extends
                    .iter()
                    .filter_map(|heritage| {
                        if let oxc_ast::ast::Expression::Identifier(ident) = &heritage.expression {
                            Some(ident.name.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                extends.insert(name, parent_names);
            }
        };

    match stmt {
        Statement::TSInterfaceDeclaration(iface) => {
            collect_from_iface(iface, bodies, extends);
        }
        Statement::ExportNamedDeclaration(export_decl) => {
            if let Some(ref decl) = export_decl.declaration
                && let Declaration::TSInterfaceDeclaration(iface) = decl
            {
                collect_from_iface(iface, bodies, extends);
            }
        }
        Statement::TSModuleDeclaration(ns_decl) => {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns_decl.body {
                for inner_stmt in &block.body {
                    collect_interface_info(inner_stmt, bodies, extends);
                }
            }
        }
        _ => {}
    }
}

/// Recursively resolve all fields for an interface, including inherited ones.
fn resolve_interface_fields(
    name: &str,
    bodies: &HashMap<String, Vec<ObjectField>>,
    extends: &HashMap<String, Vec<String>>,
) -> Vec<ObjectField> {
    let mut visited = HashSet::new();
    resolve_interface_fields_inner(name, bodies, extends, &mut visited)
}

fn resolve_interface_fields_inner(
    name: &str,
    bodies: &HashMap<String, Vec<ObjectField>>,
    extends: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
) -> Vec<ObjectField> {
    if !visited.insert(name.to_string()) {
        return Vec::new(); // cycle detected
    }

    let mut all_fields = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    // First add parent fields (so child fields override)
    if let Some(parents) = extends.get(name) {
        for parent in parents {
            let parent_fields = resolve_interface_fields_inner(parent, bodies, extends, visited);
            for field in parent_fields {
                if seen_names.insert(field.name.clone()) {
                    all_fields.push(field);
                }
            }
        }
    }

    // Then add own fields (override parents with same name)
    if let Some(own_fields) = bodies.get(name) {
        for field in own_fields {
            if seen_names.insert(field.name.clone()) {
                all_fields.push(field.clone());
            }
        }
    }

    all_fields
}
