use super::KeyAction;
use crate::{slash::parse_slash_command, state::AppState};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum FirstSessionOutcome {
    Return(Option<KeyAction>),
    Continue,
}

/// Welcome screen keystroke passthrough:
/// Any printable character (without Ctrl/Alt) dismisses welcome and starts typing.
/// We only trigger on printable chars without Ctrl/Alt modifiers to avoid interfering
/// with other keybindings.
pub fn handle_first_session_key(event: KeyEvent, state: &mut AppState) -> FirstSessionOutcome {
    let has_ctrl_or_alt =
        event.modifiers.contains(KeyModifiers::CONTROL) || event.modifiers.contains(KeyModifiers::ALT);

    match event.code {
        KeyCode::Enter => {
            if !state.input.buffer.is_empty() {
                state.exit_first_session();
                let message = state.input.take();

                if let Some(command) = message.strip_prefix("!cmd ") {
                    return FirstSessionOutcome::Return(Some(KeyAction::ExecuteShellCommand {
                        command: command.to_string(),
                    }));
                }

                if let Some(cmd) = message.strip_prefix('/') {
                    return FirstSessionOutcome::Return(parse_slash_command(cmd.to_string()));
                }

                return FirstSessionOutcome::Return(Some(KeyAction::SendMessage { message }));
            }
        }
        KeyCode::Backspace => state.input.backspace(),
        KeyCode::Delete => state.input.delete(),
        KeyCode::Left => state.input.move_left(),
        KeyCode::Right => state.input.move_right(),
        KeyCode::Home => state.input.move_home(),
        KeyCode::End => state.input.move_end(),
        KeyCode::Char(c) if !has_ctrl_or_alt => {
            state.exit_first_session();
            state.input.insert_char(c);
        }
        _ => {}
    }

    if !has_ctrl_or_alt {
        return FirstSessionOutcome::Return(None);
    }

    FirstSessionOutcome::Continue
}
