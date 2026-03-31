//! Hover support for stdlib modules and functions.
//!
//! Provides hover text for:
//! - Stdlib module names (Array, String, Option, etc.) showing available functions
//! - Bare stdlib function names (sort, map, trim, etc.) showing signature and module

use crate::stdlib::StdlibRegistry;

/// Pretty-print a type variable index as a letter (0 -> T, 1 -> U, 2 -> V, ...).
fn type_var_name(index: usize) -> &'static str {
    match index {
        0 => "T",
        1 => "U",
        2 => "V",
        3 => "W",
        _ => "T",
    }
}

/// Format a checker Type for hover display, using readable type variable names.
pub(super) fn format_type(ty: &crate::checker::Type) -> String {
    use crate::checker::Type;
    match ty {
        Type::Number => "number".to_string(),
        Type::String => "string".to_string(),
        Type::Bool => "boolean".to_string(),
        Type::Undefined => "undefined".to_string(),
        Type::Unit => "()".to_string(),
        Type::Unknown => "unknown".to_string(),
        Type::Named(n) | Type::Foreign(n) => n.clone(),
        Type::Promise(inner) => format!("Promise<{}>", format_type(inner)),
        Type::Var(id) => type_var_name(*id).to_string(),
        Type::Array(inner) => format!("Array<{}>", format_type(inner)),
        Type::Map { key, value } => {
            format!("Map<{}, {}>", format_type(key), format_type(value))
        }
        Type::RecordMap { key, value } => {
            format!("Record<{}, {}>", format_type(key), format_type(value))
        }
        Type::Set { element } => format!("Set<{}>", format_type(element)),
        _ if ty.is_option() => {
            if let Some(inner) = ty.option_inner() {
                format!("Option<{}>", format_type(inner))
            } else {
                "Option<unknown>".to_string()
            }
        }
        _ if ty.is_result() => {
            let ok = ty
                .result_ok()
                .map(format_type)
                .unwrap_or_else(|| "unknown".to_string());
            let err = ty
                .result_err()
                .map(format_type)
                .unwrap_or_else(|| "unknown".to_string());
            format!("Result<{ok}, {err}>")
        }
        Type::Settable(inner) => format!("Settable<{}>", format_type(inner)),
        Type::Tuple(types) => {
            let t: Vec<_> = types.iter().map(format_type).collect();
            format!("[{}]", t.join(", "))
        }
        Type::Function {
            params,
            return_type,
        } => {
            let p: Vec<_> = params.iter().map(format_type).collect();
            format!("({}) -> {}", p.join(", "), format_type(return_type))
        }
        Type::Record(fields) => {
            let f: Vec<_> = fields
                .iter()
                .map(|(n, t)| format!("{n}: {}", format_type(t)))
                .collect();
            format!("{{ {} }}", f.join(", "))
        }
        Type::Opaque { name, .. } => name.clone(),
        Type::Union { name, .. } => name.clone(),
        Type::TsUnion(members) => {
            let m: Vec<_> = members.iter().map(format_type).collect();
            m.join(" | ")
        }
        Type::StringLiteral(s) => format!("\"{s}\""),
        Type::Never => "never".to_string(),
    }
}

/// Format a stdlib function's parameter list for display.
pub(super) fn format_params(f: &crate::stdlib::StdlibFn) -> String {
    if f.is_variadic() {
        "...args".to_string()
    } else {
        let params: Vec<String> = f.params.iter().map(format_type).collect();
        params.join(", ")
    }
}

/// Format a stdlib function signature for display.
fn format_fn_signature(f: &crate::stdlib::StdlibFn) -> String {
    let ret = format_type(&f.return_type);
    format!("{}.{}({}) -> {}", f.module, f.name, format_params(f), ret)
}

/// Generate hover text for a stdlib module name (Array, String, Option, etc.).
/// Returns None if the word is not a stdlib module.
pub(super) fn hover_stdlib_module(word: &str) -> Option<String> {
    let registry = StdlibRegistry::new();
    if !registry.is_module(word) {
        return None;
    }

    let functions = registry.module_functions(word);
    if functions.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(format!("```floe\nmodule {word}\n```"));
    lines.push("**Available functions:**".to_string());
    for f in &functions {
        let ret = format_type(&f.return_type);
        lines.push(format!("- `{}({}) -> {}`", f.name, format_params(f), ret));
    }

    Some(lines.join("\n"))
}

/// Generate hover text for a specific stdlib method on a module (e.g., Array.map, String.split).
/// Returns None if the module or function is not found.
pub(super) fn hover_stdlib_method(module: &str, method: &str) -> Option<String> {
    let registry = StdlibRegistry::new();
    if !registry.is_module(module) {
        return None;
    }

    let functions = registry.module_functions(module);
    let matching: Vec<_> = functions.iter().filter(|f| f.name == method).collect();

    if matching.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for f in &matching {
        lines.push(format!("```floe\n{}\n```", format_fn_signature(f)));
    }
    lines.push(format!("Stdlib function from `{module}`."));
    Some(lines.join("\n"))
}

/// Generate hover text for a bare stdlib function name (sort, map, trim, etc.).
/// Returns None if the word is not a stdlib function.
pub(super) fn hover_stdlib_function(word: &str) -> Option<String> {
    let registry = StdlibRegistry::new();
    let matches = registry.lookup_by_name(word);
    if matches.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    for f in &matches {
        lines.push(format!("```floe\n{}\n```", format_fn_signature(f)));
    }

    if matches.len() == 1 {
        lines.push(format!("Stdlib function from `{}`.", matches[0].module));
    } else {
        let modules: Vec<&str> = matches.iter().map(|f| f.module).collect();
        lines.push(format!(
            "Stdlib function available in: {}.",
            modules
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_array_module() {
        let result = hover_stdlib_module("Array");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Array"));
        assert!(text.contains("sort"));
        assert!(text.contains("map"));
        assert!(text.contains("filter"));
    }

    #[test]
    fn hover_string_module() {
        let result = hover_stdlib_module("String");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module String"));
        assert!(text.contains("trim"));
        assert!(text.contains("split"));
    }

    #[test]
    fn hover_option_module() {
        let result = hover_stdlib_module("Option");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Option"));
        assert!(text.contains("unwrapOr"));
        assert!(text.contains("isSome"));
    }

    #[test]
    fn hover_result_module() {
        let result = hover_stdlib_module("Result");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Result"));
        assert!(text.contains("map"));
        assert!(text.contains("isOk"));
    }

    #[test]
    fn hover_console_module() {
        let result = hover_stdlib_module("Console");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Console"));
        assert!(text.contains("log"));
    }

    #[test]
    fn hover_math_module() {
        let result = hover_stdlib_module("Math");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Math"));
        assert!(text.contains("floor"));
        assert!(text.contains("sqrt"));
    }

    #[test]
    fn hover_json_module() {
        let result = hover_stdlib_module("JSON");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module JSON"));
        assert!(text.contains("stringify"));
        assert!(text.contains("parse"));
    }

    #[test]
    fn hover_number_module() {
        let result = hover_stdlib_module("Number");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Number"));
        assert!(text.contains("parse"));
        assert!(text.contains("clamp"));
    }

    #[test]
    fn hover_map_module() {
        let result = hover_stdlib_module("Map");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Map"));
        assert!(text.contains("get"));
        assert!(text.contains("set"));
        assert!(text.contains("keys"));
        assert!(text.contains("merge"));
    }

    #[test]
    fn hover_set_module() {
        let result = hover_stdlib_module("Set");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("module Set"));
        assert!(text.contains("add"));
        assert!(text.contains("union"));
        assert!(text.contains("intersect"));
    }

    #[test]
    fn hover_non_module() {
        assert!(hover_stdlib_module("Foo").is_none());
        assert!(hover_stdlib_module("const").is_none());
    }

    #[test]
    fn hover_bare_sort_function() {
        let result = hover_stdlib_function("sort");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("Array.sort"));
        assert!(text.contains("Array<T>"));
        assert!(text.contains("Stdlib function from `Array`"));
    }

    #[test]
    fn hover_bare_trim_function() {
        let result = hover_stdlib_function("trim");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("String.trim"));
        assert!(text.contains("string"));
        assert!(text.contains("Stdlib function from `String`"));
    }

    #[test]
    fn hover_bare_map_function_multiple_modules() {
        let result = hover_stdlib_function("map");
        assert!(result.is_some());
        let text = result.unwrap();
        // map exists in Array, Option, and Result
        assert!(text.contains("Array.map"));
        assert!(text.contains("Option.map"));
        assert!(text.contains("Result.map"));
        assert!(text.contains("available in:"));
    }

    #[test]
    fn hover_bare_unwrap_or_function() {
        let result = hover_stdlib_function("unwrapOr");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("unwrapOr"));
    }

    #[test]
    fn hover_bare_nonexistent_function() {
        assert!(hover_stdlib_function("nonexistent").is_none());
        assert!(hover_stdlib_function("const").is_none());
    }

    #[test]
    fn hover_bare_floor_function() {
        let result = hover_stdlib_function("floor");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("Math.floor"));
        assert!(text.contains("number"));
        assert!(text.contains("Stdlib function from `Math`"));
    }

    #[test]
    fn hover_bare_log_function() {
        let result = hover_stdlib_function("log");
        assert!(result.is_some());
        let text = result.unwrap();
        // log exists in Console and Math
        assert!(text.contains("Console.log") || text.contains("Math.log"));
    }
}
