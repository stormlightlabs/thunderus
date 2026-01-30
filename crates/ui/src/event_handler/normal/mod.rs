mod inspector;
mod keys;
mod memory_hits;
mod welcome;

use crate::state::AppState;
use crossterm::event::KeyEvent;

use super::KeyAction;

/// Handle keys in normal mode (no pending approval)
pub fn handle_normal_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    if state.is_first_session() {
        match welcome::handle_first_session_key(event, state) {
            welcome::FirstSessionOutcome::Return(action) => return action,
            welcome::FirstSessionOutcome::Continue => {}
        }
    }

    if let Some(action) = inspector::handle_inspector_key(event, state) {
        return Some(action);
    }

    if let Some(action) = memory_hits::handle_memory_hits_key(event, state) {
        return Some(action);
    }

    keys::handle_main_key(&event, state)
}
