mod action_keys;
mod char_keys;
mod edit_keys;

use super::KeyAction;
use crate::state::AppState;

use crossterm::event::KeyEvent;

enum KeyHandling {
    Handled(Option<KeyAction>),
    Pass,
}

pub(super) fn handle_main_key(event: &KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    for handler in [
        edit_keys::handle_edit_keys,
        action_keys::handle_action_keys,
        char_keys::handle_char_keys,
    ] {
        match handler(event, state) {
            KeyHandling::Handled(action) => return action,
            KeyHandling::Pass => {}
        }
    }

    None
}
