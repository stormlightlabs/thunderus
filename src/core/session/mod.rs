//! Append-only JSONL session persistence.
//!
//! Records session metadata, transcript events, tool audits, and context
//! control actions for audit and resume without storing full raw provider
//! payloads.
//!
//! Each [`SessionRecord`] is one append-only JSONL line tagged with
//! `schema_version`, a monotonic `seq`, `time`, and `type`.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::acp::permissions::PendingPermission;
use crate::app::{Entry, ToolStatus};
use crate::context::{ContextItem, ContextItemKind, ContextLedger, ContextSource, ContextVisibility};
use crate::memory::{MemoryDeleteRecord, MemoryDeletion, MemoryRootKind, MemoryScope, MemorySource, MemoryWrite};
use crate::prompt::{EnvironmentMetadata, HistoryReuse, PromptBundle};
use crate::skills::{SkillActivation, SkillReferenceMeta};
use crate::tools::{WriteOp, shell};
use crate::{datetime, internals, tools};

/// Current JSONL schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// A single line in a session JSONL file.
///
/// Every record carries `schema_version`, `seq` (monotonic within the session),
/// `time` (ISO 8601 UTC), and a `type` tag. Records are never rewritten;
/// appends are the only mutation.
/// Effective config metadata persisted in `session_meta` for audit and export.
///
/// Includes loaded config file paths with SHA-256 hashes, per-key origins,
/// and diagnostics. Never includes raw secret values.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigMeta {
    /// Effective session directory used for append-only JSONL files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<String>,
    /// Whether optional memory/retrieval was active for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_enabled: Option<bool>,
    /// Loaded config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SessionConfigFile>,
    /// Per-key origin labels (e.g. `"env:THNDRS_MODEL"`, `"project:.thndrs/config.toml"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub origins: BTreeMap<String, String>,
    /// Non-fatal config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Loaded MCP config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_files: Vec<SessionConfigFile>,
    /// Non-fatal MCP config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_diagnostics: Vec<String>,
}

/// A loaded config file recorded in session metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigFile {
    /// Display path: workspace-relative, `~`-relative, or absolute.
    pub path: String,
    /// Source label: `"global"` or `"project"`.
    pub source: String,
    /// Lowercase hex SHA-256 of file bytes.
    pub sha256: String,
}

/// MCP-specific metadata attached to external tool calls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpToolSessionMeta {
    /// Configured MCP server name.
    pub server_name: String,
    /// Original MCP tool name before provider-visible namespacing.
    pub original_tool_name: String,
}

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
    /// An explicit memory write, without the memory body or title.
    #[serde(rename = "memory_write")]
    MemoryWrite {
        schema_version: u32,
        seq: u64,
        time: String,
        memory: MemoryMutationMeta,
    },
    /// A completed memory deletion audit, without forgotten content.
    #[serde(rename = "memory_delete")]
    MemoryDelete {
        schema_version: u32,
        seq: u64,
        time: String,
        deletion: MemoryDeleteRecord,
    },
    /// A manual or automatic compaction audit record.
    #[serde(rename = "compaction")]
    Compaction {
        schema_version: u32,
        seq: u64,
        time: String,
        audit: CompactionAudit,
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
    /// A shell command execution completed.
    ///
    /// Records the command argv, working directory, lifecycle status, exit
    /// code, elapsed time, and process kind for session audit. stdout/stderr
    /// are not stored directly — they are captured in the `tool_finished`
    /// record's output lines (which are already redacted and capped).
    #[serde(rename = "shell_exec")]
    ShellExec {
        schema_version: u32,
        seq: u64,
        time: String,
        turn_id: String,
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
    /// Queued input recorded for audit. Resume does not rebuild pending queues.
    #[serde(rename = "queued_input")]
    QueuedInput {
        schema_version: u32,
        seq: u64,
        time: String,
        /// "steering" or "follow-up".
        kind: String,
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
            | SessionRecord::Context { seq, .. }
            | SessionRecord::ContextLedger { seq, .. }
            | SessionRecord::ContextPin { seq, .. }
            | SessionRecord::ContextDrop { seq, .. }
            | SessionRecord::ContextRecovery { seq, .. }
            | SessionRecord::MemoryWrite { seq, .. }
            | SessionRecord::MemoryDelete { seq, .. }
            | SessionRecord::Compaction { seq, .. }
            | SessionRecord::User { seq, .. }
            | SessionRecord::PromptMetadata { seq, .. }
            | SessionRecord::AssistantFinished { seq, .. }
            | SessionRecord::ReasoningFinished { seq, .. }
            | SessionRecord::Usage { seq, .. }
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
            Entry::Tool { name, arguments, status, output } if *status != ToolStatus::Running => {
                let (tool_name, call_id) = split_tool_name_id(name);
                Some(SessionRecord::ToolFinished {
                    schema_version: SCHEMA_VERSION,
                    seq,
                    time: time.to_string(),
                    turn_id: turn_id.to_string(),
                    call_id,
                    status: *status,
                    output: output.clone(),
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
            SessionRecord::ShellExec { command, process_status, elapsed_ms, .. } => {
                Some(Entry::Status { text: format!("shell {process_status}: {command} ({elapsed_ms}ms)") })
            }
            SessionRecord::SkillActivated { name, path, .. } => {
                Some(Entry::Status { text: format!("skill activated: {name} ({path})") })
            }
            SessionRecord::AcpPermissionRequest { tool_call_id, title, .. } => {
                Some(Entry::Status { text: format!("acp permission requested: {title} ({tool_call_id})") })
            }
            SessionRecord::AcpPermissionOutcome { tool_call_id, outcome, .. } => {
                Some(Entry::Status { text: format!("acp permission {tool_call_id}: {outcome}") })
            }
            _ => None,
        }
    }
}

/// Persisted ACP permission option metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpPermissionOptionRecord {
    /// ACP option id.
    pub id: String,
    /// Human-readable option label.
    pub name: String,
    /// Lowercase option kind.
    pub kind: String,
}

/// ACP session metadata persisted once per ACP run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpSessionMetadata {
    /// Configured ACP agent name.
    pub agent_name: String,
    /// Opaque external ACP session id returned by the agent.
    pub acp_session_id: String,
    /// Redacted command display used to start the agent.
    pub command: String,
    /// Selected ACP protocol version.
    pub protocol_version: String,
    /// Optional ACP agent info name.
    pub agent_info_name: Option<String>,
    /// Optional ACP agent info version.
    pub agent_info_version: Option<String>,
    /// Optional ACP client info name.
    pub client_info_name: Option<String>,
    /// Optional ACP client info version.
    pub client_info_version: Option<String>,
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
    /// Web search mode label.
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
    /// This captures the structural metadata of the turn — model, search mode,
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

/// Metadata for a loaded context source, without the content itself.
///
/// Records the path, scope, content hash, and truncation state so the
/// session can audit which AGENTS.md was loaded and whether it was capped.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextSourceMeta {
    /// Absolute path to the source file.
    pub path: String,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Stable hash of the full original content (before truncation).
    pub content_hash: u64,
    /// Whether the content was truncated to fit the size cap.
    pub truncated: bool,
    /// Original byte count of the file (before truncation).
    pub byte_count: usize,
}

impl From<&ContextSource> for ContextSourceMeta {
    fn from(source: &ContextSource) -> Self {
        ContextSourceMeta {
            path: source.path.display().to_string(),
            scope: source.scope.clone(),
            content_hash: source.content_hash,
            truncated: source.truncated,
            byte_count: source.byte_count,
        }
    }
}

impl ContextSourceMeta {
    /// Extract metadata from a [`ContextSource`], omitting the content.
    pub fn from_source(source: &ContextSource) -> Self {
        Self::from(source)
    }
}

/// Content-free metadata for one item in a context ledger.
///
/// This deliberately omits [`ContextItem::content`]. Paths, hashes, sizes,
/// visibility, and the selection reason are sufficient to explain why an item
/// was or was not visible without preserving repository or memory content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextItemMeta {
    /// Stable context item id.
    pub id: String,
    /// Domain kind used by context selection.
    pub kind: ContextItemKind,
    /// File-backed source path, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Context scope, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Hash of the source content, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<u64>,
    /// Source byte size.
    pub byte_count: usize,
    /// Conservative token estimate used by selection.
    pub token_estimate: usize,
    /// Inclusion state for this snapshot or action.
    pub visibility: ContextVisibility,
    /// Why selection assigned this visibility.
    pub reason: String,
}

impl From<&ContextItem> for ContextItemMeta {
    fn from(item: &ContextItem) -> Self {
        ContextItemMeta {
            id: item.id.clone(),
            kind: item.kind.clone(),
            source_path: item.source_path.as_ref().map(|path| path.display().to_string()),
            scope: Some(item.scope.clone()),
            content_hash: item.content_hash,
            byte_count: item.byte_count,
            token_estimate: item.token_estimate,
            visibility: item.visibility.clone(),
            reason: tools::shell::redact_secrets(&item.reason),
        }
    }
}

/// Content-free context ledger snapshot persisted for a prompt turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextLedgerMeta {
    /// All candidate and selected item metadata for this turn.
    pub items: Vec<ContextItemMeta>,
    /// Available input tokens after completion reservation and provider overhead.
    pub available_input: u64,
    /// Target token budget used for normal selection.
    pub target: u64,
    /// Token threshold that may trigger automatic compaction.
    pub auto_compaction_threshold: u64,
    /// Estimated tokens of rendered items.
    pub used: u64,
    /// Content-free diagnostic summaries emitted by context selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ContextDiagnosticMeta>,
}

impl From<&ContextLedger> for ContextLedgerMeta {
    fn from(ledger: &ContextLedger) -> Self {
        ContextLedgerMeta {
            items: ledger.items.iter().map(ContextItemMeta::from).collect(),
            available_input: ledger.budget.available_input,
            target: ledger.budget.target,
            auto_compaction_threshold: ledger.budget.auto_compaction_threshold,
            used: ledger.budget.used,
            diagnostics: ledger
                .diagnostics
                .iter()
                .map(|diagnostic| ContextDiagnosticMeta {
                    severity: diagnostic.severity.label().to_string(),
                    code: diagnostic.code.clone(),
                    message: tools::shell::redact_secrets(&diagnostic.message),
                })
                .collect(),
        }
    }
}

/// Content-free diagnostic emitted while selecting context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextDiagnosticMeta {
    /// Diagnostic severity label.
    pub severity: String,
    /// Stable diagnostic code.
    pub code: String,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Content-free audit metadata for an explicit memory write.
///
/// This intentionally does not carry a memory title, tags, paths, or body.
/// Those fields can contain user-supplied text; the id, file location, scope,
/// source, and content hash are sufficient to audit the mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MemoryMutationMeta {
    /// Stable memory id.
    pub id: String,
    /// File-backed memory path. `None` for session-scoped memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Memory lifetime and ownership scope.
    pub scope: MemoryScope,
    /// Root that owns the memory when it is file-backed.
    pub root: MemoryRootKind,
    /// Origin of the explicit write.
    pub source: MemorySource,
    /// Hash of the written memory content.
    pub content_hash: u64,
    /// Byte size of the rendered memory file or session body.
    pub byte_count: usize,
    /// Whether the memory body was capped in its returned item projection.
    pub truncated: bool,
}

impl From<&MemoryWrite> for MemoryMutationMeta {
    fn from(write: &MemoryWrite) -> Self {
        let item = &write.item;
        MemoryMutationMeta {
            id: item.id.clone(),
            path: (!item.path.as_os_str().is_empty()).then(|| item.path.display().to_string()),
            scope: item.scope,
            root: item.root,
            source: item.source,
            content_hash: item.content_hash,
            byte_count: item.byte_count,
            truncated: item.truncated,
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

/// Complete durable audit payload for one compaction.
///
/// The summary is intentionally retained because it becomes model-visible
/// working context. The covered source is represented only by ranges, stable
/// handles, and hashes; full transcript, file, memory, and provider payload
/// content never belongs in this record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactionAudit {
    /// Summary that replaces the covered range in the active working set.
    pub summary: String,
    /// Inclusive first session sequence replaced by this summary.
    pub covered_start_seq: u64,
    /// Inclusive final session sequence replaced by this summary.
    pub covered_end_seq: u64,
    /// Content hashes for covered sources, where known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_hashes: Vec<CompactionSourceHash>,
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
}

impl CompactionAudit {
    /// Return a copy with known secret-shaped fragments redacted.
    fn redacted(&self) -> Self {
        CompactionAudit {
            summary: tools::shell::redact_secrets(&self.summary),
            covered_start_seq: self.covered_start_seq,
            covered_end_seq: self.covered_end_seq,
            source_hashes: self.source_hashes.clone(),
            trigger: self.trigger,
            risk: self.risk,
            review: self.review,
            recovery_handles: self.recovery_handles.clone(),
            model: self.model.clone(),
            usage: self.usage,
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionWriter {
    path: PathBuf,
    seq: u64,
    session_id: String,
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

        std::fs::write(&path, format!("{}\n", record.to_json().map_err(io_err)?))?;

        Ok(SessionWriter { path, seq: 1, session_id: session_id.to_string() })
    }

    /// Sequence number that will be assigned to the next appended record.
    pub fn next_sequence(&self) -> u64 {
        self.seq
    }

    /// Reopen an existing session file for append-only continuation.
    ///
    /// The next sequence number is derived from the highest readable record
    /// sequence. Corrupt trailing lines are ignored consistently with
    /// [`SessionReader`].
    pub fn resume(path: &Path, session_id: &str) -> std::io::Result<Self> {
        let records = SessionReader::read_records(path);
        let seq = records
            .iter()
            .map(SessionRecord::seq)
            .max()
            .map_or(0, |max_seq| max_seq.saturating_add(1));

        let _file = std::fs::OpenOptions::new().append(true).open(path)?;

        Ok(SessionWriter { path: path.to_path_buf(), seq, session_id: session_id.to_string() })
    }

    /// Append a record to the session file.
    pub fn append(&mut self, mut record: SessionRecord) -> std::io::Result<()> {
        let seq = self.seq;
        self.seq += 1;
        set_seq(&mut record, seq);

        let line = record.to_json().map_err(io_err)?;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(&self.path)?;
        writeln!(file, "{line}")?;
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

    /// Append a `tool_started` record for a tool call that has begun.
    ///
    /// This records the command start: tool name, call id, and arguments.
    /// The matching `tool_finished` (via [`append_entry`]) records the
    /// output, status, and summary. For `run_shell`, an additional
    /// [`append_shell_exec`] record captures exit code, elapsed time, and
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

    /// Append a content-free audit record for an explicit memory write.
    pub fn append_memory_write(&mut self, write: &MemoryWrite) -> std::io::Result<()> {
        self.append(SessionRecord::MemoryWrite {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: datetime::now_iso8601(),
            memory: MemoryMutationMeta::from(write),
        })
    }

    /// Append a content-free audit record for a completed memory deletion.
    pub fn append_memory_delete(&mut self, deletion: &MemoryDeletion) -> std::io::Result<()> {
        let time = datetime::now_iso8601();
        self.append(SessionRecord::MemoryDelete {
            schema_version: SCHEMA_VERSION,
            seq: 0,
            time: time.clone(),
            deletion: deletion.to_audit_record(&time),
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
    /// Records the command argv, working directory, lifecycle status, exit
    /// code, elapsed time, and process kind. stdout/stderr are not stored
    /// here — they are captured in the `tool_finished` record's output lines
    /// (already redacted and capped).
    pub fn append_shell_exec(&mut self, turn_id: &str, result: &shell::ProcessResult) -> std::io::Result<()> {
        let record = SessionRecord::ShellExec {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            turn_id: turn_id.to_string(),
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
    pub fn append_queued(&mut self, kind: &str, text: &str) -> std::io::Result<()> {
        let record = SessionRecord::QueuedInput {
            schema_version: SCHEMA_VERSION,
            seq: self.seq,
            time: datetime::now_iso8601(),
            kind: kind.to_string(),
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

/// Reads a session JSONL file and reconstructs transcript entries.
///
/// Corrupt lines are skipped silently — the rest of the file is still
/// readable. This makes resume resilient to partial writes.
pub struct SessionReader;

impl SessionReader {
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
                _ => {}
            }
        }

        SessionSummary { title, model, input_tokens, output_tokens }
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
        mtime_b.cmp(&mtime_a)
    });
    files
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

/// Convert a serde_json error into an io::Error.
fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
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
    })
}

/// Set the seq field on a record (used by `SessionWriter::append`).
fn set_seq(record: &mut SessionRecord, seq: u64) {
    match record {
        SessionRecord::SessionMeta { seq: s, .. }
        | SessionRecord::Context { seq: s, .. }
        | SessionRecord::ContextLedger { seq: s, .. }
        | SessionRecord::ContextPin { seq: s, .. }
        | SessionRecord::ContextDrop { seq: s, .. }
        | SessionRecord::ContextRecovery { seq: s, .. }
        | SessionRecord::MemoryWrite { seq: s, .. }
        | SessionRecord::MemoryDelete { seq: s, .. }
        | SessionRecord::Compaction { seq: s, .. }
        | SessionRecord::User { seq: s, .. }
        | SessionRecord::PromptMetadata { seq: s, .. }
        | SessionRecord::AssistantFinished { seq: s, .. }
        | SessionRecord::ReasoningFinished { seq: s, .. }
        | SessionRecord::Usage { seq: s, .. }
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
