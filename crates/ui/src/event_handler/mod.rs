mod approval;
mod fuzzy_finder;
mod key_action;
mod normal_mode;
mod slash_parser;

pub use key_action::KeyAction;

use crate::state::AppState;

use crossterm::event::{Event, KeyEvent, KeyEventKind};

use self::{approval::handle_approval_key, fuzzy_finder::handle_fuzzy_finder_key, normal_mode::handle_normal_key};

/// Event handler for the TUI application
pub struct EventHandler;

impl EventHandler {
    /// Read a single event from the terminal
    ///
    /// Returns `Some(event)` if an event is available, `None` on timeout or error.
    /// Terminal errors are logged to stderr but not propagated, since they are
    /// typically fatal and the application will exit on the next iteration.
    pub fn read() -> Option<Event> {
        match crossterm::event::poll(std::time::Duration::from_millis(100)) {
            Ok(true) => match crossterm::event::read() {
                Ok(event) => Some(event),
                Err(e) => {
                    eprintln!("Terminal error: {}", e);
                    None
                }
            },
            Ok(false) => None,
            Err(e) => {
                eprintln!("Event poll error: {}", e);
                None
            }
        }
    }

    /// Handle a keyboard event and return whether it should exit
    pub fn handle_key_event(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        if event.kind != KeyEventKind::Press {
            return None;
        }

        if state.approval_ui.pending_hint.is_some() {
            state.approval_ui.pending_hint = None;
        }

        if state.is_fuzzy_finder_active() {
            return handle_fuzzy_finder_key(event, state);
        }

        match state.approval_ui.pending_approval {
            Some(_) => handle_approval_key(event, state),
            None => handle_normal_key(event, state),
        }
    }

    /// Handle any event and refresh git branch on focus gained
    pub fn handle_event(event: &Event, state: &mut AppState) -> Option<KeyAction> {
        match event {
            Event::FocusGained => {
                state.refresh_git_branch();
                None
            }
            Event::Key(key_event) => Self::handle_key_event(*key_event, state),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_state() -> AppState {
        let mut state = AppState::new(
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
        state.set_first_session(false);
        state
    }

    #[test]
    fn test_welcome_screen_keystroke_passthrough() {
        let mut state = AppState::new(
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
        assert!(state.is_first_session());
        assert_eq!(state.input.buffer, "");

        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert!(!state.is_first_session());
        assert_eq!(state.input.buffer, "x");
    }

    #[test]
    fn test_handle_event_focus_gained_refreshes_git_branch() {
        let mut state = create_test_state();

        state.config.git_branch = Some("initial-branch".to_string());

        let temp = tempfile::TempDir::new().unwrap();
        let working_dir = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["checkout", "-b", "focus-test-branch"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        state.config.cwd = working_dir.to_path_buf();

        let event = crossterm::event::Event::FocusGained;
        let action = EventHandler::handle_event(&event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.config.git_branch, Some("focus-test-branch".to_string()));
    }

    #[test]
    fn test_handle_event_key_event_delegates_to_handle_key_event() {
        let mut state = create_test_state();

        let key_event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let event = crossterm::event::Event::Key(key_event);
        let action = EventHandler::handle_event(&event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "x");
    }
}
