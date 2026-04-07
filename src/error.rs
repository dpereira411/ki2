use std::path::PathBuf;

use thiserror::Error;

use crate::diagnostic::Diagnostic;
use crate::sexpr::ParseError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error at {path}{location}: {source}")]
    SExpr {
        path: PathBuf,
        location: String,
        #[source]
        source: ParseError,
    },
    #[error(
        "parse error at {path}{span_suffix}: {message}",
        span_suffix = diagnostic.display_span_suffix(),
        message = diagnostic.message
    )]
    Validation {
        path: PathBuf,
        diagnostic: Diagnostic,
    },
}
