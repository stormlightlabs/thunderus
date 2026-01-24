use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent};

use super::KeyAction;

/// Handle keys in fuzzy finder mode
pub fn handle_fuzzy_finder_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
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
