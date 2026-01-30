use super::{KeyAction, KeyHandling};
use crate::state::AppState;

use crossterm::event::{KeyCode, KeyEvent};

pub(super) fn handle_edit_keys(event: &KeyEvent, state: &mut AppState) -> KeyHandling {
    match event.code {
        KeyCode::Up => {
            state.input.navigate_up();
            KeyHandling::Handled(None)
        }
        KeyCode::Down => {
            state.input.navigate_down();
            KeyHandling::Handled(None)
        }
        KeyCode::Backspace => {
            state.reset_ctrl_c_count();
            if state.input.is_navigating_history() {
                state.input.reset_history_navigation();
                state.input.clear();
            } else {
                state.input.backspace();
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Delete => {
            state.reset_ctrl_c_count();
            if state.input.is_navigating_history() {
                state.input.reset_history_navigation();
                state.input.clear();
            } else {
                state.input.delete();
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Left => {
            state.reset_ctrl_c_count();
            if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                state.scroll_horizontal(-10);
            } else {
                state.input.move_left();
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Right => {
            state.reset_ctrl_c_count();
            if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                state.scroll_horizontal(10);
            } else {
                state.input.move_right();
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Home => {
            state.reset_ctrl_c_count();
            state.input.move_home();
            KeyHandling::Handled(None)
        }
        KeyCode::End => {
            state.reset_ctrl_c_count();
            state.input.move_end();
            KeyHandling::Handled(None)
        }
        KeyCode::Esc => {
            state.reset_ctrl_c_count();
            if state.is_generating() {
                return KeyHandling::Handled(Some(KeyAction::CancelGeneration));
            } else if state.is_paused() {
                return KeyHandling::Handled(Some(KeyAction::StartReconcileRitual));
            } else if state.ui.is_reconciling() {
                return KeyHandling::Handled(Some(KeyAction::ReconcileDiscard));
            } else if state.input.is_navigating_history() && state.input.buffer.is_empty() {
                state.input.enter_fork_mode();
                return KeyHandling::Handled(Some(KeyAction::NoOp));
            } else if state.input.is_in_fork_mode() {
                state.input.exit_fork_mode();
                return KeyHandling::Handled(Some(KeyAction::NoOp));
            } else if !state.input.buffer.is_empty() {
                state.input.clear();
            } else if !state.input.message_history.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::RewindLastMessage));
            }
            KeyHandling::Handled(None)
        }
        _ => KeyHandling::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
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
    fn test_handle_normal_key_backspace() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 5;

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "Hell");
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_navigation() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);
        assert_eq!(state.input.cursor, 3);

        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_home_end() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);
        assert_eq!(state.input.cursor, 0);

        let event = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_esc_clears_input() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_backspace_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let action = handle_edit_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert!(!state.input.is_navigating_history());
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_delete_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_edit_keys(&event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let action = handle_edit_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert!(!state.input.is_navigating_history());
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_backspace_normal_editing() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 5;

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let action = handle_edit_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "Hell");
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_delete_normal_editing() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 2;

        let event = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let action = handle_edit_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "Helo");
        assert_eq!(state.input.cursor, 2);
    }
}
