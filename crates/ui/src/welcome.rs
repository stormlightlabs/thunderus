use super::ScreenMode;
use super::colors;
use super::components::{AsciiLogo, BrandGreeting, CardItem, MutedSectionTitle, TopBorderedInputRow};
use super::components::{HintFooter, HintToken};
use super::elements::{Suggestions, WelcomeContent, WelcomeMainColumn, WelcomeShell};
use super::layout::{AreaSpec, ConstraintSpec, DynamicConstraintSpec};
use super::screen::{Screen, ScreenAction};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use std::path::Path;

/// The Thunderus ASCII logo
const ASCII_LOGO: &str = r#"
▗▄▄▄▖▗▖ ▗▖▗▖ ▗▖▗▖  ▗▖▗▄▄▄ ▗▄▄▄▖▗▄▄▖ ▗▖ ▗▖ ▗▄▄▖
  █  ▐▌ ▐▌▐▌ ▐▌▐▛▚▖▐▌▐▌  █▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌
  █  ▐▛▀▜▌▐▌ ▐▌▐▌ ▝▜▌▐▌  █▐▛▀▀▘▐▛▀▚▖▐▌ ▐▌ ▝▀▚▖
  █  ▐▌ ▐▌▝▚▄▞▘▐▌  ▐▌▐▙▄▄▀▐▙▄▄▖▐▌ ▐▌▝▚▄▞▘▗▄▄▞▘
"#;

const INIT_PROMPT: &str = include_str!("../../../meta/INIT.txt");
const DEFAULT_WELCOME_SUGGESTION: &str = "What is your name?";
const README_IMPROVEMENT_SUGGESTION: &str = "Read README.md and suggest improvements.";

#[derive(Debug, Clone)]
pub struct WelcomeApp {
    pub input_buffer: String,
    pub cursor_position: usize,
    pub selected_suggestion: Option<usize>,
    pub suggestions: Vec<String>,
    pending_submission: Option<String>,
    pending_command: Option<String>,
    should_activate_file_finder: bool,
}

impl Default for WelcomeApp {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_default();
        Self {
            input_buffer: String::new(),
            cursor_position: 0,
            selected_suggestion: None,
            suggestions: build_welcome_suggestions(&workspace_root),
            pending_submission: None,
            pending_command: None,
            should_activate_file_finder: false,
        }
    }
}

impl WelcomeApp {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_pending_submission(&mut self) -> Option<String> {
        self.pending_submission.take()
    }

    pub fn take_pending_command(&mut self) -> Option<String> {
        self.pending_command.take()
    }

    pub fn take_activate_file_finder(&mut self) -> bool {
        let should_activate = self.should_activate_file_finder;
        self.should_activate_file_finder = false;
        should_activate
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, frame_size: Rect) -> ScreenAction {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return ScreenAction::None;
        }

        for (idx, area) in suggestion_areas(frame_size, &self.suggestions).iter().enumerate() {
            if point_in_rect(mouse.column, mouse.row, *area) {
                self.selected_suggestion = Some(idx);
                let Some(content) = self.suggestions.get(idx).cloned() else {
                    continue;
                };
                self.pending_submission = Some(content);
                self.input_buffer.clear();
                self.cursor_position = 0;
                return ScreenAction::SwitchTo(ScreenMode::Chat);
            }
        }

        ScreenAction::None
    }
}

impl Screen for WelcomeApp {
    fn handle_input(&mut self, key: KeyEvent) -> ScreenAction {
        if key.kind != KeyEventKind::Press {
            return ScreenAction::None;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => ScreenAction::Quit,
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => ScreenAction::Quit,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => ScreenAction::Quit,
            KeyCode::Char('@') => {
                self.should_activate_file_finder = true;
                ScreenAction::None
            }
            KeyCode::Char(ch) => {
                self.selected_suggestion = None;
                self.input_buffer.insert(self.cursor_position, ch);
                self.cursor_position += 1;
                ScreenAction::None
            }
            KeyCode::Backspace => {
                self.selected_suggestion = None;
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input_buffer.remove(self.cursor_position);
                }
                ScreenAction::None
            }
            KeyCode::Left => {
                self.selected_suggestion = None;
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
                ScreenAction::None
            }
            KeyCode::Right => {
                self.selected_suggestion = None;
                if self.cursor_position < self.input_buffer.len() {
                    self.cursor_position += 1;
                }
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.input_buffer.is_empty() && self.selected_suggestion.is_none() {
                    return ScreenAction::None;
                }

                let content = if self.input_buffer.is_empty() {
                    let idx = self.selected_suggestion.unwrap_or(0);
                    self.suggestions
                        .get(idx)
                        .cloned()
                        .or_else(|| self.suggestions.first().cloned())
                        .unwrap_or_default()
                } else {
                    self.input_buffer.clone()
                };

                self.input_buffer.clear();
                self.cursor_position = 0;

                if content.starts_with('/') {
                    self.pending_command = Some(content);
                    ScreenAction::None
                } else {
                    self.pending_submission = Some(content);
                    ScreenAction::SwitchTo(ScreenMode::Chat)
                }
            }
            KeyCode::Up => {
                if self.suggestions.is_empty() {
                    self.selected_suggestion = None;
                    return ScreenAction::None;
                }

                self.selected_suggestion = match self.selected_suggestion {
                    None => Some(0),
                    Some(0) => Some(self.suggestions.len() - 1),
                    Some(idx) => Some(idx - 1),
                };

                ScreenAction::None
            }
            KeyCode::Down => {
                if self.suggestions.is_empty() {
                    self.selected_suggestion = None;
                    return ScreenAction::None;
                }

                self.selected_suggestion = match self.selected_suggestion {
                    None => Some(0),
                    Some(idx) if idx >= self.suggestions.len() - 1 => Some(0),
                    Some(idx) => Some(idx + 1),
                };

                ScreenAction::None
            }
            _ => ScreenAction::None,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        draw_welcome_screen(frame, self);
    }
}

pub fn draw_welcome_screen(frame: &mut Frame, app: &WelcomeApp) {
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
}

fn draw_main_content(frame: &mut Frame, area: Rect, app: &WelcomeApp) {
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

fn draw_suggestions(frame: &mut Frame, area: Rect, app: &WelcomeApp) {
    if app.suggestions.is_empty() {
        return;
    }

    let suggestions_layout = Suggestions.split(area, &app.suggestions);
    if suggestions_layout.is_empty() {
        return;
    }

    MutedSectionTitle.render(frame, suggestions_layout[0], "Try asking");

    for (idx, suggestion) in app.suggestions.iter().enumerate() {
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

fn draw_input_area(frame: &mut Frame, area: Rect, app: &WelcomeApp) {
    TopBorderedInputRow.render(frame, area, &app.input_buffer, true);
}

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

fn build_welcome_suggestions(workspace_root: &Path) -> Vec<String> {
    let mut suggestions = vec![DEFAULT_WELCOME_SUGGESTION.to_string()];

    if has_agents_md(workspace_root) {
        suggestions.push(README_IMPROVEMENT_SUGGESTION.to_string());
    } else {
        suggestions.push(init_prompt_suggestion());
    }

    suggestions
}

fn has_agents_md(workspace_root: &Path) -> bool {
    workspace_root.join("AGENTS.md").exists() || workspace_root.join("agents.md").exists()
}

fn init_prompt_suggestion() -> String {
    let mut first_candidate: Option<String> = None;

    for line in INIT_PROMPT.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty()
            || trimmed.ends_with(':')
            || trimmed.starts_with('-')
            || trimmed.starts_with('`')
            || trimmed.starts_with('"')
            || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        {
            continue;
        }

        if first_candidate.is_none() {
            first_candidate = Some(trimmed.to_string());
        }

        if trimmed.to_ascii_lowercase().contains("analyze this codebase") {
            return trimmed.to_string();
        }
    }

    first_candidate.unwrap_or_else(|| README_IMPROVEMENT_SUGGESTION.to_string())
}

pub(crate) fn suggestion_areas(frame_area: Rect, suggestions: &Vec<String>) -> Vec<Rect> {
    if suggestions.is_empty() {
        return Vec::new();
    }

    let shell_layout = WelcomeShell.split(frame_area);
    let Some(main_content_area) = shell_layout.first().copied() else {
        return Vec::new();
    };

    let centered = WelcomeMainColumn.area(main_content_area);
    let content_layout = WelcomeContent.split(centered);
    let Some(suggestions_area) = content_layout.get(5).copied() else {
        return Vec::new();
    };

    Suggestions.card_areas(suggestions_area, suggestions)
}

fn point_in_rect(x: u16, y: u16, area: Rect) -> bool {
    x >= area.x && x < area.right() && y >= area.y && y < area.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent};

    #[test]
    fn test_welcome_default() {
        let app = WelcomeApp::new();
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.cursor_position, 0);
        assert!(!app.suggestions.is_empty());
    }

    #[test]
    fn test_init_prompt_suggestion_is_loaded() {
        let suggestion = init_prompt_suggestion();
        assert!(suggestion.to_ascii_lowercase().contains("analyze this codebase"));
    }

    #[test]
    fn test_build_welcome_suggestions_hides_agents_prompt_when_agents_exists() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("thndrs-ui-suggestions-{unique}"));
        std::fs::create_dir_all(&workspace).expect("workspace should be created");

        let without_agents = build_welcome_suggestions(&workspace);
        assert!(without_agents[1].to_ascii_lowercase().contains("analyze this codebase"));

        std::fs::write(workspace.join("AGENTS.md"), "# rules\n").expect("agents file should be created");
        let with_agents = build_welcome_suggestions(&workspace);
        assert_eq!(with_agents[1], README_IMPROVEMENT_SUGGESTION);

        std::fs::remove_dir_all(workspace).expect("workspace should be removed");
    }

    #[test]
    fn test_enter_sets_pending_submission() {
        let mut app = WelcomeApp::new();
        app.handle_input(KeyEvent::from(KeyCode::Char('h')));
        app.handle_input(KeyEvent::from(KeyCode::Char('i')));

        let action = app.handle_input(KeyEvent::from(KeyCode::Enter));

        assert_eq!(action, ScreenAction::SwitchTo(ScreenMode::Chat));
        assert_eq!(app.take_pending_submission().as_deref(), Some("hi"));
    }
}
