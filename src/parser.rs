pub mod ast;
mod expr;
mod jsx;
mod pattern;

#[cfg(test)]
mod tests;

use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::lexer::span::Span;
use crate::lexer::token::{TemplatePart as LexTemplatePart, Token, TokenKind};
use crate::lower::lower_program;
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

/// The Floe parser. Produces an AST from a token stream.
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

    /// Parse a complete program using the CST pipeline (lexer → CST → lower → AST).
    pub fn parse_program_cst(source: &str) -> Result<Program, Vec<ParseError>> {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        let cst_parse = CstParser::new(source, tokens).parse();

        if !cst_parse.errors.is_empty() {
            return Err(cst_parse
                .errors
                .into_iter()
                .map(|e| ParseError {
                    message: e.message,
                    span: e.span,
                })
                .collect());
        }

        let root = cst_parse.syntax();
        lower_program(&root, source)
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
            TokenKind::Fn => {
                let mut decl = self.parse_function_decl()?;
                decl.exported = exported;
                ItemKind::Function(decl)
            }
            TokenKind::Type | TokenKind::Opaque => {
                let mut decl = self.parse_type_decl()?;
                decl.exported = exported;
                ItemKind::TypeDecl(decl)
            }
            TokenKind::For if !exported => {
                let block = self.parse_for_block()?;
                ItemKind::ForBlock(block)
            }
            _ if !exported
                && self.check_identifier("test")
                && matches!(self.peek_kind(), Some(TokenKind::String(_))) =>
            {
                let block = self.parse_test_block()?;
                ItemKind::TestBlock(block)
            }
            TokenKind::Async if self.peek_kind() == Some(&TokenKind::Fn) => {
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

        // Check for `import trusted { ... }` (all specifiers trusted)
        let trusted = self.check_identifier("trusted");
        if trusted {
            self.advance();
        }

        let specifiers = if self.check(&TokenKind::LeftBrace) {
            self.advance();
            let specs = self.parse_comma_separated(|p| p.parse_import_specifier())?;
            self.expect(&TokenKind::RightBrace)?;
            specs
        } else {
            // import "module" (bare import, no specifiers)
            Vec::new()
        };

        // `from` is required when there are specifiers, optional for bare imports
        if self.check(&TokenKind::From) {
            self.advance();
        }
        let source = self.expect_string()?;

        Ok(ImportDecl {
            trusted,
            specifiers,
            source,
        })
    }

    fn parse_import_specifier(&mut self) -> Result<ImportSpecifier, ParseError> {
        let start_span = self.current_span();

        // Check for `trusted` modifier on individual specifier
        let trusted = self.check_identifier("trusted");
        if trusted {
            self.advance();
        }

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
            trusted,
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
        } else if self.check(&TokenKind::LeftParen) && self.is_tuple_destructuring() {
            // Tuple destructuring: `const (a, b) = ...`
            self.advance();
            let names = self.parse_comma_separated(|p| p.expect_identifier())?;
            self.expect(&TokenKind::RightParen)?;
            ConstBinding::Tuple(names)
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

        self.expect(&TokenKind::Fn)?;
        let name = self.expect_identifier()?;

        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_comma_separated(|p| p.parse_param())?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.check(&TokenKind::ThinArrow) {
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

    // ── For Blocks ───────────────────────────────────────────────

    fn parse_for_block(&mut self) -> Result<ForBlock, ParseError> {
        let start_span = self.current_span();
        self.expect(&TokenKind::For)?;

        let type_name = self.parse_type_expr()?;

        self.expect(&TokenKind::LeftBrace)?;

        let mut functions = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Allow `export` prefix on for-block functions
            let exported = self.check(&TokenKind::Export);
            if exported {
                self.advance();
            }

            if self.check(&TokenKind::Fn) || self.check(&TokenKind::Async) {
                let mut decl = self.parse_for_block_function()?;
                decl.exported = exported;
                functions.push(decl);
            } else {
                return Err(self.error("expected `fn` inside for block"));
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(ForBlock {
            type_name,
            functions,
            span: self.merge_spans(start_span, end_span),
        })
    }

    /// Parse a function declaration inside a `for` block.
    /// `self` parameters get their type inferred from the `for` block's type.
    fn parse_for_block_function(&mut self) -> Result<FunctionDecl, ParseError> {
        let async_fn = if self.check(&TokenKind::Async) {
            self.advance();
            true
        } else {
            false
        };

        self.expect(&TokenKind::Fn)?;
        let name = self.expect_identifier()?;

        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_comma_separated(|p| p.parse_for_block_param())?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.check(&TokenKind::ThinArrow) {
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

    /// Parse a parameter inside a `for` block function.
    /// Handles `self` as a special parameter name (no type annotation needed).
    fn parse_for_block_param(&mut self) -> Result<Param, ParseError> {
        let start_span = self.current_span();

        // Handle `self` keyword as parameter
        if self.check(&TokenKind::SelfKw) {
            self.advance();
            let end_span = self.previous_span();
            return Ok(Param {
                name: "self".to_string(),
                type_ann: Option::None, // type inferred from for block
                default: Option::None,
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Regular parameter
        self.parse_param()
    }

    // ── Test Blocks ──────────────────────────────────────────────

    fn parse_test_block(&mut self) -> Result<TestBlock, ParseError> {
        let start_span = self.current_span();
        // `test` is a contextual keyword - it's an identifier
        if !self.check_identifier("test") {
            return Err(self.error("expected `test`"));
        }
        self.advance();

        let name = self.expect_string()?;

        self.expect(&TokenKind::LeftBrace)?;

        let mut body = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            if self.check(&TokenKind::Assert) {
                let assert_span = self.current_span();
                self.advance(); // consume `assert`
                let expr = self.parse_expr()?;
                let end_span = self.previous_span();
                body.push(TestStatement::Assert(
                    expr,
                    self.merge_spans(assert_span, end_span),
                ));
            } else {
                // Allow regular items (const, fn calls, etc.) inside test blocks
                let item = self.parse_item()?;
                if let ItemKind::Expr(expr) = item.kind {
                    body.push(TestStatement::Expr(expr));
                } else {
                    // Convert non-expression items to a parse error for now
                    return Err(self.error(
                        "only `assert` statements and expressions are allowed inside test blocks",
                    ));
                }
            }
        }

        self.expect(&TokenKind::RightBrace)?;
        let end_span = self.previous_span();

        Ok(TestBlock {
            name,
            body,
            span: self.merge_spans(start_span, end_span),
        })
    }

    // ── Type Expressions ─────────────────────────────────────────

    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let start_span = self.current_span();

        // Unit type: `()` — must check before function type
        if self.check(&TokenKind::LeftParen) && self.is_unit_type() {
            self.advance(); // (
            self.advance(); // )
            let end_span = self.previous_span();
            return Ok(TypeExpr {
                kind: TypeExprKind::Named {
                    name: "()".to_string(),
                    type_args: Vec::new(),
                },
                span: self.merge_spans(start_span, end_span),
            });
        }

        // Function type: `(params) -> ReturnType`
        if self.check(&TokenKind::LeftParen) && self.is_function_type() {
            return self.parse_function_type();
        }

        // Tuple type: `(T, U)` — paren with comma-separated types, no `->` after `)`
        if self.check(&TokenKind::LeftParen) && self.is_tuple_type() {
            self.advance(); // (
            let types = self.parse_comma_separated(|p| p.parse_type_expr())?;
            self.expect(&TokenKind::RightParen)?;
            let end_span = self.previous_span();
            return Ok(TypeExpr {
                kind: TypeExprKind::Tuple(types),
                span: self.merge_spans(start_span, end_span),
            });
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

        // Named type: `string`, `Option<T>`, `Result<T, E>`, `JSX.Element`
        let mut name = self.expect_identifier()?;

        // Support dotted type names (e.g. `JSX.Element`)
        while self.check(&TokenKind::Dot) {
            self.advance();
            let part = self.expect_identifier()?;
            name = format!("{name}.{part}");
        }

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
        self.expect(&TokenKind::ThinArrow)?;
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

    /// Is the current `(` the start of a unit type `()`?
    /// True when `(` is immediately followed by `)` and NOT by `->`.
    fn is_unit_type(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos + 1].kind == TokenKind::RightParen
            && !(self.pos + 2 < self.tokens.len()
                && self.tokens[self.pos + 2].kind == TokenKind::ThinArrow)
    }

    /// Heuristic: is the current `(` the start of a tuple type `(T, U)`?
    /// True when parens contain a comma and are NOT followed by `->`.
    fn is_tuple_type(&self) -> bool {
        let mut depth = 0;
        let mut has_comma = false;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Must have a comma and NOT be followed by `->`
                        if !has_comma {
                            return false;
                        }
                        return !(i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::ThinArrow);
                    }
                }
                TokenKind::Comma if depth == 1 => has_comma = true,
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` the start of a tuple destructuring `(a, b) = ...`?
    /// Look ahead for `) =`.
    fn is_tuple_destructuring(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check: must be followed by `=` or `:` (type ann then `=`)
                        if i + 1 < self.tokens.len() {
                            return matches!(
                                self.tokens[i + 1].kind,
                                TokenKind::Equal | TokenKind::Colon
                            );
                        }
                        return false;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` the start of a function type?
    /// Look ahead for `) ->`.
    fn is_function_type(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Check if followed by `->`
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::ThinArrow;
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `<` the start of generic type arguments in a call?
    /// Look ahead for `< types > (` pattern. Handles nesting.
    fn is_generic_call(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LessThan => depth += 1,
                TokenKind::GreaterThan => {
                    depth -= 1;
                    if depth == 0 {
                        // Must be followed by `(`
                        return i + 1 < self.tokens.len()
                            && self.tokens[i + 1].kind == TokenKind::LeftParen;
                    }
                }
                // If we see something that can't be in a type, bail
                TokenKind::LeftBrace
                | TokenKind::RightBrace
                | TokenKind::Semicolon
                | TokenKind::Equal
                | TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
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

    fn peek_nth_kind(&self, n: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + n).map(|t| &t.kind)
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

    /// Check if the current token is `|` used in union type declarations.
    fn check_pipe_in_union(&self) -> bool {
        self.check(&TokenKind::VerticalBar)
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
                | TokenKind::Fn
                | TokenKind::Export
                | TokenKind::Import
                | TokenKind::Type
                | TokenKind::Opaque
                | TokenKind::For
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
