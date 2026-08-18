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
    /// Merge one more file that belongs to the same `node_modules` level.
    ///
    /// A global keeps the first definition, and a type takes the last one.
    /// The two rules disagree, and they have disagreed since before the
    /// loader walked more than one directory. Keep them, so a project with
    /// one `node_modules` loads exactly what it always loaded.
    fn merge(&mut self, other: AmbientDeclarations, seen_globals: &mut HashSet<String>) {
        for (name, ty) in other.globals {
            if seen_globals.insert(name.clone()) {
                self.globals.push((name, ty));
            }
        }
        self.types.extend(other.types);
        self.namespaces.extend(other.namespaces);
    }

    /// Merge a `node_modules` level that sits farther up the tree than every
    /// level merged so far.
    ///
    /// A name that is already present keeps the definition it has, for types
    /// as well as for globals, so the nearest level wins. Node module
    /// resolution works that way, and the walk would otherwise let a package
    /// at the repo root redefine a DOM type for a package that does not even
    /// depend on it.
    fn merge_farther_level(
        &mut self,
        other: AmbientDeclarations,
        seen_globals: &mut HashSet<String>,
    ) {
        for (name, ty) in other.globals {
            if seen_globals.insert(name.clone()) {
                self.globals.push((name, ty));
            }
        }
        for (name, ty) in other.types {
            self.types.entry(name).or_insert(ty);
        }
        self.namespaces.extend(other.namespaces);
    }
}

/// What one `node_modules` level contributes to the load.
///
/// Files inside a level merge with `merge`. Levels merge with
/// `merge_farther_level`, nearest first.
#[derive(Default)]
struct LevelDeclarations {
    decls: AmbientDeclarations,
    seen_globals: HashSet<String>,
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
///
/// `load_ambient_types` calls this once and passes the list down. Each call
/// stats every ancestor, and the list is the same for every lookup in one
/// load.
fn node_modules_dirs(start: &Path) -> Vec<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join("node_modules"))
        .filter(|dir| dir.is_dir())
        .collect()
}

// ── TS lib file loading ─────────────────────────────────────────

/// Find the TypeScript lib directory, and the level it came from.
///
/// The nearest `node_modules` that holds `typescript` wins, so a package that
/// pins its own copy gets that copy and not the workspace root copy.
fn find_ts_lib_dir(node_modules: &[PathBuf]) -> Option<(usize, PathBuf)> {
    node_modules
        .iter()
        .enumerate()
        .find_map(|(level, dir)| ts_lib_dir_in(dir).map(|lib_dir| (level, lib_dir)))
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
        // Sort by parsed version, not by name. A lexicographic sort puts
        // `typescript@5.10.0` under `typescript@5.9.2` and hands back the
        // older one.
        let mut ts_dirs: Vec<(PnpmVersion, PathBuf)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_str()?.to_string();
                let version = name.strip_prefix("typescript@")?;

                Some((
                    parse_pnpm_version(version),
                    e.path().join("node_modules/typescript/lib"),
                ))
            })
            .filter(|(_, path)| path.is_dir())
            .collect();
        ts_dirs.sort();
        if let Some((_, dir)) = ts_dirs.pop() {
            return Some(dir);
        }
    }

    None
}

/// A pnpm store version, as the three numbers a sort can compare.
type PnpmVersion = (u32, u32, u32);

/// Parse the version out of a pnpm store directory name.
///
/// pnpm writes `typescript@5.9.2`, and it appends peer suffixes such as
/// `5.9.2_@types+node@26.2.0`. This reads the leading `major.minor.patch`
/// and stops at the first character that is not a digit or a dot, so a
/// prerelease or a peer suffix drops off.
fn parse_pnpm_version(version: &str) -> PnpmVersion {
    let numeric = version
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or("");
    let mut parts = numeric.split('.').map(|p| p.parse::<u32>().unwrap_or(0));

    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
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
/// walk yields the nearest level first, so the first name seen is the
/// nearest one, and a later duplicate is dropped.
fn discover_types_packages(node_modules: &[PathBuf]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();

    for dir in node_modules {
        for name in types_packages_in(dir) {
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

/// Find the entry .d.ts for a types package, and the level it came from.
///
/// `@types/{name}` wins over a plain `{name}` at every level, not only
/// inside one level. tsc resolves it that way, and a nearer runtime package
/// that ships no useful ambient types must not shadow a farther `@types`
/// package that does. So this runs two full walks: `@types/{name}` over
/// every level first, then `{name}` over every level.
///
/// The plain `{name}` walk serves packages that ship their own types, such
/// as `@cloudflare/workers-types`.
///
/// This is also the rule that keeps discovery and resolution in step.
/// `discover_types_packages` reports a name because some level holds
/// `@types/{name}`, and the first walk here finds that same directory.
fn find_types_entry(node_modules: &[PathBuf], package_name: &str) -> Option<(usize, PathBuf)> {
    let at_types = node_modules.iter().enumerate().find_map(|(level, dir)| {
        types_entry_at(&dir.join("@types").join(package_name)).map(|entry| (level, entry))
    });
    if at_types.is_some() {
        return at_types;
    }

    node_modules.iter().enumerate().find_map(|(level, dir)| {
        types_entry_at(&dir.join(package_name)).map(|entry| (level, entry))
    })
}

/// Find the entry .d.ts inside one package directory.
fn types_entry_at(types_dir: &Path) -> Option<PathBuf> {
    if !types_dir.is_dir() {
        return None;
    }

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
    let node_modules = node_modules_dirs(project_dir);

    // One bucket per `node_modules` level. Everything a level holds merges
    // into its own bucket first, so a level loads exactly the way the whole
    // load did when the loader read one directory.
    let mut levels: Vec<LevelDeclarations> = (0..node_modules.len())
        .map(|_| LevelDeclarations::default())
        .collect();
    let mut loaded_any = false;

    // Load TS lib files based on compilerOptions.lib
    if let Some((level, lib_dir)) = find_ts_lib_dir(&node_modules) {
        let bucket = &mut levels[level];
        let mut visited_libs: HashSet<String> = HashSet::new();
        for filename in &config.lib_files {
            load_lib_file(
                &lib_dir,
                filename,
                &mut visited_libs,
                &mut bucket.decls,
                &mut bucket.seen_globals,
            );
        }
        loaded_any = !visited_libs.is_empty();
    }

    // Load @types packages
    let types_to_load = match config.types {
        Some(explicit) => explicit,
        None => discover_types_packages(&node_modules),
    };
    for package_name in &types_to_load {
        if let Some((level, entry)) = find_types_entry(&node_modules, package_name) {
            let bucket = &mut levels[level];
            load_types_package(&entry, &mut bucket.decls, &mut bucket.seen_globals);
            loaded_any = true;
        }
    }

    if !loaded_any {
        return None;
    }

    // Fold the levels nearest first. The nearest level that declares a name
    // keeps it.
    let mut merged = AmbientDeclarations::default();
    let mut seen_globals: HashSet<String> = HashSet::new();
    for level in levels {
        merged.merge_farther_level(level.decls, &mut seen_globals);
    }

    Some(merged)
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
    //!
    //! A fixture cannot be sealed off from the machine it runs on. `TempDir`
    //! honours `$TMPDIR`, and the walk climbs from there to the filesystem
    //! root, so a fixture inherits every `@types` package installed above
    //! `$TMPDIR`. Planting one `@types` package above `$TMPDIR` broke three
    //! of these cases, and a stray ancestor `@types/node` could satisfy a
    //! positive assertion on its own.
    //!
    //! Two rules make a case read only its own fixture:
    //!
    //! 1. **Assert against a control load.** `namespaces_added_by` loads the
    //!    fixture, loads an empty project in a second temp dir, and returns
    //!    the difference. Both temp dirs sit under `$TMPDIR`, so both see the
    //!    same ancestors, and the ancestors cancel out.
    //! 2. **Give every fixture name a `Floe` prefix**, so a failure names the
    //!    fixture rather than an installed package.
    //!
    //! Most cases also pin `compilerOptions.types`, which switches
    //! auto-discovery off and reads none of the ancestors at all.

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A minimal TypeScript lib file that declares one namespace.
    const INTL_LIB: &str = "declare namespace FloeIntl {\n\
         interface FloeDateTimeFormat { format(d: string): string; }\n\
     }\n";

    /// An `@types/node` shaped file: a namespace inside `declare global`.
    const NODE_TYPES: &str = "declare global {\n\
         namespace FloeNodeJS {\n\
             interface FloeTimeout { ref(): FloeTimeout; }\n\
         }\n\
     }\n\
     export {};\n";

    /// A tsconfig that asks for one lib file and discovers `@types` itself.
    const TSCONFIG_ES2022: &str = r#"{ "compilerOptions": { "lib": ["es2022"] } }"#;

    /// Build a tsconfig that asks for no lib file and names its `@types`.
    fn tsconfig_types(names: &[&str]) -> String {
        let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();

        format!(
            r#"{{ "compilerOptions": {{ "lib": [], "types": [{}] }} }}"#,
            quoted.join(", ")
        )
    }

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

    /// Every namespace the fixture at `base/project` adds, and nothing the
    /// machine contributed on its own.
    ///
    /// `namespaces` is a plain union with no precedence, so an `@types`
    /// package above `$TMPDIR` lands in it whole. The control load runs the
    /// same tsconfig against an empty project in a second temp dir, which
    /// has the same ancestors and none of the fixture, so subtracting it
    /// leaves exactly what the fixture produced.
    fn namespaces_added_by(base: &Path, project: &str, tsconfig: &str) -> HashSet<String> {
        let control_config = format!("{project}/tsconfig.json");
        let (_control_dir, control_base) = setup_files(&[(&control_config, tsconfig)]);
        let control = load_ambient_types(&control_base.join(project))
            .map(|ambient| ambient.namespaces)
            .unwrap_or_default();

        let fixture = load_ambient_types(&base.join(project))
            .expect("the fixture holds a lib dir or an @types package")
            .namespaces;

        fixture.difference(&control).cloned().collect()
    }

    /// The field names of an ambient record type, so a case can tell two
    /// declarations of one name apart.
    fn field_names(ty: &Type) -> Vec<String> {
        match ty {
            Type::Record(fields) => fields.iter().map(|(name, _)| name.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// The type recorded for one ambient global.
    fn global_type(ambient: &AmbientDeclarations, name: &str) -> Option<Type> {
        ambient
            .globals
            .iter()
            .find(|(global, _)| global == name)
            .map(|(_, ty)| ty.clone())
    }

    #[test]
    fn workspace_package_loads_the_hoisted_lib_and_types() {
        // npm and pnpm hoist `typescript` and the `@types` packages to the
        // workspace root, and leave the package with a small local
        // `node_modules`. `find_project_dir` stops at that local one, so the
        // loader has to walk up to find either.
        let (_dir, base) = setup_files(&[
            ("node_modules/typescript/lib/lib.es2022.d.ts", INTL_LIB),
            ("node_modules/@types/floe-node/index.d.ts", NODE_TYPES),
            ("packages/app/node_modules/.bin/placeholder", ""),
            ("packages/app/tsconfig.json", TSCONFIG_ES2022),
        ]);

        let added = namespaces_added_by(&base, "packages/app", TSCONFIG_ES2022);

        assert!(
            added.contains("FloeIntl"),
            "expected the hoisted lib file to load: {added:?}"
        );
        assert!(
            added.contains("FloeNodeJS"),
            "expected the hoisted @types package to load: {added:?}"
        );
    }

    #[test]
    fn nearest_typescript_lib_dir_wins_over_a_farther_one() {
        // A precedence case, not a walk case: it passes without the walk
        // too, because the nearest lib dir is the one the old loader read.
        // It guards the order the walk yields.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace FloeRootOnly { interface A { x: number; } }",
            ),
            (
                "packages/app/node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace FloePinnedOnly { interface A { x: number; } }",
            ),
            ("packages/app/tsconfig.json", TSCONFIG_ES2022),
        ]);

        let added = namespaces_added_by(&base, "packages/app", TSCONFIG_ES2022);

        assert!(
            added.contains("FloePinnedOnly"),
            "expected the pinned copy to win: {added:?}"
        );
        assert!(
            !added.contains("FloeRootOnly"),
            "expected the workspace root copy to lose: {added:?}"
        );
    }

    #[test]
    fn exported_namespaces_in_a_types_package_are_recorded() {
        // `csstype` and `undici-types` both export their namespaces.
        let tsconfig = tsconfig_types(&["floe-exports"]);
        let (_dir, base) = setup_files(&[
            (
                "node_modules/@types/floe-exports/index.d.ts",
                "export namespace FloeExported { interface Bar { x: number; } }\n\
                 export declare namespace FloeExportDeclared { interface Baz { c: number; } }\n",
            ),
            ("tsconfig.json", &tsconfig),
        ]);

        let added = namespaces_added_by(&base, ".", &tsconfig);

        assert!(
            added.contains("FloeExported"),
            "expected `export namespace` to count: {added:?}"
        );
        assert!(
            added.contains("FloeExportDeclared"),
            "expected `export declare namespace` to count: {added:?}"
        );
    }

    #[test]
    fn a_namespace_inside_a_string_named_module_is_recorded() {
        // `@types/node` writes `NodeJS` in this shape in six files. A string
        // named module contributes no name of its own, and the walk must
        // still enter it.
        let tsconfig = tsconfig_types(&["floe-module"]);
        let (_dir, base) = setup_files(&[
            (
                "node_modules/@types/floe-module/index.d.ts",
                "declare module \"floe-somepkg\" {\n\
                     global {\n\
                         namespace FloeOnlyInModuleGlobal { interface Deep { q: number; } }\n\
                     }\n\
                 }\n",
            ),
            ("tsconfig.json", &tsconfig),
        ]);

        let added = namespaces_added_by(&base, ".", &tsconfig);

        assert!(
            added.contains("FloeOnlyInModuleGlobal"),
            "expected the walk to enter the module: {added:?}"
        );
        assert!(
            !added.contains("floe-somepkg"),
            "a string named module is not a namespace: {added:?}"
        );
    }

    #[test]
    fn a_farther_level_does_not_redefine_a_nearer_type() {
        // The walk made a cross level collision possible for the first time.
        // A type has to follow the same rule a global always followed: the
        // nearest `node_modules` wins. Otherwise a package at the repo root
        // redefines a DOM type for a package that does not depend on it.
        //
        // The far package is named first in `types`, so this proves the
        // level decides and not the load order.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/@types/floe-far/index.d.ts",
                "interface FloeCollide { farField: number; }\n\
                 declare var floeCollideGlobal: FloeFarMarker;\n",
            ),
            (
                "packages/app/node_modules/@types/floe-near/index.d.ts",
                "interface FloeCollide { nearField: number; }\n\
                 declare var floeCollideGlobal: FloeNearMarker;\n",
            ),
            (
                "packages/app/tsconfig.json",
                &tsconfig_types(&["floe-far", "floe-near"]),
            ),
        ]);

        let ambient =
            load_ambient_types(&base.join("packages/app")).expect("both packages resolve");

        let collide = ambient
            .types
            .get("FloeCollide")
            .expect("both packages declare `FloeCollide`");
        let fields = field_names(collide);
        assert!(
            fields.iter().any(|f| f == "nearField"),
            "expected the nearest level to define `FloeCollide`, got {fields:?}"
        );
        assert!(
            !fields.iter().any(|f| f == "farField"),
            "expected the farther level to lose, got {fields:?}"
        );

        let global = global_type(&ambient, "floeCollideGlobal")
            .expect("both packages declare `floeCollideGlobal`");
        assert!(
            format!("{global:?}").contains("FloeNearMarker"),
            "expected the nearest level to define the global, got {global:?}"
        );
    }

    #[test]
    fn at_types_wins_over_a_nearer_plain_package() {
        // `@types/{name}` beats a plain `{name}` at every level, not only
        // inside one level. tsc resolves it that way.
        let files: [(&str, &str); 3] = [
            (
                "node_modules/@types/floe-two/index.d.ts",
                "declare namespace FloeFromAtTypes { interface A { x: number; } }",
            ),
            (
                "packages/app/node_modules/floe-two/index.d.ts",
                "declare namespace FloeFromRuntimePkg { interface A { x: number; } }",
            ),
            ("packages/app/tsconfig.json", ""),
        ];

        // Once with an explicit `types` list, once with auto-discovery. The
        // discovery site reports the name because the root holds
        // `@types/floe-two`, so the resolution site has to land there too.
        for tsconfig in [tsconfig_types(&["floe-two"]), TSCONFIG_ES2022.to_string()] {
            let mut with_config = files;
            with_config[2].1 = &tsconfig;
            let (_dir, base) = setup_files(&with_config);

            let added = namespaces_added_by(&base, "packages/app", &tsconfig);

            assert!(
                added.contains("FloeFromAtTypes"),
                "expected the farther @types package to win with {tsconfig}: {added:?}"
            );
            assert!(
                !added.contains("FloeFromRuntimePkg"),
                "expected the nearer plain package to lose with {tsconfig}: {added:?}"
            );
        }
    }

    #[test]
    fn pnpm_store_picks_the_highest_typescript_version() {
        // A lexicographic sort puts `typescript@5.10.0` under
        // `typescript@5.9.2`. The walk makes a store with several versions
        // reachable that was not reachable before.
        let (_dir, base) = setup_files(&[
            (
                "node_modules/.pnpm/typescript@5.9.2/node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace FloeOldVersion { interface A { x: number; } }",
            ),
            (
                "node_modules/.pnpm/typescript@5.10.0/node_modules/typescript/lib/lib.es2022.d.ts",
                "declare namespace FloeNewVersion { interface A { x: number; } }",
            ),
            ("tsconfig.json", TSCONFIG_ES2022),
        ]);

        let added = namespaces_added_by(&base, ".", TSCONFIG_ES2022);

        assert!(
            added.contains("FloeNewVersion"),
            "expected 5.10.0 to win over 5.9.2: {added:?}"
        );
        assert!(
            !added.contains("FloeOldVersion"),
            "expected 5.9.2 to lose: {added:?}"
        );
    }

    #[test]
    fn pnpm_version_parses_past_a_peer_suffix() {
        assert_eq!(parse_pnpm_version("5.10.0"), (5, 10, 0));
        assert_eq!(parse_pnpm_version("5.9.2"), (5, 9, 2));
        assert!(parse_pnpm_version("5.10.0") > parse_pnpm_version("5.9.2"));
        assert_eq!(parse_pnpm_version("5.9.2_@types+node@26.2.0"), (5, 9, 2));
        assert_eq!(parse_pnpm_version("6.0.0-beta.1"), (6, 0, 0));
        assert_eq!(parse_pnpm_version("nonsense"), (0, 0, 0));
    }
}
