//! tsgo-based type resolution for npm imports.
//!
//! Resolves import types by:
//! 1. Running a probe file through tsgo for call-site type resolution
//! 2. Parsing .d.ts / .ts files directly for type definitions
//! 3. Querying tsgo LSP (hover) for richer type resolution

mod probe_gen;
#[cfg(feature = "native")]
mod probe_run;
mod specifier_map;
mod typeof_resolve;

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::parser::ast::*;

use super::DtsExport;
use super::TsType;
use super::dts::{
    collect_function_aliases_from_file, collect_referenced_modules, expand_cross_module_aliases,
    parse_dts_exports_from_str, parse_dts_exports_with_import_sources, strip_import_sentinels,
};

use probe_gen::generate_probe;
#[cfg(feature = "native")]
pub use probe_run::is_tsgo_available;
#[cfg(feature = "native")]
use probe_run::{create_probe_dir, run_tsgo};
use specifier_map::build_specifier_map;

/// Result of resolving TypeScript imports via tsgo.
pub struct TsgoResult {
    /// Resolved exports per import specifier.
    pub exports: HashMap<String, Vec<DtsExport>>,
    /// Generic type parameter metadata (name + optional default) collected
    /// from .d.ts declarations, keyed by the generic's declaration name
    /// (e.g. "Context"). Lets the checker pad partial type argument lists
    /// with TypeScript's own defaults when a library writes
    /// `interface Context<E = Env, P = any, I = {}>`.
    pub generic_param_defs: HashMap<String, Vec<super::dts::GenericParamInfo>>,
    /// Import sources that resolve to `.ts`/`.tsx` files but could not be
    /// resolved because tsgo is not installed.
    pub ts_imports_missing_tsgo: HashSet<String>,
}

/// Resolves npm import types using probes, DTS parsing, and tsgo LSP.
pub struct TsgoResolver {
    project_dir: PathBuf,
    cache: HashMap<u64, Vec<DtsExport>>,
    /// None = not attempted, Some(None) = failed, Some(Some(_)) = ready
    #[cfg(feature = "native")]
    lsp_client: Option<Option<super::tsgo_lsp::TsgoLspClient>>,
}

impl TsgoResolver {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
            cache: HashMap::new(),
            #[cfg(feature = "native")]
            lsp_client: None,
        }
    }

    /// Get or initialize the LSP client lazily. Returns None if tsgo is
    /// unavailable (only attempts initialization once).
    #[cfg(feature = "native")]
    fn lsp_client(&mut self) -> Option<&mut super::tsgo_lsp::TsgoLspClient> {
        if self.lsp_client.is_none() {
            let result = match super::tsgo_lsp::TsgoLspClient::new(&self.project_dir) {
                Ok(client) => Some(client),
                Err(e) => {
                    eprintln!("[floe] tsgo LSP: {e}");
                    None
                }
            };
            self.lsp_client = Some(result);
        }
        self.lsp_client.as_mut().and_then(|opt| opt.as_mut())
    }

    /// Resolve npm and local TypeScript imports in a program by generating a
    /// probe file, running tsgo, and parsing the output `.d.ts`.
    ///
    /// `source_dir` is the directory of the `.fl` file being compiled, used to
    /// resolve relative imports to local `.ts`/`.tsx` files.
    ///
    /// Returns a [`TsgoResult`] with resolved exports and any `.ts` imports
    /// that could not be resolved because tsgo is not installed.
    pub fn resolve_imports(
        &mut self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
        source_dir: &Path,
        tsconfig_paths: &crate::resolve::TsconfigPaths,
    ) -> TsgoResult {
        #[cfg(not(feature = "native"))]
        {
            let _ = (program, resolved_imports, source_dir, tsconfig_paths);
            return TsgoResult {
                exports: HashMap::new(),
                generic_param_defs: HashMap::new(),
                ts_imports_missing_tsgo: HashSet::new(),
            };
        }

        #[cfg(feature = "native")]
        {
            let ts_imports =
                find_relative_ts_imports(program, resolved_imports, source_dir, tsconfig_paths);

            // When .ts/.tsx imports exist and tsgo is not available, still
            // resolve npm imports (which use .d.ts and don't need tsgo) but
            // record the .ts sources so the checker can emit a hard error.
            let missing_tsgo: HashSet<String> = if !ts_imports.is_empty() && !is_tsgo_available() {
                ts_imports.keys().cloned().collect()
            } else {
                HashSet::new()
            };

            // Exclude .ts imports from probe when tsgo is unavailable — the
            // probe file would reference them and fail to compile.
            let effective_ts_imports = if missing_tsgo.is_empty() {
                &ts_imports
            } else {
                &HashMap::new()
            };

            // Probe-based resolution for call-site types, enhanced with
            // DTS parsing, typeof resolution, and LSP hover for unresolved types.
            let mut result = self.run_probe(program, resolved_imports, effective_ts_imports);
            self.enhance_import_types(&mut result, program, effective_ts_imports);
            // Collect generic type parameter defaults from every resolved
            // .d.ts source so the checker can pad partial type argument
            // lists (e.g. Hono's `Context<E = Env, P extends string = any,
            // I extends Input = {}>`).
            let generic_param_defs = self.collect_generic_param_defs(program, resolved_imports);
            TsgoResult {
                exports: result,
                generic_param_defs,
                ts_imports_missing_tsgo: missing_tsgo,
            }
        }
    }

    /// Collect generic type parameter defaults (e.g. `E = Env`) from every
    /// npm package .d.ts referenced by the program. tsgo's probe output
    /// loses original interface declarations, so we re-read the source
    /// .d.ts for each specifier and collect defaults directly.
    #[cfg(feature = "native")]
    fn collect_generic_param_defs(
        &self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
    ) -> HashMap<String, Vec<super::dts::GenericParamInfo>> {
        let mut out: HashMap<String, Vec<super::dts::GenericParamInfo>> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        for item in &program.items {
            let crate::parser::ast::ItemKind::Import(imp) = &item.kind else {
                continue;
            };
            let specifier = imp.source.trim_matches('"');
            if resolved_imports.contains_key(specifier) {
                continue;
            }
            if !seen.insert(specifier.to_string()) {
                continue;
            }
            let Some(dts_path) = typeof_resolve::find_package_dts(&self.project_dir, specifier)
            else {
                continue;
            };
            let Ok(content) = std::fs::read_to_string(&dts_path) else {
                continue;
            };
            if let Ok(defs) = super::dts::collect_generic_param_defs_from_source(&content) {
                for (k, v) in defs {
                    out.entry(k).or_insert(v);
                }
            }
        }
        out
    }

    /// Run the probe system for call-site type resolution.
    #[cfg(feature = "native")]
    fn run_probe(
        &mut self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
        ts_imports: &HashMap<String, PathBuf>,
    ) -> HashMap<String, Vec<DtsExport>> {
        let probe = generate_probe(program, resolved_imports, ts_imports);
        if probe.is_empty() {
            return HashMap::new();
        }

        let hash = {
            let mut hasher = DefaultHasher::new();
            probe.hash(&mut hasher);
            hasher.finish()
        };
        if let Some(cached) = self.cache.get(&hash) {
            return build_specifier_map(program, cached, ts_imports);
        }

        let tmp = match create_probe_dir(&self.project_dir, &probe, ts_imports) {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to create probe dir: {e}");
                return HashMap::new();
            }
        };

        let dts_content = match run_tsgo(tmp.path()) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("[floe] tsgo: {e}");
                return HashMap::new();
            }
        };

        if std::env::var("FLOE_DEBUG_PROBE").is_ok() {
            eprintln!("[floe] DTS OUTPUT:\n{dts_content}");
        }

        let mut exports = match parse_dts_exports_with_import_sources(&dts_content) {
            Ok(exports) => exports,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to parse output: {e}");
                return HashMap::new();
            }
        };

        // tsgo emits cross-module type alias references as
        // `import("pkg").AliasName<args>`. `parse_dts_exports_from_str`
        // encodes the source module into the name so we can resolve those
        // references here by parsing each referenced package's `.d.ts` for
        // its function-shaped aliases. Without this, aliases like
        // `Handler<E>` from `@floeorg/hono` reach the checker as opaque
        // `Foreign` types and the lambda-hint propagation path can't read
        // the expected callback shape (#1234).
        self.expand_cross_module_aliases_in(&mut exports);

        self.cache.insert(hash, exports.clone());
        build_specifier_map(program, &exports, ts_imports)
    }

    /// Resolve cross-module type-alias references (`import("pkg").X<...>`)
    /// by parsing each referenced package's main `.d.ts`. Strips the
    /// module-source sentinel from any names that survive unexpanded so the
    /// boundary wrapper sees clean identifiers.
    #[cfg(feature = "native")]
    fn expand_cross_module_aliases_in(&self, exports: &mut [DtsExport]) {
        let mut referenced = HashSet::new();
        for export in exports.iter() {
            collect_referenced_modules(&export.ts_type, &mut referenced);
        }
        // Fast path: tsgo output had no `import("pkg").X` references, so
        // parse_dts_exports_from_str never encoded a sentinel and no
        // expansion or stripping is needed.
        if referenced.is_empty() {
            return;
        }
        let mut aliases_by_module: HashMap<String, HashMap<String, _>> = HashMap::new();
        for module in &referenced {
            if let Some(dts_path) = typeof_resolve::find_package_dts(&self.project_dir, module) {
                let aliases = collect_function_aliases_from_file(&dts_path);
                if !aliases.is_empty() {
                    aliases_by_module.insert(module.clone(), aliases);
                }
            }
        }
        for export in exports.iter_mut() {
            if !aliases_by_module.is_empty() {
                expand_cross_module_aliases(&mut export.ts_type, &aliases_by_module, 0);
            }
            strip_import_sentinels(&mut export.ts_type);
        }
    }

    /// Enhance probe results with LSP/DTS parsing for better import types.
    #[cfg(feature = "native")]
    fn enhance_import_types(
        &mut self,
        result: &mut HashMap<String, Vec<DtsExport>>,
        program: &Program,
        ts_imports: &HashMap<String, PathBuf>,
    ) {
        // Parse .ts files for exported + non-exported type definitions
        for (specifier, ts_path) in ts_imports {
            let ts_content = match std::fs::read_to_string(ts_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Exported declarations
            if let Ok(ts_exports) = parse_dts_exports_from_str(&ts_content) {
                let entry = result.entry(specifier.clone()).or_default();
                for ts_export in ts_exports {
                    if let Some(existing) = entry.iter_mut().find(|e| e.name == ts_export.name) {
                        if matches!(existing.ts_type, TsType::Any)
                            && !matches!(ts_export.ts_type, TsType::Any | TsType::Unknown)
                        {
                            existing.ts_type = ts_export.ts_type;
                        }
                    } else {
                        entry.push(ts_export);
                    }
                }
            }
            // Non-exported types (interfaces used internally)
            if let Ok(all_types) = super::dts::parse_all_types_from_str(&ts_content) {
                let entry = result.entry(specifier.clone()).or_default();
                for type_def in all_types {
                    if !entry.iter().any(|e| e.name == type_def.name) {
                        entry.push(type_def);
                    }
                }
            }
        }

        // Resolve typeof references and register npm type definitions
        typeof_resolve::resolve_typeof_types(result, &self.project_dir, program);

        // LSP enhancement for remaining unresolved types
        self.enhance_with_lsp(result, program, ts_imports);

        // Enhance per-field probes from object destructuring that resolved to `any`.
        // When tsgo can't resolve a complex generic return type, fall back to
        // resolving the function's return type via LSP and extracting fields.
        self.enhance_object_destructure_probes(result, program, ts_imports);

        // Resolve Foreign types referenced in probes — when a probe returns a
        // Named type like DraggableProvided, fetch the type's definition via LSP
        // so the checker can validate field access.
        self.resolve_foreign_type_definitions(result, program, ts_imports);
    }

    /// For Named types referenced in probe results, resolve their definitions
    /// via LSP hover so the checker can validate field access.
    #[cfg(feature = "native")]
    fn resolve_foreign_type_definitions(
        &mut self,
        result: &mut HashMap<String, Vec<DtsExport>>,
        program: &Program,
        ts_imports: &HashMap<String, PathBuf>,
    ) {
        // Collect Named types from probe results that don't have definitions yet
        let mut types_to_resolve: Vec<(String, String)> = Vec::new(); // (type_name, specifier)

        // Build import name → source path mapping for LSP queries
        let mut import_paths: HashMap<String, PathBuf> = HashMap::new();
        for item in &program.items {
            if let crate::parser::ast::ItemKind::Import(decl) = &item.kind {
                let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
                if !is_relative
                    && let Some(dts_path) =
                        typeof_resolve::find_package_dts(&self.project_dir, &decl.source)
                {
                    import_paths.entry(decl.source.clone()).or_insert(dts_path);
                }
                if let Some(ts_path) = ts_imports.get(&decl.source) {
                    import_paths
                        .entry(decl.source.clone())
                        .or_insert_with(|| ts_path.clone());
                }
            }
        }

        // Pre-collect names that already have Object definitions
        let defined_names: HashSet<&str> = result
            .values()
            .flatten()
            .filter(|e| matches!(e.ts_type, TsType::Object(_)))
            .map(|e| e.name.as_str())
            .collect();
        let mut seen: HashSet<String> = HashSet::new();

        for (specifier, exports) in result.iter() {
            for export in exports {
                if let TsType::Named(name) = &export.ts_type
                    && !defined_names.contains(name.as_str())
                    && seen.insert(name.clone())
                {
                    types_to_resolve.push((name.clone(), specifier.clone()));
                }
            }
        }

        if types_to_resolve.is_empty() {
            return;
        }

        // Parse .d.ts files to find type/interface definitions
        let mut dts_types: HashMap<String, TsType> = HashMap::new();
        for source_path in import_paths.values() {
            let content = match std::fs::read_to_string(source_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if let Ok(all_types) = super::dts::parse_all_types_from_str(&content) {
                for t in all_types {
                    if matches!(t.ts_type, TsType::Object(_)) {
                        dts_types.entry(t.name).or_insert(t.ts_type);
                    }
                }
            }
        }

        for (type_name, specifier) in types_to_resolve {
            if let Some(ts_type) = dts_types.get(&type_name)
                && let Some(exports) = result.get_mut(&specifier)
                && !exports.iter().any(|e| e.name == type_name)
            {
                exports.push(DtsExport {
                    name: type_name,
                    ts_type: ts_type.clone(),
                });
            }
        }
    }

    /// For object destructuring probes that resolved to `any`, resolve the
    /// function's return type via LSP hover and extract per-field types.
    #[cfg(feature = "native")]
    fn enhance_object_destructure_probes(
        &mut self,
        result: &mut HashMap<String, Vec<DtsExport>>,
        program: &Program,
        ts_imports: &HashMap<String, PathBuf>,
    ) {
        use crate::parser::ast::*;

        // Build import name → source path mapping
        let mut import_paths: HashMap<String, PathBuf> = HashMap::new();
        for item in &program.items {
            if let ItemKind::Import(decl) = &item.kind
                && let Some(ts_path) = ts_imports.get(&decl.source)
            {
                for spec in &decl.specifiers {
                    let name = spec.alias.as_deref().unwrap_or(&spec.name);
                    import_paths.insert(name.to_string(), ts_path.clone());
                }
            }
        }

        // Collect object destructuring calls with unresolved per-field probes
        let mut imported_names: HashMap<String, String> = HashMap::new();
        for item in &program.items {
            if let ItemKind::Import(decl) = &item.kind {
                let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
                if !is_relative || ts_imports.contains_key(&decl.source) {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_deref().unwrap_or(&spec.name);
                        imported_names.insert(name.to_string(), decl.source.clone());
                    }
                }
            }
        }

        struct FieldEnhancement {
            fn_name: String,
            specifier: String,
            field_probes: Vec<(String, String)>,
        }
        let all_consts = probe_gen::collect_all_consts(program);
        let mut to_enhance: Vec<FieldEnhancement> = Vec::new();

        for decl in &all_consts {
            if let ConstBinding::Object(fields) = &decl.binding {
                let inner = probe_gen::unwrap_try_await_expr(&decl.value);
                if let ExprKind::Call { callee, .. } = &inner.kind {
                    let callee_name = probe_gen::expr_to_callee_name(callee);
                    if let Some(fn_name) = callee_name {
                        let root = fn_name.split('.').next().unwrap_or("");
                        if !imported_names.contains_key(&fn_name)
                            && !imported_names.contains_key(root)
                        {
                            continue;
                        }
                        let specifier = imported_names
                            .get(&fn_name)
                            .or_else(|| imported_names.get(root));
                        let Some(specifier) = specifier else {
                            continue;
                        };

                        // Check if any per-field probes are `any`
                        let exports = result.get(specifier);
                        let has_any_field = fields.iter().any(|f| {
                            let probe_prefix = format!("__probe_{}_", f.field);
                            exports.is_some_and(|exps| {
                                exps.iter().any(|e| {
                                    e.name.starts_with(&probe_prefix)
                                        && matches!(e.ts_type, TsType::Any | TsType::Unknown)
                                })
                            })
                        });

                        if has_any_field {
                            let field_probes: Vec<(String, String)> = fields
                                .iter()
                                .map(|f| (f.field.clone(), format!("__probe_{}_", f.field)))
                                .collect();
                            to_enhance.push(FieldEnhancement {
                                fn_name,
                                specifier: specifier.clone(),
                                field_probes,
                            });
                        }
                    }
                }
            }
        }

        if to_enhance.is_empty() {
            return;
        }

        let Some(client) = self.lsp_client() else {
            return;
        };

        for FieldEnhancement {
            fn_name,
            specifier,
            field_probes,
        } in to_enhance
        {
            let root = fn_name.split('.').next().unwrap_or(&fn_name);
            let Some(source_path) = import_paths.get(root) else {
                continue;
            };

            // Query LSP for the function's type
            let Some(hover) = client.query_symbol_type(source_path, root) else {
                continue;
            };
            // Parse hover to extract return type fields
            // Hover format: "function useQuery(...): { data: T, isLoading: boolean, ... }"
            let Some(ret_start) = hover.rfind("): ").or_else(|| hover.rfind(") -> ")) else {
                continue;
            };
            let ret_str = if hover[ret_start..].starts_with("): ") {
                &hover[ret_start + 3..]
            } else {
                &hover[ret_start + 4..]
            };

            // Try to parse the return type as a TS object
            let snippet = format!("export declare let _q: {ret_str};");
            let Ok(ret_exports) = super::dts::parse_dts_exports_from_str(&snippet) else {
                continue;
            };
            let Some(ret_export) = ret_exports.first() else {
                continue;
            };

            // Extract field types from the return type.
            // If the return type is a named/generic type (not inline object),
            // run a secondary tsgo probe with the import + field accesses to expand it.
            let owned_field_types: HashMap<&str, TsType> = match &ret_export.ts_type {
                TsType::Object(fields) => fields
                    .iter()
                    .map(|f| (f.name.as_str(), f.ty.clone()))
                    .collect(),
                TsType::Generic { args, .. } if !args.is_empty() => {
                    // Named generic return type (e.g. UseQueryResult<IssueDto[], Error>).
                    // The first type arg is the data type. data is TData | undefined.
                    let mut map: HashMap<&str, TsType> = HashMap::new();
                    if field_probes.iter().any(|(f, _)| f == "data") {
                        // data: TData | undefined
                        map.insert(
                            "data",
                            TsType::Union(vec![args[0].clone(), TsType::Undefined]),
                        );
                    }
                    if map.is_empty() {
                        continue;
                    }
                    map
                }
                _ => continue,
            };

            // Update per-field probes with resolved types
            if let Some(exports) = result.get_mut(&specifier) {
                for (field_name, probe_prefix) in &field_probes {
                    if let Some(field_ty) = owned_field_types.get(field_name.as_str())
                        && let Some(probe) = exports.iter_mut().find(|e| {
                            e.name.starts_with(probe_prefix)
                                && matches!(e.ts_type, TsType::Any | TsType::Unknown)
                        })
                    {
                        probe.ts_type = field_ty.clone();
                    }
                }
            }
        }
    }

    /// hover at the symbol's declaration site in the source file.
    #[cfg(feature = "native")]
    fn enhance_with_lsp(
        &mut self,
        result: &mut HashMap<String, Vec<DtsExport>>,
        program: &Program,
        ts_imports: &HashMap<String, PathBuf>,
    ) {
        // Collect (specifier, export_index, symbol_name, source_path) for unresolved exports
        let mut to_resolve: Vec<(String, usize, String, PathBuf)> = Vec::new();

        // Build import name → (source specifier, source path) mapping
        // Covers both local .ts imports and npm packages
        let mut import_paths: HashMap<String, PathBuf> = HashMap::new();
        for item in &program.items {
            if let ItemKind::Import(decl) = &item.kind {
                // Local .ts import
                if let Some(ts_path) = ts_imports.get(&decl.source) {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_deref().unwrap_or(&spec.name);
                        import_paths.insert(name.to_string(), ts_path.clone());
                    }
                }
                // npm import — find the package .d.ts
                let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
                if !is_relative
                    && let Some(dts_path) =
                        typeof_resolve::find_package_dts(&self.project_dir, &decl.source)
                {
                    for spec in &decl.specifiers {
                        let name = spec.alias.as_deref().unwrap_or(&spec.name);
                        import_paths
                            .entry(name.to_string())
                            .or_insert(dts_path.clone());
                    }
                }
            }
        }

        for (specifier, exports) in result.iter() {
            for (i, export) in exports.iter().enumerate() {
                // Enhance exports that are Any (type-only, unresolved) or
                // typeof references that weren't resolved by the typeof pass
                if !matches!(export.ts_type, TsType::Any) {
                    if let TsType::Named(ref s) = export.ts_type {
                        if !s.starts_with("typeof ") {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                if let Some(path) = import_paths.get(&export.name) {
                    to_resolve.push((specifier.clone(), i, export.name.clone(), path.clone()));
                }
            }
        }

        if to_resolve.is_empty() {
            return;
        }

        let Some(client) = self.lsp_client() else {
            return;
        };

        for (specifier, idx, symbol_name, source_path) in to_resolve {
            if let Some(hover_text) = client.query_symbol_type(&source_path, &symbol_name)
                && let Some(ts_type) = parse_hover_to_tstype(&hover_text)
                && let Some(exports) = result.get_mut(&specifier)
                && let Some(export) = exports.get_mut(idx)
            {
                export.ts_type = ts_type;
            }
        }
    }
}

/// Parse a hover text string from the LSP into a TsType.
/// Best-effort — hover format may vary across tsgo versions.
/// Currently handles: `function ...`, `type X = ...`, `const x: Type`.
#[cfg(feature = "native")]
fn parse_hover_to_tstype(hover: &str) -> Option<TsType> {
    let hover = hover.trim();

    // "type X = { ... }" or "interface X { ... }"
    if let Some(rest) = hover
        .strip_prefix("type ")
        .and_then(|s| s.split_once('='))
        .map(|(_, rhs)| rhs.trim())
    {
        // Try to parse as a .d.ts snippet
        let snippet = format!("export declare let _q: {rest};");
        if let Ok(exports) = super::dts::parse_dts_exports_from_str(&snippet)
            && let Some(export) = exports.first()
        {
            return Some(export.ts_type.clone());
        }
    }

    // "function name<T>(params): RetType" → parse as function
    if hover.starts_with("function ") {
        let snippet = format!("export declare {hover};");
        if let Ok(exports) = super::dts::parse_dts_exports_from_str(&snippet)
            && let Some(export) = exports.first()
        {
            return Some(export.ts_type.clone());
        }
    }

    // "const x: Type" → extract the type
    if let Some(rest) = hover.strip_prefix("let ")
        && let Some((_, ty_str)) = rest.split_once(':')
    {
        let snippet = format!("export declare let _q: {};", ty_str.trim());
        if let Ok(exports) = super::dts::parse_dts_exports_from_str(&snippet)
            && let Some(export) = exports.first()
        {
            return Some(export.ts_type.clone());
        }
    }

    None
}

/// Find imports that don't resolve to `.fl` files but do resolve to
/// `.ts`/`.tsx` files. Handles both relative imports and tsconfig path aliases.
fn find_relative_ts_imports(
    program: &Program,
    resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
    source_dir: &Path,
    tsconfig_paths: &crate::resolve::TsconfigPaths,
) -> HashMap<String, PathBuf> {
    let mut ts_imports = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            // Skip already-resolved .fl imports
            if resolved_imports.contains_key(&decl.source) {
                continue;
            }

            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            if is_relative {
                if let Some(ts_path) = crate::resolve::resolve_ts_path(source_dir, &decl.source) {
                    ts_imports.insert(decl.source.clone(), ts_path);
                }
            } else if let Some(resolved_path) = tsconfig_paths.resolve(&decl.source) {
                // Tsconfig path alias that resolved to a .ts/.tsx file
                let ext = resolved_path.extension().and_then(|e| e.to_str());
                if matches!(ext, Some("ts" | "tsx")) {
                    ts_imports.insert(decl.source.clone(), resolved_path);
                }
            }
        }
    }
    ts_imports
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    // Re-import sub-module functions needed by tests
    use probe_gen::type_decl_to_ts;

    #[test]
    fn generate_probe_basic_import() {
        let source = r#"import { useState } from "react"
let (count, setCount) = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.contains("import { useState } from \"react\";"));
        // Tuple binding: destructures into _r0_0, _r0_1
        assert!(probe.contains("_tmp0 = useState(0);"));
        assert!(probe.contains("export let [_r0_0, _r0_1] = _tmp0;"));
    }

    #[test]
    fn generate_probe_with_type_args() {
        let source = r#"import { useState } from "react"
type Todo = { text: string }
let (todos, setTodos) = useState<Array<Todo>>([])"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.contains("import { useState } from \"react\";"));
        assert!(probe.contains("type Todo = {"));
        assert!(probe.contains("_tmp0 = useState<Array<Todo>>([]);"));
        assert!(probe.contains("export let [_r0_0, _r0_1] = _tmp0;"));
    }

    #[test]
    fn generate_probe_empty_for_no_npm_imports() {
        let source = r#"import { foo } from "./local"
let x = 42"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_re_exports_unused_imports() {
        let source = r#"import { useState, useEffect } from "react"
let (count, setCount) = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Tuple binding: destructured
        assert!(probe.contains("_tmp0 = useState(0);"));
        // useState has a call probe, so it uses plain re-export
        assert!(
            probe.contains("= useState;"),
            "should re-export useState (plain, has call probe), got:\n{probe}"
        );
        // useEffect has no call probe, so it uses _expand
        assert!(
            probe.contains("_expand(useEffect)"),
            "should re-export useEffect via _expand, got:\n{probe}"
        );
    }

    #[test]
    fn type_decl_to_ts_record() {
        let source = "type Todo = { text: string, done: boolean }";
        let program = Parser::new(source).parse_program().unwrap();
        if let ItemKind::TypeDecl(decl) = &program.items[0].kind {
            let ts = type_decl_to_ts(decl);
            assert!(ts.contains("text: string;"));
            assert!(ts.contains("done: boolean;"));
        } else {
            panic!("expected type decl");
        }
    }

    #[test]
    fn resolve_imports_with_real_react() {
        // Integration test: requires node_modules with react installed
        let todo_app_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/todo-app");
        if !todo_app_dir.join("node_modules").is_dir() {
            eprintln!("Skipping: no node_modules in todo-app");
            return;
        }

        let source = r#"
import { useState } from "react"
type Todo = { text: string, done: bool }
let (todos, setTodos) = useState<Array<Todo>>([])
let (input, setInput) = useState("")
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        eprintln!("PROBE:\n{probe}");
        let mut resolver = TsgoResolver::new(&todo_app_dir);
        let result = resolver.resolve_imports(
            &program,
            &HashMap::new(),
            Path::new("."),
            &crate::resolve::TsconfigPaths::default(),
        );

        eprintln!(
            "tsgo result keys: {:?}",
            result.exports.keys().collect::<Vec<_>>()
        );
        if let Some(react_exports) = result.exports.get("react") {
            for export in react_exports {
                eprintln!("  export: {} -> {:?}", export.name, export.ts_type);
            }
            // Should have useState function type
            assert!(
                react_exports.iter().any(|e| e.name == "useState"),
                "should have useState export"
            );
            // Should have probe results for the calls
            assert!(
                react_exports.iter().any(|e| e.name.starts_with("__probe_")),
                "should have probe call results, got: {:?}",
                react_exports.iter().map(|e| &e.name).collect::<Vec<_>>()
            );
        } else {
            panic!(
                "should have react exports, got keys: {:?}",
                result.exports.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn resolve_imports_union_type_with_usestate() {
        let todo_app_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/todo-app");
        if !todo_app_dir.join("node_modules").is_dir() {
            eprintln!("Skipping: no node_modules in todo-app");
            return;
        }

        let source = r#"
import { useState } from "react"
type Filter = | All | Active | Completed
let (filter, setFilter) = useState<Filter>(Filter.All)
"#;
        let program = Parser::new(source).parse_program().unwrap();

        // Check what probe is generated
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        eprintln!("PROBE:\n{probe}");

        let mut resolver = TsgoResolver::new(&todo_app_dir);
        let result = resolver.resolve_imports(
            &program,
            &HashMap::new(),
            Path::new("."),
            &crate::resolve::TsconfigPaths::default(),
        );

        if let Some(react_exports) = result.exports.get("react") {
            for export in react_exports {
                eprintln!("  export: {} -> {:?}", export.name, export.ts_type);
            }
            // setFilter should be Dispatch<SetStateAction<Filter>>, not Dispatch<unknown>
            let probe = react_exports
                .iter()
                .find(|e| e.name == "__probe_filter_setFilter");
            assert!(probe.is_some(), "should have probe for filter/setFilter");
            let ts_type_str = format!("{:?}", probe.unwrap().ts_type);
            assert!(
                !ts_type_str.contains("unknown"),
                "setFilter should not be unknown, got: {ts_type_str}"
            );
        } else {
            panic!("should have react exports");
        }
    }

    #[test]
    fn type_expr_to_ts_option() {
        let source = "type Foo = { bar: Option<string> }";
        let program = Parser::new(source).parse_program().unwrap();
        if let ItemKind::TypeDecl(decl) = &program.items[0].kind {
            let ts = type_decl_to_ts(decl);
            assert!(ts.contains("FloeOption<string>"));
        } else {
            panic!("expected type decl");
        }
    }

    #[test]
    fn generate_probe_emits_imported_floe_function_stubs() {
        use crate::lexer::span::Span;
        use crate::resolve::ResolvedImports;

        let s = Span::new(0, 0, 0, 0);

        let source = r#"import { fetchProducts } from "./api"
import { useSuspenseQuery } from "@tanstack/react-query"

let test() ={
    let { data } = useSuspenseQuery({
        queryKey: ["products"],
        queryFn: () -> fetchProducts(),
    })
}"#;
        let program = Parser::new(source).parse_program().unwrap();

        // Build resolved imports with a mock fetchProducts function
        let mut resolved = HashMap::new();
        let fetch_fn = FunctionDecl {
            exported: true,
            async_fn: false,
            name: "fetchProducts".to_string(),
            type_params: vec![],
            params: vec![Param {
                name: "category".to_string(),
                type_ann: Some(TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "string".to_string(),
                        type_args: vec![],
                        bounds: vec![],
                    },
                    span: s,
                }),
                default: Some(Expr::synthetic(ExprKind::String("".to_string()), s)),
                destructure: None,
                span: s,
            }],
            return_type: Some(TypeExpr {
                kind: TypeExprKind::Named {
                    name: "Promise".to_string(),
                    type_args: vec![TypeExpr {
                        kind: TypeExprKind::Named {
                            name: "Result".to_string(),
                            type_args: vec![
                                TypeExpr {
                                    kind: TypeExprKind::Tuple(vec![
                                        TypeExpr {
                                            kind: TypeExprKind::Named {
                                                name: "Array".to_string(),
                                                type_args: vec![TypeExpr {
                                                    kind: TypeExprKind::Named {
                                                        name: "Product".to_string(),
                                                        type_args: vec![],
                                                        bounds: vec![],
                                                    },
                                                    span: s,
                                                }],
                                                bounds: vec![],
                                            },
                                            span: s,
                                        },
                                        TypeExpr {
                                            kind: TypeExprKind::Named {
                                                name: "number".to_string(),
                                                type_args: vec![],
                                                bounds: vec![],
                                            },
                                            span: s,
                                        },
                                    ]),
                                    span: s,
                                },
                                TypeExpr {
                                    kind: TypeExprKind::Named {
                                        name: "ApiError".to_string(),
                                        type_args: vec![],
                                        bounds: vec![],
                                    },
                                    span: s,
                                },
                            ],
                            bounds: vec![],
                        },
                        span: s,
                    }],
                    bounds: vec![],
                },
                span: s,
            }),
            body: Box::new(Expr::synthetic(ExprKind::Unit, s)),
        };

        let mut imports = ResolvedImports::default();
        imports.function_decls.push(fetch_fn);
        resolved.insert("./api".to_string(), imports);

        let probe = generate_probe(&program, &resolved, &HashMap::new());

        // Should contain the declare function stub
        assert!(
            probe.contains("declare function fetchProducts(category?: string): Promise<"),
            "probe should emit declare function stub for imported Floe function, got:\n{probe}"
        );
        // Should contain Result<T, E> expansion
        assert!(
            probe.contains("ok: true"),
            "probe should expand Result type, got:\n{probe}"
        );
        // Should NOT contain `declare const fetchProducts: any` (free var fallback)
        assert!(
            !probe.contains("declare let fetchProducts: any"),
            "fetchProducts should not be declared as `any`, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_includes_relative_ts_imports() {
        let source = r#"import { newDate } from "../utils/date"
let year = newDate()"#;
        let program = Parser::new(source).parse_program().unwrap();

        // Simulate a resolved TS path
        let mut ts_imports = HashMap::new();
        ts_imports.insert(
            "../utils/date".to_string(),
            PathBuf::from("/project/src/utils/date.ts"),
        );

        let probe = generate_probe(&program, &HashMap::new(), &ts_imports);

        // Should import using a local filename (symlinked into probe dir)
        assert!(
            probe.contains("import { newDate } from \"./date.ts\";"),
            "probe should use local filename for relative TS import, got:\n{probe}"
        );
        // newDate has a call probe, so it uses plain re-export
        assert!(
            probe.contains("= newDate;"),
            "probe should re-export newDate, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_object_destructure_from_ts_import() {
        let source = r#"import { useQuery } from "../hooks/use-query"
let { data, isLoading } = useQuery("key")"#;
        let program = Parser::new(source).parse_program().unwrap();

        let mut ts_imports = HashMap::new();
        ts_imports.insert(
            "../hooks/use-query".to_string(),
            PathBuf::from("/project/src/hooks/use-query.ts"),
        );

        let probe = generate_probe(&program, &HashMap::new(), &ts_imports);

        // Should use property access instead of destructuring
        assert!(
            probe.contains("_tmp0")
                && probe.contains("_r0_0 = _tmp0.data")
                && probe.contains("_r0_1 = _tmp0.isLoading"),
            "probe should generate per-field property access, got:\n{probe}"
        );
    }

    #[test]
    fn specifier_map_object_destructure_creates_per_field_probes() {
        let source = r#"import { useQuery } from "../hooks/use-query"
let { data, isLoading } = useQuery("key")"#;
        let program = Parser::new(source).parse_program().unwrap();

        let mut ts_imports = HashMap::new();
        ts_imports.insert(
            "../hooks/use-query".to_string(),
            PathBuf::from("/project/src/hooks/use-query.ts"),
        );

        // Simulate tsgo probe exports: _r0_0 = data type, _r0_1 = isLoading type
        // Plus the re-export: _r1 = useQuery itself
        let probe_exports = vec![
            DtsExport {
                name: "_r0_0".to_string(),
                ts_type: TsType::Primitive("string".to_string()),
            },
            DtsExport {
                name: "_r0_1".to_string(),
                ts_type: TsType::Primitive("boolean".to_string()),
            },
            DtsExport {
                name: "_r1".to_string(),
                ts_type: TsType::Function {
                    params: vec![],
                    return_type: Box::new(TsType::Unknown),
                },
            },
        ];

        let result = build_specifier_map(&program, &probe_exports, &ts_imports);
        let exports = result
            .get("../hooks/use-query")
            .expect("should have specifier");
        let names: Vec<&str> = exports.iter().map(|e| e.name.as_str()).collect();

        assert!(
            names.contains(&"__probe_data_0"),
            "should have per-field probe for data, got: {names:?}"
        );
        assert!(
            names.contains(&"__probe_isLoading_0"),
            "should have per-field probe for isLoading, got: {names:?}"
        );
    }

    #[test]
    fn generate_probe_empty_when_only_fl_imports() {
        // Relative imports that resolve to .fl files should not be in the probe
        let source = r#"import { User } from "./types"
let x = 42"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_includes_type_alias_probe() {
        let source = r#"import { tv, VariantProps } from "tailwind-variants"
let spinnerVariants = tv({})
typealias SpinnerProps = VariantProps<typeof spinnerVariants>"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        assert!(
            probe.contains("__tprobe_SpinnerProps"),
            "probe should contain type probe: {probe}"
        );
        assert!(
            probe.contains("VariantProps<typeof spinnerVariants>"),
            "probe should contain the type expression: {probe}"
        );
    }

    #[test]
    fn generate_probe_emits_typeof_const_for_type_probe() {
        let source = r#"import { tv, VariantProps } from "tailwind-variants"
let spinnerVariants = tv({})
typealias SpinnerProps = VariantProps<typeof spinnerVariants>"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        // The probe should declare spinnerVariants so typeof can resolve
        assert!(
            probe.contains("let spinnerVariants = tv("),
            "probe should declare let for typeof resolution: {probe}"
        );
    }

    #[test]
    fn type_probe_not_emitted_for_local_only_alias() {
        let source = r#"import { useState } from "react"
typealias MyNum = number"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        assert!(
            !probe.contains("__tprobe_MyNum"),
            "local-only type alias should not be probed: {probe}"
        );
    }

    #[test]
    fn generate_probe_jsx_callback_prop() {
        let source = r#"import { NavLink } from "react-router-dom"

let page() ={
    <NavLink className={(state) -> "active"} to="/home" />
}"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Should contain the JSX callback helper type
        assert!(
            probe.contains("type _JCB<T>"),
            "probe should contain _JCB helper type, got:\n{probe}"
        );
        // Should contain the JSX callback probe for NavLink's className
        assert!(
            probe.contains("__jsx_NavLink_className"),
            "probe should contain jsx callback probe, got:\n{probe}"
        );
        assert!(
            probe.contains("Parameters<typeof NavLink>[0][\"className\"]"),
            "probe should extract callback param type from component props, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_jsx_no_probe_for_event_handlers() {
        let source = r#"import { Button } from "some-lib"

let page() ={
    <Button onClick={(e) -> handle(e)} />
}"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Should NOT probe event handler props (already handled by event_handler_context)
        assert!(
            !probe.contains("__jsx_Button_onClick"),
            "probe should not contain jsx callback probe for event handlers, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_jsx_no_probe_for_non_arrow_props() {
        let source = r#"import { NavLink } from "react-router-dom"

let page() ={
    <NavLink className="static" to="/home" />
}"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Should NOT probe string props (no arrow function)
        assert!(
            !probe.contains("__jsx_NavLink_className"),
            "probe should not probe non-arrow props, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_jsx_children_render_prop() {
        let source = r#"import { Draggable } from "@hello-pangea/dnd"

let page() ={
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) ->
            <div />
        }
    </Draggable>
}"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Should contain probes for each parameter position
        assert!(
            probe.contains("__jsxc_Draggable_0"),
            "probe should contain children probe for param 0, got:\n{probe}"
        );
        assert!(
            probe.contains("__jsxc_Draggable_1"),
            "probe should contain children probe for param 1, got:\n{probe}"
        );
        // Should reference the children prop via helper pattern
        assert!(
            probe.contains(".children"),
            "probe should extract from children prop, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_inlined_member_call_preserves_type_args() {
        let source = r#"import { useQueryClient } from "@tanstack/react-query"
type IssueDto = { key: string, summary: string }

let Component() ={
    let queryClient = useQueryClient()
    let data = queryClient.getQueryData<Array<IssueDto>>(["issues"])
}"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            probe.contains(".getQueryData<Array<IssueDto>>("),
            "inlined probe should preserve type arguments, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_property_getter_then_method_call() {
        // A chain with a property getter (`.req`) followed by a method call
        // (`.param(...)`) must emit `.req` bare and `.param(null! as any)` as
        // a call so tsgo picks the right overload.
        let source = r#"import trusted { Context } from "hono"

export let handler(c: Context<unknown>) -> string ={
    match c.req.param("code") {
        None -> "missing",
        Some(v) -> v,
    }
}
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // The probe must emit the parameter's full TS annotation so tsgo
        // propagates the concrete generic args through the chain (#1276).
        assert!(
            probe.contains("declare const __chain_base_Context: Context<unknown>;"),
            "probe should declare the Context chain base with its full type annotation, got:\n{probe}"
        );
        // `.req` is a property, NOT a method call — it must NOT get `(null! as any)`
        assert!(
            !probe.contains(".req(null! as any)"),
            "property getter `.req` must not be invoked as a method, got:\n{probe}"
        );
        // The terminal `.param(...)` must be probed as a call so tsgo picks
        // the overload matching a string-typed key argument.
        assert!(
            probe.contains(
                "__chain_call_Context$req$param = __chain_base_Context.req.param(null! as any);"
            ),
            "expected __chain_call_ probe with .req.param(null! as any), got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_threads_registration_path_into_handler_context() {
        // Regression for #1281. When a handler is registered via a pipe
        // chain `|> get("/users/:id", handleUser)`, the probe must emit a
        // per-function chain base with the path threaded into Context's
        // P parameter — otherwise tsgo resolves `c.req.param("id")` via
        // the default `any` param and returns `string | undefined` rather
        // than the narrowed `string`.
        let source = r#"import trusted { router, get, Context } from "hono"

type Bindings = { DB: string }

let handleUser(c: Context<{ Bindings: Bindings }>) -> string = {
    c.req.param("id")
}

export let app = router<Bindings>()
    |> get("/users/:id", handleUser)
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            probe.contains("declare const __chain_base_handleUser__Context: Context<{ Bindings: Bindings }, \"/users/:id\">;"),
            "per-function chain base must thread the registered path, got:\n{probe}"
        );
        assert!(
            probe.contains(
                "export let __chain_call_handleUser__Context$req$param = __chain_base_handleUser__Context.req.param(null! as any);"
            ),
            "per-function chain call probe missing for handler body, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_emits_depth_1_terminal_call_chain() {
        // Regression for #1351. hono's `c.json(body, status)` call chain is
        // only one member step deep — `collect_param_rooted_chains` used to
        // filter it out alongside bare `c.env` reads, and the per-function
        // walker never emitted `__chain_call_Context$json`. Without that
        // probe, tsgo's narrow `JSONRespondReturn<T, S>` return type can't
        // reach the checker. The depth-1 exclusion now applies only to bare
        // member accesses — terminal calls always emit.
        let source = r#"import trusted { router, get, Context } from "hono"

type Bindings = { DB: string }

let handleCreate(c: Context<{ Bindings: Bindings }>) -> string = {
    c.json("hi", 201)
}

export let app = router<Bindings>()
    |> get("/x", handleCreate)
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            probe.contains(
                "export let __chain_call_handleCreate__Context$json = __chain_base_handleCreate__Context.json(null! as any);"
            ),
            "depth-1 terminal-call chain must emit __chain_call_ probe, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_skips_depth_1_bare_member_access() {
        // Complement to the depth-1 terminal-call regression: a bare member
        // read like `c.env` still adds nothing beyond the parent Context
        // probe, so the depth-1 exclusion is preserved for non-call chains.
        let source = r#"import trusted { router, get, Context } from "hono"

type Bindings = { DB: string }

let handler(c: Context<{ Bindings: Bindings }>) -> unknown = {
    c.env
}

export let app = router<Bindings>()
    |> get("/x", handler)
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            !probe.contains("__chain_handler__Context$env"),
            "bare `c.env` should not emit a depth-1 chain probe, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_recognizes_functional_handler_registration() {
        // Regression for #1353. `collect_handler_paths` previously required
        // the path string at arg 0 — fine for hono-native `app.get(path, h)`
        // and for the pipe form `|> get(path, h)` (whose RHS Call still has
        // path at arg 0), but the `@floeorg/hono` wrapper uses functional
        // signatures like `get(router, "/x", handler)` where the path is at
        // arg 1 and the handler at arg 2. Without a match, no
        // `__chain_base_<fn>__<Base>` was emitted, so per-function chain
        // probes never fired for the direct-call style.
        let source = r#"import trusted { router, get, Context } from "hono"

type Bindings = { DB: string }

let handleUser(c: Context<{ Bindings: Bindings }>) -> string = {
    c.req.param("id")
}

export let app = get(router<Bindings>(), "/users/:id", handleUser)
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            probe.contains("declare const __chain_base_handleUser__Context: Context<{ Bindings: Bindings }, \"/users/:id\">;"),
            "functional handler registration should populate the per-function chain base, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_preserves_nested_generic_type_argument() {
        // Regression for #1276. Without the full type annotation in the probe,
        // `c.env.DB` on `Context<{ Bindings: Bindings }>` collapses to the
        // bare `Context` whose default E resolves `env` to `any`. The probe
        // must emit the full annotation so tsgo threads `Bindings` through.
        let source = r#"import trusted { Context } from "hono"

type Bindings = { DB: string }

export let handler(c: Context<{ Bindings: Bindings }>) -> string = {
    c.env.DB
}
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(
            probe.contains("declare const __chain_base_Context: Context<{ Bindings: Bindings }>;"),
            "probe must preserve nested generic type args, got:\n{probe}"
        );
    }
}
