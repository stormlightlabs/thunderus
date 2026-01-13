use crate::components::{Footer, Header, Sidebar, Transcript as TranscriptComponent};
use crate::layout::TuiLayout;
use crate::state::AppState;
use crate::transcript::Transcript as TranscriptState;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Result;
use std::panic;

/// Main TUI application
///
/// Handles rendering and state management for the Thunderus TUI
pub struct App {
    state: AppState,
    transcript: TranscriptState,
}

impl App {
    /// Create a new application
    pub fn new(state: AppState) -> Self {
        Self { state, transcript: TranscriptState::new() }
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
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;

        let original_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let backend = CrosstermBackend::new(std::io::stdout());
            if let Ok(mut terminal) = Terminal::new(backend) {
                let _ = terminal.show_cursor();
            }
            original_hook(panic_info);
        }));

        terminal.clear()?;
        self.draw(&mut terminal)?;

        Ok(())
    }

    /// Draw the UI
    pub fn draw(&self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        terminal.draw(|frame| {
            let size = frame.area();
            let layout = TuiLayout::calculate(size, self.state.sidebar_visible);
            let header = Header::new(&self.state);
            header.render(frame, layout.header);

            let transcript_component = TranscriptComponent::new(&self.transcript);
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
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig};

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
        );
        App::new(state)
    }

    #[test]
    fn test_app_new() {
        let app = create_test_app();
        assert_eq!(app.state().profile, "test");
        assert_eq!(app.transcript().len(), 0);
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
}
