pub mod ast;

use crate::lexer::Lexer;
use crate::lexer::span::Span;
use crate::lexer::token::{TemplatePart as LexTemplatePart, Token, TokenKind};
use ast::*;

/// A parse error with location and message.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

/// The ZenScript parser. Produces an AST from a token stream.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let tokens = Lexer::new(source).tokenize();
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn from_tokens(mut tokens: Vec<Token>) -> Self {
        // Ensure there's always an Eof token at the end
        if tokens.is_empty() || !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Eof)) {
            let span = tokens
                .last()
                .map(|t| Span::new(t.span.end, t.span.end, t.span.line, t.span.column))
                .unwrap_or(Span::new(0, 0, 1, 1));
            tokens.push(Token::new(TokenKind::Eof, span));
        }
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    /// Parse a complete program.
    pub fn parse_program(&mut self) -> Result<Program, Vec<ParseError>> {
        let start_span = self.current_span();
        let mut items = Vec::new();

        while !self.is_at_end() {
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        if self.errors.is_empty() {
            let end_span = self.previous_span();
            Ok(Program {
                items,
                span: self.merge_spans(start_span, end_span),
            })
        } else {
            Err(self.errors.clone())
        }
    }

    // ── Item Parsing ─────────────────────────────────────────────

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let start_span = self.current_span();

        // Handle export prefix
        let exported = self.check(&TokenKind::Export);
        if exported {
            self.advance();
        }

        let kind = match self.current_kind() {
            TokenKind::Import if !exported => {
                let decl = self.parse_import()?;
                ItemKind::Import(decl)
            }
            TokenKind::Import => {
                // export + import is not valid
                return Err(self.error("cannot export an import statement"));
            }
            TokenKind::Const => {
                let mut decl = self.parse_const_decl()?;
                decl.exported = exported;
                ItemKind::Const(decl)
            }
            TokenKind::Function => {
                let mut decl = self.parse_function_decl()?;
                decl.exported = exported;
                ItemKind::Function(decl)
            }
            TokenKind::Type | TokenKind::Opaque => {
                let mut decl = self.parse_type_decl()?;
                decl.exported = exported;
                ItemKind::TypeDecl(decl)
            }
            TokenKind::Async if self.peek_kind() == Some(&TokenKind::Function) => {
                let mut decl = self.parse_function_decl()?;
                decl.exported = exported;
                ItemKind::Function(decl)
            }
            _ if exported => {
                return Err(self.error("expected declaration after 'export'"));
            }
            _ => {
                let expr = self.parse_expr()?;
                ItemKind::Expr(expr)
            }
        };

        let end_span = self.previous_span();
        Ok(Item {
            kind,
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Import ───────────────────────────────────────────────────

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        self.expect(&TokenKind::Import)?;

        let specifiers = if self.check(&TokenKind::LeftBrace) {
            self.advance();
            let specs = self.parse_comma_separated(|p| p.parse_import_specifier())?;
            self.expect(&TokenKind::RightBrace)?;
            specs
        } else {
            // import "module" (bare import, no specifiers)
            Vec::new()
        };

        self.expect(&TokenKind::From)?;
        let source = self.expect_string()?;

        Ok(ImportDecl { specifiers, source })
    }

    fn parse_import_specifier(&mut self) -> Result<ImportSpecifier, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_identifier()?;

        // Check for `as alias`
        let alias = if self.check_identifier("as") {
            self.advance();
            Some(self.expect_identifier()?)
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(ImportSpecifier {
            name,
            alias,
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Const Declaration ────────────────────────────────────────

    fn parse_const_decl(&mut self) -> Result<ConstDecl, ParseError> {
        self.expect(&TokenKind::Const)?;

        let binding = if self.check(&TokenKind::LeftBracket) {
            // Array destructuring: `const [a, b] = ...`
            self.advance();
            let names = self.parse_comma_separated(|p| p.expect_identifier())?;
            self.expect(&TokenKind::RightBracket)?;
            ConstBinding::Array(names)
        } else if self.check(&TokenKind::LeftBrace) {
            // Object destructuring: `const { a, b } = ...`
            self.advance();
            let names = self.parse_comma_separated(|p| p.expect_identifier())?;
            self.expect(&TokenKind::RightBrace)?;
            ConstBinding::Object(names)
        } else {
            ConstBinding::Name(self.expect_identifier()?)
        };

        let type_ann = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            Option::None
        };

        self.expect(&TokenKind::Equal)?;
        let value = self.parse_expr()?;

        Ok(ConstDecl {
            exported: false,
            binding,
            type_ann,
            value,
        })
    }

    // ── Function Declaration ─────────────────────────────────────

    fn parse_function_decl(&mut self) -> Result<FunctionDecl, ParseError> {
        let async_fn = if self.check(&TokenKind::Async) {
            self.advance();
            true
        } else {
            false
        };

        self.expect(&TokenKind::Function)?;
        let name = self.expect_identifier()?;

        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_comma_separated(|p| p.parse_param())?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            Option::None
        };

        let body = self.parse_block_expr()?;

        Ok(FunctionDecl {
            exported: false,
            async_fn,
            name,
            params,
            return_type,
            body: Box::new(body),
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_identifier()?;

        let type_ann = if self.check(&TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            Option::None
        };

        let default = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(Param {
            name,
            type_ann,
            default,
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Type Declarations ────────────────────────────────────────

    fn parse_type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let opaque = if self.check(&TokenKind::Opaque) {
            self.advance();
            true
        } else {
            false
        };

        self.expect(&TokenKind::Type)?;
        let name = self.expect_identifier()?;

        // Optional type parameters: <T, U>
        let type_params = if self.check(&TokenKind::LessThan) {
            self.advance();
            let params = self.parse_comma_separated(|p| p.expect_identifier())?;
            self.expect(&TokenKind::GreaterThan)?;
            params
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::Equal)?;

        let def = self.parse_type_def()?;

        Ok(TypeDecl {
            exported: false,
            opaque,
            name,
            type_params,
            def,
        })
    }

    fn parse_type_def(&mut self) -> Result<TypeDef, ParseError> {
        // Check if this is a union type (starts with `|`)
        if self.check_pipe_in_union() {
            let variants = self.parse_union_variants()?;
            return Ok(TypeDef::Union(variants));
        }

        // Check if this is a record type (starts with `{`)
        if self.check(&TokenKind::LeftBrace) {
            let fields = self.parse_record_fields()?;
            return Ok(TypeDef::Record(fields));
        }

        // Otherwise it's a type alias
        let type_expr = self.parse_type_expr()?;
        Ok(TypeDef::Alias(type_expr))
    }

    fn parse_union_variants(&mut self) -> Result<Vec<Variant>, ParseError> {
        let mut variants = Vec::new();

        loop {
            // Expect `|` before each variant
            if !self.check_pipe_in_union() {
                break;
            }
            self.advance(); // consume `|`

            let start_span = self.current_span();
            let name = self.expect_identifier()?;

            let fields = if self.check(&TokenKind::LeftParen) {
                self.advance();
                let f = self.parse_comma_separated(|p| p.parse_variant_field())?;
                self.expect(&TokenKind::RightParen)?;
                f
            } else {
                Vec::new()
            };

            let end_span = self.previous_span();
            variants.push(Variant {
                name,
                fields,
                span: self.merge_spans(start_span, end_span),
            });
        }

        if variants.is_empty() {
            return Err(self.error("expected at least one variant in union type"));
        }

        Ok(variants)
    }

    fn parse_variant_field(&mut self) -> Result<VariantField, ParseError> {
        let start_span = self.current_span();

        // Check if this is a named field: `name: Type`
        if self.is_identifier() && self.peek_kind() == Some(&TokenKind::Colon) {
            let name = self.expect_identifier()?;
            self.advance(); // consume ':'
            let type_ann = self.parse_type_expr()?;
            let end_span = self.previous_span();
            return Ok(VariantField {
                name: Some(name),
                type_ann,
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Positional field: just a type
        let type_ann = self.parse_type_expr()?;
        let end_span = self.previous_span();
        Ok(VariantField {
            name: Option::None,
            type_ann,
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_record_fields(&mut self) -> Result<Vec<RecordField>, ParseError> {
        self.expect(&TokenKind::LeftBrace)?;
        let fields = self.parse_comma_separated(|p| p.parse_record_field())?;
        self.expect(&TokenKind::RightBrace)?;
        Ok(fields)
    }

    fn parse_record_field(&mut self) -> Result<RecordField, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        let type_ann = self.parse_type_expr()?;

        let default = if self.check(&TokenKind::Equal) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(RecordField {
            name,
            type_ann,
            default,
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Type Expressions ─────────────────────────────────────────

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let start_span = self.current_span();

        // Function type: `(params) => ReturnType`
        if self.check(&TokenKind::LeftParen) && self.is_function_type() {
            return self.parse_function_type();
        }

        // Array type sugar: `[T]` for `Array<T>` — skip, we use `Array<T>` syntax
        // Tuple: `[T, U]`
        if self.check(&TokenKind::LeftBracket) {
            self.advance();
            let types = self.parse_comma_separated(|p| p.parse_type_expr())?;
            self.expect(&TokenKind::RightBracket)?;
            let end_span = self.previous_span();
            return Ok(TypeExpr {
                kind: TypeExprKind::Tuple(types),
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Record type: `{ ... }`
        if self.check(&TokenKind::LeftBrace) {
            let fields = self.parse_record_fields()?;
            let end_span = self.previous_span();
            return Ok(TypeExpr {
                kind: TypeExprKind::Record(fields),
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Named type: `string`, `Option<T>`, `Result<T, E>`
        let name = self.expect_identifier()?;

        let type_args = if self.check(&TokenKind::LessThan) {
            self.advance();
            let args = self.parse_comma_separated(|p| p.parse_type_expr())?;
            self.expect(&TokenKind::GreaterThan)?;
            args
        } else {
            Vec::new()
        };

        let end_span = self.previous_span();
        Ok(TypeExpr {
            kind: TypeExprKind::Named { name, type_args },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_function_type(&mut self) -> Result<TypeExpr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_comma_separated(|p| p.parse_type_expr())?;
        self.expect(&TokenKind::RightParen)?;
        self.expect(&TokenKind::FatArrow)?;
        let return_type = self.parse_type_expr()?;
        let end_span = self.previous_span();
        Ok(TypeExpr {
            kind: TypeExprKind::Function {
                params,
                return_type: Box::new(return_type),
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    /// Heuristic: is the current `(` the start of a function type?
    /// Look ahead for `) =>`.
    fn is_function_type(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if followed by `=>`
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::FatArrow;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    // ── Expression Parsing (Pratt parser) ────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipe_expr()
    }

    /// Pipe: lowest precedence binary operator.
    /// `expr |> expr |> expr`
    fn parse_pipe_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_or_expr()?;

        while self.check(&TokenKind::Pipe) {
            self.advance();
            let right = self.parse_or_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Pipe {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Logical OR: `a || b`
    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;

        while self.check(&TokenKind::PipePipe) {
            self.advance();
            let right = self.parse_and_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: BinOp::Or,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Logical AND: `a && b`
    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality_expr()?;

        while self.check(&TokenKind::AmpAmp) {
            self.advance();
            let right = self.parse_equality_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op: BinOp::And,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Equality: `a == b`, `a != b`
    fn parse_equality_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::EqualEqual => BinOp::Eq,
                TokenKind::BangEqual => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Comparison: `a < b`, `a > b`, `a <= b`, `a >= b`
    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_additive_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::LessThan => BinOp::Lt,
                TokenKind::GreaterThan => BinOp::Gt,
                TokenKind::LessEqual => BinOp::LtEq,
                TokenKind::GreaterEqual => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Additive: `a + b`, `a - b`
    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplicative_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Multiplicative: `a * b`, `a / b`, `a % b`
    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary_expr()?;

        loop {
            let op = match self.current_kind() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr()?;
            let span = self.merge_spans(left.span, right.span);
            left = Expr {
                kind: ExprKind::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }

        Ok(left)
    }

    /// Unary: `!a`, `-a`
    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            TokenKind::Bang => {
                self.advance();
                let operand = self.parse_unary_expr()?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span: self.merge_spans(start_span, end_span),
                })
            }
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary_expr()?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span: self.merge_spans(start_span, end_span),
                })
            }
            TokenKind::Await => {
                self.advance();
                let operand = self.parse_unary_expr()?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    span: self.merge_spans(start_span, end_span),
                })
            }
            _ => self.parse_postfix_expr(),
        }
    }

    /// Postfix: `expr?`, `expr.field`, `expr[index]`, `expr(args)`
    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            match self.current_kind() {
                // Unwrap: `expr?`
                TokenKind::Question => {
                    self.advance();
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Unwrap(Box::new(expr)),
                        span,
                    };
                }
                // Member access: `expr.field`
                TokenKind::Dot => {
                    self.advance();
                    let field = self.expect_identifier()?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Member {
                            object: Box::new(expr),
                            field,
                        },
                        span,
                    };
                }
                // Index: `expr[index]`
                TokenKind::LeftBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&TokenKind::RightBracket)?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Index {
                            object: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    };
                }
                // Call: `expr(args)` — but only for lowercase identifiers or member exprs
                // Uppercase identifiers with `(` are constructors, handled in primary
                TokenKind::LeftParen => {
                    // If the callee is an uppercase identifier, it was already
                    // parsed as a Construct in primary. Only parse calls for
                    // lowercase identifiers, members, etc.
                    if matches!(&expr.kind, ExprKind::Identifier(name) if name.starts_with(char::is_uppercase))
                    {
                        break;
                    }
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(&TokenKind::RightParen)?;
                    let span = self.merge_spans(expr.span, self.previous_span());
                    expr = Expr {
                        kind: ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                        span,
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Primary expressions: literals, identifiers, constructors, match, etc.
    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            // Number literal
            TokenKind::Number(n) => {
                let n = n.clone();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Number(n),
                    span: start_span,
                })
            }

            // String literal
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::String(s),
                    span: start_span,
                })
            }

            // Template literal
            TokenKind::TemplateLiteral(parts) => {
                let ast_parts = self.convert_template_parts(parts.clone())?;
                self.advance();
                Ok(Expr {
                    kind: ExprKind::TemplateLiteral(ast_parts),
                    span: start_span,
                })
            }

            // Boolean literal
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(b),
                    span: start_span,
                })
            }

            // Placeholder
            TokenKind::Underscore => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Placeholder,
                    span: start_span,
                })
            }

            // None
            TokenKind::None => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::None,
                    span: start_span,
                })
            }

            // Ok(expr)
            TokenKind::Ok => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Ok(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Err(expr)
            TokenKind::Err => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Err(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Some(expr)
            TokenKind::Some => {
                self.advance();
                self.expect(&TokenKind::LeftParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Some(Box::new(inner)),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Match expression
            TokenKind::Match => self.parse_match_expr(),

            // Return
            TokenKind::Return => {
                self.advance();
                let value = if self.is_at_end()
                    || self.check(&TokenKind::RightBrace)
                    || self.check(&TokenKind::Semicolon)
                {
                    Option::None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Return(value),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // If expression
            TokenKind::If => self.parse_if_expr(),

            // Block expression: `{ ... }`
            TokenKind::LeftBrace => self.parse_block_expr(),

            // Array literal: `[1, 2, 3]`
            TokenKind::LeftBracket => {
                self.advance();
                let elements = self.parse_comma_separated(|p| p.parse_expr())?;
                self.expect(&TokenKind::RightBracket)?;
                let end_span = self.previous_span();
                Ok(Expr {
                    kind: ExprKind::Array(elements),
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Parenthesized expression or arrow function
            TokenKind::LeftParen => {
                if self.is_arrow_function() {
                    self.parse_arrow_function()
                } else {
                    self.advance();
                    let inner = self.parse_expr()?;
                    self.expect(&TokenKind::RightParen)?;
                    let end_span = self.previous_span();
                    Ok(Expr {
                        kind: ExprKind::Grouped(Box::new(inner)),
                        span: self.merge_spans(start_span, end_span),
                    })
                }
            }

            // JSX: `<Component ...>` or `<>`
            TokenKind::LessThan => self.parse_jsx_expr(),

            // Identifier — could be a constructor (uppercase) or variable (lowercase)
            TokenKind::Identifier(name) => {
                let name = name.clone();

                // Uppercase identifier followed by `(` is a constructor
                if name.starts_with(char::is_uppercase)
                    && self.peek_kind() == Some(&TokenKind::LeftParen)
                {
                    return self.parse_construct_expr();
                }

                // Single-arg arrow function: `x => expr`
                if self.peek_kind() == Some(&TokenKind::FatArrow) {
                    return self.parse_arrow_function_single_arg();
                }

                self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier(name),
                    span: start_span,
                })
            }

            // Banned keyword — report it
            TokenKind::Banned(banned) => {
                let msg = format!(
                    "banned keyword '{}': {}",
                    banned.as_str(),
                    banned.help_message()
                );
                Err(ParseError {
                    message: msg,
                    span: start_span,
                })
            }

            _ => Err(self.error(&format!("unexpected token: {:?}", self.current_kind()))),
        }
    }

    // ── Constructors ─────────────────────────────────────────────

    fn parse_construct_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        let type_name = self.expect_identifier()?;
        self.expect(&TokenKind::LeftParen)?;

        // Check for spread: `..expr`
        let spread = if self.check(&TokenKind::DotDot) {
            self.advance();
            let spread_expr = self.parse_expr()?;
            // Expect comma after spread if there are more args
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
            Some(Box::new(spread_expr))
        } else {
            Option::None
        };

        let args = if !self.check(&TokenKind::RightParen) {
            self.parse_call_args()?
        } else {
            Vec::new()
        };

        self.expect(&TokenKind::RightParen)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Construct {
                type_name,
                spread,
                args,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Call Arguments ───────────────────────────────────────────

    fn parse_call_args(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.parse_comma_separated(|p| p.parse_call_arg())
    }

    fn parse_call_arg(&mut self) -> Result<Arg, ParseError> {
        // Check for named argument: `label: expr`
        if self.is_identifier() && self.peek_kind() == Some(&TokenKind::Colon) {
            let label = self.expect_identifier()?;
            self.advance(); // consume ':'
            let value = self.parse_expr()?;
            return Ok(Arg::Named { label, value });
        }

        let expr = self.parse_expr()?;
        Ok(Arg::Positional(expr))
    }

    // ── Arrow Functions ──────────────────────────────────────────

    fn parse_arrow_function(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_comma_separated(|p| p.parse_param())?;
        self.expect(&TokenKind::RightParen)?;
        self.expect(&TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Arrow {
                params,
                body: Box::new(body),
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_arrow_function_single_arg(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_identifier()?;
        self.expect(&TokenKind::FatArrow)?;
        let body = self.parse_expr()?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Arrow {
                params: vec![Param {
                    name,
                    type_ann: Option::None,
                    default: Option::None,
                    span: start_span,
                }],
                body: Box::new(body),
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    /// Heuristic: is the current `(` the start of an arrow function?
    /// Look ahead for `) =>`.
    fn is_arrow_function(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::FatArrow;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    // ── Match Expression ─────────────────────────────────────────

    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::Match)?;
        let subject = self.parse_expr()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut arms = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let arm = self.parse_match_arm()?;
            arms.push(arm);
            // Optional comma between arms
            if self.check(&TokenKind::Comma) {
                self.advance();
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Match {
                subject: Box::new(subject),
                arms,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start_span = self.current_span();
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::ThinArrow)?;
        let body = self.parse_expr()?;
        let end_span = self.previous_span();

        Ok(MatchArm {
            pattern,
            body,
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let start_span = self.current_span();

        match self.current_kind() {
            // Wildcard: `_`
            TokenKind::Underscore => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Wildcard,
                    span: start_span,
                })
            }

            // Boolean literal pattern
            TokenKind::Bool(b) => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Literal(LiteralPattern::Bool(b)),
                    span: start_span,
                })
            }

            // String literal pattern
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Literal(LiteralPattern::String(s)),
                    span: start_span,
                })
            }

            // Number literal pattern (possibly a range)
            TokenKind::Number(n) => {
                let n = n.clone();
                self.advance();

                // Check for range: `1..10`
                if self.check(&TokenKind::DotDot) {
                    self.advance();
                    let end_num = match self.current_kind() {
                        TokenKind::Number(e) => {
                            let e = e.clone();
                            self.advance();
                            e
                        }
                        _ => return Err(self.error("expected number after '..' in range pattern")),
                    };
                    let end_span = self.previous_span();
                    return Ok(Pattern {
                        kind: PatternKind::Range {
                            start: LiteralPattern::Number(n),
                            end: LiteralPattern::Number(end_num),
                        },
                        span: self.merge_spans(start_span, end_span),
                    });
                }

                Ok(Pattern {
                    kind: PatternKind::Literal(LiteralPattern::Number(n)),
                    span: start_span,
                })
            }

            // Record pattern: `{ x, y }` or `{ ctrl: true }`
            TokenKind::LeftBrace => {
                self.advance();
                let fields = self.parse_comma_separated(|p| p.parse_record_pattern_field())?;
                self.expect(&TokenKind::RightBrace)?;
                let end_span = self.previous_span();
                Ok(Pattern {
                    kind: PatternKind::Record { fields },
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // None pattern
            TokenKind::None => {
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Variant {
                        name: "None".to_string(),
                        fields: Vec::new(),
                    },
                    span: start_span,
                })
            }

            // Ok/Err/Some variant patterns
            TokenKind::Ok | TokenKind::Err | TokenKind::Some => {
                let name = match self.current_kind() {
                    TokenKind::Ok => "Ok".to_string(),
                    TokenKind::Err => "Err".to_string(),
                    TokenKind::Some => "Some".to_string(),
                    _ => unreachable!(),
                };
                self.advance();

                let fields = if self.check(&TokenKind::LeftParen) {
                    self.advance();
                    let f = self.parse_comma_separated(|p| p.parse_pattern())?;
                    self.expect(&TokenKind::RightParen)?;
                    f
                } else {
                    Vec::new()
                };

                let end_span = self.previous_span();
                Ok(Pattern {
                    kind: PatternKind::Variant { name, fields },
                    span: self.merge_spans(start_span, end_span),
                })
            }

            // Identifier — could be a variant pattern or a binding
            TokenKind::Identifier(name) => {
                let name = name.clone();

                // Uppercase + `(` → variant pattern: `Click(el, { x, y })`
                if name.starts_with(char::is_uppercase) {
                    self.advance();
                    let fields = if self.check(&TokenKind::LeftParen) {
                        self.advance();
                        let f = self.parse_comma_separated(|p| p.parse_pattern())?;
                        self.expect(&TokenKind::RightParen)?;
                        f
                    } else {
                        Vec::new()
                    };
                    let end_span = self.previous_span();
                    return Ok(Pattern {
                        kind: PatternKind::Variant { name, fields },
                        span: self.merge_spans(start_span, end_span),
                    });
                }

                // Lowercase → binding: `x`, `msg`
                self.advance();
                Ok(Pattern {
                    kind: PatternKind::Binding(name),
                    span: start_span,
                })
            }

            _ => Err(self.error(&format!(
                "unexpected token in pattern: {:?}",
                self.current_kind()
            ))),
        }
    }

    fn parse_record_pattern_field(&mut self) -> Result<(String, Pattern), ParseError> {
        let name = self.expect_identifier()?;

        // `field: pattern` or just `field` (shorthand for `field: field`)
        let pattern = if self.check(&TokenKind::Colon) {
            self.advance();
            self.parse_pattern()?
        } else {
            // Shorthand: `{ x }` means `{ x: x }`
            Pattern {
                kind: PatternKind::Binding(name.clone()),
                span: self.previous_span(),
            }
        };

        Ok((name, pattern))
    }

    // ── If Expression ────────────────────────────────────────────

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::If)?;
        let condition = self.parse_expr()?;
        let then_branch = self.parse_block_expr()?;

        let else_branch = if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                Some(Box::new(self.parse_if_expr()?))
            } else {
                Some(Box::new(self.parse_block_expr()?))
            }
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(Expr {
            kind: ExprKind::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Block Expression ─────────────────────────────────────────

    fn parse_block_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LeftBrace)?;

        let mut items = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let item = self.parse_item()?;
            items.push(item);
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(Expr {
            kind: ExprKind::Block(items),
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── JSX ──────────────────────────────────────────────────────

    fn parse_jsx_expr(&mut self) -> Result<Expr, ParseError> {
        let start_span = self.current_span();
        let element = self.parse_jsx_element()?;
        let span = self.merge_spans(start_span, element.span);
        Ok(Expr {
            kind: ExprKind::Jsx(element),
            span,
        })
    }

    fn parse_jsx_element(&mut self) -> Result<JsxElement, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::LessThan)?;

        // Fragment: `<>`
        if self.check(&TokenKind::GreaterThan) {
            self.advance();
            let children = self.parse_jsx_children(Option::None)?;
            // Expect `</>`
            self.expect(&TokenKind::LessThan)?;
            self.expect(&TokenKind::Slash)?;
            self.expect(&TokenKind::GreaterThan)?;
            let end_span = self.previous_span();
            return Ok(JsxElement {
                kind: JsxElementKind::Fragment { children },
                span: self.merge_spans(start_span, end_span),
            });
        }

        let name = self.expect_identifier()?;

        // Parse props
        let mut props = Vec::new();
        while !self.check(&TokenKind::GreaterThan)
            && !self.check(&TokenKind::Slash)
            && !self.is_at_end()
        {
            let prop = self.parse_jsx_prop()?;
            props.push(prop);
        }

        // Self-closing: `<Tag ... />`
        if self.check(&TokenKind::Slash) {
            self.advance();
            self.expect(&TokenKind::GreaterThan)?;
            let end_span = self.previous_span();
            return Ok(JsxElement {
                kind: JsxElementKind::Element {
                    name,
                    props,
                    children: Vec::new(),
                    self_closing: true,
                },
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Opening tag close
        self.expect(&TokenKind::GreaterThan)?;

        // Children
        let children = self.parse_jsx_children(Some(&name))?;

        // Closing tag: `</Tag>`
        self.expect(&TokenKind::LessThan)?;
        self.expect(&TokenKind::Slash)?;
        let closing_name = self.expect_identifier()?;
        if closing_name != name {
            return Err(self.error(&format!(
                "mismatched closing tag: expected </{name}>, found </{closing_name}>"
            )));
        }
        self.expect(&TokenKind::GreaterThan)?;
        let end_span = self.previous_span();

        Ok(JsxElement {
            kind: JsxElementKind::Element {
                name,
                props,
                children,
                self_closing: false,
            },
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_jsx_prop(&mut self) -> Result<JsxProp, ParseError> {
        let start_span = self.current_span();
        let name = self.expect_identifier()?;

        let value = if self.check(&TokenKind::Equal) {
            self.advance();
            if self.check(&TokenKind::LeftBrace) {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightBrace)?;
                Some(expr)
            } else if matches!(self.current_kind(), TokenKind::String(_)) {
                Some(self.parse_primary_expr()?)
            } else {
                return Err(self.error("expected '{' or string after '=' in JSX prop"));
            }
        } else {
            Option::None
        };

        let end_span = self.previous_span();
        Ok(JsxProp {
            name,
            value,
            span: self.merge_spans(start_span, end_span),
        })
    }

    fn parse_jsx_children(
        &mut self,
        _parent_name: Option<&str>,
    ) -> Result<Vec<JsxChild>, ParseError> {
        let mut children = Vec::new();

        loop {
            // Check for closing tag `</`
            if self.check(&TokenKind::LessThan) && self.peek_kind() == Some(&TokenKind::Slash) {
                break;
            }

            if self.is_at_end() {
                break;
            }

            match self.current_kind() {
                // Expression child: `{expr}`
                TokenKind::LeftBrace => {
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect(&TokenKind::RightBrace)?;
                    children.push(JsxChild::Expr(expr));
                }
                // Nested JSX element
                TokenKind::LessThan => {
                    let element = self.parse_jsx_element()?;
                    children.push(JsxChild::Element(element));
                }
                // Text content — collect text-like tokens
                _ => {
                    if let Some(text) = self.token_as_jsx_text() {
                        let mut text_buf = text;
                        self.advance();
                        while !self.is_at_end()
                            && !self.check(&TokenKind::LeftBrace)
                            && !self.check(&TokenKind::LessThan)
                        {
                            if let Some(t) = self.token_as_jsx_text() {
                                text_buf.push(' ');
                                text_buf.push_str(&t);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        children.push(JsxChild::Text(text_buf));
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(children)
    }

    // ── Template literal conversion ──────────────────────────────

    fn convert_template_parts(
        &self,
        parts: Vec<LexTemplatePart>,
    ) -> Result<Vec<TemplatePart>, ParseError> {
        let mut result = Vec::new();
        for part in parts {
            match part {
                LexTemplatePart::Raw(s) => {
                    result.push(TemplatePart::Raw(s));
                }
                LexTemplatePart::Interpolation(tokens) => {
                    let mut sub_parser = Parser::from_tokens(tokens);
                    let expr = sub_parser.parse_expr()?;
                    result.push(TemplatePart::Expr(expr));
                }
            }
        }
        Ok(result)
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn current_kind(&self) -> TokenKind {
        self.tokens[self.pos].kind.clone()
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn previous_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.tokens[0].span
        }
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos + 1).map(|t| &t.kind)
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.tokens[self.pos].kind) == std::mem::discriminant(kind)
    }

    fn check_identifier(&self, name: &str) -> bool {
        matches!(&self.tokens[self.pos].kind, TokenKind::Identifier(n) if n == name)
    }

    fn is_identifier(&self) -> bool {
        matches!(self.tokens[self.pos].kind, TokenKind::Identifier(_))
    }

    /// Try to interpret the current token as JSX text content.
    fn token_as_jsx_text(&self) -> Option<String> {
        match &self.tokens[self.pos].kind {
            TokenKind::Identifier(s) => Some(s.clone()),
            TokenKind::Number(s) => Some(s.clone()),
            TokenKind::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Check if the current token is `|` used in union type declarations.
    /// The lexer emits bare `|` as `Identifier("|")`.
    fn check_pipe_in_union(&self) -> bool {
        self.check_identifier("|")
    }

    fn is_at_end(&self) -> bool {
        self.tokens[self.pos].kind == TokenKind::Eof
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<&Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!(
                "expected {:?}, found {:?}",
                kind,
                self.current_kind()
            )))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match self.current_kind() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error(&format!(
                "expected identifier, found {:?}",
                self.current_kind()
            ))),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.current_kind() {
            TokenKind::String(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.error(&format!("expected string, found {:?}", self.current_kind()))),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            span: self.current_span(),
        }
    }

    fn merge_spans(&self, start: Span, end: Span) -> Span {
        Span::new(start.start, end.end, start.line, start.column)
    }

    /// Error recovery: skip tokens until we find a likely statement boundary.
    fn synchronize(&mut self) {
        while !self.is_at_end() {
            // Stop at statement boundaries
            match self.current_kind() {
                TokenKind::Const
                | TokenKind::Function
                | TokenKind::Export
                | TokenKind::Import
                | TokenKind::Type
                | TokenKind::Opaque
                | TokenKind::Return => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    /// Parse a comma-separated list, allowing trailing comma.
    fn parse_comma_separated<T, F>(&mut self, mut parse_fn: F) -> Result<Vec<T>, ParseError>
    where
        F: FnMut(&mut Self) -> Result<T, ParseError>,
    {
        let mut items = Vec::new();

        // Check if list is empty (next token would be a closing delimiter)
        if self.check(&TokenKind::RightParen)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::RightBracket)
            || self.check(&TokenKind::GreaterThan)
        {
            return Ok(items);
        }

        items.push(parse_fn(self)?);

        while self.check(&TokenKind::Comma) {
            self.advance();
            // Allow trailing comma
            if self.check(&TokenKind::RightParen)
                || self.check(&TokenKind::RightBrace)
                || self.check(&TokenKind::RightBracket)
                || self.check(&TokenKind::GreaterThan)
            {
                break;
            }
            items.push(parse_fn(self)?);
        }

        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<Program, Vec<ParseError>> {
        Parser::new(input).parse_program()
    }

    fn parse_ok(input: &str) -> Program {
        parse(input).unwrap_or_else(|errs| {
            panic!(
                "parse failed:\n{}",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
    }

    fn first_item(input: &str) -> ItemKind {
        parse_ok(input).items.into_iter().next().unwrap().kind
    }

    fn first_expr(input: &str) -> ExprKind {
        match first_item(input) {
            ItemKind::Expr(e) => e.kind,
            other => panic!("expected expression item, got {other:?}"),
        }
    }

    // ── Literals ─────────────────────────────────────────────────

    #[test]
    fn number_literal() {
        assert_eq!(first_expr("42"), ExprKind::Number("42".to_string()));
    }

    #[test]
    fn string_literal() {
        assert_eq!(
            first_expr(r#""hello""#),
            ExprKind::String("hello".to_string())
        );
    }

    #[test]
    fn bool_literal() {
        assert_eq!(first_expr("true"), ExprKind::Bool(true));
        assert_eq!(first_expr("false"), ExprKind::Bool(false));
    }

    #[test]
    fn none_literal() {
        assert_eq!(first_expr("None"), ExprKind::None);
    }

    #[test]
    fn placeholder() {
        assert_eq!(first_expr("_"), ExprKind::Placeholder);
    }

    // ── Identifiers ──────────────────────────────────────────────

    #[test]
    fn identifier() {
        assert_eq!(
            first_expr("myVar"),
            ExprKind::Identifier("myVar".to_string())
        );
    }

    // ── Binary Operators ─────────────────────────────────────────

    #[test]
    fn binary_add() {
        let expr = first_expr("1 + 2");
        assert!(matches!(expr, ExprKind::Binary { op: BinOp::Add, .. }));
    }

    #[test]
    fn binary_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let expr = first_expr("1 + 2 * 3");
        match expr {
            ExprKind::Binary {
                op: BinOp::Add,
                right,
                ..
            } => {
                assert!(matches!(
                    right.kind,
                    ExprKind::Binary { op: BinOp::Mul, .. }
                ));
            }
            _ => panic!("expected binary add"),
        }
    }

    #[test]
    fn comparison() {
        let expr = first_expr("a == b");
        assert!(matches!(expr, ExprKind::Binary { op: BinOp::Eq, .. }));
    }

    #[test]
    fn logical_and_or() {
        // a || b && c should parse as a || (b && c)
        let expr = first_expr("a || b && c");
        match expr {
            ExprKind::Binary {
                op: BinOp::Or,
                right,
                ..
            } => {
                assert!(matches!(
                    right.kind,
                    ExprKind::Binary { op: BinOp::And, .. }
                ));
            }
            _ => panic!("expected binary or"),
        }
    }

    // ── Unary Operators ──────────────────────────────────────────

    #[test]
    fn unary_not() {
        let expr = first_expr("!x");
        assert!(matches!(
            expr,
            ExprKind::Unary {
                op: UnaryOp::Not,
                ..
            }
        ));
    }

    #[test]
    fn unary_neg() {
        let expr = first_expr("-42");
        assert!(matches!(
            expr,
            ExprKind::Unary {
                op: UnaryOp::Neg,
                ..
            }
        ));
    }

    // ── Pipe Operator ────────────────────────────────────────────

    #[test]
    fn pipe_simple() {
        let expr = first_expr("x |> f(y)");
        assert!(matches!(expr, ExprKind::Pipe { .. }));
    }

    #[test]
    fn pipe_chained() {
        let expr = first_expr("x |> f |> g");
        match expr {
            ExprKind::Pipe { left, .. } => {
                assert!(matches!(left.kind, ExprKind::Pipe { .. }));
            }
            _ => panic!("expected chained pipe"),
        }
    }

    // ── Unwrap ───────────────────────────────────────────────────

    #[test]
    fn unwrap_operator() {
        let expr = first_expr("fetchUser(id)?");
        assert!(matches!(expr, ExprKind::Unwrap(_)));
    }

    // ── Function Calls ───────────────────────────────────────────

    #[test]
    fn function_call() {
        let expr = first_expr("f(1, 2)");
        match expr {
            ExprKind::Call { args, .. } => {
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected call"),
        }
    }

    #[test]
    fn named_args() {
        let expr = first_expr("f(name: x, limit: 10)");
        match expr {
            ExprKind::Call { args, .. } => {
                assert!(matches!(&args[0], Arg::Named { label, .. } if label == "name"));
                assert!(matches!(&args[1], Arg::Named { label, .. } if label == "limit"));
            }
            _ => panic!("expected call"),
        }
    }

    // ── Constructors ─────────────────────────────────────────────

    #[test]
    fn constructor() {
        let expr = first_expr(r#"User(name: "Ryan", email: e)"#);
        match expr {
            ExprKind::Construct {
                type_name, args, ..
            } => {
                assert_eq!(type_name, "User");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("expected construct"),
        }
    }

    #[test]
    fn constructor_with_spread() {
        let expr = first_expr(r#"User(..user, name: "New")"#);
        match expr {
            ExprKind::Construct { spread, args, .. } => {
                assert!(spread.is_some());
                assert_eq!(args.len(), 1);
            }
            _ => panic!("expected construct"),
        }
    }

    // ── Result/Option Constructors ───────────────────────────────

    #[test]
    fn ok_constructor() {
        let expr = first_expr("Ok(42)");
        assert!(matches!(expr, ExprKind::Ok(_)));
    }

    #[test]
    fn err_constructor() {
        let expr = first_expr(r#"Err("not found")"#);
        assert!(matches!(expr, ExprKind::Err(_)));
    }

    #[test]
    fn some_constructor() {
        let expr = first_expr("Some(x)");
        assert!(matches!(expr, ExprKind::Some(_)));
    }

    // ── Arrow Functions ──────────────────────────────────────────

    #[test]
    fn arrow_function_multi_arg() {
        let expr = first_expr("(a, b) => a + b");
        match expr {
            ExprKind::Arrow { params, .. } => {
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, "a");
                assert_eq!(params[1].name, "b");
            }
            _ => panic!("expected arrow"),
        }
    }

    #[test]
    fn arrow_function_single_arg() {
        let expr = first_expr("x => x + 1");
        match expr {
            ExprKind::Arrow { params, .. } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "x");
            }
            _ => panic!("expected arrow"),
        }
    }

    #[test]
    fn arrow_function_typed() {
        let expr = first_expr("(x: number) => x + 1");
        match expr {
            ExprKind::Arrow { params, .. } => {
                assert!(params[0].type_ann.is_some());
            }
            _ => panic!("expected arrow"),
        }
    }

    // ── Match Expressions ────────────────────────────────────────

    #[test]
    fn match_simple() {
        let expr = first_expr("match x { Ok(v) -> v, Err(e) -> e }");
        match expr {
            ExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
            }
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn match_wildcard() {
        let expr = first_expr("match x { _ -> 0 }");
        match expr {
            ExprKind::Match { arms, .. } => {
                assert!(matches!(arms[0].pattern.kind, PatternKind::Wildcard));
            }
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn match_nested_variant() {
        let expr = first_expr("match err { Network(Timeout(ms)) -> ms, _ -> 0 }");
        match expr {
            ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
                PatternKind::Variant { name, fields } => {
                    assert_eq!(name, "Network");
                    assert_eq!(fields.len(), 1);
                    assert!(
                        matches!(&fields[0].kind, PatternKind::Variant { name, .. } if name == "Timeout")
                    );
                }
                _ => panic!("expected variant pattern"),
            },
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn match_range() {
        let expr = first_expr("match n { 1..10 -> true, _ -> false }");
        match expr {
            ExprKind::Match { arms, .. } => {
                assert!(matches!(arms[0].pattern.kind, PatternKind::Range { .. }));
            }
            _ => panic!("expected match"),
        }
    }

    #[test]
    fn match_record_destructure() {
        let expr = first_expr(r#"match action { Click(el, { x, y }) -> handle(el, x, y) }"#);
        match expr {
            ExprKind::Match { arms, .. } => match &arms[0].pattern.kind {
                PatternKind::Variant { fields, .. } => {
                    assert_eq!(fields.len(), 2);
                    assert!(matches!(&fields[1].kind, PatternKind::Record { .. }));
                }
                _ => panic!("expected variant"),
            },
            _ => panic!("expected match"),
        }
    }

    // ── Const Declaration ────────────────────────────────────────

    #[test]
    fn const_decl() {
        match first_item("const x = 42") {
            ItemKind::Const(decl) => {
                assert_eq!(decl.binding, ConstBinding::Name("x".to_string()));
                assert!(!decl.exported);
            }
            other => panic!("expected const, got {other:?}"),
        }
    }

    #[test]
    fn const_decl_typed() {
        match first_item("const x: number = 42") {
            ItemKind::Const(decl) => {
                assert!(decl.type_ann.is_some());
            }
            other => panic!("expected const, got {other:?}"),
        }
    }

    #[test]
    fn export_const() {
        match first_item("export const x = 42") {
            ItemKind::Const(decl) => {
                assert!(decl.exported);
            }
            other => panic!("expected const, got {other:?}"),
        }
    }

    // ── Function Declaration ─────────────────────────────────────

    #[test]
    fn function_decl() {
        match first_item("function add(a: number, b: number): number { a + b }") {
            ItemKind::Function(decl) => {
                assert_eq!(decl.name, "add");
                assert_eq!(decl.params.len(), 2);
                assert!(decl.return_type.is_some());
                assert!(!decl.async_fn);
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn async_function() {
        match first_item(
            "async function fetchUser(id: string): Result<User, ApiError> { Ok(user) }",
        ) {
            ItemKind::Function(decl) => {
                assert!(decl.async_fn);
                assert_eq!(decl.name, "fetchUser");
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    #[test]
    fn function_with_defaults() {
        match first_item("function f(x: number = 10) { x }") {
            ItemKind::Function(decl) => {
                assert!(decl.params[0].default.is_some());
            }
            other => panic!("expected function, got {other:?}"),
        }
    }

    // ── Import ───────────────────────────────────────────────────

    #[test]
    fn import_named() {
        match first_item(r#"import { useState, useEffect } from "react""#) {
            ItemKind::Import(decl) => {
                assert_eq!(decl.specifiers.len(), 2);
                assert_eq!(decl.specifiers[0].name, "useState");
                assert_eq!(decl.specifiers[1].name, "useEffect");
                assert_eq!(decl.source, "react");
            }
            other => panic!("expected import, got {other:?}"),
        }
    }

    // ── Type Declarations ────────────────────────────────────────

    #[test]
    fn type_alias() {
        match first_item("type UserId = Brand<string, UserId>") {
            ItemKind::TypeDecl(decl) => {
                assert_eq!(decl.name, "UserId");
                assert!(matches!(decl.def, TypeDef::Alias(_)));
            }
            other => panic!("expected type decl, got {other:?}"),
        }
    }

    #[test]
    fn type_record() {
        match first_item("type User = { id: UserId, name: string }") {
            ItemKind::TypeDecl(decl) => {
                assert_eq!(decl.name, "User");
                match decl.def {
                    TypeDef::Record(fields) => assert_eq!(fields.len(), 2),
                    other => panic!("expected record, got {other:?}"),
                }
            }
            other => panic!("expected type decl, got {other:?}"),
        }
    }

    #[test]
    fn type_union() {
        let input = r#"type Route = | Home | Profile(id: string) | NotFound"#;
        match first_item(input) {
            ItemKind::TypeDecl(decl) => {
                assert_eq!(decl.name, "Route");
                match decl.def {
                    TypeDef::Union(variants) => {
                        assert_eq!(variants.len(), 3);
                        assert_eq!(variants[0].name, "Home");
                        assert!(variants[0].fields.is_empty());
                        assert_eq!(variants[1].name, "Profile");
                        assert_eq!(variants[1].fields.len(), 1);
                        assert_eq!(variants[2].name, "NotFound");
                    }
                    other => panic!("expected union, got {other:?}"),
                }
            }
            other => panic!("expected type decl, got {other:?}"),
        }
    }

    #[test]
    fn opaque_type() {
        match first_item("opaque type HashedPassword = string") {
            ItemKind::TypeDecl(decl) => {
                assert!(decl.opaque);
                assert_eq!(decl.name, "HashedPassword");
            }
            other => panic!("expected type decl, got {other:?}"),
        }
    }

    // ── Member Access ────────────────────────────────────────────

    #[test]
    fn member_access() {
        let expr = first_expr("a.b.c");
        match expr {
            ExprKind::Member { object, field } => {
                assert_eq!(field, "c");
                assert!(matches!(object.kind, ExprKind::Member { field: ref f, .. } if f == "b"));
            }
            _ => panic!("expected member access"),
        }
    }

    // ── Array Literal ────────────────────────────────────────────

    #[test]
    fn array_literal() {
        let expr = first_expr("[1, 2, 3]");
        match expr {
            ExprKind::Array(elements) => {
                assert_eq!(elements.len(), 3);
            }
            _ => panic!("expected array"),
        }
    }

    // ── Index Access ─────────────────────────────────────────────

    #[test]
    fn index_access() {
        let expr = first_expr("arr[0]");
        assert!(matches!(expr, ExprKind::Index { .. }));
    }

    // ── JSX ──────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        let expr = first_expr("<Button />");
        match expr {
            ExprKind::Jsx(JsxElement {
                kind:
                    JsxElementKind::Element {
                        name, self_closing, ..
                    },
                ..
            }) => {
                assert_eq!(name, "Button");
                assert!(self_closing);
            }
            _ => panic!("expected jsx element"),
        }
    }

    #[test]
    fn jsx_with_props() {
        let expr = first_expr(r#"<Button label="Save" onClick={handleSave} />"#);
        match expr {
            ExprKind::Jsx(JsxElement {
                kind: JsxElementKind::Element { props, .. },
                ..
            }) => {
                assert_eq!(props.len(), 2);
                assert_eq!(props[0].name, "label");
                assert_eq!(props[1].name, "onClick");
            }
            _ => panic!("expected jsx element"),
        }
    }

    #[test]
    fn jsx_with_children() {
        let expr = first_expr("<div>{x}</div>");
        match expr {
            ExprKind::Jsx(JsxElement {
                kind: JsxElementKind::Element { children, .. },
                ..
            }) => {
                assert_eq!(children.len(), 1);
                assert!(matches!(&children[0], JsxChild::Expr(_)));
            }
            _ => panic!("expected jsx element"),
        }
    }

    #[test]
    fn jsx_fragment() {
        let expr = first_expr("<>{x}</>");
        match expr {
            ExprKind::Jsx(JsxElement {
                kind: JsxElementKind::Fragment { children },
                ..
            }) => {
                assert_eq!(children.len(), 1);
            }
            _ => panic!("expected fragment"),
        }
    }

    // ── Banned Keywords ──────────────────────────────────────────

    #[test]
    fn banned_keyword_error() {
        let result = parse("let x = 5");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].message.contains("banned keyword"));
    }

    // ── Block & Return ───────────────────────────────────────────

    #[test]
    fn block_with_return() {
        match first_item("function f() { const x = 1\nreturn x }") {
            ItemKind::Function(decl) => match decl.body.kind {
                ExprKind::Block(items) => {
                    assert_eq!(items.len(), 2);
                }
                _ => panic!("expected block"),
            },
            other => panic!("expected function, got {other:?}"),
        }
    }

    // ── Pipe with placeholder ────────────────────────────────────

    #[test]
    fn pipe_with_placeholder() {
        let expr = first_expr("x |> f(y, _, z)");
        match expr {
            ExprKind::Pipe { right, .. } => match right.kind {
                ExprKind::Call { args, .. } => {
                    assert_eq!(args.len(), 3);
                    assert!(
                        matches!(&args[1], Arg::Positional(e) if matches!(e.kind, ExprKind::Placeholder))
                    );
                }
                _ => panic!("expected call in pipe rhs"),
            },
            _ => panic!("expected pipe"),
        }
    }

    // ── Await ────────────────────────────────────────────────────

    #[test]
    fn await_expr() {
        let expr = first_expr("await fetchUser(id)");
        assert!(matches!(expr, ExprKind::Await(_)));
    }

    // ── If Expression ────────────────────────────────────────────

    #[test]
    fn if_else_expr() {
        let expr = first_expr("if x { 1 } else { 2 }");
        match expr {
            ExprKind::If { else_branch, .. } => {
                assert!(else_branch.is_some());
            }
            _ => panic!("expected if"),
        }
    }

    // ── Grouped Expression ───────────────────────────────────────

    #[test]
    fn grouped() {
        let expr = first_expr("(1 + 2)");
        assert!(matches!(expr, ExprKind::Grouped(_)));
    }

    // ── Type Expression ──────────────────────────────────────────

    #[test]
    fn generic_type() {
        match first_item("const x: Result<User, ApiError> = Ok(user)") {
            ItemKind::Const(decl) => {
                let type_ann = decl.type_ann.unwrap();
                match type_ann.kind {
                    TypeExprKind::Named { name, type_args } => {
                        assert_eq!(name, "Result");
                        assert_eq!(type_args.len(), 2);
                    }
                    _ => panic!("expected named type"),
                }
            }
            other => panic!("expected const, got {other:?}"),
        }
    }

    // ── Full program ─────────────────────────────────────────────

    #[test]
    fn full_program() {
        let input = r#"
import { useState } from "react"

type Todo = { id: string, text: string, done: bool }

export function TodoApp() {
    const [todos, setTodos] = useState([])
    return <div>{todos |> map(t => <li>{t.text}</li>)}</div>
}
"#;
        let program = parse_ok(input);
        assert_eq!(program.items.len(), 3);
    }
}
