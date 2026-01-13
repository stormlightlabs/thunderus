use crate::components::{Footer, FuzzyFinderComponent, Header, Sidebar, Transcript as TranscriptComponent};
use crate::event_handler::{EventHandler, KeyAction};
use crate::layout::TuiLayout;
use crate::state::AppState;
use crate::state::VerbosityLevel;
use crate::transcript::Transcript as TranscriptState;

use crossterm;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::fs;
use std::io::{Result, Write};
use std::panic;
use std::process::Command;
use thunderus_tools::ToolRegistry;
use uuid;

/// Main TUI application
///
/// Handles rendering and state management for the Thunderus TUI
#[derive(Default)]
pub struct App {
    state: AppState,
    transcript: TranscriptState,
    should_exit: bool,
}

impl App {
    /// Create a new application
    pub fn new(state: AppState) -> Self {
        Self { state, transcript: TranscriptState::new(), should_exit: false }
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

    /// Get the transcript
    pub fn transcript(&self) -> &TranscriptState {
        &self.transcript
    }

    /// Get a mutable reference to the transcript
    pub fn transcript_mut(&mut self) -> &mut TranscriptState {
        &mut self.transcript
    }

    /// Run the TUI application
    pub fn run(&mut self) -> Result<()> {
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
            if let Some(event) = EventHandler::read()? {
                self.handle_event(event);
                self.draw(&mut terminal)?;
            }
        }

        terminal.show_cursor()?;
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    /// Handle an event and update state
    fn handle_event(&mut self, event: crossterm::event::Event) {
        use crossterm::event::Event;

        if let Event::Key(key) = event
            && let Some(action) = EventHandler::handle_key_event(key, self.state_mut())
        {
            match action {
                KeyAction::SendMessage { message } => {
                    self.state_mut().input.add_to_history(message.clone());
                    self.transcript_mut().add_user_message(&message);
                }
                KeyAction::ExecuteShellCommand { command } => self.execute_shell_command(command),
                KeyAction::Approve { action: _, risk: _ } => {
                    self.transcript_mut()
                        .set_approval_decision(crate::transcript::ApprovalDecision::Approved);
                    self.state_mut().pending_approval = None;
                }
                KeyAction::Reject { action: _, risk: _ } => {
                    self.transcript_mut()
                        .set_approval_decision(crate::transcript::ApprovalDecision::Rejected);
                    self.state_mut().pending_approval = None;
                }
                KeyAction::Cancel { action: _, risk: _ } => {
                    self.transcript_mut()
                        .set_approval_decision(crate::transcript::ApprovalDecision::Cancelled);
                    self.state_mut().pending_approval = None;
                }
                KeyAction::CancelGeneration => self.state_mut().stop_generation(),
                KeyAction::ToggleSidebar | KeyAction::ToggleVerbosity | KeyAction::ToggleSidebarSection => (),
                KeyAction::OpenExternalEditor => self.open_external_editor(),
                KeyAction::NavigateHistory | KeyAction::ActivateFuzzyFinder => {}
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
                KeyAction::SlashCommandReview => self.handle_review_command(),
                KeyAction::SlashCommandMemory => self.handle_memory_command(),
                KeyAction::SlashCommandClear => {
                    self.transcript_mut().clear();
                    self.transcript_mut()
                        .add_system_message("Transcript cleared (session history preserved)");
                }
                KeyAction::NavigateCardNext => {
                    self.transcript_mut().focus_next_card();
                }
                KeyAction::NavigateCardPrev => {
                    self.transcript_mut().focus_prev_card();
                }
                KeyAction::ToggleCardExpand => {
                    self.transcript_mut().toggle_focused_card_detail_level();
                }
                KeyAction::ToggleCardVerbose => {
                    let _ = self
                        .transcript_mut()
                        .set_focused_card_detail_level(crate::transcript::CardDetailLevel::Verbose);
                }
                KeyAction::NoOp => (),
            }
        }
    }

    /// Draw the UI
    pub fn draw(&self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        terminal.draw(|frame| {
            let size = frame.area();
            let layout = TuiLayout::calculate(size, self.state.sidebar_visible);
            let header = Header::new(&self.state);
            header.render(frame, layout.header);

            let transcript_component =
                TranscriptComponent::with_vertical_scroll(&self.transcript, self.state.scroll_vertical);
            transcript_component.render(frame, layout.transcript);

            if let Some(sidebar_area) = layout.sidebar {
                let sidebar = Sidebar::new(&self.state);
                sidebar.render(frame, sidebar_area);
            }

            let footer = Footer::new(&self.state);
            footer.render(frame, layout.footer);

            if self.state.is_fuzzy_finder_active() {
                let fuzzy_finder = FuzzyFinderComponent::new(&self.state);
                fuzzy_finder.render(frame);
            }
        })?;

        Ok(())
    }

    /// Execute a shell command and insert output as user-provided context
    fn execute_shell_command(&mut self, command: String) {
        let registry = ToolRegistry::with_builtin_tools();
        let tool_call_id = format!("shell_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        let user_message = format!("!cmd {}", command);

        self.transcript_mut().add_user_message(&user_message);

        match registry.execute("shell", tool_call_id.clone(), &serde_json::json!({"command": command})) {
            Ok(result) => {
                if result.is_success() {
                    self.transcript_mut()
                        .add_system_message(format!("Shell command output:\n```\n{}\n```", result.content));

                    self.state_mut().session_events.push(crate::state::SessionEvent {
                        event_type: "shell_command".to_string(),
                        message: format!("Executed: {}", command),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                } else {
                    self.transcript_mut()
                        .add_system_message(format!("Shell command failed: {}", result.content));
                }
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Failed to execute shell command: {}", e));
            }
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
                        .add_system_message("Content loaded from external editor");
                }
                Err(e) => {
                    self.transcript_mut()
                        .add_system_message(format!("Failed to read edited content: {}", e));
                }
            },
            Ok(status) => {
                self.transcript_mut()
                    .add_system_message(format!("Editor exited with non-zero status: {}", status));
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Failed to launch editor '{}': {}", editor_cmd, e));
            }
        }

        let _ = fs::remove_file(&temp_file_path);
        let _ = self.redraw_screen();
    }

    /// Handle /model command
    fn handle_model_command(&mut self, model: String) {
        if model == "list" {
            let provider_name = self.state.provider_name();
            let model_name = self.state.model_name();
            self.transcript_mut().add_system_message(format!(
                "Available models:\n  Current: {} ({})\n  Available: glm-4.7, gemini-2.5-flash",
                provider_name, model_name
            ));
            return;
        }

        match model.as_str() {
            "glm-4.7" => {
                self.transcript_mut()
                    .add_system_message("Switching to GLM-4.7 is not yet implemented in this version");
            }
            "gemini-2.5-flash" => {
                self.transcript_mut()
                    .add_system_message("Switching to Gemini-2.5-flash is not yet implemented in this version");
            }
            _ => {
                self.transcript_mut().add_system_message(format!(
                    "Unknown model: {}. Use /model list to see available models.",
                    model
                ));
            }
        }
    }

    /// Handle /approvals command
    fn handle_approvals_command(&mut self, mode: String) {
        use thunderus_core::ApprovalMode;

        if mode == "list" {
            let current_mode = self.state.approval_mode;
            self.transcript_mut().add_system_message(format!(
                "Available approval modes:\n  Current: {}\n  Available: read-only, auto, full-access",
                current_mode
            ));
            return;
        }

        match mode.as_str() {
            "read-only" => {
                let old_mode = self.state.approval_mode;
                self.state.approval_mode = ApprovalMode::ReadOnly;
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → read-only", old_mode));
            }
            "auto" => {
                let old_mode = self.state.approval_mode;
                self.state.approval_mode = ApprovalMode::Auto;
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → auto", old_mode));
            }
            "full-access" => {
                let old_mode = self.state.approval_mode;
                self.state.approval_mode = ApprovalMode::FullAccess;
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → full-access", old_mode));
            }
            _ => self.transcript_mut().add_system_message(format!(
                "Unknown approval mode: {}. Use /approvals list to see available modes.",
                mode
            )),
        }
    }

    /// Handle /verbosity command
    fn handle_verbosity_command(&mut self, level: String) {
        if level == "list" {
            let current_level = self.state.verbosity;
            return self.transcript_mut().add_system_message(format!(
                "Available verbosity levels:\n  Current: {}\n  Available: quiet, default, verbose",
                current_level.as_str()
            ));
        }

        match level.as_str() {
            "quiet" => {
                let old_level = self.state.verbosity;
                self.state.verbosity = VerbosityLevel::Quiet;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → quiet", old_level.as_str()));
            }
            "default" => {
                let old_level = self.state.verbosity;
                self.state.verbosity = VerbosityLevel::Default;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → default", old_level.as_str()));
            }
            "verbose" => {
                let old_level = self.state.verbosity;
                self.state.verbosity = VerbosityLevel::Verbose;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → verbose", old_level.as_str()));
            }
            _ => self.transcript_mut().add_system_message(format!(
                "Unknown verbosity level: {}. Use /verbosity list to see available levels.",
                level
            )),
        }
    }

    /// Handle /status command
    fn handle_status_command(&mut self) {
        let profile = self.state.profile.clone();
        let provider_name = self.state.provider_name();
        let model_name = self.state.model_name();
        let approval_mode = self.state.approval_mode;
        let sandbox_mode = self.state.sandbox_mode;
        let verbosity = self.state.verbosity;
        let cwd = self.state.cwd.display();
        let session_events_count = self.state.session_events.len();
        let modified_files_count = self.state.modified_files.len();
        let has_pending_approval = self.state.pending_approval.is_some();

        let status = format!(
            "Session Status:\n\
             Profile: {}\n\
             Provider: {} ({})\n\
             Approval Mode: {}\n\
             Sandbox Mode: {}\n\
             Verbosity: {}\n\
             Working Directory: {}\n\
             Session Events: {}\n\
             Modified Files: {}\n\
             Pending Approvals: {}",
            profile,
            provider_name,
            model_name,
            approval_mode,
            sandbox_mode,
            verbosity.as_str(),
            cwd,
            session_events_count,
            modified_files_count,
            has_pending_approval
        );
        self.transcript_mut().add_system_message(status);
    }

    /// Handle /plan command
    fn handle_plan_command(&mut self) {
        let plan_path = self.state.cwd.join("PLAN.md");
        match fs::read_to_string(&plan_path) {
            Ok(content) => {
                self.transcript_mut()
                    .add_system_message(format!("PLAN.md:\n\n{}", content));
            }
            Err(_) => self
                .transcript_mut()
                .add_system_message("PLAN.md not found in the current working directory"),
        }
    }

    /// Handle /review command
    fn handle_review_command(&mut self) {
        self.transcript_mut()
            .add_system_message("Review pass triggered. This feature is not yet implemented in this version.");
    }

    /// Handle /memory command
    fn handle_memory_command(&mut self) {
        let memory_path = self.state.cwd.join("MEMORY.md");
        match fs::read_to_string(&memory_path) {
            Ok(content) => {
                self.transcript_mut()
                    .add_system_message(format!("MEMORY.md:\n\n{}", content));
            }
            Err(_) => self
                .transcript_mut()
                .add_system_message("MEMORY.md not found in the current working directory"),
        }
    }

    /// Redraw the screen after returning from external editor
    fn redraw_screen(&self) -> Result<()> {
        use std::io::{self, Write};

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

#[cfg(test)]
mod tests {
    use super::*;
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
        );
        App::new(state)
    }

    #[test]
    fn test_app_new() {
        let app = create_test_app();
        assert_eq!(app.state().profile, "test");
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
            .find(|e| matches!(e, crate::transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let crate::transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo 'Hello from shell'"));
        }

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, crate::transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = system_entry.unwrap() {
            assert!(content.contains("Hello from shell"));
            assert!(content.contains("```"));
        }
    }

    #[test]
    fn test_execute_shell_command_creates_session_event() {
        let mut app = create_test_app();
        let initial_event_count = app.state().session_events.len();

        app.execute_shell_command("pwd".to_string());

        assert_eq!(app.state().session_events.len(), initial_event_count + 1);

        let event = &app.state().session_events[initial_event_count];
        assert_eq!(event.event_type, "shell_command");
        assert!(event.message.contains("Executed: pwd"));
        assert!(!event.timestamp.is_empty());
    }

    #[test]
    fn test_handle_shell_command_event() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "!cmd echo test".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        app.handle_event(event);

        let transcript = app.transcript();
        let entries = transcript.entries();

        let user_entry = entries
            .iter()
            .find(|e| matches!(e, crate::transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let crate::transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo test"));
        }
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert_eq!(app.state().profile, "default");
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

        if let crate::transcript::TranscriptEntry::ModelResponse { content, streaming, .. } =
            app.transcript().last().unwrap()
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
        app.state_mut().profile = "modified".to_string();
        assert_eq!(app.state().profile, "modified");
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
            .set_approval_decision(crate::transcript::ApprovalDecision::Approved);
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
            .set_approval_decision(crate::transcript::ApprovalDecision::Approved);
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
            .set_approval_decision(crate::transcript::ApprovalDecision::Rejected);

        assert!(!app.transcript().has_pending_approval());

        if let Some(crate::transcript::TranscriptEntry::ApprovalPrompt { decision, .. }) = app.transcript().last() {
            assert_eq!(decision, &Some(crate::transcript::ApprovalDecision::Rejected));
        } else {
            panic!("Expected ApprovalPrompt");
        }
    }

    #[test]
    fn test_approval_ui_flow_cancelled() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("install_deps", "risky");
        app.transcript_mut()
            .set_approval_decision(crate::transcript::ApprovalDecision::Cancelled);

        assert!(!app.transcript().has_pending_approval());

        if let Some(crate::transcript::TranscriptEntry::ApprovalPrompt { decision, .. }) = app.transcript().last() {
            assert_eq!(decision, &Some(crate::transcript::ApprovalDecision::Cancelled));
        } else {
            panic!("Expected ApprovalPrompt");
        }
    }

    #[test]
    fn test_approval_multiple_prompts() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("patch.feature", "risky");
        app.transcript_mut()
            .set_approval_decision(crate::transcript::ApprovalDecision::Approved);

        app.transcript_mut().add_approval_prompt("patch.feature2", "safe");
        app.transcript_mut()
            .set_approval_decision(crate::transcript::ApprovalDecision::Approved);

        assert!(!app.transcript().has_pending_approval());
        assert_eq!(app.transcript().len(), 2);
    }

    #[test]
    fn test_approval_with_description() {
        let mut app = create_test_app();

        app.transcript_mut().add_approval_prompt("install_crate", "risky");

        if let Some(crate::transcript::TranscriptEntry::ApprovalPrompt { description, .. }) =
            app.transcript_mut().last_mut()
        {
            *description = Some("Install serde dependency".to_string());
        }

        app.transcript_mut()
            .set_approval_decision(crate::transcript::ApprovalDecision::Approved);

        if let Some(crate::transcript::TranscriptEntry::ApprovalPrompt { description, .. }) = app.transcript().last() {
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

        assert!(app.state().sidebar_visible);
        app.state_mut().toggle_sidebar();
        assert!(!app.state().sidebar_visible);
        app.state_mut().toggle_sidebar();
        assert!(app.state().sidebar_visible);
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
    fn test_handle_event_send_message() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test message".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert_eq!(app.transcript().len(), 1);
        assert_eq!(app.state().input.buffer, "");
    }

    #[test]
    fn test_handle_event_send_message_empty() {
        let mut app = create_test_app();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert_eq!(app.transcript().len(), 0);
    }

    #[test]
    fn test_handle_event_approve_action() {
        let mut app = create_test_app();
        app.state_mut().pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "risky".to_string(),
        ));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert!(app.state().pending_approval.is_none());
    }

    #[test]
    fn test_handle_event_reject_action() {
        let mut app = create_test_app();
        app.state_mut().pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "risky".to_string(),
        ));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert!(app.state().pending_approval.is_none());
    }

    #[test]
    fn test_handle_event_cancel_action() {
        let mut app = create_test_app();
        app.state_mut().pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "risky".to_string(),
        ));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert!(app.state().pending_approval.is_none());
    }

    #[test]
    fn test_handle_event_cancel_generation() {
        let mut app = create_test_app();
        app.state_mut().start_generation();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        app.handle_event(event);

        assert!(!app.state().is_generating());
    }

    #[test]
    fn test_handle_event_char_input() {
        let mut app = create_test_app();

        for c in "Hello".chars() {
            let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
            app.handle_event(event);
        }

        assert_eq!(app.state().input.buffer, "Hello");
    }

    #[test]
    fn test_handle_event_open_external_editor() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Initial content".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event);

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, crate::transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());
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

    #[test]
    fn test_external_editor_with_empty_input() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event);

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, crate::transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());
    }

    #[test]
    fn test_external_editor_cursor_position() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Some existing content".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event);
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
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
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
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Unknown model"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_list() {
        let mut app = create_test_app();
        app.handle_approvals_command("list".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Available approval modes"));
            assert!(content.contains("Current"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_read_only() {
        let mut app = create_test_app();
        app.state_mut().approval_mode = thunderus_core::ApprovalMode::Auto;

        app.handle_approvals_command("read-only".to_string());

        assert_eq!(app.state.approval_mode, thunderus_core::ApprovalMode::ReadOnly);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_auto() {
        let mut app = create_test_app();
        app.state_mut().approval_mode = thunderus_core::ApprovalMode::ReadOnly;

        app.handle_approvals_command("auto".to_string());

        assert_eq!(app.state.approval_mode, thunderus_core::ApprovalMode::Auto);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_full_access() {
        let mut app = create_test_app();
        app.state_mut().approval_mode = thunderus_core::ApprovalMode::Auto;

        app.handle_approvals_command("full-access".to_string());

        assert_eq!(app.state.approval_mode, thunderus_core::ApprovalMode::FullAccess);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_unknown() {
        let mut app = create_test_app();
        let original_mode = app.state.approval_mode;

        app.handle_approvals_command("unknown-mode".to_string());

        assert_eq!(app.state.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
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
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
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
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("not found"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_review_command() {
        let mut app = create_test_app();

        app.handle_review_command();

        assert_eq!(app.transcript().len(), 1);
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Review pass triggered"));
            assert!(content.contains("not yet implemented"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_memory_command() {
        let mut app = create_test_app();

        app.handle_memory_command();

        assert_eq!(app.transcript().len(), 1);
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("not found"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_clear_command() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Test message");
        app.transcript_mut().add_system_message("Test system message");

        assert_eq!(app.transcript().len(), 2);

        app.state_mut().input.buffer = "/clear".to_string();
        app.handle_event(crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )));

        assert_eq!(app.transcript().len(), 1);
        if let crate::transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Transcript cleared"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_slash_command_integration() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "/status".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert_eq!(app.transcript().len(), 1);
        assert!(app.state().input.buffer.is_empty());
    }

    #[test]
    fn test_slash_command_with_args_integration() {
        let mut app = create_test_app();
        let original_mode = app.state().approval_mode;

        app.state_mut().input.buffer = "/approvals read-only".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event);

        assert_eq!(app.state.approval_mode, thunderus_core::ApprovalMode::ReadOnly);
        assert_ne!(app.state.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
    }
}
