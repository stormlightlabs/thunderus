//! Application state, message types, and event dispatch (The Elm architecture/TEA).
//!
//! `App` holds the mutable session and prompt state. `Msg` represents input,
//! provider, tool, permission, and lifecycle events. [`update`] applies one
//! message and returns pure follow-up messages plus bounded effects for the
//! application adapter to execute.
//!
//! The root module declares the shared state and message vocabulary. The child
//! modules implement the event families that [`update`] dispatches:
//!
//! - `onboarding` handles provider setup, credential recovery, and OAuth.
//! - `input` handles editing, history, pickers, and prompt submission.
//! - `commands` handles slash-command parsing and command actions.
//! - `context` handles context inspection and compaction operations.
//! - `agent_lifecycle` handles agent events and session persistence.

mod agent_lifecycle;
mod commands;
mod context;
mod input;
mod onboarding;
mod transcript_blocks;
#[cfg(test)]
pub use agent_lifecycle::{handle_agent_event, remember_input};
use commands::{handle_command, handle_running_command};

pub use context::CONTEXT_INSPECTION_MAX_ITEMS;
pub use context::start_auto_compaction;

#[cfg(test)]
use input::accept_model_suggestion;

pub use commands::command_suggestions_for_app;
pub use input::{
    Action, FilePickerSource, InputFocus, KeyBinding, KeyHelp, Keymap, Mode, PickerItem, PickerState, PromptAccessory,
    translate_input, translate_input_with_keymap,
};
pub use input::{audit_queue_transition, next_detail_target};
pub use onboarding::setup_model_options;
pub use onboarding::{
    ChatGptOAuthDriver, ChatGptOAuthMethod, ChatGptOAuthRecovery, FirstRunRecovery, RecoveryIntent, RecoveryStage,
};
pub use transcript_blocks::{
    BlockContentState, ToolLifecycleError, ToolLifecycleState, TranscriptBlock, TranscriptBlockId, TranscriptBlockKind,
    TranscriptBlocks,
};

use input::{
    offline_model_picker_items, open_model_picker, open_reasoning_effort_picker, open_session_picker, open_skill_picker,
};
use onboarding::{
    PendingSetupReasoningEffort, advance_after_setup_model_config, handle_first_run_action, poll_chatgpt_oauth_on_tick,
    provider_authenticated, provider_for_model, selected_provider_missing,
};

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thndrs_agent::CancelToken;
use thndrs_agent::ProviderRequestAccounting;
pub use thndrs_agent::ToolStatus;
use thndrs_agent::context::{self as agent_context, CompactionConfig, CompactionPolicy, ReductionConfig};
use thndrs_agent::context::{
    CompactionSummaryCandidate, ContextItemKind, ContextVisibility, HarnessCandidate, InstructionCandidate,
    PendingPermissionCandidate, PinnedCandidate, SelectionInput, SkillCandidate, TranscriptCandidate,
    UserTurnCandidate,
};

use crate::acp::config::provider_label;
use crate::acp::permissions::{PendingPermission, PermissionDecision};
use crate::cli::commands::auth::CredentialScope;
use crate::cli::commands::setup::SetupProviderArg;
use crate::cli::git::{self, GitStatusSummary};
use crate::cli::input::history::{INPUT_HISTORY_LIMIT, InputHistoryStore};
use crate::cli::{Cli, MIN_TICK_RATE_MS, ReasoningEffort, Theme, WebSearchMode};
use crate::input::{PromptInput, TerminalInput};
use crate::providers::{codex, opencode};
use crate::thndrs_core::auth;
use crate::tools::shell::ProcessRegistry;
use crate::{config, fuzzy, internals, prompt, session, skills, tools, utils};

/// Cancel an ACP permission request from an application adapter without UI input.
pub use agent_lifecycle::cancel_pending_permission;

fn append_loaded_skill(app: &mut App, loaded: &skills::LoadedSkill, include_content: bool) {
    for diagnostic in &loaded.diagnostics {
        app.transcript.entries.push(Entry::Error { text: diagnostic.summary() });
    }
    let text = format!(
        "# Skill: {}\n\n_Source: {}_\n\n{}",
        loaded.activation.name,
        loaded.activation.path.display(),
        loaded.markdown
    );
    let token_estimate = agent_context::estimate_tokens(text.len());
    let available_input = app.refresh_context_ledger(None).budget.available_input;
    let context_percent = if available_input == 0 {
        None
    } else {
        Some(((token_estimate as u64).saturating_mul(100) / available_input).min(100) as u8)
    };
    app.transcript.entries.push(Entry::Skill {
        name: loaded.activation.name.clone(),
        path: loaded.activation.path.display().to_string(),
        content: if include_content { text } else { String::new() },
        token_estimate,
        context_percent,
    });
    if let Some(ref mut writer) = app.session.writer {
        let _ = writer.append_skill_activation(&loaded.activation);
    }
}
/// Submit a user turn from an application adapter without synthesizing key input.
pub use input::submit_user_turn;

/// How long a Ctrl+D quit confirmation stays armed.
const QUIT_CONFIRM_TIMEOUT_MS: u64 = 3_000;
/// How long transient status-bar feedback remains visible.
const STATUS_TOAST_TIMEOUT_MS: u64 = 2_000;
/// How long the UI waits for an agent to acknowledge cancellation before
/// releasing the stopped prompt.
const STOPPING_GRACE_MS: u64 = 250;

pub const VISIBLE_ROWS: usize = 8;

/// Shared cap for large filesystem-backed picker inventories so fuzzy matching
/// stays responsive while still surfacing enough nearby files or skills.
const LARGE_PICKER_LIMIT: usize = 200;
const MODEL_PICKER_LIMIT: usize = 50;

/// Whether the application writes a durable local record for the current run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RunPersistence {
    /// Keep the append-only session, tool artifacts, and per-session log.
    #[default]
    Durable,
    /// Keep the run in memory and preserve shared settings and prompt history.
    Ephemeral,
}

impl RunPersistence {
    /// Return whether this run avoids per-session filesystem writes.
    pub const fn is_ephemeral(self) -> bool {
        matches!(self, Self::Ephemeral)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionStartup {
    New,
    Existing,
}

/// Semantic run state, used for the status line.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum RunState {
    /// Nothing in flight.
    #[default]
    Idle,
    /// Agent stream active.
    Working,
    /// A stop has been requested via Escape; stream is winding down.
    Stopping,
    /// A recoverable error occurred; the prompt is editable again.
    Error(String),
}

/// The user-facing prompt state, derived from [`RunState`] and the transcript.
///
/// This drives prompt-line styling (color, hint text) to show editable,
/// submitted, streaming, stopped, errored.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PromptState {
    /// Idle — user can type and submit.
    #[default]
    Editable,
    /// Prompt submitted; waiting for the first agent event.
    Submitted,
    /// Agent is actively streaming reasoning or assistant text.
    Streaming,
    /// Agent is executing a tool call.
    RunningTool,
    /// Run was cancelled; prompt is editable again.
    Stopped,
    /// Run failed with an error; prompt is editable again.
    Errored,
}

/// Client-observed time-to-first-token state for the active and last turn.
#[derive(Clone, Debug, Default)]
pub struct TurnTtftState {
    pending_since: Option<Instant>,
    last_completed: Option<Duration>,
}

impl TurnTtftState {
    /// Start timing a newly submitted local user turn.
    pub fn start_turn(&mut self) {
        self.pending_since = Some(Instant::now());
    }

    /// Stop timing on the first semantic provider output.
    pub fn stop_on_semantic_output(&mut self) {
        if let Some(started_at) = self.pending_since.take() {
            self.last_completed = Some(started_at.elapsed());
        }
    }

    /// Clear an unfinished pending measurement without replacing the last one.
    pub fn clear_pending(&mut self) {
        self.pending_since = None;
    }

    /// Whether the current turn is still waiting for semantic output.
    pub fn is_pending(&self) -> bool {
        self.pending_since.is_some()
    }

    /// Last successfully measured TTFT.
    pub fn last_completed(&self) -> Option<Duration> {
        self.last_completed
    }

    #[cfg(test)]
    pub fn set_pending_for_test(&mut self) {
        self.pending_since = Some(Instant::now());
    }

    #[cfg(test)]
    pub fn set_last_completed_for_test(&mut self, duration: Duration) {
        self.pending_since = None;
        self.last_completed = Some(duration);
    }
}

/// Client-observed elapsed time for the active or most recent turn.
#[derive(Clone, Debug, Default)]
pub struct TurnTimingState {
    turn_started_at: Option<Instant>,
    last_turn: Option<Duration>,
}

impl TurnTimingState {
    /// Start timing a new turn.
    pub fn start_turn(&mut self) {
        self.turn_started_at = Some(Instant::now());
        self.last_turn = None;
    }

    /// Ensure an externally started turn has a timing origin.
    pub fn ensure_started(&mut self) {
        if self.turn_started_at.is_none() {
            self.start_turn();
        }
    }

    /// Freeze the current timing when the turn settles.
    pub fn finish_turn(&mut self) {
        if let Some(started_at) = self.turn_started_at.take() {
            self.last_turn = Some(started_at.elapsed());
        }
    }

    /// Whether the turn timer is currently running.
    pub fn is_active(&self) -> bool {
        self.turn_started_at.is_some()
    }

    /// The elapsed time for the active or most recently completed turn.
    pub fn elapsed(&self) -> Option<Duration> {
        self.turn_started_at
            .map(|started_at| started_at.elapsed())
            .or(self.last_turn)
    }

    #[cfg(test)]
    pub fn set_completed_for_test(&mut self, turn: Duration) {
        self.turn_started_at = None;
        self.last_turn = Some(turn);
    }
}

/// Client-observed timing and transcript links for one provider request.
#[derive(Debug, Default)]
pub struct RequestObservationState {
    request_id: Option<String>,
    attempt: u32,
    started_at: Option<Instant>,
    duration: Option<Duration>,
    time_to_first_token: Option<Duration>,
    tool_count: Option<u64>,
    completed_tools: u64,
    tool_duration: Duration,
    active_tools: HashMap<String, Instant>,
    transcript_start: usize,
}

impl RequestObservationState {
    fn start(&mut self, accounting: &ProviderRequestAccounting, transcript_start: usize) {
        *self = Self {
            request_id: Some(accounting.request_id.clone()),
            attempt: accounting.attempt,
            started_at: Some(Instant::now()),
            transcript_start,
            ..Self::default()
        };
    }

    fn matches(&self, accounting: &ProviderRequestAccounting) -> bool {
        self.request_id.as_deref() == Some(accounting.request_id.as_str()) && self.attempt == accounting.attempt
    }

    fn stop_on_semantic_output(&mut self) {
        if self.time_to_first_token.is_none()
            && let Some(started_at) = self.started_at
        {
            self.time_to_first_token = Some(started_at.elapsed());
        }
    }

    fn finish_request(&mut self, accounting: &ProviderRequestAccounting) {
        if !self.matches(accounting) {
            return;
        }
        if self.duration.is_none()
            && let Some(started_at) = self.started_at
        {
            self.duration = Some(started_at.elapsed());
        }
        if let Some(tool_count) = accounting.tool_count {
            self.tool_count = Some(tool_count);
        }
    }

    fn start_tool(&mut self, id: &str) {
        self.stop_on_semantic_output();
        self.active_tools.insert(id.to_string(), Instant::now());
    }

    fn finish_tool(&mut self, id: &str) {
        let Some(started_at) = self.active_tools.remove(id) else { return };
        self.completed_tools = self.completed_tools.saturating_add(1);
        self.tool_duration = self.tool_duration.saturating_add(started_at.elapsed());
    }

    fn duration_ms(&self, accounting: &ProviderRequestAccounting) -> Option<u64> {
        self.matches(accounting).then_some(self.duration?).map(duration_millis)
    }

    fn time_to_first_token_ms(&self, accounting: &ProviderRequestAccounting) -> Option<u64> {
        self.matches(accounting)
            .then_some(self.time_to_first_token?)
            .map(duration_millis)
    }

    fn tool_duration_ms(&self, accounting: &ProviderRequestAccounting) -> Option<u64> {
        if !self.matches(accounting) {
            return None;
        }
        let expected = self.tool_count?;
        (self.completed_tools >= expected && self.active_tools.is_empty()).then(|| duration_millis(self.tool_duration))
    }
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

/// Where input submitted during an active run should be queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum QueueTarget {
    /// Inject before the next provider request in the active run.
    Steering,
    /// Submit as a new user turn after the active run finishes.
    #[default]
    FollowUp,
}

/// Stable identity for one queued input within a session.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct QueueItemId(pub u64);

impl std::fmt::Display for QueueItemId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "q{}", self.0)
    }
}

/// Durable audit state for a queued input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueueAuditState {
    Recorded,
    Failed(String),
}

/// Settlement state for one queue item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueSettlement {
    Pending,
    Sent,
    Cancelled,
    Deleted,
}

impl QueueSettlement {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Cancelled => "cancelled",
            Self::Deleted => "deleted",
        }
    }
}

/// One inspectable queued prompt or steering message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueueItem {
    pub id: QueueItemId,
    pub target: QueueTarget,
    pub text: String,
    pub created_at: String,
    pub audit: QueueAuditState,
    pub settlement: QueueSettlement,
}

impl QueueItem {
    /// A single-line, bounded, display-safe summary. The full text remains in
    /// the queue and session journal, never in status or tracing output.
    pub fn preview(&self, max_chars: usize) -> String {
        let normalized = self.text.split_whitespace().collect::<Vec<_>>().join(" ");
        crate::utils::truncate_ellipsis(&normalized, max_chars)
    }
}

/// Ordered queue state, including settled items retained for inspection.
#[derive(Debug, Default)]
pub struct QueueState {
    pub items: Vec<QueueItem>,
    next_id: u64,
}

impl QueueState {
    pub fn push(&mut self, target: QueueTarget, text: String, created_at: String) -> QueueItemId {
        self.next_id = self.next_id.saturating_add(1);
        let id = QueueItemId(self.next_id);
        self.items.push(QueueItem {
            id,
            target,
            text,
            created_at,
            audit: QueueAuditState::Recorded,
            settlement: QueueSettlement::Pending,
        });
        id
    }

    pub fn pending(&self, target: QueueTarget) -> impl Iterator<Item = &QueueItem> {
        self.items
            .iter()
            .filter(move |item| item.target == target && item.settlement == QueueSettlement::Pending)
    }

    pub fn pending_count(&self, target: QueueTarget) -> usize {
        self.pending(target).count()
    }

    pub fn pending_id(&self, target: QueueTarget) -> Option<QueueItemId> {
        self.pending(target).next().map(|item| item.id)
    }

    pub fn item(&self, id: QueueItemId) -> Option<&QueueItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn item_mut(&mut self, id: QueueItemId) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub fn settle(&mut self, id: QueueItemId, settlement: QueueSettlement) -> Option<String> {
        let item = self.item_mut(id)?;
        item.settlement = settlement;
        Some(item.text.clone())
    }

    pub fn restore_next_id(&mut self) {
        self.next_id = self.items.iter().map(|item| item.id.0).max().unwrap_or_default();
    }
}

impl QueueTarget {
    pub fn toggle(self) -> Self {
        match self {
            QueueTarget::Steering => QueueTarget::FollowUp,
            QueueTarget::FollowUp => QueueTarget::Steering,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            QueueTarget::Steering => "steering",
            QueueTarget::FollowUp => "follow-up",
        }
    }
}

/// Lines to render when a tool detail pane is open.
///
/// The detail pane expands a transcript [`Entry::Tool`] into a scrollable
/// surface so the user can read full tool output without leaving the TUI.
/// It tracks which transcript entry (by index) is expanded and the current
/// scroll offset within that entry's rendered output rows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DetailPane {
    /// Index into `app.transcript.entries` for the expanded tool entry.
    pub entry_index: usize,
    /// Scroll offset: number of rendered output rows skipped from the top.
    pub scroll: usize,
}

impl DetailPane {
    /// Scroll up one line, clamped at zero.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Scroll down one rendered row.
    ///
    /// The renderer clamps this value once terminal width and wrapped
    /// row count are known.
    pub fn scroll_down(&mut self, total: usize) {
        if total > 0 {
            self.scroll = self.scroll.saturating_add(1);
        }
    }
}

/// One bounded semantic transcript match. Byte offsets always lie on UTF-8
/// boundaries in the entry's searchable projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptMatch {
    pub entry_index: usize,
    pub start: usize,
    pub end: usize,
}

/// Focused transcript search state kept separate from the composer draft.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TranscriptSearchState {
    pub query: PromptInput,
    pub matches: Vec<TranscriptMatch>,
    pub selected: usize,
    pub truncated: bool,
}

impl TranscriptSearchState {
    pub const MAX_MATCHES: usize = 1_000;
    const MAX_ENTRY_BYTES: usize = 32 * 1024;

    pub fn refresh(&mut self, entries: &TranscriptBlocks) {
        self.matches.clear();
        self.selected = 0;
        self.truncated = false;
        let query = self.query.as_str();
        if query.is_empty() {
            return;
        }
        for (entry_index, entry) in entries.iter().enumerate() {
            let text = transcript_search_text(entry);
            let bounded = bounded_utf8(&text, Self::MAX_ENTRY_BYTES);
            for (start, _) in bounded.match_indices(query) {
                if self.matches.len() == Self::MAX_MATCHES {
                    self.truncated = true;
                    return;
                }
                self.matches
                    .push(TranscriptMatch { entry_index, start, end: start.saturating_add(query.len()) });
            }
        }
    }

    pub fn next(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.matches.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.matches.len() - 1);
        }
    }

    pub fn current(&self) -> Option<TranscriptMatch> {
        self.matches.get(self.selected).copied()
    }
}

fn transcript_search_text(entry: &Entry) -> String {
    match entry {
        Entry::User { text }
        | Entry::Agent { text, .. }
        | Entry::Reasoning { text, .. }
        | Entry::Status { text }
        | Entry::Error { text } => text.clone(),
        Entry::Skill { name, path, content, .. } => format!("{name} {path} {content}"),
        Entry::Tool { name, arguments, status, .. } => {
            // Compact transcript search deliberately excludes hidden tool output.
            format!("{name} {arguments} {status:?}")
        }
    }
}

fn bounded_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    &text[..end]
}

/// Focused queue inspector/editor. Its private edit buffer cannot overwrite
/// the composer draft or participate in prompt history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueuePane {
    pub selected: usize,
    pub editing: Option<PromptInput>,
}

/// One transcript row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// User-submitted text.
    User { text: String },
    /// Agent text, possibly still streaming.
    Agent { text: String, streaming: bool },
    /// Reasoning/thinking text, kept separate from final assistant text.
    Reasoning { text: String, streaming: bool },
    /// Explicitly activated skill instructions with a compact UI projection.
    Skill {
        name: String,
        path: String,
        content: String,
        token_estimate: usize,
        context_percent: Option<u8>,
    },
    /// A tool call block.
    Tool {
        name: String,
        arguments: String,
        status: ToolStatus,
        output: Vec<String>,
    },
    /// A status row (e.g. context sources loaded).
    Status { text: String },
    /// An error row.
    Error { text: String },
}

/// Events from the background agent stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    Started,
    Status(String),
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Application-owned ChatGPT Codex capacity headers observed on a response.
    CodexUsage(codex::CodexUsageStatus),
    /// One successful provider request with exact size and optional usage.
    RequestAccounting(Box<ProviderRequestAccounting>),
    /// One serialized provider request immediately before dispatch.
    RequestStarted(Box<ProviderRequestAccounting>),
    AssistantDelta(String),
    ReasoningDelta(String),
    ToolStarted {
        id: String,
        name: String,
        arguments: String,
    },
    ToolFinished {
        id: String,
        output: Vec<String>,
        status: ToolStatus,
        /// Structured write result if this was a file-write tool, else `None`.
        write_result: Option<tools::WriteResult>,
        /// Structured shell result if this was a `run_shell` tool, else `None`.
        /// Boxed to avoid a large enum variant (`ProcessResult` carries multiple `Vec<String>` values).
        shell_result: Option<Box<tools::shell::ProcessResult>>,
    },
    /// A state-aware projection relation for one completed tool result.
    StateProjectionDecision {
        /// Provider-assigned tool-call id.
        id: String,
        /// Proven duplicate or supersession decision.
        decision: thndrs_agent::context::StateProjectionDecision,
    },
    ModelMetadataLoaded(Vec<(String, String)>),
    Retrying {
        attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
        error: String,
    },
    PermissionRequest(PendingPermission),
    PermissionResolved {
        tool_call_id: String,
        outcome: String,
    },
    AcpSession(session::AcpSessionMetadata),
    Finished,
    Failed(String),
    Cancelled,
}

/// Identity attached to an effect and its eventual completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectRequest {
    /// Session that requested the effect.
    pub session_id: String,
    /// Monotonic session turn associated with the request.
    pub turn: u64,
}

/// Side effects requested by the pure application update path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    StartAgent(EffectRequest),
    CancelAgent(EffectRequest),
    SettleAgent(EffectRequest),
    DrainBackgroundProcesses,
    ShutdownProcesses,
    ClearTerminal,
    SuspendTerminal,
}

/// Semantic completion values returned by effect executors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EffectResult {
    Agent {
        request: EffectRequest,
        event: AgentEvent,
    },
    BackgroundProcesses(Vec<tools::shell::ProcessResult>),
    Failed {
        request: Option<EffectRequest>,
        operation: &'static str,
        error: String,
    },
}

/// Pure result of applying one application message.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateResult {
    pub follow_up: Option<Msg>,
    pub effects: Vec<Effect>,
}

/// The single message type fed into `update`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Msg {
    /// A semantic action from the normalized terminal input boundary.
    Action(Action),
    /// Compatibility adapter for callers that still provide a raw key.
    Key(crossterm::event::KeyEvent),
    /// Compatibility adapter for callers that still provide a raw mouse event.
    Mouse(crossterm::event::MouseEvent),
    /// Periodic tick.
    Tick,
    /// Clear the transcript.
    Clear,
    /// Quit the app.
    Quit,
    /// An agent stream event.
    Agent(AgentEvent),
    /// A semantic result returned by an application effect executor.
    Effect(EffectResult),
    /// Updated git working tree summary from the background watcher.
    GitStatusChanged(Option<GitStatusSummary>),
}

/// Durable identity and append-only audit state for one application session.
#[derive(Debug)]
pub struct SessionState {
    /// Persistence policy selected for the current run.
    pub run_persistence: RunPersistence,
    /// Durable policy governing request and artifact content capture.
    pub context_capture_policy: session::ContextCapturePolicy,
    /// Stable session identity used by records and prompt history.
    pub id: String,
    /// Root lineage identity used for stable context item ids across forks.
    pub context_id_namespace: String,
    /// Append-only session writer, when durable persistence is available.
    pub writer: Option<session::SessionWriter>,
    /// Dedicated workspace-local persistence for submitted prompt recall.
    pub input_history_store: InputHistoryStore,
    /// Monotonic turn counter for session record correlation.
    pub turn_count: u64,
    /// Most recent completed provider request accounting.
    pub last_request_accounting: Option<ProviderRequestAccounting>,
    /// Serialized request attempt awaiting a terminal accounting event.
    pub active_request_accounting: Option<ProviderRequestAccounting>,
    /// Non-fatal config diagnostics captured for this session.
    pub config_diagnostics: Vec<String>,
    /// MCP config files captured by the session audit.
    pub mcp_config_files: Vec<session::SessionConfigFile>,
    /// Non-fatal MCP config audit diagnostics.
    pub mcp_config_diagnostics: Vec<String>,
}

/// Transcript, context selection, and compaction state.
#[derive(Debug)]
pub struct TranscriptState {
    /// Chronological user, provider, tool, and status entries.
    pub entries: TranscriptBlocks,
    /// Loaded context sources (for example, AGENTS.md).
    pub context_sources: Vec<crate::context::ContextSource>,
    /// Filesystem discovery diagnostics for project instructions.
    pub context_diagnostics: Vec<crate::context::InstructionDiagnostic>,
    /// Latest provider-neutral context ledger.
    pub context_ledger: Option<agent_context::ContextLedger>,
    /// Content-free request snapshots and context transformations used by
    /// `/context changes` and semantic transcript events.
    pub context_history: session::ContextHistory,
    /// Discovered Agent Skills metadata.
    pub skills: Vec<skills::SkillMetadata>,
    /// Skill discovery diagnostics.
    pub skill_diagnostics: Vec<skills::SkillDiagnostic>,
    /// Reusable prompt templates exposed through slash-command completion.
    pub prompt_templates: Vec<prompt::templates::PromptTemplate>,
    /// Prompt-template discovery diagnostics.
    pub prompt_template_diagnostics: Vec<prompt::templates::PromptTemplateDiagnostic>,
    /// Tool-call ids mapped to bounded redacted recovery handles.
    pub tool_artifacts: HashMap<String, String>,
    /// State-aware model-projection decisions indexed by tool-call id.
    pub tool_projection_decisions: HashMap<String, agent_context::StateProjectionDecision>,
    /// Durable context lifecycle/protection state.
    pub context_lifecycles: BTreeMap<String, agent_context::ContextLifecycle>,
    /// In-flight compaction request.
    pub pending_manual_compaction: Option<context::PendingManualCompaction>,
    /// Summary awaiting explicit review.
    pub pending_compaction_review: Option<context::PendingCompactionReview>,
    /// Last compaction review state.
    pub last_compaction_review: Option<session::CompactionReviewResult>,
    /// Task-local context pins.
    pub context_pins: Vec<PinnedCandidate>,
    /// Explicitly dropped context ids.
    pub context_dropped_ids: Vec<String>,
    /// Summaries retained across context rebuilds.
    pub compaction_summaries: Vec<CompactionSummaryCandidate>,
}

/// Prompt editing, history, queue, and recovery-draft state.
#[derive(Debug)]
pub struct ComposerState {
    pub mode: Mode,
    pub input: PromptInput,
    /// Submitted prompt history for Up/Down recall.
    pub input_history: Vec<String>,
    /// Current history navigation index.
    pub history_cursor: Option<usize>,
    /// Draft captured before history navigation starts.
    pub history_draft: String,
    /// Current target for input submitted during an active run.
    pub queue_target: QueueTarget,
    /// Inspectable, ordered steering and follow-up inputs.
    pub queue: QueueState,
    /// Prompt restored after provider failure or retained for an internal turn.
    pub last_input: Option<String>,
    /// Kill-ring for readline-style yank.
    pub kill_ring: Vec<String>,
}

/// Setup/recovery state carried by the focused setup overlay.
#[derive(Debug)]
pub struct SetupState {
    pub recovery: FirstRunRecovery,
    pub pending_setup_reasoning_effort: Option<PendingSetupReasoningEffort>,
}

impl SetupState {
    fn new(recovery: FirstRunRecovery) -> Self {
        Self { recovery, pending_setup_reasoning_effort: None }
    }
}

/// Focused overlay variants. The enum makes picker, detail, setup, help, and
/// permission surfaces mutually exclusive by construction.
#[derive(Debug)]
enum OverlaySurface {
    None,
    Help {
        scroll: usize,
    },
    Commands {
        selected: usize,
    },
    Files {
        source: FilePickerSource,
        picker: PickerState,
    },
    Models {
        picker: PickerState,
    },
    ReasoningEffort {
        picker: PickerState,
        pending_setup: Option<PendingSetupReasoningEffort>,
    },
    Skills {
        picker: PickerState,
    },
    Sessions {
        picker: PickerState,
    },
    Context,
    Detail(DetailPane),
    TranscriptSearch(TranscriptSearchState),
    Queue(QueuePane),
    Setup(SetupState),
    Permission(PendingPermission),
}

/// Focus, auth recovery, and all other mutually-exclusive transient surfaces.
#[derive(Debug)]
pub struct OverlayState {
    surface: OverlaySurface,
    /// OAuth effect drivers belong to the auth/recovery domain even while no
    /// setup form is focused (tests and adapters can configure them first).
    oauth_driver: ChatGptOAuthDriver,
    browser_login: Option<auth::ChatGptCodexBrowserLogin>,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self { surface: OverlaySurface::None, oauth_driver: ChatGptOAuthDriver::default(), browser_login: None }
    }
}

impl OverlayState {
    /// Project the focused surface into the legacy accessory vocabulary.
    pub fn accessory(&self) -> PromptAccessory {
        match &self.surface {
            OverlaySurface::None
            | OverlaySurface::Detail(_)
            | OverlaySurface::TranscriptSearch(_)
            | OverlaySurface::Queue(_)
            | OverlaySurface::Setup(_)
            | OverlaySurface::Permission(_) => PromptAccessory::None,
            OverlaySurface::Help { .. } => PromptAccessory::Help,
            OverlaySurface::Commands { selected } => PromptAccessory::Commands { selected: *selected },
            OverlaySurface::Files { source, .. } => PromptAccessory::Files(*source),
            OverlaySurface::Models { .. } => PromptAccessory::Models,
            OverlaySurface::ReasoningEffort { .. } => PromptAccessory::ReasoningEffort,
            OverlaySurface::Skills { .. } => PromptAccessory::Skills,
            OverlaySurface::Sessions { .. } => PromptAccessory::Sessions,
            OverlaySurface::Context => PromptAccessory::Context,
        }
    }

    /// Return the active picker, if the focused surface owns one.
    pub fn picker(&self) -> Option<&PickerState> {
        match &self.surface {
            OverlaySurface::Files { picker, .. }
            | OverlaySurface::Models { picker }
            | OverlaySurface::ReasoningEffort { picker, .. }
            | OverlaySurface::Skills { picker }
            | OverlaySurface::Sessions { picker } => Some(picker),
            _ => None,
        }
    }

    /// Mutably access the active picker.
    pub fn picker_mut(&mut self) -> Option<&mut PickerState> {
        match &mut self.surface {
            OverlaySurface::Files { picker, .. }
            | OverlaySurface::Models { picker }
            | OverlaySurface::ReasoningEffort { picker, .. }
            | OverlaySurface::Skills { picker }
            | OverlaySurface::Sessions { picker } => Some(picker),
            _ => None,
        }
    }

    /// Return the setup recovery state when setup owns focus.
    pub fn setup(&self) -> Option<&FirstRunRecovery> {
        match &self.surface {
            OverlaySurface::Setup(setup) => Some(&setup.recovery),
            _ => None,
        }
    }

    /// Mutably access setup recovery state.
    pub fn setup_mut(&mut self) -> Option<&mut FirstRunRecovery> {
        match &mut self.surface {
            OverlaySurface::Setup(setup) => Some(&mut setup.recovery),
            _ => None,
        }
    }

    /// Return the pending permission when permission owns focus.
    pub fn permission(&self) -> Option<&PendingPermission> {
        match &self.surface {
            OverlaySurface::Permission(permission) => Some(permission),
            _ => None,
        }
    }

    /// Mutably access the pending permission.
    pub fn permission_mut(&mut self) -> Option<&mut PendingPermission> {
        match &mut self.surface {
            OverlaySurface::Permission(permission) => Some(permission),
            _ => None,
        }
    }

    /// Take the pending permission and return focus to the prompt.
    pub fn take_permission(&mut self) -> Option<PendingPermission> {
        match std::mem::replace(&mut self.surface, OverlaySurface::None) {
            OverlaySurface::Permission(permission) => Some(permission),
            other => {
                self.surface = other;
                None
            }
        }
    }

    /// Return the active detail pane.
    pub fn detail(&self) -> Option<&DetailPane> {
        match &self.surface {
            OverlaySurface::Detail(detail) => Some(detail),
            _ => None,
        }
    }

    /// Mutably access the active detail pane.
    pub fn detail_mut(&mut self) -> Option<&mut DetailPane> {
        match &mut self.surface {
            OverlaySurface::Detail(detail) => Some(detail),
            _ => None,
        }
    }

    /// Whether the detail surface owns focus.
    pub fn is_detail(&self) -> bool {
        matches!(self.surface, OverlaySurface::Detail(_))
    }

    pub fn transcript_search(&self) -> Option<&TranscriptSearchState> {
        match &self.surface {
            OverlaySurface::TranscriptSearch(search) => Some(search),
            _ => None,
        }
    }

    pub fn transcript_search_mut(&mut self) -> Option<&mut TranscriptSearchState> {
        match &mut self.surface {
            OverlaySurface::TranscriptSearch(search) => Some(search),
            _ => None,
        }
    }

    pub fn queue(&self) -> Option<&QueuePane> {
        match &self.surface {
            OverlaySurface::Queue(queue) => Some(queue),
            _ => None,
        }
    }

    pub fn queue_mut(&mut self) -> Option<&mut QueuePane> {
        match &mut self.surface {
            OverlaySurface::Queue(queue) => Some(queue),
            _ => None,
        }
    }

    pub fn show_transcript_search(&mut self) {
        self.surface = OverlaySurface::TranscriptSearch(TranscriptSearchState::default());
    }

    pub fn show_queue(&mut self) {
        self.surface = OverlaySurface::Queue(QueuePane::default());
    }

    /// Close details without disturbing another focused surface.
    pub fn close_detail(&mut self) {
        if self.is_detail() {
            self.close();
        }
    }

    /// Access the auth driver owned by this overlay domain.
    pub fn oauth_driver(&self) -> &ChatGptOAuthDriver {
        &self.oauth_driver
    }

    /// Mutably configure the auth driver without changing focus.
    pub fn oauth_driver_mut(&mut self) -> &mut ChatGptOAuthDriver {
        &mut self.oauth_driver
    }

    /// Access the short-lived browser callback owned by auth recovery.
    pub fn browser_login(&self) -> Option<&auth::ChatGptCodexBrowserLogin> {
        self.browser_login.as_ref()
    }

    /// Mutably access the short-lived browser callback.
    pub fn browser_login_mut(&mut self) -> Option<&mut auth::ChatGptCodexBrowserLogin> {
        self.browser_login.as_mut()
    }

    /// Replace the short-lived browser callback.
    pub fn set_browser_login(&mut self, login: Option<auth::ChatGptCodexBrowserLogin>) {
        self.browser_login = login;
    }

    /// Replace focus with a setup/recovery surface.
    pub fn show_setup(&mut self, recovery: FirstRunRecovery) {
        self.surface = OverlaySurface::Setup(SetupState::new(recovery));
    }

    /// Replace focus with a command suggestion surface.
    pub fn show_commands(&mut self) {
        self.surface = OverlaySurface::Commands { selected: 0 };
    }

    /// Mutably access the selected command suggestion.
    pub fn command_selected_mut(&mut self) -> Option<&mut usize> {
        match &mut self.surface {
            OverlaySurface::Commands { selected } => Some(selected),
            _ => None,
        }
    }

    /// Replace focus with help.
    pub fn show_help(&mut self) {
        self.surface = OverlaySurface::Help { scroll: 0 };
    }

    /// Return the help surface scroll anchor, when help owns focus.
    pub fn help_scroll(&self) -> Option<usize> {
        match &self.surface {
            OverlaySurface::Help { scroll } => Some(*scroll),
            _ => None,
        }
    }

    /// Mutably access the help surface scroll anchor.
    pub fn help_scroll_mut(&mut self) -> Option<&mut usize> {
        match &mut self.surface {
            OverlaySurface::Help { scroll } => Some(scroll),
            _ => None,
        }
    }

    /// Replace focus with context inspection.
    pub fn show_context(&mut self) {
        self.surface = OverlaySurface::Context;
    }

    /// Replace focus with a picker. Non-picker accessories are rejected.
    pub fn show_picker(&mut self, accessory: PromptAccessory, picker: PickerState) -> Result<(), &'static str> {
        let pending_setup = match accessory {
            PromptAccessory::ReasoningEffort => self.take_pending_setup_reasoning_effort(),
            _ => None,
        };
        self.surface = match accessory {
            PromptAccessory::Files(source) => OverlaySurface::Files { source, picker },
            PromptAccessory::Models => OverlaySurface::Models { picker },
            PromptAccessory::ReasoningEffort => OverlaySurface::ReasoningEffort { picker, pending_setup },
            PromptAccessory::Skills => OverlaySurface::Skills { picker },
            PromptAccessory::Sessions => OverlaySurface::Sessions { picker },
            _ => return Err("overlay accessory does not own a picker"),
        };
        Ok(())
    }

    /// Open tool details when no other modal surface owns focus.
    pub fn show_detail(&mut self, entry_index: usize) {
        if matches!(self.surface, OverlaySurface::None | OverlaySurface::Detail(_)) {
            self.surface = OverlaySurface::Detail(DetailPane { entry_index, scroll: 0 });
        }
    }

    /// Show one ACP permission request with exclusive focus.
    pub fn show_permission(&mut self, permission: PendingPermission) {
        self.surface = OverlaySurface::Permission(permission);
    }

    /// Set setup's deferred reasoning transition.
    pub fn set_pending_setup_reasoning_effort(&mut self, pending: PendingSetupReasoningEffort) {
        if let OverlaySurface::Setup(setup) = &mut self.surface {
            setup.pending_setup_reasoning_effort = Some(pending);
        }
    }

    /// Inspect setup's deferred reasoning transition while its picker is open.
    pub fn pending_setup_reasoning_effort(&self) -> Option<PendingSetupReasoningEffort> {
        match &self.surface {
            OverlaySurface::Setup(setup) => setup.pending_setup_reasoning_effort,
            OverlaySurface::ReasoningEffort { pending_setup, .. } => *pending_setup,
            _ => None,
        }
    }

    /// Take setup's deferred reasoning transition before closing its picker.
    pub fn take_pending_setup_reasoning_effort(&mut self) -> Option<PendingSetupReasoningEffort> {
        match &mut self.surface {
            OverlaySurface::Setup(setup) => setup.pending_setup_reasoning_effort.take(),
            OverlaySurface::ReasoningEffort { pending_setup, .. } => pending_setup.take(),
            _ => None,
        }
    }

    /// Complete the current overlay transition.
    pub fn close(&mut self) {
        self.surface = OverlaySurface::None;
    }
}

/// Visual intent for a transient status-bar message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusToastKind {
    Success,
    Error,
}

/// Short-lived feedback displayed in place of the operational status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusToast {
    pub text: String,
    pub kind: StatusToastKind,
    expires_at_tick: u64,
}

/// Runtime configuration, provider status, and owned background processes.
#[derive(Debug)]
pub struct RuntimeState {
    pub cli: Cli,
    pub cwd: PathBuf,
    pub model: String,
    pub websearch: WebSearchMode,
    pub theme: Theme,
    pub verbose: bool,
    pub user_label: String,
    pub model_picker_items: Vec<PickerItem>,
    /// Configurable semantic bindings used by the input translation boundary.
    pub keymap: Keymap,
    pub run_state: RunState,
    /// Transient provider backoff shown in the status line instead of the transcript.
    pub provider_retry: Option<String>,
    /// Short-lived user feedback shown in the status line.
    pub status_toast: Option<StatusToast>,
    /// Identity of the agent run whose effect results are currently accepted.
    pub active_effect_request: Option<EffectRequest>,
    pub git_status: Option<GitStatusSummary>,
    pub session_tokens_in: u64,
    pub session_tokens_out: u64,
    pub session_usage: SessionUsage,
    pub codex_usage: Option<codex::CodexUsageStatus>,
    pub ttft: TurnTtftState,
    pub turn_timing: TurnTimingState,
    pub request_observation: RequestObservationState,
    pub ui_tick: u64,
    pub ctrl_d_pending: Option<u64>,
    pub stopping_deadline: Option<u64>,
    pub stopping_timed_out: bool,
    pub process_registry: ProcessRegistry,
    pub quit: bool,
}

/// Measured provider consumption accumulated for the active session.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionUsage {
    /// Completed provider requests represented in these totals.
    pub request_count: u64,
    /// Provider-reported input tokens, when reported for every request.
    pub input_tokens: Option<u64>,
    /// Provider-reported output tokens, when reported for every request.
    pub output_tokens: Option<u64>,
    /// Provider-reported reasoning tokens, when reported for every request.
    pub reasoning_tokens: Option<u64>,
    /// Provider-reported cache-read tokens, when reported for every request.
    pub cache_read_input_tokens: Option<u64>,
    /// Provider-reported cache-write tokens, when reported for every request.
    pub cache_creation_input_tokens: Option<u64>,
}

impl SessionUsage {
    /// Add one completed request without treating missing components as zero.
    pub fn record(&mut self, usage: Option<&thndrs_agent::ProviderUsageComponents>) {
        let previous_requests = self.request_count;
        self.request_count = self.request_count.saturating_add(1);
        let component = usage.cloned().unwrap_or_default();
        merge_usage_total(&mut self.input_tokens, component.input_tokens, previous_requests);
        merge_usage_total(&mut self.output_tokens, component.output_tokens, previous_requests);
        merge_usage_total(
            &mut self.reasoning_tokens,
            component.reasoning_tokens,
            previous_requests,
        );
        merge_usage_total(
            &mut self.cache_read_input_tokens,
            component.cache_read_input_tokens,
            previous_requests,
        );
        merge_usage_total(
            &mut self.cache_creation_input_tokens,
            component.cache_creation_input_tokens,
            previous_requests,
        );
    }
}

fn merge_usage_total(total: &mut Option<u64>, value: Option<u64>, previous_requests: u64) {
    *total = match (*total, value, previous_requests) {
        (_, None, _) | (None, _, 1..) => None,
        (Some(total), Some(value), _) => Some(total.saturating_add(value)),
        (None, Some(value), 0) => Some(value),
    };
}

/// The full application state used to draw the screen.
#[derive(Debug)]
pub struct App {
    pub session: SessionState,
    pub transcript: TranscriptState,
    pub composer: ComposerState,
    pub overlay: OverlayState,
    pub runtime: RuntimeState,
}

impl App {
    fn build(value: &Cli, session_startup: SessionStartup) -> Self {
        let workspace_root = crate::context::discover_workspace_root(&value.cwd);
        let mut cli_snapshot = value.clone();
        cli_snapshot.cwd = workspace_root.clone();
        cli_snapshot.tick_rate_ms = cli_snapshot.tick_rate_ms.max(MIN_TICK_RATE_MS);
        let context_inventory = crate::context::discover_instructions(&workspace_root);
        let context_sources = context_inventory.sources;
        let context_diagnostics = context_inventory.diagnostics;
        let skill_inventory = skills::discover(&workspace_root, &value.skill_dirs);
        let prompt_template_inventory = prompt::templates::discover(&workspace_root);
        let transcript = TranscriptBlocks::new();
        let sessions_dir = value
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&workspace_root));
        let run_persistence = if value.ephemeral { RunPersistence::Ephemeral } else { RunPersistence::Durable };
        let session_id = available_session_id(&sessions_dir);
        let input_history_store = InputHistoryStore::for_workspace(&workspace_root);
        let input_history = input_history_store.load_recent().ok().flatten().unwrap_or_default();
        let (mcp_config_files, mcp_config_diagnostics) = agent_lifecycle::load_mcp_config_audit(&workspace_root);

        let config_meta = (!run_persistence.is_ephemeral()).then(|| {
            let files: Vec<session::SessionConfigFile> = value
                .config_layers
                .iter()
                .filter_map(|layer| {
                    let path = layer.display_path.as_ref()?;
                    Some(session::SessionConfigFile {
                        path: path.clone(),
                        source: layer.source.as_str().to_string(),
                        sha256: layer.hash.clone().unwrap_or_default(),
                    })
                })
                .collect();
            let origins: std::collections::BTreeMap<String, String> = value
                .config_origins
                .iter()
                .map(|(key, origin)| (key.clone(), format!("{}:{}", origin.source.as_str(), origin.detail)))
                .collect();
            let diagnostics = value.config_diagnostics.clone();
            let session_dir = Some(sessions_dir.display().to_string());
            session::SessionConfigMeta {
                session_dir,
                files,
                origins,
                diagnostics,
                mcp_files: mcp_config_files.clone(),
                mcp_diagnostics: mcp_config_diagnostics.clone(),
            }
        });

        let requested_capture_policy = if value.capture_context_content {
            session::ContextCapturePolicy::retained_content()
        } else {
            session::ContextCapturePolicy::metadata_only()
        };
        let mut session_writer = (!run_persistence.is_ephemeral() && session_startup == SessionStartup::New)
            .then(|| {
                session::SessionWriter::create(
                    &sessions_dir,
                    &session_id,
                    &workspace_root.display().to_string(),
                    "scratch",
                    provider_label(&value.model),
                    &value.model,
                    value.websearch.label(),
                    env!("CARGO_PKG_VERSION"),
                    config_meta,
                )
                .ok()
            })
            .flatten();

        let context_capture_policy = if let Some(writer) = session_writer.as_mut() {
            if writer.append_context_capture_policy(&requested_capture_policy).is_ok() {
                requested_capture_policy
            } else {
                session::ContextCapturePolicy::metadata_only()
            }
        } else {
            session::ContextCapturePolicy::metadata_only()
        };

        if let Some(ref mut writer) = session_writer.as_mut()
            && !context_sources.is_empty()
        {
            let _ = writer.append_context(&context_sources);
        }

        let mut app = App {
            session: SessionState {
                run_persistence,
                context_capture_policy,
                context_id_namespace: session_id.clone(),
                id: session_id,
                writer: session_writer,
                input_history_store,
                turn_count: 0,
                last_request_accounting: None,
                active_request_accounting: None,
                config_diagnostics: value.config_diagnostics.clone(),
                mcp_config_files,
                mcp_config_diagnostics,
            },
            transcript: TranscriptState {
                entries: transcript,
                context_sources,
                context_diagnostics,
                context_ledger: None,
                context_history: session::ContextHistory::default(),
                skills: skill_inventory.skills,
                skill_diagnostics: skill_inventory.diagnostics,
                prompt_templates: prompt_template_inventory.templates,
                prompt_template_diagnostics: prompt_template_inventory.diagnostics,
                tool_artifacts: HashMap::new(),
                tool_projection_decisions: HashMap::new(),
                context_lifecycles: BTreeMap::new(),
                pending_manual_compaction: None,
                pending_compaction_review: None,
                last_compaction_review: None,
                context_pins: Vec::new(),
                context_dropped_ids: Vec::new(),
                compaction_summaries: Vec::new(),
            },
            composer: ComposerState {
                mode: Mode::default(),
                input: PromptInput::new(),
                input_history,
                history_cursor: None,
                history_draft: String::new(),
                queue_target: QueueTarget::default(),
                queue: QueueState::default(),
                last_input: None,
                kill_ring: Vec::new(),
            },
            overlay: OverlayState::default(),
            runtime: RuntimeState {
                cli: cli_snapshot,
                cwd: workspace_root.clone(),
                model: value.model.clone(),
                websearch: value.websearch,
                theme: value.theme,
                verbose: value.verbose,
                user_label: default_user_label(),
                model_picker_items: offline_model_picker_items(),
                keymap: Keymap::default(),
                run_state: RunState::default(),
                provider_retry: None,
                status_toast: None,
                active_effect_request: None,
                git_status: git::collect(&workspace_root),
                session_tokens_in: 0,
                session_tokens_out: 0,
                session_usage: SessionUsage::default(),
                codex_usage: None,
                ttft: TurnTtftState::default(),
                turn_timing: TurnTimingState::default(),
                request_observation: RequestObservationState::default(),
                ui_tick: 0,
                ctrl_d_pending: None,
                stopping_deadline: None,
                stopping_timed_out: false,
                process_registry: ProcessRegistry::new(),
                quit: false,
            },
        };

        if let Some(recovery) = selected_provider_missing(&app, false) {
            app.overlay.show_setup(recovery);
        }
        if session_startup == SessionStartup::New {
            app.schedule_session_collection();
        }
        app
    }
}

impl From<&Cli> for App {
    fn from(value: &Cli) -> Self {
        Self::build(value, SessionStartup::New)
    }
}

impl App {
    /// Build the initial app from parsed CLI args.
    ///
    /// Discovers the workspace root from `--cwd` (preferring the git root), loads
    /// scoped `AGENTS.md` sources if present, and records their metadata in the session.
    pub fn from_cli(cli: &Cli) -> Self {
        cli.into()
    }

    /// Build the initial app by restoring an existing durable session.
    ///
    /// The existing session is resolved and locked before any new session file
    /// is created.
    pub fn from_cli_resuming(cli: &Cli, session_id: &str) -> io::Result<Self> {
        let mut app = Self::build(cli, SessionStartup::Existing);
        app.resume_session(session_id)?;
        Ok(app)
    }

    /// Replace the active session with a newly created session.
    pub fn start_new_session(&mut self) {
        let cli = self.runtime.cli.clone();
        let mut next = Self::from_cli(&cli);
        let id = next.session.id.clone();
        next.transcript
            .entries
            .push(Entry::Status { text: format!("started new session: {id}") });
        *self = next;
    }

    /// Replace the active session with a validated durable session.
    pub fn resume_session(&mut self, session_id: &str) -> io::Result<()> {
        if self.is_ephemeral() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot resume a session in ephemeral mode",
            ));
        }

        let path = session::resolve_session_file(&self.session_directory(), session_id).map_err(io::Error::other)?;
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(session_id)
            .to_string();
        if id == self.session.id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the current session is already active",
            ));
        }
        let writer = session::SessionWriter::resume(&path, &id)
            .map_err(|error| io::Error::new(error.kind(), format!("cannot resume session `{id}`: {error}")))?;
        let summary = session::SessionReader::read_summary(&path);
        let transcript = session::SessionReader::read_transcript_blocks(&path);
        let records = session::SessionReader::read_records(&path);
        let turn_count = records
            .iter()
            .filter(|record| matches!(record, session::SessionRecord::User { .. }))
            .count() as u64;

        self.session.writer = Some(writer);
        self.session.id = id.clone();
        self.session.context_id_namespace = records
            .iter()
            .find_map(|record| match record {
                session::SessionRecord::SessionFork { lineage, .. } => {
                    lineage.first().map(|entry| entry.session_id.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| id.clone());
        let requested_capture_policy = if self.runtime.cli.capture_context_content {
            session::ContextCapturePolicy::retained_content()
        } else {
            session::ContextCapturePolicy::metadata_only()
        };
        self.session.context_capture_policy = if self
            .session
            .writer
            .as_mut()
            .is_some_and(|writer| writer.append_context_capture_policy(&requested_capture_policy).is_ok())
        {
            requested_capture_policy
        } else {
            session::ContextCapturePolicy::metadata_only()
        };
        self.transcript.entries = transcript;
        self.restore_context_state(&records);
        self.session.last_request_accounting = records.iter().rev().find_map(|record| match record {
            session::SessionRecord::RequestAccounting { accounting, .. } => Some(accounting.as_ref().clone()),
            _ => None,
        });
        if self.session.context_capture_policy.permits_content()
            && let Some(accounting) = self.session.last_request_accounting.as_mut()
            && let Some(capture) = records.iter().rev().find_map(|record| match record {
                session::SessionRecord::RequestContentCaptured { capture, .. }
                    if capture.request_id == accounting.request_id && capture.attempt == accounting.attempt =>
                {
                    Some(capture)
                }
                _ => None,
            })
        {
            accounting.model_projection = capture.messages.clone();
        }
        self.runtime.session_tokens_in = summary.input_tokens;
        self.runtime.session_tokens_out = summary.output_tokens;
        self.runtime.session_usage = SessionUsage::default();
        let has_request_accounting = records
            .iter()
            .any(|record| matches!(record, session::SessionRecord::RequestAccounting { .. }));
        for record in &records {
            match record {
                session::SessionRecord::RequestAccounting { accounting, .. } => self
                    .runtime
                    .session_usage
                    .record(accounting.provider_usage.as_ref().map(|usage| &usage.components)),
                session::SessionRecord::Usage { input_tokens, output_tokens, .. } if !has_request_accounting => {
                    self.runtime
                        .session_usage
                        .record(Some(&thndrs_agent::ProviderUsageComponents::new(
                            *input_tokens,
                            *output_tokens,
                        )));
                }
                _ => {}
            }
        }
        self.session.turn_count = turn_count;
        self.composer.last_input = None;
        self.transcript.pending_manual_compaction = None;
        self.composer.queue = restore_queue_state(&records);
        self.overlay.close();
        self.runtime.run_state = RunState::Idle;
        self.composer.input.clear();
        self.composer.history_cursor = None;
        self.composer.history_draft.clear();
        self.transcript
            .entries
            .push(Entry::Status { text: format!("resumed session: {id}") });
        self.refresh_context_ledger(None);
        self.schedule_session_collection();
        Ok(())
    }

    /// Append a display-name change without changing the active session identity.
    pub fn rename_session(&mut self, name: &str) -> io::Result<()> {
        let writer =
            self.session.writer.as_mut().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "cannot name a session in ephemeral mode")
            })?;
        writer.append_rename(name)?;
        let title = session::SessionReader::read_title(writer.path());
        self.transcript
            .entries
            .push(Entry::Status { text: format!("session named: {title}") });
        Ok(())
    }

    /// Show short-lived feedback in the status bar.
    pub fn show_status_toast(&mut self, text: impl Into<String>, kind: StatusToastKind) {
        let tick_ms = self.runtime.cli.tick_rate_ms.max(1);
        let timeout_ticks = STATUS_TOAST_TIMEOUT_MS.div_ceil(tick_ms).max(1);
        self.runtime.status_toast = Some(StatusToast {
            text: text.into(),
            kind,
            expires_at_tick: self.runtime.ui_tick.wrapping_add(timeout_ticks),
        });
    }

    /// Whether a compaction turn is currently in flight.
    ///
    /// Used by the preflight gate to avoid re-triggering auto-compaction while
    /// the configured-model summary request is the active turn.
    pub fn compaction_in_flight(&self) -> bool {
        self.transcript.pending_manual_compaction.is_some()
    }

    /// Return the local bounded artifact store when this run persists a session.
    ///
    /// The store is deliberately separate from JSONL so session records carry
    /// metadata and handles without making artifact bodies part of replay truth.
    pub fn artifact_store(&self) -> Option<crate::artifacts::ArtifactStore> {
        (!self.session.run_persistence.is_ephemeral())
            .then(|| crate::artifacts::ArtifactStore::new(self.session_directory().join("artifacts")))
    }

    /// Whether this run avoids creating a local session and per-session files.
    pub const fn is_ephemeral(&self) -> bool {
        self.session.run_persistence.is_ephemeral()
    }

    /// Return the label shown by interactive surfaces for the current run.
    pub fn run_label(&self) -> &str {
        if self.is_ephemeral() {
            "ephemeral"
        } else if self.session.id.is_empty() {
            "thndrs"
        } else {
            &self.session.id
        }
    }

    /// Render measured provider consumption separately from account capacity.
    pub fn usage_status(&self) -> String {
        let Some(accounting) = &self.session.last_request_accounting else {
            return format!(
                "usage\nrequest consumption: input unknown · fresh unknown · output unknown · reasoning unknown · cache read unknown · cache write unknown\nrequest measurement provenance: unknown\nrequest cost: unknown\n{}\n{}",
                session_usage_line(&self.runtime.session_usage),
                account_capacity_line(self.runtime.codex_usage.as_ref())
            );
        };
        let Some(usage) = accounting.provider_usage.as_ref() else {
            return format!(
                "usage\nrequest: {} attempt {}\nrequest consumption: input unknown · fresh unknown · output unknown · reasoning unknown · cache read unknown · cache write unknown\nrequest measurement provenance: unknown (provider did not report usage)\nrequest cost: unknown\n{}\n{}",
                accounting.request_id,
                accounting.attempt,
                session_usage_line(&self.runtime.session_usage),
                account_capacity_line(self.runtime.codex_usage.as_ref())
            );
        };
        let fresh_input = fresh_input_tokens(usage);
        format!(
            "usage\nrequest: {} attempt {}\nrequest consumption (provider-reported by {}): input {} · fresh {} · output {} · reasoning {} · cache read {} · cache write {}\nnormalized input: {} (derived {}/{})\nrequest cost: unknown\n{}\n{}",
            accounting.request_id,
            accounting.attempt,
            usage.provider,
            display_token(usage.components.input_tokens),
            display_token(fresh_input),
            display_token(usage.components.output_tokens),
            display_token(usage.components.reasoning_tokens),
            display_token(usage.components.cache_read_input_tokens),
            display_token(usage.components.cache_creation_input_tokens),
            display_token(usage.inclusive_input_tokens.value),
            usage.rule.label(),
            thndrs_agent::USAGE_NORMALIZATION_VERSION,
            session_usage_line(&self.runtime.session_usage),
            account_capacity_line(self.runtime.codex_usage.as_ref())
        )
    }

    /// Build the compact self-knowledge snapshot used by the startup display.
    pub fn self_knowledge_snapshot(&self) -> internals::SelfKnowledgeSnapshot {
        let tools = tools::tool_definitions();
        let provider = internals::ProviderSnapshot::new(
            provider_label(&self.runtime.model),
            &self.runtime.model,
            self.runtime.websearch,
        );
        let runtime = internals::RuntimeSnapshot::new(
            provider,
            self.runtime.cwd.display().to_string(),
            internals::RENDERER_MODE,
            tools.iter().map(|tool| tool.name.to_string()).collect(),
        );
        let references = internals::ReferenceSnapshot::from_skills(&self.transcript.skills);
        let prompt_context = internals::PromptContextSnapshot::new(
            prompt::default_fragments()
                .into_iter()
                .map(|fragment| fragment.name.to_string())
                .collect(),
            &self.transcript.context_sources,
        );
        let inventory = internals::KnowledgeInventorySnapshot::new(references, prompt_context);
        let mut diagnostics: Vec<String> = self
            .transcript
            .skill_diagnostics
            .iter()
            .map(skills::SkillDiagnostic::summary)
            .collect();
        diagnostics.extend(
            self.transcript
                .prompt_template_diagnostics
                .iter()
                .map(prompt::templates::PromptTemplateDiagnostic::summary),
        );
        diagnostics.extend(self.session.config_diagnostics.iter().cloned());
        diagnostics.extend(self.session.mcp_config_diagnostics.iter().cloned());
        internals::SelfKnowledgeSnapshot::new(
            internals::AppIdentitySnapshot::default(),
            runtime,
            inventory,
            diagnostics,
        )
    }

    /// Derive the precise, user-facing state shown by interactive surfaces.
    pub fn status_label(&self) -> String {
        if self.compaction_in_flight() {
            return "Compacting".to_string();
        }
        match self.runtime.run_state {
            RunState::Working if self.runtime.provider_retry.is_some() => {
                self.runtime.provider_retry.clone().unwrap_or_default()
            }
            RunState::Working => match self.transcript.entries.last() {
                Some(Entry::Tool { name, arguments, status: ToolStatus::Running, .. }) => {
                    running_tool_status(name, arguments)
                }
                Some(Entry::Tool { status: ToolStatus::Cancelled, .. }) => "Stopped".to_string(),
                _ => "Working".to_string(),
            },
            RunState::Stopping => "Stopping".to_string(),
            RunState::Error(_) => "Failed".to_string(),
            RunState::Idle => match self.transcript.entries.last() {
                Some(Entry::Status { text }) if text == "cancelled" => "Stopped".to_string(),
                _ => match self.last_non_status_entry() {
                    Some(Entry::Error { .. }) => "Failed".to_string(),
                    Some(Entry::Tool { status: ToolStatus::Failed, .. }) => "Failed".to_string(),
                    Some(Entry::Tool { status: ToolStatus::Cancelled, .. }) => "Stopped".to_string(),
                    Some(Entry::Agent { streaming: false, .. }) | Some(Entry::Tool { status: ToolStatus::Ok, .. }) => {
                        "Ready".to_string()
                    }
                    _ => "Ready".to_string(),
                },
            },
        }
    }

    /// Render secondary runtime telemetry for the `/status` inspection command.
    pub fn runtime_status(&self) -> String {
        let account_capacity = self
            .runtime
            .codex_usage
            .as_ref()
            .and_then(codex::CodexUsageStatus::compact_status)
            .unwrap_or_else(|| "unavailable".to_string());
        let git = self
            .runtime
            .git_status
            .as_ref()
            .map_or_else(|| "unavailable".to_string(), GitStatusSummary::display);
        format!(
            "state: {}\nmodel: {}\nreasoning: {}\nsearch: {}\naccount capacity: {}\ngit: {}\nworkspace: {}",
            self.status_label(),
            codex::display_model_id(&self.runtime.model),
            self.runtime.cli.reasoning_effort.label(),
            self.runtime.websearch.label(),
            account_capacity,
            git,
            self.runtime.cwd.display()
        )
    }

    /// Derive the prompt UI state from `run_state` and the transcript.
    pub fn prompt_state(&self) -> PromptState {
        match self.runtime.run_state {
            RunState::Working => match self.transcript.entries.last() {
                Some(Entry::Reasoning { streaming: true, .. }) => PromptState::Streaming,
                Some(Entry::Agent { streaming: true, .. }) => PromptState::Streaming,
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => PromptState::RunningTool,
                _ => PromptState::Submitted,
            },
            RunState::Stopping => PromptState::Stopped,
            RunState::Error(_) => PromptState::Errored,
            RunState::Idle => match self.transcript.entries.last() {
                Some(Entry::Status { text }) if text == "cancelled" => PromptState::Stopped,
                _ => match self.last_non_status_entry() {
                    Some(Entry::Error { .. }) => PromptState::Errored,
                    _ => PromptState::Editable,
                },
            },
        }
    }

    fn last_non_status_entry(&self) -> Option<&Entry> {
        self.transcript
            .entries
            .iter()
            .rev()
            .find(|entry| !matches!(entry, Entry::Status { .. }))
            .or_else(|| self.transcript.entries.last())
    }

    fn refresh_git_status(&mut self) {
        self.runtime.git_status = git::collect(&self.runtime.cwd);
    }

    fn session_directory(&self) -> PathBuf {
        self.runtime
            .cli
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&self.runtime.cwd))
    }

    fn schedule_session_collection(&self) {
        if self.is_ephemeral() {
            return;
        }
        let sessions_dir = self.session_directory();
        let workspace_root = self.runtime.cwd.clone();
        let policy = self.runtime.cli.session_retention.clone();
        let active_session_id = self.session.id.clone();
        thread::spawn(move || {
            let _ = session::collect_if_due(&sessions_dir, &workspace_root, &policy, Some(&active_session_id));
        });
    }

    /// Resolve the configured compaction policy from loaded config layers.
    pub fn effective_compaction_policy(&self) -> CompactionPolicy {
        let config = self
            .runtime
            .cli
            .config_layers
            .iter()
            .map(|layer| &layer.config.context.compaction)
            .rev()
            .find(|config| **config != CompactionConfig::default())
            .cloned()
            .unwrap_or_default();
        CompactionPolicy::from_config(&config)
    }

    /// Resolve the independent model-projection reducers from loaded config
    /// layers, preserving the parsed CLI snapshot when no layer overrides it.
    pub fn effective_model_reduction(&self) -> ReductionConfig {
        self.runtime
            .cli
            .config_layers
            .iter()
            .map(|layer| &layer.config.context.reduction)
            .rev()
            .find(|config| **config != ReductionConfig::default())
            .cloned()
            .unwrap_or_else(|| self.runtime.cli.context.reduction.clone())
    }
}

fn available_session_id(session_directory: &Path) -> String {
    let base = session::generate_session_id();
    let mut suffix = 1_u64;
    loop {
        let candidate = if suffix == 1 { base.clone() } else { format!("{base}-{suffix}") };
        if !session_directory.join(format!("{candidate}.jsonl")).exists() {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn restore_queue_state(records: &[session::SessionRecord]) -> QueueState {
    let mut queue = QueueState::default();
    for record in records {
        let session::SessionRecord::QueuedInput { seq, time, queue_id, kind, action, text, .. } = record else {
            continue;
        };
        let id = QueueItemId(if *queue_id == 0 { *seq } else { *queue_id });
        let target = if kind == "steering" { QueueTarget::Steering } else { QueueTarget::FollowUp };
        if action == "add" || queue.item(id).is_none() {
            queue.items.push(QueueItem {
                id,
                target,
                text: text.clone(),
                created_at: time.clone(),
                audit: QueueAuditState::Recorded,
                settlement: QueueSettlement::Pending,
            });
            continue;
        }
        match action.as_str() {
            "edit" => {
                if let Some(item) = queue.item_mut(id) {
                    item.text = text.clone();
                }
            }
            "retarget" | "send-after-step" | "send-now" => {
                if let Some(item) = queue.item_mut(id) {
                    item.target = target;
                }
            }
            "reorder-up" => {
                if let Some(index) = queue.items.iter().position(|item| item.id == id)
                    && index > 0
                {
                    queue.items.swap(index, index - 1);
                }
            }
            "reorder-down" => {
                if let Some(index) = queue.items.iter().position(|item| item.id == id)
                    && index + 1 < queue.items.len()
                {
                    queue.items.swap(index, index + 1);
                }
            }
            "sent" => {
                queue.settle(id, QueueSettlement::Sent);
            }
            "cancelled" => {
                queue.settle(id, QueueSettlement::Cancelled);
            }
            "deleted" => {
                queue.settle(id, QueueSettlement::Deleted);
            }
            _ => {}
        }
    }
    queue.restore_next_id();
    queue
}

fn running_tool_status(name: &str, arguments: &str) -> String {
    let name = name.split('#').next().unwrap_or(name);
    if name == "run_shell"
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments)
    {
        let command = if let Some(argv) = value.get("argv").and_then(serde_json::Value::as_array) {
            argv.iter()
                .filter_map(serde_json::Value::as_str)
                .take(2)
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            let program = value
                .get("program")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let argument = value
                .get("args")
                .and_then(serde_json::Value::as_array)
                .and_then(|args| args.first())
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            [program, argument]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        if !command.is_empty() {
            return format!("Running {}", utils::truncate_ellipsis(&command, 40));
        }
    }
    format!("Running {}", name.replace('_', " "))
}

fn display_token(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn fresh_input_tokens(usage: &thndrs_agent::ProviderUsage) -> Option<u64> {
    usage.fresh_input_tokens()
}

fn session_usage_line(usage: &SessionUsage) -> String {
    format!(
        "session consumption: requests {} · input {} · output {} · reasoning {} · cache read {} · cache write {}\nsession measurement provenance: provider-reported where known; unknown otherwise\nsession cost: unknown",
        usage.request_count,
        display_token(usage.input_tokens),
        display_token(usage.output_tokens),
        display_token(usage.reasoning_tokens),
        display_token(usage.cache_read_input_tokens),
        display_token(usage.cache_creation_input_tokens),
    )
}

fn account_capacity_line(usage: Option<&codex::CodexUsageStatus>) -> String {
    usage.and_then(codex::CodexUsageStatus::compact_status).map_or_else(
        || "account capacity: unavailable\ncapacity refresh: no provider observation".to_string(),
        |status| {
            format!(
                "account capacity (provider-reported): {status}\ncapacity refresh: observed in latest provider response"
            )
        },
    )
}

/// Apply one message without performing terminal, process, or agent I/O.
pub fn update_with_effects(app: &mut App, msg: &Msg) -> UpdateResult {
    let previous_run_state = app.runtime.run_state.clone();
    let previous_request = app.runtime.active_effect_request.clone();
    let follow_up = match msg {
        Msg::Action(action) => input::handle_action(app, action.clone()),
        Msg::Key(key) => {
            let input = TerminalInput::Key(*key);
            translate_input(app, input)
                .into_iter()
                .find_map(|action| input::handle_action(app, action))
        }
        Msg::Mouse(mouse) => {
            let Some(input) = TerminalInput::from_event(crossterm::event::Event::Mouse(*mouse)) else {
                return UpdateResult::default();
            };
            translate_input(app, input)
                .into_iter()
                .find_map(|action| input::handle_action(app, action))
        }
        Msg::Quit => {
            app.runtime.quit = true;
            None
        }
        Msg::Tick => {
            app.runtime.ui_tick = app.runtime.ui_tick.wrapping_add(1);
            agent_lifecycle::refresh_foreground_output(app);
            if let Some(deadline) = app.runtime.ctrl_d_pending
                && agent_lifecycle::now_or_after_deadline(app.runtime.ui_tick, deadline)
            {
                app.runtime.ctrl_d_pending = None;
            }
            if app
                .runtime
                .status_toast
                .as_ref()
                .is_some_and(|toast| agent_lifecycle::now_or_after_deadline(app.runtime.ui_tick, toast.expires_at_tick))
            {
                app.runtime.status_toast = None;
            }
            agent_lifecycle::finish_stopping_if_due(app);
            poll_chatgpt_oauth_on_tick(app);
            None
        }
        Msg::Clear => {
            app.transcript.entries.clear();
            app.overlay.close_detail();
            None
        }
        Msg::Agent(event) => agent_lifecycle::handle_agent_event(app, event.clone()),
        Msg::Effect(result) => match result {
            EffectResult::Agent { request, event } if app.runtime.active_effect_request.as_ref() == Some(request) => {
                agent_lifecycle::handle_agent_event(app, event.clone())
            }
            EffectResult::Agent { .. } => None,
            EffectResult::BackgroundProcesses(results) => {
                agent_lifecycle::record_background_results(app, results.clone());
                None
            }
            EffectResult::Failed { request, operation, error }
                if request.is_none() || request.as_ref() == app.runtime.active_effect_request.as_ref() =>
            {
                app.transcript
                    .entries
                    .push(Entry::Error { text: format!("{operation} failed: {error}") });
                None
            }
            EffectResult::Failed { .. } => None,
        },
        Msg::GitStatusChanged(status) => {
            app.runtime.git_status = status.clone();
            None
        }
    };

    let mut effects = Vec::new();
    match msg {
        Msg::Action(Action::Suspend) => effects.push(Effect::SuspendTerminal),
        Msg::Clear => effects.push(Effect::ClearTerminal),
        Msg::Quit => effects.push(Effect::ShutdownProcesses),
        Msg::Tick => effects.push(Effect::DrainBackgroundProcesses),
        _ => {}
    }

    if previous_run_state != RunState::Working && app.runtime.run_state == RunState::Working {
        let request = EffectRequest { session_id: app.session.id.clone(), turn: app.session.turn_count };
        app.runtime.active_effect_request = Some(request.clone());
        effects.push(Effect::StartAgent(request));
    } else if previous_run_state == RunState::Working && app.runtime.run_state == RunState::Stopping {
        if let Some(request) = previous_request.clone() {
            effects.push(Effect::CancelAgent(request));
        }
    } else if matches!(previous_run_state, RunState::Working | RunState::Stopping)
        && matches!(app.runtime.run_state, RunState::Idle | RunState::Error(_))
    {
        if let Some(request) = previous_request {
            effects.push(Effect::SettleAgent(request));
        }
        app.runtime.active_effect_request = None;
    }

    UpdateResult { follow_up, effects }
}

/// Compatibility projection for tests and adapters that only consume pure
/// follow-up messages. Interactive callers should use [`update_with_effects`].
pub fn update(app: &mut App, msg: &Msg) -> Option<Msg> {
    update_with_effects(app, msg).follow_up
}

fn default_user_label() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .map(|name| format!("User ({name})"))
        .unwrap_or_else(|_| String::from("You"))
}

fn is_verbose_status(text: &str) -> bool {
    text.starts_with("provider:")
        || text.starts_with("model: ")
        || text.starts_with("logs  ")
        || text.starts_with("tool budget:")
}

/// Translate the fixed Ctrl+D confirmation window to the configured tick cadence.
///
/// This keeps the user-visible timeout stable when a faster render cadence is
/// selected for smoother streaming output.
fn quit_confirm_timeout_ticks(app: &App) -> u64 {
    let tick_ms = app.runtime.cli.tick_rate_ms.max(1);
    QUIT_CONFIRM_TIMEOUT_MS / tick_ms + u64::from(!QUIT_CONFIRM_TIMEOUT_MS.is_multiple_of(tick_ms))
}

#[cfg(test)]
mod overlay_tests {
    use super::*;

    #[test]
    fn overlay_transitions_keep_one_focused_surface() {
        let mut overlay = OverlayState::default();
        assert_eq!(overlay.accessory(), PromptAccessory::None);
        assert!(overlay.picker().is_none());
        assert!(overlay.setup().is_none());
        assert!(!overlay.is_detail());

        overlay.show_help();
        assert_eq!(overlay.accessory(), PromptAccessory::Help);
        assert!(
            overlay
                .show_picker(PromptAccessory::Help, PickerState::new(Vec::new(), 8))
                .is_err()
        );
        assert_eq!(overlay.accessory(), PromptAccessory::Help);

        let _ = overlay.show_picker(
            PromptAccessory::Files(FilePickerSource::Forced),
            PickerState::new(vec![PickerItem::new("README.md", "")], 8),
        );
        assert_eq!(overlay.accessory(), PromptAccessory::Files(FilePickerSource::Forced));
        assert!(overlay.picker().is_some());
        assert!(overlay.setup().is_none());
        assert!(!overlay.is_detail());

        overlay.show_setup(FirstRunRecovery::setup(SetupProviderArg::ChatgptCodex));
        assert_eq!(overlay.accessory(), PromptAccessory::None);
        assert!(overlay.setup().is_some());
        assert!(overlay.picker().is_none());
        assert!(!overlay.is_detail());

        overlay.show_detail(3);
        assert!(!overlay.is_detail());
        assert!(overlay.setup().is_some());
        assert!(overlay.picker().is_none());

        overlay.close();
        overlay.show_detail(3);
        assert!(overlay.is_detail());
        overlay.close_detail();
        assert!(!overlay.is_detail());
    }

    #[test]
    fn command_selection_is_mutated_on_the_focused_overlay() {
        let mut overlay = OverlayState::default();
        overlay.show_commands();
        *overlay.command_selected_mut().expect("command overlay") = 2;
        assert_eq!(overlay.accessory(), PromptAccessory::Commands { selected: 2 });
    }
}
