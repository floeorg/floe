//! Ambient type loading from TypeScript lib definition files.
//!
//! Parses `lib.dom.d.ts` (and related lib files) to extract:
//! - `declare var` / `declare function` → global variable/function types
//! - `interface` definitions → for resolving member access on globals
//!
//! This replaces the hardcoded browser globals in checker.rs with real types.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::Parser;
use oxc_span::SourceType;

use super::dts::{
    collect_interface_info, convert_function, convert_oxc_type, resolve_interface_fields,
};
use super::ts_types::ObjectField;
use super::wrapper::wrap_boundary_type;
use crate::checker::Type;

/// Ambient declarations parsed from TypeScript lib files.
pub struct AmbientDeclarations {
    /// Global variables/functions (e.g., `window`, `document`, `navigator`, `fetch`).
    /// These are registered directly in the checker's type environment.
    pub globals: Vec<(String, Type)>,
    /// Type definitions (interfaces) for resolving member access.
    /// e.g., `Window`, `Navigator`, `Console`, `Location` — used when the
    /// checker resolves `Foreign("Window")` to a concrete record type.
    pub types: HashMap<String, Type>,
}

/// Find the TypeScript lib directory from a project root.
///
/// Searches for `node_modules/typescript/lib/` in standard locations
/// (npm/yarn hoisted, pnpm symlinks, and pnpm `.pnpm/` store).
fn find_ts_lib_dir(project_dir: &Path) -> Option<PathBuf> {
    // Standard location (npm, yarn, hoisted pnpm)
    let standard = project_dir.join("node_modules/typescript/lib");
    if standard.is_dir() {
        return Some(standard);
    }

    // pnpm: check .pnpm store — find the latest typescript version
    let pnpm_dir = project_dir.join("node_modules/.pnpm");
    if pnpm_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&pnpm_dir) {
            let mut ts_dirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with("typescript@"))
                })
                .map(|e| e.path().join("node_modules/typescript/lib"))
                .filter(|p| p.is_dir())
                .collect();
            // Sort to get the latest version (lexicographic is fine for semver prefixed dirs)
            ts_dirs.sort();
            if let Some(dir) = ts_dirs.pop() {
                return Some(dir);
            }
        }
    }

    None
}

/// Load ambient type declarations from the project's TypeScript installation.
///
/// Returns `None` if TypeScript is not installed or lib files can't be parsed.
pub fn load_ambient_types(project_dir: &Path) -> Option<AmbientDeclarations> {
    let lib_dir = find_ts_lib_dir(project_dir)?;
    let dom_path = lib_dir.join("lib.dom.d.ts");

    if !dom_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&dom_path).ok()?;
    Some(parse_ambient_lib(&content))
}

/// Parse ambient declarations from a TypeScript lib file.
fn parse_ambient_lib(content: &str) -> AmbientDeclarations {
    let allocator = Allocator::default();
    let source_type = SourceType::d_ts();
    let ret = Parser::new(&allocator, content, source_type).parse();

    if ret.panicked {
        return AmbientDeclarations {
            globals: Vec::new(),
            types: HashMap::new(),
        };
    }

    // ── Phase 1: Collect all interface definitions ──────────────
    let mut interface_bodies: HashMap<String, Vec<ObjectField>> = HashMap::new();
    let mut interface_extends: HashMap<String, Vec<String>> = HashMap::new();

    for stmt in &ret.program.body {
        collect_interface_info(stmt, &mut interface_bodies, &mut interface_extends);
    }

    // Resolve extends chains: merge parent fields into each interface
    let resolved_names: Vec<String> = interface_extends.keys().cloned().collect();
    for name in &resolved_names {
        let fields = resolve_interface_fields(name, &interface_bodies, &interface_extends);
        interface_bodies.insert(name.clone(), fields);
    }

    // Convert interface bodies to Floe types
    let mut types: HashMap<String, Type> = HashMap::new();
    for (name, fields) in &interface_bodies {
        let ts_type = super::TsType::Object(fields.clone());
        let floe_type = wrap_boundary_type(&ts_type);
        types.insert(name.clone(), floe_type);
    }

    // ── Phase 2: Collect `declare var` and `declare function` globals ──
    let mut globals: Vec<(String, Type)> = Vec::new();
    let mut seen_globals: HashSet<String> = HashSet::new();

    for stmt in &ret.program.body {
        match stmt {
            // `declare var name: Type;`
            Statement::VariableDeclaration(var_decl) if var_decl.declare => {
                for declarator in &var_decl.declarations {
                    let name = match &declarator.id {
                        oxc_ast::ast::BindingPattern::BindingIdentifier(ident) => {
                            ident.name.to_string()
                        }
                        _ => continue,
                    };
                    if !seen_globals.insert(name.clone()) {
                        continue;
                    }
                    let ts_type = declarator
                        .type_annotation
                        .as_ref()
                        .map(|ta| convert_oxc_type(&ta.type_annotation))
                        .unwrap_or(super::TsType::Any);
                    let floe_type = wrap_boundary_type(&ts_type);
                    globals.push((name, floe_type));
                }
            }

            // `declare function name(...): ReturnType;`
            Statement::FunctionDeclaration(func) if func.declare => {
                if let Some(ref id) = func.id {
                    let name = id.name.to_string();
                    if seen_globals.insert(name.clone()) {
                        let ts_type = convert_function(&func.params, &func.return_type);
                        let floe_type = wrap_boundary_type(&ts_type);
                        globals.push((name, floe_type));
                    }
                }
            }

            _ => {}
        }
    }

    AmbientDeclarations { globals, types }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_declare_var() {
        let content = r#"
            interface Window {
                location: Location;
                innerWidth: number;
            }
            interface Location {
                href: string;
                origin: string;
            }
            declare var window: Window;
            declare var document: Document;
        "#;

        let result = parse_ambient_lib(content);

        // Should find 2 globals
        assert_eq!(result.globals.len(), 2);
        assert_eq!(result.globals[0].0, "window");
        assert_eq!(result.globals[1].0, "document");

        // window should resolve to Foreign("Window")
        assert!(
            matches!(&result.globals[0].1, Type::Foreign(name) if name == "Window"),
            "expected Foreign(\"Window\"), got {:?}",
            result.globals[0].1
        );

        // Window interface should be in types
        assert!(result.types.contains_key("Window"));
        assert!(result.types.contains_key("Location"));

        // Window should be a Record with location and innerWidth fields
        if let Type::Record(fields) = &result.types["Window"] {
            assert!(fields.iter().any(|(name, _)| name == "location"));
            assert!(fields.iter().any(|(name, _)| name == "innerWidth"));
        } else {
            panic!(
                "Window should be a Record, got {:?}",
                result.types["Window"]
            );
        }
    }

    #[test]
    fn parse_declare_function() {
        let content = r#"
            declare function setTimeout(handler: () => void, timeout: number): number;
            declare function clearTimeout(id: number): void;
        "#;

        let result = parse_ambient_lib(content);

        assert_eq!(result.globals.len(), 2);
        assert_eq!(result.globals[0].0, "setTimeout");
        assert_eq!(result.globals[1].0, "clearTimeout");

        // setTimeout should be a function
        assert!(
            matches!(&result.globals[0].1, Type::Function { .. }),
            "expected Function, got {:?}",
            result.globals[0].1
        );
    }

    #[test]
    fn parse_interface_extends() {
        let content = r#"
            interface NavigatorID {
                userAgent: string;
            }
            interface NavigatorLanguage {
                language: string;
            }
            interface Navigator extends NavigatorID, NavigatorLanguage {
                clipboard: Clipboard;
            }
            interface Clipboard {
                writeText(text: string): Promise<void>;
            }
            declare var navigator: Navigator;
        "#;

        let result = parse_ambient_lib(content);

        // Navigator should have fields from all parents + own
        if let Type::Record(fields) = &result.types["Navigator"] {
            let field_names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
            assert!(field_names.contains(&"userAgent"), "missing userAgent");
            assert!(field_names.contains(&"language"), "missing language");
            assert!(field_names.contains(&"clipboard"), "missing clipboard");
        } else {
            panic!(
                "Navigator should be a Record, got {:?}",
                result.types["Navigator"]
            );
        }
    }

    #[test]
    fn intersection_type_takes_first() {
        let content = r#"
            interface Window {
                location: Location;
            }
            declare var window: Window & typeof globalThis;
        "#;

        let result = parse_ambient_lib(content);

        // Should resolve to Foreign("Window"), not Unknown
        assert!(
            matches!(&result.globals[0].1, Type::Foreign(name) if name == "Window"),
            "expected Foreign(\"Window\"), got {:?}",
            result.globals[0].1
        );
    }
}
