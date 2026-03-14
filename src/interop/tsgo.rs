//! tsgo-based type resolution for npm imports.
//!
//! Instead of manually converting TsType -> checker Type, we generate a
//! TypeScript "probe" file that re-exports imported symbols with concrete
//! type arguments, run tsgo (TypeScript's Go-based compiler) to emit a
//! `.d.ts`, and parse the fully-resolved types from the output.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::parser::ast::*;

use super::DtsExport;
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
    pub fn resolve_imports(&mut self, program: &Program) -> HashMap<String, Vec<DtsExport>> {
        let probe = generate_probe(program);
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
            Err(_) => return HashMap::new(),
        };

        let probe_dir = tmp.path();

        // Run tsgo
        let dts_content = match run_tsgo(probe_dir) {
            Ok(content) => content,
            Err(_) => {
                return HashMap::new();
            }
        };

        // Parse the output .d.ts
        let exports = match parse_dts_exports_from_str(&dts_content) {
            Ok(exports) => exports,
            Err(_) => return HashMap::new(),
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
}

/// Information about a plain re-export from an npm import.
struct ProbeReexport {
    /// Index for the probe variable name
    index: usize,
    /// The imported symbol name
    name: String,
}

/// Generate the TypeScript probe file content from a Floe program.
fn generate_probe(program: &Program) -> String {
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

    // Track probe calls and re-exports
    let mut probe_calls: Vec<ProbeCall> = Vec::new();
    let mut probe_reexports: Vec<ProbeReexport> = Vec::new();

    // Scan const declarations for calls to imported functions
    for item in &program.items {
        if let ItemKind::Const(decl) = &item.kind {
            if let ExprKind::Call {
                callee,
                type_args,
                args,
            } = &decl.value.kind
            {
                let callee_name = expr_to_callee_name(callee);
                if let Some(name) = &callee_name
                    && imported_names.contains_key(name)
                    && !type_args.is_empty()
                {
                    // This is a call to an imported function with type args
                    let ts_type_args: Vec<String> = type_args.iter().map(type_expr_to_ts).collect();
                    let ts_args: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
                    probe_calls.push(ProbeCall {
                        index: probe_index,
                        callee: name.clone(),
                        type_args: ts_type_args,
                        args: ts_args,
                    });
                    probe_index += 1;
                    continue;
                }
            }

            // For calls without type args, we still need to probe to get return types
            if let ExprKind::Call { callee, args, .. } = &decl.value.kind {
                let callee_name = expr_to_callee_name(callee);
                if let Some(name) = &callee_name
                    && imported_names.contains_key(name)
                {
                    let ts_args: Vec<String> = args.iter().map(arg_to_ts_approx).collect();
                    probe_calls.push(ProbeCall {
                        index: probe_index,
                        callee: name.clone(),
                        type_args: Vec::new(),
                        args: ts_args,
                    });
                    probe_index += 1;
                    continue;
                }
            }
        }
    }

    // Re-export ALL imported names so we get their types
    // (even if they were also used in calls above)
    for name in imported_names.keys() {
        probe_reexports.push(ProbeReexport {
            index: probe_index,
            name: name.clone(),
        });
        probe_index += 1;
    }

    // Emit probe const declarations
    for call in &probe_calls {
        let type_args_str = if call.type_args.is_empty() {
            String::new()
        } else {
            format!("<{}>", call.type_args.join(", "))
        };
        let args_str = call.args.join(", ");
        lines.push(format!(
            "export const _r{} = {}{type_args_str}({args_str});",
            call.index, call.callee,
        ));
    }

    // Emit re-exports for non-called imports
    for reexport in &probe_reexports {
        lines.push(format!(
            "export const _r{} = {};",
            reexport.index, reexport.name,
        ));
    }

    if probe_index == 0 {
        return String::new();
    }

    lines.join("\n") + "\n"
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
            // Floe tagged unions don't have a direct TS equivalent;
            // emit a simple union of string literals as a placeholder
            let parts: Vec<String> = variants.iter().map(|v| format!("\"{}\"", v.name)).collect();
            format!("type {}{type_params} = {};", decl.name, parts.join(" | "))
        }
    }
}

/// Convert a Floe TypeExpr to a TypeScript type string.
fn type_expr_to_ts(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named { name, type_args } => {
            let ts_name = match name.as_str() {
                "bool" => "boolean",
                "Option" if type_args.len() == 1 => {
                    let inner = type_expr_to_ts(&type_args[0]);
                    return format!("{inner} | null | undefined");
                }
                "Result" if type_args.len() == 2 => {
                    // Result<T, E> doesn't have a TS equivalent; use T
                    return type_expr_to_ts(&type_args[0]);
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
            format!("[{}]", ps.join(", "))
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
    "strict": true,
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

    // Track which names were used in calls (same logic as generate_probe)
    let mut called_names = std::collections::HashSet::new();

    // Map call probe results
    for item in &program.items {
        if let ItemKind::Const(decl) = &item.kind {
            if let ExprKind::Call {
                callee, type_args, ..
            } = &decl.value.kind
            {
                let callee_name = expr_to_callee_name(callee);
                if let Some(name) = &callee_name
                    && imported_names.contains_key(name)
                    && !type_args.is_empty()
                {
                    called_names.insert(name.clone());
                    // Find the corresponding probe export
                    let probe_name = format!("_r{probe_index}");
                    if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                        let specifier = &imported_names[name];
                        // Create a DtsExport with the original const name
                        let binding_name = const_binding_name(&decl.binding);
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
            }

            if let ExprKind::Call { callee, .. } = &decl.value.kind {
                let callee_name = expr_to_callee_name(callee);
                if let Some(name) = &callee_name
                    && imported_names.contains_key(name)
                {
                    called_names.insert(name.clone());
                    let probe_name = format!("_r{probe_index}");
                    if let Some(export) = probe_exports.iter().find(|e| e.name == probe_name) {
                        let specifier = &imported_names[name];
                        let binding_name = const_binding_name(&decl.binding);
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
            }
        }
    }

    // Map re-export probe results — ALL imported names
    for (name, specifier) in &imported_names {
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

    result
}

/// Get the binding name from a ConstBinding for identification purposes.
fn const_binding_name(binding: &ConstBinding) -> String {
    match binding {
        ConstBinding::Name(name) => name.clone(),
        ConstBinding::Array(names) => names.join("_"),
        ConstBinding::Object(names) => names.join("_"),
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
        let probe = generate_probe(&program);

        assert!(probe.contains("import { useState } from \"react\";"));
        assert!(probe.contains("export const _r0 = useState(0);"));
    }

    #[test]
    fn generate_probe_with_type_args() {
        let source = r#"import { useState } from "react"
type Todo = { text: string }
const [todos, setTodos] = useState<Array<Todo>>([])"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program);

        assert!(probe.contains("import { useState } from \"react\";"));
        assert!(probe.contains("type Todo = {"));
        assert!(probe.contains("export const _r0 = useState<Array<Todo>>([]);"));
    }

    #[test]
    fn generate_probe_empty_for_no_npm_imports() {
        let source = r#"import { foo } from "./local"
const x = 42"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program);

        assert!(probe.is_empty());
    }

    #[test]
    fn generate_probe_re_exports_unused_imports() {
        let source = r#"import { useState, useEffect } from "react"
const [count, setCount] = useState(0)"#;
        let program = Parser::new(source).parse_program().unwrap();
        let probe = generate_probe(&program);

        // useState should be a probe call, both should be re-exported
        assert!(probe.contains("export const _r0 = useState(0);"));
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
