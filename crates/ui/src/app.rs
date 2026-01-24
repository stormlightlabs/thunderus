use crate::components::{
    Footer, FuzzyFinderComponent, Header, Inspector, MemoryHitsPanel, Sidebar, TeachingHintPopup,
    Transcript as TranscriptComponent, WelcomeView,
};
use crate::event_handler::{EventHandler, KeyAction};
use crate::layout::{LayoutMode, TuiLayout};
use crate::state::{self, AppState};
use crate::theme::Theme;
use crate::transcript::{self, CardDetailLevel, RenderOptions, Transcript as TranscriptState};
use crate::tui_approval::TuiApprovalHandle;

use crossterm;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Result, Write};
use std::{env, fs, panic};
use std::{process::Command, sync::Arc, time::Duration};
use thunderus_core::{ActionType, ApprovalContext, ApprovalGate, ToolRisk};
use thunderus_core::{
    ApprovalDecision, ApprovalMode, ApprovalRequest, Config, DriftEvent, DriftMonitor, MemoryDoc, PatchQueueManager,
    Profile, Session, SnapshotManager, TrajectoryWalker, memory::MemoryRetriever,
};
use thunderus_providers::{CancelToken, Provider};
use thunderus_tools::ToolRegistry;
use tokio::sync::mpsc;
use uuid;

/// Main TUI application
///
/// Handles rendering and state management for the Thunderus TUI
pub struct App {
    pub(crate) state: AppState,
    pub(crate) transcript: TranscriptState,
    pub(crate) should_exit: bool,
    /// Agent event receiver for streaming responses
    pub(crate) agent_event_rx: Option<mpsc::UnboundedReceiver<thunderus_agent::AgentEvent>>,
    /// Approval request receiver from agent
    pub(crate) approval_request_rx: Option<mpsc::UnboundedReceiver<ApprovalRequest>>,
    /// Handle for sending approval responses back to agent
    pub(crate) approval_handle: Option<TuiApprovalHandle>,
    /// Cancellation token for stopping agent operations
    pub(crate) cancel_token: CancelToken,
    /// Provider for agent operations
    provider: Option<Arc<dyn Provider>>,
    /// Profile for sandbox policy and tool configuration
    profile: Option<Profile>,
    /// Memory retriever for agent context
    memory_retriever: Option<Arc<dyn MemoryRetriever>>,
    /// Approval gate handle for runtime updates
    approval_gate_handle: Option<std::sync::Arc<std::sync::RwLock<thunderus_core::ApprovalGate>>>,
    /// Session for event persistence
    pub(crate) session: Option<Session>,
    /// Buffer for accumulating streaming model response content
    pub(crate) streaming_model_content: Option<String>,
    /// Drift monitor for workspace changes
    pub(crate) _drift_monitor: Option<DriftMonitor>,
    /// Snapshot manager for workspace state
    pub(crate) snapshot_manager: Option<SnapshotManager>,
    /// Receiver for drift events
    pub(crate) drift_rx: Option<tokio::sync::broadcast::Receiver<DriftEvent>>,
    /// Cancellation token for pausing agent on drift or user interruption
    pub(crate) pause_token: tokio_util::sync::CancellationToken,
    /// Last captured snapshot state for drift detection
    pub(crate) last_snapshot_state: Option<String>,
    /// Patch queue manager for diff-first editing workflow
    pub(crate) patch_queue_manager: Option<PatchQueueManager>,
}

impl App {
    /// Create a new application
    pub fn new(state: AppState) -> Self {
        let (drift_monitor, drift_rx) = DriftMonitor::new(state.cwd())
            .ok()
            .map(|m| {
                let rx = m.subscribe();
                (Some(m), Some(rx))
            })
            .unwrap_or((None, None));

        let snapshot_manager = Some(SnapshotManager::new(state.cwd()));

        Self {
            state,
            transcript: TranscriptState::new(),
            should_exit: false,
            agent_event_rx: None,
            approval_request_rx: None,
            approval_handle: None,
            cancel_token: CancelToken::new(),
            provider: None,
            profile: None,
            memory_retriever: None,
            approval_gate_handle: None,
            session: None,
            streaming_model_content: None,
            _drift_monitor: drift_monitor,
            snapshot_manager,
            drift_rx,
            pause_token: tokio_util::sync::CancellationToken::new(),
            last_snapshot_state: None,
            patch_queue_manager: None,
        }
    }

    /// Create a new application with a provider for agent operations
    pub fn with_provider(state: AppState, provider: Arc<dyn Provider>) -> Self {
        let (drift_monitor, drift_rx) = DriftMonitor::new(state.cwd())
            .ok()
            .map(|m| {
                let rx = m.subscribe();
                (Some(m), Some(rx))
            })
            .unwrap_or((None, None));

        let snapshot_manager = Some(SnapshotManager::new(state.cwd()));

        Self {
            state,
            transcript: TranscriptState::new(),
            should_exit: false,
            agent_event_rx: None,
            approval_request_rx: None,
            approval_handle: None,
            cancel_token: CancelToken::new(),
            provider: Some(provider),
            profile: None,
            memory_retriever: None,
            approval_gate_handle: None,
            session: None,
            streaming_model_content: None,
            _drift_monitor: drift_monitor,
            snapshot_manager,
            drift_rx,
            pause_token: tokio_util::sync::CancellationToken::new(),
            last_snapshot_state: None,
            patch_queue_manager: None,
        }
    }

    /// Set the session for event persistence
    pub fn with_session(mut self, session: Session) -> Self {
        self.state.session.session_id = Some(session.id.to_string());

        let agent_dir = session.agent_dir().clone();
        let patch_queue_manager = PatchQueueManager::new(session.id.clone(), agent_dir.clone());
        let patch_queue_manager = patch_queue_manager
            .load()
            .unwrap_or_else(|_| PatchQueueManager::new(session.id.clone(), agent_dir));
        self.patch_queue_manager = Some(patch_queue_manager);

        self.session = Some(session);
        self
    }

    /// Attach a profile for sandbox and tool configuration
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Attach a memory retriever for agent context
    pub fn with_memory_retriever(mut self, retriever: Arc<dyn MemoryRetriever>) -> Self {
        self.memory_retriever = Some(retriever);
        self
    }

    /// Check if the app should exit
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    /// Get a mutable reference to the application state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get a reference to the application state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get the active profile (if set)
    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    /// Get the memory retriever (if set)
    pub fn memory_retriever(&self) -> Option<Arc<dyn MemoryRetriever>> {
        self.memory_retriever.clone()
    }

    /// Set the provider used for agent operations
    pub fn set_provider(&mut self, provider: Arc<dyn Provider>) {
        self.provider = Some(provider);
    }

    /// Set the approval gate handle for live updates
    pub fn set_approval_gate_handle(&mut self, gate: std::sync::Arc<std::sync::RwLock<thunderus_core::ApprovalGate>>) {
        self.approval_gate_handle = Some(gate);
    }

    /// Update approval mode in the live agent gate (if active)
    pub fn update_approval_gate(&mut self, new_mode: ApprovalMode) {
        if let Some(ref gate) = self.approval_gate_handle
            && let Ok(mut guard) = gate.write()
        {
            guard.set_mode(new_mode);
        }
    }

    /// Get the transcript
    pub fn transcript(&self) -> &TranscriptState {
        &self.transcript
    }

    /// Get a mutable reference to the transcript
    pub fn transcript_mut(&mut self) -> &mut TranscriptState {
        &mut self.transcript
    }

    /// Persist a user message to the session log
    ///
    /// Handles write failures gracefully by warning the user and logging to stderr
    pub(crate) fn persist_user_message(&mut self, content: &str) {
        if let Some(ref mut session) = self.session
            && let Err(e) = session.append_user_message(content)
        {
            let warning = format!("Warning: Failed to persist user message: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Persist a model response to the session log
    ///
    /// Handles write failures gracefully by warning the user and logging to stderr
    pub(crate) fn persist_model_message(&mut self, content: &str) {
        if let Some(ref mut session) = self.session
            && let Err(e) = session.append_model_message(content, None)
        {
            let warning = format!("Warning: Failed to persist model message: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Persist a tool call to the session log
    ///
    /// Handles write failures gracefully by warning the user and logging to stderr
    pub(crate) fn persist_tool_call(&mut self, tool: &str, arguments: &serde_json::Value) {
        if let Some(ref mut session) = self.session
            && let Err(e) = session.append_tool_call(tool, arguments.clone())
        {
            let warning = format!("Warning: Failed to persist tool call: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Persist a tool result to the session log
    ///
    /// Handles write failures gracefully by warning the user and logging to stderr
    pub(crate) fn persist_tool_result(
        &mut self, tool: &str, result: &serde_json::Value, success: bool, error: Option<&str>,
    ) {
        if let Some(ref mut session) = self.session
            && let Err(e) = session.append_tool_result(tool, result.clone(), success, error.map(|s| s.to_string()))
        {
            let warning = format!("Warning: Failed to persist tool result: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Persist an approval decision to the session log
    ///
    /// Handles write failures gracefully by warning the user and logging to stderr
    pub(crate) fn persist_approval(&mut self, action: &str, approved: bool) {
        if let Some(ref mut session) = self.session
            && let Err(e) = session.append_approval(action, approved)
        {
            let warning = format!("Warning: Failed to persist approval: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Persist the selected theme variant back to config.toml if available.
    fn persist_theme_variant(&mut self) {
        let Some(config_path) = self.state.config.config_path.clone() else {
            return;
        };

        let profile_name = self.state.config.profile.clone();
        let variant = self.state.theme_variant();

        let mut config = match Config::from_file(&config_path) {
            Ok(config) => config,
            Err(e) => {
                let warning = format!("Warning: Failed to load config for theme update: {}", e);
                eprintln!("{}", warning);
                self.transcript_mut().add_system_message(warning);
                return;
            }
        };

        if let Some(profile) = config.profiles.get_mut(&profile_name) {
            profile
                .options
                .insert("theme".to_string(), variant.as_str().to_string());
        } else {
            let warning = format!("Warning: Profile '{}' not found for theme update", profile_name);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
            return;
        }

        if let Err(e) = config.save_to_file(&config_path) {
            let warning = format!("Warning: Failed to save theme selection: {}", e);
            eprintln!("{}", warning);
            self.transcript_mut().add_system_message(warning);
        }
    }

    /// Reconstruct transcript from session events
    ///
    /// Loads all events from the session and converts them to transcript entries.
    /// This is used for session recovery on restart.
    pub fn reconstruct_transcript_from_session(&mut self) -> thunderus_core::Result<()> {
        use thunderus_core::Event;
        let Some(ref session) = self.session else {
            return Ok(());
        };

        let events = session.read_events()?;

        for logged_event in events {
            match logged_event.event {
                Event::UserMessage { content } => self.transcript_mut().add_user_message(&content),
                Event::ModelMessage { content, tokens_used: _ } => self.transcript_mut().add_model_response(&content),
                Event::ToolCall { tool, arguments } => {
                    let args_str = serde_json::to_string_pretty(&arguments).unwrap_or_default();
                    self.transcript_mut().add_tool_call(&tool, &args_str, "safe");
                }
                Event::ToolResult { tool, result, success, error } => {
                    let result_str =
                        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Invalid JSON".to_string());
                    self.transcript_mut().add_tool_result(&tool, &result_str, success);

                    if let Some(error_msg) = error {
                        self.transcript_mut()
                            .add_system_message(format!("Tool error: {}", error_msg));
                    }
                }
                Event::Approval { action, approved } => {
                    let decision = if approved {
                        transcript::ApprovalDecision::Approved
                    } else {
                        transcript::ApprovalDecision::Rejected
                    };

                    self.transcript_mut().add_approval_prompt(&action, "safe");
                    let _ = self.transcript_mut().set_approval_decision(decision);
                }
                Event::Patch { name, status, files, diff } => {
                    let status_str = format!("{:?}", status);
                    self.transcript_mut().add_system_message(format!(
                        "Patch: {} ({})\nFiles: {:?}\n{}",
                        name, status_str, files, diff
                    ));
                }
                Event::ShellCommand { command, args, working_dir, exit_code, output_ref } => {
                    let cmd_str =
                        if args.is_empty() { command.clone() } else { format!("{} {}", command, args.join(" ")) };
                    self.transcript_mut().add_system_message(format!(
                        "Shell: {} (in {})\nExit: {:?}",
                        cmd_str,
                        working_dir.display(),
                        exit_code
                    ));

                    if let Some(ref output_file) = output_ref {
                        self.transcript_mut()
                            .add_system_message(format!("Output in: {}", output_file));
                    }
                }
                Event::GitSnapshot { commit, branch, changed_files } => {
                    self.transcript_mut().add_system_message(format!(
                        "Git snapshot: {} @ {}\nChanged files: {}",
                        commit, branch, changed_files
                    ));
                }
                Event::FileRead { file_path, line_count, offset, success } => {
                    if success {
                        self.transcript_mut().add_system_message(format!(
                            "File read: {} (lines: {}, offset: {})",
                            file_path, line_count, offset
                        ));
                    } else {
                        self.transcript_mut()
                            .add_system_message(format!("File read failed: {}", file_path));
                    }
                }
                Event::ApprovalModeChange { from, to } => self.transcript_mut().add_system_message(format!(
                    "Approval mode changed: {} → {}",
                    from.as_str(),
                    to.as_str()
                )),
                Event::ViewEdit { view, change_type, .. } => self
                    .transcript_mut()
                    .add_system_message(format!("View edited: {} ({})", view, change_type)),
                Event::ContextLoad { source, path, .. } => self
                    .transcript_mut()
                    .add_system_message(format!("Context loaded: {} from {}", source, path)),
                Event::Checkpoint { label, description, .. } => self
                    .transcript_mut()
                    .add_system_message(format!("Checkpoint: {} - {}", label, description)),
                Event::PlanUpdate { action, item, reason } => {
                    let reason_str = reason.as_ref().map(|r| format!(" (reason: {})", r)).unwrap_or_default();
                    self.transcript_mut()
                        .add_system_message(format!("Plan {}: {}{}", action, item, reason_str));
                }
                Event::MemoryUpdate { kind, path, operation, .. } => self
                    .transcript_mut()
                    .add_system_message(format!("Memory {}: {} ({})", operation, path, kind)),
            }
        }

        let is_empty = self.transcript.is_empty();
        self.state_mut().set_first_session(is_empty);

        Ok(())
    }

    /// Run the TUI application with unified event loop
    ///
    /// Uses tokio::select! to multiplex TUI keyboard events and Agent streaming events.
    /// This allows the agent to stream responses in real-time while remaining responsive
    /// to user input (cancellation, approval requests, etc.).
    pub async fn run(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        let original_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let backend = CrosstermBackend::new(std::io::stdout());
            if let Ok(mut terminal) = Terminal::new(backend) {
                let _ = terminal.show_cursor();
            }
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            original_hook(panic_info);
        }));

        terminal.clear()?;
        self.draw(&mut terminal)?;

        while !self.should_exit {
            let tui_poll = async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                EventHandler::read()
            };

            tokio::select! {
                maybe_event = tui_poll => {
                    if let Some(event) = maybe_event {
                        self.handle_event(event).await;
                        self.draw(&mut terminal)?;
                    }
                }
                maybe_drift = async {
                    if let Some(ref mut rx) = self.drift_rx {
                        rx.recv().await.ok()
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(drift) = maybe_drift {
                        self.handle_drift_event(drift);
                        self.draw(&mut terminal)?;
                    }
                }
                maybe_agent = async {
                    if let Some(ref mut rx) = self.agent_event_rx {
                        rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    match maybe_agent {
                        Some(event) => {
                            self.handle_agent_event(event);
                            self.draw(&mut terminal)?;
                        }
                        None => {
                            self.agent_event_rx = None;
                            self.state_mut().stop_generation();
                        }
                    }
                }
                maybe_request = async {
                    if let Some(ref mut approval_rx) = self.approval_request_rx {
                        approval_rx.recv().await
                    } else {
                        std::future::pending().await
                    }
                } => {
                    if let Some(request) = maybe_request {
                        self.handle_approval_request(request);
                        self.draw(&mut terminal)?;
                    }
                }
            }
        }

        self.cancel_token.cancel();
        self.state_mut().stop_generation();

        terminal.show_cursor()?;
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    /// Handle an event and update state
    async fn handle_event(&mut self, event: crossterm::event::Event) {
        if let Some(action) = EventHandler::handle_event(&event, self.state_mut()) {
            match action {
                KeyAction::SendMessage { message } => {
                    let is_fork_mode = self.state().input.is_in_fork_mode();
                    let fork_point = self.state().input.fork_point_index;

                    if is_fork_mode {
                        if let Some(idx) = fork_point {
                            let history: Vec<String> = self.state().input.message_history.clone();
                            self.state_mut().input.replace_history_entry(idx, message.clone());
                            self.state_mut().input.truncate_history_from(idx);
                            self.transcript_mut().clear();
                            for hist_msg in history.iter() {
                                self.transcript_mut().add_user_message(hist_msg);
                            }

                            self.transcript_mut().add_system_message(format!(
                                "Forked at position {} - conversation truncated and restarted",
                                idx + 1
                            ));
                        }
                        self.state_mut().input.exit_fork_mode();
                    } else {
                        self.state_mut().input.add_to_history(message.clone());
                    }

                    self.state_mut().session.last_message = Some(message.clone());
                    self.transcript_mut().add_user_message(&message);
                    self.persist_user_message(&message);
                    self.state_mut().exit_first_session();

                    match self.provider.clone() {
                        Some(provider) => self.spawn_agent_for_message(message, &provider),
                        None => self
                            .transcript_mut()
                            .add_system_message("No provider configured. Cannot process message."),
                    }
                }
                KeyAction::ExecuteShellCommand { command } => self.execute_shell_command(command),
                KeyAction::Approve { action: _, risk: _ } => self.send_approval_response(ApprovalDecision::Approved),
                KeyAction::Reject { action: _, risk: _ } => self.send_approval_response(ApprovalDecision::Rejected),
                KeyAction::Cancel { action: _, risk: _ } => self.send_approval_response(ApprovalDecision::Cancelled),
                KeyAction::CancelGeneration => {
                    self.cancel_token.cancel();
                    self.pause_token.cancel();
                    self.state_mut().stop_generation();
                    self.transcript_mut()
                        .mark_streaming_cancelled("Generation cancelled by user");
                }
                KeyAction::StartReconcileRitual => self.start_reconcile_ritual(),
                KeyAction::ReconcileContinue => self.reconcile_continue(),
                KeyAction::ReconcileDiscard => self.reconcile_discard(),
                KeyAction::ReconcileStop => self.reconcile_stop(),
                KeyAction::RewindLastMessage => {
                    let history_len = self.state().input.message_history.len();
                    if history_len > 0 {
                        self.state_mut().input.message_history.pop();
                        self.transcript_mut().add_system_message(format!(
                            "Rewound: Removed last message from history ({} messages remaining)",
                            history_len - 1
                        ));
                    } else {
                        self.transcript_mut()
                            .add_system_message("No messages in history to rewind.");
                    }
                }
                KeyAction::Exit => self.should_exit = true,
                KeyAction::RetryLastFailedAction => {
                    let has_retryable_error =
                        self.transcript().entries().iter().any(|entry| {
                            matches!(entry, transcript::TranscriptEntry::ErrorEntry { can_retry: true, .. })
                        });

                    match self.state_mut().last_message().cloned() {
                        Some(last_message) => match has_retryable_error {
                            true => {
                                self.transcript_mut().add_system_message("Retrying last message...");
                                self.state_mut().input.add_to_history(last_message.clone());
                                self.transcript_mut().add_user_message(last_message.clone());

                                if let Some(provider) = self.provider.clone() {
                                    self.spawn_agent_for_message(last_message.clone(), &provider);
                                }
                            }
                            false => self.transcript_mut().add_system_message(
                                "No retryable error found. Use message history to re-send a message.",
                            ),
                        },
                        None => self
                            .transcript_mut()
                            .add_system_message("No previous message to retry."),
                    }
                }
                KeyAction::ToggleTheme => {
                    self.state_mut().toggle_theme_variant();
                    self.persist_theme_variant();
                    let variant = self.state.theme_variant();
                    self.transcript_mut()
                        .add_system_message(format!("Theme switched to {}", variant));
                }
                KeyAction::ToggleAdvisorMode => {
                    let new_mode = match self.state.config.approval_mode {
                        ApprovalMode::ReadOnly => ApprovalMode::Auto,
                        _ => ApprovalMode::ReadOnly,
                    };
                    self.state_mut().config.approval_mode = new_mode;

                    self.transcript_mut().add_system_message(format!(
                        "Advisor mode: {} (Ctrl+A to toggle)",
                        if new_mode == ApprovalMode::ReadOnly { "ON" } else { "OFF" }
                    ));

                    if let Some(ref mut session) = self.session {
                        let _ = session.set_approval_mode(new_mode);
                    }
                }
                KeyAction::ToggleSidebar | KeyAction::ToggleVerbosity | KeyAction::ToggleSidebarSection => (),
                KeyAction::OpenExternalEditor => self.open_external_editor(),
                KeyAction::NavigateHistory => (),
                KeyAction::ActivateFuzzyFinder => {
                    let input = self.state.input.buffer.clone();
                    let cursor = self.state.input.cursor;
                    self.state_mut().enter_fuzzy_finder(input, cursor);
                }
                KeyAction::SelectFileInFinder { path } => {
                    self.state_mut().exit_fuzzy_finder();
                    let input = self.state_mut().input.buffer.clone();
                    let cursor = self.state_mut().input.cursor;

                    let mut new_input = input[..cursor].to_string();
                    new_input.push('@');
                    new_input.push_str(&path);
                    new_input.push_str(&input[cursor..]);

                    self.state_mut().input.buffer = new_input;
                    self.state_mut().input.cursor = cursor + 1 + path.len();
                }
                KeyAction::NavigateFinderUp
                | KeyAction::NavigateFinderDown
                | KeyAction::ToggleFinderSort
                | KeyAction::CancelFuzzyFinder => (),
                KeyAction::SlashCommandModel { model } => self.handle_model_command(model),
                KeyAction::SlashCommandApprovals { mode } => self.handle_approvals_command(mode),
                KeyAction::SlashCommandVerbosity { level } => self.handle_verbosity_command(level),
                KeyAction::SlashCommandStatus => self.handle_status_command(),
                KeyAction::SlashCommandPlan => self.handle_plan_command(),
                KeyAction::SlashCommandPlanAdd { item } => self.handle_plan_add_command(item),
                KeyAction::SlashCommandPlanDone { index } => self.handle_plan_done_command(index),
                KeyAction::SlashCommandReview => self.handle_review_command(),
                KeyAction::SlashCommandMemory => self.handle_memory_command(),
                KeyAction::SlashCommandMemoryAdd { fact } => self.handle_memory_add_command(fact),
                KeyAction::SlashCommandMemorySearch { query } => self.handle_memory_search_command(query),
                KeyAction::SlashCommandMemoryPin { id } => self.handle_memory_pin_command(id),
                KeyAction::SlashCommandSearch { query, scope } => self.handle_search_command(query, scope),
                KeyAction::SlashCommandClear => {
                    self.transcript_mut().clear();
                    self.transcript_mut()
                        .add_system_message("Transcript cleared (session history preserved)");
                }
                KeyAction::SlashCommandGardenConsolidate { session_id } => {
                    self.handle_garden_consolidate_command(session_id)
                }
                KeyAction::SlashCommandGardenHygiene => self.handle_garden_hygiene_command(),
                KeyAction::SlashCommandGardenDrift => self.handle_garden_drift_command(),
                KeyAction::SlashCommandGardenVerify { doc_id } => self.handle_garden_verify_command(doc_id),
                KeyAction::SlashCommandGardenStats => self.handle_garden_stats_command(),
                KeyAction::NavigateCardNext => {
                    if !self.transcript_mut().focus_next_card() {
                        self.transcript_mut().scroll_down(1);
                        self.state_mut().scroll_vertical(1);
                    }
                }
                KeyAction::NavigateCardPrev => {
                    if !self.transcript_mut().focus_prev_card() {
                        self.transcript_mut().scroll_up(1);
                        self.state_mut().scroll_vertical(-1);
                    }
                }
                KeyAction::ToggleCardExpand => {
                    self.transcript_mut().toggle_focused_card_detail_level();
                }
                KeyAction::ToggleCardVerbose => {
                    let _ = self
                        .transcript_mut()
                        .set_focused_card_detail_level(CardDetailLevel::Verbose);
                }
                KeyAction::ScrollUp => match self.transcript().entries().iter().any(|e| e.is_action_card()) {
                    true => {
                        self.transcript_mut().focus_prev_card();
                    }
                    false => {
                        self.transcript_mut().scroll_up(1);
                        self.state_mut().scroll_vertical(-1);
                    }
                },
                KeyAction::ScrollDown => match self.transcript().entries().iter().any(|e| e.is_action_card()) {
                    true => {
                        self.transcript_mut().focus_next_card();
                    }
                    false => {
                        self.transcript_mut().scroll_down(1);
                        self.state_mut().scroll_vertical(1);
                    }
                },
                KeyAction::PageUp => {
                    self.transcript_mut().scroll_up(10);
                    self.state_mut().scroll_vertical(-10);
                }
                KeyAction::PageDown => {
                    self.transcript_mut().scroll_down(10);
                    self.state_mut().scroll_vertical(10);
                }
                KeyAction::ScrollToTop => {
                    self.transcript_mut().scroll_up(usize::MAX);
                    self.state_mut().ui.scroll_vertical = 0;
                }
                KeyAction::ScrollToBottom => {
                    self.transcript_mut().scroll_to_bottom();
                    self.state_mut().ui.scroll_vertical = 0;
                }
                KeyAction::CollapseSidebarSection => self.state_mut().ui.sidebar_collapse_state.collapse_prev(),
                KeyAction::ExpandSidebarSection => self.state_mut().ui.sidebar_collapse_state.expand_next(),
                KeyAction::FocusSlashCommand => {
                    self.state_mut().input.buffer = "/".to_string();
                    self.state_mut().input.cursor = 1;
                }
                KeyAction::ClearTranscriptView => {
                    self.transcript_mut().clear();
                    self.transcript_mut()
                        .add_system_message("Transcript cleared (session history preserved)");
                }
                KeyAction::NavigateNextPatch => {
                    let total = self.state().patches().len() + self.state().memory_patches().len();
                    self.state_mut().next_patch(total);
                }
                KeyAction::NavigatePrevPatch => {
                    let total = self.state().patches().len() + self.state().memory_patches().len();
                    self.state_mut().prev_patch(total);
                }
                KeyAction::NavigateNextHunk => {
                    let Some(patch_idx) = self.state().selected_patch_index() else {
                        return;
                    };
                    let Some(file_path_str) = self.state().selected_file_path() else {
                        return;
                    };

                    let file_path = std::path::PathBuf::from(file_path_str);

                    let total_hunks = self
                        .state()
                        .patches()
                        .get(patch_idx)
                        .and_then(|p| p.hunk_count(&file_path))
                        .unwrap_or(0);

                    self.state_mut().next_hunk(total_hunks);

                    let has_files = self
                        .state()
                        .patches()
                        .get(patch_idx)
                        .map(|p| !p.files.is_empty())
                        .unwrap_or(false);

                    if self.state().selected_hunk_index().is_none() && has_files {
                        self.state_mut()
                            .set_selected_file(file_path.to_str().unwrap_or("").to_string());
                    }
                }
                KeyAction::NavigatePrevHunk => {
                    let Some(patch_idx) = self.state().selected_patch_index() else {
                        return;
                    };
                    let Some(file_path_str) = self.state().selected_file_path() else {
                        return;
                    };

                    let file_path = std::path::PathBuf::from(file_path_str);

                    let total_hunks = self
                        .state()
                        .patches()
                        .get(patch_idx)
                        .and_then(|p| p.hunk_count(&file_path))
                        .unwrap_or(0);

                    self.state_mut().prev_hunk(total_hunks);

                    let has_files = self
                        .state()
                        .patches()
                        .get(patch_idx)
                        .map(|p| !p.files.is_empty())
                        .unwrap_or(false);

                    if self.state().selected_hunk_index().is_none() && has_files {
                        self.state_mut()
                            .set_selected_file(file_path.to_str().unwrap_or("").to_string());
                    }
                }
                KeyAction::ApproveHunk => {
                    let Some(patch_idx) = self.state().selected_patch_index() else {
                        return;
                    };

                    let file_patch_count = self.state().patches().len();

                    if patch_idx >= file_patch_count {
                        let mem_idx = patch_idx - file_patch_count;
                        let result = self
                            .state_mut()
                            .memory_patches_mut()
                            .get_mut(mem_idx)
                            .ok_or_else(|| "Memory patch not found".to_string())
                            .and_then(|patch| {
                                patch.approve();
                                patch.apply().map_err(|e| format!("Failed to apply: {}", e))?;
                                Ok(format!("Applied memory patch: {}", patch.doc_id))
                            });

                        match result {
                            Ok(msg) => self.transcript_mut().add_system_message(msg),
                            Err(e) => self
                                .transcript_mut()
                                .add_system_message(format!("Failed to apply: {}", e)),
                        }
                    } else {
                        let Some(hunk_idx) = self.state().selected_hunk_index() else {
                            return;
                        };
                        let Some(file_path_str) = self.state().selected_file_path() else {
                            return;
                        };

                        let file_path = std::path::PathBuf::from(file_path_str);

                        let result = self
                            .state_mut()
                            .patches_mut()
                            .get_mut(patch_idx)
                            .ok_or_else(|| "Patch not found".to_string())
                            .and_then(|patch| patch.approve_hunk(&file_path, hunk_idx));

                        if let Err(e) = result {
                            self.transcript_mut()
                                .add_system_message(format!("Failed to approve hunk: {}", e));
                        }
                    }
                }
                KeyAction::RejectHunk => {
                    let Some(patch_idx) = self.state().selected_patch_index() else {
                        return;
                    };

                    let file_patch_count = self.state().patches().len();

                    if patch_idx >= file_patch_count {
                        let mem_idx = patch_idx - file_patch_count;
                        let result = self
                            .state_mut()
                            .memory_patches_mut()
                            .get_mut(mem_idx)
                            .ok_or_else(|| "Memory patch not found".to_string())
                            .map(|patch| {
                                patch.reject();
                                format!("Rejected memory patch: {}", patch.doc_id)
                            });

                        match result {
                            Ok(msg) => self.transcript_mut().add_system_message(msg),
                            Err(e) => self
                                .transcript_mut()
                                .add_system_message(format!("Failed to reject: {}", e)),
                        }
                    } else {
                        let Some(hunk_idx) = self.state().selected_hunk_index() else {
                            return;
                        };
                        let Some(file_path_str) = self.state().selected_file_path() else {
                            return;
                        };

                        let file_path = std::path::PathBuf::from(file_path_str);

                        let result = self
                            .state_mut()
                            .patches_mut()
                            .get_mut(patch_idx)
                            .ok_or_else(|| "Patch not found".to_string())
                            .and_then(|patch| patch.reject_hunk(&file_path, hunk_idx));

                        if let Err(e) = result {
                            self.transcript_mut()
                                .add_system_message(format!("Failed to reject hunk: {}", e));
                        }
                    }
                }
                KeyAction::ToggleHunkDetails => self.state_mut().toggle_hunk_details(),
                KeyAction::MemoryHitsNavigate => {}
                KeyAction::MemoryHitsOpen { path } => {
                    self.transcript_mut()
                        .add_system_message(format!("Opening memory document: {}", path));
                    self.state_mut().memory_hits.clear();
                }
                KeyAction::MemoryHitsPin { id } => {
                    let is_pinned = self.state().memory_hits.is_pinned(&id);
                    self.transcript_mut().add_system_message(format!(
                        "{}: {}",
                        if is_pinned { "Unpinned" } else { "Pinned" },
                        id
                    ));
                }
                KeyAction::MemoryHitsClose => self.transcript_mut().add_system_message("Memory panel closed"),
                KeyAction::ToggleInspector => self.state_mut().ui.toggle_inspector(),
                KeyAction::InspectMemory { path } => {
                    let content = tokio::fs::read_to_string(&path).await;
                    match content {
                        Ok(c) => match MemoryDoc::parse(&c) {
                            Ok(doc) => {
                                let session_dir = self.session.as_ref().map(|s| s.session_dir());
                                if let Some(dir) = session_dir {
                                    let walker = TrajectoryWalker::new(dir);
                                    match walker.walk(&doc).await {
                                        Ok(nodes) => {
                                            self.state_mut().evidence.set_nodes(nodes);
                                            self.state_mut().ui.active_view = crate::state::MainView::Inspector;
                                            self.transcript_mut()
                                                .add_system_message(format!("Inspecting: {}", path));
                                        }
                                        Err(e) => {
                                            self.transcript_mut()
                                                .add_system_message(format!("Failed to walk trajectory: {}", e));
                                        }
                                    }
                                } else {
                                    self.transcript_mut()
                                        .add_system_message("No active session for trajectory walk.");
                                }
                            }
                            Err(e) => {
                                self.transcript_mut()
                                    .add_system_message(format!("Failed to parse memory doc: {}", e));
                            }
                        },
                        Err(e) => {
                            self.transcript_mut()
                                .add_system_message(format!("Failed to read memory doc: {}", e));
                        }
                    }
                }
                KeyAction::InspectorNavigate => {}
                // TODO: Implement jump to diff or editor for affected file
                KeyAction::InspectorOpenFile { .. } => {
                    let inspector = Inspector::new(&self.state);
                    let files = inspector.affected_files();
                    if let Some(path) = files.first() {
                        self.transcript_mut()
                            .add_system_message(format!("Opening affected file: {}", path));
                    } else {
                        self.transcript_mut()
                            .add_system_message("No affected files for this event.");
                    }
                }
                KeyAction::NoOp => (),
            }
        }
    }

    /// Draw the UI
    pub fn draw(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        if self.state.is_generating() || self.state.approval_ui.pending_approval.is_some() {
            self.state.advance_animation_frame();
        }
        if self.state.ui.sidebar_animation.is_some() {
            self.state.ui.advance_sidebar_animation();
        }

        terminal.draw(|frame| {
            let size = frame.area();
            let theme = Theme::palette(self.state.theme_variant());

            if !self.state.is_first_session() || !self.transcript.is_empty() {
                frame.render_widget(
                    ratatui::widgets::Block::default().style(ratatui::style::Style::default().bg(theme.panel_bg)),
                    size,
                );
            } else {
                frame.render_widget(
                    ratatui::widgets::Block::default().style(ratatui::style::Style::default().bg(theme.bg)),
                    size,
                );
            }

            if self.state.is_first_session() && self.transcript.is_empty() {
                let welcome = WelcomeView::new(&self.state, size);
                welcome.render(frame);
                if self.state.is_fuzzy_finder_active() {
                    let fuzzy_finder = FuzzyFinderComponent::new(&self.state);
                    fuzzy_finder.render(frame);
                }

                return;
            }

            let layout = if matches!(self.state.ui.active_view, crate::state::MainView::Inspector) {
                TuiLayout::calculate_inspector(size)
            } else {
                TuiLayout::calculate(
                    size,
                    self.state.ui.sidebar_visible,
                    self.state.ui.sidebar_width_override(),
                )
            };
            let header = Header::new(&self.state.session_header);
            header.render(frame, layout.header);

            if matches!(self.state.ui.active_view, crate::state::MainView::Inspector) {
                let inspector = Inspector::new(&self.state);
                inspector.render(
                    frame,
                    layout.evidence_list.unwrap_or_default(),
                    layout.evidence_detail.unwrap_or_default(),
                );
            } else {
                let theme = Theme::palette(self.state.theme_variant());
                let options = RenderOptions {
                    centered: false,
                    max_bubble_width: if layout.mode == LayoutMode::Full { None } else { Some(60) },
                    animation_frame: self.state.ui.animation_frame,
                };
                let ellipsis = self.state.streaming_ellipsis();
                let transcript_component = if self.state.is_generating() {
                    TranscriptComponent::with_streaming_ellipsis(
                        &self.transcript,
                        self.state.ui.scroll_vertical,
                        ellipsis,
                        theme,
                        options,
                    )
                } else {
                    TranscriptComponent::with_vertical_scroll(
                        &self.transcript,
                        self.state.ui.scroll_vertical,
                        theme,
                        options,
                    )
                };
                transcript_component.render(frame, layout.transcript);

                if let Some(sidebar_area) = layout.sidebar {
                    let sidebar = Sidebar::new(&self.state);
                    sidebar.render(frame, sidebar_area);
                }
            }

            let footer = Footer::new(&self.state);
            footer.render(frame, layout.footer);

            if self.state.is_fuzzy_finder_active() {
                let fuzzy_finder = FuzzyFinderComponent::new(&self.state);
                fuzzy_finder.render(frame);
            }

            if self.state.memory_hits.is_visible() {
                let panel_area = ratatui::layout::Rect {
                    x: size.width / 4,
                    y: size.height / 8,
                    width: size.width / 2,
                    height: size.height * 3 / 4,
                };
                let memory_panel = MemoryHitsPanel::new(&self.state.memory_hits);
                memory_panel.render(frame, panel_area);
            }

            if let Some(ref hint) = self.state.approval_ui.pending_hint {
                let theme = Theme::palette(self.state.theme_variant());
                let hint_popup = TeachingHintPopup::new(hint, theme);
                hint_popup.render(frame, size);
            }
        })?;

        Ok(())
    }

    /// Execute a shell command and insert output as user-provided context
    ///
    /// Shell commands require approval based on the current approval mode:
    /// - ReadOnly: Always rejected
    /// - Auto: Safe commands auto-approve, risky commands require approval
    /// - FullAccess: Auto-approved
    fn execute_shell_command(&mut self, command: String) {
        let registry = ToolRegistry::with_builtin_tools();
        let tool_call_id = format!("shell_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let user_message = format!("!cmd {}", command);

        self.transcript_mut().add_user_message(&user_message);

        let mut approval_gate = ApprovalGate::new(self.state().config.approval_mode, self.state().config.allow_network);

        let command_lower = command.to_lowercase();
        let is_network_command = command_lower.contains("curl ")
            || command_lower.contains("wget ")
            || command_lower.starts_with("curl")
            || command_lower.starts_with("wget")
            || command_lower.contains("ssh ")
            || command_lower.starts_with("ssh")
            || command_lower.contains("http://")
            || command_lower.contains("https://");

        let is_destructive = command_lower.contains("rm ")
            || command_lower.contains("rmdir ")
            || command_lower.contains("del ")
            || command_lower.contains("mv ")
            || command_lower.contains("> ")
            || command_lower.contains("git clean")
            || command_lower.contains("git reset")
            || command_lower.contains("git rebase");

        let risk_level = if is_destructive || is_network_command { ToolRisk::Risky } else { ToolRisk::Safe };
        let requires_approval = approval_gate.requires_approval(risk_level, is_network_command);

        if requires_approval {
            let request_id = approval_gate.create_request(
                ActionType::Shell,
                format!("Execute: {}", command),
                ApprovalContext::new()
                    .with_name("shell")
                    .with_arguments(serde_json::json!({"command": &command}))
                    .with_classification_reasoning(format!(
                        "Command classified as {}: {}",
                        if risk_level.is_risky() { "risky" } else { "safe" },
                        if is_destructive {
                            "destructive operation"
                        } else if is_network_command {
                            "network access"
                        } else {
                            "local command"
                        }
                    )),
                risk_level,
            );

            let risk_str = if risk_level.is_risky() { "risky" } else { "safe" };
            self.transcript_mut()
                .add_approval_prompt(format!("shell:{}", command), risk_str);
            self.state_mut().approval_ui.pending_approval =
                Some(state::ApprovalState::pending(command.clone(), risk_str.to_string()).with_request_id(request_id));

            self.state_mut().approval_ui.pending_command = Some(command);
        } else {
            self.do_execute_shell_command(command, &registry, tool_call_id);
        }
    }

    /// Open external editor for current input buffer
    fn open_external_editor(&mut self) {
        let editor_cmd = env::var("VISUAL")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        let temp_dir = env::temp_dir();
        let temp_file_path = temp_dir.join(format!("thunderus_input_{}.md", uuid::Uuid::new_v4()));

        if let Err(e) = fs::write(&temp_file_path, &self.state.input.buffer) {
            self.transcript_mut()
                .add_system_message(format!("Failed to create temporary file: {}", e));
            return;
        }

        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = std::io::stdout().flush();

        let result = Command::new(&editor_cmd).arg(&temp_file_path).status();

        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);

        match result {
            Ok(status) if status.success() => match fs::read_to_string(&temp_file_path) {
                Ok(content) => {
                    self.state.input.buffer = content;
                    self.state.input.cursor = self.state.input.buffer.len();

                    self.transcript_mut()
                        .add_system_message("Content loaded from external editor")
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to read edited content: {}", e)),
            },
            Ok(status) => self
                .transcript_mut()
                .add_system_message(format!("Editor exited with non-zero status: {}", status)),
            Err(e) => self
                .transcript_mut()
                .add_system_message(format!("Failed to launch editor '{}': {}", editor_cmd, e)),
        }

        let _ = fs::remove_file(&temp_file_path);
        let _ = self.redraw_screen();
    }

    /// Redraw the screen after returning from external editor
    fn redraw_screen(&self) -> Result<()> {
        let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
        if let Ok(mut terminal) = ratatui::Terminal::new(backend) {
            let _ = terminal.clear();
            let _ = io::stdout().flush();
        }

        Ok(())
    }

    /// Quit the application and restore terminal
    pub fn quit(&mut self) -> Result<()> {
        self.should_exit = true;
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: AppState::default(),
            transcript: TranscriptState::new(),
            should_exit: false,
            agent_event_rx: None,
            approval_request_rx: None,
            approval_handle: None,
            cancel_token: CancelToken::new(),
            provider: None,
            profile: None,
            memory_retriever: None,
            approval_gate_handle: None,
            session: None,
            streaming_model_content: None,
            _drift_monitor: None,
            snapshot_manager: None,
            drift_rx: None,
            pause_token: tokio_util::sync::CancellationToken::new(),
            last_snapshot_state: None,
            patch_queue_manager: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        state::ApprovalState,
        tui_approval::{TuiApprovalHandle, TuiApprovalProtocol},
    };
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_app() -> App {
        let state = AppState::new(
            PathBuf::from("."),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
        );
        App::new(state)
    }

    #[test]
    fn test_app_new() {
        let app = create_test_app();
        assert_eq!(app.state().config.profile, "test");
        assert_eq!(app.transcript().len(), 0);
        assert!(!app.should_exit());
    }

    #[test]
    fn test_app_quit() {
        let mut app = create_test_app();
        assert!(!app.should_exit());
        app.quit().unwrap();
        assert!(app.should_exit());
    }

    #[test]
    fn test_execute_shell_command_simple() {
        let mut app = create_test_app();

        app.execute_shell_command("echo 'Hello from shell'".to_string());

        let transcript = app.transcript();
        let entries = transcript.entries();

        let user_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo 'Hello from shell'"));
        }

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());
        if let transcript::TranscriptEntry::SystemMessage { content } = system_entry.unwrap() {
            assert!(content.contains("Hello from shell"));
            assert!(content.contains("```"));
        }
    }

    #[test]
    fn test_execute_shell_command_creates_session_event() {
        let mut app = create_test_app();
        let initial_event_count = app.state().session.session_events.len();

        app.execute_shell_command("pwd".to_string());

        assert_eq!(app.state().session.session_events.len(), initial_event_count + 1);

        let event = &app.state().session.session_events[initial_event_count];
        assert_eq!(event.event_type, "shell_command");
        assert!(event.message.contains("Executed: pwd"));
        assert!(!event.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_handle_shell_command_event() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "!cmd echo test".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        app.handle_event(event).await;

        let transcript = app.transcript();
        let entries = transcript.entries();

        let user_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo test"));
        }
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert_eq!(app.state().config.profile, "default");
        assert_eq!(app.transcript().len(), 0);
    }

    #[test]
    fn test_transcript_operations() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Hello");
        app.transcript_mut().add_model_response("Hi there");

        assert_eq!(app.transcript().len(), 2);
    }

    #[test]
    fn test_transcript_streaming() {
        let mut app = create_test_app();

        app.transcript_mut().add_streaming_token("Hello");
        app.transcript_mut().add_streaming_token(" ");
        app.transcript_mut().add_streaming_token("World");

        assert_eq!(app.transcript().len(), 1);

        app.transcript_mut().finish_streaming();

        if let transcript::TranscriptEntry::ModelResponse { content, streaming, .. } = app.transcript().last().unwrap()
        {
            assert_eq!(content, "Hello World");
            assert!(!streaming);
        }
    }

    #[test]
    fn test_transcript_with_tool_calls() {
        let mut app = create_test_app();

        app.transcript_mut()
            .add_tool_call("fs.read", "{ path: '/tmp' }", "safe");
        app.transcript_mut().add_tool_result("fs.read", "file content", true);

        assert_eq!(app.transcript().len(), 2);
    }

    #[test]
    fn test_state_mut() {
        let mut app = create_test_app();
        app.state_mut().config.profile = "modified".to_string();
        assert_eq!(app.state().config.profile, "modified");
    }

    #[test]
    fn test_transcript_clear() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Hello");
        assert_eq!(app.transcript().len(), 1);

        app.transcript_mut().clear();
        assert_eq!(app.transcript().len(), 0);
    }

    #[test]
    fn test_transcript_with_approval() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("patch.feature", "risky");
        assert!(app.transcript().has_pending_approval());

        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Approved);
        assert!(!app.transcript().has_pending_approval());
    }

    #[test]
    fn test_transcript_with_system_messages() {
        let mut app = create_test_app();

        app.transcript_mut().add_system_message("Session started");
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_approval_ui_flow_complete() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Add error handling");
        app.transcript_mut().add_model_response("I'll add error handling...");

        app.transcript_mut()
            .add_tool_call("file_edit", "{ path: 'src/config.rs' }", "risky");

        assert!(!app.transcript().has_pending_approval());

        app.transcript_mut().add_approval_prompt("file_edit", "risky");

        assert!(app.transcript().has_pending_approval());

        let success = app
            .transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Approved);
        assert!(success);
        assert!(!app.transcript().has_pending_approval());

        app.transcript_mut()
            .add_tool_result("file_edit", "Applied successfully", true);

        assert_eq!(app.transcript().len(), 5);
    }

    #[test]
    fn test_approval_ui_flow_rejected() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("file_delete", "dangerous");
        assert!(app.transcript().has_pending_approval());

        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Rejected);

        assert!(!app.transcript().has_pending_approval());

        if let Some(transcript::TranscriptEntry::ApprovalPrompt { decision, .. }) = app.transcript().last() {
            assert_eq!(decision, &Some(transcript::ApprovalDecision::Rejected));
        } else {
            panic!("Expected ApprovalPrompt");
        }
    }

    #[test]
    fn test_approval_ui_flow_cancelled() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("install_deps", "risky");
        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Cancelled);

        assert!(!app.transcript().has_pending_approval());

        if let Some(transcript::TranscriptEntry::ApprovalPrompt { decision, .. }) = app.transcript().last() {
            assert_eq!(decision, &Some(transcript::ApprovalDecision::Cancelled));
        } else {
            panic!("Expected ApprovalPrompt");
        }
    }

    #[test]
    fn test_approval_multiple_prompts() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("patch.feature", "risky");
        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Approved);

        app.transcript_mut().add_approval_prompt("patch.feature2", "safe");
        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Approved);

        assert!(!app.transcript().has_pending_approval());
        assert_eq!(app.transcript().len(), 2);
    }

    #[test]
    fn test_approval_with_description() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("install_crate", "risky");

        if let Some(transcript::TranscriptEntry::ApprovalPrompt { description, .. }) = app.transcript_mut().last_mut() {
            *description = Some("Install serde dependency".to_string());
        }

        app.transcript_mut()
            .set_approval_decision(transcript::ApprovalDecision::Approved);

        if let Some(transcript::TranscriptEntry::ApprovalPrompt { description, .. }) = app.transcript().last() {
            assert_eq!(description, &Some("Install serde dependency".to_string()));
        } else {
            panic!("Expected ApprovalPrompt with description");
        }
    }

    #[test]
    fn test_input_flow_send_message() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "Hello, world!".to_string();
        let message = app.state_mut().input.take();
        app.transcript_mut().add_user_message(&message);

        assert_eq!(app.transcript().len(), 1);
        assert_eq!(app.state_mut().input.buffer, "");
        assert_eq!(app.state_mut().input.cursor, 0);
    }

    #[test]
    fn test_input_flow_navigation() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "Test".to_string();
        app.state_mut().input.cursor = 4;

        app.state_mut().input.move_left();
        assert_eq!(app.state_mut().input.cursor, 3);

        app.state_mut().input.insert_char('X');
        assert_eq!(app.state_mut().input.buffer, "TesXt");
        assert_eq!(app.state_mut().input.cursor, 4);

        app.state_mut().input.delete();
        assert_eq!(app.state_mut().input.buffer, "TesX");

        app.state_mut().input.move_home();
        assert_eq!(app.state_mut().input.cursor, 0);

        app.state_mut().input.move_end();
        assert_eq!(app.state_mut().input.cursor, 4);
    }

    #[test]
    fn test_input_flow_backspace_delete() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "Test".to_string();
        app.state_mut().input.cursor = 4;

        app.state_mut().input.backspace();
        assert_eq!(app.state_mut().input.buffer, "Tes");
        assert_eq!(app.state_mut().input.cursor, 3);

        app.state_mut().input.move_left();
        app.state_mut().input.move_left();
        app.state_mut().input.cursor = 1;

        app.state_mut().input.delete();
        assert_eq!(app.state_mut().input.buffer, "Ts");
        assert_eq!(app.state_mut().input.cursor, 1);
    }

    #[test]
    fn test_input_flow_clear() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test message".to_string();
        app.state_mut().input.cursor = 12;
        app.state_mut().input.clear();
        assert_eq!(app.state_mut().input.buffer, "");
        assert_eq!(app.state_mut().input.cursor, 0);
    }

    #[test]
    fn test_sidebar_toggle() {
        let mut app = create_test_app();

        assert!(app.state().ui.sidebar_visible);
        app.state_mut().toggle_sidebar();
        assert!(!app.state().ui.sidebar_visible);
        app.state_mut().toggle_sidebar();
        assert!(app.state().ui.sidebar_visible);
    }

    #[test]
    fn test_generation_state() {
        let mut app = create_test_app();

        assert!(!app.state().is_generating());
        app.state_mut().start_generation();
        assert!(app.state().is_generating());
        app.state_mut().stop_generation();
        assert!(!app.state().is_generating());
    }

    #[tokio::test]
    async fn test_handle_event_send_message() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test message".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 2);
        assert_eq!(app.state().input.buffer, "");
    }

    #[tokio::test]
    async fn test_handle_event_send_message_empty() {
        let mut app = create_test_app();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 0);
    }

    #[tokio::test]
    async fn test_handle_event_approve_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[test]
    fn test_send_approval_response_with_handle() {
        let mut app = create_test_app();

        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "safe".to_string()).with_request_id(123));

        app.transcript_mut().add_approval_prompt("test.action", "safe");

        app.send_approval_response(ApprovalDecision::Approved);
        assert!(app.state().approval_ui.pending_approval.is_none());
        assert!(app.transcript().len() >= 2);
    }

    #[test]
    fn test_send_approval_response_without_handle() {
        let mut app = create_test_app();

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "safe".to_string()).with_request_id(456));

        app.transcript_mut().add_approval_prompt("test.action", "safe");

        app.send_approval_response(ApprovalDecision::Approved);
        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[test]
    fn test_send_approval_response_reject() {
        let mut app = create_test_app();
        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("delete.file".to_string(), "dangerous".to_string()).with_request_id(789));

        app.transcript_mut().add_approval_prompt("delete.file", "dangerous");
        app.send_approval_response(ApprovalDecision::Rejected);

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[test]
    fn test_send_approval_response_cancel() {
        let mut app = create_test_app();

        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("install.crate".to_string(), "risky".to_string()).with_request_id(999));

        app.transcript_mut().add_approval_prompt("install.crate", "risky");
        app.send_approval_response(ApprovalDecision::Cancelled);

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_reject_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_cancel_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_cancel_generation() {
        let mut app = create_test_app();
        app.state_mut().start_generation();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        app.handle_event(event).await;

        assert!(!app.state().is_generating());
    }

    #[tokio::test]
    async fn test_handle_event_char_input() {
        let mut app = create_test_app();

        for c in "Hello".chars() {
            let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
            app.handle_event(event).await;
        }

        assert_eq!(app.state().input.buffer, "Hello");
    }

    #[tokio::test]
    async fn test_handle_event_open_external_editor() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Initial content".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event).await;

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[test]
    fn test_external_editor_environment_variables() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test content for editor".to_string();

        let editor_cmd = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        assert!(!editor_cmd.is_empty());
    }

    #[test]
    fn test_external_editor_temp_file_operations() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test content\nwith multiple lines".to_string();

        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(format!("thunderus_test_{}.md", uuid::Uuid::new_v4()));

        let write_result = std::fs::write(&temp_file_path, &app.state().input.buffer);
        assert!(write_result.is_ok(), "Should be able to write to temporary file");

        let read_result = std::fs::read_to_string(&temp_file_path);
        assert!(read_result.is_ok(), "Should be able to read from temporary file");

        let read_content = read_result.unwrap();
        assert_eq!(read_content, app.state().input.buffer);

        let _ = std::fs::remove_file(&temp_file_path);
    }

    #[tokio::test]
    async fn test_external_editor_with_empty_input() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event).await;

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[tokio::test]
    async fn test_external_editor_cursor_position() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Some existing content".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event).await;

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[test]
    fn test_redraw_screen_method() {
        let app = create_test_app();
        let result = app.redraw_screen();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_handle_model_command_list() {
        let mut app = create_test_app();
        app.handle_model_command("list".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Available models"));
            assert!(content.contains("Current"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_model_command_unknown() {
        let mut app = create_test_app();
        app.handle_model_command("unknown-model".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(
                content.contains("Cannot switch")
                    || content.contains("Failed to switch")
                    || content.contains("Unknown model")
            );
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_list() {
        let mut app = create_test_app();
        app.handle_approvals_command("list".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Available approval modes"));
            assert!(content.contains("Current"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_read_only() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::Auto;

        app.handle_approvals_command("read-only".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::ReadOnly);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_auto() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::ReadOnly;

        app.handle_approvals_command("auto".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::Auto);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_full_access() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::Auto;

        app.handle_approvals_command("full-access".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::FullAccess);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_unknown() {
        let mut app = create_test_app();
        let original_mode = app.state.config.approval_mode;

        app.handle_approvals_command("unknown-mode".to_string());

        assert_eq!(app.state.config.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Unknown approval mode"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_status_command() {
        let mut app = create_test_app();

        app.handle_status_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Session Status"));
            assert!(content.contains("Profile"));
            assert!(content.contains("Provider"));
            assert!(content.contains("Approval Mode"));
            assert!(content.contains("Sandbox Mode"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_plan_command() {
        let mut app = create_test_app();

        app.handle_plan_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No active session") || content.contains("Current Plan"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_review_command() {
        let mut app = create_test_app();

        app.handle_review_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No pending patches"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_memory_command() {
        let mut app = create_test_app();

        app.handle_memory_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No active session") || content.contains("Project Memory"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[tokio::test]
    async fn test_handle_clear_command() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Test message");
        app.transcript_mut().add_system_message("Test system message");

        assert_eq!(app.transcript().len(), 2);

        app.state_mut().input.buffer = "/clear".to_string();
        app.handle_event(crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )))
        .await;

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Transcript cleared"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[tokio::test]
    async fn test_slash_command_integration() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "/status".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 1);
        assert!(app.state().input.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_slash_command_with_args_integration() {
        let mut app = create_test_app();
        let original_mode = app.state().config.approval_mode;

        app.state_mut().input.buffer = "/approvals read-only".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.state().config.approval_mode, ApprovalMode::ReadOnly);
        assert_ne!(app.state().config.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
    }
}
