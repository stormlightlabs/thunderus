use crate::{
    components::DiffView,
    state::{AppState, SidebarSection},
    theme::Theme,
};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

/// Sidebar component displaying session statistics
///
/// Clean, minimal design:
/// - No heavy borders around sections
/// - Section headers with subtle underlines
/// - Empty sections are hidden
/// - Compact vertical spacing
pub struct Sidebar<'a> {
    state: &'a AppState,
}

impl<'a> Sidebar<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render sidebar to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let sidebar_area = area;

        frame.render_widget(Block::default().bg(theme.bg), sidebar_area);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(5),
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Length(4),
                Constraint::Length(5),
                Constraint::Min(0),
            ])
            .split(sidebar_area);

        let pad_left = 3;
        let pad_right = 1;
        let pad_rect = |area: Rect| Rect {
            x: area.x.saturating_add(pad_left),
            y: area.y,
            width: area.width.saturating_sub(pad_left + pad_right),
            height: area.height,
        };

        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::TokenUsage)
        {
            self.render_token_usage(frame, pad_rect(sections[0]));
        }

        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Events)
            && !self.state.session.session_events.is_empty()
        {
            self.render_session_events(frame, pad_rect(sections[1]));
        }

        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Modified)
            && !self.state.session.modified_files.is_empty()
        {
            self.render_modified_files(frame, pad_rect(sections[2]));
        }

        if !self.state.ui.sidebar_collapse_state.is_collapsed(SidebarSection::Diffs) {
            self.render_git_diff_queue(frame, pad_rect(sections[3]));
        }

        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Context)
        {
            self.render_context(frame, pad_rect(sections[5]));
        }
    }

    fn render_token_usage(&self, frame: &mut Frame<'_>, area: Rect) {
        let stats = &self.state.session.stats;
        let theme = Theme::palette(self.state.theme_variant());

        let in_display = if stats.input_tokens >= 1000 {
            format!("{:.1}k", stats.input_tokens as f64 / 1000.0)
        } else {
            format!("{}", stats.input_tokens)
        };

        let out_display = if stats.output_tokens >= 1000 {
            format!("{:.1}k", stats.output_tokens as f64 / 1000.0)
        } else {
            format!("{}", stats.output_tokens)
        };

        let lines = vec![
            Line::from(vec![Span::styled(" Tokens", Style::default().fg(theme.muted))]),
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(in_display, Style::default().fg(theme.green)),
                Span::styled(" / ", Style::default().fg(theme.muted)),
                Span::styled(out_display, Style::default().fg(theme.cyan)),
            ]),
            Line::default(),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_session_events(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.state.session.session_events.is_empty() {
            return;
        }

        let theme = Theme::palette(self.state.theme_variant());
        let mut lines = Vec::new();

        lines.push(Line::from(vec![Span::styled(
            " Events",
            Style::default().fg(theme.muted),
        )]));

        for event in self.state.session.session_events.iter().take(2) {
            let msg = if event.message.len() > 15 {
                format!("{}...", &event.message[..12])
            } else {
                event.message.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(&event.event_type, Style::default().fg(theme.blue)),
                Span::styled(" ", Style::default()),
                Span::styled(msg, Style::default().fg(theme.fg)),
            ]));
        }

        if self.state.session.session_events.len() > 2 {
            lines.push(Line::from(Span::styled(
                format!(" +{}", self.state.session.session_events.len() - 2),
                Style::default().fg(theme.muted),
            )));
        }
        lines.push(Line::default());

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_modified_files(&self, frame: &mut Frame<'_>, area: Rect) {
        if self.state.session.modified_files.is_empty() {
            return;
        }

        let theme = Theme::palette(self.state.theme_variant());
        let mut lines = Vec::new();

        lines.push(Line::from(vec![Span::styled(
            " Modified",
            Style::default().fg(theme.muted),
        )]));

        for file in self.state.session.modified_files.iter().take(2) {
            let mod_color = match file.mod_type.as_str() {
                "edited" => theme.yellow,
                "created" => theme.green,
                "deleted" => theme.red,
                _ => theme.muted,
            };

            let filename = file.path.split('/').next_back().unwrap_or(&file.path);
            let display = if filename.len() > 14 { format!("{}...", &filename[..11]) } else { filename.to_string() };
            let mod_char = file.mod_type.chars().next().unwrap_or('?').to_uppercase().to_string();
            lines.push(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(mod_char, Style::default().fg(mod_color)),
                Span::styled(" ", Style::default()),
                Span::styled(display, Style::default().fg(theme.fg)),
            ]));
        }

        if self.state.session.modified_files.len() > 2 {
            lines.push(Line::from(Span::styled(
                format!(" +{}", self.state.session.modified_files.len() - 2),
                Style::default().fg(theme.muted),
            )));
        }
        lines.push(Line::default());

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_git_diff_queue(&self, frame: &mut Frame<'_>, area: Rect) {
        let diff_view = DiffView::with_memory_patches(self.state, self.state.patches(), self.state.memory_patches());
        diff_view.render(frame, area);
    }

    fn render_context(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let lines = vec![
            Line::from(vec![Span::styled(" Context", Style::default().fg(theme.muted))]),
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled("M", Style::default().fg(theme.green)),
                Span::styled(" memory  ", Style::default().fg(theme.fg)),
                Span::styled("P", Style::default().fg(theme.cyan)),
                Span::styled(" plan", Style::default().fg(theme.fg)),
            ]),
            Line::default(),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    #[allow(dead_code)]
    fn render_lsp_mcp_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let lines = vec![
            Line::from(vec![Span::styled(" Integrations", Style::default().fg(theme.muted))]),
            Line::from(vec![
                Span::styled(" LSP ", Style::default().fg(theme.purple)),
                Span::styled("-", Style::default().fg(theme.muted)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    #[allow(dead_code)]
    fn render_files(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let lines = vec![
            Line::from(vec![Span::styled(" Files", Style::default().fg(theme.muted))]),
            Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled("crates/", Style::default().fg(theme.fg)),
            ]),
        ];

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_sidebar_new() {
        let state = create_test_state();
        let sidebar = Sidebar::new(&state);
        assert_eq!(sidebar.state.config.profile, "test");
    }

    #[test]
    fn test_sidebar_initial_state() {
        let state = create_test_state();
        let sidebar = Sidebar::new(&state);
        assert_eq!(sidebar.state.session.stats.input_tokens, 0);
        assert_eq!(sidebar.state.session.stats.output_tokens, 0);
    }

    #[test]
    fn test_sidebar_with_stats() {
        let mut state = create_test_state();
        state.session.stats.input_tokens = 100;
        state.session.stats.output_tokens = 200;

        let sidebar = Sidebar::new(&state);
        assert_eq!(sidebar.state.session.stats.input_tokens, 100);
        assert_eq!(sidebar.state.session.stats.output_tokens, 200);
    }
}
