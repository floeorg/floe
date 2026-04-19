use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use floe_doc_check::check_paths;

/// Syntax-check Floe code samples embedded in Markdown docs.
#[derive(Parser, Debug)]
#[command(name = "floe-doc-check", version)]
struct Cli {
    /// Markdown files or directories to scan. Directories are walked recursively.
    #[arg(required = true)]
    paths: Vec<PathBuf>,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let errors = check_paths(&cli.paths)?;

    if errors.is_empty() {
        println!("All Floe code samples parse cleanly.");
        return Ok(ExitCode::SUCCESS);
    }

    for err in &errors {
        println!("{err}");
    }
    eprintln!("\n{} parse error(s) in Floe code samples.", errors.len());
    Ok(ExitCode::FAILURE)
}
