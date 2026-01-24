use crate::state::AppState;
use crossterm::event::{KeyCode, KeyEvent};

use super::KeyAction;

/// Handle keys when there's a pending approval
pub fn handle_approval_key(event: KeyEvent, state: &mut AppState) -> Option<KeyAction> {
    match event.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => state
            .approval_ui
            .pending_approval
            .as_ref()
            .map(|approval| KeyAction::Approve { action: approval.action.clone(), risk: approval.risk.clone() }),
        KeyCode::Char('n') | KeyCode::Char('N') => state
            .approval_ui
            .pending_approval
            .as_ref()
            .map(|approval| KeyAction::Reject { action: approval.action.clone(), risk: approval.risk.clone() }),
        KeyCode::Char('c') | KeyCode::Char('C') => state
            .approval_ui
            .pending_approval
            .as_ref()
            .map(|approval| KeyAction::Cancel { action: approval.action.clone(), risk: approval.risk.clone() }),
        KeyCode::Esc => Some(KeyAction::CancelGeneration),
        _ => None,
    }
}
