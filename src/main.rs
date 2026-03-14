use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use floe::checker::Checker;
use floe::codegen::Codegen;
use floe::diagnostic;
use floe::parser::Parser as ZsParser;
use floe::resolve;

#[derive(Parser)]
#[command(name = "floe", version, about = "The Floe compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile .fl files to .ts/.tsx
    Build {
        /// File or directory to compile ("-" for stdin)
        path: PathBuf,
        /// Output directory (defaults to same directory as input)
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
        /// Emit compiled output to stdout instead of writing files
        #[arg(long)]
        emit_stdout: bool,
    },
    /// Type-check .fl files without emitting output
    Check {
        /// File or directory to check
        path: PathBuf,
    },
    /// Watch .fl files and recompile on change
    Watch {
        /// File or directory to watch
        path: PathBuf,
        /// Output directory (defaults to same directory as input)
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
    /// Scaffold a new Floe project
    Init {
        /// Project directory (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Format .fl files
    Fmt {
        /// File or directory to format
        path: PathBuf,
        /// Check formatting without writing (exit 1 if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Start the language server (LSP)
    Lsp,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            path,
            out_dir,
            emit_stdout,
        } => {
            if emit_stdout || path.as_os_str() == "-" {
                cmd_build_stdin()
            } else {
                cmd_build(&path, out_dir.as_deref())
            }
        }
        Command::Check { path } => cmd_check(&path),
        Command::Fmt { path, check } => cmd_fmt(&path, check),
        Command::Watch { path, out_dir } => cmd_watch(&path, out_dir.as_deref()),
        Command::Init { path } => cmd_init(path.as_deref()),
        Command::Lsp => {
            tokio::runtime::Runtime::new()?.block_on(floe::lsp::run_lsp());
            Ok(())
        }
    }
}

// ── Build (stdin → stdout) ───────────────────────────────────────

fn cmd_build_stdin() -> Result<()> {
    use std::io::Read;

    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .context("failed to read from stdin")?;

    let filename = std::env::var("FLOE_FILENAME").unwrap_or_else(|_| "<stdin>".to_string());

    let program = ZsParser::new(&source).parse_program().map_err(|errs| {
        let diags = diagnostic::from_parse_errors(&errs);
        let rendered = diagnostic::render_diagnostics(&filename, &source, &diags);
        anyhow::anyhow!("{rendered}")
    })?;

    // Resolve imports from other .fl files (using FLOE_FILENAME for path context)
    let file_path = Path::new(&filename);
    let resolved = resolve::resolve_imports(file_path, &program);

    // Type check
    let (check_diags, expr_types) = Checker::with_imports(resolved).check_full(&program);
    let type_errors: Vec<_> = check_diags
        .iter()
        .filter(|d| d.severity == diagnostic::Severity::Error)
        .collect();
    if !type_errors.is_empty() {
        let rendered = diagnostic::render_diagnostics(&filename, &source, &check_diags);
        return Err(anyhow::anyhow!("{rendered}"));
    }

    let output = Codegen::with_expr_types(expr_types).generate(&program);
    print!("{}", output.code);

    Ok(())
}

// ── Build ────────────────────────────────────────────────────────

fn cmd_build(path: &Path, out_dir: Option<&Path>) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    let mut compiled = 0;
    let mut errors = 0;

    for file in &files {
        match compile_file(file, out_dir) {
            Ok(out_path) => {
                println!("  compiled {}", out_path.display());
                compiled += 1;
            }
            Err(e) => {
                eprintln!("  error {}: {e}", file.display());
                errors += 1;
            }
        }
    }

    println!();
    if errors > 0 {
        bail!("{compiled} compiled, {errors} failed");
    }
    println!("{compiled} file(s) compiled successfully");
    Ok(())
}

fn compile_file(file: &Path, out_dir: Option<&Path>) -> Result<PathBuf> {
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read {}", file.display()))?;

    let filename = file.to_string_lossy();
    let program = ZsParser::new(&source).parse_program().map_err(|errs| {
        let diags = diagnostic::from_parse_errors(&errs);
        let rendered = diagnostic::render_diagnostics(&filename, &source, &diags);
        anyhow::anyhow!("{rendered}")
    })?;

    // Resolve imports from other .fl files
    let resolved = resolve::resolve_imports(file, &program);

    // Type check
    let (check_diags, expr_types) = Checker::with_imports(resolved).check_full(&program);
    let type_errors: Vec<_> = check_diags
        .iter()
        .filter(|d| d.severity == diagnostic::Severity::Error)
        .collect();
    if !type_errors.is_empty() {
        let rendered = diagnostic::render_diagnostics(&filename, &source, &check_diags);
        return Err(anyhow::anyhow!("{rendered}"));
    }

    let output = Codegen::with_expr_types(expr_types).generate(&program);
    let ext = if output.has_jsx { "tsx" } else { "ts" };

    let out_path = if let Some(dir) = out_dir {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create output directory {}", dir.display()))?;
        dir.join(file.file_stem().unwrap()).with_extension(ext)
    } else {
        file.with_extension(ext)
    };

    std::fs::write(&out_path, &output.code)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    Ok(out_path)
}

// ── Check ────────────────────────────────────────────────────────

fn cmd_check(path: &Path) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    let mut checked = 0;
    let mut errors = 0;

    for file in &files {
        let source = std::fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;

        let filename = file.to_string_lossy();
        match ZsParser::new(&source).parse_program() {
            Ok(program) => {
                let resolved = resolve::resolve_imports(file, &program);
                let check_diags = Checker::with_imports(resolved).check(&program);
                let type_errors: Vec<_> = check_diags
                    .iter()
                    .filter(|d| d.severity == diagnostic::Severity::Error)
                    .collect();
                if type_errors.is_empty() {
                    checked += 1;
                } else {
                    let rendered = diagnostic::render_diagnostics(&filename, &source, &check_diags);
                    eprint!("{rendered}");
                    errors += 1;
                }
            }
            Err(errs) => {
                let diags = diagnostic::from_parse_errors(&errs);
                let rendered = diagnostic::render_diagnostics(&filename, &source, &diags);
                eprint!("{rendered}");
                errors += 1;
            }
        }
    }

    println!();
    if errors > 0 {
        bail!("{checked} ok, {errors} with errors");
    }
    println!("{checked} file(s) checked, no errors");
    Ok(())
}

// ── Fmt ──────────────────────────────────────────────────────────

fn cmd_fmt(path: &Path, check_only: bool) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    let mut unformatted = 0;
    let mut formatted = 0;

    for file in &files {
        let source = std::fs::read_to_string(file)
            .with_context(|| format!("failed to read {}", file.display()))?;

        let result = floe::formatter::format(&source);

        if result == source {
            formatted += 1;
            continue;
        }

        if check_only {
            println!("  would reformat {}", file.display());
            unformatted += 1;
        } else {
            std::fs::write(file, &result)
                .with_context(|| format!("failed to write {}", file.display()))?;
            println!("  formatted {}", file.display());
            formatted += 1;
        }
    }

    println!();
    if check_only && unformatted > 0 {
        bail!("{unformatted} file(s) would be reformatted");
    }

    let total = formatted + unformatted;
    if check_only {
        println!("{total} file(s) already formatted");
    } else {
        println!("{total} file(s) formatted");
    }
    Ok(())
}

// ── Watch ────────────────────────────────────────────────────────

fn cmd_watch(path: &Path, out_dir: Option<&Path>) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    use std::sync::mpsc;

    println!("watching {} for changes...", path.display());

    // Initial build
    if let Err(e) = cmd_build(path, out_dir) {
        eprintln!("{e}");
    }

    let (tx, rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res
            && (event.kind.is_modify() || event.kind.is_create())
        {
            for p in &event.paths {
                if p.extension().is_some_and(|ext| ext == "fl") {
                    let _ = tx.send(p.clone());
                }
            }
        }
    })?;

    let watch_path = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    watcher.watch(watch_path, RecursiveMode::Recursive)?;

    for changed_file in rx {
        println!("\n  changed: {}", changed_file.display());
        match compile_file(&changed_file, out_dir) {
            Ok(out_path) => println!("  compiled {}", out_path.display()),
            Err(e) => eprintln!("  error: {e}"),
        }
    }

    Ok(())
}

// ── Init ─────────────────────────────────────────────────────────

fn cmd_init(path: Option<&Path>) -> Result<()> {
    let dir = path.unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;

    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir)?;

    // Create a sample main.fl
    let main_zs = src_dir.join("main.fl");
    if !main_zs.exists() {
        std::fs::write(
            &main_zs,
            r#"import { useState } from "react"

type Todo = {
  id: string,
  text: string,
  done: bool,
}

export function App() {
  const [todos, setTodos] = useState([])

  return <div>
    <h1>Floe App</h1>
    {todos |> map(t => <p key={t.id}>{t.text}</p>)}
  </div>
}
"#,
        )?;
        println!("  created {}", main_zs.display());
    }

    // Create tsconfig.json if missing
    let tsconfig = dir.join("tsconfig.json");
    if !tsconfig.exists() {
        std::fs::write(
            &tsconfig,
            r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true
  },
  "include": ["src/**/*.ts", "src/**/*.tsx"]
}
"#,
        )?;
        println!("  created {}", tsconfig.display());
    }

    println!("\nFloe project initialized!");
    println!("  floe build src/   - compile .fl files");
    println!("  floe watch src/   - watch and recompile");

    Ok(())
}

// ── File Discovery ───────────────────────────────────────────────

fn discover_fl_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "fl") {
            return Ok(vec![path.to_path_buf()]);
        }
        bail!("{} is not a .fl file", path.display());
    }

    if !path.is_dir() {
        bail!("{} does not exist", path.display());
    }

    let mut files = Vec::new();
    collect_fl_files(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_fl_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Skip node_modules and hidden dirs
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && (name.starts_with('.') || name == "node_modules" || name == "target")
            {
                continue;
            }
            collect_fl_files(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "fl") {
            files.push(path);
        }
    }
    Ok(())
}
