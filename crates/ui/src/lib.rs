//! Terminal UI for Thunderus

mod commands;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, KeyEvent, MouseEvent};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Terminal, layout::Rect};
use std::io;
use thiserror::Error;

pub mod chat;
pub mod components;
pub mod elements;
pub mod event;
pub mod files;
pub mod finder;
pub mod help;
pub mod layout;
pub mod screen;
pub mod scroll;
pub mod settings;
pub mod welcome;

pub use chat::{ChatApp, ChatMessage, IncomingStreamEvent, StreamingState, TokenUsage, draw_chat_screen};
pub use chat::{ToolCallDisplay, ToolCallStatus};
use files::FileBrowserApp;
use help::HelpApp;
use screen::Screen;
use settings::SettingsApp;
use welcome::WelcomeApp;

type Submitter<'a> = dyn FnMut(String) -> std::result::Result<(), String> + 'a;
type Poller<'a> = dyn FnMut() -> Option<IncomingStreamEvent> + 'a;

/// UI Errors
#[derive(Error, Debug)]
pub enum UiError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

pub type Result<T> = std::result::Result<T, UiError>;

/// Color scheme based on designs/README.md Oxocarbon Dark theme
pub mod colors {
    use ratatui::style::Color;

    pub const ACCENT_CYAN: Color = Color::Rgb(0x33, 0xb1, 0xff);
    pub const ACCENT_PINK: Color = Color::Rgb(0xff, 0x7e, 0xb6);
    pub const ACCENT_PURPLE: Color = Color::Rgb(0xbe, 0x95, 0xff);
    pub const ACCENT_GREEN: Color = Color::Rgb(0x42, 0xbe, 0x65);
    pub const ACCENT_YELLOW: Color = Color::Rgb(0xf1, 0xc2, 0x1b);
    pub const ACCENT_RED: Color = Color::Rgb(0xfa, 0x4d, 0x56);
    pub const BG_PRIMARY: Color = Color::Rgb(0x16, 0x16, 0x16);
    pub const BG_SECONDARY: Color = Color::Rgb(0x1c, 0x1c, 0x1c);
    pub const BG_TERTIARY: Color = Color::Rgb(0x26, 0x26, 0x26);
    pub const BG_TERMINAL: Color = Color::Rgb(0x0c, 0x0c, 0x0c);
    pub const TEXT_PRIMARY: Color = Color::Rgb(0xf4, 0xf4, 0xf4);
    pub const TEXT_SECONDARY: Color = Color::Rgb(0xc6, 0xc6, 0xc6);
    pub const TEXT_MUTED: Color = Color::Rgb(0x8d, 0x8d, 0x8d);
    pub const BORDER_COLOR: Color = Color::Rgb(0x39, 0x39, 0x39);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Welcome,
    Chat,
    Files,
    Settings,
    Help,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Quit,
    OpenSettings,
    StartNewChat,
    OpenFiles,
    CloseActiveChat,
    OpenHelp,
    Welcome(welcome::WelcomeMsg),
    Chat(chat::ChatMsg),
    Files(files::FilesMsg),
    Settings(settings::SettingsMsg),
    Help(help::HelpMsg),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cmd {
    #[default]
    None,
}

/// The TUI application state
pub struct App {
    pub running: bool,
    pub screen_mode: ScreenMode,
    pub welcome: WelcomeApp,
    pub chat: ChatApp,
    pub file_browser: FileBrowserApp,
    pub settings: SettingsApp,
    pub help: HelpApp,
    previous_screen: Option<ScreenMode>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            screen_mode: ScreenMode::Welcome,
            welcome: WelcomeApp::new(),
            chat: ChatApp::new(),
            file_browser: FileBrowserApp::default(),
            settings: SettingsApp::new(),
            help: HelpApp::new(),
            previous_screen: None,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        if let Some(msg) = event::map_key(self, key) {
            let _ = self.update(msg);
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, frame_size: Rect) {
        if let Some(msg) = event::map_mouse(self, mouse, frame_size) {
            let _ = self.update(msg);
        }
    }

    pub fn update(&mut self, msg: Msg) -> Cmd {
        match msg {
            Msg::Quit => {
                self.running = false;
            }
            Msg::OpenSettings => {
                self.open_settings();
            }
            Msg::StartNewChat => {
                self.start_new_chat();
            }
            Msg::OpenFiles => {
                commands::execute_slash_command(self, "/files");
            }
            Msg::CloseActiveChat => {
                self.close_active_chat();
            }
            Msg::OpenHelp => {
                self.open_help();
            }
            Msg::Welcome(sub_msg) => {
                let action = welcome::update(&mut self.welcome, sub_msg);
                self.apply_screen_action(action);
                self.process_pending_actions();
            }
            Msg::Chat(sub_msg) => {
                let action = chat::update_chat(&mut self.chat, sub_msg);
                self.apply_screen_action(action);
                if self.screen_mode != ScreenMode::Welcome || !self.chat.is_file_finder_active() {
                    self.process_pending_actions();
                }
            }
            Msg::Files(sub_msg) => {
                let action = files::update(&mut self.file_browser, sub_msg);
                self.apply_screen_action(action);
                self.process_pending_actions();
            }
            Msg::Settings(sub_msg) => {
                let action = settings::update(&mut self.settings, sub_msg);
                self.apply_screen_action(action);
                self.process_pending_actions();
            }
            Msg::Help(sub_msg) => {
                let action = help::update(&mut self.help, sub_msg);
                self.apply_screen_action(action);
                self.process_pending_actions();
            }
        }

        Cmd::None
    }

    pub(crate) fn open_settings(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Settings;
    }

    pub(crate) fn open_help(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Help;
    }

    fn exit_to_previous_or_chat(&mut self) {
        self.screen_mode = self.previous_screen.take().unwrap_or(ScreenMode::Chat);
    }

    fn start_new_chat(&mut self) {
        self.chat.clear_chat();
        self.chat.deactivate_file_finder();
        self.screen_mode = ScreenMode::Chat;
        self.previous_screen = None;
    }

    fn close_active_chat(&mut self) {
        self.chat.deactivate_file_finder();
        if matches!(
            self.screen_mode,
            ScreenMode::Chat | ScreenMode::Files | ScreenMode::Welcome
        ) {
            self.screen_mode = ScreenMode::Welcome;
            self.previous_screen = None;
        } else {
            self.exit_to_previous_or_chat();
        }
    }

    fn apply_screen_action(&mut self, action: screen::ScreenAction) {
        match action {
            screen::ScreenAction::None => {}
            screen::ScreenAction::Quit => self.running = false,
            screen::ScreenAction::SwitchTo(mode) => {
                if matches!(mode, ScreenMode::Settings | ScreenMode::Help) {
                    self.previous_screen = Some(self.screen_mode);
                }
                self.screen_mode = mode;
            }
            screen::ScreenAction::ReturnToPrevious => self.exit_to_previous_or_chat(),
        }
    }

    fn process_pending_actions(&mut self) {
        if self.welcome.take_activate_file_finder() {
            self.chat.activate_file_finder();
        }

        if let Some(content) = self.welcome.take_pending_submission() {
            self.chat.submit_user_message(content);
            self.screen_mode = ScreenMode::Chat;
        }

        if let Some(command) = self.welcome.take_pending_command() {
            commands::execute_slash_command(self, &command);
        }

        if let Some(command) = self.chat.take_pending_command() {
            commands::execute_slash_command(self, &command);
        }
    }

    pub(crate) fn push_assistant_message(&mut self, content: String) {
        self.chat.messages.push(ChatMessage::assistant(content));
        self.screen_mode = ScreenMode::Chat;
    }
}

/// Setup terminal for TUI
pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore terminal to normal state
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Run the welcome screen TUI
pub fn run_welcome_app() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    let result = run_app(&mut terminal, &mut app, None, None);

    restore_terminal(&mut terminal)?;

    result
}

/// Run the welcome screen TUI with streaming callbacks for conversation handling.
pub fn run_welcome_app_with_streaming<S, P>(mut submit_message: S, mut poll_event: P) -> Result<()>
where
    S: FnMut(String) -> std::result::Result<(), String>,
    P: FnMut() -> Option<IncomingStreamEvent>,
{
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    let submit_ref: &mut Submitter<'_> = &mut submit_message;
    let poll_ref: &mut Poller<'_> = &mut poll_event;
    let result = run_app(&mut terminal, &mut app, Some(submit_ref), Some(poll_ref));

    restore_terminal(&mut terminal)?;

    result
}

/// Run the chat screen TUI
pub fn run_chat_app() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    app.screen_mode = ScreenMode::Chat;
    let result = run_app(&mut terminal, &mut app, None, None);

    restore_terminal(&mut terminal)?;

    result
}

fn run_app(
    terminal: &mut Terminal<impl Backend>, app: &mut App, mut submitter: Option<&mut Submitter<'_>>,
    mut poller: Option<&mut Poller<'_>>,
) -> Result<()> {
    loop {
        if let Some(submitter) = submitter.as_deref_mut()
            && let Some(pending_message) = app.chat.take_pending_submission()
            && let Err(error) = submitter(pending_message)
        {
            app.chat.handle_stream_event(IncomingStreamEvent::Error(error));
        }

        if submitter.is_none() && app.chat.take_pending_submission().is_some() {
            app.chat.handle_stream_event(IncomingStreamEvent::Error(
                "No response backend configured for this UI mode.".to_string(),
            ));
        }

        if let Some(poller) = poller.as_deref_mut() {
            while let Some(event) = poller() {
                app.chat.handle_stream_event(event);
            }
        }

        terminal.draw(|frame| {
            match app.screen_mode {
                ScreenMode::Welcome => {
                    welcome::view(frame, &app.welcome);
                    if app.chat.is_file_finder_active() {
                        app.chat.draw_file_finder_overlay(frame, frame.area());
                    }
                }
                ScreenMode::Chat => {
                    app.chat.sync_scroll_state_for_frame(frame.area());
                    Screen::draw(&app.chat, frame);
                }
                ScreenMode::Files => files::view(frame, &app.file_browser),
                ScreenMode::Settings => settings::view(frame, &app.settings),
                ScreenMode::Help => help::view(frame, &app.help),
            }

            let frame_area = frame.area();
            if frame_area.height > 0 {
                draw_status_bar(frame, Rect::new(frame_area.x, frame_area.y, frame_area.width, 1), app);
            }
        })?;

        if !app.running {
            break Ok(());
        }

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            let frame_size = terminal.size()?;
            let event = crossterm::event::read()?;
            if let Some(msg) = event::map_event(app, event, Rect::new(0, 0, frame_size.width, frame_size.height)) {
                let _ = app.update(msg);
            }
        }
    }
}

fn draw_status_bar(frame: &mut ratatui::Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let left = format!(" {} ", screen_label(app.screen_mode));
    let right = if app.screen_mode == ScreenMode::Chat {
        app.chat
            .last_model
            .as_deref()
            .map(|model| format!(" model: {model} "))
            .unwrap_or_else(|| format!(" {} ", components::app_version_string()))
    } else {
        format!(" {} ", components::app_version_string())
    };

    let total_width = area.width as usize;
    let used_width = left.chars().count() + right.chars().count();
    let spacer = " ".repeat(total_width.saturating_sub(used_width).max(1));

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::raw(spacer),
        Span::styled(right, Style::default().fg(colors::TEXT_MUTED)),
    ]);

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(colors::BG_SECONDARY)),
        area,
    );
}

fn screen_label(mode: ScreenMode) -> &'static str {
    match mode {
        ScreenMode::Welcome => "WELCOME",
        ScreenMode::Chat => "CHAT",
        ScreenMode::Files => "FILES",
        ScreenMode::Settings => "SETTINGS",
        ScreenMode::Help => "HELP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn test_app_default() {
        let app = App::new();
        assert!(app.running);
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert!(!app.welcome.suggestions.is_empty());
    }

    #[test]
    fn test_question_mark_is_inserted_in_input() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('?')));

        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert_eq!(app.welcome.input_buffer, "?");
        assert_eq!(app.welcome.cursor_position, 1);
    }

    #[test]
    fn test_f1_opens_help() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::from(KeyCode::F(1)));

        assert_eq!(app.screen_mode, ScreenMode::Help);
        assert_eq!(app.previous_screen, Some(ScreenMode::Chat));
    }

    #[test]
    fn test_mouse_click_submits_suggestion() {
        let mut app = App::new();
        let frame_area = Rect::new(0, 0, 120, 40);
        let target_idx = app.welcome.suggestions.len().saturating_sub(1);
        let target_suggestion =
            welcome::suggestion_areas(frame_area, &app.welcome.suggestions, &app.welcome.input_buffer)[target_idx];
        let click_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target_suggestion.x + 1,
            row: target_suggestion.y + 1,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click_event, frame_area);

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].role, chat::MessageRole::User);
    }

    #[test]
    fn test_enter_transitions_to_chat() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('h')));
        app.handle_input(KeyEvent::from(KeyCode::Char('i')));
        app.handle_input(KeyEvent::from(KeyCode::Enter));

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].role, chat::MessageRole::User);
        assert_eq!(app.chat.messages[0].content, "hi");
    }

    #[test]
    fn test_at_from_welcome_opens_chat_file_picker() {
        let mut app = App::new();
        app.handle_input(KeyEvent::from(KeyCode::Char('@')));
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert!(app.chat.is_file_finder_active());
    }

    #[test]
    fn test_ctrl_d_quits_in_chat_mode() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        let quit_event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        app.handle_input(quit_event);

        assert!(!app.running);
    }

    #[test]
    fn test_ctrl_n_starts_new_chat() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Welcome;
        app.chat.messages.push(ChatMessage::user("old".to_string()));
        app.chat.input_buffer = "draft".to_string();

        app.handle_input(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert!(app.chat.messages.is_empty());
        assert!(app.chat.input_buffer.is_empty());
    }

    #[test]
    fn test_ctrl_o_opens_file_browser() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Files);
    }

    #[test]
    fn test_ctrl_w_returns_to_welcome() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Welcome);
    }

    #[test]
    fn test_screen_modes() {
        assert_ne!(ScreenMode::Welcome, ScreenMode::Chat);
        assert_ne!(ScreenMode::Settings, ScreenMode::Help);
        assert_eq!(ScreenMode::Files, ScreenMode::Files);
    }

    #[test]
    fn test_app_has_settings_and_help() {
        let app = App::new();
        assert!(!app.settings.has_changes);
    }

    #[test]
    fn test_open_settings_and_help() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.open_settings();
        assert_eq!(app.screen_mode, ScreenMode::Settings);
        assert_eq!(app.previous_screen, Some(ScreenMode::Chat));

        app.exit_to_previous_or_chat();
        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.previous_screen, None);

        app.open_help();
        assert_eq!(app.screen_mode, ScreenMode::Help);

        app.exit_to_previous_or_chat();
        assert_eq!(app.screen_mode, ScreenMode::Chat);
    }
}
