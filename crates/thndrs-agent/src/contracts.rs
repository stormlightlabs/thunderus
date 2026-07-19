//! Provider-neutral run contracts shared by application adapters.

use std::borrow::Cow;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Status of an executed tool call.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool started, not yet finished.
    #[default]
    Running,
    /// Tool finished successfully.
    Ok,
    /// Tool failed.
    Failed,
    /// Tool was cancelled while running.
    Cancelled,
}

impl ToolStatus {
    /// Compact session/transcript label for a file-write result.
    pub const fn icon(self) -> &'static str {
        match self {
            ToolStatus::Ok => "✓ wrote",
            ToolStatus::Failed => "✕ write failed",
            ToolStatus::Running => "⠋ writing",
            ToolStatus::Cancelled => "✕ write cancelled",
        }
    }

    /// Stable lowercase status label.
    pub const fn label(self) -> &'static str {
        match self {
            ToolStatus::Running => "running",
            ToolStatus::Ok => "ok",
            ToolStatus::Failed => "failed",
            ToolStatus::Cancelled => "cancelled",
        }
    }
}

/// The kind of bounded evidence associated with a tool execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ToolEvidenceKind {
    /// The normal result produced by a tool execution.
    Output,
}

/// Provider-neutral message supplied to or produced by an agent turn.
///
/// Provider adapters lower this representation to their wire payloads inside
/// the application. The shared contract never exposes those payloads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentMessage {
    /// System or harness guidance.
    System(String),
    /// User input for the current or an earlier turn.
    User(String),
    /// Completed assistant text.
    Assistant(String),
    /// An assistant tool-use request.
    ToolUse(ToolUseRequest),
    /// A completed application-owned tool result.
    ToolResult {
        /// Provider or application id that correlates the result with a tool request.
        id: String,
        /// Application-owned result returned by the tool executor.
        output: ToolOutput,
    },
}

/// Decision returned by an application-owned tool permission hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolPermissionDecision {
    /// The tool call may execute.
    Allow,
    /// The tool call must be rejected before execution.
    Reject,
    /// The prompt turn was cancelled while waiting for permission.
    Cancelled,
}

impl ToolPermissionDecision {
    /// Stable outcome label for session records.
    pub const fn outcome_label(self) -> &'static str {
        match self {
            Self::Allow => "allowed",
            Self::Reject => "rejected",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Provider-neutral semantic output from an agent turn.
///
/// Application adapters may attach local-tool audit details, UI state, or
/// transport state when projecting these events to their own surfaces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    /// A turn started.
    Started,
    /// Informational provider or run status.
    Status(String),
    /// Incremental token accounting.
    Usage {
        /// Number of input tokens observed for the request or increment.
        input_tokens: u64,
        /// Number of output tokens observed for the request or increment.
        output_tokens: u64,
    },
    /// Incremental assistant text.
    AssistantDelta(String),
    /// Incremental reasoning text.
    ReasoningDelta(String),
    /// A tool call was requested.
    ToolStarted {
        /// Provider-assigned id for the tool call.
        id: String,
        /// Application catalog name selected for the call.
        name: String,
        /// Raw JSON arguments supplied for the call.
        arguments: String,
    },
    /// A tool call completed.
    ToolFinished {
        /// Provider-assigned id for the tool call.
        id: String,
        /// Output lines returned by the application-owned executor.
        output: Vec<String>,
        /// Final execution status.
        status: ToolStatus,
    },
    /// Model metadata available for application model pickers.
    ModelMetadataLoaded(Vec<(String, String)>),
    /// A retry was scheduled after a recoverable provider failure.
    Retrying {
        /// One-based retry attempt that will run next.
        attempt: u32,
        /// Maximum retry attempts configured for the request.
        max_attempts: u32,
        /// Delay before the next attempt, in milliseconds.
        delay_ms: u64,
        /// Redacted, provider-neutral failure detail.
        error: String,
    },
    /// The turn completed normally.
    Finished,
    /// The turn failed recoverably.
    Failed(String),
    /// The turn was cancelled cooperatively.
    Cancelled,
}

/// A tool-use request from a provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolUseRequest {
    /// Tool name chosen from the application-supplied catalog.
    pub name: String,
    /// Raw JSON arguments supplied by the provider.
    pub arguments: String,
    /// Provider-assigned id used to correlate the eventual result.
    pub tool_use_id: String,
}

impl ToolUseRequest {
    /// Build a tool-use request from its provider-neutral fields.
    pub fn new(name: impl Into<String>, arguments: impl Into<String>, tool_use_id: impl Into<String>) -> Self {
        Self { name: name.into(), arguments: arguments.into(), tool_use_id: tool_use_id.into() }
    }
}

/// Bounded, provider-neutral metadata about durable tool evidence.
///
/// `byte_count` describes the UTF-8, newline-joined compatibility rendering
/// when constructed by [`ToolOutput::ok`] or [`ToolOutput::failed`]. Custom
/// executors may provide their own exact measurement. `artifact_handle` is an
/// application-owned opaque reference and never contains the evidence body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolEvidenceMetadata {
    /// Stable identity for this evidence within the owning application.
    pub identity: String,
    /// Coarse evidence classification.
    pub kind: ToolEvidenceKind,
    /// Exact UTF-8 byte count at the application's declared boundary.
    pub byte_count: usize,
    /// Optional application-computed content hash.
    pub content_hash: Option<String>,
    /// Optional opaque handle for bounded redacted recovery.
    pub artifact_handle: Option<String>,
}

impl ToolEvidenceMetadata {
    /// Build compatibility metadata for newline-joined text lines.
    pub fn for_lines(identity: impl Into<String>, lines: &[String]) -> Self {
        Self {
            identity: identity.into(),
            kind: ToolEvidenceKind::Output,
            byte_count: lines.join("\n").len(),
            content_hash: None,
            artifact_handle: None,
        }
    }
}

/// User-facing bounded projection of a tool result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolDisplayProjection {
    /// Lines rendered by the CLI, TUI, or ACP adapter.
    pub lines: Vec<String>,
}

impl ToolDisplayProjection {
    /// Build a display projection from already bounded lines.
    pub fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

/// Model-facing bounded projection of a tool result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolModelProjection {
    /// Lines lowered into the next provider request.
    pub lines: Vec<String>,
}

impl ToolModelProjection {
    /// Build a model projection from already bounded lines.
    pub fn new(lines: Vec<String>) -> Self {
        Self { lines }
    }
}

/// Structured output returned by an application-owned tool executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolOutput {
    /// Tool name selected for execution.
    pub name: String,
    /// Execution status.
    pub status: ToolStatus,
    /// Metadata for bounded redacted evidence retained by the application.
    pub evidence: ToolEvidenceMetadata,
    /// User-facing projection. This is the source for UI and ACP surfaces.
    pub display: ToolDisplayProjection,
    /// Model-facing projection. This is the source for provider feedback.
    pub model: ToolModelProjection,
    /// Failure detail when execution did not succeed.
    pub error: Option<String>,
}

impl ToolOutput {
    /// Build a successful output value.
    pub fn ok(name: impl Into<String>, output: Vec<String>) -> Self {
        let name = name.into();
        Self {
            evidence: ToolEvidenceMetadata::for_lines(&name, &output),
            name,
            status: ToolStatus::Ok,
            display: ToolDisplayProjection::new(output.clone()),
            model: ToolModelProjection::new(output),
            error: None,
        }
    }

    /// Build a failed output value.
    pub fn failed(name: impl Into<String>, error: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            evidence: ToolEvidenceMetadata::for_lines(&name, &[]),
            name,
            status: ToolStatus::Failed,
            display: ToolDisplayProjection::new(Vec::new()),
            model: ToolModelProjection::new(Vec::new()),
            error: Some(error.into()),
        }
    }

    /// Attach an application-computed content hash for state-aware identity.
    ///
    /// The hash describes application evidence rather than provider-visible
    /// content. Applications use it with a tool-specific source key when they
    /// need to prove that two bounded projections observed the same state.
    pub fn with_evidence_content_hash(mut self, content_hash: impl Into<String>) -> Self {
        self.evidence.content_hash = Some(content_hash.into());
        self
    }

    /// Return display lines, materializing the structured failure detail.
    pub fn display_lines(&self) -> Vec<String> {
        projection_lines(&self.display.lines, self.error.as_deref())
    }

    /// Return model lines, materializing the structured failure detail.
    pub fn model_lines(&self) -> Vec<String> {
        projection_lines(&self.model.lines, self.error.as_deref())
    }
}

/// A tool definition exposed to a provider/model.
#[derive(Clone, Debug)]
pub struct ToolDefinition {
    /// Stable name selected by the provider in a tool-use request.
    pub name: Cow<'static, str>,
    /// Model-visible guidance for using the tool.
    pub description: Cow<'static, str>,
    /// JSON Schema for the tool arguments.
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Build a provider-visible tool definition.
    pub fn new(
        name: impl Into<Cow<'static, str>>, description: impl Into<Cow<'static, str>>, input_schema: serde_json::Value,
    ) -> Self {
        Self { name: name.into(), description: description.into(), input_schema }
    }
}

/// Provider-neutral input for one agent turn.
///
/// Applications assemble messages and tool definitions, then their provider
/// adapter performs the wire-level request. The turn contract remains useful
/// to deterministic fakes and future application adapters without bringing
/// provider protocol types into this crate.
#[derive(Clone, Debug)]
pub struct AgentTurn {
    /// Ordered conversation and tool-result messages.
    pub messages: Vec<AgentMessage>,
    /// Tool definitions available for this turn.
    pub tools: Vec<ToolDefinition>,
}

impl AgentTurn {
    /// Create a turn from application-owned messages and tool definitions.
    pub fn new(messages: Vec<AgentMessage>, tools: Vec<ToolDefinition>) -> Self {
        Self { messages, tools }
    }
}

/// Bounded exponential retry policy for provider requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Number of retry attempts after the initial request.
    pub max_retries: u32,
    /// Delay before the first retry.
    pub base_delay: Duration,
}

impl RetryPolicy {
    /// Build a retry policy with the supplied attempt limit and initial delay.
    pub const fn new(max_retries: u32, base_delay: Duration) -> Self {
        Self { max_retries, base_delay }
    }

    /// Return the exponential-backoff delay for a one-based retry attempt.
    pub fn delay_for_attempt(self, attempt: u32) -> Duration {
        self.base_delay * 2u32.saturating_pow(attempt.saturating_sub(1))
    }
}

fn projection_lines(lines: &[String], error: Option<&str>) -> Vec<String> {
    let mut lines = lines.to_vec();
    let Some(error) = error.map(str::trim).filter(|error| !error.is_empty()) else {
        return lines;
    };
    let error_line = format!("error: {error}");
    if !lines.iter().any(|line| line == error || line == &error_line) {
        lines.insert(0, error_line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_outcomes_have_stable_labels() {
        assert_eq!(ToolPermissionDecision::Allow.outcome_label(), "allowed");
        assert_eq!(ToolPermissionDecision::Reject.outcome_label(), "rejected");
        assert_eq!(ToolPermissionDecision::Cancelled.outcome_label(), "cancelled");
    }

    #[test]
    fn retry_delays_double_per_attempt() {
        let policy = RetryPolicy::new(3, Duration::from_millis(25));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(25));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(100));
    }

    #[test]
    fn tool_output_preserves_success_and_failure_states() {
        let success = ToolOutput::ok("read", vec!["ok".to_string()]);
        assert_eq!(success.status, ToolStatus::Ok);
        assert_eq!(success.display.lines, vec!["ok"]);
        assert_eq!(success.model.lines, vec!["ok"]);
        assert_eq!(success.evidence.byte_count, 2);

        let failure = ToolOutput::failed("read", "missing");
        assert_eq!(failure.error.as_deref(), Some("missing"));
        assert_eq!(failure.display_lines(), vec!["error: missing"]);
        assert_eq!(failure.model_lines(), vec!["error: missing"]);
    }

    #[test]
    fn display_and_model_projections_can_diverge_without_raw_output() {
        let mut output = ToolOutput::ok("read", vec!["full result".to_string()]);
        output.display.lines = vec!["shown to user".to_string()];
        output.model.lines = vec!["sent to model".to_string()];

        assert_eq!(output.display.lines, vec!["shown to user"]);
        assert_eq!(output.model.lines, vec!["sent to model"]);
        assert_eq!(output.evidence.artifact_handle, None);
    }

    #[test]
    fn turn_keeps_provider_neutral_messages_and_tools_together() {
        let request = ToolUseRequest::new("read_file", r#"{"path":"README.md"}"#, "call_1");
        let turn = AgentTurn::new(
            vec![
                AgentMessage::User("inspect the readme".to_string()),
                AgentMessage::ToolUse(request),
            ],
            vec![ToolDefinition::new(
                "read_file",
                "Read a file",
                serde_json::json!({"type": "object"}),
            )],
        );

        assert_eq!(turn.messages.len(), 2);
        assert_eq!(turn.tools[0].name, "read_file");
    }
}
