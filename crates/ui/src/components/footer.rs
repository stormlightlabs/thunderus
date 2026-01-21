use crate::{state::AppState, theme::Theme};

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    style::Stylize,
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

/// Footer component displaying input composer, model selector, and hints
///
/// - Row 1: Input card with blue accent bar (2 chars)
/// - Row 2: Model/agent selector
/// - Row 3: Keyboard shortcuts (right-aligned)
pub struct Footer<'a> {
    state: &'a AppState,
}

impl<'a> Footer<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render footer to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());

        let rows = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Length(1),
                ratatui::layout::Constraint::Length(3),
                ratatui::layout::Constraint::Length(1),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(area);

        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "─".repeat(area.width as usize),
                Style::default().fg(theme.muted),
            )])),
            rows[0],
        );

        self.render_input_card(frame, rows[1], theme);
        self.render_model_selector(frame, rows[2], theme);

        let action_hints_area = Rect { x: rows[1].x, y: rows[1].y + rows[1].height, width: rows[1].width, height: 1 };
        let action_spans = vec![
            Span::styled("[Enter]", Style::default().fg(theme.blue)),
            Span::styled(" send  ", Style::default().fg(theme.muted)),
            Span::styled("[Esc]", Style::default().fg(theme.blue)),
            Span::styled(" exit", Style::default().fg(theme.muted)),
        ];
        frame.render_widget(
            Paragraph::new(Line::from(action_spans)).alignment(Alignment::Right),
            action_hints_area,
        );

        self.render_hints(frame, rows[3], theme);
    }

    /// Render input card with blue accent bar (like welcome screen)
    fn render_input_card(&self, frame: &mut Frame<'_>, area: Rect, theme: crate::theme::ThemePalette) {
        if area.width < 10 || area.height < 1 {
            return;
        }

        let panel_block = Block::default().style(Style::default().bg(theme.panel_bg));
        frame.render_widget(panel_block, area);

        let accent_width = 2;
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
            if self.state.input.is_navigating_history() {
                "<no message>".to_string()
            } else {
                "Type a message...".to_string()
            }
        } else {
            self.state.input.buffer.clone()
        };

        let input_style = if self.state.input.buffer.is_empty() {
            if self.state.input.is_navigating_history() {
                Style::default().fg(theme.yellow).bg(theme.panel_bg)
            } else {
                Style::default().fg(theme.muted).bg(theme.panel_bg)
            }
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

        let cursor_text = format!("1:{} ", self.state.input.cursor + 1);
        let cursor_paragraph =
            Paragraph::new(Span::styled(cursor_text, Style::default().fg(theme.muted))).alignment(Alignment::Right);

        frame.render_widget(cursor_paragraph, input_area);
    }

    /// Render model/agent selector row
    fn render_model_selector(&self, frame: &mut Frame<'_>, area: Rect, theme: crate::theme::ThemePalette) {
        let models = &self.state.model_selector.available_models;
        let current = &self.state.model_selector.current_model;

        let mut spans = Vec::new();

        for (idx, model) in models.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::styled(" ", Style::default().bg(theme.panel_bg)));
            }

            let is_selected = model == current;
            let style = if is_selected {
                Style::default().fg(theme.blue).bg(theme.panel_bg).bold()
            } else {
                Style::default().fg(theme.muted).bg(theme.panel_bg)
            };

            spans.push(Span::styled(model.as_str(), style));
        }

        if let Some(ref agent) = self.state.model_selector.current_agent {
            spans.push(Span::styled(" | ", Style::default().fg(theme.muted).bg(theme.panel_bg)));
            spans.push(Span::styled(
                format!("@{}", agent),
                Style::default().fg(theme.purple).bg(theme.panel_bg).bold(),
            ));
        }

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().bg(theme.panel_bg))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Render keyboard hints row
    fn render_hints(&self, frame: &mut Frame<'_>, area: Rect, theme: crate::theme::ThemePalette) {
        let hints = self.get_hints(theme);
        let mut spans = Vec::new();

        for (i, hint) in hints.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("   "));
            }
            spans.push(hint);
        }

        let paragraph = Paragraph::new(Line::from(spans))
            .block(Block::default().bg(theme.bg))
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    fn get_hints(&self, theme: crate::theme::ThemePalette) -> Vec<Span<'_>> {
        let mut hints = Vec::new();

        if self.state.ui.is_first_session && self.state.input.buffer.is_empty() {
            hints.push(Span::styled(
                "[Press Esc]",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            ));
            return hints;
        }

        if self.state.approval_ui.pending_approval.is_some() {
            hints.push(Span::styled(
                "[y] approve [n] reject [c] cancel",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            ));
        } else if self.state.is_generating() {
            hints.push(Span::styled(
                "[Ctrl+C: cancel]",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            ));
        } else {
            if self.state.input.message_history.len() > 1 {
                hints.push(Span::styled(
                    "[↑↓] history",
                    Style::default().fg(theme.muted).bg(theme.panel_bg),
                ));

                if let Some(position) = self.state.input.history_position() {
                    hints.push(Span::styled(
                        format!("[{}]", position),
                        Style::default().fg(theme.muted).bg(theme.panel_bg),
                    ));
                }
            }

            hints.push(Span::styled(
                "[Ctrl+Shift+G] editor",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            ));

            if self.state.ui.sidebar_visible {
                hints.push(Span::styled(
                    "[Ctrl+S] hide",
                    Style::default().fg(theme.muted).bg(theme.panel_bg),
                ));
            } else {
                hints.push(Span::styled(
                    "[Ctrl+S] show",
                    Style::default().fg(theme.muted).bg(theme.panel_bg),
                ));
            }

            hints.push(Span::styled(
                "[Ctrl+T] theme",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            ));
        }

        hints
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
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
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
        assert!(hints.iter().any(|s| s.content.contains("[Ctrl+S]")));
    }

    #[test]
    fn test_get_hints_generating_state() {
        let mut state = create_test_state();
        state.ui.set_first_session(false);
        state.start_generation();

        let _footer = Footer::new(&state);
        let theme = Theme::palette(state.theme_variant());
        let hints = _footer.get_hints(theme);
        assert!(hints.iter().any(|s| s.content.contains("Ctrl+C")));
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
        assert!(hints.iter().any(|s| s.content.contains("[y]")));
        assert!(hints.iter().any(|s| s.content.contains("[n]")));
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
