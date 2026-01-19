use crate::{layout::HeaderSections, state::AppState, theme::Theme};

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
        let sections = HeaderSections::new(area);

        if sections.cwd.width > 0 {
            let cwd_span = Span::styled(self.cwd_display(), Style::default().fg(Theme::CYAN));
            let cwd = Paragraph::new(Line::from(cwd_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(cwd, sections.cwd);
        }

        if sections.profile.width > 0 {
            let profile_span = Span::styled(
                format!("@{}", self.state.config.profile),
                Style::default().fg(Theme::PURPLE),
            );
            let profile = Paragraph::new(Line::from(profile_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(profile, sections.profile);
        }

        if sections.provider.width > 0 {
            let provider_text = format!("{}/{}", self.state.provider_name(), self.state.model_name());
            let provider_span = Span::styled(provider_text, Style::default().fg(Theme::BLUE));
            let provider = Paragraph::new(Line::from(provider_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(provider, sections.provider);
        }

        if sections.approval.width > 0 {
            let approval_span = Theme::approval_mode_span(self.state.config.approval_mode.as_str());
            let mode_label = Span::styled("[", Style::default().fg(Theme::MUTED));
            let mode_close = Span::styled("]", Style::default().fg(Theme::MUTED));
            let approval = Paragraph::new(Line::from(vec![mode_label, approval_span, mode_close]));
            frame.render_widget(approval, sections.approval);
        }

        if sections.git.width > 0 {
            let git_text = if let Some(ref branch) = self.state.config.git_branch {
                format!("🌿 {}", branch)
            } else {
                String::new()
            };
            let git_span = Span::styled(git_text, Style::default().fg(Theme::GREEN));
            let git = Paragraph::new(Line::from(git_span)).block(Block::default().borders(Borders::RIGHT));
            frame.render_widget(git, sections.git);
        }

        if sections.sandbox.width > 0 {
            let sandbox_span = Theme::sandbox_mode_span(self.state.config.sandbox_mode.as_str());
            let sandbox_label = Span::styled("🔒", Style::default().fg(Theme::YELLOW));
            let sandbox = Paragraph::new(Line::from(vec![
                sandbox_label,
                Span::styled(" ", Style::default()),
                sandbox_span,
            ]));
            frame.render_widget(sandbox, sections.sandbox);
        }

        if sections.network.width > 0 {
            let network_span = if self.state.config.allow_network {
                Span::styled("ON", Style::default().fg(Theme::GREEN))
            } else {
                Span::styled("OFF", Style::default().fg(Theme::MUTED))
            };
            let network_label = Span::styled("Net:", Style::default().fg(Theme::MUTED));
            let network = Paragraph::new(Line::from(vec![
                network_label,
                Span::styled(" ", Style::default()),
                network_span,
            ]));
            frame.render_widget(network, sections.network);
        }

        if sections.verbosity.width > 0 {
            let verbosity_span = Theme::verbosity_span(self.state.config.verbosity.as_str());
            let verbosity = Paragraph::new(Line::from(verbosity_span));
            frame.render_widget(verbosity, sections.verbosity);
        }
    }

    /// Format cwd for display (truncate if too long)
    fn cwd_display(&self) -> String {
        let cwd = self.state.config.cwd.display().to_string();
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
        assert_eq!(header.state.config.profile, "work-profile");
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
        state.config.cwd = PathBuf::from("/workspace");
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
        assert_eq!(header.state.config.approval_mode, ApprovalMode::FullAccess);
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
        assert_eq!(header_readonly.state.config.approval_mode.as_str(), "read-only");

        let state_auto = AppState::new(
            cwd.clone(),
            "test".to_string(),
            provider.clone(),
            ApprovalMode::Auto,
            SandboxMode::Policy,
        );
        let header_auto = Header::new(&state_auto);
        assert_eq!(header_auto.state.config.approval_mode.as_str(), "auto");

        let state_full = AppState::new(
            cwd.clone(),
            "test".to_string(),
            provider,
            ApprovalMode::FullAccess,
            SandboxMode::Policy,
        );
        let header_full = Header::new(&state_full);
        assert_eq!(header_full.state.config.approval_mode.as_str(), "full-access");
    }
}
