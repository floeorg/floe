use super::{
    FnTypeParam, Lowerer, RecordEntry, RecordField, RecordSpread, SyntaxKind, SyntaxNode, TypeDef,
    TypeExpr, TypeExprKind, Variant, VariantField,
};

impl<'src> Lowerer<'src> {
    pub(super) fn lower_type_def_record(&mut self, node: &SyntaxNode) -> TypeDef {
        let mut entries = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::RECORD_FIELD => {
                    if let Some(field) = self.lower_record_field(&child) {
                        entries.push(RecordEntry::Field(Box::new(field)));
                    }
                }
                SyntaxKind::RECORD_SPREAD => {
                    let span = self.node_span(&child);
                    // Lower the type expression inside the spread
                    let type_expr_node =
                        child.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR);
                    let type_expr = type_expr_node
                        .as_ref()
                        .and_then(|n| self.lower_type_expr(n));
                    // Extract the base type name for checker lookups
                    let type_name = type_expr
                        .as_ref()
                        .map(|te| match &te.kind {
                            TypeExprKind::Named { name, .. } | TypeExprKind::TypeOf(name) => {
                                name.clone()
                            }
                            _ => String::new(),
                        })
                        .or_else(|| self.collect_idents_direct(&child).first().cloned())
                        .unwrap_or_default();
                    if !type_name.is_empty() {
                        entries.push(RecordEntry::Spread(RecordSpread {
                            type_name,
                            type_expr,
                            span,
                        }));
                    }
                }
                _ => {}
            }
        }
        TypeDef::Record(entries)
    }

    pub(super) fn lower_type_def_union(&mut self, node: &SyntaxNode) -> TypeDef {
        let mut variants = Vec::new();

        // Check for newtype case: VARIANT_FIELD directly inside TYPE_DEF_UNION (no VARIANT wrapper)
        // This happens for `type OrderId { number }` — synthesize a variant from the parent type name
        let has_direct_field = node
            .children()
            .any(|c| c.kind() == SyntaxKind::VARIANT_FIELD);
        if has_direct_field {
            // Get the type name from the parent TYPE_DECL
            if let Some(parent) = node.parent()
                && let Some(type_name) = self.collect_idents_direct(&parent).first().cloned()
            {
                let span = self.node_span(node);
                let mut fields = Vec::new();
                for child in node.children() {
                    if child.kind() == SyntaxKind::VARIANT_FIELD
                        && let Some(field) = self.lower_variant_field(&child)
                    {
                        fields.push(field);
                    }
                }
                variants.push(Variant {
                    name: type_name,
                    fields,
                    span,
                });
            }
            return TypeDef::Union(variants);
        }

        for child in node.children() {
            if child.kind() == SyntaxKind::VARIANT
                && let Some(variant) = self.lower_variant(&child)
            {
                variants.push(variant);
            }
        }
        TypeDef::Union(variants)
    }

    pub(super) fn lower_type_def_alias(&mut self, node: &SyntaxNode) -> Option<TypeDef> {
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                let type_expr = self.lower_type_expr(&child)?;
                return Some(TypeDef::Alias(type_expr));
            }
        }
        None
    }

    pub(super) fn lower_type_def_string_literal_union(&mut self, node: &SyntaxNode) -> TypeDef {
        let mut variants = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::STRING
            {
                variants.push(self.unquote_string(token.text()));
            }
        }
        TypeDef::StringLiteralUnion(variants)
    }

    pub(super) fn lower_variant(&mut self, node: &SyntaxNode) -> Option<Variant> {
        let span = self.node_span(node);
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut fields = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::VARIANT_FIELD
                && let Some(field) = self.lower_variant_field(&child)
            {
                fields.push(field);
            }
        }

        Some(Variant { name, fields, span })
    }

    pub(super) fn lower_variant_field(&mut self, node: &SyntaxNode) -> Option<VariantField> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);

        // If there's an ident followed by a type expr, it's named
        let mut type_expr_node = None;
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                type_expr_node = Some(child);
                break;
            }
        }

        let type_ann = self.lower_type_expr(&type_expr_node?)?;

        // Check if first ident is the field name (before the colon)
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        let name = if has_colon {
            idents.first().cloned()
        } else {
            None
        };

        Some(VariantField {
            name,
            type_ann,
            span,
        })
    }

    pub(super) fn lower_record_field(&mut self, node: &SyntaxNode) -> Option<RecordField> {
        let span = self.node_span(node);
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut type_ann = None;
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                type_ann = self.lower_type_expr(&child);
                break;
            }
        }

        let default = self.lower_expr_after_eq(node);

        Some(RecordField {
            name,
            type_ann: type_ann?,
            default,
            span,
        })
    }

    pub(super) fn lower_fn_type_param(&mut self, node: &SyntaxNode) -> Option<FnTypeParam> {
        let span = self.node_span(node);
        let label = self.collect_idents_direct(node).first().cloned();
        let type_expr_node = node
            .children()
            .find(|c| c.kind() == SyntaxKind::TYPE_EXPR)?;
        let type_ann = self.lower_type_expr(&type_expr_node)?;
        Some(FnTypeParam {
            label,
            type_ann,
            span,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn lower_type_expr(&mut self, node: &SyntaxNode) -> Option<TypeExpr> {
        let span = self.node_span(node);

        // Intersection type: A & B — single scan to find AMP position
        let amp_pos = node
            .children_with_tokens()
            .find(|c| c.kind() == SyntaxKind::AMP)
            .map(|c| c.text_range().start());
        if let Some(amp_pos) = amp_pos {
            // Left side is tokens/nodes before &, right side is child TYPE_EXPRs after &.
            // Chained intersections (A & B & C) are flattened.
            let mut types = Vec::new();

            // Lower the left side: idents/tokens before &
            let idents = self.collect_idents(node);
            if !idents.is_empty() {
                let name = idents.join(".");
                let is_typeof = node
                    .children_with_tokens()
                    .next()
                    .is_some_and(|first| first.kind() == SyntaxKind::KW_TYPEOF);
                if is_typeof {
                    types.push(TypeExpr {
                        kind: TypeExprKind::TypeOf(name),
                        span,
                    });
                } else {
                    let type_args: Vec<TypeExpr> = node
                        .children()
                        .filter(|c| {
                            c.kind() == SyntaxKind::TYPE_EXPR && c.text_range().start() < amp_pos
                        })
                        .filter_map(|c| self.lower_type_expr(&c))
                        .collect();
                    types.push(TypeExpr {
                        kind: TypeExprKind::Named {
                            name,
                            type_args,
                            bounds: Vec::new(),
                        },
                        span,
                    });
                }
            } else {
                let has_record = node
                    .children()
                    .any(|c| c.kind() == SyntaxKind::RECORD_FIELD);
                if has_record {
                    let fields: Vec<RecordField> = node
                        .children()
                        .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
                        .filter_map(|c| self.lower_record_field(&c))
                        .collect();
                    types.push(TypeExpr {
                        kind: TypeExprKind::Record(fields),
                        span,
                    });
                }
            }

            // Lower the right side: child TYPE_EXPRs after &, flattening nested intersections
            for child in node.children() {
                if child.kind() == SyntaxKind::TYPE_EXPR
                    && child.text_range().start() > amp_pos
                    && let Some(te) = self.lower_type_expr(&child)
                {
                    match te.kind {
                        TypeExprKind::Intersection(inner) => types.extend(inner),
                        _ => types.push(te),
                    }
                }
            }

            if types.len() >= 2 {
                return Some(TypeExpr {
                    kind: TypeExprKind::Intersection(types),
                    span,
                });
            } else if types.len() == 1 {
                return Some(types.into_iter().next().unwrap());
            }
        }

        // Collect direct ident tokens
        let idents = self.collect_idents(node);

        // Check for parens → unit or function type
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);
        let has_rparen = self.has_token(node, SyntaxKind::R_PAREN);
        let has_fat_arrow = self.has_token(node, SyntaxKind::FAT_ARROW);
        let has_thin_arrow = self.has_token(node, SyntaxKind::THIN_ARROW);

        // Unit type: ()
        if has_lparen && has_rparen && idents.is_empty() && !has_fat_arrow && !has_thin_arrow {
            let child_type_exprs: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .collect();
            if child_type_exprs.is_empty() {
                return Some(TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "()".to_string(),
                        type_args: Vec::new(),
                        bounds: Vec::new(),
                    },
                    span,
                });
            }
            // Tuple type: (T, U) — parens with multiple child type exprs, no arrow
            if child_type_exprs.len() >= 2 {
                let types: Vec<TypeExpr> = child_type_exprs
                    .iter()
                    .filter_map(|c| self.lower_type_expr(c))
                    .collect();
                return Some(TypeExpr {
                    kind: TypeExprKind::Tuple(types),
                    span,
                });
            }
        }

        // Function type: (params) -> ReturnType
        if has_fat_arrow || has_thin_arrow {
            let params: Vec<FnTypeParam> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::FN_TYPE_PARAM)
                .filter_map(|c| self.lower_fn_type_param(&c))
                .collect();
            let return_type = node
                .children()
                .find(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .and_then(|c| self.lower_type_expr(&c));

            if let Some(return_type) = return_type {
                return Some(TypeExpr {
                    kind: TypeExprKind::Function {
                        params,
                        return_type: Box::new(return_type),
                    },
                    span,
                });
            }
        }

        // Tuple: [T, U]
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        if has_lbracket {
            let types: Vec<TypeExpr> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .filter_map(|c| self.lower_type_expr(&c))
                .collect();
            return Some(TypeExpr {
                kind: TypeExprKind::Tuple(types),
                span,
            });
        }

        // Record type: { ... }
        let has_record_fields = node
            .children()
            .any(|c| c.kind() == SyntaxKind::RECORD_FIELD);
        if has_record_fields {
            let fields: Vec<RecordField> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
                .filter_map(|c| self.lower_record_field(&c))
                .collect();
            return Some(TypeExpr {
                kind: TypeExprKind::Record(fields),
                span,
            });
        }

        // typeof <ident> — check first token to avoid scanning all children
        let has_typeof = node
            .children_with_tokens()
            .next()
            .is_some_and(|first| first.kind() == SyntaxKind::KW_TYPEOF);
        if has_typeof && !idents.is_empty() {
            let name = idents.join(".");
            return Some(TypeExpr {
                kind: TypeExprKind::TypeOf(name),
                span,
            });
        }

        let string_lit = node.children_with_tokens().find_map(|t| {
            t.as_token()
                .filter(|tok| tok.kind() == SyntaxKind::STRING)
                .map(|tok| self.unquote_string(tok.text()))
        });
        if let Some(value) = string_lit {
            return Some(TypeExpr {
                kind: TypeExprKind::StringLiteral(value),
                span,
            });
        }

        // Named type with optional type args
        if !idents.is_empty() {
            // Join dotted names
            let name = idents.join(".");

            let type_args: Vec<TypeExpr> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .filter_map(|c| self.lower_type_expr(&c))
                .collect();

            return Some(TypeExpr {
                kind: TypeExprKind::Named {
                    name,
                    type_args,
                    bounds: Vec::new(),
                },
                span,
            });
        }

        None
    }
}
