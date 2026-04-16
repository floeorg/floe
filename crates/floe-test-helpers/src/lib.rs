//! Shared test utilities for Floe compiler crates.
//!
//! Provides common helpers for parsing, type-checking, and asserting
//! diagnostics across `floe-core`, `floe-lsp`, and other crates.

use floe_core::checker::{Checker, ErrorCode};
use floe_core::diagnostic::{Diagnostic, Severity};
use floe_core::parser::Parser;

/// Parse and type-check a source string, returning all diagnostics.
///
/// Panics if parsing fails (use this only when the source is expected to parse).
pub fn check(source: &str) -> Vec<Diagnostic> {
    let program = Parser::new(source)
        .parse_program()
        .expect("parse should succeed");
    Checker::new().check(&program)
}

/// Returns `true` if any diagnostic has the given error code.
pub fn has_error(diagnostics: &[Diagnostic], code: ErrorCode) -> bool {
    diagnostics
        .iter()
        .any(|d| d.code.as_deref() == Some(code.code()))
}

/// Returns `true` if any error-severity diagnostic contains `text` in its message.
pub fn has_error_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.message.contains(text))
}

/// Returns `true` if any warning-severity diagnostic contains `text` in its message.
pub fn has_warning_containing(diagnostics: &[Diagnostic], text: &str) -> bool {
    diagnostics
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains(text))
}
