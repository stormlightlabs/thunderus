mod cards;
mod code;
mod context;
mod entry;
mod messages;
mod scrollbar;
mod wrap;

use crate::{theme::ThemePalette, transcript::Transcript};

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    pub centered: bool,
    pub max_bubble_width: Option<usize>,
    pub animation_frame: u8,
}

/// Renders transcript entries to frame
pub struct TranscriptRenderer<'a> {
    transcript: &'a Transcript,
    scroll_vertical: u16,
    streaming_ellipsis: &'a str,
    theme: ThemePalette,
    options: RenderOptions,
}

impl<'a> TranscriptRenderer<'a> {
    /// Create a new renderer for given transcript
    pub fn new(transcript: &'a Transcript, theme: ThemePalette) -> Self {
        Self { transcript, scroll_vertical: 0, streaming_ellipsis: "", theme, options: RenderOptions::default() }
    }

    /// Create a new renderer with scroll offset
    pub fn with_vertical_scroll(
        transcript: &'a Transcript, scroll: u16, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        Self { transcript, scroll_vertical: scroll, streaming_ellipsis: "", theme, options }
    }

    /// Create a new renderer with streaming ellipsis animation
    pub fn with_streaming_ellipsis(
        transcript: &'a Transcript, scroll: u16, ellipsis: &'a str, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        Self { transcript, scroll_vertical: scroll, streaming_ellipsis: ellipsis, theme, options }
    }

    /// Render transcript to the given area with scrollbar indicator
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let entries = self.transcript.render_entries();
        let mut text_lines = Vec::new();
        let padding_x = 1usize;
        let padding_y = 0usize;
        let scrollbar_width = 1usize;
        let content_width = area.width.saturating_sub((padding_x * 2 + scrollbar_width) as u16) as usize;

        for (idx, entry) in entries.iter().enumerate() {
            if idx > 0 {
                text_lines.push(Line::default());
            }
            self.render_entry(entry, content_width, self.streaming_ellipsis, &mut text_lines);
        }

        let mut padded_lines = Vec::new();
        let left_pad = Span::styled(" ", Style::default().bg(self.theme.bg));
        let right_pad = Span::styled("  ", Style::default().bg(self.theme.bg));

        for _ in 0..padding_y {
            padded_lines.push(Line::from(vec![left_pad.clone()]));
        }

        for line in text_lines {
            let mut spans = Vec::new();
            spans.push(left_pad.clone());
            spans.extend(line.spans);
            spans.push(right_pad.clone());
            padded_lines.push(Line::from(spans));
        }

        for _ in 0..padding_y {
            padded_lines.push(Line::from(vec![left_pad.clone()]));
        }

        frame.render_widget(Block::default().style(Style::default().bg(self.theme.bg)), area);

        let paragraph = Paragraph::new(Text::from(padded_lines))
            .wrap(Wrap { trim: true })
            .scroll((0, self.scroll_vertical));

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Theme, ThemeVariant, transcript::Transcript};

    #[test]
    fn test_renderer_new() {
        let transcript = Transcript::new();
        let theme = Theme::palette(ThemeVariant::Iceberg);
        let _ = TranscriptRenderer::new(&transcript, theme);
    }

    #[test]
    fn test_renderer_with_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi there");
        let theme = Theme::palette(ThemeVariant::Iceberg);
        let _ = TranscriptRenderer::new(&transcript, theme);
    }
}
