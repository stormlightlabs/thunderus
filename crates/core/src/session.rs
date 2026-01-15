use crate::error::{Error, Result, SessionError};
use crate::layout::{AgentDir, SessionId};

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
}
