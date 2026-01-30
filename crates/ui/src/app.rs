use crate::snapshot_capture::{SnapshotCapture, SnapshotMode};
use crate::state::AppState;
use crate::transcript::Transcript as TranscriptState;
use crate::tui_approval::TuiApprovalHandle;

use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Result;
use std::sync::Arc;
use thunderus_core::{
    ApprovalGate, ApprovalMode, ApprovalRequest, Config, DriftEvent, DriftMonitor, PatchQueueManager, Profile, Session,
    SnapshotManager, memory::MemoryRetriever,
};
use thunderus_providers::{CancelToken, Provider};
use tokio::sync::mpsc;

mod event_handling;
mod event_loop;
mod external_editor;
mod rendering;
mod session_replay;
mod shell;

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
    approval_gate_handle: Option<Arc<std::sync::RwLock<ApprovalGate>>>,
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
    /// Snapshot capture for regression testing
    pub(crate) snapshot_capture: Option<SnapshotCapture>,
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
        let snapshot_capture = if state.is_test_mode() {
            let snapshot_dir = state.cwd().join(".thunderus").join("snapshots");
            Some(SnapshotCapture::new(true, snapshot_dir).with_mode(SnapshotMode::EveryState))
        } else {
            None
        };

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
            snapshot_capture,
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
        let snapshot_capture = if state.is_test_mode() {
            let snapshot_dir = state.cwd().join(".thunderus").join("snapshots");
            Some(SnapshotCapture::new(true, snapshot_dir).with_mode(SnapshotMode::EveryState))
        } else {
            None
        };

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
            snapshot_capture,
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
    pub fn set_approval_gate_handle(&mut self, gate: Arc<std::sync::RwLock<ApprovalGate>>) {
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
        session_replay::reconstruct_transcript_from_session(self)
    }

    /// Run the TUI application with unified event loop
    ///
    /// Uses [tokio::select!] to multiplex TUI keyboard events and Agent streaming events.
    /// This allows the agent to stream responses in real-time while remaining responsive
    /// to user input (cancellation, approval requests, etc.).
    pub async fn run(&mut self) -> Result<()> {
        event_loop::run(self).await
    }

    /// Handle an event and update state
    async fn handle_event(&mut self, event: crossterm::event::Event) {
        event_handling::handle_event(self, event).await;
    }

    /// Draw the UI
    pub fn draw(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        rendering::draw(self, terminal)
    }

    /// Execute a shell command and insert output as user-provided context
    ///
    /// Shell commands require approval based on the current approval mode:
    /// - ReadOnly: Always rejected
    /// - Auto: Safe commands auto-approve, risky commands require approval
    /// - FullAccess: Auto-approved
    fn execute_shell_command(&mut self, command: String) {
        shell::execute_shell_command(self, command);
    }

    /// Open external editor for current input buffer
    fn open_external_editor(&mut self) {
        external_editor::open_external_editor(self);
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
            snapshot_capture: None,
        }
    }
}

#[cfg(test)]
pub(crate) fn create_test_app() -> App {
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    let state = crate::state::AppState::new(
        PathBuf::from("."),
        "test".to_string(),
        ProviderConfig::Glm {
            api_key: "test".to_string(),
            model: "glm-4.7".to_string(),
            base_url: "https://api.example.com".to_string(),
            thinking: Default::default(),
            options: Default::default(),
        },
        ApprovalMode::Auto,
        SandboxMode::Policy,
        false,
    );
    App::new(state)
}

#[cfg(test)]
mod tests {
    use super::App;
    use super::create_test_app;
    use crate::transcript;

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
    fn test_app_default() {
        let app = App::default();
        assert_eq!(app.state().config.profile, "default");
        assert_eq!(app.transcript().len(), 0);
    }

    #[test]
    fn test_state_mut() {
        let mut app = create_test_app();
        app.state_mut().config.profile = "modified".to_string();
        assert_eq!(app.state().config.profile, "modified");
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
}
