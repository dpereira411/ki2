use std::fmt;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("unexpected token at byte {0}")]
    UnexpectedToken(usize),
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("maximum nesting exceeded at byte {0}")]
    MaxNestingExceeded(usize),
    #[error("expected a single root expression, found {0}")]
    ExpectedSingleRoot(usize),
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}
