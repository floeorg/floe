//! Syntax-check Floe code samples embedded in documentation.
//!
//! Walks a set of doc files (Markdown and Astro), extracts every Floe code
//! sample, and runs each through the Floe parser. Reports parse errors as
//! `path:line:column: message` with line numbers rewritten back to the
//! original source file so editors can jump straight to the bad sample.
//!
//! Markdown sources contribute every ```floe fenced block. Astro sources
//! contribute every template literal preceded by an `@floe-check` marker
//! comment — see `extract_astro` for the rationale.

use std::path::{Path, PathBuf};

use floe_core::parser::Parser;

pub mod extract;
pub mod extract_astro;

pub use extract::{CodeBlock, extract_blocks};
pub use extract_astro::extract_astro_blocks;

#[derive(Debug, Clone)]
pub struct BlockError {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for BlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.path.display(),
            self.line,
            self.column,
            self.message
        )
    }
}

pub fn check_block(block: &CodeBlock) -> Vec<BlockError> {
    match Parser::parse(&block.code) {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .into_iter()
            .map(|err| BlockError {
                path: block.path.clone(),
                line: block.start_line + err.span.line.saturating_sub(1),
                column: err.span.column,
                message: err.message,
            })
            .collect(),
    }
}

/// Recursively collect every file under `root` whose extension matches one
/// of `extensions`. Symlinks are not followed, to avoid cycles.
pub fn find_files_with_extensions(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let matches_ext = |p: &Path| {
        p.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|e| extensions.contains(&e))
    };

    if root.is_file() {
        return if matches_ext(root) {
            vec![root.to_path_buf()]
        } else {
            Vec::new()
        };
    }

    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| matches_ext(p))
        .collect()
}

pub fn check_paths(roots: &[PathBuf]) -> std::io::Result<Vec<BlockError>> {
    let mut errors = Vec::new();

    let mut markdown_files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|r| find_files_with_extensions(r, &["md", "mdx"]))
        .collect();
    markdown_files.sort();
    markdown_files.dedup();

    let mut astro_files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|r| find_files_with_extensions(r, &["astro"]))
        .collect();
    astro_files.sort();
    astro_files.dedup();

    for path in &markdown_files {
        let source = std::fs::read_to_string(path)?;
        for block in extract_blocks(&source, path) {
            if block.is_ignored() {
                continue;
            }
            errors.extend(check_block(&block));
        }
    }

    for path in &astro_files {
        let source = std::fs::read_to_string(path)?;
        for block in extract_astro_blocks(&source, path) {
            errors.extend(check_block(&block));
        }
    }

    Ok(errors)
}
