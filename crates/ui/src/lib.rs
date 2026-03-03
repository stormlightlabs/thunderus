//! Terminal UI for Thunderus

use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::{Backend, CrosstermBackend},
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use std::io;
use thiserror::Error;

pub mod chat;
pub mod components;
pub mod elements;
pub mod files;
pub mod layout;
pub mod tool;

pub use chat::{
    ChatApp, ChatMessage, IncomingStreamEvent, StreamingState, TokenUsage, ToolCallDisplay, ToolCallStatus,
    draw_chat_screen,
};
use components::{AsciiLogo, BrandGreeting, CardItem, HintFooter, HintToken, MutedSectionTitle, TopBorderedInputRow};
use elements::{Suggestions, WelcomeContent, WelcomeMainColumn, WelcomeShell};
use files::{FileBrowserAction, FileBrowserApp, draw_file_browser_screen};
use layout::{AreaSpec, ConstraintSpec};

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

/// The Thunderus ASCII logo
const ASCII_LOGO: &str = r#"
▗▄▄▄▖▗▖ ▗▖▗▖ ▗▖▗▖  ▗▖▗▄▄▄ ▗▄▄▄▖▗▄▄▖ ▗▖ ▗▖ ▗▄▄▖
  █  ▐▌ ▐▌▐▌ ▐▌▐▛▚▖▐▌▐▌  █▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌
  █  ▐▛▀▜▌▐▌ ▐▌▐▌ ▝▜▌▐▌  █▐▛▀▀▘▐▛▀▚▖▐▌ ▐▌ ▝▀▚▖
  █  ▐▌ ▐▌▝▚▄▞▘▐▌  ▐▌▐▙▄▄▀▐▙▄▄▖▐▌ ▐▌▝▚▄▞▘▗▄▄▞▘
"#;

fn logo_text() -> &'static str {
    ASCII_LOGO.trim_matches('\n')
}

fn logo_dimensions() -> (u16, u16) {
    let mut max_width = 0u16;
    let mut height = 0u16;

    for line in logo_text().lines() {
        height = height.saturating_add(1);
        max_width = max_width.max(line.chars().count() as u16);
    }

    (max_width, height)
}

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

/// Suggestion items shown on welcome screen
pub const SUGGESTIONS: [&str; 2] = [
    // NOTE: This is for debugging
    "What is your name?",
    // TODO: inject meta/INIT.txt
    // TODO: if AGENTS.md exists, don't show this
    "Initialize a new project by creating a new AGENTS.md",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenMode {
    Welcome,
    Chat,
    Files,
}

/// The TUI application state
pub struct App {
    pub running: bool,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub selected_suggestion: Option<usize>,
    pub screen_mode: ScreenMode,
    pub chat: ChatApp,
    pub file_browser: FileBrowserApp,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            input_buffer: String::new(),
            cursor_position: 0,
            selected_suggestion: None,
            screen_mode: ScreenMode::Welcome,
            chat: ChatApp::new(),
            file_browser: FileBrowserApp::default(),
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        match self.screen_mode {
            ScreenMode::Welcome => self.handle_welcome_input(key),
            ScreenMode::Chat => {
                self.chat.handle_input(key);
                if let Some(command) = self.chat.take_pending_command() {
                    self.execute_slash_command(&command);
                }
                if !self.chat.running {
                    self.running = false;
                }
            }
            ScreenMode::Files => match self.file_browser.handle_input(key) {
                FileBrowserAction::None => {}
                FileBrowserAction::Quit => self.running = false,
                FileBrowserAction::ExitToChat => self.screen_mode = ScreenMode::Chat,
            },
        }
    }

    fn handle_welcome_input(&mut self, key: KeyEvent) {
        if self.chat.is_file_finder_active() {
            self.chat.handle_input(key);
            if !self.chat.running {
                self.running = false;
            }
            return;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('@') => self.chat.activate_file_finder(),
            KeyCode::Char(c) => {
                self.selected_suggestion = None;
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                self.selected_suggestion = None;
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                self.selected_suggestion = None;
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                self.selected_suggestion = None;
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() || self.selected_suggestion.is_some() {
                    let content = if self.input_buffer.is_empty() {
                        let idx = self.selected_suggestion.unwrap_or(0);
                        SUGGESTIONS[idx].to_string()
                    } else {
                        self.input_buffer.clone()
                    };

                    if content.starts_with('/') {
                        self.execute_slash_command(&content);
                    } else {
                        self.chat.submit_user_message(content);
                        self.screen_mode = ScreenMode::Chat;
                    }

                    self.input_buffer.clear();
                    self.cursor_position = 0;
                }
            }
            KeyCode::Up => {
                self.selected_suggestion = match self.selected_suggestion {
                    None => Some(0),
                    Some(0) => Some(SUGGESTIONS.len() - 1),
                    Some(idx) => Some(idx - 1),
                }
            }
            KeyCode::Down => {
                self.selected_suggestion = match self.selected_suggestion {
                    None => Some(0),
                    Some(idx) if idx >= SUGGESTIONS.len() - 1 => Some(0),
                    Some(idx) => Some(idx + 1),
                };
            }
            _ => {}
        }
    }

    fn execute_slash_command(&mut self, command: &str) {
        match parse_slash_command(command) {
            SlashCommand::DebugChat => {
                self.chat.load_debug_chat();
                self.screen_mode = ScreenMode::Chat;
            }
            SlashCommand::DebugFiles => {
                self.file_browser.load_debug_fixture();
                self.screen_mode = ScreenMode::Files;
            }
            SlashCommand::Files => {
                if let Err(error) = self.file_browser.reload_workspace() {
                    self.chat.messages.push(ChatMessage::assistant(format!(
                        "Unable to load workspace files: {error}"
                    )));
                    self.screen_mode = ScreenMode::Chat;
                } else {
                    self.screen_mode = ScreenMode::Files;
                }
            }
            SlashCommand::Unknown(raw) => {
                self.chat.messages.push(ChatMessage::assistant(format!(
                    "Unknown command `{raw}`. Available: `/debug chat`, `/debug files`, `/files`."
                )));
                self.screen_mode = ScreenMode::Chat;
            }
            SlashCommand::Empty => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, frame_size: Rect) {
        if self.screen_mode != ScreenMode::Welcome {
            return;
        }

        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }

        for (idx, area) in suggestion_areas(frame_size).iter().enumerate() {
            if point_in_rect(mouse.column, mouse.row, *area) {
                self.selected_suggestion = Some(idx);
                break;
            }
        }
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

        terminal.draw(|f| match app.screen_mode {
            ScreenMode::Welcome => draw_welcome_screen(f, app),
            ScreenMode::Chat => draw_chat_screen(f, &app.chat),
            ScreenMode::Files => draw_file_browser_screen(f, &app.file_browser),
        })?;

        if !app.running {
            break Ok(());
        }

        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            match crossterm::event::read()? {
                Event::Key(key) => app.handle_input(key),
                Event::Mouse(mouse) => {
                    let size = terminal.size()?;
                    app.handle_mouse(mouse, Rect::new(0, 0, size.width, size.height));
                }
                _ => {}
            }
        }
    }
}

/// Draw the welcome screen
pub fn draw_welcome_screen(frame: &mut Frame, app: &App) {
    let size = frame.area();

    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let main_layout = WelcomeShell.split(size);
    if main_layout.len() < 3 {
        return;
    }

    draw_main_content(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    draw_input_area(frame, main_layout[2], app);

    if app.chat.is_file_finder_active() {
        chat::draw_file_finder_overlay(frame, size, &app.chat);
    }
}

fn draw_main_content(frame: &mut Frame, area: Rect, app: &App) {
    let content_area = WelcomeMainColumn.area(area);
    let content_layout = WelcomeContent.split(content_area);
    if content_layout.len() < 6 {
        return;
    }

    draw_logo(frame, content_layout[1]);
    BrandGreeting.render(frame, content_layout[3], "What can I help you build?");
    draw_suggestions(frame, content_layout[5], app);
}

fn draw_logo(frame: &mut Frame, area: Rect) {
    let (logo_width, logo_height) = logo_dimensions();
    let render_width = logo_width.min(area.width);
    let render_height = logo_height.min(area.height);
    let render_x = area.x + (area.width.saturating_sub(render_width)) / 2;
    let render_y = area.y + (area.height.saturating_sub(render_height)) / 2;
    let render_area = Rect::new(render_x, render_y, render_width, render_height);

    AsciiLogo.render(frame, render_area, logo_text());
}

/// Draws suggestions as bordered card items matching the design's .card-item
fn draw_suggestions(frame: &mut Frame, area: Rect, app: &App) {
    let suggestions_layout = Suggestions.split(area);
    if suggestions_layout.is_empty() {
        return;
    }

    MutedSectionTitle.render(frame, suggestions_layout[0], "Try asking");

    for (idx, suggestion) in SUGGESTIONS.iter().enumerate() {
        let is_selected = app.selected_suggestion == Some(idx);
        if let Some(slot_area) = suggestions_layout.get(1 + idx) {
            if slot_area.height >= 3 {
                CardItem.render(frame, *slot_area, suggestion, is_selected);
            } else {
                draw_compact_suggestion(frame, *slot_area, suggestion, is_selected);
            }
        }
    }
}

fn draw_compact_suggestion(frame: &mut Frame, area: Rect, label: &str, is_selected: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let prefix_style = if is_selected {
        Style::default().fg(colors::ACCENT_CYAN)
    } else {
        Style::default().fg(colors::TEXT_MUTED)
    };
    let text_style = if is_selected {
        Style::default().fg(colors::TEXT_PRIMARY)
    } else {
        Style::default().fg(colors::TEXT_SECONDARY)
    };

    let line = Line::from(vec![Span::styled("> ", prefix_style), Span::styled(label, text_style)]);
    let paragraph = Paragraph::new(line).style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(paragraph, area);
}

fn suggestion_areas(frame_area: Rect) -> Vec<Rect> {
    let shell_layout = WelcomeShell.split(frame_area);
    let Some(main_content_area) = shell_layout.first().copied() else {
        return Vec::new();
    };

    let centered = WelcomeMainColumn.area(main_content_area);
    let content_layout = WelcomeContent.split(centered);
    let Some(suggestions_area) = content_layout.get(5).copied() else {
        return Vec::new();
    };

    Suggestions.card_areas(suggestions_area)
}

fn point_in_rect(x: u16, y: u16, area: Rect) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

fn draw_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("?"),
        HintToken::Text(" for help, "),
        HintToken::Key("ctrl+n"),
        HintToken::Text(" for new chat, "),
        HintToken::Key("@"),
        HintToken::Text(" to pin files, "),
        HintToken::Key("ctrl+d"),
        HintToken::Text(" to quit"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_input_area(frame: &mut Frame, area: Rect, app: &App) {
    TopBorderedInputRow.render(frame, area, &app.input_buffer, true);
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlashCommand {
    Empty,
    DebugChat,
    DebugFiles,
    Files,
    Unknown(String),
}

fn parse_slash_command(raw: &str) -> SlashCommand {
    let command = raw.trim();
    if command.is_empty() || command == "/" {
        return SlashCommand::Empty;
    }

    match command {
        "/debug chat" => SlashCommand::DebugChat,
        "/debug files" => SlashCommand::DebugFiles,
        "/files" => SlashCommand::Files,
        _ => SlashCommand::Unknown(command.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

    #[test]
    fn test_app_default() {
        let app = App::new();
        assert!(app.running);
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.cursor_position, 0);
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
    }

    #[test]
    fn test_app_input_handling() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('h')));
        assert_eq!(app.input_buffer, "h");
        assert_eq!(app.cursor_position, 1);

        app.handle_input(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.input_buffer, "hi");
        assert_eq!(app.cursor_position, 2);

        app.handle_input(KeyEvent::from(KeyCode::Backspace));
        assert_eq!(app.input_buffer, "h");
        assert_eq!(app.cursor_position, 1);

        let quit_event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        app.handle_input(quit_event);
        assert!(!app.running);
    }

    #[test]
    fn test_mouse_click_selects_suggestion() {
        let mut app = App::new();
        let frame_area = Rect::new(0, 0, 120, 40);
        let target_idx = SUGGESTIONS.len().saturating_sub(1);
        let target_suggestion = suggestion_areas(frame_area)[target_idx];

        let click_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target_suggestion.x + 1,
            row: target_suggestion.y + 1,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click_event, frame_area);
        assert_eq!(app.selected_suggestion, Some(target_idx));
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
    fn test_enter_submits_selected_suggestion_without_input() {
        let mut app = App::new();
        app.selected_suggestion = Some(1);

        app.handle_input(KeyEvent::from(KeyCode::Enter));

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].content, SUGGESTIONS[1]);
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
    fn test_parse_slash_command() {
        assert_eq!(parse_slash_command("/debug chat"), SlashCommand::DebugChat);
        assert_eq!(parse_slash_command("/debug files"), SlashCommand::DebugFiles);
        assert_eq!(parse_slash_command("/files"), SlashCommand::Files);
        assert_eq!(
            parse_slash_command("/unknown"),
            SlashCommand::Unknown("/unknown".to_string())
        );
    }
}
