use super::{KeyAction, KeyHandling};
use crate::state::AppState;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) fn handle_char_keys(event: &KeyEvent, state: &mut AppState) -> KeyHandling {
    let KeyCode::Char(c) = event.code else {
        return KeyHandling::Pass;
    };

    if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
        if state.is_generating() {
            return KeyHandling::Handled(Some(KeyAction::CancelGeneration));
        } else if state.record_ctrl_c_press() {
            return KeyHandling::Handled(Some(KeyAction::Exit));
        } else {
            state.show_hint("Press Ctrl+C again to exit");
        }
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 's' {
        state.toggle_sidebar();
        return KeyHandling::Handled(Some(KeyAction::ToggleSidebar));
    } else if event.modifiers.contains(KeyModifiers::CONTROL)
        && event.modifiers.contains(KeyModifiers::SHIFT)
        && c == 'g'
    {
        return KeyHandling::Handled(Some(KeyAction::OpenExternalEditor));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'u' {
        return KeyHandling::Handled(Some(KeyAction::PageUp));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'd' {
        return KeyHandling::Handled(Some(KeyAction::Exit));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 't' {
        return KeyHandling::Handled(Some(KeyAction::ToggleTheme));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'a' {
        return KeyHandling::Handled(Some(KeyAction::ToggleAdvisorMode));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
        return KeyHandling::Handled(Some(KeyAction::RetryLastFailedAction));
    } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'l' {
        return KeyHandling::Handled(Some(KeyAction::ClearTranscriptView));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'd' {
        if state.selected_patch_index().is_some() {
            return KeyHandling::Handled(Some(KeyAction::ToggleHunkDetails));
        }
        state.input.insert_char(c);
        return KeyHandling::Handled(None);
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'r' {
        if state.selected_patch_index().is_some() {
            return KeyHandling::Handled(Some(KeyAction::RejectHunk));
        }
        return KeyHandling::Handled(Some(KeyAction::RetryLastFailedAction));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'g' {
        return KeyHandling::Handled(Some(KeyAction::ScrollToTop));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'G' {
        return KeyHandling::Handled(Some(KeyAction::ScrollToBottom));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '/' {
        return KeyHandling::Handled(Some(KeyAction::FocusSlashCommand));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '[' {
        return KeyHandling::Handled(Some(KeyAction::CollapseSidebarSection));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == ']' {
        return KeyHandling::Handled(Some(KeyAction::ExpandSidebarSection));
    } else if !event.modifiers.contains(KeyModifiers::CONTROL) && c == '@' {
        return KeyHandling::Handled(Some(KeyAction::ActivateFuzzyFinder));
    }

    state.reset_ctrl_c_count();
    if state.input.is_navigating_history() {
        state.input.reset_history_navigation();
        state.input.clear();
    }
    state.input.insert_char(c);
    KeyHandling::Handled(None)
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
    fn test_handle_normal_key_char_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE);
        handle_char_keys(&event, &mut state);

        assert_eq!(state.input.buffer, "H");
    }

    #[test]
    fn test_handle_normal_key_multiple_chars() {
        let mut state = create_test_state();

        for c in "Hello".chars() {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            handle_char_keys(&event, &mut state);
        }

        assert_eq!(state.input.buffer, "Hello");
    }

    #[test]
    fn test_handle_normal_key_ctrl_c_cancel() {
        let mut state = create_test_state();
        state.start_generation();

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::CancelGeneration))
        ));
    }

    #[test]
    fn test_handle_normal_key_toggle_sidebar() {
        let mut state = create_test_state();
        assert!(state.ui.sidebar_visible);

        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::ToggleSidebar))));
        assert!(!state.ui.sidebar_visible);
    }

    #[test]
    fn test_handle_normal_key_ctrl_shift_g_open_editor() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::OpenExternalEditor))
        ));
    }

    #[test]
    fn test_handle_normal_key_ctrl_g_without_shift_is_regular_char() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "g");
    }

    #[test]
    fn test_handle_normal_key_ctrl_u_page_up() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::PageUp))));
    }

    #[test]
    fn test_handle_normal_key_ctrl_d_exits() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);
        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::Exit))));

        state.input.insert_char('a');
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);
        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::Exit))));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_top() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::ScrollToTop))));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_bottom() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::ScrollToBottom))));
    }

    #[test]
    fn test_handle_normal_key_left_bracket_collapse_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::CollapseSidebarSection))
        ));
    }

    #[test]
    fn test_handle_normal_key_right_bracket_expand_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::ExpandSidebarSection))
        ));
    }

    #[test]
    fn test_handle_normal_key_ctrl_r_retry() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::RetryLastFailedAction))
        ));
    }

    #[test]
    fn test_handle_normal_key_r_retry() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::RetryLastFailedAction))
        ));
    }

    #[test]
    fn test_handle_normal_key_slash_focus_command() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::FocusSlashCommand))
        ));
    }

    #[test]
    fn test_handle_normal_key_ctrl_l_clear_transcript() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::ClearTranscriptView))
        ));
    }

    #[test]
    fn test_handle_normal_key_g_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "gsome text");
    }

    #[test]
    fn test_handle_normal_key_slash_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();
        state.input.cursor = state.input.buffer.len();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "some text/");
    }

    #[test]
    fn test_handle_normal_key_d_toggles_details_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::ToggleHunkDetails))
        ));
    }

    #[test]
    fn test_handle_normal_key_d_inserts_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(None)));
        assert_eq!(state.input.buffer, "d");
    }

    #[test]
    fn test_handle_normal_key_r_rejects_hunk_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(action, KeyHandling::Handled(Some(KeyAction::RejectHunk))));
    }

    #[test]
    fn test_handle_normal_key_r_retries_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_char_keys(&event, &mut state);

        assert!(matches!(
            action,
            KeyHandling::Handled(Some(KeyAction::RetryLastFailedAction))
        ));
    }
}
