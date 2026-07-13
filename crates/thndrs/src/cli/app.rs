//! Application state, message types, and event dispatch (The Elm architecture/TEA).
//!
//! `App` holds the mutable session and prompt state. `Msg` represents input,
//! provider, tool, permission, and lifecycle events. [`update`] applies one
//! message and may return another message for the caller to process.
//!
//! The root module declares the shared state and message vocabulary. The child
//! modules implement the event families that [`update`] dispatches:
//!
//! - [`onboarding`] handles provider setup, credential recovery, and OAuth.
//! - [`input`] handles editing, history, pickers, and prompt submission.
//! - [`commands`] handles slash-command parsing and command actions.
//! - [`context`] is reserved for context and compaction operations.
//! - [`agent_lifecycle`] is reserved for agent events and session persistence.

mod agent_lifecycle;
mod commands;
mod context;
mod input;
mod onboarding;
use commands::{handle_command, handle_running_command};

#[cfg(test)]
use input::accept_model_suggestion;

pub use commands::command_suggestions_for_app;
pub use input::{FilePickerSource, Mode, PickerItem, PickerState, PromptAccessory};
pub use onboarding::{ChatGptOAuthDriver, ChatGptOAuthRecovery, FirstRunRecovery, RecoveryStage};

use input::{
    handle_key, handle_mouse, load_legacy_project_input_history, offline_model_picker_items, open_model_picker,
    open_reasoning_effort_picker, open_skill_picker, submit_user_turn,
};
use onboarding::{
    PendingSetupReasoningEffort, advance_after_setup_model_config, handle_first_run_key, poll_chatgpt_oauth_on_tick,
    provider_authenticated, provider_for_model, selected_provider_missing,
};

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use serde::{Deserialize, Serialize};
use thndrs_agent::CancelToken;
pub use thndrs_agent::ToolStatus;
use thndrs_agent::context as agent_context;
use thndrs_agent::context::{
    CompactionSummaryCandidate, ContextItemKind, ContextVisibility, HarnessCandidate, InstructionCandidate,
    PinnedCandidate, SelectionInput, SkillCandidate, TranscriptCandidate, UserTurnCandidate,
};

use crate::acp::config::provider_label;
use crate::acp::permissions::{PendingPermission, PermissionDecision};
use crate::cli::commands::auth::CredentialScope;
use crate::cli::commands::setup::SetupProviderArg;
use crate::cli::input::history::{INPUT_HISTORY_LIMIT, InputHistoryStore};
use crate::cli::{Cli, ReasoningEffort, Theme, WebSearchMode};
use crate::input::PromptInput;
use crate::providers::{codex, opencode, umans};
use crate::renderer::git::GitStatusSummary;
use crate::thndrs_core::auth;
use crate::tools::shell::ProcessRegistry;
use crate::{config, fuzzy, internals, prompt, session, skills, tools, utils};
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
const PROJECT_INPUT_HISTORY_SESSION_LIMIT: usize = 32;
const PROJECT_INPUT_HISTORY_BYTES_PER_SESSION: usize = 64 * 1024;

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
    /// Dedicated workspace-local persistence for submitted prompt recall.
    input_history_store: InputHistoryStore,
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
    pub context_sources: Vec<crate::context::ContextSource>,
    /// Filesystem discovery diagnostics for project instructions.
    pub context_diagnostics: Vec<crate::context::InstructionDiagnostic>,
    /// Latest provider-neutral context ledger used for inspection and prompt
    /// assembly. It is replaced at each turn boundary.
    pub context_ledger: Option<agent_context::ContextLedger>,
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
    /// In-flight compaction (manual or automatic). The original active
    /// context is retained until the configured provider summary and audit
    /// record both succeed.
    pending_manual_compaction: Option<PendingManualCompaction>,
    /// A generated summary awaiting explicit review before it changes the
    /// active working set.
    pending_compaction_review: Option<PendingCompactionReview>,
    /// Last compaction review state for the context health surface.
    pub last_compaction_review: Option<session::CompactionReviewResult>,
    /// Task-local pins retained across turn-boundary ledger rebuilds.
    context_pins: Vec<PinnedCandidate>,
    /// Explicitly dropped context ids retained until the user recovers or
    /// resets them.
    context_dropped_ids: Vec<String>,
    /// Summaries that can stand in for older transcript entries.
    compaction_summaries: Vec<CompactionSummaryCandidate>,
    /// Current target for input submitted while the agent is running.
    pub queue_target: QueueTarget,
    /// Setup state to resume after choosing a GPT-5.6 reasoning effort.
    pending_setup_reasoning_effort: Option<PendingSetupReasoningEffort>,
    /// Active fuzzy picker state, used by file and model pickers.
    pub picker: Option<PickerState>,
    /// Inline prompt accessory rendered above the input.
    pub prompt_accessory: PromptAccessory,
    /// Focused first-run or credential recovery surface.
    pub first_run_recovery: Option<FirstRunRecovery>,
    /// ChatGPT OAuth functions used by focused recovery.
    pub chatgpt_oauth_driver: ChatGptOAuthDriver,
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
        let workspace_root = crate::context::discover_workspace_root(&value.cwd);
        let mut cli_snapshot = value.clone();
        cli_snapshot.cwd = workspace_root.clone();
        let context_inventory = crate::context::discover_instructions(&workspace_root);
        let context_sources = context_inventory.sources;
        let context_diagnostics = context_inventory.diagnostics;
        let skill_inventory = skills::discover(&workspace_root, &value.skill_dirs);
        let transcript = Vec::new();
        let sessions_dir = value
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&workspace_root));
        let session_id = session::generate_session_id();
        let input_history_store = InputHistoryStore::for_workspace(&workspace_root);
        let input_history = match input_history_store.load_recent() {
            Ok(Some(history)) => history,
            Ok(None) | Err(_) => {
                let history = load_legacy_project_input_history(&sessions_dir);
                let _ = input_history_store.seed_if_missing(&session_id, &history);
                history
            }
        };
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
            input_history,
            input_history_store,
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
            context_diagnostics,
            context_ledger: None,
            skills: skill_inventory.skills,
            skill_diagnostics: skill_inventory.diagnostics,
            ui_tick: 0,
            ctrl_d_pending: None,
            session_writer,
            turn_count: 0,
            process_registry: ProcessRegistry::new(),
            last_input: None,
            pending_manual_compaction: None,
            pending_compaction_review: None,
            last_compaction_review: None,
            context_pins: Vec::new(),
            context_dropped_ids: Vec::new(),
            compaction_summaries: Vec::new(),
            queue_target: QueueTarget::default(),
            pending_setup_reasoning_effort: None,
            picker: None,
            prompt_accessory: PromptAccessory::None,
            first_run_recovery: None,
            chatgpt_oauth_driver: ChatGptOAuthDriver::default(),
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

/// Pending compaction request with enough information to atomically replace
/// active context after a successful configured-model response.
///
/// Carries both the manual (`/compact`) and automatic (preflight pressure)
/// paths. For automatic compaction, `original_user_turn` holds the user turn
/// to restart after the summary is applied; for manual compaction it is
/// `None` because `/compact` is a command, not a submitted turn.
#[derive(Clone, Debug)]
struct PendingManualCompaction {
    original_transcript: Vec<Entry>,
    covered_start_seq: u64,
    covered_end_seq: u64,
    recovery_handle: String,
    /// Manual or automatic initiation, written to the audit record.
    trigger: session::CompactionTrigger,
    /// The user turn to restart after a successful automatic compaction.
    /// `None` for manual compaction.
    original_user_turn: Option<String>,
}

/// A provider-generated summary waiting for the user to approve or reject its
/// replacement of the active transcript range.
#[derive(Clone, Debug)]
struct PendingCompactionReview {
    pending: PendingManualCompaction,
    summary: String,
}

impl App {
    /// Build the initial app from parsed CLI args.
    ///
    /// Discovers the workspace root from `--cwd` (preferring the git root), loads
    /// scoped `AGENTS.md` sources if present, and records their metadata in the session.
    pub fn from_cli(cli: &Cli) -> Self {
        cli.into()
    }

    /// Whether a compaction turn is currently in flight.
    ///
    /// Used by the preflight gate to avoid re-triggering auto-compaction while
    /// the configured-model summary request is the active turn.
    pub fn compaction_in_flight(&self) -> bool {
        self.pending_manual_compaction.is_some()
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

impl App {
    /// Rebuild the deterministic context ledger for a turn boundary.
    ///
    /// The caller owns discovery, transcript projection, and persistence. The
    /// agent library receives only typed candidates and returns the policy
    /// result. This method also stores the latest ledger for bounded inspection.
    pub fn refresh_context_ledger(&mut self, user_turn: Option<&str>) -> agent_context::ContextLedger {
        let pinned_paths = self
            .context_pins
            .iter()
            .filter_map(|pin| pin.source_path.clone())
            .collect::<Vec<_>>();
        let instruction_selection = crate::context::select_instructions(&self.context_sources, &[], &pinned_paths);
        let applicable_paths = instruction_selection
            .applicable
            .iter()
            .map(|source| source.path.clone())
            .collect::<std::collections::HashSet<_>>();

        let mut harness = prompt::default_fragments()
            .into_iter()
            .map(|fragment| HarnessCandidate::new(fragment.name, fragment.content.len()))
            .collect::<Vec<_>>();
        harness.push(HarnessCandidate::new(
            "tool_catalog",
            tools::tool_definitions()
                .into_iter()
                .map(|tool| tool.name.len() + tool.description.len())
                .sum(),
        ));

        let instructions = self
            .context_sources
            .iter()
            .map(|source| InstructionCandidate {
                path: source.path.clone(),
                scope: source.scope.clone(),
                content_hash: source.content_hash,
                byte_count: source.byte_count,
                content: Some(source.content.clone()),
                truncated: source.truncated,
                applicable: applicable_paths.contains(&source.path),
            })
            .collect();
        let skills = self
            .skills
            .iter()
            .map(|skill| {
                SkillCandidate::discovered(&skill.name, skill.path.clone(), skill.content_hash, skill.byte_count)
            })
            .collect();
        let transcript = self
            .transcript
            .iter()
            .enumerate()
            .map(|(index, entry)| TranscriptCandidate {
                seq: index as u64 + 1,
                session_id: self.session_id.clone(),
                label: transcript_candidate_label(entry),
                bytes: transcript_candidate_bytes(entry),
                ui_only: matches!(entry, Entry::Status { .. } | Entry::Error { .. }),
                streaming: matches!(
                    entry,
                    Entry::Agent { streaming: true, .. } | Entry::Reasoning { streaming: true, .. }
                ),
            })
            .collect();
        let selection_input = SelectionInput {
            harness,
            user_turn: user_turn.map(|text| UserTurnCandidate::new(&self.session_id, self.turn_count + 1, text.len())),
            instructions,
            pins: self.context_pins.clone(),
            compaction_summaries: self.compaction_summaries.clone(),
            transcript,
            skills,
            dropped_ids: self.context_dropped_ids.clone(),
        };

        let provider = provider_label(&self.model);
        let (limits, mut diagnostics) = agent_context::ModelContextLimits::resolve(provider, &self.model, None, None);
        let mut ledger = agent_context::select_context(&selection_input, limits);
        diagnostics.extend(
            self.context_diagnostics
                .iter()
                .map(|diagnostic| agent_context::ContextDiagnostic {
                    severity: match diagnostic.severity {
                        crate::context::InstructionSeverity::Info => agent_context::DiagnosticSeverity::Info,
                        crate::context::InstructionSeverity::Warning => agent_context::DiagnosticSeverity::Warning,
                        crate::context::InstructionSeverity::Error => agent_context::DiagnosticSeverity::Error,
                    },
                    code: "instruction_discovery".to_string(),
                    message: diagnostic.summary(),
                }),
        );
        ledger.diagnostics.extend(diagnostics);
        self.context_ledger = Some(ledger.clone());
        ledger
    }

    /// Open the bounded context inspection surface.
    pub fn open_context_surface(&mut self) {
        self.refresh_context_ledger(None);
        self.prompt_accessory = PromptAccessory::Context;
        self.input.clear();
    }

    /// Build the bounded semantic table used by the normal, narrow, and
    /// small-height context surfaces.
    pub fn context_table_view(&self) -> crate::renderer::view::TableView {
        use crate::renderer::view::{ColumnAlignment, ColumnWidthPolicy, TableCellView, TableView};

        let Some(ledger) = &self.context_ledger else {
            return TableView {
                header: vec![TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }],
                rows: vec![vec![TableCellView {
                    text: "no ledger".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                }]],
                selected_row: None,
                narrow_fallback: vec!["context unavailable".to_string()],
            };
        };

        let review = self
            .last_compaction_review
            .map(compaction_review_label)
            .unwrap_or("none");
        let counts = ledger.counts();
        let mut rows = vec![
            context_table_row(
                "budget",
                &format!("{} / {}", ledger.budget.used, ledger.budget.target),
                "tokens",
                "target",
            ),
            context_table_row(
                "source",
                ledger.budget.limits.source.label(),
                ledger.budget.limits.confidence.label(),
                "limits",
            ),
            context_table_row(
                "compaction",
                &format!("{} / {}", compaction_mode_label(self), review),
                &counts.visible.to_string(),
                "review",
            ),
        ];
        rows.extend(ledger.items.iter().take(CONTEXT_INSPECTION_MAX_ITEMS).map(|item| {
            vec![
                TableCellView {
                    text: redact_context_display(&item.id),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(34),
                },
                TableCellView {
                    text: format!("{} / {}", item.kind.label(), item.visibility.label()),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(26),
                },
                TableCellView {
                    text: item.token_estimate.to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(9),
                },
                TableCellView {
                    text: redact_context_display(&item.label),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
            ]
        }));
        let mut narrow_fallback = vec![
            format!("budget {} / {} tokens", ledger.budget.used, ledger.budget.target),
            format!(
                "limits {} ({})",
                ledger.budget.limits.source.label(),
                ledger.budget.limits.confidence.label()
            ),
            format!("compaction {} review {}", compaction_mode_label(self), review),
            format!(
                "items {} visible {} pinned {} dropped {} archived {} blocked {}",
                ledger.items.len(),
                counts.visible,
                counts.pinned,
                counts.dropped,
                counts.archived,
                counts.blocked
            ),
        ];
        narrow_fallback.extend(
            ledger
                .items
                .iter()
                .take(CONTEXT_INSPECTION_MAX_ITEMS)
                .map(|item| redact_context_display(&item.summary())),
        );
        TableView {
            header: vec![
                TableCellView {
                    text: "context".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(34),
                },
                TableCellView {
                    text: "state".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Percent(26),
                },
                TableCellView {
                    text: "tokens".to_string(),
                    alignment: ColumnAlignment::Right,
                    width: ColumnWidthPolicy::Fixed(9),
                },
                TableCellView {
                    text: "label".to_string(),
                    alignment: ColumnAlignment::Left,
                    width: ColumnWidthPolicy::Flexible,
                },
            ],
            rows,
            selected_row: None,
            narrow_fallback,
        }
    }

    fn pin_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let (candidate, item) = if let Some(item) = self.context_item(reference)? {
            if item.kind == ContextItemKind::Harness {
                return Err("harness context is always loaded and cannot be pinned".to_string());
            }
            (
                PinnedCandidate {
                    id: item.id.clone(),
                    kind: item.kind.clone(),
                    label: item.label.clone(),
                    source_path: item.source_path.clone(),
                    scope: item.scope.clone(),
                    content_hash: item.content_hash,
                    bytes: item.byte_count,
                },
                item.clone(),
            )
        } else {
            let path = self.resolve_context_path(reference)?;
            let candidate = PinnedCandidate::file(ContextItemKind::PinnedFile, path.clone(), ".", file_size(&path));
            let item = agent_context::ContextItem {
                id: candidate.id.clone(),
                kind: candidate.kind.clone(),
                label: candidate.label.clone(),
                source_path: candidate.source_path.clone(),
                scope: candidate.scope.clone(),
                content_hash: candidate.content_hash,
                byte_count: candidate.bytes,
                content: None,
                token_estimate: agent_context::estimate_tokens(candidate.bytes),
                visibility: ContextVisibility::Pinned,
                reason: "user pin".to_string(),
            };
            (candidate, item)
        };
        if self.context_pins.iter().any(|pin| pin.id == candidate.id) {
            return Err(format!(
                "context item `{}` is already pinned",
                redact_context_display(&candidate.id)
            ));
        }
        if let Some(writer) = self.session_writer.as_mut() {
            writer
                .append_context_pin(&item, "user pinned context item")
                .map_err(|error| format!("failed to record context pin: {error}"))?;
        }
        self.context_pins.push(candidate);
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn drop_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let item = self
            .context_item(reference)?
            .ok_or_else(|| format!("unknown context item `{}`", redact_context_display(reference)))?
            .clone();
        if item.kind == ContextItemKind::Harness {
            return Err("harness context cannot be dropped".to_string());
        }
        if self.context_dropped_ids.iter().any(|id| id == &item.id) {
            return Err(format!(
                "context item `{}` is already dropped",
                redact_context_display(&item.id)
            ));
        }
        if let Some(writer) = self.session_writer.as_mut() {
            writer
                .append_context_drop(&item, "user dropped context item")
                .map_err(|error| format!("failed to record context drop: {error}"))?;
        }
        self.context_dropped_ids.push(item.id);
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn recover_context_reference(&mut self, reference: &str) -> Result<(), String> {
        self.ensure_context_ledger();
        let item = self
            .context_item(reference)?
            .ok_or_else(|| format!("unknown context item `{}`", redact_context_display(reference)))?
            .clone();
        if item.kind == ContextItemKind::Harness {
            return Err("harness context is always available and needs no recovery".to_string());
        }
        let was_dropped = self.context_dropped_ids.iter().any(|id| id == &item.id);
        let needs_pin = !item.visibility.is_rendered();
        if !was_dropped && !needs_pin {
            return Err(format!(
                "context item `{}` is already active",
                redact_context_display(&item.id)
            ));
        }
        if let Some(writer) = self.session_writer.as_mut() {
            writer
                .append_context_recovery(&item, "user recovered context item")
                .map_err(|error| format!("failed to record context recovery: {error}"))?;
        }
        self.context_dropped_ids.retain(|id| id != &item.id);
        if needs_pin && !self.context_pins.iter().any(|pin| pin.id == item.id) {
            self.context_pins.push(PinnedCandidate {
                id: item.id.clone(),
                kind: item.kind.clone(),
                label: item.label.clone(),
                source_path: item.source_path.clone(),
                scope: item.scope.clone(),
                content_hash: item.content_hash,
                bytes: item.byte_count,
            });
        }
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn reset_context_drops(&mut self) -> Result<(), String> {
        if self.context_dropped_ids.is_empty() {
            return Err("no dropped context items to reset".to_string());
        }
        self.context_dropped_ids.clear();
        self.refresh_context_ledger(None);
        Ok(())
    }

    fn ensure_context_ledger(&mut self) {
        if self.context_ledger.is_none() {
            self.refresh_context_ledger(None);
        }
    }

    fn restore_context_state(&mut self, records: &[session::SessionRecord]) {
        self.context_pins.clear();
        self.context_dropped_ids.clear();
        self.compaction_summaries.clear();
        self.last_compaction_review = None;
        for record in records {
            match record {
                session::SessionRecord::ContextPin { item, .. } => {
                    if item.kind != ContextItemKind::Harness && !self.context_pins.iter().any(|pin| pin.id == item.id) {
                        self.context_pins.push(pinned_candidate_from_meta(item));
                    }
                }
                session::SessionRecord::ContextDrop { item, .. } => {
                    if !self.context_dropped_ids.iter().any(|id| id == &item.id) {
                        self.context_dropped_ids.push(item.id.clone());
                    }
                }
                session::SessionRecord::ContextRecovery { item, .. } => {
                    self.context_dropped_ids.retain(|id| id != &item.id);
                    if item.kind != ContextItemKind::Harness
                        && !item.visibility.is_rendered()
                        && !self.context_pins.iter().any(|pin| pin.id == item.id)
                    {
                        self.context_pins.push(pinned_candidate_from_meta(item));
                    }
                }
                session::SessionRecord::Compaction { audit, .. } => {
                    for candidate in &mut self.compaction_summaries {
                        candidate.latest = false;
                    }
                    let mut candidate = CompactionSummaryCandidate::new(
                        &self.session_id,
                        audit.covered_start_seq,
                        audit.covered_end_seq,
                        audit.summary.len(),
                        true,
                    );
                    candidate.content = Some(audit.summary.clone());
                    self.compaction_summaries.push(candidate);
                    self.last_compaction_review = audit.review;
                }
                session::SessionRecord::CompactionReview { review, .. } => {
                    self.last_compaction_review = Some(*review);
                }
                _ => {}
            }
        }
        self.context_ledger = None;
    }

    fn context_item(&self, reference: &str) -> Result<Option<&agent_context::ContextItem>, String> {
        let Some(ledger) = &self.context_ledger else {
            return Ok(None);
        };
        let matches = ledger
            .items
            .iter()
            .filter(|item| item.id == reference || item.id.starts_with(reference))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [item] => Ok(Some(item)),
            _ => Err(format!("context reference `{reference}` is ambiguous")),
        }
    }

    fn resolve_context_path(&self, value: &str) -> Result<PathBuf, String> {
        let path = Path::new(value);
        let path = if path.is_absolute() { path.to_path_buf() } else { self.cwd.join(path) };
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("cannot pin `{}`: {error}", redact_context_display(value)))?;
        if !canonical.starts_with(&self.cwd) {
            return Err("context pins must stay inside the workspace".to_string());
        }
        if !canonical.is_file() {
            return Err(format!("context pin is not a file: {}", redact_context_display(value)));
        }
        Ok(canonical)
    }
}

const CONTEXT_INSPECTION_MAX_ITEMS: usize = 64;
const CONTEXT_DISPLAY_MAX_BYTES: usize = 160;

fn transcript_candidate_label(entry: &Entry) -> String {
    match entry {
        Entry::User { .. } => "user".to_string(),
        Entry::Agent { .. } => "assistant".to_string(),
        Entry::Reasoning { .. } => "reasoning".to_string(),
        Entry::Tool { name, .. } => format!("tool:{name}"),
        Entry::Status { .. } => "status".to_string(),
        Entry::Error { .. } => "error".to_string(),
    }
}

fn pinned_candidate_from_meta(item: &session::ContextItemMeta) -> PinnedCandidate {
    PinnedCandidate {
        id: item.id.clone(),
        kind: item.kind.clone(),
        label: item.source_path.clone().unwrap_or_else(|| item.id.clone()),
        source_path: item.source_path.clone().map(PathBuf::from),
        scope: item.scope.clone().unwrap_or_else(|| ".".to_string()),
        content_hash: item.content_hash,
        bytes: item.byte_count,
    }
}

fn transcript_candidate_bytes(entry: &Entry) -> usize {
    match entry {
        Entry::User { text }
        | Entry::Agent { text, .. }
        | Entry::Reasoning { text, .. }
        | Entry::Status { text }
        | Entry::Error { text } => text.len(),
        Entry::Tool { name, arguments, output, .. } => {
            name.len() + arguments.len() + output.iter().map(String::len).sum::<usize>()
        }
    }
}

fn file_size(path: &Path) -> usize {
    std::fs::metadata(path)
        .map(|metadata| metadata.len().min(usize::MAX as u64) as usize)
        .unwrap_or(0)
}

fn redact_context_display(value: &str) -> String {
    let redacted = tools::shell::redact_secrets(value);
    utils::truncate_ellipsis(&redacted, CONTEXT_DISPLAY_MAX_BYTES)
}

fn context_table_row(name: &str, state: &str, tokens: &str, label: &str) -> Vec<crate::renderer::view::TableCellView> {
    use crate::renderer::view::{ColumnAlignment, ColumnWidthPolicy, TableCellView};
    vec![
        TableCellView {
            text: name.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(34),
        },
        TableCellView {
            text: state.to_string(),
            alignment: ColumnAlignment::Left,
            width: ColumnWidthPolicy::Percent(26),
        },
        TableCellView {
            text: tokens.to_string(),
            alignment: ColumnAlignment::Right,
            width: ColumnWidthPolicy::Fixed(9),
        },
        TableCellView { text: label.to_string(), alignment: ColumnAlignment::Left, width: ColumnWidthPolicy::Flexible },
    ]
}

fn compaction_mode_label(app: &App) -> &'static str {
    effective_compaction_policy(app).mode.label()
}

fn compaction_review_label(review: session::CompactionReviewResult) -> &'static str {
    match review {
        session::CompactionReviewResult::NotRequired => "not-required",
        session::CompactionReviewResult::Pending => "pending",
        session::CompactionReviewResult::Approved => "approved",
        session::CompactionReviewResult::Rejected => "rejected",
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
            poll_chatgpt_oauth_on_tick(app);
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

/// Start an idle `/compact` request through the selected provider model.
fn start_manual_compaction(app: &mut App) -> Option<Msg> {
    start_compaction(app, session::CompactionTrigger::Manual, None)
}

/// Start an automatic compaction triggered by preflight context pressure.
///
/// `original_user_turn` is the user turn that was about to be sent to the
/// provider; it is restarted after the summary is applied. The user turn is
/// already in the transcript, so the covered range starts from sequence 1.
///
/// Returns `None` (without spawning) when compaction cannot start, leaving
/// the submitted turn recoverable.
pub fn start_auto_compaction(app: &mut App, original_user_turn: String) -> Option<Msg> {
    start_compaction(app, session::CompactionTrigger::Automatic, Some(original_user_turn))
}

/// Shared core for manual and automatic compaction.
///
/// Saves the active transcript, builds a configured-model summary request,
/// submits it as the turn to run, and records enough state to atomically
/// replace active context on success or restore it on failure.
fn start_compaction(
    app: &mut App, trigger: session::CompactionTrigger, original_user_turn: Option<String>,
) -> Option<Msg> {
    let original_transcript = app.transcript.clone();
    let source = render_compaction_source(&original_transcript);
    let policy = effective_compaction_policy(app);
    let covered_start_seq = 1;
    let covered_end_seq = app
        .session_writer
        .as_ref()
        .map_or(0, |writer| writer.next_sequence().saturating_sub(1));
    let recovery_handle = format!("session:{}:{covered_start_seq}..{covered_end_seq}", app.session_id);
    let request = match agent_context::prepare_manual_compaction(policy, &app.model, &source, &recovery_handle) {
        Ok(request) => request,
        Err(message) => {
            app.transcript.push(Entry::Error { text: message });
            if trigger == session::CompactionTrigger::Automatic
                && let Some(turn) = original_user_turn
            {
                app.last_input = Some(turn);
            }
            return None;
        }
    };

    let started = match submit_user_turn(app, request.prompt) {
        Some(msg) => msg,
        None => {
            app.transcript = original_transcript;
            if trigger == session::CompactionTrigger::Automatic
                && let Some(turn) = original_user_turn
            {
                app.last_input = Some(turn);
            }
            return None;
        }
    };
    app.pending_manual_compaction = Some(PendingManualCompaction {
        original_transcript,
        covered_start_seq,
        covered_end_seq,
        recovery_handle,
        trigger,
        original_user_turn,
    });
    Some(started)
}

/// Resolve the configured compaction policy from loaded config layers.
pub fn effective_compaction_policy(app: &App) -> agent_context::CompactionPolicy {
    let config = app
        .cli
        .config_layers
        .iter()
        .map(|layer| &layer.config.context.compaction)
        .rev()
        .find(|config| **config != agent_context::CompactionConfig::default())
        .cloned()
        .unwrap_or_default();
    agent_context::CompactionPolicy::from_config(&config)
}

/// Render active transcript material for the configured compaction model.
fn render_compaction_source(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::User { text } => format!("user: {text}"),
            Entry::Agent { text, .. } => format!("assistant: {text}"),
            Entry::Reasoning { text, .. } => format!("reasoning: {text}"),
            Entry::Tool { name, arguments, output, .. } => {
                format!("tool {name} {arguments}: {}", output.join("\n"))
            }
            Entry::Status { text } => format!("status: {text}"),
            Entry::Error { text } => format!("error: {text}"),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn session_directory(app: &App) -> PathBuf {
    app.cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(&app.cwd))
}

fn run_history_command(app: &mut App) -> Option<Msg> {
    let dir = session_directory(app);
    let files = session::list_session_files(&dir);
    if files.is_empty() {
        app.transcript
            .push(Entry::Status { text: String::from("no sessions found") });
    } else {
        let rows = files
            .into_iter()
            .take(20)
            .map(|path| {
                let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("session");
                let summary = session::SessionReader::read_summary(&path);
                format!(
                    "{id}\t{}\t{}\tin {} out {}",
                    summary.title, summary.model, summary.input_tokens, summary.output_tokens
                )
            })
            .collect::<Vec<_>>();
        app.transcript
            .push(Entry::Status { text: format!("sessions:\n{}", rows.join("\n")) });
    }
    app.input.clear();
    None
}

fn show_session_command(app: &mut App, session_id: &str) -> Option<Msg> {
    let path = match session::resolve_session_file(&session_directory(app), session_id) {
        Ok(path) => path,
        Err(error) => {
            app.transcript.push(Entry::Error { text: error.to_string() });
            return None;
        }
    };
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let summary = session::SessionReader::read_summary(&path);
    app.transcript.push(Entry::Status {
        text: format!(
            "session: {id}\ntitle: {}\nmodel: {}\ntokens: in {} out {}\npath: {}",
            summary.title,
            summary.model,
            summary.input_tokens,
            summary.output_tokens,
            path.display()
        ),
    });
    app.input.clear();
    None
}

fn resume_session_command(app: &mut App, session_id: &str) -> Option<Msg> {
    let path = match session::resolve_session_file(&session_directory(app), session_id) {
        Ok(path) => path,
        Err(error) => {
            app.transcript.push(Entry::Error { text: error.to_string() });
            return None;
        }
    };
    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(session_id)
        .to_string();
    if id == app.session_id {
        app.transcript
            .push(Entry::Error { text: String::from("the current session is already active") });
        return None;
    }
    let writer = match session::SessionWriter::resume(&path, &id) {
        Ok(writer) => writer,
        Err(error) => {
            app.transcript
                .push(Entry::Error { text: format!("cannot resume session `{id}`: {error}") });
            return None;
        }
    };
    let summary = session::SessionReader::read_summary(&path);
    let transcript = session::SessionReader::read_transcript(&path);
    let records = session::SessionReader::read_records(&path);
    let turn_count = records
        .iter()
        .filter(|record| matches!(record, session::SessionRecord::User { .. }))
        .count() as u64;

    app.session_writer = Some(writer);
    app.session_id = id.clone();
    app.transcript = transcript;
    app.restore_context_state(&records);
    app.session_tokens_in = summary.input_tokens;
    app.session_tokens_out = summary.output_tokens;
    app.turn_count = turn_count;
    app.last_input = None;
    app.pending_manual_compaction = None;
    app.queued_steering.clear();
    app.queued_followups.clear();
    app.pending_permission = None;
    app.run_state = RunState::Idle;
    app.input.clear();
    app.history_cursor = None;
    app.history_draft.clear();
    app.transcript
        .push(Entry::Status { text: format!("resumed session: {id}") });
    None
}

fn read_session_log_command(app: &mut App, requested_session_id: Option<&str>) -> Option<Msg> {
    let id = match requested_session_id {
        Some(query) => match session::resolve_session_file(&session_directory(app), query) {
            Ok(path) => path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(query)
                .to_string(),
            Err(error) => {
                app.transcript.push(Entry::Error { text: error.to_string() });
                return None;
            }
        },
        None => app.session_id.clone(),
    };
    let path = app
        .cwd
        .join(".thndrs")
        .join("logs")
        .join("sessions")
        .join(format!("thndrs-{id}.log"));
    let lines = session::read_redacted_log_tail(&path, 100);
    if lines.is_empty() {
        app.transcript
            .push(Entry::Error { text: format!("debug log `{}` is empty or missing", path.display()) });
        return None;
    }
    app.transcript
        .push(Entry::Status { text: format!("debug log {id}:\n{}", lines.join("\n")) });
    app.input.clear();
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
            match finish_manual_compaction(app) {
                None => persist_final_response(app),
                Some(None) => {}
                Some(Some(restart)) => return Some(restart),
            }
            if app.queued_followups.is_empty() {
                None
            } else {
                let next = app.queued_followups.remove(0);
                submit_user_turn(app, next)
            }
        }
        AgentEvent::Failed(msg) => {
            let manual_compaction = restore_failed_manual_compaction(app);
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.transcript.push(Entry::Error { text: msg.clone() });
            app.run_state = RunState::Error(msg);
            if !manual_compaction && let Some(input) = app.last_input.take() {
                app.input.set_text(&input);
            }
            persist_last_entry(app);
            refresh_git_status(app);
            None
        }
        AgentEvent::Cancelled => {
            restore_failed_manual_compaction(app);
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

/// Apply a completed manual compaction only after its audit record is durable.
/// Apply a completed compaction only after its audit record is durable.
///
/// Returns `None` when the finished event did not belong to a compaction.
/// Returns `Some(None)` when a manual compaction finished (no restart needed).
/// Returns `Some(Some(msg))` when an automatic compaction finished and the
/// original user turn should restart with the compacted working set.
fn finish_manual_compaction(app: &mut App) -> Option<Option<Msg>> {
    let pending = app.pending_manual_compaction.take()?;
    let summary = app.transcript.iter().rev().find_map(|entry| match entry {
        Entry::Agent { text, .. } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    });
    let Some(summary) = summary else {
        restore_failed_compaction(app, pending);
        app.transcript
            .push(Entry::Error { text: "compaction model returned no summary".to_string() });
        return Some(None);
    };

    let risk = classify_compaction_risk(&pending.original_transcript);
    let review = if effective_compaction_policy(app).requires_review(match risk {
        session::CompactionRisk::Low => agent_context::CompactionRisk::Low,
        session::CompactionRisk::High => agent_context::CompactionRisk::High,
    }) {
        session::CompactionReviewResult::Pending
    } else {
        session::CompactionReviewResult::NotRequired
    };
    let audit = session::CompactionAudit {
        summary: summary.clone(),
        covered_start_seq: pending.covered_start_seq,
        covered_end_seq: pending.covered_end_seq,
        source_hashes: Vec::new(),
        trigger: pending.trigger,
        risk,
        review: Some(review),
        recovery_handles: vec![pending.recovery_handle.clone()],
        model: app.model.clone(),
        usage: None,
    };
    if let Some(writer) = app.session_writer.as_mut()
        && let Err(error) = writer.append_compaction(&audit)
    {
        restore_failed_compaction(app, pending);
        app.transcript
            .push(Entry::Error { text: format!("failed to record compaction audit: {error}") });
        return Some(None);
    }

    if review == session::CompactionReviewResult::Pending {
        let recovery_handle = pending.recovery_handle.clone();
        let saved_pending = pending.clone();
        restore_failed_compaction(app, saved_pending);
        app.pending_compaction_review = Some(PendingCompactionReview { pending, summary });
        app.last_compaction_review = Some(review);
        app.transcript
            .push(Entry::Status { text: format!("compaction review pending  {recovery_handle}") });
        return Some(None);
    }
    app.last_compaction_review = Some(review);
    apply_compaction(app, pending, summary)
}

/// Apply an approved or review-free summary to the active working set.
fn apply_compaction(app: &mut App, pending: PendingManualCompaction, summary: String) -> Option<Option<Msg>> {
    let is_automatic = pending.trigger == session::CompactionTrigger::Automatic;
    let original_user_turn = pending.original_user_turn.clone();
    for candidate in &mut app.compaction_summaries {
        candidate.latest = false;
    }
    let mut summary_candidate = CompactionSummaryCandidate::new(
        &app.session_id,
        pending.covered_start_seq,
        pending.covered_end_seq,
        summary.len(),
        true,
    );
    summary_candidate.content = Some(summary.clone());
    app.compaction_summaries.push(summary_candidate);

    if is_automatic {
        app.transcript.clear();
    } else {
        app.transcript = pending.original_transcript;
        app.transcript
            .push(Entry::Status { text: format!("compacted  {}", pending.recovery_handle) });
    }
    let summary_entry = Entry::Agent { text: summary, streaming: false };
    app.transcript.push(summary_entry.clone());
    if let Some(writer) = app.session_writer.as_mut() {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(&summary_entry, &turn_id);
    }
    if is_automatic {
        app.transcript
            .push(Entry::Status { text: format!("auto-compacted  {}", pending.recovery_handle) });
        if let Some(turn) = original_user_turn {
            return Some(submit_user_turn(app, turn));
        }
    }
    Some(None)
}

/// Restore active context when a compaction request fails or cannot complete.
///
/// For automatic compaction, the submitted user turn is preserved by restoring
/// `last_input` so the user can resubmit or edit it. For manual compaction,
/// only the transcript is restored.
fn restore_failed_manual_compaction(app: &mut App) -> bool {
    if let Some(pending) = app.pending_manual_compaction.take() {
        restore_failed_compaction(app, pending);
        true
    } else {
        false
    }
}

/// Restore the saved transcript and, for automatic compaction, the submitted
/// user turn.
fn restore_failed_compaction(app: &mut App, pending: PendingManualCompaction) {
    app.transcript = pending.original_transcript;
    if pending.trigger == session::CompactionTrigger::Automatic
        && let Some(turn) = pending.original_user_turn
    {
        app.last_input = Some(turn);
    }
}

/// Map transcript signals to the durable compaction-risk classification.
fn classify_compaction_risk(entries: &[Entry]) -> session::CompactionRisk {
    let signals = agent_context::CompactionRiskSignals {
        has_tool_output_or_diff: entries.iter().any(|entry| matches!(entry, Entry::Tool { .. })),
        has_failure_or_permission: entries.iter().any(|entry| matches!(entry, Entry::Error { .. })),
        has_correction_or_unresolved_work: entries.iter().any(
            |entry| matches!(entry, Entry::Status { text } if text.contains("permission") || text.contains("failed")),
        ),
    };
    match signals.classify() {
        agent_context::CompactionRisk::Low => session::CompactionRisk::Low,
        agent_context::CompactionRisk::High => session::CompactionRisk::High,
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
    let overflow = app
        .input_history
        .len()
        .saturating_add(1)
        .saturating_sub(INPUT_HISTORY_LIMIT);
    if overflow > 0 {
        app.input_history.drain(..overflow);
    }
    app.input_history.push(text.to_string());
    let _ = app.input_history_store.append(&app.session_id, text);
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
