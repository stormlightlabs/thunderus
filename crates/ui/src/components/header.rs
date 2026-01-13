use crate::{layout::header_sections, state::AppState, theme::Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Header component displaying session information
///
/// Shows (depending on terminal width):
/// - Current working directory
/// - Profile name
/// - Provider/model
/// - Approval mode
pub struct Header<'a> {
    state: &'a AppState,
}

impl<'a> Header<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render the header to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let (cwd_area, profile_area, provider_area, approval_area, git_area, sandbox_area, verbosity_area) =
            header_sections(area);

        if cwd_area.width > 0 {
            let cwd_span = Span::styled(self.cwd_display(), Style::default().fg(Theme::CYAN));
            let cwd = Paragraph::new(Line::from(cwd_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(cwd, cwd_area);
        }

        if profile_area.width > 0 {
            let profile_span = Span::styled(format!("@{}", self.state.profile), Style::default().fg(Theme::PURPLE));
            let profile = Paragraph::new(Line::from(profile_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(profile, profile_area);
        }

        if provider_area.width > 0 {
            let provider_text = format!("{}/{}", self.state.provider_name(), self.state.model_name());
            let provider_span = Span::styled(provider_text, Style::default().fg(Theme::BLUE));
            let provider = Paragraph::new(Line::from(provider_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(provider, provider_area);
        }

        if approval_area.width > 0 {
            let approval_span = Theme::approval_mode_span(self.state.approval_mode.as_str());
            let mode_label = Span::styled("[", Style::default().fg(Theme::MUTED));
            let mode_close = Span::styled("]", Style::default().fg(Theme::MUTED));
            let approval = Paragraph::new(Line::from(vec![mode_label, approval_span, mode_close]));
            frame.render_widget(approval, approval_area);
        }

        if git_area.width > 0 {
            let git_text = if let Some(ref branch) = self.state.git_branch {
                format!("🌿 {}", branch)
            } else {
                String::new()
            };
            let git_span = Span::styled(git_text, Style::default().fg(Theme::GREEN));
            let git = Paragraph::new(Line::from(git_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(git, git_area);
        }

        if sandbox_area.width > 0 {
            let sandbox_span = Theme::sandbox_mode_span(self.state.sandbox_mode.as_str());
            let sandbox_label = Span::styled("🔒", Style::default().fg(Theme::YELLOW));
            let sandbox = Paragraph::new(Line::from(vec![
                sandbox_label,
                Span::styled(" ", Style::default()),
                sandbox_span,
            ]));
            frame.render_widget(sandbox, sandbox_area);
        }

        if verbosity_area.width > 0 {
            let verbosity_span = Theme::verbosity_span(self.state.verbosity.as_str());
            let verbosity = Paragraph::new(Line::from(verbosity_span));
            frame.render_widget(verbosity, verbosity_area);
        }
    }

    /// Format cwd for display (truncate if too long)
    fn cwd_display(&self) -> String {
        let cwd = self.state.cwd.display().to_string();
        if cwd.len() > 20 {
            let start = cwd.len().saturating_sub(20);
            format!("...{}", &cwd[start..])
        } else {
            cwd
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use thunderus_core::{ApprovalMode, ProviderConfig, SandboxMode};

    fn create_test_state() -> AppState {
        AppState::new(
            PathBuf::from("/very/long/path/to/workspace"),
            "work-profile".to_string(),
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
    fn test_header_new() {
        let state = create_test_state();
        let header = Header::new(&state);
        assert_eq!(header.state.profile, "work-profile");
    }

    #[test]
    fn test_cwd_display_truncation() {
        let state = create_test_state();
        let header = Header::new(&state);
        let display = header.cwd_display();
        assert!(display.len() <= 23);
        assert!(display.starts_with("..."));
    }

    #[test]
    fn test_cwd_display_no_truncation() {
        let mut state = create_test_state();
        state.cwd = PathBuf::from("/workspace");
        let header = Header::new(&state);
        let display = header.cwd_display();

        assert_eq!(display, "/workspace");
        assert!(!display.starts_with("..."));
    }

    #[test]
    fn test_header_with_gemini() {
        let state = AppState::new(
            PathBuf::from("."),
            "default".to_string(),
            ProviderConfig::Gemini {
                api_key: "test".to_string(),
                model: "gemini-2.5-flash".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );

        let header = Header::new(&state);
        assert_eq!(header.state.provider_name(), "Gemini");
        assert_eq!(header.state.model_name(), "gemini-2.5-flash".to_string());
        assert_eq!(header.state.approval_mode, ApprovalMode::FullAccess);
    }

    #[test]
    fn test_header_approval_modes() {
        let cwd = PathBuf::from(".");
        let provider = ProviderConfig::Glm {
            api_key: "test".to_string(),
            model: "glm-4.7".to_string(),
            base_url: "https://api.example.com".to_string(),
        };

        let state_readonly = AppState::new(
            cwd.clone(),
            "test".to_string(),
            provider.clone(),
            ApprovalMode::ReadOnly,
            SandboxMode::Policy,
        );
        let header_readonly = Header::new(&state_readonly);
        assert_eq!(header_readonly.state.approval_mode.as_str(), "read-only");

        let state_auto = AppState::new(
            cwd.clone(),
            "test".to_string(),
            provider.clone(),
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );
        let header_auto = Header::new(&state_auto);
        assert_eq!(header_auto.state.approval_mode.as_str(), "auto");

        let state_full = AppState::new(
            cwd.clone(),
            "test".to_string(),
            provider,
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );
        let header_full = Header::new(&state_full);
        assert_eq!(header_full.state.approval_mode.as_str(), "full-access");
    }
}
