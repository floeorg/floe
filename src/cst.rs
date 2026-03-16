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
            Some(TokenKind::For) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_for_block_or_inline();
                self.builder.finish_node();
            }
            Some(TokenKind::Trait) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_trait_decl();
                self.builder.finish_node();
            }
            _ if !exported && self.at_identifier("test") && self.peek_is_string() => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_test_block();
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
            self.parse_comma_separated(Self::parse_import_specifier_or_for, TokenKind::RightBrace);
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

    /// Parse either a regular import specifier or a `for Type` import specifier.
    fn parse_import_specifier_or_for(&mut self) {
        if self.at(TokenKind::For) {
            // `for Type` import specifier
            self.builder
                .start_node(SyntaxKind::IMPORT_FOR_SPECIFIER.into());
            self.bump(); // `for`
            self.eat_trivia();
            self.expect_ident(); // type name
            self.builder.finish_node();
        } else {
            self.parse_import_specifier();
        }
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

        // Optional deriving clause: `deriving (Display)`
        self.eat_trivia();
        if self.at(TokenKind::Deriving) {
            self.builder.start_node(SyntaxKind::DERIVING_CLAUSE.into());
            self.bump(); // consume `deriving`
            self.eat_trivia();
            self.expect(TokenKind::LeftParen);
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
            self.builder.finish_node();
        }

        self.builder.finish_node();
    }

    fn parse_type_def(&mut self) {
        if self.at_pipe_in_union() {
            self.parse_union_variants();
        } else if self.at_string_literal_union() {
            self.parse_string_literal_union();
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

    fn parse_string_literal_union(&mut self) {
        self.builder
            .start_node(SyntaxKind::TYPE_DEF_STRING_UNION.into());

        // First string literal
        self.bump(); // string
        self.eat_trivia();

        // Parse remaining `| "string"` pairs
        while self.at(TokenKind::VerticalBar) {
            self.bump(); // |
            self.eat_trivia();
            if self.at(TokenKind::String("".into())) {
                self.bump(); // string
                self.eat_trivia();
            } else {
                self.error("expected string literal after `|` in string literal union");
                break;
            }
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
        self.parse_comma_separated(Self::parse_record_entry, TokenKind::RightBrace);
        self.expect(TokenKind::RightBrace);
    }

    fn parse_record_entry(&mut self) {
        // Check for spread: `...TypeName`
        if self.at(TokenKind::DotDotDot) {
            self.builder.start_node(SyntaxKind::RECORD_SPREAD.into());
            self.bump(); // consume `...`
            self.eat_trivia();
            self.expect_ident(); // TypeName
            self.builder.finish_node();
            return;
        }

        self.parse_record_field();
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

    /// Parse either a for-block (`for Type { ... }`) or inline for-declaration
    /// (`[export] for Type fn ...`).
    fn parse_for_block_or_inline(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_BLOCK.into());

        self.expect(TokenKind::For);
        self.eat_trivia();

        // Parse the type name (e.g., `User`, `Array<T>`)
        self.parse_type_expr();
        self.eat_trivia();

        // Optional trait bound: `for User: Display { ... }`
        if self.at(TokenKind::Colon) {
            self.bump(); // :
            self.eat_trivia();
            self.expect_ident(); // trait name
            self.eat_trivia();
        }

        // Inline form: `[export] for Type fn name(...) { ... }`
        if self.at(TokenKind::Fn) || self.at(TokenKind::Async) {
            self.parse_for_block_function();
            self.builder.finish_node();
            return;
        }

        // Block form: `for Type { ... }`
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

    // ── Trait Declarations ────────────────────────────────────────

    fn parse_trait_decl(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_DECL.into());

        self.expect(TokenKind::Trait);
        self.eat_trivia();

        self.expect_ident(); // trait name
        self.eat_trivia();

        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse method declarations inside the trait
        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            if self.at(TokenKind::Fn) {
                self.parse_trait_method();
                self.eat_trivia();
            } else {
                self.error("expected `fn` inside trait");
                self.bump();
                self.eat_trivia();
            }
        }

        self.expect(TokenKind::RightBrace);

        self.builder.finish_node();
    }

    fn parse_trait_method(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

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

        // Optional body (default implementation)
        if self.at(TokenKind::LeftBrace) {
            self.parse_block_expr();
        }

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

    // ── Test Blocks ──────────────────────────────────────────────

    fn parse_test_block(&mut self) {
        self.builder.start_node(SyntaxKind::TEST_BLOCK.into());

        // `test` is a contextual keyword (an identifier)
        self.bump(); // consume "test" identifier
        self.eat_trivia();

        // Test name (string literal)
        self.expect_kind(TokenKind::String("".into()));
        self.eat_trivia();

        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse test body: assert statements and expressions
        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            if self.at(TokenKind::Assert) {
                self.parse_assert_stmt();
            } else {
                self.parse_expr();
            }
            self.eat_trivia();
        }

        self.expect(TokenKind::RightBrace);

        self.builder.finish_node();
    }

    fn parse_assert_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ASSERT_EXPR.into());

        self.expect(TokenKind::Assert);
        self.eat_trivia();
        self.parse_expr();

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

            // Pipe into match: `x |> match { ... }`
            if self.at(TokenKind::Match) {
                self.parse_subjectless_match_expr();
            } else {
                self.parse_or_expr();
            }

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
            Some(TokenKind::Collect) => {
                self.builder.start_node(SyntaxKind::COLLECT_EXPR.into());
                self.bump(); // collect
                self.eat_trivia();
                self.parse_block_expr();
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

    /// Parse `match { arms }` without a subject — used for `x |> match { ... }`.
    fn parse_subjectless_match_expr(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_EXPR.into());
        self.expect(TokenKind::Match);
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
            Some(TokenKind::LeftBracket) => {
                // Array pattern: [], [a, b], [first, ..rest]
                self.bump(); // [
                self.eat_trivia();
                // Parse elements and optional rest pattern
                while !self.at(TokenKind::RightBracket) && !self.at_end() {
                    // Check for rest pattern: ..name
                    if self.at(TokenKind::DotDot) {
                        self.bump(); // ..
                        self.eat_trivia();
                        // Expect identifier or _ after ..
                        if matches!(
                            self.current_kind(),
                            Some(TokenKind::Identifier(_)) | Some(TokenKind::Underscore)
                        ) {
                            self.bump();
                        } else {
                            self.error("expected identifier after '..' in array pattern");
                        }
                        self.eat_trivia();
                        if self.at(TokenKind::Comma) {
                            self.bump();
                            self.eat_trivia();
                        }
                        break;
                    }
                    self.parse_pattern();
                    self.eat_trivia();
                    if self.at(TokenKind::Comma) {
                        self.bump();
                        self.eat_trivia();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RightBracket);
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

    fn peek_is_string(&self) -> bool {
        // Look ahead past trivia to find the next non-trivia token
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            let kind = &self.tokens[i].kind;
            if matches!(
                kind,
                TokenKind::Whitespace | TokenKind::Comment | TokenKind::BlockComment
            ) {
                i += 1;
                continue;
            }
            return matches!(kind, TokenKind::String(_));
        }
        false
    }

    fn at_pipe_in_union(&self) -> bool {
        self.at(TokenKind::VerticalBar)
    }

    /// Check if we're at a string literal union: `"A" | "B" | ...`
    /// This is true when the current token is a string and the next non-trivia token is `|`.
    fn at_string_literal_union(&self) -> bool {
        self.at(TokenKind::String("".into()))
            && matches!(
                self.peek_nth_non_trivia_kind(1),
                Some(TokenKind::VerticalBar)
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::syntax::SyntaxKind;

    /// Helper: parse source through CstParser and return the Parse result.
    fn cst_parse(source: &str) -> Parse {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        CstParser::new(source, tokens).parse()
    }

    /// Helper: assert the CST text round-trips exactly.
    fn assert_lossless(source: &str) {
        let parse = cst_parse(source);
        assert_eq!(
            parse.syntax().text().to_string(),
            source,
            "CST text should match original source"
        );
    }

    /// Helper: assert no CST errors.
    fn assert_no_errors(source: &str) -> Parse {
        let parse = cst_parse(source);
        assert!(
            parse.errors.is_empty(),
            "unexpected CST errors: {:?}",
            parse.errors
        );
        parse
    }

    // ── Const declarations ────────────────────────────────────────

    #[test]
    fn const_simple() {
        assert_no_errors("const x = 42");
    }

    #[test]
    fn const_typed() {
        assert_no_errors("const x: number = 42");
    }

    #[test]
    fn const_exported() {
        assert_no_errors("export const name = \"hello\"");
    }

    #[test]
    fn const_string_value() {
        assert_no_errors("const greeting = \"world\"");
    }

    #[test]
    fn const_bool_value() {
        assert_no_errors("const flag = true");
    }

    // ── Function declarations ─────────────────────────────────────

    #[test]
    fn function_no_params() {
        assert_no_errors("fn greet() { 42 }");
    }

    #[test]
    fn function_with_params() {
        assert_no_errors("fn add(a: number, b: number) -> number { a + b }");
    }

    #[test]
    fn function_async() {
        assert_no_errors("async fn fetch(url: string) -> string { url }");
    }

    #[test]
    fn function_exported() {
        assert_no_errors("export fn hello() { 1 }");
    }

    // ── Imports ───────────────────────────────────────────────────

    #[test]
    fn import_bare() {
        assert_no_errors("import \"./module\"");
    }

    #[test]
    fn import_with_specifiers() {
        assert_no_errors("import { foo, bar } from \"./module\"");
    }

    #[test]
    fn import_aliased() {
        // "as" is a banned keyword but allowed contextually in imports
        let parse = cst_parse("import { foo as f } from \"./module\"");
        // Should have at most an error for "as" being banned, but still parses
        let text = parse.syntax().text().to_string();
        assert_eq!(text, "import { foo as f } from \"./module\"");
    }

    #[test]
    fn import_for_specifier() {
        assert_no_errors("import { for User } from \"./helpers\"");
    }

    // ── Exports ───────────────────────────────────────────────────

    #[test]
    fn export_function() {
        assert_no_errors("export fn myFunc() { 1 }");
    }

    #[test]
    fn export_type() {
        assert_no_errors("export type Color = | Red | Green | Blue");
    }

    // ── Type declarations ─────────────────────────────────────────

    #[test]
    fn type_record() {
        assert_no_errors("type User = { name: string, age: number }");
    }

    #[test]
    fn type_union() {
        assert_no_errors("type Color = | Red | Green | Blue");
    }

    #[test]
    fn type_string_literal_union() {
        assert_no_errors(r#"type HttpMethod = "GET" | "POST" | "PUT" | "DELETE""#);
    }

    #[test]
    fn type_string_literal_union_two() {
        assert_no_errors(r#"type Status = "ok" | "error""#);
    }

    #[test]
    fn type_alias() {
        assert_no_errors("type Name = string");
    }

    #[test]
    fn type_opaque() {
        assert_no_errors("opaque type Id = string");
    }

    #[test]
    fn type_generic() {
        assert_no_errors("type Box<T> = { value: T }");
    }

    #[test]
    fn type_exported() {
        assert_no_errors("export type Point = { x: number, y: number }");
    }

    // ── Expressions ───────────────────────────────────────────────

    #[test]
    fn binary_add() {
        assert_no_errors("1 + 2");
    }

    #[test]
    fn binary_comparison() {
        assert_no_errors("a == b");
    }

    #[test]
    fn unary_not() {
        assert_no_errors("!flag");
    }

    #[test]
    fn unary_neg() {
        assert_no_errors("-42");
    }

    #[test]
    fn call_expr() {
        assert_no_errors("f(a, b)");
    }

    #[test]
    fn member_access() {
        assert_no_errors("user.name");
    }

    #[test]
    fn constructor_simple() {
        assert_no_errors("User(name: \"Alice\")");
    }

    #[test]
    fn ok_expr() {
        assert_no_errors("Ok(42)");
    }

    #[test]
    fn err_expr() {
        assert_no_errors("Err(\"fail\")");
    }

    #[test]
    fn some_expr() {
        assert_no_errors("Some(1)");
    }

    #[test]
    fn none_expr() {
        assert_no_errors("None");
    }

    #[test]
    fn return_is_banned() {
        // `return` should produce a banned keyword error
        let parse = cst_parse("fn f() { return 42 }");
        assert!(
            parse.errors.iter().any(|e| e.message.contains("banned")),
            "expected banned keyword error for return, got: {:?}",
            parse.errors
        );
    }

    #[test]
    fn array_literal() {
        assert_no_errors("[1, 2, 3]");
    }

    #[test]
    fn tuple_literal() {
        assert_no_errors("(1, 2)");
    }

    // ── Pipe expressions ──────────────────────────────────────────

    #[test]
    fn pipe_simple() {
        assert_no_errors("x |> f(y, _)");
    }

    #[test]
    fn pipe_chain() {
        assert_no_errors("data |> filter(.done) |> map(.name)");
    }

    // ── Match expressions ─────────────────────────────────────────

    #[test]
    fn match_basic() {
        assert_no_errors("match x { Ok(v) -> v, Err(e) -> e }");
    }

    #[test]
    fn match_wildcard() {
        assert_no_errors("match x { _ -> 0 }");
    }

    #[test]
    fn match_guard() {
        assert_no_errors("match x { n when n > 0 -> n, _ -> 0 }");
    }

    // ── JSX ───────────────────────────────────────────────────────

    #[test]
    fn jsx_self_closing() {
        assert_no_errors("<Input />");
    }

    #[test]
    fn jsx_with_children() {
        assert_no_errors("<div>hello</div>");
    }

    #[test]
    fn jsx_with_props() {
        assert_no_errors("<Button onClick={handler} />");
    }

    // ── Lambda / arrow functions ──────────────────────────────────

    #[test]
    fn lambda_pipe_style() {
        assert_no_errors("|x| x + 1");
    }

    #[test]
    fn lambda_zero_arg() {
        assert_no_errors("|| 42");
    }

    // ── For blocks ────────────────────────────────────────────────

    #[test]
    fn for_block_basic() {
        assert_no_errors("for User { fn greet(self) -> string { self.name } }");
    }

    #[test]
    fn for_block_with_trait() {
        assert_no_errors("for User: Display { fn show(self) -> string { self.name } }");
    }

    // ── Trait declarations ────────────────────────────────────────

    #[test]
    fn trait_basic() {
        assert_no_errors("trait Display { fn show(self) -> string }");
    }

    // ── Test blocks ───────────────────────────────────────────────

    #[test]
    fn test_block_basic() {
        assert_no_errors("test \"my test\" { assert 1 == 1 }");
    }

    // ── Trivia preservation ───────────────────────────────────────

    #[test]
    fn trivia_comments_preserved() {
        assert_lossless("// comment\nconst x = 1");
    }

    #[test]
    fn trivia_whitespace_preserved() {
        assert_lossless("const  x  =  1");
    }

    #[test]
    fn trivia_block_comment_preserved() {
        assert_lossless("/* block */ const x = 1");
    }

    // ── Error recovery ────────────────────────────────────────────

    #[test]
    fn error_recovery_missing_equal() {
        // Should not panic, produces CST errors
        let parse = cst_parse("const x 42");
        assert!(!parse.errors.is_empty());
    }

    #[test]
    fn error_recovery_malformed_function() {
        let parse = cst_parse("fn () { }");
        assert!(!parse.errors.is_empty());
    }

    #[test]
    fn error_recovery_empty_input() {
        let parse = cst_parse("");
        assert!(parse.errors.is_empty());
        assert_lossless("");
    }

    #[test]
    fn error_recovery_random_tokens() {
        // Should not panic regardless of input
        let _ = cst_parse("!@#$%^");
        let _ = cst_parse("}{)(][");
        let _ = cst_parse(";;; , , ,");
    }

    // ── Lossless round-trips ──────────────────────────────────────

    #[test]
    fn lossless_const() {
        assert_lossless("const x = 42");
    }

    #[test]
    fn lossless_function() {
        assert_lossless("fn add(a: number, b: number) -> number { a + b }");
    }

    #[test]
    fn lossless_import() {
        assert_lossless("import { foo, bar } from \"./module\"");
    }

    #[test]
    fn lossless_match() {
        assert_lossless("match x { Ok(v) -> v, _ -> 0 }");
    }

    #[test]
    fn lossless_jsx() {
        assert_lossless("<div>hello</div>");
    }

    #[test]
    fn lossless_pipe() {
        assert_lossless("x |> f(y, _)");
    }

    #[test]
    fn lossless_for_block() {
        assert_lossless("for User { fn greet(self) -> string { self.name } }");
    }

    // ── CST node kind checks ──────────────────────────────────────

    #[test]
    fn root_is_program() {
        let parse = cst_parse("const x = 1");
        assert_eq!(parse.syntax().kind(), SyntaxKind::PROGRAM);
    }

    #[test]
    fn has_item_children() {
        let parse = cst_parse("const x = 1\nconst y = 2");
        let items: Vec<_> = parse
            .syntax()
            .children()
            .filter(|c| c.kind() == SyntaxKind::ITEM)
            .collect();
        assert_eq!(items.len(), 2);
    }
}
