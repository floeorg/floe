pub mod checker;
pub mod codegen;
pub mod cst;
pub mod desugar;
pub mod diagnostic;
pub mod formatter;
pub mod interop;
pub mod lexer;
pub mod lower;
#[cfg(feature = "cli")]
pub mod lsp;
pub mod parser;
pub mod resolve;
pub mod sourcemap;
pub mod stdlib;
pub mod syntax;
pub mod type_layout;
pub mod walk;

use std::path::{Path, PathBuf};

/// Find the project root directory (where node_modules lives).
/// Prioritizes finding `node_modules` over `package.json` to handle
/// pnpm workspaces where node_modules is hoisted to the workspace root.
pub fn find_project_dir(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    let mut package_json_dir: Option<PathBuf> = None;
    loop {
        if dir.join("node_modules").is_dir() {
            return dir;
        }
        if package_json_dir.is_none() && dir.join("package.json").is_file() {
            package_json_dir = Some(dir.clone());
        }
        if !dir.pop() {
            return package_json_dir.unwrap_or_else(|| start.to_path_buf());
        }
    }
}
