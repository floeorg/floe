//! Typeof resolution — resolves `typeof X` types in probe output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::parser::ast::*;

use super::{DtsExport, TsType};

/// Resolve `typeof X` types in the specifier map by looking up X's actual type
/// in the original package .d.ts files (following `export *` re-exports) or
/// in the source .ts/.tsx files for relative imports.
///
/// When tsgo probes re-export an imported name (`export const _r0 = getYear;`),
/// TypeScript infers the type as `typeof getYear` rather than expanding the
/// function signature. This function resolves those references by parsing the
/// source files directly.
pub(super) fn resolve_typeof_types(
    result: &mut HashMap<String, Vec<DtsExport>>,
    project_dir: &Path,
    program: &Program,
) {
    // Build a map of import name -> module source
    let mut import_sources: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            for spec in &decl.specifiers {
                let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                import_sources.insert(effective_name.to_string(), decl.source.clone());
            }
        }
    }

    // Collect all (specifier, export_name, typeof_name) tuples that need resolution
    let to_resolve: Vec<(String, String, String)> = result
        .iter()
        .flat_map(|(specifier, exports)| {
            exports.iter().filter_map(|e| {
                if let TsType::Named(ref s) = e.ts_type
                    && let Some(ref_name) = s.strip_prefix("typeof ")
                {
                    return Some((specifier.clone(), e.name.clone(), ref_name.to_string()));
                }
                None
            })
        })
        .collect();

    if to_resolve.is_empty() {
        return;
    }

    // Cache parsed exports to avoid re-parsing the same module
    let mut module_cache: HashMap<String, Vec<DtsExport>> = HashMap::new();

    for (specifier, export_name, typeof_name) in to_resolve {
        let module_source = import_sources
            .get(&typeof_name)
            .unwrap_or(&specifier)
            .clone();

        let module_exports = module_cache
            .entry(module_source.clone())
            .or_insert_with(|| {
                // Try npm package .d.ts (follows `export *` re-exports)
                if let Some(dts_path) = find_package_dts(project_dir, &module_source)
                    && let Ok(exports) = super::super::dts::parse_dts_exports(&dts_path)
                {
                    return exports;
                }
                Vec::new()
            });

        // Look for the typeof name in the module exports
        if let Some(found) = module_exports.iter().find(|e| e.name == typeof_name)
            && let Some(exports) = result.get_mut(&specifier)
            && let Some(entry) = exports.iter_mut().find(|e| e.name == export_name)
        {
            entry.ts_type = found.ts_type.clone();
        }
    }

    // Parse npm packages for type-only imports (Any/Foreign) so the checker
    // can resolve their fields (e.g. DropResult.droppableId from @hello-pangea/dnd).
    // Also parse packages that have Named type references in the probe output.
    for (name, source) in &import_sources {
        // Only parse npm packages (not relative imports — those are handled separately)
        if source.starts_with("./") || source.starts_with("../") {
            continue;
        }
        // Check if this import has an Any export (type-only) that needs resolution
        let needs_parsing = result.get(source.as_str()).is_some_and(|exports| {
            exports
                .iter()
                .any(|e| e.name == *name && matches!(e.ts_type, TsType::Any))
        });
        if needs_parsing {
            module_cache.entry(source.clone()).or_insert_with(|| {
                if let Some(dts_path) = find_package_dts(project_dir, source)
                    && let Ok(exports) = super::super::dts::parse_dts_exports(&dts_path)
                {
                    return exports;
                }
                Vec::new()
            });
        }
    }

    // Register type/interface definitions from ALL parsed npm packages so the
    // checker can resolve Foreign type member access (e.g. DropResult.droppableId).
    for (module_source, module_exports) in &module_cache {
        let specifier = import_sources
            .iter()
            .find(|(_, src)| *src == module_source)
            .map(|(_, src)| src.clone())
            .unwrap_or_else(|| module_source.clone());
        let entry = result.entry(specifier).or_default();
        for export in module_exports {
            if matches!(export.ts_type, TsType::Object(_))
                && !entry.iter().any(|e| e.name == export.name)
            {
                entry.push(export.clone());
            }
        }
    }
}

/// Find the main .d.ts file for an npm package by reading its package.json.
/// Checks both `node_modules/<pkg>` and `node_modules/@types/<pkg>`.
fn find_package_dts(project_dir: &Path, module_name: &str) -> Option<PathBuf> {
    // Try the package itself first, then @types
    let candidates = [
        project_dir.join("node_modules").join(module_name),
        project_dir.join("node_modules/@types").join(module_name),
    ];

    for pkg_dir in &candidates {
        let pkg_json_path = pkg_dir.join("package.json");

        if let Ok(content) = std::fs::read_to_string(&pkg_json_path)
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        {
            for field in &["types", "typings"] {
                if let Some(types_path) = json[field].as_str() {
                    let full_path = pkg_dir.join(types_path);
                    if full_path.exists() {
                        return Some(full_path);
                    }
                }
            }
        }

        // Fallback: try index.d.ts
        let index_dts = pkg_dir.join("index.d.ts");
        if index_dts.exists() {
            return Some(index_dts);
        }
    }

    None
}
