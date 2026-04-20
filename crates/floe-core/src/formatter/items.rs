use crate::pretty::{self, Document};
use crate::syntax::{SyntaxKind, SyntaxNode};

use super::Formatter;

impl Formatter<'_> {
    pub(crate) fn fmt_item(&mut self, node: &SyntaxNode) -> Document {
        let has_export = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|t| t.kind() == SyntaxKind::KW_EXPORT)
        });

        let mut parts = Vec::new();
        if has_export {
            parts.push(pretty::str("export "));
        }
        for child in node.children() {
            parts.push(self.fmt_node(&child));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_expr_item(&mut self, node: &SyntaxNode) -> Document {
        if let Some(child) = node.children().next() {
            return self.fmt_node(&child);
        }
        if let Some(text) = self.first_content_token(node) {
            return pretty::str(text);
        }
        pretty::nil()
    }

    // ── Import ──────────────────────────────────────────────────

    pub(crate) fn fmt_import(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("import ")];

        let has_trusted = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| tok.kind() == SyntaxKind::KW_TRUSTED)
        });
        if has_trusted {
            parts.push(pretty::str("trusted "));
        }

        let specifiers: Vec<_> = node
            .children()
            .filter(|c| {
                c.kind() == SyntaxKind::IMPORT_SPECIFIER
                    || c.kind() == SyntaxKind::IMPORT_FOR_SPECIFIER
            })
            .collect();

        if !specifiers.is_empty() {
            parts.push(pretty::str("{ "));
            for (i, spec) in specifiers.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                if spec.kind() == SyntaxKind::IMPORT_FOR_SPECIFIER {
                    parts.push(self.fmt_import_for_specifier(spec));
                } else {
                    parts.push(self.fmt_import_specifier(spec));
                }
            }
            parts.push(pretty::str(" } "));
        }

        parts.push(pretty::str("from "));

        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                parts.push(pretty::str(tok.text()));
            }
        }

        pretty::concat(parts)
    }

    fn fmt_import_specifier(&self, node: &SyntaxNode) -> Document {
        let idents: Vec<_> = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::BANNED)
            .collect();

        let has_trusted = self.has_token(node, SyntaxKind::KW_TRUSTED);
        let mut parts = Vec::new();

        if has_trusted {
            parts.push(pretty::str("trusted "));
        }
        if let Some(name) = idents.first() {
            parts.push(pretty::str(name.text()));
        }
        if idents.len() > 1 {
            parts.push(pretty::str(" as "));
            if let Some(alias) = idents.last() {
                parts.push(pretty::str(alias.text()));
            }
        }
        pretty::concat(parts)
    }

    fn fmt_import_for_specifier(&self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("for ")];
        if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }
        pretty::concat(parts)
    }

    // ── Re-export ───────────────────────────────────────────────

    pub(crate) fn fmt_reexport(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("{ ")];
        let specifiers: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::REEXPORT_SPECIFIER)
            .collect();
        for (i, spec) in specifiers.iter().enumerate() {
            if i > 0 {
                parts.push(pretty::str(", "));
            }
            parts.push(self.fmt_reexport_specifier(spec));
        }
        parts.push(pretty::str(" } from "));
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                parts.push(pretty::str(tok.text()));
            }
        }
        pretty::concat(parts)
    }

    fn fmt_reexport_specifier(&self, node: &SyntaxNode) -> Document {
        let idents: Vec<_> = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT || t.kind() == SyntaxKind::BANNED)
            .collect();
        let mut parts = Vec::new();
        if let Some(name) = idents.first() {
            parts.push(pretty::str(name.text()));
        }
        if idents.len() > 1 {
            parts.push(pretty::str(" as "));
            if let Some(alias) = idents.last() {
                parts.push(pretty::str(alias.text()));
            }
        }
        pretty::concat(parts)
    }

    // ── Const ───────────────────────────────────────────────────

    pub(crate) fn fmt_const(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("let ")];

        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lbrace_before_eq = self.has_brace_destructuring(node);
        let has_lparen_before_eq = self.has_paren_destructuring(node);

        if has_lbracket {
            parts.push(pretty::str("["));
            let idents = self.collect_idents(node);
            parts.push(pretty::str(idents.join(", ")));
            parts.push(pretty::str("]"));
        } else if has_lbrace_before_eq {
            parts.push(pretty::str("{ "));
            let fields = self.collect_destructure_fields(node);
            parts.push(pretty::str(fields.join(", ")));
            parts.push(pretty::str(" }"));
        } else if has_lparen_before_eq {
            parts.push(pretty::str("("));
            let idents = self.collect_idents_before_eq(node);
            parts.push(pretty::str(idents.join(", ")));
            parts.push(pretty::str(")"));
        } else {
            let idents = self.collect_idents_before_colon_or_eq(node);
            if let Some(name) = idents.first() {
                parts.push(pretty::str(name));
            }
        }

        let type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();
        if let Some(type_expr) = type_exprs.first() {
            parts.push(pretty::str(": "));
            parts.push(self.fmt_type_expr(type_expr));
        }

        parts.push(pretty::str(" = "));
        parts.push(self.fmt_expr_after_eq(node));

        pretty::concat(parts)
    }

    // ── Function ────────────────────────────────────────────────

    pub(crate) fn fmt_function(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        // All function declarations — top-level, for-block, trait — use
        // `let NAME(params) [-> Ret] = body`.

        if self.has_token(node, SyntaxKind::KW_ASYNC) {
            parts.push(pretty::str("async "));
        }
        parts.push(pretty::str("let "));

        if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }

        parts.push(self.fmt_type_params(node));

        let params: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::PARAM)
            .collect();
        let return_type = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR);
        let block = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let mut sig = Vec::new();
        sig.push(pretty::str("("));
        let mut has_comment = false;
        if !params.is_empty() {
            let mut inner = vec![pretty::break_("", "")];
            let mut prev_end: Option<u32> = None;
            for (i, p) in params.iter().enumerate() {
                let p_start: u32 = p.text_range().start().into();
                if i > 0 {
                    inner.push(pretty::break_(",", ", "));
                }
                if let Some(prev) = prev_end {
                    for c in self.pop_comments_in_range(prev, p_start) {
                        inner.push(pretty::str(c.text));
                        inner.push(pretty::line());
                        has_comment = true;
                    }
                }
                inner.push(self.fmt_param(p));
                prev_end = Some(p.text_range().end().into());
            }
            if has_comment {
                sig.push(pretty::force_break());
            }
            sig.push(pretty::nest(4, pretty::concat(inner)));
            sig.push(pretty::break_(",", ""));
        }
        sig.push(pretty::str(")"));

        if let Some(rt) = &return_type {
            sig.push(pretty::str(" -> "));
            sig.push(self.fmt_type_expr(rt));
        }

        parts.push(pretty::group(pretty::concat(sig)));

        // Trait method declarations have no body — `let NAME(params) -> Ret`
        // with no `= ...`. Emit `=` only when a block follows.
        let has_block = node.children().any(|c| c.kind() == SyntaxKind::BLOCK_EXPR);
        if has_block {
            parts.push(pretty::str(" ="));
        }

        if let Some(block) = &block {
            // Synthetic single-expression body (`let f(x) = x + 1`) was
            // wrapped in BLOCK_EXPR at parse time. If the block contains
            // exactly one EXPR_ITEM, emit the bare expression.
            let items: Vec<_> = block
                .children()
                .filter(|c| !matches!(c.kind(), SyntaxKind::L_BRACE | SyntaxKind::R_BRACE))
                .collect();
            let has_braces = block.children_with_tokens().any(|t| {
                t.as_token()
                    .is_some_and(|t| t.kind() == SyntaxKind::L_BRACE)
            });
            if !has_braces && items.len() == 1 && items[0].kind() == SyntaxKind::EXPR_ITEM {
                parts.push(pretty::str(" "));
                parts.push(self.fmt_expr_item(&items[0]));
            } else {
                parts.push(pretty::str(" "));
                parts.push(self.fmt_block(block));
            }
        }

        pretty::concat(parts)
    }

    fn fmt_type_params(&self, node: &SyntaxNode) -> Document {
        let mut in_angle = false;
        let mut started = false;
        let mut parts = Vec::new();

        for token in node.children_with_tokens() {
            if let Some(tok) = token.as_token() {
                match tok.kind() {
                    SyntaxKind::LESS_THAN => {
                        parts.push(pretty::str("<"));
                        in_angle = true;
                        started = true;
                    }
                    SyntaxKind::GREATER_THAN if in_angle => {
                        parts.push(pretty::str(">"));
                        return pretty::concat(parts);
                    }
                    SyntaxKind::IDENT if in_angle => {
                        parts.push(pretty::str(tok.text()));
                    }
                    SyntaxKind::COLON if in_angle => {
                        parts.push(pretty::str(": "));
                    }
                    SyntaxKind::COMMA if in_angle => {
                        parts.push(pretty::str(", "));
                    }
                    _ if in_angle => {}
                    SyntaxKind::L_PAREN if !started => return pretty::nil(),
                    _ => {}
                }
            }
        }
        pretty::nil()
    }

    pub(crate) fn fmt_param(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();

        let has_lbrace = self.has_token(node, SyntaxKind::L_BRACE);
        let has_lparen = self.has_paren_destructuring(node);
        let has_underscore = node.children_with_tokens().any(|t| {
            t.as_token()
                .is_some_and(|tok| tok.kind() == SyntaxKind::UNDERSCORE)
        });
        let has_self = self.has_token(node, SyntaxKind::KW_SELF);

        if has_self {
            parts.push(pretty::str("self"));
        } else if has_lbrace {
            let idents = self.collect_idents_before_colon_or_eq(node);
            parts.push(pretty::str("{ "));
            parts.push(pretty::str(idents.join(", ")));
            parts.push(pretty::str(" }"));
        } else if has_lparen {
            let idents = self.collect_idents_before_colon_or_eq(node);
            parts.push(pretty::str("("));
            parts.push(pretty::str(idents.join(", ")));
            parts.push(pretty::str(")"));
        } else if has_underscore {
            parts.push(pretty::str("_"));
        } else if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            parts.push(pretty::str(": "));
            parts.push(self.fmt_type_expr(&type_expr));
        }

        if self.has_token(node, SyntaxKind::EQUAL) {
            parts.push(pretty::str(" = "));
            parts.push(self.fmt_expr_after_eq(node));
        }

        pretty::concat(parts)
    }

    // ── Type Declaration ────────────────────────────────────────

    pub(crate) fn fmt_type_decl(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();

        if self.has_token(node, SyntaxKind::KW_OPAQUE) {
            parts.push(pretty::str("opaque "));
        }
        parts.push(pretty::str("type "));

        let idents = self.collect_idents(node);
        if let Some(name) = idents.first() {
            parts.push(pretty::str(name));
        }
        if idents.len() > 1 {
            parts.push(pretty::str("<"));
            parts.push(pretty::str(idents[1..].join(", ")));
            parts.push(pretty::str(">"));
        }

        for child in node.children() {
            match child.kind() {
                SyntaxKind::TYPE_DEF_UNION => {
                    parts.push(pretty::str(" = "));
                    parts.push(self.fmt_union(&child));
                }
                SyntaxKind::TYPE_DEF_RECORD => {
                    parts.push(pretty::str(" = "));
                    parts.push(self.fmt_record_def(&child));
                }
                SyntaxKind::TYPE_DEF_ALIAS | SyntaxKind::TYPE_DEF_STRING_UNION => {
                    parts.push(pretty::str(" = "));
                    parts.push(self.fmt_type_alias_def(&child));
                }
                SyntaxKind::DERIVING_CLAUSE => {
                    parts.push(pretty::str(" deriving ("));
                    let deriving_idents = self.collect_idents(&child);
                    parts.push(pretty::str(deriving_idents.join(", ")));
                    parts.push(pretty::str(")"));
                }
                _ => {}
            }
        }

        pretty::concat(parts)
    }

    pub(crate) fn fmt_union(&mut self, node: &SyntaxNode) -> Document {
        let variants: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::VARIANT)
            .collect();

        let mut inner = Vec::new();
        for variant in &variants {
            inner.push(pretty::line());
            inner.push(pretty::str("| "));
            inner.push(self.fmt_variant(variant));
        }

        pretty::nest(4, pretty::concat(inner))
    }

    fn fmt_variant(&mut self, node: &SyntaxNode) -> Document {
        let name = node
            .children_with_tokens()
            .filter_map(|t| t.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT && t.text() != "|")
            .map(|t| t.text().to_string());

        let mut parts = Vec::new();
        if let Some(name) = name {
            parts.push(pretty::str(name));
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
            parts.push(pretty::str(open));
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_variant_field(field));
            }
            parts.push(pretty::str(close));
        }

        pretty::concat(parts)
    }

    fn fmt_variant_field(&mut self, node: &SyntaxNode) -> Document {
        let has_colon = self.has_token(node, SyntaxKind::COLON);
        let idents = self.collect_idents(node);
        let mut parts = Vec::new();

        if has_colon && let Some(name) = idents.first() {
            parts.push(pretty::str(name));
            parts.push(pretty::str(": "));
        }

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            parts.push(self.fmt_type_expr(&type_expr));
        }

        pretty::concat(parts)
    }

    pub(crate) fn fmt_record_def(&mut self, node: &SyntaxNode) -> Document {
        let members: Vec<_> = node
            .children()
            .filter(|c| {
                c.kind() == SyntaxKind::RECORD_FIELD || c.kind() == SyntaxKind::RECORD_SPREAD
            })
            .collect();

        if members.is_empty() {
            return pretty::str("{}");
        }

        let mut inner = Vec::new();
        let mut prev_end: Option<u32> = None;
        for member in &members {
            let m_start: u32 = member.text_range().start().into();
            if let Some(prev) = prev_end {
                for c in self.pop_comments_in_range(prev, m_start) {
                    inner.push(pretty::line());
                    inner.push(pretty::str(c.text));
                }
            }
            inner.push(pretty::line());
            if member.kind() == SyntaxKind::RECORD_SPREAD {
                inner.push(self.fmt_record_spread(member));
            } else {
                inner.push(self.fmt_record_field(member));
            }
            inner.push(pretty::str(","));
            prev_end = Some(member.text_range().end().into());
        }

        pretty::concat(vec![
            pretty::str("{"),
            pretty::nest(4, pretty::concat(inner)),
            pretty::line(),
            pretty::str("}"),
        ])
    }

    fn fmt_record_spread(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("...")];
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            parts.push(self.fmt_type_expr(&type_expr));
        } else if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_record_field(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }
        parts.push(pretty::str(": "));
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            parts.push(self.fmt_type_expr(&type_expr));
        }
        if self.has_token(node, SyntaxKind::EQUAL) {
            parts.push(pretty::str(" = "));
            parts.push(self.fmt_expr_after_eq(node));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_type_alias_def(&mut self, node: &SyntaxNode) -> Document {
        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            self.fmt_type_expr(&type_expr)
        } else if node.kind() == SyntaxKind::TYPE_DEF_STRING_UNION {
            self.fmt_string_union_def(node)
        } else {
            self.fmt_verbatim(node)
        }
    }

    fn fmt_string_union_def(&self, node: &SyntaxNode) -> Document {
        let mut parts = Vec::new();
        let mut first = true;
        for t in node.children_with_tokens() {
            if let Some(tok) = t.as_token()
                && tok.kind() == SyntaxKind::STRING
            {
                if !first {
                    parts.push(pretty::str(" | "));
                }
                parts.push(pretty::str(tok.text()));
                first = false;
            }
        }
        pretty::concat(parts)
    }

    // ── For Block ───────────────────────────────────────────────

    pub(crate) fn fmt_for_block(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("for ")];

        if let Some(type_expr) = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR) {
            parts.push(self.fmt_type_expr(&type_expr));
        }

        if self.has_token(node, SyntaxKind::COLON) {
            parts.push(pretty::str(": "));
            parts.push(self.fmt_expr_after_keyword(node, SyntaxKind::COLON));
        }

        parts.push(pretty::str(" {"));

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

        parts.push(self.fmt_method_list(&methods));

        pretty::concat(parts)
    }

    // ── Trait Declaration ───────────────────────────────────────

    pub(crate) fn fmt_trait_decl(&mut self, node: &SyntaxNode) -> Document {
        let mut parts = vec![pretty::str("trait ")];

        if let Some(name) = self.first_ident(node) {
            parts.push(pretty::str(name));
        }

        parts.push(pretty::str(" {"));

        let methods: Vec<(bool, SyntaxNode)> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::FUNCTION_DECL)
            .map(|c| (false, c))
            .collect();

        parts.push(self.fmt_method_list(&methods));

        pretty::concat(parts)
    }

    fn fmt_method_list(&mut self, methods: &[(bool, SyntaxNode)]) -> Document {
        let mut inner = Vec::new();
        for (i, (exported, func)) in methods.iter().enumerate() {
            inner.push(pretty::line());
            if i > 0 {
                inner.push(pretty::line());
            }
            let mut method_parts = Vec::new();
            if *exported {
                method_parts.push(pretty::str("export "));
            }
            method_parts.push(self.fmt_function(func));
            inner.push(pretty::concat(method_parts));
        }

        pretty::concat(vec![
            pretty::nest(4, pretty::concat(inner)),
            pretty::line(),
            pretty::str("}"),
        ])
    }

    // ── Type Expressions ────────────────────────────────────────

    pub(crate) fn fmt_fn_type_param(&mut self, node: &SyntaxNode) -> Document {
        let label = self.collect_idents(node).into_iter().next();
        let type_expr = node.children().find(|c| c.kind() == SyntaxKind::TYPE_EXPR);
        let mut parts = Vec::new();
        if let Some(label) = label {
            parts.push(pretty::str(label));
            parts.push(pretty::str(": "));
        }
        if let Some(te) = type_expr {
            parts.push(self.fmt_type_expr(&te));
        }
        pretty::concat(parts)
    }

    pub(crate) fn fmt_type_expr(&mut self, node: &SyntaxNode) -> Document {
        let idents = self.collect_idents(node);
        let has_fat_arrow = self.has_token(node, SyntaxKind::THIN_ARROW);
        let has_lbracket = self.has_token(node, SyntaxKind::L_BRACKET);
        let has_lparen = self.has_token(node, SyntaxKind::L_PAREN);
        let has_record_fields = node
            .children()
            .any(|c| c.kind() == SyntaxKind::RECORD_FIELD);
        let child_type_exprs: Vec<_> = node
            .children()
            .filter(|c| c.kind() == SyntaxKind::TYPE_EXPR)
            .collect();

        // String literal type
        let string_token = node.children_with_tokens().find_map(|t| {
            t.as_token()
                .filter(|tok| tok.kind() == SyntaxKind::STRING)
                .map(|tok| tok.text().to_string())
        });
        if let Some(s) = string_token {
            return pretty::str(s);
        }

        // Unit type: ()
        if has_lparen && idents.is_empty() && !has_fat_arrow && child_type_exprs.is_empty() {
            return pretty::str("()");
        }

        // Tuple type: (T, U)
        if has_lparen && !has_fat_arrow && !child_type_exprs.is_empty() && idents.is_empty() {
            let mut parts = vec![pretty::str("(")];
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_type_expr(te));
            }
            parts.push(pretty::str(")"));
            return pretty::concat(parts);
        }

        // Function type: (params) -> ReturnType
        if has_fat_arrow {
            let fn_params: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::FN_TYPE_PARAM)
                .collect();
            let mut parts = vec![pretty::str("(")];
            for (i, param) in fn_params.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_fn_type_param(param));
            }
            parts.push(pretty::str(") -> "));
            if let Some(ret) = child_type_exprs.last() {
                parts.push(self.fmt_type_expr(ret));
            }
            return pretty::concat(parts);
        }

        // Array type: [T, U]
        if has_lbracket {
            let mut parts = vec![pretty::str("[")];
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_type_expr(te));
            }
            parts.push(pretty::str("]"));
            return pretty::concat(parts);
        }

        // Record type
        if has_record_fields {
            let fields: Vec<_> = node
                .children()
                .filter(|c| c.kind() == SyntaxKind::RECORD_FIELD)
                .collect();
            let mut parts = vec![pretty::str("{ ")];
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_record_field(field));
            }
            parts.push(pretty::str(" }"));
            return pretty::concat(parts);
        }

        // Intersection type: A & B
        let has_amp = self.has_token(node, SyntaxKind::AMP);
        if has_amp {
            let mut parts = Vec::new();
            let has_typeof = self.has_token(node, SyntaxKind::KW_TYPEOF);
            if has_typeof {
                parts.push(pretty::str("typeof "));
            }
            let has_dot = self.has_token(node, SyntaxKind::DOT);
            if has_dot {
                parts.push(pretty::str(idents.join(".")));
            } else if let Some(name) = idents.first() {
                parts.push(pretty::str(name));
            }

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
                parts.push(pretty::str("<"));
                for (i, (_, te)) in type_args.iter().enumerate() {
                    if i > 0 {
                        parts.push(pretty::str(", "));
                    }
                    parts.push(self.fmt_type_expr(te));
                }
                parts.push(pretty::str(">"));
            }
            for (_, te) in &intersection_rhs {
                parts.push(pretty::str(" & "));
                parts.push(self.fmt_type_expr(te));
            }
            return pretty::concat(parts);
        }

        // typeof type expression
        let mut parts = Vec::new();
        let has_typeof = self.has_token(node, SyntaxKind::KW_TYPEOF);
        if has_typeof {
            parts.push(pretty::str("typeof "));
        }

        // Named type with dots
        let has_dot = self.has_token(node, SyntaxKind::DOT);
        if has_dot {
            parts.push(pretty::str(idents.join(".")));
        } else if let Some(name) = idents.first() {
            parts.push(pretty::str(name));
        }

        // Type args
        if !child_type_exprs.is_empty() {
            parts.push(pretty::str("<"));
            for (i, te) in child_type_exprs.iter().enumerate() {
                if i > 0 {
                    parts.push(pretty::str(", "));
                }
                parts.push(self.fmt_type_expr(te));
            }
            parts.push(pretty::str(">"));
        }

        pretty::concat(parts)
    }
}
