use super::ChatApp;
use super::ChatMessage;
use super::input_render;
use super::message_render;
use crate::colors;
use crate::layout::split as split_rects;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph};

pub fn draw_chat_screen(frame: &mut Frame, app: &ChatApp) {
    let size = frame.area();

    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let main_layout = chat_main_layout(size, app.chat_input_row_height(size.width));
    if main_layout.len() < 4 {
        return;
    }

    draw_messages(frame, main_layout[0], app);
    input_render::draw_hints(frame, main_layout[1]);
    input_render::draw_token_usage_row(frame, main_layout[2], app);
    input_render::draw_input_area(frame, main_layout[3], app);

    if app.file_finder.active {
        app.draw_file_finder_overlay(frame, size);
    }
}

pub(super) fn chat_main_layout(area: Rect, input_row_height: u16) -> Vec<Rect> {
    split_rects(
        area,
        Direction::Vertical,
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(input_render::chat_input_row_height(1)),
            Constraint::Length(input_row_height),
        ],
    )
}

pub(super) fn message_viewport_area(area: Rect) -> Rect {
    Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(1, 1, 2, 1))
        .inner(area)
}

pub fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let container = Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(1, 1, 2, 1));
    frame.render_widget(container.clone(), area);

    let inner = message_viewport_area(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.messages.is_empty() {
        message_render::draw_empty_state(frame, inner);
        return;
    }

    let start = app.rendered_messages.min(app.messages.len().saturating_sub(1));
    let visible_messages = &app.messages[start..];

    let total_rows = transcript_row_count(visible_messages, inner.width);
    let viewport_top = total_rows.saturating_sub(inner.height as usize);

    let mut transcript_row = 0usize;
    let inner_bottom = inner.y.saturating_add(inner.height);
    let mut cursor_y = inner.y;

    for (idx, msg) in visible_messages.iter().enumerate() {
        let message_height = msg.estimate_height(inner.width).max(1) as usize;
        let message_bottom = transcript_row.saturating_add(message_height);
        let skip_rows = viewport_top.saturating_sub(transcript_row);

        if message_bottom > viewport_top {
            let available = inner_bottom.saturating_sub(cursor_y);
            if available == 0 {
                break;
            }

            let visible_rows = message_height.saturating_sub(skip_rows);
            let draw_height = (visible_rows as u16).min(available);
            if draw_height == 0 {
                transcript_row = message_bottom;
                continue;
            }
            let area = Rect::new(inner.x, cursor_y, inner.width, draw_height);
            if skip_rows == 0 {
                message_render::draw_message(frame, area, msg, &app.streaming_state);
            } else {
                message_render::draw_message_with_offset(frame, area, msg, &app.streaming_state, skip_rows);
            }
            cursor_y = cursor_y.saturating_add(draw_height);
        }

        transcript_row = message_bottom;

        if idx + 1 >= visible_messages.len() {
            continue;
        }

        let divider_row = transcript_row;
        transcript_row = transcript_row.saturating_add(1);
        if divider_row < viewport_top {
            continue;
        }

        if cursor_y >= inner_bottom {
            break;
        }

        draw_message_divider(frame, Rect::new(inner.x, cursor_y, inner.width, 1));
        cursor_y = cursor_y.saturating_add(1);
    }
}

fn transcript_row_count(messages: &[ChatMessage], content_width: u16) -> usize {
    let mut rows = 0usize;
    for (idx, message) in messages.iter().enumerate() {
        rows = rows.saturating_add(message.estimate_height(content_width).max(1) as usize);
        if idx + 1 < messages.len() {
            rows = rows.saturating_add(1);
        }
    }
    rows
}

fn draw_message_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 {
        return;
    }

    let divider = Paragraph::new("\u{2500}".repeat(area.width as usize))
        .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
    frame.render_widget(divider, area);
}
