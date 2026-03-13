use crate::chat;
use crate::files;
use crate::help;
use crate::settings;
use crate::welcome;
use crate::{App, Msg, ScreenMode};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

pub(crate) fn map_key(app: &App, key: KeyEvent) -> Option<Msg> {
    if key.kind != KeyEventKind::Press {
        return None;
    }

    match key.code {
        KeyCode::Char(',') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Msg::OpenSettings),
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Msg::StartNewChat),
        KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Msg::OpenFiles),
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Msg::CloseActiveChat),
        KeyCode::F(1) => return Some(Msg::OpenHelp),
        _ => {}
    }

    if app.screen_mode == ScreenMode::Welcome && app.chat.is_file_finder_active() {
        return chat::map_chat_key_to_msg(key, true).map(Msg::Chat);
    }

    match app.screen_mode {
        ScreenMode::Welcome => welcome::map_welcome_key_to_msg(key).map(Msg::Welcome),
        ScreenMode::Chat => chat::map_chat_key_to_msg(key, app.chat.is_file_finder_active()).map(Msg::Chat),
        ScreenMode::Files => files::map_files_key_to_msg(key).map(Msg::Files),
        ScreenMode::Settings => settings::map_settings_key_to_msg(key).map(Msg::Settings),
        ScreenMode::Help => help::map_help_key_to_msg(key).map(Msg::Help),
    }
}

pub(crate) fn map_mouse(app: &App, mouse: MouseEvent, frame_size: Rect) -> Option<Msg> {
    if app.screen_mode != ScreenMode::Welcome {
        return None;
    }

    if app.chat.is_file_finder_active() {
        return None;
    }

    if !matches!(mouse.kind, MouseEventKind::Down(_)) {
        return None;
    }

    welcome::map_welcome_mouse_to_msg(mouse, frame_size, &app.welcome.suggestions, &app.welcome.input_buffer)
        .map(Msg::Welcome)
}
