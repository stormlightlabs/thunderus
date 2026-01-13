use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::Result;

use crate::state::AppState;

/// Event handler for the TUI application
pub struct EventHandler;

impl EventHandler {
    /// Read a single event from the terminal
    pub fn read() -> Result<Option<Event>> {
        match crossterm::event::poll(std::time::Duration::from_millis(100)) {
            Ok(true) => Ok(Some(crossterm::event::read()?)),
            _ => Ok(None),
        }
    }

    /// Handle a keyboard event and return whether it should exit
    pub fn handle_key_event(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        if event.kind != KeyEventKind::Press {
            return None;
        }

        if state.is_fuzzy_finder_active() {
            return Self::handle_fuzzy_finder_key(event, state);
        }

        match state.pending_approval {
            Some(_) => Self::handle_approval_key(event, state),
            None => Self::handle_normal_key(event, state),
        }
    }

    /// Handle keys in fuzzy finder mode
    fn handle_fuzzy_finder_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        if !state.is_fuzzy_finder_active() {
            return None;
        }

        match event.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                if let Some(finder) = state.fuzzy_finder_mut() {
                    finder.select_up();
                }
                Some(KeyAction::NavigateFinderUp)
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                if let Some(finder) = state.fuzzy_finder_mut() {
                    finder.select_down();
                }
                Some(KeyAction::NavigateFinderDown)
            }
            KeyCode::Enter => {
                if let Some(finder) = state.fuzzy_finder()
                    && let Some(file) = finder.selected()
                {
                    return Some(KeyAction::SelectFileInFinder { path: file.relative_path.clone() });
                }
                None
            }
            KeyCode::Esc => {
                state.exit_fuzzy_finder();
                Some(KeyAction::CancelFuzzyFinder)
            }
            KeyCode::Char(c) => {
                if let Some(finder) = state.fuzzy_finder_mut() {
                    let mut pattern = finder.pattern().to_string();
                    pattern.push(c);
                    finder.set_pattern(pattern);
                }
                None
            }
            KeyCode::Backspace => {
                if let Some(finder) = state.fuzzy_finder_mut() {
                    let mut pattern = finder.pattern().to_string();
                    pattern.pop();
                    finder.set_pattern(pattern);
                }
                None
            }
            KeyCode::Tab if event.modifiers.is_empty() => {
                if let Some(finder) = state.fuzzy_finder_mut() {
                    finder.toggle_sort();
                }
                Some(KeyAction::ToggleFinderSort)
            }
            _ => None,
        }
    }

    /// Handle keys when there's a pending approval
    fn handle_approval_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
        match event.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => state
                .pending_approval
                .as_ref()
                .map(|approval| KeyAction::Approve { action: approval.action.clone(), risk: approval.risk.clone() }),
            KeyCode::Char('n') | KeyCode::Char('N') => state
                .pending_approval
                .as_ref()
                .map(|approval| KeyAction::Reject { action: approval.action.clone(), risk: approval.risk.clone() }),
            KeyCode::Char('c') | KeyCode::Char('C') => state
                .pending_approval
                .as_ref()
                .map(|approval| KeyAction::Cancel { action: approval.action.clone(), risk: approval.risk.clone() }),
            KeyCode::Esc => Some(KeyAction::CancelGeneration),
            _ => None,
        }
    }

    /// Handle keys in normal mode (no pending approval)
    fn handle_normal_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
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
            KeyCode::Char('v') | KeyCode::Char('V') => {
                if state.input.buffer.is_empty()
                    && event.modifiers.contains(KeyModifiers::CONTROL)
                    && event.modifiers.contains(KeyModifiers::SHIFT)
                {
                    state.verbosity.toggle();
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
                if !state.input.buffer.is_empty() {
                    let message = state.input.take();

                    if let Some(command) = message.strip_prefix("!cmd ") {
                        return Some(KeyAction::ExecuteShellCommand { command: command.to_string() });
                    }

                    if let Some(cmd) = message.strip_prefix('/') {
                        return Self::parse_slash_command(cmd.to_string());
                    }

                    return Some(KeyAction::SendMessage { message });
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
            KeyCode::Char(c) => {
                if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                    return Some(KeyAction::CancelGeneration);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 's' {
                    state.toggle_sidebar();
                } else if event.modifiers.contains(KeyModifiers::CONTROL)
                    && event.modifiers.contains(KeyModifiers::SHIFT)
                    && c == 'g'
                {
                    return Some(KeyAction::OpenExternalEditor);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'u' {
                    return Some(KeyAction::PageUp);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'd' {
                    return Some(KeyAction::PageDown);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'r' {
                    return Some(KeyAction::RetryLastFailedAction);
                } else if event.modifiers.contains(KeyModifiers::CONTROL) && c == 'l' {
                    return Some(KeyAction::ClearTranscriptView);
                } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'g'
                {
                    return Some(KeyAction::ScrollToTop);
                } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == 'G'
                {
                    return Some(KeyAction::ScrollToBottom);
                } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '/'
                {
                    return Some(KeyAction::FocusSlashCommand);
                } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == '['
                {
                    return Some(KeyAction::CollapseSidebarSection);
                } else if !event.modifiers.contains(KeyModifiers::CONTROL) && state.input.buffer.is_empty() && c == ']'
                {
                    return Some(KeyAction::ExpandSidebarSection);
                } else {
                    if state.input.is_navigating_history() {
                        state.input.reset_history_navigation();
                        state.input.clear();
                    }
                    state.input.insert_char(c);
                }
            }
            KeyCode::Backspace => {
                if state.input.is_navigating_history() {
                    state.input.reset_history_navigation();
                    state.input.clear();
                } else {
                    state.input.backspace();
                }
            }
            KeyCode::Delete => {
                if state.input.is_navigating_history() {
                    state.input.reset_history_navigation();
                    state.input.clear();
                } else {
                    state.input.delete();
                }
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
            KeyCode::Home => state.input.move_home(),
            KeyCode::End => state.input.move_end(),
            KeyCode::Esc => {
                if state.is_generating() {
                    return Some(KeyAction::CancelGeneration);
                } else if !state.input.buffer.is_empty() {
                    state.input.clear();
                }
            }
            _ => (),
        }
        None
    }

    /// Parse a slash command and return the appropriate action
    fn parse_slash_command(cmd: String) -> Option<KeyAction> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        match parts[0] {
            "model" => {
                if parts.len() > 1 {
                    Some(KeyAction::SlashCommandModel { model: parts[1].to_string() })
                } else {
                    Some(KeyAction::SlashCommandModel { model: "list".to_string() })
                }
            }
            "approvals" => {
                if parts.len() > 1 {
                    Some(KeyAction::SlashCommandApprovals { mode: parts[1].to_string() })
                } else {
                    Some(KeyAction::SlashCommandApprovals { mode: "list".to_string() })
                }
            }
            "verbosity" => {
                if parts.len() > 1 {
                    Some(KeyAction::SlashCommandVerbosity { level: parts[1].to_string() })
                } else {
                    Some(KeyAction::SlashCommandVerbosity { level: "list".to_string() })
                }
            }
            "status" => Some(KeyAction::SlashCommandStatus),
            "plan" => Some(KeyAction::SlashCommandPlan),
            "review" => Some(KeyAction::SlashCommandReview),
            "memory" => Some(KeyAction::SlashCommandMemory),
            "clear" => Some(KeyAction::SlashCommandClear),
            _ => None,
        }
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
    /// Toggle verbosity level
    ToggleVerbosity,
    /// Toggle sidebar section collapse
    /// TODO: individual section control
    ToggleSidebarSection,
    /// Open external editor for current input
    OpenExternalEditor,
    /// Navigate message history (handled internally by InputState)
    NavigateHistory,
    /// Activate fuzzy finder
    ActivateFuzzyFinder,
    /// Select file in fuzzy finder
    SelectFileInFinder { path: String },
    /// Navigate fuzzy finder up
    NavigateFinderUp,
    /// Navigate fuzzy finder down
    NavigateFinderDown,
    /// Toggle fuzzy finder sort mode
    ToggleFinderSort,
    /// Cancel fuzzy finder
    CancelFuzzyFinder,
    /// Slash command: switch provider/model
    SlashCommandModel { model: String },
    /// Slash command: change approval mode
    SlashCommandApprovals { mode: String },
    /// Slash command: change verbosity level
    SlashCommandVerbosity { level: String },
    /// Slash command: show session stats
    SlashCommandStatus,
    /// Slash command: display PLAN.md content
    SlashCommandPlan,
    /// Slash command: trigger review pass
    SlashCommandReview,
    /// Slash command: display MEMORY.md content
    SlashCommandMemory,
    /// Slash command: clear transcript (keep session history)
    SlashCommandClear,
    /// Navigate to next action card
    NavigateCardNext,
    /// Navigate to previous action card
    NavigateCardPrev,
    /// Toggle expand/collapse on focused card
    ToggleCardExpand,
    /// Toggle verbose mode on focused card
    ToggleCardVerbose,
    /// Scroll transcript up by one line
    ScrollUp,
    /// Scroll transcript down by one line
    ScrollDown,
    /// Page up in transcript
    PageUp,
    /// Page down in transcript
    PageDown,
    /// Jump to top of transcript
    ScrollToTop,
    /// Jump to bottom of transcript
    ScrollToBottom,
    /// Collapse previous sidebar section
    CollapseSidebarSection,
    /// Expand next sidebar section
    ExpandSidebarSection,
    /// Retry last failed action
    RetryLastFailedAction,
    /// Focus slash command input
    FocusSlashCommand,
    /// Clear transcript view (keep history)
    ClearTranscriptView,
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
    fn test_handle_normal_key_ctrl_shift_g_open_editor() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::OpenExternalEditor)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_g_without_shift_is_regular_char() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "g");
    }

    #[test]
    fn test_parse_slash_command_model() {
        let action = EventHandler::parse_slash_command("model glm-4.7".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "glm-4.7");
        }
    }

    #[test]
    fn test_parse_slash_command_model_list() {
        let action = EventHandler::parse_slash_command("model".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_approvals() {
        let action = EventHandler::parse_slash_command("approvals read-only".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandApprovals { .. })));
        if let Some(KeyAction::SlashCommandApprovals { mode }) = action {
            assert_eq!(mode, "read-only");
        }
    }

    #[test]
    fn test_parse_slash_command_approvals_list() {
        let action = EventHandler::parse_slash_command("approvals".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandApprovals { .. })));
        if let Some(KeyAction::SlashCommandApprovals { mode }) = action {
            assert_eq!(mode, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity() {
        let action = EventHandler::parse_slash_command("verbosity verbose".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandVerbosity { .. })));
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action {
            assert_eq!(level, "verbose");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity_list() {
        let action = EventHandler::parse_slash_command("verbosity".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandVerbosity { .. })));
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action {
            assert_eq!(level, "list");
        }
    }

    #[test]
    fn test_parse_slash_command_verbosity_all_levels() {
        let action_quiet = EventHandler::parse_slash_command("verbosity quiet".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_quiet {
            assert_eq!(level, "quiet");
        }

        let action_default = EventHandler::parse_slash_command("verbosity default".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_default {
            assert_eq!(level, "default");
        }

        let action_verbose = EventHandler::parse_slash_command("verbosity verbose".to_string());
        if let Some(KeyAction::SlashCommandVerbosity { level }) = action_verbose {
            assert_eq!(level, "verbose");
        }
    }

    #[test]
    fn test_parse_slash_command_status() {
        let action = EventHandler::parse_slash_command("status".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandStatus)));
    }

    #[test]
    fn test_parse_slash_command_plan() {
        let action = EventHandler::parse_slash_command("plan".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandPlan)));
    }

    #[test]
    fn test_parse_slash_command_review() {
        let action = EventHandler::parse_slash_command("review".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandReview)));
    }

    #[test]
    fn test_parse_slash_command_memory() {
        let action = EventHandler::parse_slash_command("memory".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandMemory)));
    }

    #[test]
    fn test_parse_slash_command_clear() {
        let action = EventHandler::parse_slash_command("clear".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandClear)));
    }

    #[test]
    fn test_handle_normal_key_j_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::NavigateCardNext)));
    }

    #[test]
    fn test_handle_normal_key_k_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::NavigateCardPrev)));
    }

    #[test]
    fn test_handle_normal_key_space_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::ToggleCardExpand)));
    }

    #[test]
    fn test_handle_normal_key_v_with_empty_input() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::ToggleCardVerbose)));
    }

    #[test]
    fn test_handle_normal_key_j_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "jtest");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_k_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "ktest");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_space_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, " test");
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_v_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "test".to_string();

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "vtest");
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_unknown() {
        let action = EventHandler::parse_slash_command("unknown_command".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_empty() {
        let action = EventHandler::parse_slash_command("".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_parse_slash_command_whitespace_only() {
        let action = EventHandler::parse_slash_command("   ".to_string());
        assert!(action.is_none());
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command() {
        let mut state = create_test_state();
        state.input.buffer = "/status".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SlashCommandStatus)));
    }

    #[test]
    fn test_handle_normal_key_enter_slash_command_with_args() {
        let mut state = create_test_state();
        state.input.buffer = "/model glm-4.7".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.input.buffer, "");
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
    }

    #[test]
    fn test_handle_normal_key_backslash_not_slash() {
        let mut state = create_test_state();
        state.input.buffer = "This is a \\ not a slash".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_handle_normal_key_slash_in_middle() {
        let mut state = create_test_state();
        state.input.buffer = "This has / in the middle".to_string();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::SendMessage { .. })));
    }

    #[test]
    fn test_parse_slash_command_extra_whitespace() {
        let action = EventHandler::parse_slash_command("  model   glm-4.7  ".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "glm-4.7");
        }
    }

    #[test]
    fn test_parse_slash_command_multiple_words() {
        let action = EventHandler::parse_slash_command("model some model name".to_string());
        assert!(matches!(action, Some(KeyAction::SlashCommandModel { .. })));
        if let Some(KeyAction::SlashCommandModel { model }) = action {
            assert_eq!(model, "some");
        }
    }

    #[test]
    fn test_handle_normal_key_ctrl_shift_v_toggle_verbosity() {
        let mut state = create_test_state();
        assert_eq!(state.verbosity.as_str(), "quiet");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ToggleVerbosity)));
        assert_eq!(state.verbosity.as_str(), "default");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.verbosity.as_str(), "verbose");

        let event = KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let _action = EventHandler::handle_key_event(event, &mut state);

        assert_eq!(state.verbosity.as_str(), "quiet");
    }

    #[test]
    fn test_handle_normal_key_backspace_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert!(!state.input.is_navigating_history());
        assert_eq!(state.input.buffer, "");
    }

    #[test]
    fn test_handle_normal_key_delete_resets_history_navigation() {
        let mut state = create_test_state();
        state.input.add_to_history("history message".to_string());

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        EventHandler::handle_key_event(event, &mut state);

        assert!(state.input.is_navigating_history());

        let event = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

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
        let action = EventHandler::handle_key_event(event, &mut state);

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
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "Helo");
        assert_eq!(state.input.cursor, 2);
    }

    #[test]
    fn test_handle_normal_key_ctrl_u_page_up() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::PageUp)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_d_page_down() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::PageDown)));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_top() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ScrollToTop)));
    }

    #[test]
    fn test_handle_normal_key_g_jump_to_bottom() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ScrollToBottom)));
    }

    #[test]
    fn test_handle_normal_key_left_bracket_collapse_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::CollapseSidebarSection)));
    }

    #[test]
    fn test_handle_normal_key_right_bracket_expand_section() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ExpandSidebarSection)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_r_retry() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::RetryLastFailedAction)));
    }

    #[test]
    fn test_handle_normal_key_slash_focus_command() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::FocusSlashCommand)));
    }

    #[test]
    fn test_handle_normal_key_ctrl_l_clear_transcript() {
        let mut state = create_test_state();

        let event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(matches!(action, Some(KeyAction::ClearTranscriptView)));
    }

    #[test]
    fn test_handle_normal_key_g_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();

        let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "gsome text");
    }

    #[test]
    fn test_handle_normal_key_slash_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "some text".to_string();
        state.input.cursor = state.input.buffer.len();

        let event = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = EventHandler::handle_key_event(event, &mut state);

        assert!(action.is_none());
        assert_eq!(state.input.buffer, "some text/");
    }
}
