//! Package-level compilation: orchestrates `analyse` + codegen across
//! many source files in a single project.
//!
//! The CLI feeds a file list in; `PackageCompiler` handles per-file
//! parse → analyse → codegen and tracks diagnostics / outputs. Tsgo
//! and ambient-type loading happen once per project directory instead
//! of once per file, which was a noticeable cost pre-refactor.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analyse::{self, ModuleInputs};
use crate::codegen::Codegen;
use crate::diagnostic::{self, Diagnostic, Severity};
use crate::interop::TsgoResolver;
use crate::interop::ambient::{self, AmbientDeclarations};
use crate::parser::Parser;
use crate::resolve::{self, ResolvedImports, TsconfigPaths};

/// One compiled file's outputs. Ready to write to disk or hand to an
/// in-memory consumer (LSP, wasm playground).
pub struct CompiledFile {
    pub source_path: PathBuf,
    /// Generated `.ts` / `.tsx` body.
    pub code: String,
    /// Whether the emitted code uses JSX (determines `.tsx` vs `.ts`).
    pub has_jsx: bool,
    /// Companion `.d.fl.ts` declaration (empty when the module has no exports).
    pub dts: String,
    /// Diagnostics surfaced during parse + analyse.
    pub diagnostics: Vec<Diagnostic>,
    /// Original source text, kept so callers can render diagnostics
    /// without re-reading the file.
    pub source: String,
}

impl CompiledFile {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Aggregate result of compiling many files. Aggregated so callers get a
/// single "did the build succeed" answer instead of walking per-file
/// diagnostics themselves.
pub struct BuildReport {
    pub files: Vec<CompiledFile>,
    pub parse_errors: Vec<(PathBuf, Vec<Diagnostic>)>,
}

impl BuildReport {
    pub fn has_errors(&self) -> bool {
        !self.parse_errors.is_empty() || self.files.iter().any(|f| f.has_errors())
    }

    pub fn ok_count(&self) -> usize {
        self.files.iter().filter(|f| !f.has_errors()).count()
    }

    pub fn error_count(&self) -> usize {
        self.parse_errors.len() + self.files.iter().filter(|f| f.has_errors()).count()
    }
}

/// Project-rooted compiler. One instance per `floe build` / `floe check`
/// invocation — reuses the project's tsgo session and ambient-type cache
/// across every module it compiles, where the pre-refactor code re-loaded
/// both per file.
pub struct PackageCompiler {
    project_dir: PathBuf,
    tsconfig_paths: TsconfigPaths,
    ambient: Option<AmbientDeclarations>,
}

impl PackageCompiler {
    /// Construct a compiler rooted at `project_dir`. The project root is
    /// usually `find_project_dir(source_dir)` — the directory holding
    /// `node_modules` / `package.json`.
    pub fn new(project_dir: PathBuf) -> Self {
        let tsconfig_paths = TsconfigPaths::from_project_dir(&project_dir);
        let ambient = ambient::load_ambient_types(&project_dir);
        Self {
            project_dir,
            tsconfig_paths,
            ambient,
        }
    }

    /// Compile one file. Parse errors are reported via the returned
    /// `CompiledFile.diagnostics`; type errors land there too. Production
    /// callers still render diagnostics themselves.
    pub fn compile_file(&self, path: &Path, source: String) -> CompiledFile {
        let program = match Parser::new(&source).parse_program() {
            Ok(p) => p,
            Err(errs) => {
                return CompiledFile {
                    source_path: path.to_path_buf(),
                    code: String::new(),
                    has_jsx: false,
                    dts: String::new(),
                    diagnostics: diagnostic::from_parse_errors(&errs),
                    source,
                };
            }
        };

        let source_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let resolved = resolve::resolve_imports(path, &program, &self.tsconfig_paths);
        let mut tsgo_resolver = TsgoResolver::new(&self.project_dir);
        let tsgo_result =
            tsgo_resolver.resolve_imports(&program, &resolved, &source_dir, &self.tsconfig_paths);

        let analysed = analyse::analyse_parsed(
            program,
            ModuleInputs {
                resolved_imports: resolved.clone(),
                dts_imports: tsgo_result.exports,
                ambient: self.ambient.clone(),
                ts_imports_missing_tsgo: tsgo_result.ts_imports_missing_tsgo,
            },
        );

        let output = Codegen::with_imports(&resolved).generate(&analysed.program);
        CompiledFile {
            source_path: path.to_path_buf(),
            code: output.code,
            has_jsx: output.has_jsx,
            dts: output.dts,
            diagnostics: analysed.diagnostics,
            source,
        }
    }

    /// Compile many files, reading source from disk. Files that can't be
    /// read are skipped and reported via `parse_errors` on the report.
    pub fn compile_files(&self, paths: &[PathBuf]) -> BuildReport {
        let mut files = Vec::with_capacity(paths.len());
        let mut parse_errors = Vec::new();
        for path in paths {
            match std::fs::read_to_string(path) {
                Ok(source) => files.push(self.compile_file(path, source)),
                Err(e) => {
                    parse_errors.push((
                        path.clone(),
                        vec![Diagnostic::error(
                            format!("failed to read {}: {e}", path.display()),
                            crate::lexer::span::Span::new(0, 0, 1, 1),
                        )],
                    ));
                }
            }
        }
        BuildReport {
            files,
            parse_errors,
        }
    }

    /// Check-only variant: run analyse without invoking codegen. Returns
    /// one set of diagnostics per input file so `floe check` can report
    /// them without building TS output it throws away.
    pub fn check_files(&self, paths: &[PathBuf]) -> Vec<(PathBuf, String, Vec<Diagnostic>)> {
        paths
            .iter()
            .map(|path| {
                let source = std::fs::read_to_string(path).unwrap_or_default();
                let diagnostics = self.check_one(path, &source);
                (path.clone(), source, diagnostics)
            })
            .collect()
    }

    fn check_one(&self, path: &Path, source: &str) -> Vec<Diagnostic> {
        let program = match Parser::new(source).parse_program() {
            Ok(p) => p,
            Err(errs) => return diagnostic::from_parse_errors(&errs),
        };
        let source_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let resolved: HashMap<String, ResolvedImports> =
            resolve::resolve_imports(path, &program, &self.tsconfig_paths);
        let mut tsgo_resolver = TsgoResolver::new(&self.project_dir);
        let tsgo_result =
            tsgo_resolver.resolve_imports(&program, &resolved, &source_dir, &self.tsconfig_paths);
        analyse::analyse_parsed(
            program,
            ModuleInputs {
                resolved_imports: resolved,
                dts_imports: tsgo_result.exports,
                ambient: self.ambient.clone(),
                ts_imports_missing_tsgo: tsgo_result.ts_imports_missing_tsgo,
            },
        )
        .diagnostics
    }
}
