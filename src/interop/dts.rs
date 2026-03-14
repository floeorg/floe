//! .d.ts export parsing: reads declaration files and extracts exports.

use super::*;

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

pub(super) fn parse_function_export(rest: &str) -> Option<DtsExport> {
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

pub(super) fn parse_const_export(rest: &str) -> Option<DtsExport> {
    // name: Type;
    let colon = rest.find(':')?;
    let name = rest[..colon].trim().to_string();
    let type_str = rest[colon + 1..].trim().trim_end_matches(';').trim();
    let ts_type = parse_type_str(type_str);

    Some(DtsExport { name, ts_type })
}

pub(super) fn parse_type_export(rest: &str) -> Option<DtsExport> {
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

pub(super) fn parse_interface_export(
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
