use std::collections::HashMap;

/// Detect if the cursor is in a pipe context (after `|>`).
/// Returns true if we find `|>` before the cursor (ignoring whitespace).
pub(super) fn is_pipe_context(source: &str, offset: usize) -> bool {
    let before = &source[..offset];
    let trimmed = before.trim_end();
    trimmed.ends_with("|>")
}

/// Extract the base type name from a full type string.
/// e.g., "Array<User>" -> "Array", "Option<string>" -> "Option", "string" -> "string"
pub(super) fn base_type_name(type_str: &str) -> &str {
    match type_str.find('<') {
        Some(pos) => &type_str[..pos],
        None => type_str,
    }
}

/// Try to resolve the type of the expression being piped.
/// Looks at the text before `|>` and tries to determine its type using the type map.
pub(super) fn resolve_piped_type(
    source: &str,
    offset: usize,
    type_map: &HashMap<String, String>,
) -> Option<String> {
    let before = &source[..offset];
    let trimmed = before.trim_end();
    // Strip the `|>` suffix
    let before_pipe = trimmed.strip_suffix("|>")?;
    let before_pipe = before_pipe.trim_end();

    // Check for `?` unwrap at the end
    let (expr_text, unwrap) = if let Some(inner) = before_pipe.strip_suffix('?') {
        (inner.trim_end(), true)
    } else {
        (before_pipe, false)
    };

    // Try to find the last identifier or call expression
    let ident = extract_trailing_identifier(expr_text);

    if ident.is_empty() {
        // Try literal type inference
        return infer_literal_type(expr_text);
    }

    // Look up the identifier in the type map
    let type_str = type_map.get(ident)?;
    let resolved = if unwrap {
        unwrap_type(type_str)
    } else {
        type_str.clone()
    };
    Some(resolved)
}

/// Extract the trailing identifier from an expression string.
/// e.g., "users" -> "users", "getUsers()" -> "getUsers", "a.b.c" -> "c"
pub(super) fn extract_trailing_identifier(s: &str) -> &str {
    let s = s.trim_end();
    // Strip trailing () for function calls
    let s = if s.ends_with(')') {
        // Find matching open paren
        let mut depth = 0;
        let mut paren_start = s.len();
        for (i, c) in s.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        paren_start = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        &s[..paren_start]
    } else {
        s
    };

    // Extract last identifier (after `.` or standalone)
    let start = s
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    &s[start..]
}

/// Infer the type of a literal expression.
pub(super) fn infer_literal_type(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('"') || s.starts_with('`') {
        Some("string".to_string())
    } else if s == "true" || s == "false" {
        Some("bool".to_string())
    } else if s.starts_with('[') {
        Some("Array".to_string())
    } else if s.parse::<f64>().is_ok() {
        Some("number".to_string())
    } else {
        None
    }
}

/// Unwrap a Result or Option type: Result<T, E> -> T, Option<T> -> T
pub(super) fn unwrap_type(type_str: &str) -> String {
    if let Some(inner) = type_str.strip_prefix("Result<") {
        // Result<T, E> -> T (first type arg)
        if let Some(comma_pos) = find_top_level_comma(inner) {
            return inner[..comma_pos].to_string();
        }
    }
    if let Some(inner) = type_str.strip_prefix("Option<") {
        // Option<T> -> T
        if let Some(end) = inner.strip_suffix('>') {
            return end.to_string();
        }
    }
    type_str.to_string()
}

/// Find the position of the first top-level comma (not inside angle brackets).
pub(super) fn find_top_level_comma(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Check if a function's first parameter type is compatible with the piped type.
/// Uses base type name matching: "Array<User>" matches "Array<T>", etc.
pub(super) fn is_pipe_compatible(fn_first_param: &str, piped_type: &str) -> bool {
    let fn_base = base_type_name(fn_first_param);
    let piped_base = base_type_name(piped_type);

    // Exact base type match
    if fn_base == piped_base {
        return true;
    }

    // Generic type parameter (single uppercase letter like T, U, A) matches anything
    if fn_base.len() == 1
        && fn_base
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
    {
        return true;
    }

    false
}
