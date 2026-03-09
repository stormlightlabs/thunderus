//! Help screen and tutorial for Thunderus
//!
//! Provides:
//! - Tabbed navigation: Keyboard Shortcuts, Commands, Tips, About, Tutorial
//! - Version display
//! - Shortcut grid (two-column, key + description)
//! - Slash command list
//! - Tip box with rotating tips
//! - Quick start shortcuts (tutorial tab)
//! - Recent conversations list
//! - Footer links

use super::ScreenAction;
use super::colors;
use super::components::{HintFooter, HintToken, TopBorderedInputRow, app_version_string};
use super::layout::{ConstraintSpec, split as split_rects};
use super::scroll::ScrollState;
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use thndrs_mem::{Session, SessionManager};
use thndrs_ui_macros::AreaSpec;

const HELP_TABS: &[&str] = &["Keyboard Shortcuts", "Commands", "Tips", "About", "Tutorial"];
const TIPS: &[&str] = &[
    "Use @ to reference files in your workspace. Type @ followed by the filename to include file context.",
    "Use Up/Down to cycle previous prompts in chat and recover drafts",
    "Tab toggles the latest tool call, Shift+Tab toggles all tool calls in the latest assistant response",
    "Ctrl+N starts a new conversation instantly",
    "Ctrl+O opens the workspace file browser for multi-file context",
    "Ctrl+W returns to the welcome screen from chat and files",
    "Use /debug chat to load a long chat for testing scroll behavior",
    "Recent conversations are automatically saved and can be resumed",
];

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigation",
        &[
            ("New chat", "Ctrl+N"),
            ("Close chat", "Ctrl+W"),
            ("Open file browser", "Ctrl+O"),
            ("Open settings", "Ctrl+,"),
            ("Open help", "/help or F1"),
            ("Quit", "Ctrl+D / Ctrl+Q"),
        ],
    ),
    (
        "Editing",
        &[
            ("Send message", "Enter"),
            ("New line", "Shift+Enter / Ctrl+J"),
            ("Clear input", "Ctrl+K"),
            ("Focus latest", "Ctrl+L"),
            ("Pin file", "@"),
        ],
    ),
    (
        "Chat",
        &[
            ("Previous input", "Up"),
            ("Next input", "Down"),
            ("Scroll up", "PageUp"),
            ("Scroll down", "PageDown"),
            ("Toggle tool", "Tab"),
            ("Toggle all tools", "Shift+Tab"),
            ("Clear chat", "/clear"),
            ("Show history", "/history"),
            ("Show tokens", "/tokens"),
        ],
    ),
];

const COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show this help menu"),
    ("/clear", "Clear current conversation"),
    ("/model", "Show current AI model"),
    ("/tokens", "Show token usage"),
    ("/history", "Show saved sessions"),
    ("/resume <id>", "Resume a session"),
    ("/settings", "Open settings"),
    ("/files", "Open file browser"),
    ("/debug chat", "Load debug chat"),
    ("/debug files", "Load debug file tree"),
    ("/debug memory stats", "Show memory statistics"),
    ("/debug memory recall <query>", "Recall memories"),
    ("/debug log <id>", "Show session logs"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpMsg {
    Key(KeyEvent),
}

pub type HelpModel = HelpApp;

/// Help application state
#[derive(Debug, Clone)]
pub struct HelpApp {
    pub selected_tab: usize,
    pub scroll: ScrollState,
    pub tip_index: usize,
    pub recent_sessions: Vec<Session>,
    pub status_message: Option<String>,
}

impl Default for HelpApp {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpApp {
    pub fn new() -> Self {
        let recent_sessions = Self::load_recent_sessions();
        let mut app = Self {
            selected_tab: 0,
            scroll: ScrollState::with_viewport(0, 20),
            tip_index: 0,
            recent_sessions,
            status_message: None,
        };
        app.sync_scroll_state();
        app
    }

    fn load_recent_sessions() -> Vec<Session> {
        if let Ok(workspace_path) = std::env::current_dir()
            && let Ok(db) = thndrs_mem::MemoryDatabase::for_workspace(&workspace_path)
        {
            let manager = SessionManager::new(db);
            if let Ok(sessions) = manager.list_sessions(10) {
                return sessions;
            }
        }
        Vec::new()
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> ScreenAction {
        let Some(msg) = map_help_key_to_msg(key) else {
            return ScreenAction::None;
        };
        update(self, msg)
    }

    fn sync_scroll_state(&mut self) {
        self.scroll.set_viewport(self.tab_line_count(), 20);
    }

    fn tab_line_count(&self) -> usize {
        match HELP_TABS[self.selected_tab] {
            "Keyboard Shortcuts" => SHORTCUTS.iter().map(|(_, rows)| rows.len() + 3).sum::<usize>() + 3,
            "Commands" => COMMANDS.len() + 4,
            "Tips" => TIPS.len() + 10,
            "About" => 30,
            "Tutorial" => 24 + self.recent_sessions.len().min(5),
            _ => 20,
        }
    }

    fn refresh_sessions(&mut self) {
        self.recent_sessions = Self::load_recent_sessions();
        self.status_message = Some("Session list refreshed".to_string());
        self.sync_scroll_state();
    }

    fn next_tip(&mut self) {
        self.tip_index = (self.tip_index + 1) % TIPS.len();
    }

    pub fn current_tip(&self) -> &str {
        TIPS[self.tip_index % TIPS.len()]
    }

    pub fn version_string() -> String {
        app_version_string()
    }

    pub fn build_info() -> String {
        format!(
            "Build: {} | {} {}",
            Self::version_string(),
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    }
}

pub(crate) fn map_help_key_to_msg(key: KeyEvent) -> Option<HelpMsg> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    Some(HelpMsg::Key(key))
}

pub(crate) fn update(model: &mut HelpModel, msg: HelpMsg) -> ScreenAction {
    match msg {
        HelpMsg::Key(key) => match key.code {
            KeyCode::Esc => ScreenAction::ReturnToPrevious,
            KeyCode::Left | KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if model.selected_tab > 0 {
                    model.selected_tab -= 1;
                    model.scroll.set_offset(0);
                    model.sync_scroll_state();
                }
                ScreenAction::None
            }
            KeyCode::Right | KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if model.selected_tab + 1 < HELP_TABS.len() {
                    model.selected_tab += 1;
                    model.scroll.set_offset(0);
                    model.sync_scroll_state();
                }
                ScreenAction::None
            }
            KeyCode::Tab => {
                model.selected_tab = (model.selected_tab + 1) % HELP_TABS.len();
                model.scroll.set_offset(0);
                model.sync_scroll_state();
                ScreenAction::None
            }
            KeyCode::BackTab => {
                if model.selected_tab == 0 {
                    model.selected_tab = HELP_TABS.len() - 1;
                } else {
                    model.selected_tab -= 1;
                }
                model.scroll.set_offset(0);
                model.sync_scroll_state();
                ScreenAction::None
            }
            KeyCode::Up => {
                model.scroll.scroll_up(1);
                ScreenAction::None
            }
            KeyCode::Down => {
                model.scroll.scroll_down(1);
                ScreenAction::None
            }
            KeyCode::PageUp => {
                model.scroll.page_up();
                ScreenAction::None
            }
            KeyCode::PageDown => {
                model.scroll.page_down();
                ScreenAction::None
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                model.refresh_sessions();
                ScreenAction::None
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                model.next_tip();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        },
    }
}

pub fn view(frame: &mut Frame, model: &HelpModel) {
    draw_help_screen(frame, model);
}

#[derive(AreaSpec)]
pub struct HelpShell;

impl ConstraintSpec for HelpShell {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _: Rect) -> Vec<Constraint> {
        vec![Constraint::Min(0), Constraint::Length(1), Constraint::Length(3)]
    }
}

pub fn draw_help_screen(frame: &mut Frame, app: &HelpModel) {
    let size = frame.area();
    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let shell = HelpShell.split(size);
    if shell.len() < 3 {
        return;
    }

    draw_help_main(frame, shell[0], app);
    draw_help_hints(frame, shell[1]);
    draw_help_status(frame, shell[2], app);
}

fn draw_help_main(frame: &mut Frame, area: Rect, app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let header_height = 2u16;
    let content_layout = split_rects(
        inner,
        Direction::Vertical,
        vec![Constraint::Length(header_height), Constraint::Min(0)],
    );

    if content_layout.len() < 2 {
        return;
    }

    draw_help_tabs(frame, content_layout[0], app);

    let tab_content = content_layout[1];
    match HELP_TABS[app.selected_tab] {
        "Keyboard Shortcuts" => draw_shortcuts_tab(frame, tab_content, app),
        "Commands" => draw_commands_tab(frame, tab_content, app),
        "Tips" => draw_tips_tab(frame, tab_content, app),
        "About" => draw_about_tab(frame, tab_content, app),
        "Tutorial" => draw_tutorial_tab(frame, tab_content, app),
        _ => {}
    }
}

fn draw_help_tabs(frame: &mut Frame, area: Rect, app: &HelpApp) {
    let mut spans = vec![];
    for (idx, tab) in HELP_TABS.iter().enumerate() {
        let is_selected = idx == app.selected_tab;
        let style = if is_selected {
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::TEXT_SECONDARY)
        };

        if idx > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }

        let tab_text = if is_selected { format!("[{}]", tab) } else { tab.to_string() };
        spans.push(Span::styled(tab_text, style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn draw_shortcuts_tab(frame: &mut Frame, area: Rect, _app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![];

    for (section_name, shortcuts) in SHORTCUTS {
        lines.push(Line::from(vec![Span::styled(
            *section_name,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        for (name, keys) in *shortcuts {
            let name_text = format!("  {}", name);
            let keys_text = (*keys).to_string();
            let row_width = inner.width as usize;
            let name_width = name_text.chars().count();
            let keys_width = keys_text.chars().count();
            let padding = row_width.saturating_sub(name_width + keys_width).max(1);

            lines.push(Line::from(vec![
                Span::styled(name_text, Style::default().fg(colors::TEXT_SECONDARY)),
                Span::raw(" ".repeat(padding)),
                Span::styled(keys_text, Style::default().fg(colors::ACCENT_CYAN)),
            ]));
        }

        lines.push(Line::from(""));
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((_app.scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_commands_tab(frame: &mut Frame, area: Rect, _app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![];

    lines.push(Line::from(vec![Span::styled(
        "Slash Commands",
        Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for (cmd, desc) in COMMANDS {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", cmd), Style::default().fg(colors::ACCENT_CYAN)),
            Span::raw(" — "),
            Span::styled(*desc, Style::default().fg(colors::TEXT_SECONDARY)),
        ]));
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((_app.scroll.offset.min(u16::MAX as usize) as u16, 0));
    frame.render_widget(paragraph, inner);
}

fn draw_tips_tab(frame: &mut Frame, area: Rect, app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Tips",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Current Tip",
            Style::default().fg(colors::ACCENT_GREEN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    let tip_text = app.current_tip();
    for line in tip_text.lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "All Tips",
        Style::default().fg(colors::ACCENT_GREEN).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for (idx, tip) in TIPS.iter().enumerate() {
        let marker = if idx == app.tip_index % TIPS.len() { "▸ " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(format!("{}", idx + 1), Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(". ", Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(
                if tip.len() > 60 { format!("{}...", &tip[..60]) } else { tip.to_string() },
                Style::default().fg(colors::TEXT_SECONDARY),
            ),
        ]));
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_about_tab(frame: &mut Frame, area: Rect, _app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![];

    let logo = r#"
 ▐▖   ▀▛▘▌ ▌▌ ▌▙ ▌▛▀▖▛▀▘▛▀▖▌ ▌▞▀▖
 ▐▝▚▖  ▌ ▙▄▌▌ ▌▌▌▌▌ ▌▙▄ ▙▄▘▌ ▌▚▄
 ▐▞▘   ▌ ▌ ▌▌ ▌▌▝▌▌ ▌▌  ▌▚ ▌ ▌▖ ▌
 ▝     ▘ ▘ ▘▝▀ ▘ ▘▀▀ ▀▀▘▘ ▘▝▀ ▝▀
"#;

    for line in logo.lines() {
        lines.push(Line::from(vec![Span::styled(
            line.to_string(),
            Style::default().fg(colors::ACCENT_CYAN),
        )]));
    }

    lines.push(Line::from(""));

    lines.push(Line::from(vec![Span::styled(
        HelpApp::version_string(),
        Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
    )]));

    lines.push(Line::from(vec![Span::styled(
        HelpApp::build_info(),
        Style::default().fg(colors::TEXT_MUTED),
    )]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "About",
        Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    let about_text = "Thunderus is an AI coding assistant that helps you write, \
        understand, and improve code. It provides intelligent code completion, \
        file navigation, and contextual assistance powered by large language models.";

    for line in about_text.lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Links",
        Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  Documentation: ", Style::default().fg(colors::TEXT_SECONDARY)),
        Span::styled("https://docs.thunderus.dev", Style::default().fg(colors::ACCENT_CYAN)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  GitHub: ", Style::default().fg(colors::TEXT_SECONDARY)),
        Span::styled(
            "https://github.com/thunderus/thunderus",
            Style::default().fg(colors::ACCENT_CYAN),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  Report Issues: ", Style::default().fg(colors::TEXT_SECONDARY)),
        Span::styled(
            "https://github.com/thunderus/thunderus/issues",
            Style::default().fg(colors::ACCENT_CYAN),
        ),
    ]));

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((_app.scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_tutorial_tab(frame: &mut Frame, area: Rect, app: &HelpApp) {
    let block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(2, 2, 1, 1));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Quick Start",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            HelpApp::version_string(),
            Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            HelpApp::build_info(),
            Style::default().fg(colors::TEXT_MUTED),
        )]),
        Line::from(""),
    ];

    let quick_actions = [
        ("Ctrl+N", "Start a new conversation"),
        ("Ctrl+O", "Open the workspace file browser"),
        ("Ctrl+W", "Return to welcome from chat/files"),
        ("/help", "View keyboard shortcuts"),
    ];

    for (shortcut, desc) in &quick_actions {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}", shortcut), Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(" — ", Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(*desc, Style::default().fg(colors::TEXT_SECONDARY)),
        ]));
    }

    lines.push(Line::from(""));

    if !app.recent_sessions.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "Recent Conversations",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        for (idx, session) in app.recent_sessions.iter().enumerate().take(5) {
            let time_ago = format_time_ago(&session.updated_at);
            let title = session.display_title();
            let truncated_title = if title.len() > 40 { format!("{}...", &title[..37]) } else { title.to_string() };
            let session_id = if session.id.len() > 8 { &session.id[..8] } else { &session.id };

            lines.push(Line::from(vec![
                Span::styled(format!("  {}.", idx + 1), Style::default().fg(colors::ACCENT_CYAN)),
                Span::styled(" ", Style::default()),
                Span::styled(truncated_title, Style::default().fg(colors::TEXT_SECONDARY)),
                Span::styled(" — ", Style::default().fg(colors::TEXT_MUTED)),
                Span::styled(session_id.to_string(), Style::default().fg(colors::TEXT_MUTED)),
                Span::styled(" — ", Style::default().fg(colors::TEXT_MUTED)),
                Span::styled(time_ago, Style::default().fg(colors::TEXT_MUTED)),
            ]));
        }

        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![Span::styled(
        "Pro Tip",
        Style::default().fg(colors::ACCENT_GREEN).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for line in app.current_tip().lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Tip: ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled("Press ", Style::default().fg(colors::TEXT_SECONDARY)),
        Span::styled("Ctrl+T", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(" to see another tip", Style::default().fg(colors::TEXT_SECONDARY)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Thunderus - AI Coding Assistant",
        Style::default().fg(colors::TEXT_MUTED),
    )]));
    lines.push(Line::from(vec![
        Span::styled("Documentation", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(" • ", Style::default().fg(colors::TEXT_MUTED)),
        Span::styled("GitHub", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(" • ", Style::default().fg(colors::TEXT_MUTED)),
        Span::styled("Report Issue", Style::default().fg(colors::ACCENT_CYAN)),
    ]));

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .scroll((app.scroll.offset.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

fn draw_help_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("Tab"),
        HintToken::Text("/"),
        HintToken::Key("Ctrl+←/→"),
        HintToken::Text(" switch tabs, "),
        HintToken::Key("↑/↓"),
        HintToken::Text(" scroll, "),
        HintToken::Key("Ctrl+T"),
        HintToken::Text(" next tip, "),
        HintToken::Key("Ctrl+R"),
        HintToken::Text(" refresh, "),
        HintToken::Key("Esc"),
        HintToken::Text(" exit"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_help_status(frame: &mut Frame, area: Rect, app: &HelpApp) {
    let status = app
        .status_message
        .as_deref()
        .map_or_else(HelpApp::version_string, |s| s.to_string());

    TopBorderedInputRow.render(frame, area, &status, false);
}

fn format_time_ago(timestamp: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*timestamp);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 7 {
        format!("{}d ago", duration.num_days())
    } else {
        timestamp.format("%Y-%m-%d").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    #[test]
    fn test_help_app_new() {
        let app = HelpApp::new();
        assert_eq!(app.selected_tab, 0);
        assert_eq!(app.scroll.offset, 0);
    }

    #[test]
    fn test_help_tabs() {
        assert_eq!(HELP_TABS.len(), 5);
        assert_eq!(HELP_TABS[0], "Keyboard Shortcuts");
        assert_eq!(HELP_TABS[4], "Tutorial");
    }

    #[test]
    fn test_tips_not_empty() {
        assert!(!TIPS.is_empty());
    }

    #[test]
    fn test_current_tip() {
        let app = HelpApp::new();
        let tip = app.current_tip();
        assert!(!tip.is_empty());
    }

    #[test]
    fn test_next_tip() {
        let mut app = HelpApp::new();
        let first_tip = app.current_tip().to_string();
        app.next_tip();
        let second_tip = app.current_tip();
        assert_ne!(first_tip, second_tip);
    }

    #[test]
    fn test_version_string() {
        let version = HelpApp::version_string();
        assert!(version.starts_with("Thunderus v"));
    }

    #[test]
    fn test_format_time_ago() {
        let now = Utc::now();
        let just_now = now - Duration::seconds(30);
        assert_eq!(format_time_ago(&just_now), "just now");

        let minutes_ago = now - Duration::minutes(5);
        assert_eq!(format_time_ago(&minutes_ago), "5m ago");

        let hours_ago = now - Duration::hours(3);
        assert_eq!(format_time_ago(&hours_ago), "3h ago");

        let days_ago = now - Duration::days(2);
        assert_eq!(format_time_ago(&days_ago), "2d ago");
    }
}
