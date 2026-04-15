pub mod ast;

#[cfg(test)]
mod tests;

use crate::cst::CstParser;
use crate::lexer::Lexer;
use crate::lexer::span::Span;
use crate::lower::{lower_program, lower_program_lossy};
use ast::*;

/// Classification of parse errors for structured diagnostic handling.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// A banned keyword was used (e.g. `let`, `var`).
    BannedKeyword,
    /// An unexpected token was encountered.
    UnexpectedToken,
    /// A JSX closing tag did not match the opening tag.
    MismatchedTag,
    /// General parse error (default).
    General,
}

impl ParseErrorKind {
    /// Classify a parse error message into a kind.
    pub fn classify(message: &str) -> Self {
        if message.contains("banned keyword") {
            Self::BannedKeyword
        } else if message.contains("expected") {
            Self::UnexpectedToken
        } else if message.contains("mismatched closing tag") {
            Self::MismatchedTag
        } else {
            Self::General
        }
    }
}

/// A parse error with location and message.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    pub kind: ParseErrorKind,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

/// The Floe parser. Parses source code into an AST via the CST pipeline.
///
/// This is a thin wrapper around the CST parser + lowerer. All parsing goes
/// through the lossless CST and is then lowered to the typed AST.
pub struct Parser;

impl Parser {
    /// Create a parser handle. This is a convenience method that mirrors the
    /// old API: `Parser::new(source).parse_program()`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(source: &str) -> ParserHandle<'_> {
        ParserHandle { source }
    }

    /// Parse a complete program using the CST pipeline (lexer -> CST -> lower -> AST).
    pub fn parse(source: &str) -> Result<Program, Vec<ParseError>> {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        let cst_parse = CstParser::new(source, tokens).parse();

        if !cst_parse.errors.is_empty() {
            return Err(cst_parse
                .errors
                .into_iter()
                .map(|e| {
                    let kind = ParseErrorKind::classify(&e.message);
                    ParseError {
                        message: e.message,
                        span: e.span,
                        kind,
                    }
                })
                .collect());
        }

        let root = cst_parse.syntax();
        lower_program(&root, source)
    }

    /// Parse on a best-effort basis, returning whatever AST was successfully
    /// built along with any parse errors. Used by the LSP so that a partial
    /// symbol index can be built even when the source has errors.
    pub fn parse_lossy(source: &str) -> (Program, Vec<ParseError>) {
        let tokens = Lexer::new(source).tokenize_with_trivia();
        let cst_parse = CstParser::new(source, tokens).parse();

        let cst_errors: Vec<ParseError> = cst_parse
            .errors
            .iter()
            .map(|e| {
                let kind = ParseErrorKind::classify(&e.message);
                ParseError {
                    message: e.message.clone(),
                    span: e.span,
                    kind,
                }
            })
            .collect();

        let root = cst_parse.syntax();
        let (program, lower_errors) = lower_program_lossy(&root, source);

        let mut all_errors = cst_errors;
        all_errors.extend(lower_errors);
        (program, all_errors)
    }
}

/// Handle returned by `Parser::new(source)` that allows calling `parse_program()`.
/// This preserves the old `Parser::new(source).parse_program()` API.
pub struct ParserHandle<'a> {
    source: &'a str,
}

impl ParserHandle<'_> {
    /// Parse a complete program using the CST pipeline.
    pub fn parse_program(&self) -> Result<Program, Vec<ParseError>> {
        Parser::parse(self.source)
    }
}
