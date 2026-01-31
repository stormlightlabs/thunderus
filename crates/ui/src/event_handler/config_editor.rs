use crate::KeyAction;
use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle key events when the config editor is active
///
/// Escape: Cancel and close
/// Ctrl+S: Save and close
/// Tab: Navigate to next field
/// Shift+Tab: Navigate to previous field
/// Enter or Space: Toggle value on editable fields
/// Arrow keys for navigation
pub fn handle_config_editor_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    let editor = state.config_editor.as_mut()?;

    match (event.code, event.modifiers) {
        (KeyCode::Esc, _) => Some(KeyAction::ConfigEditorCancel),
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(KeyAction::ConfigEditorSave),
        (KeyCode::Tab, KeyModifiers::NONE) => {
            editor.next_field();
            Some(KeyAction::NoOp)
        }
        (KeyCode::BackTab, _) | (KeyCode::Tab, KeyModifiers::SHIFT) => {
            editor.prev_field();
            Some(KeyAction::NoOp)
        }
        (KeyCode::Enter, KeyModifiers::NONE) | (KeyCode::Char(' '), KeyModifiers::NONE) => {
            if editor.focused_field().is_editable() {
                editor.toggle_value();
            }
            Some(KeyAction::NoOp)
        }
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
            editor.next_field();
            Some(KeyAction::NoOp)
        }
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
            editor.prev_field();
            Some(KeyAction::NoOp)
        }
        _ => Some(KeyAction::NoOp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConfigEditorState;
    use thunderus_core::{ApprovalMode, SandboxMode};

    fn create_test_state_with_editor() -> AppState {
        AppState {
            config_editor: Some(ConfigEditorState::new(
                "default".to_string(),
                ApprovalMode::Auto,
                SandboxMode::Policy,
                false,
                "glm-4.7".to_string(),
                None,
            )),
            ..Default::default()
        }
    }

    #[test]
    fn test_escape_returns_cancel_action() {
        let mut state = create_test_state_with_editor();
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = handle_config_editor_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::ConfigEditorCancel)));
    }

    #[test]
    fn test_ctrl_s_returns_save_action() {
        let mut state = create_test_state_with_editor();
        let event = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        let action = handle_config_editor_key(event, &mut state);
        assert!(matches!(action, Some(KeyAction::ConfigEditorSave)));
    }

    #[test]
    fn test_tab_moves_to_next_field() {
        let mut state = create_test_state_with_editor();
        let initial_idx = state.config_editor.as_ref().unwrap().focused_field_index;

        let event = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        handle_config_editor_key(event, &mut state);

        let new_idx = state.config_editor.as_ref().unwrap().focused_field_index;
        assert_eq!(new_idx, (initial_idx + 1) % 5);
    }

    #[test]
    fn test_shift_tab_moves_to_prev_field() {
        let mut state = create_test_state_with_editor();
        state.config_editor.as_mut().unwrap().focused_field_index = 2;

        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        handle_config_editor_key(event, &mut state);

        let new_idx = state.config_editor.as_ref().unwrap().focused_field_index;
        assert_eq!(new_idx, 1);
    }

    #[test]
    fn test_enter_toggles_editable_field() {
        let mut state = create_test_state_with_editor();
        state.config_editor.as_mut().unwrap().focused_field_index = 1;

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_config_editor_key(event, &mut state);

        assert_eq!(
            state.config_editor.as_ref().unwrap().approval_mode,
            ApprovalMode::FullAccess
        );
    }

    #[test]
    fn test_enter_does_not_toggle_readonly_field() {
        let mut state = create_test_state_with_editor();
        state.config_editor.as_mut().unwrap().focused_field_index = 0;

        let original = state.config_editor.as_ref().unwrap().profile_name.clone();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_config_editor_key(event, &mut state);

        assert_eq!(state.config_editor.as_ref().unwrap().profile_name, original);
    }

    #[test]
    fn test_no_editor_returns_none() {
        let mut state = AppState::default();
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = handle_config_editor_key(event, &mut state);
        assert!(action.is_none());
    }
}
