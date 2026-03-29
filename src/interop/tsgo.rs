//! tsgo-based type resolution for npm imports.
//!
//! Instead of manually converting TsType -> checker Type, we generate a
//! TypeScript "probe" file that re-exports imported symbols with concrete
//! type arguments, run tsgo (TypeScript's Go-based compiler) to emit a
//! `.d.ts`, and parse the fully-resolved types from the output.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::parser::ast::*;

use super::DtsExport;
use super::TsType;
use super::dts::parse_dts_exports_from_str;

/// Resolves npm import types by running tsgo on a generated probe file.
pub struct TsgoResolver {
    project_dir: PathBuf,
    cache: HashMap<u64, Vec<DtsExport>>,
}

impl TsgoResolver {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
            cache: HashMap::new(),
        }
    }

    /// Resolve npm and local TypeScript imports in a program by generating a
    /// probe file, running tsgo, and parsing the output `.d.ts`.
    ///
    /// `source_dir` is the directory of the `.fl` file being compiled, used to
    /// resolve relative imports to local `.ts`/`.tsx` files.
    ///
    /// Returns a map from specifier (npm or relative) to its resolved exports.
    pub fn resolve_imports(
        &mut self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
        source_dir: &Path,
        tsconfig_paths: &crate::resolve::TsconfigPaths,
    ) -> HashMap<String, Vec<DtsExport>> {
        // Find imports that resolved to .ts/.tsx (not .fl)
        let ts_imports =
            find_relative_ts_imports(program, resolved_imports, source_dir, tsconfig_paths);

        let probe = generate_probe(program, resolved_imports, &ts_imports);
        if probe.is_empty() {
            return HashMap::new();
        }

        // Check cache by content hash
        let hash = {
            let mut hasher = DefaultHasher::new();
            probe.hash(&mut hasher);
            hasher.finish()
        };
        if let Some(cached) = self.cache.get(&hash) {
            // Reconstruct the specifier map from cached exports
            return build_specifier_map(program, cached, &ts_imports);
        }

        // Create temp directory with probe file, tsconfig, and symlinked local TS files
        let tmp = match create_probe_dir(&self.project_dir, &probe, &ts_imports) {
            Ok(dir) => dir,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to create probe dir: {e}");
                return HashMap::new();
            }
        };

        let probe_dir = tmp.path();

        // Run tsgo
        let dts_content = match run_tsgo(probe_dir) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("[floe] tsgo: {e}");
                return HashMap::new();
            }
        };

        if std::env::var("FLOE_DEBUG_PROBE").is_ok() {
            eprintln!("[floe] DTS OUTPUT:\n{dts_content}");
        }

        let exports = match parse_dts_exports_from_str(&dts_content) {
            Ok(exports) => exports,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to parse output: {e}");
                return HashMap::new();
            }
        };

        // Cache the result
        self.cache.insert(hash, exports.clone());

        let mut result = build_specifier_map(program, &exports, &ts_imports);

        // Post-process: resolve `typeof X` types against the original package .d.ts files
        resolve_typeof_types(&mut result, &self.project_dir, program);

        result
    }
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

/// Information about a const declaration that calls an imported function.
struct ProbeCall {
    /// Index for the probe variable name: `_r0`, `_r1`, etc.
    index: usize,
    /// The callee name (e.g. "useState")
    callee: String,
    /// Type arguments as TypeScript strings
    type_args: Vec<String>,
    /// Arguments as TypeScript expression strings
    args: Vec<String>,
    /// The const binding (for mapping back to variable names)
    #[allow(dead_code)]
    binding: ConstBinding,
}

/// Information about a plain re-export from an npm import.
struct ProbeReexport {
    /// Index for the probe variable name
    index: usize,
    /// The imported symbol name
    name: String,
}

/// Collect all const declarations from a program, including those inside function bodies.
fn collect_all_consts(program: &Program) -> Vec<&ConstDecl> {
    let mut consts = Vec::new();
    for item in &program.items {
        match &item.kind {
            ItemKind::Const(decl) => consts.push(decl),
            ItemKind::Function(func) => collect_consts_from_expr(&func.body, &mut consts),
            ItemKind::ForBlock(block) => {
                for func in &block.functions {
                    collect_consts_from_expr(&func.body, &mut consts);
                }
            }
            _ => {}
        }
    }
    consts
}

/// Recursively collect const declarations from an expression (function body, block, etc.)
fn collect_consts_from_expr<'a>(expr: &'a Expr, consts: &mut Vec<&'a ConstDecl>) {
    let items = match &expr.kind {
        ExprKind::Block(stmts) | ExprKind::Collect(stmts) => stmts,
        _ => return,
    };
    for stmt in items {
        match &stmt.kind {
            ItemKind::Const(decl) => consts.push(decl),
            ItemKind::Function(func) => collect_consts_from_expr(&func.body, consts),
            _ => {}
        }
    }
}

/// Generate the TypeScript probe file content from a Floe program.
///
/// `ts_imports` maps relative import sources to their absolute `.ts`/`.tsx`
/// paths, so the probe can import them using absolute paths that tsgo can
/// resolve from the temp directory.
fn generate_probe(
    program: &Program,
    resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
    ts_imports: &HashMap<String, PathBuf>,
) -> String {
    let mut lines = Vec::new();
    let mut probe_index = 0usize;

    // Collect external import specifiers (npm + relative TS) and their imported names
    let mut external_imports: Vec<(&ImportDecl, &Item)> = Vec::new();
    let mut imported_names: HashMap<String, String> = HashMap::new(); // name -> specifier

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            let is_ts_import = ts_imports.contains_key(&decl.source);
            if !is_relative || is_ts_import {
                external_imports.push((decl, item));
                for spec in &decl.specifiers {
                    let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                    imported_names.insert(effective_name.to_string(), decl.source.clone());
                }
            }
        }
    }

    if external_imports.is_empty() {
        return String::new();
    }

    // Emit import statements
    for (decl, _) in &external_imports {
        let names: Vec<String> = decl
            .specifiers
            .iter()
            .map(|s| {
                if let Some(alias) = &s.alias {
                    format!("{} as {}", s.name, alias)
                } else {
                    s.name.clone()
                }
            })
            .collect();
        // For relative TS imports, use a local filename that will be symlinked
        // into the probe directory (avoids tsgo emitting .d.ts next to the originals)
        let source = if let Some(abs_path) = ts_imports.get(&decl.source) {
            let filename = abs_path.file_name().unwrap_or_default().to_string_lossy();
            format!("./{filename}")
        } else {
            decl.source.clone()
        };
        lines.push(format!(
            "import {{ {} }} from \"{}\";",
            names.join(", "),
            source
        ));
    }

    // Emit Floe runtime type aliases so tsgo preserves them through inference
    lines.push("type FloeOption<T> = T | null | undefined;".to_string());

    // Emit type declarations from the program so tsgo can resolve them
    for item in &program.items {
        if let ItemKind::TypeDecl(decl) = &item.kind {
            let ts_type = type_decl_to_ts(decl);
            if !ts_type.is_empty() {
                lines.push(ts_type);
            }
        }
    }

    // Also emit type declarations from resolved .fl imports
    for resolved in resolved_imports.values() {
        for decl in &resolved.type_decls {
            let ts_type = type_decl_to_ts(decl);
            if !ts_type.is_empty() {
                lines.push(ts_type);
            }
        }
    }

    // Track probe calls and re-exports
    let mut probe_calls: Vec<ProbeCall> = Vec::new();
    let mut probe_reexports: Vec<ProbeReexport> = Vec::new();

    // Collect all const declarations from all scopes (top-level + function bodies)
    let all_consts = collect_all_consts(program);

    // Build a map of local const names -> their expression (for inlining in probes)
    let mut local_const_exprs: HashMap<String, String> = HashMap::new();
    for decl in &all_consts {
        if let ConstBinding::Name(name) = &decl.binding {
            let inner = unwrap_try_await_expr(&decl.value);
            // Only track consts whose value involves an import (directly or via member)
            if let ExprKind::Call { callee, .. } = &inner.kind {
                let callee_name = expr_to_callee_name(callee);
                if let Some(cn) = &callee_name {
                    let root = cn.split('.').next().unwrap_or("");
                    if imported_names.contains_key(cn) || imported_names.contains_key(root) {
                        let mut ts_expr = expr_to_ts_approx(inner);
                        // Substitute any local const references in the expression
                        // e.g. z.array(PostSchema) → z.array(z.object({...}))
                        for (const_name, const_expr) in &local_const_exprs {
                            if ts_expr.contains(const_name.as_str()) {
                                ts_expr = ts_expr.replace(const_name.as_str(), const_expr);
                            }
                        }
                        local_const_exprs.insert(name.clone(), ts_expr);
                    }
                }
            }
        }
    }

    // Scan const declarations for calls to imported functions.
    // Unwrap Try/Unwrap/Await wrappers to find the underlying call.
    for decl in &all_consts {
        let inner_value = unwrap_try_await_expr(&decl.value);

        // Handle Construct nodes (uppercase calls like QueryClient({...}))
        if let ExprKind::Construct {
            type_name, args, ..
        } = &inner_value.kind
            && imported_names.contains_key(type_name)
        {
            let ts_args: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
            lines.push(format!(
                "export const _r{} = new {}({});",
                probe_index,
                type_name,
                ts_args.join(", "),
            ));
            probe_index += 1;
            continue;
        }

        if let ExprKind::Call {
            callee,
            type_args,
            args,
        } = &inner_value.kind
        {
            let callee_name = expr_to_callee_name(callee);
            if let Some(name) = &callee_name {
                // Direct import call: useState(...), useSuspenseQuery(...)
                let is_imported = imported_names.contains_key(name);
                // Member call on import: z.object(...), z.array(...)
                let is_member_of_import = name.contains('.')
                    && imported_names.contains_key(name.split('.').next().unwrap_or(""));

                if is_imported || is_member_of_import {
                    let ts_type_args: Vec<String> = type_args.iter().map(type_expr_to_ts).collect();
                    let ts_args: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
                    probe_calls.push(ProbeCall {
                        index: probe_index,
                        callee: name.clone(),
                        type_args: ts_type_args,
                        args: ts_args,
                        binding: decl.binding.clone(),
                    });
                    probe_index += 1;
                    continue;
                }

                // Member call on a local const that was assigned from an import call:
                // e.g. `UserSchema.parse(json)` where `UserSchema = z.object({...})`
                // Inline the const's expression to let tsgo resolve the full chain
                if name.contains('.') {
                    let obj_name = name.split('.').next().unwrap_or("");
                    let method_chain = &name[obj_name.len() + 1..]; // preserves full chain e.g. "auth.signInWithPassword"
                    if let Some(obj_expr) = local_const_exprs.get(obj_name) {
                        let ts_args: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
                        let binding_name = const_binding_name(&decl.binding);
                        // Use a separate counter to avoid conflicting with _rN indices
                        let inlined_id = format!("inlined_{}", lines.len());
                        lines.push(format!(
                            "export const __probe_{binding_name}_{inlined_id} = {obj_expr}.{method_chain}({});",
                            ts_args.join(", "),
                        ));
                        // Don't increment probe_index — these don't use _rN naming
                        continue;
                    }
                }
            }
        }
    }

    // Re-export ALL imported names so we get their types
    // (even if they were also used in calls above)
    // Sort keys for deterministic probe/map ordering
    let mut sorted_import_names: Vec<_> = imported_names.keys().cloned().collect();
    sorted_import_names.sort();
    for name in &sorted_import_names {
        probe_reexports.push(ProbeReexport {
            index: probe_index,
            name: name.clone(),
        });
        probe_index += 1;
    }

    // Collect free variables referenced in probe call args and declare them
    // so tsgo doesn't error on undefined identifiers
    let mut declared_names: HashSet<String> = imported_names.keys().cloned().collect();
    // Also include type names and function names
    let mut local_functions: HashMap<String, &FunctionDecl> = HashMap::new();
    for item in &program.items {
        match &item.kind {
            ItemKind::TypeDecl(decl) => {
                declared_names.insert(decl.name.clone());
            }
            ItemKind::Function(decl) => {
                declared_names.insert(decl.name.clone());
                local_functions.insert(decl.name.clone(), decl);
            }
            _ => {}
        }
    }
    // Also collect functions defined inside other functions (nested)
    for item in &program.items {
        if let ItemKind::Function(func) = &item.kind {
            collect_nested_functions(&func.body, &mut declared_names, &mut local_functions);
        }
    }

    // Also register imported Floe function names as declared so they don't
    // become `declare const X: any` free-variable stubs
    for resolved in resolved_imports.values() {
        for func in &resolved.function_decls {
            declared_names.insert(func.name.clone());
        }
    }

    // Collect ALL referenced identifiers (even declared ones) to find local function refs
    let mut all_referenced: HashSet<String> = HashSet::new();
    let empty_set: HashSet<String> = HashSet::new();
    for call in &probe_calls {
        for arg_str in &call.args {
            collect_free_vars_from_ts(arg_str, &empty_set, &mut all_referenced);
        }
    }

    // Emit local function declarations with proper TS signatures
    for (name, func) in &local_functions {
        if all_referenced.contains(name.as_str()) {
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    let ty = p
                        .type_ann
                        .as_ref()
                        .map(type_expr_to_ts)
                        .unwrap_or_else(|| "any".to_string());
                    format!("{}: {}", p.name, ty)
                })
                .collect();
            let ret = func
                .return_type
                .as_ref()
                .map(type_expr_to_ts)
                .unwrap_or_else(|| "any".to_string());
            // Wrap return type in Promise<> for async functions
            // (can't use `async` in ambient declarations)
            let ret = if func.async_fn {
                format!("Promise<{ret}>")
            } else {
                ret
            };
            lines.push(format!(
                "declare function {name}({}): {ret};",
                params.join(", ")
            ));
        }
    }

    // Emit declare function stubs for imported Floe functions so tsgo
    // can infer generic types when they appear in probe call arguments
    // (e.g. useSuspenseQuery({ queryFn: async () => fetchProducts() }))
    for resolved in resolved_imports.values() {
        for func in &resolved.function_decls {
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    let ty = p
                        .type_ann
                        .as_ref()
                        .map(type_expr_to_ts)
                        .unwrap_or_else(|| "any".to_string());
                    let opt = if p.default.is_some() { "?" } else { "" };
                    format!("{}{opt}: {}", p.name, ty)
                })
                .collect();
            let ret = func
                .return_type
                .as_ref()
                .map(type_expr_to_ts)
                .unwrap_or_else(|| "any".to_string());
            let ret = if func.async_fn {
                format!("Promise<{ret}>")
            } else {
                ret
            };
            lines.push(format!(
                "declare function {}({}): {ret};",
                func.name,
                params.join(", ")
            ));
        }
    }
    // Collect free vars (excluding declared names) and emit as `any`
    let mut free_vars: HashSet<String> = HashSet::new();
    for call in &probe_calls {
        for arg_str in &call.args {
            collect_free_vars_from_ts(arg_str, &declared_names, &mut free_vars);
        }
    }
    for var in &free_vars {
        lines.push(format!("declare const {var}: any;"));
    }

    // Emit probe const declarations
    for call in &probe_calls {
        let type_args_str = if call.type_args.is_empty() {
            String::new()
        } else {
            format!("<{}>", call.type_args.join(", "))
        };
        let args_str = call.args.join(", ");

        // For array destructuring, also destructure and re-export each element
        // so tsgo inlines type aliases (e.g., Dispatch<...> → function type)
        if let ConstBinding::Array(names) = &call.binding {
            let tmp = format!("_tmp{}", call.index);
            lines.push(format!(
                "const {tmp} = {}{type_args_str}({args_str});",
                call.callee,
            ));
            let destructured: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(i, _)| format!("_r{}_{i}", call.index))
                .collect();
            lines.push(format!(
                "export const [{}] = {tmp};",
                destructured.join(", "),
            ));
        } else if let ConstBinding::Object(names) = &call.binding {
            // For object destructuring: const { data } = useSuspenseQuery(...)
            let tmp = format!("_tmp{}", call.index);
            lines.push(format!(
                "const {tmp} = {}{type_args_str}({args_str});",
                call.callee,
            ));
            lines.push(format!(
                "export const {{ {} }} = {tmp};",
                names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| format!("{n}: _r{}_{i}", call.index))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        } else {
            lines.push(format!(
                "export const _r{} = {}{type_args_str}({args_str});",
                call.index, call.callee,
            ));
        }
    }

    // Emit re-exports for non-called imports.
    // For imports NOT used in call probes, use `_expand()` to force TypeScript
    // to inline function signatures instead of emitting `typeof X` references.
    // For imports WITH call probes, keep plain re-export — `_expand` would
    // collapse overloaded/generic functions (like useState<T>) to their base signature.
    if !probe_reexports.is_empty() {
        let called_names: HashSet<&str> = probe_calls
            .iter()
            .map(|c| c.callee.split('.').next().unwrap())
            .collect();

        let needs_expand = probe_reexports
            .iter()
            .any(|r| !called_names.contains(r.name.as_str()));

        if needs_expand {
            lines.push(
                "declare function _expand<A extends any[], R>(fn: (...args: A) => R): (...args: A) => R;".to_string(),
            );
            lines.push("declare function _expand<T>(x: T): T;".to_string());
        }

        for reexport in &probe_reexports {
            if called_names.contains(reexport.name.as_str()) {
                // Already has call probes — keep plain re-export
                lines.push(format!(
                    "export const _r{} = {};",
                    reexport.index, reexport.name,
                ));
            } else {
                // No call probes — use _expand to inline the type
                lines.push(format!(
                    "export const _r{} = _expand({});",
                    reexport.index, reexport.name,
                ));
            }
        }
    }

    // Scan the source for member accesses on imported names (e.g. z.object, z.string)
    // and generate probes so tsgo resolves their types
    let mut member_accesses: Vec<(String, String)> = Vec::new(); // (object_name, field)
    collect_member_accesses_on_imports(program, &imported_names, &mut member_accesses);
    member_accesses.sort();
    member_accesses.dedup();

    for (obj, field) in &member_accesses {
        lines.push(format!(
            "export const __member_{obj}_{field} = {obj}.{field};",
        ));
    }

    // Emit type probes for type aliases that reference imported types.
    // This lets tsgo resolve conditional/mapped types (e.g. VariantProps<T>).
    // Also emit const bindings for any local consts used in typeof expressions
    // so tsgo can resolve `typeof spinnerVariants` → the inferred type.
    let mut has_type_probes = false;
    let mut typeof_consts_emitted: HashSet<String> = HashSet::new();
    for item in &program.items {
        if let ItemKind::TypeDecl(decl) = &item.kind {
            match &decl.def {
                TypeDef::Alias(type_expr)
                    if type_expr_references_imports(type_expr, &imported_names) =>
                {
                    collect_typeof_names(type_expr, &mut |name| {
                        if !typeof_consts_emitted.contains(name) {
                            if let Some(expr) = local_const_exprs.get(name) {
                                lines.push(format!("const {name} = {expr};"));
                            }
                            typeof_consts_emitted.insert(name.to_string());
                        }
                    });
                    let ts_type = type_expr_to_ts(type_expr);
                    lines.push(format!(
                        "export declare const __tprobe_{}: {};",
                        decl.name, ts_type
                    ));
                    has_type_probes = true;
                }
                TypeDef::Record(entries) => {
                    // Generate probes for record types with spreads referencing imports
                    let has_import_spreads = entries.iter().any(|e| {
                        if let Some(spread) = e.as_spread() {
                            if let Some(type_expr) = &spread.type_expr {
                                return type_expr_references_imports(type_expr, &imported_names);
                            }
                            imported_names.contains_key(&spread.type_name)
                        } else {
                            false
                        }
                    });
                    if has_import_spreads {
                        // Emit typeof const bindings for spreads
                        for entry in entries {
                            if let Some(spread) = entry.as_spread()
                                && let Some(type_expr) = &spread.type_expr
                            {
                                collect_typeof_names(type_expr, &mut |name| {
                                    if !typeof_consts_emitted.contains(name) {
                                        if let Some(expr) = local_const_exprs.get(name) {
                                            lines.push(format!("const {name} = {expr};"));
                                        }
                                        typeof_consts_emitted.insert(name.to_string());
                                    }
                                });
                            }
                        }
                        // Emit the full type as a probe
                        let ts_type = type_decl_to_ts(decl);
                        lines.push(format!("export {ts_type}"));
                        // Also emit a value probe so we can extract the resolved type
                        let ts_decl = type_decl_to_ts(decl);
                        // Extract the RHS of the type alias for the value probe
                        if let Some(eq_pos) = ts_decl.find('=') {
                            let rhs = ts_decl[eq_pos + 1..].trim().trim_end_matches(';');
                            lines.push(format!(
                                "export declare const __tprobe_{}: {};",
                                decl.name, rhs
                            ));
                        }
                        has_type_probes = true;
                    }
                }
                _ => {}
            }
        }
    }

    if probe_index == 0 && member_accesses.is_empty() && !has_type_probes {
        return String::new();
    }

    lines.join("\n") + "\n"
}

/// Collect names used in `typeof <name>` expressions within a type expression.
fn collect_typeof_names(type_expr: &TypeExpr, callback: &mut dyn FnMut(&str)) {
    match &type_expr.kind {
        TypeExprKind::TypeOf(name) => callback(name),
        TypeExprKind::Named { type_args, .. } => {
            for arg in type_args {
                collect_typeof_names(arg, callback);
            }
        }
        TypeExprKind::Intersection(types) | TypeExprKind::Tuple(types) => {
            for ty in types {
                collect_typeof_names(ty, callback);
            }
        }
        TypeExprKind::Array(inner) => collect_typeof_names(inner, callback),
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            for p in params {
                collect_typeof_names(p, callback);
            }
            collect_typeof_names(return_type, callback);
        }
        TypeExprKind::Record(fields) => {
            for f in fields {
                collect_typeof_names(&f.type_ann, callback);
            }
        }
    }
}

/// Check if a type expression references any imported names (for type probe detection).
fn type_expr_references_imports(
    type_expr: &TypeExpr,
    imported_names: &HashMap<String, String>,
) -> bool {
    match &type_expr.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            let root = name.split('.').next().unwrap_or(name);
            imported_names.contains_key(root)
                || type_args
                    .iter()
                    .any(|a| type_expr_references_imports(a, imported_names))
        }
        TypeExprKind::TypeOf(name) => {
            let root = name.split('.').next().unwrap_or(name);
            imported_names.contains_key(root)
        }
        TypeExprKind::Intersection(types) => types
            .iter()
            .any(|t| type_expr_references_imports(t, imported_names)),
        TypeExprKind::Array(inner) => type_expr_references_imports(inner, imported_names),
        TypeExprKind::Tuple(types) => types
            .iter()
            .any(|t| type_expr_references_imports(t, imported_names)),
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            params
                .iter()
                .any(|p| type_expr_references_imports(p, imported_names))
                || type_expr_references_imports(return_type, imported_names)
        }
        TypeExprKind::Record(fields) => fields
            .iter()
            .any(|f| type_expr_references_imports(&f.type_ann, imported_names)),
    }
}

/// Recursively collect all `X.field` member accesses where X is an imported name.
fn collect_member_accesses_on_imports(
    program: &Program,
    imported_names: &HashMap<String, String>,
    accesses: &mut Vec<(String, String)>,
) {
    for item in &program.items {
        match &item.kind {
            ItemKind::Const(decl) => {
                collect_member_accesses_expr(&decl.value, imported_names, accesses)
            }
            ItemKind::Function(func) => {
                collect_member_accesses_expr(&func.body, imported_names, accesses)
            }
            ItemKind::ForBlock(block) => {
                for func in &block.functions {
                    collect_member_accesses_expr(&func.body, imported_names, accesses);
                }
            }
            ItemKind::Expr(expr) => collect_member_accesses_expr(expr, imported_names, accesses),
            _ => {}
        }
    }
}

/// Recursively collect member accesses from an expression.
fn collect_member_accesses_expr(
    expr: &Expr,
    imported_names: &HashMap<String, String>,
    accesses: &mut Vec<(String, String)>,
) {
    match &expr.kind {
        ExprKind::Member { object, field } => {
            if let ExprKind::Identifier(name) = &object.kind
                && imported_names.contains_key(name)
            {
                accesses.push((name.clone(), field.clone()));
            }
            collect_member_accesses_expr(object, imported_names, accesses);
        }
        ExprKind::Call { callee, args, .. } => {
            collect_member_accesses_expr(callee, imported_names, accesses);
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_member_accesses_expr(e, imported_names, accesses);
                    }
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_member_accesses_expr(left, imported_names, accesses);
            collect_member_accesses_expr(right, imported_names, accesses);
        }
        ExprKind::Pipe { left, right } => {
            collect_member_accesses_expr(left, imported_names, accesses);
            collect_member_accesses_expr(right, imported_names, accesses);
        }
        ExprKind::Block(items) | ExprKind::Collect(items) => {
            for item in items {
                match &item.kind {
                    ItemKind::Const(decl) => {
                        collect_member_accesses_expr(&decl.value, imported_names, accesses)
                    }
                    ItemKind::Function(func) => {
                        collect_member_accesses_expr(&func.body, imported_names, accesses)
                    }
                    ItemKind::Expr(e) => collect_member_accesses_expr(e, imported_names, accesses),
                    _ => {}
                }
            }
        }
        ExprKind::Arrow { body, .. } => {
            collect_member_accesses_expr(body, imported_names, accesses);
        }
        ExprKind::Match { subject, arms } => {
            collect_member_accesses_expr(subject, imported_names, accesses);
            for arm in arms {
                collect_member_accesses_expr(&arm.body, imported_names, accesses);
            }
        }
        ExprKind::Construct { args, .. } => {
            for arg in args {
                match arg {
                    Arg::Positional(e) | Arg::Named { value: e, .. } => {
                        collect_member_accesses_expr(e, imported_names, accesses);
                    }
                }
            }
        }
        ExprKind::Object(fields) => {
            for (_, value) in fields {
                collect_member_accesses_expr(value, imported_names, accesses);
            }
        }
        ExprKind::Array(elems) => {
            for e in elems {
                collect_member_accesses_expr(e, imported_names, accesses);
            }
        }
        ExprKind::Grouped(inner)
        | ExprKind::Unary { operand: inner, .. }
        | ExprKind::Unwrap(inner)
        | ExprKind::Await(inner)
        | ExprKind::Try(inner)
        | ExprKind::Ok(inner)
        | ExprKind::Err(inner)
        | ExprKind::Some(inner)
        | ExprKind::Spread(inner) => {
            collect_member_accesses_expr(inner, imported_names, accesses);
        }
        ExprKind::Parse { value, .. } => {
            collect_member_accesses_expr(value, imported_names, accesses);
        }
        ExprKind::Mock { overrides, .. } => {
            for arg in overrides {
                match arg {
                    Arg::Positional(e) => {
                        collect_member_accesses_expr(e, imported_names, accesses);
                    }
                    Arg::Named { value, .. } => {
                        collect_member_accesses_expr(value, imported_names, accesses);
                    }
                }
            }
        }
        ExprKind::TemplateLiteral(parts) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    collect_member_accesses_expr(e, imported_names, accesses);
                }
            }
        }
        ExprKind::Index { object, index } => {
            collect_member_accesses_expr(object, imported_names, accesses);
            collect_member_accesses_expr(index, imported_names, accesses);
        }
        ExprKind::Jsx(jsx) => {
            collect_member_accesses_jsx(jsx, imported_names, accesses);
        }
        ExprKind::Tuple(elems) => {
            for e in elems {
                collect_member_accesses_expr(e, imported_names, accesses);
            }
        }
        _ => {}
    }
}

fn collect_member_accesses_jsx(
    jsx: &JsxElement,
    imported_names: &HashMap<String, String>,
    accesses: &mut Vec<(String, String)>,
) {
    match &jsx.kind {
        JsxElementKind::Element {
            props, children, ..
        } => {
            for prop in props {
                if let Some(value) = &prop.value {
                    collect_member_accesses_expr(value, imported_names, accesses);
                }
            }
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_member_accesses_expr(e, imported_names, accesses),
                    JsxChild::Element(el) => {
                        collect_member_accesses_jsx(el, imported_names, accesses)
                    }
                    _ => {}
                }
            }
        }
        JsxElementKind::Fragment { children } => {
            for child in children {
                match child {
                    JsxChild::Expr(e) => collect_member_accesses_expr(e, imported_names, accesses),
                    JsxChild::Element(el) => {
                        collect_member_accesses_jsx(el, imported_names, accesses)
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Convert a Floe TypeDecl to a TypeScript type declaration string.
fn type_decl_to_ts(decl: &TypeDecl) -> String {
    let type_params = if decl.type_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", decl.type_params.join(", "))
    };

    match &decl.def {
        TypeDef::Record(entries) => {
            let fs: Vec<String> = entries
                .iter()
                .filter_map(|e| e.as_field())
                .map(|f| format!("  {}: {};", f.name, type_expr_to_ts(&f.type_ann)))
                .collect();
            let spreads: Vec<String> = entries
                .iter()
                .filter_map(|e| e.as_spread())
                .map(|s| {
                    if let Some(type_expr) = &s.type_expr {
                        type_expr_to_ts(type_expr)
                    } else {
                        s.type_name.clone()
                    }
                })
                .collect();
            if spreads.is_empty() {
                format!(
                    "type {}{type_params} = {{\n{}\n}};",
                    decl.name,
                    fs.join("\n")
                )
            } else {
                let spread_parts: Vec<String> = spreads.to_vec();
                if fs.is_empty() {
                    format!(
                        "type {}{type_params} = {};",
                        decl.name,
                        spread_parts.join(" & ")
                    )
                } else {
                    format!(
                        "type {}{type_params} = {} & {{\n{}\n}};",
                        decl.name,
                        spread_parts.join(" & "),
                        fs.join("\n")
                    )
                }
            }
        }
        TypeDef::Alias(ty) => {
            format!("type {}{type_params} = {};", decl.name, type_expr_to_ts(ty))
        }
        TypeDef::Union(variants) => {
            // Emit as const enum so Filter.All works in the probe
            let members: Vec<String> = variants
                .iter()
                .map(|v| format!("  {} = \"{}\"", v.name, v.name))
                .collect();
            format!(
                "const enum {}{type_params} {{\n{}\n}}",
                decl.name,
                members.join(",\n")
            )
        }
        TypeDef::StringLiteralUnion(variants) => {
            let members: Vec<String> = variants.iter().map(|v| format!("\"{}\"", v)).collect();
            format!("type {}{type_params} = {};", decl.name, members.join(" | "))
        }
    }
}

/// Convert a Floe TypeExpr to a TypeScript type string.
fn type_expr_to_ts(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            let ts_name = match name.as_str() {
                "bool" => "boolean",
                "Option" if type_args.len() == 1 => {
                    let inner = type_expr_to_ts(&type_args[0]);
                    return format!("FloeOption<{inner}>");
                }
                "Result" if type_args.len() == 2 => {
                    // Result<T, E> → discriminated union matching Floe's codegen
                    let ok = type_expr_to_ts(&type_args[0]);
                    let err = type_expr_to_ts(&type_args[1]);
                    return format!("{{ ok: true, value: {ok} }} | {{ ok: false, error: {err} }}");
                }
                other => other,
            };
            if type_args.is_empty() {
                ts_name.to_string()
            } else {
                let args: Vec<String> = type_args.iter().map(type_expr_to_ts).collect();
                format!("{ts_name}<{}>", args.join(", "))
            }
        }
        TypeExprKind::Record(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, type_expr_to_ts(&f.type_ann)))
                .collect();
            format!("{{ {} }}", fs.join("; "))
        }
        TypeExprKind::Function {
            params,
            return_type,
        } => {
            let ps: Vec<String> = params
                .iter()
                .enumerate()
                .map(|(i, p)| format!("_p{i}: {}", type_expr_to_ts(p)))
                .collect();
            format!("({}) => {}", ps.join(", "), type_expr_to_ts(return_type))
        }
        TypeExprKind::Array(inner) => {
            format!("{}[]", type_expr_to_ts(inner))
        }
        TypeExprKind::Tuple(parts) => {
            let ps: Vec<String> = parts.iter().map(type_expr_to_ts).collect();
            format!("readonly [{}]", ps.join(", "))
        }
        TypeExprKind::TypeOf(name) => format!("typeof {name}"),
        TypeExprKind::Intersection(types) => {
            let parts: Vec<String> = types.iter().map(type_expr_to_ts).collect();
            parts.join(" & ")
        }
    }
}

/// Extract the callee name from a Call expression.
/// Returns `Some("name")` for simple identifiers, `None` for complex expressions.
fn expr_to_callee_name(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Identifier(name) => Some(name.clone()),
        ExprKind::Member { object, field } => {
            let obj_name = expr_to_callee_name(object)?;
            Some(format!("{obj_name}.{field}"))
        }
        _ => None,
    }
}

/// Convert an Arg to an approximate TypeScript expression string.
fn arg_to_ts_approx(arg: &Arg) -> String {
    match arg {
        Arg::Positional(expr) => expr_to_ts_approx(expr),
        Arg::Named { value, .. } => expr_to_ts_approx(value),
    }
}

/// Convert a Floe expression to an approximate TypeScript expression string.
/// Used for probe file arguments -- doesn't need to be semantically correct,
/// just valid enough for TypeScript to infer types.
fn expr_to_ts_approx(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Number(n) => n.clone(),
        ExprKind::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::Array(elems) => {
            let es: Vec<String> = elems.iter().map(expr_to_ts_approx).collect();
            format!("[{}]", es.join(", "))
        }
        ExprKind::Construct { args, .. } => {
            // Approximate as an object literal
            let fs: Vec<String> = args
                .iter()
                .map(|a| match a {
                    Arg::Named { label, value } => {
                        format!("{label}: {}", expr_to_ts_approx(value))
                    }
                    Arg::Positional(expr) => expr_to_ts_approx(expr),
                })
                .collect();
            format!("{{ {} }}", fs.join(", "))
        }
        ExprKind::Call {
            callee,
            type_args,
            args,
        } => {
            let callee_str = expr_to_ts_approx(callee);
            let type_args_str = if type_args.is_empty() {
                String::new()
            } else {
                let ta: Vec<String> = type_args.iter().map(type_expr_to_ts).collect();
                format!("<{}>", ta.join(", "))
            };
            let args_str: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
            format!("{callee_str}{type_args_str}({})", args_str.join(", "))
        }
        ExprKind::Member { object, field } => {
            format!("{}.{field}", expr_to_ts_approx(object))
        }
        ExprKind::Arrow { params, body, .. } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    if let Some(ty) = &p.type_ann {
                        format!("{}: {}", p.name, type_expr_to_ts(ty))
                    } else {
                        p.name.clone()
                    }
                })
                .collect();
            format!("({}) => {}", ps.join(", "), expr_to_ts_approx(body))
        }
        ExprKind::Object(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|(key, value)| format!("{key}: {}", expr_to_ts_approx(value)))
                .collect();
            format!("{{ {} }}", fs.join(", "))
        }
        ExprKind::Grouped(inner) => format!("({})", expr_to_ts_approx(inner)),
        ExprKind::Unit => "undefined".to_string(),
        ExprKind::None => "null".to_string(),
        // For anything else, use a placeholder that TypeScript can handle
        _ => "undefined as any".to_string(),
    }
}

/// Create a temporary directory with the probe file and tsconfig.
fn create_probe_dir(
    project_dir: &Path,
    probe_content: &str,
    ts_imports: &HashMap<String, PathBuf>,
) -> Result<tempfile::TempDir, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let probe_dir = tmp.path();

    // Write probe.ts
    std::fs::write(probe_dir.join("probe.ts"), probe_content)
        .map_err(|e| format!("failed to write probe.ts: {e}"))?;

    // Symlink local .ts/.tsx files into the probe directory so tsgo can
    // resolve them without absolute paths (which cause tsgo to emit stray
    // .d.ts files next to the original sources)
    for abs_path in ts_imports.values() {
        if let Some(filename) = abs_path.file_name() {
            let link = probe_dir.join(filename);
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(abs_path, &link).ok();
            }
            #[cfg(not(unix))]
            {
                std::fs::copy(abs_path, &link).ok();
            }
        }
    }

    // Write tsconfig.json, inheriting paths from the project's tsconfig if available
    let paths_config = read_project_tsconfig_paths(project_dir);
    let tsconfig = format!(
        r#"{{
  "compilerOptions": {{
    "moduleResolution": "bundler",
    "strict": false,
    "strictNullChecks": true,
    "noImplicitAny": false,
    "jsx": "react-jsx",
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "./out",
    "rootDir": "/",
    "skipLibCheck": true{paths_config}
  }},
  "include": ["probe.ts"]
}}"#
    );
    std::fs::write(probe_dir.join("tsconfig.json"), &tsconfig)
        .map_err(|e| format!("failed to write tsconfig.json: {e}"))?;

    // Symlink node_modules from the project directory
    let node_modules = project_dir.join("node_modules");
    if node_modules.is_dir() {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&node_modules, probe_dir.join("node_modules"))
                .map_err(|e| format!("failed to symlink node_modules: {e}"))?;
        }
        #[cfg(not(unix))]
        {
            // On Windows, try junction or copy
            let _ = std::fs::create_dir_all(probe_dir.join("node_modules"));
            // Fall through — types may not resolve without node_modules
        }
    }

    Ok(tmp)
}

/// Read `paths` and `baseUrl` from the project's tsconfig.json and format them
/// as JSON properties to include in the probe's tsconfig.
/// Returns an empty string if no paths are configured.
fn read_project_tsconfig_paths(project_dir: &Path) -> String {
    crate::resolve::ParsedTsconfig::from_project_dir(project_dir)
        .map(|p| p.to_probe_json_fragment())
        .unwrap_or_default()
}

/// Find `probe.d.ts` under the output directory.
///
/// With `rootDir: "/"` in the probe tsconfig, tsgo mirrors the full absolute
/// path under `outDir`, so the file ends up at `out/<full-temp-path>/probe.d.ts`
/// instead of `out/probe.d.ts`. We search recursively to handle both cases.
fn find_probe_dts(probe_dir: &Path) -> Option<PathBuf> {
    let out_dir = probe_dir.join("out");
    // Fast path: check the simple location first
    let simple = out_dir.join("probe.d.ts");
    if simple.exists() {
        return Some(simple);
    }
    // Slow path: search recursively under out/
    fn walk(dir: &Path) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n == "probe.d.ts") {
                return Some(path);
            }
            if path.is_dir()
                && let Some(found) = walk(&path)
            {
                return Some(found);
            }
        }
        None
    }
    walk(&out_dir)
}

/// Run tsgo on the probe directory and return the output `.d.ts` content.
fn run_tsgo(probe_dir: &Path) -> Result<String, String> {
    // Try tsgo first, then fall back to npx @typescript/native-preview
    let tsgo_result = Command::new("tsgo")
        .args(["-p", "tsconfig.json"])
        .current_dir(probe_dir)
        .output();

    let output = match tsgo_result {
        Ok(output) if output.status.success() || find_probe_dts(probe_dir).is_some() => output,
        _ => {
            // Fall back to npx @typescript/native-preview
            let npx_result = Command::new("npx")
                .args(["@typescript/native-preview", "-p", "tsconfig.json"])
                .current_dir(probe_dir)
                .output()
                .map_err(|e| format!("failed to run tsgo or npx: {e}"))?;

            if !npx_result.status.success() && find_probe_dts(probe_dir).is_none() {
                let stderr = String::from_utf8_lossy(&npx_result.stderr);
                return Err(format!("tsgo failed: {stderr}"));
            }
            npx_result
        }
    };

    // Even if tsgo reports errors (e.g. for unused variables), check if the .d.ts was emitted
    let _ = output;
    let dts_path =
        find_probe_dts(probe_dir).ok_or_else(|| "tsgo did not emit probe.d.ts".to_string())?;

    std::fs::read_to_string(&dts_path).map_err(|e| format!("failed to read probe.d.ts: {e}"))
}

/// Build a specifier-to-exports map from the resolved probe exports.
///
/// The probe uses `_r0`, `_r1`, etc. as export names. We map these back
/// to the original import specifiers by replaying the same probe generation
/// logic to know which index corresponds to which import.
fn build_specifier_map(
    program: &Program,
    probe_exports: &[DtsExport],
    ts_imports: &HashMap<String, PathBuf>,
) -> HashMap<String, Vec<DtsExport>> {
    let mut result: HashMap<String, Vec<DtsExport>> = HashMap::new();
    let mut probe_index = 0usize;

    // Collect external imports (npm + relative TS)
    let mut imported_names: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            let is_ts_import = ts_imports.contains_key(&decl.source);
            if !is_relative || is_ts_import {
                for spec in &decl.specifiers {
                    let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                    imported_names.insert(effective_name.to_string(), decl.source.clone());
                }
            }
        }
    }

    // Use the same recursive const collection as generate_probe
    let all_consts = collect_all_consts(program);

    // Map call probe results (including Construct nodes)
    for decl in &all_consts {
        let inner_value = unwrap_try_await_expr(&decl.value);

        // Handle Construct nodes (uppercase calls like QueryClient({...}))
        if let ExprKind::Construct { type_name, .. } = &inner_value.kind
            && imported_names.contains_key(type_name)
        {
            let specifier = &imported_names[type_name];
            let binding_name = const_binding_name(&decl.binding);
            let probe_name = format!("_r{probe_index}");
            if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                result
                    .entry(specifier.clone())
                    .or_default()
                    .push(DtsExport {
                        name: format!("__probe_{}", binding_name),
                        ts_type: export.ts_type.clone(),
                    });
            }
            probe_index += 1;
            continue;
        }

        if let ExprKind::Call { callee, .. } = &inner_value.kind {
            let callee_name = expr_to_callee_name(callee);
            if let Some(name) = &callee_name {
                let is_imported = imported_names.contains_key(name);
                let root_name = name.split('.').next().unwrap_or("");
                let is_member_of_import =
                    name.contains('.') && imported_names.contains_key(root_name);

                if !is_imported && !is_member_of_import {
                    continue;
                }

                let specifier = if is_imported {
                    &imported_names[name]
                } else {
                    &imported_names[root_name]
                };
                let binding_name = const_binding_name(&decl.binding);

                // For array bindings, collect individual element types
                if let ConstBinding::Array(names) = &decl.binding {
                    let elem_types: Vec<TsType> = names
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            let elem_name = format!("_r{}_{i}", probe_index);
                            probe_exports
                                .iter()
                                .find(|e| e.name == elem_name)
                                .map(|e| e.ts_type.clone())
                                .unwrap_or(TsType::Unknown)
                        })
                        .collect();
                    result
                        .entry(specifier.clone())
                        .or_default()
                        .push(DtsExport {
                            name: format!("__probe_{}", binding_name),
                            ts_type: TsType::Tuple(elem_types),
                        });
                } else if let ConstBinding::Object(names) = &decl.binding {
                    // For object destructuring: const { data } = f(...)
                    let elem_types: Vec<TsType> = names
                        .iter()
                        .enumerate()
                        .map(|(i, _)| {
                            let elem_name = format!("_r{}_{i}", probe_index);
                            probe_exports
                                .iter()
                                .find(|e| e.name == elem_name)
                                .map(|e| e.ts_type.clone())
                                .unwrap_or(TsType::Unknown)
                        })
                        .collect();
                    // Create individual probes for each destructured field
                    // Use probe_index to disambiguate when same field name appears multiple times
                    for (i, name) in names.iter().enumerate() {
                        if i < elem_types.len() {
                            result
                                .entry(specifier.clone())
                                .or_default()
                                .push(DtsExport {
                                    name: format!("__probe_{name}_{probe_index}"),
                                    ts_type: elem_types[i].clone(),
                                });
                        }
                    }
                } else {
                    let probe_name = format!("_r{probe_index}");
                    if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                        result
                            .entry(specifier.clone())
                            .or_default()
                            .push(DtsExport {
                                name: format!("__probe_{}", binding_name),
                                ts_type: export.ts_type.clone(),
                            });
                    }
                }
                probe_index += 1;
                continue;
            }
        }
    }

    // Map re-export probe results — ALL imported names (sorted for deterministic order)
    let mut sorted_import_names: Vec<_> = imported_names.iter().collect();
    sorted_import_names.sort_by_key(|(name, _)| (*name).clone());
    for (name, specifier) in sorted_import_names {
        let probe_name = format!("_r{probe_index}");
        if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
            result
                .entry(specifier.clone())
                .or_default()
                .push(DtsExport {
                    name: name.clone(),
                    ts_type: export.ts_type.clone(),
                });
        }
        probe_index += 1;
    }

    // Map member access probe results (__member_X_field exports)
    // and inlined const call probe results (__probe_X_N exports)
    // and type alias probe results (__tprobe_X exports)
    for export in probe_exports {
        if let Some(rest) = export.name.strip_prefix("__member_") {
            // Find which specifier this belongs to
            if let Some(underscore_pos) = rest.find('_') {
                let obj_name = &rest[..underscore_pos];
                if let Some(specifier) = imported_names.get(obj_name) {
                    result
                        .entry(specifier.clone())
                        .or_default()
                        .push(export.clone());
                }
            }
        }
        // Type alias probes (__tprobe_SpinnerProps, etc.)
        // tsgo resolved the complex types (conditional, mapped) for us.
        // Add to any specifier so the checker can find them.
        if export.name.starts_with("__tprobe_")
            && let Some(first_specifier) = result.keys().next().cloned()
        {
            result
                .entry(first_specifier)
                .or_default()
                .push(export.clone());
        }
        // Inlined const call probes (__probe_user_5, __probe_posts_7, etc.)
        // These are generated for calls like UserSchema.parse(json) where
        // UserSchema is a local const assigned from an import call.
        // Add to any available specifier so the checker can find them.
        if export.name.starts_with("__probe_")
            && !result.values().flatten().any(|e| e.name == export.name)
            && let Some(first_specifier) = result.keys().next().cloned()
        {
            result
                .entry(first_specifier)
                .or_default()
                .push(export.clone());
        }
    }

    result
}

/// Get the binding name from a ConstBinding for identification purposes.
fn const_binding_name(binding: &ConstBinding) -> String {
    match binding {
        ConstBinding::Name(name) => name.clone(),
        ConstBinding::Array(names) => names.join("_"),
        ConstBinding::Object(names) => names.join("_"),
        ConstBinding::Tuple(names) => names.join("_"),
    }
}

/// Unwrap Try, Unwrap, and Await wrappers to find the inner expression.
/// e.g. `try await fetch(url)?` → `fetch(url)`
fn unwrap_try_await_expr(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Try(inner) | ExprKind::Unwrap(inner) | ExprKind::Await(inner) => {
            unwrap_try_await_expr(inner)
        }
        _ => expr,
    }
}

/// Collect function declarations nested inside expression bodies.
fn collect_nested_functions<'a>(
    expr: &'a Expr,
    declared: &mut HashSet<String>,
    functions: &mut HashMap<String, &'a FunctionDecl>,
) {
    let items = match &expr.kind {
        ExprKind::Block(items) | ExprKind::Collect(items) => items,
        _ => return,
    };
    for item in items {
        if let ItemKind::Function(decl) = &item.kind {
            declared.insert(decl.name.clone());
            functions.insert(decl.name.clone(), decl);
            collect_nested_functions(&decl.body, declared, functions);
        }
    }
}

/// Extract identifier-like tokens from a TypeScript expression string
/// and collect any that aren't in `declared`. This is a rough heuristic
/// to find free variables that need `declare const` in the probe.
fn collect_free_vars_from_ts(ts: &str, declared: &HashSet<String>, free: &mut HashSet<String>) {
    for token in ts.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if token.is_empty() || token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // Skip TS keywords and common literals
        if matches!(
            token,
            "const"
                | "let"
                | "var"
                | "function"
                | "return"
                | "new"
                | "true"
                | "false"
                | "null"
                | "undefined"
                | "as"
                | "any"
                | "void"
                | "number"
                | "string"
                | "boolean"
                | "object"
                | "export"
                | "import"
                | "from"
                | "type"
                | "async"
                | "await"
                | "readonly"
        ) {
            continue;
        }
        if !declared.contains(token) {
            free.insert(token.to_string());
        }
    }
}

// ── typeof resolution ───────────────────────────────────────

/// Resolve `typeof X` types in the specifier map by looking up X's actual type
/// in the original package .d.ts files (following `export *` re-exports) or
/// in the source .ts/.tsx files for relative imports.
///
/// When tsgo probes re-export an imported name (`export const _r0 = getYear;`),
/// TypeScript infers the type as `typeof getYear` rather than expanding the
/// function signature. This function resolves those references by parsing the
/// source files directly.
fn resolve_typeof_types(
    result: &mut HashMap<String, Vec<DtsExport>>,
    project_dir: &Path,
    program: &Program,
) {
    // Build a map of import name -> module source
    let mut import_sources: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            for spec in &decl.specifiers {
                let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                import_sources.insert(effective_name.to_string(), decl.source.clone());
            }
        }
    }

    // Collect all (specifier, export_name, typeof_name) tuples that need resolution
    let to_resolve: Vec<(String, String, String)> = result
        .iter()
        .flat_map(|(specifier, exports)| {
            exports.iter().filter_map(|e| {
                if let TsType::Named(ref s) = e.ts_type
                    && let Some(ref_name) = s.strip_prefix("typeof ")
                {
                    return Some((specifier.clone(), e.name.clone(), ref_name.to_string()));
                }
                None
            })
        })
        .collect();

    if to_resolve.is_empty() {
        return;
    }

    // Cache parsed exports to avoid re-parsing the same module
    let mut module_cache: HashMap<String, Vec<DtsExport>> = HashMap::new();

    for (specifier, export_name, typeof_name) in to_resolve {
        let module_source = import_sources
            .get(&typeof_name)
            .unwrap_or(&specifier)
            .clone();

        let module_exports = module_cache
            .entry(module_source.clone())
            .or_insert_with(|| {
                // Try npm package .d.ts (follows `export *` re-exports)
                if let Some(dts_path) = find_package_dts(project_dir, &module_source)
                    && let Ok(exports) = super::dts::parse_dts_exports(&dts_path)
                {
                    return exports;
                }
                Vec::new()
            });

        // Look for the typeof name in the module exports
        if let Some(found) = module_exports.iter().find(|e| e.name == typeof_name)
            && let Some(exports) = result.get_mut(&specifier)
            && let Some(entry) = exports.iter_mut().find(|e| e.name == export_name)
        {
            entry.ts_type = found.ts_type.clone();
        }
    }
}

/// Find the main .d.ts file for an npm package by reading its package.json.
fn find_package_dts(project_dir: &Path, module_name: &str) -> Option<PathBuf> {
    let pkg_dir = project_dir.join("node_modules").join(module_name);
    let pkg_json_path = pkg_dir.join("package.json");

    if let Ok(content) = std::fs::read_to_string(&pkg_json_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
    {
        for field in &["types", "typings"] {
            if let Some(types_path) = json[field].as_str() {
                let full_path = pkg_dir.join(types_path);
                if full_path.exists() {
                    return Some(full_path);
                }
            }
        }
    }

    // Fallback: try index.d.ts
    let index_dts = pkg_dir.join("index.d.ts");
    if index_dts.exists() {
        return Some(index_dts);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn generate_probe_basic_import() {
        let source = r#"import { useState } from "react"
const [count, setCount] = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.contains("import { useState } from \"react\";"));
        // Array binding: destructures into _r0_0, _r0_1
        assert!(probe.contains("_tmp0 = useState(0);"));
        assert!(probe.contains("export const [_r0_0, _r0_1] = _tmp0;"));
    }

    #[test]
    fn generate_probe_with_type_args() {
        let source = r#"import { useState } from "react"
type Todo { text: string }
const [todos, setTodos] = useState<Array<Todo>>([])"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.contains("import { useState } from \"react\";"));
        assert!(probe.contains("type Todo = {"));
        assert!(probe.contains("_tmp0 = useState<Array<Todo>>([]);"));
        assert!(probe.contains("export const [_r0_0, _r0_1] = _tmp0;"));
    }

    #[test]
    fn generate_probe_empty_for_no_npm_imports() {
        let source = r#"import { foo } from "./local"
const x = 42"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_re_exports_unused_imports() {
        let source = r#"import { useState, useEffect } from "react"
const [count, setCount] = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());

        // Array binding: destructured
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
        let source = "type Todo { text: string, done: bool }";
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
import trusted { useState } from "react"
type Todo { text: string, done: bool }
const [todos, setTodos] = useState<Array<Todo>>([])
const [input, setInput] = useState("")
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

        eprintln!("tsgo result keys: {:?}", result.keys().collect::<Vec<_>>());
        if let Some(react_exports) = result.get("react") {
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
                result.keys().collect::<Vec<_>>()
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
import trusted { useState } from "react"
type Filter { | All | Active | Completed }
const [filter, setFilter] = useState<Filter>(Filter.All)
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

        if let Some(react_exports) = result.get("react") {
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
        let source = "type Foo { bar: Option<string> }";
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
import trusted { useSuspenseQuery } from "@tanstack/react-query"

fn test() {
    const { data } = useSuspenseQuery({
        queryKey: ["products"],
        queryFn: async () => fetchProducts(),
    })
}"#;
        let program = Parser::new(source).parse_program().unwrap();

        // Build resolved imports with a mock fetchProducts function
        let mut resolved = HashMap::new();
        let fetch_fn = FunctionDecl {
            exported: true,
            async_fn: true,
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
            !probe.contains("declare const fetchProducts: any"),
            "fetchProducts should not be declared as `any`, got:\n{probe}"
        );
    }

    #[test]
    fn generate_probe_includes_relative_ts_imports() {
        let source = r#"import trusted { newDate } from "../utils/date"
const year = newDate()"#;
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
    fn generate_probe_empty_when_only_fl_imports() {
        // Relative imports that resolve to .fl files should not be in the probe
        let source = r#"import { User } from "./types"
const x = 42"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_includes_type_alias_probe() {
        let source = r#"import trusted { tv, VariantProps } from "tailwind-variants"
const spinnerVariants = tv({})
type SpinnerProps = VariantProps<typeof spinnerVariants>"#;
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
        let source = r#"import trusted { tv, VariantProps } from "tailwind-variants"
const spinnerVariants = tv({})
type SpinnerProps = VariantProps<typeof spinnerVariants>"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        // The probe should declare spinnerVariants so typeof can resolve
        assert!(
            probe.contains("const spinnerVariants = tv("),
            "probe should declare const for typeof resolution: {probe}"
        );
    }

    #[test]
    fn type_probe_not_emitted_for_local_only_alias() {
        let source = r#"import trusted { useState } from "react"
type MyNum = number"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new(), &HashMap::new());
        assert!(
            !probe.contains("__tprobe_MyNum"),
            "local-only type alias should not be probed: {probe}"
        );
    }
}
