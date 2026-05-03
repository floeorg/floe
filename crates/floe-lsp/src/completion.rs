use std::collections::HashMap;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, SymbolKind, Url};

use floe_core::checker::Type;
use floe_core::interop::{DtsExport, TsType, ts_type_to_string};

use super::index::SymbolIndex;

/// Detect if the cursor is inside a comment (// or /* */).
pub(super) fn is_in_comment(source: &str, offset: usize) -> bool {
    let before = &source[..offset];

    // Check for line comment: find the last newline before offset,
    // then check if there's a `//` between that newline and the offset
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_before = &source[line_start..offset];
    if let Some(slash_pos) = line_before.find("//") {
        // Make sure the // isn't inside a string
        let before_slash = &line_before[..slash_pos];
        let quote_count = before_slash.chars().filter(|&c| c == '"').count();
        if quote_count % 2 == 0 {
            return true;
        }
    }

    // Check for block comment: scan for /* ... */ pairs
    let mut i = 0;
    let bytes = source.as_bytes();
    while i < offset.saturating_sub(1) {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Found opening /*, look for closing */
            let mut j = i + 2;
            while j < source.len().saturating_sub(1) {
                if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                    break;
                }
                j += 1;
            }
            // If offset is between /* and */, we're in a block comment
            if offset > i + 1 && offset <= j {
                return true;
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }

    false
}

/// Detect if the cursor is inside a string literal (not an import path).
pub(super) fn is_in_string_literal(source: &str, offset: usize) -> bool {
    // Count unescaped quotes before the offset on the same line
    let before = &source[..offset];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_before = &source[line_start..offset];

    let mut in_string = false;
    let mut chars = line_before.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_string = !in_string,
            '\\' if in_string => {
                chars.next(); // skip escaped char
            }
            _ => {}
        }
    }
    // Also check for template literals (backtick)
    if !in_string {
        let backtick_count = line_before.chars().filter(|&c| c == '`').count();
        if backtick_count % 2 == 1 {
            return true;
        }
    }
    in_string
}

/// Detect if the cursor is inside an import path string (e.g., `from "./|"`).
pub(super) fn is_in_import_string(source: &str, offset: usize) -> bool {
    let before = &source[..offset];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &source[line_start..];
    let line_trimmed = line.trim();

    // Must be on an import line
    if !line_trimmed.starts_with("import") {
        return false;
    }

    // Check if offset is inside the string after "from"
    let line_before_cursor = &source[line_start..offset];
    if let Some(from_pos) = line_before_cursor.find("from") {
        let after_from = &line_before_cursor[from_pos + 4..];
        let quote_count = after_from.chars().filter(|&c| c == '"').count();
        return quote_count % 2 == 1; // odd = inside a string
    }

    false
}

/// Generate import path completion items by scanning the filesystem.
pub(super) fn import_path_completions(
    uri: &Url,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Extract what the user has typed so far inside the string
    let before = &source[..offset];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_before = &source[line_start..offset];

    // Find the opening quote after "from"
    let typed_path = if let Some(from_pos) = line_before.find("from") {
        let after_from = &line_before[from_pos + 4..];
        // Find the opening quote
        if let Some(quote_pos) = after_from.find('"') {
            &after_from[quote_pos + 1..]
        } else {
            return items;
        }
    } else {
        return items;
    };

    // Get the directory of the current file
    let Ok(current_file) = uri.to_file_path() else {
        return items;
    };
    let Some(current_dir) = current_file.parent() else {
        return items;
    };

    // Resolve the partial path relative to current dir
    let (search_dir, name_prefix) = if typed_path.contains('/') {
        let last_slash = typed_path.rfind('/').unwrap();
        let dir_part = &typed_path[..last_slash + 1];
        let name_part = &typed_path[last_slash + 1..];
        let resolved = current_dir.join(dir_part);
        (resolved, name_part.to_string())
    } else if typed_path.starts_with('.') {
        // Just "./" or "../" typed — search current or parent dir
        let resolved = current_dir.join(typed_path);
        (resolved, String::new())
    } else {
        (current_dir.to_path_buf(), typed_path.to_string())
    };

    // List directory contents
    let Ok(entries) = std::fs::read_dir(&search_dir) else {
        return items;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Filter by prefix
        if !name_prefix.is_empty() && !file_name.starts_with(&name_prefix) {
            continue;
        }

        // Skip hidden files
        if file_name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            items.push(CompletionItem {
                label: format!("{file_name}/"),
                kind: Some(CompletionItemKind::FOLDER),
                insert_text: Some(format!("{file_name}/")),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        } else {
            let detail = match path.extension().and_then(|e| e.to_str()) {
                Some("fl") => "Floe module",
                Some("ts" | "tsx") => "TypeScript module",
                _ => continue,
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&file_name);
            if !name_prefix.is_empty() && !stem.starts_with(&name_prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: stem.to_string(),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(detail.to_string()),
                insert_text: Some(stem.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    items
}

/// Extract the identifier before a dot at the cursor position.
/// For `row.fi|` returns `Some("row")`, for `foo|` returns `None`.
pub(super) fn identifier_before_dot(source: &str, offset: usize) -> Option<&str> {
    let bytes = source.as_bytes();

    let mut pos = offset;
    while pos > 0 && (bytes[pos - 1].is_ascii_alphanumeric() || bytes[pos - 1] == b'_') {
        pos -= 1;
    }

    if pos == 0 || bytes[pos - 1] != b'.' {
        return None;
    }
    pos -= 1;

    let end = pos;
    while pos > 0 && (bytes[pos - 1].is_ascii_alphanumeric() || bytes[pos - 1] == b'_') {
        pos -= 1;
    }

    if pos == end {
        return None;
    }

    Some(&source[pos..end])
}

/// Generate completions for dot-access on a variable (e.g., `row.` → row's fields).
pub(super) fn dot_access_completions(
    obj_name: &str,
    prefix: &str,
    type_map: &HashMap<String, String>,
    index: &SymbolIndex,
    dts_imports: &HashMap<String, Vec<DtsExport>>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Look up the object's type in the type_map
    let obj_type = match type_map.get(obj_name) {
        Some(ty) => ty.as_str(),
        None => return items,
    };

    // Strategy 1: If the type matches a known type name, look up its fields in the symbol index
    for sym in &index.symbols {
        if sym.kind == SymbolKind::PROPERTY
            && sym.owner_type.as_deref() == Some(obj_type)
            && (prefix.is_empty() || sym.name.starts_with(prefix))
        {
            items.push(CompletionItem {
                label: sym.name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(sym.detail.clone()),
                insert_text: Some(sym.name.clone()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    // Strategy 2: If type_map has __field_ entries for this type, use those
    let field_prefix = format!("__field_{obj_type}_");
    for (key, field_ty) in type_map {
        if let Some(field_name) = key.strip_prefix(&field_prefix)
            && (prefix.is_empty() || field_name.starts_with(prefix))
            && !items.iter().any(|i| i.label == field_name)
        {
            items.push(CompletionItem {
                label: field_name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(format!("(property) {field_name}: {field_ty}")),
                insert_text: Some(field_name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    // Strategy 3: If the type is a record literal like "{ id: number, name: string }",
    // parse field names from the display string using depth-aware splitting
    // to handle nested generics like Map<string, number>
    if items.is_empty() && obj_type.starts_with('{') && obj_type.ends_with('}') {
        let inner = &obj_type[1..obj_type.len() - 1];
        for field_str in split_top_level(inner, ',') {
            let field_str = field_str.trim();
            if let Some(colon_pos) = field_str.find(':') {
                let field_name = field_str[..colon_pos].trim();
                let field_type = field_str[colon_pos + 1..].trim();
                if !field_name.is_empty() && (prefix.is_empty() || field_name.starts_with(prefix)) {
                    items.push(CompletionItem {
                        label: field_name.to_string(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(format!("(property) {field_name}: {field_type}")),
                        insert_text: Some(field_name.to_string()),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        ..Default::default()
                    });
                }
            }
        }
    }

    // Strategy 4: imported type from a .d.ts module (e.g. `Context<...>` from
    // Hono). The obj_type's base name is looked up across every cached dts
    // import; if one exports an interface/type-alias whose resolved shape is
    // an `Object`, enumerate its fields. Lets `c.` pop up `env`, `req`, `res`,
    // `json`, ... on a parameter typed as an imported generic.
    if items.is_empty() {
        let base = base_type_name(obj_type);
        for exports in dts_imports.values() {
            let Some(export) = exports.iter().find(|e| e.name == base) else {
                continue;
            };
            let TsType::Object(fields) = &export.ts_type else {
                continue;
            };
            for field in fields {
                if !prefix.is_empty() && !field.name.starts_with(prefix) {
                    continue;
                }
                if items.iter().any(|i: &CompletionItem| i.label == field.name) {
                    continue;
                }
                let rendered = ts_type_to_string(&field.ty);
                let kind = if matches!(&field.ty, TsType::Function { .. }) {
                    CompletionItemKind::METHOD
                } else {
                    CompletionItemKind::PROPERTY
                };
                items.push(CompletionItem {
                    label: field.name.clone(),
                    kind: Some(kind),
                    detail: Some(format!("(property) {}: {}", field.name, rendered)),
                    insert_text: Some(field.name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
            }
            // First matching module wins — prevents duplicate items when the
            // same name is re-exported across related packages.
            if !items.is_empty() {
                break;
            }
        }
    }

    items
}

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
        Some("boolean".to_string())
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

/// Split a string at top-level commas, respecting nested `<>`, `()`, `{}`.
fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c == delim && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Base-name compatibility: `Array<User>` matches `Array<T>`, single-letter
/// type vars match anything.
pub(super) fn is_pipe_compatible(fn_first_param: &str, piped_type: &str) -> bool {
    is_base_name_compatible(base_type_name(fn_first_param), base_type_name(piped_type))
}

/// Typed variant — avoids per-keystroke `.to_string()` on the symbol's
/// `Arc<Type>` since pipe completions iterate every function in every
/// open document.
pub(super) fn is_pipe_compatible_typed(fn_first_param: &Type, piped_type: &str) -> bool {
    is_base_name_compatible(type_base_name(fn_first_param), base_type_name(piped_type))
}

fn is_base_name_compatible(fn_base: &str, piped_base: &str) -> bool {
    if fn_base == piped_base {
        return true;
    }
    // Single uppercase letter (T, U, A…) acts as a generic type var.
    fn_base.len() == 1
        && fn_base
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
}

/// Base name of a `Type` for pipe compat without allocating.
fn type_base_name(ty: &Type) -> &str {
    match ty {
        Type::Number => "number",
        Type::String | Type::StringLiteral(_) => "string",
        Type::Bool => "boolean",
        Type::Unit => "()",
        Type::Undefined => "undefined",
        Type::Never => "never",
        Type::Unknown => "unknown",
        Type::Array(_) => "Array",
        Type::Map { .. } => "Map",
        Type::RecordMap { .. } => "Record",
        Type::Set { .. } => "Set",
        Type::Promise(_) => "Promise",
        Type::Settable(_) => "Settable",
        Type::Tuple(_) => "tuple",
        Type::Record(_) => "record",
        Type::Function { .. } => "function",
        Type::TsUnion(_) => "union",
        Type::Named(name)
        | Type::Foreign { name, .. }
        | Type::Opaque { name, .. }
        | Type::Union { name, .. } => name,
        Type::Var(_) => "",
        Type::Error => "<error>",
    }
}
