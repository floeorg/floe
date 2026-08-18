//! Typeof resolution — resolves `typeof X` types in probe output.

use std::collections::HashMap;
use std::path::Path;

use crate::parser::ast::{ItemKind, Program};

use super::super::package_exports::find_package_dts;
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

    // Cache parsed exports to avoid re-parsing the same module. Populated by
    // both the `typeof`-resolution loop below and the Any-fallback pass further
    // down (which runs even when there are no typeof references to resolve).
    let mut module_cache: HashMap<String, Vec<DtsExport>> = HashMap::new();

    for (specifier, export_name, typeof_name) in to_resolve {
        let module_source = import_sources
            .get(&typeof_name)
            .unwrap_or(&specifier)
            .clone();

        let module_exports = module_cache
            .entry(module_source.clone())
            .or_insert_with(|| load_dts_exports_for(project_dir, &module_source));

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
            module_cache
                .entry(source.clone())
                .or_insert_with(|| load_dts_exports_for(project_dir, source));
        }
    }

    // Register type definitions from ALL parsed npm packages so the checker
    // can resolve Foreign member access (e.g. DropResult.droppableId) and
    // function-typed aliases like hono's `export type Next = () => Promise<void>`.
    // tsgo's probe emits `any` for type-only imports because they can't appear
    // as values; the dts fallback here supplies the real structural shape.
    for (module_source, module_exports) in &module_cache {
        let specifier = import_sources
            .iter()
            .find(|(_, src)| *src == module_source)
            .map(|(_, src)| src.clone())
            .unwrap_or_else(|| module_source.clone());
        let entry = result.entry(specifier).or_default();
        for export in module_exports {
            if is_resolvable_type(&export.ts_type) {
                // Replace Any entries with richer definitions
                if let Some(existing) = entry.iter_mut().find(|e| e.name == export.name) {
                    if matches!(existing.ts_type, TsType::Any) {
                        existing.ts_type = export.ts_type.clone();
                    }
                } else {
                    entry.push(export.clone());
                }
            }
        }
    }
}

/// Types rich enough to be worth registering as replacements for a tsgo-erased
/// `Any`. Excludes `Any`/`Unknown`/bare `Named`/`This` — those add no
/// information over what the probe already produced.
fn is_resolvable_type(ty: &TsType) -> bool {
    match ty {
        TsType::Object(_)
        | TsType::Function { .. }
        | TsType::Generic { .. }
        | TsType::Array(_)
        | TsType::Tuple(_)
        | TsType::Union(_)
        | TsType::Primitive(_)
        | TsType::StringLiteral(_)
        | TsType::NumberLiteral(_)
        | TsType::BooleanLiteral(_)
        | TsType::Null
        | TsType::Undefined
        | TsType::IndexedAccess { .. } => true,
        TsType::Any | TsType::Unknown | TsType::Named(_) | TsType::This => false,
    }
}

/// Load exports for an import specifier from its backing .d.ts file.
/// `node:X` specifiers read from the `declare module "node:X"` block;
/// all others read top-level exports. Returns an empty vec if the package
/// can't be located or the parser fails.
fn load_dts_exports_for(project_dir: &Path, specifier: &str) -> Vec<DtsExport> {
    let Some(dts_path) = find_package_dts(project_dir, specifier) else {
        return Vec::new();
    };
    let parsed = if specifier.starts_with("node:") {
        super::super::dts::parse_dts_exports_for_specifier(&dts_path, specifier)
    } else {
        super::super::dts::parse_dts_exports(&dts_path)
    };
    parsed.unwrap_or_default()
}
