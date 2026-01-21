//! Error types for the memory store

use std::path::PathBuf;
use thiserror::Error;

/// Result type for store operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the memory store
#[derive(Error, Debug)]
pub enum Error {
    /// SQLite database error
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Document not found
    #[error("Document not found: namespace={namespace}, key={key}")]
    NotFound { namespace: String, key: String },

    /// Document already exists
    #[error("Document already exists: namespace={namespace}, key={key}")]
    AlreadyExists { namespace: String, key: String },

    /// Invalid search query
    #[error("Invalid search query: {0}")]
    InvalidQuery(String),

    /// Database corruption or schema mismatch
    #[error("Database error: {0}")]
    Database(String),

    /// Unknown memory path
    #[error("Unknown memory path: {0}")]
    UnknownMemoryPath(PathBuf),

    /// Metadata validation error
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),

    /// Token budget exceeded
    #[error("Token budget exceeded: requested={requested}, budget={budget}")]
    TokenBudgetExceeded { requested: usize, budget: usize },

    #[error("Connection error: {0}")]
    ConnectionError(#[from] tokio_rusqlite::Error),
}

impl Error {
    /// Create a database error with a message
    pub fn database(msg: impl Into<String>) -> Self {
        Self::Database(msg.into())
    }

    /// Create an invalid query error
    pub fn invalid_query(msg: impl Into<String>) -> Self {
        Self::InvalidQuery(msg.into())
    }

    /// Create an invalid metadata error
    pub fn invalid_metadata(msg: impl Into<String>) -> Self {
        Self::InvalidMetadata(msg.into())
    }

    /// Create a not found error
    pub fn not_found(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self::NotFound { namespace: namespace.into(), key: key.into() }
    }

    /// Create an already exists error
    pub fn already_exists(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self::AlreadyExists { namespace: namespace.into(), key: key.into() }
    }

    /// Create a token budget exceeded error
    pub fn token_budget_exceeded(requested: usize, budget: usize) -> Self {
        Self::TokenBudgetExceeded { requested, budget }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::invalid_query("test query");
        assert_eq!(err.to_string(), "Invalid search query: test query");

        let err = Error::not_found("core", "CORE.md");
        assert!(err.to_string().contains("core"));
        assert!(err.to_string().contains("CORE.md"));
    }

    #[test]
    fn test_error_from_sqlite() {
        let sqlite_err = rusqlite::Error::InvalidPath("test path".into());
        let err: Error = sqlite_err.into();
        assert!(matches!(err, Error::Sqlite(_)));
    }

    #[test]
    fn test_error_from_json() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Json(_)));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn test_token_budget_exceeded() {
        let err = Error::token_budget_exceeded(5000, 2000);
        assert!(matches!(
            err,
            Error::TokenBudgetExceeded { requested: 5000, budget: 2000 }
        ));
        assert_eq!(err.to_string(), "Token budget exceeded: requested=5000, budget=2000");
    }

    #[test]
    fn test_already_exists() {
        let err = Error::already_exists("core", "CORE.md");
        assert!(matches!(err, Error::AlreadyExists { .. }));
        assert!(err.to_string().contains("already exists"));
    }
}
