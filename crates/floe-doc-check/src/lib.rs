//! Syntax-check Floe code samples embedded in Markdown documentation.
//!
//! Walks a set of Markdown files, extracts every ```floe fenced code block,
//! and runs each through the Floe parser. Reports parse errors as
//! `path:line:column: message` with line numbers rewritten back to the
//! original Markdown file so editors can jump straight to the bad sample.

use std::path::{Path, PathBuf};

use floe_core::parser::Parser;

pub mod extract;

pub use extract::{CodeBlock, extract_blocks};

/// A parse error reported against its original position in the Markdown file.
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

/// Parse the code in a block and map any errors back to the Markdown file.
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

/// Recursively find Markdown files under `root`.
///
/// Includes `*.md` and `*.mdx`. Follows symlinks is `false` to avoid cycles.
pub fn find_markdown_files(root: &Path) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("md") | Some("mdx")
            )
        })
        .collect()
}

/// Check every ```floe block in every Markdown file under `roots`.
///
/// Returns the full error list. An empty vector means every sample parsed.
pub fn check_paths(roots: &[PathBuf]) -> std::io::Result<Vec<BlockError>> {
    let mut errors = Vec::new();
    let mut files: Vec<PathBuf> = roots.iter().flat_map(|r| find_markdown_files(r)).collect();
    files.sort();
    files.dedup();

    for path in &files {
        let source = std::fs::read_to_string(path)?;
        for block in extract_blocks(&source, path) {
            if block.is_ignored() {
                continue;
            }
            errors.extend(check_block(&block));
        }
    }
    Ok(errors)
}
