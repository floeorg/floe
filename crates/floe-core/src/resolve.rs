//! Module resolver for cross-file import resolution.
//!
//! Resolves relative `.fl` imports by parsing the imported file and extracting
//! exported symbols (types, functions, for-block functions, consts).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::parser::Parser;
use crate::parser::ast::*;

/// Raw parsed tsconfig.json data (shared between TsconfigPaths and probe tsconfig generation).
pub struct ParsedTsconfig {
    /// Absolute path to the tsconfig.json file
    pub tsconfig_path: PathBuf,
    /// Resolved absolute baseUrl directory
    pub base_url: PathBuf,
    /// Raw compilerOptions.paths object (if present)
    pub paths: Option<serde_json::Map<String, serde_json::Value>>,
}

impl ParsedTsconfig {
    /// Parse the nearest tsconfig.json from a project directory.
    pub fn from_project_dir(project_dir: &Path) -> Option<Self> {
        let tsconfig_path = find_tsconfig_from(project_dir)?;
        let content = std::fs::read_to_string(&tsconfig_path).ok()?;
        let stripped = strip_jsonc_comments(&content);
        let json: serde_json::Value = serde_json::from_str(&stripped).ok()?;

        let tsconfig_dir = tsconfig_path.parent().unwrap_or(project_dir);

        let base_url = json
            .pointer("/compilerOptions/baseUrl")
            .and_then(|v| v.as_str())
            .map(|b| tsconfig_dir.join(b))
            .unwrap_or_else(|| tsconfig_dir.to_path_buf());

        let paths = json
            .pointer("/compilerOptions/paths")
            .and_then(|v| v.as_object())
            .cloned();

        Some(Self {
            tsconfig_path,
            base_url,
            paths,
        })
    }

    /// Format paths and baseUrl as JSON properties for inclusion in a probe tsconfig.
    /// Returns an empty string if no paths are configured.
    pub fn to_probe_json_fragment(&self) -> String {
        let mut parts = String::new();

        // baseUrl as absolute path so it works from the probe temp dir
        let base_url_json =
            serde_json::to_string(&self.base_url.display().to_string()).unwrap_or_default();
        parts.push_str(&format!(",\n    \"baseUrl\": {base_url_json}"));

        if let Some(ref paths_obj) = self.paths {
            // Rewrite path targets to absolute paths
            let mut rewritten = serde_json::Map::new();
            for (pattern, targets) in paths_obj {
                if let Some(arr) = targets.as_array() {
                    let abs_targets: Vec<serde_json::Value> = arr
                        .iter()
                        .filter_map(|t| t.as_str())
                        .map(|t| {
                            serde_json::Value::String(self.base_url.join(t).display().to_string())
                        })
                        .collect();
                    rewritten.insert(pattern.clone(), serde_json::Value::Array(abs_targets));
                }
            }
            parts.push_str(&format!(
                ",\n    \"paths\": {}",
                serde_json::to_string(&rewritten).unwrap_or_default()
            ));
        }

        parts
    }
}

/// Parsed tsconfig.json path aliases.
/// Maps alias prefixes (e.g. "#/") to their target directories.
#[derive(Debug, Clone, Default)]
pub struct TsconfigPaths {
    /// Alias prefix -> resolved base directories (e.g. "#/*" -> ["/abs/path/src"])
    pub mappings: Vec<(String, Vec<PathBuf>)>,
}

impl TsconfigPaths {
    /// Parse path aliases from the nearest tsconfig.json.
    pub fn from_project_dir(project_dir: &Path) -> Self {
        let parsed = match ParsedTsconfig::from_project_dir(project_dir) {
            Some(p) => p,
            None => return Self::default(),
        };

        let paths = match parsed.paths {
            Some(ref map) => map,
            None => return Self::default(),
        };

        let mut mappings = Vec::new();
        for (pattern, targets) in paths {
            let targets = match targets.as_array() {
                Some(arr) => arr,
                None => continue,
            };

            // Strip trailing "*" from pattern (e.g. "#/*" -> "#/")
            let prefix = pattern.trim_end_matches('*');

            let resolved_dirs: Vec<PathBuf> = targets
                .iter()
                .filter_map(|t| t.as_str())
                .map(|t| parsed.base_url.join(t.trim_end_matches('*')))
                .collect();

            if !resolved_dirs.is_empty() {
                mappings.push((prefix.to_string(), resolved_dirs));
            }
        }

        Self { mappings }
    }

    /// Try to resolve a specifier using tsconfig path aliases.
    /// Returns the resolved file path if found.
    pub fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        for (prefix, dirs) in &self.mappings {
            if let Some(rest) = specifier.strip_prefix(prefix) {
                for dir in dirs {
                    let candidate = dir.join(rest);
                    // Try .fl, .ts, .tsx extensions and /index variants
                    for ext in &[".fl", ".ts", ".tsx", "/index.fl", "/index.ts", "/index.tsx"] {
                        let path = PathBuf::from(format!("{}{}", candidate.display(), ext));
                        if path.exists() {
                            return Some(path);
                        }
                    }
                    if candidate.exists() && candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    /// Check if a specifier matches any path alias.
    pub fn matches(&self, specifier: &str) -> bool {
        self.mappings
            .iter()
            .any(|(prefix, _)| specifier.starts_with(prefix))
    }
}

/// Strip comments and trailing commas from JSONC content so it can be parsed by serde_json.
/// Handles `//` line comments, `/* */` block comments, and trailing commas before `}` or `]`.
pub fn strip_jsonc_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(&ch) = chars.peek() {
        if in_string {
            result.push(ch);
            chars.next();
            if ch == '\\' {
                // Skip escaped character
                if let Some(&next) = chars.peek() {
                    result.push(next);
                    chars.next();
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                result.push(ch);
                chars.next();
            }
            '/' => {
                chars.next();
                match chars.peek() {
                    Some('/') => {
                        // Line comment — skip until newline
                        chars.next();
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '\n' {
                                result.push('\n');
                                break;
                            }
                        }
                    }
                    Some('*') => {
                        // Block comment — skip until */
                        chars.next();
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '*' && chars.peek() == Some(&'/') {
                                chars.next();
                                result.push(' ');
                                break;
                            }
                        }
                    }
                    _ => {
                        result.push('/');
                    }
                }
            }
            _ => {
                result.push(ch);
                chars.next();
            }
        }
    }

    // Remove trailing commas before } or ]
    let mut cleaned = String::with_capacity(result.len());
    let result_chars: Vec<char> = result.chars().collect();
    let len = result_chars.len();
    let mut i = 0;
    while i < len {
        if result_chars[i] == ',' {
            // Look ahead past whitespace for } or ]
            let mut j = i + 1;
            while j < len && result_chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (result_chars[j] == '}' || result_chars[j] == ']') {
                // Skip the trailing comma
                i += 1;
                continue;
            }
        }
        cleaned.push(result_chars[i]);
        i += 1;
    }
    cleaned
}

/// Find the nearest tsconfig.json by walking up from a directory.
pub fn find_tsconfig_from(dir: &Path) -> Option<PathBuf> {
    let mut current = dir.to_path_buf();
    loop {
        let tsconfig = current.join("tsconfig.json");
        if tsconfig.exists() {
            return Some(tsconfig);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Symbols exported from a resolved module.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ResolvedImports {
    /// Exported type declarations: name -> TypeDecl
    pub type_decls: Vec<TypeDecl>,
    /// Exported function declarations: name -> FunctionDecl
    pub function_decls: Vec<FunctionDecl>,
    /// Exported for-block declarations
    pub for_blocks: Vec<ForBlock>,
    /// Exported const names (we just need to know they exist, typed as Unknown)
    pub const_names: Vec<String>,
    /// Exported trait declarations
    pub trait_decls: Vec<TraitDecl>,
    /// npm type names referenced in exported signatures (transitively needed)
    pub foreign_type_names: Vec<String>,
    /// npm/external import declarations from the resolved .fl file.
    /// Used by probe_gen to emit the imports needed to resolve structural type
    /// aliases (e.g. `type Database = DrizzleD1Database<Schema>` needs
    /// `drizzle-orm` imported).
    pub npm_imports: Vec<ImportDecl>,
}

/// Resolve all relative imports for a given file.
///
/// Returns a map from import source (e.g., `"./types"`) to its resolved symbols.
/// Non-relative imports (npm packages) are skipped, unless they match a tsconfig path alias.
/// Circular imports are handled by tracking visited files.
pub fn resolve_imports(
    file_path: &Path,
    program: &Program,
    tsconfig_paths: &TsconfigPaths,
) -> HashMap<String, ResolvedImports> {
    resolve_imports_with_paths(file_path, program, tsconfig_paths).0
}

/// Like `resolve_imports` but also returns every `.fl` file path that
/// was traversed during resolution. The caller (typically the build
/// cache) uses the path list to fingerprint dependencies for
/// invalidation — without it, edits to an indirectly-imported file
/// would be served stale from cache.
/// Per-file cache of resolved exports keyed by (canonical_path, source_hash).
/// Pass to `resolve_imports_cached` to skip re-parsing unchanged deps.
pub type ResolveCache = HashMap<PathBuf, (u64, ResolvedImports)>;

/// Like `resolve_imports_with_paths` but checks `cache` before parsing
/// each dependency. Unchanged deps skip re-parsing entirely. The cache
/// is updated with fresh entries for deps that were re-parsed.
pub fn resolve_imports_cached(
    file_path: &Path,
    program: &Program,
    tsconfig_paths: &TsconfigPaths,
    cache: &mut ResolveCache,
) -> (HashMap<String, ResolvedImports>, HashSet<PathBuf>) {
    resolve_imports_inner_with_cache(file_path, program, tsconfig_paths, Some(cache))
}

pub fn resolve_imports_with_paths(
    file_path: &Path,
    program: &Program,
    tsconfig_paths: &TsconfigPaths,
) -> (HashMap<String, ResolvedImports>, HashSet<PathBuf>) {
    resolve_imports_inner_with_cache(file_path, program, tsconfig_paths, None)
}

fn resolve_imports_inner_with_cache(
    file_path: &Path,
    program: &Program,
    tsconfig_paths: &TsconfigPaths,
    mut cache: Option<&mut ResolveCache>,
) -> (HashMap<String, ResolvedImports>, HashSet<PathBuf>) {
    let mut results = HashMap::new();
    let mut visited = HashSet::new();

    // The file's own path should be in the visited set to prevent circular imports
    if let Ok(canonical) = file_path.canonicalize() {
        visited.insert(canonical);
    } else {
        visited.insert(file_path.to_path_buf());
    }

    let base_dir = file_path.parent().unwrap_or(Path::new("."));

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            // Already resolved this source
            if results.contains_key(&decl.source) {
                continue;
            }

            let is_relative = decl.source.starts_with('.');

            if is_relative {
                if let Some(resolved) = resolve_single_cached(
                    base_dir,
                    &decl.source,
                    &mut visited,
                    tsconfig_paths,
                    cache.as_deref_mut(),
                ) {
                    results.insert(decl.source.clone(), resolved);
                }
            } else if let Some(resolved_path) = tsconfig_paths.resolve(&decl.source) {
                // Tsconfig path alias that resolved to a .fl file
                if resolved_path.extension().is_some_and(|e| e == "fl")
                    && let Some(stem) = resolved_path.file_stem()
                {
                    let alias_dir = resolved_path.parent().unwrap_or(Path::new("."));
                    let relative_source = format!("./{}", stem.to_string_lossy());
                    if let Some(resolved) = resolve_single_cached(
                        alias_dir,
                        &relative_source,
                        &mut visited,
                        tsconfig_paths,
                        cache.as_deref_mut(),
                    ) {
                        results.insert(decl.source.clone(), resolved);
                    }
                }
                // .ts/.tsx aliases are handled by tsgo, not here
            }
        }
    }

    // Don't include the root file itself — the caller fingerprints that
    // separately via the top-level source hash.
    if let Ok(canonical) = file_path.canonicalize() {
        visited.remove(&canonical);
    } else {
        visited.remove(file_path);
    }
    (results, visited)
}

/// Try the cache before falling through to `resolve_single_import`.
/// Reads the dep file, hashes it, and returns the cached exports when
/// the hash matches. On a miss, delegates to the full resolve and writes
/// the fresh result back into the cache.
fn resolve_single_cached(
    base_dir: &Path,
    source: &str,
    visited: &mut HashSet<PathBuf>,
    tsconfig_paths: &TsconfigPaths,
    cache: Option<&mut ResolveCache>,
) -> Option<ResolvedImports> {
    if let Some(cache) = cache {
        // Resolve the path without parsing to check the cache.
        if let Some(dep_path) = resolve_path(base_dir, source) {
            let canonical = dep_path.canonicalize().unwrap_or(dep_path.clone());
            if let Ok(dep_bytes) = std::fs::read(&canonical) {
                let hash = crate::build::cache::ModuleInterface::fingerprint(&dep_bytes);
                if let Some((cached_hash, cached_imports)) = cache.get(&canonical)
                    && *cached_hash == hash
                {
                    visited.insert(canonical);
                    return Some(cached_imports.clone());
                }
                // Cache miss — parse the already-read bytes (no second read).
                let source_code = String::from_utf8(dep_bytes).ok()?;
                let resolved =
                    resolve_from_source(&dep_path, &source_code, visited, tsconfig_paths)?;
                cache.insert(canonical, (hash, resolved.clone()));
                return Some(resolved);
            }
        }
    }
    resolve_single_import(base_dir, source, visited, tsconfig_paths)
}

/// Resolve a single import source to its exported symbols.
fn resolve_single_import(
    base_dir: &Path,
    source: &str,
    visited: &mut HashSet<PathBuf>,
    tsconfig_paths: &TsconfigPaths,
) -> Option<ResolvedImports> {
    let resolved_path = resolve_path(base_dir, source)?;

    // Check for circular imports
    let canonical = resolved_path
        .canonicalize()
        .unwrap_or(resolved_path.clone());
    if visited.contains(&canonical) {
        return Some(ResolvedImports::default());
    }
    visited.insert(canonical);

    let source_code = std::fs::read_to_string(&resolved_path).ok()?;
    resolve_from_source(&resolved_path, &source_code, visited, tsconfig_paths)
}

/// Core: given a dep's path and already-read source, parse and extract
/// its exported symbols. Shared by `resolve_single_import` (reads from
/// disk) and `resolve_single_cached` (reuses already-hashed bytes).
fn resolve_from_source(
    resolved_path: &Path,
    source_code: &str,
    visited: &mut HashSet<PathBuf>,
    tsconfig_paths: &TsconfigPaths,
) -> Option<ResolvedImports> {
    let program = Parser::new(source_code).parse_program().ok()?;

    let mut imports = ResolvedImports::default();

    // Also resolve transitive imports from the imported file
    let transitive_dir = resolved_path.parent().unwrap_or(Path::new("."));
    let transitive = resolve_imports_inner(transitive_dir, &program, visited, tsconfig_paths);

    // Collect transitive type decls, for-blocks, and trait decls so the checker can register them
    for resolved in transitive.values() {
        imports
            .type_decls
            .extend(resolved.type_decls.iter().cloned());
        imports
            .for_blocks
            .extend(resolved.for_blocks.iter().cloned());
        imports
            .function_decls
            .extend(resolved.function_decls.iter().cloned());
        imports
            .trait_decls
            .extend(resolved.trait_decls.iter().cloned());
    }

    // Collect ALL type decls (exported and non-exported) to build a type map
    // for resolving spreads within this file's scope.
    let mut all_type_decls: Vec<TypeDecl> = Vec::new();
    for item in &program.items {
        if let ItemKind::TypeDecl(decl) = &item.kind {
            all_type_decls.push(decl.clone());
        }
    }

    // Include transitive types in the type map so spreads referencing
    // re-exported types can also be resolved.
    let mut type_map: HashMap<String, TypeDecl> = HashMap::new();
    for decl in &imports.type_decls {
        type_map.insert(decl.name.clone(), decl.clone());
    }
    for decl in &all_type_decls {
        type_map.insert(decl.name.clone(), decl.clone());
    }

    // Flatten spreads in all type decls so importers get fully resolved records.
    let flattened_map = flatten_spreads_in_type_decls(&type_map);

    // Collect exported for-blocks first so we can find their type dependencies
    let mut exported_for_blocks = Vec::new();
    for item in &program.items {
        if let ItemKind::ForBlock(block) = &item.kind {
            let mut exported_block = block.clone();
            exported_block.functions.retain(|f| f.exported);
            if !exported_block.functions.is_empty() {
                exported_for_blocks.push(exported_block);
            }
        }
    }

    // Collect type names referenced in exported signatures and type declarations
    let mut referenced_types: HashSet<String> = HashSet::new();
    for block in &exported_for_blocks {
        for func in &block.functions {
            collect_fn_type_names(func, &mut referenced_types);
        }
    }
    for item in &program.items {
        match &item.kind {
            ItemKind::Function(func) if func.exported => {
                collect_fn_type_names(func, &mut referenced_types);
            }
            ItemKind::TypeDecl(decl) if decl.exported => {
                collect_type_decl_type_names(decl, &mut referenced_types);
            }
            _ => {}
        }
    }

    // Track exported type names so we don't duplicate them
    let mut exported_type_names: HashSet<String> = HashSet::new();

    for item in &program.items {
        match &item.kind {
            ItemKind::TypeDecl(decl) if decl.exported => {
                exported_type_names.insert(decl.name.clone());
                // Use the flattened version if available
                if let Some(flattened) = flattened_map.get(&decl.name) {
                    imports.type_decls.push(flattened.clone());
                } else {
                    imports.type_decls.push(decl.clone());
                }
            }
            ItemKind::Function(decl) if decl.exported => {
                imports.function_decls.push(decl.clone());
            }
            ItemKind::TraitDecl(decl) if decl.exported => {
                imports.trait_decls.push(decl.clone());
            }
            ItemKind::Const(decl) if decl.exported => {
                if let ConstBinding::Name(name) = &decl.binding {
                    imports.const_names.push(name.clone());
                }
            }
            _ => {}
        }
    }

    imports.for_blocks.extend(exported_for_blocks);

    // Include non-exported types that are referenced by exported signatures
    for decl in &all_type_decls {
        if !exported_type_names.contains(&decl.name) && referenced_types.contains(&decl.name) {
            if let Some(flattened) = flattened_map.get(&decl.name) {
                imports.type_decls.push(flattened.clone());
            } else {
                imports.type_decls.push(decl.clone());
            }
        }
    }

    // Identify foreign (npm) type names: referenced in exported signatures
    // but not defined locally as type declarations or builtins
    let all_local_type_names: HashSet<&str> =
        all_type_decls.iter().map(|d| d.name.as_str()).collect();
    for name in &referenced_types {
        if !all_local_type_names.contains(name.as_str())
            && !crate::type_layout::is_builtin_type(name)
            && !exported_type_names.contains(name)
        {
            imports.foreign_type_names.push(name.clone());
        }
    }

    // Collect npm/external import declarations so probe_gen can emit them when
    // resolving structural type aliases (e.g. `type Database = DrizzleD1Database<Schema>`
    // needs the drizzle-orm import to be available in the TypeScript probe).
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with('.');
            if !is_relative {
                imports.npm_imports.push(decl.clone());
            }
        }
    }

    Some(imports)
}

/// Internal version that accepts a mutable visited set for transitive resolution.
fn resolve_imports_inner(
    base_dir: &Path,
    program: &Program,
    visited: &mut HashSet<PathBuf>,
    tsconfig_paths: &TsconfigPaths,
) -> HashMap<String, ResolvedImports> {
    let mut results = HashMap::new();

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            if !decl.source.starts_with('.') {
                continue;
            }
            if results.contains_key(&decl.source) {
                continue;
            }
            if let Some(resolved) =
                resolve_single_import(base_dir, &decl.source, visited, tsconfig_paths)
            {
                results.insert(decl.source.clone(), resolved);
            }
        }
    }

    results
}

/// Flatten record type spreads in a set of type declarations.
///
/// This resolves `...OtherType` entries in record types by looking up the
/// spread target in the provided type map and inlining its fields. This
/// ensures that exported types have no unresolved spread references, so
/// importers don't need the spread target in their own scope.
fn flatten_spreads_in_type_decls(
    type_map: &HashMap<String, TypeDecl>,
) -> HashMap<String, TypeDecl> {
    let mut result = HashMap::new();

    for (name, decl) in type_map {
        let entries = match &decl.def {
            TypeDef::Record(entries) => entries,
            _ => continue,
        };

        let has_spreads = entries.iter().any(|e| matches!(e, RecordEntry::Spread(_)));
        if !has_spreads {
            continue;
        }

        let mut flat_fields: Vec<RecordEntry> = Vec::new();

        for entry in entries {
            match entry {
                RecordEntry::Field(_) => {
                    flat_fields.push(entry.clone());
                }
                RecordEntry::Spread(spread) => {
                    // Look up the spread target type in the file's type map
                    if let Some(target_decl) = type_map.get(&spread.type_name)
                        && let TypeDef::Record(target_entries) = &target_decl.def
                    {
                        // Recursively flatten the target if it also has spreads
                        let resolved_entries =
                            resolve_spread_entries(target_entries, type_map, &mut HashSet::new());
                        for target_entry in resolved_entries {
                            if let RecordEntry::Field(_) = &target_entry {
                                flat_fields.push(target_entry);
                            }
                        }
                    }
                    // Unknown spread targets are left out; the checker will report the error
                }
            }
        }

        let mut flattened = decl.clone();
        flattened.def = TypeDef::Record(flat_fields);
        result.insert(name.clone(), flattened);
    }

    result
}

/// Recursively resolve spread entries, handling chains like A spreads B spreads C.
fn resolve_spread_entries(
    entries: &[RecordEntry],
    type_map: &HashMap<String, TypeDecl>,
    visited: &mut HashSet<String>,
) -> Vec<RecordEntry> {
    let mut result = Vec::new();

    for entry in entries {
        match entry {
            RecordEntry::Field(_) => {
                result.push(entry.clone());
            }
            RecordEntry::Spread(spread) => {
                // Guard against circular spreads
                if visited.contains(&spread.type_name) {
                    continue;
                }
                visited.insert(spread.type_name.clone());

                if let Some(target_decl) = type_map.get(&spread.type_name)
                    && let TypeDef::Record(target_entries) = &target_decl.def
                {
                    let resolved = resolve_spread_entries(target_entries, type_map, visited);
                    result.extend(resolved);
                }

                visited.remove(&spread.type_name);
            }
        }
    }

    result
}

/// Resolve a relative import path to a TypeScript file.
/// Tries `source.ts`, `source.tsx`, `source/index.ts`, `source/index.tsx`.
/// Used when the import doesn't resolve to a `.fl` file.
pub fn resolve_ts_path(base_dir: &Path, source: &str) -> Option<PathBuf> {
    let relative = PathBuf::from(source);
    let base = base_dir.join(&relative);

    for ext in &["ts", "tsx"] {
        let candidate = base.with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // Try source/index.ts, source/index.tsx
    for ext in &["ts", "tsx"] {
        let candidate = base.join("index").with_extension(ext);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Resolve a relative import path to an actual file path.
/// Tries `source.fl` first, then `source/index.fl`.
fn resolve_path(base_dir: &Path, source: &str) -> Option<PathBuf> {
    let relative = PathBuf::from(source);
    let candidate = base_dir.join(&relative).with_extension("fl");
    if candidate.is_file() {
        return Some(candidate);
    }

    // Try source/index.fl
    let index_candidate = base_dir.join(&relative).join("index.fl");
    if index_candidate.is_file() {
        return Some(index_candidate);
    }

    None
}

/// Collect type names from a function's parameter and return type annotations.
fn collect_type_decl_type_names(decl: &TypeDecl, names: &mut HashSet<String>) {
    match &decl.def {
        TypeDef::Record(fields) => {
            for entry in fields {
                if let RecordEntry::Field(field) = entry {
                    collect_type_names(&field.type_ann, names);
                }
            }
        }
        TypeDef::Union(variants) => {
            for variant in variants {
                for field in &variant.fields {
                    collect_type_names(&field.type_ann, names);
                }
            }
        }
        TypeDef::Alias(type_expr) => {
            collect_type_names(type_expr, names);
        }
        TypeDef::StringLiteralUnion(_) => {}
    }
}

fn collect_fn_type_names(func: &FunctionDecl, names: &mut HashSet<String>) {
    for param in &func.params {
        if let Some(type_ann) = &param.type_ann {
            collect_type_names(type_ann, names);
        }
    }
    if let Some(ret) = &func.return_type {
        collect_type_names(ret, names);
    }
}

/// Recursively collect named type references from a type expression.
fn collect_type_names(type_expr: &TypeExpr, names: &mut HashSet<String>) {
    match &type_expr.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            names.insert(name.clone());
            for arg in type_args {
                collect_type_names(arg, names);
            }
        }
        TypeExprKind::Array(inner) => collect_type_names(inner, names),
        TypeExprKind::Tuple(parts) => {
            for part in parts {
                collect_type_names(part, names);
            }
        }
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            for param in params {
                collect_type_names(param, names);
            }
            collect_type_names(return_type, names);
        }
        TypeExprKind::Record(fields) => {
            for field in fields {
                collect_type_names(&field.type_ann, names);
            }
        }
        TypeExprKind::TypeOf(name) => {
            // typeof references a value binding, not a type — but the root name
            // should still be tracked for import resolution
            let root = name.split('.').next().unwrap_or(name);
            names.insert(root.to_string());
        }
        TypeExprKind::Intersection(types) => {
            for ty in types {
                collect_type_names(ty, names);
            }
        }
        TypeExprKind::StringLiteral(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a temp dir, write files, and return (TempDir, base_path).
    fn setup_files(files: &[(&str, &str)]) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, content).unwrap();
        }
        let base = dir.path().to_path_buf();
        (dir, base)
    }

    /// Helper: parse a source string into a Program.
    fn parse_program(source: &str) -> Program {
        Parser::new(source).parse_program().unwrap()
    }

    // ── Path resolution ───────────────────────────────────────────

    #[test]
    fn resolve_path_fl_suffix() {
        let (_dir, base) = setup_files(&[("types.fl", "let x = 1")]);
        let result = resolve_path(&base, "./types");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("types.fl"));
    }

    #[test]
    fn resolve_path_index_fallback() {
        let (_dir, base) = setup_files(&[("utils/index.fl", "let y = 2")]);
        let result = resolve_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("index.fl"));
    }

    #[test]
    fn resolve_path_prefer_fl_over_index() {
        let (_dir, base) = setup_files(&[("mod.fl", "let a = 1"), ("mod/index.fl", "let b = 2")]);
        let result = resolve_path(&base, "./mod");
        assert!(result.is_some());
        // Should prefer mod.fl
        assert!(result.unwrap().ends_with("mod.fl"));
    }

    #[test]
    fn resolve_path_missing_file() {
        let (_dir, base) = setup_files(&[]);
        let result = resolve_path(&base, "./nonexistent");
        assert!(result.is_none());
    }

    // ── Empty / no-import programs ────────────────────────────────

    #[test]
    fn empty_program_no_imports() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        assert!(result.is_empty());
    }

    #[test]
    fn program_without_imports() {
        let (_dir, base) = setup_files(&[("main.fl", "let x = 1")]);
        let main_path = base.join("main.fl");
        let program = parse_program("let x = 1");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        assert!(result.is_empty());
    }

    // ── npm imports skipped ───────────────────────────────────────

    #[test]
    fn npm_imports_skipped() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { useState } from \"react\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        assert!(result.is_empty());
    }

    // ── Exported types, functions, consts extracted ────────────────

    #[test]
    fn exported_type_extracted() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            ("types.fl", "export type User = { name: string }"),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { User } from \"./types\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./types").unwrap();
        assert_eq!(resolved.type_decls.len(), 1);
        assert_eq!(resolved.type_decls[0].name, "User");
    }

    #[test]
    fn exported_function_extracted() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            ("helpers.fl", "export let greet = () => { 1 }"),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { greet } from \"./helpers\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./helpers").unwrap();
        assert_eq!(resolved.function_decls.len(), 1);
        assert_eq!(resolved.function_decls[0].name, "greet");
    }

    #[test]
    fn exported_const_extracted() {
        let (_dir, base) = setup_files(&[("main.fl", ""), ("config.fl", "export let MAX = 100")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { MAX } from \"./config\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./config").unwrap();
        assert_eq!(resolved.const_names, vec!["MAX".to_string()]);
    }

    // ── Non-exported items excluded ───────────────────────────────

    #[test]
    fn non_exported_items_excluded() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "internal.fl",
                "type Secret = { key: string }\nfn helper() { 1 }\nconst private = 42",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Secret } from \"./internal\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./internal").unwrap();
        assert!(resolved.type_decls.is_empty());
        assert!(resolved.function_decls.is_empty());
        assert!(resolved.const_names.is_empty());
    }

    // ── Multiple imports from same module ─────────────────────────

    #[test]
    fn multiple_imports_same_module() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "lib.fl",
                "export type A = { x: number }\nexport let b = () => { 1 }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { A } from \"./lib\"\nimport { b } from \"./lib\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        // Should only resolve once
        assert_eq!(result.len(), 1);
        let resolved = result.get("./lib").unwrap();
        assert!(!resolved.type_decls.is_empty());
        assert!(!resolved.function_decls.is_empty());
    }

    // ── For-block exports ─────────────────────────────────────────

    #[test]
    fn for_block_exported_functions() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "ext.fl",
                "for User { export fn greet(self) => string { self.name } }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { for User } from \"./ext\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./ext").unwrap();
        // The exported for-block function should be present
        assert!(!resolved.for_blocks.is_empty());
        assert_eq!(resolved.for_blocks[0].functions.len(), 1);
    }

    #[test]
    fn block_level_export_for_exports_all_methods() {
        // `export for X { ... }` should export every method in the block,
        // matching per-method `export fn` behavior.
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "ext.fl",
                "export for User {\n    fn greet(self) => string { self.name }\n    fn shout(self) => string { self.name }\n}",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { for User } from \"./ext\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./ext").unwrap();
        assert_eq!(resolved.for_blocks.len(), 1);
        assert_eq!(resolved.for_blocks[0].functions.len(), 2);
        assert!(resolved.for_blocks[0].functions.iter().all(|f| f.exported));
    }

    // ── Missing files handled gracefully ──────────────────────────

    #[test]
    fn missing_import_file_no_panic() {
        let (_dir, base) = setup_files(&[("main.fl", "")]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { foo } from \"./missing\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        // Missing file should not appear in results
        assert!(!result.contains_key("./missing"));
    }

    // ── Edge cases ────────────────────────────────────────────────

    #[test]
    fn dotted_relative_import() {
        let (_dir, base) = setup_files(&[("sub/main.fl", ""), ("lib.fl", "export let X = 1")]);
        let main_path = base.join("sub/main.fl");
        let program = parse_program("import { X } from \"../lib\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        assert!(result.contains_key("../lib"));
    }

    #[test]
    fn exported_trait_extracted() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "traits.fl",
                "export trait Display { fn show(self) => string }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Display } from \"./traits\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./traits").unwrap();
        assert_eq!(resolved.trait_decls.len(), 1);
        assert_eq!(resolved.trait_decls[0].name, "Display");
    }

    #[test]
    fn resolve_index_in_subdir() {
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            ("components/index.fl", "export let Button = () => { 1 }"),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Button } from \"./components\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./components").unwrap();
        assert_eq!(resolved.function_decls.len(), 1);
    }

    // ── Spread flattening during import ──────────────────────────

    #[test]
    fn spread_flattened_on_export() {
        // WithRating is not exported, but Product uses ...WithRating.
        // The resolver should flatten the spread so importers get a flat record.
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "types.fl",
                "type WithRating = { rating: number }\nexport type Product = { ...WithRating, title: string }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { Product } from \"./types\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./types").unwrap();

        // Product should be present
        assert_eq!(resolved.type_decls.len(), 1);
        let product = &resolved.type_decls[0];
        assert_eq!(product.name, "Product");

        // The record should have no spreads — they should be flattened to fields
        if let TypeDef::Record(entries) = &product.def {
            for entry in entries {
                assert!(
                    matches!(entry, RecordEntry::Field(_)),
                    "expected all entries to be fields after flattening, but found a spread"
                );
            }
            // Should have both rating and title fields
            let field_names: Vec<&str> = entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| f.name.as_str()))
                .collect();
            assert!(
                field_names.contains(&"rating"),
                "expected flattened field 'rating', got: {:?}",
                field_names
            );
            assert!(
                field_names.contains(&"title"),
                "expected field 'title', got: {:?}",
                field_names
            );
        } else {
            panic!("expected Product to be a Record type");
        }
    }

    #[test]
    fn spread_chain_flattened_on_export() {
        // A spreads into B, B spreads into C — C should be fully flat.
        let (_dir, base) = setup_files(&[
            ("main.fl", ""),
            (
                "types.fl",
                "type A = { x: number }\ntype B = { ...A, y: string }\nexport type C = { ...B, z: boolean }",
            ),
        ]);
        let main_path = base.join("main.fl");
        let program = parse_program("import { C } from \"./types\"");
        let result = resolve_imports(&main_path, &program, &TsconfigPaths::default());
        let resolved = result.get("./types").unwrap();

        let c_decl = &resolved.type_decls[0];
        assert_eq!(c_decl.name, "C");

        if let TypeDef::Record(entries) = &c_decl.def {
            let field_names: Vec<&str> = entries
                .iter()
                .filter_map(|e| e.as_field().map(|f| f.name.as_str()))
                .collect();
            assert_eq!(
                field_names.len(),
                3,
                "expected 3 fields, got: {:?}",
                field_names
            );
            assert!(field_names.contains(&"x"));
            assert!(field_names.contains(&"y"));
            assert!(field_names.contains(&"z"));
        } else {
            panic!("expected C to be a Record type");
        }
    }

    // ── TypeScript path resolution ───────────────────────────────

    #[test]
    fn resolve_ts_path_ts_suffix() {
        let (_dir, base) = setup_files(&[("utils.ts", "export function foo() {}")]);
        let result = resolve_ts_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("utils.ts"));
    }

    #[test]
    fn resolve_ts_path_tsx_suffix() {
        let (_dir, base) = setup_files(&[("Button.tsx", "export function Button() {}")]);
        let result = resolve_ts_path(&base, "./Button");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("Button.tsx"));
    }

    #[test]
    fn resolve_ts_path_prefers_ts_over_tsx() {
        let (_dir, base) = setup_files(&[
            ("mod.ts", "export let a = 1"),
            ("mod.tsx", "export let b = 2"),
        ]);
        let result = resolve_ts_path(&base, "./mod");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("mod.ts"));
    }

    #[test]
    fn resolve_ts_path_index_fallback() {
        let (_dir, base) = setup_files(&[("utils/index.ts", "export let x = 1")]);
        let result = resolve_ts_path(&base, "./utils");
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("index.ts"));
    }

    #[test]
    fn resolve_ts_path_missing() {
        let (_dir, base) = setup_files(&[]);
        let result = resolve_ts_path(&base, "./nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn resolve_ts_path_not_used_when_fl_exists() {
        // When both .fl and .ts exist, resolve_path should find .fl
        // and resolve_ts_path should find .ts — but the resolver
        // only calls resolve_ts_path when .fl resolution fails.
        let (_dir, base) = setup_files(&[
            ("types.fl", "export type X = { y: number }"),
            ("types.ts", "export type X = { y: number }"),
        ]);
        let fl_result = resolve_path(&base, "./types");
        let ts_result = resolve_ts_path(&base, "./types");
        assert!(fl_result.unwrap().ends_with("types.fl"));
        assert!(ts_result.unwrap().ends_with("types.ts"));
    }
}
