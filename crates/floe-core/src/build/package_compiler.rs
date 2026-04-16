//! Package-level compilation: orchestrates `analyse` + codegen across
//! many source files in a single project. Ambient types and tsconfig
//! paths load once per project instead of once per file. When a
//! `CacheStore` is attached, `check_file` skips re-analysing modules
//! whose source and every dependency's source fingerprint matches the
//! cached run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analyse::{self, ExternTypes, ModuleInputs};
use crate::codegen::Codegen;
use crate::diagnostic::{self, Diagnostic, Severity};
use crate::interop::TsgoResolver;
use crate::interop::ambient::{self, AmbientDeclarations};
use crate::parser::Parser;
use crate::resolve::{self, ResolvedImports, TsconfigPaths};

use super::cache::{CacheStore, ModuleInterface};

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
    cache: Option<CacheStore>,
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
            cache: None,
        }
    }

    /// Enable the on-disk cache rooted at `cache_dir`. Call sites that
    /// want incremental `floe check` turn this on before any check
    /// runs; builders that always want fresh codegen leave it off.
    pub fn with_cache(mut self, cache_dir: PathBuf) -> Self {
        self.cache = Some(CacheStore::new(cache_dir));
        self
    }

    /// Compile one file through the full pipeline (parse → analyse →
    /// codegen). Parse errors surface as diagnostics on the returned
    /// `CompiledFile`.
    pub fn compile_file(&self, path: &Path, source: String) -> CompiledFile {
        match self.analyse_path(path, &source) {
            Ok((analysed, _dep_paths, resolved_imports)) => {
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
    /// only. When a cache is attached and the file's fingerprints match
    /// the last clean analyse, skips the full pipeline and returns an
    /// empty diagnostic list.
    pub fn check_file(&self, path: &Path, source: &str) -> Vec<Diagnostic> {
        if self.cache_hit(path, source) {
            return Vec::new();
        }
        match self.analyse_path(path, source) {
            Ok((analysed, dep_paths, resolved)) => {
                self.write_cache(path, source, &analysed.diagnostics, &dep_paths, &resolved);
                analysed.diagnostics
            }
            Err(parse_errors) => parse_errors,
        }
    }

    /// Read the cached `ResolvedImports` for a module, if fresh. Used by
    /// downstream modules to skip re-resolving this one's interface
    /// during their own analyse pass. `None` means the cache is missing,
    /// stale, or corrupt — caller should re-resolve.
    pub fn cached_imports(
        &self,
        path: &Path,
        source: &str,
    ) -> Option<HashMap<String, ResolvedImports>> {
        let cache = self.cache.as_ref()?;
        let relative = self.relative_source(path)?;
        let interface = cache.read(&relative)?;
        let dep_sources = read_dep_sources(&interface.dependency_hashes);
        if !CacheStore::is_fresh(&interface, source, &dep_sources) {
            return None;
        }
        Some(interface.resolved_imports)
    }

    /// True when the cache says the module's fingerprints still match.
    /// Corrupt / missing caches fall through as a miss so the full
    /// pipeline runs and overwrites bad bytes.
    fn cache_hit(&self, path: &Path, source: &str) -> bool {
        let Some(cache) = &self.cache else {
            return false;
        };
        let Some(relative) = self.relative_source(path) else {
            return false;
        };
        let Some(interface) = cache.read(&relative) else {
            return false;
        };
        let dep_sources = read_dep_sources(&interface.dependency_hashes);
        CacheStore::is_fresh(&interface, source, &dep_sources)
    }

    /// Persist the freshly-analysed interface so the next run can skip
    /// re-analyse. Writes are best-effort — a failure here just means
    /// the next run re-analyses, which is the same state we're in now.
    fn write_cache(
        &self,
        path: &Path,
        source: &str,
        diagnostics: &[Diagnostic],
        dep_paths: &std::collections::HashSet<PathBuf>,
        resolved: &HashMap<String, ResolvedImports>,
    ) {
        let Some(cache) = &self.cache else { return };
        let Some(relative) = self.relative_source(path) else {
            return;
        };
        let had_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
        let interface = ModuleInterface {
            source_hash: ModuleInterface::fingerprint(source.as_bytes()),
            dependency_hashes: fingerprint_paths(dep_paths),
            had_errors,
            resolved_imports: resolved.clone(),
        };
        let _ = cache.write(&relative, &interface);
    }

    /// Compute a stable cache key for `path`. Strips the project dir
    /// prefix so caches travel with the project rather than pinning to
    /// a particular worktree layout.
    fn relative_source(&self, path: &Path) -> Option<PathBuf> {
        let canonical = path.canonicalize().ok()?;
        canonical
            .strip_prefix(&self.project_dir)
            .ok()
            .map(|p| p.to_path_buf())
            .or(Some(canonical))
    }

    /// Shared setup for `compile_file` / `check_file`: parse, resolve
    /// imports, run tsgo, analyse. Returns `(analysed, dep_paths,
    /// resolved)` — `dep_paths` are the `.fl` files transitively reached
    /// during resolution, used by the cache to fingerprint dependencies
    /// for invalidation.
    #[allow(clippy::type_complexity)]
    fn analyse_path(
        &self,
        path: &Path,
        source: &str,
    ) -> Result<
        (
            analyse::AnalysedModule,
            std::collections::HashSet<PathBuf>,
            HashMap<String, ResolvedImports>,
        ),
        Vec<Diagnostic>,
    > {
        let program = Parser::new(source)
            .parse_program()
            .map_err(|errs| diagnostic::from_parse_errors(&errs))?;
        let source_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| path.parent().unwrap_or(Path::new(".")).to_path_buf());
        let (resolved, dep_paths) =
            resolve::resolve_imports_with_paths(path, &program, &self.tsconfig_paths);
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
        Ok((analysed, dep_paths, resolved))
    }
}

/// Read every `.fl` dependency and compute its xxh3 fingerprint. Paths
/// that can't be read drop out — the cache treats them as "missing"
/// which forces a re-analyse next run.
fn fingerprint_paths(paths: &std::collections::HashSet<PathBuf>) -> HashMap<PathBuf, u64> {
    paths
        .iter()
        .filter_map(|path| {
            std::fs::read(path)
                .ok()
                .map(|bytes| (path.clone(), ModuleInterface::fingerprint(&bytes)))
        })
        .collect()
}

/// Read every dependency source for freshness comparison. Missing files
/// are dropped so `is_fresh` reports them as changed.
fn read_dep_sources(hashes: &HashMap<PathBuf, u64>) -> HashMap<PathBuf, String> {
    hashes
        .keys()
        .filter_map(|path| {
            std::fs::read_to_string(path)
                .ok()
                .map(|s| (path.clone(), s))
        })
        .collect()
}
