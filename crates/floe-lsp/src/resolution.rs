use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use floe_core::checker::ErrorCode;
use floe_core::diagnostic::{self as floe_diag};
use floe_core::interop;
use floe_core::parser::ast::{ItemKind, Program};
use floe_core::resolve::TsconfigPaths;

use super::index::SymbolIndex;

/// Resolve an npm package specifier to its .d.ts file path.
///
/// Walks up from the project directory, because a workspace hoists
/// `node_modules` above the package that imports from it. The per-directory
/// lookup, including `exports` handling and the `node:` scheme, lives in
/// `floe_core::interop`, so the language server and the compiler resolve a
/// specifier the same way.
pub(super) fn resolve_npm_dts(specifier: &str, project_dir: &Path) -> Option<PathBuf> {
    let mut dir = project_dir.to_path_buf();
    loop {
        if let Some(found) = interop::find_package_dts(&dir, specifier) {
            return Some(found);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Resolve a relative import to an actual file path.
/// Checks .fl, .ts, .tsx extensions and /index variants.
pub(super) fn resolve_relative_import(specifier: &str, source_dir: &Path) -> Option<PathBuf> {
    let base = source_dir.join(specifier);
    for ext in &[".fl", ".ts", ".tsx", "/index.fl", "/index.ts", "/index.tsx"] {
        let path = PathBuf::from(format!("{}{}", base.display(), ext));
        if path.exists() {
            return path.canonicalize().ok();
        }
    }
    // Maybe it already has an extension
    if base.exists() && base.is_file() {
        return base.canonicalize().ok();
    }
    None
}

/// Enrich a symbol index with type info from resolved .d.ts files.
/// Also returns diagnostics for unresolvable relative imports.
pub(super) fn enrich_from_imports<T>(
    program: &Program<T>,
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
                    .with_error_code(ErrorCode::ModuleNotFound),
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
                    .with_error_code(ErrorCode::ModuleNotFound),
                );
            }
            // Path aliases are resolved as local files, not npm packages
            continue;
        }

        // npm package — try to resolve .d.ts
        let exports = if let Some(cached) = dts_cache.get(specifier) {
            cached.clone()
        } else if let Some(dts_path) = resolve_npm_dts(specifier, project_dir) {
            match interop::parse_dts_exports_for_specifier(&dts_path, specifier) {
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
                .with_error_code(ErrorCode::PackageNotFound),
            );
            continue;
        };

        new_cache.insert(specifier.clone(), exports.clone());

        // Enrich imported symbols with type info from .d.ts. Detail
        // format mirrors `SymbolIndex::build`'s import rendering so
        // hover reads `Symbol.detail` directly.
        for sym in &mut index.symbols {
            if sym.import_source.as_deref() != Some(specifier) {
                continue;
            }
            let Some(dts_export) = exports.iter().find(|e| e.name == sym.name) else {
                continue;
            };
            let type_str = interop::ts_type_to_string(&dts_export.ts_type);
            sym.detail = format!("(import) {}: {type_str}\nfrom \"{specifier}\"", sym.name);

            if let interop::TsType::Function { params, .. } = &dts_export.ts_type {
                sym.kind = tower_lsp::lsp_types::SymbolKind::FUNCTION;
                sym.first_param_type = params
                    .first()
                    .map(|p| Arc::new(interop::wrap_boundary_type(&p.ty)));
            }
        }
    }

    (import_diags, new_cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate a minimal `@types/node` layout and verify that `node:X`
    /// specifiers route through to `@types/node/X.d.ts`. Regression for #1356.
    #[test]
    fn resolve_npm_dts_routes_node_scheme_to_types_node() {
        let dir = tempfile::tempdir().unwrap();
        let at_node = dir.path().join("node_modules/@types/node");
        std::fs::create_dir_all(&at_node).unwrap();
        std::fs::write(
            at_node.join("crypto.d.ts"),
            "declare module \"node:crypto\" {}",
        )
        .unwrap();
        std::fs::write(at_node.join("index.d.ts"), "").unwrap();

        let resolved = resolve_npm_dts("node:crypto", dir.path()).expect("should resolve");
        assert!(
            resolved.ends_with("crypto.d.ts"),
            "expected crypto.d.ts, got {}",
            resolved.display()
        );
    }

    #[test]
    fn resolve_npm_dts_node_scheme_falls_back_to_index() {
        let dir = tempfile::tempdir().unwrap();
        let at_node = dir.path().join("node_modules/@types/node");
        std::fs::create_dir_all(&at_node).unwrap();
        // No crypto.d.ts — only index.d.ts.
        std::fs::write(at_node.join("index.d.ts"), "").unwrap();

        let resolved = resolve_npm_dts("node:crypto", dir.path()).expect("should resolve");
        assert!(
            resolved.ends_with("index.d.ts"),
            "expected index.d.ts fallback, got {}",
            resolved.display()
        );
    }

    #[test]
    fn resolve_npm_dts_node_scheme_missing_types_node_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        // No @types/node installed at all.
        assert!(resolve_npm_dts("node:crypto", dir.path()).is_none());
    }
}
