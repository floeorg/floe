use super::*;

impl<'src> CstParser<'src> {
    // ── Items ────────────────────────────────────────────────────

    pub(super) fn parse_item(&mut self) {
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
            Some(TokenKind::LeftBrace) if exported => {
                // `export { X, Y } from "module"` — re-export
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_reexport();
                self.builder.finish_node();
            }
            Some(TokenKind::Const) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_const_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Fn) if !self.peek_is(TokenKind::LeftParen) => {
                // `fn name(...)` is a function declaration; `fn(...)` is a lambda expression
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
                self.parse_for_block();
                self.builder.finish_node();
            }
            Some(TokenKind::Trait) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_trait_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Use) if !exported => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_use_decl();
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

        // `import trusted { ... }` — module-level trusted
        if self.at(TokenKind::Trusted) {
            self.bump(); // trusted
            self.eat_trivia();
        }

        if self.at(TokenKind::LeftBrace) {
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_import_specifier_or_for, TokenKind::RightBrace);
            self.expect(TokenKind::RightBrace);
            self.eat_trivia();
        } else if matches!(self.current_kind(), Some(TokenKind::Identifier(_)))
            && !self.at(TokenKind::From)
        {
            // Default import: `import [trusted] Ident from "..."`
            self.builder.start_node(SyntaxKind::IMPORT_SPECIFIER.into());
            self.bump(); // ident
            self.builder.finish_node();
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

    // ── Re-export ─────────────────────────────────────────────────

    fn parse_reexport(&mut self) {
        self.builder.start_node(SyntaxKind::REEXPORT_DECL.into());
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_reexport_specifier, TokenKind::RightBrace);
        self.expect(TokenKind::RightBrace);
        self.eat_trivia();

        if self.at(TokenKind::From) {
            self.bump();
            self.eat_trivia();
        }
        self.expect_kind(TokenKind::String("".into()));

        self.builder.finish_node();
    }

    fn parse_reexport_specifier(&mut self) {
        self.builder
            .start_node(SyntaxKind::REEXPORT_SPECIFIER.into());
        self.expect_ident();
        self.eat_trivia();

        // Check for `as alias`
        if self.at_identifier("as")
            || self.at(TokenKind::Banned(crate::lexer::token::BannedKeyword::As))
        {
            self.bump();
            self.eat_trivia();
            self.expect_ident();
        }

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
        // `trusted foo` — per-specifier trusted
        if self.at(TokenKind::Trusted) && self.peek_is_ident() {
            self.bump(); // trusted
            self.eat_trivia();
        }
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
            // Object destructuring: { a, b } or { a: x, b: y }
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_destructure_field, TokenKind::RightBrace);
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

    // ── Use Declaration ─────────────────────────────────────────

    fn parse_use_decl(&mut self) {
        self.builder.start_node(SyntaxKind::USE_DECL.into());
        self.expect(TokenKind::Use);
        self.eat_trivia();

        // Optional binding: `use x <- expr` or `use (a, b) <- expr` or `use <- expr`
        if !self.at(TokenKind::LeftArrow) {
            if self.at(TokenKind::LeftParen) {
                // Tuple destructuring: `use (a, b) <- expr`
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightParen);
                self.expect(TokenKind::RightParen);
            } else {
                self.expect_ident();
            }
            self.eat_trivia();
        }

        self.expect(TokenKind::LeftArrow);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }

    // ── Function Declaration ────────────────────────────────────

    fn parse_function_decl(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

        self.expect(TokenKind::Fn);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        // Optional type parameters: <T, U>
        if self.at(TokenKind::LessThan) {
            self.bump(); // <
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::GreaterThan);
            self.expect(TokenKind::GreaterThan);
            self.eat_trivia();
        }

        if self.at(TokenKind::Equal) {
            // `fn name = expr` — derived function binding (pointfree style)
            self.bump(); // =
            self.eat_trivia();
            self.parse_expr();
        } else {
            // `fn name(params) { body }` — standard function declaration
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
        }

        self.builder.finish_node();
    }

    pub(super) fn parse_param(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM.into());

        if self.at(TokenKind::LeftBrace) {
            // Destructured param: { name, age } or { name: n, age: a }
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_destructure_field, TokenKind::RightBrace);
            self.expect(TokenKind::RightBrace);
            self.eat_trivia();
        } else if self.at(TokenKind::LeftParen) {
            // Tuple destructured param: (a, b)
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
            self.eat_trivia();
        } else if self.at(TokenKind::SelfKw) {
            self.bump(); // self
            self.eat_trivia();
        } else if self.at(TokenKind::Underscore) {
            self.bump(); // _
            self.eat_trivia();
        } else {
            self.expect_ident();
            self.eat_trivia();
        }

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

        // New syntax: `type Name { ... }` for records/unions/newtypes
        //            `type Name(Type)` for newtypes with paren syntax
        // Old syntax: `type Name = ...` for aliases and string literal unions
        if self.at(TokenKind::LeftBrace) {
            self.parse_type_body_in_braces();
        } else if self.at(TokenKind::LeftParen) {
            // Newtype with paren syntax: `type UserId(string)`
            self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_variant_field, TokenKind::RightParen);
            self.expect(TokenKind::RightParen);
            self.builder.finish_node();
        } else {
            self.expect(TokenKind::Equal);
            self.eat_trivia();
            self.parse_type_def_after_eq();
        }

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

    /// Parse type body inside `{ }`: disambiguate between record, union, and newtype.
    fn parse_type_body_in_braces(&mut self) {
        // Peek at first non-trivia token inside `{` to disambiguate:
        // - `|` → union variants
        // - lowercase ident + `:` → record fields
        // - `...` → record fields (spread)
        // - `}` → empty record
        // - anything else → newtype wrapper
        let first_inside = self.peek_inside_brace();

        match first_inside {
            Some(TokenKind::VerticalBar) => {
                self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
                self.bump(); // {
                self.eat_trivia();
                self.parse_union_variants_inner();
                self.expect(TokenKind::RightBrace);
                self.builder.finish_node();
            }
            Some(TokenKind::DotDotDot) => {
                // Record with spread
                self.builder.start_node(SyntaxKind::TYPE_DEF_RECORD.into());
                self.parse_record_fields();
                self.builder.finish_node();
            }
            Some(TokenKind::Identifier(name)) if name.starts_with(char::is_lowercase) => {
                // Peek further: if followed by `:`, it's a record field.
                // Otherwise it's a newtype (e.g. `type OrderId { number }`)
                if self.peek_inside_brace_second() == Some(TokenKind::Colon) {
                    self.builder.start_node(SyntaxKind::TYPE_DEF_RECORD.into());
                    self.parse_record_fields();
                    self.builder.finish_node();
                } else {
                    // Newtype wrapping a lowercase type like `number`, `string`, `boolean`
                    self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
                    self.bump(); // {
                    self.eat_trivia();
                    self.builder.start_node(SyntaxKind::VARIANT_FIELD.into());
                    self.parse_type_expr();
                    self.builder.finish_node();
                    self.eat_trivia();
                    self.expect(TokenKind::RightBrace);
                    self.builder.finish_node();
                }
            }
            Some(TokenKind::RightBrace) => {
                // Empty record: `type Foo {}`
                self.builder.start_node(SyntaxKind::TYPE_DEF_RECORD.into());
                self.parse_record_fields();
                self.builder.finish_node();
            }
            _ => {
                // Newtype: `type OrderId { number }`
                // Parse as single-variant union matching the type name
                self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
                self.bump(); // {
                self.eat_trivia();
                // Synthesize a variant with the type's name — the lowerer
                // will pick up the inner type expression as a variant field
                self.builder.start_node(SyntaxKind::VARIANT_FIELD.into());
                self.parse_type_expr();
                self.builder.finish_node();
                self.eat_trivia();
                self.expect(TokenKind::RightBrace);
                self.builder.finish_node();
            }
        }
    }

    /// Peek at the first non-trivia token after the current `{`.
    fn peek_inside_brace(&self) -> Option<TokenKind> {
        let mut i = self.pos + 1;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                return Some(self.tokens[i].kind.clone());
            }
            i += 1;
        }
        None
    }

    /// Peek at the second non-trivia token after the current `{`.
    fn peek_inside_brace_second(&self) -> Option<TokenKind> {
        let mut i = self.pos + 1;
        let mut count = 0;
        while i < self.tokens.len() {
            if !self.tokens[i].kind.is_trivia() {
                count += 1;
                if count == 2 {
                    return Some(self.tokens[i].kind.clone());
                }
            }
            i += 1;
        }
        None
    }

    /// Parse after `=`: aliases and string literal unions only.
    fn parse_type_def_after_eq(&mut self) {
        if self.at_string_literal_union() {
            self.parse_string_literal_union();
        } else {
            self.builder.start_node(SyntaxKind::TYPE_DEF_ALIAS.into());
            self.parse_type_expr();
            self.builder.finish_node();
        }
    }

    /// Parse union variants inside `{ }`. The `{` is already consumed, `}` is consumed by caller.
    fn parse_union_variants_inner(&mut self) {
        while self.at_pipe_in_union() {
            self.builder.start_node(SyntaxKind::VARIANT.into());
            self.bump(); // |
            self.eat_trivia();
            self.expect_ident();
            self.eat_trivia();

            // Variant fields: { named } or (positional)
            if self.at(TokenKind::LeftBrace) {
                self.bump(); // {
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_variant_field, TokenKind::RightBrace);
                self.expect(TokenKind::RightBrace);
                self.eat_trivia();
            } else if self.at(TokenKind::LeftParen) {
                self.bump(); // (
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_variant_field, TokenKind::RightParen);
                self.expect(TokenKind::RightParen);
                self.eat_trivia();
            }

            self.builder.finish_node();
        }
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

    pub(super) fn parse_record_fields(&mut self) {
        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_record_entry, TokenKind::RightBrace);
        self.expect(TokenKind::RightBrace);
    }

    fn parse_record_entry(&mut self) {
        // Check for spread: `...TypeName` or `...Generic<T>`
        if self.at(TokenKind::DotDotDot) {
            self.builder.start_node(SyntaxKind::RECORD_SPREAD.into());
            self.bump(); // consume `...`
            self.eat_trivia();
            self.parse_type_expr();
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

    /// Parse a for-block: `for Type { fn ... }`.
    fn parse_for_block(&mut self) {
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

        self.expect(TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse function declarations inside the block (with optional export)
        while !self.at(TokenKind::RightBrace) && !self.at_end() {
            if self.at(TokenKind::Export) {
                self.bump();
                self.eat_trivia();
            }
            if self.at(TokenKind::Fn) {
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
            let prev_pos = self.pos;
            if self.at(TokenKind::Assert) {
                self.parse_assert_stmt();
            } else {
                self.parse_expr();
            }
            self.eat_trivia();
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
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
}
