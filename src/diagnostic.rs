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
}
