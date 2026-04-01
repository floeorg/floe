//! tsgo-based type resolution for npm imports.
//!
//! Resolves import types by:
//! 1. Running a probe file through tsgo for call-site type resolution
//! 2. Parsing .d.ts / .ts files directly for type definitions
//! 3. Querying tsgo LSP (hover) for richer type resolution

mod probe_gen;
#[cfg(feature = "cli")]
mod probe_run;
mod specifier_map;
mod typeof_resolve;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::parser::ast::*;

use super::DtsExport;
use super::TsType;
use super::dts::parse_dts_exports_from_str;

use probe_gen::generate_probe;
#[cfg(feature = "cli")]
use probe_run::{create_probe_dir, run_tsgo};
use specifier_map::build_specifier_map;

/// Resolves npm import types using probes, DTS parsing, and tsgo LSP.
pub struct TsgoResolver {
    project_dir: PathBuf,
    cache: HashMap<u64, Vec<DtsExport>>,
    /// None = not attempted, Some(None) = failed, Some(Some(_)) = ready
    #[cfg(feature = "cli")]
    lsp_client: Option<Option<super::tsgo_lsp::TsgoLspClient>>,
}

impl TsgoResolver {
    pub fn new(project_dir: &Path) -> Self {
        Self {
            project_dir: project_dir.to_path_buf(),
            cache: HashMap::new(),
            #[cfg(feature = "cli")]
            lsp_client: None,
        }
    }

    /// Get or initialize the LSP client lazily. Returns None if tsgo is
    /// unavailable (only attempts initialization once).
    #[cfg(feature = "cli")]
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
    /// Returns a map from specifier (npm or relative) to its resolved exports.
    pub fn resolve_imports(
        &mut self,
        program: &Program,
        resolved_imports: &HashMap<String, crate::resolve::ResolvedImports>,
        source_dir: &Path,
        tsconfig_paths: &crate::resolve::TsconfigPaths,
    ) -> HashMap<String, Vec<DtsExport>> {
        #[cfg(not(feature = "cli"))]
        {
            let _ = (program, resolved_imports, source_dir, tsconfig_paths);
            return HashMap::new();
        }

        #[cfg(feature = "cli")]
        {
            let ts_imports =
                find_relative_ts_imports(program, resolved_imports, source_dir, tsconfig_paths);

            // Probe-based resolution for call-site types, enhanced with
            // DTS parsing, typeof resolution, and LSP hover for unresolved types.
            let mut result = self.run_probe(program, resolved_imports, &ts_imports);
            self.enhance_import_types(&mut result, program, &ts_imports);
            result
        }
    }

    /// Run the probe system for call-site type resolution.
    #[cfg(feature = "cli")]
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

        let exports = match parse_dts_exports_from_str(&dts_content) {
            Ok(exports) => exports,
            Err(e) => {
                eprintln!("[floe] tsgo: failed to parse output: {e}");
                return HashMap::new();
            }
        };

        self.cache.insert(hash, exports.clone());
        build_specifier_map(program, &exports, ts_imports)
    }

    /// Enhance probe results with LSP/DTS parsing for better import types.
    #[cfg(feature = "cli")]
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
    }

    /// hover at the symbol's declaration site in the source file.
    #[cfg(feature = "cli")]
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
#[cfg(feature = "cli")]
fn parse_hover_to_tstype(hover: &str) -> Option<TsType> {
    let hover = hover.trim();

    // "type X = { ... }" or "interface X { ... }"
    if let Some(rest) = hover
        .strip_prefix("type ")
        .and_then(|s| s.split_once('='))
        .map(|(_, rhs)| rhs.trim())
    {
        // Try to parse as a .d.ts snippet
        let snippet = format!("export declare const _q: {rest};");
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
    if let Some(rest) = hover.strip_prefix("const ")
        && let Some((_, ty_str)) = rest.split_once(':')
    {
        let snippet = format!("export declare const _q: {};", ty_str.trim());
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
        let source = "type Todo { text: string, done: boolean }";
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

    #[test]
    fn generate_probe_jsx_callback_prop() {
        let source = r#"import { NavLink } from "react-router-dom"

fn page() {
    <NavLink className={(state) => "active"} to="/home" />
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

fn page() {
    <Button onClick={(e) => handle(e)} />
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

fn page() {
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

fn page() {
    <Draggable draggableId="id" index={0}>
        {(provided, snapshot) =>
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
        // Should reference the children prop
        assert!(
            probe.contains("[\"children\"]"),
            "probe should extract from children prop, got:\n{probe}"
        );
    }
}
