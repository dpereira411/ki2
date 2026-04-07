use std::path::PathBuf;

use kiutils_sexpr::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    Generic,
    Validation,
    Expecting { expected: String },
    Unexpected { found: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub kind: DiagnosticKind,
    pub message: String,
    pub path: Option<PathBuf>,
    pub span: Option<Span>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl Diagnostic {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            kind: DiagnosticKind::Generic,
            message: message.into(),
            path: None,
            span: None,
            line: None,
            column: None,
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            kind: DiagnosticKind::Validation,
            message: message.into(),
            path: None,
            span: None,
            line: None,
            column: None,
        }
    }

    pub fn expecting(code: &'static str, expected: impl Into<String>) -> Self {
        let expected = expected.into();
        Self {
            severity: Severity::Error,
            code,
            kind: DiagnosticKind::Expecting {
                expected: expected.clone(),
            },
            message: format!("expecting {expected}"),
            path: None,
            span: None,
            line: None,
            column: None,
        }
    }

    pub fn unexpected(code: &'static str, found: impl Into<String>) -> Self {
        let found = found.into();
        Self {
            severity: Severity::Error,
            code,
            kind: DiagnosticKind::Unexpected {
                found: found.clone(),
            },
            message: format!("unexpected {found}"),
            path: None,
            span: None,
            line: None,
            column: None,
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_position(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    // Upstream parity: local render helper for validation error locations. This is not a 1:1
    // KiCad routine because the current CLI still exposes local diagnostic objects, but the
    // rendered error text is now intentionally biased toward KiCad-style line/column wording
    // instead of repo-local byte-span noise when both are available. Raw spans are still preserved
    // on the diagnostic object for programmatic use.
    pub fn display_span_suffix(&self) -> String {
        match (self.line, self.column, self.span) {
            (Some(line), Some(column), Some(_span)) => format!(":{line}:{column}"),
            (Some(line), Some(column), None) => format!(":{line}:{column}"),
            (None, None, Some(span)) => format!(":{}..{}", span.start, span.end),
            _ => String::new(),
        }
    }
}
