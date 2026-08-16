//! Ratatui adapters for renderer-owned rows.
//!
//! These helpers translate the pure row model into Ratatui cells. They do not
//! own transcript history, terminal modes, or viewport navigation.

use ratatui::Frame as RatatuiFrame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color as RatatuiColor, Modifier, Style};
use ratatui::text::{Line, Span as RatatuiSpan};
use ratatui::widgets::{Clear, Paragraph, Widget};

use super::row::{Frame, Row};
use super::style::{CellStyle, Color};

/// Render one complete renderer-owned frame through Ratatui.
pub fn render_logical_frame(frame: &mut RatatuiFrame<'_>, logical: &Frame) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let visible_height = area.height as usize;
    let source_start = logical.rows.len().saturating_sub(visible_height);
    let visible_rows = &logical.rows[source_start..];
    let destination_start = visible_height.saturating_sub(visible_rows.len());

    for (index, row) in visible_rows.iter().enumerate() {
        let y = area.y.saturating_add((destination_start + index) as u16);
        let row_area = Rect::new(area.x, y, area.width, 1);
        frame.render_widget(Paragraph::new(line_from_row(row)), row_area);
    }

    if !logical.cursor_visible {
        return;
    }
    let Some(cursor) = logical.cursor else {
        return;
    };
    if cursor.row < source_start {
        return;
    }

    let visible_row = cursor.row - source_start + destination_start;
    if visible_row >= visible_height {
        return;
    }
    let x = area
        .x
        .saturating_add((cursor.col as u16).min(area.width.saturating_sub(1)));
    let y = area.y.saturating_add(visible_row as u16);
    frame.set_cursor_position(Position::new(x, y));
}

/// Render transcript rows into an insertion buffer owned by Ratatui.
///
/// The inline terminal coordinator uses this adapter for
/// [`ratatui::Terminal::insert_before`], keeping all terminal writes inside
/// Ratatui.
pub fn render_rows_to_buffer(rows: &[Row], buffer: &mut Buffer) {
    let area = buffer.area;
    for (index, row) in rows.iter().take(area.height as usize).enumerate() {
        Paragraph::new(line_from_row(row)).render(Rect::new(area.x, area.y + index as u16, area.width, 1), buffer);
    }
}

fn line_from_row(row: &Row) -> Line<'static> {
    let spans = row
        .spans
        .iter()
        .map(|span| RatatuiSpan::styled(span.text.clone(), ratatui_style(span.style)))
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn ratatui_style(style: CellStyle) -> Style {
    let mut result = Style::default().fg(ratatui_color(style.fg)).bg(ratatui_color(style.bg));
    let mut modifiers = Modifier::empty();
    if style.bold {
        modifiers.insert(Modifier::BOLD);
    }
    if style.italic {
        modifiers.insert(Modifier::ITALIC);
    }
    if style.underlined {
        modifiers.insert(Modifier::UNDERLINED);
    }
    if style.dim {
        modifiers.insert(Modifier::DIM);
    }
    result = result.add_modifier(modifiers);
    result
}

fn ratatui_color(color: Color) -> RatatuiColor {
    match color {
        Color::Reset => RatatuiColor::Reset,
        Color::Black => RatatuiColor::Black,
        Color::DarkGrey => RatatuiColor::DarkGray,
        Color::Red => RatatuiColor::LightRed,
        Color::DarkRed => RatatuiColor::Red,
        Color::Green => RatatuiColor::LightGreen,
        Color::DarkGreen => RatatuiColor::Green,
        Color::Yellow => RatatuiColor::LightYellow,
        Color::DarkYellow => RatatuiColor::Yellow,
        Color::Blue => RatatuiColor::LightBlue,
        Color::DarkBlue => RatatuiColor::Blue,
        Color::Magenta => RatatuiColor::LightMagenta,
        Color::DarkMagenta => RatatuiColor::Magenta,
        Color::Cyan => RatatuiColor::LightCyan,
        Color::DarkCyan => RatatuiColor::Cyan,
        Color::White => RatatuiColor::White,
        Color::Grey => RatatuiColor::Gray,
        Color::Rgb { r, g, b } => RatatuiColor::Rgb(r, g, b),
        Color::AnsiValue(value) => RatatuiColor::Indexed(value),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::style::{Color as RatatuiColor, Modifier};

    use super::*;
    use crate::renderer::row::CursorCoord;
    use crate::renderer::style::Span;

    #[test]
    fn adapter_bottom_aligns_clipped_frame_and_translates_cursor() {
        let backend = TestBackend::new(8, 2);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut logical = Frame::new(8);
        logical.push(Row::padded(vec![Span::plain("old")], 8, CellStyle::default()));
        logical.push(Row::padded(vec![Span::plain("prompt")], 8, CellStyle::default()));
        logical.push(Row::padded(
            vec![Span::styled("status", CellStyle::new().fg(Color::Green).bold())],
            8,
            CellStyle::default(),
        ));
        logical.set_cursor(CursorCoord::new(1, 6));

        terminal
            .draw(|frame| render_logical_frame(frame, &logical))
            .expect("draw logical frame");

        let buffer = terminal.backend().buffer();
        let first_row = (0..8).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        let second_row = (0..8).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
        assert_eq!(first_row, "  prompt");
        assert_eq!(second_row, "  status");
        assert_eq!(buffer[(2, 1)].fg, RatatuiColor::LightGreen);
        assert!(buffer[(2, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(terminal.backend_mut().get_cursor_position(), Ok(Position::new(6, 0)));
    }
}
