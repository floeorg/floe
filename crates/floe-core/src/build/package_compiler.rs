//! Package-level compilation: orchestrates `analyse` + codegen across
//! many source files in a single project. Ambient types and tsconfig
//! paths load once per project instead of once per file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analyse::{self, ExternTypes, ModuleInputs};
use crate::codegen::Codegen;
use crate::diagnostic::{self, Diagnostic, Severity};
use crate::interop::TsgoResolver;
use crate::interop::ambient::{self, AmbientDeclarations};
use crate::parser::Parser;
use crate::resolve::{self, ResolvedImports, TsconfigPaths};

/// One compiled file's outputs.
pub struct CompiledFile {
    pub source_path: PathBuf,
    pub code: String,
    pub has_jsx: bool,
    pub dts: String,
    pub diagnostics: Vec<Diagnostic>,
    pub source: String,
}

impl CompiledFile {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Project-rooted compiler. One instance per build / check invocation —
/// the ambient type cache and tsconfig paths load once and are reused
/// across every module.
pub struct PackageCompiler {
    project_dir: PathBuf,
    tsconfig_paths: TsconfigPaths,
    ambient: Option<AmbientDeclarations>,
}

impl PackageCompiler {
    /// Construct a compiler rooted at the directory holding
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

    /// Compile one file through the full pipeline (parse → analyse →
    /// codegen). Parse errors surface as diagnostics on the returned
    /// `CompiledFile`.
    pub fn compile_file(&self, path: &Path, source: String) -> CompiledFile {
        match self.analyse_path(path, &source) {
            Ok((analysed, resolved_imports)) => {
                let output = Codegen::with_imports(&resolved_imports).generate(&analysed.program);
                CompiledFile {
                    source_path: path.to_path_buf(),
                    code: output.code,
                    has_jsx: output.has_jsx,
                    dts: output.dts,
                    diagnostics: analysed.diagnostics,
                    source,
                }
            }
            Err(diagnostics) => CompiledFile {
                source_path: path.to_path_buf(),
                code: String::new(),
                has_jsx: false,
                dts: String::new(),
                diagnostics,
                source,
            },
        }
    }

    /// Check one file without invoking codegen. Returns the diagnostics
    /// only — callers that need source text already have it.
    pub fn check_file(&self, path: &Path, source: &str) -> Vec<Diagnostic> {
        match self.analyse_path(path, source) {
            Ok((analysed, _)) => analysed.diagnostics,
            Err(parse_errors) => parse_errors,
        }
    }

    /// Shared setup for `compile_file` / `check_file`: parse, resolve
    /// imports, run tsgo, analyse. Returns parse errors as `Err` so
    /// callers short-circuit their downstream work cleanly.
    #[allow(clippy::type_complexity)]
    fn analyse_path(
        &self,
        path: &Path,
        source: &str,
    ) -> Result<(analyse::AnalysedModule, HashMap<String, ResolvedImports>), Vec<Diagnostic>> {
        let program = Parser::new(source)
            .parse_program()
            .map_err(|errs| diagnostic::from_parse_errors(&errs))?;
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
                externs: ExternTypes {
                    dts_imports: tsgo_result.exports,
                    ambient: self.ambient.clone(),
                    ts_imports_missing_tsgo: tsgo_result.ts_imports_missing_tsgo,
                },
            },
        );
        Ok((analysed, resolved))
    }
}
