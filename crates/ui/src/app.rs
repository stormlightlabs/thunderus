use crate::components::{Footer, Header, Sidebar, Transcript as TranscriptComponent};
use crate::event_handler::{EventHandler, KeyAction};
use crate::layout::TuiLayout;
use crate::state::AppState;
use crate::transcript::Transcript as TranscriptState;
use crossterm;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Result;
use std::panic;

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
                    self.transcript_mut().add_user_message(&message);
                }
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
                KeyAction::CancelGeneration => {
                    self.state_mut().stop_generation();
                }
                KeyAction::ToggleSidebar => {}
                KeyAction::NoOp => {}
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
        })?;

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
}
