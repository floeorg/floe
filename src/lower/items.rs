use super::*;

impl<'src> Lowerer<'src> {
    pub(super) fn lower_item(&mut self, node: &SyntaxNode) -> Option<Item> {
        let span = self.node_span(node);

        // Find the declaration node inside ITEM
        for child in node.children() {
            match child.kind() {
                SyntaxKind::IMPORT_DECL => {
                    let decl = self.lower_import(&child)?;
                    return Some(Item {
                        kind: ItemKind::Import(decl),
                        span,
                    });
                }
                SyntaxKind::CONST_DECL => {
                    let decl = self.lower_const(&child, node)?;
                    return Some(Item {
                        kind: ItemKind::Const(decl),
                        span,
                    });
                }
                SyntaxKind::FUNCTION_DECL => {
                    let decl = self.lower_function(&child, node)?;
                    return Some(Item {
                        kind: ItemKind::Function(decl),
                        span,
                    });
                }
                SyntaxKind::TYPE_DECL => {
                    let decl = self.lower_type_decl(&child, node)?;
                    return Some(Item {
                        kind: ItemKind::TypeDecl(decl),
                        span,
                    });
                }
                SyntaxKind::FOR_BLOCK => {
                    let exported = self.has_keyword(node, SyntaxKind::KW_EXPORT);
                    let block = self.lower_for_block(&child, exported)?;
                    return Some(Item {
                        kind: ItemKind::ForBlock(block),
                        span,
                    });
                }
                SyntaxKind::TRAIT_DECL => {
                    let decl = self.lower_trait_decl(&child, node)?;
                    return Some(Item {
                        kind: ItemKind::TraitDecl(decl),
                        span,
                    });
                }
                SyntaxKind::TEST_BLOCK => {
                    let block = self.lower_test_block(&child)?;
                    return Some(Item {
                        kind: ItemKind::TestBlock(block),
                        span,
                    });
                }
                _ => {}
            }
        }

        // Could be an expression item directly in ITEM
        if let Some(expr) = self.lower_first_expr(node) {
            return Some(Item {
                kind: ItemKind::Expr(expr),
                span,
            });
        }

        None
    }

    fn lower_import(&mut self, node: &SyntaxNode) -> Option<ImportDecl> {
        let mut specifiers = Vec::new();
        let mut for_specifiers = Vec::new();
        let mut source = String::new();

        for child in node.children() {
            if child.kind() == SyntaxKind::IMPORT_SPECIFIER
                && let Some(spec) = self.lower_import_specifier(&child)
            {
                specifiers.push(spec);
            } else if child.kind() == SyntaxKind::IMPORT_FOR_SPECIFIER
                && let Some(spec) = self.lower_import_for_specifier(&child)
            {
                for_specifiers.push(spec);
            }
        }

        // Find the string token for the source
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::STRING
            {
                source = self.unquote_string(token.text());
            }
        }

        // Check for module-level `trusted` keyword (an IDENT "trusted" directly in IMPORT_DECL)
        let module_trusted = node.children_with_tokens().any(|child| {
            child
                .as_token()
                .is_some_and(|t| t.kind() == SyntaxKind::IDENT && t.text() == "trusted")
        });

        Some(ImportDecl {
            trusted: module_trusted,
            specifiers,
            for_specifiers,
            source,
        })
    }

    fn lower_import_for_specifier(&mut self, node: &SyntaxNode) -> Option<ForImportSpecifier> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);
        let type_name = idents.first()?.clone();

        Some(ForImportSpecifier { type_name, span })
    }

    fn lower_import_specifier(&mut self, node: &SyntaxNode) -> Option<ImportSpecifier> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);

        // Check for per-specifier `trusted` — appears as first IDENT "trusted"
        let per_trusted = idents.first().is_some_and(|name| name == "trusted") && idents.len() >= 2;

        let (name, alias) = if per_trusted {
            // "trusted", "name" [, "alias"]
            (idents[1].clone(), idents.get(2).cloned())
        } else {
            (idents.first()?.clone(), idents.get(1).cloned())
        };

        Some(ImportSpecifier {
            name,
            alias,
            trusted: per_trusted,
            span,
        })
    }

    fn lower_const(&mut self, node: &SyntaxNode, item_node: &SyntaxNode) -> Option<ConstDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);

        let mut binding = None;
        let mut type_ann = None;

        // Collect idents only before `=` to avoid capturing value-side idents
        let idents = self.collect_idents_before_eq(node);
        let has_lbracket = self.has_token_before_eq(node, SyntaxKind::L_BRACKET);
        let has_lbrace = self.has_token_before_eq(node, SyntaxKind::L_BRACE);
        let has_lparen = self.has_token_before_eq(node, SyntaxKind::L_PAREN);

        if has_lbracket {
            binding = Some(ConstBinding::Array(idents));
        } else if has_lparen
            && idents.len() >= 2
            && !node.children().any(|c| c.kind() == SyntaxKind::TYPE_EXPR)
        {
            // Tuple destructuring: const (a, b) = ...
            binding = Some(ConstBinding::Tuple(idents));
        } else if has_lbrace && !node.children().any(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            // Object destructuring — but only if { } is NOT a type expr's record
            // We need to check if the braces are for destructuring vs type annotation
            let fields = self.collect_object_destructure_fields(node, true);
            binding = Some(ConstBinding::Object(fields));
        } else if let Some(name) = idents.first() {
            binding = Some(ConstBinding::Name(name.clone()));
        }

        // Type annotation
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                type_ann = self.lower_type_expr(&child);
                break;
            }
        }

        // Value expression — find the expression after `=`
        let value = self.lower_expr_after_eq(node);

        Some(ConstDecl {
            exported,
            binding: binding?,
            type_ann,
            value: value?,
        })
    }

    pub(super) fn lower_function(
        &mut self,
        node: &SyntaxNode,
        item_node: &SyntaxNode,
    ) -> Option<FunctionDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);
        let async_fn = self.has_keyword(node, SyntaxKind::KW_ASYNC);

        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        // Collect type parameters: idents between < and >
        let type_params = self.collect_type_params(node);

        // Detect `fn name = expr` (derived binding) vs `fn name(params) { body }`
        let is_binding = self.has_token(node, SyntaxKind::EQUAL);

        let mut params = Vec::new();
        let mut return_type = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PARAM if !is_binding => {
                    if let Some(param) = self.lower_param(&child) {
                        params.push(param);
                    }
                }
                SyntaxKind::TYPE_EXPR if !is_binding => {
                    if return_type.is_none() {
                        return_type = self.lower_type_expr(&child);
                    }
                }
                SyntaxKind::BLOCK_EXPR if !is_binding => {
                    body = self.lower_expr_node(&child);
                }
                _ if is_binding && body.is_none() => {
                    // For `fn name = expr`, the body is the expression after `=`
                    body = self.lower_expr_node(&child);
                }
                _ => {}
            }
        }

        // For binding form, also try token expressions (e.g. identifiers)
        if is_binding && body.is_none() {
            body = self.lower_token_expr_after_eq(node);
        }

        Some(FunctionDecl {
            exported,
            async_fn,
            name,
            type_params,
            params,
            return_type,
            body: Box::new(body?),
        })
    }

    pub(super) fn lower_param(&mut self, node: &SyntaxNode) -> Option<Param> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);

        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);

        let (name, destructure) = if has_lbrace {
            // Destructured param: { name, age } or { name: n, age: a }
            let fields = self.collect_object_destructure_fields(node, false);
            let synthetic_name = format!(
                "_{}",
                fields
                    .iter()
                    .map(|f| f.bound_name())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            (synthetic_name, Some(ParamDestructure::Object(fields)))
        } else if has_lparen {
            // Tuple destructured param: (a, b)
            let fields: Vec<String> = idents.clone();
            let synthetic_name = format!("_{}", fields.join("_"));
            (synthetic_name, Some(ParamDestructure::Array(fields)))
        } else {
            (idents.first()?.clone(), None)
        };

        let mut type_ann = None;

        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR && type_ann.is_none() {
                type_ann = self.lower_type_expr(&child);
            }
        }

        // Default value: find expression after `=`
        let default = self.lower_expr_after_eq(node);

        Some(Param {
            name,
            type_ann,
            default,
            destructure,
            span,
        })
    }

    fn lower_type_decl(&mut self, node: &SyntaxNode, item_node: &SyntaxNode) -> Option<TypeDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);
        let opaque = self.has_keyword(node, SyntaxKind::KW_OPAQUE);

        // Collect idents: first is name, rest are type params
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();
        let type_params = idents[1..].to_vec();

        let mut def = None;
        let mut deriving = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::TYPE_DEF_RECORD => {
                    def = Some(self.lower_type_def_record(&child));
                }
                SyntaxKind::TYPE_DEF_UNION => {
                    def = Some(self.lower_type_def_union(&child));
                }
                SyntaxKind::TYPE_DEF_ALIAS => {
                    def = Some(self.lower_type_def_alias(&child)?);
                }
                SyntaxKind::TYPE_DEF_STRING_UNION => {
                    def = Some(self.lower_type_def_string_literal_union(&child));
                }
                SyntaxKind::DERIVING_CLAUSE => {
                    deriving = self.collect_idents_direct(&child);
                }
                _ => {}
            }
        }

        Some(TypeDecl {
            exported,
            opaque,
            name,
            type_params,
            def: def?,
            deriving,
        })
    }

    fn lower_for_block(&mut self, node: &SyntaxNode, item_exported: bool) -> Option<ForBlock> {
        let span = self.node_span(node);

        // Find the type expression (first TYPE_EXPR child)
        let mut type_name = None;
        let mut trait_name = None;
        let mut functions = Vec::new();

        // Collect idents that appear after a colon (trait name)
        let mut saw_colon = false;
        let mut next_exported = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::KW_EXPORT {
                        next_exported = true;
                    } else if token.kind() == SyntaxKind::COLON {
                        saw_colon = true;
                    } else if saw_colon && token.kind() == SyntaxKind::IDENT {
                        trait_name = Some(token.text().to_string());
                        saw_colon = false;
                    }
                }
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::TYPE_EXPR if type_name.is_none() => {
                        type_name = self.lower_type_expr(&child);
                    }
                    SyntaxKind::FUNCTION_DECL => {
                        if let Some(mut decl) = self.lower_for_block_function(&child) {
                            decl.exported = next_exported || item_exported;
                            functions.push(decl);
                        }
                        next_exported = false;
                    }
                    _ => {}
                },
            }
        }

        Some(ForBlock {
            type_name: type_name?,
            trait_name,
            functions,
            span,
        })
    }

    fn lower_trait_decl(&mut self, node: &SyntaxNode, item_node: &SyntaxNode) -> Option<TraitDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);
        let span = self.node_span(node);

        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut methods = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::FUNCTION_DECL
                && let Some(method) = self.lower_trait_method(&child)
            {
                methods.push(method);
            }
        }

        Some(TraitDecl {
            exported,
            name,
            methods,
            span,
        })
    }

    fn lower_trait_method(&mut self, node: &SyntaxNode) -> Option<TraitMethod> {
        let span = self.node_span(node);

        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut params = Vec::new();
        let mut return_type = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PARAM => {
                    if let Some(param) = self.lower_for_block_param(&child) {
                        params.push(param);
                    }
                }
                SyntaxKind::TYPE_EXPR => {
                    if return_type.is_none() {
                        return_type = self.lower_type_expr(&child);
                    }
                }
                SyntaxKind::BLOCK_EXPR => {
                    body = self.lower_expr_node(&child);
                }
                _ => {}
            }
        }

        Some(TraitMethod {
            name,
            params,
            return_type,
            body,
            span,
        })
    }

    fn lower_for_block_function(&mut self, node: &SyntaxNode) -> Option<FunctionDecl> {
        let async_fn = self.has_keyword(node, SyntaxKind::KW_ASYNC);

        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut params = Vec::new();
        let mut return_type = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PARAM => {
                    if let Some(param) = self.lower_for_block_param(&child) {
                        params.push(param);
                    }
                }
                SyntaxKind::TYPE_EXPR => {
                    if return_type.is_none() {
                        return_type = self.lower_type_expr(&child);
                    }
                }
                SyntaxKind::BLOCK_EXPR => {
                    body = self.lower_expr_node(&child);
                }
                _ => {}
            }
        }

        Some(FunctionDecl {
            exported: false,
            async_fn,
            name,
            type_params: self.collect_type_params(node),
            params,
            return_type,
            body: Box::new(body?),
        })
    }

    fn lower_for_block_param(&mut self, node: &SyntaxNode) -> Option<Param> {
        let span = self.node_span(node);

        // Check if this is a `self` parameter
        let has_self = self.has_keyword(node, SyntaxKind::KW_SELF);
        if has_self {
            return Some(Param {
                name: "self".to_string(),
                type_ann: None,
                default: None,
                destructure: None,
                span,
            });
        }

        // Regular parameter
        self.lower_param(node)
    }

    fn lower_test_block(&mut self, node: &SyntaxNode) -> Option<TestBlock> {
        let span = self.node_span(node);

        // Find the string token for the test name
        let mut name = String::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::STRING
            {
                name = self.unquote_string(token.text());
                break;
            }
        }

        // Lower body: assert expressions and regular expressions
        let mut body = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::ASSERT_EXPR => {
                    let assert_span = self.node_span(&child);
                    if let Some(expr) = self.lower_first_expr(&child) {
                        body.push(TestStatement::Assert(expr, assert_span));
                    }
                }
                _ => {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        body.push(TestStatement::Expr(expr));
                    }
                }
            }
        }

        Some(TestBlock { name, body, span })
    }
}
