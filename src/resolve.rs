//! Module resolver for cross-file import resolution.
//!
//! Resolves relative `.fl` imports by parsing the imported file and extracting
//! exported symbols (types, functions, for-block functions, consts).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::parser::Parser;
use crate::parser::ast::*;

/// Parsed tsconfig.json path aliases.
/// Maps alias prefixes (e.g. "#/") to their target directories.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Alias prefix -> resolved base directories (e.g. "#/*" -> ["/abs/path/src"])
    pub mappings: Vec<(String, Vec<PathBuf>)>,
}

impl TsconfigPaths {
    /// Parse path aliases from the nearest tsconfig.json.
    pub fn from_project_dir(project_dir: &Path) -> Self {
        let tsconfig_path = match find_tsconfig(project_dir) {
            Some(p) => p,
            None => return Self::default(),
        };

        let content = match std::fs::read_to_string(&tsconfig_path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Self::default(),
        };

        let tsconfig_dir = tsconfig_path.parent().unwrap_or(project_dir);

        let base_url = json
            .pointer("/compilerOptions/baseUrl")
            .and_then(|v| v.as_str())
            .map(|b| tsconfig_dir.join(b))
            .unwrap_or_else(|| tsconfig_dir.to_path_buf());

        let paths = match json.pointer("/compilerOptions/paths") {
            Some(serde_json::Value::Object(map)) => map,
            _ => return Self::default(),
        };

        let mut mappings = Vec::new();
        for (pattern, targets) in paths {
            let targets = match targets.as_array() {
                Some(arr) => arr,
                None => continue,
            };

            // Strip trailing "*" from pattern (e.g. "#/*" -> "#/")
            let prefix = pattern.trim_end_matches('*');

            let resolved_dirs: Vec<PathBuf> = targets
                .iter()
                .filter_map(|t| t.as_str())
                .map(|t| base_url.join(t.trim_end_matches('*')))
                .collect();

            if !resolved_dirs.is_empty() {
                mappings.push((prefix.to_string(), resolved_dirs));
            }
        }

        Self { mappings }
    }

    /// Try to resolve a specifier using tsconfig path aliases.
    /// Returns the resolved file path if found.
    pub fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        for (prefix, dirs) in &self.mappings {
            if let Some(rest) = specifier.strip_prefix(prefix) {
                for dir in dirs {
                    let candidate = dir.join(rest);
                    // Try .fl, .ts, .tsx extensions and /index variants
                    for ext in &[".fl", ".ts", ".tsx", "/index.fl", "/index.ts", "/index.tsx"] {
                        let path = PathBuf::from(format!("{}{}", candidate.display(), ext));
                        if path.exists() {
                            return Some(path);
                        }
                    }
                    if candidate.exists() && candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// Check if a specifier matches any path alias.
    pub fn matches(&self, specifier: &str) -> bool {
        self.mappings
            .iter()
            .any(|(prefix, _)| specifier.starts_with(prefix))
    }
}

/// Find the nearest tsconfig.json by walking up from a directory.
fn find_tsconfig(dir: &Path) -> Option<PathBuf> {
    let mut current = dir.to_path_buf();
    loop {
        let tsconfig = current.join("tsconfig.json");
        if tsconfig.exists() {
            return Some(tsconfig);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Symbols exported from a resolved module.
#[derive(Debug, Clone, Default)]
pub struct ResolvedImports {
    /// Exported type declarations: name -> TypeDecl
    pub type_decls: Vec<TypeDecl>,
    /// Exported function declarations: name -> FunctionDecl
    pub function_decls: Vec<FunctionDecl>,
    /// Exported for-block declarations
    pub for_blocks: Vec<ForBlock>,
    /// Exported const names (we just need to know they exist, typed as Unknown)
    pub const_names: Vec<String>,
    /// Exported trait declarations
    pub trait_decls: Vec<TraitDecl>,
}

/// Resolve all relative imports for a given file.
///
/// Returns a map from import source (e.g., `"./types"`) to its resolved symbols.
/// Non-relative imports (npm packages) are skipped.
/// Circular imports are handled by tracking visited files.
pub fn resolve_imports(file_path: &Path, program: &Program) -> HashMap<String, ResolvedImports> {
    let mut results = HashMap::new();
    let mut visited = HashSet::new();

    // The file's own path should be in the visited set to prevent circular imports
    if let Ok(canonical) = file_path.canonicalize() {
        visited.insert(canonical);
    } else {
        visited.insert(file_path.to_path_buf());
    }

    let base_dir = file_path.parent().unwrap_or(Path::new("."));

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            // Skip npm/non-relative imports
            if !decl.source.starts_with('.') {
                continue;
            }

            // Already resolved this source
            if results.contains_key(&decl.source) {
                continue;
            }

            if let Some(resolved) = resolve_single_import(base_dir, &decl.source, &mut visited) {
                results.insert(decl.source.clone(), resolved);
            }
        }
    }

    results
}

/// Resolve a single import source to its exported symbols.
fn resolve_single_import(
    base_dir: &Path,
    source: &str,
    visited: &mut HashSet<PathBuf>,
) -> Option<ResolvedImports> {
    let resolved_path = resolve_path(base_dir, source)?;

    // Check for circular imports
    let canonical = resolved_path
        .canonicalize()
        .unwrap_or(resolved_path.clone());
    if visited.contains(&canonical) {
        // Circular import — return empty to avoid infinite recursion
        return Some(ResolvedImports::default());
    }
    visited.insert(canonical);

    let source_code = std::fs::read_to_string(&resolved_path).ok()?;
    let program = Parser::new(&source_code).parse_program().ok()?;

    let mut imports = ResolvedImports::default();

    // Also resolve transitive imports from the imported file
    let transitive_dir = resolved_path.parent().unwrap_or(Path::new("."));
    let transitive = resolve_imports_inner(transitive_dir, &program, visited);

    // Collect transitive type decls, for-blocks, and trait decls so the checker can register them
    for resolved in transitive.values() {
        imports
            .type_decls
            .extend(resolved.type_decls.iter().cloned());
        imports
            .for_blocks
            .extend(resolved.for_blocks.iter().cloned());
        imports
            .function_decls
            .extend(resolved.function_decls.iter().cloned());
        imports
            .trait_decls
            .extend(resolved.trait_decls.iter().cloned());
    }

    // Collect ALL type decls (exported and non-exported) to build a type map
    // for resolving spreads within this file's scope.
    let mut all_type_decls: Vec<TypeDecl> = Vec::new();
    for item in &program.items {
        if let ItemKind::TypeDecl(decl) = &item.kind {
            all_type_decls.push(decl.clone());
        }
    }

    // Include transitive types in the type map so spreads referencing
    // re-exported types can also be resolved.
    let mut type_map: HashMap<String, TypeDecl> = HashMap::new();
    for decl in &imports.type_decls {
        type_map.insert(decl.name.clone(), decl.clone());
    }
    for decl in &all_type_decls {
        type_map.insert(decl.name.clone(), decl.clone());
    }

    // Flatten spreads in all type decls so importers get fully resolved records.
    let flattened_map = flatten_spreads_in_type_decls(&type_map);

    for item in &program.items {
        match &item.kind {
            ItemKind::TypeDecl(decl) if decl.exported => {
                // Use the flattened version if available
                if let Some(flattened) = flattened_map.get(&decl.name) {
                    imports.type_decls.push(flattened.clone());
                } else {
                    imports.type_decls.push(decl.clone());
                }
            }
            ItemKind::Function(decl) if decl.exported => {
                imports.function_decls.push(decl.clone());
            }
            ItemKind::ForBlock(block) => {
                // Only include exported for-block functions
                let mut exported_block = block.clone();
                exported_block.functions.retain(|f| f.exported);
                if !exported_block.functions.is_empty() {
                    imports.for_blocks.push(exported_block);
                }
            }
            ItemKind::TraitDecl(decl) if decl.exported => {
                imports.trait_decls.push(decl.clone());
            }
            ItemKind::Const(decl) if decl.exported => {
                if let ConstBinding::Name(name) = &decl.binding {
                    imports.const_names.push(name.clone());
                }
            }
            _ => {}
        }
    }

    Some(imports)
}

/// Internal version that accepts a mutable visited set for transitive resolution.
fn resolve_imports_inner(
    base_dir: &Path,
    program: &Program,
    visited: &mut HashSet<PathBuf>,
) -> HashMap<String, ResolvedImports> {
    let mut results = HashMap::new();

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            if !decl.source.starts_with('.') {
                continue;
            }
            if results.contains_key(&decl.source) {
                continue;
            }
            if let Some(resolved) = resolve_single_import(base_dir, &decl.source, visited) {
                results.insert(decl.source.clone(), resolved);
            }
        }
    }

    results
}

/// Flatten record type spreads in a set of type declarations.
///
/// This resolves `...OtherType` entries in record types by looking up the
/// spread target in the provided type map and inlining its fields. This
/// ensures that exported types have no unresolved spread references, so
/// importers don't need the spread target in their own scope.
fn flatten_spreads_in_type_decls(
    type_map: &HashMap<String, TypeDecl>,
) -> HashMap<String, TypeDecl> {
    let mut result = HashMap::new();

    for (name, decl) in type_map {
        let entries = match &decl.def {
            TypeDef::Record(entries) => entries,
            _ => continue,
        };

        let has_spreads = entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
        if !has_spreads {
            continue;
        }

        let mut flat_fields: Vec<RecordEntry> = Vec::new();

        for entry in entries {
            match entry {
                RecordEntry::Field(_) => {
                    flat_fields.push(entry.clone());
                }
                RecordEntry::Spread(spread) => {
                    // Look up the spread target type in the file's type map
                    if let Some(target_decl) = type_map.get(&spread.type_name)
                        && let TypeDef::Record(target_entries) = &target_decl.def
                    {
                        // Recursively flatten the target if it also has spreads
                        let resolved_entries =
                            resolve_spread_entries(target_entries, type_map, &mut HashSet::new());
                        for target_entry in resolved_entries {
                            if let RecordEntry::Field(_) = &target_entry {
                                flat_fields.push(target_entry);
                            }
                        }
                    }
                    // Unknown spread targets are left out; the checker will report the error
                }
            }
        }

        let mut flattened = decl.clone();
        flattened.def = TypeDef::Record(flat_fields);
        result.insert(name.clone(), flattened);
    }

    result
}

/// Recursively resolve spread entries, handling chains like A spreads B spreads C.
fn resolve_spread_entries(
    entries: &[RecordEntry],
    type_map: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> Vec<RecordEntry> {
    let mut result = Vec::new();

    for entry in entries {
        match entry {
            RecordEntry::Field(_) => {
                result.push(entry.clone());
            }
            RecordEntry::Spread(spread) => {
                // Guard against circular spreads
                if visited.contains(&spread.type_name) {
                    continue;
                }
                visited.insert(spread.type_name.clone());

                if let Some(target_decl) = type_map.get(&spread.type_name)
                    && let TypeDef::Record(target_entries) = &target_decl.def
                {
                    let resolved = resolve_spread_entries(target_entries, type_map, visited);
                    result.extend(resolved);
                }

                visited.remove(&spread.type_name);
            }
        }
    }

    result
}

/// Resolve a relative import path to a TypeScript file.
/// Tries `source.ts`, `source.tsx`, `source/index.ts`, `source/index.tsx`.
/// Used when the import doesn't resolve to a `.fl` file.
pub fn resolve_ts_path(base_dir: &Path, source: &str) -> Option<PathBuf> {
    let relative = PathBuf::from(source);
    let base = base_dir.join(&relative);

    for ext in &["ts", "tsx"] {
        let candidate = base.with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Try source/index.ts, source/index.tsx
    for ext in &["ts", "tsx"] {
        let candidate = base.join("index").with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Resolve a relative import path to an actual file path.
/// Tries `source.fl` first, then `source/index.fl`.
fn resolve_path(base_dir: &Path, source: &str) -> Option<PathBuf> {
    let relative = PathBuf::from(source);
    let candidate = base_dir.join(&relative).with_extension("fl");
    if candidate.is_file() {
        return Some(candidate);
    }

    // Try source/index.fl
    let index_candidate = base_dir.join(&relative).join("index.fl");
    if index_candidate.is_file() {
        return Some(index_candidate);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a temp dir, write files, and return (TempDir, base_path).
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

    /// Helper: parse a source string into a Program.
    fn parse_program(source: &str) -> Program {
        Parser::new(source).parse_program().unwrap()
    }

    // ── Path resolution ───────────────────────────────────────────

    #[test]
    fn resolve_path_fl_suffix() {
        let (_dir, base) = setup_files(&[("types.fl", "const x = 1")]);
        let result = resolve_path(&base, "./types");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("types.fl"));
    }

    #[test]
    fn resolve_path_index_fallback() {
        let (_dir, base) = setup_files(&[("utils/index.fl", "const y = 2")]);
        let result = resolve_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("index.fl"));
    }

    #[test]
    fn resolve_path_prefer_fl_over_index() {
        let (_dir, base) =
            setup_files(&[("mod.fl", "const a = 1"), ("mod/index.fl", "const b = 2")]);
        let result = resolve_path(&base, "./mod");
        assert!(result.is_some());
        // Should prefer mod.fl
        assert!(result.unwrap().ends_with("mod.fl"));
    }

    #[test]
    fn resolve_path_missing_file() {
        let (_dir, base) = setup_files(&[]);
        let result = resolve_path(&base, "./nonexistent");
        assert!(result.is_none());
    }

    // ── Empty / no-import programs ────────────────────────────────

    #[test]
    fn empty_program_no_imports() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("");
        let result = resolve_imports(&main_path, &program);
        assert!(result.is_empty());
    }

    #[test]
    fn program_without_imports() {
        let (_dir, base) = setup_files(&[("main.fl", "const x = 1")]);
        let main_path = base.join("main.fl");
        let program = parse_program("const x = 1");
        let result = resolve_imports(&main_path, &program);
        assert!(result.is_empty());
    }

    // ── npm imports skipped ───────────────────────────────────────

    #[test]
    fn npm_imports_skipped() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { useState } from \"react\"");
        let result = resolve_imports(&main_path, &program);
        assert!(result.is_empty());
    }

    // ── Exported types, functions, consts extracted ────────────────

    #[test]
    fn exported_type_extracted() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            ("types.fl", "export type User = { name: string }"),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { User } from \"./types\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./types").unwrap();
        assert_eq!(resolved.type_decls.len(), 1);
        assert_eq!(resolved.type_decls[0].name, "User");
    }

    #[test]
    fn exported_function_extracted() {
        let (_dir, base) =
            setup_files(&[("main.fl", ""), ("helpers.fl", "export fn greet() { 1 }")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { greet } from \"./helpers\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./helpers").unwrap();
        assert_eq!(resolved.function_decls.len(), 1);
        assert_eq!(resolved.function_decls[0].name, "greet");
    }

    #[test]
    fn exported_const_extracted() {
        let (_dir, base) = setup_files(&[("main.fl", ""), ("config.fl", "export const MAX = 100")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { MAX } from \"./config\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./config").unwrap();
        assert_eq!(resolved.const_names, vec!["MAX".to_string()]);
    }

    // ── Non-exported items excluded ───────────────────────────────

    #[test]
    fn non_exported_items_excluded() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "internal.fl",
                "type Secret = { key: string }\nfn helper() { 1 }\nconst private = 42",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Secret } from \"./internal\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./internal").unwrap();
        assert!(resolved.type_decls.is_empty());
        assert!(resolved.function_decls.is_empty());
        assert!(resolved.const_names.is_empty());
    }

    // ── Multiple imports from same module ─────────────────────────

    #[test]
    fn multiple_imports_same_module() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "lib.fl",
                "export type A = { x: number }\nexport fn b() { 1 }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { A } from \"./lib\"\nimport { b } from \"./lib\"");
        let result = resolve_imports(&main_path, &program);
        // Should only resolve once
        assert_eq!(result.len(), 1);
        let resolved = result.get("./lib").unwrap();
        assert!(!resolved.type_decls.is_empty());
        assert!(!resolved.function_decls.is_empty());
    }

    // ── For-block exports ─────────────────────────────────────────

    #[test]
    fn for_block_exported_functions() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "ext.fl",
                "for User { export fn greet(self) -> string { self.name } }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { for User } from \"./ext\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./ext").unwrap();
        // The exported for-block function should be present
        assert!(!resolved.for_blocks.is_empty());
        assert_eq!(resolved.for_blocks[0].functions.len(), 1);
    }

    // ── Missing files handled gracefully ──────────────────────────

    #[test]
    fn missing_import_file_no_panic() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { foo } from \"./missing\"");
        let result = resolve_imports(&main_path, &program);
        // Missing file should not appear in results
        assert!(!result.contains_key("./missing"));
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn dotted_relative_import() {
        let (_dir, base) = setup_files(&[("sub/main.fl", ""), ("lib.fl", "export const X = 1")]);
        let main_path = base.join("sub/main.fl");
        let program = parse_program("import { X } from \"../lib\"");
        let result = resolve_imports(&main_path, &program);
        assert!(result.contains_key("../lib"));
    }

    #[test]
    fn exported_trait_extracted() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "traits.fl",
                "export trait Display { fn show(self) -> string }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Display } from \"./traits\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./traits").unwrap();
        assert_eq!(resolved.trait_decls.len(), 1);
        assert_eq!(resolved.trait_decls[0].name, "Display");
    }

    #[test]
    fn resolve_index_in_subdir() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            ("components/index.fl", "export fn Button() { 1 }"),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Button } from \"./components\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./components").unwrap();
        assert_eq!(resolved.function_decls.len(), 1);
    }

    // ── Spread flattening during import ──────────────────────────

    #[test]
    fn spread_flattened_on_export() {
        // WithRating is not exported, but Product uses ...WithRating.
        // The resolver should flatten the spread so importers get a flat record.
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "types.fl",
                "type WithRating { rating: number }\nexport type Product { ...WithRating, title: string }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Product } from \"./types\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./types").unwrap();

        // Product should be present
        assert_eq!(resolved.type_decls.len(), 1);
        let product = &resolved.type_decls[0];
        assert_eq!(product.name, "Product");

        // The record should have no spreads — they should be flattened to fields
        if let TypeDef::Record(entries) = &product.def {
            for entry in entries {
                assert!(
                    matches!(entry, RecordEntry::Field(_)),
                    "expected all entries to be fields after flattening, but found a spread"
                );
            }
            // Should have both rating and title fields
            let field_names: Vec<&str> = entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| f.name.as_str()))
                .collect();
            assert!(
                field_names.contains(&"rating"),
                "expected flattened field 'rating', got: {:?}",
                field_names
            );
            assert!(
                field_names.contains(&"title"),
                "expected field 'title', got: {:?}",
                field_names
            );
        } else {
            panic!("expected Product to be a Record type");
        }
    }

    #[test]
    fn spread_chain_flattened_on_export() {
        // A spreads into B, B spreads into C — C should be fully flat.
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "types.fl",
                "type A { x: number }\ntype B { ...A, y: string }\nexport type C { ...B, z: boolean }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { C } from \"./types\"");
        let result = resolve_imports(&main_path, &program);
        let resolved = result.get("./types").unwrap();

        let c_decl = &resolved.type_decls[0];
        assert_eq!(c_decl.name, "C");

        if let TypeDef::Record(entries) = &c_decl.def {
            let field_names: Vec<&str> = entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| f.name.as_str()))
                .collect();
            assert_eq!(
                field_names.len(),
                3,
                "expected 3 fields, got: {:?}",
                field_names
            );
            assert!(field_names.contains(&"x"));
            assert!(field_names.contains(&"y"));
            assert!(field_names.contains(&"z"));
        } else {
            panic!("expected C to be a Record type");
        }
    }

    // ── TypeScript path resolution ───────────────────────────────

    #[test]
    fn resolve_ts_path_ts_suffix() {
        let (_dir, base) = setup_files(&[("utils.ts", "export function foo() {}")]);
        let result = resolve_ts_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("utils.ts"));
    }

    #[test]
    fn resolve_ts_path_tsx_suffix() {
        let (_dir, base) = setup_files(&[("Button.tsx", "export function Button() {}")]);
        let result = resolve_ts_path(&base, "./Button");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("Button.tsx"));
    }

    #[test]
    fn resolve_ts_path_prefers_ts_over_tsx() {
        let (_dir, base) = setup_files(&[
            ("mod.ts", "export const a = 1"),
            ("mod.tsx", "export const b = 2"),
        ]);
        let result = resolve_ts_path(&base, "./mod");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("mod.ts"));
    }

    #[test]
    fn resolve_ts_path_index_fallback() {
        let (_dir, base) = setup_files(&[("utils/index.ts", "export const x = 1")]);
        let result = resolve_ts_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("index.ts"));
    }

    #[test]
    fn resolve_ts_path_missing() {
        let (_dir, base) = setup_files(&[]);
        let result = resolve_ts_path(&base, "./nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn resolve_ts_path_not_used_when_fl_exists() {
        // When both .fl and .ts exist, resolve_path should find .fl
        // and resolve_ts_path should find .ts — but the resolver
        // only calls resolve_ts_path when .fl resolution fails.
        let (_dir, base) = setup_files(&[
            ("types.fl", "export type X { y: number }"),
            ("types.ts", "export type X = { y: number }"),
        ]);
        let fl_result = resolve_path(&base, "./types");
        let ts_result = resolve_ts_path(&base, "./types");
        assert!(fl_result.unwrap().ends_with("types.fl"));
        assert!(ts_result.unwrap().ends_with("types.ts"));
    }
}
