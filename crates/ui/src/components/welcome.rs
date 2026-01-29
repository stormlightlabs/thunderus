use crate::{
    layout::{LayoutMode, WelcomeLayout},
    state::AppState,
    theme::{Theme, ThemePalette},
};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

const THUNDERUS_LOGO: [&str; 4] = [
    r"▐▖   ▀▛▘▌ ▌▌ ▌▙ ▌▛▀▖▛▀▘▛▀▖▌ ▌▞▀▖",
    r"▐▝▚▖  ▌ ▙▄▌▌ ▌▌▌▌▌ ▌▙▄ ▙▄▘▌ ▌▚▄ ",
    r"▐▞▘   ▌ ▌ ▌▌ ▌▌▝▌▌ ▌▌  ▌▚ ▌ ▌▖ ▌",
    r"▝     ▘ ▘ ▘▝▀ ▘ ▘▀▀ ▀▀▘▘ ▘▝▀ ▝▀ ",
];

const INPUT_PLACEHOLDER: &str = "Type a message to start a session...";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Renders a clean, centered welcome screen following DESIGN.txt layout:
///
/// Header: thunderus | branch | model | approval
/// Center: Welcome card with logo and message
/// Footer: Input prompt with Esc hint
pub struct WelcomeView<'a> {
    state: &'a AppState,
    layout: WelcomeLayout,
    mode: LayoutMode,
}

impl<'a> WelcomeView<'a> {
    pub fn new(state: &'a AppState, area: Rect) -> Self {
        let mode = LayoutMode::from(area.width);
        let layout = WelcomeLayout::calculate(area, mode);
        Self { state, layout, mode }
    }

    /// Render the complete welcome screen
    pub fn render(&self, frame: &mut Frame<'_>) {
        let theme = Theme::palette(self.state.theme_variant());
        frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), frame.area());

        self.render_header_bar(frame, theme);
        self.render_welcome_card(frame, theme);
        self.render_footer(frame, theme);
    }

    /// Render header bar: thunderus | branch | model | approval (left) + cwd (right)
    fn render_header_bar(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = self.layout.header;
        if area.height == 0 {
            return;
        }

        let mut left_spans = Vec::new();

        left_spans.push(Span::styled(" thunderus", Style::default().fg(theme.fg)));
        left_spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));

        if let Some(branch) = self.state.git_branch() {
            left_spans.push(Span::styled(branch, Style::default().fg(theme.cyan)));
        } else {
            let cwd = self.state.cwd();
            let basename = cwd
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_else(|| "~".into());
            left_spans.push(Span::styled(basename.to_string(), Style::default().fg(theme.cyan)));
        }
        left_spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));

        left_spans.push(Span::styled(
            self.state.model_selector.current_model.clone(),
            Style::default().fg(theme.blue),
        ));
        left_spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));

        let approval_str = match self.state.config.approval_mode {
            thunderus_core::ApprovalMode::ReadOnly => "read-only",
            thunderus_core::ApprovalMode::Auto => "auto",
            thunderus_core::ApprovalMode::FullAccess => "full-access",
        };
        let approval_color = Theme::approval_mode_color(theme, approval_str);
        left_spans.push(Span::styled(approval_str, Style::default().fg(approval_color)));

        let mut line = Line::from(left_spans);
        let left_width = line.spans.iter().map(|s| s.content.width()).sum::<usize>() as u16;
        let mut right = self.state.cwd().display().to_string();
        let max_right = area.width.saturating_sub(left_width + 2);
        if max_right > 0 {
            if right.len() > max_right as usize {
                let keep = max_right.saturating_sub(3) as usize;
                if keep > 0 && keep < right.len() {
                    right = format!("...{}", &right[right.len() - keep..]);
                }
            }
            let pad = area.width.saturating_sub(left_width + right.len() as u16);
            if pad > 0 {
                line.spans
                    .push(Span::styled(" ".repeat(pad as usize), Style::default().bg(theme.bg)));
            }
            line.spans.push(Span::styled(right, Style::default().fg(theme.muted)));
        }

        let paragraph = Paragraph::new(line)
            .block(Block::default().style(Style::default().bg(theme.bg)))
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }

    /// Render centered welcome card with logo
    fn render_welcome_card(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = frame.area();
        let card_width = 50.min(area.width.saturating_sub(8));
        let card_height = 10.min(area.height.saturating_sub(6));
        let card_x = area.x + (area.width.saturating_sub(card_width)) / 2;
        let card_y = area.y + (area.height.saturating_sub(card_height)) / 2;
        let inner = Rect { x: card_x, y: card_y, width: card_width, height: card_height };

        let mut lines = Vec::new();

        match self.mode {
            LayoutMode::Compact => {
                if inner.height >= 8 {
                    lines.push(Line::default());
                    for (idx, logo_line) in THUNDERUS_LOGO.iter().enumerate() {
                        let color = if idx < 2 { theme.cyan } else { theme.blue };
                        lines.push(Line::from(Span::styled(*logo_line, Style::default().fg(color))));
                    }
                    lines.push(Line::default());
                }
            }
            _ => {
                lines.push(Line::default());
                lines.push(Line::from(Span::styled(
                    "Welcome to Thunderus",
                    Style::default().fg(theme.fg),
                )));
                lines.push(Line::default());
            }
        }

        let content = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(content, inner);
    }

    /// Render footer: input prompt and hints
    fn render_footer(&self, frame: &mut Frame<'_>, theme: ThemePalette) {
        let area = frame.area();

        let footer_height = 4;
        let footer_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(footer_height),
            width: area.width,
            height: footer_height,
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .split(footer_area);

        self.render_input_card(frame, rows[0], theme);

        let hints_area = Rect { x: rows[1].x + 2, y: rows[1].y, width: rows[1].width.saturating_sub(4), height: 1 };

        let left_spans = vec![Span::styled(
            format!("v{}  github.com/stormlightlabs/thunderus", VERSION),
            Style::default().fg(theme.muted),
        )];
        let left = Paragraph::new(Line::from(left_spans)).alignment(Alignment::Left);

        let right_spans = vec![
            Span::styled("esc", Style::default().fg(theme.blue)),
            Span::styled(" dismiss", Style::default().fg(theme.muted)),
        ];
        let right = Paragraph::new(Line::from(right_spans)).alignment(Alignment::Right);

        let half_width = hints_area.width / 2;
        let left_area = Rect { x: hints_area.x, y: hints_area.y, width: half_width, height: 1 };
        let right_area = Rect { x: hints_area.x + half_width, y: hints_area.y, width: half_width, height: 1 };

        frame.render_widget(left, left_area);
        frame.render_widget(right, right_area);
    }

    /// Render input card styled like the main session input
    fn render_input_card(&self, frame: &mut Frame<'_>, area: Rect, theme: ThemePalette) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let panel_block = Block::default().style(Style::default().bg(theme.active));
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

        let mut spans = Vec::new();
        if self.state.input.buffer.is_empty() {
            spans.push(Span::styled("█", Style::default().bg(theme.fg).fg(theme.fg)));
            spans.push(Span::styled(
                INPUT_PLACEHOLDER,
                Style::default().fg(theme.muted).bg(theme.active),
            ));
        } else {
            let input_style = Style::default().fg(theme.fg).bg(theme.active);
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
                thinking: Default::default(),
                options: Default::default(),
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
        )
    }

    #[test]
    fn test_welcome_view_new() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 100, 30);
        let view = WelcomeView::new(&state, area);
        assert_eq!(view.mode, LayoutMode::Full);
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
        assert!(view.layout.header.height > 0);
    }

    #[test]
    fn test_welcome_layout_compact_mode() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 70, 25);
        let view = WelcomeView::new(&state, area);
        assert_eq!(view.mode, LayoutMode::Compact);
    }

    #[test]
    fn test_welcome_medium_mode() {
        let state = create_test_state();
        let area = Rect::new(0, 0, 90, 30);
        let view = WelcomeView::new(&state, area);
        assert_eq!(view.mode, LayoutMode::Medium);
    }
}
