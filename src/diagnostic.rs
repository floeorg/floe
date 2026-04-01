use crate::lexer::span::Span;

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Help,
}

/// A compiler diagnostic with source location, message, and optional help text.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    /// Optional label shown inline at the span location.
    pub label: Option<String>,
    /// Optional help/suggestion text shown below.
    pub help: Option<String>,
    /// Error code for categorization (e.g., "E001").
    pub code: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span,
            label: None,
            help: None,
            code: None,
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span,
            label: None,
            help: None,
            code: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_error_code(mut self, code: crate::checker::error_codes::ErrorCode) -> Self {
        self.code = Some(code.code().to_string());
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Help => "help",
        };
        if let Some(code) = &self.code {
            write!(f, "{prefix}[{code}]")?;
        } else {
            write!(f, "{prefix}")?;
        }
        write!(
            f,
            " at {}:{}: {}",
            self.span.line, self.span.column, self.message
        )?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {help}")?;
        }
        Ok(())
    }
}

/// Render diagnostics with pretty source annotations using ariadne.
pub fn render_diagnostics(filename: &str, source: &str, diagnostics: &[Diagnostic]) -> String {
    use ariadne::{Color, Label, Report, ReportKind, Source};

    let mut output = Vec::new();

    for diag in diagnostics {
        let kind = match diag.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
            Severity::Help => ReportKind::Advice,
        };

        let span = (filename, diag.span.start..diag.span.end);
        let mut builder = Report::build(kind, span.clone());

        if let Some(code) = &diag.code {
            builder = builder.with_code(code);
        }

        builder = builder.with_message(&diag.message);

        let label_text = diag.label.as_deref().unwrap_or(&diag.message);
        let color = match diag.severity {
            Severity::Error => Color::Red,
            Severity::Warning => Color::Yellow,
            Severity::Help => Color::Cyan,
        };

        builder = builder.with_label(Label::new(span).with_message(label_text).with_color(color));

        if let Some(help) = &diag.help {
            builder = builder.with_help(help);
        }

        let report = builder.finish();
        report
            .write((filename, Source::from(source)), &mut output)
            .expect("failed to write diagnostic");
    }

    String::from_utf8(output).expect("diagnostic output was not valid utf-8")
}

/// Convert parser errors to diagnostics.
pub fn from_parse_errors(errors: &[crate::parser::ParseError]) -> Vec<Diagnostic> {
    use crate::parser::ParseErrorKind;

    errors
        .iter()
        .map(|e| {
            let mut diag = Diagnostic::error(&e.message, e.span);

            match &e.kind {
                ParseErrorKind::BannedKeyword => {
                    if let Some(help_start) = e.message.find(": ") {
                        let help_text = &e.message[help_start + 2..];
                        diag = diag.with_label("banned in Floe").with_help(help_text);
                    }
                }
                ParseErrorKind::UnexpectedToken => {
                    diag = diag.with_label("unexpected token here");
                }
                ParseErrorKind::MismatchedTag => {
                    diag = diag.with_label("mismatched tag");
                }
                ParseErrorKind::General => {}
            }

            diag
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let diag = Diagnostic::error("unexpected token", Span::new(0, 3, 1, 1));
        assert!(diag.to_string().contains("error"));
        assert!(diag.to_string().contains("unexpected token"));
    }

    #[test]
    fn error_with_code() {
        let diag = Diagnostic::error("banned keyword", Span::new(0, 3, 1, 1))
            .with_code("E001")
            .with_help("use const instead");
        let s = diag.to_string();
        assert!(s.contains("[E001]"));
        assert!(s.contains("help: use const instead"));
    }

    #[test]
    fn warning_display() {
        let diag = Diagnostic::warning("unused variable", Span::new(5, 8, 1, 6));
        assert!(diag.to_string().contains("warning"));
    }

    #[test]
    fn render_single_error() {
        let diag = Diagnostic::error("unexpected token", Span::new(6, 7, 1, 7)).with_label("here");
        let output = render_diagnostics("test.fl", "const ? = 42", &[diag]);
        assert!(output.contains("unexpected token"));
        assert!(output.contains("test.fl"));
    }

    #[test]
    fn render_banned_keyword() {
        let diag = Diagnostic::error("banned keyword 'let'", Span::new(0, 3, 1, 1))
            .with_label("banned in Floe")
            .with_help("Use `const` - all bindings are immutable in Floe");
        let output = render_diagnostics("test.fl", "let x = 42", &[diag]);
        assert!(output.contains("banned"));
        assert!(output.contains("const"));
    }

    #[test]
    fn render_multiple_errors() {
        let diags = vec![
            Diagnostic::error("first error", Span::new(0, 3, 1, 1)),
            Diagnostic::error("second error", Span::new(10, 13, 1, 11)),
        ];
        let output = render_diagnostics("test.fl", "let x = 42\nlet y = 20", &diags);
        assert!(output.contains("first error"));
        assert!(output.contains("second error"));
    }

    #[test]
    fn from_parse_errors_converts() {
        let parse_errors = vec![crate::parser::ParseError {
            message: "expected identifier, found Number".to_string(),
            span: Span::new(0, 3, 1, 1),
            kind: crate::parser::ParseErrorKind::UnexpectedToken,
        }];
        let diags = from_parse_errors(&parse_errors);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Error);
        assert!(diags[0].label.is_some());
    }
}
