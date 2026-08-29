//! Serializable contracts for frontend protocol version 1.

use serde::{Deserialize, Serialize};

use crate::app::{AgentEvent, App, Entry, RunState};
use crate::cli::ReasoningEffort;

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
        result: Option<Box<ResponseResult>>,
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
        Self::Response { version: PROTOCOL_VERSION, id, ok: true, result: Some(Box::new(result)), error: None }
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
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FrontendSnapshot {
    /// Sequence of the latest event reflected by this snapshot.
    pub event_sequence: u64,
    pub session: FrontendSession,
    pub sessions: Vec<FrontendSessionOption>,
    pub workspace: String,
    pub model: String,
    pub reasoning_effort: String,
    pub run: FrontendRunState,
    pub transcript: Vec<TranscriptItem>,
    pub queue: Vec<QueueItem>,
    pub usage: UsageSummary,
    pub context: ContextSummary,
    pub pending_permission: Option<PendingPermission>,
    pub capabilities: FrontendCapabilities,
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
            sessions: session_options(app),
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
            context: ContextSummary::from_app(app),
            pending_permission: app.overlay.permission().map(PendingPermission::from),
            capabilities: FrontendCapabilities::from_app(app),
            truncated: skipped > 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FrontendSession {
    pub id: String,
    pub ephemeral: bool,
    pub turn_count: u64,
}

/// Rust-owned metadata needed to choose a persisted session.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FrontendSessionOption {
    pub id: String,
    pub title: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub current: bool,
}

fn session_options(app: &App) -> Vec<FrontendSessionOption> {
    if app.is_ephemeral() {
        return Vec::new();
    }
    crate::session::list_session_files(&app.session_directory())
        .into_iter()
        .filter_map(|path| {
            let id = path.file_stem()?.to_str()?.to_string();
            let summary = crate::session::SessionReader::read_summary(&path);
            Some(FrontendSessionOption {
                current: id == app.session.id,
                id,
                title: summary.title,
                model: summary.model,
                input_tokens: summary.input_tokens,
                output_tokens: summary.output_tokens,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
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

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Provider-neutral context-window state for compact display and inspection.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ContextSummary {
    pub used_tokens: u64,
    pub context_window: u64,
    pub available_input: u64,
    pub target_tokens: u64,
    pub auto_compaction_threshold: u64,
    pub compaction_state: String,
    pub limit_source: String,
}

impl ContextSummary {
    pub(super) fn from_app(app: &App) -> Self {
        let fallback;
        let budget = if let Some(ledger) = app.transcript.context_ledger.as_ref() {
            &ledger.budget
        } else {
            let provider = crate::acp::config::provider_label(&app.runtime.model);
            let (limits, _) =
                thndrs_agent::context::ModelContextLimits::resolve(provider, &app.runtime.model, None, None);
            fallback = thndrs_agent::context::ContextBudget::from_limits(limits, &[]);
            &fallback
        };
        let compaction_state = if app.transcript.pending_compaction_review.is_some() {
            "review"
        } else if app.compaction_in_flight() {
            "compacting"
        } else if budget.exceeds_auto_compaction() {
            "pressure"
        } else {
            "idle"
        };
        Self {
            used_tokens: budget.used,
            context_window: budget.limits.context_window,
            available_input: budget.available_input,
            target_tokens: budget.target,
            auto_compaction_threshold: app
                .effective_compaction_policy()
                .auto_compaction_threshold(budget.available_input),
            compaction_state: compaction_state.to_string(),
            limit_source: budget.limits.source.label().to_string(),
        }
    }
}

/// One pending backend-owned permission decision.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PendingPermission {
    pub tool_call_id: String,
    pub title: String,
    pub selected: usize,
    pub options: Vec<PermissionOption>,
}

impl From<&crate::acp::permissions::PendingPermission> for PendingPermission {
    fn from(permission: &crate::acp::permissions::PendingPermission) -> Self {
        Self {
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
        }
    }
}

/// Commands supported by the bridge and provider-specific selectable values.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct FrontendCapabilities {
    pub commands: Vec<String>,
    pub models: Vec<ModelOption>,
    pub reasoning_efforts: Vec<ReasoningOption>,
}

impl FrontendCapabilities {
    pub(super) fn from_app(app: &App) -> Self {
        let mut models: Vec<ModelOption> = crate::providers::opencode::known_models()
            .into_iter()
            .chain(crate::providers::codex::known_models())
            .map(|model| ModelOption { label: model.id.to_string(), detail: model.description.to_string() })
            .collect();
        models.extend(
            app.runtime
                .model_picker_items
                .iter()
                .map(|item| ModelOption { label: item.label.clone(), detail: item.detail.clone() }),
        );
        if !models.iter().any(|option| option.label == app.runtime.model) {
            models.insert(
                0,
                ModelOption { label: app.runtime.model.clone(), detail: "Current model".to_string() },
            );
        }
        models.sort_by(|left, right| left.label.cmp(&right.label));
        models.dedup_by(|left, right| left.label == right.label);
        let mut commands = vec![
            "state.snapshot".to_string(),
            "turn.submit".to_string(),
            "turn.cancel".to_string(),
            "queue.submit".to_string(),
            "queue.delete".to_string(),
            "session.new".to_string(),
            "session.close".to_string(),
            "permission.respond".to_string(),
            "model.select".to_string(),
            "reasoning.select".to_string(),
            "shutdown".to_string(),
        ];
        if !app.is_ephemeral() {
            commands.push("session.load".to_string());
        }
        Self { commands, models, reasoning_efforts: reasoning_options(&app.runtime.model) }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReasoningOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

fn reasoning_options(model: &str) -> Vec<ReasoningOption> {
    crate::providers::reasoning_options(model)
        .into_iter()
        .map(|effort| ReasoningOption {
            value: effort.label().to_string(),
            label: effort.display_label().to_string(),
            description: effort.description().to_string(),
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct QueueItem {
    pub id: String,
    pub target: String,
    pub text: String,
    pub settlement: String,
}

/// Semantic transcript content; provider continuation data is never included.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
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
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
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
    #[serde(rename = "context.updated")]
    ContextUpdated { context: ContextSummary },
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
    ModelUpdated {
        model: String,
        options: Vec<ModelOption>,
        reasoning_efforts: Vec<ReasoningOption>,
    },
    #[serde(rename = "reasoning.updated")]
    ReasoningUpdated { effort: String },
    /// Authoritative replacement after queue or session state changes.
    #[serde(rename = "snapshot.updated")]
    SnapshotUpdated { snapshot: Box<FrontendSnapshot> },
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
                model: String::new(),
                options: options
                    .iter()
                    .take(200)
                    .map(|(label, detail)| ModelOption {
                        label: bounded_diagnostic(label),
                        detail: bounded_diagnostic(detail),
                    })
                    .collect(),
                reasoning_efforts: Vec::new(),
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

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ModelOption {
    pub label: String,
    pub detail: String,
}

pub(super) fn model_updated(app: &App) -> FrontendEvent {
    let capabilities = FrontendCapabilities::from_app(app);
    FrontendEvent::ModelUpdated {
        model: app.runtime.model.clone(),
        options: capabilities.models,
        reasoning_efforts: capabilities.reasoning_efforts,
    }
}

pub(super) fn reasoning_updated(app: &App) -> FrontendEvent {
    FrontendEvent::ReasoningUpdated { effort: app.runtime.cli.reasoning_effort.label().to_string() }
}

pub(super) fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffort> {
    ReasoningEffort::parse(value)
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
