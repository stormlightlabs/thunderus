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

/// Structured output returned by an application-owned tool executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolOutput {
    /// Tool name selected for execution.
    pub name: String,
    /// Execution status.
    pub status: ToolStatus,
    /// Output lines safe for display and provider feedback.
    pub output: Vec<String>,
    /// Failure detail when execution did not succeed.
    pub error: Option<String>,
}

impl ToolOutput {
    /// Build a successful output value.
    pub fn ok(name: impl Into<String>, output: Vec<String>) -> Self {
        Self { name: name.into(), status: ToolStatus::Ok, output, error: None }
    }

    /// Build a failed output value.
    pub fn failed(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self { name: name.into(), status: ToolStatus::Failed, output: Vec::new(), error: Some(error.into()) }
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
    ToolResult { id: String, output: ToolOutput },
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
    Usage { input_tokens: u64, output_tokens: u64 },
    /// Incremental assistant text.
    AssistantDelta(String),
    /// Incremental reasoning text.
    ReasoningDelta(String),
    /// A tool call was requested.
    ToolStarted {
        id: String,
        name: String,
        arguments: String,
    },
    /// A tool call completed.
    ToolFinished {
        id: String,
        output: Vec<String>,
        status: ToolStatus,
    },
    /// Model metadata available for application model pickers.
    ModelMetadataLoaded(Vec<(String, String)>),
    /// A retry was scheduled after a recoverable provider failure.
    Retrying {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: String,
    },
    /// The turn completed normally.
    Finished,
    /// The turn failed recoverably.
    Failed(String),
    /// The turn was cancelled cooperatively.
    Cancelled,
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
        assert_eq!(ToolOutput::ok("read", vec!["ok".to_string()]).status, ToolStatus::Ok);
        assert_eq!(ToolOutput::failed("read", "missing").error.as_deref(), Some("missing"));
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
