use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{KeyAction, slash_parser::parse_slash_command};

/// Handle keys in normal mode (no pending approval)
///
/// Welcome screen keystroke passthrough:
/// Any printable character (without Ctrl/Alt) dismisses welcome and starts typing.
/// We only trigger on printable chars without Ctrl/Alt modifiers to avoid interfering
/// with other keybindings.
pub fn handle_normal_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    if state.is_first_session() {
        let has_ctrl_or_alt =
            event.modifiers.contains(KeyModifiers::CONTROL) || event.modifiers.contains(KeyModifiers::ALT);

        match event.code {
            KeyCode::Enter => {
                if !state.input.buffer.is_empty() {
                    state.exit_first_session();
                    let message = state.input.take();

                    if let Some(command) = message.strip_prefix("!cmd ") {
                        return Some(KeyAction::ExecuteShellCommand { command: command.to_string() });
                    }

                    if let Some(cmd) = message.strip_prefix('/') {
                        return parse_slash_command(cmd.to_string());
                    }

                    return Some(KeyAction::SendMessage { message });
                }
            }
            KeyCode::Backspace => state.input.backspace(),
            KeyCode::Delete => state.input.delete(),
            KeyCode::Left => state.input.move_left(),
            KeyCode::Right => state.input.move_right(),
            KeyCode::Home => state.input.move_home(),
            KeyCode::End => state.input.move_end(),
            KeyCode::Char(c) if !has_ctrl_or_alt => state.input.insert_char(c),
            _ => {}
        }

        return None;
    }

    if matches!(state.ui.active_view, crate::state::MainView::Inspector) {
        match event.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                state.evidence.select_prev();
                return Some(KeyAction::InspectorNavigate);
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                state.evidence.select_next();
                return Some(KeyAction::InspectorNavigate);
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                state.evidence.scroll_up();
                return Some(KeyAction::InspectorNavigate);
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                state.evidence.scroll_down();
                return Some(KeyAction::InspectorNavigate);
            }
            KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Esc => {
                state.ui.toggle_inspector();
                return Some(KeyAction::ToggleInspector);
            }
            KeyCode::Char('f') | KeyCode::Char('F') => {
                return Some(KeyAction::InspectorOpenFile { path: String::new() });
            }
            _ => {}
        }
    }

    if state.memory_hits.is_visible() {
        match event.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                state.memory_hits.select_next();
                return Some(KeyAction::MemoryHitsNavigate);
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                state.memory_hits.select_prev();
                return Some(KeyAction::MemoryHitsNavigate);
            }
            KeyCode::Enter => {
                if let Some(hit) = state.memory_hits.selected_hit() {
                    return Some(KeyAction::MemoryHitsOpen { path: hit.path.clone() });
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if let Some(hit) = state.memory_hits.selected_hit() {
                    let id = hit.id.clone();
                    state.memory_hits.toggle_pin(&id);
                    return Some(KeyAction::MemoryHitsPin { id });
                }
            }
            KeyCode::Char('i') | KeyCode::Char('I') => {
                if let Some(hit) = state.memory_hits.selected_hit() {
                    return Some(KeyAction::InspectMemory { path: hit.path.clone() });
                }
            }
            KeyCode::Esc => {
                state.memory_hits.clear();
                return Some(KeyAction::MemoryHitsClose);
            }
            _ => {}
        }
    }

    match event.code {
        KeyCode::Up => state.input.navigate_up(),
        KeyCode::Down => state.input.navigate_down(),
        KeyCode::Char(' ') => {
            if state.input.buffer.is_empty() {
                return Some(KeyAction::ToggleCardExpand);
            } else {
                state.input.insert_char(' ');
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL)
                && state.input.buffer.is_empty()
                && state.selected_patch_index().is_some()
            {
                return Some(KeyAction::ApproveHunk);
            } else {
                state.input.insert_char('a');
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL) {
                if state.input.buffer.is_empty() && event.modifiers.contains(KeyModifiers::SHIFT) {
                    return Some(KeyAction::NavigateNextPatch);
                } else if state.input.buffer.is_empty() {
                    return Some(KeyAction::NavigateNextHunk);
                } else {
                    state.input.insert_char('n');
                }
            } else {
                state.input.insert_char('n');
            }
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            if !event.modifiers.contains(KeyModifiers::CONTROL) {
                if state.input.buffer.is_empty() && event.modifiers.contains(KeyModifiers::SHIFT) {
                    return Some(KeyAction::NavigatePrevPatch);
                } else if state.input.buffer.is_empty() {
                    return Some(KeyAction::NavigatePrevHunk);
                } else {
                    state.input.insert_char('p');
                }
            } else {
                state.input.insert_char('p');
            }
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if state.input.buffer.is_empty()
                && event.modifiers.contains(KeyModifiers::CONTROL)
                && event.modifiers.contains(KeyModifiers::SHIFT)
            {
                state.config.verbosity.toggle();
                return Some(KeyAction::ToggleVerbosity);
            } else if state.input.buffer.is_empty()
                && !event.modifiers.contains(KeyModifiers::CONTROL)
                && !event.modifiers.contains(KeyModifiers::SHIFT)
            {
                return Some(KeyAction::ToggleCardVerbose);
            } else {
                state.input.insert_char('v');
            }
        }
        KeyCode::Enter => {
            if state.ui.is_reconciling() {
                return Some(KeyAction::ReconcileContinue);
            }
            if !state.input.buffer.is_empty() {
                let message = state.input.take();

                if let Some(command) = message.strip_prefix("!cmd ") {
                    return Some(KeyAction::ExecuteShellCommand { command: command.to_string() });
                }

                if let Some(cmd) = message.strip_prefix('/') {
                    return parse_slash_command(cmd.to_string());
                }

                return Some(KeyAction::SendMessage { message });
            }
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            if state.ui.is_reconciling() {
                return Some(KeyAction::ReconcileStop);
            } else if state.input.buffer.is_empty() {
                return Some(KeyAction::Exit);
            } else {
                state.input.insert_char('q');
            }
        }
        KeyCode::Char('j') | KeyCode::Char('J') => {
            if state.input.buffer.is_empty() {
                return Some(KeyAction::NavigateCardNext);
            } else {
                state.input.insert_char('j');
            }
        }
        KeyCode::Char('k') | KeyCode::Char('K') => {
            if state.input.buffer.is_empty() {
                return Some(KeyAction::NavigateCardPrev);
            } else {
                state.input.insert_char('k');
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if state.input.buffer.is_empty() {
                return Some(KeyAction::ToggleInspector);
            } else {
                state.input.insert_char('i');
            }
        }
        KeyCode::Char(c) => {
            if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                if state.is_generating() {
                    return Some(KeyAction::CancelGeneration);
                } else if state.record_ctrl_c_press() {
                    return Some(KeyAction::Exit);
                } else {
                    state.show_hint("Press Ctrl+C again to exit");
                }
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 's' {
                state.toggle_sidebar();
                return Some(KeyAction::ToggleSidebar);
            } else if event.modifiers.contains(KeyModifiers::CONTROL)
                && event.modifiers.contains(KeyModifiers::SHIFT)
                && c == 'g'
            {
                return Some(KeyAction::OpenExternalEditor);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'u' {
                return Some(KeyAction::PageUp);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'd' {
                return Some(KeyAction::Exit);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 't' {
                return Some(KeyAction::ToggleTheme);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'a' {
                return Some(KeyAction::ToggleAdvisorMode);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                return Some(KeyAction::RetryLastFailedAction);
            } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'l' {
                return Some(KeyAction::ClearTranscriptView);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'd' {
                if state.selected_patch_index().is_some() {
                    return Some(KeyAction::ToggleHunkDetails);
                }
                state.input.insert_char(c);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'r' {
                if state.selected_patch_index().is_some() {
                    return Some(KeyAction::RejectHunk);
                }
                return Some(KeyAction::RetryLastFailedAction);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'g' {
                return Some(KeyAction::ScrollToTop);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'G' {
                return Some(KeyAction::ScrollToBottom);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '/' {
                return Some(KeyAction::FocusSlashCommand);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '[' {
                return Some(KeyAction::CollapseSidebarSection);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == ']' {
                return Some(KeyAction::ExpandSidebarSection);
            } else if !event.modifiers.contains(KeyModifiers::CONTROL) && c == '@' {
                return Some(KeyAction::ActivateFuzzyFinder);
            } else {
                state.reset_ctrl_c_count();
                if state.input.is_navigating_history() {
                    state.input.reset_history_navigation();
                    state.input.clear();
                }
                state.input.insert_char(c);
            }
        }
        KeyCode::Backspace => {
            state.reset_ctrl_c_count();
            if state.input.is_navigating_history() {
                state.input.reset_history_navigation();
                state.input.clear();
            } else {
                state.input.backspace();
            }
        }
        KeyCode::Delete => {
            state.reset_ctrl_c_count();
            if state.input.is_navigating_history() {
                state.input.reset_history_navigation();
                state.input.clear();
            } else {
                state.input.delete();
            }
        }
        KeyCode::Left => {
            state.reset_ctrl_c_count();
            if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                state.scroll_horizontal(-10);
            } else {
                state.input.move_left();
            }
        }
        KeyCode::Right => {
            state.reset_ctrl_c_count();
            if state.input.buffer.is_empty() && event.modifiers.is_empty() {
                state.scroll_horizontal(10);
            } else {
                state.input.move_right();
            }
        }
        KeyCode::Home => {
            state.reset_ctrl_c_count();
            state.input.move_home();
        }
        KeyCode::End => {
            state.reset_ctrl_c_count();
            state.input.move_end();
        }
        KeyCode::Esc => {
            state.reset_ctrl_c_count();
            if state.is_generating() {
                return Some(KeyAction::CancelGeneration);
            } else if state.is_paused() {
                return Some(KeyAction::StartReconcileRitual);
            } else if state.ui.is_reconciling() {
                return Some(KeyAction::ReconcileDiscard);
            } else if state.input.is_navigating_history() && state.input.buffer.is_empty() {
                state.input.enter_fork_mode();
                return Some(KeyAction::NoOp);
            } else if state.input.is_in_fork_mode() {
                state.input.exit_fork_mode();
                return Some(KeyAction::NoOp);
            } else if !state.input.buffer.is_empty() {
                state.input.clear();
            } else if !state.input.message_history.is_empty() {
                return Some(KeyAction::RewindLastMessage);
            }
        }
        _ => (),
    }
    None
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
        handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "H");
    }

    #[test]
    fn test_handle_normal_key_multiple_chars() {
        let mut state = create_test_state();

        for c in "Hello".chars() {
            let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            handle_normal_key(event, &mut state);
        }

        assert_eq!(state.input.buffer, "Hello");
    }

    #[test]
    fn test_handle_normal_key_backspace() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 5;

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "Hell");
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_enter() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_handle_normal_key_enter_empty() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_navigation() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);
        assert_eq!(state.input.cursor, 3);

        let event = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_home_end() {
        let mut state = create_test_state();
        state.input.buffer = "Test".to_string();
        state.input.cursor = 4;

        let event = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);
        assert_eq!(state.input.cursor, 0);

        let event = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_toggle_sidebar() {
        let mut state = create_test_state();
        assert!(state.ui.sidebar_visible);

        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        handle_normal_key(event, &mut state);

        assert!(!state.ui.sidebar_visible);
    }

    #[test]
    fn test_handle_normal_key_esc_clears_input() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_ctrl_c_cancel() {
        let mut state = create_test_state();
        state.start_generation();

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::CancelGeneration)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_shift_g_open_editor() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::OpenExternalEditor)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_g_without_shift_is_regular_char() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "g");
    }

    #[test]
    fn test_handle_normal_key_j_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::NavigateCardNext)));
    }

    #[test]
    fn test_handle_normal_key_k_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::NavigateCardPrev)));
    }

    #[test]
    fn test_handle_normal_key_space_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::ToggleCardExpand)));
    }

    #[test]
    fn test_handle_normal_key_v_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::ToggleCardVerbose)));
    }

    #[test]
    fn test_handle_normal_key_j_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "jtest");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_k_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "ktest");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_space_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, " test");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_v_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "vtest");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command() {
        let mut state = create_test_state();
        state.input.buffer = "/status".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SlashCommandStatus)));
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command_with_args() {
        let mut state = create_test_state();
        state.input.buffer = "/model glm-4.7".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
    }

    #[test]
    fn test_handle_normal_key_backslash_not_slash() {
        let mut state = create_test_state();
        state.input.buffer = "This is a \\ not a slash".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_handle_normal_key_slash_in_middle() {
        let mut state = create_test_state();
        state.input.buffer = "This has / in the middle".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_handle_normal_key_ctrl_shift_v_toggle_verbosity() {
        let mut state = create_test_state();
        assert_eq!(state.config.verbosity.as_str(), "quiet");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ToggleVerbosity)));
        assert_eq!(state.config.verbosity.as_str(), "default");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = handle_normal_key(event, &mut state);

        assert_eq!(state.config.verbosity.as_str(), "verbose");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = handle_normal_key(event, &mut state);

        assert_eq!(state.config.verbosity.as_str(), "quiet");
    }

    #[test]
    fn test_handle_normal_key_backspace_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert!(!state.input.is_navigating_history());
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_delete_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_normal_key(event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert!(!state.input.is_navigating_history());
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_backspace_normal_editing() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 5;

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "Hell");
        assert_eq!(state.input.cursor, 4);
    }

    #[test]
    fn test_handle_normal_key_delete_normal_editing() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();
        state.input.cursor = 2;

        let event = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "Helo");
        assert_eq!(state.input.cursor, 2);
    }

    #[test]
    fn test_handle_normal_key_ctrl_u_page_up() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::PageUp)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_d_exits() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::Exit)));

        state.input.insert_char('a');
        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::Exit)));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_top() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ScrollToTop)));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_bottom() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ScrollToBottom)));
    }

    #[test]
    fn test_handle_normal_key_left_bracket_collapse_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::CollapseSidebarSection)));
    }

    #[test]
    fn test_handle_normal_key_right_bracket_expand_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ExpandSidebarSection)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_r_retry() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::RetryLastFailedAction)));
    }

    #[test]
    fn test_handle_normal_key_r_retry() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::RetryLastFailedAction)));
    }

    #[test]
    fn test_handle_normal_key_slash_focus_command() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::FocusSlashCommand)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_l_clear_transcript() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ClearTranscriptView)));
    }

    #[test]
    fn test_handle_normal_key_g_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "gsome text");
    }

    #[test]
    fn test_handle_normal_key_slash_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();
        state.input.cursor = state.input.buffer.len();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "some text/");
    }

    #[test]
    fn test_handle_normal_key_a_approves_hunk_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ApproveHunk)));
    }

    #[test]
    fn test_handle_normal_key_a_inserts_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "a");
    }

    #[test]
    fn test_handle_normal_key_a_inserts_when_typing() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);
        assert!(action.is_none());
        assert_eq!(state.input.buffer, "atest");
    }

    #[test]
    fn test_handle_normal_key_ctrl_a_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);
        assert!(action.is_none());
        assert_eq!(state.input.buffer, "a");
    }

    #[test]
    fn test_handle_normal_key_d_toggles_details_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ToggleHunkDetails)));
    }

    #[test]
    fn test_handle_normal_key_d_inserts_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "d");
    }

    #[test]
    fn test_handle_normal_key_n_navigates_hunk() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::NavigateNextHunk)));
    }

    #[test]
    fn test_handle_normal_key_shift_n_navigates_patch() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::SHIFT);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::NavigateNextPatch)));
    }

    #[test]
    fn test_handle_normal_key_n_inserts_when_no_patch_selected() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::NavigateNextHunk)));
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_p_navigates_hunk() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::NavigatePrevHunk)));
    }

    #[test]
    fn test_handle_normal_key_shift_p_navigates_patch() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::SHIFT);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::NavigatePrevPatch)));
    }

    #[test]
    fn test_handle_normal_key_p_inserts_when_no_patch_selected() {
        let mut state = create_test_state();
        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::NavigatePrevHunk)));
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_ctrl_n_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "n");
    }

    #[test]
    fn test_handle_normal_key_ctrl_p_inserts_character() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        let action = handle_normal_key(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "p");
    }

    #[test]
    fn test_handle_normal_key_r_rejects_hunk_when_patch_selected() {
        let mut state = create_test_state();
        state.ui.diff_navigation.selected_patch_index = Some(0);

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::RejectHunk)));
    }

    #[test]
    fn test_handle_normal_key_r_retries_when_no_patch_selected() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        let action = handle_normal_key(event, &mut state);

        assert!(matches!(action, Some(KeyAction::RetryLastFailedAction)));
    }
}
