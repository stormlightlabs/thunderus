use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Base directory name for agent data (repo-local, versionable)
pub const AGENT_DIR_NAME: &str = ".agent";

/// Subdirectory for session data
pub const SESSIONS_DIR: &str = "sessions";

/// Filename for JSONL event logs within a session
pub const EVENTS_FILE: &str = "events.jsonl";

/// Subdirectory for patches within a session
pub const PATCHES_DIR: &str = "patches";

/// Pattern for patch files
pub const PATCH_FILE_PATTERN: &str = "*.patch";

/// Subdirectory for materialized Markdown views
pub const VIEWS_DIR: &str = "views";

/// Materialized view filenames
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewFile {
    /// Always-loaded "project memory" (Claude-style)
    Memory,
    /// Current plan + checkpoints
    Plan,
    /// Architectural Decision Records (ADR-lite)
    Decisions,
}

impl ViewFile {
    /// Get the filename for this view
    pub fn filename(&self) -> &'static str {
        match self {
            ViewFile::Memory => "MEMORY.md",
            ViewFile::Plan => "PLAN.md",
            ViewFile::Decisions => "DECISIONS.md",
        }
    }

    /// All view files in order
    pub fn all() -> &'static [ViewFile] {
        &[ViewFile::Memory, ViewFile::Plan, ViewFile::Decisions]
    }
}

impl fmt::Display for ViewFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.filename())
    }
}

/// Session identifier based on timestamp (ISO 8601 format)
///
/// Format: `YYYY-MM-DDTHH-MM-SS-Z` (RFC 3339 format, safe for filenames)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Create a new SessionId with current time
    pub fn new() -> Self {
        Self::now()
    }

    /// Create a SessionId from current timestamp
    pub fn now() -> Self {
        let now = chrono::Utc::now();
        let timestamp = now.format("%Y-%m-%dT%H-%M-%SZ").to_string();
        Self(timestamp)
    }

    /// Create a SessionId from a timestamp string
    ///
    /// The string should be in ISO 8601-like format (RFC 3339)
    pub fn from_timestamp(timestamp: impl Into<String>) -> Result<Self, SessionIdError> {
        let ts = timestamp.into();
        Self::validate_timestamp(&ts)?;
        Ok(Self(ts))
    }

    /// Get the timestamp string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate timestamp format
    fn validate_timestamp(ts: &str) -> Result<(), SessionIdError> {
        if ts.is_empty() {
            return Err(SessionIdError::Empty);
        }

        if !ts
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == 'T' || c == 'Z' || c == ':')
        {
            return Err(SessionIdError::InvalidFormat);
        }
        Ok(())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for SessionId {
    type Error = SessionIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_timestamp(value)
    }
}

impl TryFrom<&str> for SessionId {
    type Error = SessionIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_timestamp(value.to_string())
    }
}

/// Errors that can occur when creating or parsing SessionId
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionIdError {
    /// Timestamp string is empty
    Empty,
    /// Timestamp format is invalid
    InvalidFormat,
}

impl fmt::Display for SessionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionIdError::Empty => write!(f, "SessionId timestamp cannot be empty"),
            SessionIdError::InvalidFormat => write!(f, "SessionId has invalid timestamp format"),
        }
    }
}

impl std::error::Error for SessionIdError {}

/// Represents the `.agent/` directory layout and provides path resolution
#[derive(Debug, Clone)]
pub struct AgentDir {
    /// Root directory containing the `.agent/` folder
    root: PathBuf,
}

impl AgentDir {
    /// Create a new AgentDir from the given root directory
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }

    /// Create AgentDir from current working directory
    pub fn from_current_dir() -> std::io::Result<Self> {
        Ok(Self::new(std::env::current_dir()?))
    }

    /// Get the root directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the `.agent/` directory path
    pub fn agent_dir(&self) -> PathBuf {
        self.root.join(AGENT_DIR_NAME)
    }

    /// Get the sessions directory path (`.agent/sessions/`)
    pub fn sessions_dir(&self) -> PathBuf {
        self.agent_dir().join(SESSIONS_DIR)
    }

    /// Get the views directory path (`.agent/views/`)
    pub fn views_dir(&self) -> PathBuf {
        self.agent_dir().join(VIEWS_DIR)
    }

    /// Get path to a session directory (`.agent/sessions/<timestamp>/`)
    pub fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        self.sessions_dir().join(session_id.as_str())
    }

    /// Get path to events file for a session (`.agent/sessions/<timestamp>/events.jsonl`)
    pub fn events_file(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join(EVENTS_FILE)
    }

    /// Get path to patches directory for a session (`.agent/sessions/<timestamp>/patches/`)
    pub fn patches_dir(&self, session_id: &SessionId) -> PathBuf {
        self.session_dir(session_id).join(PATCHES_DIR)
    }

    /// Get path to all patches in a session (glob pattern)
    pub fn patches_glob(&self, session_id: &SessionId) -> PathBuf {
        let patches_dir = self.patches_dir(session_id);
        patches_dir.join(PATCH_FILE_PATTERN)
    }

    /// Get path to a specific patch file (`.agent/sessions/<timestamp>/patches/<name>.patch`)
    pub fn patch_file(&self, session_id: &SessionId, patch_name: &str) -> PathBuf {
        let patch_name = if patch_name.ends_with(".patch") {
            patch_name.to_string()
        } else {
            format!("{}.patch", patch_name)
        };
        self.patches_dir(session_id).join(patch_name)
    }

    /// Get path to a view file (`.agent/views/<MEMORY.md|PLAN.md|DECISIONS.md>`)
    pub fn view_file(&self, view: ViewFile) -> PathBuf {
        self.views_dir().join(view.filename())
    }

    /// Get all view file paths
    pub fn all_view_files(&self) -> Vec<PathBuf> {
        ViewFile::all().iter().map(|view| self.view_file(*view)).collect()
    }

    /// List all available sessions, sorted by timestamp (newest first)
    ///
    /// Returns SessionId for each valid session directory that contains an events.jsonl file
    pub fn list_sessions(&self) -> Vec<SessionId> {
        let sessions_dir = self.sessions_dir();

        if !sessions_dir.exists() {
            return Vec::new();
        }

        let mut sessions: Vec<SessionId> = fs::read_dir(sessions_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let session_id_str = entry.file_name().to_string_lossy().to_string();
                SessionId::from_timestamp(session_id_str).ok()
            })
            .filter(|session_id| {
                let events_file = self.events_file(session_id);
                events_file.exists()
            })
            .collect();

        sessions.sort();
        sessions.reverse();
        sessions
    }

    /// Get the most recent session (if any)
    pub fn latest_session(&self) -> Option<SessionId> {
        self.list_sessions().into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::TempDir;

    #[test]
    fn test_view_file_filenames() {
        assert_eq!(ViewFile::Memory.filename(), "MEMORY.md");
        assert_eq!(ViewFile::Plan.filename(), "PLAN.md");
        assert_eq!(ViewFile::Decisions.filename(), "DECISIONS.md");
    }

    #[test]
    fn test_view_file_all() {
        let all = ViewFile::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&ViewFile::Memory));
        assert!(all.contains(&ViewFile::Plan));
        assert!(all.contains(&ViewFile::Decisions));
    }

    #[test]
    fn test_session_id_new() {
        let id = SessionId::new();
        assert!(!id.as_str().is_empty());
        assert!(id.as_str().contains('-'));
    }

    #[test]
    fn test_session_id_default() {
        let id1: SessionId = Default::default();
        let id2 = SessionId::new();
        assert!(!id1.as_str().is_empty());
        assert!(!id2.as_str().is_empty());
        assert!(id1.as_str().contains('-'));
        assert!(id2.as_str().contains('-'));
    }

    #[test]
    fn test_session_id_from_valid_timestamp() {
        let id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        assert_eq!(id.as_str(), "2025-01-11T14-30-45Z");
    }

    #[test]
    fn test_session_id_from_invalid_timestamp_empty() {
        let result = SessionId::from_timestamp("");
        assert!(matches!(result, Err(SessionIdError::Empty)));
    }

    #[test]
    fn test_session_id_from_invalid_characters() {
        let result = SessionId::from_timestamp("invalid@timestamp#");
        assert!(matches!(result, Err(SessionIdError::InvalidFormat)));
    }

    #[test]
    fn test_session_id_try_from_string() {
        let id: Result<SessionId, _> = "2025-01-11T14-30-45Z".try_into();
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), "2025-01-11T14-30-45Z");
    }

    #[test]
    fn test_session_id_try_from_string_invalid() {
        let id: Result<SessionId, _> = "invalid@".try_into();
        assert!(id.is_err());
    }

    #[test]
    fn test_session_id_ord() {
        let id1 = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        let id2 = SessionId::from_timestamp("2025-01-11T15-30-45Z").unwrap();
        assert!(id1 < id2);
    }

    #[test]
    fn test_agent_dir_paths() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());

        assert_eq!(agent.agent_dir(), temp.path().join(".agent"));
        assert_eq!(agent.sessions_dir(), temp.path().join(".agent/sessions"));
        assert_eq!(agent.views_dir(), temp.path().join(".agent/views"));
    }

    #[test]
    fn test_agent_dir_session_paths() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());
        let session_id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();

        let expected_session = temp.path().join(".agent/sessions/2025-01-11T14-30-45Z");
        assert_eq!(agent.session_dir(&session_id), expected_session);

        let expected_events = expected_session.join("events.jsonl");
        assert_eq!(agent.events_file(&session_id), expected_events);

        let expected_patches = expected_session.join("patches");
        assert_eq!(agent.patches_dir(&session_id), expected_patches);
    }

    #[test]
    fn test_agent_dir_patch_file() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());
        let session_id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();

        let patch1 = agent.patch_file(&session_id, "feature");
        assert_eq!(
            patch1,
            temp.path()
                .join(".agent/sessions/2025-01-11T14-30-45Z/patches/feature.patch")
        );

        let patch2 = agent.patch_file(&session_id, "bugfix.patch");
        assert_eq!(
            patch2,
            temp.path()
                .join(".agent/sessions/2025-01-11T14-30-45Z/patches/bugfix.patch")
        );
    }

    #[test]
    fn test_agent_dir_view_files() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());

        assert_eq!(
            agent.view_file(ViewFile::Memory),
            temp.path().join(".agent/views/MEMORY.md")
        );
        assert_eq!(
            agent.view_file(ViewFile::Plan),
            temp.path().join(".agent/views/PLAN.md")
        );
        assert_eq!(
            agent.view_file(ViewFile::Decisions),
            temp.path().join(".agent/views/DECISIONS.md")
        );
    }

    #[test]
    fn test_agent_dir_all_view_files() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());

        let all_views = agent.all_view_files();
        assert_eq!(all_views.len(), 3);
        assert!(all_views.contains(&temp.path().join(".agent/views/MEMORY.md")));
        assert!(all_views.contains(&temp.path().join(".agent/views/PLAN.md")));
        assert!(all_views.contains(&temp.path().join(".agent/views/DECISIONS.md")));
    }

    #[test]
    fn test_patches_glob() {
        let temp = TempDir::new().unwrap();
        let agent = AgentDir::new(temp.path());
        let session_id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();

        let glob = agent.patches_glob(&session_id);
        assert_eq!(
            glob,
            temp.path().join(".agent/sessions/2025-01-11T14-30-45Z/patches/*.patch")
        );
    }

    #[test]
    fn test_session_id_clone_eq_hash() {
        let id1 = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        let id2 = id1.clone();
        let id3 = SessionId::from_timestamp("2025-01-11T15-30-45Z").unwrap();

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);

        let mut set = HashSet::new();
        set.insert(id1);
        assert!(set.contains(&id2));
        assert!(!set.contains(&id3));
    }

    #[test]
    fn test_view_file_display() {
        assert_eq!(ViewFile::Memory.to_string(), "MEMORY.md");
        assert_eq!(ViewFile::Plan.to_string(), "PLAN.md");
        assert_eq!(ViewFile::Decisions.to_string(), "DECISIONS.md");
    }

    #[test]
    fn test_session_id_display() {
        let id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        assert_eq!(id.to_string(), "2025-01-11T14-30-45Z");
    }
}
