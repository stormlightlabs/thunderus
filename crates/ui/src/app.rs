use crate::components::{Footer, Header, Sidebar, Transcript};
use crate::layout::TuiLayout;
use crate::state::AppState;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Result;
use std::panic;

/// Main TUI application
///
/// Handles rendering and state management for the Thunderus TUI
pub struct App {
    state: AppState,
    transcript_content: String,
}

impl App {
    /// Create a new application
    pub fn new(state: AppState) -> Self {
        Self { state, transcript_content: String::new() }
    }

    /// Get a mutable reference to the application state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get a reference to the application state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get the current transcript content
    pub fn transcript_content(&self) -> &str {
        &self.transcript_content
    }

    /// Set the transcript content
    pub fn set_transcript_content(&mut self, content: String) {
        self.transcript_content = content;
    }

    /// Append to the transcript content
    pub fn append_transcript(&mut self, content: &str) {
        if !self.transcript_content.is_empty() {
            self.transcript_content.push('\n');
        }
        self.transcript_content.push_str(content);
    }

    /// Clear the transcript content
    pub fn clear_transcript(&mut self) {
        self.transcript_content.clear();
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

            let mut transcript = Transcript::new(&self.transcript_content);
            if self.state.is_generating() {
                transcript = transcript.streaming();
            }
            transcript.render(frame, layout.transcript);

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
        assert!(app.transcript_content().is_empty());
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert_eq!(app.state().profile, "default");
    }

    #[test]
    fn test_set_transcript_content() {
        let mut app = create_test_app();
        app.set_transcript_content("Hello, world!".to_string());
        assert_eq!(app.transcript_content(), "Hello, world!");
    }

    #[test]
    fn test_append_transcript() {
        let mut app = create_test_app();
        app.append_transcript("First");
        app.append_transcript("Second");
        app.append_transcript("Third");

        assert_eq!(app.transcript_content(), "First\nSecond\nThird");
    }

    #[test]
    fn test_clear_transcript() {
        let mut app = create_test_app();
        app.set_transcript_content("Content".to_string());
        assert_eq!(app.transcript_content(), "Content");

        app.clear_transcript();
        assert!(app.transcript_content().is_empty());
    }

    #[test]
    fn test_state_mut() {
        let mut app = create_test_app();
        app.state_mut().profile = "modified".to_string();
        assert_eq!(app.state().profile, "modified");
    }

    #[test]
    fn test_toggle_sidebar() {
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
    fn test_transcript_with_content() {
        let mut app = create_test_app();
        let content = "User: Hello\nAgent: Hi there!";
        app.set_transcript_content(content.to_string());

        assert_eq!(app.transcript_content(), content);
        assert!(!app.transcript_content().is_empty());
    }
}
