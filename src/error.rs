use std::path::PathBuf;

use thiserror::Error;

use crate::diagnostic::Diagnostic;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error at {path}: {source}")]
    SExpr {
        path: PathBuf,
        #[source]
        source: kiutils_sexpr::ParseError,
    },
    #[error(
        "validation error at {path}{span_suffix}: {message}",
        span_suffix = diagnostic.display_span_suffix(),
        message = diagnostic.message
    )]
    Validation {
        path: PathBuf,
        diagnostic: Diagnostic,
    },
}
