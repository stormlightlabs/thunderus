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

        let input_paragraph = Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Theme::BLUE)),
            Span::styled(input_text, input_style),
        ]))
        .block(Block::default().borders(Borders::ALL));

        frame.render_widget(input_paragraph, input_area);

        let hints = self.get_hints();
        let hints_paragraph = Paragraph::new(Line::from(hints)).block(Block::default().borders(Borders::ALL));

        frame.render_widget(hints_paragraph, hints_area);
    }

    fn get_hints(&self) -> Vec<Span<'_>> {
        let mut hints = Vec::new();

        if self.state.is_generating() {
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
}
