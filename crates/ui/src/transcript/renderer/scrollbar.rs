use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

impl<'a> super::TranscriptRenderer<'a> {
    /// Render scrollbar indicator on right edge of transcript
    #[allow(dead_code)]
    pub(super) fn render_scrollbar(&self, frame: &mut Frame<'_>, area: Rect, content_line_count: usize) {
        if area.height <= 1 || content_line_count == 0 {
            return;
        }

        let visible_height = area.height as usize;
        let content_height = content_line_count;

        if content_height <= visible_height {
            return;
        }

        let scroll_ratio = self.scroll_vertical as f64 / (content_height - visible_height) as f64;
        let thumb_size = ((visible_height as f64) / (content_height as f64) * visible_height as f64).ceil() as u16;
        let thumb_position = (scroll_ratio * (visible_height - thumb_size as usize) as f64).ceil() as u16;

        let scrollbar_x = area.x + area.width.saturating_sub(1);
        let scrollbar_top = area.top();
        let scrollbar_height = area.height;

        for y in 0..scrollbar_height {
            let is_thumb = y >= thumb_position && y < thumb_position + thumb_size;
            let symbol = "|";
            let style = if is_thumb {
                Style::default().fg(self.theme.blue).bg(self.theme.bg)
            } else {
                Style::default().fg(self.theme.border).bg(self.theme.bg)
            };

            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(symbol, style)])),
                Rect::new(scrollbar_x, scrollbar_top + y, 1, 1),
            );
        }
    }
}
