use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use floe_core::analyse::{self, ExternTypes, ModuleInputs};
use floe_core::build::PackageCompiler;
use floe_core::codegen::Codegen;
use floe_core::diagnostic;
use floe_core::find_project_dir;
use floe_core::resolve::{self, ResolvedImports, TsconfigPaths};

/// Resolve the source directory, project directory, and tsconfig paths for a file.
fn resolve_context(file_path: &Path) -> (PathBuf, PathBuf, TsconfigPaths) {
    let source_dir = file_path
        .parent()
        .unwrap_or(Path::new("."))
        .canonicalize()
        .unwrap_or_else(|_| file_path.parent().unwrap_or(Path::new(".")).to_path_buf());
    let project_dir = find_project_dir(&source_dir);
    let tsconfig_paths = TsconfigPaths::from_project_dir(&project_dir);
    (source_dir, project_dir, tsconfig_paths)
}

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
        /// Output directory (defaults to .floe/ at project root)
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
        /// Output directory (defaults to .floe/ at project root)
        #[arg(short, long)]
        out_dir: Option<PathBuf>,
    },
    /// Scaffold a new Floe project
    Init {
        /// Project directory (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Run inline test blocks
    Test {
        /// File or directory to test
        path: PathBuf,
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
            if path.as_os_str() == "-" {
                cmd_build_stdin()
            } else if emit_stdout {
                cmd_build_file_stdout(&path)
            } else {
                cmd_build(&path, out_dir.as_deref())
            }
        }
        Command::Check { path } => cmd_check(&path),
        Command::Test { path } => cmd_test(&path),
        Command::Fmt { path, check } => cmd_fmt(&path, check),
        Command::Watch { path, out_dir } => cmd_watch(&path, out_dir.as_deref()),
        Command::Init { path } => cmd_init(path.as_deref()),
        Command::Lsp => {
            tokio::runtime::Runtime::new()?.block_on(floe_lsp::run_lsp());
            Ok(())
        }
    }
}

// ── Shared Compilation Pipeline ──────────────────────────────────

/// Result of parsing, resolving, and type-checking a single source file.
struct CompileResult {
    program: floe_core::parser::ast::TypedProgram,
    resolved: HashMap<String, ResolvedImports>,
}

/// Parse, resolve imports, and type-check a single source. Returns an error
/// string (with rendered diagnostics) on failure.
fn compile_source(file_path: &Path, filename: &str, source: &str) -> Result<CompileResult> {
    let (source_dir, project_dir, tsconfig_paths) = resolve_context(file_path);
    // Parse once to seed the import resolver — analyse re-parses inside, but
    // the cost is negligible next to tsgo and ambient loading.
    let program = floe_core::parser::Parser::new(source)
        .parse_program()
        .map_err(|errs| {
            let diags = diagnostic::from_parse_errors(&errs);
            let rendered = diagnostic::render_diagnostics(filename, source, &diags);
            anyhow::anyhow!("{rendered}")
        })?;

    let resolved = resolve::resolve_imports(file_path, &program, &tsconfig_paths);
    let mut tsgo_resolver = floe_core::interop::TsgoResolver::new(&project_dir);
    let tsgo_result =
        tsgo_resolver.resolve_imports(&program, &resolved, &source_dir, &tsconfig_paths);
    let ambient = floe_core::interop::ambient::load_ambient_types(&project_dir);

    let analysed = analyse::analyse_parsed(
        program,
        ModuleInputs {
            resolved_imports: resolved.clone(),
            externs: ExternTypes {
                dts_imports: tsgo_result.exports,
                ambient,
                ts_imports_missing_tsgo: tsgo_result.ts_imports_missing_tsgo,
            },
        },
    );

    // Render errors to stderr so the user sees them even when compilation
    // continues (strict reporting is `floe check`'s job).
    let type_errors: Vec<_> = analysed
        .diagnostics
        .iter()
        .filter(|d| d.severity == diagnostic::Severity::Error)
        .collect();
    if !type_errors.is_empty() {
        let rendered = diagnostic::render_diagnostics(filename, source, &analysed.diagnostics);
        eprintln!("{rendered}");
    }

    Ok(CompileResult {
        program: analysed.program,
        resolved,
    })
}

// ── Build (file -> stdout) ────────────────────────────────────────

fn cmd_build_file_stdout(path: &Path) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let filename = path.display().to_string();

    let result = compile_source(path, &filename, &source)?;
    let output = Codegen::with_imports(&result.resolved).generate(&result.program);
    print!("{}", output.code);

    // Write .d.fl.ts to .floe/ so TypeScript can resolve types via rootDirs
    if !output.dts.is_empty() {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", path.display()))?;
        let cwd = std::env::current_dir().context("failed to get current directory")?;
        let relative = canonical.strip_prefix(&cwd).unwrap_or(&canonical);
        let project_dir = find_project_dir(&cwd);
        let out_dir = project_dir.join(".floe");
        let dts_name = format!(
            "{}.d.fl.ts",
            relative.file_stem().unwrap_or_default().to_string_lossy()
        );
        let dts_path = out_dir.join(relative).with_file_name(dts_name);
        if let Some(parent) = dts_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&dts_path, &output.dts).ok();
    }

    Ok(())
}

// ── Build (stdin -> stdout) ───────────────────────────────────────

fn cmd_build_stdin() -> Result<()> {
    use std::io::Read;

    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .context("failed to read from stdin")?;

    let filename = std::env::var("FLOE_FILENAME").unwrap_or_else(|_| "<stdin>".to_string());
    let file_path = Path::new(&filename);

    let result = compile_source(file_path, &filename, &source)?;
    let output = Codegen::with_imports(&result.resolved).generate(&result.program);
    print!("{}", output.code);

    Ok(())
}

// ── Build ────────────────────────────────────────────────────────

fn cmd_build(path: &Path, out_dir: Option<&Path>) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let project_dir = find_project_dir(&cwd);
    let default_out_dir = project_dir.join(".floe");
    let out_dir = out_dir.unwrap_or(&default_out_dir);

    let compiler = PackageCompiler::new(project_dir);
    let mut compiled = 0;
    let mut errors = 0;

    for file in &files {
        match compile_and_write(&compiler, file, out_dir, &cwd) {
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

fn compile_and_write(
    compiler: &PackageCompiler,
    file: &Path,
    out_dir: &Path,
    cwd: &Path,
) -> Result<PathBuf> {
    let source = read_fl_file(file)?;
    let compiled = compiler.compile_file(file, source);

    let type_errors: Vec<_> = compiled
        .diagnostics
        .iter()
        .filter(|d| d.severity == diagnostic::Severity::Error)
        .collect();
    if !type_errors.is_empty() {
        let rendered = diagnostic::render_diagnostics(
            &file.to_string_lossy(),
            &compiled.source,
            &compiled.diagnostics,
        );
        eprintln!("{rendered}");
    }

    let ext = if compiled.has_jsx { "tsx" } else { "ts" };
    let canonical = file
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", file.display()))?;
    let relative = canonical.strip_prefix(cwd).unwrap_or(&canonical);
    let out_path = out_dir.join(relative).with_extension(ext);

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let code_with_header = format!("// @ts-nocheck\n{}", compiled.code);
    std::fs::write(&out_path, &code_with_header)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    if !compiled.dts.is_empty() {
        let dts_name = format!(
            "{}.d.fl.ts",
            relative.file_stem().unwrap_or_default().to_string_lossy()
        );
        let dts_path = out_dir.join(relative).with_file_name(dts_name);
        std::fs::write(&dts_path, &compiled.dts)
            .with_context(|| format!("failed to write {}", dts_path.display()))?;
    }

    Ok(out_path)
}

// ── Check ────────────────────────────────────────────────────────

fn cmd_check(path: &Path) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    // All files in a check run share the project root, so one compiler
    // instance serves the whole pass — ambient types and tsconfig paths
    // load once instead of once per file. The on-disk cache skips
    // unchanged modules on subsequent runs.
    let first_project_dir = find_project_dir(
        &files[0]
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from(".")),
    );
    let cache_dir = first_project_dir.join(".floe").join("cache");
    let compiler = PackageCompiler::new(first_project_dir).with_cache(cache_dir);

    let mut checked = 0;
    let mut errors = 0;

    for file in &files {
        let source = read_fl_file(file)?;
        let filename = file.to_string_lossy();
        let diagnostics = compiler.check_file(file, &source);
        let type_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == diagnostic::Severity::Error)
            .collect();
        if type_errors.is_empty() {
            checked += 1;
        } else {
            let rendered = diagnostic::render_diagnostics(&filename, &source, &diagnostics);
            eprint!("{rendered}");
            errors += 1;
        }
    }

    println!();
    if errors > 0 {
        bail!("{checked} ok, {errors} with errors");
    }
    println!("{checked} file(s) checked, no errors");
    Ok(())
}

// ── Test ─────────────────────────────────────────────────────────

fn cmd_test(path: &Path) -> Result<()> {
    let files = discover_fl_files(path)?;
    if files.is_empty() {
        bail!("no .fl files found in {}", path.display());
    }

    // Find all files that contain test blocks
    let mut test_files = Vec::new();
    for file in &files {
        let source = read_fl_file(file)?;

        // Quick check: does the file contain "test " keyword?
        if source.contains("test ") {
            let filename = file.to_string_lossy();
            match floe_core::parser::Parser::new(&source).parse_program() {
                Ok(program) => {
                    // Check if program has any test blocks
                    let has_tests = program.items.iter().any(|item| {
                        matches!(item.kind, floe_core::parser::ast::ItemKind::TestBlock(_))
                    });
                    if has_tests {
                        test_files.push((
                            file.clone(),
                            source.clone(),
                            filename.to_string(),
                            program,
                        ));
                    }
                }
                Err(errs) => {
                    let diags = diagnostic::from_parse_errors(&errs);
                    let rendered = diagnostic::render_diagnostics(&filename, &source, &diags);
                    eprint!("{rendered}");
                }
            }
        }
    }

    if test_files.is_empty() {
        println!("no test blocks found");
        return Ok(());
    }

    let mut total_files = 0;
    let mut errors = 0;

    for (file, source, filename, program) in &mut test_files {
        let (_source_dir, _project_dir, tsconfig_paths) = resolve_context(file);
        let resolved = resolve::resolve_imports(file, program, &tsconfig_paths);

        let analysed = analyse::analyse_parsed(
            program.clone(),
            ModuleInputs {
                resolved_imports: resolved.clone(),
                ..Default::default()
            },
        );
        let type_errors: Vec<_> = analysed
            .diagnostics
            .iter()
            .filter(|d| d.severity == diagnostic::Severity::Error)
            .collect();
        if !type_errors.is_empty() {
            let rendered = diagnostic::render_diagnostics(filename, source, &analysed.diagnostics);
            eprint!("{rendered}");
            errors += 1;
            continue;
        }

        let output = Codegen::with_imports(&resolved)
            .with_test_mode()
            .generate(&analysed.program);

        // Write to a temp file and execute with a JS runtime
        let ext = if output.has_jsx { "tsx" } else { "ts" };
        let temp_dir = std::env::temp_dir().join("floe-tests");
        std::fs::create_dir_all(&temp_dir)?;
        let temp_file = temp_dir.join(file.file_stem().unwrap()).with_extension(ext);
        std::fs::write(&temp_file, &output.code)?;

        println!("testing {}...", file.display());

        // Try to run with tsx, ts-node, or npx tsx
        let runners = ["tsx", "npx"];
        let mut ran = false;
        for runner in &runners {
            let result = if *runner == "npx" {
                std::process::Command::new("npx")
                    .arg("tsx")
                    .arg(&temp_file)
                    .status()
            } else {
                std::process::Command::new(runner).arg(&temp_file).status()
            };

            match result {
                Ok(status) => {
                    if !status.success() {
                        errors += 1;
                    }
                    ran = true;
                    break;
                }
                Err(_) => continue,
            }
        }

        if !ran {
            eprintln!(
                "  warning: could not find a TypeScript runner (tsx). Install with: npm install -g tsx"
            );
            // Fall back to just checking - print the generated test code location
            println!("  generated: {}", temp_file.display());
        }

        total_files += 1;
    }

    println!();
    if errors > 0 {
        bail!("{total_files} file(s) tested, {errors} with failures");
    }
    println!("{total_files} file(s) tested, all passed");
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
    let mut skipped = 0;

    for file in &files {
        let source = read_fl_file(file)?;

        let Some(result) = floe_core::formatter::format(&source) else {
            eprintln!("  skipping {} (parse error)", file.display());
            skipped += 1;
            continue;
        };

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
    if skipped > 0 {
        eprintln!("{skipped} file(s) skipped due to parse errors");
    }
    Ok(())
}

// ── Watch ────────────────────────────────────────────────────────

fn cmd_watch(path: &Path, out_dir: Option<&Path>) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    use std::sync::mpsc;

    println!("watching {} for changes...", path.display());

    // Default output directory: .floe/ at the project root
    let project_dir =
        find_project_dir(&std::env::current_dir().context("failed to get current directory")?);
    let default_out_dir = project_dir.join(".floe");
    let out_dir = out_dir.unwrap_or(&default_out_dir);

    // Initial build
    if let Err(e) = cmd_build(path, Some(out_dir)) {
        eprintln!("{e}");
    }

    let cwd = std::env::current_dir().context("failed to get current directory")?;
    let project_dir = find_project_dir(&cwd);
    let compiler = PackageCompiler::new(project_dir);

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
        match compile_and_write(&compiler, &changed_file, out_dir, &cwd) {
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
  let [todos, setTodos] = useState([])

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

// ── File I/O Helpers ─────────────────────────────────────────────

/// Read a .fl source file with error context.
fn read_fl_file(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
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
