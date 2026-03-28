use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::{self as floe_diag};
use crate::interop;
use crate::parser::ast::*;
use crate::resolve::TsconfigPaths;

use super::symbols::SymbolIndex;

/// Resolve an npm package specifier to its .d.ts file path.
/// Walks node_modules looking for package.json types/typings field.
pub(super) fn resolve_npm_dts(specifier: &str, project_dir: &Path) -> Option<PathBuf> {
    // Walk up directories looking for node_modules
    let mut dir = project_dir.to_path_buf();
    loop {
        let pkg_dir = dir.join("node_modules").join(specifier);
        if pkg_dir.is_dir() {
            // Check package.json for types/typings field
            let pkg_json = pkg_dir.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_json)
                && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
            {
                // Try "types", then "typings"
                for field in &["types", "typings"] {
                    if let Some(types_path) = json.get(field).and_then(|v| v.as_str()) {
                        let full = pkg_dir.join(types_path);
                        if full.exists() {
                            return Some(full);
                        }
                    }
                }
            }
            // Fallback: index.d.ts
            let index_dts = pkg_dir.join("index.d.ts");
            if index_dts.exists() {
                return Some(index_dts);
            }
        }

        // Also check @types/<pkg>
        let at_types = dir.join("node_modules").join("@types").join(specifier);
        if at_types.is_dir() {
            let index_dts = at_types.join("index.d.ts");
            if index_dts.exists() {
                return Some(index_dts);
            }
        }

        if !dir.pop() {
            break;
        }
    }
    None
}

/// Resolve a relative import to an actual file path.
/// Checks .fl, .ts, .tsx extensions and /index variants.
pub(super) fn resolve_relative_import(specifier: &str, source_dir: &Path) -> Option<PathBuf> {
    let base = source_dir.join(specifier);
    for ext in &[".fl", ".ts", ".tsx", "/index.fl", "/index.ts", "/index.tsx"] {
        let path = PathBuf::from(format!("{}{}", base.display(), ext));
        if path.exists() {
            return Some(path);
        }
    }
    // Maybe it already has an extension
    if base.exists() && base.is_file() {
        return Some(base);
    }
    None
}

/// Enrich a symbol index with type info from resolved .d.ts files.
/// Also returns diagnostics for unresolvable relative imports.
pub(super) fn enrich_from_imports(
    program: &Program,
    project_dir: &Path,
    source_dir: &Path,
    index: &mut SymbolIndex,
    dts_cache: &HashMap<String, Vec<interop::DtsExport>>,
    tsconfig_paths: &TsconfigPaths,
) -> (
    Vec<floe_diag::Diagnostic>,
    HashMap<String, Vec<interop::DtsExport>>,
) {
    let mut import_diags = Vec::new();
    let mut new_cache = HashMap::new();

    for item in &program.items {
        let ItemKind::Import(decl) = &item.kind else {
            continue;
        };

        let specifier = &decl.source;
        let is_relative = specifier.starts_with("./") || specifier.starts_with("../");

        if is_relative {
            // Validate relative imports exist
            if resolve_relative_import(specifier, source_dir).is_none() {
                import_diags.push(
                    floe_diag::Diagnostic::error(
                        format!("cannot find module `\"{specifier}\"`"),
                        item.span,
                    )
                    .with_label("module not found")
                    .with_help("check the file path and extension")
                    .with_code("E012"),
                );
            }
            continue;
        }

        // Check if this is a tsconfig path alias (e.g. "#/utils")
        if tsconfig_paths.matches(specifier) {
            if tsconfig_paths.resolve(specifier).is_none() {
                import_diags.push(
                    floe_diag::Diagnostic::error(
                        format!("cannot find module `\"{specifier}\"`"),
                        item.span,
                    )
                    .with_label("path alias resolved but file not found")
                    .with_help("check the file path matches a tsconfig paths alias")
                    .with_code("E012"),
                );
            }
            // Path aliases are resolved as local files, not npm packages
            continue;
        }

        // npm package — try to resolve .d.ts
        let exports = if let Some(cached) = dts_cache.get(specifier) {
            cached.clone()
        } else if let Some(dts_path) = resolve_npm_dts(specifier, project_dir) {
            match interop::parse_dts_exports(&dts_path) {
                Ok(exports) => exports,
                Err(_) => continue,
            }
        } else {
            import_diags.push(
                floe_diag::Diagnostic::error(
                    format!("cannot find module `\"{specifier}\"`"),
                    item.span,
                )
                .with_label("package not found")
                .with_help("check that the package is installed (`npm install`)")
                .with_code("E013"),
            );
            continue;
        };

        new_cache.insert(specifier.clone(), exports.clone());

        // Enrich imported symbols with type info from .d.ts
        for sym in &mut index.symbols {
            if sym.import_source.as_deref() != Some(specifier) {
                continue;
            }
            // Find matching export
            if let Some(dts_export) = exports.iter().find(|e| e.name == sym.name) {
                let type_str = interop::ts_type_to_string(&dts_export.ts_type);
                sym.detail = format!("{} (from \"{}\")", type_str, specifier);

                // If it's a function export, extract first param and return type
                if let interop::TsType::Function {
                    params,
                    return_type,
                } = &dts_export.ts_type
                {
                    sym.kind = tower_lsp::lsp_types::SymbolKind::FUNCTION;
                    sym.first_param_type = params.first().map(interop::ts_type_to_string);
                    sym.return_type_str = Some(interop::ts_type_to_string(return_type));
                }
            }
        }
    }

    (import_diags, new_cache)
}
