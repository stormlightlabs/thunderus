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
/// - Token usage counter
/// - Approval gates triggered count
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
        let Some((token_area, approval_area)) = layout.sidebar_sections() else {
            return;
        };

        let token_text = format!(
            "Tokens: {} in, {} out ({} total)",
            self.state.stats.input_tokens,
            self.state.stats.output_tokens,
            self.state.stats.total_tokens()
        );
        let token_paragraph = Paragraph::new(Line::from(vec![
            Span::styled("📊", Style::default().fg(Theme::BLUE)),
            Span::raw(" "),
            Span::styled(token_text, Style::default().fg(Theme::FG)),
        ]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });

        frame.render_widget(token_paragraph, token_area);

        let approval_text = format!(
            "Gates: {}, Tools: {}",
            self.state.stats.approval_gates, self.state.stats.tools_executed
        );
        let approval_paragraph = Paragraph::new(Line::from(vec![
            Span::styled("🔐", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled(approval_text, Style::default().fg(Theme::FG)),
        ]))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });

        frame.render_widget(approval_paragraph, approval_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig};

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

        let state_auto = AppState::new(cwd.clone(), "auto".to_string(), provider.clone(), ApprovalMode::Auto);
        let sidebar_auto = Sidebar::new(&state_auto);
        assert_eq!(sidebar_auto.state.approval_mode, ApprovalMode::Auto);

        let state_full = AppState::new(
            cwd.clone(),
            "full".to_string(),
            provider.clone(),
            ApprovalMode::FullAccess,
        );
        let sidebar_full = Sidebar::new(&state_full);
        assert_eq!(sidebar_full.state.approval_mode, ApprovalMode::FullAccess);

        let state_readonly = AppState::new(cwd, "readonly".to_string(), provider, ApprovalMode::ReadOnly);
        let sidebar_readonly = Sidebar::new(&state_readonly);
        assert_eq!(sidebar_readonly.state.approval_mode, ApprovalMode::ReadOnly);
    }
}
