use crate::session::Seq;
use serde::{Deserialize, Serialize};

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
        working_dir: std::path::PathBuf,
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
        from: crate::config::ApprovalMode,
        /// New approval mode
        to: crate::config::ApprovalMode,
    },
    /// User manually edited a materialized markdown view
    ViewEdit {
        /// Which view was edited (MEMORY.md, PLAN.md, DECISIONS.md)
        view: String,
        /// Type of change (manual, merge, conflict_resolved)
        change_type: String,
        /// New content after edit
        content: String,
        /// Event sequence numbers this edit references
        seq_refs: Vec<Seq>,
    },
    /// Loaded external context file (CLAUDE.md, AGENTS.md, etc.)
    ContextLoad {
        /// Source filename (e.g., "CLAUDE.md")
        source: String,
        /// Absolute path to the loaded file
        path: String,
        /// Hash of the content for deduplication
        content_hash: String,
    },
    /// User-defined checkpoint in a plan
    Checkpoint {
        /// Label for the checkpoint
        label: String,
        /// Description of what was accomplished
        description: String,
        /// Optional snapshot ID reference
        snapshot_id: Option<String>,
    },
    /// Plan item update (add, complete, remove)
    PlanUpdate {
        /// Action performed (add, complete, remove, update)
        action: String,
        /// The plan item text
        item: String,
        /// Optional reason for the update
        reason: Option<String>,
    },
    /// Memory document created or updated
    MemoryUpdate {
        /// Kind of memory (core, semantic, procedural, episodic)
        kind: String,
        /// Path to the memory file
        path: String,
        /// Operation performed (create, update, delete)
        operation: String,
        /// Hash of the new content
        content_hash: String,
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
    pub(super) fn new(seq: Seq, session_id: &crate::layout::SessionId, event: Event) -> Self {
        let now = chrono::Utc::now();
        let timestamp = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self { seq, session_id: session_id.as_str().to_string(), timestamp, event }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::layout::SessionId;
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
    fn test_view_edit_event_serialization() {
        let event = Event::ViewEdit {
            view: "MEMORY.md".to_string(),
            change_type: "merge".to_string(),
            content: "# Memory\n\nContent".to_string(),
            seq_refs: vec![10, 20, 30],
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("view-edit"));
        assert!(json.contains("MEMORY.md"));
        assert!(json.contains("merge"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_context_load_event_serialization() {
        let event = Event::ContextLoad {
            source: "AGENTS.md".to_string(),
            path: "/repo/AGENTS.md".to_string(),
            content_hash: "xyz789".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("context-load"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_checkpoint_event_serialization() {
        let event = Event::Checkpoint {
            label: "Test checkpoint".to_string(),
            description: "Test description".to_string(),
            snapshot_id: Some("test_snap".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("checkpoint"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_plan_update_event_serialization() {
        let event = Event::PlanUpdate {
            action: "remove".to_string(),
            item: "Deprecated task".to_string(),
            reason: Some("No longer needed".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("plan-update"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_memory_update_event_serialization() {
        let event = Event::MemoryUpdate {
            kind: "core".to_string(),
            path: "/repo/memory/core/CORE.md".to_string(),
            operation: "update".to_string(),
            content_hash: "new_hash".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("memory-update"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }
}
