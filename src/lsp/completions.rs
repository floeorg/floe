use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::stdlib::StdlibRegistry;

const EVENT_HANDLERS: &[&str] = &[
    "onClick",
    "onChange",
    "onKeyDown",
    "onSubmit",
    "onFocus",
    "onBlur",
    "onMouseEnter",
    "onMouseLeave",
    "onInput",
    "onKeyUp",
    "onKeyPress",
];

use super::completion::{
    dot_access_completions, identifier_before_dot, import_path_completions, is_in_comment,
    is_in_import_string, is_in_string_literal, is_pipe_context, resolve_piped_type,
};
use super::stdlib_hover;
use super::symbols::{SymbolIndex, symbol_kind_to_completion};
use super::{BUILTINS, FloeLsp, KEYWORDS, position_to_offset, word_prefix_at_offset};

impl FloeLsp {
    pub(super) async fn handle_completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let prefix = word_prefix_at_offset(&doc.content, offset);

        // ── Context suppression: no completions in comments or non-import strings ──
        if is_in_comment(&doc.content, offset) {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        }

        // ── Import path completions ─────────────────────────────
        if is_in_import_string(&doc.content, offset) {
            let items = import_path_completions(&uri, &doc.content, offset);
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // Suppress completions inside non-import string literals
        if is_in_string_literal(&doc.content, offset) {
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        }

        let registry = StdlibRegistry::new();

        // ── Dot-access field completions ────────────────────────
        if let Some(obj_name) = identifier_before_dot(&doc.content, offset)
            && !registry.is_module(obj_name)
        {
            let items = dot_access_completions(obj_name, &prefix, &doc.type_map, &doc.index);
            // Always return here — never fall through to global completions in dot context.
            // If we can't resolve fields, returning empty is better than dumping every symbol.
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // ── Stdlib module method completions (Array., String., etc.) ──
        if let Some(module_name) = stdlib_module_prefix(&doc.content, offset, &registry) {
            let functions = registry.module_functions(&module_name);
            if !functions.is_empty() {
                let items: Vec<CompletionItem> = functions
                    .into_iter()
                    .filter(|f| prefix.is_empty() || f.name.starts_with(&*prefix))
                    .map(|f| {
                        let ret = stdlib_hover::format_type(&f.return_type);
                        let detail = format!(
                            "{}.{}({}) -> {}",
                            f.module,
                            f.name,
                            stdlib_hover::format_params(f),
                            ret
                        );
                        CompletionItem {
                            label: f.name.to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(detail),
                            insert_text: Some(f.name.to_string()),
                            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                            ..Default::default()
                        }
                    })
                    .collect();
                if !items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }
        }

        // ── Pipe-aware completions ──────────────────────────────
        if is_pipe_context(&doc.content, offset) {
            let piped_type = resolve_piped_type(&doc.content, offset, &doc.type_map);
            let items = self.pipe_completions(&docs, &uri, &prefix, piped_type.as_deref());
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // ── Match arm variant completions (#143) ─────────────────
        if let Some(variants) = detect_match_context(&doc.content, offset, &doc.index) {
            let items: Vec<CompletionItem> = variants
                .into_iter()
                .filter(|v| prefix.is_empty() || v.starts_with(&prefix))
                .map(|name| CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("match variant".to_string()),
                    insert_text: Some(format!("{name} -> $0,")),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                })
                .collect();
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // ── JSX attribute completions (#144) ─────────────────────
        if is_in_jsx_tag(&doc.content, offset) {
            let items = jsx_attribute_completions(&prefix);
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // ── Lambda event completions (#145) ──────────────────────
        if let Some(items) = lambda_event_completions(&doc.content, offset, &prefix)
            && !items.is_empty()
        {
            return Ok(Some(CompletionResponse::Array(items)));
        }

        // ── Normal completions ──────────────────────────────────
        let mut items = Vec::new();

        // Symbols from the current document
        for sym in doc.index.all_completions() {
            if !prefix.is_empty() && !sym.name.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: sym.name.clone(),
                kind: Some(symbol_kind_to_completion(sym.kind)),
                detail: Some(sym.detail.clone()),
                insert_text: Some(sym.name.clone()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }

        // Symbols from other open documents (cross-file completions + auto-import)
        for (other_uri, other_doc) in docs.iter() {
            if other_uri == &uri {
                continue;
            }
            for sym in &other_doc.index.symbols {
                if sym.import_source.is_some() {
                    continue;
                }
                if !prefix.is_empty() && !sym.name.starts_with(&prefix) {
                    continue;
                }
                let relative_path = other_uri
                    .path_segments()
                    .and_then(|mut s| s.next_back())
                    .unwrap_or("unknown")
                    .trim_end_matches(".fl");

                let import_edit =
                    format!("import {{ {} }} from \"./{}\"\n", sym.name, relative_path);

                items.push(CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(symbol_kind_to_completion(sym.kind)),
                    detail: Some(format!(
                        "{} (auto-import from {})",
                        sym.detail, relative_path
                    )),
                    insert_text: Some(sym.name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    additional_text_edits: Some(vec![TextEdit {
                        range: Range {
                            start: Position::new(0, 0),
                            end: Position::new(0, 0),
                        },
                        new_text: import_edit,
                    }]),
                    ..Default::default()
                });
            }
        }

        // Keywords
        for (kw, snippet) in KEYWORDS {
            if !prefix.is_empty() && !kw.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Builtins
        for (name, snippet, detail) in BUILTINS {
            if !prefix.is_empty() && !name.starts_with(&prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(detail.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(items)))
    }
}

// ── Completion heuristic helpers ─────────────────────────────────

/// Detect if the cursor is right after `ModuleName.` where ModuleName is a known
/// stdlib module. Returns the module name if found.
///
/// For example, in `Array.m|` (where `|` is cursor), this returns `Some("Array")`.
/// In `nums |> Array.f|`, this also returns `Some("Array")`.
fn stdlib_module_prefix(source: &str, offset: usize, registry: &StdlibRegistry) -> Option<String> {
    let candidate = super::completion::identifier_before_dot(source, offset)?;
    if registry.is_module(candidate) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Detect if cursor is inside a `match expr { ... }` block.
/// If so, look up the matched expression's type and return its variant names.
pub(super) fn detect_match_context(
    source: &str,
    offset: usize,
    index: &SymbolIndex,
) -> Option<Vec<String>> {
    let before = &source[..offset];

    // Find the innermost unclosed `match ... {` before the cursor
    // Scan backwards for `{` that belongs to a match expression
    let mut brace_depth: i32 = 0;
    let mut search_pos = before.len();

    while search_pos > 0 {
        search_pos -= 1;
        let ch = before.as_bytes()[search_pos];
        match ch {
            b'}' => brace_depth += 1,
            b'{' => {
                if brace_depth == 0 {
                    // This is an unmatched open brace — check if preceded by `match <expr>`
                    let before_brace = before[..search_pos].trim_end();
                    // Extract the expression between `match` and `{`
                    if let Some(match_pos) = before_brace.rfind("match ") {
                        let expr_text = before_brace[match_pos + 6..].trim();
                        // Look up the expression in the symbol index to find its type
                        let variants = find_variants_for_expr(expr_text, index);
                        if !variants.is_empty() {
                            return Some(variants);
                        }
                    }
                    // Not a match block, stop searching
                    return None;
                }
                brace_depth -= 1;
            }
            _ => {}
        }
    }

    None
}

/// Given a match expression text, try to find variant names for it.
/// Looks up the expression as a type name in the symbol index.
fn find_variants_for_expr(expr: &str, index: &SymbolIndex) -> Vec<String> {
    // The expr could be a variable name; look for a type with the same name
    // or look for a type whose variants are in the index
    let expr = expr.trim();

    // Strategy: check if expr matches a type name directly
    let type_syms = index.find_by_name(expr);
    for sym in &type_syms {
        if sym.kind == SymbolKind::TYPE_PARAMETER {
            // Found a type — collect its variants from the index
            let prefix = format!("{}.", expr);
            let variants: Vec<String> = index
                .symbols
                .iter()
                .filter(|s| s.kind == SymbolKind::ENUM_MEMBER && s.detail.starts_with(&prefix))
                .map(|s| s.name.clone())
                .collect();
            if !variants.is_empty() {
                return variants;
            }
        }
    }

    // Strategy 2: the expr might be a variable — look for all ENUM_MEMBER symbols
    // This is a best-effort fallback
    let all_variants: Vec<String> = index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::ENUM_MEMBER)
        .map(|s| s.name.clone())
        .collect();

    // Only return if there are some variants to suggest
    if !all_variants.is_empty() {
        return all_variants;
    }

    Vec::new()
}

/// Detect if cursor is inside a JSX opening tag (e.g., `<button on|`).
pub(super) fn is_in_jsx_tag(source: &str, offset: usize) -> bool {
    let before = &source[..offset];

    // Scan backwards for `<` that isn't closed by `>`
    let mut angle_depth: i32 = 0;
    for ch in before.chars().rev() {
        match ch {
            '>' => angle_depth += 1,
            '<' => {
                if angle_depth == 0 {
                    // Found an unclosed `<` — we're inside a tag
                    return true;
                }
                angle_depth -= 1;
            }
            _ => {}
        }
    }
    false
}

/// Generate JSX attribute completion items.
pub(super) fn jsx_attribute_completions(prefix: &str) -> Vec<CompletionItem> {
    let common_attrs = [
        "className",
        "id",
        "style",
        "key",
        "ref",
        "disabled",
        "type",
        "value",
        "placeholder",
        "href",
        "src",
        "alt",
        "title",
        "name",
        "role",
        "tabIndex",
        "autoFocus",
        "checked",
        "readOnly",
        "required",
        "hidden",
    ];

    let mut items = Vec::new();

    for attr in EVENT_HANDLERS.iter().chain(common_attrs.iter()) {
        if !prefix.is_empty() && !attr.starts_with(prefix) {
            continue;
        }
        let is_event = attr.starts_with("on");
        items.push(CompletionItem {
            label: attr.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(if is_event {
                "event handler".to_string()
            } else {
                "JSX attribute".to_string()
            }),
            insert_text: Some(format!("{attr}={{$1}}")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });
    }

    items
}

/// Detect if cursor is inside a lambda body used as an event handler callback,
/// and provide event-type completions (e.g., `e.target`, `e.preventDefault()`).
pub(super) fn lambda_event_completions(
    source: &str,
    offset: usize,
    prefix: &str,
) -> Option<Vec<CompletionItem>> {
    let before = &source[..offset];

    // Check if we're typing after a `.` on an expression chain
    let dot_pos = before.rfind('.')?;
    let before_dot = before[..dot_pos].trim_end();

    // Extract the full dotted expression chain backwards (e.g., "e.target" or "e")
    // Find where the expression chain starts (first non-word, non-dot character)
    let chain_start = before_dot
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let chain = &before_dot[chain_start..];

    if chain.is_empty() {
        return None;
    }

    // Split the chain by dots to get root param and path
    let parts: Vec<&str> = chain.split('.').collect();
    let param_name = parts[0];

    if param_name.is_empty() {
        return None;
    }

    // Check if this param is a lambda parameter by scanning backwards for `(param) =>`
    let pre_chain = &before[..chain_start];
    let arrow_pattern = format!("({param_name}) =>");
    if !pre_chain.contains(&arrow_pattern) {
        return None;
    }

    // Now check if this lambda is used as an event handler callback
    // Find the `={(` pattern before the lambda
    let lambda_start = before.rfind(&format!("({param_name}) =>"))?;
    let before_lambda = before[..lambda_start].trim_end();
    let before_eq = before_lambda.strip_suffix('{')?;
    let before_eq = before_eq.trim_end().strip_suffix('=')?;
    let attr_name_end = before_eq.trim_end();

    // Extract the attribute name
    let attr_start = attr_name_end
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let attr_name = &attr_name_end[attr_start..];

    if !EVENT_HANDLERS.contains(&attr_name) {
        return None;
    }

    // Determine completion level from the dot chain
    // parts[0] = param_name, parts[1..] = property path so far
    // dot_count = number of dots including the trailing one we're completing after
    let dot_count = parts.len(); // e.g., ["e"] = 1 dot (e.), ["e", "target"] = 2 dots (e.target.)

    let mut items = Vec::new();

    if dot_count == 1 {
        // First level: e.target, e.preventDefault(), etc.
        let event_props = [
            ("target", "EventTarget", false),
            ("currentTarget", "EventTarget", false),
            ("type", "string", false),
            ("preventDefault()", "void", true),
            ("stopPropagation()", "void", true),
            ("key", "string", false),
            ("bubbles", "boolean", false),
            ("defaultPrevented", "boolean", false),
            ("timeStamp", "number", false),
        ];

        for (name, ty, is_method) in &event_props {
            if !prefix.is_empty() && !name.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(if *is_method {
                    CompletionItemKind::METHOD
                } else {
                    CompletionItemKind::PROPERTY
                }),
                detail: Some(ty.to_string()),
                insert_text: Some(name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    } else if dot_count == 2 && parts.get(1) == Some(&"target") {
        // Second level: e.target.value, e.target.checked, etc.
        let target_props = [
            ("value", "string"),
            ("checked", "boolean"),
            ("name", "string"),
            ("id", "string"),
            ("tagName", "string"),
            ("className", "string"),
            ("textContent", "string"),
            ("innerHTML", "string"),
            ("disabled", "boolean"),
        ];

        for (name, ty) in &target_props {
            if !prefix.is_empty() && !name.starts_with(prefix) {
                continue;
            }
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(ty.to_string()),
                insert_text: Some(name.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    if items.is_empty() { None } else { Some(items) }
}
