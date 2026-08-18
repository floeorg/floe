//! Ambient type loading from TypeScript lib definition files.
//!
//! Loads ambient types based on the project's `tsconfig.json`:
//! - `compilerOptions.lib` → TS built-in lib files (lib.dom.d.ts, lib.es2020.d.ts, etc.)
//! - `compilerOptions.types` → `@types/*` packages (e.g., @types/node)
//! - Auto-includes all `@types/*` when `types` is not set (TS default)
//!
//! Extracts:
//! - `declare var` / `declare function` → global variable/function types
//! - `interface` definitions → for resolving member access on globals
//! - `declare global { ... }` blocks → for @types/node style globals

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Declaration, Statement, TSModuleDeclaration, TSModuleDeclarationBody, TSModuleDeclarationName,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

use super::dts::{
    collect_and_resolve_interfaces, collect_type_aliases, convert_function,
    convert_variable_declarator,
};
use super::wrapper::wrap_boundary_type;
use crate::checker::Type;

/// Ambient declarations parsed from TypeScript lib files.
#[derive(Debug, Default, Clone)]
pub struct AmbientDeclarations {
    /// Global variables/functions (e.g., `window`, `document`, `navigator`, `fetch`).
    pub globals: Vec<(String, Type)>,
    /// Type definitions (interfaces) for resolving member access.
    pub types: HashMap<String, Type>,
    /// Names of ambient namespaces (`declare namespace Intl { ... }`), including
    /// the qualified name of a nested one (`NodeJS`, `Intl`, `JSX`, `A.B`).
    ///
    /// Floe does not model namespaces yet (#848), so `types` holds each member
    /// under its bare name and loses the namespace it came from. This set keeps
    /// the namespace names themselves, which is what the type resolver needs to
    /// tell `Intl.DateTimeFormat` apart from a typo.
    pub namespaces: HashSet<String>,
}

impl AmbientDeclarations {
    fn merge(&mut self, other: AmbientDeclarations, seen_globals: &mut HashSet<String>) {
        for (name, ty) in other.globals {
            if seen_globals.insert(name.clone()) {
                self.globals.push((name, ty));
            }
        }
        self.types.extend(other.types);
        self.namespaces.extend(other.namespaces);
    }
}

// ── TypeScript lib configuration ────────────────────────────────

/// Parsed `compilerOptions.lib` and `compilerOptions.types` from tsconfig.json.
struct TsAmbientConfig {
    /// Lib file names to load (e.g., ["lib.es2020.d.ts", "lib.dom.d.ts"]).
    lib_files: Vec<String>,
    /// `@types/*` packages to load. `None` means auto-include all.
    types: Option<Vec<String>>,
}

/// Parse ambient config from the project's tsconfig.json.
fn parse_ambient_config(project_dir: &Path) -> TsAmbientConfig {
    let Some(tsconfig_path) = crate::resolve::find_tsconfig_from(project_dir) else {
        return TsAmbientConfig {
            lib_files: default_lib_files(),
            types: None,
        };
    };

    let Ok(content) = std::fs::read_to_string(&tsconfig_path) else {
        return TsAmbientConfig {
            lib_files: default_lib_files(),
            types: None,
        };
    };

    let stripped = crate::resolve::strip_jsonc_comments(&content);
    let json: serde_json::Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(_) => {
            return TsAmbientConfig {
                lib_files: default_lib_files(),
                types: None,
            };
        }
    };

    // Parse compilerOptions.lib
    let lib_files = json
        .pointer("/compilerOptions/lib")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(lib_name_to_filename)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(default_lib_files);

    // Parse compilerOptions.types — None means "auto-include all @types/*"
    let types = json
        .pointer("/compilerOptions/types")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    TsAmbientConfig { lib_files, types }
}

/// Default lib files when tsconfig doesn't specify `lib`.
fn default_lib_files() -> Vec<String> {
    vec!["lib.es5.d.ts".to_string(), "lib.dom.d.ts".to_string()]
}

/// Convert a tsconfig lib name to its filename.
/// e.g., "ES2020" → "lib.es2020.d.ts", "DOM" → "lib.dom.d.ts"
fn lib_name_to_filename(name: &str) -> String {
    format!("lib.{}.d.ts", name.to_lowercase())
}

// ── Reference directive resolution ──────────────────────────────

/// Extract `/// <reference lib="..." />` directives from file content.
fn extract_reference_libs(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("/// <reference lib=\"") {
                rest.strip_suffix("\" />").map(lib_name_to_filename)
            } else {
                None
            }
        })
        .collect()
}

/// Extract `/// <reference path="..." />` directives from file content.
fn extract_reference_paths(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("/// <reference path=\"") {
                rest.strip_suffix("\" />").map(String::from)
            } else {
                None
            }
        })
        .collect()
}

// ── node_modules lookup ─────────────────────────────────────────

/// Every `node_modules` directory that Node searches from `start`, nearest
/// first.
///
/// `find_project_dir` returns the first parent that holds a `node_modules`,
/// and in an npm or pnpm workspace that is the package directory. A package
/// directory holds a small local `node_modules`, while `typescript` and the
/// `@types` packages hoist to the workspace root. A loader that reads one
/// directory finds nothing there, so it has to walk up the tree the way Node
/// module resolution does. The walk stops at the filesystem root.
fn node_modules_dirs(start: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    start
        .ancestors()
        .map(|dir| dir.join("node_modules"))
        .filter(|dir| dir.is_dir())
}

// ── TS lib file loading ─────────────────────────────────────────

/// Find the TypeScript lib directory from a project root.
///
/// The nearest `node_modules` that holds `typescript` wins, so a package that
/// pins its own copy gets that copy and not the workspace root copy.
fn find_ts_lib_dir(project_dir: &Path) -> Option<PathBuf> {
    node_modules_dirs(project_dir).find_map(|node_modules| ts_lib_dir_in(&node_modules))
}

/// Find the TypeScript lib directory inside one `node_modules`.
fn ts_lib_dir_in(node_modules: &Path) -> Option<PathBuf> {
    let standard = node_modules.join("typescript/lib");
    if standard.is_dir() {
        return Some(standard);
    }

    let pnpm_dir = node_modules.join(".pnpm");
    if pnpm_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&pnpm_dir)
    {
        let mut ts_dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("typescript@"))
            })
            .map(|e| e.path().join("node_modules/typescript/lib"))
            .filter(|p| p.is_dir())
            .collect();
        ts_dirs.sort();
        if let Some(dir) = ts_dirs.pop() {
            return Some(dir);
        }
    }

    None
}

/// Load a TS lib file and recursively follow `/// <reference lib="..." />` directives.
fn load_lib_file(
    lib_dir: &Path,
    filename: &str,
    visited: &mut HashSet<String>,
    merged: &mut AmbientDeclarations,
    seen_globals: &mut HashSet<String>,
) {
    if !visited.insert(filename.to_string()) {
        return;
    }

    let path = lib_dir.join(filename);
    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    // Follow reference lib directives first (load dependencies before this file)
    let ref_libs = extract_reference_libs(&content);
    for ref_lib in &ref_libs {
        load_lib_file(lib_dir, ref_lib, visited, merged, seen_globals);
    }

    let result = parse_ambient_lib(&content);
    merged.merge(result, seen_globals);
}

// ── @types/* package loading ────────────────────────────────────

/// Find all installed `@types/*` package names.
///
/// A workspace spreads the `@types` packages over more than one
/// `node_modules`, so this is the union of every one the walk passes. The
/// nearest copy of a name wins, and a later duplicate is dropped.
fn discover_types_packages(project_dir: &Path) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();

    for node_modules in node_modules_dirs(project_dir) {
        for name in types_packages_in(&node_modules) {
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }
    }

    names
}

/// Find the `@types/*` package names inside one `node_modules`.
fn types_packages_in(node_modules: &Path) -> Vec<String> {
    let types_dir = node_modules.join("@types");
    if !types_dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&types_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            // Skip scoped packages (e.g., @types/babel__core) for now
            if name.starts_with('.') {
                return None;
            }
            Some(name)
        })
        .collect()
}

/// Find the entry .d.ts for a types package.
///
/// Searches every `node_modules` from the project directory up, nearest
/// first, so a package that hoists to a workspace root still resolves.
fn find_types_entry(project_dir: &Path, package_name: &str) -> Option<PathBuf> {
    node_modules_dirs(project_dir)
        .find_map(|node_modules| types_entry_in(&node_modules, package_name))
}

/// Find the entry .d.ts for a types package inside one `node_modules`.
///
/// Searches in `@types/{name}` for standard @types packages, and also
/// directly in `{name}` for packages that ship their own types (e.g.,
/// `@cloudflare/workers-types`).
fn types_entry_in(node_modules: &Path, package_name: &str) -> Option<PathBuf> {
    // Try @types/{name} first (standard convention)
    let at_types_dir = node_modules.join(format!("@types/{package_name}"));
    // Then try the package directly (for packages that ship their own types)
    let direct_dir = node_modules.join(package_name);

    let types_dir = if at_types_dir.is_dir() {
        at_types_dir
    } else if direct_dir.is_dir() {
        direct_dir
    } else {
        return None;
    };

    // Check index.d.ts (most common)
    let index = types_dir.join("index.d.ts");
    if index.exists() {
        return Some(index);
    }

    // Check package.json types/typings field
    let pkg_json = types_dir.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_json)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(types_field) = json
            .get("types")
            .or_else(|| json.get("typings"))
            .and_then(|v| v.as_str())
    {
        let entry = types_dir.join(types_field);
        if entry.exists() {
            return Some(entry);
        }
    }

    None
}

/// Load an @types package, following `/// <reference path="..." />` directives.
fn load_types_package(
    entry_path: &Path,
    merged: &mut AmbientDeclarations,
    seen_globals: &mut HashSet<String>,
) {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    load_types_file(entry_path, &mut visited, merged, seen_globals);
}

/// Load a single .d.ts file from an @types package, following path references.
fn load_types_file(
    file_path: &Path,
    visited: &mut HashSet<PathBuf>,
    merged: &mut AmbientDeclarations,
    seen_globals: &mut HashSet<String>,
) {
    let canonical = file_path
        .canonicalize()
        .unwrap_or_else(|_| file_path.to_path_buf());
    if !visited.insert(canonical) {
        return;
    }

    let Ok(content) = std::fs::read_to_string(file_path) else {
        return;
    };

    let parent = file_path.parent().unwrap_or(Path::new("."));

    // Follow reference path directives
    let ref_paths = extract_reference_paths(&content);
    for ref_path in &ref_paths {
        let resolved = parent.join(ref_path);
        if resolved.exists() {
            load_types_file(&resolved, visited, merged, seen_globals);
        }
    }

    let result = parse_ambient_lib(&content);
    merged.merge(result, seen_globals);
}

// ── Main entry point ────────────────────────────────────────────

/// Load ambient type declarations based on the project's tsconfig.json.
///
/// Reads `compilerOptions.lib` and `compilerOptions.types` to determine
/// which lib files and @types packages to load.
pub fn load_ambient_types(project_dir: &Path) -> Option<AmbientDeclarations> {
    let config = parse_ambient_config(project_dir);

    let mut merged = AmbientDeclarations::default();
    let mut seen_globals: HashSet<String> = HashSet::new();
    let mut loaded_any = false;

    // Load TS lib files based on compilerOptions.lib
    if let Some(lib_dir) = find_ts_lib_dir(project_dir) {
        let mut visited_libs: HashSet<String> = HashSet::new();
        for filename in &config.lib_files {
            load_lib_file(
                &lib_dir,
                filename,
                &mut visited_libs,
                &mut merged,
                &mut seen_globals,
            );
        }
        loaded_any = !visited_libs.is_empty();
    }

    // Load @types packages
    let types_to_load = match config.types {
        Some(explicit) => explicit,
        None => discover_types_packages(project_dir),
    };
    for package_name in &types_to_load {
        if let Some(entry) = find_types_entry(project_dir, package_name) {
            load_types_package(&entry, &mut merged, &mut seen_globals);
            loaded_any = true;
        }
    }

    if loaded_any { Some(merged) } else { None }
}

// ── Parser ──────────────────────────────────────────────────────

/// Parse ambient declarations from a single .d.ts file.
///
/// Handles top-level declarations and `declare global { ... }` blocks.
fn parse_ambient_lib(content: &str) -> AmbientDeclarations {
    let allocator = Allocator::default();
    let source_type = SourceType::d_ts();
    let ret = Parser::new(&allocator, content, source_type).parse();

    if ret.panicked {
        return AmbientDeclarations::default();
    }

    // Phase 1: Collect and resolve all interface + type-alias definitions.
    // Aliases lose to interfaces on name collision — interface members are
    // richer (resolved inheritance chain, call signatures), so when a lib
    // file declares both `interface X` and `type X = ...` the interface
    // wins.
    let interface_bodies = collect_and_resolve_interfaces(&ret.program.body);
    let mut types: HashMap<String, Type> = HashMap::new();
    for (name, ts_type) in collect_type_aliases(&ret.program.body) {
        if !interface_bodies.contains_key(&name) {
            types.insert(name, wrap_boundary_type(&ts_type));
        }
    }
    for (name, fields) in &interface_bodies {
        let ts_type = super::TsType::Object(fields.clone());
        types.insert(name.clone(), wrap_boundary_type(&ts_type));
    }

    // Phase 2: Collect globals from top-level and `declare global` blocks
    let mut globals: Vec<(String, Type)> = Vec::new();
    let mut seen_globals: HashSet<String> = HashSet::new();

    for stmt in &ret.program.body {
        collect_globals_from_stmt(stmt, &mut globals, &mut seen_globals, false);
    }

    // Phase 3: Collect namespace names. Phase 1 flattens a namespace member to
    // its bare name, so the namespace itself would otherwise be invisible.
    let mut namespaces: HashSet<String> = HashSet::new();
    for stmt in &ret.program.body {
        collect_namespace_names(stmt, None, &mut namespaces);
    }

    AmbientDeclarations {
        globals,
        types,
        namespaces,
    }
}

/// Record the name of every ambient namespace, and recurse into it.
///
/// `prefix` is the qualified name of the enclosing namespace, so
/// `namespace A { namespace B { } }` records both `A` and `A.B`.
///
/// The statement kinds match `collect_interface_info` and
/// `collect_type_alias_info` in `dts.rs`. A namespace that those two passes
/// reach must appear here too, or the resolver rejects a type that the type
/// tables carry.
fn collect_namespace_names(
    stmt: &Statement<'_>,
    prefix: Option<&str>,
    namespaces: &mut HashSet<String>,
) {
    match stmt {
        Statement::TSModuleDeclaration(ns_decl) => {
            collect_namespace_declaration(ns_decl, prefix, namespaces);
        }
        // `export namespace Foo { ... }` and `export declare namespace Foo
        // { ... }`. `csstype` and `undici-types` both ship this shape.
        Statement::ExportNamedDeclaration(export_decl) => {
            if let Some(Declaration::TSModuleDeclaration(ns_decl)) = &export_decl.declaration {
                collect_namespace_declaration(ns_decl, prefix, namespaces);
            }
        }
        // `declare global { ... }` adds no name of its own, so its members
        // keep the prefix they already had.
        Statement::TSGlobalDeclaration(global_decl) => {
            for inner in &global_decl.body.body {
                collect_namespace_names(inner, prefix, namespaces);
            }
        }
        _ => {}
    }
}

/// Record one namespace or module declaration, and walk into its body.
///
/// A `declare module "pkg"` block carries a string literal name. It is a
/// module and not a namespace, so it contributes no name of its own, and it
/// leaves the prefix unchanged for its children, the way `declare global`
/// does. The walk still enters it, because `@types/node` declares the
/// `NodeJS` namespace inside `declare module "buffer"` and five more like it.
fn collect_namespace_declaration(
    ns_decl: &TSModuleDeclaration<'_>,
    prefix: Option<&str>,
    namespaces: &mut HashSet<String>,
) {
    let qualified = match &ns_decl.id {
        TSModuleDeclarationName::Identifier(ident) => match prefix {
            Some(outer) => Some(format!("{outer}.{}", ident.name)),
            None => Some(ident.name.to_string()),
        },
        TSModuleDeclarationName::StringLiteral(_) => None,
    };

    let inner_prefix = qualified.as_deref().or(prefix);
    if let Some(TSModuleDeclarationBody::TSModuleBlock(block)) = &ns_decl.body {
        for inner in &block.body {
            collect_namespace_names(inner, inner_prefix, namespaces);
        }
    }

    if let Some(qualified) = qualified {
        namespaces.insert(qualified);
    }
}

/// Extract `declare var` and `declare function` from a statement.
/// Recurses into `declare global { ... }` blocks.
///
/// `inside_global` is true when we're inside a `declare global` block,
/// where declarations don't carry the `declare` flag themselves.
fn collect_globals_from_stmt(
    stmt: &Statement<'_>,
    globals: &mut Vec<(String, Type)>,
    seen: &mut HashSet<String>,
    inside_global: bool,
) {
    match stmt {
        Statement::VariableDeclaration(var_decl) if var_decl.declare || inside_global => {
            for declarator in &var_decl.declarations {
                if let Some(export) = convert_variable_declarator(declarator)
                    && seen.insert(export.name.clone())
                {
                    globals.push((export.name, wrap_boundary_type(&export.ts_type)));
                }
            }
        }
        Statement::FunctionDeclaration(func) if func.declare || inside_global => {
            if let Some(ref id) = func.id {
                let name = id.name.to_string();
                if seen.insert(name.clone()) {
                    let ts_type = convert_function(&func.params, &func.return_type);
                    globals.push((name, wrap_boundary_type(&ts_type)));
                }
            }
        }
        // `declare global { ... }` — oxc parses this as TSGlobalDeclaration
        Statement::TSGlobalDeclaration(global_decl) => {
            for inner_stmt in &global_decl.body.body {
                collect_globals_from_stmt(inner_stmt, globals, seen, true);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_declare_var() {
        let content = r#"
            interface Window {
                location: Location;
                innerWidth: number;
            }
            interface Location {
                href: string;
                origin: string;
            }
            declare var window: Window;
            declare var document: Document;
        "#;

        let result = parse_ambient_lib(content);

        assert_eq!(result.globals.len(), 2);
        assert_eq!(result.globals[0].0, "window");
        assert_eq!(result.globals[1].0, "document");

        assert!(
            matches!(&result.globals[0].1, Type::Foreign { name, .. } if name == "Window"),
            "expected Foreign(\"Window\"), got {:?}",
            result.globals[0].1
        );

        assert!(result.types.contains_key("Window"));
        assert!(result.types.contains_key("Location"));

        if let Type::Record(fields) = &result.types["Window"] {
            assert!(fields.iter().any(|(name, _)| name == "location"));
            assert!(fields.iter().any(|(name, _)| name == "innerWidth"));
        } else {
            panic!(
                "Window should be a Record, got {:?}",
                result.types["Window"]
            );
        }
    }

    #[test]
    fn parse_declare_function() {
        let content = r#"
            declare function setTimeout(handler: () => void, timeout: number): number;
            declare function clearTimeout(id: number): void;
        "#;

        let result = parse_ambient_lib(content);

        assert_eq!(result.globals.len(), 2);
        assert_eq!(result.globals[0].0, "setTimeout");
        assert_eq!(result.globals[1].0, "clearTimeout");

        assert!(
            matches!(&result.globals[0].1, Type::Function { .. }),
            "expected Function, got {:?}",
            result.globals[0].1
        );
    }

    #[test]
    fn parse_interface_extends() {
        let content = r#"
            interface NavigatorID {
                userAgent: string;
            }
            interface NavigatorLanguage {
                language: string;
            }
            interface Navigator extends NavigatorID, NavigatorLanguage {
                clipboard: Clipboard;
            }
            interface Clipboard {
                writeText(text: string): Promise<void>;
            }
            declare var navigator: Navigator;
        "#;

        let result = parse_ambient_lib(content);

        if let Type::Record(fields) = &result.types["Navigator"] {
            let field_names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
            assert!(field_names.contains(&"userAgent"), "missing userAgent");
            assert!(field_names.contains(&"language"), "missing language");
            assert!(field_names.contains(&"clipboard"), "missing clipboard");
        } else {
            panic!(
                "Navigator should be a Record, got {:?}",
                result.types["Navigator"]
            );
        }
    }

    #[test]
    fn intersection_type_takes_first() {
        let content = r#"
            interface Window {
                location: Location;
            }
            declare var window: Window & typeof globalThis;
        "#;

        let result = parse_ambient_lib(content);

        assert!(
            matches!(&result.globals[0].1, Type::Foreign { name, .. } if name == "Window"),
            "expected Foreign(\"Window\"), got {:?}",
            result.globals[0].1
        );
    }

    #[test]
    fn declare_global_extracts_globals() {
        let content = r#"
            declare global {
                function fetch(input: string): Promise<Response>;
                var process: NodeJS.Process;
                interface Response {
                    ok: boolean;
                }
            }
        "#;

        let result = parse_ambient_lib(content);

        let global_names: Vec<&str> = result.globals.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            global_names.contains(&"fetch"),
            "missing fetch from declare global"
        );
        assert!(
            global_names.contains(&"process"),
            "missing process from declare global"
        );

        assert!(
            result.types.contains_key("Response"),
            "missing Response interface from declare global"
        );
    }

    #[test]
    fn lib_name_mapping() {
        assert_eq!(lib_name_to_filename("ES2020"), "lib.es2020.d.ts");
        assert_eq!(lib_name_to_filename("DOM"), "lib.dom.d.ts");
        assert_eq!(lib_name_to_filename("ESNext"), "lib.esnext.d.ts");
        assert_eq!(
            lib_name_to_filename("ES2015.Collection"),
            "lib.es2015.collection.d.ts"
        );
    }

    #[test]
    fn extract_reference_lib_directives() {
        let content = r#"/// <reference no-default-lib="true"/>
/// <reference lib="es2019" />
/// <reference lib="es2020.bigint" />
/// <reference lib="es2020.date" />

interface Foo { x: number; }
"#;

        let refs = extract_reference_libs(content);
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], "lib.es2019.d.ts");
        assert_eq!(refs[1], "lib.es2020.bigint.d.ts");
        assert_eq!(refs[2], "lib.es2020.date.d.ts");
    }

    #[test]
    fn extract_reference_path_directives() {
        let content = r#"/// <reference path="globals.d.ts" />
/// <reference path="web-globals/fetch.d.ts" />
/// <reference lib="es2020" />
"#;

        let refs = extract_reference_paths(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], "globals.d.ts");
        assert_eq!(refs[1], "web-globals/fetch.d.ts");
    }

    #[test]
    fn top_level_type_alias_registers_as_ambient_type() {
        let content = r#"
            type AlgorithmIdentifier = string | { name: string };
        "#;
        let result = parse_ambient_lib(content);
        assert!(
            result.types.contains_key("AlgorithmIdentifier"),
            "expected AlgorithmIdentifier in types, got {:?}",
            result.types.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn type_alias_inside_declare_global_registers() {
        let content = r#"
            declare global {
                type MyToken = string;
            }
        "#;
        let result = parse_ambient_lib(content);
        assert!(result.types.contains_key("MyToken"));
    }

    #[test]
    fn interface_wins_when_name_collides_with_type_alias() {
        // lib files sometimes declare both an interface and an alias by the
        // same name. The interface has richer member info — keep it.
        let content = r#"
            type Foo = string;
            interface Foo { bar: number; }
        "#;
        let result = parse_ambient_lib(content);
        let ty = result.types.get("Foo").expect("Foo should be present");
        assert!(
            matches!(ty, Type::Record(_) | Type::Foreign { .. }),
            "expected the interface body to win, got {ty:?}"
        );
    }

    #[test]
    fn namespace_names_are_recorded() {
        // The type collectors flatten a namespace member to its bare name, so
        // the namespace set is the only record that the namespace exists.
        let content = r#"
            declare namespace Intl {
                interface DateTimeFormat { format(d: Date): string; }
                type LocalesArgument = string;
            }
            declare global {
                namespace NodeJS {
                    interface Timeout { ref(): Timeout; }
                }
            }
        "#;
        let result = parse_ambient_lib(content);

        assert!(
            result.namespaces.contains("Intl"),
            "{:?}",
            result.namespaces
        );
        assert!(
            result.namespaces.contains("NodeJS"),
            "a namespace inside `declare global` counts too: {:?}",
            result.namespaces
        );
        // The members stay under their bare names, which is what this fixes.
        assert!(result.types.contains_key("DateTimeFormat"));
        assert!(result.types.contains_key("Timeout"));
    }

    #[test]
    fn nested_namespace_records_its_qualified_name() {
        let content = r#"
            declare namespace A {
                namespace B {
                    interface C { x: number; }
                }
            }
        "#;
        let result = parse_ambient_lib(content);

        assert!(result.namespaces.contains("A"), "{:?}", result.namespaces);
        assert!(result.namespaces.contains("A.B"), "{:?}", result.namespaces);
    }

    #[test]
    fn declare_module_is_not_a_namespace() {
        // `declare module "pkg"` names a module, not a namespace, and a Floe
        // type never writes `"pkg".Thing`.
        let content = r#"
            declare module "some-pkg" {
                export interface Thing { x: number; }
            }
        "#;
        let result = parse_ambient_lib(content);

        assert!(
            result.namespaces.is_empty(),
            "expected no namespaces, got {:?}",
            result.namespaces
        );
    }
}

#[cfg(test)]
mod fs_tests {
    //! Loader tests that read real files.
    //!
    //! `mod tests` above drives `parse_ambient_lib` with in-memory source
    //! strings, so it cannot see which directories the loader opens. These
    //! cases build a real `node_modules` tree with `TempDir`, the way
    //! `resolve::tests` does, and they drive `load_ambient_types` end to end.

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A minimal TypeScript lib file that declares one namespace.
    const INTL_LIB: &str = "declare namespace Intl {\n\
         interface DateTimeFormat { format(d: string): string; }\n\
     }\n";

    /// An `@types/node` shaped file: a namespace inside `declare global`.
    const NODE_TYPES: &str = "declare global {\n\
         namespace NodeJS {\n\
             interface Timeout { ref(): Timeout; }\n\
         }\n\
     }\n\
     export {};\n";

    /// A tsconfig that asks for one lib file and nothing else.
    const TSCONFIG_ES2022: &str = r#"{ "compilerOptions": { "lib": ["es2022"] } }"#;

    /// A tsconfig that asks for no lib file, so only `@types` packages load.
    const TSCONFIG_NO_LIB: &str = r#"{ "compilerOptions": { "lib": [] } }"#;

    /// Write every `(relative path, content)` pair into a fresh temp dir.
    fn setup_files(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        let base = dir.path().to_path_buf();
        (dir, base)
    }

    #[test]
    fn workspace_package_loads_the_hoisted_lib_and_types() {
        // npm and pnpm hoist `typescript` and `@types/node` to the workspace
        // root, and leave the package with a small local `node_modules`.
        // `find_project_dir` stops at that local one, so the loader has to
        // walk up to find either.
        let (_dir, base) = setup_files(&[
            ("node_modules/typescript/lib/lib.es2022.d.ts", INTL_LIB),
            ("node_modules/@types/node/index.d.ts", NODE_TYPES),
            ("packages/app/node_modules/.bin/placeholder", ""),
            ("packages/app/tsconfig.json", TSCONFIG_ES2022),
        ]);

        let ambient = load_ambient_types(&base.join("packages/app"))
            .expect("the workspace root holds both a lib dir and an @types package");

        assert!(
            ambient.namespaces.contains("Intl"),
            "expected the hoisted lib file to load: {:?}",
            ambient.namespaces
        );
        assert!(
            ambient.namespaces.contains("NodeJS"),
            "expected the hoisted @types package to load: {:?}",
            ambient.namespaces
        );
    }

    #[test]
    fn a_pinned_typescript_beats_the_workspace_root_one() {
        // The nearest `node_modules` wins, so a package that pins its own
        // `typescript` gets that copy.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace RootOnly { interface A { x: number; } }",
            ),
            (
                "packages/app/node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace PinnedOnly { interface A { x: number; } }",
            ),
            ("packages/app/tsconfig.json", TSCONFIG_ES2022),
        ]);

        let ambient =
            load_ambient_types(&base.join("packages/app")).expect("the pinned lib dir loads");

        assert!(
            ambient.namespaces.contains("PinnedOnly"),
            "expected the pinned copy to win: {:?}",
            ambient.namespaces
        );
        assert!(
            !ambient.namespaces.contains("RootOnly"),
            "expected the workspace root copy to lose: {:?}",
            ambient.namespaces
        );
    }

    #[test]
    fn exported_namespaces_in_a_types_package_are_recorded() {
        // `csstype` and `undici-types` both export their namespaces.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/@types/exportsns/index.d.ts",
                "export namespace Exported { interface Bar { x: number; } }\n\
                 export declare namespace ExportDeclared { interface Baz { c: number; } }\n",
            ),
            ("tsconfig.json", TSCONFIG_NO_LIB),
        ]);

        let ambient = load_ambient_types(&base).expect("the @types package loads");

        assert!(
            ambient.namespaces.contains("Exported"),
            "expected `export namespace` to count: {:?}",
            ambient.namespaces
        );
        assert!(
            ambient.namespaces.contains("ExportDeclared"),
            "expected `export declare namespace` to count: {:?}",
            ambient.namespaces
        );
    }

    #[test]
    fn a_namespace_inside_a_string_named_module_is_recorded() {
        // `@types/node` writes `NodeJS` in this shape in six files. A string
        // named module contributes no name of its own, and the walk must
        // still enter it.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/@types/modulens/index.d.ts",
                "declare module \"somepkg\" {\n\
                     global {\n\
                         namespace OnlyInModuleGlobal { interface Deep { q: number; } }\n\
                     }\n\
                 }\n",
            ),
            ("tsconfig.json", TSCONFIG_NO_LIB),
        ]);

        let ambient = load_ambient_types(&base).expect("the @types package loads");

        assert!(
            ambient.namespaces.contains("OnlyInModuleGlobal"),
            "expected the walk to enter the module: {:?}",
            ambient.namespaces
        );
        assert!(
            !ambient.namespaces.contains("somepkg"),
            "a string named module is not a namespace: {:?}",
            ambient.namespaces
        );
    }
}
