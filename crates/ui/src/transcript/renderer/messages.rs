use crate::transcript::{ErrorType, StatusType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

impl<'a> super::TranscriptRenderer<'a> {
    /// Render user message with role prefix
    pub(super) fn render_user_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(self.theme.blue)),
            Span::styled("User", Style::default().fg(self.theme.blue).bold()),
        ]));
        self.render_accent_bar_message(content, self.theme.blue, self.theme.bg, width, lines);
    }

    /// Render message with accent bar on the left
    pub(super) fn render_accent_bar_message(
        &self, content: &str, accent_color: Color, bg_color: Color, width: usize, lines: &mut Vec<Line<'static>>,
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
    pub(super) fn render_model_response(
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

    /// Render system message (subtle, muted styling)
    pub(super) fn render_system_message(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        lines.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(self.theme.muted)),
            Span::styled("System", Style::default().fg(self.theme.muted).bold()),
        ]));
        self.render_system_message_body(content, width, lines);
    }

    pub(super) fn render_system_message_body(&self, content: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        let accent_bar = Span::styled("│", Style::default().fg(self.theme.cyan).bg(self.theme.bg));
        let content_style = Style::default().fg(self.theme.fg).bg(self.theme.bg);
        let content_width = width.saturating_sub(2);

        for source_line in content.lines() {
            if source_line.is_empty() {
                continue;
            }
            for wrapped_line in self.wrap_text_to_width(source_line, content_width) {
                lines.push(Line::from(vec![
                    accent_bar.clone(),
                    Span::styled(format!(" {}", wrapped_line), content_style),
                ]));
            }
        }
    }

    /// Render thinking indicator (muted, indented)
    pub(super) fn render_thinking_indicator(&self, duration_secs: f32, lines: &mut Vec<Line<'static>>) {
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
    pub(super) fn render_status_line(&self, message: &str, status_type: StatusType, lines: &mut Vec<Line<'static>>) {
        let status_color = match status_type {
            StatusType::Ready => self.theme.green,
            StatusType::Building => self.theme.blue,
            StatusType::Generating => self.theme.cyan,
            StatusType::WaitingApproval => self.theme.yellow,
            StatusType::Interrupted => self.theme.red,
            StatusType::Idle => self.theme.muted,
        };

        lines.push(Line::from(vec![
            Span::styled("::: ", Style::default().fg(self.theme.muted)),
            Span::styled(message.to_string(), Style::default().fg(status_color)),
        ]));
    }

    /// Render error entry with retry hint
    pub(super) fn render_error_entry(
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
}
