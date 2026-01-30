use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent};

use super::KeyAction;

pub fn handle_memory_hits_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    if !state.memory_hits.is_visible() {
        return None;
    }

    match event.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            state.memory_hits.select_next();
            Some(KeyAction::MemoryHitsNavigate)
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            state.memory_hits.select_prev();
            Some(KeyAction::MemoryHitsNavigate)
        }
        KeyCode::Enter => state
            .memory_hits
            .selected_hit()
            .map(|hit| KeyAction::MemoryHitsOpen { path: hit.path.clone() }),
        KeyCode::Char('p') | KeyCode::Char('P') => match state.memory_hits.selected_hit() {
            Some(hit) => {
                let id = hit.id.clone();
                state.memory_hits.toggle_pin(&id);
                Some(KeyAction::MemoryHitsPin { id })
            }
            None => None,
        },
        KeyCode::Char('i') | KeyCode::Char('I') => state
            .memory_hits
            .selected_hit()
            .map(|hit| KeyAction::InspectMemory { path: hit.path.clone() }),
        KeyCode::Esc => {
            state.memory_hits.clear();
            Some(KeyAction::MemoryHitsClose)
        }
        _ => None,
    }
}
