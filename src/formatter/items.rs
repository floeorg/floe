use crate::syntax::{SyntaxKind, SyntaxNode};

use super::Formatter;

impl Formatter<'_> {
    pub(crate) fn fmt_item(&mut self, node: &SyntaxNode) {
        let has_export = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|t| t.kind() == SyntaxKind::KW_EXPORT)
        });

        if has_export {
            self.write("export ");
        }

        for child in node.children() {
            self.fmt_node(&child);
        }
    }

    pub(crate) fn fmt_expr_item(&mut self, node: &SyntaxNode) {
        for child in node.children() {
            self.fmt_node(&child);
        }
        if node.children().next().is_none() {
            self.fmt_tokens_only(node);
        }
    }

    // ── Import ──────────────────────────────────────────────────

    pub(crate) fn fmt_import(&mut self, node: &SyntaxNode) {
        self.write("import ");

        // Check for module-level `trusted` keyword
        let has_trusted = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| tok.kind() == SyntaxKind::KW_TRUSTED)
        });
        if has_trusted {
            self.write("trusted ");
        }

        let specifiers: Vec<_> = node
            .children()
            .filter(|c| {
                c.kind() == SyntaxKind::IMPORT_SPECIFIER
                    || c.kind() == SyntaxKind::IMPORT_FOR_SPECIFIER
            })
            .collect();

        if !specifiers.is_empty() {
            self.write("{ ");
            for (i, spec) in specifiers.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                if spec.kind() == SyntaxKind::IMPORT_FOR_SPECIFIER {
                    self.fmt_import_for_specifier(spec);
                } else {
                    self.fmt_import_specifier(spec);
                }
            }
            self.write(" } ");
        }

        self.write("from ");

        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                self.write(tok.text());
            }
        }
    }

    fn fmt_import_specifier(&mut self, node: &SyntaxNode) {
        let idents: Vec<_> = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::BANNED)
            .collect();

        // Check for per-specifier `trusted` — KW_TRUSTED token before the idents
        let has_trusted = self.has_token(node, SyntaxKind::KW_TRUSTED);

        if has_trusted {
            self.write("trusted ");
            if let Some(name) = idents.first() {
                self.write(name.text());
            }
            if idents.len() > 1 {
                self.write(" as ");
                if let Some(alias) = idents.last() {
                    self.write(alias.text());
                }
            }
        } else {
            if let Some(name) = idents.first() {
                self.write(name.text());
            }
            if idents.len() > 1 {
                self.write(" as ");
                if let Some(alias) = idents.last() {
                    self.write(alias.text());
                }
            }
        }
    }

    // ── Re-export ───────────────────────────────────────────────

    pub(crate) fn fmt_reexport(&mut self, node: &SyntaxNode) {
        // The `export` keyword is emitted by fmt_item
        self.write("{ ");
        let specifiers: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::REEXPORT_SPECIFIER)
            .collect();
        for (i, spec) in specifiers.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.fmt_reexport_specifier(spec);
        }
        self.write(" } ");
        self.write("from ");
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                self.write(tok.text());
            }
        }
    }

    fn fmt_reexport_specifier(&mut self, node: &SyntaxNode) {
        let idents: Vec<_> = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::BANNED)
            .collect();

        if let Some(name) = idents.first() {
            self.write(name.text());
        }
        if idents.len() > 1 {
            self.write(" as ");
            if let Some(alias) = idents.last() {
                self.write(alias.text());
            }
        }
    }

    fn fmt_import_for_specifier(&mut self, node: &SyntaxNode) {
        self.write("for ");
        if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }
    }

    // ── Const ───────────────────────────────────────────────────

    pub(crate) fn fmt_const(&mut self, node: &SyntaxNode) {
        self.write("const ");

        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lbrace_before_eq = self.has_brace_destructuring(node);
        let has_lparen_before_eq = self.has_paren_destructuring(node);

        if has_lbracket {
            self.write("[");
            let idents = self.collect_idents(node);
            self.write(&idents.join(", "));
            self.write("]");
        } else if has_lbrace_before_eq {
            self.write("{ ");
            let fields = self.collect_destructure_fields(node);
            self.write(&fields.join(", "));
            self.write(" }");
        } else if has_lparen_before_eq {
            self.write("(");
            let idents = self.collect_idents_before_eq(node);
            self.write(&idents.join(", "));
            self.write(")");
        } else {
            let idents = self.collect_idents_before_colon_or_eq(node);
            if let Some(name) = idents.first() {
                self.write(name);
            }
        }

        let type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();
        if let Some(type_expr) = type_exprs.first() {
            self.write(": ");
            self.fmt_type_expr(type_expr);
        }

        self.write(" = ");

        let expr = self.find_expr_after_eq(node);
        if let Some(expr) = expr {
            self.fmt_node(&expr);
        } else {
            self.fmt_token_expr_after_eq(node);
        }
    }

    // ── Function ────────────────────────────────────────────────

    pub(crate) fn fmt_function(&mut self, node: &SyntaxNode) {
        if self.has_token(node, SyntaxKind::KW_ASYNC) {
            self.write("async ");
        }
        self.write("fn ");

        if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }

        // Emit generic type parameters: <T, R: Trait>
        self.fmt_type_params(node);

        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();

        let return_type = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR);

        // Try inline params + return type
        let inline = self.try_inline(|f| {
            f.write("(");
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    f.write(", ");
                }
                f.fmt_param(param);
            }
            f.write(")");
            if let Some(rt) = &return_type {
                f.write(" -> ");
                f.fmt_type_expr(rt);
            }
            f.write(" {");
        });

        if self.fits_inline(&inline) {
            // Inline: fn name(a: T, b: U) -> R {
            self.write("(");
            for (i, param) in params.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_param(param);
            }
            self.write(")");

            if let Some(rt) = &return_type {
                self.write(" -> ");
                self.fmt_type_expr(rt);
            }
        } else {
            // Multi-line: fn name(\n    a: T,\n    b: U,\n) -> R
            self.write("(");
            self.indent += 1;
            for param in &params {
                self.newline();
                self.write_indent();
                self.fmt_param(param);
                self.write(",");
            }
            self.indent -= 1;
            self.newline();
            self.write_indent();
            self.write(")");

            if let Some(rt) = &return_type {
                self.write(" -> ");
                self.fmt_type_expr(rt);
            }
        }

        if let Some(block) = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR) {
            self.write(" ");
            self.fmt_block(&block);
        }
    }

    /// Emit generic type parameters from a function node: `<T, R: Trait>`.
    /// Reproduces tokens between `<` and `>` (idents, colons, commas).
    fn fmt_type_params(&mut self, node: &SyntaxNode) {
        let mut in_angle = false;
        let mut started = false;
        for token in node.children_with_tokens() {
            if let Some(tok) = token.as_token() {
                match tok.kind() {
                    SyntaxKind::LESS_THAN => {
                        self.write("<");
                        in_angle = true;
                        started = true;
                    }
                    SyntaxKind::GREATER_THAN if in_angle => {
                        self.write(">");
                        return;
                    }
                    SyntaxKind::IDENT if in_angle => {
                        self.write(tok.text());
                    }
                    SyntaxKind::COLON if in_angle => {
                        self.write(": ");
                    }
                    SyntaxKind::COMMA if in_angle => {
                        self.write(", ");
                    }
                    _ if in_angle => {}
                    // Stop looking once we've passed the point where `<` would appear
                    SyntaxKind::L_PAREN if !started => return,
                    _ => {}
                }
            }
        }
    }

    pub(crate) fn fmt_param(&mut self, node: &SyntaxNode) {
        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        let has_lparen = self.has_paren_destructuring(node);
        let has_underscore = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| tok.kind() == SyntaxKind::UNDERSCORE)
        });

        let has_self = self.has_token(node, SyntaxKind::KW_SELF);

        if has_self {
            self.write("self");
        } else if has_lbrace {
            // Destructured param: { name, age }
            let idents = self.collect_idents_before_colon_or_eq(node);
            self.write("{ ");
            self.write(&idents.join(", "));
            self.write(" }");
        } else if has_lparen {
            // Tuple destructured param: (a, b)
            let idents = self.collect_idents_before_colon_or_eq(node);
            self.write("(");
            self.write(&idents.join(", "));
            self.write(")");
        } else if has_underscore {
            self.write("_");
        } else if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.write(": ");
            self.fmt_type_expr(&type_expr);
        }

        if self.has_token(node, SyntaxKind::EQUAL) {
            self.write(" = ");
            self.fmt_token_expr_after_eq(node);
        }
    }

    // ── Type Declaration ────────────────────────────────────────

    pub(crate) fn fmt_type_decl(&mut self, node: &SyntaxNode) {
        if self.has_token(node, SyntaxKind::KW_OPAQUE) {
            self.write("opaque ");
        }
        self.write("type ");

        let idents = self.collect_idents_direct(node);
        if let Some(name) = idents.first() {
            self.write(name);
        }

        if idents.len() > 1 {
            self.write("<");
            self.write(&idents[1..].join(", "));
            self.write(">");
        }

        for child in node.children() {
            match child.kind() {
                SyntaxKind::TYPE_DEF_UNION => {
                    self.fmt_union(&child);
                }
                SyntaxKind::TYPE_DEF_RECORD => {
                    self.write(" ");
                    self.fmt_record_def(&child);
                }
                SyntaxKind::TYPE_DEF_ALIAS | SyntaxKind::TYPE_DEF_STRING_UNION => {
                    self.write(" = ");
                    self.fmt_type_alias_def(&child);
                }
                SyntaxKind::DERIVING_CLAUSE => {
                    self.write(" deriving (");
                    let deriving_idents = self.collect_idents_direct(&child);
                    self.write(&deriving_idents.join(", "));
                    self.write(")");
                }
                _ => {}
            }
        }
    }

    pub(crate) fn fmt_union(&mut self, node: &SyntaxNode) {
        let variants: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::VARIANT)
            .collect();

        // Newtype case: no VARIANT children, just VARIANT_FIELD directly
        // Emit paren syntax: `type UserId(string)`
        if variants.is_empty() {
            self.write("(");
            let fields: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::VARIANT_FIELD)
                .collect();
            for (i, child) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_variant_field(child);
            }
            self.write(")");
            return;
        }

        self.write(" {");
        self.indent += 1;
        for variant in &variants {
            self.newline();
            self.write_indent();
            self.write("| ");
            self.fmt_variant(variant);
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    fn fmt_variant(&mut self, node: &SyntaxNode) {
        // Skip the "|" ident — it's the union separator, not the variant name
        let name = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT && t.text() != "|")
            .map(|t| t.text().to_string());
        if let Some(name) = name {
            self.write(&name);
        }

        let fields: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::VARIANT_FIELD)
            .collect();

        if !fields.is_empty() {
            let all_positional = fields.iter().all(|f| !self.has_token(f, SyntaxKind::COLON));
            let (open, close) = if all_positional {
                ("(", ")")
            } else {
                (" { ", " }")
            };
            self.write(open);
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_variant_field(field);
            }
            self.write(close);
        }
    }

    fn fmt_variant_field(&mut self, node: &SyntaxNode) {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        let idents = self.collect_idents(node);

        if has_colon && let Some(name) = idents.first() {
            self.write(name);
            self.write(": ");
        }

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }
    }

    pub(crate) fn fmt_record_def(&mut self, node: &SyntaxNode) {
        // Collect fields and spreads in source order
        let members: Vec<_> = node
            .children()
            .filter(|c| {
                c.kind() == SyntaxKind::RECORD_FIELD || c.kind() == SyntaxKind::RECORD_SPREAD
            })
            .collect();

        self.write("{");
        if members.is_empty() {
            self.write("}");
            return;
        }

        self.indent += 1;
        for member in &members {
            self.newline();
            self.write_indent();
            if member.kind() == SyntaxKind::RECORD_SPREAD {
                self.fmt_record_spread(member);
            } else {
                self.fmt_record_field(member);
            }
            self.write(",");
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    fn fmt_record_spread(&mut self, node: &SyntaxNode) {
        self.write("...");
        // Look for a TYPE_EXPR child (generic types like Foo<Bar>)
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        } else if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }
    }

    pub(crate) fn fmt_record_field(&mut self, node: &SyntaxNode) {
        if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }
        self.write(": ");
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }

        if self.has_token(node, SyntaxKind::EQUAL) {
            self.write(" = ");
            self.fmt_token_expr_after_eq(node);
        }
    }

    pub(crate) fn fmt_type_alias_def(&mut self, node: &SyntaxNode) {
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        } else if node.kind() == SyntaxKind::TYPE_DEF_STRING_UNION {
            self.fmt_string_union_def(node);
        } else {
            self.fmt_verbatim(node);
        }
    }

    fn fmt_string_union_def(&mut self, node: &SyntaxNode) {
        let mut first = true;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                if !first {
                    self.write(" | ");
                }
                self.write(tok.text());
                first = false;
            }
        }
    }

    // ── For Block ───────────────────────────────────────────────

    pub(crate) fn fmt_for_block(&mut self, node: &SyntaxNode) {
        self.write("for ");

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr);
        }

        // Optional trait bound: `for User: Display`
        if self.has_token(node, SyntaxKind::COLON) {
            self.write(": ");
            self.fmt_token_expr_after_keyword(node, SyntaxKind::COLON);
        }

        self.write(" {");

        let mut methods: Vec<(bool, SyntaxNode)> = Vec::new();
        let mut next_is_export = false;
        for child_or_tok in node.children_with_tokens() {
            if let Some(tok) = child_or_tok.as_token()
                && tok.kind() == SyntaxKind::KW_EXPORT
            {
                next_is_export = true;
            }
            if let Some(child) = child_or_tok.into_node()
                && child.kind() == SyntaxKind::FUNCTION_DECL
            {
                methods.push((next_is_export, child));
                next_is_export = false;
            }
        }

        self.fmt_method_list(&methods);
    }

    // ── Trait Declaration ───────────────────────────────────────

    pub(crate) fn fmt_trait_decl(&mut self, node: &SyntaxNode) {
        self.write("trait ");

        if let Some(name) = self.first_ident(node) {
            self.write(&name);
        }

        self.write(" {");

        let methods: Vec<(bool, SyntaxNode)> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::FUNCTION_DECL)
            .map(|c| (false, c))
            .collect();

        self.fmt_method_list(&methods);
    }

    /// Format a list of methods inside a block (`for` or `trait`).
    /// Each entry is `(exported, FUNCTION_DECL node)`.
    fn fmt_method_list(&mut self, methods: &[(bool, SyntaxNode)]) {
        self.indent += 1;
        for (i, (exported, func)) in methods.iter().enumerate() {
            self.newline();
            if i > 0 {
                self.newline();
            }
            self.write_indent();
            if *exported {
                self.write("export ");
            }
            self.fmt_function(func);
        }
        self.indent -= 1;
        self.newline();
        self.write_indent();
        self.write("}");
    }

    // ── Type Expressions ────────────────────────────────────────

    pub(crate) fn fmt_type_expr(&mut self, node: &SyntaxNode) {
        let idents = self.collect_idents(node);
        let has_fat_arrow = self.has_token(node, SyntaxKind::FAT_ARROW);
        let has_thin_arrow = self.has_token(node, SyntaxKind::THIN_ARROW);
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);
        let has_record_fields = node
            .children()
            .any(|c| c.kind() == SyntaxKind::RECORD_FIELD);
        let child_type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();

        // String literal type: "div", "button"
        let string_token = node.children_with_tokens().find_map(|t| {
            t.as_token()
                .filter(|tok| tok.kind() == SyntaxKind::STRING)
                .map(|tok| tok.text().to_string())
        });
        if let Some(s) = string_token {
            self.write(&s);
            return;
        }

        // Unit type: ()
        if has_lparen
            && idents.is_empty()
            && !has_fat_arrow
            && !has_thin_arrow
            && child_type_exprs.is_empty()
        {
            self.write("()");
            return;
        }

        // Tuple type: (T, U)
        if has_lparen
            && !has_thin_arrow
            && !has_fat_arrow
            && !child_type_exprs.is_empty()
            && idents.is_empty()
        {
            self.write("(");
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write(")");
            return;
        }

        // Function type: (params) -> ReturnType
        if has_fat_arrow || has_thin_arrow {
            self.write("(");
            let param_count = child_type_exprs.len().saturating_sub(1);
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i == param_count {
                    break;
                }
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write(") -> ");
            if let Some(ret) = child_type_exprs.last() {
                self.fmt_type_expr(ret);
            }
            return;
        }

        // Tuple: [T, U]
        if has_lbracket {
            self.write("[");
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write("]");
            return;
        }

        // Record type
        if has_record_fields {
            let fields: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
                .collect();
            self.write("{ ");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_record_field(field);
            }
            self.write(" }");
            return;
        }

        // Intersection type: A & B & C
        // The CST nests this as: outer TYPE_EXPR has the LHS tokens + AMP + child TYPE_EXPR(RHS)
        let has_amp = self.has_token(node, SyntaxKind::AMP);
        if has_amp {
            // Format the LHS (everything before the &)
            let has_typeof = self.has_token(node, SyntaxKind::KW_TYPEOF);
            if has_typeof {
                self.write("typeof ");
            }
            let has_dot = self.has_token(node, SyntaxKind::DOT);
            if has_dot {
                self.write(&idents.join("."));
            } else if let Some(name) = idents.first() {
                self.write(name);
            }
            // LHS type args (e.g. Generic<T> & B)
            // Count child TYPE_EXPRs that are type args (before AMP) vs intersection members (after AMP)
            // In the CST, the AMP token separates type args from intersection RHS
            let amp_position = node
                .children_with_tokens()
                .position(|t| t.as_token().is_some_and(|t| t.kind() == SyntaxKind::AMP));
            let type_expr_positions: Vec<(usize, SyntaxNode)> = node
                .children_with_tokens()
                .enumerate()
                .filter_map(|(i, t)| {
                    t.into_node()
                        .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
                        .map(|n| (i, n))
                })
                .collect();
            let (type_args, intersection_rhs): (Vec<_>, Vec<_>) = type_expr_positions
                .into_iter()
                .partition(|(i, _)| amp_position.is_some_and(|ap| *i < ap));

            if !type_args.is_empty() {
                self.write("<");
                for (i, (_, te)) in type_args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.fmt_type_expr(te);
                }
                self.write(">");
            }
            // Format each RHS intersection member
            for (_, te) in &intersection_rhs {
                self.write(" & ");
                self.fmt_type_expr(te);
            }
            return;
        }

        // typeof type expression
        let has_typeof = self.has_token(node, SyntaxKind::KW_TYPEOF);
        if has_typeof {
            self.write("typeof ");
        }

        // Named type with dots
        let has_dot = self.has_token(node, SyntaxKind::DOT);
        if has_dot {
            self.write(&idents.join("."));
        } else if let Some(name) = idents.first() {
            self.write(name);
        }

        // Type args
        if !child_type_exprs.is_empty() {
            self.write("<");
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.fmt_type_expr(te);
            }
            self.write(">");
        }
    }
}
