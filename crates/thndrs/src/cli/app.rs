//! Application state, message types, and event dispatch (The Elm architecture/TEA).
//!
//! `App` holds the mutable session and prompt state. `Msg` represents input,
//! provider, tool, permission, and lifecycle events. [`update`] applies one
//! message and may return another message for the caller to process.
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
#[cfg(test)]
pub use agent_lifecycle::{handle_agent_event, remember_input};
use commands::{handle_command, handle_running_command};

pub use context::CONTEXT_INSPECTION_MAX_ITEMS;
pub use context::start_auto_compaction;

#[cfg(test)]
use input::accept_model_suggestion;

pub use commands::command_suggestions_for_app;
pub use input::{FilePickerSource, Mode, PickerItem, PickerState, PromptAccessory};
pub use onboarding::setup_model_options;
pub use onboarding::{ChatGptOAuthDriver, ChatGptOAuthMethod, ChatGptOAuthRecovery, FirstRunRecovery, RecoveryStage};

use input::{
    handle_key, handle_mouse, offline_model_picker_items, open_model_picker, open_reasoning_effort_picker,
    open_skill_picker, submit_user_turn,
};
use onboarding::{
    PendingSetupReasoningEffort, advance_after_setup_model_config, handle_first_run_key, poll_chatgpt_oauth_on_tick,
    provider_authenticated, provider_for_model, selected_provider_missing,
};

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
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
use crate::input::PromptInput;
use crate::providers::{codex, opencode, umans};
use crate::thndrs_core::auth;
use crate::tools::shell::ProcessRegistry;
use crate::{config, fuzzy, internals, prompt, session, skills, tools, utils};

/// How long a Ctrl+D quit confirmation stays armed.
const QUIT_CONFIRM_TIMEOUT_MS: u64 = 3_000;
/// How long the UI waits for an agent to acknowledge cancellation before
/// releasing the stopped prompt.
const STOPPING_GRACE_MS: u64 = 250;

pub const VISIBLE_ROWS: usize = 8;

/// Shared cap for large filesystem-backed picker inventories so fuzzy matching
/// stays responsive while still surfacing enough nearby files or skills.
const LARGE_PICKER_LIMIT: usize = 200;
const MODEL_PICKER_LIMIT: usize = 50;

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
    /// One successful provider request with exact size and optional usage.
    RequestAccounting(Box<ProviderRequestAccounting>),
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
    /// Most recent completed provider request accounting for `/tokens` and
    /// context export. The request projection is in-memory only.
    pub last_request_accounting: Option<ProviderRequestAccounting>,
    /// Discovered Agent Skills metadata.
    pub skills: Vec<skills::SkillMetadata>,
    /// Skill discovery diagnostics for ignored malformed skills.
    pub skill_diagnostics: Vec<skills::SkillDiagnostic>,
    /// Reusable prompt templates exposed through slash-command completion.
    pub prompt_templates: Vec<prompt::templates::PromptTemplate>,
    /// Non-fatal diagnostics for malformed or unreadable prompt templates.
    pub prompt_template_diagnostics: Vec<prompt::templates::PromptTemplateDiagnostic>,
    /// Monotonic UI tick used for lightweight animated affordances.
    pub ui_tick: u64,
    /// When `Some`, the user pressed Ctrl+D once and we are waiting for a
    /// second press within roughly three seconds to actually quit. The value
    /// is the tick deadline at which the pending confirmation expires.
    pub ctrl_d_pending: Option<u64>,
    /// Tick deadline that bounds how long a cancelled run may remain in the
    /// `Stopping` state while its worker unwinds.
    pub stopping_deadline: Option<u64>,
    /// Append-only session writer. `None` when persistence is disabled
    /// (e.g. the sessions directory is not writable).
    pub session_writer: Option<session::SessionWriter>,
    /// Tool-call ids mapped to their bounded redacted recovery handles.
    pub tool_artifacts: HashMap<String, String>,
    /// State-aware model-projection decisions indexed by tool-call id.
    pub(crate) tool_projection_decisions: HashMap<String, agent_context::StateProjectionDecision>,
    /// Durable lifecycle/protection state reconstructed from append-only
    /// context records and applied to each new ledger snapshot.
    pub(crate) context_lifecycles: BTreeMap<String, agent_context::ContextLifecycle>,
    /// Monotonic turn counter for session record correlation.
    pub turn_count: u64,
    /// Registry of background processes started via `run_shell`.
    pub process_registry: ProcessRegistry,
    /// The active provider prompt, retained so user input can be restored on
    /// provider failure. This can be an internal compaction request that is
    /// intentionally absent from the visible transcript. Cleared on successful
    /// completion.
    pub last_input: Option<String>,
    /// In-flight compaction (manual or automatic). The original active
    /// context is retained until the configured provider summary and audit
    /// record both succeed.
    pending_manual_compaction: Option<context::PendingManualCompaction>,
    /// A generated summary awaiting explicit review before it changes the
    /// active working set.
    pending_compaction_review: Option<context::PendingCompactionReview>,
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
    /// Short-lived browser PKCE callback owned by the application adapter.
    pub chatgpt_browser_login: Option<auth::ChatGptCodexBrowserLogin>,
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
        cli_snapshot.tick_rate_ms = cli_snapshot.tick_rate_ms.max(MIN_TICK_RATE_MS);
        let context_inventory = crate::context::discover_instructions(&workspace_root);
        let context_sources = context_inventory.sources;
        let context_diagnostics = context_inventory.diagnostics;
        let skill_inventory = skills::discover(&workspace_root, &value.skill_dirs);
        let prompt_template_inventory = prompt::templates::discover(&workspace_root);
        let transcript = Vec::new();
        let sessions_dir = value
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&workspace_root));
        let session_id = session::generate_session_id();
        let input_history_store = InputHistoryStore::for_workspace(&workspace_root);
        let input_history = input_history_store.load_recent().ok().flatten().unwrap_or_default();
        let (mcp_config_files, mcp_config_diagnostics) = agent_lifecycle::load_mcp_config_audit(&workspace_root);

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

        let mut app = App {
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
            git_status: git::collect(&workspace_root),
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
            last_request_accounting: None,
            skills: skill_inventory.skills,
            skill_diagnostics: skill_inventory.diagnostics,
            prompt_templates: prompt_template_inventory.templates,
            prompt_template_diagnostics: prompt_template_inventory.diagnostics,
            ui_tick: 0,
            ctrl_d_pending: None,
            stopping_deadline: None,
            session_writer,
            tool_artifacts: HashMap::new(),
            tool_projection_decisions: HashMap::new(),
            context_lifecycles: BTreeMap::new(),
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
            chatgpt_browser_login: None,
            queued_steering: Vec::new(),
            queued_followups: Vec::new(),
            kill_ring: Vec::new(),
            detail_pane: DetailPane::default(),
            pending_permission: None,
            config_diagnostics: value.config_diagnostics.clone(),
            mcp_config_files,
            mcp_config_diagnostics,
            quit: false,
        };

        app.first_run_recovery = selected_provider_missing(&app);
        app
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

    /// Whether a compaction turn is currently in flight.
    ///
    /// Used by the preflight gate to avoid re-triggering auto-compaction while
    /// the configured-model summary request is the active turn.
    pub fn compaction_in_flight(&self) -> bool {
        self.pending_manual_compaction.is_some()
    }

    /// Return the local bounded artifact store for this session workspace.
    ///
    /// The store is deliberately separate from JSONL so session records carry
    /// metadata and handles without making artifact bodies part of replay truth.
    pub fn artifact_store(&self) -> crate::artifacts::ArtifactStore {
        crate::artifacts::ArtifactStore::new(self.session_directory().join("artifacts"))
    }

    /// Render the bounded `/tokens` inspection projection.
    pub fn token_accounting_status(&self) -> String {
        let Some(accounting) = &self.last_request_accounting else {
            return format!(
                "tokens\nsession totals: in {} out {}\nrequest accounting: unavailable",
                self.session_tokens_in, self.session_tokens_out
            );
        };
        let estimate = accounting
            .estimated_input_tokens
            .value
            .map_or_else(|| "unknown".to_string(), |value| value.to_string());
        let estimate_source = match &accounting.estimated_input_tokens.provenance {
            thndrs_agent::MeasurementProvenance::Estimated { version, .. } => format!("estimated/{version}"),
            _ => "unknown".to_string(),
        };
        let Some(usage) = accounting.provider_usage.as_ref() else {
            return format!(
                "tokens\nrequest: {} attempt {}\nlocal: {} bytes, {} tokens ({})\nprovider: unknown\nshadow receipts: {}\napplied receipts: {}\nbaseline fallback receipts: {}",
                accounting.request_id,
                accounting.attempt,
                accounting.serialized_bytes.value,
                estimate,
                estimate_source,
                accounting.shadow_receipts.len(),
                accounting.applied_receipts.len(),
                accounting.fallback_receipts.len()
            );
        };
        let inclusive = usage
            .inclusive_input_tokens
            .value
            .map_or_else(|| "unknown".to_string(), |value| value.to_string());
        let estimate_error = match (
            accounting.estimated_input_tokens.value,
            usage.inclusive_input_tokens.value,
        ) {
            (Some(estimated), Some(provider)) => (provider as i128 - estimated as i128).to_string(),
            _ => "unknown".to_string(),
        };
        format!(
            "tokens\nrequest: {} attempt {}\nlocal: {} bytes, {} tokens ({})\nprovider {} reported: {} input / {} output\ncache: {} read / {} create\nnormalized input: {} ({})\nestimate error: {} tokens\nshadow receipts: {}\napplied receipts: {}\nbaseline fallback receipts: {}",
            accounting.request_id,
            accounting.attempt,
            accounting.serialized_bytes.value,
            estimate,
            estimate_source,
            usage.provider,
            display_token(usage.components.input_tokens),
            display_token(usage.components.output_tokens),
            display_token(usage.components.cache_read_input_tokens),
            display_token(usage.components.cache_creation_input_tokens),
            inclusive,
            usage.rule.label(),
            estimate_error,
            accounting.shadow_receipts.len(),
            accounting.applied_receipts.len(),
            accounting.fallback_receipts.len()
        )
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
        diagnostics.extend(
            self.prompt_template_diagnostics
                .iter()
                .map(prompt::templates::PromptTemplateDiagnostic::summary),
        );
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

    fn refresh_git_status(&mut self) {
        self.git_status = git::collect(&self.cwd);
    }

    fn session_directory(&self) -> PathBuf {
        self.cli
            .session_dir
            .clone()
            .unwrap_or_else(|| session::sessions_dir(&self.cwd))
    }

    /// Resolve the configured compaction policy from loaded config layers.
    pub fn effective_compaction_policy(&self) -> CompactionPolicy {
        let config = self
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
        self.cli
            .config_layers
            .iter()
            .map(|layer| &layer.config.context.reduction)
            .rev()
            .find(|config| **config != ReductionConfig::default())
            .cloned()
            .unwrap_or_else(|| self.cli.context.reduction.clone())
    }
}

fn display_token(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

/// The only mutation path. Returns an optional follow-up message.
pub fn update(app: &mut App, msg: &Msg) -> Option<Msg> {
    match msg {
        Msg::Key(key) => handle_key(app, *key),
        Msg::Mouse(mouse) => handle_mouse(app, *mouse),
        Msg::Quit => {
            let results = app.process_registry.shutdown();
            agent_lifecycle::record_background_results(app, results);
            app.quit = true;
            None
        }
        Msg::Tick => {
            app.ui_tick = app.ui_tick.wrapping_add(1);
            if let Some(deadline) = app.ctrl_d_pending
                && agent_lifecycle::now_or_after_deadline(app.ui_tick, deadline)
            {
                app.ctrl_d_pending = None;
            }
            agent_lifecycle::drain_background_processes(app);
            agent_lifecycle::finish_stopping_if_due(app);
            poll_chatgpt_oauth_on_tick(app);
            None
        }
        Msg::Clear => {
            app.transcript.clear();
            app.detail_pane = DetailPane::default();
            None
        }
        Msg::Agent(event) => agent_lifecycle::handle_agent_event(app, event.clone()),
        Msg::GitStatusChanged(status) => {
            app.git_status = status.clone();
            None
        }
    }
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

/// Translate the fixed Ctrl+D confirmation window to the configured tick cadence.
///
/// This keeps the user-visible timeout stable when a faster render cadence is
/// selected for smoother streaming output.
fn quit_confirm_timeout_ticks(app: &App) -> u64 {
    let tick_ms = app.cli.tick_rate_ms.max(1);
    QUIT_CONFIRM_TIMEOUT_MS / tick_ms + u64::from(!QUIT_CONFIRM_TIMEOUT_MS.is_multiple_of(tick_ms))
}
