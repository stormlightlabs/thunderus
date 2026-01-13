use crate::theme::Theme;
use crate::{syntax::SyntaxHighlighter, transcript::entry::CardDetailLevel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const CODE_BLOCK_START: &str = "```";
const CODE_BLOCK_END: &str = "```";

/// Context for rendering tool call cards
struct ToolCallContext<'a> {
    tool: &'a str,
    arguments: &'a str,
    risk: &'a str,
    description: Option<&'a str>,
    rendering: RenderContext<'a>,
}

/// Context for rendering tool result cards
struct ToolResultContext<'a> {
    tool: &'a str,
    result: &'a str,
    success: bool,
    error: Option<&'a str>,
    rendering: RenderContext<'a>,
}

/// Context for rendering approval prompt cards
struct ApprovalPromptContext<'a> {
    action: &'a str,
    risk: &'a str,
    description: Option<&'a str>,
    decision: Option<super::ApprovalDecision>,
    rendering: RenderContext<'a>,
}

struct RenderContext<'a> {
    width: usize,
    detail_level: CardDetailLevel,
    lines: &'a mut Vec<Line<'static>>,
}

impl<'a> RenderContext<'a> {
    pub fn new(width: usize, detail_level: CardDetailLevel, lines: &'a mut Vec<Line<'static>>) -> Self {
        Self { width, detail_level, lines }
    }
}

/// Renders transcript entries to frame
pub struct TranscriptRenderer<'a> {
    transcript: &'a super::Transcript,
    scroll_vertical: u16,
}

impl<'a> TranscriptRenderer<'a> {
    /// Create a new renderer for given transcript
    pub fn new(transcript: &'a super::Transcript) -> Self {
        Self { transcript, scroll_vertical: 0 }
    }

    /// Create a new renderer with scroll offset
    pub fn with_vertical_scroll(transcript: &'a super::Transcript, scroll: u16) -> Self {
        Self { transcript, scroll_vertical: scroll }
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
            .wrap(Wrap { trim: true })
            .scroll((0, self.scroll_vertical));

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
            super::TranscriptEntry::ToolCall { tool, arguments, risk, description, detail_level } => {
                self.render_tool_call(ToolCallContext {
                    tool,
                    arguments,
                    risk,
                    description: description.as_deref(),
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::ToolResult { tool, result, success, error, detail_level } => {
                self.render_tool_result(ToolResultContext {
                    tool,
                    result,
                    success: *success,
                    error: error.as_deref(),
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
            }
            super::TranscriptEntry::ApprovalPrompt { action, risk, description, decision, detail_level } => {
                self.render_approval_prompt(ApprovalPromptContext {
                    action,
                    risk,
                    description: description.as_deref(),
                    decision: *decision,
                    rendering: RenderContext::new(width, *detail_level, lines),
                });
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
    fn render_tool_call(&self, ctx: ToolCallContext) {
        let ToolCallContext { tool, arguments, risk, description, rendering } = ctx;

        rendering.lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        rendering.lines.push(Line::from(vec![
            Span::styled("🔧", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
            Span::raw(" ["),
            Span::styled(risk.to_string(), Style::default().fg(risk_color)),
            Span::raw("]"),
        ]));

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }
            }
            CardDetailLevel::Detailed | CardDetailLevel::Verbose => {
                if let Some(desc) = description {
                    rendering.lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Theme::MUTED)),
                        Span::styled(desc.to_string(), Style::default().fg(Theme::FG)),
                    ]));
                }

                rendering.lines.push(Line::from(vec![Span::styled(
                    "  Args: ",
                    Style::default().fg(Theme::MUTED),
                )]));
                self.wrap_text(arguments, Theme::FG, rendering.width, rendering.lines);

                if rendering.detail_level == CardDetailLevel::Verbose {
                    rendering.lines.push(Line::from(vec![Span::styled(
                        "  [verbose mode - full execution details]",
                        Style::default().fg(Theme::MUTED),
                    )]));
                }
            }
        }
    }

    /// Render tool result card
    fn render_tool_result(&self, ctx: ToolResultContext) {
        let ToolResultContext { tool, result, success, error, rendering } = ctx;

        rendering.lines.push(Line::default());
        let status = if success { ("✅", Theme::GREEN) } else { ("❌", Theme::RED) };

        rendering.lines.push(Line::from(vec![
            Span::styled(status.0, Style::default().fg(status.1)),
            Span::raw(" "),
            Span::styled(tool.to_string(), Style::default().fg(Theme::FG)),
        ]));

        if let Some(err) = error {
            rendering.lines.push(Line::from(vec![
                Span::styled("  Error: ", Style::default().fg(Theme::RED)),
                Span::styled(err.to_string(), Style::default().fg(Theme::RED)),
            ]));
        }

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [output truncated - press 'v' for verbose]",
                    Style::default().fg(Theme::MUTED),
                )]));
            }
            CardDetailLevel::Detailed | CardDetailLevel::Verbose => {
                rendering
                    .lines
                    .push(Line::from(vec![Span::styled("  ", Style::default().fg(Theme::MUTED))]));
                self.render_with_code_highlighting(result, Theme::FG, rendering.width, rendering.lines);

                if rendering.detail_level == CardDetailLevel::Verbose {
                    rendering.lines.push(Line::from(vec![Span::styled(
                        "  [verbose mode - full execution trace]",
                        Style::default().fg(Theme::MUTED),
                    )]));
                }
            }
        }
    }

    /// Render approval prompt card
    fn render_approval_prompt(&self, ctx: ApprovalPromptContext) {
        let ApprovalPromptContext { action, risk, description, decision, rendering } = ctx;

        rendering.lines.push(Line::default());
        let risk_color = super::TranscriptEntry::risk_level_color_str(risk);

        rendering.lines.push(Line::from(vec![
            Span::styled("⚠️", Style::default().fg(Theme::YELLOW)),
            Span::raw(" "),
            Span::styled("Approval Required", Style::default().fg(risk_color)),
        ]));

        rendering.lines.push(Line::from(vec![Span::styled(
            "  Action: ",
            Style::default().fg(Theme::MUTED),
        )]));
        self.wrap_text(
            &format!("{} [{}]", action, risk),
            Theme::FG,
            rendering.width,
            rendering.lines,
        );

        if let Some(desc) = description {
            rendering.lines.push(Line::from(vec![Span::styled(
                "  Description: ",
                Style::default().fg(Theme::MUTED),
            )]));
            self.wrap_text(desc, Theme::FG, rendering.width, rendering.lines);
        }

        match decision {
            None => {
                rendering.lines.push(Line::default());
                rendering.lines.push(Line::from(vec![
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

                match rendering.detail_level {
                    CardDetailLevel::Brief => {}
                    CardDetailLevel::Detailed => {
                        rendering.lines.push(Line::from(vec![Span::styled(
                            "  [detailed mode - scope and risk assessment shown]",
                            Style::default().fg(Theme::MUTED),
                        )]));
                    }
                    CardDetailLevel::Verbose => {
                        rendering.lines.push(Line::from(vec![Span::styled(
                            "  [verbose mode - full approval context available]",
                            Style::default().fg(Theme::MUTED),
                        )]));
                    }
                }
            }
            Some(super::ApprovalDecision::Approved) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✓", Style::default().fg(Theme::GREEN)),
                    Span::styled(" Approved", Style::default().fg(Theme::GREEN)),
                ]));
            }
            Some(super::ApprovalDecision::Rejected) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("✗", Style::default().fg(Theme::RED)),
                    Span::styled(" Rejected", Style::default().fg(Theme::RED)),
                ]));
            }
            Some(super::ApprovalDecision::Cancelled) => {
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("⏹", Style::default().fg(Theme::YELLOW)),
                    Span::styled(" Cancelled", Style::default().fg(Theme::YELLOW)),
                ]));
            }
        }
    }

    /// Render text with code block highlighting
    fn render_with_code_highlighting(
        &self, text: &str, default_color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let mut remaining = text;

        while let Some(start_idx) = remaining.find(CODE_BLOCK_START) {
            if start_idx > 0 {
                let before_code = &remaining[..start_idx];
                self.wrap_text(before_code, default_color, max_width, lines);
            }

            let after_start = &remaining[start_idx + CODE_BLOCK_START.len()..];

            let lang_end_idx = after_start.find('\n').unwrap_or(after_start.len());
            let lang = after_start[..lang_end_idx].trim();

            let code_start = if lang_end_idx < after_start.len() { lang_end_idx + 1 } else { lang_end_idx };
            let code_block = &after_start[code_start..];

            if let Some(end_idx) = code_block.find(CODE_BLOCK_END) {
                let code = &code_block[..end_idx];
                remaining = &code_block[end_idx + CODE_BLOCK_END.len()..];

                self.render_code_block(code, lang, max_width, lines);
            } else {
                self.wrap_text(after_start, Theme::CYAN, max_width, lines);
                break;
            }
        }

        if !remaining.is_empty() {
            self.wrap_text(remaining, default_color, max_width, lines);
        }
    }

    /// Render a code block with syntax highlighting
    fn render_code_block(&self, code: &str, lang: &str, _max_width: usize, lines: &mut Vec<Line<'static>>) {
        let highlighter = SyntaxHighlighter::new();
        let highlighted = highlighter.highlight_code(code, lang.trim());

        for span in &highlighted {
            lines.push(Line::from(vec![Span::raw(format!("  {}", span.content))]));
        }

        if !highlighted.is_empty() && !code.lines().last().map(|l| l.trim().is_empty()).unwrap_or(true) {
            lines.push(Line::default());
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
    /// - Smart wrapping for file paths and URLs
    fn wrap_text(&self, text: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if max_width == 0 {
            return;
        }

        for source_line in text.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
                continue;
            }

            if self.is_path_or_url(source_line) {
                self.smart_wrap_path(source_line, color, max_width, lines);
            } else {
                self.wrap_normal_text(source_line, color, max_width, lines);
            }
        }
    }

    /// Check if text looks like a file path or URL
    fn is_path_or_url(&self, text: &str) -> bool {
        text.starts_with('/')
            || text.starts_with("./")
            || text.starts_with("../")
            || text.starts_with("http://")
            || text.starts_with("https://")
            || text.starts_with("git@")
            || text.starts_with("file://")
    }

    /// Smart wrap for paths and URLs (prefer breaking at path separators)
    fn smart_wrap_path(
        &self, path: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        if path.width() <= max_width {
            lines.push(Line::from(vec![Span::styled(
                path.to_string(),
                Style::default().fg(color),
            )]));
            return;
        }

        let mut remaining = path;
        while remaining.width() > max_width {
            if let Some(idx) = self.find_break_point(remaining, max_width) {
                let chunk = &remaining[..idx];
                lines.push(Line::from(vec![Span::styled(
                    chunk.to_string(),
                    Style::default().fg(color),
                )]));
                remaining = &remaining[idx..];
            } else {
                lines.push(Line::from(vec![Span::styled(
                    remaining.to_string(),
                    Style::default().fg(color),
                )]));
                break;
            }
        }

        if !remaining.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                remaining.to_string(),
                Style::default().fg(color),
            )]));
        }
    }

    /// Find a good break point in path/URL (prefer /, ., etc.)
    fn find_break_point(&self, text: &str, max_width: usize) -> Option<usize> {
        let mut break_idx = None;
        for (i, ch) in text.char_indices() {
            if i > 0
                && i % max_width == 0
                && let Some(idx) = break_idx
            {
                return Some(idx);
            }
            if matches!(ch, '/' | '.' | '-' | '_') {
                break_idx = Some(i + ch.len_utf8());
            }
        }
        break_idx
    }

    /// Normal word-based text wrapping
    fn wrap_normal_text(
        &self, text: &str, color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            lines.push(Line::default());
            return;
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
