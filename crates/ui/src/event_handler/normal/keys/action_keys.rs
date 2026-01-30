use super::{KeyAction, KeyHandling};
use crate::{slash::parse_slash_command, state::AppState};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn handle_action_keys(event: &KeyEvent, state: &mut AppState) -> KeyHandling {
    match event.code {
        KeyCode::Char(' ') => {
            if state.input.buffer.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::ToggleCardExpand));
            } else {
                state.input.insert_char(' ');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL)
                && state.input.buffer.is_empty()
                && state.selected_patch_index().is_some()
            {
                return KeyHandling::Handled(Some(KeyAction::ApproveHunk));
            } else {
                state.input.insert_char('a');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL) {
                if state.input.buffer.is_empty() && event.modifiers.contains(KeyModifiers::SHIFT) {
                    return KeyHandling::Handled(Some(KeyAction::NavigateNextPatch));
                } else if state.input.buffer.is_empty() {
                    return KeyHandling::Handled(Some(KeyAction::NavigateNextHunk));
                } else {
                    state.input.insert_char('n');
                }
            } else {
                state.input.insert_char('n');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL) {
                if state.input.buffer.is_empty() && event.modifiers.contains(KeyModifiers::SHIFT) {
                    return KeyHandling::Handled(Some(KeyAction::NavigatePrevPatch));
                } else if state.input.buffer.is_empty() {
                    return KeyHandling::Handled(Some(KeyAction::NavigatePrevHunk));
                } else {
                    state.input.insert_char('p');
                }
            } else {
                state.input.insert_char('p');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if state.input.buffer.is_empty()
                && event.modifiers.contains(KeyModifiers::CONTROL)
                && event.modifiers.contains(KeyModifiers::SHIFT)
            {
                state.config.verbosity.toggle();
                return KeyHandling::Handled(Some(KeyAction::ToggleVerbosity));
            } else if state.input.buffer.is_empty()
                && !event.modifiers.contains(KeyModifiers::CONTROL)
                && !event.modifiers.contains(KeyModifiers::SHIFT)
            {
                return KeyHandling::Handled(Some(KeyAction::ToggleCardVerbose));
            } else {
                state.input.insert_char('v');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Enter => {
            if state.ui.is_reconciling() {
                return KeyHandling::Handled(Some(KeyAction::ReconcileContinue));
            }
            if !state.input.buffer.is_empty() {
                let message = state.input.take();

                if let Some(command) = message.strip_prefix("!cmd ") {
                    return KeyHandling::Handled(Some(KeyAction::ExecuteShellCommand { command: command.to_string() }));
                }

                if let Some(cmd) = message.strip_prefix('/') {
                    return KeyHandling::Handled(parse_slash_command(cmd.to_string()));
                }

                return KeyHandling::Handled(Some(KeyAction::SendMessage { message }));
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            if state.ui.is_reconciling() {
                return KeyHandling::Handled(Some(KeyAction::ReconcileStop));
            } else if state.input.buffer.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::Exit));
            } else {
                state.input.insert_char('q');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('j') | KeyCode::Char('J') => {
            if state.input.buffer.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::NavigateCardNext));
            } else {
                state.input.insert_char('j');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('k') | KeyCode::Char('K') => {
            if state.input.buffer.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::NavigateCardPrev));
            } else {
                state.input.insert_char('k');
            }
            KeyHandling::Handled(None)
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if state.input.buffer.is_empty() {
                return KeyHandling::Handled(Some(KeyAction::ToggleInspector));
            } else {
                state.input.insert_char('i');
            }
            KeyHandling::Handled(None)
        }
        _ => KeyHandling::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_handle_normal_key_enter() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::SendMessage { .. }))
        ));
    }

    #[test]
    fn test_handle_normal_key_enter_empty() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
    }

    #[test]
    fn test_handle_normal_key_j_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigateCardNext))
        ));
    }

    #[test]
    fn test_handle_normal_key_k_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigateCardPrev))
        ));
    }

    #[test]
    fn test_handle_normal_key_space_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::ToggleCardExpand))
        ));
    }

    #[test]
    fn test_handle_normal_key_v_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::ToggleCardVerbose))
        ));
    }

    #[test]
    fn test_handle_normal_key_j_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "jtest");
        assert!(matches!(action, KeyHandling::Handled(None)));
    }

    #[test]
    fn test_handle_normal_key_k_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "ktest");
        assert!(matches!(action, KeyHandling::Handled(None)));
    }

    #[test]
    fn test_handle_normal_key_space_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, " test");
        assert!(matches!(action, KeyHandling::Handled(None)));
    }

    #[test]
    fn test_handle_normal_key_v_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "vtest");
        assert!(matches!(action, KeyHandling::Handled(None)));
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command() {
        let mut state = create_test_state();
        state.input.buffer = "/status".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::SlashCommandStatus))
        ));
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command_with_args() {
        let mut state = create_test_state();
        state.input.buffer = "/model glm-4.7".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::SlashCommandModel { .. }))
        ));
    }

    #[test]
    fn test_handle_normal_key_backslash_not_slash() {
        let mut state = create_test_state();
        state.input.buffer = "This is a \\ not a slash".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::SendMessage { .. }))
        ));
    }

    #[test]
    fn test_handle_normal_key_slash_in_middle() {
        let mut state = create_test_state();
        state.input.buffer = "This has / in the middle".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::SendMessage { .. }))
        ));
    }

    #[test]
    fn test_handle_normal_key_ctrl_shift_v_toggle_verbosity() {
        let mut state = create_test_state();
        assert_eq!(state.config.verbosity.as_str(), "quiet");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::ToggleVerbosity))));
        assert_eq!(state.config.verbosity.as_str(), "default");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = handle_action_keys(&event, &mut state);

        assert_eq!(state.config.verbosity.as_str(), "verbose");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = handle_action_keys(&event, &mut state);

        assert_eq!(state.config.verbosity.as_str(), "quiet");
    }

    #[test]
    fn test_handle_normal_key_a_approves_hunk_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::ApproveHunk))));
    }

    #[test]
    fn test_handle_normal_key_a_inserts_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "a");
    }

    #[test]
    fn test_handle_normal_key_a_inserts_when_typing() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);
        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "atest");
    }

    #[test]
    fn test_handle_normal_key_ctrl_a_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let action = handle_action_keys(&event, &mut state);
        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "a");
    }

    #[test]
    fn test_handle_normal_key_n_navigates_hunk() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigateNextHunk))
        ));
    }

    #[test]
    fn test_handle_normal_key_shift_n_navigates_patch() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::SHIFT);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigateNextPatch))
        ));
    }

    #[test]
    fn test_handle_normal_key_n_inserts_when_no_patch_selected() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigateNextHunk))
        ));
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_p_navigates_hunk() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigatePrevHunk))
        ));
    }

    #[test]
    fn test_handle_normal_key_shift_p_navigates_patch() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::SHIFT);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigatePrevPatch))
        ));
    }

    #[test]
    fn test_handle_normal_key_p_inserts_when_no_patch_selected() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = handle_action_keys(&event, &mut state);
        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::NavigatePrevHunk))
        ));
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_ctrl_n_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "n");
    }

    #[test]
    fn test_handle_normal_key_ctrl_p_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let action = handle_action_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "p");
    }
}
