//! Application state, message types, and the `update` function.
//!
//! This follows the Elm architecture (TEA):
//!
//! `update(&mut App, Msg) -> Option<Msg>` is the only mutation path.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};

use crate::acp::config::provider_label;
use crate::acp::permissions::{PendingPermission, PermissionDecision};
use crate::cancel::CancelToken;
use crate::cli::commands::auth::CredentialScope;
use crate::cli::commands::setup::SetupProviderArg;
use crate::cli::{Cli, Theme, WebSearchMode};
use crate::input::PromptInput;
use crate::providers::{codex, opencode, umans};
use crate::renderer::git::GitStatusSummary;
use crate::thndrs_core::auth;
use crate::tools::shell::ProcessRegistry;
use crate::{context, fuzzy, internals, prompt, session, skills, tools};
use crate::{mcp, renderer};

/// Number of UI ticks the user has to press Ctrl+D a second time before the
/// quit confirmation expires and a fresh double-press is needed.
///
/// With the default 100 ms tick rate this is roughly 3 seconds.
const QUIT_CONFIRM_TIMEOUT_TICKS: u64 = 30;

pub const VISIBLE_ROWS: usize = 8;

/// Shared cap for large filesystem-backed picker inventories so fuzzy matching
/// stays responsive while still surfacing enough nearby files or skills.
const LARGE_PICKER_LIMIT: usize = 200;
const MODEL_PICKER_LIMIT: usize = 50;

/// Top-level interaction mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Mode {
    /// Normal prompt entry.
    #[default]
    Prompt,
    /// Slash-command entry, entered with `:`.
    Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum PromptAccessory {
    #[default]
    None,
    Help,
    Commands {
        selected: usize,
    },
    Files(FilePickerSource),
    Models,
    Skills,
}

/// Focused first-run and credential recovery surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstRunRecovery {
    /// Provider being configured or diagnosed.
    pub provider: Option<SetupProviderArg>,
    /// Current recovery step.
    pub stage: RecoveryStage,
    /// Whether a prompt submit is waiting on this recovery.
    pub pending_provider_prompt: bool,
    /// Selected action row.
    pub selected: usize,
    /// Hidden API-key buffer. This is never rendered or written to transcripts.
    pub secret_input: String,
}

impl FirstRunRecovery {
    fn missing_provider(provider: SetupProviderArg, pending_provider_prompt: bool) -> Self {
        Self {
            provider: Some(provider),
            stage: RecoveryStage::MissingCredential,
            pending_provider_prompt,
            selected: 0,
            secret_input: String::new(),
        }
    }

    fn acp_missing(pending_provider_prompt: bool) -> Self {
        Self {
            provider: None,
            stage: RecoveryStage::AcpMissing,
            pending_provider_prompt,
            selected: 0,
            secret_input: String::new(),
        }
    }

    fn login(provider: SetupProviderArg) -> Self {
        Self {
            provider: Some(provider),
            stage: if provider == SetupProviderArg::ChatgptCodex {
                RecoveryStage::Instructions
            } else {
                RecoveryStage::EnterKey
            },
            pending_provider_prompt: false,
            selected: 0,
            secret_input: String::new(),
        }
    }

    fn logout(provider: SetupProviderArg) -> Self {
        Self {
            provider: Some(provider),
            stage: RecoveryStage::LogoutConfirm,
            pending_provider_prompt: false,
            selected: 0,
            secret_input: String::new(),
        }
    }
}

/// Step within the first-run recovery surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryStage {
    /// Selected provider is missing an API-key credential.
    MissingCredential,
    /// Hidden API-key entry is active.
    EnterKey,
    /// Select global/project storage before writing the key.
    ConfirmStore,
    /// Show setup instructions in a focused surface.
    Instructions,
    /// Confirm logout and storage scope.
    LogoutConfirm,
    /// ACP model recovery, separate from provider API-key setup.
    AcpMissing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilePickerSource {
    Forced,
    Mention { token_start: usize },
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
    pub(crate) fn set_pending_for_test(&mut self) {
        self.pending_since = Some(Instant::now());
    }

    #[cfg(test)]
    pub(crate) fn set_last_completed_for_test(&mut self, duration: Duration) {
        self.pending_since = None;
        self.last_completed = Some(duration);
    }
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

/// Status of a tool entry in the transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool started, not yet finished.
    #[default]
    Running,
    /// Tool finished successfully.
    Ok,
    /// Tool failed.
    Failed,
    /// Tool was cancelled while running (e.g. the user interrupted the run).
    Cancelled,
}

impl ToolStatus {
    /// Unicode icon & label used in session-record transcript entries for file writes.
    pub fn icon(&self) -> &'static str {
        match self {
            ToolStatus::Ok => "✓ wrote",
            ToolStatus::Failed => "✕ write failed",
            ToolStatus::Running => "⠋ writing",
            ToolStatus::Cancelled => "✕ write cancelled",
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
    /// Index into `app.transcript` for the expanded tool entry.
    pub entry_index: usize,
    /// Scroll offset: number of rendered output rows skipped from the top.
    pub scroll: usize,
    /// Whether the detail pane is currently open.
    pub open: bool,
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

/// One transcript row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    /// User-submitted text.
    User { text: String },
    /// Agent text, possibly still streaming.
    Agent { text: String, streaming: bool },
    /// Reasoning/thinking text, kept separate from final assistant text.
    Reasoning { text: String, streaming: bool },
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
        /// Boxed to avoid a large enum variant (ProcessResult carries multiple Vec<String>).
        shell_result: Option<Box<tools::shell::ProcessResult>>,
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

/// The single message type fed into `update`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Msg {
    /// A raw key event from the terminal.
    Key(crossterm::event::KeyEvent),
    /// A raw mouse event from the terminal.
    Mouse(crossterm::event::MouseEvent),
    /// Periodic tick.
    Tick,
    /// Clear the transcript.
    Clear,
    /// Quit the app.
    Quit,
    /// An agent stream event.
    Agent(AgentEvent),
    /// Updated git working tree summary from the background watcher.
    GitStatusChanged(Option<GitStatusSummary>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum KeyOutcome {
    Unhandled,
    Handled,
    Followup(Msg),
}

impl KeyOutcome {
    fn with(followup: Option<Msg>) -> Self {
        match followup {
            Some(msg) => Self::Followup(msg),
            None => Self::Handled,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
}

impl PickerItem {
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { label: label.into(), detail: detail.into() }
    }

    fn searchable(&self) -> String {
        if self.detail.is_empty() { self.label.clone() } else { format!("{} {}", self.label, self.detail) }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerState {
    pub query: String,
    pub all_items: Vec<PickerItem>,
    pub matches: Vec<PickerItem>,
    /// Character indices of fuzzy match highlights, parallel to `matches`.
    pub match_indices: Vec<Vec<usize>>,
    pub selected: usize,
    pub scroll: usize,
    limit: usize,
}

impl PickerState {
    pub fn new(all_items: Vec<PickerItem>, limit: usize) -> Self {
        let (matches, match_indices) = split_filter_items(&all_items, "", limit);
        Self { query: String::new(), all_items, matches, match_indices, selected: 0, scroll: 0, limit }
    }

    fn refresh_matches(&mut self) {
        let (matches, match_indices) = split_filter_items(&self.all_items, &self.query, self.limit);
        self.matches = matches;
        self.match_indices = match_indices;
        self.selected = self.selected.min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn move_up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn move_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(VISIBLE_ROWS);
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        self.selected = (self.selected + VISIBLE_ROWS).min(self.matches.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    pub fn selected(&self) -> Option<&PickerItem> {
        self.matches.get(self.selected)
    }

    fn ensure_selected_visible(&mut self) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VISIBLE_ROWS {
            self.scroll = self.selected.saturating_sub(VISIBLE_ROWS - 1);
        }
    }
}

/// The full application state used to draw the screen.
#[derive(Debug)]
pub struct App {
    /// Snapshot of the effective CLI config used by command-like TUI flows.
    pub cli: Cli,
    pub session_id: String,
    pub mode: Mode,
    pub run_state: RunState,
    pub input: PromptInput,
    /// Submitted prompt history for Up/Down recall.
    pub input_history: Vec<String>,
    /// Current index into `input_history` while navigating history.
    pub history_cursor: Option<usize>,
    /// Draft input captured before history navigation starts.
    pub history_draft: String,
    pub transcript: Vec<Entry>,
    pub cwd: PathBuf,
    /// Current git working tree summary for the status line.
    pub git_status: Option<GitStatusSummary>,
    pub model: String,
    pub model_picker_items: Vec<PickerItem>,
    pub user_label: String,
    pub websearch: WebSearchMode,
    /// UI color theme.
    pub theme: Theme,
    /// Whether diagnostic provider/log status rows should be shown in transcript.
    pub verbose: bool,
    /// Provider token usage accumulated for this session.
    pub session_tokens_in: u64,
    pub session_tokens_out: u64,
    /// In-memory client-observed TTFT for the active and last completed turn.
    pub ttft: TurnTtftState,
    /// Loaded context sources (e.g. AGENTS.md).
    pub context_sources: Vec<context::ContextSource>,
    /// Discovered Agent Skills metadata.
    pub skills: Vec<skills::SkillMetadata>,
    /// Skill discovery diagnostics for ignored malformed skills.
    pub skill_diagnostics: Vec<skills::SkillDiagnostic>,
    /// Monotonic UI tick used for lightweight animated affordances.
    pub ui_tick: u64,
    /// When `Some`, the user pressed Ctrl+D once and we are waiting for a
    /// second press within [`QUIT_CONFIRM_TIMEOUT_TICKS`] ticks to actually
    /// quit. The value is the tick deadline at which the pending confirmation
    /// expires.
    pub ctrl_d_pending: Option<u64>,
    /// Append-only session writer. `None` when persistence is disabled
    /// (e.g. the sessions directory is not writable).
    pub session_writer: Option<session::SessionWriter>,
    /// Monotonic turn counter for session record correlation.
    pub turn_count: u64,
    /// Registry of background processes started via `run_shell`.
    pub process_registry: ProcessRegistry,
    /// The last submitted prompt text, retained so it can be restored on
    /// provider failure. Cleared on successful completion.
    pub last_input: Option<String>,
    /// Current target for input submitted while the agent is running.
    pub queue_target: QueueTarget,
    /// Active fuzzy picker state, used by file and model pickers.
    pub picker: Option<PickerState>,
    /// Inline prompt accessory rendered above the input.
    pub prompt_accessory: PromptAccessory,
    /// Focused first-run or credential recovery surface.
    pub first_run_recovery: Option<FirstRunRecovery>,
    /// Steering messages waiting to be sent to the active agent thread.
    pub queued_steering: Vec<String>,
    /// Follow-up prompts to submit as new turns after the active run completes.
    pub queued_followups: Vec<String>,
    /// Kill-ring for readline-style yank (Ctrl+Y).
    pub kill_ring: Vec<String>,
    /// Scrollable detail pane for inspecting full tool output.
    pub detail_pane: DetailPane,
    /// One pending ACP permission request, if an external agent is blocked on a user decision.
    pub pending_permission: Option<PendingPermission>,
    /// Non-fatal config diagnostics from effective config loading. Surfaced in
    /// verbose startup rows and prompt inspection.
    pub config_diagnostics: Vec<String>,
    /// MCP config files captured when the session audit metadata was written.
    pub mcp_config_files: Vec<session::SessionConfigFile>,
    /// Non-fatal MCP config diagnostics from the latest MCP config audit load.
    pub mcp_config_diagnostics: Vec<String>,
    /// When true the loop should stop and the app exit.
    pub quit: bool,
}

impl From<&Cli> for App {
    fn from(value: &Cli) -> Self {
        let workspace_root = context::discover_workspace_root(&value.cwd);
        let mut cli_snapshot = value.clone();
        cli_snapshot.cwd = workspace_root.clone();
        let context_sources = match context::load_agents_md(&workspace_root) {
            Some(source) => vec![source],
            None => Vec::new(),
        };
        let skill_inventory = skills::discover(&workspace_root, &value.skill_dirs);

        let transcript = Vec::new();
        let sessions_dir = value
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&workspace_root));
        let session_id = session::generate_session_id();
        let (mcp_config_files, mcp_config_diagnostics) = load_mcp_config_audit(&workspace_root);

        let config_meta = {
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
            Some(session::SessionConfigMeta {
                session_dir,
                files,
                origins,
                diagnostics,
                mcp_files: mcp_config_files.clone(),
                mcp_diagnostics: mcp_config_diagnostics.clone(),
            })
        };

        let mut session_writer = session::SessionWriter::create(
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
        .ok();

        if let Some(ref mut writer) = session_writer.as_mut()
            && !context_sources.is_empty()
        {
            let _ = writer.append_context(&context_sources);
        }

        App {
            cli: cli_snapshot,
            session_id,
            mode: Mode::default(),
            run_state: RunState::default(),
            input: PromptInput::new(),
            input_history: Vec::new(),
            history_cursor: None,
            history_draft: String::new(),
            transcript,
            git_status: renderer::git::collect(&workspace_root),
            cwd: workspace_root,
            model: value.model.clone(),
            model_picker_items: offline_model_picker_items(),
            user_label: default_user_label(),
            websearch: value.websearch,
            theme: value.theme,
            verbose: value.verbose,
            session_tokens_in: 0,
            session_tokens_out: 0,
            ttft: TurnTtftState::default(),
            context_sources,
            skills: skill_inventory.skills,
            skill_diagnostics: skill_inventory.diagnostics,
            ui_tick: 0,
            ctrl_d_pending: None,
            session_writer,
            turn_count: 0,
            process_registry: ProcessRegistry::new(),
            last_input: None,
            queue_target: QueueTarget::default(),
            picker: None,
            prompt_accessory: PromptAccessory::None,
            first_run_recovery: None,
            queued_steering: Vec::new(),
            queued_followups: Vec::new(),
            kill_ring: Vec::new(),
            detail_pane: DetailPane::default(),
            pending_permission: None,
            config_diagnostics: value.config_diagnostics.clone(),
            mcp_config_files,
            mcp_config_diagnostics,
            quit: false,
        }
    }
}

impl App {
    /// Build the initial app from parsed CLI args.
    ///
    /// Discovers the workspace root from `--cwd` (preferring the git root), loads root
    /// `AGENTS.md` if present, and records context source metadata in the session.
    pub fn from_cli(cli: &Cli) -> Self {
        cli.into()
    }

    /// Build the compact self-knowledge snapshot used by the startup display.
    pub fn self_knowledge_snapshot(&self) -> internals::SelfKnowledgeSnapshot {
        let tools = tools::tool_definitions();
        let provider = internals::ProviderSnapshot::new(provider_label(&self.model), &self.model, self.websearch);
        let runtime = internals::RuntimeSnapshot::new(
            provider,
            self.cwd.display().to_string(),
            internals::RENDERER_MODE,
            tools.iter().map(|tool| tool.name.to_string()).collect(),
        );
        let references = internals::ReferenceSnapshot::from_skills(&self.skills);
        let prompt_context = internals::PromptContextSnapshot::new(
            prompt::default_fragments()
                .into_iter()
                .map(|fragment| fragment.name.to_string())
                .collect(),
            &self.context_sources,
        );
        let inventory = internals::KnowledgeInventorySnapshot::new(references, prompt_context);
        let mut diagnostics: Vec<String> = self
            .skill_diagnostics
            .iter()
            .map(skills::SkillDiagnostic::summary)
            .collect();
        diagnostics.extend(self.config_diagnostics.iter().cloned());
        diagnostics.extend(self.mcp_config_diagnostics.iter().cloned());
        internals::SelfKnowledgeSnapshot::new(
            internals::AppIdentitySnapshot::default(),
            runtime,
            inventory,
            diagnostics,
        )
    }

    /// Derive the granular status label for the status line.
    ///
    /// Maps `RunState` plus the last transcript entry into one of idle, sending,
    /// thinking, working, running tool, stopping, cancelled, failed, error, done.
    pub fn status_label(&self) -> &'static str {
        match self.run_state {
            RunState::Working => match self.transcript.last() {
                Some(Entry::Reasoning { streaming: true, .. }) => "thinking",
                Some(Entry::Agent { streaming: true, .. }) => "working",
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => "running tool",
                Some(Entry::Tool { status: ToolStatus::Cancelled, .. }) => "cancelled tool",
                Some(Entry::User { .. }) | None => "sending",
                _ => "working",
            },
            RunState::Stopping => "stopping",
            RunState::Error(_) => "failed",
            RunState::Idle => match self.transcript.last() {
                Some(Entry::Status { text }) if text == "cancelled" => "cancelled",
                _ => match self.last_non_status_entry() {
                    Some(Entry::Error { .. }) => "failed",
                    Some(Entry::Tool { status: ToolStatus::Failed, .. }) => "failed",
                    Some(Entry::Tool { status: ToolStatus::Cancelled, .. }) => "cancelled",
                    Some(Entry::Agent { streaming: false, .. }) | Some(Entry::Tool { status: ToolStatus::Ok, .. }) => {
                        "done"
                    }
                    _ => "idle",
                },
            },
        }
    }

    /// Derive the prompt UI state from `run_state` and the transcript.
    pub fn prompt_state(&self) -> PromptState {
        match self.run_state {
            RunState::Working => match self.transcript.last() {
                Some(Entry::Reasoning { streaming: true, .. }) => PromptState::Streaming,
                Some(Entry::Agent { streaming: true, .. }) => PromptState::Streaming,
                Some(Entry::Tool { status: ToolStatus::Running, .. }) => PromptState::RunningTool,
                _ => PromptState::Submitted,
            },
            RunState::Stopping => PromptState::Stopped,
            RunState::Error(_) => PromptState::Errored,
            RunState::Idle => match self.transcript.last() {
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
            .iter()
            .rev()
            .find(|entry| !matches!(entry, Entry::Status { .. }))
            .or_else(|| self.transcript.last())
    }
}

/// The only mutation path. Returns an optional follow-up message.
pub fn update(app: &mut App, msg: &Msg) -> Option<Msg> {
    match msg {
        Msg::Key(key) => handle_key(app, *key),
        Msg::Mouse(mouse) => handle_mouse(app, *mouse),
        Msg::Quit => {
            app.process_registry.cancel_all();
            app.quit = true;
            None
        }
        Msg::Tick => {
            app.ui_tick = app.ui_tick.wrapping_add(1);
            if let Some(deadline) = app.ctrl_d_pending
                && now_or_after_deadline(app.ui_tick, deadline)
            {
                app.ctrl_d_pending = None;
            }
            None
        }
        Msg::Clear => {
            app.transcript.clear();
            app.detail_pane = DetailPane::default();
            None
        }
        Msg::Agent(event) => handle_agent_event(app, event.clone()),
        Msg::GitStatusChanged(status) => {
            app.git_status = status.clone();
            None
        }
    }
}

pub fn command_suggestions_for_app(app: &App) -> Vec<(&'static str, &'static str)> {
    let query = command_query(app);
    let commands = [
        ("clear", "clear transcript"),
        ("quit", "exit app"),
        ("exit", "exit app"),
        ("help", "show help"),
        ("bg", "list background processes"),
        ("model", "switch Umans model"),
        ("skills", "browse loaded skills"),
        ("doctor", "show redacted diagnostics"),
        ("auth status", "show credential sources"),
        ("config path", "show config paths"),
        ("config show", "show redacted config"),
        ("setup", "open setup"),
        ("login", "enter provider key"),
        ("logout", "remove provider key"),
    ];
    commands
        .into_iter()
        .filter(|(cmd, _)| cmd.starts_with(&query))
        .collect()
}

/// - Ctrl+C always quits immediately, even mid-input.
/// - Ctrl+D requires a double-press: the first press shows a confirmation
///   message; the second press within [`QUIT_CONFIRM_TIMEOUT_TICKS`] ticks
///   quits. Any other key (or timeout) cancels the pending state.
/// - Printable characters append to the input buffer.
/// - Backspace removes the last character.
/// - `Enter` submits: slash commands (`/clear`, `/quit`) are routed, otherwise
///   the input is appended as [`Entry::User`] and cleared.
/// - Escape cancels an active agent stream.
/// - Up/Down recall prompt history.
fn handle_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit = true;
        return Some(Msg::Quit);
    }

    if key.code == KeyCode::Char('d')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
    {
        if let Some(deadline) = app.ctrl_d_pending
            && !now_or_after_deadline(app.ui_tick, deadline)
        {
            app.ctrl_d_pending = None;
            app.quit = true;
            return Some(Msg::Quit);
        } else {
            let deadline = app.ui_tick.wrapping_add(QUIT_CONFIRM_TIMEOUT_TICKS);
            app.ctrl_d_pending = Some(deadline);
            app.transcript
                .push(Entry::Status { text: String::from("Press CTRL+D again to quit.") });
            return None;
        }
    }

    if key.code == KeyCode::Char('t')
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && app.run_state == RunState::Working
    {
        app.queue_target = app.queue_target.toggle();
        app.transcript
            .push(Entry::Status { text: format!("queue target: {}", app.queue_target.label()) });
        return None;
    }

    app.ctrl_d_pending = None;

    if app.pending_permission.is_some() {
        return handle_permission_key(app, key);
    }

    if app.first_run_recovery.is_some() {
        return handle_first_run_key(app, key);
    }

    if app.detail_pane.open {
        return handle_detail_pane_key(app, key);
    }

    if !matches!(app.prompt_accessory, PromptAccessory::None) {
        match handle_accessory_key(app, key) {
            KeyOutcome::Unhandled => {}
            KeyOutcome::Handled => return None,
            KeyOutcome::Followup(msg) => return Some(msg),
        }
    }

    match app.mode {
        Mode::Command => handle_command_key(app, key),
        Mode::Prompt => handle_prompt_key(app, key),
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) -> Option<Msg> {
    if app.first_run_recovery.is_some() {
        return None;
    }

    match app.prompt_accessory {
        PromptAccessory::Files(_) | PromptAccessory::Models | PromptAccessory::Skills => {
            if let Some(picker) = app.picker.as_mut() {
                match mouse.kind {
                    MouseEventKind::ScrollUp => picker.move_up(),
                    MouseEventKind::ScrollDown => picker.move_down(),
                    _ => {}
                }
            }
            None
        }
        _ => None,
    }
}

fn handle_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    match app.prompt_accessory {
        PromptAccessory::Help => match key.code {
            KeyCode::Esc => {
                close_prompt_accessory(app);
                KeyOutcome::Handled
            }
            _ => KeyOutcome::Unhandled,
        },
        PromptAccessory::Commands { .. } => handle_command_accessory_key(app, key),
        PromptAccessory::Files(_) => handle_file_accessory_key(app, key),
        PromptAccessory::Models => handle_model_accessory_key(app, key),
        PromptAccessory::Skills => handle_skill_accessory_key(app, key),
        PromptAccessory::None => KeyOutcome::Unhandled,
    }
}

fn handle_command_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let count = command_suggestions_for_app(app).len();
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = selected.saturating_sub(1);
            }
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            if let PromptAccessory::Commands { selected } = &mut app.prompt_accessory {
                *selected = (*selected + 1).min(count.saturating_sub(1));
            }
            KeyOutcome::Handled
        }
        KeyCode::Enter
            if count > 0
                && !command_suggestions_for_app(app)
                    .iter()
                    .any(|(cmd, _)| *cmd == command_query(app)) =>
        {
            KeyOutcome::with(accept_command_suggestion(app))
        }
        _ => KeyOutcome::Unhandled,
    }
}

fn handle_file_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let source = match app.prompt_accessory {
        PromptAccessory::Files(source) => source,
        _ => return KeyOutcome::Unhandled,
    };
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_file_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace if source == FilePickerSource::Forced => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) if source == FilePickerSource::Forced => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

fn handle_model_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_model_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

fn handle_skill_accessory_key(app: &mut App, key: KeyEvent) -> KeyOutcome {
    let Some(picker) = app.picker.as_mut() else {
        return KeyOutcome::Unhandled;
    };
    match key.code {
        KeyCode::Esc => {
            close_prompt_accessory(app);
            KeyOutcome::Handled
        }
        KeyCode::Enter => {
            accept_skill_suggestion(app);
            KeyOutcome::Handled
        }
        KeyCode::Up => {
            picker.move_up();
            KeyOutcome::Handled
        }
        KeyCode::Down => {
            picker.move_down();
            KeyOutcome::Handled
        }
        KeyCode::PageUp => {
            picker.page_up();
            KeyOutcome::Handled
        }
        KeyCode::PageDown => {
            picker.page_down();
            KeyOutcome::Handled
        }
        KeyCode::Backspace => {
            picker.query.pop();
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        KeyCode::Char(ch) => {
            picker.query.push(ch);
            picker.refresh_matches();
            KeyOutcome::Handled
        }
        _ => KeyOutcome::Unhandled,
    }
}

/// Handle keys in Command mode: typed chars build the command buffer,
/// Enter executes, Esc/Backspace-on-empty returns to Prompt.
fn handle_command_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Prompt;
            app.input.clear();
            close_prompt_accessory(app);
            None
        }
        KeyCode::Backspace => {
            if app.input.is_empty() {
                app.mode = Mode::Prompt;
                close_prompt_accessory(app);
            } else {
                app.input.backspace();
                sync_prompt_accessory(app);
            }
            None
        }
        KeyCode::Enter => {
            let text = app.input.as_str().trim().to_string();
            app.input.clear();
            app.mode = Mode::Prompt;
            close_prompt_accessory(app);
            if text.is_empty() { None } else { handle_command(app, &text) }
        }
        KeyCode::Char(ch) => {
            app.input.insert_char(ch);
            sync_prompt_accessory(app);
            None
        }
        _ => None,
    }
}

/// Handle keys in normal Prompt mode.
///
/// Cursor keybinds:
/// - `left` / `ctrl+b`: move cursor left
/// - `right` / `ctrl+f`: move cursor right
/// - `alt+left` / `ctrl+left` / `alt+b`: move cursor word left
/// - `alt+right` / `ctrl+right` / `alt+f`: move cursor word right
/// - `home` / `ctrl+a`: move to line start
/// - `end` / `ctrl+e`: move to line end
/// - `shift+enter` / `ctrl+j`: insert newline
/// - `backspace`: delete char before cursor
/// - `delete`: delete char after cursor (forward delete)
fn handle_prompt_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    if app.pending_permission.is_some() {
        return None;
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Left | KeyCode::Char('b') => {
                app.input.cursor_word_left();
                exit_history_navigation(app);
                true
            }
            KeyCode::Right | KeyCode::Char('f') => {
                app.input.cursor_word_right();
                exit_history_navigation(app);
                true
            }
            KeyCode::Backspace => {
                let killed = app.input.kill_word_left();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('d') => {
                let killed = app.input.kill_word_right();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Left => {
                app.input.cursor_word_left();
                exit_history_navigation(app);
                true
            }
            KeyCode::Right => {
                app.input.cursor_word_right();
                exit_history_navigation(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) {
        let handled = match key.code {
            KeyCode::Char('a') => {
                app.input.cursor_to_start();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('e') => {
                app.input.cursor_to_end();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('b') => {
                app.input.cursor_left();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('f') => {
                app.input.cursor_right();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('j') => {
                exit_history_navigation(app);
                app.input.insert_char('\n');
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('k') => {
                let killed = app.input.kill_to_end_of_line();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('u') => {
                let killed = app.input.kill_to_start_of_line();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('w') => {
                let killed = app.input.kill_word_left();
                if !killed.is_empty() {
                    app.kill_ring.push(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('y') => {
                if let Some(killed) = app.kill_ring.last() {
                    app.input.yank(killed);
                }
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            KeyCode::Char('t') => {
                app.input.transpose_chars();
                exit_history_navigation(app);
                sync_prompt_accessory(app);
                true
            }
            _ => false,
        };
        if handled {
            return None;
        }
    }

    match key.code {
        KeyCode::Char('?') if app.input.is_empty() => {
            app.prompt_accessory = PromptAccessory::Help;
            None
        }
        KeyCode::Char(':') if app.input.is_empty() && matches!(app.run_state, RunState::Idle | RunState::Error(_)) => {
            app.mode = Mode::Command;
            app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
            None
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            exit_history_navigation(app);
            app.input.insert_char('\n');
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Up => {
            if !app.input.cursor_up() {
                recall_older_input(app);
            } else {
                exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Down => {
            if !app.input.cursor_down() {
                recall_newer_input(app);
            } else {
                exit_history_navigation(app);
            }
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Left => {
            app.input.cursor_left();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Right => {
            app.input.cursor_right();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Home => {
            app.input.cursor_to_start();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::End => {
            app.input.cursor_to_end();
            exit_history_navigation(app);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::PageUp | KeyCode::PageDown => None,
        KeyCode::Delete => {
            exit_history_navigation(app);
            app.input.delete_forward();
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Char(ch) => {
            exit_history_navigation(app);
            app.input.insert_char(ch);
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Backspace => {
            exit_history_navigation(app);
            app.input.backspace();
            sync_prompt_accessory(app);
            None
        }
        KeyCode::Enter => handle_submit(app),
        KeyCode::Tab => {
            toggle_detail_pane(app);
            None
        }
        KeyCode::Esc if app.run_state == RunState::Working => {
            cancel_stream(app);
            None
        }
        _ => None,
    }
}

fn command_query(app: &App) -> String {
    if app.mode == Mode::Command {
        app.input.as_str().trim_start().to_string()
    } else {
        app.input
            .as_str()
            .strip_prefix('/')
            .unwrap_or("")
            .trim_start()
            .to_string()
    }
}

fn accept_command_suggestion(app: &mut App) -> Option<Msg> {
    let suggestions = command_suggestions_for_app(app);
    if suggestions.is_empty() {
        return None;
    }
    let selected = match app.prompt_accessory {
        PromptAccessory::Commands { selected } => selected.min(suggestions.len() - 1),
        _ => 0,
    };
    let command = suggestions[selected].0;
    let replacement = if app.mode == Mode::Command { format!("{command} ") } else { format!("/{command} ") };
    app.input.set_text(&replacement);
    app.prompt_accessory = PromptAccessory::None;
    None
}

fn open_file_picker(app: &mut App, source: FilePickerSource) {
    match tools::searchable_file_paths(&app.cwd, 2_000) {
        Ok(files) => {
            let items = files.into_iter().map(|path| PickerItem::new(path, "")).collect();
            app.picker = Some(PickerState::new(items, LARGE_PICKER_LIMIT));
            app.prompt_accessory = PromptAccessory::Files(source);
            sync_file_picker_query(app);
        }
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("file picker failed: {err}") });
        }
    }
}

fn open_model_picker(app: &mut App) {
    let items = if app.model_picker_items.is_empty() {
        offline_model_picker_items()
    } else {
        app.model_picker_items.clone()
    };
    app.picker = Some(PickerState::new(items, MODEL_PICKER_LIMIT));
    app.prompt_accessory = PromptAccessory::Models;
}

fn open_skill_picker(app: &mut App) {
    for diagnostic in &app.skill_diagnostics {
        app.transcript.push(Entry::Error { text: diagnostic.summary() });
    }

    if app.skills.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("skills  none loaded") });
        return;
    }

    let items = app
        .skills
        .iter()
        .map(|skill| PickerItem::new(skill.name.clone(), skill.description.clone()))
        .collect();
    app.picker = Some(PickerState::new(items, LARGE_PICKER_LIMIT));
    app.prompt_accessory = PromptAccessory::Skills;
}

fn offline_model_picker_items() -> Vec<PickerItem> {
    umans::known_models()
        .into_iter()
        .map(|model| PickerItem::new(model.id, model.description))
        .chain(
            opencode::known_models()
                .into_iter()
                .map(|model| PickerItem::new(model.id, model.description)),
        )
        .chain(
            codex::known_models()
                .into_iter()
                .map(|model| PickerItem::new(model.id, model.description)),
        )
        .collect()
}

fn close_prompt_accessory(app: &mut App) {
    if matches!(
        app.prompt_accessory,
        PromptAccessory::Files(_) | PromptAccessory::Models | PromptAccessory::Skills
    ) {
        app.picker = None;
    }
    app.prompt_accessory = PromptAccessory::None;
}

fn provider_for_model(model: &str) -> SetupProviderArg {
    if opencode::is_zen_model_id(model) {
        SetupProviderArg::OpencodeZen
    } else if opencode::is_go_model_id(model) {
        SetupProviderArg::OpencodeGo
    } else if codex::is_model_id(model) {
        SetupProviderArg::ChatgptCodex
    } else {
        SetupProviderArg::Umans
    }
}

fn provider_authenticated(provider: SetupProviderArg, cwd: &std::path::Path) -> bool {
    if provider == SetupProviderArg::ChatgptCodex {
        return auth::resolve_chatgpt_codex_auth().is_ok();
    }
    let Some(env_var) = provider.api_key_env_var() else {
        return false;
    };
    auth::credential_source(env_var, cwd).is_some()
}

fn selected_provider_missing(app: &App) -> Option<FirstRunRecovery> {
    if let Some(acp_name) = crate::acp::config::parse_model_id(&app.model) {
        if app.cli.acp_agents.contains_key(acp_name) {
            return None;
        }
        return Some(FirstRunRecovery::acp_missing(true));
    }

    let provider = provider_for_model(&app.model);
    if !provider_authenticated(provider, &app.cwd) {
        Some(FirstRunRecovery::missing_provider(provider, true))
    } else {
        None
    }
}

fn recovery_action_count(recovery: &FirstRunRecovery) -> usize {
    match recovery.stage {
        RecoveryStage::MissingCredential if recovery.provider == Some(SetupProviderArg::ChatgptCodex) => 4,
        RecoveryStage::MissingCredential => 5,
        RecoveryStage::EnterKey => 1,
        RecoveryStage::ConfirmStore | RecoveryStage::LogoutConfirm => 3,
        RecoveryStage::Instructions => 2,
        RecoveryStage::AcpMissing => 4,
    }
}

fn handle_first_run_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    let recovery = app.first_run_recovery.as_mut()?;

    if recovery.stage == RecoveryStage::EnterKey {
        match key.code {
            KeyCode::Esc => {
                recovery.secret_input.clear();
                recovery.stage = RecoveryStage::MissingCredential;
                recovery.selected = 0;
            }
            KeyCode::Backspace => {
                recovery.secret_input.pop();
            }
            KeyCode::Enter => {
                if recovery.secret_input.trim().is_empty() {
                    app.transcript
                        .push(Entry::Error { text: String::from("API key cannot be empty") });
                } else {
                    recovery.stage = RecoveryStage::ConfirmStore;
                    recovery.selected = 0;
                }
            }
            KeyCode::Char(ch) => recovery.secret_input.push(ch),
            _ => {}
        }
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            app.first_run_recovery = None;
            None
        }
        KeyCode::Up => {
            recovery.selected = recovery.selected.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            let max = recovery_action_count(recovery).saturating_sub(1);
            recovery.selected = (recovery.selected + 1).min(max);
            None
        }
        KeyCode::Enter => accept_recovery_action(app),
        _ => None,
    }
}

fn accept_recovery_action(app: &mut App) -> Option<Msg> {
    let recovery = app.first_run_recovery.clone()?;

    match recovery.stage {
        RecoveryStage::MissingCredential => match recovery.selected {
            0 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    app.first_run_recovery = None;
                    open_model_picker(app);
                    return None;
                }
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::EnterKey;
                    active.selected = 0;
                    active.secret_input.clear();
                }
            }
            1 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if let Some(active) = app.first_run_recovery.as_mut() {
                        active.stage = RecoveryStage::Instructions;
                        active.selected = 0;
                    }
                    return None;
                }
                app.first_run_recovery = None;
                open_model_picker(app);
            }
            2 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if recovery.pending_provider_prompt {
                        app.transcript.push(Entry::Status {
                            text: String::from(
                                "setup required before submitting this ChatGPT Codex prompt; run `thndrs login chatgpt-codex` or switch model",
                            ),
                        });
                    } else {
                        app.first_run_recovery = None;
                        app.transcript
                            .push(Entry::Status { text: String::from("setup skipped") });
                    }
                    return None;
                }
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::Instructions;
                    active.selected = 0;
                }
            }
            3 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    app.quit = true;
                    return Some(Msg::Quit);
                }
                if recovery.pending_provider_prompt {
                    app.transcript.push(Entry::Status {
                        text: String::from(
                            "setup required before submitting this provider-backed prompt; enter a key or switch model",
                        ),
                    });
                } else {
                    app.first_run_recovery = None;
                    app.transcript
                        .push(Entry::Status { text: String::from("setup skipped") });
                }
            }
            4 => {
                app.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::ConfirmStore => store_recovery_credential(app, &recovery),
        RecoveryStage::Instructions => match recovery.selected {
            0 => {
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::MissingCredential;
                    active.selected = 0;
                }
            }
            1 => app.first_run_recovery = None,
            _ => {}
        },
        RecoveryStage::LogoutConfirm => remove_recovery_credential(app, &recovery),
        RecoveryStage::AcpMissing => match recovery.selected {
            0 => {
                app.first_run_recovery = None;
                open_model_picker(app);
            }
            1 => {
                app.transcript.push(Entry::Status {
                    text: String::from("ACP setup: run `thndrs acp list` or `thndrs acp registry` outside the TUI"),
                });
            }
            2 => {
                if recovery.pending_provider_prompt {
                    app.transcript.push(Entry::Status {
                        text: String::from(
                            "ACP agent config is required before submitting this prompt; switch model or configure ACP",
                        ),
                    });
                } else {
                    app.first_run_recovery = None;
                }
            }
            3 => {
                app.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::EnterKey => {}
    }

    None
}

fn selected_scope(selected: usize) -> Option<CredentialScope> {
    match selected {
        0 => Some(CredentialScope::Global),
        1 => Some(CredentialScope::Project),
        _ => None,
    }
}

fn store_recovery_credential(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.first_run_recovery = None;
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.first_run_recovery = Some(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        return;
    };

    let key = recovery.secret_input.trim();
    let path = match crate::cli::commands::auth::credential_path(scope, &app.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };

    let Some(env_var) = provider.api_key_env_var() else {
        app.first_run_recovery = Some(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        app.transcript
            .push(Entry::Error { text: String::from("ChatGPT Codex uses OAuth login, not API-key storage") });
        return;
    };
    match auth::set_credential(&path, env_var, key) {
        Ok(()) => {
            if scope == CredentialScope::Project {
                if let Err(err) = auth::ensure_git_exclude(&app.cwd) {
                    app.transcript
                        .push(Entry::Error { text: format!("git exclude update failed: {err}") });
                }
            }
            app.transcript
                .push(Entry::Status { text: format!("{} credential stored in {}", provider.label(), scope.label()) });
            app.first_run_recovery = None;
        }
        Err(err) => app
            .transcript
            .push(Entry::Error { text: format!("credential write failed: {err}") }),
    }
}

fn remove_recovery_credential(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.first_run_recovery = None;
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.first_run_recovery = None;
        app.transcript
            .push(Entry::Status { text: String::from("logout cancelled") });
        return;
    };
    let path = match crate::cli::commands::auth::credential_path(scope, &app.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };
    let Some(env_var) = provider.api_key_env_var() else {
        app.first_run_recovery = None;
        app.transcript
            .push(Entry::Error { text: String::from("ChatGPT Codex credentials are stored in ~/.thndrs/auth.json") });
        return;
    };
    match auth::remove_credential(&path, env_var) {
        Ok(()) => {
            app.first_run_recovery = None;
            app.transcript.push(Entry::Status {
                text: format!("{} credential removed from {}", provider.label(), scope.label()),
            });
        }
        Err(err) => app
            .transcript
            .push(Entry::Error { text: format!("credential remove failed: {err}") }),
    }
}

/// Find the index of the most recent `Entry::Tool` in the transcript.
fn last_tool_entry_index(app: &App) -> Option<usize> {
    app.transcript
        .iter()
        .rposition(|entry| matches!(entry, Entry::Tool { .. }))
}

/// Toggle the detail pane on the most recent tool entry.
///
/// When opening, the pane targets the last `Entry::Tool` in the transcript
/// and resets the scroll offset. When closing, it clears the open flag.
fn toggle_detail_pane(app: &mut App) {
    let Some(index) = last_tool_entry_index(app) else {
        return;
    };
    if app.detail_pane.open && app.detail_pane.entry_index == index {
        app.detail_pane.open = false;
    } else {
        app.detail_pane = DetailPane { entry_index: index, scroll: 0, open: true };
    }
}

/// Handle keys while the detail pane is open.
///
/// - `Tab`/`Esc` close the pane and return control to the prompt.
/// - `Up`/`PageUp` scroll up.
/// - `Down`/`PageDown` scroll down.
/// - All other keys are swallowed so the prompt is not mutated while the
///   detail pane has focus.
fn handle_detail_pane_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    let total = detail_pane_output_count(app);
    match key.code {
        KeyCode::Tab | KeyCode::Esc => {
            app.detail_pane.open = false;
            None
        }
        KeyCode::Up | KeyCode::PageUp => {
            app.detail_pane.scroll_up();
            None
        }
        KeyCode::Down | KeyCode::PageDown => {
            app.detail_pane.scroll_down(total);
            None
        }
        _ => None,
    }
}

/// Count output lines available for the detail pane's current target entry.
fn detail_pane_output_count(app: &App) -> usize {
    let Some(entry) = app.transcript.get(app.detail_pane.entry_index) else {
        return 0;
    };
    match entry {
        Entry::Tool { output, .. } => output.len(),
        _ => 0,
    }
}

/// Run fuzzy filter and split results into parallel item + index vectors.
fn split_filter_items(all_items: &[PickerItem], query: &str, limit: usize) -> (Vec<PickerItem>, Vec<Vec<usize>>) {
    if query.trim().is_empty() {
        return (
            all_items.iter().take(limit).cloned().collect(),
            all_items.iter().take(limit).map(|_| Vec::new()).collect(),
        );
    }

    let searchable_items: Vec<String> = all_items.iter().map(PickerItem::searchable).collect();
    let filtered = fuzzy::fuzzy_filter(&searchable_items, query, limit);
    filtered
        .into_iter()
        .filter_map(|(matched, indices)| {
            searchable_items
                .iter()
                .position(|candidate| candidate == &matched)
                .and_then(|idx| all_items.get(idx).cloned().map(|item| (item, indices)))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .unzip()
}

fn insert_file_path(app: &mut App, path: &str) {
    if !app.input.is_empty() && !app.input.text_before_cursor().ends_with(char::is_whitespace) {
        app.input.insert_char(' ');
    }
    app.input.insert_str(path);
}

fn accept_file_suggestion(app: &mut App) {
    let Some(path) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };

    match app.prompt_accessory {
        PromptAccessory::Files(FilePickerSource::Mention { token_start }) => {
            let end = app.input.cursor();
            let replacement = format!("@{path} ");
            app.input.replace_range(token_start, end, &replacement);
        }
        PromptAccessory::Files(FilePickerSource::Forced) => {
            insert_file_path(app, &path);
        }
        _ => {}
    }

    close_prompt_accessory(app);
}

fn accept_model_suggestion(app: &mut App) {
    let Some(model) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };

    app.model = model.clone();
    app.cli.model = model.clone();
    app.transcript.push(Entry::Status { text: format!("model: {model}") });
    close_prompt_accessory(app);
}

fn accept_skill_suggestion(app: &mut App) {
    let Some(name) = app
        .picker
        .as_ref()
        .and_then(|picker| picker.selected().map(|item| item.label.clone()))
    else {
        return;
    };
    let Some(skill) = app.skills.iter().find(|skill| skill.name == name).cloned() else {
        close_prompt_accessory(app);
        return;
    };

    match skills::load_skill(&skill) {
        Ok(loaded) => {
            for diagnostic in &loaded.diagnostics {
                app.transcript.push(Entry::Error { text: diagnostic.summary() });
            }
            let text = format!(
                "# Skill: {}\n\n_Source: {}_\n\n{}",
                loaded.activation.name,
                loaded.activation.path.display(),
                loaded.markdown
            );
            app.transcript.push(Entry::Agent { text, streaming: false });
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_skill_activation(&loaded.activation);
            }
        }
        Err(diagnostic) => app.transcript.push(Entry::Error { text: diagnostic.summary() }),
    }
    close_prompt_accessory(app);
}

fn sync_prompt_accessory(app: &mut App) {
    if app.mode == Mode::Command {
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
        return;
    }

    if app.input.as_str().starts_with('/') {
        app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
        return;
    }

    if let Some((token_start, _query)) = active_at_token(app) {
        if !matches!(app.prompt_accessory, PromptAccessory::Files(FilePickerSource::Mention { token_start: existing }) if existing == token_start)
        {
            open_file_picker(app, FilePickerSource::Mention { token_start });
        } else {
            sync_file_picker_query(app);
        }
        return;
    }

    if !matches!(app.prompt_accessory, PromptAccessory::Help) {
        close_prompt_accessory(app);
    }
}

fn active_at_token(app: &App) -> Option<(usize, String)> {
    let before = app.input.text_before_cursor();
    let chars: Vec<char> = before.chars().collect();
    let token_start = chars.iter().rposition(|ch| ch.is_whitespace()).map_or(0, |idx| idx + 1);
    if chars.get(token_start) != Some(&'@') {
        return None;
    }
    let query: String = chars[token_start + 1..].iter().collect();
    Some((token_start, query))
}

fn sync_file_picker_query(app: &mut App) {
    let query = match app.prompt_accessory {
        PromptAccessory::Files(FilePickerSource::Mention { .. }) => active_at_token(app).map(|(_, query)| query),
        PromptAccessory::Files(FilePickerSource::Forced) => app.picker.as_ref().map(|picker| picker.query.clone()),
        _ => None,
    };
    let Some(query) = query else {
        return;
    };
    if let Some(picker) = app.picker.as_mut()
        && picker.query != query
    {
        picker.query = query;
        picker.refresh_matches();
    }
}

/// Handle an `Enter` submit. Slash commands are routed; otherwise the input is
/// appended as [`Entry::User`] and cleared, and the fake agent stream is started.
///
/// Returns an optional follow-up [`Msg`].
fn handle_submit(app: &mut App) -> Option<Msg> {
    if app.pending_permission.is_some() {
        return None;
    }

    if app.run_state == RunState::Working {
        let text = app.input.as_str().trim().to_string();
        if text.is_empty() {
            app.input.clear();
            return None;
        }
        if let Some(literal) = text.strip_prefix("//") {
            queue_running_input(app, &format!("/{literal}"));
            return None;
        }
        if let Some(command) = text.strip_prefix('/') {
            app.input.clear();
            return handle_running_command(app, command);
        }
        queue_running_input(app, &text);
        return None;
    }

    if !matches!(app.run_state, RunState::Idle | RunState::Error(_)) {
        return None;
    }

    let text = app.input.as_str().trim().to_string();
    if text.is_empty() {
        app.input.clear();
        return None;
    }

    if let Some(command) = text.strip_prefix('/') {
        return handle_command(app, command);
    }

    submit_user_turn(app, text)
}

fn queue_running_input(app: &mut App, text: &str) {
    app.input.clear();
    remember_input(app, text);
    let (kind, count) = match app.queue_target {
        QueueTarget::Steering => {
            app.queued_steering.push(text.to_string());
            ("steering", app.queued_steering.len())
        }
        QueueTarget::FollowUp => {
            app.queued_followups.push(text.to_string());
            ("follow-up", app.queued_followups.len())
        }
    };
    let audit_error = app
        .session_writer
        .as_mut()
        .and_then(|writer| writer.append_queued(kind, text).err());
    app.transcript
        .push(Entry::Status { text: format!("queued {kind} ({count})") });
    if let Some(err) = audit_error {
        app.transcript
            .push(Entry::Error { text: format!("failed to record queued {kind} in session audit log: {err}") });
    }
}

fn submit_user_turn(app: &mut App, text: String) -> Option<Msg> {
    if let Some(recovery) = selected_provider_missing(app) {
        app.first_run_recovery = Some(recovery);
        return None;
    }

    remember_input(app, &text);
    let user_entry = Entry::User { text: text.clone() };
    app.transcript.push(user_entry.clone());
    app.input.clear();
    app.history_cursor = None;
    app.history_draft.clear();
    app.last_input = Some(text);
    app.ttft.start_turn();
    app.turn_count += 1;
    let turn_id = format!("turn_{}", app.turn_count);
    refresh_mcp_config_audit(app, &turn_id);
    if let Some(ref mut writer) = app.session_writer {
        let _ = writer.append_entry(&user_entry, &turn_id);
    }
    Some(Msg::Agent(AgentEvent::Started))
}

/// Route a slash command (the part after `/` or the text after `:`).
fn handle_command(app: &mut App, command: &str) -> Option<Msg> {
    if command_contains_api_key_like_argument(command) {
        app.transcript.push(Entry::Error {
            text: String::from("slash commands do not accept API keys as arguments; use /login <provider>"),
        });
        app.input.clear();
        return None;
    }

    if command == "mcp" {
        list_mcp_servers(app);
        return None;
    }
    if command == "mcp tools" {
        list_mcp_tools(app, "");
        return None;
    }
    if let Some(name) = command.strip_prefix("mcp tools ") {
        list_mcp_tools(app, name.trim());
        return None;
    }
    if let Some(rest) = command.strip_prefix("login ") {
        app.input.clear();
        match parse_api_key_provider(rest.trim()) {
            Some(provider) => {
                app.first_run_recovery = Some(FirstRunRecovery::login(provider));
            }
            None => app.transcript.push(Entry::Error {
                text: String::from("usage: /login <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            }),
        }
        return None;
    }
    if let Some(rest) = command.strip_prefix("logout ") {
        app.input.clear();
        match parse_api_key_provider(rest.trim()) {
            Some(SetupProviderArg::ChatgptCodex) => {
                app.transcript.push(Entry::Status {
                    text: String::from(
                        "ChatGPT Codex logout is CLI-only; run `thndrs logout chatgpt-codex` outside the TUI",
                    ),
                });
            }
            Some(provider) => {
                app.first_run_recovery = Some(FirstRunRecovery::logout(provider));
            }
            None => app.transcript.push(Entry::Error {
                text: String::from("usage: /logout <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            }),
        }
        return None;
    }

    match command {
        "clear" => {
            app.transcript.clear();
            app.input.clear();
            app.queued_steering.clear();
            app.queued_followups.clear();
            Some(Msg::Clear)
        }
        "quit" | "exit" => {
            app.input.clear();
            app.quit = true;
            Some(Msg::Quit)
        }
        "help" => {
            app.prompt_accessory = PromptAccessory::Help;
            None
        }
        "bg" => {
            list_background_processes(app);
            None
        }
        "model" => {
            open_model_picker(app);
            None
        }
        "skills" => {
            open_skill_picker(app);
            None
        }
        "doctor" => {
            run_doctor_slash(app);
            app.input.clear();
            None
        }
        "auth status" => {
            run_auth_status_slash(app);
            app.input.clear();
            None
        }
        "config path" => {
            run_config_slash(app, &crate::cli::commands::config::ConfigCommand::Path);
            app.input.clear();
            None
        }
        "config show" => {
            run_config_slash(
                app,
                &crate::cli::commands::config::ConfigCommand::Show(crate::cli::commands::config::ConfigShowCommand {
                    redacted: true,
                }),
            );
            app.input.clear();
            None
        }
        "config edit" => {
            app.transcript.push(Entry::Status {
                text: String::from(
                    "config edit is CLI-only; run `thndrs config edit --global` or `thndrs config edit --project` outside the TUI",
                ),
            });
            app.input.clear();
            None
        }
        "setup" => {
            let provider = provider_for_model(&app.model);
            app.first_run_recovery = Some(FirstRunRecovery::missing_provider(provider, false));
            app.input.clear();
            None
        }
        "login" => {
            app.transcript.push(Entry::Error {
                text: String::from("usage: /login <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            });
            app.input.clear();
            None
        }
        "logout" => {
            app.transcript.push(Entry::Error {
                text: String::from("usage: /logout <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            });
            app.input.clear();
            None
        }
        _ => None,
    }
}

fn parse_api_key_provider(input: &str) -> Option<SetupProviderArg> {
    match input {
        "umans" => Some(SetupProviderArg::Umans),
        "opencode-go" => Some(SetupProviderArg::OpencodeGo),
        "opencode-zen" => Some(SetupProviderArg::OpencodeZen),
        "chatgpt-codex" => Some(SetupProviderArg::ChatgptCodex),
        _ => None,
    }
}

fn command_contains_api_key_like_argument(command: &str) -> bool {
    let mut parts = command.split_whitespace();
    let Some(head) = parts.next() else {
        return false;
    };
    let skip = match head {
        "login" | "logout" => 1,
        _ => 0,
    };
    parts.skip(skip).any(is_api_key_like)
}

fn is_api_key_like(value: &str) -> bool {
    let value = value.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == '`' || ch == ',' || ch == ';');
    let lower = value.to_ascii_lowercase();
    value.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("opencode_go_key=")
        || lower.contains("opencode_zen_key=")
        || lower.contains("umans_api_key=")
        || (value.len() >= 32
            && value.chars().any(|ch| ch.is_ascii_digit())
            && value.chars().any(|ch| ch.is_ascii_alphabetic()))
}

fn run_doctor_slash(app: &mut App) {
    let mut output = Vec::new();
    let command = crate::cli::commands::doctor::DoctorCommand { json: false };
    let result = crate::cli::commands::doctor::run_with_writer(&app.cli, &command, &mut output);
    push_command_output(app, "doctor", &output, result);
}

fn run_auth_status_slash(app: &mut App) {
    let mut output = Vec::new();
    let result = crate::cli::commands::auth::write_auth_status(&app.cwd, &mut output);
    push_command_output(app, "auth status", &output, result);
}

fn run_config_slash(app: &mut App, command: &crate::cli::commands::config::ConfigCommand) {
    let mut output = Vec::new();
    let result = crate::cli::commands::config::run_with_writer(&app.cli, command, &mut output);
    push_command_output(app, "config", &output, result);
}

fn push_command_output(app: &mut App, label: &str, output: &[u8], result: std::io::Result<()>) {
    let text = String::from_utf8_lossy(output).trim_end().to_string();
    if !text.is_empty() {
        app.transcript.push(Entry::Status { text });
    }
    if let Err(err) = result {
        app.transcript
            .push(Entry::Error { text: format!("{label} exited with {}: {err}", err.kind()) });
    }
}

/// Handle a slash command submitted while the agent is working.
///
/// Safe commands (`quit`, `exit`, `help`, `bg`) execute immediately. Commands
/// that mutate idle-only UI state are rejected instead of being queued as text.
/// Prefix with `//` to queue a literal slash-prefixed follow-up.
fn handle_running_command(app: &mut App, command: &str) -> Option<Msg> {
    let is_safe = matches!(command, "quit" | "exit" | "help" | "bg");
    if is_safe {
        return handle_command(app, command);
    }
    app.transcript.push(Entry::Status {
        text: format!("/{command} is not available while the agent is working; use //{command} to queue it as text"),
    });
    None
}

/// List background processes in the transcript.
fn list_background_processes(app: &mut App) {
    let bg_ids: Vec<u64> = app.process_registry.background_ids().collect();
    if bg_ids.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("no background processes") });
    } else {
        let lines: Vec<String> = bg_ids
            .iter()
            .filter_map(|id| {
                app.process_registry.get(*id).map(|p| {
                    let elapsed = p.elapsed().as_secs();
                    let cmd = p.command.join(" ");
                    format!("[{id}] {cmd} cwd={} ({elapsed}s)", p.cwd.display())
                })
            })
            .collect();
        app.transcript
            .push(Entry::Status { text: format!("background processes:\n{}", lines.join("\n")) });
    }
}

fn list_mcp_servers(app: &mut App) {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(&app.cwd, &env_vars) {
        Ok(effective) if effective.config.servers.is_empty() => {
            app.transcript
                .push(Entry::Status { text: String::from("no MCP servers configured") });
        }
        Ok(effective) => {
            let mut lines = Vec::new();
            for (name, server) in &effective.config.servers {
                let status = if server.enabled { "enabled" } else { "disabled" };
                lines.push(format!("{name}\t{status}\t{:?}", server.transport));
            }
            lines.extend(
                effective
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("diagnostic: {diagnostic}")),
            );
            app.transcript
                .push(Entry::Status { text: format!("MCP servers:\n{}", lines.join("\n")) });
        }
        Err(err) => app
            .transcript
            .push(Entry::Error { text: format!("failed to load MCP config: {err}") }),
    }
}

fn list_mcp_tools(app: &mut App, name: &str) {
    if name.is_empty() {
        app.transcript
            .push(Entry::Error { text: String::from("usage: /mcp tools <name>") });
        return;
    }

    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    let effective = match mcp::config::load_effective_mcp(&app.cwd, &env_vars) {
        Ok(effective) => effective,
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("failed to load MCP config: {err}") });
            return;
        }
    };
    let Some(server) = effective.config.servers.get(name) else {
        app.transcript
            .push(Entry::Error { text: format!("MCP server `{name}` is not configured") });
        return;
    };
    if !server.enabled {
        app.transcript
            .push(Entry::Error { text: format!("MCP server `{name}` is disabled") });
        return;
    }

    match mcp::manager::McpClient::connect(name.to_string(), server) {
        Ok(client) => {
            let lines: Vec<String> = client
                .tool_definitions()
                .into_iter()
                .map(|tool| format!("{}\t{}", tool.name, tool.description))
                .collect();
            app.transcript.push(Entry::Status {
                text: if lines.is_empty() {
                    format!("MCP server `{name}` exposes no tools")
                } else {
                    format!("MCP tools for `{name}`:\n{}", lines.join("\n"))
                },
            });
        }
        Err(err) => app.transcript.push(Entry::Error { text: err.to_string() }),
    }
}

/// Process an [`AgentEvent`] and mutate `app` accordingly.
fn handle_agent_event(app: &mut App, event: AgentEvent) -> Option<Msg> {
    match event {
        AgentEvent::Started => {
            app.run_state = RunState::Working;
            None
        }
        AgentEvent::Status(text) => {
            if app.verbose || !is_verbose_status(&text) {
                app.transcript.push(Entry::Status { text });
            }
            None
        }
        AgentEvent::Usage { input_tokens, output_tokens } => {
            app.session_tokens_in = app.session_tokens_in.saturating_add(input_tokens);
            app.session_tokens_out = app.session_tokens_out.saturating_add(output_tokens);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_usage(input_tokens, output_tokens);
            }
            None
        }
        AgentEvent::AssistantDelta(delta) => {
            app.ttft.stop_on_semantic_output();
            finalize_reasoning(app);
            if let Some(Entry::Agent { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Agent { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            app.ttft.stop_on_semantic_output();
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Reasoning { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ToolStarted { id, name, arguments } => {
            app.ttft.stop_on_semantic_output();
            finalize_streaming(app);
            app.transcript.push(Entry::Tool {
                name: format!("{name}#{id}"),
                arguments: arguments.clone(),
                status: ToolStatus::Running,
                output: Vec::new(),
            });
            if let Some(ref mut writer) = app.session_writer {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_tool_started(&turn_id, &id, &name, &arguments);
            }
            None
        }
        AgentEvent::ToolFinished { id, output, status, write_result, shell_result } => {
            app.ttft.stop_on_semantic_output();
            finalize_streaming(app);
            for entry in app.transcript.iter_mut().rev() {
                if let Entry::Tool { name, output: out, status: s, .. } = entry
                    && name.ends_with(&format!("#{id}"))
                {
                    *out = output;
                    *s = status;
                    break;
                }
            }

            persist_last_entry(app);

            if let Some(result) = write_result
                && let Some(ref mut writer) = app.session_writer
            {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_file_write(&turn_id, &result, status);
            }

            if let Some(result) = shell_result {
                if result.kind == tools::shell::ProcessKind::Background {
                    let cancel = CancelToken::new();
                    let id =
                        app.process_registry
                            .register(result.command.clone(), result.cwd.clone(), result.kind, cancel);
                    app.transcript.push(Entry::Status {
                        text: format!("background process [{id}] started: {}", result.command.join(" ")),
                    });
                }

                if let Some(ref mut writer) = app.session_writer {
                    let turn_id = format!("turn_{}", app.turn_count);
                    let _ = writer.append_shell_exec(&turn_id, &result);
                }
            }
            refresh_git_status(app);
            None
        }
        AgentEvent::ModelMetadataLoaded(items) => {
            app.model_picker_items = items
                .into_iter()
                .map(|(label, detail)| PickerItem::new(label, detail))
                .collect();
            None
        }
        AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => {
            discard_retry_output(app);
            app.run_state = RunState::Working;
            app.transcript.push(Entry::Status {
                text: format!(
                    "retrying provider request ({attempt}/{max_attempts}) in {:.1}s after: {error}",
                    delay_ms as f64 / 1000.0
                ),
            });
            None
        }
        AgentEvent::PermissionRequest(permission) => {
            finalize_streaming(app);
            if app.pending_permission.is_some() {
                let _ = permission.cancel();
                app.transcript.push(Entry::Error {
                    text: "acp: received a second permission request while one is pending; cancelled it".to_string(),
                });
                return None;
            }
            let turn_id = format!("turn_{}", app.turn_count);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_permission_request(&turn_id, &permission);
            }
            app.pending_permission = Some(permission);
            None
        }
        AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            let turn_id = format!("turn_{}", app.turn_count);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_permission_outcome(&turn_id, &tool_call_id, &outcome);
            }
            None
        }
        AgentEvent::AcpSession(metadata) => {
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_session(&metadata);
            }
            None
        }
        AgentEvent::Finished => {
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.run_state = RunState::Idle;
            app.last_input = None;
            refresh_git_status(app);
            persist_final_response(app);
            if app.queued_followups.is_empty() {
                None
            } else {
                let next = app.queued_followups.remove(0);
                submit_user_turn(app, next)
            }
        }
        AgentEvent::Failed(msg) => {
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.transcript.push(Entry::Error { text: msg.clone() });
            app.run_state = RunState::Error(msg);
            if let Some(input) = app.last_input.take() {
                app.input.set_text(&input);
            }
            persist_last_entry(app);
            refresh_git_status(app);
            None
        }
        AgentEvent::Cancelled => {
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            cancel_running_tools(app);
            if app.run_state == RunState::Working {
                app.transcript.push(Entry::Status { text: String::from("cancelled") });
            }
            app.run_state = RunState::Idle;
            app.last_input = None;
            app.queued_steering.clear();
            persist_last_entry(app);
            refresh_git_status(app);
            None
        }
    }
}

fn handle_permission_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Up => {
            if let Some(permission) = app.pending_permission.as_mut() {
                permission.move_up();
            }
            None
        }
        KeyCode::Down => {
            if let Some(permission) = app.pending_permission.as_mut() {
                permission.move_down();
            }
            None
        }
        KeyCode::Enter => {
            if let Some(permission) = app.pending_permission.take()
                && let Some(PermissionDecision::Selected(option_id)) = permission.select()
            {
                app.transcript.push(Entry::Status {
                    text: format!("acp permission {}: selected {option_id}", permission.tool_call_id),
                });
            }
            None
        }
        KeyCode::Esc => {
            if let Some(permission) = app.pending_permission.take() {
                let _ = permission.cancel();
                app.transcript
                    .push(Entry::Status { text: format!("acp permission {}: cancelled", permission.tool_call_id) });
            }
            None
        }
        _ => None,
    }
}

fn refresh_git_status(app: &mut App) {
    app.git_status = renderer::git::collect(&app.cwd);
}

/// Cancel an active stream by marking all streaming entries complete,
/// recording a cancelled status entry, and transitioning to `Stopping`.
///
/// The app loop observes the transition out of `Working` and drops the
/// background receiver, which stops the agent thread on its next failed send.
/// When the `Cancelled` agent event arrives (or the channel disconnects), the
/// state transitions from `Stopping` to `Idle`.
fn cancel_stream(app: &mut App) {
    cancel_pending_permission(app);
    finalize_streaming(app);
    app.transcript.push(Entry::Status { text: String::from("cancelled") });
    app.run_state = RunState::Stopping;
    persist_last_entry(app);
}

fn cancel_pending_permission(app: &mut App) {
    if let Some(permission) = app.pending_permission.take() {
        let _ = permission.cancel();
    }
}

fn remember_input(app: &mut App, text: &str) {
    if text.is_empty() || app.input_history.last().is_some_and(|last| last == text) {
        return;
    }
    app.input_history.push(text.to_string());
}

fn recall_older_input(app: &mut App) {
    if app.input_history.is_empty() {
        return;
    }

    let next = match app.history_cursor {
        Some(0) => 0,
        Some(index) => index.saturating_sub(1),
        None => {
            app.history_draft = app.input.text();
            app.input_history.len() - 1
        }
    };
    app.history_cursor = Some(next);
    app.input.set_text(&app.input_history[next]);
}

fn recall_newer_input(app: &mut App) {
    let Some(index) = app.history_cursor else {
        return;
    };

    if index + 1 < app.input_history.len() {
        let next = index + 1;
        app.history_cursor = Some(next);
        app.input.set_text(&app.input_history[next]);
    } else {
        app.history_cursor = None;
        app.input.set_text(&app.history_draft);
        app.history_draft.clear();
    }
}

fn exit_history_navigation(app: &mut App) {
    if app.history_cursor.is_some() {
        app.history_cursor = None;
        app.history_draft.clear();
    }
}

/// Persist the last transcript entry to the session file, if a writer exists.
///
/// Only finalized entries are written — streaming/running entries are skipped
/// by `SessionRecord::from_entry`.
fn persist_last_entry(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.last()
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

/// Persist the final model response even if provider status rows were appended
/// after the last assistant/reasoning delta.
fn persist_final_response(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.iter().rev().find(|entry| {
            matches!(
                entry,
                Entry::Agent { streaming: false, .. } | Entry::Reasoning { streaming: false, .. }
            )
        })
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

/// Whether `ui_tick` is at or past `deadline`, accounting for wrap-around.
///
/// If `deadline` has wrapped (e.g. `ui_tick` is small and `deadline` is near
/// `u64::MAX`), we treat the deadline as already passed — a wrap is so rare
/// that expiring early is the safe choice.
fn now_or_after_deadline(ui_tick: u64, deadline: u64) -> bool {
    if deadline >= ui_tick { deadline.wrapping_sub(ui_tick) > u64::MAX / 2 } else { true }
}

/// Mark all streaming `Assistant` and `Reasoning` entries as complete.
fn finalize_streaming(app: &mut App) {
    for entry in &mut app.transcript {
        match entry {
            Entry::Agent { streaming, .. } => *streaming = false,
            Entry::Reasoning { streaming, .. } => *streaming = false,
            _ => {}
        }
    }
}

/// Mark any running tool entries as cancelled.
///
/// Called when the active run is interrupted so that the renderer can show a
/// distinct cancelled-tool row instead of leaving the tool in a running state.
fn cancel_running_tools(app: &mut App) {
    for entry in &mut app.transcript {
        if let Entry::Tool { status, .. } = entry
            && *status == ToolStatus::Running
        {
            *status = ToolStatus::Cancelled;
        }
    }
}

/// Mark active reasoning entries complete when the model moves on to visible
/// assistant text or a tool call.
fn finalize_reasoning(app: &mut App) {
    for entry in &mut app.transcript {
        if let Entry::Reasoning { streaming, .. } = entry {
            *streaming = false;
        }
    }
}

/// Remove partial assistant/reasoning output from a provider attempt that is
/// about to be retried. Tool entries and prior completed transcript context are
/// left intact.
fn discard_retry_output(app: &mut App) {
    while matches!(
        app.transcript.last(),
        Some(Entry::Agent { .. } | Entry::Reasoning { .. })
    ) {
        app.transcript.pop();
    }
}

fn load_mcp_config_audit(workspace: &Path) -> (Vec<session::SessionConfigFile>, Vec<String>) {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(workspace, &env_vars) {
        Ok(effective) => {
            let files = effective
                .layers
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
            (files, effective.diagnostics)
        }
        Err(err) => (Vec::new(), vec![format!("failed to load MCP config: {err}")]),
    }
}

fn refresh_mcp_config_audit(app: &mut App, turn_id: &str) {
    let (current_files, current_diagnostics) = load_mcp_config_audit(&app.cwd);
    if app.mcp_config_files == current_files && app.mcp_config_diagnostics == current_diagnostics {
        return;
    }

    let previous_files = std::mem::replace(&mut app.mcp_config_files, current_files.clone());
    app.mcp_config_diagnostics = current_diagnostics.clone();
    app.transcript
        .push(Entry::Status { text: mcp_config_changed_status(&previous_files, &current_files) });
    if let Some(ref mut writer) = app.session_writer {
        let _ = writer.append_mcp_config_changed(turn_id, previous_files, current_files, current_diagnostics);
    }
}

fn mcp_config_changed_status(
    previous_files: &[session::SessionConfigFile], current_files: &[session::SessionConfigFile],
) -> String {
    let previous = config_file_hash_summary(previous_files);
    let current = config_file_hash_summary(current_files);
    format!("MCP config changed: {previous} -> {current}")
}

fn config_file_hash_summary(files: &[session::SessionConfigFile]) -> String {
    if files.is_empty() {
        return "none".to_string();
    }

    files
        .iter()
        .map(|file| format!("{}:{}:{}", file.source, file.path, file.sha256))
        .collect::<Vec<_>>()
        .join(", ")
}

fn default_user_label() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .map(|name| format!("User ({name})"))
        .unwrap_or_else(|_| String::from("You"))
}

fn is_verbose_status(text: &str) -> bool {
    text.starts_with("provider:") || text.starts_with("logs  ") || text.starts_with("tool budget:")
}
