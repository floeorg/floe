pub mod ast;

#[cfg(test)]
mod tests;

use crate::cst::{CstError, CstParser, Parse as CstParse};
use crate::lexer::Lexer;
use crate::lexer::span::Span;
use crate::lower::{lower_program, lower_program_lossy};
use crate::parse::ModuleExtra;
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
        let cst_parse = lex_and_cst(source);
        if !cst_parse.errors.is_empty() {
            return Err(classify_cst_errors(cst_parse.errors));
        }
        lower_program(&cst_parse.syntax(), source)
    }

    /// Parse a complete module, returning the AST alongside the parse-time
    /// side channel ([`ModuleExtra`]) that records comments, doc comments, and
    /// empty line positions. Consumers that don't need the side channel can
    /// use [`Parser::parse`] instead — it skips the `ModuleExtra` construction
    /// pass, which matters for compiler hot paths that discard the extras.
    pub fn parse_module(source: &str) -> Result<(Program, ModuleExtra), Vec<ParseError>> {
        let (cst_parse, extra) = lex_and_cst_with_extra(source);
        if !cst_parse.errors.is_empty() {
            return Err(classify_cst_errors(cst_parse.errors));
        }
        lower_program(&cst_parse.syntax(), source).map(|program| (program, extra))
    }

    /// Parse on a best-effort basis, returning whatever AST was successfully
    /// built along with any parse errors. Used by the LSP so that a partial
    /// symbol index can be built even when the source has errors.
    pub fn parse_lossy(source: &str) -> (Program, Vec<ParseError>) {
        let cst_parse = lex_and_cst(source);
        let root = cst_parse.syntax();
        let mut errors = classify_cst_errors(cst_parse.errors);
        let (program, lower_errors) = lower_program_lossy(&root, source);
        errors.extend(lower_errors);
        (program, errors)
    }

    /// Best-effort parse that also returns the [`ModuleExtra`] side channel.
    pub fn parse_lossy_module(source: &str) -> (Program, ModuleExtra, Vec<ParseError>) {
        let (cst_parse, extra) = lex_and_cst_with_extra(source);
        let root = cst_parse.syntax();
        let mut errors = classify_cst_errors(cst_parse.errors);
        let (program, lower_errors) = lower_program_lossy(&root, source);
        errors.extend(lower_errors);
        (program, extra, errors)
    }
}

fn lex_and_cst(source: &str) -> CstParse {
    let tokens = Lexer::new(source).tokenize_with_trivia();
    CstParser::new(source, tokens).parse()
}

fn lex_and_cst_with_extra(source: &str) -> (CstParse, ModuleExtra) {
    let tokens = Lexer::new(source).tokenize_with_trivia();
    let extra = ModuleExtra::from_tokens(source, &tokens);
    let parse = CstParser::new(source, tokens).parse();
    (parse, extra)
}

fn classify_cst_errors(errors: Vec<CstError>) -> Vec<ParseError> {
    errors
        .into_iter()
        .map(|e| ParseError {
            kind: ParseErrorKind::classify(&e.message),
            message: e.message,
            span: e.span,
        })
        .collect()
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
