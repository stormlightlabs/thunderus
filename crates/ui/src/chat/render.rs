use super::ChatApp;
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

    let main_layout = split_rects(
        size,
        Direction::Vertical,
        vec![
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(input_render::chat_input_row_height(1)),
            Constraint::Length(app.chat_input_row_height(size.width)),
        ],
    );
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

pub fn draw_messages(frame: &mut Frame, area: Rect, app: &ChatApp) {
    let container = Block::default()
        .style(Style::default().bg(colors::BG_TERMINAL))
        .padding(ratatui::widgets::Padding::new(1, 1, 2, 1));
    frame.render_widget(container.clone(), area);

    let inner = container.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.messages.is_empty() {
        message_render::draw_empty_state(frame, inner);
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

        message_render::draw_message(frame, layout[slot], msg, &app.streaming_state);
        slot += 1;

        if idx + 1 < app.messages.len() && slot < layout.len() {
            draw_message_divider(frame, layout[slot]);
            slot += 1;
        }
    }
}

fn draw_message_divider(frame: &mut Frame, area: Rect) {
    if area.width == 0 {
        return;
    }

    let divider = Paragraph::new("\u{2500}".repeat(area.width as usize))
        .style(Style::default().fg(colors::BORDER_COLOR).bg(colors::BG_TERMINAL));
    frame.render_widget(divider, area);
}
