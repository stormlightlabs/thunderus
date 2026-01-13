use crate::{layout::TuiLayout, state::AppState, theme::Theme};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

/// Footer component displaying input composer and hints
///
/// Shows:
/// - Single-line input composer
/// - Hints for available keys
pub struct Footer<'a> {
    state: &'a AppState,
}

impl<'a> Footer<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render footer to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let layout = TuiLayout::calculate(area, self.state.sidebar_visible);
        let input_area = layout.footer_input();
        let hints_area = layout.footer_hints();

        let input_text = if self.state.input.buffer.is_empty() {
            "Type a message...".to_string()
        } else {
            self.state.input.buffer.clone()
        };

        let input_style = if self.state.input.buffer.is_empty() {
            Style::default().fg(Theme::MUTED)
        } else {
            Style::default().fg(Theme::FG)
        };

        let mut input_spans = vec![Span::styled("> ", Style::default().fg(Theme::BLUE))];

        if self.state.input.buffer.is_empty() {
            input_spans.push(Span::styled(input_text, input_style));
            input_spans.push(Span::styled("█", Style::default().bg(Theme::FG).fg(Theme::FG)));
        } else {
            let cursor_pos = self.state.input.cursor.min(self.state.input.buffer.len());
            let before_cursor = &self.state.input.buffer[..cursor_pos];
            let after_cursor = &self.state.input.buffer[cursor_pos..];

            if !before_cursor.is_empty() {
                input_spans.push(Span::styled(before_cursor.to_string(), input_style));
            }

            input_spans.push(Span::styled("█", Style::default().bg(Theme::FG).fg(Theme::FG)));

            if !after_cursor.is_empty() {
                input_spans.push(Span::styled(after_cursor.to_string(), input_style));
            }
        }

        let input_paragraph = Paragraph::new(Line::from(input_spans)).block(Block::default().borders(Borders::ALL));

        frame.render_widget(input_paragraph, input_area);

        let hints = self.get_hints();
        let hints_paragraph = Paragraph::new(Line::from(hints)).block(Block::default().borders(Borders::ALL));

        frame.render_widget(hints_paragraph, hints_area);
    }

    fn get_hints(&self) -> Vec<Span<'_>> {
        let mut hints = Vec::new();

        if self.state.pending_approval.is_some() {
            hints.push(Span::styled("[y]", Style::default().fg(Theme::GREEN).bold()));
            hints.push(Span::raw(" approve "));
            hints.push(Span::styled("[n]", Style::default().fg(Theme::RED).bold()));
            hints.push(Span::raw(" reject "));
            hints.push(Span::styled("[c]", Style::default().fg(Theme::YELLOW).bold()));
            hints.push(Span::raw(" cancel "));
        } else if self.state.is_generating() {
            hints.push(Span::styled("Esc", Style::default().fg(Theme::RED)));
            hints.push(Span::raw(": cancel "));
        } else {
            hints.push(Span::styled("Enter", Style::default().fg(Theme::GREEN)));
            hints.push(Span::raw(": send "));

            if self.state.sidebar_visible {
                hints.push(Span::styled("Ctrl+S", Style::default().fg(Theme::BLUE)));
                hints.push(Span::raw(": hide "));
            } else {
                hints.push(Span::styled("Ctrl+S", Style::default().fg(Theme::BLUE)));
                hints.push(Span::raw(": show "));
            }
        }

        hints
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
    fn test_footer_new() {
        let state = create_test_state();
        let footer = Footer::new(&state);
        assert_eq!(footer.state.profile, "test");
    }

    #[test]
    fn test_get_hints_normal_state() {
        let state = create_test_state();
        let _footer = Footer::new(&state);

        let hints = _footer.get_hints();
        assert!(hints.iter().any(|s| s.content.contains("Enter")));
        assert!(hints.iter().any(|s| s.content.contains("Ctrl+S")));
    }

    #[test]
    fn test_get_hints_generating_state() {
        let mut state = create_test_state();
        state.start_generation();

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();
        assert!(hints.iter().any(|s| s.content.contains("Esc")));
        assert!(hints.iter().any(|s| s.content.contains("cancel")));
    }

    #[test]
    fn test_get_hints_sidebar_visible() {
        let mut state = create_test_state();
        state.sidebar_visible = true;

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();
        assert!(hints.iter().any(|s| s.content.contains("hide")));
    }

    #[test]
    fn test_get_hints_sidebar_hidden() {
        let mut state = create_test_state();
        state.sidebar_visible = false;

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();
        assert!(hints.iter().any(|s| s.content.contains("show")));
    }

    #[test]
    fn test_get_hints_with_input() {
        let mut state = create_test_state();
        state.input.buffer = "Hello".to_string();

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();
        assert!(!hints.is_empty());
    }

    #[test]
    fn test_render_input_empty() {
        let state = create_test_state();
        let _footer = Footer::new(&state);

        let input_text = if state.input.buffer.is_empty() { "Type a message..." } else { &state.input.buffer };

        assert_eq!(input_text, "Type a message...");
    }

    #[test]
    fn test_render_input_with_content() {
        let mut state = create_test_state();
        state.input.buffer = "Test message".to_string();

        let _footer = Footer::new(&state);

        assert_eq!(state.input.buffer, "Test message");
    }

    #[test]
    fn test_input_state_default() {
        let state = create_test_state();

        assert_eq!(state.input.buffer, "");
        assert_eq!(state.input.cursor, 0);
    }

    #[test]
    fn test_get_hints_with_pending_approval() {
        let mut state = create_test_state();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "risky".to_string(),
        ));

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();

        assert!(hints.iter().any(|s| s.content.contains("[y]")));
        assert!(hints.iter().any(|s| s.content.contains("[n]")));
        assert!(hints.iter().any(|s| s.content.contains("[c]")));
        assert!(hints.iter().any(|s| s.content.contains("approve")));
    }

    #[test]
    fn test_get_hints_normal_without_approval() {
        let state = create_test_state();

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();

        assert!(hints.iter().any(|s| s.content.contains("Enter")));
        assert!(!hints.iter().any(|s| s.content.contains("[y]")));
    }

    #[test]
    fn test_get_hints_approval_overrides_generation() {
        let mut state = create_test_state();
        state.start_generation();
        state.pending_approval = Some(crate::state::ApprovalState::pending(
            "test.action".to_string(),
            "safe".to_string(),
        ));

        let _footer = Footer::new(&state);
        let hints = _footer.get_hints();

        assert!(hints.iter().any(|s| s.content.contains("[y]")));
        assert!(!hints.iter().any(|s| s.content.contains("Esc")));
    }
}
