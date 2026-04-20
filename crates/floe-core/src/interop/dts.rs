//! .d.ts export parsing: reads declaration files and extracts exports using oxc_parser.

use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, ExportDefaultDeclarationKind, ExportNamedDeclaration,
    FormalParameters, MethodDefinitionKind, PropertyKey, Statement, TSEnumDeclaration,
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

/// Generic type parameter metadata captured at declaration time. Used to
/// pad partial type argument lists with TypeScript's own defaults so that
/// `Context<{ Bindings: B }>` and `Context<{ Bindings: B }, any, {}>`
/// resolve to the same Foreign type when the declaration reads
/// `interface Context<E = Env, P extends string = any, I extends Input = {}>`.
#[derive(Debug, Clone)]
pub struct GenericParamInfo {
    pub name: String,
    pub default: Option<TsType>,
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
            (".tsx", ".d.ts"),
            (".ts", ".d.ts"),
        ] {
            if let Some(stripped) = base_str.strip_suffix(ext) {
                let dts_path = PathBuf::from(format!("{stripped}{dts_ext}"));
                if dts_path.exists() {
                    return Some(dts_path);
                }
            }
        }
    }

    // Try adding .d.ts / .d.mts / .d.cts directly (barrel exports often
    // omit extensions entirely).
    for dts_ext in &[".d.ts", ".d.mts", ".d.cts"] {
        let with_dts = parent_dir.join(format!("{source}{dts_ext}"));
        if with_dts.exists() {
            return Some(with_dts);
        }
    }

    // Try as directory with index.d.ts / index.d.mts / index.d.cts
    if base.is_dir() {
        for idx in &["index.d.ts", "index.d.mts", "index.d.cts"] {
            let index = base.join(idx);
            if index.exists() {
                return Some(index);
            }
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
    // `/// <reference path="..." />` directives are consumed by the parser
    // without surfacing — we rescan the raw source to pull them back out.
    let mut reexport_sources = extract_triple_slash_references(content);
    // All top-level declarations by name — used to resolve `export default X`,
    // `export { X as Y }`, and `typeof X` against the declared types.
    let mut local_types: HashMap<String, TsType> = HashMap::new();
    let mut aliased_reexports: Vec<(String, String)> = Vec::new();
    let mut default_export_target: Option<String> = None;

    for stmt in &ret.program.body {
        if let Statement::TSExportAssignment(assign) = stmt
            && let oxc_ast::ast::Expression::Identifier(ident) = &assign.expression
        {
            export_assignment_name = Some(ident.name.to_string());
        }

        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            extract_from_export_named(export_decl, &mut exports, &mut seen_names);

            if let Some(ref decl) = export_decl.declaration {
                record_local_type(decl, &mut local_types);
            }

            // `export { X as Y }` without a declaration — collected here,
            // resolved against local_types once the full file is scanned.
            if export_decl.declaration.is_none() {
                for spec in &export_decl.specifiers {
                    let exported_name = spec.exported.name().to_string();
                    let local_name = spec.local.name().to_string();
                    if !exported_name.is_empty()
                        && !local_name.is_empty()
                        && exported_name != local_name
                    {
                        aliased_reexports.push((exported_name, local_name));
                    }
                }
            }
        }

        if let Statement::ExportDefaultDeclaration(default_decl) = stmt {
            match &default_decl.declaration {
                ExportDefaultDeclarationKind::Identifier(ident) => {
                    default_export_target = Some(ident.name.to_string());
                }
                ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                    let ts_type = convert_function(&func.params, &func.return_type);
                    if seen_names.insert("default".to_string()) {
                        exports.push(DtsExport {
                            name: "default".to_string(),
                            ts_type,
                        });
                    }
                }
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    if let Some(mut export) = convert_class_declaration(class) {
                        export.name = "default".to_string();
                        if seen_names.insert(export.name.clone()) {
                            exports.push(export);
                        }
                    }
                }
                _ => {}
            }
        }

        // Non-exported top-level declarations are recorded for `typeof X`,
        // `export default X`, and `export { X as Y }` resolution.
        for (name, ts_type) in statement_entries(stmt) {
            local_types.entry(name).or_insert(ts_type);
        }

        if let Statement::ExportAllDeclaration(export_all) = stmt {
            reexport_sources.push(export_all.source.value.to_string());
        }

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

    // Resolve `export default X` against locally-declared types.
    if let Some(ref target) = default_export_target
        && let Some(ts_type) = local_types.get(target)
        && seen_names.insert("default".to_string())
    {
        exports.push(DtsExport {
            name: "default".to_string(),
            ts_type: ts_type.clone(),
        });
    }

    // Resolve aliased re-exports (`export { X as Y }`) for every declaration kind.
    for (exported_name, local_name) in &aliased_reexports {
        if seen_names.contains(exported_name) {
            continue;
        }
        if let Some(ts_type) = local_types.get(local_name) {
            seen_names.insert(exported_name.clone());
            exports.push(DtsExport {
                name: exported_name.clone(),
                ts_type: ts_type.clone(),
            });
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

    // Second pass: resolve `typeof X` references against local declarations.
    // Uses the full `local_types` map so `typeof someConst` works for any
    // declaration kind, not just functions.
    for export in &mut exports {
        if let TsType::Named(ref s) = export.ts_type
            && let Some(ref_name) = s.strip_prefix("typeof ")
            && let Some(resolved_type) = local_types.get(ref_name)
        {
            export.ts_type = resolved_type.clone();
        }
    }

    // Collect generic parameter constraints from function declarations.
    // When tsgo doesn't resolve generics (e.g. `ResultDate extends Date`),
    // the probe output may reference the parameter name instead of the
    // resolved type. Map parameter names to their constraint bounds.
    let mut generic_param_bounds: HashMap<String, TsType> = HashMap::new();
    for stmt in &ret.program.body {
        collect_fn_generic_constraints(stmt, &mut generic_param_bounds);
    }
    if !generic_param_bounds.is_empty() {
        for export in &mut exports {
            resolve_generic_params(&mut export.ts_type, &generic_param_bounds);
        }
    }

    // Third pass: collect type aliases and resolve interface extends.
    // Type aliases like `type DraggableId<T = string> = T` are resolved
    // to their default type parameter value (e.g. `string`).
    let mut type_aliases: HashMap<String, TsType> = HashMap::new();
    for stmt in &ret.program.body {
        collect_type_alias_defaults(stmt, &mut type_aliases);
    }

    let mut interface_bodies = collect_and_resolve_interfaces(&ret.program.body);

    // Resolve type aliases in interface field types
    // (e.g. DraggableId<TId> → string when DraggableId<T = string> = T)
    if !type_aliases.is_empty() {
        for fields in interface_bodies.values_mut() {
            for field in fields.iter_mut() {
                resolve_field_type_aliases(&mut field.ty, &type_aliases);
            }
        }
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

/// Parse .d.ts exports from a string. Strips `IMPORT_SOURCE_SENTINEL` from any
/// `import("pkg").X` references so callers see clean identifiers. Use
/// `parse_dts_exports_with_import_sources` when the caller (e.g. the tsgo
/// probe runner) needs the encoded source to drive cross-module alias
/// expansion.
pub(super) fn parse_dts_exports_from_str(content: &str) -> Result<Vec<DtsExport>, String> {
    let mut exports = parse_dts_content(content).map(|r| r.exports)?;
    for export in &mut exports {
        strip_import_sentinels(&mut export.ts_type);
    }
    Ok(exports)
}

/// Variant of `parse_dts_exports_from_str` that preserves the
/// `IMPORT_SOURCE_SENTINEL` encoding. Caller is responsible for either
/// expanding or stripping the encoding before the types reach the boundary
/// wrapper. Used by the tsgo probe pipeline so it can resolve cross-module
/// type alias references (`import("pkg").Handler<E>`) against the owning
/// module's .d.ts (#1234).
pub(super) fn parse_dts_exports_with_import_sources(
    content: &str,
) -> Result<Vec<DtsExport>, String> {
    parse_dts_content(content).map(|r| r.exports)
}

/// Parse ALL type/interface declarations from a source file, including non-exported ones.
/// Used to resolve type references like `IssueFilters` that appear in probe output
/// but aren't exported from the source module. Strips
/// `IMPORT_SOURCE_SENTINEL` from any `import("pkg").X` references so callers
/// never see the internal encoding (only the tsgo probe runner wants it).
pub(super) fn parse_all_types_from_str(content: &str) -> Result<Vec<DtsExport>, String> {
    let mut types = parse_all_types_raw(content)?;
    for t in &mut types {
        strip_import_sentinels(&mut t.ts_type);
    }
    Ok(types)
}

fn parse_all_types_raw(content: &str) -> Result<Vec<DtsExport>, String> {
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
                let ts_type = convert_interface_body_named(&iface.body.body, Some(&name));
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
                    let ts_type = convert_interface_body_named(&iface.body.body, Some(&name));
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

/// Extract exports from an `export` declaration (export function/const/type/interface/class/enum).
fn extract_from_export_named(
    export_decl: &ExportNamedDeclaration<'_>,
    exports: &mut Vec<DtsExport>,
    seen_names: &mut HashSet<String>,
) {
    let Some(ref decl) = export_decl.declaration else {
        return;
    };
    extract_from_declaration(decl, exports, seen_names);
}

/// Convert a declaration into `DtsExport`s and append unseen names.
/// Overloaded functions produce duplicates; the first wins.
fn extract_from_declaration(
    decl: &Declaration<'_>,
    exports: &mut Vec<DtsExport>,
    seen_names: &mut HashSet<String>,
) {
    for (name, ts_type) in declaration_entries(decl) {
        if seen_names.insert(name.clone()) {
            exports.push(DtsExport { name, ts_type });
        }
    }
}

/// Convert a class declaration to an object type whose fields are the
/// class's public members (methods and properties). A "constructor"
/// synthetic field, typed as `(args) => Self`, is added when an explicit
/// constructor is present so callers can `new Foo(x)` through the normal
/// callable boundary.
fn convert_class_declaration(class: &Class<'_>) -> Option<DtsExport> {
    let name = class.id.as_ref()?.name.to_string();
    let mut fields: Vec<ObjectField> = Vec::new();
    let mut ctor_params: Option<Vec<FunctionParam>> = None;

    for el in &class.body.body {
        match el {
            ClassElement::MethodDefinition(method) => {
                let field_name = property_key_name(&method.key);
                match method.kind {
                    MethodDefinitionKind::Constructor => {
                        ctor_params = Some(convert_formal_params(&method.value.params));
                    }
                    MethodDefinitionKind::Method => {
                        if let Some(n) = field_name {
                            let ret = method
                                .value
                                .return_type
                                .as_ref()
                                .map(|ta| convert_oxc_type(&ta.type_annotation))
                                .unwrap_or(TsType::Any);
                            fields.push(ObjectField {
                                name: n,
                                ty: TsType::Function {
                                    params: convert_formal_params(&method.value.params),
                                    return_type: Box::new(ret),
                                },
                                optional: method.optional,
                            });
                        }
                    }
                    MethodDefinitionKind::Get => {
                        if let Some(n) = field_name {
                            let ty = method
                                .value
                                .return_type
                                .as_ref()
                                .map(|ta| convert_oxc_type(&ta.type_annotation))
                                .unwrap_or(TsType::Any);
                            fields.push(ObjectField {
                                name: n,
                                ty,
                                optional: method.optional,
                            });
                        }
                    }
                    MethodDefinitionKind::Set => {
                        if let Some(n) = field_name {
                            let ty = method
                                .value
                                .params
                                .items
                                .first()
                                .and_then(|p| p.type_annotation.as_ref())
                                .map(|ta| convert_oxc_type(&ta.type_annotation))
                                .unwrap_or(TsType::Any);
                            // Only add if not already present (getter + setter).
                            if !fields.iter().any(|f| f.name == n) {
                                fields.push(ObjectField {
                                    name: n,
                                    ty,
                                    optional: method.optional,
                                });
                            }
                        }
                    }
                }
            }
            ClassElement::PropertyDefinition(prop) => {
                if let Some(n) = property_key_name(&prop.key) {
                    let ty = prop
                        .type_annotation
                        .as_ref()
                        .map(|ta| convert_oxc_type(&ta.type_annotation))
                        .unwrap_or(TsType::Any);
                    fields.push(ObjectField {
                        name: n,
                        ty,
                        optional: prop.optional,
                    });
                }
            }
            _ => {}
        }
    }

    // Resolve any `this` return types to the class name.
    for field in &mut fields {
        resolve_this_in_type(&mut field.ty, &name);
    }

    // Synthetic constructor field makes `new Foo(x)` callable through the
    // normal function boundary.
    if let Some(params) = ctor_params {
        fields.push(ObjectField {
            name: "constructor".to_string(),
            ty: TsType::Function {
                params,
                return_type: Box::new(TsType::Named(name.clone())),
            },
            optional: false,
        });
    }

    Some(DtsExport {
        name,
        ts_type: TsType::Object(fields),
    })
}

/// Convert an enum declaration. `enum Color { Red, Green }` becomes a
/// union of the member names as string literals (`"Red" | "Green"`);
/// numeric-valued members widen to `number`.
fn convert_enum_declaration(enum_decl: &TSEnumDeclaration<'_>) -> Option<DtsExport> {
    let name = enum_decl.id.name.to_string();
    let mut has_string = false;
    let mut has_number = false;
    let mut members: Vec<TsType> = Vec::new();

    for member in &enum_decl.body.members {
        let member_name = enum_member_name(&member.id)?;
        match &member.initializer {
            Some(oxc_ast::ast::Expression::StringLiteral(s)) => {
                has_string = true;
                members.push(TsType::StringLiteral(s.value.to_string()));
            }
            Some(oxc_ast::ast::Expression::NumericLiteral(n)) => {
                has_number = true;
                members.push(TsType::NumberLiteral(n.value));
            }
            _ => {
                // No initializer / computed — default to the member name.
                members.push(TsType::StringLiteral(member_name));
            }
        }
    }

    let ts_type = if has_number && !has_string {
        TsType::Primitive("number".to_string())
    } else if members.len() == 1 {
        members.into_iter().next().unwrap()
    } else {
        TsType::Union(members)
    };

    Some(DtsExport { name, ts_type })
}

/// Shared conversion table: for each named top-level declaration we
/// support, produce an `(ident name, TsType)` pair. Used from every
/// declaration-site — exported, non-exported top-level, inside a
/// namespace — so the kind list lives in one place.
fn declaration_entries(decl: &Declaration<'_>) -> Vec<(String, TsType)> {
    match decl {
        Declaration::FunctionDeclaration(func) => func
            .id
            .as_ref()
            .map(|id| {
                vec![(
                    id.name.to_string(),
                    convert_function(&func.params, &func.return_type),
                )]
            })
            .unwrap_or_default(),
        Declaration::VariableDeclaration(var_decl) => var_decl
            .declarations
            .iter()
            .filter_map(convert_variable_declarator)
            .map(|e| (e.name, e.ts_type))
            .collect(),
        Declaration::TSTypeAliasDeclaration(type_decl) => vec![(
            type_decl.id.name.to_string(),
            convert_oxc_type(&type_decl.type_annotation),
        )],
        Declaration::TSInterfaceDeclaration(iface) => {
            let name = iface.id.name.to_string();
            let ts_type = convert_interface_body_named(&iface.body.body, Some(&name));
            vec![(name, ts_type)]
        }
        Declaration::ClassDeclaration(class) => convert_class_declaration(class)
            .map(|e| vec![(e.name, e.ts_type)])
            .unwrap_or_default(),
        Declaration::TSEnumDeclaration(enum_decl) => convert_enum_declaration(enum_decl)
            .map(|e| vec![(e.name, e.ts_type)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Same as `declaration_entries` but for the Statement-level variants at
/// the top of a file or namespace body. The payload types differ from
/// `Declaration`, so dispatch happens here rather than through a shared
/// projection.
fn statement_entries(stmt: &Statement<'_>) -> Vec<(String, TsType)> {
    match stmt {
        Statement::FunctionDeclaration(func) => func
            .id
            .as_ref()
            .map(|id| {
                vec![(
                    id.name.to_string(),
                    convert_function(&func.params, &func.return_type),
                )]
            })
            .unwrap_or_default(),
        Statement::VariableDeclaration(var_decl) => var_decl
            .declarations
            .iter()
            .filter_map(convert_variable_declarator)
            .map(|e| (e.name, e.ts_type))
            .collect(),
        Statement::TSTypeAliasDeclaration(type_decl) => vec![(
            type_decl.id.name.to_string(),
            convert_oxc_type(&type_decl.type_annotation),
        )],
        Statement::TSInterfaceDeclaration(iface) => {
            let name = iface.id.name.to_string();
            let ts_type = convert_interface_body_named(&iface.body.body, Some(&name));
            vec![(name, ts_type)]
        }
        Statement::ClassDeclaration(class) => convert_class_declaration(class)
            .map(|e| vec![(e.name, e.ts_type)])
            .unwrap_or_default(),
        Statement::TSEnumDeclaration(enum_decl) => convert_enum_declaration(enum_decl)
            .map(|e| vec![(e.name, e.ts_type)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Record a declaration's type in `local_types` so later passes
/// (`export default`, aliased re-exports, `typeof X`) can resolve it.
fn record_local_type(decl: &Declaration<'_>, local_types: &mut HashMap<String, TsType>) {
    for (name, ts_type) in declaration_entries(decl) {
        local_types.entry(name).or_insert(ts_type);
    }
}

/// Extract function/const/type/interface/class/enum declarations from inside
/// a namespace body. Recurses into nested namespaces (`declare namespace A.B`)
/// and nested inner namespaces so their exports bubble up to the parent.
fn extract_from_namespace_body(body: &Option<TSModuleDeclarationBody<'_>>) -> Vec<DtsExport> {
    let mut exports = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    let Some(body) = body else { return exports };

    let block = match body {
        TSModuleDeclarationBody::TSModuleBlock(block) => block,
        TSModuleDeclarationBody::TSModuleDeclaration(nested) => {
            // `declare namespace A.B { ... }` parses as nested modules.
            return extract_from_namespace_body(&nested.body);
        }
    };

    for stmt in &block.body {
        match stmt {
            Statement::ExportNamedDeclaration(export_decl) => {
                extract_from_export_named(export_decl, &mut exports, &mut seen_names);
            }
            Statement::TSModuleDeclaration(inner) => {
                for ex in extract_from_namespace_body(&inner.body) {
                    if seen_names.insert(ex.name.clone()) {
                        exports.push(ex);
                    }
                }
            }
            other => {
                for (name, ts_type) in statement_entries(other) {
                    if seen_names.insert(name.clone()) {
                        exports.push(DtsExport { name, ts_type });
                    }
                }
            }
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
pub(super) fn convert_function(
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
pub(super) fn convert_variable_declarator(
    declarator: &VariableDeclarator<'_>,
) -> Option<DtsExport> {
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

/// Convert an object-literal / interface body into a `TsType`. Handles
/// property signatures, method signatures (getters / regular / setters),
/// construct signatures (`new (...) => T`), call signatures (callable
/// objects), and index signatures (`[k: K]: V` → `Record<K, V>`). When the
/// body has no named fields but does carry a call signature, the caller
/// typically wants to treat the whole thing as a function — we surface
/// that as `TsType::Function` to match existing behavior.
fn convert_type_literal(members: &[TSSignature<'_>]) -> TsType {
    let fields = collect_object_fields(members);
    if fields.is_empty() {
        // No named fields: look for a lone call signature and surface as a
        // function type (common for overloaded builder methods in npm).
        for sig in members {
            if let TSSignature::TSCallSignatureDeclaration(call) = sig {
                return TsType::Function {
                    params: convert_formal_params(&call.params),
                    return_type: Box::new(
                        call.return_type
                            .as_ref()
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any),
                    ),
                };
            }
        }
        // Construct-only signature `new (...)`: treat as its return type so
        // `new Foo(x)` is usable.
        for sig in members {
            if let TSSignature::TSConstructSignatureDeclaration(ctor) = sig {
                return TsType::Function {
                    params: convert_formal_params(&ctor.params),
                    return_type: Box::new(
                        ctor.return_type
                            .as_ref()
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any),
                    ),
                };
            }
        }
    }
    // Index signature { [k: K]: V } becomes a Record<K, V> when there are
    // no other fields; otherwise the caller has a mixed dict-plus-fields
    // shape that we best-effort as the object so the named fields survive.
    if fields.is_empty() {
        for sig in members {
            if let TSSignature::TSIndexSignature(idx) = sig
                && let Some((k, v)) = index_signature_kv(idx)
            {
                return TsType::Generic {
                    name: "Record".to_string(),
                    args: vec![k, v],
                };
            }
        }
    }
    TsType::Object(fields)
}

/// Collect every supported field shape (properties, getters, methods,
/// setters) from an object-literal / interface body into `ObjectField`s.
fn collect_object_fields(members: &[TSSignature<'_>]) -> Vec<ObjectField> {
    members
        .iter()
        .filter_map(|sig| match sig {
            TSSignature::TSPropertySignature(prop) => convert_property_signature(prop),
            TSSignature::TSMethodSignature(method) => {
                let name = property_key_name(&method.key)?;
                match method.kind {
                    oxc_ast::ast::TSMethodSignatureKind::Get => {
                        let ty = method
                            .return_type
                            .as_ref()
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any);
                        Some(ObjectField {
                            name,
                            ty,
                            optional: method.optional,
                        })
                    }
                    oxc_ast::ast::TSMethodSignatureKind::Set => {
                        // Setter: expose the field name with the setter's
                        // parameter type so consumers can at least read the
                        // type. Getter + setter pairs merge later.
                        let ty = method
                            .params
                            .items
                            .first()
                            .and_then(|p| p.type_annotation.as_ref())
                            .map(|ta| convert_oxc_type(&ta.type_annotation))
                            .unwrap_or(TsType::Any);
                        Some(ObjectField {
                            name,
                            ty,
                            optional: method.optional,
                        })
                    }
                    oxc_ast::ast::TSMethodSignatureKind::Method => {
                        let ty = convert_function(&method.params, &method.return_type);
                        Some(ObjectField {
                            name,
                            ty,
                            optional: method.optional,
                        })
                    }
                }
            }
            _ => None,
        })
        .collect()
}

/// Extract `K` and `V` from an index signature `[k: K]: V`.
fn index_signature_kv(idx: &oxc_ast::ast::TSIndexSignature<'_>) -> Option<(TsType, TsType)> {
    let key_param = idx.parameters.first()?;
    let key = convert_oxc_type(&key_param.type_annotation.type_annotation);
    let value = convert_oxc_type(&idx.type_annotation.type_annotation);
    Some((key, value))
}

/// Merge intersection members. Object members' fields are concatenated
/// (later wins on name collision); non-object members collapse through
/// the `T & {}` no-op pattern. When only one non-empty member survives,
/// emit it directly to avoid a spurious union-like wrapper.
fn merge_intersection(parts: Vec<TsType>) -> TsType {
    let mut merged_fields: Vec<ObjectField> = Vec::new();
    let mut field_index: HashMap<String, usize> = HashMap::new();
    let mut non_object: Vec<TsType> = Vec::new();
    for part in parts {
        match part {
            TsType::Object(fields) if fields.is_empty() => {}
            TsType::Object(fields) => {
                for field in fields {
                    if let Some(&idx) = field_index.get(&field.name) {
                        merged_fields[idx] = field;
                    } else {
                        field_index.insert(field.name.clone(), merged_fields.len());
                        merged_fields.push(field);
                    }
                }
            }
            other => non_object.push(other),
        }
    }
    match (merged_fields.is_empty(), non_object.len()) {
        (true, 0) => TsType::Object(Vec::new()),
        (true, 1) => non_object.into_iter().next().unwrap(),
        (false, 0) => TsType::Object(merged_fields),
        // Mixed: surface the object side (the dominant case for npm patterns
        // like `SomeClass & { extra: X }`). The non-object halves are
        // usually opaque class references we can't structurally merge.
        (false, _) => TsType::Object(merged_fields),
        (true, _) => non_object.into_iter().next().unwrap(),
    }
}

/// `keyof T` — when `T` is a concrete object type, yield the union of its
/// field names as string literals. For everything else fall back to
/// `string` (what TypeScript uses when the keys can't be enumerated).
fn keyof_of(ty: &TsType) -> TsType {
    if let TsType::Object(fields) = ty {
        let keys: Vec<TsType> = fields
            .iter()
            .map(|f| TsType::StringLiteral(f.name.clone()))
            .collect();
        match keys.len() {
            0 => TsType::Primitive("string".to_string()),
            1 => keys.into_iter().next().unwrap(),
            _ => TsType::Union(keys),
        }
    } else {
        TsType::Primitive("string".to_string())
    }
}

/// Walk `ty` and replace every `TsType::This` with `TsType::Named(self_name)`.
/// Used when converting interface / class bodies so fluent-builder return
/// types stay usable.
fn resolve_this_in_type(ty: &mut TsType, self_name: &str) {
    match ty {
        TsType::This => *ty = TsType::Named(self_name.to_string()),
        TsType::Union(parts) | TsType::Tuple(parts) => {
            for p in parts {
                resolve_this_in_type(p, self_name);
            }
        }
        TsType::Array(inner) => resolve_this_in_type(inner, self_name),
        TsType::Generic { args, .. } => {
            for a in args {
                resolve_this_in_type(a, self_name);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for p in params {
                resolve_this_in_type(&mut p.ty, self_name);
            }
            resolve_this_in_type(return_type, self_name);
        }
        TsType::Object(fields) => {
            for f in fields {
                resolve_this_in_type(&mut f.ty, self_name);
            }
        }
        _ => {}
    }
}

/// Convert interface body members to `TsType::Object`. When `self_name`
/// is provided, every `this` return type inside the body resolves to that
/// interface name so fluent / builder chains keep working.
pub(super) fn convert_interface_body_named(
    members: &[TSSignature<'_>],
    self_name: Option<&str>,
) -> TsType {
    let fields = collect_object_fields(members);
    let mut ty = TsType::Object(fields);
    if let Some(name) = self_name {
        resolve_this_in_type(&mut ty, name);
    }
    ty
}

/// Scan `.d.ts` content for `/// <reference path="..." />` directives.
/// These must appear at the top of the file; stop at the first non-
/// comment, non-blank line.
pub(super) fn extract_triple_slash_references(content: &str) -> Vec<String> {
    content
        .lines()
        .take_while(|line| is_header_line(line))
        .filter_map(parse_reference_line)
        .collect()
}

/// True for a blank line or a line starting with `///` — i.e. the zone
/// at the top of a file where reference directives can legally appear.
fn is_header_line(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t.starts_with("///")
}

/// Extract the path from one line if it's a `/// <reference path="..." />`
/// directive. Returns `None` for blanks, other triple-slash directives,
/// or malformed syntax.
fn parse_reference_line(line: &str) -> Option<String> {
    let body = line.trim().strip_prefix("///")?.trim();
    let rest = body.strip_prefix("<reference")?;
    let after_path = rest.split_once("path=").map(|(_, r)| r)?.trim_start();
    let quote = after_path
        .chars()
        .next()
        .filter(|c| *c == '"' || *c == '\'')?;
    let inner = &after_path[1..];
    let end = inner.find(quote)?;
    Some(inner[..end].to_string())
}

/// Extract a name from a PropertyKey.
fn property_key_name(key: &PropertyKey<'_>) -> Option<String> {
    key.name().map(|n| n.to_string())
}

/// Extract a name from a TSEnumMember's key. Identifier and string-literal
/// names are supported; computed members return `None`.
fn enum_member_name(name: &oxc_ast::ast::TSEnumMemberName<'_>) -> Option<String> {
    match name {
        oxc_ast::ast::TSEnumMemberName::Identifier(id) => Some(id.name.to_string()),
        oxc_ast::ast::TSEnumMemberName::String(s) => Some(s.value.to_string()),
        _ => None,
    }
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

            // Constructor type: `new (params) => ReturnType` — surface as a
            // regular function type so callers can wire it through the normal
            // callable boundary.
            $prefix::TSConstructorType(ctor) => {
                let ret = convert_oxc_type(&ctor.return_type.type_annotation);
                TsType::Function {
                    params: convert_formal_params(&ctor.params),
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
            // Also handles callable objects: { (params): ReturnType; },
            // construct signatures `new (...)`, and index signatures `[k: K]: V`.
            $prefix::TSTypeLiteral(lit) => convert_type_literal(&lit.members),

            // Parenthesized type: (T)
            $prefix::TSParenthesizedType(paren) => convert_oxc_type(&paren.type_annotation),

            // Intersection: T & U — merge object members rather than
            // dropping everything after the first, so `{a: number} & {b: string}`
            // keeps both fields (lib.dom leans on this heavily).
            $prefix::TSIntersectionType(inter) => {
                let parts: Vec<TsType> = inter.types.iter().map(|t| convert_oxc_type(t)).collect();
                merge_intersection(parts)
            }

            // Literal types (string/number/boolean literals) — preserved so
            // unions like `"up" | "down"` keep their discriminators. Floe
            // widens where it makes sense at the boundary (number/boolean).
            $prefix::TSLiteralType(lit) => match &lit.literal {
                oxc_ast::ast::TSLiteral::StringLiteral(s) => {
                    TsType::StringLiteral(s.value.to_string())
                }
                oxc_ast::ast::TSLiteral::NumericLiteral(n) => TsType::NumberLiteral(n.value),
                oxc_ast::ast::TSLiteral::BooleanLiteral(b) => TsType::BooleanLiteral(b.value),
                oxc_ast::ast::TSLiteral::BigIntLiteral(_) => {
                    TsType::Primitive("bigint".to_string())
                }
                oxc_ast::ast::TSLiteral::UnaryExpression(u) => {
                    // Negative numeric literal: `-1` comes in as a UnaryExpression.
                    if let oxc_ast::ast::Expression::NumericLiteral(n) = &u.argument {
                        TsType::NumberLiteral(-n.value)
                    } else {
                        TsType::Unknown
                    }
                }
                _ => TsType::Unknown,
            },

            // import("module").Name[<Args>] — without a qualifier, the
            // import points at the module itself (the default export slot),
            // which at our boundary is just `Unknown` until the target is
            // resolved.
            //
            // When a qualifier IS present we encode the source module into
            // the name using a unit-separator sentinel (`\x1F` is not a
            // valid TS identifier char). A later pass in the tsgo runner
            // decodes this, parses the referenced module's .d.ts for type
            // aliases, and substitutes function-shaped aliases so lambda
            // hints propagate through cross-module callback signatures
            // like `handler: Handler<E>` from `@floeorg/hono` (#1234).
            $prefix::TSImportType(import_ty) => {
                if let Some(ref qualifier) = import_ty.qualifier {
                    let raw_name = import_qualifier_to_string(qualifier);
                    let module = import_ty.source.value.to_string();
                    let name = if module.is_empty() {
                        raw_name
                    } else {
                        encode_import_source(&module, &raw_name)
                    };
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
                    TsType::Unknown
                }
            }

            // Type operator: readonly T, keyof T, unique T. `keyof` produces
            // a union of string literal keys; the other operators are erasures
            // at the type level so we just unwrap them.
            $prefix::TSTypeOperatorType(op) => match op.operator {
                oxc_ast::ast::TSTypeOperatorOperator::Keyof => {
                    keyof_of(&convert_oxc_type(&op.type_annotation))
                }
                _ => convert_oxc_type(&op.type_annotation),
            },

            // typeof expression: typeof useState, typeof React.Component
            $prefix::TSTypeQuery(query) => {
                let name = match &query.expr_name {
                    oxc_ast::ast::TSTypeQueryExprName::IdentifierReference(ident) => {
                        ident.name.to_string()
                    }
                    oxc_ast::ast::TSTypeQueryExprName::QualifiedName(qn) => {
                        ts_qualified_name_to_string(qn)
                    }
                    _ => "unknown".to_string(),
                };
                TsType::Named(format!("typeof {name}"))
            }

            // Conditional type: T extends U ? X : Y — approximate as the
            // union of the two branches. Real evaluation would require a
            // full type-level interpreter.
            $prefix::TSConditionalType(cond) => TsType::Union(vec![
                convert_oxc_type(&cond.true_type),
                convert_oxc_type(&cond.false_type),
            ]),

            // `infer R` inside a conditional — the binder, not useful at
            // our boundary, but surface it as a fresh Named so downstream
            // unions (Generic args for ReturnType<T>) remain usable.
            $prefix::TSInferType(_) => TsType::Unknown,

            // Mapped type: `{ [K in keyof T]: ... }` — without type-level
            // evaluation we can't materialise the fields, so surface as
            // Unknown. The common case `{ [K in keyof T]: T[K] }` (Readonly)
            // then gets treated as a plain object shape by callers that
            // narrow against the original type.
            $prefix::TSMappedType(_) => TsType::Unknown,

            // Indexed access: T['k'] — without evaluating the lookup we
            // can't return the field type. Use the source type so the
            // surrounding code still typechecks as "something from T".
            $prefix::TSIndexedAccessType(access) => convert_oxc_type(&access.object_type),

            // Template literal type: `` `prefix.${K}` `` — widen to string.
            $prefix::TSTemplateLiteralType(_) => TsType::Primitive("string".to_string()),

            // `this` appearing as a type — resolved by the enclosing
            // interface/class when possible (see `resolve_this_in_type`).
            $prefix::TSThisType(_) => TsType::This,

            // Everything else: surface as Unknown. `Named("unknown")` is
            // a string-typed escape hatch that wrapper.rs treats as the
            // widest TS type — callers should narrow before use.
            _ => TsType::Unknown,
        }
    };
}

/// Convert an oxc TSType to our TsType enum.
pub(super) fn convert_oxc_type(ty: &OxcTSType<'_>) -> TsType {
    convert_shared_type_arms!(OxcTSType, ty)
}

/// Convert a TSTypeName to a string like "Foo" or "React.FC".
fn ts_type_name_to_string(name: &TSTypeName<'_>) -> String {
    match name {
        TSTypeName::IdentifierReference(ident) => ident.name.to_string(),
        TSTypeName::QualifiedName(qn) => ts_qualified_name_to_string(qn),
        TSTypeName::ThisExpression(_) => "this".to_string(),
    }
}

/// Convert a TSQualifiedName (`A.B.C`) to its dotted string form. Reused
/// for both named type refs and `typeof X.Y` queries so neither has to
/// clone the qualified name through a throwaway allocator.
fn ts_qualified_name_to_string(qn: &oxc_ast::ast::TSQualifiedName<'_>) -> String {
    format!("{}.{}", ts_type_name_to_string(&qn.left), qn.right.name)
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
pub(super) fn collect_interface_info(
    stmt: &Statement<'_>,
    bodies: &mut HashMap<String, Vec<ObjectField>>,
    extends: &mut HashMap<String, Vec<String>>,
) {
    let collect_from_iface =
        |iface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
         bodies: &mut HashMap<String, Vec<ObjectField>>,
         extends: &mut HashMap<String, Vec<String>>| {
            let name = iface.id.name.to_string();
            if let TsType::Object(fields) =
                convert_interface_body_named(&iface.body.body, Some(&name))
            {
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
        // `declare global { ... }` — oxc parses this as TSGlobalDeclaration
        Statement::TSGlobalDeclaration(global_decl) => {
            for inner_stmt in &global_decl.body.body {
                collect_interface_info(inner_stmt, bodies, extends);
            }
        }
        _ => {}
    }
}

/// Recursively resolve all fields for an interface, including inherited ones.
pub(super) fn resolve_interface_fields(
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

/// Collect and resolve all interface definitions from AST statements.
///
/// Collects interface bodies and extends chains, then resolves extends
/// by merging parent fields into each child. Shared between `parse_dts_content`
/// and `parse_ambient_lib`.
pub(super) fn collect_and_resolve_interfaces(
    stmts: &[Statement<'_>],
) -> HashMap<String, Vec<ObjectField>> {
    let mut bodies: HashMap<String, Vec<ObjectField>> = HashMap::new();
    let mut extends: HashMap<String, Vec<String>> = HashMap::new();

    for stmt in stmts {
        collect_interface_info(stmt, &mut bodies, &mut extends);
    }

    let resolved_names: Vec<String> = extends.keys().cloned().collect();
    for name in &resolved_names {
        let fields = resolve_interface_fields(name, &bodies, &extends);
        bodies.insert(name.clone(), fields);
    }

    bodies
}

/// Collect type alias default values from statements.
/// For `type DraggableId<TId extends string = string> = TId`, records
/// DraggableId → Primitive("string") (the default type parameter value).
fn collect_type_alias_defaults(stmt: &Statement<'_>, aliases: &mut HashMap<String, TsType>) {
    match stmt {
        Statement::TSTypeAliasDeclaration(type_decl) => {
            process_type_alias(type_decl, aliases);
        }
        Statement::ExportNamedDeclaration(export_decl) => {
            if let Some(ref decl) = export_decl.declaration
                && let Declaration::TSTypeAliasDeclaration(type_decl) = decl
            {
                process_type_alias(type_decl, aliases);
            }
        }
        Statement::TSModuleDeclaration(ns_decl) => {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns_decl.body {
                for inner_stmt in &block.body {
                    collect_type_alias_defaults(inner_stmt, aliases);
                }
            }
        }
        _ => {}
    }
}

/// Process a single type alias declaration, resolving identity and chained aliases.
fn process_type_alias(
    type_decl: &oxc_ast::ast::TSTypeAliasDeclaration<'_>,
    aliases: &mut HashMap<String, TsType>,
) {
    let name = type_decl.id.name.to_string();
    if let OxcTSType::TSTypeReference(ref_type) = &type_decl.type_annotation
        && let TSTypeName::IdentifierReference(ident) = &ref_type.type_name
    {
        let ref_name = ident.name.to_string();

        // Direct type parameter reference (identity alias: `type X<T> = T`)
        if let Some(ref type_params) = type_decl.type_parameters {
            for param in &type_params.params {
                if param.name.to_string() == ref_name {
                    if let Some(ref default) = param.default {
                        aliases.insert(name, convert_oxc_type(default));
                    }
                    return;
                }
            }
        }

        // Chained alias (`type DraggableId<T> = Id<T>` where Id is already resolved)
        if let Some(resolved) = aliases.get(&ref_name).cloned() {
            aliases.insert(name, resolved);
        }
    }
}

/// Function-shaped type alias body + generic params (alias declaration order).
/// For `type Handler<E> = (c: Context<{ Bindings: E }>) => Response`, `params`
/// is `["E"]` and `body` is the `TsType::Function` literal.
#[derive(Debug, Clone)]
pub(super) struct TypeAliasDef {
    pub(super) params: Vec<String>,
    pub(super) body: TsType,
}

/// Sentinel used to encode `import("module").Name` as `module\x1FName` inside a
/// `TsType::Named` or `TsType::Generic`. `\x1F` (unit separator) is not a valid
/// TS identifier char, so this can't clash with a legitimate type name.
pub(super) const IMPORT_SOURCE_SENTINEL: char = '\x1F';

pub(super) fn encode_import_source(module: &str, name: &str) -> String {
    format!("{module}{IMPORT_SOURCE_SENTINEL}{name}")
}

/// Decode `module\x1FName` back into `(module, name)`. Returns `None` when the
/// sentinel is absent (i.e. the name is a plain local reference).
pub(super) fn decode_import_source(encoded: &str) -> Option<(&str, &str)> {
    encoded.split_once(IMPORT_SOURCE_SENTINEL)
}

/// Strip `IMPORT_SOURCE_SENTINEL` prefixes from every name inside a TsType so
/// the type can be safely wrapped at the Floe boundary. Called after
/// cross-module alias expansion has had its chance to use the encoded source.
pub(super) fn strip_import_sentinels(ty: &mut TsType) {
    match ty {
        TsType::Named(name) => {
            if let Some((_, clean)) = decode_import_source(name) {
                *name = clean.to_string();
            }
        }
        TsType::Generic { name, args } => {
            if let Some((_, clean)) = decode_import_source(name) {
                *name = clean.to_string();
            }
            for arg in args {
                strip_import_sentinels(arg);
            }
        }
        TsType::Union(parts) => {
            for p in parts {
                strip_import_sentinels(p);
            }
        }
        TsType::Array(inner) => strip_import_sentinels(inner),
        TsType::Object(fields) => {
            for f in fields {
                strip_import_sentinels(&mut f.ty);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for p in params {
                strip_import_sentinels(&mut p.ty);
            }
            strip_import_sentinels(return_type);
        }
        TsType::Tuple(parts) => {
            for p in parts {
                strip_import_sentinels(p);
            }
        }
        _ => {}
    }
}

/// Names that `wrap_boundary_type` special-cases and must stay as
/// `TsType::Generic { name, ... }` references so the boundary wrapper can
/// recognise them. Expanding these to their underlying body would strip the
/// name and defeat helpers like `unwrap_set_state_action` (which peels
/// `SetStateAction<T>` out of `Dispatch<...>` to produce `(T) -> ()`).
const BOUNDARY_RESERVED_ALIASES: &[&str] = &[
    "Array",
    "ReadonlyArray",
    "Promise",
    "FloeOption",
    "Record",
    "Dispatch",
    "SetStateAction",
];

fn is_boundary_reserved(name: &str) -> bool {
    BOUNDARY_RESERVED_ALIASES.contains(&name)
}

/// Expand cross-module type alias references encoded with
/// `IMPORT_SOURCE_SENTINEL`. `aliases_by_module[mod][name]` is the resolved
/// alias body (collected by parsing `mod`'s .d.ts). Only function-shaped
/// aliases are in the map — see `collect_function_alias_bodies`. Names on
/// `BOUNDARY_RESERVED_ALIASES` are left as references so the boundary
/// wrapper can apply its custom handling.
pub(super) fn expand_cross_module_aliases(
    ty: &mut TsType,
    aliases_by_module: &HashMap<String, HashMap<String, TypeAliasDef>>,
    depth: u32,
) {
    if depth > 16 {
        return;
    }
    match ty {
        TsType::Named(name) => {
            if let Some((module, alias_name)) = decode_import_source(name)
                && !is_boundary_reserved(alias_name)
                && let Some(aliases) = aliases_by_module.get(module)
                && let Some(def) = aliases.get(alias_name)
                && def.params.is_empty()
            {
                *ty = def.body.clone();
                expand_cross_module_aliases(ty, aliases_by_module, depth + 1);
            }
        }
        TsType::Generic { name, args } => {
            for arg in args.iter_mut() {
                expand_cross_module_aliases(arg, aliases_by_module, depth + 1);
            }
            if let Some((module, alias_name)) = decode_import_source(name)
                && !is_boundary_reserved(alias_name)
                && let Some(aliases) = aliases_by_module.get(module)
                && let Some(def) = aliases.get(alias_name)
            {
                let mut body = def.body.clone();
                if !def.params.is_empty() {
                    let subst: HashMap<String, TsType> = def
                        .params
                        .iter()
                        .cloned()
                        .zip(args.iter().cloned())
                        .collect();
                    substitute_type_params(&mut body, &subst);
                }
                *ty = body;
                expand_cross_module_aliases(ty, aliases_by_module, depth + 1);
            }
        }
        TsType::Union(parts) => {
            for p in parts {
                expand_cross_module_aliases(p, aliases_by_module, depth + 1);
            }
        }
        TsType::Array(inner) => expand_cross_module_aliases(inner, aliases_by_module, depth + 1),
        TsType::Object(fields) => {
            for f in fields {
                expand_cross_module_aliases(&mut f.ty, aliases_by_module, depth + 1);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for p in params {
                expand_cross_module_aliases(&mut p.ty, aliases_by_module, depth + 1);
            }
            expand_cross_module_aliases(return_type, aliases_by_module, depth + 1);
        }
        TsType::Tuple(parts) => {
            for p in parts {
                expand_cross_module_aliases(p, aliases_by_module, depth + 1);
            }
        }
        _ => {}
    }
}

/// Walk a TsType and collect every unique module referenced via the
/// `IMPORT_SOURCE_SENTINEL` encoding. Used by the tsgo runner to decide which
/// external .d.ts files need to be parsed for alias expansion.
pub(super) fn collect_referenced_modules(ty: &TsType, out: &mut HashSet<String>) {
    match ty {
        TsType::Named(name) => {
            if let Some((module, _)) = decode_import_source(name) {
                out.insert(module.to_string());
            }
        }
        TsType::Generic { name, args } => {
            if let Some((module, _)) = decode_import_source(name) {
                out.insert(module.to_string());
            }
            for a in args {
                collect_referenced_modules(a, out);
            }
        }
        TsType::Union(parts) | TsType::Tuple(parts) => {
            for p in parts {
                collect_referenced_modules(p, out);
            }
        }
        TsType::Array(inner) => collect_referenced_modules(inner, out),
        TsType::Object(fields) => {
            for f in fields {
                collect_referenced_modules(&f.ty, out);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for p in params {
                collect_referenced_modules(&p.ty, out);
            }
            collect_referenced_modules(return_type, out);
        }
        _ => {}
    }
}

/// Parse a .d.ts file at `path` and return only its function-shaped type
/// alias definitions, keyed by alias name. Returns an empty map on any
/// failure — cross-module alias expansion is a best-effort optimisation.
pub(super) fn collect_function_aliases_from_file(
    path: &std::path::Path,
) -> HashMap<String, TypeAliasDef> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let allocator = Allocator::default();
    let source_type = SourceType::tsx();
    let ret = Parser::new(&allocator, &content, source_type).parse();
    if ret.panicked {
        return HashMap::new();
    }
    let mut aliases: HashMap<String, TypeAliasDef> = HashMap::new();
    for stmt in &ret.program.body {
        collect_function_alias_bodies(stmt, &mut aliases);
    }
    aliases
}

/// Recognise aliases whose body is (or is a union containing) a function. Only
/// these participate in lambda-hint propagation, so we restrict expansion to
/// them to avoid churning unrelated Foreign references.
fn contains_function_shape(ty: &TsType) -> bool {
    match ty {
        TsType::Function { .. } => true,
        TsType::Union(parts) => parts.iter().any(contains_function_shape),
        _ => false,
    }
}

/// Recurses into `declare namespace` bodies so block-scoped aliases are
/// collected alongside top-level ones.
fn collect_function_alias_bodies(
    stmt: &Statement<'_>,
    aliases: &mut HashMap<String, TypeAliasDef>,
) {
    let decl = match stmt {
        Statement::TSTypeAliasDeclaration(td) => Some(td.as_ref()),
        Statement::ExportNamedDeclaration(ed) => match &ed.declaration {
            Some(Declaration::TSTypeAliasDeclaration(td)) => Some(td.as_ref()),
            _ => None,
        },
        Statement::TSModuleDeclaration(ns) => {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns.body {
                for inner in &block.body {
                    collect_function_alias_bodies(inner, aliases);
                }
            }
            None
        }
        _ => None,
    };
    let Some(type_decl) = decl else {
        return;
    };
    let body = convert_oxc_type(&type_decl.type_annotation);
    if !contains_function_shape(&body) {
        return;
    }
    let name = type_decl.id.name.to_string();
    let params = type_decl
        .type_parameters
        .as_ref()
        .map(|tps| tps.params.iter().map(|p| p.name.to_string()).collect())
        .unwrap_or_default();
    aliases.insert(name, TypeAliasDef { params, body });
}

/// Substitute named type-parameter references in a TsType with their bound
/// arguments. Used when expanding a generic alias like `Handler<E>` — the
/// alias body's `E` references are replaced with whatever was passed at the
/// reference site.
fn substitute_type_params(ty: &mut TsType, subst: &HashMap<String, TsType>) {
    match ty {
        TsType::Named(name) => {
            if let Some(replacement) = subst.get(name) {
                *ty = replacement.clone();
            }
        }
        TsType::Generic { name, args } => {
            if let Some(replacement) = subst.get(name) {
                *ty = replacement.clone();
            } else {
                for arg in args {
                    substitute_type_params(arg, subst);
                }
            }
        }
        TsType::Union(parts) => {
            for p in parts {
                substitute_type_params(p, subst);
            }
        }
        TsType::Array(inner) => substitute_type_params(inner, subst),
        TsType::Object(fields) => {
            for f in fields {
                substitute_type_params(&mut f.ty, subst);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for p in params {
                substitute_type_params(&mut p.ty, subst);
            }
            substitute_type_params(return_type, subst);
        }
        TsType::Tuple(parts) => {
            for p in parts {
                substitute_type_params(p, subst);
            }
        }
        _ => {}
    }
}

/// Resolve type aliases in a TsType field. Replaces Named("DraggableId<TId>")
/// and Generic { name: "DraggableId", .. } with the alias's default type.
fn resolve_field_type_aliases(ty: &mut TsType, aliases: &HashMap<String, TsType>) {
    match ty {
        TsType::Named(name) => {
            let base_name = name.split('<').next().unwrap_or(name);
            if let Some(resolved) = aliases.get(base_name) {
                *ty = resolved.clone();
            }
        }
        TsType::Generic { name, .. } => {
            if let Some(resolved) = aliases.get(name.as_str()) {
                *ty = resolved.clone();
            }
        }
        TsType::Union(members) => {
            for member in members {
                resolve_field_type_aliases(member, aliases);
            }
        }
        TsType::Array(inner) => {
            resolve_field_type_aliases(inner, aliases);
        }
        TsType::Object(fields) => {
            for field in fields {
                resolve_field_type_aliases(&mut field.ty, aliases);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for param in params {
                resolve_field_type_aliases(&mut param.ty, aliases);
            }
            resolve_field_type_aliases(return_type, aliases);
        }
        TsType::Tuple(types) => {
            for t in types {
                resolve_field_type_aliases(t, aliases);
            }
        }
        _ => {}
    }
}

/// Collect generic parameter constraint bounds from function declarations.
/// For `function f<ResultDate extends Date>(...): ResultDate`, records
/// `ResultDate → Named("Date")`.
fn collect_fn_generic_constraints(stmt: &Statement<'_>, bounds: &mut HashMap<String, TsType>) {
    use oxc_ast::ast::{Declaration, TSModuleDeclarationBody};

    // Helper: extract constraints from a TSTypeParameterDeclaration
    fn extract_params(
        params: &oxc_ast::ast::TSTypeParameterDeclaration<'_>,
        bounds: &mut HashMap<String, TsType>,
    ) {
        let param_names: Vec<String> = params.params.iter().map(|p| p.name.to_string()).collect();
        for param in &params.params {
            let name = param.name.to_string();
            if let Some(ref constraint) = param.constraint {
                let bound = convert_oxc_type(constraint);
                // Only add if the bound is a concrete type (not another parameter)
                if !matches!(&bound, TsType::Named(n) if param_names.contains(n)) {
                    bounds.insert(name, bound);
                }
            }
        }
    }

    match stmt {
        Statement::ExportNamedDeclaration(export_decl) => {
            if let Some(ref decl) = export_decl.declaration {
                match decl {
                    Declaration::FunctionDeclaration(fn_decl) => {
                        if let Some(ref tp) = fn_decl.type_parameters {
                            extract_params(tp, bounds);
                        }
                    }
                    Declaration::TSTypeAliasDeclaration(type_decl) => {
                        if let Some(ref tp) = type_decl.type_parameters {
                            extract_params(tp, bounds);
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::TSModuleDeclaration(ns_decl) => {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns_decl.body {
                for inner_stmt in &block.body {
                    collect_fn_generic_constraints(inner_stmt, bounds);
                }
            }
        }
        _ => {}
    }
}

/// Resolve generic parameter names to their constraint bounds in a TsType.
fn resolve_generic_params(ty: &mut TsType, bounds: &HashMap<String, TsType>) {
    match ty {
        TsType::Named(name) => {
            if let Some(resolved) = bounds.get(name) {
                *ty = resolved.clone();
            }
        }
        TsType::Generic { args, .. } => {
            for arg in args {
                resolve_generic_params(arg, bounds);
            }
        }
        TsType::Function {
            params,
            return_type,
        } => {
            for param in params {
                resolve_generic_params(&mut param.ty, bounds);
            }
            resolve_generic_params(return_type, bounds);
        }
        TsType::Object(fields) => {
            for field in fields {
                resolve_generic_params(&mut field.ty, bounds);
            }
        }
        TsType::Array(inner) => resolve_generic_params(inner, bounds),
        TsType::Union(parts) => {
            for part in parts {
                resolve_generic_params(part, bounds);
            }
        }
        TsType::Tuple(parts) => {
            for part in parts {
                resolve_generic_params(part, bounds);
            }
        }
        _ => {}
    }
}

/// Collect generic type-parameter defaults for interfaces and type aliases
/// so the checker can pad partial type-argument lists. Keyed by the
/// generic's declaration name (e.g. "Context"), value is one entry per
/// positional type parameter in source order.
pub fn collect_generic_param_defs_from_source(
    source: &str,
) -> Result<HashMap<String, Vec<GenericParamInfo>>, String> {
    use oxc_allocator::Allocator;
    use oxc_parser::{ParseOptions, Parser as OxcParser};
    use oxc_span::SourceType;

    let allocator = Allocator::default();
    let source_type = SourceType::ts();
    let ret = OxcParser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: false,
            ..ParseOptions::default()
        })
        .parse();
    if !ret.errors.is_empty() {
        return Err(format!("parse errors: {:?}", ret.errors));
    }
    let mut out: HashMap<String, Vec<GenericParamInfo>> = HashMap::new();
    for stmt in &ret.program.body {
        collect_generic_param_defs(stmt, &mut out);
    }
    Ok(out)
}

/// Walk a single statement (recursively into module blocks and export
/// declarations) and record the generic parameter list for each
/// interface and type alias declaration.
fn collect_generic_param_defs(
    stmt: &Statement<'_>,
    out: &mut HashMap<String, Vec<GenericParamInfo>>,
) {
    use oxc_ast::ast::{Declaration, TSModuleDeclarationBody};

    match stmt {
        Statement::TSInterfaceDeclaration(iface) => {
            record_generic_params(&iface.id.name, iface.type_parameters.as_deref(), out);
        }
        Statement::TSTypeAliasDeclaration(type_decl) => {
            record_generic_params(
                &type_decl.id.name,
                type_decl.type_parameters.as_deref(),
                out,
            );
        }
        Statement::ExportNamedDeclaration(export_decl) => {
            if let Some(ref decl) = export_decl.declaration {
                match decl {
                    Declaration::TSInterfaceDeclaration(iface) => {
                        record_generic_params(
                            &iface.id.name,
                            iface.type_parameters.as_deref(),
                            out,
                        );
                    }
                    Declaration::TSTypeAliasDeclaration(type_decl) => {
                        record_generic_params(
                            &type_decl.id.name,
                            type_decl.type_parameters.as_deref(),
                            out,
                        );
                    }
                    _ => {}
                }
            }
        }
        Statement::TSModuleDeclaration(ns_decl) => {
            if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns_decl.body {
                for inner in &block.body {
                    collect_generic_param_defs(inner, out);
                }
            }
        }
        _ => {}
    }
}

fn record_generic_params(
    name: &str,
    type_params: Option<&oxc_ast::ast::TSTypeParameterDeclaration<'_>>,
    out: &mut HashMap<String, Vec<GenericParamInfo>>,
) {
    let Some(tp) = type_params else {
        return;
    };
    let infos: Vec<GenericParamInfo> = tp
        .params
        .iter()
        .map(|p| GenericParamInfo {
            name: p.name.to_string(),
            default: p.default.as_ref().map(convert_oxc_type),
        })
        .collect();
    if !infos.is_empty() {
        out.insert(name.to_string(), infos);
    }
}
