use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use floe_core::checker::Type;

use super::stdlib_hover;
use super::{
    FloeLsp, find_enclosing_call_method_sig, find_expr_type_at_offset, position_to_offset,
    word_at_offset,
};

impl FloeLsp {
    pub(super) async fn handle_hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);
        let word = word_at_offset(&doc.content, offset);

        if word.is_empty() {
            // Check for pipe operator `|>` at cursor
            let bytes = doc.content.as_bytes();
            let is_pipe = (offset > 0
                && offset < bytes.len()
                && bytes[offset - 1] == b'|'
                && bytes[offset] == b'>')
                || (offset + 1 < bytes.len() && bytes[offset] == b'|' && bytes[offset + 1] == b'>');
            if is_pipe
                && let Some(ref typed_program) = doc.typed_program
                && let Some(pipe_ty) = super::find_pipe_input_type_at_offset(typed_program, offset)
            {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n|> {pipe_ty}\n```\nPipe input type"),
                    }),
                    range: None,
                }));
            }
            return Ok(None);
        }

        // Check if cursor is on an import line — skip stdlib hover for keywords like `from`
        let is_import_line = {
            let line_start = doc.content[..offset].rfind('\n').map_or(0, |p| p + 1);
            doc.content[line_start..]
                .trim_start()
                .starts_with("import ")
        };

        // Compute word start position and whether this is member access (X.word)
        let word_start = {
            let mut s = offset;
            let bytes = doc.content.as_bytes();
            while s > 0 && (bytes[s - 1].is_ascii_alphanumeric() || bytes[s - 1] == b'_') {
                s -= 1;
            }
            s
        };
        let is_member_access =
            word_start > 0 && doc.content.as_bytes().get(word_start - 1) == Some(&b'.');

        // Check symbol index — for definitions, imports, and bindings at the cursor.
        let symbols = doc.index.find_by_name(word);
        let best_sym = symbols
            .iter()
            .find(|s| offset >= s.start && offset <= s.end);

        // If both SymbolIndex and typed AST match, prefer the tighter span.
        // This avoids showing a function definition when the cursor is on a
        // usage of the same name inside the function body.
        let typed_ast = doc
            .typed_program
            .as_ref()
            .and_then(|p| find_expr_type_at_offset(p, offset));

        if let Some(sym) = best_sym {
            if let Some((ast_width, ref ty)) = typed_ast
                && ast_width < sym.end - sym.start
            {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n{word}: {}\n```", ty),
                    }),
                    range: None,
                }));
            }

            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```floe\n{}\n```", sym.detail),
                }),
                range: None,
            }));
        }

        // Check for member access (e.g. z.object, Array.map, user.name)
        if is_member_access {
            let bytes = doc.content.as_bytes();
            let dot_pos = word_start - 1;
            let mut obj_start = dot_pos;
            while obj_start > 0
                && (bytes[obj_start - 1].is_ascii_alphanumeric() || bytes[obj_start - 1] == b'_')
            {
                obj_start -= 1;
            }
            let obj_name = &doc.content[obj_start..dot_pos];

            // Check stdlib module method (e.g., Array.map, String.split)
            if let Some(hover_text) = stdlib_hover::hover_stdlib_method(obj_name, word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: hover_text,
                    }),
                    range: None,
                }));
            }

            // Check tsgo member probes (npm imports like z.object)
            if let Some(ty) = doc.type_map.get(&format!("__member_{obj_name}_{word}")) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n(property) {obj_name}.{word}: {ty}\n```"),
                    }),
                    range: None,
                }));
            }

            // The Member's stored type for chain probes is an internal marker
            // (e.g. `Context.req.param`) that renders uselessly — synthesize
            // from the enclosing Call's args and return type instead.
            if let Some(ref typed_program) = doc.typed_program
                && let Some((arg_tys, ret_ty)) =
                    find_enclosing_call_method_sig(typed_program, offset, word)
            {
                let params = arg_tys
                    .iter()
                    .map(Type::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n(method) {word}({params}) -> {ret_ty}\n```"),
                    }),
                    range: None,
                }));
            }

            // Resolve field type from type_map: look for __field_{type}_{field}
            // or fall back to showing the object type
            if let Some(obj_ty) = doc.type_map.get(obj_name) {
                // Try to find the specific field type
                if let Some(field_ty) = doc.type_map.get(&format!("__field_{obj_ty}_{word}")) {
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```floe\n(property) {word}: {field_ty}\n```"),
                        }),
                        range: None,
                    }));
                }

                // Check typed AST before falling back to generic display
                if let Some((_, ref ty)) = typed_ast {
                    return Ok(Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("```floe\n(property) {word}: {}\n```", ty),
                        }),
                        range: None,
                    }));
                }

                // Fall back to showing object type + member
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!(
                            "```floe\n(property) {obj_name}.{word}\n```\n`{obj_name}: {obj_ty}`"
                        ),
                    }),
                    range: None,
                }));
            }

            // Typed AST fallback for member access on call results (e.g. db.insert(...).values)
            // where obj_name can't be extracted from text (preceded by `)` not an identifier)
            if let Some((_, ref ty)) = typed_ast {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n(property) {word}: {}\n```", ty),
                    }),
                    range: None,
                }));
            }
        }

        // Check stdlib module names (Array, String, Option, etc.)
        if !is_import_line && let Some(hover_text) = stdlib_hover::hover_stdlib_module(word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: hover_text,
                }),
                range: None,
            }));
        }

        // In-file bindings shadow stdlib — without this, hovering on an
        // imported `post` would show `Http.post` via the fallback below.
        if !is_member_access && !is_import_line && !symbols.is_empty() {
            if let Some((_, ref ty)) = typed_ast {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n{word}: {}\n```", ty),
                    }),
                    range: None,
                }));
            }
            if let Some(sym) = symbols.first() {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("```floe\n{}\n```", sym.detail),
                    }),
                    range: None,
                }));
            }
        }

        // Check bare stdlib function names (for pipe context only, not member access)
        if !is_member_access
            && !is_import_line
            && let Some(hover_text) = stdlib_hover::hover_stdlib_function(word)
        {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: hover_text,
                }),
                range: None,
            }));
        }

        // Check typed AST — the single source of truth for expression types.
        // Typed AST fallback — for expressions not matched by symbol index or stdlib.
        if let Some((_, ref ty)) = typed_ast {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```floe\n{word}: {}\n```", ty),
                }),
                range: None,
            }));
        }

        // Fallback to builtin hover
        let hover_text = match word {
            "parse" => {
                "```floe\nparse<T>(value: unknown) -> Result<T, Error>\n```\nCompiler built-in: validates a value matches type T at runtime."
            }
            "mock" => {
                "```floe\nmock<T> -> T\n```\nCompiler built-in: generates test data from a type definition. Zero runtime, always in sync with the type."
            }
            "match" => {
                "```floe\nmatch expr { pattern -> body, ... }\n```\nExhaustive pattern matching expression."
            }
            // Note: |> is handled earlier (before empty word check) since it's not a word char
            "todo" => "```floe\ntodo\n```\nPlaceholder for unfinished code. Throws at runtime.",
            "unreachable" => {
                "```floe\nunreachable\n```\nAsserts a code path is impossible. Throws at runtime."
            }
            _ => return Ok(None),
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text.to_string(),
            }),
            range: None,
        }))
    }
}
