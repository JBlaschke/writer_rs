//! Shared error and result types (ir.md §5).
//! Owner: T01. Stub created by T00.

/// An error produced while processing a document.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error without a more specific category.
    #[error("{0}")]
    Other(String),
}

/// A result using the shared writer error type.
pub type Result<T> = std::result::Result<T, Error>;
