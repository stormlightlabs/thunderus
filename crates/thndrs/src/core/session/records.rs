//! Session record contracts and provider-neutral prompt metadata.

use super::*;

fn default_queue_action() -> String {
    String::from("add")
}

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
        #[serde(default, skip_serializing_if = "String::is_empty")]
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

    pub(super) fn record_time(&self) -> Option<String> {
        serde_json::to_value(self)
            .ok()?
            .get("time")?
            .as_str()
            .map(str::to_string)
    }

    pub(super) fn artifact_handles(&self) -> Vec<String> {
        let Ok(value) = serde_json::to_value(self) else {
            return Vec::new();
        };
        let mut handles = BTreeSet::new();
        collect_handles(&value, None, &mut handles);
        handles.into_iter().collect()
    }

    /// Set the seq field on a record (used by `SessionWriter::append`).
    pub(super) fn set_seq(&mut self, seq: u64) {
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
    /// This captures the structural metadata of the turn — model,
    /// context sources (hashes and truncation, not content), tool count, and
    /// transcript tail size. It does not store prompt text, AGENTS.md content,
    /// or provider request/response bodies.
    pub fn from_bundle(bundle: &PromptBundle) -> Self {
        let environment: &EnvironmentMetadata = &bundle.environment;
        let snapshot: internals::SelfKnowledgeSnapshot = bundle.into();
        PromptMetadata {
            model: environment.model.clone(),
            provider: snapshot.runtime.provider.provider,
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
    pub(super) fn redacted(&self) -> Self {
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
