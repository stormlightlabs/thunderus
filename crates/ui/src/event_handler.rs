use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::Result;

use crate::state::AppState;

/// Event handler for the TUI application
pub struct EventHandler;

impl EventHandler {
    /// Read a single event from the terminal
    pub fn read() -> Result<Option<Event>> {
        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            Ok(Some(crossterm::event::read()?))
        } else {
            Ok(None)
        }
    }

    /// Handle a keyboard event and return whether it should exit
    pub fn handle_key_event(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        if event.kind != KeyEventKind::Press {
            return None;
        }

        match state.pending_approval {
            Some(_) => Self::handle_approval_key(event, state),
            None => Self::handle_normal_key(event, state),
        }
    }

    /// Handle keys when there's a pending approval
    fn handle_approval_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        match event.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(ref approval) = state.pending_approval {
                    return Some(KeyAction::Approve { action: approval.action.clone(), risk: approval.risk.clone() });
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(ref approval) = state.pending_approval {
                    return Some(KeyAction::Reject { action: approval.action.clone(), risk: approval.risk.clone() });
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if let Some(ref approval) = state.pending_approval {
                    return Some(KeyAction::Cancel { action: approval.action.clone(), risk: approval.risk.clone() });
                }
            }
            KeyCode::Esc => {
                return Some(KeyAction::CancelGeneration);
            }
            _ => {}
        }
        None
    }

    /// Handle keys in normal mode (no pending approval)
    fn handle_normal_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        match event.code {
            KeyCode::Enter => {
                if !state.input.buffer.is_empty() {
                    let message = state.input.take();

                    if let Some(command) = message.strip_prefix("!cmd ") {
                        return Some(KeyAction::ExecuteShellCommand { command: command.to_string() });
                    }

                    return Some(KeyAction::SendMessage { message });
                }
            }
            KeyCode::Char('j') | KeyCode::Char('J') => {
                if state.input.buffer.is_empty() {
                    state.scroll_vertical(1);
                } else {
                    state.input.insert_char('j');
                }
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                if state.input.buffer.is_empty() {
                    state.scroll_vertical(-1);
                } else {
                    state.input.insert_char('k');
                }
            }
            KeyCode::Char(c) => {
                if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    return Some(KeyAction::CancelGeneration);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 's' {
                    state.toggle_sidebar();
                } else {
                    state.input.insert_char(c);
                }
            }
            KeyCode::Backspace => {
                state.input.backspace();
            }
            KeyCode::Delete => {
                state.input.delete();
            }
            KeyCode::Left => {
                if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                    state.scroll_horizontal(-10);
                } else {
                    state.input.move_left();
                }
            }
            KeyCode::Right => {
                if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                    state.scroll_horizontal(10);
                } else {
                    state.input.move_right();
                }
            }
            KeyCode::Home => {
                state.input.move_home();
            }
            KeyCode::End => {
                state.input.move_end();
            }
            KeyCode::Esc => {
                if state.is_generating() {
                    return Some(KeyAction::CancelGeneration);
                } else if !state.input.buffer.is_empty() {
                    state.input.clear();
                }
            }
            _ => {}
        }
        None
    }
}

/// Actions that can be triggered by key events
#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    /// User wants to send a message
    SendMessage { message: String },
    /// User wants to execute a shell command
    ExecuteShellCommand { command: String },
    /// User approves an action
    Approve { action: String, risk: String },
    /// User rejects an action
    Reject { action: String, risk: String },
    /// User cancels an action
    Cancel { action: String, risk: String },
    /// User wants to cancel generation
    CancelGeneration,
    /// Toggle sidebar
    ToggleSidebar,
    /// No action (e.g., navigation in input)
    NoOp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_state() -> AppState {
        AppState::new(
            PathBuf::from("."),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        )
    }

    #[test]
    fn test_handle_normal_key_char_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "H");
    }

    #[test]
    fn test_handle_normal_key_multiple_chars() {
        let mut state = create_test_state();

        for c in "Hello".chars() {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            EventHandler::handle_key_event(event, &mut state);
        }

        assert_eq!(state.input.buffer, "Hello");
    }

    #[test]
    fn test_handle_normal_key_backspace() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 5;

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "Hell");
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_enter() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_handle_normal_key_enter_empty() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_navigation() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);
        assert_eq!(state.input.cursor, 3);

        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_home_end() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);
        assert_eq!(state.input.cursor, 0);

        let event = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_toggle_sidebar() {
        let mut state = create_test_state();
        assert!(state.sidebar_visible);

        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        EventHandler::handle_key_event(event, &mut state);

        assert!(!state.sidebar_visible);
    }

    #[test]
    fn test_handle_normal_key_esc_clears_input() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_ctrl_c_cancel() {
        let mut state = create_test_state();
        state.start_generation();

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::CancelGeneration)));
    }

    #[test]
    fn test_handle_approval_key_approve() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "patch.feature".to_string(),
            "risky".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        if let Some(KeyAction::Approve { action, risk }) = action {
            assert_eq!(action, "patch.feature");
            assert_eq!(risk, "risky");
        } else {
            panic!("Expected Approve action");
        }
    }

    #[test]
    fn test_handle_approval_key_reject() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "delete.file".to_string(),
            "dangerous".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        if let Some(KeyAction::Reject { action, risk }) = action {
            assert_eq!(action, "delete.file");
            assert_eq!(risk, "dangerous");
        } else {
            panic!("Expected Reject action");
        }
    }

    #[test]
    fn test_handle_approval_key_cancel() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "install.deps".to_string(),
            "risky".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        if let Some(KeyAction::Cancel { action, risk }) = action {
            assert_eq!(action, "install.deps");
            assert_eq!(risk, "risky");
        } else {
            panic!("Expected Cancel action");
        }
    }

    #[test]
    fn test_handle_approval_key_uppercase() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "safe".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::Approve { .. })));
    }

    #[test]
    fn test_handle_approval_key_esc_generates_cancel() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "safe".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::CancelGeneration)));
    }

    #[test]
    fn test_handle_approval_key_no_match() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "safe".to_string(),
        ));

        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_no_pending_approval() {
        let mut state = create_test_state();

        state.input.buffer = "test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "testx");
    }

    #[test]
    fn test_handle_scroll_vertical_down() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.scroll_vertical, 1);
    }

    #[test]
    fn test_handle_scroll_vertical_up() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.scroll_vertical, 0);
    }

    #[test]
    fn test_handle_scroll_horizontal_right() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.scroll_horizontal, 10);
    }

    #[test]
    fn test_handle_scroll_horizontal_left() {
        let mut state = create_test_state();
        state.scroll_horizontal = 20;

        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.scroll_horizontal, 10);
    }

    #[test]
    fn test_handle_scroll_horizontal_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.cursor, 4);
        assert_eq!(state.scroll_horizontal, 0);
    }

    #[test]
    fn test_handle_scroll_vertical_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "j".to_string();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "jj");
        assert_eq!(state.scroll_vertical, 0);
    }

    #[test]
    fn test_handle_normal_key_enter_shell_command() {
        let mut state = create_test_state();
        state.input.buffer = "!cmd ls -la".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::ExecuteShellCommand { .. })));

        if let Some(KeyAction::ExecuteShellCommand { command }) = action {
            assert_eq!(command, "ls -la");
        }
    }

    #[test]
    fn test_handle_normal_key_enter_shell_command_with_spaces() {
        let mut state = create_test_state();
        state.input.buffer = "!cmd    echo   'hello world'".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ExecuteShellCommand { .. })));

        if let Some(KeyAction::ExecuteShellCommand { command }) = action {
            assert_eq!(command, "   echo   'hello world'");
        }
    }

    #[test]
    fn test_handle_normal_key_enter_regular_message() {
        let mut state = create_test_state();
        state.input.buffer = "This is a regular message".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));

        if let Some(KeyAction::SendMessage { message }) = action {
            assert_eq!(message, "This is a regular message");
        }
    }

    #[test]
    fn test_handle_normal_key_enter_empty_cmd_prefix() {
        let mut state = create_test_state();
        state.input.buffer = "!cmd".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));

        if let Some(KeyAction::SendMessage { message }) = action {
            assert_eq!(message, "!cmd");
        }
    }

    #[test]
    fn test_handle_normal_key_enter_cmd_with_only_spaces() {
        let mut state = create_test_state();
        state.input.buffer = "!cmd   ".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ExecuteShellCommand { .. })));

        if let Some(KeyAction::ExecuteShellCommand { command }) = action {
            assert_eq!(command, "  ");
        }
    }
}
