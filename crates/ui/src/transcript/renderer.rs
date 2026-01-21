use crate::theme::ThemePalette;
use crate::transcript::ErrorType;
use crate::{syntax::SyntaxHighlighter, transcript::entry::CardDetailLevel};

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const CODE_BLOCK_START: &str = "```";
const CODE_BLOCK_END: &str = "```";

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    pub centered: bool,
    pub max_bubble_width: Option<usize>,
    pub animation_frame: u8,
}

/// Context for rendering tool call cards
struct ToolCallContext<'a> {
    tool: &'a str,
    arguments: &'a str,
    risk: &'a str,
    /// WHAT: Plain language description
    description: Option<&'a str>,
    /// WHY: Task context
    task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    scope: Option<&'a str>,
    /// RISK: Classification reasoning
    classification_reasoning: Option<&'a str>,
    rendering: RenderContext<'a>,
}

/// Context for rendering tool result cards
struct ToolResultContext<'a> {
    tool: &'a str,
    result: &'a str,
    success: bool,
    error: Option<&'a str>,
    /// RESULT: Exit code
    exit_code: Option<i32>,
    /// RESULT: Next steps
    next_steps: Option<&'a Vec<String>>,
    rendering: RenderContext<'a>,
}

/// Context for rendering approval prompt cards
struct ApprovalPromptContext<'a> {
    action: &'a str,
    risk: &'a str,
    /// WHAT: Plain language description
    description: Option<&'a str>,
    /// WHY: Task context
    task_context: Option<&'a str>,
    /// SCOPE: Files/paths affected
    scope: Option<&'a str>,
    /// RISK: Risk reasoning
    risk_reasoning: Option<&'a str>,
    decision: Option<super::ApprovalDecision>,
    rendering: RenderContext<'a>,
}

/// Context for rendering patch display with hunk labels
struct PatchDisplayContext<'a> {
    patch_name: &'a str,
    file_path: &'a str,
    diff_content: &'a str,
    hunk_labels: &'a [Option<String>],
    rendering: RenderContext<'a>,
}

struct RenderContext<'a> {
    width: usize,
    detail_level: CardDetailLevel,
    lines: &'a mut Vec<Line<'static>>,
    compact_mode: bool,
    theme: ThemePalette,
    animation_frame: u8,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        width: usize, detail_level: CardDetailLevel, lines: &'a mut Vec<Line<'static>>, theme: ThemePalette,
        animation_frame: u8,
    ) -> Self {
        let compact_mode = width < 80;
        Self { width, detail_level, lines, compact_mode, theme, animation_frame }
    }

    /// Check if we should render compact (single-line) cards
    pub fn is_compact(&self) -> bool {
        self.compact_mode
    }
}

/// Renders transcript entries to frame
pub struct TranscriptRenderer<'a> {
    transcript: &'a super::Transcript,
    scroll_vertical: u16,
    streaming_ellipsis: &'a str,
    theme: ThemePalette,
    options: RenderOptions,
}

impl<'a> TranscriptRenderer<'a> {
    /// Create a new renderer for given transcript
    pub fn new(transcript: &'a super::Transcript, theme: ThemePalette) -> Self {
        Self { transcript, scroll_vertical: 0, streaming_ellipsis: "", theme, options: RenderOptions::default() }
    }

    /// Create a new renderer with scroll offset
    pub fn with_vertical_scroll(
        transcript: &'a super::Transcript, scroll: u16, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        Self { transcript, scroll_vertical: scroll, streaming_ellipsis: "", theme, options }
    }

    /// Create a new renderer with streaming ellipsis animation
    pub fn with_streaming_ellipsis(
        transcript: &'a super::Transcript, scroll: u16, ellipsis: &'a str, theme: ThemePalette, options: RenderOptions,
    ) -> Self {
        Self { transcript, scroll_vertical: scroll, streaming_ellipsis: ellipsis, theme, options }
    }

    /// Render transcript to the given area with scrollbar indicator
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let entries = self.transcript.render_entries();
        let mut text_lines = Vec::new();
        let padding_x = 2usize;
        let padding_y = 1usize;
        let scrollbar_width = 1usize;
        let content_width = area.width.saturating_sub((padding_x * 2 + scrollbar_width) as u16) as usize;

        for (idx, entry) in entries.iter().enumerate() {
            if idx > 0 {
                text_lines.push(Line::from(vec![Span::styled(
                    "─".repeat(content_width),
                    Style::default().fg(self.theme.muted),
                )]));
                text_lines.push(Line::default());
            }
            self.render_entry(entry, content_width, self.streaming_ellipsis, &mut text_lines);
        }

        let mut padded_lines = Vec::new();
        let left_pad = Span::styled("  ", Style::default().bg(self.theme.bg));
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

        let line_count = padded_lines.len();

        frame.render_widget(Block::default().style(Style::default().bg(self.theme.panel_bg)), area);

        let paragraph = Paragraph::new(Text::from(padded_lines))
            .wrap(Wrap { trim: true })
            .scroll((0, self.scroll_vertical));

        frame.render_widget(paragraph, area);

        self.render_scrollbar(frame, area, line_count);
    }

    /// Render scrollbar indicator on right edge of transcript
    fn render_scrollbar(&self, frame: &mut Frame<'_>, area: Rect, content_line_count: usize) {
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
            let symbol = if is_thumb { "▮" } else { "│" };
            let style = Style::default()
                .fg(if is_thumb { self.theme.fg } else { self.theme.muted })
                .bg(self.theme.panel_bg);

            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(symbol, style)])),
                Rect::new(scrollbar_x, scrollbar_top + y, 1, 1),
            );
        }
    }

    /// Render a single transcript entry
    fn render_entry(
        &self, entry: &super::TranscriptEntry, width: usize, ellipsis: &str, lines: &mut Vec<Line<'static>>,
    ) {
        match entry {
            super::TranscriptEntry::UserMessage { content } => self.render_user_message(content, width, lines),
            super::TranscriptEntry::ModelResponse { content, streaming } => {
                self.render_model_response(content, *streaming, ellipsis, width, lines)
            }
            super::TranscriptEntry::ToolCall {
                tool,
                arguments,
                risk,
                description,
                task_context,
                scope,
                classification_reasoning,
                detail_level,
            } => self.render_tool_call(ToolCallContext {
                tool,
                arguments,
                risk,
                description: description.as_deref(),
                task_context: task_context.as_deref(),
                scope: scope.as_deref(),
                classification_reasoning: classification_reasoning.as_deref(),
                rendering: RenderContext::new(width, *detail_level, lines, self.theme, self.options.animation_frame),
            }),
            super::TranscriptEntry::ToolResult {
                tool,
                result,
                success,
                error,
                exit_code,
                next_steps,
                detail_level,
            } => self.render_tool_result(ToolResultContext {
                tool,
                result,
                success: *success,
                error: error.as_deref(),
                exit_code: *exit_code,
                next_steps: next_steps.as_ref(),
                rendering: RenderContext::new(width, *detail_level, lines, self.theme, self.options.animation_frame),
            }),
            super::TranscriptEntry::PatchDisplay { patch_name, file_path, diff_content, hunk_labels, detail_level } => {
                self.render_patch_display(PatchDisplayContext {
                    patch_name,
                    file_path,
                    diff_content,
                    hunk_labels,
                    rendering: RenderContext::new(
                        width,
                        *detail_level,
                        lines,
                        self.theme,
                        self.options.animation_frame,
                    ),
                });
            }
            super::TranscriptEntry::ApprovalPrompt {
                action,
                risk,
                description,
                task_context,
                scope,
                risk_reasoning,
                decision,
                detail_level,
            } => self.render_approval_prompt(ApprovalPromptContext {
                action,
                risk,
                description: description.as_deref(),
                task_context: task_context.as_deref(),
                scope: scope.as_deref(),
                risk_reasoning: risk_reasoning.as_deref(),
                decision: *decision,
                rendering: RenderContext::new(width, *detail_level, lines, self.theme, self.options.animation_frame),
            }),
            super::TranscriptEntry::SystemMessage { content } => self.render_system_message(content, width, lines),
            super::TranscriptEntry::ErrorEntry { message, error_type, can_retry, context } => {
                self.render_error_entry(message, *error_type, *can_retry, context.as_deref(), width, lines)
            }
            super::TranscriptEntry::ThinkingIndicator { duration_secs } => {
                self.render_thinking_indicator(*duration_secs, lines)
            }
            super::TranscriptEntry::StatusLine { message, status_type } => {
                self.render_status_line(message, *status_type, lines)
            }
        }
    }

    /// Render user message with role prefix
    fn render_user_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(self.theme.blue)),
            Span::styled("User", Style::default().fg(self.theme.blue).bold()),
        ]));
        self.render_accent_bar_message(content, self.theme.blue, self.theme.panel_bg, width, lines);
    }

    /// Render message with accent bar on the left
    fn render_accent_bar_message(
        &self, content: &str, accent_color: ratatui::style::Color, bg_color: ratatui::style::Color, width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        let accent_bar = Span::styled("┃ ", Style::default().fg(accent_color).bg(bg_color));
        let content_style = Style::default().fg(self.theme.fg).bg(bg_color);
        let content_width = width.saturating_sub(3);

        lines.push(Line::from(vec![accent_bar.clone()]));

        for source_line in content.lines() {
            if source_line.is_empty() {
                lines.push(Line::from(vec![accent_bar.clone()]));
            } else {
                for wrapped_line in self.wrap_text_to_width(source_line, content_width) {
                    lines.push(Line::from(vec![
                        accent_bar.clone(),
                        Span::styled(wrapped_line, content_style),
                    ]));
                }
            }
        }

        lines.push(Line::from(vec![accent_bar.clone()]));
    }

    /// Render model response with role prefix
    fn render_model_response(
        &self, content: &str, streaming: bool, ellipsis: &str, width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::from(vec![
            Span::styled("◆ ", Style::default().fg(self.theme.cyan)),
            Span::styled("Assistant", Style::default().fg(self.theme.cyan).bold()),
        ]));

        let content_style = Style::default().fg(self.theme.fg).bg(self.theme.bg);

        for source_line in content.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
            } else {
                for wrapped_line in self.wrap_text_to_width(source_line, width) {
                    lines.push(Line::from(vec![Span::styled(wrapped_line, content_style)]));
                }
            }
        }

        if streaming {
            let cursor_line = Line::from(vec![
                Span::styled(ellipsis.to_string(), Style::default().fg(self.theme.muted)),
                Span::styled("█", Style::default().fg(self.theme.fg)),
            ]);
            lines.push(cursor_line);
        }
    }

    fn render_card(
        &self, title: &str, border_color: ratatui::style::Color, width: usize, content: Vec<Line<'static>>,
        lines: &mut Vec<Line<'static>>,
    ) {
        let card_width = width.max(20).min(width);
        let border_style = Style::default().fg(border_color).bg(self.theme.panel_bg);
        let content_bg = Style::default().bg(self.theme.panel_bg);
        let padding = 2usize;
        let prefix = "┌─ ";
        let title_width = title.width();
        let base_len = prefix.width() + title_width + 1;
        let fill_len = card_width.saturating_sub(base_len + 1);

        lines.push(Line::from(vec![
            Span::styled(prefix, border_style),
            Span::styled(title.to_string(), border_style),
            Span::styled(" ", border_style),
            Span::styled("─".repeat(fill_len), border_style),
            Span::styled("┐", border_style),
        ]));

        let mut padded_content = Vec::with_capacity(content.len() + 2);
        padded_content.push(Line::default());
        padded_content.extend(content);
        padded_content.push(Line::default());

        for line in padded_content {
            let line_width = line.to_string().width();
            let inner_width = card_width.saturating_sub(2);
            let content_width = inner_width.saturating_sub(padding * 2);
            let padding_needed = content_width.saturating_sub(line_width);
            let mut spans = Vec::new();
            spans.push(Span::styled("│  ", border_style));
            spans.extend(line.spans);
            spans.push(Span::styled(" ".repeat(padding_needed), content_bg));
            spans.push(Span::styled("  │", border_style));
            lines.push(Line::from(spans));
        }

        lines.push(Line::from(vec![
            Span::styled("└", border_style),
            Span::styled("─".repeat(card_width.saturating_sub(2)), border_style),
            Span::styled("┘", border_style),
        ]));
    }

    fn wrap_text_styled(&self, text: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if max_width == 0 {
            return;
        }

        for source_line in text.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
                continue;
            }

            if self.is_path_or_url(source_line) {
                self.smart_wrap_path_styled(source_line, style, max_width, lines);
            } else {
                self.wrap_normal_text_styled(source_line, style, max_width, lines);
            }
        }
    }

    fn smart_wrap_path_styled(&self, path: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if path.width() <= max_width {
            lines.push(Line::from(vec![Span::styled(path.to_string(), style)]));
            return;
        }

        let mut remaining = path;
        while remaining.width() > max_width {
            if let Some(idx) = self.find_break_point(remaining, max_width) {
                let chunk = &remaining[..idx];
                lines.push(Line::from(vec![Span::styled(chunk.to_string(), style)]));
                remaining = &remaining[idx..];
            } else {
                lines.push(Line::from(vec![Span::styled(remaining.to_string(), style)]));
                break;
            }
        }

        if !remaining.is_empty() {
            lines.push(Line::from(vec![Span::styled(remaining.to_string(), style)]));
        }
    }

    fn wrap_normal_text_styled(&self, text: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
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
                    lines.push(Line::from(vec![Span::styled(current_line.clone(), style)]));
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
                            lines.push(Line::from(vec![Span::styled(chunk.clone(), style)]));
                            chunk.clear();
                            chunk_width = 0;
                        }

                        chunk.push(ch);
                        chunk_width += ch_width;
                    }

                    if !chunk.is_empty() {
                        lines.push(Line::from(vec![Span::styled(chunk.clone(), style)]));
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
            lines.push(Line::from(vec![Span::styled(current_line, style)]));
        }
    }

    /// Wrap text to a specific width, returning Vec<String>
    fn wrap_text_to_width(&self, text: &str, max_width: usize) -> Vec<String> {
        let mut result = Vec::new();
        if max_width == 0 {
            return result;
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return result;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in words {
            let word_width = word.width();
            let space_width = if current_line.is_empty() { 0 } else { 1 };

            if current_width + space_width + word_width > max_width {
                if !current_line.is_empty() {
                    result.push(current_line.clone());
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
                            result.push(chunk.clone());
                            chunk.clear();
                            chunk_width = 0;
                        }

                        chunk.push(ch);
                        chunk_width += ch_width;
                    }

                    if !chunk.is_empty() {
                        result.push(chunk);
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
            result.push(current_line);
        }

        result
    }

    /// Render tool call as compact inline format
    ///
    /// Format: `* ToolName "args" (result summary)`
    fn render_tool_call(&self, ctx: ToolCallContext) {
        let ToolCallContext {
            tool,
            arguments,
            risk,
            description,
            task_context,
            scope,
            classification_reasoning,
            rendering,
        } = ctx;

        let theme = rendering.theme;
        let risk_color = super::TranscriptEntry::risk_level_color(theme, risk);

        if matches!(rendering.detail_level, CardDetailLevel::Brief) {
            let info = description
                .or(scope)
                .map(
                    |s| {
                        if s.len() > 50 { format!("{}...", &s[..47]) } else { s.to_string() }
                    },
                )
                .unwrap_or_else(|| {
                    if arguments.contains("path") {
                        arguments
                            .split('"')
                            .nth(1)
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| arguments.chars().take(40).collect())
                    } else {
                        arguments.chars().take(40).collect()
                    }
                });

            rendering.lines.push(Line::from(vec![
                Span::styled("* ", Style::default().fg(theme.yellow)),
                Span::styled(tool.to_string(), Style::default().fg(theme.fg).bold()),
                Span::styled(format!(" \"{}\"", info), Style::default().fg(theme.muted)),
            ]));
            return;
        }

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);

        let mut content_lines: Vec<Line<'static>> = Vec::new();

        content_lines.push(Line::from(vec![
            Span::styled("Risk: ", muted_style),
            Span::styled(
                risk.to_string(),
                Style::default().fg(risk_color).bg(theme.panel_bg).bold(),
            ),
        ]));

        if let Some(desc) = description {
            content_lines.push(Line::from(vec![
                Span::styled("WHAT: ", label_style),
                Span::styled(desc.to_string(), content_style),
            ]));
        }

        if let Some(ctx) = task_context {
            content_lines.push(Line::from(vec![
                Span::styled("WHY:  ", label_style),
                Span::styled(ctx.to_string(), content_style),
            ]));
        }

        if let Some(sc) = scope {
            content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
            self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
        }

        if matches!(rendering.detail_level, CardDetailLevel::Verbose)
            && let Some(reasoning) = classification_reasoning
        {
            content_lines.push(Line::from(vec![
                Span::styled("RISK:  ", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                Span::styled(
                    reasoning.to_string(),
                    Style::default().fg(theme.fg).bg(theme.panel_bg).italic(),
                ),
            ]));
        }

        content_lines.push(Line::from(vec![Span::styled("Args:", muted_style)]));
        self.wrap_text_styled(arguments, content_style, content_width, &mut content_lines);

        let title = format!("Tool: {}", tool);
        self.render_card(&title, risk_color, rendering.width, content_lines, rendering.lines);
    }

    /// Render tool result as compact inline format
    ///
    /// Format: `-> Result summary` or `✓ ToolName` / `× ToolName error`
    fn render_tool_result(&self, ctx: ToolResultContext) {
        let ToolResultContext { tool, result, success, error, exit_code, next_steps, rendering } = ctx;

        let theme = rendering.theme;

        if matches!(rendering.detail_level, CardDetailLevel::Brief) {
            let (symbol, color) = if success { ("✓", theme.green) } else { ("×", theme.red) };

            let preview = if let Some(err) = error {
                format!(
                    " {}",
                    if err.len() > 50 { format!("{}...", &err[..47]) } else { err.to_string() }
                )
            } else if let Some(first_line) = result.lines().next() {
                let line = first_line.trim();
                if line.is_empty() {
                    String::new()
                } else if line.len() > 50 {
                    format!(" ({}...)", &line[..47])
                } else {
                    format!(" ({})", line)
                }
            } else {
                String::new()
            };

            rendering.lines.push(Line::from(vec![
                Span::styled(format!("{} ", symbol), Style::default().fg(color)),
                Span::styled(tool.to_string(), Style::default().fg(theme.fg)),
                Span::styled(preview, Style::default().fg(theme.muted)),
            ]));
            return;
        }

        rendering.lines.push(Line::default());

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();

        let (status_symbol, status_color) = if success { ("✓", theme.green) } else { ("✗", theme.red) };

        let mut content_lines: Vec<Line<'static>> = Vec::new();
        content_lines.push(Line::from(vec![
            Span::styled("Status: ", muted_style),
            Span::styled(
                status_symbol,
                Style::default().fg(status_color).bg(theme.panel_bg).bold(),
            ),
        ]));

        if let Some(err) = error {
            content_lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(theme.red).bg(theme.panel_bg)),
                Span::styled(err.to_string(), Style::default().fg(theme.red).bg(theme.panel_bg)),
            ]));
        }

        self.render_with_code_highlighting(result, theme.fg, content_width, &mut content_lines);

        if let Some(code) = exit_code {
            let color = if code == 0 { theme.green } else { theme.red };
            content_lines.push(Line::from(vec![
                Span::styled("Exit code: ", muted_style),
                Span::styled(code.to_string(), Style::default().fg(color).bg(theme.panel_bg).bold()),
            ]));
        }

        if let Some(steps) = next_steps
            && !steps.is_empty()
        {
            content_lines.push(Line::from(vec![Span::styled("Next steps:", label_style)]));
            for step in steps {
                content_lines.push(Line::from(vec![
                    Span::styled("- ", muted_style),
                    Span::styled(step.clone(), content_style),
                ]));
            }
        }

        let title = format!("Result: {}", tool);
        let border_color = if success { theme.green } else { theme.red };
        self.render_card(&title, border_color, rendering.width, content_lines, rendering.lines);
    }

    /// Render approval prompt card with teaching context (WHAT, WHY, SCOPE, RISK)
    fn render_approval_prompt(&self, ctx: ApprovalPromptContext) {
        let ApprovalPromptContext {
            action,
            risk,
            description,
            task_context,
            scope,
            risk_reasoning,
            decision,
            rendering,
        } = ctx;

        rendering.lines.push(Line::default());
        let theme = rendering.theme;
        let risk_color = super::TranscriptEntry::risk_level_color(theme, risk);

        if rendering.is_compact() {
            let action_preview = description
                .or(scope)
                .map(|s| s.to_string())
                .unwrap_or_else(|| action.to_string());

            rendering.lines.push(Line::from(vec![
                Span::styled("[", Style::default().fg(theme.muted)),
                Span::styled("Approve:", Style::default().fg(theme.yellow)),
                Span::raw(" "),
                Span::styled(action_preview.clone(), Style::default().fg(risk_color)),
                Span::raw(" | "),
                Span::styled("[y/n/c]", Style::default().fg(theme.muted)),
            ]));
            return;
        }

        let content_width = rendering.width.saturating_sub(6);
        let content_style = Style::default().fg(theme.fg).bg(theme.panel_bg);
        let muted_style = Style::default().fg(theme.muted).bg(theme.panel_bg);
        let label_style = Style::default().fg(theme.cyan).bg(theme.panel_bg).bold();

        let mut content_lines: Vec<Line<'static>> = Vec::new();
        content_lines.push(Line::from(vec![
            Span::styled("Action: ", muted_style),
            Span::styled(format!("{} [{}]", action, risk), content_style),
        ]));

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![Span::styled(desc.to_string(), content_style)]));
                }
            }
            CardDetailLevel::Detailed => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHAT: ", label_style),
                        Span::styled(desc.to_string(), content_style),
                    ]));
                }

                if let Some(ctx) = task_context {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHY:  ", label_style),
                        Span::styled(ctx.to_string(), content_style),
                    ]));
                }

                if let Some(sc) = scope {
                    content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
                    self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
                }
            }
            CardDetailLevel::Verbose => {
                if let Some(desc) = description {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHAT: ", label_style),
                        Span::styled(desc.to_string(), content_style),
                    ]));
                }

                if let Some(ctx) = task_context {
                    content_lines.push(Line::from(vec![
                        Span::styled("WHY:  ", label_style),
                        Span::styled(ctx.to_string(), content_style),
                    ]));
                }

                if let Some(sc) = scope {
                    content_lines.push(Line::from(vec![Span::styled("SCOPE:", label_style)]));
                    self.wrap_text_styled(sc, content_style, content_width, &mut content_lines);
                }

                if let Some(reasoning) = risk_reasoning {
                    content_lines.push(Line::from(vec![
                        Span::styled("RISK:  ", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                        Span::styled(
                            reasoning.to_string(),
                            Style::default().fg(theme.fg).bg(theme.panel_bg).italic(),
                        ),
                    ]));
                }
            }
        }

        match decision {
            None => {
                let blink = rendering.animation_frame % 2 == 0;
                let focus_style = if blink {
                    Style::default().fg(theme.green).bg(theme.panel_bg).bold().reversed()
                } else {
                    Style::default().fg(theme.green).bg(theme.panel_bg).bold()
                };

                content_lines.push(Line::default());
                content_lines.push(Line::from(vec![
                    Span::styled("[", muted_style),
                    Span::styled("y", focus_style),
                    Span::styled("] approve  ", muted_style),
                    Span::styled("[", muted_style),
                    Span::styled("n", Style::default().fg(theme.red).bg(theme.panel_bg).bold()),
                    Span::styled("] reject  ", muted_style),
                    Span::styled("[", muted_style),
                    Span::styled("c", Style::default().fg(theme.yellow).bg(theme.panel_bg).bold()),
                    Span::styled("] cancel", muted_style),
                ]));

                match rendering.detail_level {
                    CardDetailLevel::Brief => {}
                    CardDetailLevel::Detailed => {
                        content_lines.push(Line::from(vec![Span::styled(
                            "[detailed mode - scope and risk assessment shown]",
                            muted_style,
                        )]));
                    }
                    CardDetailLevel::Verbose => {
                        content_lines.push(Line::from(vec![Span::styled(
                            "[verbose mode - full approval context available]",
                            muted_style,
                        )]));
                    }
                }
            }
            Some(super::ApprovalDecision::Approved) => {
                content_lines.push(Line::from(vec![
                    Span::styled("✓", Style::default().fg(theme.green).bg(theme.panel_bg)),
                    Span::styled(" Approved", Style::default().fg(theme.green).bg(theme.panel_bg)),
                ]));
            }
            Some(super::ApprovalDecision::Rejected) => {
                content_lines.push(Line::from(vec![
                    Span::styled("✗", Style::default().fg(theme.red).bg(theme.panel_bg)),
                    Span::styled(" Rejected", Style::default().fg(theme.red).bg(theme.panel_bg)),
                ]));
            }
            Some(super::ApprovalDecision::Cancelled) => {
                content_lines.push(Line::from(vec![
                    Span::styled("-", Style::default().fg(theme.yellow).bg(theme.panel_bg)),
                    Span::styled(" Cancelled", Style::default().fg(theme.yellow).bg(theme.panel_bg)),
                ]));
            }
        }

        let pulse_border = if decision.is_none() && rendering.animation_frame % 2 == 0 {
            risk_color
        } else if decision.is_none() {
            theme.muted
        } else {
            risk_color
        };

        self.render_card(
            "Approval Required",
            pulse_border,
            rendering.width,
            content_lines,
            rendering.lines,
        );
    }

    /// Render patch display with hunk-level intent labels
    fn render_patch_display(&self, ctx: PatchDisplayContext) {
        let PatchDisplayContext { patch_name, file_path, diff_content, hunk_labels, rendering } = ctx;
        let theme = rendering.theme;

        rendering.lines.push(Line::default());

        rendering.lines.push(Line::from(vec![
            Span::styled("Patch: ", Style::default().fg(theme.muted)),
            Span::styled(patch_name.to_string(), Style::default().fg(theme.fg).bold()),
        ]));

        rendering.lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("File: ", Style::default().fg(theme.muted)),
            Span::styled(file_path.to_string(), Style::default().fg(theme.cyan)),
        ]));

        let mut hunk_index = 0;
        let mut in_hunk = false;
        let mut current_hunk_lines = Vec::new();

        for line in diff_content.lines() {
            if line.starts_with("@@") {
                if !current_hunk_lines.is_empty() && in_hunk {
                    if hunk_index < hunk_labels.len()
                        && let Some(label) = &hunk_labels[hunk_index]
                    {
                        rendering.lines.push(Line::default());
                        rendering.lines.push(Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled("Note: ", Style::default().fg(theme.muted)),
                            Span::styled(label.clone(), Style::default().fg(theme.yellow).italic()),
                        ]));
                    }

                    for hunk_line in &current_hunk_lines {
                        rendering
                            .lines
                            .push(Line::from(vec![Span::raw(format!("  {}", hunk_line))]));
                    }
                    current_hunk_lines.clear();
                    hunk_index += 1;
                }

                in_hunk = true;
                current_hunk_lines.push(line.to_string());
            } else if in_hunk {
                current_hunk_lines.push(line.to_string());
            } else if line.starts_with("diff --git") || line.starts_with("index") {
                rendering.lines.push(Line::from(vec![Span::styled(
                    format!("  {}", line),
                    Style::default().fg(theme.muted),
                )]));
            }
        }

        if !current_hunk_lines.is_empty() && in_hunk {
            if hunk_index < hunk_labels.len()
                && let Some(label) = &hunk_labels[hunk_index]
            {
                rendering.lines.push(Line::default());
                rendering.lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled("Note: ", Style::default().fg(theme.muted)),
                    Span::styled(label.clone(), Style::default().fg(theme.yellow).italic()),
                ]));
            }

            for hunk_line in &current_hunk_lines {
                rendering
                    .lines
                    .push(Line::from(vec![Span::raw(format!("  {}", hunk_line))]));
            }
        }

        match rendering.detail_level {
            CardDetailLevel::Brief => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [press 'v' for detailed diff view]",
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
            CardDetailLevel::Detailed => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [detailed mode - full diff with intent labels]",
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
            CardDetailLevel::Verbose => {
                rendering.lines.push(Line::from(vec![Span::styled(
                    "  [verbose mode - all patch details shown]",
                    Style::default().fg(theme.muted).italic(),
                )]));
            }
        }
    }

    /// Render text with code block highlighting
    fn render_with_code_highlighting(
        &self, text: &str, default_color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let default_style = Style::default().fg(default_color).bg(self.theme.panel_bg);
        let mut remaining = text;

        while let Some(start_idx) = remaining.find(CODE_BLOCK_START) {
            if start_idx > 0 {
                let before_code = &remaining[..start_idx];
                self.wrap_text_styled(before_code, default_style, max_width, lines);
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
                self.wrap_text_styled(
                    after_start,
                    Style::default().fg(self.theme.cyan).bg(self.theme.panel_bg),
                    max_width,
                    lines,
                );
                break;
            }
        }

        if !remaining.is_empty() {
            self.wrap_text_styled(remaining, default_style, max_width, lines);
        }
    }

    /// Render a code block with syntax highlighting and boxed frame
    fn render_code_block(&self, code: &str, lang: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        let highlighter = SyntaxHighlighter::new();
        let highlighted = highlighter.highlight_code(code, lang.trim());
        let mut code_lines = Vec::new();

        for span in &highlighted {
            let styled = Span::styled(span.content.to_string(), span.style.bg(self.theme.panel_bg));
            code_lines.push(Line::from(vec![styled]));
        }

        let title = if lang.is_empty() { " Code ".to_string() } else { format!(" {} ", lang.to_uppercase()) };

        self.render_card(&title, self.theme.muted, width, code_lines, lines);
    }

    /// Render system message
    fn render_system_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(self.theme.muted)),
            Span::styled("System", Style::default().fg(self.theme.purple)),
            Span::styled("] ", Style::default().fg(self.theme.muted)),
        ]));
        self.wrap_text(content, self.theme.fg, width, lines);
    }

    /// Render thinking indicator (muted, indented)
    fn render_thinking_indicator(&self, duration_secs: f32, lines: &mut Vec<Line<'static>>) {
        let text = if duration_secs < 1.0 {
            "Thinking...".to_string()
        } else {
            format!("Thought for {:.0}s", duration_secs)
        };

        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(text, Style::default().fg(self.theme.muted).italic()),
        ]));
    }

    /// Render status line with triple colon prefix
    fn render_status_line(&self, message: &str, status_type: super::StatusType, lines: &mut Vec<Line<'static>>) {
        let status_color = match status_type {
            super::StatusType::Ready => self.theme.green,
            super::StatusType::Building => self.theme.blue,
            super::StatusType::Generating => self.theme.cyan,
            super::StatusType::WaitingApproval => self.theme.yellow,
            super::StatusType::Interrupted => self.theme.red,
            super::StatusType::Idle => self.theme.muted,
        };

        lines.push(Line::from(vec![
            Span::styled("::: ", Style::default().fg(self.theme.muted)),
            Span::styled(message.to_string(), Style::default().fg(status_color)),
        ]));
    }

    /// Render error entry with retry hint
    fn render_error_entry(
        &self, message: &str, error_type: ErrorType, can_retry: bool, context: Option<&str>, _width: usize,
        lines: &mut Vec<Line<'static>>,
    ) {
        lines.push(Line::default());

        let (type_label, type_color) = match error_type {
            ErrorType::Provider => ("Provider", self.theme.yellow),
            ErrorType::Network => ("Network", self.theme.yellow),
            ErrorType::SessionWrite => ("Session", self.theme.yellow),
            ErrorType::Terminal => ("Terminal", self.theme.red),
            ErrorType::Cancelled => ("Cancelled", self.theme.muted),
            ErrorType::Other => ("Error", self.theme.red),
        };

        let message = message.to_string();
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(self.theme.muted)),
            Span::styled(type_label, Style::default().fg(type_color)),
            Span::styled(" Error] ", Style::default().fg(self.theme.muted)),
            Span::styled(message.clone(), Style::default().fg(self.theme.red)),
        ]));

        if let Some(ctx) = context {
            let ctx = ctx.to_string();
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(ctx, Style::default().fg(self.theme.muted).italic()),
            ]));
        }

        if can_retry {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("Press R to retry", Style::default().fg(self.theme.cyan).italic()),
            ]));
        }
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
    use crate::{Theme, transcript::Transcript};

    #[test]
    fn test_renderer_new() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let _ = TranscriptRenderer::new(&transcript, theme);
    }

    #[test]
    fn test_renderer_with_entries() {
        let mut transcript = Transcript::new();
        transcript.add_user_message("Hello");
        transcript.add_model_response("Hi there");
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let _ = TranscriptRenderer::new(&transcript, theme);
    }

    #[test]
    fn test_wrap_text_basic() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();

        renderer.wrap_text("Hello world", theme.fg, 20, &mut lines);

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "Hello world");
    }

    #[test]
    fn test_wrap_text_with_wrap() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();
        renderer.wrap_text("This is a long line that should wrap", theme.fg, 20, &mut lines);
        assert!(lines.len() > 1);
        assert!(lines[0].to_string().contains("This"));
    }

    #[test]
    fn test_wrap_text_empty() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();
        renderer.wrap_text("", theme.fg, 20, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_newlines() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();

        renderer.wrap_text("Line 1\nLine 2\nLine 3", theme.fg, 20, &mut lines);

        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_wrap_text_zero_width() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello", theme.fg, 0, &mut lines);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_long_word() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();
        renderer.wrap_text("supercalifragilisticexpialidocious", theme.fg, 10, &mut lines);
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_wrap_text_unicode() {
        let transcript = Transcript::new();
        let theme = Theme::palette(crate::theme::ThemeVariant::Iceberg);
        let renderer = TranscriptRenderer::new(&transcript, theme);
        let mut lines = Vec::new();
        renderer.wrap_text("Hello 世界 🌍", theme.fg, 20, &mut lines);
        assert!(!lines.is_empty());
    }
}
