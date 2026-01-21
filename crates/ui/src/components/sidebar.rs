use crate::{
    components::DiffView,
    layout,
    state::{AppState, SidebarSection},
    theme::Theme,
};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Sidebar component displaying session statistics
///
/// Shows:
/// - Session events (chronological list)
/// - Modified files list
/// - Git diff queue preview
/// - LSPs & MCPs status
///
/// TODO: Collapsible sidebar sections:
/// - Individual section collapse (Events, Modified, Diffs, Integrations)
/// - Select-based collapse (navigate to section, then collapse)
/// - Use [ and ] keys for section-level control
pub struct Sidebar<'a> {
    state: &'a AppState,
}

impl<'a> Sidebar<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render sidebar to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, _: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let layout = layout::TuiLayout::calculate(
            frame.area(),
            self.state.ui.sidebar_visible,
            self.state.ui.sidebar_width_override(),
        );
        let Some(sidebar_area) = layout.sidebar else {
            return;
        };

        frame.render_widget(Block::default().bg(theme.panel_bg), sidebar_area);

        let Some(sidebar) = layout.sidebar_sections() else {
            return;
        };

        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::TokenUsage)
        {
            self.render_token_usage(frame, sidebar.token_usage);
        }
        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Events)
        {
            self.render_session_events(frame, sidebar.session_events);
        }
        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Modified)
        {
            self.render_modified_files(frame, sidebar.modified_files);
        }
        if !self.state.ui.sidebar_collapse_state.is_collapsed(SidebarSection::Diffs) {
            self.render_git_diff_queue(frame, sidebar.git_diff);
        }
        if !self
            .state
            .ui
            .sidebar_collapse_state
            .is_collapsed(SidebarSection::Integrations)
        {
            self.render_lsp_mcp_status(frame, sidebar.lsp_mcp_status);
        }
    }

    /// Note: Sidebar auto-hide on narrow terminals is handled by [TuiLayout::calculate]
    /// when [layout::LayoutMode] is Medium (80-99 cols) or Compact (< 80 cols).
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

        let lines = vec![Line::from(vec![
            Span::styled(in_display, Style::default().fg(theme.green).bg(theme.panel_bg)),
            Span::styled(" / ", Style::default().fg(theme.muted).bg(theme.panel_bg)),
            Span::styled(out_display, Style::default().fg(theme.cyan).bg(theme.panel_bg)),
        ])];

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Tokens", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_session_events(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();
        let theme = Theme::palette(self.state.theme_variant());

        if self.state.session.session_events.is_empty() {
            lines.push(Line::from(Span::styled(
                "No events",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            )));
        } else {
            for event in self.state.session.session_events.iter().take(3) {
                lines.push(Line::from(vec![
                    Span::styled(&event.event_type, Style::default().fg(theme.blue).bg(theme.panel_bg)),
                    Span::raw(" "),
                    Span::styled(&event.message, Style::default().fg(theme.fg).bg(theme.panel_bg)),
                ]));
            }
            if self.state.session.session_events.len() > 3 {
                lines.push(Line::from(Span::styled(
                    format!("+ {} more", self.state.session.session_events.len() - 3),
                    Style::default().fg(theme.muted).bg(theme.panel_bg),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Events", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_modified_files(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();
        let theme = Theme::palette(self.state.theme_variant());

        if self.state.session.modified_files.is_empty() {
            lines.push(Line::from(Span::styled(
                "No changes",
                Style::default().fg(theme.muted).bg(theme.panel_bg),
            )));
        } else {
            for file in self.state.session.modified_files.iter().take(2) {
                let mod_color = match file.mod_type.as_str() {
                    "edited" => theme.yellow,
                    "created" => theme.green,
                    "deleted" => theme.red,
                    _ => theme.muted,
                };
                lines.push(Line::from(vec![
                    Span::styled(&file.mod_type, Style::default().fg(mod_color).bg(theme.panel_bg)),
                    Span::raw(" "),
                    Span::styled(&file.path, Style::default().fg(theme.fg).bg(theme.panel_bg)),
                ]));
            }
            if self.state.session.modified_files.len() > 2 {
                lines.push(Line::from(Span::styled(
                    format!("+ {} more", self.state.session.modified_files.len() - 2),
                    Style::default().fg(theme.muted).bg(theme.panel_bg),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Modified", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_git_diff_queue(&self, frame: &mut Frame<'_>, area: Rect) {
        let diff_view = DiffView::new(self.state, self.state.patches());
        diff_view.render(frame, area);
    }

    fn render_lsp_mcp_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let lines = vec![
            Line::from(vec![
                Span::styled("LSP: ", Style::default().fg(theme.purple).bg(theme.panel_bg)),
                Span::styled("Not connected", Style::default().fg(theme.muted).bg(theme.panel_bg)),
            ]),
            Line::from(vec![
                Span::styled("MCP: ", Style::default().fg(theme.cyan).bg(theme.panel_bg)),
                Span::styled("Not connected", Style::default().fg(theme.muted).bg(theme.panel_bg)),
            ]),
        ];

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled("Integrations", Style::default().fg(theme.blue).bold()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .wrap(Wrap { trim: true });
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
            },
            ApprovalMode::Auto,
            SandboxMode::Policy,
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
        assert_eq!(sidebar.state.session.stats.approval_gates, 0);
        assert_eq!(sidebar.state.session.stats.tools_executed, 0);
    }

    #[test]
    fn test_sidebar_with_stats() {
        let mut state = create_test_state();
        state.session.stats.input_tokens = 100;
        state.session.stats.output_tokens = 200;
        state.session.stats.approval_gates = 5;
        state.session.stats.tools_executed = 10;

        let sidebar = Sidebar::new(&state);
        assert_eq!(sidebar.state.session.stats.input_tokens, 100);
        assert_eq!(sidebar.state.session.stats.output_tokens, 200);
        assert_eq!(sidebar.state.session.stats.total_tokens(), 300);
        assert_eq!(sidebar.state.session.stats.approval_gates, 5);
        assert_eq!(sidebar.state.session.stats.tools_executed, 10);
    }

    #[test]
    fn test_sidebar_with_different_modes() {
        let cwd = PathBuf::from(".");
        let provider = ProviderConfig::Glm {
            api_key: "test".to_string(),
            model: "glm-4.7".to_string(),
            base_url: "https://api.example.com".to_string(),
        };

        let state_auto = AppState::new(
            cwd.clone(),
            "auto".to_string(),
            provider.clone(),
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );
        let sidebar_auto = Sidebar::new(&state_auto);
        assert_eq!(sidebar_auto.state.config.approval_mode, ApprovalMode::Auto);

        let state_full = AppState::new(
            cwd.clone(),
            "full".to_string(),
            provider.clone(),
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );
        let sidebar_full = Sidebar::new(&state_full);
        assert_eq!(sidebar_full.state.config.approval_mode, ApprovalMode::FullAccess);

        let state_readonly = AppState::new(
            cwd,
            "readonly".to_string(),
            provider,
            ApprovalMode::ReadOnly,
            SandboxMode::Policy,
        );
        let sidebar_readonly = Sidebar::new(&state_readonly);
        assert_eq!(sidebar_readonly.state.config.approval_mode, ApprovalMode::ReadOnly);
    }
}
