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
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::Block,
};
use std::io;
use thiserror::Error;

pub mod components;

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
pub const SUGGESTIONS: [&str; 5] = [
    "Refactor this function to use async/await",
    "Find all TODO comments in the codebase",
    "Write tests for the auth module",
    "Explain how the database connection works",
    "Fix the bug in src/utils.js line 42",
];

/// The TUI application state
pub struct App {
    pub running: bool,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub selected_suggestion: Option<usize>,
}

impl Default for App {
    fn default() -> Self {
        Self { running: true, input_buffer: String::new(), cursor_position: 0, selected_suggestion: None }
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
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => self.running = false,
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Enter => todo!("Submit the input"),
            KeyCode::Up => {
                self.selected_suggestion = match self.selected_suggestion {
                    None => Some(0),
                    Some(0) => Some(SUGGESTIONS.len() - 1),
                    Some(idx) => Some(idx - 1),
                };
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

    pub fn handle_mouse(&mut self, mouse: MouseEvent, frame_size: Rect) {
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
    let result = run_app(&mut terminal, &mut app);

    restore_terminal(&mut terminal)?;

    result
}

fn run_app(terminal: &mut Terminal<impl Backend>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw_welcome_screen(f, app))?;

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

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(size);

    draw_main_content(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    components::draw_input_separator(frame, main_layout[2]);
    draw_input_area(frame, main_layout[3], app);
}

fn draw_main_content(frame: &mut Frame, area: Rect, app: &App) {
    let (_, logo_height) = logo_dimensions();
    let content_width = 60u16.min(area.width);
    let content_x = area.x + (area.width.saturating_sub(content_width)) / 2;

    let content_area = Rect::new(content_x, area.y, content_width, area.height);

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_height),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Min(1),
        ])
        .split(content_area);

    draw_logo(frame, content_layout[1]);
    components::draw_brand_greeting(frame, content_layout[3], "What can I help you build?");
    draw_suggestions(frame, content_layout[5], app);
}

fn draw_logo(frame: &mut Frame, area: Rect) {
    let (logo_width, logo_height) = logo_dimensions();
    let render_width = logo_width.min(area.width);
    let render_height = logo_height.min(area.height);
    let render_x = area.x + (area.width.saturating_sub(render_width)) / 2;
    let render_y = area.y + (area.height.saturating_sub(render_height)) / 2;
    let render_area = Rect::new(render_x, render_y, render_width, render_height);

    components::draw_ascii_logo(frame, render_area, logo_text());
}

/// Draws suggestions as bordered card items matching the design's .card-item
fn draw_suggestions(frame: &mut Frame, area: Rect, app: &App) {
    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)];
    for _ in 0..SUGGESTIONS.len() {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Min(0));

    let suggestions_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    components::draw_section_title_muted(frame, suggestions_layout[0], "Try asking");

    for (idx, suggestion) in SUGGESTIONS.iter().enumerate() {
        let is_selected = app.selected_suggestion == Some(idx);
        let slot_area = suggestions_layout[1 + idx];
        components::draw_card_item(frame, slot_area, suggestion, is_selected);
    }
}

fn suggestion_areas(frame_area: Rect) -> Vec<Rect> {
    let (_, logo_height) = logo_dimensions();
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(frame_area);

    let main_content_area = main_layout[0];
    let content_width = 60u16.min(main_content_area.width);
    let content_x = main_content_area.x + (main_content_area.width.saturating_sub(content_width)) / 2;
    let content_area = Rect::new(content_x, main_content_area.y, content_width, main_content_area.height);

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(logo_height),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Min(1),
        ])
        .split(content_area);

    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)];
    for _ in 0..SUGGESTIONS.len() {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Min(0));

    let suggestions_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(content_layout[5]);

    SUGGESTIONS
        .iter()
        .enumerate()
        .map(|(idx, _)| suggestions_layout[1 + idx])
        .collect()
}

fn point_in_rect(x: u16, y: u16, area: Rect) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

fn draw_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        components::HintToken::Text("Press "),
        components::HintToken::Key("?"),
        components::HintToken::Text(" for help, "),
        components::HintToken::Key("ctrl+n"),
        components::HintToken::Text(" for new chat, "),
        components::HintToken::Key("@file"),
        components::HintToken::Text(" to reference files"),
        components::HintToken::Key("ctrl+d"),
        components::HintToken::Text(" to quit"),
    ];
    components::draw_hint_line(frame, area, &tokens);
}

fn draw_input_area(frame: &mut Frame, area: Rect, app: &App) {
    let input_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    components::draw_input_line(frame, input_layout[1], &app.input_buffer, true);
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
        let third_suggestion = suggestion_areas(frame_area)[2];

        let click_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: third_suggestion.x + 1,
            row: third_suggestion.y + 1,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click_event, frame_area);
        assert_eq!(app.selected_suggestion, Some(2));
    }
}
