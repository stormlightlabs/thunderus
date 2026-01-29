use crate::{
    state::HeaderState,
    theme::{Theme, ThemeVariant},
};
use unicode_width::UnicodeWidthStr;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};

/// Minimal session header displaying task title and usage statistics
///
/// - Left: # Task title (from first user message)
/// - Right: tokens % ($cost) version
pub struct Header<'a> {
    state: &'a HeaderState,
    theme_variant: ThemeVariant,
}

impl<'a> Header<'a> {
    /// Create a new session header
    pub fn new(state: &'a HeaderState) -> Self {
        Self { state, theme_variant: ThemeVariant::Iceberg }
    }

    /// Create a session header with a specific theme
    pub fn with_theme(state: &'a HeaderState, theme_variant: ThemeVariant) -> Self {
        Self { state, theme_variant }
    }

    /// Render the session header to the given frame with horizontal padding
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = Theme::palette(self.theme_variant);
        let h_padding: u16 = 2;

        let padded_area = Rect {
            x: area.x + h_padding,
            y: area.y,
            width: area.width.saturating_sub(h_padding * 2),
            height: area.height,
        };

        let task_title = self
            .state
            .task_title
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "New Session".to_string());

        let title_spans = vec![Span::styled(task_title, Style::default().fg(theme.fg).bg(theme.bg))];

        let tokens = self.state.tokens_display();
        let percent = self.state.context_percentage();
        let cost = self.state.cost_display();
        let version = env!("CARGO_PKG_VERSION");

        let stats_spans = vec![
            Span::styled(format!("{} ", tokens), Style::default().fg(theme.fg).bg(theme.bg)),
            Span::styled(format!("{}%", percent), Style::default().fg(theme.cyan).bg(theme.bg)),
            Span::styled(format!(" ({})", cost), Style::default().fg(theme.muted).bg(theme.bg)),
            Span::styled(format!(" v{}", version), Style::default().fg(theme.muted).bg(theme.bg)),
        ];

        let title_width = title_spans.iter().map(|s| s.content.width()).sum::<usize>();
        let stats_width = stats_spans.iter().map(|s| s.content.width()).sum::<usize>();
        let spacing = padded_area.width.saturating_sub((title_width + stats_width) as u16);

        let mut all_spans = title_spans;
        if spacing > 0 {
            all_spans.push(Span::styled(
                " ".repeat(spacing as usize),
                Style::default().bg(theme.bg),
            ));
        }
        all_spans.extend(stats_spans);

        let header = Paragraph::new(Line::from(all_spans)).block(ratatui::widgets::Block::default().bg(theme.bg));

        frame.render_widget(header, padded_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_header_new() {
        let state = HeaderState::new();
        let header = Header::new(&state);
        assert!(header.state.task_title.is_none());
    }

    #[test]
    fn test_session_header_with_title() {
        let mut state = HeaderState::new();
        state.set_task_title_from_message("Fix the login bug");
        let header = Header::new(&state);
        assert_eq!(header.state.task_title, Some("Fix the login bug".to_string()));
    }

    #[test]
    fn test_session_header_with_tokens() {
        let mut state = HeaderState::new();
        state.update_tokens(14295);
        state.update_cost(0.05);
        let header = Header::new(&state);
        assert_eq!(header.state.tokens_used, 14295);
        assert_eq!(header.state.tokens_display(), "14.3k");
        assert_eq!(header.state.cost_display(), "$0.05");
    }
}
