use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Renders transcript entries to frame
pub struct TranscriptRenderer<'a> {
    transcript: &'a super::Transcript,
}

impl<'a> TranscriptRenderer<'a> {
    /// Create a new renderer for given transcript
    pub fn new(transcript: &'a super::Transcript) -> Self {
        Self { transcript }
    }

    /// Render transcript to the given area
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let entries = self.transcript.render_entries();
        let mut text_lines = Vec::new();
        let content_width = area.width.saturating_sub(4) as usize;

        for entry in entries {
            self.render_entry(entry, content_width, &mut text_lines);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::BORDER))
            .title(Span::styled("Transcript", Style::default().fg(Theme::BLUE)));

        let paragraph = Paragraph::new(Text::from(text_lines))
            .block(block)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Render a single transcript entry
    fn render_entry(&self, entry: &super::TranscriptEntry, width: usize, lines: &mut Vec<Line<'static>>) {
        match entry {
            super::TranscriptEntry::UserMessage { content } => {
                self.render_user_message(content, width, lines);
            }
            super::TranscriptEntry::ModelResponse { content, streaming } => {
                self.render_model_response(content, *streaming, width, lines);
            }
            super::TranscriptEntry::ToolCall { tool, arguments, risk, description } => {
                self.render_tool_call(tool, arguments, risk, description.as_deref(), width, lines);
            }
            super::TranscriptEntry::ToolResult { tool, result, success, error } => {
                self.render_tool_result(tool, result, *success, error.as_deref(), width, lines);
            }
            super::TranscriptEntry::ApprovalPrompt { action, risk, description, decision } => {
                self.render_approval_prompt(action, risk, description.as_deref(), *decision, width, lines);
            }
            super::TranscriptEntry::SystemMessage { content } => {
                self.render_system_message(content, width, lines);
            }
        }
    }

    /// Render user message
    fn render_user_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("You", Style::default().fg(Theme::GREEN)),
            Span::styled(": ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Render model response
    fn render_model_response(&self, content: &str, streaming: bool, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::default());
        let prefix = if streaming { "Agent (streaming...)" } else { "Agent" };
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Theme::BLUE)),
            Span::styled(": ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Render tool call card
    fn render_tool_call(
        &self, tool: &str, arguments: &str, risk: &str, description: Option<&str>, width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        lines.push(Line::from(vec![
            Span::styled("🔧", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
            Span::raw(" ["),
            Span::styled(risk.to_string(), Style::default().fg(risk_color)),
            Span::raw("]"),
        ]));

        if let Some(desc) = description {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().fg(Theme::MUTED)),
                Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
            ]));
        }

        lines.push(Line::from(vec![Span::styled(
            "  Args: ",
            Style::default().fg(Theme::MUTED),
        )]));
        self.wrap_text(arguments, Theme::FG, width, lines);
    }

    /// Render tool result card
    fn render_tool_result(
        &self, tool: &str, result: &str, success: bool, error: Option<&str>, width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::default());
        let status = if success { ("✅", Theme::GREEN) } else { ("❌", Theme::RED) };

        lines.push(Line::from(vec![
            Span::styled(status.0, Style::default().fg(status.1)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
        ]));

        if let Some(err) = error {
            lines.push(Line::from(vec![
                Span::styled("  Error: ", Style::default().fg(Theme::RED)),
                Span::styled(err.to_string(), Style::default().fg(Theme::RED)),
            ]));
        }

        lines.push(Line::from(vec![Span::styled("  ", Style::default().fg(Theme::MUTED))]));
        self.wrap_text(result, Theme::FG, width, lines);
    }

    /// Render approval prompt card
    fn render_approval_prompt(
        &self, action: &str, risk: &str, description: Option<&str>, decision: Option<super::ApprovalDecision>,
        width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        lines.push(Line::from(vec![
            Span::styled("⚠️", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled("Approval Required", Style::default().fg(risk_color)),
        ]));

        lines.push(Line::from(vec![Span::styled(
            "  Action: ",
            Style::default().fg(Theme::MUTED),
        )]));
        self.wrap_text(&format!("{} [{}]", action, risk), Theme::FG, width, lines);

        if let Some(desc) = description {
            lines.push(Line::from(vec![Span::styled(
                "  Description: ",
                Style::default().fg(Theme::MUTED),
            )]));
            self.wrap_text(desc, Theme::FG, width, lines);
        }

        match decision {
            None => {
                lines.push(Line::default());
                lines.push(Line::from(vec![
                    Span::styled("  [", Style::default().fg(Theme::MUTED)),
                    Span::styled("y", Style::default().fg(Theme::GREEN).bold()),
                    Span::styled("] approve  ", Style::default().fg(Theme::MUTED)),
                    Span::styled("[", Style::default().fg(Theme::MUTED)),
                    Span::styled("n", Style::default().fg(Theme::RED).bold()),
                    Span::styled("] reject  ", Style::default().fg(Theme::MUTED)),
                    Span::styled("[", Style::default().fg(Theme::MUTED)),
                    Span::styled("c", Style::default().fg(Theme::YELLOW).bold()),
                    Span::styled("] cancel", Style::default().fg(Theme::MUTED)),
                ]));
            }
            Some(super::ApprovalDecision::Approved) => {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✓", Style::default().fg(Theme::GREEN)),
                    Span::styled(" Approved", Style::default().fg(Theme::GREEN)),
                ]));
            }
            Some(super::ApprovalDecision::Rejected) => {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✗", Style::default().fg(Theme::RED)),
                    Span::styled(" Rejected", Style::default().fg(Theme::RED)),
                ]));
            }
            Some(super::ApprovalDecision::Cancelled) => {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("⏹", Style::default().fg(Theme::YELLOW)),
                    Span::styled(" Cancelled", Style::default().fg(Theme::YELLOW)),
                ]));
            }
        }
    }

    /// Render system message
    fn render_system_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(Theme::MUTED)),
            Span::styled("System", Style::default().fg(Theme::PURPLE)),
            Span::styled("] ", Style::default().fg(Theme::MUTED)),
        ]));
        self.wrap_text(content, Theme::FG, width, lines);
    }

    /// Wrap text into lines with proper word wrapping based on width
    ///
    /// This implementation:
    /// - Respects newlines in the source text
    /// - Wraps at word boundaries when possible
    /// - Breaks long words if they exceed width
    /// - Uses Unicode-aware width calculation
    fn wrap_text(&self, text: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if max_width == 0 {
            return;
        }

        for source_line in text.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
                continue;
            }

            let words: Vec<&str> = source_line.split_whitespace().collect();
            if words.is_empty() {
                lines.push(Line::default());
                continue;
            }

            let mut current_line = String::new();
            let mut current_width = 0;

            for word in words {
                let word_width = word.width();
                let space_width = if current_line.is_empty() { 0 } else { 1 };

                if current_width + space_width + word_width > max_width {
                    if !current_line.is_empty() {
                        lines.push(Line::from(vec![Span::styled(
                            current_line.clone(),
                            Style::default().fg(color),
                        )]));
                        current_line = String::new();
                        current_width = 0;
                    }

                    if word_width > max_width {
                        let chars = word.chars().peekable();
                        let mut chunk_width = 0;
                        let mut chunk = String::new();

                        for ch in chars {
                            let ch_width = ch.width().unwrap_or(0);

                            if chunk_width + ch_width > max_width {
                                lines.push(Line::from(vec![Span::styled(
                                    chunk.clone(),
                                    Style::default().fg(color),
                                )]));
                                chunk.clear();
                                chunk_width = 0;
                            }

                            chunk.push(ch);
                            chunk_width += ch_width;
                        }

                        if !chunk.is_empty() {
                            lines.push(Line::from(vec![Span::styled(
                                chunk.clone(),
                                Style::default().fg(color),
                            )]));
                        }
                        continue;
                    }
                }
                if !current_line.is_empty() {
                    current_line.push(' ');
                    current_width += 1;
                }
                current_line.push_str(word);
                current_width += word_width;
            }

            if !current_line.is_empty() {
                lines.push(Line::from(vec![Span::styled(current_line, Style::default().fg(color))]));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Transcript;

    #[test]
    fn test_renderer_new() {
        let transcript = Transcript::new();
        let _ = TranscriptRenderer::new(&transcript);
    }

    #[test]
    fn test_renderer_with_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi there");
        let _ = TranscriptRenderer::new(&transcript);
    }

    #[test]
    fn test_wrap_text_basic() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();

        renderer.wrap_text("Hello world", Theme::FG, 20, &mut lines);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "Hello world");
    }

    #[test]
    fn test_wrap_text_with_wrap() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("This is a long line that should wrap", Theme::FG, 20, &mut lines);
        assert!(lines.len() > 1);
        assert!(lines[0].to_string().contains("This"));
    }

    #[test]
    fn test_wrap_text_empty() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("", Theme::FG, 20, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_newlines() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();

        renderer.wrap_text("Line 1\nLine 2\nLine 3", Theme::FG, 20, &mut lines);

        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_wrap_text_zero_width() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello", Theme::FG, 0, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_long_word() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("supercalifragilisticexpialidocious", Theme::FG, 10, &mut lines);
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_wrap_text_unicode() {
        let transcript = Transcript::new();
        let renderer = TranscriptRenderer::new(&transcript);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello 世界 🌍", Theme::FG, 20, &mut lines);
        assert!(!lines.is_empty());
    }
}
