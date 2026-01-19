use crate::config::ApprovalMode;
use crate::error::{Error, Result, SessionError};
use crate::layout::{AgentDir, SessionId};
use crate::teaching::TeachingState;

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Monotonically increasing sequence number for events
pub type Seq = u64;

/// Event types that can be logged in a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    /// User message to the agent
    UserMessage {
        /// Content of the user message
        content: String,
    },
    /// Model response to the user
    ModelMessage {
        /// Content of the model response
        content: String,
        /// Tokens used (optional, not always available)
        tokens_used: Option<TokensUsed>,
    },
    /// Tool call initiated by the model
    ToolCall {
        /// Name of the tool being called
        tool: String,
        /// Arguments passed to the tool
        arguments: serde_json::Value,
    },
    /// Result from a tool execution
    ToolResult {
        /// Name of the tool that was called
        tool: String,
        /// Result of the tool execution
        result: serde_json::Value,
        /// Whether the tool call was successful
        success: bool,
        /// Error message if the tool call failed
        error: Option<String>,
    },
    /// Approval action by the user
    Approval {
        /// Action being approved
        action: String,
        /// Whether the action was approved
        approved: bool,
    },
    /// Patch proposed or applied
    Patch {
        /// Name of the patch
        name: String,
        /// Status of the patch
        status: PatchStatus,
        /// Files affected by the patch
        files: Vec<String>,
        /// Patch content (unified diff format)
        diff: String,
    },
    /// Shell command execution
    ShellCommand {
        /// Command that was executed
        command: String,
        /// Arguments passed to the command
        args: Vec<String>,
        /// Working directory
        working_dir: PathBuf,
        /// Exit code of the command
        exit_code: Option<i32>,
        /// Output file reference (if output was too large to inline)
        output_ref: Option<String>,
    },
    /// Git snapshot (commit state)
    GitSnapshot {
        /// Commit hash
        commit: String,
        /// Branch name
        branch: String,
        /// Number of changed files
        changed_files: usize,
    },
    /// File read operation (for tracking read history and edit validation)
    FileRead {
        /// Absolute path to the file that was read
        file_path: String,
        /// Number of lines read
        line_count: usize,
        /// Offset used for reading (0-indexed)
        offset: usize,
        /// Whether the read was successful
        success: bool,
    },
    /// Approval mode change
    ApprovalModeChange {
        /// Previous approval mode
        from: ApprovalMode,
        /// New approval mode
        to: ApprovalMode,
    },
}

/// Token usage information for model responses
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokensUsed {
    /// Number of input tokens
    pub input: u32,
    /// Number of output tokens
    pub output: u32,
    /// Total tokens (input + output)
    pub total: u32,
}

impl TokensUsed {
    pub fn new(input: u32, output: u32) -> Self {
        Self { input, output, total: input + output }
    }
}

/// Status of a patch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PatchStatus {
    /// Patch is proposed but not yet applied
    Proposed,
    /// Patch has been approved
    Approved,
    /// Patch has been applied
    Applied,
    /// Patch was rejected
    Rejected,
    /// Patch application failed
    Failed,
}

/// Session metadata persisted to metadata.json
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMetadata {
    /// Approval mode for this session
    pub approval_mode: ApprovalMode,
    /// Whether network access is allowed
    pub allow_network: bool,
    /// Session title (optional)
    pub title: Option<String>,
    /// Session tags
    pub tags: Vec<String>,
    /// Teaching state for tracking taught concepts
    #[serde(default)]
    pub teaching_state: TeachingState,
    /// When the session was created
    pub created_at: String,
    /// When the session was last updated
    pub updated_at: String,
}

impl SessionMetadata {
    /// Create new session metadata with default values
    pub fn new(approval_mode: ApprovalMode) -> Self {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self {
            approval_mode,
            allow_network: false,
            title: None,
            tags: Vec::new(),
            teaching_state: TeachingState::default(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Update the approval mode
    pub fn with_approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.approval_mode = mode;
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Update the network permission
    pub fn with_allow_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Set the session title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }

    /// Add a tag to the session
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self.updated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self
    }
}

/// A logged event with its sequence number and timestamp
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoggedEvent {
    /// Monotonic sequence number
    pub seq: Seq,
    /// Session ID this event belongs to
    pub session_id: String,
    /// Timestamp when the event was logged (ISO 8601)
    pub timestamp: String,
    /// The event data
    pub event: Event,
}

impl LoggedEvent {
    fn new(seq: Seq, session_id: &SessionId, event: Event) -> Self {
        let now = chrono::Utc::now();
        let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self { seq, session_id: session_id.as_str().to_string(), timestamp, event }
    }
}

/// Session manages events and their storage in JSONL format
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique identifier for this session
    pub id: SessionId,
    /// Agent directory layout
    agent_dir: AgentDir,
    /// Next sequence number to assign
    next_seq: Seq,
}

impl Session {
    /// Create a new session with a fresh SessionId
    ///
    /// This creates the session directory and initializes the events.jsonl file
    pub fn new(agent_dir: AgentDir) -> Result<Self> {
        Self::with_id(agent_dir, SessionId::new())
    }

    /// Create a session with a specific SessionId
    ///
    /// This creates the session directory and initializes the events.jsonl file
    pub fn with_id(agent_dir: AgentDir, id: SessionId) -> Result<Self> {
        let session_dir = agent_dir.session_dir(&id);

        std::fs::create_dir_all(&session_dir).map_err(|_e| SessionError::AlreadyExists(id.to_string()))?;

        let patches_dir = agent_dir.patches_dir(&id);
        std::fs::create_dir_all(&patches_dir)?;

        let events_file = agent_dir.events_file(&id);
        if !events_file.exists() {
            File::create(&events_file)?;
        }

        let next_seq = Self::load_next_seq(&events_file)?;

        Ok(Self { id, agent_dir, next_seq })
    }

    /// Load an existing session by ID
    ///
    /// Returns error if the session does not exist
    pub fn load(agent_dir: AgentDir, id: SessionId) -> Result<Self> {
        let session_dir = agent_dir.session_dir(&id);

        if !session_dir.exists() {
            return Err(Error::Session(SessionError::NotFound(id.to_string())));
        }

        let events_file = agent_dir.events_file(&id);
        if !events_file.exists() {
            return Err(Error::Session(SessionError::EventsNotFound(id.to_string())));
        }

        let next_seq = Self::load_next_seq(&events_file)?;

        Ok(Self { id, agent_dir, next_seq })
    }

    /// Load the next sequence number from the events file
    fn load_next_seq(events_file: &Path) -> Result<Seq> {
        if !events_file.exists() {
            return Ok(0);
        }

        let file = File::open(events_file)?;
        let reader = BufReader::new(file);

        let mut max_seq: Seq = 0;
        for (line_num, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| SessionError::Corrupted(e.to_string()))?;
            if line.is_empty() {
                continue;
            }

            let logged_event: LoggedEvent = serde_json::from_str(&line)
                .map_err(|e| SessionError::InvalidEvent { line: line_num + 1, reason: e.to_string() })?;

            if logged_event.seq >= max_seq {
                max_seq = logged_event.seq + 1;
            }
        }

        Ok(max_seq)
    }

    /// Get the path to the events file for this session
    pub fn events_file(&self) -> PathBuf {
        self.agent_dir.events_file(&self.id)
    }

    /// Get the path to the patches directory for this session
    pub fn patches_dir(&self) -> PathBuf {
        self.agent_dir.patches_dir(&self.id)
    }

    /// Get the path to the session directory
    pub fn session_dir(&self) -> PathBuf {
        self.agent_dir.session_dir(&self.id)
    }

    /// Append an event to the session log
    ///
    /// The event is assigned a sequence number and written to the JSONL file
    pub fn append_event(&mut self, event: Event) -> Result<Seq> {
        let seq = self.next_seq;
        let logged_event = LoggedEvent::new(seq, &self.id, event);

        let events_file = self.events_file();
        let mut file = OpenOptions::new().create(true).append(true).open(&events_file)?;

        let json_line = serde_json::to_string(&logged_event)
            .map_err(|e| Error::Parse(format!("JSON serialization error: {}", e)))?;
        writeln!(file, "{}", json_line)?;

        self.next_seq += 1;
        Ok(seq)
    }

    /// Append a user message
    pub fn append_user_message(&mut self, content: impl Into<String>) -> Result<Seq> {
        self.append_event(Event::UserMessage { content: content.into() })
    }

    /// Append a model response
    pub fn append_model_message(&mut self, content: impl Into<String>, tokens_used: Option<TokensUsed>) -> Result<Seq> {
        self.append_event(Event::ModelMessage { content: content.into(), tokens_used })
    }

    /// Append a tool call
    pub fn append_tool_call(&mut self, tool: impl Into<String>, arguments: serde_json::Value) -> Result<Seq> {
        self.append_event(Event::ToolCall { tool: tool.into(), arguments })
    }

    /// Append a tool result
    pub fn append_tool_result(
        &mut self, tool: impl Into<String>, result: serde_json::Value, success: bool, error: Option<String>,
    ) -> Result<Seq> {
        self.append_event(Event::ToolResult { tool: tool.into(), result, success, error })
    }

    /// Append an approval action
    pub fn append_approval(&mut self, action: impl Into<String>, approved: bool) -> Result<Seq> {
        self.append_event(Event::Approval { action: action.into(), approved })
    }

    /// Append a patch
    pub fn append_patch(
        &mut self, name: impl Into<String>, status: PatchStatus, files: Vec<String>, diff: impl Into<String>,
    ) -> Result<Seq> {
        self.append_event(Event::Patch { name: name.into(), status, files, diff: diff.into() })
    }

    /// Append a shell command
    pub fn append_shell_command(
        &mut self, command: impl Into<String>, args: Vec<String>, working_dir: PathBuf, exit_code: Option<i32>,
        output_ref: Option<String>,
    ) -> Result<Seq> {
        self.append_event(Event::ShellCommand { command: command.into(), args, working_dir, exit_code, output_ref })
    }

    /// Append a git snapshot
    pub fn append_git_snapshot(
        &mut self, commit: impl Into<String>, branch: impl Into<String>, changed_files: usize,
    ) -> Result<Seq> {
        self.append_event(Event::GitSnapshot { commit: commit.into(), branch: branch.into(), changed_files })
    }

    /// Append a file read operation
    pub fn append_file_read(
        &mut self, file_path: impl Into<String>, line_count: usize, offset: usize, success: bool,
    ) -> Result<Seq> {
        self.append_event(Event::FileRead { file_path: file_path.into(), line_count, offset, success })
    }

    /// Read all events from the session log
    pub fn read_events(&self) -> Result<Vec<LoggedEvent>> {
        let events_file = self.events_file();
        let file = File::open(&events_file)?;
        let reader = BufReader::new(file);

        let mut events = Vec::new();
        for (line_num, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| SessionError::Corrupted(e.to_string()))?;
            if line.is_empty() {
                continue;
            }

            let logged_event: LoggedEvent = serde_json::from_str(&line)
                .map_err(|e| SessionError::InvalidEvent { line: line_num + 1, reason: e.to_string() })?;

            events.push(logged_event);
        }

        Ok(events)
    }

    /// Read events from a specific sequence number onwards
    pub fn read_events_from(&self, from_seq: Seq) -> Result<Vec<LoggedEvent>> {
        let all_events = self.read_events()?;
        Ok(all_events.into_iter().filter(|e| e.seq >= from_seq).collect())
    }

    /// Get the count of events in the session
    pub fn event_count(&self) -> Result<usize> {
        let events_file = self.events_file();
        if !events_file.exists() {
            return Ok(0);
        }

        let file = File::open(&events_file)?;
        let reader = BufReader::new(file);

        let mut count = 0;
        for line in reader.lines() {
            let line = line.map_err(|e| SessionError::Corrupted(e.to_string()))?;
            if !line.is_empty() {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Check if the session exists on disk
    pub fn exists(&self) -> bool {
        self.session_dir().exists() && self.events_file().exists()
    }

    /// Check if a file has been read in this session
    ///
    /// Returns the sequence number of the most recent read event for the file,
    /// or None if the file has not been read
    pub fn was_file_read(&self, file_path: &str) -> Result<Option<Seq>> {
        let events = self.read_events()?;

        let last_read_seq = events
            .iter()
            .rev()
            .filter_map(|e| {
                if let Event::FileRead { file_path: ref fp, success, .. } = e.event {
                    if fp == file_path && success { Some(e.seq) } else { None }
                } else {
                    None
                }
            })
            .next();

        Ok(last_read_seq)
    }

    /// Get all files that have been read in this session
    ///
    /// Returns a vector of (file_path, seq) tuples for all successful reads
    pub fn read_files(&self) -> Result<Vec<(String, Seq)>> {
        let events = self.read_events()?;

        let files: Vec<(String, Seq)> = events
            .iter()
            .filter_map(|e| {
                if let Event::FileRead { file_path: ref fp, success, .. } = e.event {
                    if success { Some((fp.clone(), e.seq)) } else { None }
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    }

    /// Get the path to the metadata file for this session
    pub fn metadata_file(&self) -> PathBuf {
        self.agent_dir.metadata_file(&self.id)
    }

    /// Load session metadata from disk
    ///
    /// Returns default metadata if the file doesn't exist
    pub fn load_metadata(&self) -> Result<SessionMetadata> {
        let metadata_file = self.metadata_file();

        if !metadata_file.exists() {
            return Ok(SessionMetadata::new(ApprovalMode::Auto));
        }

        let file = File::open(&metadata_file)?;
        let metadata: SessionMetadata = serde_json::from_reader(file)
            .map_err(|e| Error::Parse(format!("Failed to parse session metadata: {}", e)))?;

        Ok(metadata)
    }

    /// Save session metadata to disk
    pub fn save_metadata(&self, metadata: &SessionMetadata) -> Result<()> {
        let metadata_file = self.metadata_file();

        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| Error::Parse(format!("Failed to serialize session metadata: {}", e)))?;

        std::fs::write(&metadata_file, json)
            .map_err(|e| Error::Other(format!("Failed to write session metadata: {}", e)))?;

        Ok(())
    }

    /// Append an approval mode change event
    pub fn append_approval_mode_change(&mut self, from: ApprovalMode, to: ApprovalMode) -> Result<Seq> {
        self.append_event(Event::ApprovalModeChange { from, to })
    }

    /// Get the current approval mode from metadata
    ///
    /// This loads the metadata from disk and returns the approval mode
    pub fn get_approval_mode(&self) -> Result<ApprovalMode> {
        Ok(self.load_metadata()?.approval_mode)
    }

    /// Set the approval mode and persist it to metadata
    ///
    /// This updates the metadata file and logs a mode change event
    pub fn set_approval_mode(&mut self, new_mode: ApprovalMode) -> Result<()> {
        let current_mode = self.get_approval_mode()?;

        if current_mode != new_mode {
            self.append_approval_mode_change(current_mode, new_mode)?;

            let mut metadata = self.load_metadata()?;
            metadata = metadata.with_approval_mode(new_mode);
            self.save_metadata(&metadata)?;
        }

        Ok(())
    }

    /// Get the network permission from metadata
    pub fn get_allow_network(&self) -> Result<bool> {
        Ok(self.load_metadata()?.allow_network)
    }

    /// Set the network permission and persist it to metadata
    pub fn set_allow_network(&mut self, allow: bool) -> Result<()> {
        let mut metadata = self.load_metadata()?;
        metadata = metadata.with_allow_network(allow);
        self.save_metadata(&metadata)?;
        Ok(())
    }

    /// Get the teaching state from metadata
    pub fn get_teaching_state(&self) -> Result<TeachingState> {
        Ok(self.load_metadata()?.teaching_state)
    }

    /// Get a hint for a concept if it hasn't been taught yet
    ///
    /// Returns Some(hint) if this is the first time the concept is encountered,
    /// or None if it has already been taught. Persists the updated teaching state.
    pub fn get_hint_for_concept(&mut self, concept: &str) -> Result<Option<String>> {
        let mut metadata = self.load_metadata()?;
        let hint = metadata.teaching_state.get_hint(concept);
        if hint.is_some() {
            self.save_metadata(&metadata)?;
        }
        Ok(hint)
    }

    /// Mark a concept as taught without displaying a hint
    pub fn mark_concept_taught(&mut self, concept: &str) -> Result<()> {
        let mut metadata = self.load_metadata()?;
        metadata.teaching_state.mark_taught(concept);
        self.save_metadata(&metadata)?;
        Ok(())
    }

    /// Check if a concept has been taught
    pub fn is_concept_taught(&self, concept: &str) -> Result<bool> {
        Ok(self.load_metadata()?.teaching_state.has_taught(concept))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_session() -> (TempDir, Session) {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session = Session::new(agent_dir).unwrap();
        (temp, session)
    }

    fn create_test_session_with_id(id_str: &str) -> (TempDir, Session) {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let id = SessionId::from_timestamp(id_str).unwrap();
        let session = Session::with_id(agent_dir, id).unwrap();
        (temp, session)
    }

    #[test]
    fn test_session_new() {
        let (temp, session) = create_test_session();

        assert!(!session.id.as_str().is_empty());
        assert!(session.session_dir().exists());
        assert!(session.patches_dir().exists());
        assert!(session.events_file().exists());

        assert_eq!(session.next_seq, 0);
        assert_eq!(session.event_count().unwrap(), 0);
        drop(temp);
    }

    #[test]
    fn test_session_with_id() {
        let (temp, session) = create_test_session_with_id("2025-01-11T14-30-45Z");

        assert_eq!(session.id.as_str(), "2025-01-11T14-30-45Z");
        assert!(session.session_dir().exists());
        assert!(session.events_file().exists());
        drop(temp);
    }

    #[test]
    fn test_session_load_existing() {
        let (temp, session) = create_test_session();

        let agent_dir = AgentDir::new(temp.path());
        let loaded_session = Session::load(agent_dir, session.id.clone()).unwrap();

        assert_eq!(loaded_session.id, session.id);
        assert_eq!(loaded_session.next_seq, session.next_seq);
    }

    #[test]
    fn test_session_load_nonexistent() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();

        let result = Session::load(agent_dir, id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Session(SessionError::NotFound(_))));
    }

    #[test]
    fn test_append_user_message() {
        let (temp, mut session) = create_test_session();

        let seq = session.append_user_message("Hello, world!").unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[0].session_id, session.id.as_str());
        assert!(matches!(events[0].event, Event::UserMessage { .. }));

        if let Event::UserMessage { content } = &events[0].event {
            assert_eq!(content, "Hello, world!");
        } else {
            panic!("Expected UserMessage event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_model_message() {
        let (temp, mut session) = create_test_session();

        let tokens = TokensUsed::new(10, 20);
        let seq = session
            .append_model_message("Response content", Some(tokens.clone()))
            .unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ModelMessage { content, tokens_used } = &events[0].event {
            assert_eq!(content, "Response content");
            assert_eq!(tokens_used, &Some(tokens));
        } else {
            panic!("Expected ModelMessage event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_tool_call() {
        let (temp, mut session) = create_test_session();

        let args = serde_json::json!({ "path": "/tmp/test" });
        let seq = session.append_tool_call("fs.read", args).unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ToolCall { tool, arguments } = &events[0].event {
            assert_eq!(tool, "fs.read");
            assert_eq!(arguments, &serde_json::json!({ "path": "/tmp/test" }));
        } else {
            panic!("Expected ToolCall event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_tool_result() {
        let (_temp, mut session) = create_test_session();

        let result = serde_json::json!({ "output": "test output" });
        let seq = session.append_tool_result("fs.read", result, true, None).unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ToolResult { tool, result: res, success, error } = &events[0].event {
            assert_eq!(tool, "fs.read");
            assert_eq!(res, &serde_json::json!({ "output": "test output" }));
            assert!(*success);
            assert!(error.is_none());
        } else {
            panic!("Expected ToolResult event");
        }
    }

    #[test]
    fn test_append_approval() {
        let (temp, mut session) = create_test_session();

        let seq = session.append_approval("patch.feature", true).unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::Approval { action, approved } = &events[0].event {
            assert_eq!(action, "patch.feature");
            assert!(*approved);
        } else {
            panic!("Expected Approval event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_patch() {
        let (temp, mut session) = create_test_session();

        let diff = "@@ -1,1 +1,1 @@\n-old\n+new";
        let seq = session
            .append_patch(
                "feature-patch",
                PatchStatus::Proposed,
                vec!["src/main.rs".to_string()],
                diff,
            )
            .unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::Patch { name, status, files, diff: d } = &events[0].event {
            assert_eq!(name, "feature-patch");
            assert_eq!(status, &PatchStatus::Proposed);
            assert_eq!(files, &vec!["src/main.rs".to_string()]);
            assert_eq!(d, diff);
        } else {
            panic!("Expected Patch event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_shell_command() {
        let (temp, mut session) = create_test_session();

        let seq = session
            .append_shell_command(
                "cargo",
                vec!["test".to_string()],
                PathBuf::from("/workspace"),
                Some(0),
                None,
            )
            .unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ShellCommand { command, args, working_dir, exit_code, output_ref } = &events[0].event {
            assert_eq!(command, "cargo");
            assert_eq!(args, &vec!["test".to_string()]);
            assert_eq!(working_dir, &PathBuf::from("/workspace"));
            assert_eq!(exit_code, &Some(0));
            assert!(output_ref.is_none());
        } else {
            panic!("Expected ShellCommand event");
        }
        drop(temp);
    }

    #[test]
    fn test_append_git_snapshot() {
        let (temp, mut session) = create_test_session();

        let seq = session.append_git_snapshot("abc123def456", "main", 5).unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::GitSnapshot { commit, branch, changed_files } = &events[0].event {
            assert_eq!(commit, "abc123def456");
            assert_eq!(branch, "main");
            assert_eq!(*changed_files, 5);
        } else {
            panic!("Expected GitSnapshot event");
        }
        drop(temp);
    }

    #[test]
    fn test_monotonic_sequence() {
        let (temp, mut session) = create_test_session();

        let seq1 = session.append_user_message("First").unwrap();
        let seq2 = session.append_user_message("Second").unwrap();
        let seq3 = session.append_user_message("Third").unwrap();

        assert_eq!(seq1, 0);
        assert_eq!(seq2, 1);
        assert_eq!(seq3, 2);

        assert_eq!(session.next_seq, 3);
        assert_eq!(session.event_count().unwrap(), 3);
        drop(temp);
    }

    #[test]
    fn test_load_preserves_sequence() {
        let (temp, mut session) = create_test_session();

        session.append_user_message("First").unwrap();
        session.append_user_message("Second").unwrap();

        let agent_dir = AgentDir::new(temp.path());
        let mut loaded_session = Session::load(agent_dir, session.id.clone()).unwrap();

        assert_eq!(loaded_session.next_seq, 2);

        let seq = loaded_session.append_user_message("Third").unwrap();
        assert_eq!(seq, 2);

        let events = loaded_session.read_events().unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_read_events_from() {
        let (temp, mut session) = create_test_session();

        for i in 0..10 {
            session.append_user_message(format!("Message {}", i)).unwrap();
        }

        let events_from_5 = session.read_events_from(5).unwrap();
        assert_eq!(events_from_5.len(), 5);
        assert_eq!(events_from_5[0].seq, 5);
        assert_eq!(events_from_5[4].seq, 9);
        drop(temp);
    }

    #[test]
    fn test_event_json_serialization() {
        let event = Event::UserMessage { content: "test".to_string() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("user-message"));
        assert!(json.contains("test"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_logged_event_json_serialization() {
        let id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        let event = Event::UserMessage { content: "test".to_string() };
        let logged = LoggedEvent::new(0, &id, event);

        let json = serde_json::to_string(&logged).unwrap();
        let deserialized: LoggedEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(logged.seq, deserialized.seq);
        assert_eq!(logged.session_id, deserialized.session_id);
        assert_eq!(logged.event, deserialized.event);
    }

    #[test]
    fn test_tokens_used() {
        let tokens = TokensUsed::new(10, 20);
        assert_eq!(tokens.input, 10);
        assert_eq!(tokens.output, 20);
        assert_eq!(tokens.total, 30);
    }

    #[test]
    fn test_patch_status_serialization() {
        let statuses = [
            PatchStatus::Proposed,
            PatchStatus::Approved,
            PatchStatus::Applied,
            PatchStatus::Rejected,
            PatchStatus::Failed,
        ];

        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: PatchStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, &deserialized);
        }
    }

    #[test]
    fn test_jsonl_format() {
        let (temp, mut session) = create_test_session();

        session.append_user_message("First").unwrap();
        session
            .append_tool_call("test", serde_json::json!({"arg": "value"}))
            .unwrap();

        let events_file_content = std::fs::read_to_string(session.events_file()).unwrap();
        let lines: Vec<&str> = events_file_content.lines().collect();
        assert_eq!(lines.len(), 2);

        let event1: LoggedEvent = serde_json::from_str(lines[0]).unwrap();
        let event2: LoggedEvent = serde_json::from_str(lines[1]).unwrap();

        assert_eq!(event1.seq, 0);
        assert_eq!(event2.seq, 1);
        drop(temp);
    }

    #[test]
    fn test_session_exists() {
        let (temp, session) = create_test_session();
        assert!(session.exists());
        drop(temp);

        let temp2 = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp2.path());
        let id = SessionId::new();
        let non_existent = Session { id, agent_dir, next_seq: 0 };
        assert!(!non_existent.exists());
        drop(temp2);
    }

    #[test]
    fn test_empty_session() {
        let (temp, session) = create_test_session();
        let events = session.read_events().unwrap();
        assert!(events.is_empty());
        assert_eq!(session.event_count().unwrap(), 0);
        drop(temp);
    }

    #[test]
    fn test_mixed_events() {
        let (temp, mut session) = create_test_session();

        session.append_user_message("Hello").unwrap();
        session.append_model_message("Hi there", None).unwrap();
        session.append_tool_call("test", serde_json::json!({})).unwrap();
        session
            .append_tool_result("test", serde_json::json!({}), true, None)
            .unwrap();
        session.append_approval("action", true).unwrap();
        session.append_git_snapshot("abc", "main", 1).unwrap();

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 6);

        assert!(matches!(events[0].event, Event::UserMessage { .. }));
        assert!(matches!(events[1].event, Event::ModelMessage { .. }));
        assert!(matches!(events[2].event, Event::ToolCall { .. }));
        assert!(matches!(events[3].event, Event::ToolResult { .. }));
        assert!(matches!(events[4].event, Event::Approval { .. }));
        assert!(matches!(events[5].event, Event::GitSnapshot { .. }));
        drop(temp);
    }

    #[test]
    fn test_session_paths() {
        let (temp, session) = create_test_session();

        let session_dir = session.session_dir();
        let events_file = session.events_file();
        let patches_dir = session.patches_dir();

        assert!(session_dir.is_absolute());
        assert!(events_file.is_absolute());
        assert!(patches_dir.is_absolute());

        assert!(events_file.starts_with(&session_dir));
        assert!(patches_dir.starts_with(&session_dir));
        drop(temp);
    }

    #[test]
    fn test_invalid_event_in_jsonl() {
        let (temp, session) = create_test_session();

        let events_file = session.events_file();

        let mut file = OpenOptions::new().append(true).open(&events_file).unwrap();
        writeln!(file, "invalid json").unwrap();

        let result = session.read_events();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::Session(SessionError::InvalidEvent { .. })
        ));
        drop(temp);
    }

    #[test]
    fn test_empty_lines_in_jsonl() {
        let (temp, mut session) = create_test_session();

        session.append_user_message("First").unwrap();

        let events_file = session.events_file();

        let mut file = OpenOptions::new().append(true).open(&events_file).unwrap();
        writeln!(file).unwrap();
        writeln!(file).unwrap();

        session.append_user_message("Second").unwrap();

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        drop(temp);
    }

    #[test]
    fn test_timestamp_format() {
        let (_temp, mut session) = create_test_session();

        session.append_user_message("Test").unwrap();

        let events = session.read_events().unwrap();
        assert!(!events[0].timestamp.is_empty());

        let parsed_time: chrono::DateTime<chrono::Utc> = chrono::DateTime::parse_from_rfc3339(&events[0].timestamp)
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(parsed_time.timestamp() > 0);
    }

    #[test]
    fn test_error_on_invalid_session_id() {
        use crate::layout::SessionIdError;

        let result = SessionId::from_timestamp("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionIdError::Empty));

        let result = SessionId::from_timestamp("invalid@timestamp");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SessionIdError::InvalidFormat));
    }

    #[test]
    fn test_tool_result_with_error() {
        let (_temp, mut session) = create_test_session();

        let result = serde_json::json!({});
        let seq = session
            .append_tool_result("tool.name", result, false, Some("Error message".to_string()))
            .unwrap();

        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        if let Event::ToolResult { success, error, .. } = &events[0].event {
            assert!(!success);
            assert_eq!(error, &Some("Error message".to_string()));
        } else {
            panic!("Expected ToolResult event");
        }
    }

    #[test]
    fn test_all_patch_statuses() {
        let statuses = vec![
            PatchStatus::Proposed,
            PatchStatus::Approved,
            PatchStatus::Applied,
            PatchStatus::Rejected,
            PatchStatus::Failed,
        ];

        for status in statuses {
            let (temp, mut session) = create_test_session();
            let expected_status = status.clone();
            session.append_patch("test", status, vec![], "").unwrap();

            let events = session.read_events().unwrap();
            if let Event::Patch { status: s, .. } = &events[0].event {
                assert_eq!(s, &expected_status);
            } else {
                panic!("Expected Patch event");
            }
            drop(temp);
        }
    }

    #[test]
    fn test_concurrent_session_with_same_id() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let id = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();

        let session1 = Session::with_id(agent_dir.clone(), id.clone()).unwrap();

        let result = Session::with_id(agent_dir.clone(), id.clone());
        assert!(result.is_ok());

        let session2 = Session::load(agent_dir, id).unwrap();
        assert_eq!(session1.id, session2.id);
        drop(temp);
    }

    #[test]
    fn test_load_next_seq_from_existing_file() {
        let (temp, mut session) = create_test_session();

        for i in 0..5 {
            session.append_user_message(format!("Message {}", i)).unwrap();
        }

        let agent_dir = AgentDir::new(temp.path());
        let loaded = Session::load(agent_dir, session.id.clone()).unwrap();

        assert_eq!(loaded.next_seq, 5);
        drop(temp);
    }

    #[test]
    fn test_session_id_ord() {
        let id1 = SessionId::from_timestamp("2025-01-11T14-30-45Z").unwrap();
        let id2 = SessionId::from_timestamp("2025-01-11T15-30-45Z").unwrap();
        assert!(id1 < id2);
    }

    #[test]
    fn test_append_file_read() {
        let (temp, mut session) = create_test_session();

        let seq = session.append_file_read("/path/to/file.txt", 100, 0, true).unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::FileRead { file_path, line_count, offset, success } = &events[0].event {
            assert_eq!(file_path, "/path/to/file.txt");
            assert_eq!(*line_count, 100);
            assert_eq!(*offset, 0);
            assert!(*success);
        } else {
            panic!("Expected FileRead event");
        }
        drop(temp);
    }

    #[test]
    fn test_was_file_read() {
        let (temp, mut session) = create_test_session();

        assert!(session.was_file_read("/path/to/file.txt").unwrap().is_none());

        session.append_file_read("/path/to/file.txt", 100, 0, true).unwrap();
        assert_eq!(session.was_file_read("/path/to/file.txt").unwrap(), Some(0));

        session.append_user_message("some message").unwrap();
        session.append_file_read("/path/to/file.txt", 200, 0, true).unwrap();
        assert_eq!(session.was_file_read("/path/to/file.txt").unwrap(), Some(2));

        session.append_file_read("/another/file.txt", 50, 0, false).unwrap();
        assert!(session.was_file_read("/another/file.txt").unwrap().is_none());

        assert!(session.was_file_read("/different/file.rs").unwrap().is_none());
        drop(temp);
    }

    #[test]
    fn test_read_files() {
        let (temp, mut session) = create_test_session();

        assert!(session.read_files().unwrap().is_empty());

        session.append_file_read("/path/to/file1.txt", 100, 0, true).unwrap();
        session.append_file_read("/path/to/file2.rs", 200, 10, true).unwrap();
        session.append_file_read("/path/to/file3.md", 50, 0, true).unwrap();

        let files = session.read_files().unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], ("/path/to/file1.txt".to_string(), 0));
        assert_eq!(files[1], ("/path/to/file2.rs".to_string(), 1));
        assert_eq!(files[2], ("/path/to/file3.md".to_string(), 2));

        session.append_file_read("/path/to/file4.txt", 100, 0, false).unwrap();

        let files = session.read_files().unwrap();
        assert_eq!(files.len(), 3);
        drop(temp);
    }

    #[test]
    fn test_file_read_event_serialization() {
        let event =
            Event::FileRead { file_path: "/test/path.txt".to_string(), line_count: 42, offset: 10, success: true };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("file-read"));
        assert!(json.contains("/test/path.txt"));
        assert!(json.contains("42"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_session_metadata_new() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto);
        assert_eq!(metadata.approval_mode, ApprovalMode::Auto);
        assert!(!metadata.allow_network);
        assert!(metadata.title.is_none());
        assert!(metadata.tags.is_empty());
        assert!(!metadata.created_at.is_empty());
        assert!(!metadata.updated_at.is_empty());
    }

    #[test]
    fn test_session_metadata_with_approval_mode() {
        let metadata = SessionMetadata::new(ApprovalMode::ReadOnly);
        let updated = metadata.with_approval_mode(ApprovalMode::FullAccess);
        assert_eq!(updated.approval_mode, ApprovalMode::FullAccess);
        assert!(!updated.updated_at.is_empty());
    }

    #[test]
    fn test_session_metadata_with_allow_network() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto);
        let updated = metadata.with_allow_network(true);
        assert!(updated.allow_network);
    }

    #[test]
    fn test_session_metadata_serialization() {
        let metadata = SessionMetadata::new(ApprovalMode::Auto).with_title("Test Session");
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("\"auto\""));
        assert!(json.contains("Test Session"));

        let deserialized: SessionMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.approval_mode, ApprovalMode::Auto);
        assert_eq!(deserialized.title, Some("Test Session".to_string()));
    }

    #[test]
    fn test_approval_mode_change_event() {
        let (temp, mut session) = create_test_session();

        let seq = session
            .append_approval_mode_change(ApprovalMode::Auto, ApprovalMode::ReadOnly)
            .unwrap();
        assert_eq!(seq, 0);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ApprovalModeChange { from, to } = &events[0].event {
            assert_eq!(from, &ApprovalMode::Auto);
            assert_eq!(to, &ApprovalMode::ReadOnly);
        } else {
            panic!("Expected ApprovalModeChange event");
        }
        drop(temp);
    }

    #[test]
    fn test_get_approval_mode_default() {
        let (temp, session) = create_test_session();
        let mode = session.get_approval_mode().unwrap();
        assert_eq!(mode, ApprovalMode::Auto);
        drop(temp);
    }

    #[test]
    fn test_set_approval_mode() {
        let (temp, mut session) = create_test_session();

        session.set_approval_mode(ApprovalMode::ReadOnly).unwrap();

        let mode = session.get_approval_mode().unwrap();
        assert_eq!(mode, ApprovalMode::ReadOnly);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let Event::ApprovalModeChange { from, to } = &events[0].event {
            assert_eq!(from, &ApprovalMode::Auto);
            assert_eq!(to, &ApprovalMode::ReadOnly);
        } else {
            panic!("Expected ApprovalModeChange event");
        }
        drop(temp);
    }

    #[test]
    fn test_set_same_approval_mode_no_event() {
        let (temp, mut session) = create_test_session();
        session.set_approval_mode(ApprovalMode::Auto).unwrap();

        let events = session.read_events().unwrap();
        assert!(events.is_empty());
        drop(temp);
    }

    #[test]
    fn test_get_allow_network_default() {
        let (temp, session) = create_test_session();

        let allow = session.get_allow_network().unwrap();
        assert!(!allow);
        drop(temp);
    }

    #[test]
    fn test_set_allow_network() {
        let (temp, mut session) = create_test_session();

        session.set_allow_network(true).unwrap();

        let allow = session.get_allow_network().unwrap();
        assert!(allow);
        drop(temp);
    }

    #[test]
    fn test_metadata_persistence() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());

        let mut session = Session::new(agent_dir.clone()).unwrap();
        session.set_approval_mode(ApprovalMode::FullAccess).unwrap();
        session.set_allow_network(true).unwrap();

        let loaded_session = Session::load(agent_dir, session.id.clone()).unwrap();
        assert_eq!(loaded_session.get_approval_mode().unwrap(), ApprovalMode::FullAccess);
        assert!(loaded_session.get_allow_network().unwrap());
        drop(temp);
    }

    #[test]
    fn test_metadata_file_path() {
        let (temp, session) = create_test_session();
        let metadata_path = session.metadata_file();

        assert!(metadata_path.ends_with("metadata.json"));
        drop(temp);
    }

    #[test]
    fn test_get_teaching_state_default() {
        let (temp, session) = create_test_session();
        let teaching_state = session.get_teaching_state().unwrap();
        assert!(!teaching_state.has_taught("any_concept"));
        drop(temp);
    }

    #[test]
    fn test_get_hint_for_concept_first_time() {
        let (temp, mut session) = create_test_session();
        let hint = session.get_hint_for_concept("sed_risky_explained").unwrap();
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("sed -i"));
        drop(temp);
    }

    #[test]
    fn test_get_hint_for_concept_second_time() {
        let (temp, mut session) = create_test_session();
        let hint1 = session.get_hint_for_concept("network_command_explained").unwrap();
        assert!(hint1.is_some());

        let hint2 = session.get_hint_for_concept("network_command_explained").unwrap();
        assert!(hint2.is_none());
        drop(temp);
    }

    #[test]
    fn test_mark_concept_taught() {
        let (temp, mut session) = create_test_session();
        session.mark_concept_taught("custom_concept").unwrap();
        assert!(session.is_concept_taught("custom_concept").unwrap());
        drop(temp);
    }

    #[test]
    fn test_is_concept_taught_not_taught() {
        let (temp, session) = create_test_session();
        assert!(!session.is_concept_taught("unknown_concept").unwrap());
        drop(temp);
    }

    #[test]
    fn test_teaching_state_persistence() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());

        let mut session = Session::new(agent_dir.clone()).unwrap();
        session.mark_concept_taught("test_concept").unwrap();

        let loaded_session = Session::load(agent_dir, session.id.clone()).unwrap();
        assert!(loaded_session.is_concept_taught("test_concept").unwrap());
        drop(temp);
    }

    #[test]
    fn test_multiple_concepts_tracked() {
        let (temp, mut session) = create_test_session();
        session.mark_concept_taught("concept1").unwrap();
        session.mark_concept_taught("concept2").unwrap();
        session.mark_concept_taught("concept3").unwrap();

        assert!(session.is_concept_taught("concept1").unwrap());
        assert!(session.is_concept_taught("concept2").unwrap());
        assert!(session.is_concept_taught("concept3").unwrap());
        assert!(!session.is_concept_taught("concept4").unwrap());
        drop(temp);
    }
}
