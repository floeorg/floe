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

    /// Resolve npm imports in a program by generating a probe file, running
    /// tsgo, and parsing the output `.d.ts`.
    ///
    /// Returns a map from npm specifier to its resolved exports. The exports
    /// contain fully-resolved types (no unresolved generics).
    pub fn resolve_imports(
        &mut self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
    ) -> HashMap<String, Vec<DtsExport>> {
        let probe = generate_probe(program, resolved_imports);
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
            return build_specifier_map(program, cached);
        }

        // Create temp directory with probe file and tsconfig
        let tmp = match create_probe_dir(&self.project_dir, &probe) {
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

        let exports = match parse_dts_exports_from_str(&dts_content) {
            Ok(exports) => exports,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to parse output: {e}");
                return HashMap::new();
            }
        };

        // Cache the result
        self.cache.insert(hash, exports.clone());

        build_specifier_map(program, &exports)
    }
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
    if let ExprKind::Block(stmts) = &expr.kind {
        for stmt in stmts {
            match &stmt.kind {
                ItemKind::Const(decl) => consts.push(decl),
                ItemKind::Function(func) => collect_consts_from_expr(&func.body, consts),
                _ => {}
            }
        }
    }
}

/// Generate the TypeScript probe file content from a Floe program.
fn generate_probe(
    program: &Program,
    resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
) -> String {
    let mut lines = Vec::new();
    let mut probe_index = 0usize;

    // Collect npm import specifiers and their imported names
    let mut npm_imports: Vec<(&ImportDecl, &Item)> = Vec::new();
    let mut imported_names: HashMap<String, String> = HashMap::new(); // name -> specifier

    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            if !is_relative {
                npm_imports.push((decl, item));
                for spec in &decl.specifiers {
                    let effective_name = spec.alias.as_deref().unwrap_or(&spec.name);
                    imported_names.insert(effective_name.to_string(), decl.source.clone());
                }
            }
        }
    }

    if npm_imports.is_empty() {
        return String::new();
    }

    // Emit import statements
    for (decl, _) in &npm_imports {
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
        lines.push(format!(
            "import {{ {} }} from \"{}\";",
            names.join(", "),
            decl.source
        ));
    }

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

    // Scan const declarations for calls to imported functions
    for decl in &all_consts {
        // Handle Construct nodes (uppercase calls like QueryClient({...}))
        if let ExprKind::Construct {
            type_name, args, ..
        } = &decl.value.kind
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
        } = &decl.value.kind
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

    // Emit re-exports for non-called imports
    for reexport in &probe_reexports {
        lines.push(format!(
            "export const _r{} = {};",
            reexport.index, reexport.name,
        ));
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

    if probe_index == 0 && member_accesses.is_empty() {
        return String::new();
    }

    lines.join("\n") + "\n"
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
        ExprKind::Block(items) => {
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
        | ExprKind::Return(Some(inner))
        | ExprKind::Ok(inner)
        | ExprKind::Err(inner)
        | ExprKind::Some(inner)
        | ExprKind::Spread(inner) => {
            collect_member_accesses_expr(inner, imported_names, accesses);
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
        TypeDef::Record(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| format!("  {}: {};", f.name, type_expr_to_ts(&f.type_ann)))
                .collect();
            format!(
                "type {}{type_params} = {{\n{}\n}};",
                decl.name,
                fs.join("\n")
            )
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
                    return format!("{inner} | null | undefined");
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
fn create_probe_dir(project_dir: &Path, probe_content: &str) -> Result<tempfile::TempDir, String> {
    let tmp = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let probe_dir = tmp.path();

    // Write probe.ts
    std::fs::write(probe_dir.join("probe.ts"), probe_content)
        .map_err(|e| format!("failed to write probe.ts: {e}"))?;

    // Write tsconfig.json
    let tsconfig = r#"{
  "compilerOptions": {
    "moduleResolution": "bundler",
    "strict": false,
    "noImplicitAny": false,
    "jsx": "react-jsx",
    "declaration": true,
    "emitDeclarationOnly": true,
    "outDir": "./out",
    "skipLibCheck": true
  },
  "include": ["probe.ts"]
}"#;
    std::fs::write(probe_dir.join("tsconfig.json"), tsconfig)
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

/// Run tsgo on the probe directory and return the output `.d.ts` content.
fn run_tsgo(probe_dir: &Path) -> Result<String, String> {
    // Try tsgo first, then fall back to npx @typescript/native-preview
    let tsgo_result = Command::new("tsgo")
        .args(["-p", "tsconfig.json"])
        .current_dir(probe_dir)
        .output();

    let output = match tsgo_result {
        Ok(output) if output.status.success() || probe_dir.join("out/probe.d.ts").exists() => {
            output
        }
        _ => {
            // Fall back to npx @typescript/native-preview
            let npx_result = Command::new("npx")
                .args(["@typescript/native-preview", "-p", "tsconfig.json"])
                .current_dir(probe_dir)
                .output()
                .map_err(|e| format!("failed to run tsgo or npx: {e}"))?;

            if !npx_result.status.success() && !probe_dir.join("out/probe.d.ts").exists() {
                let stderr = String::from_utf8_lossy(&npx_result.stderr);
                return Err(format!("tsgo failed: {stderr}"));
            }
            npx_result
        }
    };

    // Even if tsgo reports errors (e.g. for unused variables), check if the .d.ts was emitted
    let _ = output;
    let dts_path = probe_dir.join("out/probe.d.ts");
    if !dts_path.exists() {
        return Err("tsgo did not emit probe.d.ts".to_string());
    }

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
) -> HashMap<String, Vec<DtsExport>> {
    let mut result: HashMap<String, Vec<DtsExport>> = HashMap::new();
    let mut probe_index = 0usize;

    // Collect npm imports
    let mut imported_names: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let ItemKind::Import(decl) = &item.kind {
            let is_relative = decl.source.starts_with("./") || decl.source.starts_with("../");
            if !is_relative {
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
        // Handle Construct nodes (uppercase calls like QueryClient({...}))
        if let ExprKind::Construct { type_name, .. } = &decl.value.kind
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

        if let ExprKind::Call { callee, .. } = &decl.value.kind {
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
                    for (i, name) in names.iter().enumerate() {
                        if i < elem_types.len() {
                            result
                                .entry(specifier.clone())
                                .or_default()
                                .push(DtsExport {
                                    name: format!("__probe_{name}"),
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

/// Collect function declarations nested inside expression bodies.
fn collect_nested_functions<'a>(
    expr: &'a Expr,
    declared: &mut HashSet<String>,
    functions: &mut HashMap<String, &'a FunctionDecl>,
) {
    if let ExprKind::Block(items) = &expr.kind {
        for item in items {
            if let ItemKind::Function(decl) = &item.kind {
                declared.insert(decl.name.clone());
                functions.insert(decl.name.clone(), decl);
                collect_nested_functions(&decl.body, declared, functions);
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn generate_probe_basic_import() {
        let source = r#"import { useState } from "react"
const [count, setCount] = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new());

        assert!(probe.contains("import { useState } from \"react\";"));
        // Array binding: destructures into _r0_0, _r0_1
        assert!(probe.contains("_tmp0 = useState(0);"));
        assert!(probe.contains("export const [_r0_0, _r0_1] = _tmp0;"));
    }

    #[test]
    fn generate_probe_with_type_args() {
        let source = r#"import { useState } from "react"
type Todo = { text: string }
const [todos, setTodos] = useState<Array<Todo>>([])"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new());

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
        let probe = generate_probe(&program, &HashMap::new());

        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_re_exports_unused_imports() {
        let source = r#"import { useState, useEffect } from "react"
const [count, setCount] = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new());

        // Array binding: destructured
        assert!(probe.contains("_tmp0 = useState(0);"));
        // All imports are re-exported (useState and useEffect)
        assert!(probe.contains("= useState;"), "should re-export useState");
        assert!(probe.contains("= useEffect;"), "should re-export useEffect");
    }

    #[test]
    fn type_decl_to_ts_record() {
        let source = "type Todo = { text: string, done: bool }";
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
type Todo = { text: string, done: bool }
const [todos, setTodos] = useState<Array<Todo>>([])
const [input, setInput] = useState("")
"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program, &HashMap::new());
        eprintln!("PROBE:\n{probe}");
        let mut resolver = TsgoResolver::new(&todo_app_dir);
        let result = resolver.resolve_imports(&program, &HashMap::new());

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
type Filter = | All | Active | Completed
const [filter, setFilter] = useState<Filter>(Filter.All)
"#;
        let program = Parser::new(source).parse_program().unwrap();

        // Check what probe is generated
        let probe = generate_probe(&program, &HashMap::new());
        eprintln!("PROBE:\n{probe}");

        let mut resolver = TsgoResolver::new(&todo_app_dir);
        let result = resolver.resolve_imports(&program, &HashMap::new());

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
        let source = "type Foo = { bar: Option<string> }";
        let program = Parser::new(source).parse_program().unwrap();
        if let ItemKind::TypeDecl(decl) = &program.items[0].kind {
            let ts = type_decl_to_ts(decl);
            assert!(ts.contains("string | null | undefined"));
        } else {
            panic!("expected type decl");
        }
    }
}
