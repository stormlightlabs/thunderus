use super::ChatApp;
use super::{StreamingState, formatters};
use crate::colors;
use crate::components::top_bordered_row_height;
use crate::components::{HintFooter, HintToken, TopBorderedInputRow};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use std::path::PathBuf;
use thndrs_core::estimate_token_cost_usd;

pub const INPUT_PROMPT_PREFIX: &str = "❯ ";

pub fn chat_input_row_height(input_content_line_count: u16) -> u16 {
    top_bordered_row_height(input_content_line_count)
}

pub fn draw_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("Enter"),
        HintToken::Text(" to send, "),
        HintToken::Key("Shift+Enter"),
        HintToken::Text(" newline, "),
        HintToken::Key("Ctrl+U"),
        HintToken::Text(" clear line, "),
        HintToken::Key("Ctrl+L"),
        HintToken::Text(" clear input, "),
        HintToken::Key("@"),
        HintToken::Text(" to pin files, "),
        HintToken::Key("Tab"),
        HintToken::Text(" to expand tools, "),
        HintToken::Key("Up/Down"),
        HintToken::Text(" history, "),
        HintToken::Key("ctrl+d"),
        HintToken::Text(" to quit"),
    ];
    HintFooter.render(frame, area, &tokens);
}

pub fn draw_token_usage_row(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let inner = TopBorderedInputRow.render_container(frame, area);
    let row_text = build_token_usage_text(app);
    let paragraph = Paragraph::new(row_text)
        .style(Style::default().fg(colors::TEXT_MUTED).bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

pub fn draw_input_area(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let show_cursor = app.streaming_state == StreamingState::Idle;
    let inner = TopBorderedInputRow.render_container(frame, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();

    if !app.pinned_files().is_empty() {
        lines.push(Line::from(vec![Span::styled(
            pinned_files_input_text(app.pinned_files()),
            Style::default().fg(colors::ACCENT_GREEN),
        )]));
    }

    let cursor_char = if show_cursor { "\u{2588}" } else { " " };
    let input_segments = app.input_buffer.split('\n').collect::<Vec<_>>();

    for (idx, segment) in input_segments.iter().enumerate() {
        let prefix = if idx == 0 { INPUT_PROMPT_PREFIX } else { "  " };
        let prefix_style = if idx == 0 {
            Style::default().fg(colors::ACCENT_CYAN)
        } else {
            Style::default().fg(colors::TEXT_MUTED)
        };

        let mut spans = vec![
            Span::styled(prefix, prefix_style),
            Span::styled(*segment, Style::default().fg(colors::TEXT_PRIMARY)),
        ];

        if idx + 1 == input_segments.len() {
            spans.push(Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)));
        }

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(INPUT_PROMPT_PREFIX, Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(cursor_char, Style::default().fg(colors::ACCENT_CYAN)),
        ]));
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

pub fn pinned_files_input_text(pinned_files: &[PathBuf]) -> String {
    let mut out = String::new();
    out.push('@');
    out.push(' ');
    for (idx, path) in pinned_files.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push('[');
        out.push_str(&path.display().to_string());
        out.push(']');
    }
    out
}

fn build_token_usage_text(app: &ChatApp) -> String {
    let pinned_summary = if app.pinned_files().is_empty() {
        "pinned: none".to_string()
    } else {
        let joined = app
            .pinned_files()
            .iter()
            .take(2)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if app.pinned_files().len() > 2 {
            format!(" (+{} more)", app.pinned_files().len() - 2)
        } else {
            String::new()
        };
        format!("pinned: {joined}{suffix}")
    };

    let Some(usage) = app.last_usage else {
        if let Some(warning) = app.submission_warning.as_deref() {
            return format!("Token usage appears here after the first response.  |  {pinned_summary}  |  {warning}");
        }
        return format!("Token usage appears here after the first response.  |  {pinned_summary}");
    };

    let mut parts = Vec::with_capacity(5);

    if let Some(model) = app.last_model.as_deref() {
        parts.push(format!("model: {model}"));
    }

    parts.push(format!(
        "prompt: {}",
        formatters::u32_with_grouping(usage.prompt_tokens)
    ));
    parts.push(format!(
        "completion: {}",
        formatters::u32_with_grouping(usage.completion_tokens)
    ));
    parts.push(format!("total: {}", formatters::u32_with_grouping(usage.total_tokens)));

    if let Some(model) = app.last_model.as_deref()
        && let Some(cost) = estimate_token_cost_usd(model, usage.prompt_tokens, usage.completion_tokens)
    {
        parts.push(format!("est: {}", formatters::usd(cost)));
    }

    parts.push(pinned_summary);
    if let Some(warning) = app.submission_warning.as_deref() {
        parts.push(warning.to_string());
    }
    parts.join("  |  ")
}
