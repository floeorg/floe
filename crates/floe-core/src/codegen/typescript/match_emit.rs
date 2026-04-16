use std::collections::HashMap;

use crate::parser::ast::*;
use crate::pretty::{self, Document};
use crate::type_layout;

use super::super::escape_string;
use super::expression::expr_contains_await;
use super::generator::{THROW_NON_EXHAUSTIVE, TypeScriptGenerator};

impl<'a> TypeScriptGenerator<'a> {
    // ── Match Lowering ───────────────────────────────────────────

    pub(super) fn emit_match(&mut self, subject: &TypedExpr, arms: &[TypedMatchArm]) -> Document {
        self.emit_match_arms(subject, arms, 0)
    }

    fn emit_match_arms(
        &mut self,
        subject: &TypedExpr,
        arms: &[TypedMatchArm],
        index: usize,
    ) -> Document {
        if index >= arms.len() {
            return pretty::str(THROW_NON_EXHAUSTIVE);
        }

        let arm = &arms[index];
        let is_last = index == arms.len() - 1;

        if is_last
            && arm.guard.is_none()
            && matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            )
        {
            return self.emit_match_body(subject, &arm.pattern, &arm.body);
        }

        if let Some(guard) = &arm.guard {
            let subject_str = self.emit_expr_string(subject);
            let bindings = collect_bindings(&subject_str, &arm.pattern, &self.ctx.variant_info);
            let has_bindings = !bindings.is_empty();

            if has_bindings {
                let cond_doc = self.emit_pattern_condition(subject, &arm.pattern);
                let guard_doc = self.emit_expr(guard);
                let body_doc = self.emit_expr(&arm.body);
                // Render the rest of the match once and reuse it for both the
                // guard-fall-through and the pattern-miss branches — otherwise
                // this is quadratic in arm count.
                let next_doc = self.emit_match_arms(subject, arms, index + 1);

                let mut binding_strs = String::new();
                for (name, access) in &bindings {
                    binding_strs.push_str(&format!("const {name} = {access}; "));
                }

                let guard_str = Self::doc_to_string(&guard_doc);
                let body_str = Self::doc_to_string(&body_doc);
                let next_str = Self::doc_to_string(&next_doc);

                return pretty::concat([
                    cond_doc,
                    pretty::str(format!(
                        " ? (() => {{ {binding_strs}if ({guard_str}) {{ return {body_str}; }} return {next_str}; }})()"
                    )),
                    pretty::str(format!(" : {next_str}")),
                ]);
            }

            let is_trivial_pattern = matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            );

            let mut docs = Vec::new();
            if !is_trivial_pattern {
                docs.push(self.emit_pattern_condition(subject, &arm.pattern));
                docs.push(pretty::str(" && "));
            }
            docs.push(self.emit_expr(guard));
            docs.push(pretty::str(" ? "));
            docs.push(self.emit_match_body(subject, &arm.pattern, &arm.body));
            docs.push(pretty::str(" : "));
            if is_last {
                docs.push(pretty::str(THROW_NON_EXHAUSTIVE));
            } else {
                docs.push(self.emit_match_arms(subject, arms, index + 1));
            }
            return pretty::concat(docs);
        }

        let mut docs = vec![
            self.emit_pattern_condition(subject, &arm.pattern),
            pretty::str(" ? "),
            self.emit_match_body(subject, &arm.pattern, &arm.body),
            pretty::str(" : "),
        ];
        if is_last {
            docs.push(pretty::str(THROW_NON_EXHAUSTIVE));
        } else {
            docs.push(self.emit_match_arms(subject, arms, index + 1));
        }
        pretty::concat(docs)
    }

    fn emit_pattern_condition(&mut self, subject: &TypedExpr, pattern: &Pattern) -> Document {
        match &pattern.kind {
            PatternKind::Literal(lit) => {
                let needs_parens = matches!(
                    subject.kind,
                    ExprKind::Binary { .. } | ExprKind::Pipe { .. } | ExprKind::Unary { .. }
                );
                let mut docs = Vec::new();
                if needs_parens {
                    docs.push(pretty::str("("));
                }
                docs.push(self.emit_expr(subject));
                if needs_parens {
                    docs.push(pretty::str(")"));
                }
                docs.push(pretty::str(" === "));
                docs.push(self.emit_literal_pattern(lit));
                pretty::concat(docs)
            }
            PatternKind::Range { start, end } => pretty::concat([
                pretty::str("("),
                self.emit_expr(subject),
                pretty::str(" >= "),
                self.emit_literal_pattern(start),
                pretty::str(" && "),
                self.emit_expr(subject),
                pretty::str(" <= "),
                self.emit_literal_pattern(end),
                pretty::str(")"),
            ]),
            PatternKind::Variant { name, fields } => {
                let subj_str = self.emit_expr_string(subject);
                let mut docs = vec![pretty::str(type_layout::variant_discriminant(
                    name, &subj_str,
                ))];

                let field_names = self
                    .ctx
                    .variant_info
                    .get(name.as_str())
                    .map(|(_, names)| names.clone());
                for (i, field_pat) in fields.iter().enumerate() {
                    if !matches!(
                        field_pat.kind,
                        PatternKind::Wildcard | PatternKind::Binding(_)
                    ) {
                        docs.push(pretty::str(" && "));
                        let field_access = type_layout::variant_field_accessor(
                            name,
                            i,
                            fields.len(),
                            field_names.as_deref(),
                            &subj_str,
                        );
                        let field_expr = TypedExpr::synthetic_typed(
                            ExprKind::Identifier(field_access),
                            subject.span,
                        );
                        docs.push(self.emit_pattern_condition(&field_expr, field_pat));
                    }
                }
                pretty::concat(docs)
            }
            PatternKind::Record { fields } => {
                let mut docs = Vec::new();
                let mut first = true;
                for (name, pat) in fields {
                    if matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                        continue;
                    }
                    if !first {
                        docs.push(pretty::str(" && "));
                    }
                    first = false;
                    let field_expr = TypedExpr::synthetic_typed(
                        ExprKind::Identifier(format!(
                            "{}.{}",
                            self.emit_expr_string(subject),
                            name
                        )),
                        subject.span,
                    );
                    docs.push(self.emit_pattern_condition(&field_expr, pat));
                }
                if first {
                    pretty::str("true")
                } else {
                    pretty::concat(docs)
                }
            }
            PatternKind::Tuple(patterns) => {
                let mut docs = Vec::new();
                let mut first = true;
                for (i, pat) in patterns.iter().enumerate() {
                    if matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                        continue;
                    }
                    if !first {
                        docs.push(pretty::str(" && "));
                    }
                    first = false;
                    let elem_expr = TypedExpr::synthetic_typed(
                        ExprKind::Identifier(format!("{}[{}]", self.emit_expr_string(subject), i)),
                        subject.span,
                    );
                    docs.push(self.emit_pattern_condition(&elem_expr, pat));
                }
                if first {
                    pretty::str("true")
                } else {
                    pretty::concat(docs)
                }
            }
            PatternKind::StringPattern { segments } => {
                let mut s = String::new();
                let subj_str = self.emit_expr_string(subject);
                s.push_str(&subj_str);
                s.push_str(".match(/^");
                for segment in segments {
                    match segment {
                        StringPatternSegment::Literal(lit) => {
                            s.push_str(&escape_regex(lit));
                        }
                        StringPatternSegment::Capture(_) => {
                            s.push_str("([^/]+)");
                        }
                    }
                }
                s.push_str("$/)");
                pretty::str(s)
            }
            PatternKind::Array { elements, rest } => {
                let subj_str = self.emit_expr_string(subject);
                let mut docs = Vec::new();

                if elements.is_empty() && rest.is_none() {
                    docs.push(pretty::str(format!("{subj_str}.length === 0")));
                } else if rest.is_some() {
                    docs.push(pretty::str(format!(
                        "{subj_str}.length >= {}",
                        elements.len()
                    )));
                    for (i, pat) in elements.iter().enumerate() {
                        if !matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                            docs.push(pretty::str(" && "));
                            let elem_expr = TypedExpr::synthetic_typed(
                                ExprKind::Identifier(format!("{subj_str}[{i}]")),
                                subject.span,
                            );
                            docs.push(self.emit_pattern_condition(&elem_expr, pat));
                        }
                    }
                } else {
                    docs.push(pretty::str(format!(
                        "{subj_str}.length === {}",
                        elements.len()
                    )));
                    for (i, pat) in elements.iter().enumerate() {
                        if !matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                            docs.push(pretty::str(" && "));
                            let elem_expr = TypedExpr::synthetic_typed(
                                ExprKind::Identifier(format!("{subj_str}[{i}]")),
                                subject.span,
                            );
                            docs.push(self.emit_pattern_condition(&elem_expr, pat));
                        }
                    }
                }
                pretty::concat(docs)
            }
            PatternKind::Binding(_) | PatternKind::Wildcard => pretty::str("true"),
        }
    }

    fn emit_match_body(
        &mut self,
        subject: &TypedExpr,
        pattern: &Pattern,
        body: &TypedExpr,
    ) -> Document {
        // String patterns need special handling
        if let PatternKind::StringPattern { segments } = &pattern.kind {
            let captures: Vec<&str> = segments
                .iter()
                .filter_map(|seg| match seg {
                    StringPatternSegment::Capture(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();

            if captures.is_empty() && !matches!(body.kind, ExprKind::Block(_)) {
                return self.emit_expr(body);
            }

            let subj_str = self.emit_expr_string(subject);
            let mut s = format!("(() => {{ const _m = {subj_str}.match(/^");
            for segment in segments {
                match segment {
                    StringPatternSegment::Literal(lit) => s.push_str(&escape_regex(lit)),
                    StringPatternSegment::Capture(_) => s.push_str("([^/]+)"),
                }
            }
            s.push_str("$/); ");

            for (i, name) in captures.iter().enumerate() {
                s.push_str(&format!("const {} = _m![{}]; ", name, i + 1));
            }

            if let ExprKind::Block(items) = &body.kind {
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last && matches!(item.kind, ItemKind::Expr(_)) {
                        if let ItemKind::Expr(expr) = &item.kind {
                            s.push_str("return ");
                            s.push_str(&self.emit_expr_string(expr));
                            s.push_str("; ");
                        }
                    } else {
                        let item_doc = self.emit_item(item);
                        s.push_str(&Self::doc_to_string(&item_doc));
                        s.push(' ');
                    }
                }
            } else {
                s.push_str("return ");
                s.push_str(&self.emit_expr_string(body));
                s.push(';');
            }
            s.push_str(" })()");
            return pretty::str(s);
        }

        let subject_str = self.emit_expr_string(subject);
        let bindings = collect_bindings(&subject_str, pattern, &self.ctx.variant_info);
        let needs_iife = !bindings.is_empty() || matches!(body.kind, ExprKind::Block(_));
        if needs_iife {
            let has_await = expr_contains_await(body);
            let mut s = String::new();
            if has_await {
                s.push_str("await (async () => { ");
            } else {
                s.push_str("(() => { ");
            }
            for (name, access) in &bindings {
                s.push_str(&format!("const {name} = {access}; "));
            }
            if let ExprKind::Block(items) = &body.kind {
                for (i, item) in items.iter().enumerate() {
                    let is_last = i == items.len() - 1;
                    if is_last && matches!(item.kind, ItemKind::Expr(_)) {
                        if let ItemKind::Expr(expr) = &item.kind {
                            s.push_str("return ");
                            s.push_str(&self.emit_expr_string(expr));
                            s.push_str("; ");
                        }
                    } else {
                        let item_doc = self.emit_item(item);
                        s.push_str(&Self::doc_to_string(&item_doc));
                        s.push(' ');
                    }
                }
            } else {
                s.push_str("return ");
                s.push_str(&self.emit_expr_string(body));
                s.push(';');
            }
            s.push_str(" })()");
            pretty::str(s)
        } else {
            self.emit_expr(body)
        }
    }

    fn emit_literal_pattern(&self, lit: &LiteralPattern) -> Document {
        match lit {
            LiteralPattern::Number(n) => pretty::str(n),
            LiteralPattern::String(s) => pretty::str(format!("\"{}\"", escape_string(s))),
            LiteralPattern::Bool(b) => pretty::str(if *b { "true" } else { "false" }),
        }
    }
}

/// Collect variable bindings from a match pattern. `subject_str` is the
/// already-rendered JS expression for the subject (e.g. `"user.role"`).
pub(super) fn collect_bindings(
    subject_str: &str,
    pattern: &Pattern,
    variant_info: &HashMap<String, (String, Vec<String>)>,
) -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    collect_bindings_inner(subject_str, pattern, variant_info, &mut bindings);
    bindings
}

fn collect_bindings_inner(
    subject_str: &str,
    pattern: &Pattern,
    variant_info: &HashMap<String, (String, Vec<String>)>,
    bindings: &mut Vec<(String, String)>,
) {
    match &pattern.kind {
        PatternKind::Binding(name) => {
            bindings.push((name.clone(), subject_str.to_string()));
        }
        PatternKind::Variant { name, fields } => {
            let field_names = variant_info.get(name.as_str()).map(|(_, names)| names);
            for (i, field_pat) in fields.iter().enumerate() {
                let field_access = type_layout::variant_field_accessor(
                    name,
                    i,
                    fields.len(),
                    field_names.map(|v| v.as_slice()),
                    subject_str,
                );
                collect_bindings_inner(&field_access, field_pat, variant_info, bindings);
            }
        }
        PatternKind::Record { fields } => {
            for (name, pat) in fields {
                let field_access = format!("{subject_str}.{name}");
                collect_bindings_inner(&field_access, pat, variant_info, bindings);
            }
        }
        PatternKind::Tuple(patterns) => {
            for (i, pat) in patterns.iter().enumerate() {
                let elem_access = format!("{subject_str}[{i}]");
                collect_bindings_inner(&elem_access, pat, variant_info, bindings);
            }
        }
        PatternKind::Array { elements, rest } => {
            for (i, pat) in elements.iter().enumerate() {
                let elem_access = format!("{subject_str}[{i}]");
                collect_bindings_inner(&elem_access, pat, variant_info, bindings);
            }
            if let Some(name) = rest
                && name != "_"
            {
                let rest_access = format!("{subject_str}.slice({})", elements.len());
                bindings.push((name.clone(), rest_access));
            }
        }
        PatternKind::StringPattern { .. } => {}
        PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Range { .. } => {}
    }
}

fn escape_regex(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '/' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$'
            | '|' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}
