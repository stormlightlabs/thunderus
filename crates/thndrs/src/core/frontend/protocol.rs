//! Serializable contracts for frontend protocol version 1.

use serde::{Deserialize, Serialize};

use crate::app::{AgentEvent, App, Entry, RunState};

/// Current frontend protocol version.
pub const PROTOCOL_VERSION: u16 = 1;
const MAX_SNAPSHOT_ENTRIES: usize = 200;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_EVENT_TEXT_BYTES: usize = 64 * 1024;
const MAX_TOOL_OUTPUT_LINES: usize = 200;

/// One versioned request from a frontend.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct CommandEnvelope {
    /// Wire schema version used to encode this command.
    pub version: u16,
    /// Stable caller-supplied request identifier.
    pub id: String,
    /// Requested semantic application operation.
    #[serde(flatten)]
    pub command: Command,
}

/// Frontend commands recognized by protocol version 1.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Negotiate a version and receive the initial snapshot.
    Initialize { supported_versions: Vec<u16> },
    /// Request a fresh bounded snapshot.
    #[serde(rename = "state.snapshot")]
    StateSnapshot,
    /// Start a user turn through the shared application lifecycle.
    #[serde(rename = "turn.submit")]
    TurnSubmit { text: String },
    /// Cooperatively cancel the active turn.
    #[serde(rename = "turn.cancel")]
    TurnCancel,
    /// Queue input for a later milestone's interaction surface.
    #[serde(rename = "queue.submit")]
    QueueSubmit { text: String, target: String },
    /// Delete queued input.
    #[serde(rename = "queue.delete")]
    QueueDelete { id: String },
    /// Resolve a pending permission.
    #[serde(rename = "permission.respond")]
    PermissionRespond {
        tool_call_id: String,
        option_id: Option<String>,
    },
    /// Start a new session.
    #[serde(rename = "session.new")]
    SessionNew,
    /// Load a persisted session.
    #[serde(rename = "session.load")]
    SessionLoad { session_id: String },
    /// Close the current session.
    #[serde(rename = "session.close")]
    SessionClose,
    /// Select a provider/model route.
    #[serde(rename = "model.select")]
    ModelSelect { model: String },
    /// Select normalized reasoning effort.
    #[serde(rename = "reasoning.select")]
    ReasoningSelect { effort: String },
    /// Settle active work and terminate the bridge.
    Shutdown,
}

/// Stable machine-readable response failure category.
#[derive(Clone, Copy, Debug, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    MalformedCommand,
    InvalidRequest,
    NotInitialized,
    AlreadyInitialized,
    UnsupportedVersion,
    UnsupportedCommand,
    SetupRequired,
    Busy,
    NoActiveRun,
}

/// Successful direct command result.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseResult {
    Initialized {
        protocol_version: u16,
        snapshot: FrontendSnapshot,
    },
    Snapshot {
        snapshot: FrontendSnapshot,
    },
    Accepted,
    Shutdown,
}

/// One protocol line written by the Rust bridge.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolMessage {
    Response {
        version: u16,
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<ResponseResult>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ResponseError>,
    },
    Event {
        version: u16,
        sequence: u64,
        event: FrontendEvent,
    },
    ProtocolError {
        version: u16,
        error: ResponseError,
    },
}

impl ProtocolMessage {
    pub(super) fn response_ok(id: String, result: ResponseResult) -> Self {
        Self::Response { version: PROTOCOL_VERSION, id, ok: true, result: Some(result), error: None }
    }

    pub(super) fn response_error(id: String, code: ErrorCode, message: String) -> Self {
        Self::Response {
            version: PROTOCOL_VERSION,
            id,
            ok: false,
            result: None,
            error: Some(ResponseError { code, message }),
        }
    }

    pub(super) fn event(sequence: u64, event: FrontendEvent) -> Self {
        Self::Event { version: PROTOCOL_VERSION, sequence, event }
    }

    pub(super) fn protocol_error(code: ErrorCode, message: String) -> Self {
        Self::ProtocolError { version: PROTOCOL_VERSION, error: ResponseError { code, message } }
    }
}

/// Human-readable detail paired with a stable error code.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ResponseError {
    pub code: ErrorCode,
    pub message: String,
}

/// Bounded frontend-visible application state.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct FrontendSnapshot {
    /// Sequence of the latest event reflected by this snapshot.
    pub event_sequence: u64,
    pub session: FrontendSession,
    pub workspace: String,
    pub model: String,
    pub reasoning_effort: String,
    pub run: FrontendRunState,
    pub transcript: Vec<TranscriptItem>,
    pub queue: Vec<QueueItem>,
    pub usage: UsageSummary,
    pub truncated: bool,
}

impl FrontendSnapshot {
    pub(super) fn from_app(app: &App, event_sequence: u64) -> Self {
        let entry_count = app.transcript.entries.len();
        let skipped = entry_count.saturating_sub(MAX_SNAPSHOT_ENTRIES);
        let transcript = app
            .transcript
            .entries
            .iter()
            .enumerate()
            .skip(skipped)
            .map(|(index, entry)| TranscriptItem::from_entry(index, entry))
            .collect();
        let queue = app
            .composer
            .queue
            .items
            .iter()
            .map(|item| QueueItem {
                id: item.id.to_string(),
                target: item.target.label().to_string(),
                text: bounded_secret_text(&item.text, MAX_TEXT_BYTES),
                settlement: item.settlement.label().to_string(),
            })
            .collect();
        Self {
            event_sequence,
            session: FrontendSession {
                id: app.session.id.clone(),
                ephemeral: app.is_ephemeral(),
                turn_count: app.session.turn_count,
            },
            workspace: app.runtime.cwd.display().to_string(),
            model: app.runtime.model.clone(),
            reasoning_effort: app.runtime.cli.reasoning_effort.label().to_string(),
            run: FrontendRunState::from(&app.runtime.run_state),
            transcript,
            queue,
            usage: UsageSummary {
                input_tokens: app.runtime.session_tokens_in,
                output_tokens: app.runtime.session_tokens_out,
            },
            truncated: skipped > 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct FrontendSession {
    pub id: String,
    pub ephemeral: bool,
    pub turn_count: u64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FrontendRunState {
    Idle,
    Working,
    Stopping,
    Error { message: String },
}

impl From<&RunState> for FrontendRunState {
    fn from(value: &RunState) -> Self {
        match value {
            RunState::Idle => Self::Idle,
            RunState::Working => Self::Working,
            RunState::Stopping => Self::Stopping,
            RunState::Error(message) => Self::Error { message: bounded_diagnostic(message) },
        }
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub target: String,
    pub text: String,
    pub settlement: String,
}

/// Semantic transcript content; provider continuation data is never included.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptItem {
    User {
        id: String,
        text: String,
    },
    Assistant {
        id: String,
        text: String,
        streaming: bool,
    },
    Reasoning {
        id: String,
        text: String,
        streaming: bool,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
        status: String,
        output: Vec<String>,
    },
    Skill {
        id: String,
        name: String,
        path: String,
    },
    Status {
        id: String,
        text: String,
    },
    Error {
        id: String,
        text: String,
    },
}

impl TranscriptItem {
    fn from_entry(index: usize, entry: &Entry) -> Self {
        let id = format!("entry-{index}");
        match entry {
            Entry::User { text } => Self::User { id, text: bounded_secret_text(text, MAX_TEXT_BYTES) },
            Entry::Agent { text, streaming } => {
                Self::Assistant { id, text: bounded_text(text, MAX_TEXT_BYTES), streaming: *streaming }
            }
            Entry::Reasoning { text, streaming } => {
                Self::Reasoning { id, text: bounded_text(text, MAX_TEXT_BYTES), streaming: *streaming }
            }
            Entry::Tool { name, arguments, status, output } => Self::Tool {
                id,
                name: bounded_diagnostic(name),
                arguments: bounded_secret_text(arguments, MAX_TEXT_BYTES),
                status: status.label().to_string(),
                output: bounded_lines(output),
            },
            Entry::Skill { name, path, .. } => {
                Self::Skill { id, name: bounded_diagnostic(name), path: bounded_diagnostic(path) }
            }
            Entry::Status { text } => Self::Status { id, text: bounded_diagnostic(text) },
            Entry::Error { text } => Self::Error { id, text: bounded_diagnostic(text) },
        }
    }
}

/// Provider-neutral asynchronous event vocabulary.
#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum FrontendEvent {
    #[serde(rename = "run.started")]
    RunStarted,
    #[serde(rename = "run.finished")]
    RunFinished,
    #[serde(rename = "run.cancelled")]
    RunCancelled,
    #[serde(rename = "run.failed")]
    RunFailed { message: String },
    #[serde(rename = "assistant.delta")]
    AssistantDelta { text: String },
    #[serde(rename = "reasoning.delta")]
    ReasoningDelta { text: String },
    #[serde(rename = "tool.started")]
    ToolStarted {
        id: String,
        name: String,
        arguments: String,
    },
    #[serde(rename = "tool.finished")]
    ToolFinished {
        id: String,
        status: String,
        output: Vec<String>,
    },
    #[serde(rename = "usage.updated")]
    UsageUpdated { input_tokens: u64, output_tokens: u64 },
    #[serde(rename = "permission.requested")]
    PermissionRequested {
        tool_call_id: String,
        title: String,
        selected: usize,
        options: Vec<PermissionOption>,
    },
    #[serde(rename = "permission.resolved")]
    PermissionResolved { tool_call_id: String, outcome: String },
    #[serde(rename = "status.updated")]
    StatusUpdated { message: String },
    #[serde(rename = "model.updated")]
    ModelUpdated { options: Vec<ModelOption> },
}

impl FrontendEvent {
    pub(super) fn from_agent_event(event: &AgentEvent) -> Option<Self> {
        match event {
            AgentEvent::Started => Some(Self::RunStarted),
            AgentEvent::Status(message) => Some(Self::StatusUpdated { message: bounded_diagnostic(message) }),
            AgentEvent::Usage { input_tokens, output_tokens } => {
                Some(Self::UsageUpdated { input_tokens: *input_tokens, output_tokens: *output_tokens })
            }
            AgentEvent::CodexUsage(_) | AgentEvent::RequestStarted(_) | AgentEvent::StateProjectionDecision { .. } => {
                None
            }
            AgentEvent::RequestAccounting(accounting) => Some(Self::UsageUpdated {
                input_tokens: accounting
                    .provider_usage
                    .as_ref()
                    .and_then(|usage| usage.components.input_tokens)
                    .unwrap_or_default(),
                output_tokens: accounting
                    .provider_usage
                    .as_ref()
                    .and_then(|usage| usage.components.output_tokens)
                    .unwrap_or_default(),
            }),
            AgentEvent::AssistantDelta(text) => {
                Some(Self::AssistantDelta { text: bounded_text(text, MAX_EVENT_TEXT_BYTES) })
            }
            AgentEvent::ReasoningDelta(text) => {
                Some(Self::ReasoningDelta { text: bounded_text(text, MAX_EVENT_TEXT_BYTES) })
            }
            AgentEvent::ToolStarted { id, name, arguments } => Some(Self::ToolStarted {
                id: bounded_diagnostic(id),
                name: bounded_diagnostic(name),
                arguments: bounded_secret_text(arguments, MAX_EVENT_TEXT_BYTES),
            }),
            AgentEvent::ToolFinished { id, output, status, .. } => Some(Self::ToolFinished {
                id: bounded_diagnostic(id),
                status: status.label().to_string(),
                output: bounded_lines(output),
            }),
            AgentEvent::ModelMetadataLoaded(options) => Some(Self::ModelUpdated {
                options: options
                    .iter()
                    .take(200)
                    .map(|(label, detail)| ModelOption {
                        label: bounded_diagnostic(label),
                        detail: bounded_diagnostic(detail),
                    })
                    .collect(),
            }),
            AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => Some(Self::StatusUpdated {
                message: format!(
                    "retry {attempt}/{max_attempts} in {delay_ms}ms: {}",
                    bounded_diagnostic(error)
                ),
            }),
            AgentEvent::PermissionRequest(permission) => Some(Self::PermissionRequested {
                tool_call_id: bounded_diagnostic(&permission.tool_call_id),
                title: bounded_diagnostic(&permission.title),
                selected: permission.selected,
                options: permission
                    .options
                    .iter()
                    .take(20)
                    .map(|option| PermissionOption {
                        id: bounded_diagnostic(&option.id),
                        name: bounded_diagnostic(&option.name),
                        kind: option.kind.label().to_string(),
                    })
                    .collect(),
            }),
            AgentEvent::PermissionResolved { tool_call_id, outcome } => Some(Self::PermissionResolved {
                tool_call_id: bounded_diagnostic(tool_call_id),
                outcome: bounded_diagnostic(outcome),
            }),
            AgentEvent::AcpSession(_) => None,
            AgentEvent::Finished => Some(Self::RunFinished),
            AgentEvent::Failed(message) => Some(Self::RunFailed { message: bounded_diagnostic(message) }),
            AgentEvent::Cancelled => Some(Self::RunCancelled),
        }
    }
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct ModelOption {
    pub label: String,
    pub detail: String,
}

fn bounded_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .take(MAX_TOOL_OUTPUT_LINES)
        .map(|line| bounded_secret_text(line, MAX_TEXT_BYTES))
        .collect()
}

fn bounded_diagnostic(text: &str) -> String {
    bounded_secret_text(text, 512)
}

fn bounded_secret_text(text: &str, max_bytes: usize) -> String {
    bounded_text(&crate::tools::shell::redact_secrets(text), max_bytes)
}

fn bounded_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", &text[..end])
}
