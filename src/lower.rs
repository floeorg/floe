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
                    let block = self.lower_for_block(&child)?;
                    return Some(Item {
                        kind: ItemKind::ForBlock(block),
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

        Some(ImportDecl {
            trusted: false,
            specifiers,
            source,
        })
    }

    fn lower_import_specifier(&mut self, node: &SyntaxNode) -> Option<ImportSpecifier> {
        let span = self.node_span(node);
        let idents = self.collect_idents(node);

        let name = idents.first()?.clone();
        let alias = idents.get(1).cloned();

        Some(ImportSpecifier {
            name,
            alias,
            trusted: false,
            span,
        })
    }

    fn lower_const(&mut self, node: &SyntaxNode, item_node: &SyntaxNode) -> Option<ConstDecl> {
        let exported = self.has_keyword(item_node, SyntaxKind::KW_EXPORT);

        let mut binding = None;
        let mut type_ann = None;

        // Determine binding type by looking at tokens
        let idents = self.collect_idents(node);
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);

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

    fn lower_for_block(&mut self, node: &SyntaxNode) -> Option<ForBlock> {
        let span = self.node_span(node);

        // Find the type expression (first TYPE_EXPR child)
        let mut type_name = None;
        let mut functions = Vec::new();

        let mut next_exported = false;
        for child_or_token in node.children_with_tokens() {
            match child_or_token {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::KW_EXPORT {
                        next_exported = true;
                    }
                }
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::TYPE_EXPR if type_name.is_none() => {
                        type_name = self.lower_type_expr(&child);
                    }
                    SyntaxKind::FUNCTION_DECL => {
                        if let Some(mut decl) = self.lower_for_block_function(&child) {
                            decl.exported = next_exported;
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
            functions,
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
                span,
            });
        }

        // Regular parameter
        self.lower_param(node)
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
                kind: TypeExprKind::Named { name, type_args },
                span,
            });
        }

        None
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
