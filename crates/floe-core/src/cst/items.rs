use super::{CstParser, SyntaxKind, TokenKind};
use crate::lexer::token::BannedKeyword;

impl<'src> CstParser<'src> {
    // ── Items ────────────────────────────────────────────────────

    #[allow(clippy::too_many_lines)]
    pub(super) fn parse_item(&mut self) {
        let checkpoint = self.builder.checkpoint();

        // Handle export prefix
        let exported = self.at(&TokenKind::Export);
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
            // `default` arrives as an Identifier (not a lexer keyword); the
            // bare-identifier form is allowed, anything else is rejected so
            // the TS anonymous-default foot-guns (`export default <expr>`,
            // `export default { ... }`) can never stick.
            _ if exported && self.at_identifier("default") && self.peek_is_ident() => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_default_export();
                self.builder.finish_node();
            }
            _ if exported && self.at_identifier("default") => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ERROR.into());
                self.error(
                    "`export default` must be followed by a named binding; \
                     bind the value first with `let name = ...` and write `export default name`",
                );
                self.bump();
                while !self.at_end() && !self.preceded_by_newline() {
                    self.bump();
                }
                self.builder.finish_node();
            }
            // Give a specific diagnostic for `export function` / `export class`
            // instead of the generic "expected declaration after 'export'".
            _ if exported
                && matches!(
                    self.current_kind(),
                    Some(TokenKind::Banned(
                        BannedKeyword::Function | BannedKeyword::Class
                    ))
                ) =>
            {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ERROR.into());
                self.error(
                    "`export function` and `export class` are not supported; \
                     use `let name = ...` then `export default name`",
                );
                self.bump();
                self.builder.finish_node();
            }
            Some(TokenKind::Let) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_const_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Async) => {
                // `async let name = ...` — async function binding. The
                // `async` token must live inside the FUNCTION_DECL node so
                // `lower_function` picks it up; open the item first, then
                // let `parse_const_decl` absorb the `async` prefix.
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_const_decl();
                self.builder.finish_node();
            }
            Some(TokenKind::Opaque | TokenKind::Type | TokenKind::Typealias) => {
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
            Some(TokenKind::Impl) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_impl_block();
                self.builder.finish_node();
            }
            Some(TokenKind::Trait) => {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_trait_decl();
                self.builder.finish_node();
            }
            _ if !exported && self.is_use_bind_start() => {
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
        self.expect(&TokenKind::Import);
        self.eat_trivia();

        // `import trusted { ... }` — module-level trusted
        if self.at(&TokenKind::Trusted) {
            self.bump(); // trusted
            self.eat_trivia();
        }

        if self.at(&TokenKind::LeftBrace) {
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_import_specifier_or_for, &TokenKind::RightBrace);
            self.expect(&TokenKind::RightBrace);
            self.eat_trivia();
        } else if matches!(self.current_kind(), Some(TokenKind::Identifier(_)))
            && !self.at(&TokenKind::From)
        {
            // Default import: `import [trusted] Ident from "..."`
            self.builder.start_node(SyntaxKind::IMPORT_SPECIFIER.into());
            self.bump(); // ident
            self.builder.finish_node();
            self.eat_trivia();
        }

        // `from` is required with specifiers, optional for bare imports
        if self.at(&TokenKind::From) {
            self.bump();
            self.eat_trivia();
        }
        self.expect_kind(&TokenKind::String(String::new()));

        self.builder.finish_node();
    }

    // ── Re-export ─────────────────────────────────────────────────

    fn parse_reexport(&mut self) {
        self.builder.start_node(SyntaxKind::REEXPORT_DECL.into());
        self.expect(&TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_reexport_specifier, &TokenKind::RightBrace);
        self.expect(&TokenKind::RightBrace);
        self.eat_trivia();

        self.expect(&TokenKind::From);
        self.eat_trivia();
        self.expect_kind(&TokenKind::String(String::new()));

        self.builder.finish_node();
    }

    // ── Default Export ────────────────────────────────────────────

    /// Parse the body of `export default <ident>` (the `export` keyword is
    /// already consumed). `default` is remapped to `KW_DEFAULT` so the
    /// lowerer can distinguish it from the binding identifier that follows.
    fn parse_default_export(&mut self) {
        self.builder
            .start_node(SyntaxKind::DEFAULT_EXPORT_DECL.into());

        self.bump_remap(SyntaxKind::KW_DEFAULT);
        self.eat_trivia();
        self.expect_ident();

        self.builder.finish_node();
    }

    fn parse_reexport_specifier(&mut self) {
        self.builder
            .start_node(SyntaxKind::REEXPORT_SPECIFIER.into());
        self.expect_ident();
        self.eat_trivia();

        // Check for `as alias`
        if self.at_identifier("as")
            || self.at(&TokenKind::Banned(crate::lexer::token::BannedKeyword::As))
        {
            self.bump();
            self.eat_trivia();
            self.expect_ident();
        }

        self.builder.finish_node();
    }

    /// Parse an import specifier. Two forms:
    /// - `{ Name }` or `{ Name as Alias }` — import the named value/type/trait
    /// - `{ for Type }` — import the for-block extension methods for Type
    ///   (needed for foreign-type inherent like `for Array<T>` or `for string`)
    fn parse_import_specifier_or_for(&mut self) {
        if self.at(&TokenKind::For) {
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
        if self.at(&TokenKind::Trusted) && self.peek_is_ident() {
            self.bump(); // trusted
            self.eat_trivia();
        }
        self.expect_ident();
        self.eat_trivia();

        // Check for `as alias` — "as" is a banned keyword but used contextually here
        if self.at_identifier("as")
            || self.at(&TokenKind::Banned(crate::lexer::token::BannedKeyword::As))
        {
            self.bump();
            self.eat_trivia();
            self.expect_ident();
        }

        self.builder.finish_node();
    }

    // ── Const Declaration ────────────────────────────────────────

    fn parse_const_decl(&mut self) {
        // `let NAME = ...` has two shapes: value binding or function binding.
        // If the RHS looks like a function literal (generics, or parens + body),
        // lift it into a FUNCTION_DECL CST so existing checker machinery
        // (default params, generic type params) continues to apply.
        let checkpoint = self.builder.checkpoint();
        // Optional `async` prefix — only legal when the RHS is a function
        // binding; the checkpoint-replay below drops it into FUNCTION_DECL.
        if self.at(&TokenKind::Async) {
            self.bump();
            self.eat_trivia();
        }
        self.expect(&TokenKind::Let);
        self.eat_trivia();

        if self.looks_like_let_function_binding() {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::FUNCTION_DECL.into());
            self.expect_ident_flex();
            self.eat_trivia();
            self.parse_let_function_body();
            self.builder.finish_node();
            return;
        }

        self.builder
            .start_node_at(checkpoint, SyntaxKind::CONST_DECL.into());

        if self.at(&TokenKind::LeftBracket) {
            // Array destructure `[a, b]` is not a valid const binding: on
            // arrays the runtime length isn't in the type, and on tuples it
            // hides the real shape. Use `(a, b)` for tuples or `Array.get` /
            // `match` for arrays.
            self.error("expected identifier, `{`, or `(`");
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_flex_item, &TokenKind::RightBracket);
            self.expect(&TokenKind::RightBracket);
        } else if self.at(&TokenKind::LeftBrace) {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_destructure_field, &TokenKind::RightBrace);
            self.expect(&TokenKind::RightBrace);
        } else if self.at(&TokenKind::LeftParen) && self.is_const_tuple_destructuring() {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_flex_item, &TokenKind::RightParen);
            self.expect(&TokenKind::RightParen);
        } else {
            self.expect_ident_flex();
        }
        self.eat_trivia();

        if self.at(&TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.expect(&TokenKind::Equal);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }

    /// After `let`, detect def-form `NAME [<generics>] (params) ...` shape.
    /// In def-form, params follow immediately after the name (with optional
    /// generics in between) — no `=` between them.
    fn looks_like_let_function_binding(&self) -> bool {
        if !self.is_ident_flex() {
            return false;
        }
        let mut i = self.pos + 1;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        // Optional `<generics>` directly after the name
        if matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::LessThan)
        ) {
            i = self.skip_balanced(i + 1, |k| match k {
                TokenKind::LessThan => 1,
                TokenKind::GreaterThan => -1,
                _ => 0,
            });
            while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
                i += 1;
            }
        }
        // Def-form requires `(` immediately after name (or after generics)
        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::LeftParen)
        )
    }

    /// Parse def-form body: `[<generics>] (params) [-> Ret] = body`.
    fn parse_let_function_body(&mut self) {
        // Optional `<generics>`
        if self.at(&TokenKind::LessThan) {
            self.bump(); // <
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_type_param, &TokenKind::GreaterThan);
            self.expect(&TokenKind::GreaterThan);
            self.eat_trivia();
        }

        self.expect(&TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_param, &TokenKind::RightParen);
        self.expect(&TokenKind::RightParen);
        self.eat_trivia();

        // Optional `-> ReturnType`.
        if self.at(&TokenKind::ThinArrow) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.expect(&TokenKind::Equal);
        self.eat_trivia();

        if self.at(&TokenKind::LeftBrace) {
            self.parse_block_expr();
        } else {
            // Expression body — wrap into a synthetic BLOCK_EXPR { EXPR_ITEM }
            // so the lowerer treats it like a one-expression block.
            self.builder.start_node(SyntaxKind::BLOCK_EXPR.into());
            self.builder.start_node(SyntaxKind::EXPR_ITEM.into());
            self.parse_expr();
            self.builder.finish_node();
            self.builder.finish_node();
        }
    }

    // ── Use Declaration ─────────────────────────────────────────

    fn parse_use_decl(&mut self) {
        self.builder.start_node(SyntaxKind::USE_DECL.into());
        self.bump_remap(SyntaxKind::KW_USE);
        self.eat_trivia();

        if !self.at(&TokenKind::LeftArrow) {
            if self.at(&TokenKind::LeftParen) {
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::expect_ident_item, &TokenKind::RightParen);
                self.expect(&TokenKind::RightParen);
            } else if self.at(&TokenKind::LeftBrace) {
                self.bump();
                self.eat_trivia();
                self.parse_comma_separated(Self::parse_destructure_field, &TokenKind::RightBrace);
                self.expect(&TokenKind::RightBrace);
            } else {
                self.expect_ident();
            }
            self.eat_trivia();
        }

        self.expect(&TokenKind::LeftArrow);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }

    pub(super) fn parse_param(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM.into());

        if self.at(&TokenKind::LeftBrace) {
            // Destructured param: { name, age } or { name: n, age: a }
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_destructure_field, &TokenKind::RightBrace);
            self.expect(&TokenKind::RightBrace);
            self.eat_trivia();
        } else if self.at(&TokenKind::LeftParen) {
            // Tuple destructured param: (a, b)
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_flex_item, &TokenKind::RightParen);
            self.expect(&TokenKind::RightParen);
            self.eat_trivia();
        } else if self.at(&TokenKind::SelfKw) {
            self.bump(); // self
            self.eat_trivia();
        } else if self.at(&TokenKind::Underscore) {
            self.bump(); // _
            self.eat_trivia();
        } else {
            self.expect_ident_flex();
            self.eat_trivia();
        }

        if self.at(&TokenKind::Colon) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        if self.at(&TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── Type Declaration ────────────────────────────────────────

    fn parse_type_decl(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_DECL.into());

        let is_opaque = self.at(&TokenKind::Opaque);
        if is_opaque {
            self.bump();
            self.eat_trivia();
        }

        let is_typealias = self.at(&TokenKind::Typealias);
        if is_typealias {
            if is_opaque {
                self.error(
                    "`opaque typealias` is not supported; `opaque` only applies to nominal \
                     `type` declarations",
                );
            }
            self.bump(); // typealias
        } else {
            self.expect(&TokenKind::Type);
        }
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        // Optional type parameters: <T, U>
        if self.at(&TokenKind::LessThan) {
            self.bump();
            self.eat_trivia();
            self.parse_comma_separated(Self::expect_ident_item, &TokenKind::GreaterThan);
            self.expect(&TokenKind::GreaterThan);
            self.eat_trivia();
        }

        self.expect(&TokenKind::Equal);
        self.eat_trivia();
        let def_kind = self.parse_type_def_after_eq();

        // `opaque` is the explicit way to brand a structural shape as nominal,
        // so it's exempt from the type/typealias split.
        if !is_typealias && !is_opaque && def_kind == SyntaxKind::TYPE_DEF_ALIAS {
            self.error(
                "`type` declares a nominal type; this shape is structural. \
                 Use `typealias` instead — it names a shape without creating \
                 a new nominal type",
            );
        }
        if is_typealias && def_kind == SyntaxKind::TYPE_DEF_UNION {
            self.error(
                "`typealias` names a structural shape; tagged unions and \
                 newtypes need nominal identity. Use `type` instead",
            );
        }

        self.builder.finish_node();
    }

    fn parse_type_def_after_eq(&mut self) -> SyntaxKind {
        if self.at(&TokenKind::LeftBrace) {
            self.builder.start_node(SyntaxKind::TYPE_DEF_RECORD.into());
            self.parse_record_fields();
            self.builder.finish_node();
            return SyntaxKind::TYPE_DEF_RECORD;
        }

        if self.at(&TokenKind::VerticalBar) {
            self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
            self.parse_union_variants_inner();
            self.builder.finish_node();
            return SyntaxKind::TYPE_DEF_UNION;
        }

        if self.is_ident() && self.looks_like_nominal_sum_or_newtype() {
            self.builder.start_node(SyntaxKind::TYPE_DEF_UNION.into());
            self.parse_union_variants_inner();
            self.builder.finish_node();
            return SyntaxKind::TYPE_DEF_UNION;
        }

        if self.at_string_literal_union() {
            self.parse_string_literal_union();
            return SyntaxKind::TYPE_DEF_STRING_UNION;
        }

        self.builder.start_node(SyntaxKind::TYPE_DEF_ALIAS.into());
        self.parse_type_expr();
        self.builder.finish_node();
        SyntaxKind::TYPE_DEF_ALIAS
    }

    fn looks_like_nominal_sum_or_newtype(&self) -> bool {
        let Some(TokenKind::Identifier(name)) = self.current_kind() else {
            return false;
        };
        if !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return false;
        }

        let mut i = self.pos + 1;
        while i < self.tokens.len() && self.tokens[i].kind.is_trivia() {
            i += 1;
        }
        let Some(tok) = self.tokens.get(i).map(|t| t.kind.clone()) else {
            return false;
        };

        match tok {
            TokenKind::VerticalBar | TokenKind::LeftParen => true,
            TokenKind::LeftBrace => {
                let after = self.skip_balanced(i + 1, |k| match k {
                    TokenKind::LeftBrace => 1,
                    TokenKind::RightBrace => -1,
                    _ => 0,
                });
                let mut j = after;
                while j < self.tokens.len() && self.tokens[j].kind.is_trivia() {
                    j += 1;
                }
                matches!(
                    self.tokens.get(j).map(|t| t.kind.clone()),
                    Some(TokenKind::VerticalBar)
                )
            }
            _ => false,
        }
    }

    fn parse_union_variants_inner(&mut self) {
        if self.at_pipe_in_union() {
            self.parse_single_variant();
        } else if self.is_ident() {
            self.parse_single_variant_no_pipe();
        } else {
            return;
        }

        while self.at_pipe_in_union() {
            self.parse_single_variant();
        }
    }

    fn parse_single_variant(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT.into());
        self.bump(); // |
        self.eat_trivia();
        self.parse_variant_after_pipe();
        self.builder.finish_node();
    }

    fn parse_single_variant_no_pipe(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT.into());
        self.parse_variant_after_pipe();
        self.builder.finish_node();
    }

    fn parse_variant_after_pipe(&mut self) {
        self.expect_ident();
        self.eat_trivia();

        // Variant fields: `{ name: Type, ... }` (named) or `(Type, ...)` (positional).
        // Each bracket style accepts exactly one field form — mixing them is a
        // parse error so there is a single canonical way to write each variant.
        if self.at(&TokenKind::LeftBrace) {
            self.bump(); // {
            self.eat_trivia();
            self.parse_comma_separated(Self::parse_named_variant_field, &TokenKind::RightBrace);
            self.expect(&TokenKind::RightBrace);
            self.eat_trivia();
        } else if self.at(&TokenKind::LeftParen) {
            self.bump(); // (
            self.eat_trivia();
            self.parse_comma_separated(
                Self::parse_positional_variant_field,
                &TokenKind::RightParen,
            );
            self.expect(&TokenKind::RightParen);
            self.eat_trivia();
        }
    }

    fn parse_string_literal_union(&mut self) {
        self.builder
            .start_node(SyntaxKind::TYPE_DEF_STRING_UNION.into());

        // First string literal
        self.bump(); // string
        self.eat_trivia();

        // Parse remaining `| "string"` pairs
        while self.at(&TokenKind::VerticalBar) {
            self.bump(); // |
            self.eat_trivia();
            if self.at(&TokenKind::String(String::new())) {
                self.bump(); // string
                self.eat_trivia();
            } else {
                self.error("expected string literal after `|` in string literal union");
                break;
            }
        }

        self.builder.finish_node();
    }

    /// Parse a positional variant field: a type with no name. Used inside
    /// `(...)` variant declarations. If a `name:` prefix is present the user
    /// meant a named variant, so emit a targeted error but still consume the
    /// tokens so downstream lowering stays valid.
    fn parse_positional_variant_field(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_FIELD.into());

        if self.is_ident() && self.peek_is(&TokenKind::Colon) {
            self.error(
                "named fields are not allowed in `(...)` variants; \
                 use `(Type)` for positional fields or `{ name: Type }` for named fields",
            );
            self.bump(); // name
            self.eat_trivia();
            self.bump(); // :
            self.eat_trivia();
        }
        self.parse_type_expr();

        self.builder.finish_node();
    }

    /// Parse a named variant field: `name: Type`. Used inside `{...}` variant
    /// declarations. If the `name:` prefix is missing the user meant a
    /// positional variant, so emit a targeted error.
    fn parse_named_variant_field(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_FIELD.into());

        if self.is_ident() && self.peek_is(&TokenKind::Colon) {
            self.bump(); // name
            self.eat_trivia();
            self.bump(); // :
            self.eat_trivia();
        } else {
            self.error(
                "`{...}` variants require named fields; \
                 use `{ name: Type, ... }` or switch to `(Type, ...)` for positional fields",
            );
        }
        self.parse_type_expr();

        self.builder.finish_node();
    }

    pub(super) fn parse_record_fields(&mut self) {
        self.expect(&TokenKind::LeftBrace);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_record_entry, &TokenKind::RightBrace);
        self.expect(&TokenKind::RightBrace);
    }

    fn parse_record_entry(&mut self) {
        // Check for spread: `...TypeName` or `...Generic<T>`
        if self.at(&TokenKind::DotDotDot) {
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
        self.expect_ident_flex();
        self.eat_trivia();
        self.expect(&TokenKind::Colon);
        self.eat_trivia();
        self.parse_type_expr();
        self.eat_trivia();

        if self.at(&TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            self.parse_expr();
        }

        self.builder.finish_node();
    }

    // ── For Blocks ──────────────────────────────────────────────

    /// Parse a for-block: `for Type { fn ... }` (inherent pipe-functions).
    /// The old `for Type: Trait { ... }` syntax is rejected here with a
    /// diagnostic pointing at `impl Trait for Type { ... }`.
    fn parse_for_block(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_BLOCK.into());

        self.expect(&TokenKind::For);
        self.eat_trivia();

        // Parse the type name (e.g., `User`, `Array<T>`)
        self.parse_type_expr();
        self.eat_trivia();

        // `for Type: Trait { ... }` was the old trait-impl form. It's now
        // written as `impl Trait for Type { ... }`. Reject with a clear
        // migration hint rather than silently accepting.
        if self.at(&TokenKind::Colon) {
            self.error(
                "`for Type: Trait { ... }` is no longer valid. \
                 Use `impl Trait for Type { ... }` for trait impls; \
                 `for Type { ... }` is reserved for inherent pipe-functions",
            );
            self.bump(); // :
            self.eat_trivia();
            self.expect_ident(); // trait name (consume so parsing can continue)
            self.eat_trivia();
        }

        self.expect(&TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse function declarations inside the block (with optional export)
        while !self.at(&TokenKind::RightBrace) && !self.at_end() {
            if self.at(&TokenKind::Export) {
                self.bump();
                self.eat_trivia();
            }
            if self.at(&TokenKind::Let) || self.at(&TokenKind::Async) {
                self.parse_for_block_function();
                self.eat_trivia();
            } else {
                self.error("expected `let` inside for block");
                self.bump();
                self.eat_trivia();
            }
        }

        self.expect(&TokenKind::RightBrace);

        self.builder.finish_node();
    }

    /// Parse an impl block: `impl Trait for Type { fn ... }` or
    /// `impl Trait for Type` (empty impl — legal if trait has all defaults).
    fn parse_impl_block(&mut self) {
        self.builder.start_node(SyntaxKind::IMPL_BLOCK.into());

        self.expect(&TokenKind::Impl);
        self.eat_trivia();

        // Trait name (identifier). Placed first so the node order is
        // `IMPL_BLOCK > trait-ident, FOR, type-expr, body`.
        self.expect_ident();
        self.eat_trivia();

        self.expect(&TokenKind::For);
        self.eat_trivia();

        // Target type (e.g., `User`, `Array<number>`)
        self.parse_type_expr();
        self.eat_trivia();

        // Optional body. Absence = empty impl (trait defaults).
        if self.at(&TokenKind::LeftBrace) {
            self.bump();
            self.eat_trivia();

            while !self.at(&TokenKind::RightBrace) && !self.at_end() {
                if self.at(&TokenKind::Export) {
                    self.bump();
                    self.eat_trivia();
                }
                if self.at(&TokenKind::Let) || self.at(&TokenKind::Async) {
                    self.parse_for_block_function();
                    self.eat_trivia();
                } else {
                    self.error("expected `let` inside impl block");
                    self.bump();
                    self.eat_trivia();
                }
            }

            self.expect(&TokenKind::RightBrace);
        }

        self.builder.finish_node();
    }

    // ── Trait Declarations ────────────────────────────────────────

    fn parse_trait_decl(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_DECL.into());

        self.expect(&TokenKind::Trait);
        self.eat_trivia();

        self.expect_ident(); // trait name
        self.eat_trivia();

        self.expect(&TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse method declarations inside the trait
        while !self.at(&TokenKind::RightBrace) && !self.at_end() {
            if self.at(&TokenKind::Let) {
                self.parse_trait_method();
                self.eat_trivia();
            } else {
                self.error("expected `let` inside trait");
                self.bump();
                self.eat_trivia();
            }
        }

        self.expect(&TokenKind::RightBrace);

        self.builder.finish_node();
    }

    fn parse_trait_method(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

        self.expect(&TokenKind::Let);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        self.expect(&TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_for_block_param, &TokenKind::RightParen);
        self.expect(&TokenKind::RightParen);
        self.eat_trivia();

        // Optional return type
        if self.at(&TokenKind::ThinArrow) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        // Optional body (default implementation): `= { ... }`
        if self.at(&TokenKind::Equal) {
            self.bump();
            self.eat_trivia();
            self.parse_block_expr();
        }

        self.builder.finish_node();
    }

    fn parse_for_block_function(&mut self) {
        self.builder.start_node(SyntaxKind::FUNCTION_DECL.into());

        // Optional `async` prefix
        if self.at(&TokenKind::Async) {
            self.bump(); // async
            self.eat_trivia();
        }

        self.expect(&TokenKind::Let);
        self.eat_trivia();
        self.expect_ident();
        self.eat_trivia();

        self.expect(&TokenKind::LeftParen);
        self.eat_trivia();
        self.parse_comma_separated(Self::parse_for_block_param, &TokenKind::RightParen);
        self.expect(&TokenKind::RightParen);
        self.eat_trivia();

        // Optional return type
        if self.at(&TokenKind::ThinArrow) {
            self.bump();
            self.eat_trivia();
            self.parse_type_expr();
            self.eat_trivia();
        }

        self.expect(&TokenKind::Equal);
        self.eat_trivia();
        self.parse_block_expr();

        self.builder.finish_node();
    }

    fn parse_for_block_param(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM.into());

        if self.at(&TokenKind::SelfKw) {
            // `self` parameter — bump as an ident-like token
            self.bump();
        } else {
            self.expect_ident();
            self.eat_trivia();

            if self.at(&TokenKind::Colon) {
                self.bump();
                self.eat_trivia();
                self.parse_type_expr();
                self.eat_trivia();
            }

            if self.at(&TokenKind::Equal) {
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
        self.expect_kind(&TokenKind::String(String::new()));
        self.eat_trivia();

        self.expect(&TokenKind::LeftBrace);
        self.eat_trivia();

        // Parse test body: let bindings, assert statements, and expressions
        while !self.at(&TokenKind::RightBrace) && !self.at_end() {
            let prev_pos = self.pos;
            if self.at(&TokenKind::Let) {
                let checkpoint = self.builder.checkpoint();
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::ITEM.into());
                self.parse_const_decl();
                self.builder.finish_node();
            } else if self.at(&TokenKind::Assert) {
                self.parse_assert_stmt();
            } else {
                self.parse_expr();
            }
            self.eat_trivia();
            if self.pos == prev_pos && !self.at_end() {
                self.bump();
            }
        }

        self.expect(&TokenKind::RightBrace);

        self.builder.finish_node();
    }

    fn parse_assert_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ASSERT_EXPR.into());

        self.expect(&TokenKind::Assert);
        self.eat_trivia();
        self.parse_expr();

        self.builder.finish_node();
    }
}
