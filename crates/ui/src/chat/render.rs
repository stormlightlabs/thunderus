use super::{ChatApp, ChatMessage, MessageRole, StreamingState, ToolCallDisplay, ToolCallStatus};
use super::{extractors, formatters};
use crate::colors;
use crate::components::top_bordered_row_height;
use crate::components::{HintFooter, HintToken, SectionBlock, SectionTone, ToolCallCard, TopBorderedInputRow};
use crate::layout::split as split_rects;
use crate::tool::{draw_bash_output, draw_collapsible, draw_diff, draw_task_progress, parse_diff};
use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};
use std::path::PathBuf;
use thndrs_core::{ResponseSections, estimate_token_cost_usd};

pub const INPUT_PROMPT_PREFIX: &str = "❯ ";
pub const BASH_MAX_VISIBLE_LINES: usize = 50;

pub fn draw_chat_screen(frame: &mut Frame, app: &ChatApp) {
    let size = frame.area();

    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let main_layout = split_rects(
        size,
        Direction::Vertical,
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(top_bordered_row_height(1)),
            Constraint::Length(app.chat_input_row_height(size.width)),
        ],
    );
    if main_layout.len() < 4 {
        return;
    }

    draw_messages(frame, main_layout[0], app);
    draw_hints(frame, main_layout[1]);
    draw_token_usage_row(frame, main_layout[2], app);
    draw_input_area(frame, main_layout[3], app);

    if app.file_finder.active {
        app.draw_file_finder_overlay(frame, size);
    }
}

pub fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let container = Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(Padding::new(1, 1, 2, 1));
    frame.render_widget(container.clone(), area);

    let inner = container.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.messages.is_empty() {
        draw_empty_state(frame, inner);
        return;
    }

    let mut constraints = Vec::new();
    for (idx, msg) in app.messages.iter().enumerate() {
        constraints.push(Constraint::Length(msg.estimate_height(inner.width)));
        if idx + 1 < app.messages.len() {
            constraints.push(Constraint::Length(1));
        }
    }
    constraints.push(Constraint::Min(0));

    let layout = split_rects(inner, Direction::Vertical, constraints);

    let mut slot = 0usize;
    for (idx, msg) in app.messages.iter().enumerate() {
        if slot >= layout.len() {
            break;
        }

        draw_message(frame, layout[slot], msg, &app.streaming_state);
        slot += 1;

        if idx + 1 < app.messages.len() && slot < layout.len() {
            draw_message_divider(frame, layout[slot]);
            slot += 1;
        }
    }
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
        HintToken::Text(" to scroll, "),
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

fn draw_message_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 {
        return;
    }

    let divider = Paragraph::new("\u{2500}".repeat(area.width as usize))
        .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
    frame.render_widget(divider, area);
}

fn draw_message(frame: &mut Frame, area: Rect, msg: &ChatMessage, streaming_state: &StreamingState) {
    match msg.role {
        MessageRole::User => draw_user_message(frame, area, msg),
        MessageRole::Assistant => {
            let tool_constraints = msg
                .tool_calls
                .iter()
                .map(|tool_call| Constraint::Length(tool_call.estimate_height(area.width)))
                .collect::<Vec<_>>();

            let content_offset = tool_constraints.iter().map(constraint_length).sum::<u16>();
            if !msg.tool_calls.is_empty() {
                let mut message_constraints = tool_constraints;
                message_constraints.push(Constraint::Min(0));
                let message_layout = split_rects(area, Direction::Vertical, message_constraints);

                for (idx, tool_call) in msg.tool_calls.iter().enumerate() {
                    if idx < message_layout.len() {
                        draw_tool_call_widget(frame, message_layout[idx], tool_call);
                    }
                }
            }

            if area.height > content_offset {
                let content_area = Rect::new(
                    area.x,
                    area.y + content_offset,
                    area.width,
                    area.height - content_offset,
                );

                if let Some(ref sections) = msg.sections {
                    draw_assistant_sections(frame, content_area, sections, msg.created_at);
                } else {
                    let is_streaming = *streaming_state == StreamingState::Streaming;
                    draw_assistant_raw(frame, content_area, &msg.content, is_streaming, msg.created_at);
                }
            }
        }
        MessageRole::Tool => draw_tool_output(frame, area, msg),
    }
}

fn draw_user_message(frame: &mut Frame, area: Rect, message: &ChatMessage) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(message.role, message.created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let line = Line::from(vec![
        Span::styled("❯ ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(&message.content, Style::default().fg(colors::TEXT_PRIMARY)),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn draw_tool_output(frame: &mut Frame, area: Rect, message: &ChatMessage) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );

    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(message.role, message.created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let display_content = formatters::normalize_display_content(&message.content);
    let mut lines = Vec::new();

    for line in display_content.lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(line.to_string(), Style::default().fg(colors::TEXT_MUTED)),
        ]));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled("  ", Style::default())]));
    }

    let text = Text::from(lines);
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn draw_tool_call_widget(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let state = tool_call.to_ui_state();
    let formatted_args = formatters::tool_args(&tool_call.name, &tool_call.arguments, area.width);
    let summary = Text::from(formatted_args.clone());

    if !tool_call.expanded {
        ToolCallCard.render(frame, area, &tool_call.name, &formatted_args, state, summary);
        return;
    }

    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(ToolCallCard::collapsed_height()), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        ToolCallCard.render(frame, area, &tool_call.name, &formatted_args, state, summary);
        return;
    }

    ToolCallCard.render(frame, layout[0], &tool_call.name, &formatted_args, state, summary);
    draw_expanded_tool_details(frame, layout[1], tool_call);
}

fn draw_expanded_tool_details(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // FIXME: either this or the progress_tasks fn may be redundant
    if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        draw_task_progress(frame, area, &tool_call.progress_tasks(), "Tool progress");
        return;
    }

    match tool_call.name.as_str() {
        "read" => {
            draw_read_tool_output(frame, area, tool_call);
            return;
        }
        "write" => {
            draw_write_tool_output(frame, area, tool_call);
            return;
        }
        "edit" => {
            if let Some(diff_text) = extractors::edit_diff(&tool_call.arguments) {
                let diff_lines = parse_diff(&diff_text);
                if !diff_lines.is_empty() {
                    let path = extractors::diff_path(&diff_text)
                        .or_else(|| extractors::path_arg(&tool_call.arguments))
                        .unwrap_or_else(|| "edited file".to_string());
                    let layout = split_rects(
                        area,
                        Direction::Vertical,
                        vec![Constraint::Length(1), Constraint::Min(0)],
                    );
                    if layout.len() == 2 {
                        frame.render_widget(
                            Paragraph::new(Line::from(vec![Span::styled(
                                path,
                                Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
                            )]))
                            .style(Style::default().bg(colors::BG_TERMINAL)),
                            layout[0],
                        );
                        draw_diff(frame, layout[1], &diff_lines);
                        return;
                    }
                }
            }
        }
        "bash" => {
            let command = extractors::bash_cmd(&tool_call.arguments).unwrap_or_else(|| "bash".to_string());
            let (output, exit_code) = parse_bash_output(
                tool_call.output.as_deref().unwrap_or_default(),
                tool_call.status == ToolCallStatus::Error,
            );
            let normalized = formatters::tool_output("bash", &output);
            draw_bash_output(frame, area, &command, &normalized, exit_code);
            return;
        }
        "research" => {
            draw_research_tool_output(frame, area, tool_call);
            return;
        }
        _ => {}
    }

    let normalized = formatters::tool_output(&tool_call.name, tool_call.output.as_deref().unwrap_or_default());
    let content = Text::from(normalized);
    let label = "Tool output";
    draw_collapsible(frame, area, label, true, content);
}

fn draw_read_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let path = extractors::path_arg(&tool_call.arguments).unwrap_or_else(|| "read".to_string());
    let output = formatters::tool_output("read", tool_call.output.as_deref().unwrap_or_default());
    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            path,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]))
        .style(Style::default().bg(colors::BG_TERMINAL)),
        layout[0],
    );

    let lines = if tool_call.status == ToolCallStatus::Error {
        vec![Line::from(vec![Span::styled(
            output,
            Style::default().fg(colors::ACCENT_RED),
        )])]
    } else {
        build_read_output_lines(&output)
    };
    draw_lines_with_truncation(frame, layout[1], lines, colors::TEXT_MUTED);
}

fn draw_write_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let output = formatters::tool_output("write", tool_call.output.as_deref().unwrap_or_default());
    let line_text = if tool_call.status == ToolCallStatus::Success {
        formatters::write_success_ln(&output).unwrap_or_else(|| output.lines().next().unwrap_or_default().to_string())
    } else {
        output.lines().next().unwrap_or_default().to_string()
    };
    let style = if tool_call.status == ToolCallStatus::Success {
        Style::default().fg(colors::ACCENT_GREEN)
    } else {
        Style::default().fg(colors::ACCENT_RED)
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(line_text, style)]))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_research_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let url = extractors::research_url(&tool_call.arguments).unwrap_or_else(|| "research".to_string());
    let output = formatters::tool_output("research", tool_call.output.as_deref().unwrap_or_default());

    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            url,
            Style::default()
                .fg(colors::ACCENT_CYAN)
                .add_modifier(Modifier::UNDERLINED),
        )]))
        .style(Style::default().bg(colors::BG_TERMINAL)),
        layout[0],
    );

    let wrapped = wrap_text_lines(&output, layout[1].width);
    let lines = wrapped
        .into_iter()
        .map(|line| Line::from(vec![Span::styled(line, Style::default().fg(colors::TEXT_SECONDARY))]))
        .collect::<Vec<_>>();
    draw_lines_with_truncation(frame, layout[1], lines, colors::TEXT_MUTED);
}

pub fn build_read_output_lines(output: &str) -> Vec<Line<'static>> {
    if let Some(placeholder) = parse_read_image_placeholder(output) {
        return vec![Line::from(vec![Span::styled(
            placeholder,
            Style::default().fg(colors::TEXT_MUTED),
        )])];
    }

    let number_width = output
        .lines()
        .filter_map(|line| split_numbered_line(line).map(|(number, _)| number.len()))
        .max()
        .unwrap_or(1);

    let mut lines = Vec::new();
    for raw_line in output.lines() {
        if let Some((number, content)) = split_numbered_line(raw_line) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{number:>width$} ", width = number_width),
                    Style::default().fg(colors::TEXT_MUTED),
                ),
                Span::styled(content.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
            ]));
            continue;
        }

        if let Some(name) = raw_line.strip_prefix("▶ ") {
            lines.push(Line::from(vec![
                Span::styled("▶ ", Style::default().fg(colors::ACCENT_YELLOW)),
                Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
            ]));
            continue;
        }

        if let Some(name) = raw_line.strip_prefix("⌸ ") {
            lines.push(Line::from(vec![
                Span::styled("⌸ ", Style::default().fg(colors::TEXT_SECONDARY)),
                Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
            ]));
            continue;
        }

        if raw_line.is_empty() {
            lines.push(Line::from(vec![Span::raw("")]));
            continue;
        }

        lines.push(Line::from(vec![Span::styled(
            raw_line.to_string(),
            Style::default().fg(colors::TEXT_SECONDARY),
        )]));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::raw("")]));
    }

    lines
}

fn draw_lines_with_truncation(
    frame: &mut Frame, area: Rect, mut lines: Vec<Line<'static>>, indicator_color: ratatui::style::Color,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::raw("")]));
    }

    let max_lines = area.height as usize;
    if lines.len() > max_lines {
        let keep = max_lines.saturating_sub(1);
        let hidden = lines.len().saturating_sub(keep);
        lines.truncate(keep);
        lines.push(Line::from(vec![Span::styled(
            format!("[{hidden} lines hidden]"),
            Style::default().fg(indicator_color),
        )]));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub fn parse_bash_output(raw_output: &str, is_error: bool) -> (String, i32) {
    if !is_error {
        return (raw_output.to_string(), 0);
    }

    let Some(stripped) = raw_output.strip_prefix("Command exited with code ") else {
        return (raw_output.to_string(), 1);
    };

    let mut parts = stripped.splitn(2, '\n');
    let Some(code) = parts.next().unwrap_or_default().trim().parse::<i32>().ok() else {
        return (raw_output.to_string(), 1);
    };

    let output = parts.next().unwrap_or_default().to_string();
    (output, code)
}

pub fn bash_visible_line_count(output: &str, max_visible_lines: usize) -> usize {
    if max_visible_lines == 0 {
        return 1;
    }

    let lines = output.lines().count().max(1);
    if lines <= max_visible_lines { lines } else { max_visible_lines + 1 }
}

fn split_numbered_line(line: &str) -> Option<(&str, &str)> {
    let (number_part, content) = line.split_once('\t')?;
    let number = number_part.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((number, content))
}

fn parse_read_image_placeholder(output: &str) -> Option<String> {
    let header = output.lines().next()?.trim();
    let body = header.strip_prefix("[Image: ")?.strip_suffix(']')?;

    let mut size = "unknown size".to_string();
    let mut mime = "unknown".to_string();
    for part in body.split(',').map(str::trim) {
        if part.ends_with("bytes") {
            size = part.to_string();
        } else if let Some(value) = part.strip_prefix("mime: ") {
            mime = value.to_string();
        }
    }

    Some(format!("[image: {mime}, {size}]"))
}

fn wrap_text_lines(content: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut wrapped = Vec::new();

    for line in content.lines() {
        let chars = line.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        for chunk in chars.chunks(width) {
            wrapped.push(chunk.iter().collect());
        }
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn draw_assistant_raw(frame: &mut Frame, area: Rect, content: &str, is_streaming: bool, created_at: DateTime<Utc>) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(MessageRole::Assistant, created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let mut lines = Vec::new();
    let display_content = formatters::normalize_display_content(content);

    if is_streaming && display_content.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("◉ ", Style::default().fg(colors::ACCENT_PURPLE)),
            Span::styled("Thinking...", Style::default().fg(colors::TEXT_MUTED)),
        ]));
    } else {
        for (idx, raw_line) in display_content.lines().enumerate() {
            let prefix = if idx == 0 { "◉ " } else { "  " };
            let prefix_style = if idx == 0 {
                Style::default().fg(colors::ACCENT_PURPLE)
            } else {
                Style::default().fg(colors::TEXT_MUTED)
            };
            let trimmed = raw_line.trim_start();

            let line = if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled("• ", Style::default().fg(colors::ACCENT_CYAN)),
                    Span::styled(item.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ])
            } else if trimmed.is_empty() {
                Line::from(vec![Span::styled(prefix, prefix_style)])
            } else if trimmed.ends_with(':') {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(
                        trimmed.to_string(),
                        Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(trimmed.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ])
            };

            lines.push(line);
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "◉ ",
            Style::default().fg(colors::ACCENT_PURPLE),
        )]));
    }

    if is_streaming && let Some(last_line) = lines.last_mut() {
        last_line
            .spans
            .push(Span::styled(" █", Style::default().fg(colors::ACCENT_CYAN)));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn message_header_line(role: MessageRole, created_at: DateTime<Utc>) -> Line<'static> {
    Line::from(vec![
        Span::styled(role.label(), Style::default().fg(colors::TEXT_MUTED)),
        Span::styled("  ", Style::default().fg(colors::TEXT_MUTED)),
        Span::styled(
            formatters::msg_timestamp(created_at),
            Style::default().fg(colors::TEXT_MUTED),
        ),
    ])
}

fn draw_assistant_sections(frame: &mut Frame, area: Rect, sections: &ResponseSections, created_at: DateTime<Utc>) {
    let outer = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if outer.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(message_header_line(MessageRole::Assistant, created_at))
            .style(Style::default().bg(colors::BG_TERMINAL)),
        outer[0],
    );

    let constraints = assistant_section_constraints(sections, outer[1].width);

    if constraints.is_empty() {
        return;
    }

    let layout = split_rects(outer[1], Direction::Vertical, constraints);

    let mut slot = 0;

    if let Some(ref intent) = sections.intent
        && slot < layout.len()
    {
        SectionBlock.render(
            frame,
            layout[slot],
            SectionTone::Intent,
            "◉",
            "Intent",
            Text::from(intent.clone()),
        );
        slot += 1;
    }

    if let Some(ref actions) = sections.actions
        && slot < layout.len()
    {
        SectionBlock.render(
            frame,
            layout[slot],
            SectionTone::Actions,
            "⚡",
            "Actions",
            Text::from(actions.clone()),
        );
        slot += 1;
    }

    if let Some(ref result) = sections.result
        && slot < layout.len()
    {
        SectionBlock.render(
            frame,
            layout[slot],
            SectionTone::Result,
            "✓",
            "Result",
            Text::from(result.clone()),
        );
        slot += 1;
    }

    if let Some(ref next) = sections.next
        && slot < layout.len()
    {
        SectionBlock.render(
            frame,
            layout[slot],
            SectionTone::Next,
            "→",
            "Next",
            Text::from(next.clone()),
        );
    }
}

pub fn assistant_section_constraints(sections: &ResponseSections, width: u16) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    let content_width = width.saturating_sub(2);

    if let Some(intent) = &sections.intent {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            intent,
            content_width,
            4,
        )));
    }
    if let Some(actions) = &sections.actions {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            actions,
            content_width,
            5,
        )));
    }
    if let Some(result) = &sections.result {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            result,
            content_width,
            6,
        )));
    }
    if let Some(next) = &sections.next {
        constraints.push(Constraint::Length(SectionBlock::estimate_height(
            next,
            content_width,
            4,
        )));
    }

    constraints
}

pub fn constraint_length(constraint: &Constraint) -> u16 {
    match *constraint {
        Constraint::Length(value) => value,
        _ => 0,
    }
}

fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let text =
        Text::from("Start a conversation by typing a message below.").style(Style::default().fg(colors::TEXT_MUTED));
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().bg(colors::BG_TERMINAL));

    let centered_y = area.y + area.height.saturating_sub(1) / 2;
    let centered_area = Rect::new(area.x, centered_y, area.width, 1);
    frame.render_widget(paragraph, centered_area);
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
    parts.join("  |  ")
}

pub fn wrapped_multiline_input_line_count(input: &str, first_width: u16, continuation_width: u16) -> usize {
    fn wrapped_segment_line_count(segment: &str, width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        segment.chars().count().div_ceil(width as usize).max(1)
    }

    let mut segments = input.split('\n');
    let mut total = 0usize;

    if let Some(first) = segments.next() {
        total += wrapped_segment_line_count(first, first_width.max(1));
    } else {
        return 1;
    }

    for segment in segments {
        total += wrapped_segment_line_count(segment, continuation_width.max(1));
    }

    total.max(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_read_image_placeholder() {
        let output = "[Image: 123 bytes, mime: image/png]\nbase64:AAAA";
        let placeholder = parse_read_image_placeholder(output).expect("placeholder should parse");
        assert_eq!(placeholder, "[image: image/png, 123 bytes]");
    }

    #[test]
    fn test_parse_bash_output_with_error_prefix() {
        let (output, code) = parse_bash_output("Command exited with code 2\nstderr text", true);
        assert_eq!(code, 2);
        assert_eq!(output, "stderr text");
    }

    #[test]
    fn test_parse_bash_output_success_keeps_literal_prefix_text() {
        let literal = "Command exited with code 2\nbut this is command output";
        let (output, code) = parse_bash_output(literal, false);
        assert_eq!(code, 0);
        assert_eq!(output, literal);
    }

    #[test]
    fn test_parse_bash_output_error_without_prefix_defaults_to_non_zero_exit() {
        let (output, code) = parse_bash_output("Command timed out after 120 seconds", true);
        assert_eq!(code, 1);
        assert_eq!(output, "Command timed out after 120 seconds");
    }
}
