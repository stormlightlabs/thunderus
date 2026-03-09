use super::formatters;
use super::tool_render::draw_tool_call_widget;
use super::{ChatMessage, MessageRole, StreamingState};
use crate::colors;
use crate::components::{SectionBlock, SectionTone, wrapped_line_count};
use crate::layout::split as split_rects;
use chrono::{DateTime, Utc};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use thndrs_core::ResponseSections;

pub fn draw_message(frame: &mut Frame, area: Rect, msg: &ChatMessage, streaming_state: &StreamingState) {
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
                    draw_assistant_sections(
                        frame,
                        content_area,
                        sections,
                        msg.reasoning_content.as_deref(),
                        msg.created_at,
                        *streaming_state == StreamingState::Thinking,
                    );
                } else {
                    let is_streaming = *streaming_state == StreamingState::Streaming;
                    draw_assistant_raw(
                        frame,
                        content_area,
                        &msg.content,
                        msg.reasoning_content.as_deref(),
                        is_streaming,
                        *streaming_state == StreamingState::Thinking,
                        msg.created_at,
                    );
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

fn draw_assistant_raw(
    frame: &mut Frame, area: Rect, content: &str, reasoning_content: Option<&str>, is_streaming: bool,
    is_thinking: bool, created_at: DateTime<Utc>,
) {
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

    let content_area = if let Some(reasoning) = reasoning_content.filter(|value| !value.trim().is_empty()) {
        let reasoning_height = assistant_reasoning_height(reasoning, sections[1].width);
        let split = split_rects(
            sections[1],
            Direction::Vertical,
            vec![Constraint::Length(reasoning_height), Constraint::Min(0)],
        );
        if split.len() == 2 {
            draw_assistant_reasoning(frame, split[0], reasoning, is_thinking);
            split[1]
        } else {
            sections[1]
        }
    } else {
        sections[1]
    };

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
        content_area,
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

fn draw_assistant_sections(
    frame: &mut Frame, area: Rect, sections: &ResponseSections, reasoning_content: Option<&str>,
    created_at: DateTime<Utc>, is_thinking: bool,
) {
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

    let sections_area = if let Some(reasoning) = reasoning_content.filter(|value| !value.trim().is_empty()) {
        let reasoning_height = assistant_reasoning_height(reasoning, outer[1].width);
        let split = split_rects(
            outer[1],
            Direction::Vertical,
            vec![Constraint::Length(reasoning_height), Constraint::Min(0)],
        );
        if split.len() == 2 {
            draw_assistant_reasoning(frame, split[0], reasoning, is_thinking);
            split[1]
        } else {
            outer[1]
        }
    } else {
        outer[1]
    };

    let constraints = assistant_section_constraints(sections, sections_area.width);

    if constraints.is_empty() {
        return;
    }

    let layout = split_rects(sections_area, Direction::Vertical, constraints);

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

pub fn assistant_reasoning_height(reasoning: &str, width: u16) -> u16 {
    let content = formatters::normalize_display_content(reasoning);
    let content_width = width.saturating_sub(2).max(1);
    let content_lines = wrapped_line_count(&content, content_width) as u16;
    1 + content_lines.max(1)
}

fn draw_assistant_reasoning(frame: &mut Frame, area: Rect, reasoning: &str, is_thinking: bool) {
    let sections = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if sections.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("◌ ", Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(
                "Thinking",
                Style::default().fg(colors::TEXT_MUTED).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::default().bg(colors::BG_TERMINAL)),
        sections[0],
    );

    let mut lines = Vec::new();
    for raw_line in formatters::normalize_display_content(reasoning).lines() {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(raw_line.to_string(), Style::default().fg(colors::TEXT_MUTED)),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  ",
            Style::default().fg(colors::TEXT_MUTED),
        )]));
    }

    if is_thinking && let Some(last_line) = lines.last_mut() {
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

pub fn constraint_length(constraint: &Constraint) -> u16 {
    match *constraint {
        Constraint::Length(value) => value,
        _ => 0,
    }
}

pub fn draw_empty_state(frame: &mut Frame, area: Rect) {
    let text =
        Text::from("Start a conversation by typing a message below.").style(Style::default().fg(colors::TEXT_MUTED));
    let paragraph = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().bg(colors::BG_TERMINAL));

    let centered_y = area.y + area.height.saturating_sub(1) / 2;
    let centered_area = Rect::new(area.x, centered_y, area.width, 1);
    frame.render_widget(paragraph, centered_area);
}
