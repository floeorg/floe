//! Structured diagnostic accumulator for the type checker.
//!
//! Replaces the raw `Vec<Diagnostic>` that the checker previously held.
//! The main benefits over a plain Vec:
//! - `sort()` orders diagnostics by source location so the user sees
//!   the root cause first, not a cascade artifact.
//! - `errors()` / `warnings()` filter without external logic.
//! - `has_errors()` is a one-call check for "should we block codegen?"

use crate::diagnostic::{Diagnostic, Severity};
use crate::lexer::span::Span;

use super::error_codes::ErrorCode;

/// Accumulates diagnostics during type checking. Errors and warnings
/// are stored together and can be separated at reporting time.
#[derive(Default)]
pub struct Problems {
    diagnostics: Vec<Diagnostic>,
}

impl Problems {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::error(msg, span)
                .with_label(label)
                .with_error_code(code),
        );
    }

    pub fn error_with_help(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::error(msg, span)
                .with_label(label)
                .with_help(help)
                .with_error_code(code),
        );
    }

    pub fn warning_with_help(
        &mut self,
        msg: impl Into<String>,
        span: Span,
        code: ErrorCode,
        label: impl Into<String>,
        help: impl Into<String>,
    ) {
        self.diagnostics.push(
            Diagnostic::warning(msg, span)
                .with_label(label)
                .with_help(help)
                .with_error_code(code),
        );
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Sort diagnostics by source location so the first error in the
    /// file is the first the user sees, regardless of traversal order.
    pub fn sort(&mut self) {
        self.diagnostics
            .sort_by_key(|d| (d.span.line, d.span.column));
    }

    pub fn take(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter()
    }

    pub fn has_error_within_span(&self, span: Span) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && span.contains_span(d.span))
    }
}
