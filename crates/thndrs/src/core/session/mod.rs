//! Append-only JSONL session persistence.
//!
//! Records session metadata, transcript events, tool audits, and context
//! control actions for audit and resume without storing full raw provider
//! payloads.
//!
//! Each [`SessionRecord`] is one append-only JSONL line tagged with
//! `schema_version`, a monotonic `seq`, `time`, and `type`.

mod capture;
mod collection;
mod context_changes;
mod context_export;
mod contracts;
mod export;
mod inventory;
mod lifecycle;
mod retention;
mod storage;
mod telemetry;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::acp::permissions::PendingPermission;
use crate::app::{Entry, ToolStatus, TranscriptBlocks};
use crate::artifacts::{self, ArtifactMetadata};
use crate::context::ContextSource;
use crate::prompt::{EnvironmentMetadata, HistoryReuse, PromptBundle};
use crate::skills::{SkillActivation, SkillReferenceMeta};
use crate::tools::{WriteOp, shell};
use crate::{datetime, internals, tools};

use thndrs_agent::ProviderRequestAccounting;
use thndrs_agent::context::{ContextItem, ContextLedger, RangeSummary};

pub use capture::{
    CAPTURE_RETENTION_DAYS, CONTEXT_CAPTURE_POLICY_VERSION, CapturedRequestContent, ContextCaptureMode,
    ContextCapturePolicy, MAX_CAPTURED_REQUEST_BYTES,
};
pub use collection::{
    CollectionReport, collect_if_due, collect_now, reclaimable_bytes, reclaimable_bytes_from_inventory,
};
pub use context_changes::{ContextChangeError, ContextHistory};
pub use context_export::{
    CONTEXT_EXPORT_MAX_BYTES, CONTEXT_EXPORT_MAX_RECORDS, CONTEXT_EXPORT_POLICY_VERSION, CONTEXT_EXPORT_SCHEMA_VERSION,
    ContextSnapshotDiff, PersistedContextExport,
};
pub use contracts::{
    AcpPermissionOptionRecord, AcpSessionMetadata, ContextDiagnosticMeta, ContextItemMeta, ContextLedgerMeta,
    ContextLifecycleAudit, ContextSnapshot, ContextSnapshotState, ContextSourceMeta, McpToolSessionMeta,
    SessionConfigFile, SessionConfigMeta,
};
pub use export::{SessionExport, export_session};
pub use inventory::{
    ArtifactInventoryEntry, SessionInventory, SessionInventoryDiagnostic, SessionInventoryEntry, SessionLineageState,
    SessionStorageState, SessionStorageTotals,
};
pub use lifecycle::{
    DeleteArtifactPreview, DeleteSessionOptions, PermanentDeleteOptions, SessionDeletePreview, SessionLifecycle,
    SessionLifecycleAction, SessionLifecycleError, SessionLifecycleReport, SessionOwnedState,
};
pub use retention::apply_prune_cancellable_with_progress;
pub use retention::{
    PruneCandidate, PruneFailure, PruneOverrides, PruneReason, PruneReport, SessionRetentionPolicy, apply_prune,
    apply_prune_cancellable, select_prune_candidates,
};
pub use telemetry::{CONTEXT_TELEMETRY_CARDINALITY_LIMIT, emit_context_telemetry, export_context_telemetry};

/// Current JSONL schema version.
pub const SCHEMA_VERSION: u32 = 1;

fn default_queue_action() -> String {
    String::from("add")
}

/// Maximum amount of redacted log text returned by a reader.
pub const MAX_LOG_OUTPUT_BYTES: usize = 32 * 1024;

/// Maximum length of a user-assigned session name.
pub const MAX_SESSION_NAME_CHARS: usize = 80;

/// One ancestor boundary in a forked session's root-to-parent lineage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionLineageEntry {
    pub session_id: String,
    pub turn_id: String,
}

/// A single line in a session JSONL file.
///
/// Every record carries `schema_version`, `seq` (monotonic within the session),
/// `time` (ISO 8601 UTC), and a `type` tag. Records are never rewritten;
/// appends are the only mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionRecord {
    /// First line of a session: identity and environment.
    #[serde(rename = "session_meta")]
    SessionMeta {
        schema_version: u32,
        seq: u64,
        time: String,
        session_id: String,
        cwd: String,
        title: String,
        provider: String,
        model: String,
        websearch: String,
        app_version: String,
        /// Effective config metadata: loaded config files, key origins, and
        /// diagnostics. `None` when config metadata was not captured.
        #[serde(skip_serializing_if = "Option::is_none")]
        config: Option<SessionConfigMeta>,
    },
    /// Provenance for a session created from a settled turn boundary.
    #[serde(rename = "session_fork")]
    SessionFork {
        schema_version: u32,
        seq: u64,
        time: String,
        parent_session_id: String,
        parent_turn_id: String,
        lineage: Vec<SessionLineageEntry>,
    },
    /// Content capture rules selected at session creation.
    #[serde(rename = "context_capture_policy")]
    ContextCapturePolicy {
        schema_version: u32,
        seq: u64,
        time: String,
        policy: ContextCapturePolicy,
    },
    /// Sanitized provider-neutral request content captured under the session policy.
    #[serde(rename = "request_content_captured")]
    RequestContentCaptured {
        schema_version: u32,
        seq: u64,
        time: String,
        capture: CapturedRequestContent,
    },
    /// Loaded context source metadata (AGENTS.md etc.).
    #[serde(rename = "context")]
    Context {
        schema_version: u32,
        seq: u64,
        time: String,
        sources: Vec<ContextSourceMeta>,
    },
    /// Content-free context working-set snapshot for one prompt turn.
    #[serde(rename = "context_ledger")]
    ContextLedger {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        ledger: ContextLedgerMeta,
    },
    /// Versioned context projection tied to one provider request attempt.
    #[serde(rename = "context_snapshot")]
    ContextSnapshot {
        schema_version: u32,
        seq: u64,
        time: String,
        snapshot: Box<ContextSnapshot>,
    },
    /// A user pin action for a context item.
    #[serde(rename = "context_pin")]
    ContextPin {
        schema_version: u32,
        seq: u64,
        time: String,
        item: ContextItemMeta,
        reason: String,
    },
    /// A user drop action for a context item.
    #[serde(rename = "context_drop")]
    ContextDrop {
        schema_version: u32,
        seq: u64,
        time: String,
        item: ContextItemMeta,
        reason: String,
    },
    /// A user recovery action for a context item.
    #[serde(rename = "context_recovery")]
    ContextRecovery {
        schema_version: u32,
        seq: u64,
        time: String,
        item: ContextItemMeta,
        reason: String,
    },
    /// An explicit lifecycle, relation, protection, or verification action.
    #[serde(rename = "context_lifecycle")]
    ContextLifecycle {
        schema_version: u32,
        seq: u64,
        time: String,
        audit: ContextLifecycleAudit,
    },
    /// A manual or automatic compaction audit record.
    #[serde(rename = "compaction")]
    Compaction {
        schema_version: u32,
        seq: u64,
        time: String,
        audit: CompactionAudit,
    },
    /// The user's decision on a compaction summary that required review.
    #[serde(rename = "compaction_review")]
    CompactionReview {
        schema_version: u32,
        seq: u64,
        time: String,
        recovery_handle: String,
        review: CompactionReviewResult,
    },
    /// A user-submitted prompt.
    #[serde(rename = "user")]
    User {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// Prompt assembly metadata for one user turn.
    #[serde(rename = "prompt_metadata")]
    PromptMetadata {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        metadata: PromptMetadata,
    },
    /// Final replayable assistant text.
    #[serde(rename = "assistant_finished")]
    AssistantFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// Final replayable reasoning text.
    #[serde(rename = "reasoning_finished")]
    ReasoningFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        text: String,
    },
    /// Provider token usage increment.
    #[serde(rename = "usage")]
    Usage {
        schema_version: u32,
        seq: u64,
        time: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Exact request-size accounting and provider usage for one request.
    #[serde(rename = "request_accounting")]
    RequestAccounting {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        accounting: Box<ProviderRequestAccounting>,
    },
    /// A tool call started.
    #[serde(rename = "tool_started")]
    ToolStarted {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        call_id: String,
        name: String,
        arguments: String,
        /// MCP metadata when this tool came from an MCP server.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp: Option<McpToolSessionMeta>,
    },
    /// A tool call finished.
    #[serde(rename = "tool_finished")]
    ToolFinished {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        call_id: String,
        status: ToolStatus,
        output: Vec<String>,
        /// Metadata and handle for bounded redacted recoverable evidence.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        artifact: Option<ArtifactMetadata>,
        /// MCP metadata when this tool came from an MCP server.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mcp: Option<McpToolSessionMeta>,
    },
    /// The agent run was cancelled.
    #[serde(rename = "cancelled")]
    Cancelled {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        reason: String,
    },
    /// The agent run failed.
    #[serde(rename = "failed")]
    Failed {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        error: String,
    },
    /// ACP session metadata.
    #[serde(rename = "acp_session")]
    AcpSession {
        schema_version: u32,
        seq: u64,
        time: String,
        /// Local append-only `thndrs` session id.
        local_session_id: String,
        /// Configured ACP agent name.
        agent_name: String,
        /// Opaque external ACP session id returned by the agent.
        acp_session_id: String,
        /// Redacted command display used to start the agent.
        command: String,
        /// Selected ACP protocol version.
        protocol_version: String,
        /// Optional ACP agent info name.
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_info_name: Option<String>,
        /// Optional ACP agent info version.
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_info_version: Option<String>,
        /// Optional ACP client info name.
        #[serde(skip_serializing_if = "Option::is_none")]
        client_info_name: Option<String>,
        /// Optional ACP client info version.
        #[serde(skip_serializing_if = "Option::is_none")]
        client_info_version: Option<String>,
    },
    /// Session title was renamed (latest wins).
    #[serde(rename = "session_renamed")]
    SessionRenamed {
        schema_version: u32,
        seq: u64,
        time: String,
        title: String,
    },
    /// A file write operation completed.
    ///
    /// Records the operation type, target path, and before/after metadata
    /// for session audit. Only hashes and byte counts are persisted but
    /// not file content.
    #[serde(rename = "file_write")]
    FileWrite {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        op: WriteOp,
        path: String,
        before_hash: Option<u64>,
        before_bytes: Option<usize>,
        after_hash: u64,
        after_bytes: usize,
        status: ToolStatus,
    },
    /// MCP configuration changed after the session was created.
    ///
    /// Records only file paths, sources, hashes, and loader diagnostics; raw
    /// MCP server command, env, and header values are not persisted here.
    #[serde(rename = "mcp_config_changed")]
    McpConfigChanged {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        previous_files: Vec<SessionConfigFile>,
        current_files: Vec<SessionConfigFile>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diagnostics: Vec<String>,
    },
    /// A shell command lifecycle event.
    ///
    /// One-shot commands produce a terminal record. Background commands
    /// produce a `running` start record followed by a terminal record when
    /// they exit, time out, or are cancelled. stdout/stderr are not stored
    /// directly — they are captured in redacted, capped output records.
    #[serde(rename = "shell_exec")]
    ShellExec {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        /// Registry id for a background process, when applicable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        process_id: Option<u64>,
        /// Full argv (program + args) joined with spaces.
        command: String,
        /// Working directory the command ran in.
        cwd: String,
        /// Lifecycle status: ok, failed, timeout, cancelled.
        process_status: String,
        /// Exit code if the process exited normally, else `None`.
        exit_code: Option<i32>,
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
        /// "one-shot" or "background".
        kind: String,
    },
    /// A skill was explicitly opened/activated in the session.
    #[serde(rename = "skill_activated")]
    SkillActivated {
        schema_version: u32,
        seq: u64,
        time: String,
        name: String,
        path: String,
        /// Hash of the raw `SKILL.md` file at `path`.
        content_hash: u64,
        /// Byte count of the raw `SKILL.md` file at `path`.
        byte_count: usize,
        /// Hash of the model-visible activation text after references are appended.
        #[serde(default)]
        rendered_content_hash: u64,
        /// Byte count of the model-visible activation text after references are appended.
        #[serde(default)]
        rendered_byte_count: usize,
        loaded_references: Vec<SkillReferenceRecord>,
    },
    /// Queued input lifecycle event recorded for audit and resume.
    #[serde(rename = "queued_input")]
    QueuedInput {
        schema_version: u32,
        seq: u64,
        time: String,
        /// Stable queue id. Older records use their sequence number on replay.
        #[serde(default)]
        queue_id: u64,
        /// "steering" or "follow-up".
        kind: String,
        /// "add", "edit", "retarget", "reorder", "sent", "cancelled", or "deleted".
        #[serde(default = "default_queue_action")]
        action: String,
        text: String,
    },
    /// ACP permission request metadata.
    #[serde(rename = "acp_permission_request")]
    AcpPermissionRequest {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        tool_call_id: String,
        title: String,
        options: Vec<AcpPermissionOptionRecord>,
    },
    /// ACP permission request outcome.
    #[serde(rename = "acp_permission_outcome")]
    AcpPermissionOutcome {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
        tool_call_id: String,
        outcome: String,
    },
}

impl SessionRecord {
    /// The sequence number of this record.
    pub fn seq(&self) -> u64 {
        match self {
            SessionRecord::SessionMeta { seq, .. }
            | SessionRecord::SessionFork { seq, .. }
            | SessionRecord::ContextCapturePolicy { seq, .. }
            | SessionRecord::RequestContentCaptured { seq, .. }
            | SessionRecord::Context { seq, .. }
            | SessionRecord::ContextLedger { seq, .. }
            | SessionRecord::ContextSnapshot { seq, .. }
            | SessionRecord::ContextPin { seq, .. }
            | SessionRecord::ContextDrop { seq, .. }
            | SessionRecord::ContextRecovery { seq, .. }
            | SessionRecord::ContextLifecycle { seq, .. }
            | SessionRecord::Compaction { seq, .. }
            | SessionRecord::CompactionReview { seq, .. }
            | SessionRecord::User { seq, .. }
            | SessionRecord::PromptMetadata { seq, .. }
            | SessionRecord::AssistantFinished { seq, .. }
            | SessionRecord::ReasoningFinished { seq, .. }
            | SessionRecord::Usage { seq, .. }
            | SessionRecord::RequestAccounting { seq, .. }
            | SessionRecord::ToolStarted { seq, .. }
            | SessionRecord::ToolFinished { seq, .. }
            | SessionRecord::Cancelled { seq, .. }
            | SessionRecord::Failed { seq, .. }
            | SessionRecord::AcpSession { seq, .. }
            | SessionRecord::SessionRenamed { seq, .. }
            | SessionRecord::FileWrite { seq, .. }
            | SessionRecord::McpConfigChanged { seq, .. }
            | SessionRecord::ShellExec { seq, .. }
            | SessionRecord::SkillActivated { seq, .. }
            | SessionRecord::QueuedInput { seq, .. }
            | SessionRecord::AcpPermissionRequest { seq, .. }
            | SessionRecord::AcpPermissionOutcome { seq, .. } => *seq,
        }
    }

    /// Serialize to a JSON string for JSONL append.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Convert a transcript [`Entry`] into a [`SessionRecord`].
    ///
    /// Only finalized entries are converted — streaming entries (assistant/
    /// reasoning still streaming, tools still running) are skipped because
    /// they represent incomplete live state.
    pub fn from_entry(entry: &Entry, seq: u64, time: &str, turn_id: &str) -> Option<SessionRecord> {
        Self::from_entry_with_artifact(entry, seq, time, turn_id, None)
    }

    /// Convert a transcript entry while attaching bounded artifact metadata.
    pub fn from_entry_with_artifact(
        entry: &Entry, seq: u64, time: &str, turn_id: &str, artifact: Option<ArtifactMetadata>,
    ) -> Option<SessionRecord> {
        match entry {
            Entry::User { text } => Some(SessionRecord::User {
                schema_version: SCHEMA_VERSION,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Agent { text, streaming: false } => Some(SessionRecord::AssistantFinished {
                schema_version: SCHEMA_VERSION,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Reasoning { text, streaming: false } => Some(SessionRecord::ReasoningFinished {
                schema_version: SCHEMA_VERSION,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                text: text.clone(),
            }),
            Entry::Error { text } => Some(SessionRecord::Failed {
                schema_version: SCHEMA_VERSION,
                seq,
                time: time.to_string(),
                turn_id: turn_id.to_string(),
                error: text.clone(),
            }),
            Entry::Tool { name, arguments: _, status, output } if *status != ToolStatus::Running => {
                let (tool_name, call_id) = split_tool_name_id(name);
                Some(SessionRecord::ToolFinished {
                    schema_version: SCHEMA_VERSION,
                    seq,
                    time: time.to_string(),
                    turn_id: turn_id.to_string(),
                    call_id,
                    status: *status,
                    output: artifacts::bounded_redacted_lines(output, artifacts::DEFAULT_MAX_ARTIFACT_BYTES),
                    artifact,
                    mcp: mcp_tool_session_meta(&tool_name),
                })
                .map(|r| (r, tool_name))
                .map(|(r, _)| r)
            }
            _ => None,
        }
    }

    /// Reconstruct a transcript [`Entry`] from this record.
    ///
    /// Returns `None` for records that don't map to a transcript row
    /// (session_meta, context, session_renamed).
    pub fn to_entry(&self) -> Option<Entry> {
        match self {
            SessionRecord::User { text, .. } => Some(Entry::User { text: text.clone() }),
            SessionRecord::PromptMetadata { .. } => None,
            SessionRecord::AssistantFinished { text, .. } => {
                Some(Entry::Agent { text: text.clone(), streaming: false })
            }
            SessionRecord::ReasoningFinished { text, .. } => {
                Some(Entry::Reasoning { text: text.clone(), streaming: false })
            }

            SessionRecord::ToolFinished { call_id, status, output, .. } => Some(Entry::Tool {
                name: format!("#{call_id}"),
                arguments: String::new(),
                status: *status,
                output: output.clone(),
            }),
            SessionRecord::Cancelled { reason, .. } => Some(Entry::Status { text: reason.clone() }),
            SessionRecord::Failed { error, .. } => Some(Entry::Error { text: error.clone() }),
            SessionRecord::AcpSession { agent_name, acp_session_id, client_info_name, .. } => {
                let client = client_info_name
                    .as_ref()
                    .map(|name| format!(" client {name}"))
                    .unwrap_or_default();
                Some(Entry::Status { text: format!("acp session {agent_name}: {acp_session_id}{client}") })
            }
            SessionRecord::FileWrite { op, path, status, .. } => {
                Some(Entry::Status { text: format!("{} {}: {path}", status.icon(), op.label()) })
            }
            SessionRecord::McpConfigChanged { .. } => None,
            SessionRecord::ShellExec { command, process_status, process_id, elapsed_ms, .. } => {
                let id = process_id.map_or_else(String::new, |id| format!(" [{id}]"));
                Some(Entry::Status { text: format!("shell{id} {process_status}: {command} ({elapsed_ms}ms)") })
            }
            SessionRecord::SkillActivated { name, path, rendered_byte_count, .. } => Some(Entry::Skill {
                name: name.clone(),
                path: path.clone(),
                content: String::new(),
                token_estimate: thndrs_agent::context::estimate_tokens(*rendered_byte_count),
                context_percent: None,
            }),
            SessionRecord::AcpPermissionRequest { tool_call_id, title, .. } => {
                Some(Entry::Status { text: format!("acp permission requested: {title} ({tool_call_id})") })
            }
            SessionRecord::AcpPermissionOutcome { tool_call_id, outcome, .. } => {
                Some(Entry::Status { text: format!("acp permission {tool_call_id}: {outcome}") })
            }
            _ => None,
        }
    }

    fn record_time(&self) -> Option<String> {
        serde_json::to_value(self)
            .ok()?
            .get("time")?
            .as_str()
            .map(str::to_string)
    }

    fn artifact_handles(&self) -> Vec<String> {
        let Ok(value) = serde_json::to_value(self) else {
            return Vec::new();
        };
        let mut handles = BTreeSet::new();
        collect_handles(&value, None, &mut handles);
        handles.into_iter().collect()
    }

    /// Set the seq field on a record (used by `SessionWriter::append`).
    fn set_seq(&mut self, seq: u64) {
        match self {
            SessionRecord::SessionMeta { seq: s, .. }
            | SessionRecord::SessionFork { seq: s, .. }
            | SessionRecord::ContextCapturePolicy { seq: s, .. }
            | SessionRecord::RequestContentCaptured { seq: s, .. }
            | SessionRecord::Context { seq: s, .. }
            | SessionRecord::ContextLedger { seq: s, .. }
            | SessionRecord::ContextSnapshot { seq: s, .. }
            | SessionRecord::ContextPin { seq: s, .. }
            | SessionRecord::ContextDrop { seq: s, .. }
            | SessionRecord::ContextRecovery { seq: s, .. }
            | SessionRecord::ContextLifecycle { seq: s, .. }
            | SessionRecord::Compaction { seq: s, .. }
            | SessionRecord::CompactionReview { seq: s, .. }
            | SessionRecord::User { seq: s, .. }
            | SessionRecord::PromptMetadata { seq: s, .. }
            | SessionRecord::AssistantFinished { seq: s, .. }
            | SessionRecord::ReasoningFinished { seq: s, .. }
            | SessionRecord::Usage { seq: s, .. }
            | SessionRecord::RequestAccounting { seq: s, .. }
            | SessionRecord::ToolStarted { seq: s, .. }
            | SessionRecord::ToolFinished { seq: s, .. }
            | SessionRecord::Cancelled { seq: s, .. }
            | SessionRecord::Failed { seq: s, .. }
            | SessionRecord::AcpSession { seq: s, .. }
            | SessionRecord::SessionRenamed { seq: s, .. }
            | SessionRecord::FileWrite { seq: s, .. }
            | SessionRecord::McpConfigChanged { seq: s, .. }
            | SessionRecord::ShellExec { seq: s, .. }
            | SessionRecord::SkillActivated { seq: s, .. }
            | SessionRecord::QueuedInput { seq: s, .. }
            | SessionRecord::AcpPermissionRequest { seq: s, .. }
            | SessionRecord::AcpPermissionOutcome { seq: s, .. } => *s = seq,
        }
    }
}

/// Metadata for a single prompt turn, suitable for append-only JSONL storage.
///
/// This is the audit record: enough to reconstruct *what* was sent without
/// storing *the content itself*. Full raw provider payloads are deliberately
/// excluded because they can contain prompt text, repo content, and secrets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptMetadata {
    /// Selected model name.
    pub model: String,
    /// Selected provider label.
    #[serde(default)]
    pub provider: String,
    /// Application-owned web-search backend label.
    pub search_mode: String,
    /// Renderer mode label.
    #[serde(default)]
    pub renderer_mode: String,
    /// Rounded date (YYYY-MM-DD) used for cache stability.
    pub date: String,
    /// Workspace root path.
    pub cwd: String,
    /// Ordered prompt fragment names included in the system prompt.
    #[serde(default)]
    pub prompt_fragments: Vec<String>,
    /// Model-visible documentation entry point paths.
    #[serde(default)]
    pub docs_map: Vec<String>,
    /// Metadata for each loaded AGENTS.md source (no content).
    pub context_sources: Vec<ContextSourceMeta>,
    /// Number of tools in the catalog sent this turn.
    pub tool_catalog_size: usize,
    /// Tool names in the catalog sent this turn.
    #[serde(default)]
    pub tool_names: Vec<String>,
    /// Number of available Agent Skills exposed as metadata this turn.
    pub skill_catalog_size: usize,
    /// Available skill names exposed as metadata this turn.
    #[serde(default)]
    pub skill_names: Vec<String>,
    /// Self-knowledge diagnostics visible for this turn.
    #[serde(default)]
    pub diagnostics: Vec<String>,
    /// Whether history reuse was active for this turn.
    pub history_reuse: bool,
    /// Content hash of the root AGENTS.md from the previous turn, if any.
    pub prev_context_hash: Option<u64>,
    /// Number of transcript entries included in the projected tail.
    pub transcript_tail_size: usize,
    /// Whether the user turn was non-empty.
    pub has_user_turn: bool,
}

impl PromptMetadata {
    /// Extract prompt metadata from a [`PromptBundle`] for session storage.
    ///
    /// This captures the structural metadata of the turn — model, search backend,
    /// context sources (hashes and truncation, not content), tool count, and
    /// transcript tail size. It does not store prompt text, AGENTS.md content,
    /// or provider request/response bodies.
    pub fn from_bundle(bundle: &PromptBundle) -> Self {
        let environment: &EnvironmentMetadata = &bundle.environment;
        let snapshot: internals::SelfKnowledgeSnapshot = bundle.into();
        PromptMetadata {
            model: environment.model.clone(),
            provider: snapshot.runtime.provider.provider,
            search_mode: environment.search_mode.label().to_string(),
            renderer_mode: snapshot.runtime.renderer_mode,
            date: environment.date.clone(),
            cwd: environment.cwd.clone(),
            prompt_fragments: snapshot.inventory.prompt_context.prompt_fragments,
            docs_map: snapshot
                .inventory
                .references
                .docs
                .iter()
                .map(|doc| doc.path.to_string())
                .collect(),
            context_sources: bundle
                .project_context
                .iter()
                .map(ContextSourceMeta::from_source)
                .collect(),
            tool_catalog_size: bundle.tool_catalog.len(),
            tool_names: snapshot.runtime.tools,
            skill_catalog_size: bundle.available_skills.len(),
            skill_names: snapshot
                .inventory
                .references
                .skills
                .into_iter()
                .map(|skill| skill.name)
                .collect(),
            diagnostics: snapshot.diagnostics,
            history_reuse: bundle.history_reuse == HistoryReuse::Available,
            prev_context_hash: bundle.prev_context_hash,
            transcript_tail_size: bundle.transcript_tail.len(),
            has_user_turn: !bundle.user_turn.is_empty(),
        }
    }
}

/// How compaction was initiated.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionTrigger {
    /// The user explicitly requested compaction.
    Manual,
    /// Context pressure triggered compaction before a provider request.
    Automatic,
}

/// Risk classification for the compacted range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionRisk {
    /// No known high-risk content was included in the covered range.
    Low,
    /// The covered range contains details that require explicit review policy.
    High,
}

/// Review state recorded for a compaction summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReviewResult {
    /// Review was not required by the active policy.
    NotRequired,
    /// Review is required but not yet resolved.
    Pending,
    /// A reviewer accepted the proposed summary.
    Approved,
    /// A reviewer rejected the proposed summary.
    Rejected,
}

impl CompactionReviewResult {
    pub fn label(self) -> &'static str {
        match self {
            CompactionReviewResult::NotRequired => "not-required",
            CompactionReviewResult::Pending => "pending",
            CompactionReviewResult::Approved => "approved",
            CompactionReviewResult::Rejected => "rejected",
        }
    }
}

/// Source hash associated with a compacted input item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactionSourceHash {
    /// Stable source item id or session-range handle.
    pub id: String,
    /// Hash of the source content, when one is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<u64>,
}

/// Provider token usage for the compaction request, when reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactionTokenUsage {
    /// Tokens supplied to the configured model.
    pub input_tokens: u64,
    /// Tokens returned in the compaction summary.
    pub output_tokens: u64,
}

/// Locally measured effect of replacing one closed context range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactionLocalReceipt {
    /// Exact rendered bytes before compression.
    pub before_bytes: usize,
    /// Exact rendered bytes after compression.
    pub after_bytes: usize,
    /// Conservative local estimate before compression.
    pub before_token_estimate: u64,
    /// Conservative local estimate after compression.
    pub after_token_estimate: u64,
}

/// Provider-native context-editing outcome for a reviewed range decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ProviderContextEdit {
    /// The adapter did not expose a native editing capability for this request.
    Unavailable {
        /// Human-readable capability diagnostic retained with the audit record.
        diagnostic: String,
    },
    /// An adapter applied the already-approved provider-neutral decision.
    Applied {
        /// Provider-supplied opaque edit reference.
        edit_id: String,
    },
}

/// Complete durable audit payload for one compaction.
///
/// The summary is intentionally retained because it becomes model-visible
/// working context. The covered source is represented only by ranges, stable
/// handles, and hashes; full transcript, file, and provider payload
/// content never belongs in this record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactionAudit {
    /// Summary that replaces the covered range in the active working set.
    pub summary: String,
    /// Versioned summary contract used to produce `summary`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_summary: Option<RangeSummary>,
    /// Stable item id of the summary that replaces the covered range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_id: Option<String>,
    /// Inclusive first session sequence replaced by this summary.
    pub covered_start_seq: u64,
    /// Inclusive final session sequence replaced by this summary.
    pub covered_end_seq: u64,
    /// Content hashes for covered sources, where known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_hashes: Vec<CompactionSourceHash>,
    /// Earlier summaries retained as provenance rather than rewritten prose.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_summary_ids: Vec<String>,
    /// Manual or automatic initiation.
    pub trigger: CompactionTrigger,
    /// Risk classification evaluated before applying the summary.
    pub risk: CompactionRisk,
    /// Review result, when a review policy applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<CompactionReviewResult>,
    /// Stable handles used to recover covered detail from the original session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recovery_handles: Vec<String>,
    /// Configured model that generated the summary.
    pub model: String,
    /// Provider-reported compaction token usage, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompactionTokenUsage>,
    /// Local before/after measurements for this replacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_receipt: Option<CompactionLocalReceipt>,
    /// Provider-native editing capability outcome; it never replaces the
    /// provider-neutral review and audit decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_context_edit: Option<ProviderContextEdit>,
}

impl CompactionAudit {
    /// Return a copy with known secret-shaped fragments redacted.
    fn redacted(&self) -> Self {
        CompactionAudit {
            summary: tools::shell::redact_secrets(&self.summary),
            typed_summary: self.typed_summary.as_ref().map(redact_range_summary),
            summary_id: self.summary_id.clone(),
            covered_start_seq: self.covered_start_seq,
            covered_end_seq: self.covered_end_seq,
            source_hashes: self.source_hashes.clone(),
            source_summary_ids: self.source_summary_ids.clone(),
            trigger: self.trigger,
            risk: self.risk,
            review: self.review,
            recovery_handles: self.recovery_handles.clone(),
            model: self.model.clone(),
            usage: self.usage,
            local_receipt: self.local_receipt,
            native_context_edit: self.native_context_edit.clone(),
        }
    }
}

/// Persisted metadata for a loaded skill reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SkillReferenceRecord {
    pub path: String,
    pub content_hash: u64,
    pub byte_count: usize,
    pub truncated: bool,
}

impl From<&SkillReferenceMeta> for SkillReferenceRecord {
    fn from(reference: &SkillReferenceMeta) -> Self {
        SkillReferenceRecord {
            path: reference.path.display().to_string(),
            content_hash: reference.content_hash,
            byte_count: reference.byte_count,
            truncated: reference.truncated,
        }
    }
}

/// Action variants supported by [`SessionWriter::append_context_action`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextActionKind {
    Pin,
    Drop,
    Recovery,
}

/// Append-only JSONL session writer.
///
/// Each session is a single `.jsonl` file. Records are appended one per line
/// and never rewritten. The writer tracks a monotonic `seq` counter.
#[derive(Debug)]
pub struct SessionWriter {
    path: PathBuf,
    seq: u64,
    session_id: String,
    lock_path: PathBuf,
    _lock: File,
}

impl SessionWriter {
    /// Create a new session file in `dir` with the given session id.
    ///
    /// Writes the initial `session_meta` record as the first line.
    #[expect(
        clippy::too_many_arguments,
        reason = "session metadata is written as a flat JSONL record"
    )]
    pub fn create(
        dir: &Path, session_id: &str, cwd: &str, title: &str, provider: &str, model: &str, websearch: &str,
        app_version: &str, config: Option<SessionConfigMeta>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{session_id}.jsonl"));
        let (lock_path, lock) = acquire_writer_lock(&path)?;

        let record = SessionRecord::SessionMeta {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            title: title.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            websearch: websearch.to_string(),
            app_version: app_version.to_string(),
            config,
        };

        let write_result = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
            writeln!(file, "{}", record.to_json().map_err(io_err)?)
        })();
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&lock_path);
            return Err(error);
        }

        Ok(SessionWriter { path, seq: 1, session_id: session_id.to_string(), lock_path, _lock: lock })
    }

    /// Sequence number that will be assigned to the next appended record.
    pub fn next_sequence(&self) -> u64 {
        self.seq
    }

    /// Reopen a validated existing session file for append-only continuation.
    pub fn resume(path: &Path, session_id: &str) -> std::io::Result<Self> {
        let (lock_path, lock) = acquire_writer_lock(path)?;
        let records = match SessionReader::read_validated_records(path, session_id) {
            Ok(records) => records,
            Err(error) => {
                let _ = std::fs::remove_file(&lock_path);
                return Err(error);
            }
        };
        let seq = records
            .iter()
            .map(SessionRecord::seq)
            .max()
            .map_or(0, |max_seq| max_seq.saturating_add(1));

        if let Err(error) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = std::fs::remove_file(&lock_path);
            return Err(error);
        }

        Ok(SessionWriter { path: path.to_path_buf(), seq, session_id: session_id.to_string(), lock_path, _lock: lock })
    }

    /// Append a validated display-name change without rewriting history.
    pub fn append_rename(&mut self, name: &str) -> std::io::Result<()> {
        let name = validate_session_name(name)?;
        self.append(SessionRecord::SessionRenamed {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            title: name.to_string(),
        })
    }

    /// Append a record to the session file.
    pub fn append(&mut self, mut record: SessionRecord) -> std::io::Result<()> {
        let seq = self.seq;
        record.set_seq(seq);

        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        self.seq += 1;
        Ok(())
    }

    /// Append a transcript entry as a record, if it maps to one.
    ///
    /// Streaming/live entries are skipped — only finalized entries are
    /// persisted for replay.
    pub fn append_entry(&mut self, entry: &Entry, turn_id: &str) -> std::io::Result<()> {
        if let Some(record) = SessionRecord::from_entry(entry, self.seq, &datetime::now_iso8601(), turn_id) {
            self.seq += 1;
            let line = record.to_json().map_err(io_err)?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    /// Append a finalized transcript entry with bounded artifact metadata.
    pub fn append_entry_with_artifact(
        &mut self, entry: &Entry, turn_id: &str, artifact: Option<ArtifactMetadata>,
    ) -> std::io::Result<()> {
        if let Some(record) =
            SessionRecord::from_entry_with_artifact(entry, self.seq, &datetime::now_iso8601(), turn_id, artifact)
        {
            self.seq += 1;
            let line = record.to_json().map_err(io_err)?;
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
            writeln!(file, "{line}")?;
        }
        Ok(())
    }

    /// Append a `tool_started` record for a tool call that has begun.
    ///
    /// This records the command start: tool name, call id, and arguments.
    /// The matching `tool_finished` (via [`Self::append_entry`]) records the
    /// output, status, and summary. For `run_shell`, an additional
    /// [`Self::append_shell_exec`] record captures exit code, elapsed time, and
    /// process kind.
    pub fn append_tool_started(&mut self, turn_id: &str, call_id: &str, name: &str, args: &str) -> std::io::Result<()> {
        let record = SessionRecord::ToolStarted {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: args.to_string(),
            mcp: mcp_tool_session_meta(name),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a context record with loaded AGENTS.md source metadata.
    pub fn append_context(&mut self, sources: &[ContextSource]) -> std::io::Result<()> {
        let metas: Vec<ContextSourceMeta> = sources.iter().map(ContextSourceMeta::from_source).collect();
        let record = SessionRecord::Context {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            sources: metas,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a content-free context ledger snapshot for a prompt turn.
    pub fn append_context_ledger(&mut self, turn_id: &str, ledger: &ContextLedger) -> std::io::Result<()> {
        self.append(SessionRecord::ContextLedger {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            ledger: ContextLedgerMeta::from(ledger),
        })
    }

    /// Append a versioned context snapshot for one request attempt.
    pub fn append_context_snapshot(&mut self, snapshot: ContextSnapshot) -> std::io::Result<()> {
        self.append(SessionRecord::ContextSnapshot {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            snapshot: Box::new(snapshot),
        })
    }

    /// Append the durable capture policy used by this session.
    pub fn append_context_capture_policy(&mut self, policy: &ContextCapturePolicy) -> std::io::Result<()> {
        self.append(SessionRecord::ContextCapturePolicy {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            policy: policy.clone(),
        })
    }

    /// Append sanitized normalized request content after policy validation.
    pub fn append_captured_request(&mut self, capture: CapturedRequestContent) -> std::io::Result<()> {
        self.append(SessionRecord::RequestContentCaptured {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            capture,
        })
    }

    /// Append a content-free user pin action.
    pub fn append_context_pin(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Pin)
    }

    /// Append a content-free user drop action.
    pub fn append_context_drop(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Drop)
    }

    /// Append a content-free user recovery action.
    pub fn append_context_recovery(&mut self, item: &ContextItem, reason: &str) -> std::io::Result<()> {
        self.append_context_action(item, reason, ContextActionKind::Recovery)
    }

    /// Append an explicit lifecycle transition with content-free post-state
    /// metadata. Callers should apply the returned pure transition only after
    /// this append succeeds.
    pub fn append_context_lifecycle(
        &mut self, item: &ContextItem, action: thndrs_agent::context::ContextLifecycleAction, reason: &str,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::ContextLifecycle {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            audit: ContextLifecycleAudit {
                action,
                item: ContextItemMeta::from(item),
                reason: tools::shell::redact_secrets(reason),
            },
        })
    }

    /// Append a compaction audit record without source payloads.
    pub fn append_compaction(&mut self, audit: &CompactionAudit) -> std::io::Result<()> {
        self.append(SessionRecord::Compaction {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            audit: audit.redacted(),
        })
    }

    /// Append the review decision for a previously pending compaction.
    pub fn append_compaction_review(
        &mut self, recovery_handle: &str, review: CompactionReviewResult,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::CompactionReview {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            recovery_handle: tools::shell::redact_secrets(recovery_handle),
            review,
        })
    }

    /// Append one typed context action without exposing content fields.
    fn append_context_action(
        &mut self, item: &ContextItem, reason: &str, action: ContextActionKind,
    ) -> std::io::Result<()> {
        let item = ContextItemMeta::from(item);
        let reason = tools::shell::redact_secrets(reason);
        let record = match action {
            ContextActionKind::Pin => SessionRecord::ContextPin {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
            ContextActionKind::Drop => SessionRecord::ContextDrop {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
            ContextActionKind::Recovery => SessionRecord::ContextRecovery {
                schema_version: SCHEMA_VERSION,
                seq: 0,
                time: datetime::now_iso8601(),
                item,
                reason,
            },
        };
        self.append(record)
    }

    /// Append prompt assembly provenance for a user turn.
    pub fn append_prompt_metadata(&mut self, turn_id: &str, metadata: &PromptMetadata) -> std::io::Result<()> {
        let record = SessionRecord::PromptMetadata {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            metadata: metadata.clone(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append provider token usage for the session.
    pub fn append_usage(&mut self, input_tokens: u64, output_tokens: u64) -> std::io::Result<()> {
        if input_tokens == 0 && output_tokens == 0 {
            return Ok(());
        }

        let record = SessionRecord::Usage {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            input_tokens,
            output_tokens,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append accounting for one successful provider request.
    pub fn append_request_accounting(
        &mut self, turn_id: &str, accounting: &ProviderRequestAccounting,
    ) -> std::io::Result<()> {
        self.append(SessionRecord::RequestAccounting {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            accounting: Box::new(accounting.clone()),
        })
    }

    /// Append a file-write audit record.
    ///
    /// Records the operation type, path, before/after hashes and byte counts,
    /// and status. File content is never stored — only hashes and byte counts,
    /// so secrets and large files are not persisted.
    pub fn append_file_write(
        &mut self, turn_id: &str, result: &tools::WriteResult, status: ToolStatus,
    ) -> std::io::Result<()> {
        let record = SessionRecord::FileWrite {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            op: result.op,
            path: result.path.display().to_string(),
            before_hash: result.before_hash,
            before_bytes: result.before_bytes,
            after_hash: result.after_hash,
            after_bytes: result.after_bytes,
            status,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append an audit record when MCP config file hashes change mid-session.
    pub fn append_mcp_config_changed(
        &mut self, turn_id: &str, previous_files: Vec<SessionConfigFile>, current_files: Vec<SessionConfigFile>,
        diagnostics: Vec<String>,
    ) -> std::io::Result<()> {
        let record = SessionRecord::McpConfigChanged {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            previous_files,
            current_files,
            diagnostics,
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a shell-execution audit record.
    ///
    /// Records the command argv, working directory, registry id, lifecycle
    /// status, exit code, elapsed time, and process kind. stdout/stderr are
    /// not stored here — they are captured in redacted and capped output
    /// records.
    pub fn append_shell_exec(&mut self, turn_id: &str, result: &shell::ProcessResult) -> std::io::Result<()> {
        let record = SessionRecord::ShellExec {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            process_id: result.process_id,
            command: shell::redact_secrets(&result.command.join(" ")),
            cwd: result.cwd.display().to_string(),
            process_status: result.status.label().to_string(),
            exit_code: result.exit_code,
            elapsed_ms: result.elapsed.as_millis() as u64,
            kind: result.kind.label().to_string(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a skill activation metadata record.
    pub fn append_skill_activation(&mut self, activation: &SkillActivation) -> std::io::Result<()> {
        let record = SessionRecord::SkillActivated {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            name: activation.name.clone(),
            path: activation.path.display().to_string(),
            content_hash: activation.content_hash,
            byte_count: activation.byte_count,
            rendered_content_hash: activation.rendered_content_hash,
            rendered_byte_count: activation.rendered_byte_count,
            loaded_references: activation
                .loaded_references
                .iter()
                .map(SkillReferenceRecord::from)
                .collect(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append a queued input audit record.
    pub fn append_queued(&mut self, queue_id: u64, kind: &str, action: &str, text: &str) -> std::io::Result<()> {
        let record = SessionRecord::QueuedInput {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            queue_id,
            kind: kind.to_string(),
            action: action.to_string(),
            text: text.to_string(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP permission request metadata.
    pub fn append_acp_permission_request(&mut self, turn_id: &str, request: &PendingPermission) -> std::io::Result<()> {
        let record = SessionRecord::AcpPermissionRequest {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            tool_call_id: request.tool_call_id.clone(),
            title: request.title.clone(),
            options: request
                .options
                .iter()
                .map(|option| AcpPermissionOptionRecord {
                    id: option.id.clone(),
                    name: option.name.clone(),
                    kind: option.kind.label().to_string(),
                })
                .collect(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP session metadata.
    pub fn append_acp_session(&mut self, metadata: &AcpSessionMetadata) -> std::io::Result<()> {
        let record = SessionRecord::AcpSession {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            local_session_id: self.session_id.clone(),
            agent_name: metadata.agent_name.clone(),
            acp_session_id: metadata.acp_session_id.clone(),
            command: metadata.command.clone(),
            protocol_version: metadata.protocol_version.clone(),
            agent_info_name: metadata.agent_info_name.clone(),
            agent_info_version: metadata.agent_info_version.clone(),
            client_info_name: metadata.client_info_name.clone(),
            client_info_version: metadata.client_info_version.clone(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// Append ACP permission outcome metadata.
    pub fn append_acp_permission_outcome(
        &mut self, turn_id: &str, tool_call_id: &str, outcome: &str,
    ) -> std::io::Result<()> {
        let record = SessionRecord::AcpPermissionOutcome {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
            tool_call_id: tool_call_id.to_string(),
            outcome: outcome.to_string(),
        };
        self.seq += 1;
        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// The session file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The session id.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl Drop for SessionWriter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// Reads a session JSONL file and reconstructs transcript entries.
///
/// Corrupt lines are skipped silently — the rest of the file is still
/// readable. This makes resume resilient to partial writes.
pub struct SessionReader;

impl SessionReader {
    /// Read every record while validating session identity and ordering.
    ///
    /// Unlike the recovery-oriented readers, this rejects malformed lines and
    /// is used before a session is opened for continued writing.
    pub fn read_validated_records(path: &Path, session_id: &str) -> std::io::Result<Vec<SessionRecord>> {
        let content = std::fs::read_to_string(path)?;
        let mut records = Vec::new();
        let mut previous_sequence = None;

        for (index, line) in content.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let record = SessionRecord::from_json(line).map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session record {} is corrupt: {error}", index + 1),
                )
            })?;
            if previous_sequence.is_some_and(|sequence| record.seq() <= sequence) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session record {} has an invalid sequence", index + 1),
                ));
            }
            previous_sequence = Some(record.seq());
            records.push(record);
        }

        match records.first() {
            Some(SessionRecord::SessionMeta { session_id: stored_id, .. }) if stored_id == session_id => {}
            Some(SessionRecord::SessionMeta { session_id: stored_id, .. }) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("session identity mismatch: expected `{session_id}`, found `{stored_id}`"),
                ));
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "session metadata is missing",
                ));
            }
        }

        Ok(records)
    }

    /// Read a session file and return all records, in order.
    ///
    /// Corrupt lines are skipped. Returns an empty vec if the file does
    /// not exist.
    pub fn read_records(path: &Path) -> Vec<SessionRecord> {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        content
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| SessionRecord::from_json(line).ok())
            .collect()
    }

    /// Read only the trailing portion of a session file.
    ///
    /// If the bounded read starts in the middle of a JSONL record, that first
    /// partial record is discarded. This is suitable for bounded input recall,
    /// where recent turns matter more than a full historical reconstruction.
    pub fn read_records_from_tail(path: &Path, max_bytes: usize) -> Vec<SessionRecord> {
        use std::io::{Read, Seek};

        if max_bytes == 0 {
            return Vec::new();
        }

        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(file_len) = file.metadata().map(|metadata| metadata.len()) else {
            return Vec::new();
        };
        let start = file_len.saturating_sub(max_bytes as u64);
        let read_start = start.saturating_sub(1);
        if file.seek(std::io::SeekFrom::Start(read_start)).is_err() {
            return Vec::new();
        }

        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        let content = String::from_utf8_lossy(&bytes);
        let complete_records = if start == 0 {
            content.as_ref()
        } else {
            content.split_once('\n').map_or("", |(_, records)| records)
        };

        complete_records
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| SessionRecord::from_json(line).ok())
            .collect()
    }

    /// Read a session file and reconstruct the transcript.
    ///
    /// Only records that map to [`Entry`] values are included. Metadata
    /// records (session_meta, context, session_renamed) are skipped.
    pub fn read_transcript(path: &Path) -> Vec<Entry> {
        Self::read_records(path)
            .into_iter()
            .filter_map(|r| r.to_entry())
            .collect()
    }

    /// Replay semantic session records into stable transcript blocks.
    ///
    /// Matching tool and permission lifecycle records update their original
    /// block so resume restores the same action, target, and final state.
    pub fn read_transcript_blocks(path: &Path) -> TranscriptBlocks {
        let records = Self::read_records(path);
        let context_history = ContextHistory::from_records(&records);
        let mut transcript = TranscriptBlocks::new();
        let mut previous_failure: Option<(String, String)> = None;
        for record in records {
            let duplicate_failure = match &record {
                SessionRecord::Failed { turn_id, error, .. } => previous_failure
                    .as_ref()
                    .is_some_and(|(previous_turn, previous_error)| previous_turn == turn_id && previous_error == error),
                _ => false,
            };
            previous_failure = match &record {
                SessionRecord::Failed { turn_id, error, .. } => Some((turn_id.clone(), error.clone())),
                _ => None,
            };
            if duplicate_failure {
                continue;
            }
            if let Some((id, text)) = context_history.transcript_event(&record) {
                transcript.push_context_event(id, text);
                continue;
            }
            match &record {
                SessionRecord::ToolStarted { call_id, name, arguments, .. } => {
                    if transcript.queue_tool(call_id, name, arguments).is_ok() {
                        let _ = transcript.start_tool(call_id);
                    }
                }
                SessionRecord::ToolFinished { call_id, status, output, artifact, .. } => {
                    if transcript
                        .finish_tool(
                            call_id,
                            *status,
                            output.clone(),
                            artifact.as_ref().is_some_and(|artifact| artifact.truncated),
                        )
                        .is_err()
                        && let Some(entry) = record.to_entry()
                    {
                        transcript.push(entry);
                    }
                }
                SessionRecord::AcpPermissionRequest { tool_call_id, title, .. } => {
                    transcript.push_permission(
                        tool_call_id,
                        format!("acp permission requested: {title} ({tool_call_id})"),
                    );
                }
                SessionRecord::AcpPermissionOutcome { tool_call_id, outcome, .. } => {
                    let text = format!("acp permission {tool_call_id}: {outcome}");
                    if !transcript.resolve_permission(tool_call_id, text.clone()) {
                        transcript.push_permission(tool_call_id, text);
                    }
                }
                SessionRecord::ShellExec {
                    process_id: Some(process_id), command, process_status, elapsed_ms, ..
                } => {
                    transcript.push_child_activity(
                        *process_id,
                        format!("shell [{process_id}] {process_status}: {command} ({elapsed_ms}ms)"),
                    );
                }
                _ => {
                    if let Some(entry) = record.to_entry() {
                        transcript.push(entry);
                    }
                }
            }
        }
        transcript
    }

    /// Read the session title from a session file.
    ///
    /// Returns the latest `session_renamed` title, or the initial
    /// `session_meta` title, or the file stem as a fallback.
    pub fn read_title(path: &Path) -> String {
        let records = Self::read_records(path);
        for r in records.iter().rev() {
            if let SessionRecord::SessionRenamed { title, .. } = r {
                return title.clone();
            }
        }

        for r in &records {
            if let SessionRecord::SessionMeta { title, .. } = r {
                return title.clone();
            }
        }

        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string()
    }

    /// Read compact sidebar metadata from a session file.
    pub fn read_summary(path: &Path) -> SessionSummary {
        let records = Self::read_records(path);
        let mut title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string();
        let mut model = String::from("unknown");
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut accounted_input_tokens = 0u64;
        let mut accounted_output_tokens = 0u64;
        let mut has_request_accounting = false;

        for record in records {
            match record {
                SessionRecord::SessionMeta { title: t, model: m, .. } => {
                    title = t;
                    model = m;
                }
                SessionRecord::SessionRenamed { title: t, .. } => title = t,
                SessionRecord::Usage { input_tokens: i, output_tokens: o, .. } => {
                    input_tokens = input_tokens.saturating_add(i);
                    output_tokens = output_tokens.saturating_add(o);
                }
                SessionRecord::RequestAccounting { accounting, .. } => {
                    has_request_accounting = true;
                    if let Some(usage) = accounting.provider_usage {
                        accounted_input_tokens =
                            accounted_input_tokens.saturating_add(usage.components.input_tokens.unwrap_or(0));
                        accounted_output_tokens =
                            accounted_output_tokens.saturating_add(usage.components.output_tokens.unwrap_or(0));
                    }
                }
                _ => {}
            }
        }

        if has_request_accounting {
            input_tokens = accounted_input_tokens;
            output_tokens = accounted_output_tokens;
        }

        SessionSummary { title, model, input_tokens, output_tokens }
    }

    /// Read a renderer-independent, redacted JSON projection of every valid
    /// record in sequence order. Malformed lines remain omitted just as they
    /// are for transcript recovery.
    pub fn read_redacted_records(path: &Path) -> Vec<serde_json::Value> {
        Self::read_records(path)
            .into_iter()
            .filter_map(|record| serde_json::to_value(record).ok())
            .map(redact_json_value)
            .collect()
    }
}

/// Compact session metadata for sidebar display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub title: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl SessionSummary {
    pub fn sidebar_label(&self) -> String {
        format!("{}\nin {} out {}", self.model, self.input_tokens, self.output_tokens)
    }
}

/// An error resolving a user-supplied local session identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionLookupError {
    /// No session has the supplied exact id or prefix.
    NotFound { query: String },
    /// More than one session has the supplied prefix.
    Ambiguous { query: String, matches: Vec<String> },
}

impl std::fmt::Display for SessionLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { query } => write!(formatter, "session `{query}` is not found"),
            Self::Ambiguous { query, matches } => {
                write!(
                    formatter,
                    "session prefix `{query}` is ambiguous; matches: {}",
                    matches.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for SessionLookupError {}

/// The sessions directory under a workspace root: `{root}/.thndrs/sessions/`.
pub fn sessions_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".thndrs").join("sessions")
}

/// List session files in a directory, sorted newest-first by modification time.
pub fn list_session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().is_some_and(|ext| ext == "jsonl") { Some(path) } else { None }
        })
        .collect();

    files.sort_by(|a, b| {
        let mtime_a = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let mtime_b = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        mtime_b.cmp(&mtime_a).then_with(|| b.cmp(a))
    });
    files
}

/// Resolve an exact session id or a unique id prefix.
///
/// Matching only considers `.jsonl` files in `dir`; a missing or corrupt
/// session file therefore cannot prevent other valid files from being found.
pub fn resolve_session_file(dir: &Path, query: &str) -> Result<PathBuf, SessionLookupError> {
    let files = list_session_files(dir);
    if let Some(path) = files.iter().find(|path| session_id_from_path(path) == Some(query)) {
        return Ok(path.clone());
    }

    let matches: Vec<PathBuf> = files
        .into_iter()
        .filter(|path| session_id_from_path(path).is_some_and(|id| id.starts_with(query)))
        .collect();
    match matches.as_slice() {
        [] => Err(SessionLookupError::NotFound { query: query.to_string() }),
        [path] => Ok(path.clone()),
        _ => Err(SessionLookupError::Ambiguous {
            query: query.to_string(),
            matches: matches
                .iter()
                .filter_map(|path| session_id_from_path(path).map(ToString::to_string))
                .collect(),
        }),
    }
}

/// List session titles from a directory, newest-first.
///
/// Each title is read from the session file (latest rename or session_meta).
/// Falls back to the file stem if the file cannot be parsed.
pub fn list_session_titles(dir: &Path) -> Vec<String> {
    list_session_files(dir)
        .into_iter()
        .map(|p| SessionReader::read_title(&p))
        .collect()
}

/// List session sidebar summaries, newest-first.
pub fn list_session_summaries(dir: &Path) -> Vec<SessionSummary> {
    list_session_files(dir)
        .into_iter()
        .map(|p| SessionReader::read_summary(&p))
        .collect()
}

/// Find the most recently modified session file in a directory.
///
/// Returns `None` if the directory does not exist or has no `.jsonl` files.
pub fn latest_session_file(dir: &Path) -> Option<PathBuf> {
    list_session_files(dir).into_iter().next()
}

/// Generate a session id from a timestamp (format: `session-YYYYMMDD-HHMMSS`).
pub fn generate_session_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86_400;
    let remainder = secs % 86_400;
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;
    let date = datetime::date_from_days(days);
    let date_compact = date.replace('-', "");
    format!("session-{date_compact}-{hour:02}{minute:02}{second:02}")
}

/// Create an independently resumable session from a settled parent turn.
///
/// The fork copies only the replayable semantic prefix through the requested
/// terminal turn record. Runtime-owned state and incomplete lifecycle pairs
/// stay with the parent session.
pub fn fork_session(dir: &Path, parent_path: &Path, parent_session_id: &str, turn_id: &str) -> std::io::Result<String> {
    use std::collections::HashSet;

    let records = SessionReader::read_validated_records(parent_path, parent_session_id)?;
    let boundary = records
        .iter()
        .rposition(|record| {
            matches!(
                record,
                SessionRecord::AssistantFinished { turn_id: record_turn, .. }
                    | SessionRecord::Cancelled { turn_id: record_turn, .. }
                    | SessionRecord::Failed { turn_id: record_turn, .. }
                    if record_turn == turn_id
            )
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("turn `{turn_id}` is not a settled replayable boundary"),
            )
        })?;
    if !records[..=boundary]
        .iter()
        .any(|record| matches!(record, SessionRecord::User { turn_id: record_turn, .. } if record_turn == turn_id))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("turn `{turn_id}` has no replayable user message"),
        ));
    }

    let (cwd, provider, model, websearch, app_version, config) = match &records[0] {
        SessionRecord::SessionMeta { cwd, provider, model, websearch, app_version, config, .. } => (
            cwd.clone(),
            provider.clone(),
            model.clone(),
            websearch.clone(),
            app_version.clone(),
            config.clone(),
        ),
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "validated session does not begin with metadata",
            ));
        }
    };
    let title = SessionReader::read_title(parent_path);
    let completed_tools: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::ToolFinished { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    let completed_permissions: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::AcpPermissionOutcome { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();
    let started_tools: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::ToolStarted { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();
    let requested_permissions: HashSet<String> = records[..=boundary]
        .iter()
        .filter_map(|record| match record {
            SessionRecord::AcpPermissionRequest { tool_call_id, .. } => Some(tool_call_id.clone()),
            _ => None,
        })
        .collect();

    let mut lineage = records[..=boundary]
        .iter()
        .find_map(|record| match record {
            SessionRecord::SessionFork { lineage, .. } => Some(lineage.clone()),
            _ => None,
        })
        .unwrap_or_default();
    lineage.push(SessionLineageEntry { session_id: parent_session_id.to_string(), turn_id: turn_id.to_string() });

    let base_id = generate_session_id();
    let mut suffix = 1_u64;
    let session_id = loop {
        let candidate = if suffix == 1 { base_id.clone() } else { format!("{base_id}-{suffix}") };
        if !dir.join(format!("{candidate}.jsonl")).exists() {
            break candidate;
        }
        suffix = suffix.saturating_add(1);
    };
    let mut writer = SessionWriter::create(
        dir,
        &session_id,
        &cwd,
        &title,
        &provider,
        &model,
        &websearch,
        &app_version,
        config,
    )?;
    writer.append(SessionRecord::SessionFork {
        schema_version: SCHEMA_VERSION,
        seq: 0,
        time: datetime::now_iso8601(),
        parent_session_id: parent_session_id.to_string(),
        parent_turn_id: turn_id.to_string(),
        lineage,
    })?;
    for record in records.into_iter().take(boundary + 1) {
        let copy = match &record {
            SessionRecord::SessionMeta { .. }
            | SessionRecord::SessionFork { .. }
            | SessionRecord::AcpSession { .. }
            | SessionRecord::QueuedInput { .. } => false,
            SessionRecord::ToolStarted { call_id, .. } => completed_tools.contains(call_id.as_str()),
            SessionRecord::ToolFinished { call_id, .. } => started_tools.contains(call_id.as_str()),
            SessionRecord::AcpPermissionRequest { tool_call_id, .. } => {
                completed_permissions.contains(tool_call_id.as_str())
            }
            SessionRecord::AcpPermissionOutcome { tool_call_id, .. } => {
                requested_permissions.contains(tool_call_id.as_str())
            }
            SessionRecord::CompactionReview { review: CompactionReviewResult::Pending, .. } => false,
            SessionRecord::ShellExec { process_status, .. } => process_status != "running",
            _ => true,
        };
        if copy {
            writer.append(record)?;
        }
    }
    Ok(session_id)
}

fn redact_range_summary(summary: &RangeSummary) -> RangeSummary {
    let mut redacted = summary.clone();
    redacted.objective = tools::shell::redact_secrets(&redacted.objective);
    for values in [
        &mut redacted.findings,
        &mut redacted.decisions,
        &mut redacted.paths,
        &mut redacted.failures,
        &mut redacted.verification,
        &mut redacted.blockers,
    ] {
        for value in values {
            *value = tools::shell::redact_secrets(value);
        }
    }
    for fact in &mut redacted.protected_facts {
        fact.text = tools::shell::redact_secrets(&fact.text);
    }
    redacted
}

/// Convert a serde_json error into an io::Error.
fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
}

fn validate_session_name(name: &str) -> std::io::Result<&str> {
    if name.chars().any(char::is_control) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session names cannot contain control characters",
        ));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "session names cannot be empty",
        ));
    }
    if name.chars().count() > MAX_SESSION_NAME_CHARS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("session names cannot exceed {MAX_SESSION_NAME_CHARS} characters"),
        ));
    }
    Ok(name)
}

fn acquire_writer_lock(path: &Path) -> std::io::Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.jsonl");
    let lock_path = path.with_file_name(format!("{file_name}.lock"));
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    format!("session `{}` already has an active writer", path.display()),
                )
            } else {
                error
            }
        })?;
    Ok((lock_path, lock))
}

fn session_id_from_path(path: &Path) -> Option<&str> {
    path.file_stem().and_then(|stem| stem.to_str())
}

fn redact_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(shell::redact_secrets(&text)),
        serde_json::Value::Array(items) => serde_json::Value::Array(items.into_iter().map(redact_json_value).collect()),
        serde_json::Value::Object(items) => serde_json::Value::Object(
            items
                .into_iter()
                .map(|(key, value)| (key, redact_json_value(value)))
                .collect(),
        ),
        value => value,
    }
}

/// Read the last `max_lines` of a text log, redacting values and bounding the
/// returned payload. Missing files produce an empty result.
pub fn read_redacted_log_tail(path: &Path, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };

    let mut lines = VecDeque::with_capacity(max_lines);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if lines.len() == max_lines {
            lines.pop_front();
        }
        lines.push_back(shell::redact_secrets(&line));
    }

    let mut bytes = 0usize;
    let mut output = Vec::new();
    for line in lines.into_iter().rev() {
        let line_bytes = line.len().saturating_add(1);
        if bytes.saturating_add(line_bytes) > MAX_LOG_OUTPUT_BYTES {
            break;
        }
        bytes = bytes.saturating_add(line_bytes);
        output.push(line);
    }
    output.reverse();
    output
}

/// Split a tool entry name like `"search_text#0"` into `("search_text", "0")`.
///
/// If there is no `#`, the whole string is the name and the id defaults to `"?"`.
fn split_tool_name_id(name: &str) -> (String, String) {
    match name.rsplit_once('#') {
        Some((n, id)) => (n.to_string(), id.to_string()),
        None => (name.to_string(), "?".to_string()),
    }
}

fn mcp_tool_session_meta(name: &str) -> Option<McpToolSessionMeta> {
    let rest = name.strip_prefix("mcp__")?;
    let (server_name, original_tool_name) = rest.split_once("__")?;
    if server_name.is_empty() || original_tool_name.is_empty() {
        return None;
    }
    Some(McpToolSessionMeta {
        server_name: server_name.to_string(),
        original_tool_name: original_tool_name.to_string(),
        capability: "tool".to_string(),
        requested_authority: "external MCP server access with thndrs process permissions".to_string(),
    })
}

fn collect_handles(value: &serde_json::Value, key: Option<&str>, handles: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            if key == Some("artifact")
                && let Some(handle) = fields.get("handle").and_then(serde_json::Value::as_str)
            {
                handles.insert(handle.to_string());
            }
            for (field, value) in fields {
                collect_handles(value, Some(field), handles);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_handles(value, key, handles);
            }
        }
        serde_json::Value::String(handle) if key == Some("artifact_handle") => {
            handles.insert(handle.clone());
        }
        _ => {}
    }
}
