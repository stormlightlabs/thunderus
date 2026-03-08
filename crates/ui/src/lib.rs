//! Terminal UI for Thunderus

use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{Frame, Terminal};
use ratatui::{layout::Rect, style::Style};
use std::io;
use thiserror::Error;
use thndrs_core::Role;
use thndrs_mem::{LogStore, MemoryDatabase, MemoryStore, SessionManager};

pub mod chat;
pub mod components;
pub mod elements;
pub mod files;
pub mod help;
pub mod layout;
pub mod settings;
pub mod tool;

pub use chat::{ChatApp, ChatMessage, IncomingStreamEvent, StreamingState, TokenUsage, draw_chat_screen};
pub use chat::{ToolCallDisplay, ToolCallStatus};
use components::{AsciiLogo, BrandGreeting, CardItem, HintFooter, HintToken, MutedSectionTitle, TopBorderedInputRow};
use elements::{Suggestions, WelcomeContent, WelcomeMainColumn, WelcomeShell};
use files::{FileBrowserAction, FileBrowserApp, draw_file_browser_screen};
use help::{HelpApp, draw_help_screen};
use layout::{AreaSpec, ConstraintSpec};
use settings::{SettingsApp, draw_settings_screen};

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
    Settings,
    Help,
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
    pub settings: SettingsApp,
    pub help: HelpApp,
    previous_screen: Option<ScreenMode>,
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
        if key.kind != KeyEventKind::Press {
            return;
        }

        match key.code {
            KeyCode::Char(',') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_settings();
                return;
            }
            KeyCode::F(1) => {
                self.open_help();
                return;
            }
            _ => {}
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
                FileBrowserAction::ExitToChat => self.exit_to_previous_or_chat(),
            },
            ScreenMode::Settings => {
                self.settings.handle_input(key);
                if key.code == KeyCode::Esc {
                    self.exit_to_previous_or_chat();
                }
            }
            ScreenMode::Help => {
                self.help.handle_input(key);
                if key.code == KeyCode::Esc {
                    self.exit_to_previous_or_chat();
                }
            }
        }
    }

    fn open_settings(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Settings;
    }

    fn open_help(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Help;
    }

    fn exit_to_previous_or_chat(&mut self) {
        self.screen_mode = self.previous_screen.take().unwrap_or(ScreenMode::Chat);
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
            SlashCommand::History => {
                let content =
                    format_session_history().unwrap_or_else(|error| format!("Failed to load history: {error}"));
                self.push_assistant_message(content);
            }
            SlashCommand::Resume(session_id) => match load_session_chat_messages(&session_id) {
                Ok(messages) => {
                    self.chat.set_messages(messages);
                    self.chat.queue_backend_command(format!("/resume {}", session_id));
                    self.screen_mode = ScreenMode::Chat;
                }
                Err(error) => {
                    self.push_assistant_message(format!("Failed to resume session `{session_id}`: {error}"));
                }
            },
            SlashCommand::Clear => {
                self.chat.clear_chat();
                self.chat.queue_backend_command("/clear".to_string());
                self.screen_mode = ScreenMode::Chat;
            }
            SlashCommand::Tokens => {
                let content = match self.chat.last_usage {
                    Some(usage) => format!(
                        "Token usage:\n- prompt: {}\n- completion: {}\n- total: {}",
                        usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                    ),
                    None => "No token usage recorded yet in this chat.".to_string(),
                };
                self.push_assistant_message(content);
            }
            SlashCommand::Model => {
                let content = match self.chat.last_model.as_deref() {
                    Some(model) => format!("Current model: {}", model),
                    None => "Model information is not available yet.".to_string(),
                };
                self.push_assistant_message(content);
            }
            SlashCommand::DebugMemoryStats => {
                let content =
                    format_memory_stats().unwrap_or_else(|error| format!("Failed to get memory stats: {error}"));
                self.push_assistant_message(content);
            }
            SlashCommand::DebugMemoryRecall(query) => {
                let content = format_memory_recall(&query)
                    .unwrap_or_else(|error| format!("Failed to recall memory for `{query}`: {error}"));
                self.push_assistant_message(content);
            }
            SlashCommand::DebugLog(session_id) => {
                let content = format_session_logs(&session_id)
                    .unwrap_or_else(|error| format!("Failed to get logs for `{session_id}`: {error}"));
                self.push_assistant_message(content);
            }
            SlashCommand::Settings => {
                self.open_settings();
            }
            SlashCommand::HelpCmd => {
                self.open_help();
            }
            SlashCommand::Unknown(raw) => {
                self.chat.messages.push(ChatMessage::assistant(format!(
                    "Unknown command `{raw}`. Available: `/help`, `/settings`, `/debug chat`, `/debug files`, `/files`, `/history`, `/resume <id>`, `/clear`, `/tokens`, `/model`, `/debug memory stats`, `/debug memory recall <query>`, `/debug log <id>`."
                )));
                self.screen_mode = ScreenMode::Chat;
            }
            SlashCommand::Empty => {}
        }
    }

    fn push_assistant_message(&mut self, content: String) {
        self.chat.messages.push(ChatMessage::assistant(content));
        self.screen_mode = ScreenMode::Chat;
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
                let content = SUGGESTIONS[idx].to_string();
                self.chat.submit_user_message(content);
                self.screen_mode = ScreenMode::Chat;
                self.input_buffer.clear();
                self.cursor_position = 0;
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
            ScreenMode::Settings => draw_settings_screen(f, &app.settings),
            ScreenMode::Help => draw_help_screen(f, &app.help),
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
        HintToken::Text("Type "),
        HintToken::Key("/help"),
        HintToken::Text(" for help, "),
        HintToken::Key("ctrl+,"),
        HintToken::Text(" for settings, "),
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
    History,
    Resume(String),
    Clear,
    Tokens,
    Model,
    DebugMemoryStats,
    DebugMemoryRecall(String),
    DebugLog(String),
    Settings,
    HelpCmd,
    Unknown(String),
}

fn parse_slash_command(raw: &str) -> SlashCommand {
    let command = raw.trim();
    if command.is_empty() || command == "/" {
        return SlashCommand::Empty;
    }

    if let Some(session_id) = command.strip_prefix("/resume ") {
        let session_id = session_id.trim();
        if !session_id.is_empty() {
            return SlashCommand::Resume(session_id.to_string());
        }
    }

    if let Some(query) = command.strip_prefix("/debug memory recall ") {
        let query = query.trim();
        if !query.is_empty() {
            return SlashCommand::DebugMemoryRecall(query.to_string());
        }
    }

    if let Some(session_id) = command.strip_prefix("/debug log ") {
        let session_id = session_id.trim();
        if !session_id.is_empty() {
            return SlashCommand::DebugLog(session_id.to_string());
        }
    }

    match command {
        "/debug chat" => SlashCommand::DebugChat,
        "/debug files" => SlashCommand::DebugFiles,
        "/files" => SlashCommand::Files,
        "/history" => SlashCommand::History,
        "/clear" => SlashCommand::Clear,
        "/tokens" => SlashCommand::Tokens,
        "/model" => SlashCommand::Model,
        "/debug memory stats" => SlashCommand::DebugMemoryStats,
        "/settings" => SlashCommand::Settings,
        "/help" => SlashCommand::HelpCmd,
        _ => SlashCommand::Unknown(command.to_string()),
    }
}

fn workspace_memory_database() -> std::result::Result<MemoryDatabase, String> {
    let workspace_path = std::env::current_dir().map_err(|error| error.to_string())?;
    MemoryDatabase::for_workspace(&workspace_path).map_err(|error| error.to_string())
}

fn format_session_history() -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let sessions = SessionManager::new(db)
        .list_sessions(50)
        .map_err(|error| error.to_string())?;

    if sessions.is_empty() {
        return Ok("No saved sessions found.".to_string());
    }

    let mut lines = vec!["Saved sessions:".to_string()];
    for session in sessions {
        lines.push(format!(
            "- {} | {} | {} messages | updated {}",
            session.id,
            session.display_title(),
            session.message_count,
            session.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }

    Ok(lines.join("\n"))
}

fn load_session_chat_messages(session_id: &str) -> std::result::Result<Vec<ChatMessage>, String> {
    let db = workspace_memory_database()?;
    let manager = SessionManager::new(db);
    let session = manager.get_session(session_id).map_err(|error| error.to_string())?;

    if session.is_none() {
        return Err("Session not found".to_string());
    }

    let messages = manager.get_messages(session_id).map_err(|error| error.to_string())?;

    let mut chat_messages = Vec::with_capacity(messages.len());
    for message in messages {
        match message.role {
            Role::User => chat_messages.push(ChatMessage::user_at(message.content, message.created_at)),
            Role::Assistant => chat_messages.push(ChatMessage::assistant_at(message.content, message.created_at)),
            Role::Tool => chat_messages.push(ChatMessage::tool_at(
                "tool".to_string(),
                message.content,
                message.created_at,
            )),
            Role::System => {}
        }
    }

    Ok(chat_messages)
}

fn format_memory_stats() -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let store = MemoryStore::new(db);
    let stats = store.stats().map_err(|error| error.to_string())?;

    Ok(format!(
        "Memory stats:\n- total: {}\n- archived: {}\n- sessions: {}\n- size: {} bytes ({:.2} MB)\n- embedding model: {}\n- dimensions: {}",
        stats.total_memories,
        stats.archived_memories,
        stats.total_sessions,
        stats.database_size_bytes,
        stats.database_size_bytes as f64 / 1_048_576.0,
        stats.embedding_model.as_deref().unwrap_or("unknown"),
        stats.embedding_dimensions
    ))
}

fn format_memory_recall(query: &str) -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let mut store = MemoryStore::new(db);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    let memories = runtime
        .block_on(async { store.recall(query, 5, None, None, 0.3).await })
        .map_err(|error| error.to_string())?;

    if memories.is_empty() {
        return Ok(format!("No memories found for query: {}", query));
    }

    let mut lines = vec![format!("Memory recall for `{}`:", query)];
    for memory in memories {
        let similarity = memory
            .similarity
            .map(|value| format!(" (similarity: {:.2})", value))
            .unwrap_or_default();
        lines.push(format!("- [{}] {}{}", memory.kind.as_str(), memory.content, similarity));
    }

    Ok(lines.join("\n"))
}

fn format_session_logs(session_id: &str) -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let logs = LogStore::new(db)
        .get_session_logs(session_id, 200)
        .map_err(|error| error.to_string())?;

    if logs.is_empty() {
        return Ok(format!("No logs found for session `{}`.", session_id));
    }

    let mut lines = vec![format!("Logs for session `{}`:", session_id)];
    for entry in logs {
        lines.push(entry.to_string());
    }

    Ok(lines.join("\n"))
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
    fn test_question_mark_is_inserted_in_input() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('?')));

        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert_eq!(app.input_buffer, "?");
        assert_eq!(app.cursor_position, 1);
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
        let target_idx = SUGGESTIONS.len().saturating_sub(1);
        let target_suggestion = suggestion_areas(frame_area)[target_idx];

        let click_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target_suggestion.x + 1,
            row: target_suggestion.y + 1,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click_event, frame_area);
        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.selected_suggestion, Some(target_idx));
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].role, chat::MessageRole::User);
        assert_eq!(app.chat.messages[0].content, SUGGESTIONS[target_idx]);
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
        assert_eq!(parse_slash_command("/history"), SlashCommand::History);
        assert_eq!(
            parse_slash_command("/resume abc123"),
            SlashCommand::Resume("abc123".to_string())
        );
        assert_eq!(parse_slash_command("/clear"), SlashCommand::Clear);
        assert_eq!(parse_slash_command("/tokens"), SlashCommand::Tokens);
        assert_eq!(parse_slash_command("/model"), SlashCommand::Model);
        assert_eq!(
            parse_slash_command("/debug memory stats"),
            SlashCommand::DebugMemoryStats
        );
        assert_eq!(
            parse_slash_command("/debug memory recall rust sqlite"),
            SlashCommand::DebugMemoryRecall("rust sqlite".to_string())
        );
        assert_eq!(
            parse_slash_command("/debug log sess-1"),
            SlashCommand::DebugLog("sess-1".to_string())
        );
        assert_eq!(parse_slash_command("/settings"), SlashCommand::Settings);
        assert_eq!(parse_slash_command("/help"), SlashCommand::HelpCmd);
        assert_eq!(
            parse_slash_command("/unknown"),
            SlashCommand::Unknown("/unknown".to_string())
        );
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
