//! Probe file generation — builds a TypeScript "probe" file from a Floe program.
//!
//! The probe re-exports imported symbols with concrete type arguments so tsgo
//! can emit a `.d.ts` with fully-resolved types.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::parser::ast::*;

/// Information about a const declaration that calls an imported function.
pub(super) struct ProbeCall {
    /// Index for the probe variable name: `_r0`, `_r1`, etc.
    pub(super) index: usize,
    /// The callee name (e.g. "useState")
    pub(super) callee: String,
    /// Type arguments as TypeScript strings
    pub(super) type_args: Vec<String>,
    /// Arguments as TypeScript expression strings
    pub(super) args: Vec<String>,
    /// The const binding (for mapping back to variable names)
    #[allow(dead_code)]
    pub(super) binding: ConstBinding,
}

/// Information about a plain re-export from an npm import.
pub(super) struct ProbeReexport {
    /// Index for the probe variable name
    pub(super) index: usize,
    /// The imported symbol name
    pub(super) name: String,
}

/// Collect all const declarations from a program, including those inside function bodies.
pub(super) fn collect_all_consts(program: &Program) -> Vec<&ConstDecl> {
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
pub(super) fn generate_probe(
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
                        let inlined_id = format!("inlined_{}", lines.len());
                        let call_expr =
                            format!("{obj_expr}.{method_chain}({})", ts_args.join(", "),);
                        // For object destructuring, generate per-field exports so tsgo
                        // expands opaque named types through member access
                        if let ConstBinding::Object(fields) = &decl.binding {
                            let has_await = crate::checker::expr_has_await(&decl.value);
                            let await_prefix = if has_await { "await " } else { "" };
                            lines.push(format!(
                                "const _tmp_{inlined_id} = {await_prefix}{call_expr};"
                            ));
                            for f in fields {
                                // Probe uses the original field name to access the property
                                let field = &f.field;
                                lines.push(format!(
                                    "export const __probe_{field}_{inlined_id} = _tmp_{inlined_id}.{field};"
                                ));
                            }
                        } else {
                            let binding_name = decl.binding.binding_name();
                            lines.push(format!(
                                "export const __probe_{binding_name}_{inlined_id} = {call_expr};"
                            ));
                        }
                        // Don't increment probe_index — these don't use _rN naming
                        continue;
                    }
                }
            }

            // Chained call on an import without intermediate const:
            // e.g. `getSupabaseClient().auth.signOut()` — callee_name is None because
            // expr_to_callee_name can't traverse Call nodes, but the full expression
            // can be converted to TS via expr_to_ts_approx for the probe.
            if callee_name.is_none() && expr_contains_import(callee, &imported_names) {
                let call_ts = expr_to_ts_approx(inner_value);
                let inlined_id = format!("inlined_{}", lines.len());
                if let ConstBinding::Object(fields) = &decl.binding {
                    let has_await = crate::checker::expr_has_await(&decl.value);
                    let await_prefix = if has_await { "await " } else { "" };
                    lines.push(format!(
                        "const _tmp_{inlined_id} = {await_prefix}{call_ts};"
                    ));
                    for f in fields {
                        let field = &f.field;
                        lines.push(format!(
                            "export const __probe_{field}_{inlined_id} = _tmp_{inlined_id}.{field};"
                        ));
                    }
                } else {
                    let binding_name = decl.binding.binding_name();
                    lines.push(format!(
                        "export const __probe_{binding_name}_{inlined_id} = {call_ts};"
                    ));
                }
                continue;
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
        } else if let ConstBinding::Object(fields) = &call.binding {
            // For object destructuring: const { data } = useSuspenseQuery(...)
            let tmp = format!("_tmp{}", call.index);
            lines.push(format!(
                "const {tmp} = {}{type_args_str}({args_str});",
                call.callee,
            ));
            lines.push(format!(
                "export const {{ {} }} = {tmp};",
                fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("{}: _r{}_{i}", f.field, call.index))
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

    // Emit JSX callback parameter probes: extract callback param types from
    // component props using TS conditional types (e.g. NavLink's className).
    let collector = collect_jsx_callback_probes(program, &imported_names);
    if !collector.probes.is_empty() {
        lines.push(
            "type _JCB<T> = T extends (arg: infer P, ...rest: any[]) => any ? P : never;"
                .to_string(),
        );
        for probe in &collector.probes {
            lines.push(format!(
                "export declare const __jsx_{}_{}:\
                 _JCB<NonNullable<Parameters<typeof {}>[0][\"{}\"]>>;",
                probe.component, probe.prop, probe.component, probe.prop,
            ));
        }
    }
    // Emit children render prop probes: extract each parameter type individually.
    for probe in &collector.children_probes {
        for i in 0..probe.param_count {
            lines.push(format!(
                "export declare const __jsxc_{comp}_{i}:\
                 Parameters<NonNullable<Parameters<typeof {comp}>[0][\"children\"]>>[{i}];",
                comp = probe.component,
                i = i,
            ));
        }
    }

    let has_jsx_probes = !collector.probes.is_empty() || !collector.children_probes.is_empty();

    if probe_index == 0 && member_accesses.is_empty() && !has_type_probes && !has_jsx_probes {
        return String::new();
    }

    lines.join("\n") + "\n"
}

struct JsxCallbackProbe {
    component: String,
    prop: String,
}

struct JsxChildrenProbe {
    component: String,
    param_count: usize,
}

#[derive(Default)]
struct ProbeCollector {
    probes: Vec<JsxCallbackProbe>,
    children_probes: Vec<JsxChildrenProbe>,
    seen: HashSet<(String, String)>,
    children_seen: HashSet<String>,
}

/// Walk the AST to find JSX callback props and children render props on imported components.
/// Uses `walk_program` for expression traversal; only inspects JSX elements directly.
fn collect_jsx_callback_probes(
    program: &Program,
    imported_names: &HashMap<String, String>,
) -> ProbeCollector {
    let mut collector = ProbeCollector::default();
    crate::walk::walk_program(program, &mut |expr| {
        if let ExprKind::Jsx(jsx) = &expr.kind {
            inspect_jsx_for_callback_probes(jsx, imported_names, &mut collector);
        }
    });
    collector
}

/// Inspect a JSX element tree for callback props and children render props on imported components.
/// Only recurses into nested `JsxChild::Element` nodes; expression traversal
/// is handled by the caller (`walk_program`).
fn inspect_jsx_for_callback_probes(
    jsx: &JsxElement,
    imported_names: &HashMap<String, String>,
    collector: &mut ProbeCollector,
) {
    if let JsxElementKind::Element {
        name,
        props,
        children,
        ..
    } = &jsx.kind
    {
        if name.starts_with(|c: char| c.is_uppercase()) && imported_names.contains_key(name) {
            for prop in props {
                if let JsxProp::Named {
                    name: prop_name,
                    value: Some(value),
                    ..
                } = prop
                {
                    // Skip event handlers (handled by event_handler_context)
                    if prop_name.starts_with("on") && prop_name.len() > 2 {
                        continue;
                    }
                    if matches!(value.kind, ExprKind::Arrow { .. }) {
                        let key = (name.clone(), prop_name.clone());
                        if collector.seen.insert(key) {
                            collector.probes.push(JsxCallbackProbe {
                                component: name.clone(),
                                prop: prop_name.clone(),
                            });
                        }
                    }
                }
            }
            for child in children {
                if let JsxChild::Expr(expr) = child
                    && let ExprKind::Arrow { params, .. } = &expr.kind
                    && collector.children_seen.insert(name.clone())
                {
                    collector.children_probes.push(JsxChildrenProbe {
                        component: name.clone(),
                        param_count: params.len(),
                    });
                }
            }
        }
        for child in children {
            if let JsxChild::Element(el) = child {
                inspect_jsx_for_callback_probes(el, imported_names, collector);
            }
        }
    }
    if let JsxElementKind::Fragment { children } = &jsx.kind {
        for child in children {
            if let JsxChild::Element(el) = child {
                inspect_jsx_for_callback_probes(el, imported_names, collector);
            }
        }
    }
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
        TypeExprKind::StringLiteral(_) => {}
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
        TypeExprKind::StringLiteral(_) => false,
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
                match prop {
                    JsxProp::Named { value, .. } => {
                        if let Some(value) = value {
                            collect_member_accesses_expr(value, imported_names, accesses);
                        }
                    }
                    JsxProp::Spread { expr, .. } => {
                        collect_member_accesses_expr(expr, imported_names, accesses);
                    }
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
pub(super) fn type_decl_to_ts(decl: &TypeDecl) -> String {
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
pub(super) fn type_expr_to_ts(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named {
            name, type_args, ..
        } => {
            let ts_name = match name.as_str() {
                "()" => "void",
                "undefined" => "undefined",
                "never" => "never",
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
        TypeExprKind::StringLiteral(value) => format!("\"{value}\""),
    }
}

/// Check if an expression tree contains a reference to an imported name.
/// Walks through Member and Call nodes to find if any Identifier is an import.
fn expr_contains_import(expr: &Expr, imported_names: &HashMap<String, String>) -> bool {
    match &expr.kind {
        ExprKind::Identifier(name) => imported_names.contains_key(name),
        ExprKind::Member { object, .. } => expr_contains_import(object, imported_names),
        ExprKind::Call { callee, .. } => expr_contains_import(callee, imported_names),
        _ => false,
    }
}

/// Extract the callee name from a Call expression.
/// Returns `Some("name")` for simple identifiers, `None` for complex expressions.
pub(super) fn expr_to_callee_name(expr: &Expr) -> Option<String> {
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
        // For anything else, use a placeholder that TypeScript can handle
        _ => "undefined as any".to_string(),
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

/// Unwrap Try, Unwrap, and Await wrappers to find the inner expression.
/// e.g. `try await fetch(url)?` → `fetch(url)`
pub(super) fn unwrap_try_await_expr(expr: &Expr) -> &Expr {
    match &expr.kind {
        ExprKind::Try(inner) | ExprKind::Unwrap(inner) | ExprKind::Await(inner) => {
            unwrap_try_await_expr(inner)
        }
        _ => expr,
    }
}
