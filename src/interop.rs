//! npm / .d.ts interop module.
//!
//! Resolves npm modules by shelling out to `tsc`, parses the type
//! declarations from `.d.ts` files, and wraps types at the import
//! boundary so they conform to ZenScript semantics.
//!
//! Boundary conversions:
//! - `T | null`          → `Option<T>`
//! - `T | undefined`     → `Option<T>`
//! - `T | null | undefined` → `Option<T>`
//! - `any`               → `unknown`

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::checker::Type;

// ── Module Resolution ───────────────────────────────────────────

/// Result of resolving an npm module.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Absolute path to the .d.ts file
    pub dts_path: PathBuf,
    /// The module specifier as written in the import (e.g. "react")
    pub specifier: String,
}

/// Result of looking up exports from a resolved module.
#[derive(Debug, Clone)]
pub struct ModuleExports {
    /// Named exports: name -> wrapped ZenScript type
    pub exports: HashMap<String, Type>,
    /// The module specifier
    pub specifier: String,
}

/// Resolves an npm module specifier to its .d.ts file path using tsc.
///
/// Runs `tsc --moduleResolution bundler --noEmit --traceResolution` to find
/// the declaration file for a given module specifier.
pub fn resolve_module(specifier: &str, project_dir: &Path) -> Result<ResolvedModule, String> {
    // First try: look for a tsconfig.json and use tsc's resolution
    let tsconfig = find_tsconfig(project_dir);

    let mut cmd = Command::new("tsc");
    cmd.current_dir(project_dir);

    if let Some(tsconfig_path) = &tsconfig {
        cmd.arg("--project").arg(tsconfig_path);
    }

    cmd.args(["--noEmit", "--traceResolution"]);

    // Create a temp file that imports the module so tsc resolves it
    let probe_content = format!("import {{}} from \"{specifier}\";");
    let probe_path = project_dir.join("__zs_probe__.ts");

    if std::fs::write(&probe_path, &probe_content).is_err() {
        return Err(format!(
            "failed to create probe file for module '{specifier}'"
        ));
    }

    cmd.arg(&probe_path);

    let output = cmd.output().map_err(|e| {
        let _ = std::fs::remove_file(&probe_path);
        format!("failed to run tsc: {e}. Is TypeScript installed?")
    })?;

    let _ = std::fs::remove_file(&probe_path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}\n{stderr}");

    // Parse tsc's trace resolution output to find the .d.ts path
    // Look for lines like: "File '.../node_modules/@types/react/index.d.ts' exists"
    // or "Resolution for module 'react' was found in cache from location..."
    parse_resolved_path(&combined, specifier)
}

/// Parses tsc trace output to extract the resolved .d.ts path.
fn parse_resolved_path(trace: &str, specifier: &str) -> Result<ResolvedModule, String> {
    // Pattern 1: "======== Module name 'X' was successfully resolved to 'Y'. ========"
    let success_marker = "was successfully resolved to '";
    for line in trace.lines() {
        if line.contains(&format!("Module name '{specifier}'"))
            && let Some(start) = line.find(success_marker)
        {
            let rest = &line[start + success_marker.len()..];
            if let Some(end) = rest.find("'.") {
                let dts_path = &rest[..end];
                return Ok(ResolvedModule {
                    dts_path: PathBuf::from(dts_path),
                    specifier: specifier.to_string(),
                });
            }
        }
    }

    // Pattern 2: look for resolved .d.ts in node_modules
    // Try common locations directly
    Err(format!(
        "could not resolve module '{specifier}'. Make sure the package is installed (npm install)"
    ))
}

/// Finds the nearest tsconfig.json by walking up from project_dir.
fn find_tsconfig(dir: &Path) -> Option<PathBuf> {
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

// ── Type Declaration Parsing ────────────────────────────────────

/// A raw TypeScript type as parsed from .d.ts files, before boundary wrapping.
#[derive(Debug, Clone, PartialEq)]
pub enum TsType {
    /// Primitive: string, number, boolean, void, never
    Primitive(String),
    /// `null`
    Null,
    /// `undefined`
    Undefined,
    /// `any`
    Any,
    /// `unknown`
    Unknown,
    /// Named type reference: `Element`, `HTMLDivElement`
    Named(String),
    /// Generic type: `Promise<T>`, `Array<T>`
    Generic { name: String, args: Vec<TsType> },
    /// Union: `T | U | V`
    Union(Vec<TsType>),
    /// Function: `(params) => ReturnType`
    Function {
        params: Vec<TsType>,
        return_type: Box<TsType>,
    },
    /// Array shorthand: `T[]`
    Array(Box<TsType>),
    /// Object type / record
    Object(Vec<(String, TsType)>),
    /// Tuple: `[T, U]`
    Tuple(Vec<TsType>),
}

/// Convert a TsType to a human-readable string for display.
pub fn ts_type_to_string(ty: &TsType) -> String {
    match ty {
        TsType::Primitive(s) => s.clone(),
        TsType::Null => "null".to_string(),
        TsType::Undefined => "undefined".to_string(),
        TsType::Any => "any".to_string(),
        TsType::Unknown => "unknown".to_string(),
        TsType::Named(n) => n.clone(),
        TsType::Generic { name, args } => {
            let args_str: Vec<String> = args.iter().map(ts_type_to_string).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        TsType::Union(parts) => {
            let parts_str: Vec<String> = parts.iter().map(ts_type_to_string).collect();
            parts_str.join(" | ")
        }
        TsType::Function {
            params,
            return_type,
        } => {
            let params_str: Vec<String> = params.iter().map(ts_type_to_string).collect();
            format!(
                "({}) => {}",
                params_str.join(", "),
                ts_type_to_string(return_type)
            )
        }
        TsType::Array(inner) => format!("Array<{}>", ts_type_to_string(inner)),
        TsType::Object(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|(n, t)| format!("{}: {}", n, ts_type_to_string(t)))
                .collect();
            format!("{{ {} }}", fs.join(", "))
        }
        TsType::Tuple(parts) => {
            let ps: Vec<String> = parts.iter().map(ts_type_to_string).collect();
            format!("[{}]", ps.join(", "))
        }
    }
}

/// An export entry from a .d.ts file.
#[derive(Debug, Clone)]
pub struct DtsExport {
    pub name: String,
    pub ts_type: TsType,
}

/// Reads a .d.ts file and extracts its named exports.
///
/// This is a simplified parser that handles common patterns in .d.ts files.
/// For full fidelity, a production implementation would use tsserver's API.
pub fn parse_dts_exports(dts_path: &Path) -> Result<Vec<DtsExport>, String> {
    let content = std::fs::read_to_string(dts_path)
        .map_err(|e| format!("failed to read {}: {e}", dts_path.display()))?;

    let mut exports = Vec::new();
    let mut lines = content.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // export function name(params): ReturnType;
        if let Some(rest) = trimmed
            .strip_prefix("export function ")
            .or_else(|| trimmed.strip_prefix("export declare function "))
        {
            if let Some(export) = parse_function_export(rest) {
                exports.push(export);
            }
        }
        // export const name: Type;
        else if let Some(rest) = trimmed
            .strip_prefix("export const ")
            .or_else(|| trimmed.strip_prefix("export declare const "))
        {
            if let Some(export) = parse_const_export(rest) {
                exports.push(export);
            }
        }
        // export type name = Type;
        else if let Some(rest) = trimmed
            .strip_prefix("export type ")
            .or_else(|| trimmed.strip_prefix("export declare type "))
        {
            if let Some(export) = parse_type_export(rest) {
                exports.push(export);
            }
        }
        // export interface Name { ... }
        else if let Some(rest) = trimmed
            .strip_prefix("export interface ")
            .or_else(|| trimmed.strip_prefix("export declare interface "))
            && let Some(export) = parse_interface_export(rest, &mut lines)
        {
            exports.push(export);
        }
    }

    Ok(exports)
}

fn parse_function_export(rest: &str) -> Option<DtsExport> {
    // name(params): ReturnType;
    let paren = rest.find('(')?;
    let name = rest[..paren].trim().to_string();

    // Find matching close paren (handle nested parens)
    let after_name = &rest[paren..];
    let close = find_matching_paren(after_name)?;
    let params_str = &after_name[1..close];
    let after_params = after_name[close + 1..].trim();

    let params = parse_param_types(params_str);

    let return_type = if let Some(ret_str) = after_params.strip_prefix(':') {
        let ret_str = ret_str.trim().trim_end_matches(';').trim();
        parse_type_str(ret_str)
    } else {
        TsType::Primitive("void".to_string())
    };

    Some(DtsExport {
        name,
        ts_type: TsType::Function {
            params,
            return_type: Box::new(return_type),
        },
    })
}

fn parse_const_export(rest: &str) -> Option<DtsExport> {
    // name: Type;
    let colon = rest.find(':')?;
    let name = rest[..colon].trim().to_string();
    let type_str = rest[colon + 1..].trim().trim_end_matches(';').trim();
    let ts_type = parse_type_str(type_str);

    Some(DtsExport { name, ts_type })
}

fn parse_type_export(rest: &str) -> Option<DtsExport> {
    // Name = Type;
    let eq = rest.find('=')?;
    let name = rest[..eq].trim().to_string();
    // Strip generic params from name if present
    let name = if let Some(angle) = name.find('<') {
        name[..angle].trim().to_string()
    } else {
        name
    };
    let type_str = rest[eq + 1..].trim().trim_end_matches(';').trim();
    let ts_type = parse_type_str(type_str);

    Some(DtsExport { name, ts_type })
}

fn parse_interface_export(
    rest: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'_>>,
) -> Option<DtsExport> {
    // Name { ... } or Name extends ... { ... }
    let name_end = rest
        .find('{')
        .or_else(|| rest.find("extends"))
        .unwrap_or(rest.len());
    let name = rest[..name_end].trim().to_string();
    // Strip generic params
    let name = if let Some(angle) = name.find('<') {
        name[..angle].trim().to_string()
    } else {
        name
    };

    // Collect interface body fields
    let mut fields = Vec::new();
    let mut brace_depth: i32 = if rest.contains('{') { 1 } else { 0 };

    // If opening brace wasn't on this line, skip to it
    if brace_depth == 0 {
        for line in lines.by_ref() {
            if line.contains('{') {
                brace_depth = 1;
                break;
            }
        }
    }

    while brace_depth > 0 {
        if let Some(line) = lines.next() {
            let trimmed = line.trim();
            brace_depth += trimmed.chars().filter(|&c| c == '{').count() as i32;
            brace_depth -= trimmed.chars().filter(|&c| c == '}').count() as i32;

            if brace_depth > 0 {
                // Parse field: name: Type; or name?: Type;
                if let Some(colon) = trimmed.find(':') {
                    let field_name = trimmed[..colon]
                        .trim()
                        .trim_end_matches('?')
                        .trim_start_matches("readonly ")
                        .trim()
                        .to_string();
                    let type_str = trimmed[colon + 1..].trim().trim_end_matches(';').trim();
                    if !field_name.is_empty() && !field_name.starts_with('[') {
                        fields.push((field_name, parse_type_str(type_str)));
                    }
                }
            }
        } else {
            break;
        }
    }

    Some(DtsExport {
        name,
        ts_type: TsType::Object(fields),
    })
}

// ── Type String Parsing ─────────────────────────────────────────

/// Parses a TypeScript type string into a TsType.
fn parse_type_str(s: &str) -> TsType {
    let s = s.trim();

    if s.is_empty() {
        return TsType::Primitive("void".to_string());
    }

    // Union types: T | U | V (split at top-level |)
    let union_parts = split_at_top_level(s, '|');
    if union_parts.len() > 1 {
        let parts: Vec<TsType> = union_parts
            .iter()
            .map(|part| parse_type_str(part.trim()))
            .collect();
        return TsType::Union(parts);
    }

    // Array shorthand: T[]
    if let Some(inner) = s.strip_suffix("[]") {
        return TsType::Array(Box::new(parse_type_str(inner)));
    }

    // Tuple: [T, U]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let parts = split_at_top_level(inner, ',');
        return TsType::Tuple(parts.iter().map(|p| parse_type_str(p.trim())).collect());
    }

    // Function type: (params) => ReturnType
    if s.starts_with('(')
        && let Some(close) = find_matching_paren(s)
    {
        let params_str = &s[1..close];
        let after = s[close + 1..].trim();
        if let Some(ret_str) = after.strip_prefix("=>") {
            let params = parse_param_types(params_str);
            let return_type = parse_type_str(ret_str.trim());
            return TsType::Function {
                params,
                return_type: Box::new(return_type),
            };
        }
    }

    // Generic: Name<T, U>
    if let Some(angle) = s.find('<')
        && s.ends_with('>')
    {
        let name = s[..angle].trim().to_string();
        let args_str = &s[angle + 1..s.len() - 1];
        let args = split_at_top_level(args_str, ',');
        let args: Vec<TsType> = args.iter().map(|a| parse_type_str(a.trim())).collect();

        // Normalize Array<T> to array
        if name == "Array" && args.len() == 1 {
            return TsType::Array(Box::new(args.into_iter().next().unwrap()));
        }

        return TsType::Generic { name, args };
    }

    // Object literal: { ... }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return TsType::Object(Vec::new());
        }
        let parts = split_at_top_level(inner, ';');
        let fields: Vec<(String, TsType)> = parts
            .iter()
            .filter_map(|part| {
                let part = part.trim();
                if part.is_empty() {
                    return None;
                }
                let colon = part.find(':')?;
                let name = part[..colon]
                    .trim()
                    .trim_end_matches('?')
                    .trim_start_matches("readonly ")
                    .to_string();
                let ty = parse_type_str(part[colon + 1..].trim());
                Some((name, ty))
            })
            .collect();
        return TsType::Object(fields);
    }

    // Primitives and special types
    match s {
        "string" => TsType::Primitive("string".to_string()),
        "number" => TsType::Primitive("number".to_string()),
        "boolean" => TsType::Primitive("boolean".to_string()),
        "void" => TsType::Primitive("void".to_string()),
        "never" => TsType::Primitive("never".to_string()),
        "bigint" => TsType::Primitive("bigint".to_string()),
        "symbol" => TsType::Primitive("symbol".to_string()),
        "null" => TsType::Null,
        "undefined" => TsType::Undefined,
        "any" => TsType::Any,
        "unknown" => TsType::Unknown,
        _ => TsType::Named(s.to_string()),
    }
}

/// Parse parameter types from a param string like "x: string, y: number".
fn parse_param_types(params_str: &str) -> Vec<TsType> {
    if params_str.trim().is_empty() {
        return Vec::new();
    }
    let parts = split_at_top_level(params_str, ',');
    parts
        .iter()
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            // "name: Type" or "name?: Type" or "...name: Type"
            if let Some(colon) = part.find(':') {
                Some(parse_type_str(part[colon + 1..].trim()))
            } else {
                // Bare type (rare in .d.ts but handle it)
                Some(parse_type_str(part))
            }
        })
        .collect()
}

/// Split a string at top-level occurrences of a delimiter (not inside <>, (), [], {}).
fn split_at_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32; // tracks <, (, [, {

    for ch in s.chars() {
        match ch {
            '<' | '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            c if c == delim && depth == 0 => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() || !parts.is_empty() {
        parts.push(current);
    }

    parts
}

/// Find the matching close parenthesis in a string starting with '('.
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

// ── Boundary Wrapping ───────────────────────────────────────────

/// Converts a TypeScript type to a ZenScript type, applying boundary wrapping:
/// - `T | null` → `Option<T>`
/// - `T | undefined` → `Option<T>`
/// - `T | null | undefined` → `Option<T>`
/// - `any` → `unknown`
pub fn wrap_boundary_type(ts_type: &TsType) -> Type {
    match ts_type {
        TsType::Primitive(name) => match name.as_str() {
            "string" => Type::String,
            "number" => Type::Number,
            "boolean" => Type::Bool,
            "void" => Type::Unit,
            "never" => Type::Unit,
            _ => Type::Unknown,
        },

        TsType::Null | TsType::Undefined => Type::Undefined,

        // any → unknown (forces narrowing in ZenScript)
        TsType::Any => Type::Unknown,

        TsType::Unknown => Type::Unknown,

        TsType::Named(name) => Type::Named(name.clone()),

        TsType::Generic { name, args } => {
            match name.as_str() {
                "Promise" if args.len() == 1 => {
                    // Promise<T> stays as a named type
                    Type::Named(format!("Promise<{:?}>", wrap_boundary_type(&args[0])))
                }
                _ => {
                    // Generic named type
                    Type::Named(name.clone())
                }
            }
        }

        TsType::Union(parts) => wrap_union_boundary(parts),

        TsType::Function {
            params,
            return_type,
        } => {
            let wrapped_params: Vec<Type> = params.iter().map(wrap_boundary_type).collect();
            let wrapped_return = wrap_boundary_type(return_type);
            Type::Function {
                params: wrapped_params,
                return_type: Box::new(wrapped_return),
            }
        }

        TsType::Array(inner) => Type::Array(Box::new(wrap_boundary_type(inner))),

        TsType::Object(fields) => {
            let wrapped: Vec<(String, Type)> = fields
                .iter()
                .map(|(name, ty)| (name.clone(), wrap_boundary_type(ty)))
                .collect();
            Type::Record(wrapped)
        }

        TsType::Tuple(parts) => Type::Tuple(parts.iter().map(wrap_boundary_type).collect()),
    }
}

/// Wraps a union type at the boundary, converting null/undefined members to Option.
fn wrap_union_boundary(parts: &[TsType]) -> Type {
    let has_null = parts.iter().any(|p| matches!(p, TsType::Null));
    let has_undefined = parts.iter().any(|p| matches!(p, TsType::Undefined));
    let nullable = has_null || has_undefined;

    // Filter out null and undefined from the union
    let non_null_parts: Vec<&TsType> = parts
        .iter()
        .filter(|p| !matches!(p, TsType::Null | TsType::Undefined))
        .collect();

    let inner_type = if non_null_parts.len() == 1 {
        wrap_boundary_type(non_null_parts[0])
    } else if non_null_parts.is_empty() {
        // `null | undefined` → Option<Void> (shouldn't happen in practice)
        Type::Unit
    } else {
        // Multi-type union without null/undefined: stay as Unknown for now
        // A full implementation would create proper union types
        Type::Unknown
    };

    if nullable {
        Type::Option(Box::new(inner_type))
    } else {
        inner_type
    }
}

/// Resolves a module specifier and returns wrapped ZenScript types for its exports.
///
/// This is the main entry point: resolve the module, parse its .d.ts,
/// and wrap all exported types at the boundary.
pub fn resolve_and_wrap(specifier: &str, project_dir: &Path) -> Result<ModuleExports, String> {
    let resolved = resolve_module(specifier, project_dir)?;
    let dts_exports = parse_dts_exports(&resolved.dts_path)?;

    let mut exports = HashMap::new();
    for export in dts_exports {
        let wrapped = wrap_boundary_type(&export.ts_type);
        exports.insert(export.name, wrapped);
    }

    Ok(ModuleExports {
        exports,
        specifier: specifier.to_string(),
    })
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Type Parsing ────────────────────────────────────────────

    #[test]
    fn parse_primitive_string() {
        assert_eq!(
            parse_type_str("string"),
            TsType::Primitive("string".to_string())
        );
    }

    #[test]
    fn parse_primitive_number() {
        assert_eq!(
            parse_type_str("number"),
            TsType::Primitive("number".to_string())
        );
    }

    #[test]
    fn parse_null() {
        assert_eq!(parse_type_str("null"), TsType::Null);
    }

    #[test]
    fn parse_undefined() {
        assert_eq!(parse_type_str("undefined"), TsType::Undefined);
    }

    #[test]
    fn parse_any() {
        assert_eq!(parse_type_str("any"), TsType::Any);
    }

    #[test]
    fn parse_named() {
        assert_eq!(
            parse_type_str("Element"),
            TsType::Named("Element".to_string())
        );
    }

    #[test]
    fn parse_union() {
        let ty = parse_type_str("string | null");
        assert_eq!(
            ty,
            TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null,])
        );
    }

    #[test]
    fn parse_union_three() {
        let ty = parse_type_str("string | null | undefined");
        assert_eq!(
            ty,
            TsType::Union(vec![
                TsType::Primitive("string".to_string()),
                TsType::Null,
                TsType::Undefined,
            ])
        );
    }

    #[test]
    fn parse_array_shorthand() {
        let ty = parse_type_str("string[]");
        assert_eq!(
            ty,
            TsType::Array(Box::new(TsType::Primitive("string".to_string())))
        );
    }

    #[test]
    fn parse_generic_array() {
        let ty = parse_type_str("Array<string>");
        assert_eq!(
            ty,
            TsType::Array(Box::new(TsType::Primitive("string".to_string())))
        );
    }

    #[test]
    fn parse_generic_promise() {
        let ty = parse_type_str("Promise<string>");
        assert_eq!(
            ty,
            TsType::Generic {
                name: "Promise".to_string(),
                args: vec![TsType::Primitive("string".to_string())],
            }
        );
    }

    #[test]
    fn parse_tuple() {
        let ty = parse_type_str("[string, number]");
        assert_eq!(
            ty,
            TsType::Tuple(vec![
                TsType::Primitive("string".to_string()),
                TsType::Primitive("number".to_string()),
            ])
        );
    }

    #[test]
    fn parse_function_type() {
        let ty = parse_type_str("(x: string) => void");
        assert_eq!(
            ty,
            TsType::Function {
                params: vec![TsType::Primitive("string".to_string())],
                return_type: Box::new(TsType::Primitive("void".to_string())),
            }
        );
    }

    // ── Boundary Wrapping ───────────────────────────────────────

    #[test]
    fn wrap_string_stays_string() {
        let ty = wrap_boundary_type(&TsType::Primitive("string".to_string()));
        assert_eq!(ty, Type::String);
    }

    #[test]
    fn wrap_number_stays_number() {
        let ty = wrap_boundary_type(&TsType::Primitive("number".to_string()));
        assert_eq!(ty, Type::Number);
    }

    #[test]
    fn wrap_boolean_becomes_bool() {
        let ty = wrap_boundary_type(&TsType::Primitive("boolean".to_string()));
        assert_eq!(ty, Type::Bool);
    }

    #[test]
    fn wrap_any_becomes_unknown() {
        let ty = wrap_boundary_type(&TsType::Any);
        assert_eq!(ty, Type::Unknown);
    }

    #[test]
    fn wrap_null_union_becomes_option() {
        // string | null → Option<String>
        let ts = TsType::Union(vec![TsType::Primitive("string".to_string()), TsType::Null]);
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(wrapped, Type::Option(Box::new(Type::String)));
    }

    #[test]
    fn wrap_undefined_union_becomes_option() {
        // number | undefined → Option<Number>
        let ts = TsType::Union(vec![
            TsType::Primitive("number".to_string()),
            TsType::Undefined,
        ]);
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(wrapped, Type::Option(Box::new(Type::Number)));
    }

    #[test]
    fn wrap_null_undefined_union_becomes_option() {
        // string | null | undefined → Option<String>
        let ts = TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Null,
            TsType::Undefined,
        ]);
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(wrapped, Type::Option(Box::new(Type::String)));
    }

    #[test]
    fn wrap_plain_union_stays_non_option() {
        // string | number → Unknown (multi-type union without null)
        let ts = TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Primitive("number".to_string()),
        ]);
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(wrapped, Type::Unknown);
    }

    #[test]
    fn wrap_function_wraps_params_and_return() {
        // (x: string | null) => any
        let ts = TsType::Function {
            params: vec![TsType::Union(vec![
                TsType::Primitive("string".to_string()),
                TsType::Null,
            ])],
            return_type: Box::new(TsType::Any),
        };
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(
            wrapped,
            Type::Function {
                params: vec![Type::Option(Box::new(Type::String))],
                return_type: Box::new(Type::Unknown),
            }
        );
    }

    #[test]
    fn wrap_array_wraps_inner() {
        // (string | null)[] → Array<Option<String>>
        let ts = TsType::Array(Box::new(TsType::Union(vec![
            TsType::Primitive("string".to_string()),
            TsType::Null,
        ])));
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(
            wrapped,
            Type::Array(Box::new(Type::Option(Box::new(Type::String))))
        );
    }

    #[test]
    fn wrap_object_wraps_fields() {
        let ts = TsType::Object(vec![
            ("name".to_string(), TsType::Primitive("string".to_string())),
            (
                "value".to_string(),
                TsType::Union(vec![TsType::Primitive("number".to_string()), TsType::Null]),
            ),
        ]);
        let wrapped = wrap_boundary_type(&ts);
        assert_eq!(
            wrapped,
            Type::Record(vec![
                ("name".to_string(), Type::String),
                ("value".to_string(), Type::Option(Box::new(Type::Number))),
            ])
        );
    }

    // ── .d.ts Parsing ───────────────────────────────────────────

    #[test]
    fn parse_dts_function_export() {
        let export = parse_function_export("findElement(id: string): Element | null;");
        let export = export.unwrap();
        assert_eq!(export.name, "findElement");
        assert_eq!(
            export.ts_type,
            TsType::Function {
                params: vec![TsType::Primitive("string".to_string())],
                return_type: Box::new(TsType::Union(vec![
                    TsType::Named("Element".to_string()),
                    TsType::Null,
                ])),
            }
        );
    }

    #[test]
    fn parse_dts_const_export() {
        let export = parse_const_export("VERSION: string;");
        let export = export.unwrap();
        assert_eq!(export.name, "VERSION");
        assert_eq!(export.ts_type, TsType::Primitive("string".to_string()));
    }

    #[test]
    fn parse_dts_type_export() {
        let export = parse_type_export("Config = { debug: boolean; port: number };");
        let export = export.unwrap();
        assert_eq!(export.name, "Config");
        assert_eq!(
            export.ts_type,
            TsType::Object(vec![
                (
                    "debug".to_string(),
                    TsType::Primitive("boolean".to_string())
                ),
                ("port".to_string(), TsType::Primitive("number".to_string())),
            ])
        );
    }

    #[test]
    fn parse_function_nullable_return_wraps_to_option() {
        let export = parse_function_export("findElement(id: string): Element | null;").unwrap();
        let wrapped = wrap_boundary_type(&export.ts_type);
        assert_eq!(
            wrapped,
            Type::Function {
                params: vec![Type::String],
                return_type: Box::new(Type::Option(Box::new(Type::Named("Element".to_string())))),
            }
        );
    }

    #[test]
    fn parse_function_any_param_wraps_to_unknown() {
        let export = parse_function_export("process(data: any): void;").unwrap();
        let wrapped = wrap_boundary_type(&export.ts_type);
        assert_eq!(
            wrapped,
            Type::Function {
                params: vec![Type::Unknown],
                return_type: Box::new(Type::Unit),
            }
        );
    }

    // ── Helper tests ────────────────────────────────────────────

    #[test]
    fn split_simple() {
        let parts = split_at_top_level("a | b | c", '|');
        assert_eq!(parts, vec!["a ", " b ", " c"]);
    }

    #[test]
    fn split_nested_generics() {
        let parts = split_at_top_level("Map<string, number> | null", '|');
        assert_eq!(parts, vec!["Map<string, number> ", " null"]);
    }

    #[test]
    fn find_paren() {
        assert_eq!(find_matching_paren("(a, b)"), Some(5));
        assert_eq!(find_matching_paren("((a))"), Some(4));
        assert_eq!(find_matching_paren("(a, (b, c), d)"), Some(13));
    }

    #[test]
    fn tsconfig_not_found() {
        let result = find_tsconfig(Path::new("/nonexistent/path"));
        assert!(result.is_none());
    }
}
