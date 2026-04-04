//! npm / .d.ts interop module.
//!
//! Resolves npm modules by shelling out to `tsc`, parses the type
//! declarations from `.d.ts` files, and wraps types at the import
//! boundary so they conform to Floe semantics.
//!
//! Boundary conversions:
//! - `T | null`          -> `Option<T>`
//! - `T | undefined`     -> `Option<T>`
//! - `T | null | undefined` -> `Option<T>`
//! - `any`               -> `unknown`

pub mod ambient;
mod dts;
mod ts_types;
pub mod tsgo;
#[cfg(feature = "cli")]
pub mod tsgo_lsp;
mod wrapper;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::checker::Type;

// Re-export public API
pub use dts::{DtsExport, parse_dts_exports};
pub use ts_types::{FunctionParam, ObjectField, TsType, ts_type_to_string};
pub use tsgo::{TsgoResolver, TsgoResult};
pub use wrapper::wrap_boundary_type;

// Re-export internal helpers so tests (and sibling submodules) can access via `use super::*`
#[cfg(test)]
#[allow(unused_imports)]
use dts::{
    parse_const_export, parse_dts_exports_from_str, parse_function_export, parse_interface_export,
    parse_type_export,
};
#[cfg(test)]
#[allow(unused_imports)]
use ts_types::{find_matching_paren, parse_param_types, parse_type_str, split_at_top_level};

// ── Module Resolution ───────────────────────────────────────────

/// Result of resolving an npm module.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Absolute path to the .d.ts file
    pub dts_path: PathBuf,
    /// The module specifier as written in the import (e.g. "react")
    pub specifier: String,
}

/// Result of looking up exports from a resolved module.
#[derive(Debug, Clone)]
pub struct ModuleExports {
    /// Named exports: name -> wrapped Floe type
    pub exports: HashMap<String, Type>,
    /// The module specifier
    pub specifier: String,
}

/// Resolves an npm module specifier to its .d.ts file path using tsc.
///
/// Runs `tsc --moduleResolution bundler --noEmit --traceResolution` to find
/// the declaration file for a given module specifier.
pub fn resolve_module(specifier: &str, project_dir: &Path) -> Result<ResolvedModule, String> {
    // First try: look for a tsconfig.json and use tsc's resolution
    let tsconfig = crate::resolve::find_tsconfig_from(project_dir);

    let mut cmd = Command::new("tsc");
    cmd.current_dir(project_dir);

    if let Some(tsconfig_path) = &tsconfig {
        cmd.arg("--project").arg(tsconfig_path);
    }

    cmd.args(["--noEmit", "--traceResolution"]);

    // Create a temp file that imports the module so tsc resolves it
    let probe_content = format!("import {{}} from \"{specifier}\";");
    let probe_path = project_dir.join("__floe_probe__.ts");

    if std::fs::write(&probe_path, &probe_content).is_err() {
        return Err(format!(
            "failed to create probe file for module '{specifier}'"
        ));
    }

    cmd.arg(&probe_path);

    let output = cmd.output().map_err(|e| {
        let _ = std::fs::remove_file(&probe_path);
        format!("failed to run tsc: {e}. Is TypeScript installed?")
    })?;

    let _ = std::fs::remove_file(&probe_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}\n{stderr}");

    // Parse tsc's trace resolution output to find the .d.ts path
    // Look for lines like: "File '.../node_modules/@types/react/index.d.ts' exists"
    // or "Resolution for module 'react' was found in cache from location..."
    parse_resolved_path(&combined, specifier)
}

/// Parses tsc trace output to extract the resolved .d.ts path.
fn parse_resolved_path(trace: &str, specifier: &str) -> Result<ResolvedModule, String> {
    // Pattern 1: "======== Module name 'X' was successfully resolved to 'Y'. ========"
    let success_marker = "was successfully resolved to '";
    for line in trace.lines() {
        if line.contains(&format!("Module name '{specifier}'"))
            && let Some(start) = line.find(success_marker)
        {
            let rest = &line[start + success_marker.len()..];
            if let Some(end) = rest.find("'.") {
                let dts_path = &rest[..end];
                return Ok(ResolvedModule {
                    dts_path: PathBuf::from(dts_path),
                    specifier: specifier.to_string(),
                });
            }
        }
    }

    // Pattern 2: look for resolved .d.ts in node_modules
    // Try common locations directly
    Err(format!(
        "could not resolve module '{specifier}'. Make sure the package is installed (npm install)"
    ))
}

/// Resolves a module specifier and returns wrapped Floe types for its exports.
///
/// This is the main entry point: resolve the module, parse its .d.ts,
/// and wrap all exported types at the boundary.
pub fn resolve_and_wrap(specifier: &str, project_dir: &Path) -> Result<ModuleExports, String> {
    let resolved = resolve_module(specifier, project_dir)?;
    let dts_exports = parse_dts_exports(&resolved.dts_path)?;

    let mut exports = HashMap::new();
    for export in dts_exports {
        let wrapped = wrap_boundary_type(&export.ts_type);
        exports.insert(export.name, wrapped);
    }

    Ok(ModuleExports {
        exports,
        specifier: specifier.to_string(),
    })
}
