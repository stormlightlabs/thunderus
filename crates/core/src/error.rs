use crate::layout::SessionIdError;

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for thunderus-core
pub type Result<T> = std::result::Result<T, Error>;

/// Core error types for the Thunderus agent harness
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error for file operations
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Session-related errors
    #[error("session error: {0}")]
    Session(#[from] SessionError),

    /// Layout/directory structure errors
    #[error("layout error: {0}")]
    Layout(#[from] LayoutError),

    /// Configuration errors
    #[error("configuration error: {0}")]
    Config(String),

    /// Provider errors
    #[error("provider error: {0}")]
    Provider(String),

    /// Tool execution errors
    #[error("tool error: {0}")]
    Tool(String),

    /// Parse/serialization errors
    #[error("parse error: {0}")]
    Parse(String),

    /// Validation errors
    #[error("validation error: {0}")]
    Validation(String),

    /// Generic errors
    #[error("{0}")]
    Other(String),
}

/// Session-specific errors
#[derive(Debug, Error)]
pub enum SessionError {
    /// Session not found
    #[error("session not found: {0}")]
    NotFound(String),

    /// Invalid session ID
    #[error("invalid session ID: {0}")]
    InvalidId(String),

    /// Session already exists
    #[error("session already exists: {0}")]
    AlreadyExists(String),

    /// Corrupted session data
    #[error("corrupted session data: {0}")]
    Corrupted(String),

    /// Events file not found
    #[error("events file not found for session: {0}")]
    EventsNotFound(String),

    /// Invalid event in JSONL
    #[error("invalid event at line {line}: {reason}")]
    InvalidEvent { line: usize, reason: String },
}

/// Layout and directory structure errors
#[derive(Debug, Error)]
pub enum LayoutError {
    /// Agent directory does not exist
    #[error("agent directory does not exist: {0}")]
    AgentDirNotFound(PathBuf),

    /// Sessions directory does not exist
    #[error("sessions directory does not exist: {0}")]
    SessionsDirNotFound(PathBuf),

    /// Views directory does not exist
    #[error("views directory does not exist: {0}")]
    ViewsDirNotFound(PathBuf),

    /// Session directory does not exist
    #[error("session directory does not exist: {0}")]
    SessionDirNotFound(PathBuf),

    /// Patch directory does not exist
    #[error("patch directory does not exist: {0}")]
    PatchesDirNotFound(PathBuf),

    /// Invalid directory structure
    #[error("invalid directory structure: {0}")]
    InvalidStructure(String),

    /// Path outside allowed roots
    #[error("path not in allowed roots: {0}")]
    PathOutsideRoots(PathBuf),

    /// Path traversal detected
    #[error("path traversal detected: {0}")]
    PathTraversal(PathBuf),
}

impl From<SessionIdError> for SessionError {
    fn from(err: SessionIdError) -> Self {
        SessionError::InvalidId(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let io_err = Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        assert_eq!(io_err.to_string(), "I/O error: file not found");

        let config_err = Error::Config("invalid profile".to_string());
        assert_eq!(config_err.to_string(), "configuration error: invalid profile");

        let provider_err = Error::Provider("provider unavailable".to_string());
        assert_eq!(provider_err.to_string(), "provider error: provider unavailable");

        let tool_err = Error::Tool("command failed".to_string());
        assert_eq!(tool_err.to_string(), "tool error: command failed");

        let parse_err = Error::Parse("invalid JSON".to_string());
        assert_eq!(parse_err.to_string(), "parse error: invalid JSON");

        let validation_err = Error::Validation("invalid input".to_string());
        assert_eq!(validation_err.to_string(), "validation error: invalid input");

        let other_err = Error::Other("something went wrong".to_string());
        assert_eq!(other_err.to_string(), "something went wrong");
    }

    #[test]
    fn test_session_error_display() {
        let not_found = SessionError::NotFound("session-123".to_string());
        assert_eq!(not_found.to_string(), "session not found: session-123");

        let invalid_id = SessionError::InvalidId("invalid-session".to_string());
        assert_eq!(invalid_id.to_string(), "invalid session ID: invalid-session");

        let already_exists = SessionError::AlreadyExists("session-456".to_string());
        assert_eq!(already_exists.to_string(), "session already exists: session-456");

        let corrupted = SessionError::Corrupted("session-789".to_string());
        assert_eq!(corrupted.to_string(), "corrupted session data: session-789");

        let events_not_found = SessionError::EventsNotFound("session-abc".to_string());
        assert_eq!(
            events_not_found.to_string(),
            "events file not found for session: session-abc"
        );

        let invalid_event = SessionError::InvalidEvent { line: 42, reason: "missing field".to_string() };
        assert_eq!(invalid_event.to_string(), "invalid event at line 42: missing field");
    }

    #[test]
    fn test_layout_error_display() {
        let path = PathBuf::from("/some/path");
        let agent_dir_not_found = LayoutError::AgentDirNotFound(path.clone());
        assert_eq!(
            agent_dir_not_found.to_string(),
            "agent directory does not exist: /some/path"
        );

        let sessions_not_found = LayoutError::SessionsDirNotFound(path.clone());
        assert_eq!(
            sessions_not_found.to_string(),
            "sessions directory does not exist: /some/path"
        );

        let views_not_found = LayoutError::ViewsDirNotFound(path.clone());
        assert_eq!(
            views_not_found.to_string(),
            "views directory does not exist: /some/path"
        );

        let session_not_found = LayoutError::SessionDirNotFound(path.clone());
        assert_eq!(
            session_not_found.to_string(),
            "session directory does not exist: /some/path"
        );

        let patches_not_found = LayoutError::PatchesDirNotFound(path.clone());
        assert_eq!(
            patches_not_found.to_string(),
            "patch directory does not exist: /some/path"
        );

        let invalid_structure = LayoutError::InvalidStructure("missing .agent".to_string());
        assert_eq!(
            invalid_structure.to_string(),
            "invalid directory structure: missing .agent"
        );

        let path_outside = LayoutError::PathOutsideRoots(path.clone());
        assert_eq!(path_outside.to_string(), "path not in allowed roots: /some/path");

        let path_traversal = LayoutError::PathTraversal(path.clone());
        assert_eq!(path_traversal.to_string(), "path traversal detected: /some/path");
    }

    #[test]
    fn test_error_from_session_error() {
        let session_err = SessionError::NotFound("session-123".to_string());
        let error: Error = session_err.into();
        assert_eq!(error.to_string(), "session error: session not found: session-123");
    }

    #[test]
    fn test_error_from_layout_error() {
        let layout_err = LayoutError::AgentDirNotFound(PathBuf::from("/path"));
        let error: Error = layout_err.into();
        assert_eq!(error.to_string(), "layout error: agent directory does not exist: /path");
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let error: Error = io_err.into();
        assert_eq!(error.to_string(), "I/O error: denied");
    }

    #[test]
    fn test_session_error_from_session_id_error() {
        use crate::layout::SessionIdError;

        let id_err = SessionIdError::Empty;
        let session_err: SessionError = id_err.into();
        assert_eq!(
            session_err.to_string(),
            "invalid session ID: SessionId timestamp cannot be empty"
        );

        let id_err = SessionIdError::InvalidFormat;
        let session_err: SessionError = id_err.into();
        assert_eq!(
            session_err.to_string(),
            "invalid session ID: SessionId has invalid timestamp format"
        );
    }

    #[test]
    fn test_result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: Result<i32> = Err(Error::Other("error".to_string()));
        assert!(err.is_err());
    }
}
