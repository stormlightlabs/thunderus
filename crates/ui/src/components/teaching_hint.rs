//! Teaching hint popup component
//!
//! Displays a one-time hint when a concept is encountered for the first time.
//! These hints are educational and help users understand the system's behavior.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

/// Teaching hint popup for one-time educational messages
pub struct TeachingHintPopup<'a> {
    /// The hint message to display
    pub hint: &'a str,
    /// Optional title for the hint (defaults to "💡 First Time")
    pub title: Option<&'a str>,
}

impl<'a> TeachingHintPopup<'a> {
    /// Create a new teaching hint popup
    pub fn new(hint: &'a str) -> Self {
        Self { hint, title: None }
    }

    /// Set a custom title for the hint
    pub fn with_title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    /// Calculate the popup area centered in the terminal
    fn popup_area(&self, area: Rect) -> Rect {
        let max_width = 70;
        let max_height = 8;

        let text_width = self.hint.len().min(max_width - 4) as u16;
        let width = (max_width as u16).min(text_width + 8);

        let height_lines = (self.hint.len() as f64 / (width - 4) as f64).ceil() as u16;
        let height = (max_height as u16).min(height_lines + 4);

        let x = area.x.saturating_add((area.width.saturating_sub(width)) / 2);
        let y = area.y.saturating_add((area.height.saturating_sub(height)) / 2);

        Rect::new(x, y, width, height)
    }

    /// Render the teaching hint popup
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let popup_area = self.popup_area(area);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(self.title.unwrap_or("💡 First Time"))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .padding(Padding::new(1, 1, 1, 1));

        frame.render_widget(
            Paragraph::new(self.hint)
                .block(block)
                .wrap(Wrap { trim: true })
                .alignment(Alignment::Left),
            popup_area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teaching_hint_new() {
        let hint = TeachingHintPopup::new("Test hint");
        assert_eq!(hint.hint, "Test hint");
        assert!(hint.title.is_none());
    }

    #[test]
    fn test_teaching_hint_with_title() {
        let hint = TeachingHintPopup::new("Test hint").with_title("Custom Title");
        assert_eq!(hint.hint, "Test hint");
        assert_eq!(hint.title, Some("Custom Title"));
    }

    #[test]
    fn test_teaching_hint_default_title() {
        assert_eq!(TeachingHintPopup::new("Test").title, None);
    }
}
