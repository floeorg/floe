use crate::lexer::span::Span;
use crate::lexer::token::{Token, TokenKind};
use crate::syntax::{SyntaxKind, SyntaxNode, token_kind_to_syntax};
use rowan::GreenNode;

/// Result of CST parsing.
pub struct Parse {
    pub green_node: GreenNode,
    pub errors: Vec<CstError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green_node.clone())
    }
}

#[derive(Debug, Clone)]
pub struct CstError {
    pub message: String,
    pub span: Span,
}

/// CST parser: builds a lossless green tree from a token stream (including trivia).
pub struct CstParser<'src> {
    source: &'src str,
    tokens: Vec<Token>,
    pos: usize,
    builder: rowan::GreenNodeBuilder<'static>,
    errors: Vec<CstError>,
}

impl<'src> CstParser<'src> {
    pub fn new(source: &'src str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            builder: rowan::GreenNodeBuilder::new(),
            errors: Vec::new(),
        }
    }

    pub fn parse(mut self) -> Parse {
        self.builder.start_node(SyntaxKind::PROGRAM.into());
        self.eat_trivia();

        while !self.at_end() {
            self.parse_item();
            self.eat_trivia();
        }

        // Eat any remaining trivia and EOF
        self.eat_trivia();
        if self.at_end() {
            self.bump();
        }

        self.builder.finish_node();
        Parse {
            green_node: self.builder.finish(),
            errors: self.errors,
        }
    }

    // ── Items ────────────────────────────────────────────────────

    fn parse_item(&mut self) {
        let checkpoint = self.builder.checkpoint();

        // Handle export prefix
        let exported = self.at(TokenKind::Export);
        if exported {
            self.bump(); // export
            self.eat_trivia();
        }

        match self.current_kind() {
            Some(TokenKind::Import) if !exported => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_import();
                self.builder.finish_node();
            }
            Some(TokenKind::Import) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ERROR.into());
                self.error("cannot export an import statement");
                self.bump();
                self.builder.finish_node();
            }
            Some(TokenKind::Const) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_const_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Fn) | Some(TokenKind::Async) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_function_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Opaque) | Some(TokenKind::Type) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_type_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::For) if !exported => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_for_block();
                self.builder.finish_node();
            }
            _ if exported => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ERROR.into());
                self.error("expected declaration after 'export'");
                self.builder.finish_node();
            }
            _ => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::EXPR_ITEM.into());
                self.parse_expr();
                self.builder.finish_node();
            }
        }
    }

    // ── Import ────────────────────────────────────────────────────

    fn parse_import(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_DECL.into());
        self.expect(TokenKind::Import);
        self.eat_trivia();

        if self.at(TokenKind::LeftBrace) {
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_import_specifier, TokenKind::RightBrace);
            self.expect(TokenKind::RightBrace);
            self.eat_trivia();
        }

        // `from` is required with specifiers, optional for bare imports
        if self.at(TokenKind::From) {
            self.bump();
            self.eat_trivia();
        }
        self.expect_kind(TokenKind::String("".into()));

        self.builder.finish_node();
    }

    fn parse_import_specifier(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_SPECIFIER.into());
        self.expect_ident();
        self.eat_trivia();

        // Check for `as alias` — "as" is a banned keyword but used contextually here
        if self.at_identifier("as")
            || self.at(TokenKind::Banned(crate::lexer::token::BannedKeyword::As))
        {
            self.bump();
            self.eat_trivia();
            self.expect_ident();
        }

        self.builder.finish_node();
    }

    // ── Const Declaration ────────────────────────────────────────

    fn parse_const_decl(&mut self) {
        self.builder.start_node(SyntaxKind::CONST_DECL.into());
        self.expect(TokenKind::Const);
        self.eat_trivia();

        if self.at(TokenKind::LeftBracket) {
            // Array destructuring
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightBracket);
            self.expect(TokenKind::RightBracket);
        } else if self.at(TokenKind::LeftBrace) {
            // Object destructuring
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightBrace);
            self.expect(TokenKind::RightBrace);
        } else if self.at(TokenKind::LeftParen) && self.is_const_tuple_destructuring() {
            // Tuple destructuring: const (a, b) = ...
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
        } else {
            self.expect_ident();
        }
        self.eat_trivia();

        // Optional type annotation
        if self.at(TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.expect(TokenKind::Equal);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }

    // ── Function Declaration ────────────────────────────────────

    fn parse_function_decl(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

        if self.at(TokenKind::Async) {
            self.bump();
            self.eat_trivia();
        }

        self.expect(TokenKind::Fn);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        self.expect(TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_param, TokenKind::RightParen);
        self.expect(TokenKind::RightParen);
        self.eat_trivia();

        // Optional return type
        if self.at(TokenKind::ThinArrow) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.parse_block_expr();

        self.builder.finish_node();
    }

    fn parse_param(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM.into());
        self.expect_ident();
        self.eat_trivia();

        if self.at(TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        if self.at(TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── Type Declaration ────────────────────────────────────────

    fn parse_type_decl(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_DECL.into());

        if self.at(TokenKind::Opaque) {
            self.bump();
            self.eat_trivia();
        }

        self.expect(TokenKind::Type);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        // Optional type parameters: <T, U>
        if self.at(TokenKind::LessThan) {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::GreaterThan);
            self.expect(TokenKind::GreaterThan);
            self.eat_trivia();
        }

        self.expect(TokenKind::Equal);
        self.eat_trivia();

        self.parse_type_def();

        self.builder.finish_node();
    }

    fn parse_type_def(&mut self) {
        if self.at_pipe_in_union() {
            self.parse_union_variants();
        } else if self.at(TokenKind::LeftBrace) {
            self.builder.start_node(SyntaxKind::TYPE_DEF_RECORD.into());
            self.parse_record_fields();
            self.builder.finish_node();
        } else {
            self.builder.start_node(SyntaxKind::TYPE_DEF_ALIAS.into());
            self.parse_type_expr();
            self.builder.finish_node();
        }
    }

    fn parse_union_variants(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());

        while self.at_pipe_in_union() {
            self.builder.start_node(SyntaxKind::VARIANT.into());
            self.bump(); // |
            self.eat_trivia();
            self.expect_ident();
            self.eat_trivia();

            if self.at(TokenKind::LeftParen) {
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_variant_field, TokenKind::RightParen);
                self.expect(TokenKind::RightParen);
                self.eat_trivia();
            }

            self.builder.finish_node();
        }

        self.builder.finish_node();
    }

    fn parse_variant_field(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_FIELD.into());

        // Check if this is a named field: `name: Type`
        if self.is_ident() && self.peek_is(TokenKind::Colon) {
            self.bump(); // name
            self.eat_trivia();
            self.bump(); // :
            self.eat_trivia();
            self.parse_type_expr();
        } else {
            self.parse_type_expr();
        }

        self.builder.finish_node();
    }

    fn parse_record_fields(&mut self) {
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_record_field, TokenKind::RightBrace);
        self.expect(TokenKind::RightBrace);
    }

    fn parse_record_field(&mut self) {
        self.builder.start_node(SyntaxKind::RECORD_FIELD.into());
        self.expect_ident();
        self.eat_trivia();
        self.expect(TokenKind::Colon);
        self.eat_trivia();
        self.parse_type_expr();
        self.eat_trivia();

        if self.at(TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── For Blocks ──────────────────────────────────────────────

    fn parse_for_block(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_BLOCK.into());

        self.expect(TokenKind::For);
        self.eat_trivia();

        // Parse the type name (e.g., `User`, `Array<T>`)
        self.parse_type_expr();
        self.eat_trivia();

        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse function declarations inside the block (with optional export)
        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            if self.at(TokenKind::Export) {
                self.bump();
                self.eat_trivia();
            }
            if self.at(TokenKind::Fn) || self.at(TokenKind::Async) {
                self.parse_for_block_function();
                self.eat_trivia();
            } else {
                self.error("expected `fn` inside for block");
                self.bump();
                self.eat_trivia();
            }
        }

        self.expect(TokenKind::RightBrace);

        self.builder.finish_node();
    }

    fn parse_for_block_function(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

        if self.at(TokenKind::Async) {
            self.bump();
            self.eat_trivia();
        }

        self.expect(TokenKind::Fn);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        self.expect(TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_for_block_param, TokenKind::RightParen);
        self.expect(TokenKind::RightParen);
        self.eat_trivia();

        // Optional return type
        if self.at(TokenKind::ThinArrow) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.parse_block_expr();

        self.builder.finish_node();
    }

    fn parse_for_block_param(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM.into());

        if self.at(TokenKind::SelfKw) {
            // `self` parameter — bump as an ident-like token
            self.bump();
        } else {
            self.expect_ident();
            self.eat_trivia();

            if self.at(TokenKind::Colon) {
                self.bump();
                self.eat_trivia();
                self.parse_type_expr();
                self.eat_trivia();
            }

            if self.at(TokenKind::Equal) {
                self.bump();
                self.eat_trivia();
                self.parse_expr();
            }
        }

        self.builder.finish_node();
    }

    // ── Type Expressions ────────────────────────────────────────

    fn parse_type_expr(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_EXPR.into());

        // Unit type: ()
        if self.at(TokenKind::LeftParen) && self.is_unit_type() {
            self.bump(); // (
            self.eat_trivia();
            self.bump(); // )
        }
        // Function type: (params) -> ReturnType
        else if self.at(TokenKind::LeftParen) && self.is_function_type() {
            self.parse_function_type();
        }
        // Tuple type: (T, U) — paren with comma, no `->` after `)`
        else if self.at(TokenKind::LeftParen) && self.is_paren_tuple_type() {
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
        }
        // Tuple: [T, U]
        else if self.at(TokenKind::LeftBracket) {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightBracket);
            self.expect(TokenKind::RightBracket);
        }
        // Record type: { ... }
        else if self.at(TokenKind::LeftBrace) {
            self.parse_record_fields();
        }
        // Named type
        else {
            self.expect_ident();
            self.eat_trivia();

            // Dotted names (e.g. JSX.Element)
            while self.at(TokenKind::Dot) {
                self.bump();
                self.eat_trivia();
                self.expect_ident();
                self.eat_trivia();
            }

            // Type arguments: <T, U>
            if self.at(TokenKind::LessThan) {
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_type_expr, TokenKind::GreaterThan);
                self.expect(TokenKind::GreaterThan);
            }
        }

        self.builder.finish_node();
    }

    fn parse_function_type(&mut self) {
        self.expect(TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_type_expr, TokenKind::RightParen);
        self.expect(TokenKind::RightParen);
        self.eat_trivia();
        self.expect(TokenKind::ThinArrow);
        self.eat_trivia();
        self.parse_type_expr();
    }

    // ── Expressions ─────────────────────────────────────────────

    fn parse_expr(&mut self) {
        self.parse_pipe_expr();
    }

    fn parse_pipe_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_or_expr();

        while self.at(TokenKind::Pipe) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::PIPE_EXPR.into());
            self.bump(); // |>
            self.eat_trivia();
            self.parse_or_expr();
            self.builder.finish_node();
        }
    }

    fn parse_or_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_and_expr();

        while self.at(TokenKind::PipePipe) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_and_expr();
            self.builder.finish_node();
        }
    }

    fn parse_and_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_equality_expr();

        while self.at(TokenKind::AmpAmp) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_equality_expr();
            self.builder.finish_node();
        }
    }

    fn parse_equality_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_comparison_expr();

        while self.at(TokenKind::EqualEqual) || self.at(TokenKind::BangEqual) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_comparison_expr();
            self.builder.finish_node();
        }
    }

    fn parse_comparison_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_additive_expr();

        while self.at(TokenKind::LessThan)
            || self.at(TokenKind::GreaterThan)
            || self.at(TokenKind::LessEqual)
            || self.at(TokenKind::GreaterEqual)
        {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_additive_expr();
            self.builder.finish_node();
        }
    }

    fn parse_additive_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_multiplicative_expr();

        while self.at(TokenKind::Plus) || self.at(TokenKind::Minus) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_multiplicative_expr();
            self.builder.finish_node();
        }
    }

    fn parse_multiplicative_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_unary_expr();

        while self.at(TokenKind::Star) || self.at(TokenKind::Slash) || self.at(TokenKind::Percent) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_unary_expr();
            self.builder.finish_node();
        }
    }

    fn parse_unary_expr(&mut self) {
        match self.current_kind() {
            Some(TokenKind::Bang) | Some(TokenKind::Minus) => {
                self.builder.start_node(SyntaxKind::UNARY_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.parse_unary_expr();
                self.builder.finish_node();
            }
            Some(TokenKind::Await) => {
                self.builder.start_node(SyntaxKind::AWAIT_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.parse_unary_expr();
                self.builder.finish_node();
            }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_postfix_expr(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_primary_expr();

        loop {
            self.eat_trivia();
            match self.current_kind() {
                Some(TokenKind::Question) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::UNWRAP_EXPR.into());
                    self.bump();
                    self.builder.finish_node();
                }
                Some(TokenKind::Dot) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::MEMBER_EXPR.into());
                    self.bump();
                    self.eat_trivia();
                    self.expect_ident();
                    self.builder.finish_node();
                }
                Some(TokenKind::LeftBracket) => {
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::INDEX_EXPR.into());
                    self.bump();
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightBracket);
                    self.builder.finish_node();
                }
                Some(TokenKind::LeftParen) => {
                    // Check if it's a constructor (uppercase ident) — don't parse as call
                    if self.is_uppercase_ident_at_checkpoint() {
                        break;
                    }
                    self.builder
                        .start_node_at(checkpoint, SyntaxKind::CALL_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                }
                _ => break,
            }
        }
    }

    fn parse_primary_expr(&mut self) {
        match self.current_kind() {
            Some(TokenKind::Number(_)) => self.bump(),
            Some(TokenKind::String(_)) => self.bump(),
            Some(TokenKind::TemplateLiteral(_)) => self.bump(),
            Some(TokenKind::Bool(_)) => self.bump(),
            Some(TokenKind::Underscore) => self.bump(),
            Some(TokenKind::None) => self.bump(),
            Some(TokenKind::Todo) => self.bump(),
            Some(TokenKind::Unreachable) => self.bump(),

            Some(TokenKind::Ok) => {
                self.builder.start_node(SyntaxKind::OK_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.expect(TokenKind::LeftParen);
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightParen);
                self.builder.finish_node();
            }

            Some(TokenKind::Err) => {
                self.builder.start_node(SyntaxKind::ERR_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.expect(TokenKind::LeftParen);
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightParen);
                self.builder.finish_node();
            }

            Some(TokenKind::Some) => {
                self.builder.start_node(SyntaxKind::SOME_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.expect(TokenKind::LeftParen);
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightParen);
                self.builder.finish_node();
            }

            Some(TokenKind::Match) => self.parse_match_expr(),
            Some(TokenKind::Return) => {
                self.builder.start_node(SyntaxKind::RETURN_EXPR.into());
                self.bump();
                self.eat_trivia();
                if !self.at_end()
                    && !self.at(TokenKind::RightBrace)
                    && !self.at(TokenKind::Semicolon)
                {
                    self.parse_expr();
                }
                self.builder.finish_node();
            }
            Some(TokenKind::LeftBrace) => self.parse_block_expr(),

            Some(TokenKind::LeftBracket) => {
                self.builder.start_node(SyntaxKind::ARRAY_EXPR.into());
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_expr, TokenKind::RightBracket);
                self.expect(TokenKind::RightBracket);
                self.builder.finish_node();
            }

            Some(TokenKind::LeftParen) => {
                if self.peek_is(TokenKind::RightParen) {
                    // Unit value: ()
                    self.bump(); // (
                    self.eat_trivia();
                    self.bump(); // )
                } else if self.is_paren_tuple_expr() {
                    // Tuple: (expr, expr, ...)
                    self.builder.start_node(SyntaxKind::TUPLE_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_expr, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                } else {
                    self.builder.start_node(SyntaxKind::GROUPED_EXPR.into());
                    self.bump(); // (
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightParen);
                    self.builder.finish_node();
                }
            }

            Some(TokenKind::LessThan) => self.parse_jsx_element(),

            Some(TokenKind::Dot) => {
                self.parse_dot_shorthand();
            }

            Some(TokenKind::VerticalBar) => {
                self.parse_pipe_lambda();
            }

            Some(TokenKind::PipePipe) => {
                // Zero-arg lambda: `|| expr`
                self.builder.start_node(SyntaxKind::ARROW_EXPR.into());
                self.bump(); // ||
                self.eat_trivia();
                self.parse_expr();
                self.builder.finish_node();
            }

            // `self` keyword — treat as identifier in expression context
            Some(TokenKind::SelfKw) => {
                self.bump();
            }

            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();

                // Uppercase + ( → constructor
                if name.starts_with(char::is_uppercase) && self.peek_is(TokenKind::LeftParen) {
                    self.parse_construct_expr();
                    return;
                }

                // Qualified variant: `Filter.All` or `Route.Profile(id: "123")`
                if name.starts_with(char::is_uppercase)
                    && self.peek_is(TokenKind::Dot)
                    && let Some(TokenKind::Identifier(variant_name)) =
                        self.peek_nth_non_trivia_kind(2)
                    && variant_name.starts_with(char::is_uppercase)
                {
                    // Check if there's a `(` after the variant name (3rd non-trivia)
                    let has_args =
                        matches!(self.peek_nth_non_trivia_kind(3), Some(TokenKind::LeftParen));

                    if has_args {
                        // Qualified constructor: Route.Profile(id: "123")
                        // Emit as CONSTRUCT_EXPR with variant_name as the type name
                        self.builder.start_node(SyntaxKind::CONSTRUCT_EXPR.into());
                        self.bump(); // type name (Filter/Route)
                        self.eat_trivia();
                        self.bump(); // .
                        self.eat_trivia();
                        self.bump(); // variant name (Profile) - this becomes the type_name ident
                        self.eat_trivia();
                        self.expect(TokenKind::LeftParen);
                        self.eat_trivia();

                        // Check for spread
                        if self.at(TokenKind::DotDot) {
                            self.builder.start_node(SyntaxKind::SPREAD_EXPR.into());
                            self.bump();
                            self.eat_trivia();
                            self.parse_expr();
                            self.builder.finish_node();
                            self.eat_trivia();
                            if self.at(TokenKind::Comma) {
                                self.bump();
                                self.eat_trivia();
                            }
                        }

                        if !self.at(TokenKind::RightParen) {
                            self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
                        }

                        self.expect(TokenKind::RightParen);
                        self.builder.finish_node();
                        return;
                    } else {
                        // Qualified unit variant: Filter.All → just emit the variant name as IDENT
                        self.bump(); // type name
                        self.eat_trivia();
                        self.bump(); // .
                        self.eat_trivia();
                        self.bump(); // variant name (emitted as IDENT token)
                        return;
                    }
                }

                self.bump();
            }

            Some(TokenKind::Banned(_)) => {
                self.builder.start_node(SyntaxKind::ERROR.into());
                let kind = self.current_kind().unwrap();
                if let TokenKind::Banned(banned) = kind {
                    self.error(&format!(
                        "banned keyword '{}': {}",
                        banned.as_str(),
                        banned.help_message()
                    ));
                }
                self.bump();
                self.builder.finish_node();
            }

            _ => {
                self.builder.start_node(SyntaxKind::ERROR.into());
                if let Some(kind) = self.current_kind() {
                    self.error(&format!("unexpected token: {:?}", kind));
                    self.bump();
                }
                self.builder.finish_node();
            }
        }
    }

    // ── Constructors ─────────────────────────────────────────────

    fn parse_construct_expr(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRUCT_EXPR.into());
        self.bump(); // TypeName
        self.eat_trivia();
        self.expect(TokenKind::LeftParen);
        self.eat_trivia();

        // Check for spread: `..expr`
        if self.at(TokenKind::DotDot) {
            self.builder.start_node(SyntaxKind::SPREAD_EXPR.into());
            self.bump();
            self.eat_trivia();
            self.parse_expr();
            self.builder.finish_node();
            self.eat_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.eat_trivia();
            }
        }

        if !self.at(TokenKind::RightParen) {
            self.parse_comma_separated(Self::parse_call_arg, TokenKind::RightParen);
        }

        self.expect(TokenKind::RightParen);
        self.builder.finish_node();
    }

    // ── Call Arguments ───────────────────────────────────────────

    fn parse_call_arg(&mut self) {
        self.builder.start_node(SyntaxKind::ARG.into());

        // Named arg: `label: expr` or punned `label:`
        if self.is_ident() && self.peek_is(TokenKind::Colon) {
            self.bump(); // label
            self.eat_trivia();
            self.bump(); // :

            // Punning: `label:` without a value — next non-trivia is `)` or `,`
            let next = self.next_non_trivia_kind();
            let is_pun = matches!(
                next,
                Some(TokenKind::RightParen) | Some(TokenKind::Comma) | None
            );
            if !is_pun {
                self.eat_trivia();
                self.parse_expr();
            }
        } else {
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── Dot Shorthand ────────────────────────────────────────────

    /// Parse `.field` or `.field op expr` dot shorthand expression.
    fn parse_dot_shorthand(&mut self) {
        self.builder.start_node(SyntaxKind::DOT_SHORTHAND.into());
        self.expect(TokenKind::Dot);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        // Check for optional binary operator predicate
        if self.at(TokenKind::EqualEqual)
            || self.at(TokenKind::BangEqual)
            || self.at(TokenKind::LessThan)
            || self.at(TokenKind::GreaterThan)
            || self.at(TokenKind::LessEqual)
            || self.at(TokenKind::GreaterEqual)
            || self.at(TokenKind::AmpAmp)
            || self.at(TokenKind::PipePipe)
            || self.at(TokenKind::Plus)
            || self.at(TokenKind::Minus)
            || self.at(TokenKind::Star)
            || self.at(TokenKind::Slash)
            || self.at(TokenKind::Percent)
        {
            self.bump(); // operator
            self.eat_trivia();
            self.parse_primary_expr();
        }

        self.builder.finish_node();
    }

    // ── Pipe Lambda ──────────────────────────────────────────────

    /// Parse `|params| body` pipe lambda.
    fn parse_pipe_lambda(&mut self) {
        self.builder.start_node(SyntaxKind::ARROW_EXPR.into());
        self.expect(TokenKind::VerticalBar);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_param, TokenKind::VerticalBar);
        self.expect(TokenKind::VerticalBar);
        self.eat_trivia();
        self.parse_expr();
        self.builder.finish_node();
    }

    // ── Match Expression ─────────────────────────────────────────

    fn parse_match_expr(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_EXPR.into());
        self.expect(TokenKind::Match);
        self.eat_trivia();
        self.parse_expr();
        self.eat_trivia();
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            self.parse_match_arm();
            self.eat_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.eat_trivia();
            }
        }

        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }

    fn parse_match_arm(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_ARM.into());
        self.parse_pattern();
        self.eat_trivia();

        // Optional guard: `when expr`
        if self.at(TokenKind::When) {
            self.builder.start_node(SyntaxKind::MATCH_GUARD.into());
            self.bump(); // consume `when`
            self.eat_trivia();
            self.parse_expr();
            self.builder.finish_node();
            self.eat_trivia();
        }

        self.expect(TokenKind::ThinArrow);
        self.eat_trivia();
        self.parse_expr();
        self.builder.finish_node();
    }

    fn parse_pattern(&mut self) {
        self.builder.start_node(SyntaxKind::PATTERN.into());

        match self.current_kind() {
            Some(TokenKind::Underscore) => {
                self.bump();
            }
            Some(TokenKind::Bool(_)) => {
                self.bump();
            }
            Some(TokenKind::String(_)) => {
                self.bump();
            }
            Some(TokenKind::Number(_)) => {
                self.bump();
                self.eat_trivia();
                if self.at(TokenKind::DotDot) {
                    self.bump();
                    self.eat_trivia();
                    // Expect number after ..
                    if matches!(self.current_kind(), Some(TokenKind::Number(_))) {
                        self.bump();
                    } else {
                        self.error("expected number after '..' in range pattern");
                    }
                }
            }
            Some(TokenKind::LeftBrace) => {
                self.bump(); // {
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_record_pattern_field, TokenKind::RightBrace);
                self.expect(TokenKind::RightBrace);
            }
            Some(TokenKind::LeftParen) => {
                // Tuple pattern: (x, y)
                self.bump(); // (
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                self.expect(TokenKind::RightParen);
            }
            Some(TokenKind::None) => {
                self.bump();
            }
            Some(TokenKind::Ok) | Some(TokenKind::Err) | Some(TokenKind::Some) => {
                self.bump();
                self.eat_trivia();
                if self.at(TokenKind::LeftParen) {
                    self.bump();
                    self.eat_trivia();
                    self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                    self.expect(TokenKind::RightParen);
                }
            }
            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();
                if name.starts_with(char::is_uppercase) {
                    self.bump();
                    self.eat_trivia();
                    if self.at(TokenKind::LeftParen) {
                        self.bump();
                        self.eat_trivia();
                        self.parse_comma_separated(Self::parse_pattern, TokenKind::RightParen);
                        self.expect(TokenKind::RightParen);
                    }
                } else {
                    self.bump();
                }
            }
            _ => {
                self.error(&format!(
                    "unexpected token in pattern: {:?}",
                    self.current_kind()
                ));
                if !self.at_end() {
                    self.bump();
                }
            }
        }

        self.builder.finish_node();
    }

    fn parse_record_pattern_field(&mut self) {
        self.expect_ident();
        self.eat_trivia();
        if self.at(TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_pattern();
        }
    }

    // ── Block Expression ─────────────────────────────────────────

    fn parse_block_expr(&mut self) {
        self.builder.start_node(SyntaxKind::BLOCK_EXPR.into());
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            self.parse_item();
            self.eat_trivia();
        }

        self.expect(TokenKind::RightBrace);
        self.builder.finish_node();
    }

    // ── JSX ──────────────────────────────────────────────────────

    fn parse_jsx_element(&mut self) {
        self.builder.start_node(SyntaxKind::JSX_ELEMENT.into());
        self.expect(TokenKind::LessThan);
        self.eat_trivia();

        // Fragment: <>
        if self.at(TokenKind::GreaterThan) {
            self.bump(); // >
            self.parse_jsx_children();
            // Expect </>
            self.expect(TokenKind::LessThan);
            self.expect(TokenKind::Slash);
            self.expect(TokenKind::GreaterThan);
            self.builder.finish_node();
            return;
        }

        self.expect_ident(); // tag name
        self.eat_trivia();

        // Props
        while !self.at(TokenKind::GreaterThan) && !self.at(TokenKind::Slash) && !self.at_end() {
            self.parse_jsx_prop();
            self.eat_trivia();
        }

        // Self-closing: />
        if self.at(TokenKind::Slash) {
            self.bump();
            self.expect(TokenKind::GreaterThan);
            self.builder.finish_node();
            return;
        }

        self.expect(TokenKind::GreaterThan);
        self.parse_jsx_children();

        // Closing tag: </Tag>
        self.expect(TokenKind::LessThan);
        self.expect(TokenKind::Slash);
        self.eat_trivia();
        self.expect_ident();
        self.expect(TokenKind::GreaterThan);

        self.builder.finish_node();
    }

    fn parse_jsx_prop(&mut self) {
        self.builder.start_node(SyntaxKind::JSX_PROP.into());
        self.expect_ident();
        self.eat_trivia();

        if self.at(TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            if self.at(TokenKind::LeftBrace) {
                self.bump();
                self.eat_trivia();
                self.parse_expr();
                self.eat_trivia();
                self.expect(TokenKind::RightBrace);
            } else if matches!(self.current_kind(), Some(TokenKind::String(_))) {
                self.bump();
            } else {
                self.error("expected '{' or string after '=' in JSX prop");
            }
        }

        self.builder.finish_node();
    }

    fn parse_jsx_children(&mut self) {
        loop {
            // Check for closing tag
            if self.at(TokenKind::LessThan) && self.peek_is(TokenKind::Slash) {
                break;
            }
            if self.at_end() {
                break;
            }

            match self.current_kind() {
                Some(TokenKind::LeftBrace) => {
                    self.builder.start_node(SyntaxKind::JSX_EXPR_CHILD.into());
                    self.bump();
                    self.eat_trivia();
                    self.parse_expr();
                    self.eat_trivia();
                    self.expect(TokenKind::RightBrace);
                    self.builder.finish_node();
                }
                Some(TokenKind::LessThan) => {
                    self.parse_jsx_element();
                }
                _ => {
                    if self.is_jsx_text_token() {
                        self.builder.start_node(SyntaxKind::JSX_TEXT.into());
                        self.bump();
                        while !self.at_end()
                            && !self.at(TokenKind::LeftBrace)
                            && !self.at(TokenKind::LessThan)
                            && self.is_jsx_text_token()
                        {
                            // Eat whitespace between text tokens too
                            self.bump();
                        }
                        self.builder.finish_node();
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn current_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.pos).map(|t| t.kind.clone())
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(self.source.len(), self.source.len(), 1, 1))
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current_kind()
            .is_some_and(|k| std::mem::discriminant(&k) == std::mem::discriminant(&kind))
    }

    fn at_identifier(&self, name: &str) -> bool {
        matches!(self.current_kind(), Some(TokenKind::Identifier(n)) if n == name)
    }

    fn at_pipe_in_union(&self) -> bool {
        self.at(TokenKind::VerticalBar)
    }

    fn is_ident(&self) -> bool {
        matches!(self.current_kind(), Some(TokenKind::Identifier(_)))
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.at(TokenKind::Eof)
    }

    fn peek_is(&self, kind: TokenKind) -> bool {
        // Skip trivia to find the next non-trivia token
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return std::mem::discriminant(&self.tokens[i].kind)
                    == std::mem::discriminant(&kind);
            }
            i += 1;
        }
        false
    }

    /// Get the nth non-trivia token kind after the current position (1-indexed).
    fn peek_nth_non_trivia_kind(&self, n: usize) -> Option<TokenKind> {
        let mut count = 0;
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                count += 1;
                if count == n {
                    return Some(self.tokens[i].kind.clone());
                }
            }
            i += 1;
        }
        None
    }

    fn next_non_trivia_kind(&self) -> Option<TokenKind> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return Some(self.tokens[i].kind.clone());
            }
            i += 1;
        }
        None
    }

    fn is_jsx_text_token(&self) -> bool {
        matches!(
            self.current_kind(),
            Some(TokenKind::Identifier(_))
                | Some(TokenKind::Number(_))
                | Some(TokenKind::String(_))
                | Some(TokenKind::Whitespace)
        )
    }

    fn is_uppercase_ident_at_checkpoint(&self) -> bool {
        // Walk backward through previously emitted tokens to find the last non-trivia
        // In practice, we need to check the expression that was just parsed.
        // The simplest heuristic: check if the previous non-trivia token was an uppercase ident.
        let mut i = self.pos.saturating_sub(1);
        loop {
            if i < self.tokens.len() && !self.tokens[i].kind.is_trivia() {
                return matches!(&self.tokens[i].kind, TokenKind::Identifier(name) if name.starts_with(char::is_uppercase));
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        false
    }

    /// Heuristic: is the current `(` a tuple type `(T, U)`?
    /// Has a comma at depth 1 and is NOT followed by `->`.
    fn is_paren_tuple_type(&self) -> bool {
        let mut depth = 0;
        let mut has_comma = false;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        if !has_comma {
                            return false;
                        }
                        // Find next non-trivia
                        let mut j = i + 1;
                        while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                            j += 1;
                        }
                        return !(j < self.tokens.len()
                            && self.tokens[j].kind == TokenKind::ThinArrow);
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

    /// Heuristic: is the current `(` in `const (a, b) = ...` a tuple destructuring?
    /// Check that `)` is followed by `=` or `:`.
    fn is_const_tuple_destructuring(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Find next non-trivia
                        let mut j = i + 1;
                        while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                            j += 1;
                        }
                        return j < self.tokens.len()
                            && matches!(self.tokens[j].kind, TokenKind::Equal | TokenKind::Colon);
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` a tuple expression `(a, b)`?
    /// Scans to matching `)` and checks if there's a comma at depth 1.
    fn is_paren_tuple_expr(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        return false; // no comma found
                    }
                }
                TokenKind::Comma if depth == 1 => return true,
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Heuristic: is the current `(` the start of a unit type `()`?
    fn is_unit_type(&self) -> bool {
        self.peek_is(TokenKind::RightParen) && !self.peek_after_rparen_is(TokenKind::ThinArrow)
    }

    /// Heuristic: is the current `(` the start of a function type?
    fn is_function_type(&self) -> bool {
        self.scan_for_rparen_followed_by(TokenKind::ThinArrow)
    }

    /// Scan from current `(` to matching `)`, check if followed by `kind`.
    fn scan_for_rparen_followed_by(&self, kind: TokenKind) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::LeftParen => depth += 1,
                TokenKind::RightParen => {
                    depth -= 1;
                    if depth == 0 {
                        // Find next non-trivia
                        let mut j = i + 1;
                        while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                            j += 1;
                        }
                        return j < self.tokens.len()
                            && std::mem::discriminant(&self.tokens[j].kind)
                                == std::mem::discriminant(&kind);
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn peek_after_rparen_is(&self, kind: TokenKind) -> bool {
        // Find ) after current (, then check if followed by kind
        let mut i = self.pos + 1;
        // skip trivia
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        if i < self.tokens.len() && self.tokens[i].kind == TokenKind::RightParen {
            i += 1;
            while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
                i += 1;
            }
            return i < self.tokens.len()
                && std::mem::discriminant(&self.tokens[i].kind) == std::mem::discriminant(&kind);
        }
        false
    }

    /// Consume the current token, adding it to the green tree.
    fn bump(&mut self) {
        if self.pos < self.tokens.len() {
            let token = &self.tokens[self.pos];
            let syntax_kind = token_kind_to_syntax(&token.kind);
            let text = &self.source[token.span.start..token.span.end];
            self.builder.token(syntax_kind.into(), text);
            self.pos += 1;
        }
    }

    /// Consume trivia tokens (whitespace, comments).
    fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            self.bump();
        }
    }

    fn expect(&mut self, kind: TokenKind) {
        if self.at(kind.clone()) {
            self.bump();
        } else {
            self.error(&format!(
                "expected {:?}, found {:?}",
                kind,
                self.current_kind()
            ));
        }
    }

    fn expect_kind(&mut self, kind: TokenKind) {
        if self.at(kind.clone()) {
            self.bump();
        } else {
            self.error(&format!(
                "expected {:?}, found {:?}",
                kind,
                self.current_kind()
            ));
        }
    }

    fn expect_ident(&mut self) {
        if self.is_ident() {
            self.bump();
        } else {
            self.error(&format!(
                "expected identifier, found {:?}",
                self.current_kind()
            ));
        }
    }

    fn expect_ident_item(&mut self) {
        self.expect_ident();
    }

    fn error(&mut self, message: &str) {
        self.errors.push(CstError {
            message: message.to_string(),
            span: self.current_span(),
        });
    }

    fn parse_comma_separated(&mut self, parse_fn: fn(&mut Self), closing: TokenKind) {
        if self.at(closing.clone()) {
            return;
        }

        parse_fn(self);
        self.eat_trivia();

        while self.at(TokenKind::Comma) {
            self.bump();
            self.eat_trivia();
            if self.at(closing.clone()) {
                break;
            }
            parse_fn(self);
            self.eat_trivia();
        }
    }
}

impl TokenKind {
    fn is_trivia(&self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace | TokenKind::Comment | TokenKind::BlockComment
        )
    }
}
