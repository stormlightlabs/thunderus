use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Transcript component displaying the conversation
///
/// Shows:
/// - User messages
/// - Model responses
/// - Tool call cards
/// - Tool result cards
/// - Approval prompts
pub struct Transcript<'a> {
    content: &'a str,
    streaming: bool,
}

impl<'a> Transcript<'a> {
    pub fn new(content: &'a str) -> Self {
        Self { content, streaming: false }
    }

    pub fn streaming(mut self) -> Self {
        self.streaming = true;
        self
    }

    /// Render transcript to the given frame
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let text: Text = Text::from(self.content);

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Theme::BORDER));

        if self.streaming {
            block = block.title(Span::styled(
                "Transcript (generating...)",
                Style::default().fg(Theme::YELLOW),
            ));
        } else {
            block = block.title(Span::styled("Transcript", Style::default().fg(Theme::BLUE)));
        }

        let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Render user message in transcript
    pub fn render_user_message(content: &str) -> Text<'_> {
        Text::from(vec![
            Line::from(vec![
                Span::styled("You", Style::default().fg(Theme::GREEN)),
                Span::styled(": ", Style::default().fg(Theme::MUTED)),
            ]),
            Line::from(content),
        ])
    }

    /// Render model response in transcript
    pub fn render_model_response(content: &str) -> Text<'_> {
        Text::from(vec![
            Line::from(vec![
                Span::styled("Agent", Style::default().fg(Theme::BLUE)),
                Span::styled(": ", Style::default().fg(Theme::MUTED)),
            ]),
            Line::from(content),
        ])
    }

    /// Render tool call card
    pub fn render_tool_call<'b>(tool: &'b str, args: &'b str, risk: &'b str) -> Text<'b> {
        let risk_color = Theme::risk_level_color(risk);
        Text::from(vec![
            Line::from(vec![
                Span::styled("🔧", Style::default().fg(Theme::YELLOW)),
                Span::raw(" "),
                Span::styled(tool, Style::default().fg(Theme::FG)),
                Span::raw(" ["),
                Span::styled(risk, Style::default().fg(risk_color)),
                Span::raw("]"),
            ]),
            Line::from(args),
        ])
    }

    /// Render tool result card
    pub fn render_tool_result<'b>(tool: &'b str, result: &'b str, success: bool) -> Text<'b> {
        let status = if success { ("✅", Theme::GREEN) } else { ("❌", Theme::RED) };

        Text::from(vec![
            Line::from(vec![
                Span::styled(status.0, Style::default().fg(status.1)),
                Span::raw(" "),
                Span::styled(tool, Style::default().fg(Theme::FG)),
            ]),
            Line::from(result),
        ])
    }

    /// Render approval prompt card
    pub fn render_approval_prompt<'b>(action: &'b str, description: Option<&'b str>) -> Text<'b> {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("⚠️", Style::default().fg(Theme::YELLOW)),
                Span::raw(" "),
                Span::styled("Approval Required", Style::default().fg(Theme::YELLOW)),
            ]),
            Line::from(vec![
                Span::styled("Action: ", Style::default().fg(Theme::MUTED)),
                Span::styled(action, Style::default().fg(Theme::FG)),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("[y]", Style::default().fg(Theme::GREEN)),
                Span::styled(" approve  ", Style::default().fg(Theme::MUTED)),
                Span::styled("[n]", Style::default().fg(Theme::RED)),
                Span::styled(" reject  ", Style::default().fg(Theme::MUTED)),
                Span::styled("[c]", Style::default().fg(Theme::YELLOW)),
                Span::styled(" cancel", Style::default().fg(Theme::MUTED)),
            ]),
        ];

        if let Some(desc) = description {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Description: ", Style::default().fg(Theme::MUTED)),
                Span::styled(desc, Style::default().fg(Theme::FG)),
            ]));
        }

        Text::from(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_new() {
        let transcript = Transcript::new("test content");
        assert_eq!(transcript.content, "test content");
        assert!(!transcript.streaming);
    }

    #[test]
    fn test_transcript_streaming() {
        let transcript = Transcript::new("test content").streaming();
        assert!(transcript.streaming);
    }

    #[test]
    fn test_render_user_message() {
        let text = Transcript::render_user_message("Hello, world!");
        let lines = text.lines;

        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "You"));
        assert!(lines[1].spans.iter().any(|s| s.content == "Hello, world!"));
    }

    #[test]
    fn test_render_model_response() {
        let text = Transcript::render_model_response("Hi there!");
        let lines = text.lines;

        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "Agent"));
        assert!(lines[1].spans.iter().any(|s| s.content == "Hi there!"));
    }

    #[test]
    fn test_render_tool_call() {
        let text = Transcript::render_tool_call("fs.read", "{ path: '/tmp/file' }", "safe");
        let lines = text.lines;

        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "🔧"));
        assert!(lines[0].spans.iter().any(|s| s.content == "fs.read"));
        assert!(lines[0].spans.iter().any(|s| s.content == "safe"));
    }

    #[test]
    fn test_render_tool_result() {
        let text = Transcript::render_tool_result("fs.read", "file content", true);
        let lines = text.lines;

        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "✅"));
        assert!(lines[0].spans.iter().any(|s| s.content == "fs.read"));
        assert!(lines[1].spans.iter().any(|s| s.content == "file content"));
    }

    #[test]
    fn test_render_tool_result_failure() {
        let text = Transcript::render_tool_result("fs.read", "error: file not found", false);
        let lines = text.lines;

        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content == "❌"));
    }

    #[test]
    fn test_render_approval_prompt() {
        let text = Transcript::render_approval_prompt("patch.feature", Some("Add new feature"));
        let lines = text.lines;

        assert!(lines.len() >= 3);
        assert!(lines[0].spans.iter().any(|s| s.content == "⚠️"));
        assert!(lines[0].spans.iter().any(|s| s.content == "Approval Required"));
        assert!(lines[1].spans.iter().any(|s| s.content == "patch.feature"));
    }

    #[test]
    fn test_render_approval_prompt_without_description() {
        let text = Transcript::render_approval_prompt("test.action", None);
        let lines = text.lines;

        assert!(lines.len() >= 3);
        assert!(lines[0].spans.iter().any(|s| s.content == "Approval Required"));
    }

    #[test]
    fn test_risk_level_colors() {
        assert_eq!(Theme::risk_level_color("safe"), Theme::GREEN);
        assert_eq!(Theme::risk_level_color("risky"), Theme::YELLOW);
        assert_eq!(Theme::risk_level_color("dangerous"), Theme::RED);
    }
}
