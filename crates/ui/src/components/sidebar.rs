use crate::{layout::TuiLayout, state::AppState, theme::Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
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
pub struct Sidebar<'a> {
    state: &'a AppState,
}

impl<'a> Sidebar<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render sidebar to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, _area: Rect) {
        let layout = TuiLayout::calculate(frame.area(), true);
        let Some((events_area, files_area, diff_area, lsp_area)) = layout.sidebar_sections() else {
            return;
        };

        self.render_session_events(frame, events_area);
        self.render_modified_files(frame, files_area);
        self.render_git_diff_queue(frame, diff_area);
        self.render_lsp_mcp_status(frame, lsp_area);
    }

    fn render_session_events(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();

        if self.state.session_events.is_empty() {
            lines.push(Line::from(Span::styled("No events", Style::default().fg(Theme::MUTED))));
        } else {
            for event in self.state.session_events.iter().take(3) {
                lines.push(Line::from(vec![
                    Span::styled(&event.event_type, Style::default().fg(Theme::BLUE)),
                    Span::raw(" "),
                    Span::styled(&event.message, Style::default().fg(Theme::FG)),
                ]));
            }
            if self.state.session_events.len() > 3 {
                lines.push(Line::from(Span::styled(
                    format!("+ {} more", self.state.session_events.len() - 3),
                    Style::default().fg(Theme::MUTED),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(Block::default().title("Events").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_modified_files(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();

        if self.state.modified_files.is_empty() {
            lines.push(Line::from(Span::styled(
                "No changes",
                Style::default().fg(Theme::MUTED),
            )));
        } else {
            for file in self.state.modified_files.iter().take(2) {
                let mod_color = match file.mod_type.as_str() {
                    "edited" => Theme::YELLOW,
                    "created" => Theme::GREEN,
                    "deleted" => Theme::RED,
                    _ => Theme::MUTED,
                };
                lines.push(Line::from(vec![
                    Span::styled(&file.mod_type, Style::default().fg(mod_color)),
                    Span::raw(" "),
                    Span::styled(&file.path, Style::default().fg(Theme::FG)),
                ]));
            }
            if self.state.modified_files.len() > 2 {
                lines.push(Line::from(Span::styled(
                    format!("+ {} more", self.state.modified_files.len() - 2),
                    Style::default().fg(Theme::MUTED),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(Block::default().title("Modified").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_git_diff_queue(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut lines = Vec::new();

        if self.state.git_diff_queue.is_empty() {
            lines.push(Line::from(Span::styled("No diffs", Style::default().fg(Theme::MUTED))));
        } else {
            for diff in self.state.git_diff_queue.iter().take(2) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("+{}/-{}", diff.added, diff.deleted),
                        Style::default().fg(Theme::YELLOW),
                    ),
                    Span::raw(" "),
                    Span::styled(&diff.path, Style::default().fg(Theme::FG)),
                ]));
            }
            if self.state.git_diff_queue.len() > 2 {
                lines.push(Line::from(Span::styled(
                    format!("+ {} more", self.state.git_diff_queue.len() - 2),
                    Style::default().fg(Theme::MUTED),
                )));
            }
        }

        let paragraph = Paragraph::new(lines)
            .block(Block::default().title("Diffs").borders(Borders::ALL))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    fn render_lsp_mcp_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let lines = vec![
            Line::from(vec![
                Span::styled("🔌", Style::default().fg(Theme::PURPLE)),
                Span::raw(" "),
                Span::styled("LSPs: Not connected", Style::default().fg(Theme::MUTED)),
            ]),
            Line::from(vec![
                Span::styled("🔗", Style::default().fg(Theme::CYAN)),
                Span::raw(" "),
                Span::styled("MCPs: Not connected", Style::default().fg(Theme::MUTED)),
            ]),
        ];

        let paragraph = Paragraph::new(lines)
            .block(Block::default().title("Integrations").borders(Borders::ALL))
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
        assert_eq!(sidebar.state.profile, "test");
    }

    #[test]
    fn test_sidebar_initial_state() {
        let state = create_test_state();
        let sidebar = Sidebar::new(&state);
        assert_eq!(sidebar.state.stats.input_tokens, 0);
        assert_eq!(sidebar.state.stats.output_tokens, 0);
        assert_eq!(sidebar.state.stats.approval_gates, 0);
        assert_eq!(sidebar.state.stats.tools_executed, 0);
    }

    #[test]
    fn test_sidebar_with_stats() {
        let mut state = create_test_state();
        state.stats.input_tokens = 100;
        state.stats.output_tokens = 200;
        state.stats.approval_gates = 5;
        state.stats.tools_executed = 10;

        let sidebar = Sidebar::new(&state);

        assert_eq!(sidebar.state.stats.input_tokens, 100);
        assert_eq!(sidebar.state.stats.output_tokens, 200);
        assert_eq!(sidebar.state.stats.total_tokens(), 300);
        assert_eq!(sidebar.state.stats.approval_gates, 5);
        assert_eq!(sidebar.state.stats.tools_executed, 10);
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
        assert_eq!(sidebar_auto.state.approval_mode, ApprovalMode::Auto);

        let state_full = AppState::new(
            cwd.clone(),
            "full".to_string(),
            provider.clone(),
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );
        let sidebar_full = Sidebar::new(&state_full);
        assert_eq!(sidebar_full.state.approval_mode, ApprovalMode::FullAccess);

        let state_readonly = AppState::new(
            cwd,
            "readonly".to_string(),
            provider,
            ApprovalMode::ReadOnly,
            SandboxMode::Policy,
        );
        let sidebar_readonly = Sidebar::new(&state_readonly);
        assert_eq!(sidebar_readonly.state.approval_mode, ApprovalMode::ReadOnly);
    }
}
