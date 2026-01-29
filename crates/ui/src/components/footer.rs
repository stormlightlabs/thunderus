use crate::{
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

/// Footer component displaying input composer, model selector, and hints
///
/// OpenCode-style layout:
/// - Row 1: Divider
/// - Row 2: Input card with blue accent bar
/// - Row 3: Model selector (left) + hints (right)
pub struct Footer<'a> {
    state: &'a AppState,
}

impl<'a> Footer<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render footer to the given frame with horizontal padding
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let h_padding: u16 = 2;

        let padded_area = Rect {
            x: area.x + h_padding,
            y: area.y,
            width: area.width.saturating_sub(h_padding * 2),
            height: area.height,
        };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(3), Constraint::Length(1)])
            .split(padded_area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "─".repeat(padded_area.width as usize),
                Style::default().fg(theme.muted),
            )])),
            rows[0],
        );

        self.render_input_card(frame, rows[1], theme);
        self.render_bottom_row(frame, rows[2], theme, padded_area.width);
    }

    /// Render input card with blue accent bar (like welcome screen)
    fn render_input_card(&self, frame: &mut Frame<'_>, area: Rect, theme: ThemePalette) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let panel_block = Block::default().style(Style::default().bg(theme.bg));
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
            let placeholder = if self.state.input.is_in_fork_mode() {
                format!(
                    "[FORK] Edit message #{}",
                    self.state.input.history_index.map(|i| i + 1).unwrap_or(1)
                )
            } else if self.state.input.is_navigating_history() {
                "<no message>".to_string()
            } else {
                "Type a message...".to_string()
            };
            let placeholder_style = if self.state.input.is_in_fork_mode() {
                Style::default().fg(theme.green).bg(theme.bg)
            } else if self.state.input.is_navigating_history() {
                Style::default().fg(theme.yellow).bg(theme.bg)
            } else {
                Style::default().fg(theme.muted).bg(theme.bg)
            };
            spans.push(Span::styled(placeholder, placeholder_style));
        } else {
            let input_style = Style::default().fg(theme.fg).bg(theme.bg);
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

        let cursor_text = format!("1:{} ", self.state.input.cursor + 1);
        let cursor_paragraph =
            Paragraph::new(Span::styled(cursor_text, Style::default().fg(theme.muted))).alignment(Alignment::Right);

        frame.render_widget(cursor_paragraph, input_area);
    }

    /// Render model/agent selector row (left-aligned)
    fn render_model_selector(&self, frame: &mut Frame<'_>, area: Rect, theme: ThemePalette) {
        let models = &self.state.model_selector.available_models;
        let current = &self.state.model_selector.current_model;

        let mut spans = Vec::new();

        for (idx, model) in models.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(" ", Style::default()));
            }

            let is_selected = model == current;
            let style =
                if is_selected { Style::default().fg(theme.blue).bold() } else { Style::default().fg(theme.muted) };

            spans.push(Span::styled(model.as_str(), style));
        }

        if let Some(ref agent) = self.state.model_selector.current_agent {
            spans.push(Span::styled(" | ", Style::default().fg(theme.muted)));
            spans.push(Span::styled(
                format!("@{}", agent),
                Style::default().fg(theme.purple).bold(),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }

    /// Render model selector (left) and hints (right) on the same row
    fn render_bottom_row(&self, frame: &mut Frame<'_>, area: Rect, theme: ThemePalette, width: u16) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        self.render_model_selector(frame, columns[0], theme);
        self.render_hints(frame, columns[1], theme, width);
    }

    /// Render keyboard hints row (right-aligned, responsive)
    fn render_hints(&self, frame: &mut Frame<'_>, area: Rect, theme: ThemePalette, width: u16) {
        let hints = self.get_responsive_hints(theme, width);

        let paragraph = Paragraph::new(Line::from(hints)).alignment(Alignment::Right);

        frame.render_widget(paragraph, area);
    }

    /// Generate responsive keyboard hints based on available width
    ///
    /// Prioritizes important hints when space is limited:
    /// - Narrow (<80): Just essential (ctrl+c quit)
    /// - Medium (80-120): Core hints (ctrl+c, ctrl+s, enter)
    /// - Wide (>120): All hints
    fn get_responsive_hints(&self, theme: ThemePalette, width: u16) -> Vec<Span<'_>> {
        let mut hints = Vec::new();
        let hint_style = Style::default().fg(theme.muted);
        let key_style = Style::default().fg(theme.blue);

        if self.state.ui.is_first_session && self.state.input.buffer.is_empty() {
            hints.push(Span::styled("esc", key_style));
            hints.push(Span::styled(" dismiss", hint_style));
            return hints;
        }

        if self.state.approval_ui.pending_approval.is_some() {
            hints.push(Span::styled("y", key_style));
            hints.push(Span::styled(" approve • ", hint_style));
            hints.push(Span::styled("n", key_style));
            hints.push(Span::styled(" reject • ", hint_style));
            hints.push(Span::styled("c", key_style));
            hints.push(Span::styled(" cancel", hint_style));
            return hints;
        }

        if self.state.is_generating() {
            hints.push(Span::styled("ctrl+c", key_style));
            hints.push(Span::styled(" cancel", hint_style));
            return hints;
        }

        let is_narrow = width < 80;
        let is_medium = (80..120).contains(&width);
        let is_wide = width >= 120;

        hints.push(Span::styled("ctrl+c", key_style));
        hints.push(Span::styled(" quit", hint_style));

        if is_narrow {
            return hints;
        }

        hints.insert(0, Span::styled(" • ", hint_style));
        hints.insert(
            0,
            if self.state.ui.sidebar_visible {
                Span::styled(" hide", hint_style)
            } else {
                Span::styled(" show", hint_style)
            },
        );
        hints.insert(0, Span::styled("ctrl+s", key_style));

        if is_medium {
            return hints;
        }

        hints.insert(0, Span::styled(" • ", hint_style));
        hints.insert(0, Span::styled(" theme", hint_style));
        hints.insert(0, Span::styled("ctrl+t", key_style));

        if is_wide {
            hints.insert(0, Span::styled(" • ", hint_style));
            hints.insert(0, Span::styled(" editor", hint_style));
            hints.insert(0, Span::styled("ctrl+shift+g", key_style));
        }

        hints
    }

    /// Generate keyboard hints (for tests, uses full width)
    #[cfg(test)]
    fn get_hints(&self, theme: ThemePalette) -> Vec<Span<'_>> {
        self.get_responsive_hints(theme, 200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ApprovalState;

    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_state() -> AppState {
        AppState::new(
            PathBuf::from("."),
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
    fn test_footer_new() {
        let state = create_test_state();
        let footer = Footer::new(&state);
        assert_eq!(footer.state.config.profile, "test");
    }

    #[test]
    fn test_footer_model_selector() {
        let state = create_test_state();
        let footer = Footer::new(&state);
        assert_eq!(footer.state.model_selector.current_model, "glm-4.7");
    }

    #[test]
    fn test_get_hints_normal_state() {
        let mut state = create_test_state();
        state.ui.set_first_session(false);
        let _footer = Footer::new(&state);
        let theme = Theme::palette(state.theme_variant());
        let hints = _footer.get_hints(theme);
        assert!(hints.iter().any(|s| s.content.contains("ctrl+s")));
    }

    #[test]
    fn test_get_hints_generating_state() {
        let mut state = create_test_state();
        state.ui.set_first_session(false);
        state.start_generation();

        let _footer = Footer::new(&state);
        let theme = Theme::palette(state.theme_variant());
        let hints = _footer.get_hints(theme);
        assert!(hints.iter().any(|s| s.content.contains("ctrl+c")));
    }

    #[test]
    fn test_get_hints_with_pending_approval() {
        let mut state = create_test_state();
        state.ui.set_first_session(false);
        state.approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let _footer = Footer::new(&state);
        let theme = Theme::palette(state.theme_variant());
        let hints = _footer.get_hints(theme);
        assert!(hints.iter().any(|s| s.content == "y"));
        assert!(hints.iter().any(|s| s.content == "n"));
    }

    #[test]
    fn test_model_selector_models() {
        let state = create_test_state();
        assert_eq!(state.model_selector.models().len(), 3);
        assert!(state.model_selector.models().contains(&"GLM-4.7".to_string()));
    }

    #[test]
    fn test_model_selector_display_name() {
        let state = create_test_state();
        assert_eq!(state.model_selector.display_name(), "glm-4.7");
    }
}
