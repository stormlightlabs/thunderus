use crate::state::{AppState, MainView};
use crossterm::event::{KeyCode, KeyEvent};

use super::KeyAction;

pub fn handle_inspector_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    if !matches!(state.ui.active_view, MainView::Inspector) {
        return None;
    }

    match event.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            state.evidence.select_prev();
            Some(KeyAction::InspectorNavigate)
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            state.evidence.select_next();
            Some(KeyAction::InspectorNavigate)
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            state.evidence.scroll_up();
            Some(KeyAction::InspectorNavigate)
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            state.evidence.scroll_down();
            Some(KeyAction::InspectorNavigate)
        }
        KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Esc => {
            state.ui.toggle_inspector();
            Some(KeyAction::ToggleInspector)
        }
        KeyCode::Char('f') | KeyCode::Char('F') => Some(KeyAction::InspectorOpenFile { path: String::new() }),
        _ => None,
    }
}
