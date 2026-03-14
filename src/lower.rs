use crate::lexer::span::Span;
use crate::parser::ParseError;
use crate::parser::ast::*;
use crate::syntax::{SyntaxKind, SyntaxNode, ZenLang};

/// Lower a CST `SyntaxNode` (rowan) tree into the existing AST.
pub fn lower_program(root: &SyntaxNode, source: &str) -> Result<Program, Vec<ParseError>> {
    let mut lowerer = Lowerer {
        source,
        errors: Vec::new(),
    };
    let program = lowerer.lower_root(root);
    if lowerer.errors.is_empty() {
        Ok(program)
    } else {
        Err(lowerer.errors)
    }
}

struct Lowerer<'src> {
    source: &'src str,
    errors: Vec<ParseError>,
}

impl<'src> Lowerer<'src> {
    fn lower_root(&mut self, root: &SyntaxNode) -> Program {
        assert_eq!(root.kind(), SyntaxKind::PROGRAM);
        let span = self.node_span(root);
        let mut items = Vec::new();

        for child in root.children() {
            match child.kind() {
                SyntaxKind::ITEM => {
                    if let Some(item) = self.lower_item(&child) {
                        items.push(item);
                    }
                }
                SyntaxKind::EXPR_ITEM => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        let span = self.node_span(&child);
                        items.push(Item {
                            kind: ItemKind::Expr(expr),
                            span,
                        });
                    }
                }
                SyntaxKind::ERROR => {
                    // Collect error text
                    let text = child.text().to_string();
                    self.errors.push(ParseError {
                        message: format!("parse error: {text}"),
                        span: self.node_span(&child),
                    });
                }
                _ => {}
            }
        }

        Program { items, span }
    }

    fn lower_item(&mut self, node: &SyntaxNode) -> Option<Item> {
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
        let mut source = String::new();

        for child in node.children() {
            if child.kind() == SyntaxKind::IMPORT_SPECIFIER
                && let Some(spec) = self.lower_import_specifier(&child)
            {
                specifiers.push(spec);
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

        Some(ImportDecl { specifiers, source })
    }

    fn lower_import_specifier(&mut self, node: &SyntaxNode) -> Option<ImportSpecifier> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);

        let name = idents.first()?.clone();
        let alias = idents.get(1).cloned();

        Some(ImportSpecifier { name, alias, span })
    }

    fn lower_const(&mut self, node: &SyntaxNode, item_node: &SyntaxNode) -> Option<ConstDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);

        let mut binding = None;
        let mut type_ann = None;

        // Determine binding type by looking at tokens
        let idents = self.collect_idents(node);
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);

        if has_lbracket {
            binding = Some(ConstBinding::Array(idents));
        } else if has_lbrace && !node.children().any(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            // Object destructuring — but only if { } is NOT a type expr's record
            // We need to check if the braces are for destructuring vs type annotation
            binding = Some(ConstBinding::Object(idents));
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

    fn lower_function(
        &mut self,
        node: &SyntaxNode,
        item_node: &SyntaxNode,
    ) -> Option<FunctionDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);
        let async_fn = self.has_keyword(node, SyntaxKind::KW_ASYNC);

        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let mut params = Vec::new();
        let mut return_type = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PARAM => {
                    if let Some(param) = self.lower_param(&child) {
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
            exported,
            async_fn,
            name,
            params,
            return_type,
            body: Box::new(body?),
        })
    }

    fn lower_param(&mut self, node: &SyntaxNode) -> Option<Param> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);
        let name = idents.first()?.clone();

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
                _ => {}
            }
        }

        Some(TypeDecl {
            exported,
            opaque,
            name,
            type_params,
            def: def?,
        })
    }

    fn lower_type_def_record(&mut self, node: &SyntaxNode) -> TypeDef {
        let mut fields = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::RECORD_FIELD
                && let Some(field) = self.lower_record_field(&child)
            {
                fields.push(field);
            }
        }
        TypeDef::Record(fields)
    }

    fn lower_type_def_union(&mut self, node: &SyntaxNode) -> TypeDef {
        let mut variants = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::VARIANT
                && let Some(variant) = self.lower_variant(&child)
            {
                variants.push(variant);
            }
        }
        TypeDef::Union(variants)
    }

    fn lower_type_def_alias(&mut self, node: &SyntaxNode) -> Option<TypeDef> {
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                let type_expr = self.lower_type_expr(&child)?;
                return Some(TypeDef::Alias(type_expr));
            }
        }
        None
    }

    fn lower_variant(&mut self, node: &SyntaxNode) -> Option<Variant> {
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

    fn lower_variant_field(&mut self, node: &SyntaxNode) -> Option<VariantField> {
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

    fn lower_record_field(&mut self, node: &SyntaxNode) -> Option<RecordField> {
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

    fn lower_type_expr(&mut self, node: &SyntaxNode) -> Option<TypeExpr> {
        let span = self.node_span(node);

        // Collect direct ident tokens
        let idents = self.collect_idents(node);

        // Check for parens → unit or function type
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);
        let has_rparen = self.has_token(node, SyntaxKind::R_PAREN);
        let has_fat_arrow = self.has_token(node, SyntaxKind::FAT_ARROW);

        // Unit type: ()
        if has_lparen && has_rparen && idents.is_empty() && !has_fat_arrow {
            let child_type_exprs: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .collect();
            if child_type_exprs.is_empty() {
                return Some(TypeExpr {
                    kind: TypeExprKind::Named {
                        name: "()".to_string(),
                        type_args: Vec::new(),
                    },
                    span,
                });
            }
        }

        // Function type: (params) => ReturnType
        if has_fat_arrow {
            let type_exprs: Vec<TypeExpr> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
                .filter_map(|c| self.lower_type_expr(&c))
                .collect();

            if let Some((return_type, params)) = type_exprs.split_last() {
                return Some(TypeExpr {
                    kind: TypeExprKind::Function {
                        params: params.to_vec(),
                        return_type: Box::new(return_type.clone()),
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
                kind: TypeExprKind::Named { name, type_args },
                span,
            });
        }

        None
    }

    // ── Expression lowering ─────────────────────────────────────

    fn lower_expr_node(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let span = self.node_span(node);

        match node.kind() {
            SyntaxKind::PIPE_EXPR => {
                let exprs = self.lower_child_exprs(node);
                if exprs.len() >= 2 {
                    let mut iter = exprs.into_iter();
                    let left = iter.next()?;
                    let right = iter.next()?;
                    Some(Expr {
                        span: self.node_span(node),
                        kind: ExprKind::Pipe {
                            left: Box::new(left),
                            right: Box::new(right),
                        },
                    })
                } else {
                    exprs.into_iter().next()
                }
            }

            SyntaxKind::BINARY_EXPR => {
                let op = self.find_binary_op(node)?;
                let exprs = self.lower_child_exprs(node);
                if exprs.len() >= 2 {
                    let mut iter = exprs.into_iter();
                    let left = iter.next()?;
                    let right = iter.next()?;
                    Some(Expr {
                        span,
                        kind: ExprKind::Binary {
                            left: Box::new(left),
                            op,
                            right: Box::new(right),
                        },
                    })
                } else {
                    None
                }
            }

            SyntaxKind::UNARY_EXPR => {
                let op = self.find_unary_op(node)?;
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Unary {
                        op,
                        operand: Box::new(operand),
                    },
                })
            }

            SyntaxKind::AWAIT_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Await(Box::new(operand)),
                })
            }

            SyntaxKind::UNWRAP_EXPR => {
                let operand = self.lower_child_exprs(node).into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Unwrap(Box::new(operand)),
                })
            }

            SyntaxKind::MEMBER_EXPR => {
                let exprs = self.lower_child_exprs(node);
                let object = exprs.into_iter().next()?;
                let idents = self.collect_idents(node);
                let field = idents.last()?.clone();
                Some(Expr {
                    span,
                    kind: ExprKind::Member {
                        object: Box::new(object),
                        field,
                    },
                })
            }

            SyntaxKind::INDEX_EXPR => {
                let exprs = self.lower_child_exprs(node);
                let mut iter = exprs.into_iter();
                let object = iter.next()?;
                let index = iter.next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Index {
                        object: Box::new(object),
                        index: Box::new(index),
                    },
                })
            }

            SyntaxKind::CALL_EXPR => {
                let mut child_exprs = Vec::new();
                let mut args = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::ARG => {
                            if let Some(arg) = self.lower_arg(&child) {
                                args.push(arg);
                            }
                        }
                        _ => {
                            if let Some(expr) = self.lower_expr_node(&child) {
                                child_exprs.push(expr);
                            }
                        }
                    }
                }

                // Also check for token-level expressions (ident, number, etc.)
                if child_exprs.is_empty()
                    && let Some(expr) = self.lower_token_expr(node)
                {
                    child_exprs.push(expr);
                }

                let callee = child_exprs.into_iter().next()?;
                Some(Expr {
                    span,
                    kind: ExprKind::Call {
                        callee: Box::new(callee),
                        args,
                    },
                })
            }

            SyntaxKind::CONSTRUCT_EXPR => {
                let idents = self.collect_idents_direct(node);
                let type_name = idents.first()?.clone();

                let mut spread = None;
                let mut args = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::SPREAD_EXPR => {
                            let inner = self.lower_child_exprs(&child).into_iter().next()?;
                            spread = Some(Box::new(inner));
                        }
                        SyntaxKind::ARG => {
                            if let Some(arg) = self.lower_arg(&child) {
                                args.push(arg);
                            }
                        }
                        _ => {}
                    }
                }

                Some(Expr {
                    span,
                    kind: ExprKind::Construct {
                        type_name,
                        spread,
                        args,
                    },
                })
            }

            SyntaxKind::ARROW_EXPR => {
                let mut params = Vec::new();
                let mut body = None;

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::PARAM => {
                            if let Some(param) = self.lower_param(&child) {
                                params.push(param);
                            }
                        }
                        _ => {
                            if body.is_none() {
                                body = self.lower_expr_node(&child);
                            }
                        }
                    }
                }

                // If no child expression nodes, try token expr
                if body.is_none() {
                    body = self.lower_token_expr_after_fat_arrow(node);
                }

                Some(Expr {
                    span,
                    kind: ExprKind::Arrow {
                        params,
                        body: Box::new(body?),
                    },
                })
            }

            SyntaxKind::MATCH_EXPR => {
                let mut subject = None;
                let mut arms = Vec::new();

                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::MATCH_ARM => {
                            if let Some(arm) = self.lower_match_arm(&child) {
                                arms.push(arm);
                            }
                        }
                        _ => {
                            if subject.is_none() {
                                subject = self.lower_expr_node(&child);
                                if subject.is_none() {
                                    subject = self.lower_token_expr_in_node(&child);
                                }
                            }
                        }
                    }
                }

                // If subject wasn't a child node, try as token in match expr node directly
                if subject.is_none() {
                    subject = self.lower_token_expr(node);
                }

                Some(Expr {
                    span,
                    kind: ExprKind::Match {
                        subject: Box::new(subject?),
                        arms,
                    },
                })
            }

            SyntaxKind::IF_EXPR => {
                let mut exprs = Vec::new();
                for child in node.children() {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        exprs.push(expr);
                    }
                }

                // Also collect token-level expressions
                if exprs.is_empty()
                    && let Some(expr) = self.lower_token_expr(node)
                {
                    exprs.push(expr);
                }

                let condition = exprs.first().cloned()?;
                let then_branch = exprs.get(1).cloned()?;
                let else_branch = exprs.get(2).cloned().map(Box::new);

                Some(Expr {
                    span,
                    kind: ExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                })
            }

            SyntaxKind::BLOCK_EXPR => {
                let mut items = Vec::new();
                for child in node.children() {
                    match child.kind() {
                        SyntaxKind::ITEM => {
                            if let Some(item) = self.lower_item(&child) {
                                items.push(item);
                            }
                        }
                        SyntaxKind::EXPR_ITEM => {
                            if let Some(expr) = self.lower_first_expr(&child) {
                                items.push(Item {
                                    kind: ItemKind::Expr(expr),
                                    span: self.node_span(&child),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Some(Expr {
                    span,
                    kind: ExprKind::Block(items),
                })
            }

            SyntaxKind::RETURN_EXPR => {
                let value = self.lower_child_exprs(node).into_iter().next();
                if value.is_none() {
                    // Try token expr
                    let tok_expr = self.lower_token_expr(node);
                    return Some(Expr {
                        span,
                        kind: ExprKind::Return(tok_expr.map(Box::new)),
                    });
                }
                Some(Expr {
                    span,
                    kind: ExprKind::Return(value.map(Box::new)),
                })
            }

            SyntaxKind::OK_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Ok(Box::new(inner)),
                })
            }

            SyntaxKind::ERR_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Err(Box::new(inner)),
                })
            }

            SyntaxKind::SOME_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Some(Box::new(inner)),
                })
            }

            SyntaxKind::GROUPED_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Grouped(Box::new(inner)),
                })
            }

            SyntaxKind::ARRAY_EXPR => {
                let elements = self.lower_child_exprs_and_tokens(node);
                Some(Expr {
                    span,
                    kind: ExprKind::Array(elements),
                })
            }

            SyntaxKind::JSX_ELEMENT => {
                let element = self.lower_jsx_element(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Jsx(element),
                })
            }

            SyntaxKind::SPREAD_EXPR => {
                let inner = self.lower_first_expr_in(node)?;
                Some(Expr {
                    span,
                    kind: ExprKind::Spread(Box::new(inner)),
                })
            }

            SyntaxKind::ERROR => None,

            // For other kinds, try to extract token-level expressions
            _ => self.lower_token_expr_in_node(node),
        }
    }

    fn lower_first_expr(&mut self, node: &SyntaxNode) -> Option<Expr> {
        // First try child nodes
        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                return Some(expr);
            }
        }
        // Then try tokens
        self.lower_token_expr(node)
    }

    fn lower_first_expr_in(&mut self, node: &SyntaxNode) -> Option<Expr> {
        self.lower_first_expr(node)
    }

    fn lower_child_exprs(&mut self, node: &SyntaxNode) -> Vec<Expr> {
        let mut exprs = Vec::new();
        let mut found_first_token_expr = false;

        for child in node.children() {
            if let Some(expr) = self.lower_expr_node(&child) {
                exprs.push(expr);
            }
        }

        // If no child expr nodes, try token exprs
        if exprs.is_empty() {
            for token in node.children_with_tokens() {
                if let Some(token) = token.as_token()
                    && let Some(expr) = self.token_to_expr(token)
                    && (!found_first_token_expr || token.kind() != SyntaxKind::IDENT)
                {
                    exprs.push(expr);
                    found_first_token_expr = true;
                }
            }
        }

        exprs
    }

    fn lower_child_exprs_and_tokens(&mut self, node: &SyntaxNode) -> Vec<Expr> {
        let mut exprs = Vec::new();

        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Node(child) => {
                    if let Some(expr) = self.lower_expr_node(&child) {
                        exprs.push(expr);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr) = self.token_to_expr(&token) {
                        exprs.push(expr);
                    }
                }
            }
        }

        exprs
    }

    fn lower_token_expr(&mut self, node: &SyntaxNode) -> Option<Expr> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && let Some(expr) = self.token_to_expr(token)
            {
                return Some(expr);
            }
        }
        None
    }

    fn lower_token_expr_in_node(&mut self, node: &SyntaxNode) -> Option<Expr> {
        self.lower_token_expr(node)
    }

    fn lower_token_expr_after_fat_arrow(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_arrow = false;
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::FAT_ARROW {
                    past_arrow = true;
                    continue;
                }
                if past_arrow && let Some(expr) = self.token_to_expr(token) {
                    return Some(expr);
                }
            }
        }
        None
    }

    fn token_to_expr(&self, token: &rowan::SyntaxToken<ZenLang>) -> Option<Expr> {
        let span = self.token_span(token);
        let text = token.text();

        match token.kind() {
            SyntaxKind::NUMBER => Some(Expr {
                kind: ExprKind::Number(text.to_string()),
                span,
            }),
            SyntaxKind::STRING => Some(Expr {
                kind: ExprKind::String(self.unquote_string(text)),
                span,
            }),
            SyntaxKind::TEMPLATE_LITERAL => {
                // Template literals are complex — for now, store as raw
                // The lowering for interpolations needs the original token parts
                // We'll handle this separately
                Some(Expr {
                    kind: ExprKind::TemplateLiteral(vec![TemplatePart::Raw(
                        text[1..text.len().saturating_sub(1)].to_string(),
                    )]),
                    span,
                })
            }
            SyntaxKind::BOOL => Some(Expr {
                kind: ExprKind::Bool(text == "true"),
                span,
            }),
            SyntaxKind::IDENT => Some(Expr {
                kind: ExprKind::Identifier(text.to_string()),
                span,
            }),
            SyntaxKind::UNDERSCORE => Some(Expr {
                kind: ExprKind::Placeholder,
                span,
            }),
            SyntaxKind::KW_NONE => Some(Expr {
                kind: ExprKind::None,
                span,
            }),
            _ => None,
        }
    }

    fn lower_arg(&mut self, node: &SyntaxNode) -> Option<Arg> {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        if has_colon {
            let idents = self.collect_idents_direct(node);
            let label = idents.first()?.clone();
            let value = self.lower_first_expr(node)?;
            Some(Arg::Named { label, value })
        } else {
            let expr = self.lower_first_expr(node)?;
            Some(Arg::Positional(expr))
        }
    }

    fn lower_match_arm(&mut self, node: &SyntaxNode) -> Option<MatchArm> {
        let span = self.node_span(node);
        let mut pattern = None;
        let mut body = None;

        for child in node.children() {
            match child.kind() {
                SyntaxKind::PATTERN => {
                    if pattern.is_none() {
                        pattern = self.lower_pattern(&child);
                    }
                }
                _ => {
                    if body.is_none() {
                        body = self.lower_expr_node(&child);
                    }
                }
            }
        }

        // If body wasn't found in child nodes, check tokens after ->
        if body.is_none() {
            body = self.lower_token_expr_after_arrow(node);
        }

        Some(MatchArm {
            pattern: pattern?,
            body: body?,
            span,
        })
    }

    fn lower_token_expr_after_arrow(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_arrow = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::THIN_ARROW {
                        past_arrow = true;
                        continue;
                    }
                    if past_arrow && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_arrow {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }

    fn lower_pattern(&mut self, node: &SyntaxNode) -> Option<Pattern> {
        let span = self.node_span(node);

        // Check tokens for simple patterns
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::UNDERSCORE => {
                        return Some(Pattern {
                            kind: PatternKind::Wildcard,
                            span,
                        });
                    }
                    SyntaxKind::BOOL => {
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::Bool(
                                token.text() == "true",
                            )),
                            span,
                        });
                    }
                    SyntaxKind::STRING => {
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::String(
                                self.unquote_string(token.text()),
                            )),
                            span,
                        });
                    }
                    SyntaxKind::NUMBER => {
                        // Check for range
                        if self.has_token(node, SyntaxKind::DOT_DOT) {
                            let numbers = self.collect_numbers(node);
                            if numbers.len() >= 2 {
                                return Some(Pattern {
                                    kind: PatternKind::Range {
                                        start: LiteralPattern::Number(numbers[0].clone()),
                                        end: LiteralPattern::Number(numbers[1].clone()),
                                    },
                                    span,
                                });
                            }
                        }
                        return Some(Pattern {
                            kind: PatternKind::Literal(LiteralPattern::Number(
                                token.text().to_string(),
                            )),
                            span,
                        });
                    }
                    SyntaxKind::KW_NONE => {
                        return Some(Pattern {
                            kind: PatternKind::Variant {
                                name: "None".to_string(),
                                fields: Vec::new(),
                            },
                            span,
                        });
                    }
                    SyntaxKind::KW_OK | SyntaxKind::KW_ERR | SyntaxKind::KW_SOME => {
                        let name = token.text().to_string();
                        let fields: Vec<Pattern> = node
                            .children()
                            .filter(|c| c.kind() == SyntaxKind::PATTERN)
                            .filter_map(|c| self.lower_pattern(&c))
                            .collect();
                        return Some(Pattern {
                            kind: PatternKind::Variant { name, fields },
                            span,
                        });
                    }
                    SyntaxKind::IDENT => {
                        let name = token.text().to_string();
                        if name.starts_with(char::is_uppercase) {
                            let fields: Vec<Pattern> = node
                                .children()
                                .filter(|c| c.kind() == SyntaxKind::PATTERN)
                                .filter_map(|c| self.lower_pattern(&c))
                                .collect();
                            return Some(Pattern {
                                kind: PatternKind::Variant { name, fields },
                                span,
                            });
                        } else {
                            return Some(Pattern {
                                kind: PatternKind::Binding(name),
                                span,
                            });
                        }
                    }
                    SyntaxKind::L_BRACE => {
                        // Record pattern
                        let fields = self.lower_record_pattern_fields(node);
                        return Some(Pattern {
                            kind: PatternKind::Record { fields },
                            span,
                        });
                    }
                    _ => {}
                }
            }
        }

        None
    }

    fn lower_record_pattern_fields(&mut self, node: &SyntaxNode) -> Vec<(String, Pattern)> {
        let mut fields = Vec::new();
        let idents = self.collect_idents(node);

        // Simple approach: collect ident tokens and check for colon patterns
        // This needs more sophisticated handling for complex patterns
        for ident in &idents {
            // For now, assume shorthand: `{ x }` → `{ x: x }`
            fields.push((
                ident.clone(),
                Pattern {
                    kind: PatternKind::Binding(ident.clone()),
                    span: self.node_span(node),
                },
            ));
        }

        fields
    }

    // ── JSX lowering ────────────────────────────────────────────

    fn lower_jsx_element(&mut self, node: &SyntaxNode) -> Option<JsxElement> {
        let span = self.node_span(node);

        // Detect fragment: no tag name idents
        let idents = self.collect_idents_direct(node);

        if idents.is_empty() {
            // Fragment
            let children = self.lower_jsx_children(node);
            return Some(JsxElement {
                kind: JsxElementKind::Fragment { children },
                span,
            });
        }

        let name = idents.first()?.clone();
        let self_closing = self.has_token(node, SyntaxKind::SLASH);

        let mut props = Vec::new();
        let mut children = Vec::new();

        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_PROP => {
                    if let Some(prop) = self.lower_jsx_prop(&child) {
                        props.push(prop);
                    }
                }
                SyntaxKind::JSX_EXPR_CHILD => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        children.push(JsxChild::Expr(expr));
                    }
                }
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChild::Text(text.trim().to_string()));
                    }
                }
                SyntaxKind::JSX_ELEMENT => {
                    if let Some(element) = self.lower_jsx_element(&child) {
                        children.push(JsxChild::Element(element));
                    }
                }
                _ => {}
            }
        }

        Some(JsxElement {
            kind: JsxElementKind::Element {
                name,
                props,
                children,
                self_closing,
            },
            span,
        })
    }

    fn lower_jsx_children(&mut self, node: &SyntaxNode) -> Vec<JsxChild> {
        let mut children = Vec::new();
        for child in node.children() {
            match child.kind() {
                SyntaxKind::JSX_EXPR_CHILD => {
                    if let Some(expr) = self.lower_first_expr(&child) {
                        children.push(JsxChild::Expr(expr));
                    }
                }
                SyntaxKind::JSX_TEXT => {
                    let text = child.text().to_string();
                    if !text.trim().is_empty() {
                        children.push(JsxChild::Text(text.trim().to_string()));
                    }
                }
                SyntaxKind::JSX_ELEMENT => {
                    if let Some(element) = self.lower_jsx_element(&child) {
                        children.push(JsxChild::Element(element));
                    }
                }
                _ => {}
            }
        }
        children
    }

    fn lower_jsx_prop(&mut self, node: &SyntaxNode) -> Option<JsxProp> {
        let span = self.node_span(node);
        let idents = self.collect_idents_direct(node);
        let name = idents.first()?.clone();

        let value = self.lower_first_expr(node);

        Some(JsxProp { name, value, span })
    }

    // ── Utility helpers ─────────────────────────────────────────

    fn node_span(&self, node: &SyntaxNode) -> Span {
        let range = node.text_range();
        let start = range.start().into();
        let end = range.end().into();

        // Compute line/column from byte offset
        let (line, column) = self.offset_to_line_col(start);
        Span::new(start, end, line, column)
    }

    fn token_span(&self, token: &rowan::SyntaxToken<ZenLang>) -> Span {
        let range = token.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        let (line, column) = self.offset_to_line_col(start);
        Span::new(start, end, line, column)
    }

    fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for &b in &self.source.as_bytes()[..offset.min(self.source.len())] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn collect_idents(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::IDENT
            {
                idents.push(token.text().to_string());
            }
        }
        idents
    }

    /// Collect only direct ident tokens (not from child nodes).
    fn collect_idents_direct(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::IDENT
            {
                idents.push(token.text().to_string());
            }
        }
        idents
    }

    fn collect_numbers(&self, node: &SyntaxNode) -> Vec<String> {
        let mut numbers = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token()
                && token.kind() == SyntaxKind::NUMBER
            {
                numbers.push(token.text().to_string());
            }
        }
        numbers
    }

    fn has_keyword(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    fn has_token(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.children_with_tokens()
            .any(|t| t.as_token().is_some_and(|t| t.kind() == kind))
    }

    fn unquote_string(&self, text: &str) -> String {
        // Remove surrounding quotes
        if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
            let inner = &text[1..text.len() - 1];
            // Process escape sequences
            let mut result = String::new();
            let mut chars = inner.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some('n') => result.push('\n'),
                        Some('t') => result.push('\t'),
                        Some('r') => result.push('\r'),
                        Some('\\') => result.push('\\'),
                        Some('"') => result.push('"'),
                        Some('0') => result.push('\0'),
                        Some(c) => {
                            result.push('\\');
                            result.push(c);
                        }
                        None => result.push('\\'),
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        } else {
            text.to_string()
        }
    }

    fn find_binary_op(&self, node: &SyntaxNode) -> Option<BinOp> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                let op = match token.kind() {
                    SyntaxKind::PLUS => Some(BinOp::Add),
                    SyntaxKind::MINUS => Some(BinOp::Sub),
                    SyntaxKind::STAR => Some(BinOp::Mul),
                    SyntaxKind::SLASH => Some(BinOp::Div),
                    SyntaxKind::PERCENT => Some(BinOp::Mod),
                    SyntaxKind::EQUAL_EQUAL => Some(BinOp::Eq),
                    SyntaxKind::BANG_EQUAL => Some(BinOp::NotEq),
                    SyntaxKind::LESS_THAN => Some(BinOp::Lt),
                    SyntaxKind::GREATER_THAN => Some(BinOp::Gt),
                    SyntaxKind::LESS_EQUAL => Some(BinOp::LtEq),
                    SyntaxKind::GREATER_EQUAL => Some(BinOp::GtEq),
                    SyntaxKind::AMP_AMP => Some(BinOp::And),
                    SyntaxKind::PIPE_PIPE => Some(BinOp::Or),
                    _ => None,
                };
                if op.is_some() {
                    return op;
                }
            }
        }
        None
    }

    fn find_unary_op(&self, node: &SyntaxNode) -> Option<UnaryOp> {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                match token.kind() {
                    SyntaxKind::BANG => return Some(UnaryOp::Not),
                    SyntaxKind::MINUS => return Some(UnaryOp::Neg),
                    _ => {}
                }
            }
        }
        None
    }

    fn lower_expr_after_eq(&mut self, node: &SyntaxNode) -> Option<Expr> {
        let mut past_eq = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::EQUAL {
                        past_eq = true;
                        continue;
                    }
                    if past_eq && let Some(expr) = self.token_to_expr(&token) {
                        return Some(expr);
                    }
                }
                rowan::NodeOrToken::Node(child) => {
                    if past_eq {
                        return self.lower_expr_node(&child);
                    }
                }
            }
        }
        None
    }
}
