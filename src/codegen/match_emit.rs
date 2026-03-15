use std::collections::HashMap;

use crate::parser::ast::*;

use super::Codegen;

impl Codegen {
    // ── Match Lowering ───────────────────────────────────────────

    pub(super) fn emit_match(&mut self, subject: &Expr, arms: &[MatchArm]) {
        // Emit as nested ternary: `subject.tag === "A" ? ... : subject.tag === "B" ? ... : unreachable()`
        self.emit_match_arms(subject, arms, 0);
    }

    fn emit_match_arms(&mut self, subject: &Expr, arms: &[MatchArm], index: usize) {
        if index >= arms.len() {
            // Should be unreachable if match is exhaustive
            self.push("(() => { throw new Error(\"non-exhaustive match\"); })()");
            return;
        }

        let arm = &arms[index];
        let is_last = index == arms.len() - 1;

        // Wildcard or binding at the end without a guard → just emit the body
        if is_last
            && arm.guard.is_none()
            && matches!(
                arm.pattern.kind,
                PatternKind::Wildcard | PatternKind::Binding(_)
            )
        {
            self.emit_match_body(subject, &arm.pattern, &arm.body);
            return;
        }

        // For guards with bindings, we need an IIFE so bindings are in scope
        // for the guard expression
        if let Some(guard) = &arm.guard {
            let bindings = collect_bindings(
                subject,
                &arm.pattern,
                &|s| self.expr_to_string(s),
                &self.variant_info,
            );
            let has_bindings = !bindings.is_empty();

            if has_bindings {
                // Emit pattern condition first, then IIFE for bindings + guard
                self.emit_pattern_condition(subject, &arm.pattern);
                self.push(" ? ");
                self.push("(() => { ");
                for (name, access) in &bindings {
                    self.push(&format!("const {name} = {access}; "));
                }
                self.push("if (");
                self.emit_expr(guard);
                self.push(") { return ");
                self.emit_expr(&arm.body);
                self.push("; } ");
                // Fall through to next arm
                self.push("return ");
                self.emit_match_arms(subject, arms, index + 1);
                self.push("; })()");
            } else {
                // No bindings needed for guard - simpler inline condition
                let is_trivial_pattern = matches!(
                    arm.pattern.kind,
                    PatternKind::Wildcard | PatternKind::Binding(_)
                );
                if !is_trivial_pattern {
                    self.emit_pattern_condition(subject, &arm.pattern);
                    self.push(" && ");
                }
                self.emit_expr(guard);
                self.push(" ? ");
                self.emit_match_body(subject, &arm.pattern, &arm.body);
                self.push(" : ");
                if is_last {
                    self.push("(() => { throw new Error(\"non-exhaustive match\"); })()");
                } else {
                    self.emit_match_arms(subject, arms, index + 1);
                }
            }
        } else {
            self.emit_pattern_condition(subject, &arm.pattern);
            self.push(" ? ");
            self.emit_match_body(subject, &arm.pattern, &arm.body);
            self.push(" : ");

            if is_last {
                self.push("(() => { throw new Error(\"non-exhaustive match\"); })()");
            } else {
                self.emit_match_arms(subject, arms, index + 1);
            }
        }
    }

    fn emit_pattern_condition(&mut self, subject: &Expr, pattern: &Pattern) {
        match &pattern.kind {
            PatternKind::Literal(lit) => {
                self.emit_expr(subject);
                self.push(" === ");
                self.emit_literal_pattern(lit);
            }
            PatternKind::Range { start, end } => {
                self.push("(");
                self.emit_expr(subject);
                self.push(" >= ");
                self.emit_literal_pattern(start);
                self.push(" && ");
                self.emit_expr(subject);
                self.push(" <= ");
                self.emit_literal_pattern(end);
                self.push(")");
            }
            PatternKind::Variant { name, fields } => {
                // Check tag
                self.emit_expr(subject);
                self.push(&format!(".tag === \"{}\"", name));

                // Nested conditions for sub-patterns
                for (i, field_pat) in fields.iter().enumerate() {
                    if !matches!(
                        field_pat.kind,
                        PatternKind::Wildcard | PatternKind::Binding(_)
                    ) {
                        self.push(" && ");
                        // Access the field — for single-field variants use .value
                        let field_access = if fields.len() == 1 {
                            format!("{}.value", self.expr_to_string(subject))
                        } else {
                            format!("{}._{i}", self.expr_to_string(subject))
                        };
                        let field_expr = Expr {
                            kind: ExprKind::Identifier(field_access),
                            span: subject.span,
                        };
                        self.emit_pattern_condition(&field_expr, field_pat);
                    }
                }
            }
            PatternKind::Record { fields } => {
                let mut first = true;
                for (name, pat) in fields {
                    if matches!(pat.kind, PatternKind::Wildcard | PatternKind::Binding(_)) {
                        continue;
                    }
                    if !first {
                        self.push(" && ");
                    }
                    first = false;
                    let field_expr = Expr {
                        kind: ExprKind::Identifier(format!(
                            "{}.{}",
                            self.expr_to_string(subject),
                            name
                        )),
                        span: subject.span,
                    };
                    self.emit_pattern_condition(&field_expr, pat);
                }
                if first {
                    // All fields are bindings/wildcards — always true
                    self.push("true");
                }
            }
            PatternKind::StringPattern { segments } => {
                // Emit: subject.match(/^...regex...$/)
                self.emit_expr(subject);
                self.push(".match(/^");
                for segment in segments {
                    match segment {
                        StringPatternSegment::Literal(s) => {
                            self.push(&escape_regex(s));
                        }
                        StringPatternSegment::Capture(_) => {
                            self.push("([^/]+)");
                        }
                    }
                }
                self.push("$/)");
            }
            PatternKind::Binding(_) | PatternKind::Wildcard => {
                self.push("true");
            }
        }
    }

    fn emit_match_body(&mut self, subject: &Expr, pattern: &Pattern, body: &Expr) {
        // String patterns need special handling: extract captures from regex match
        if let PatternKind::StringPattern { segments } = &pattern.kind {
            let captures: Vec<&str> = segments
                .iter()
                .filter_map(|seg| match seg {
                    StringPatternSegment::Capture(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();

            if captures.is_empty() && !matches!(body.kind, ExprKind::Block(_)) {
                self.emit_expr(body);
                return;
            }

            self.push("(() => { const _m = ");
            self.emit_expr(subject);
            self.push(".match(/^");
            for segment in segments {
                match segment {
                    StringPatternSegment::Literal(s) => {
                        self.push(&escape_regex(s));
                    }
                    StringPatternSegment::Capture(_) => {
                        self.push("([^/]+)");
                    }
                }
            }
            self.push("$/); ");

            for (i, name) in captures.iter().enumerate() {
                self.push(&format!("const {} = _m![{}]; ", name, i + 1));
            }

            if matches!(body.kind, ExprKind::Block(_)) {
                if let ExprKind::Block(items) = &body.kind {
                    for item in items {
                        self.emit_item(item);
                        self.push(" ");
                    }
                }
            } else {
                self.push("return ");
                self.emit_expr(body);
                self.push(";");
            }
            self.push(" })()");
            return;
        }

        // For patterns with bindings, wrap in an IIFE to introduce variables
        let bindings = collect_bindings(
            subject,
            pattern,
            &|s| self.expr_to_string(s),
            &self.variant_info,
        );
        let needs_iife = !bindings.is_empty() || matches!(body.kind, ExprKind::Block(_));
        if needs_iife {
            self.push("(() => { ");
            for (name, access) in &bindings {
                self.push(&format!("const {name} = {access}; "));
            }
            if matches!(body.kind, ExprKind::Block(_)) {
                // For block bodies, emit statements directly inside the IIFE
                if let ExprKind::Block(items) = &body.kind {
                    for item in items {
                        self.emit_item(item);
                        self.push(" ");
                    }
                }
            } else {
                self.push("return ");
                self.emit_expr(body);
                self.push(";");
            }
            self.push(" })()");
        } else {
            self.emit_expr(body);
        }
    }

    fn emit_literal_pattern(&mut self, lit: &LiteralPattern) {
        match lit {
            LiteralPattern::Number(n) => self.push(n),
            LiteralPattern::String(s) => self.push(&format!("\"{}\"", super::escape_string(s))),
            LiteralPattern::Bool(b) => self.push(if *b { "true" } else { "false" }),
        }
    }
}

/// Collect variable bindings from a match pattern.
pub(super) fn collect_bindings(
    subject: &Expr,
    pattern: &Pattern,
    expr_to_str: &dyn Fn(&Expr) -> String,
    variant_info: &HashMap<String, (String, Vec<String>)>,
) -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    collect_bindings_inner(subject, pattern, expr_to_str, variant_info, &mut bindings);
    bindings
}

fn collect_bindings_inner(
    subject: &Expr,
    pattern: &Pattern,
    expr_to_str: &dyn Fn(&Expr) -> String,
    variant_info: &HashMap<String, (String, Vec<String>)>,
    bindings: &mut Vec<(String, String)>,
) {
    match &pattern.kind {
        PatternKind::Binding(name) => {
            bindings.push((name.clone(), expr_to_str(subject)));
        }
        PatternKind::Variant { name, fields } => {
            // Look up field names from variant definition
            let field_names = variant_info.get(name.as_str()).map(|(_, names)| names);
            for (i, field_pat) in fields.iter().enumerate() {
                let field_access = if let Some(names) = field_names
                    && let Some(fname) = names.get(i)
                {
                    format!("{}.{}", expr_to_str(subject), fname)
                } else if fields.len() == 1 {
                    format!("{}.value", expr_to_str(subject))
                } else {
                    format!("{}._{i}", expr_to_str(subject))
                };
                let field_expr = Expr {
                    kind: ExprKind::Identifier(field_access.clone()),
                    span: subject.span,
                };
                collect_bindings_inner(&field_expr, field_pat, expr_to_str, variant_info, bindings);
            }
        }
        PatternKind::Record { fields } => {
            for (name, pat) in fields {
                let field_access = format!("{}.{}", expr_to_str(subject), name);
                let field_expr = Expr {
                    kind: ExprKind::Identifier(field_access.clone()),
                    span: subject.span,
                };
                collect_bindings_inner(&field_expr, pat, expr_to_str, variant_info, bindings);
            }
        }
        PatternKind::StringPattern { .. } => {
            // String pattern bindings are handled directly in emit_match_body
        }
        PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Range { .. } => {}
    }
}

/// Escape special regex characters in a literal string segment.
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
