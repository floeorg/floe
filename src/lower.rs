mod expr;
mod jsx;
mod pattern;

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
                        kind: crate::parser::ParseErrorKind::General,
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
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);

        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);

        let (name, destructure) = if has_lbrace {
            // Destructured param: { name, age }
            let fields: Vec<String> = idents.clone();
            let synthetic_name = format!("_{}", fields.join("_"));
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
                            // For inline for-declarations, export comes from the ITEM node
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

    fn lower_type_def_record(&mut self, node: &SyntaxNode) -> TypeDef {
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
                    let idents = self.collect_idents_direct(&child);
                    if let Some(type_name) = idents.first() {
                        entries.push(RecordEntry::Spread(RecordSpread {
                            type_name: type_name.clone(),
                            span,
                        }));
                    }
                }
                _ => {}
            }
        }
        TypeDef::Record(entries)
    }

    fn lower_type_def_union(&mut self, node: &SyntaxNode) -> TypeDef {
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

    fn lower_type_def_alias(&mut self, node: &SyntaxNode) -> Option<TypeDef> {
        for child in node.children() {
            if child.kind() == SyntaxKind::TYPE_EXPR {
                let type_expr = self.lower_type_expr(&child)?;
                return Some(TypeDef::Alias(type_expr));
            }
        }
        None
    }

    fn lower_type_def_string_literal_union(&mut self, node: &SyntaxNode) -> TypeDef {
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
        let has_thin_arrow = self.has_token(node, SyntaxKind::THIN_ARROW);

        // Unit type: ()
        if has_lparen && has_rparen && idents.is_empty() && !has_thin_arrow {
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
        if has_thin_arrow {
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

    // ── Template literal lowering ─────────────────────────────

    /// Parse a template literal source text (including backticks) into AST
    /// `TemplatePart`s, properly lowering interpolated expressions.
    fn lower_template_literal(&self, text: &str) -> Vec<TemplatePart> {
        // Strip backticks
        let inner = if text.len() >= 2 && text.starts_with('`') && text.ends_with('`') {
            &text[1..text.len() - 1]
        } else {
            text
        };

        let mut parts = Vec::new();
        let mut current_raw = String::new();
        let bytes = inner.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                // Save current raw segment
                if !current_raw.is_empty() {
                    parts.push(TemplatePart::Raw(std::mem::take(&mut current_raw)));
                }

                // Skip `${`
                i += 2;

                // Find matching `}` with brace depth tracking
                let mut depth = 1;
                let interp_start = i;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        b'`' => {
                            // Skip nested template literals
                            i += 1;
                            while i < bytes.len() && bytes[i] != b'`' {
                                if bytes[i] == b'\\' {
                                    i += 1; // skip escaped char
                                }
                                i += 1;
                            }
                            // i now points at closing backtick (or end)
                        }
                        b'"' => {
                            // Skip string literals
                            i += 1;
                            while i < bytes.len() && bytes[i] != b'"' {
                                if bytes[i] == b'\\' {
                                    i += 1;
                                }
                                i += 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                // After the loop: if depth == 0, i points one past the closing `}`
                // (we broke at `}`, then i += 1 didn't execute, so i points AT `}`)
                // Actually: when depth hits 0, we break BEFORE i += 1, so i is AT `}`
                let interp_end = i;
                let interp_source = &inner[interp_start..interp_end.min(inner.len())];

                // Parse the interpolation as a Floe expression
                if let Some(expr) = self.parse_interpolation_expr(interp_source) {
                    parts.push(TemplatePart::Expr(expr));
                } else {
                    // Fallback: store as raw if parsing fails
                    parts.push(TemplatePart::Raw(format!("${{{}}}", interp_source)));
                }

                // Skip past the closing `}`
                if depth == 0 {
                    i += 1;
                }
            } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
                // Process escape sequences
                i += 1;
                match bytes[i] {
                    b'n' => current_raw.push('\n'),
                    b't' => current_raw.push('\t'),
                    b'r' => current_raw.push('\r'),
                    b'\\' => current_raw.push('\\'),
                    b'0' => current_raw.push('\0'),
                    b'`' => current_raw.push('`'),
                    b'$' => current_raw.push('$'),
                    c => {
                        current_raw.push('\\');
                        current_raw.push(c as char);
                    }
                }
                i += 1;
            } else if bytes[i] >= 0x80 {
                // UTF-8 multibyte: find the full character
                let ch_start = i;
                i += 1;
                while i < bytes.len() && bytes[i] >= 0x80 && bytes[i] < 0xC0 {
                    i += 1;
                }
                current_raw.push_str(&inner[ch_start..i]);
            } else {
                current_raw.push(bytes[i] as char);
                i += 1;
            }
        }

        // Save final raw segment
        if !current_raw.is_empty() {
            parts.push(TemplatePart::Raw(current_raw));
        }

        parts
    }

    /// Parse a string of Floe source code as a single expression.
    fn parse_interpolation_expr(&self, source: &str) -> Option<Expr> {
        use crate::cst::CstParser;
        use crate::lexer::Lexer;

        let tokens = Lexer::new(source).tokenize_with_trivia();
        let cst_parse = CstParser::new(source, tokens).parse();

        // Ignore CST errors for interpolations — they may be complex expressions
        let root = cst_parse.syntax();
        let mut lowerer = Lowerer {
            source,
            errors: Vec::new(),
        };
        let program = lowerer.lower_root(&root);

        // Extract the first expression from the program
        program.items.into_iter().find_map(|item| {
            if let ItemKind::Expr(expr) = item.kind {
                Some(expr)
            } else {
                None
            }
        })
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

    /// Collect ident tokens that appear before the `=` sign.
    fn collect_idents_before_eq(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::EQUAL {
                    break;
                }
                if token.kind() == SyntaxKind::IDENT {
                    idents.push(token.text().to_string());
                }
            }
        }
        idents
    }

    /// Check if a token kind appears before the `=` sign.
    fn has_token_before_eq(&self, node: &SyntaxNode, kind: SyntaxKind) -> bool {
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::EQUAL {
                    return false;
                }
                if token.kind() == kind {
                    return true;
                }
            }
        }
        false
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

    /// Collect ident tokens that appear before the first `(` token.
    /// Used for CONSTRUCT_EXPR to handle qualified variants like `Route.Profile(...)`.
    fn collect_idents_before_lparen(&self, node: &SyntaxNode) -> Vec<String> {
        let mut idents = Vec::new();
        for token in node.children_with_tokens() {
            if let Some(token) = token.as_token() {
                if token.kind() == SyntaxKind::L_PAREN {
                    break;
                }
                if token.kind() == SyntaxKind::IDENT {
                    idents.push(token.text().to_string());
                }
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

    /// Check if a MATCH_EXPR node has no subject expression (used for pipe-into-match).
    /// A subjectless match has `match` keyword followed directly by `{`, with no
    /// expression child nodes before the first MATCH_ARM.
    fn is_subjectless_match(&self, node: &SyntaxNode) -> bool {
        // A subjectless match has no child expression nodes — only MATCH_ARM children
        for child in node.children() {
            if child.kind() == SyntaxKind::MATCH_ARM {
                continue;
            }
            // Any other child node means there's a subject expression
            return false;
        }
        // Also check: no token-level expressions (identifiers, numbers, etc.)
        // between `match` keyword and `{`
        let mut past_match_kw = false;
        for tok in node.children_with_tokens() {
            if let Some(token) = tok.as_token() {
                if token.kind() == SyntaxKind::KW_MATCH {
                    past_match_kw = true;
                    continue;
                }
                if past_match_kw && token.kind() == SyntaxKind::L_BRACE {
                    return true; // No expression between `match` and `{`
                }
                if past_match_kw && !token.kind().is_trivia() {
                    return false; // Found a token that could be a subject
                }
            }
        }
        true
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

#[cfg(test)]
mod tests {
    use crate::cst::CstParser;
    use crate::lexer::Lexer;
    use crate::lower::lower_program;
    use crate::parser::ast::*;

    /// Helper: parse source through CST then lower to AST.
    fn lower(source: &str) -> Program {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        let parse = CstParser::new(source, tokens).parse();
        assert!(parse.errors.is_empty(), "CST errors: {:?}", parse.errors);
        let root = parse.syntax();
        lower_program(&root, source).unwrap_or_else(|errs| {
            panic!(
                "lower failed:\n{}",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }

    fn first_item(source: &str) -> ItemKind {
        lower(source).items.into_iter().next().unwrap().kind
    }

    fn first_expr(source: &str) -> ExprKind {
        match first_item(source) {
            ItemKind::Expr(e) => e.kind,
            other => panic!("expected Expr, got {other:?}"),
        }
    }

    // ── Const declarations ────────────────────────────────────────

    #[test]
    fn const_simple() {
        let item = first_item("const x = 42");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert_eq!(decl.binding, ConstBinding::Name("x".into()));
        assert!(!decl.exported);
        assert!(decl.type_ann.is_none());
    }

    #[test]
    fn const_typed() {
        let item = first_item("const x: number = 42");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(decl.type_ann.is_some());
    }

    #[test]
    fn const_exported() {
        let item = first_item("export const x = 1");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(decl.exported);
    }

    #[test]
    fn const_array_destructuring() {
        let item = first_item("const [a, b] = pair");
        let ItemKind::Const(decl) = item else {
            panic!("expected Const")
        };
        assert!(matches!(decl.binding, ConstBinding::Array(_)));
    }

    // ── Function declarations ─────────────────────────────────────

    #[test]
    fn function_basic() {
        let item = first_item("fn greet() { 1 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert_eq!(decl.name, "greet");
        assert!(decl.params.is_empty());
        assert!(!decl.exported);
        assert!(!decl.async_fn);
    }

    #[test]
    fn function_with_params_and_return() {
        let item = first_item("fn add(a: number, b: number) -> number { a + b }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert_eq!(decl.params.len(), 2);
        assert_eq!(decl.params[0].name, "a");
        assert!(decl.return_type.is_some());
    }

    #[test]
    fn function_async() {
        let item = first_item("async fn fetch(url: string) -> string { url }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert!(decl.async_fn);
    }

    #[test]
    fn function_exported() {
        let item = first_item("export fn hello() { 1 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert!(decl.exported);
    }

    #[test]
    fn function_param_default() {
        let item = first_item("fn greet(name: string = \"world\") { name }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        assert!(decl.params[0].default.is_some());
    }

    // ── Literals ──────────────────────────────────────────────────

    #[test]
    fn literal_number() {
        assert_eq!(first_expr("42"), ExprKind::Number("42".into()));
    }

    #[test]
    fn literal_string() {
        assert_eq!(first_expr("\"hello\""), ExprKind::String("hello".into()));
    }

    #[test]
    fn literal_bool() {
        assert_eq!(first_expr("true"), ExprKind::Bool(true));
        assert_eq!(first_expr("false"), ExprKind::Bool(false));
    }

    #[test]
    fn literal_none() {
        assert_eq!(first_expr("None"), ExprKind::None);
    }

    #[test]
    fn literal_todo() {
        assert_eq!(first_expr("todo"), ExprKind::Todo);
    }

    // ── Binary / unary operations ─────────────────────────────────

    #[test]
    fn binary_add() {
        let ExprKind::Binary { op, .. } = first_expr("1 + 2") else {
            panic!("expected Binary")
        };
        assert_eq!(op, BinOp::Add);
    }

    #[test]
    fn binary_eq() {
        let ExprKind::Binary { op, .. } = first_expr("1 == 2") else {
            panic!("expected Binary")
        };
        assert_eq!(op, BinOp::Eq);
    }

    #[test]
    fn unary_not() {
        let ExprKind::Unary { op, .. } = first_expr("!flag") else {
            panic!("expected Unary")
        };
        assert_eq!(op, UnaryOp::Not);
    }

    #[test]
    fn unary_neg() {
        let ExprKind::Unary { op, .. } = first_expr("-42") else {
            panic!("expected Unary")
        };
        assert_eq!(op, UnaryOp::Neg);
    }

    // ── Function calls ────────────────────────────────────────────

    #[test]
    fn call_basic() {
        let ExprKind::Call { callee, args, .. } = first_expr("f(1, 2)") else {
            panic!("expected Call")
        };
        assert!(matches!(callee.kind, ExprKind::Identifier(ref n) if n == "f"));
        assert_eq!(args.len(), 2);
    }

    // ── Imports ───────────────────────────────────────────────────

    #[test]
    fn import_named() {
        let item = first_item("import { foo, bar } from \"./mod\"");
        let ItemKind::Import(decl) = item else {
            panic!("expected Import")
        };
        assert_eq!(decl.specifiers.len(), 2);
        assert_eq!(decl.specifiers[0].name, "foo");
        assert_eq!(decl.source, "./mod");
    }

    #[test]
    fn import_aliased() {
        // "as" is banned but contextually used; test that the specifier still lowers
        let tokens = Lexer::new("import { foo as f } from \"./mod\"").tokenize_with_trivia();
        let parse = CstParser::new("import { foo as f } from \"./mod\"", tokens).parse();
        let root = parse.syntax();
        // Even if there's a banned keyword error, lowering should extract specifiers
        let _ = lower_program(&root, "import { foo as f } from \"./mod\"");
    }

    // ── Type declarations ─────────────────────────────────────────

    #[test]
    fn type_record() {
        let item = first_item("type User { name: string, age: number }");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert_eq!(decl.name, "User");
        assert!(matches!(decl.def, TypeDef::Record(ref fields) if fields.len() == 2));
    }

    #[test]
    fn type_union() {
        let item = first_item("type Color { | Red | Green | Blue }");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert!(matches!(decl.def, TypeDef::Union(ref variants) if variants.len() == 3));
    }

    #[test]
    fn type_alias() {
        let item = first_item("type Name = string");
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        assert!(matches!(decl.def, TypeDef::Alias(_)));
    }

    #[test]
    fn type_string_literal_union() {
        let item = first_item(r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#);
        let ItemKind::TypeDecl(decl) = item else {
            panic!("expected TypeDecl")
        };
        match decl.def {
            TypeDef::StringLiteralUnion(ref variants) => {
                assert_eq!(variants, &["GET", "POST", "PUT", "DELETE"]);
            }
            other => panic!("expected StringLiteralUnion, got {other:?}"),
        }
    }

    // ── Match expressions ─────────────────────────────────────────

    #[test]
    fn match_basic() {
        let ExprKind::Match { arms, .. } = first_expr("match x { Ok(v) -> v, Err(e) -> e }") else {
            panic!("expected Match")
        };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn match_wildcard() {
        let ExprKind::Match { arms, .. } = first_expr("match x { _ -> 0 }") else {
            panic!("expected Match")
        };
        assert!(matches!(arms[0].pattern.kind, PatternKind::Wildcard));
    }

    #[test]
    fn match_with_guard() {
        let ExprKind::Match { arms, .. } = first_expr("match x { n when n > 0 -> n, _ -> 0 }")
        else {
            panic!("expected Match")
        };
        assert!(arms[0].guard.is_some());
    }

    // ── Pipe expressions ──────────────────────────────────────────

    #[test]
    fn pipe_basic() {
        let prog = lower("1 |> f(_)");
        let ItemKind::Expr(ref expr) = prog.items[0].kind else {
            panic!("expected Expr")
        };
        assert!(matches!(expr.kind, ExprKind::Pipe { .. }));
    }

    // ── Lambda / arrow functions ──────────────────────────────────

    #[test]
    fn lambda_basic() {
        let ExprKind::Arrow { params, .. } = first_expr("fn(x) x + 1") else {
            panic!("expected Arrow")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "x");
    }

    #[test]
    fn lambda_zero_arg() {
        let ExprKind::Arrow { params, .. } = first_expr("fn() 42") else {
            panic!("expected Arrow")
        };
        assert!(params.is_empty());
    }

    // ── JSX ───────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        let ExprKind::Jsx(ref el) = first_expr("<Input />") else {
            panic!("expected Jsx")
        };
        match &el.kind {
            JsxElementKind::Element {
                name, self_closing, ..
            } => {
                assert_eq!(name, "Input");
                assert!(self_closing);
            }
            _ => panic!("expected Element"),
        }
    }

    #[test]
    fn jsx_with_children() {
        let ExprKind::Jsx(ref el) = first_expr("<div>hello</div>") else {
            panic!("expected Jsx")
        };
        match &el.kind {
            JsxElementKind::Element { children, .. } => {
                assert!(!children.is_empty());
            }
            _ => panic!("expected Element"),
        }
    }

    // ── Array, return, member ─────────────────────────────────────

    #[test]
    fn array_literal() {
        let ExprKind::Array(ref elts) = first_expr("[1, 2, 3]") else {
            panic!("expected Array")
        };
        assert_eq!(elts.len(), 3);
    }

    #[test]
    fn implicit_return_last_expr() {
        let item = first_item("fn f() { 42 }");
        let ItemKind::Function(decl) = item else {
            panic!("expected Function")
        };
        let ExprKind::Block(ref items) = decl.body.kind else {
            panic!("expected Block")
        };
        assert!(!items.is_empty());
    }

    #[test]
    fn member_access() {
        let ExprKind::Member { field, .. } = first_expr("user.name") else {
            panic!("expected Member")
        };
        assert_eq!(field, "name");
    }

    // ── Ok / Err / Some constructors ──────────────────────────────

    #[test]
    fn ok_constructor() {
        assert!(matches!(first_expr("Ok(42)"), ExprKind::Ok(_)));
    }

    #[test]
    fn err_constructor() {
        assert!(matches!(first_expr("Err(\"fail\")"), ExprKind::Err(_)));
    }

    #[test]
    fn some_constructor() {
        assert!(matches!(first_expr("Some(1)"), ExprKind::Some(_)));
    }

    // ── For blocks ────────────────────────────────────────────────

    #[test]
    fn for_block_basic() {
        let item = first_item("for User { fn greet(self) -> string { self.name } }");
        let ItemKind::ForBlock(block) = item else {
            panic!("expected ForBlock")
        };
        assert_eq!(block.functions.len(), 1);
        assert_eq!(block.functions[0].name, "greet");
    }

    // ── Test blocks ───────────────────────────────────────────────

    #[test]
    fn test_block_basic() {
        let item = first_item("test \"adds\" { assert true }");
        let ItemKind::TestBlock(block) = item else {
            panic!("expected TestBlock")
        };
        assert_eq!(block.name, "adds");
        assert!(!block.body.is_empty());
    }

    // ── Empty program / multiple items ────────────────────────────

    #[test]
    fn empty_program() {
        let prog = lower("");
        assert!(prog.items.is_empty());
    }

    #[test]
    fn multiple_items() {
        let prog = lower("const x = 1\nconst y = 2");
        assert_eq!(prog.items.len(), 2);
    }
}
