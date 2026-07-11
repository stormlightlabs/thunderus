//! ACP server session state primitives.
//!
//! The types here are intentionally small and protocol-agnostic: map local
//! `thndrs` session IDs to opaque ACP IDs, normalize/validate `cwd`, and guard
//! against concurrent prompt turns per ACP session.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::cli::WebSearchMode;
use crate::mcp::config::McpConfig;
use crate::session::SessionWriter;

/// Prefix for generated opaque ACP session IDs.
pub const ACP_SESSION_ID_PREFIX: &str = "acp-session";

/// Width of the zero-padded sequence component in generated session IDs.
pub const ACP_SESSION_SEQUENCE_WIDTH: usize = 8;

/// Lightweight metadata placeholder for local session state.
///
/// This tracks local `thndrs` session metadata independently from ACP IDs so
/// handlers can persist or inspect local session details later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalSessionMetadata {
    /// Local append-only session ID used by `thndrs` telemetry and session JSONL.
    pub local_session_id: String,
    /// Current model label selected for this ACP session.
    pub model: Option<String>,
    /// Current web-search mode selected for this ACP session.
    pub websearch: Option<WebSearchMode>,
}

/// Runtime state for one active ACP session.
#[derive(Debug)]
pub struct AcpServerSession {
    /// Opaque ACP session ID.
    pub acp_session_id: String,
    /// Local metadata placeholder.
    pub metadata: LocalSessionMetadata,
    /// Validated and normalized working directory.
    pub cwd: PathBuf,
    /// Whether an ACP turn is currently active.
    pub turn_in_progress: bool,
    /// Optional local session writer persisted to JSONL.
    pub session_writer: Option<SessionWriter>,
    /// MCP servers accepted from ACP `session/new`.
    pub mcp_config: Option<McpConfig>,
}

impl Clone for AcpServerSession {
    /// Clone only serializable session state. A local writer is intentionally
    /// not duplicated because it owns an exclusive append lock.
    fn clone(&self) -> Self {
        Self {
            acp_session_id: self.acp_session_id.clone(),
            metadata: self.metadata.clone(),
            cwd: self.cwd.clone(),
            turn_in_progress: self.turn_in_progress,
            session_writer: None,
            mcp_config: self.mcp_config.clone(),
        }
    }
}

/// Errors for ACP session state operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcpSessionError {
    /// The requested local session already exists.
    DuplicateLocalSession { local_session_id: String },
    /// ACP session lookup by ID failed.
    MissingSession { acp_session_id: String },
    /// Session `cwd` is not valid or could not be normalized.
    InvalidCwd { cwd: String, reason: String },
    /// A turn is already running in this ACP session.
    TurnInProgress { acp_session_id: String },
    /// No turn is active in this ACP session.
    TurnNotActive { acp_session_id: String },
}

impl fmt::Display for AcpSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateLocalSession { local_session_id } => {
                write!(f, "duplicate local session id: {local_session_id}")
            }
            Self::MissingSession { acp_session_id } => {
                write!(f, "missing session: {acp_session_id}")
            }
            Self::InvalidCwd { cwd, reason } => {
                write!(f, "invalid session cwd `{cwd}`: {reason}")
            }
            Self::TurnInProgress { acp_session_id } => {
                write!(f, "turn already active for session `{acp_session_id}`")
            }
            Self::TurnNotActive { acp_session_id } => {
                write!(f, "no active turn for session `{acp_session_id}`")
            }
        }
    }
}

impl std::error::Error for AcpSessionError {}

/// Deterministic ACP session state map used by protocol handlers.
#[derive(Debug, Default)]
pub struct AcpSessionStore {
    sessions: HashMap<String, AcpServerSession>,
    local_to_acp: HashMap<String, String>,
    next_sequence: u64,
}

impl AcpSessionStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a deterministic opaque ACP session id.
    pub fn next_id(&mut self) -> String {
        self.next_sequence += 1;
        generate_session_id(self.next_sequence)
    }

    /// Create a new ACP session and attach local session metadata placeholders.
    ///
    /// Returns `Err` when the local session id already exists.
    pub fn create_session(
        &mut self, local_session_id: impl Into<String>, workspace_root: &Path, requested_cwd: Option<&Path>,
    ) -> Result<String, AcpSessionError> {
        let local_session_id = local_session_id.into();
        if self.local_to_acp.contains_key(&local_session_id) {
            return Err(AcpSessionError::DuplicateLocalSession { local_session_id });
        }

        let cwd = validate_and_normalize_cwd(workspace_root, requested_cwd)?;
        let acp_session_id = self.next_id();
        let session = AcpServerSession {
            acp_session_id: acp_session_id.clone(),
            metadata: LocalSessionMetadata { local_session_id: local_session_id.clone(), model: None, websearch: None },
            cwd,
            turn_in_progress: false,
            session_writer: None,
            mcp_config: None,
        };

        self.sessions.insert(acp_session_id.clone(), session);
        self.local_to_acp.insert(local_session_id, acp_session_id.clone());
        Ok(acp_session_id)
    }

    /// Attach an existing local session to a caller-supplied ACP session id.
    ///
    /// Used by `session/load` and `session/resume`, where the client names a
    /// persisted session returned from `session/list`.
    pub fn attach_existing_session(
        &mut self, acp_session_id: impl Into<String>, local_session_id: impl Into<String>, cwd: &Path,
        session_writer: Option<SessionWriter>,
    ) -> Result<String, AcpSessionError> {
        let acp_session_id = acp_session_id.into();
        let local_session_id = local_session_id.into();
        let cwd = validate_and_normalize_cwd(cwd, None)?;

        if self.sessions.contains_key(&acp_session_id) {
            return Err(AcpSessionError::DuplicateLocalSession { local_session_id });
        }
        if self.local_to_acp.contains_key(&local_session_id) {
            return Err(AcpSessionError::DuplicateLocalSession { local_session_id });
        }

        let session = AcpServerSession {
            acp_session_id: acp_session_id.clone(),
            metadata: LocalSessionMetadata { local_session_id: local_session_id.clone(), model: None, websearch: None },
            cwd,
            turn_in_progress: false,
            session_writer,
            mcp_config: None,
        };

        self.sessions.insert(acp_session_id.clone(), session);
        self.local_to_acp.insert(local_session_id, acp_session_id.clone());
        Ok(acp_session_id)
    }

    /// Return the opaque ACP session id for a local session id.
    pub fn acp_session_id_for_local(&self, local_session_id: &str) -> Option<&str> {
        self.local_to_acp.get(local_session_id).map(String::as_str)
    }

    /// Return the local session id for an ACP session id.
    pub fn local_session_id(&self, acp_session_id: &str) -> Option<&str> {
        self.sessions
            .get(acp_session_id)
            .map(|session| session.metadata.local_session_id.as_str())
    }

    /// Return a snapshot of one ACP session.
    pub fn session(&self, acp_session_id: &str) -> Option<&AcpServerSession> {
        self.sessions.get(acp_session_id)
    }

    /// Return snapshots of all active ACP sessions sorted by ACP id.
    pub fn sessions(&self) -> Vec<&AcpServerSession> {
        let mut sessions = self.sessions.values().collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.acp_session_id.cmp(&right.acp_session_id));
        sessions
    }

    /// Mark a turn as active for this ACP session.
    ///
    /// This is a strict guard: only one active turn is allowed per session.
    pub fn begin_turn(&mut self, acp_session_id: &str) -> Result<(), AcpSessionError> {
        let Some(session) = self.sessions.get_mut(acp_session_id) else {
            return Err(AcpSessionError::MissingSession { acp_session_id: acp_session_id.to_string() });
        };
        if session.turn_in_progress {
            return Err(AcpSessionError::TurnInProgress { acp_session_id: acp_session_id.to_string() });
        }
        session.turn_in_progress = true;
        Ok(())
    }

    /// Mark a turn as complete for this ACP session.
    pub fn end_turn(&mut self, acp_session_id: &str) -> Result<(), AcpSessionError> {
        let Some(session) = self.sessions.get_mut(acp_session_id) else {
            return Err(AcpSessionError::MissingSession { acp_session_id: acp_session_id.to_string() });
        };
        if !session.turn_in_progress {
            return Err(AcpSessionError::TurnNotActive { acp_session_id: acp_session_id.to_string() });
        }
        session.turn_in_progress = false;
        Ok(())
    }

    /// Update local metadata placeholders for a session.
    pub fn update_session_metadata(
        &mut self, acp_session_id: &str, model: Option<String>, websearch: Option<WebSearchMode>,
    ) -> Result<(), AcpSessionError> {
        let Some(session) = self.sessions.get_mut(acp_session_id) else {
            return Err(AcpSessionError::MissingSession { acp_session_id: acp_session_id.to_string() });
        };
        session.metadata.model = model;
        session.metadata.websearch = websearch;
        Ok(())
    }

    /// Attach a local session writer for persistence.
    pub fn attach_session_writer(
        &mut self, acp_session_id: &str, session_writer: SessionWriter,
    ) -> Result<(), AcpSessionError> {
        let Some(session) = self.sessions.get_mut(acp_session_id) else {
            return Err(AcpSessionError::MissingSession { acp_session_id: acp_session_id.to_string() });
        };
        session.session_writer = Some(session_writer);
        Ok(())
    }

    /// Attach ACP-provided MCP server config to a session.
    pub fn attach_mcp_config(&mut self, acp_session_id: &str, mcp_config: McpConfig) -> Result<(), AcpSessionError> {
        let Some(session) = self.sessions.get_mut(acp_session_id) else {
            return Err(AcpSessionError::MissingSession { acp_session_id: acp_session_id.to_string() });
        };
        session.mcp_config = Some(mcp_config);
        Ok(())
    }

    /// Return the active session writer when persistence is enabled.
    pub fn session_writer(&self, acp_session_id: &str) -> Option<&SessionWriter> {
        self.sessions
            .get(acp_session_id)
            .and_then(|session| session.session_writer.as_ref())
    }

    /// Return a mutable session writer when persistence is enabled.
    pub fn session_writer_mut(&mut self, acp_session_id: &str) -> Option<&mut SessionWriter> {
        self.sessions
            .get_mut(acp_session_id)
            .and_then(|session| session.session_writer.as_mut())
    }

    /// Drop a stored session and return its local session id when present.
    pub fn remove_session(&mut self, acp_session_id: &str) -> Option<String> {
        let session = self.sessions.remove(acp_session_id)?;
        self.local_to_acp.retain(|_, mapped_acp| mapped_acp != acp_session_id);
        Some(session.metadata.local_session_id)
    }

    /// Return `true` when the ACP session currently has an active turn.
    pub fn is_turn_active(&self, acp_session_id: &str) -> bool {
        self.sessions
            .get(acp_session_id)
            .is_some_and(|session| session.turn_in_progress)
    }

    /// Force clear all active turns in store state. Intended for tests only.
    pub fn clear_turns_for_test(&mut self) {
        for session in self.sessions.values_mut() {
            session.turn_in_progress = false;
        }
    }
}

/// Generate deterministic opaque ACP session ids.
pub fn generate_session_id(sequence: u64) -> String {
    format!(
        "{ACP_SESSION_ID_PREFIX}-{sequence:0width$}",
        width = ACP_SESSION_SEQUENCE_WIDTH
    )
}

/// Normalize and validate a workspace cwd for a new ACP session.
///
/// Relative paths are resolved against `workspace_root`. Result paths must
/// exist, be directories, and be normalized to an absolute path.
pub fn validate_and_normalize_cwd(
    workspace_root: &Path, requested_cwd: Option<&Path>,
) -> Result<PathBuf, AcpSessionError> {
    let requested = requested_cwd.unwrap_or(workspace_root);
    if requested.as_os_str().is_empty() {
        return canonicalize_cwd(workspace_root);
    }

    let candidate = if requested.is_absolute() { requested.to_path_buf() } else { workspace_root.join(requested) };
    if !candidate.exists() {
        return Err(AcpSessionError::InvalidCwd {
            cwd: candidate.to_string_lossy().to_string(),
            reason: "path does not exist".to_string(),
        });
    }
    if !candidate.is_dir() {
        return Err(AcpSessionError::InvalidCwd {
            cwd: candidate.to_string_lossy().to_string(),
            reason: "path is not a directory".to_string(),
        });
    }
    canonicalize_cwd(&candidate)
}

fn canonicalize_cwd(path: &Path) -> Result<PathBuf, AcpSessionError> {
    let normalized = path.canonicalize().map_err(|error| AcpSessionError::InvalidCwd {
        cwd: path.to_string_lossy().to_string(),
        reason: error.to_string(),
    })?;
    Ok(normalized)
}
