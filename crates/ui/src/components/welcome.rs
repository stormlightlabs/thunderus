use crate::{
    layout::{LayoutMode, WelcomeLayout},
    state::AppState,
    theme::{Theme, ThemePalette},
};

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

const THUNDERUS_LOGO: [&str; 4] = [
    r"▐▖   ▀▛▘▌ ▌▌ ▌▙ ▌▛▀▖▛▀▘▛▀▖▌ ▌▞▀▖",
    r"▐▝▚▖  ▌ ▙▄▌▌ ▌▌▌▌▌ ▌▙▄ ▙▄▘▌ ▌▚▄ ",
    r"▐▞▘   ▌ ▌ ▌▌ ▌▌▝▌▌ ▌▌  ▌▚ ▌ ▌▖ ▌",
    r"▝     ▘ ▘ ▘▝▀ ▘ ▘▀▀ ▀▀▘▘ ▘▝▀ ▝▀ ",
];

const INPUT_PLACEHOLDER: &str = "What do you want to build?";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Renders a clean, centered welcome screen without header/sidebar:
/// - Gradient logo (cyan to blue)
/// - Input card with blue accent bar
/// - Recent sessions row
/// - Centered keyboard shortcuts
/// - Rotating tips with yellow indicator
/// - Status bar (path:branch | version)
pub struct WelcomeView<'a> {
    state: &'a AppState,
    layout: WelcomeLayout,
}

impl<'a> WelcomeView<'a> {
    pub fn new(state: &'a AppState, area: Rect) -> Self {
        let mode = LayoutMode::from(area.width);
        let layout = WelcomeLayout::calculate(area, mode);
        Self { state, layout }
    }

    /// Render the complete welcome screen
    pub fn render(&self, frame: &mut Frame<'_>) {
        let theme = Theme::palette(self.state.theme_variant());
        frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), frame.area());

        self.render_header(frame, theme);
        self.render_logo(frame, theme);
        self.render_input_card(frame, theme);
        self.render_recent_sessions(frame, theme);
        self.render_shortcuts(frame, theme);
        self.render_tip(frame, theme);
        self.render_status_bar(frame, theme);
    }

    /// Render the logo with gradient colors (cyan for top, blue for bottom)
    fn render_logo(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let mut lines = Vec::new();

        lines.push(Line::default());

        for (idx, logo_line) in THUNDERUS_LOGO.iter().enumerate() {
            let color = if idx < 2 { theme.cyan } else { theme.blue };
            lines.push(Line::from(Span::styled(*logo_line, Style::default().fg(color))));
        }

        lines.push(Line::default());

        let paragraph = Paragraph::new(lines)
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, self.layout.logo);
    }

    /// Render the input card with blue accent bar on the left
    fn render_input_card(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.input_card;
        if area.width < 10 || area.height < 1 {
            return;
        }

        let panel_block = Block::default().style(Style::default().bg(theme.panel_bg));
        frame.render_widget(panel_block, area);

        let accent_width = 1;
        let accent_area = Rect { x: area.x, y: area.y, width: accent_width, height: area.height };
        let accent_block = Block::default().style(Style::default().bg(theme.blue));
        frame.render_widget(accent_block, accent_area);

        let input_area = Rect {
            x: area.x + accent_width + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(accent_width + 2),
            height: 1,
        };

        let input_text = if self.state.input.buffer.is_empty() {
            INPUT_PLACEHOLDER.to_string()
        } else {
            self.state.input.buffer.clone()
        };

        let input_style = if self.state.input.buffer.is_empty() {
            Style::default().fg(theme.muted).bg(theme.panel_bg)
        } else {
            Style::default().fg(theme.fg).bg(theme.panel_bg)
        };

        let mut spans = Vec::new();

        if self.state.input.buffer.is_empty() {
            spans.push(Span::styled(input_text, input_style));
            spans.push(Span::styled("█", Style::default().bg(theme.fg).fg(theme.fg)));
        } else {
            let cursor_pos = self.state.input.cursor.min(self.state.input.buffer.len());
            let before_cursor = &self.state.input.buffer[..cursor_pos];
            let after_cursor = &self.state.input.buffer[cursor_pos..];

            if !before_cursor.is_empty() {
                spans.push(Span::styled(before_cursor.to_string(), input_style));
            }
            spans.push(Span::styled("█", Style::default().bg(theme.fg).fg(theme.fg)));
            if !after_cursor.is_empty() {
                spans.push(Span::styled(after_cursor.to_string(), input_style));
            }
        }

        let input_paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(input_paragraph, input_area);
    }

    /// Render recent sessions as clickable cards
    fn render_recent_sessions(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.recent_sessions;
        if area.height == 0 || self.state.welcome.recent_sessions.is_empty() {
            return;
        }

        let mut spans = Vec::new();
        spans.push(Span::styled("Recent: ", Style::default().fg(theme.muted)));

        for (idx, session) in self.state.welcome.recent_sessions.iter().take(3).enumerate() {
            if idx > 0 {
                spans.push(Span::styled("  ", Style::default()));
            }
            let title = session
                .title
                .as_deref()
                .unwrap_or(&session.id[..8.min(session.id.len())]);
            let truncated = if title.len() > 20 { format!("{}...", &title[..17]) } else { title.to_string() };
            spans.push(Span::styled(
                format!("[{}]", truncated),
                Style::default().fg(theme.cyan),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Render centered keyboard shortcuts
    fn render_shortcuts(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.shortcuts;
        if area.height == 0 {
            return;
        }

        let mode = LayoutMode::from(area.width + 20);
        let shortcuts = match mode {
            LayoutMode::Full | LayoutMode::Inspector => vec![
                ("Enter", "send"),
                ("Backspace", "start"),
                ("Tab", "autocomplete"),
                ("Ctrl+S", "sidebar"),
                ("Ctrl+T", "theme"),
                ("Esc", "exit"),
            ],
            LayoutMode::Medium => vec![
                ("Enter", "send"),
                ("Backspace", "start"),
                ("Tab", "complete"),
                ("Ctrl+T", "theme"),
                ("Esc", "exit"),
            ],
            LayoutMode::Compact => vec![("Enter", "send"), ("Esc", "exit")],
        };

        let mut spans = Vec::new();
        for (idx, (key, action)) in shortcuts.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled("  ", Style::default()));
            }
            spans.push(Span::styled(*key, Style::default().fg(theme.blue)));
            spans.push(Span::styled(format!(" {}", action), Style::default().fg(theme.muted)));
        }

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Left);

        let aligned_area =
            Rect { x: area.x.saturating_add(2), y: area.y, width: area.width.saturating_sub(2), height: area.height };

        frame.render_widget(paragraph, aligned_area);
    }

    /// Render tip with yellow indicator dot
    fn render_tip(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.tips;
        if area.height == 0 {
            return;
        }

        let tip = self.state.welcome.current_tip();

        let spans = vec![
            Span::styled("● ", Style::default().fg(theme.yellow)),
            Span::styled(tip, Style::default().fg(theme.muted).italic()),
        ];

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Render the header: git branch + working directory
    fn render_header(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.header;
        if area.height == 0 {
            return;
        }

        let cwd = self.state.cwd();
        let cwd_display = cwd.to_string_lossy().replace(
            dirs::home_dir()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_default()
                .as_str(),
            "~",
        );

        let mut spans = Vec::new();

        if let Some(branch) = self.state.git_branch() {
            spans.push(Span::styled("  λ ", Style::default().fg(theme.muted)));
            spans.push(Span::styled(branch, Style::default().fg(theme.muted)));
            spans.push(Span::styled("  ", Style::default()));
        } else {
            spans.push(Span::styled("  ", Style::default()));
        }

        spans.push(Span::styled(cwd_display, Style::default().fg(theme.muted)));

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }

    /// Render status bar: github URL | version
    fn render_status_bar(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.status_bar;
        if area.height == 0 {
            return;
        }

        let left_spans = vec![Span::styled(
            "  github.com/stormlightlabs/thunderus",
            Style::default().fg(theme.muted),
        )];

        let left_paragraph = Paragraph::new(Line::from(left_spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Left);

        let right_spans = vec![Span::styled(
            format!("v{}  ", VERSION),
            Style::default().fg(theme.muted),
        )];

        let right_paragraph = Paragraph::new(Line::from(right_spans))
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Right);

        let left_area = Rect { x: area.x, y: area.y, width: area.width / 2, height: area.height };
        let right_area = Rect { x: area.x + area.width / 2, y: area.y, width: area.width / 2, height: area.height };

        frame.render_widget(left_paragraph, left_area);
        frame.render_widget(right_paragraph, right_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_state() -> AppState {
        AppState::new(
            PathBuf::from("/home/user/project"),
            "test".to_string(),
            ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
        )
    }

    #[test]
    fn test_welcome_view_new() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 100, 30);
        let view = WelcomeView::new(&state, area);
        assert_eq!(view.layout.logo.width, 96);
    }

    #[test]
    fn test_thunderus_logo_lines() {
        assert_eq!(THUNDERUS_LOGO.len(), 4);
        for line in THUNDERUS_LOGO {
            assert!(!line.is_empty());
        }
    }

    #[test]
    fn test_welcome_layout_calculation() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 120, 30);
        let view = WelcomeView::new(&state, area);

        assert!(view.layout.logo.height > 0);
        assert!(view.layout.input_card.height > 0);
        assert!(view.layout.shortcuts.height > 0);
        assert!(view.layout.tips.height > 0);
        assert!(view.layout.status_bar.height > 0);
    }

    #[test]
    fn test_welcome_layout_compact_mode() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 70, 25);
        let view = WelcomeView::new(&state, area);
        assert_eq!(view.layout.recent_sessions.height, 0);
    }
}
