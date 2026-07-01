//! Crossterm terminal backend for the direct renderer.
//!
//! Generic over `W: Write` so the row-writing logic can be unit-tested against
//! a [`Vec<u8>`] buffer. The backend owns raw-mode setup/teardown, cursor
//! hide/show, clearing live rows, cursor movement, and writing styled rows.
//!
//! The row model is independent of crossterm I/O; this module is the only place
//! that translates [`super::Row`] into ANSI escape sequences.

#![allow(dead_code)]

use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style as cts;
use crossterm::terminal::{Clear as CtClear, ClearType};
use crossterm::{QueueableCommand, queue};

use super::row::{CursorCoord, Frame, Row};
use super::style::{CellStyle, Color, Span};

/// A crossterm-backed terminal output.
///
/// Wraps any `Write` sink. In production this is `io::Stdout`; in tests it is a
/// `Vec<u8>` so writes can be asserted on without a real terminal.
pub struct TerminalBackend<W: Write> {
    writer: W,
    width: u16,
    height: u16,
}

impl<W: Write> TerminalBackend<W> {
    /// Create a backend over `writer` with the given terminal size.
    pub fn new(writer: W, width: u16, height: u16) -> Self {
        TerminalBackend { writer, width, height }
    }

    /// Replace the terminal size (e.g. on resize).
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    /// Current terminal width in columns.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Current terminal height in rows.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Access the underlying writer (for direct queueing when needed).
    pub fn writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Hide the cursor.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Hide)
    }

    /// Show the cursor.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Show)
    }

    /// Clear `count` rows starting at `top_row` (0-based, from the top of the
    /// live region).
    pub fn clear_rows(&mut self, top_row: u16, count: u16) -> io::Result<()> {
        for i in 0..count {
            let row = top_row + i;
            queue!(self.writer, MoveTo(0, row), CtClear(ClearType::CurrentLine))?;
        }
        self.writer.flush()
    }

    /// Move the cursor to a coordinate within the live region.
    pub fn move_cursor(&mut self, coord: CursorCoord) -> io::Result<()> {
        queue!(self.writer, MoveTo(coord.col as u16, coord.row as u16))
    }

    /// Write a single styled row at `row` (0-based). Does not move to a new
    /// line afterward.
    pub fn write_row(&mut self, row: usize, styled: &Row) -> io::Result<()> {
        queue!(self.writer, MoveTo(0, row as u16))?;
        write_spans(&mut self.writer, &styled.spans, styled.width)?;
        self.writer.flush()
    }

    /// Write multiple rows starting at `top_row`.
    pub fn write_rows(&mut self, top_row: u16, rows: &[Row]) -> io::Result<()> {
        for (i, row) in rows.iter().enumerate() {
            let y = top_row + i as u16;
            queue!(self.writer, MoveTo(0, y))?;
            write_spans(&mut self.writer, &row.spans, row.width)?;
        }
        self.writer.flush()
    }

    /// Write committed (scrollback) rows once. Each row is followed by a
    /// newline so it becomes part of native terminal scrollback.
    pub fn write_committed(&mut self, rows: &[Row]) -> io::Result<()> {
        for row in rows {
            write_spans(&mut self.writer, &row.spans, row.width)?;
            writeln!(self.writer)?;
        }
        self.writer.flush()
    }

    /// Render a complete frame: clear the live region, write all rows, then
    /// place the cursor (if any).
    pub fn render_frame(&mut self, frame: &Frame, top_row: u16) -> io::Result<()> {
        let count = frame.rows.len() as u16;
        self.clear_rows(top_row, count)?;
        self.write_rows(top_row, &frame.rows)?;
        if let Some(cursor) = frame.cursor {
            self.move_cursor(CursorCoord { row: top_row as usize + cursor.row, col: cursor.col })?;
        }
        self.writer.flush()
    }

    /// Flush any buffered output.
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// Write a span slice as styled content, padding to `width` with the last
/// span's background (or reset) if needed.
fn write_spans(writer: &mut impl Write, spans: &[Span], width: usize) -> io::Result<()> {
    let mut used = 0usize;
    let mut last_bg = Color::Reset;

    for span in spans {
        if used >= width {
            break;
        }
        let remaining = width - used;
        let text: String = if span.text.chars().count() > remaining {
            span.text.chars().take(remaining).collect()
        } else {
            span.text.clone()
        };
        let taken = text.chars().count();
        used += taken;
        last_bg = span.style.bg;

        let style = style_to_crossterm(span.style);
        if !text.is_empty() {
            queue!(writer, cts::PrintStyledContent(style.apply(text)))?;
        }
    }

    if used < width {
        let mut pad_style = cts::ContentStyle::new();
        pad_style.background_color = color_to_crossterm(last_bg);
        queue!(
            writer,
            cts::PrintStyledContent(pad_style.apply(" ".repeat(width - used)))
        )?;
    }

    queue!(writer, cts::ResetColor)?;
    Ok(())
}

/// Convert a renderer [`CellStyle`] to a crossterm [`cts::ContentStyle`].
pub(crate) fn style_to_crossterm(style: CellStyle) -> cts::ContentStyle {
    let mut out = cts::ContentStyle::new();
    out.foreground_color = color_to_crossterm(style.fg);
    out.background_color = color_to_crossterm(style.bg);
    out.attributes = modifiers_to_crossterm(style);
    out
}

/// Convert a renderer [`Color`] to a crossterm [`CtColor`].
pub(crate) fn color_to_crossterm(color: Color) -> Option<cts::Color> {
    Some(match color {
        Color::Reset => cts::Color::Reset,
        Color::Black => cts::Color::Black,
        Color::DarkRed => cts::Color::DarkRed,
        Color::DarkGreen => cts::Color::DarkGreen,
        Color::DarkYellow => cts::Color::DarkYellow,
        Color::DarkBlue => cts::Color::DarkBlue,
        Color::DarkMagenta => cts::Color::DarkMagenta,
        Color::DarkCyan => cts::Color::DarkCyan,
        Color::Grey => cts::Color::Grey,
        Color::DarkGrey => cts::Color::DarkGrey,
        Color::Red => cts::Color::Red,
        Color::Green => cts::Color::Green,
        Color::Yellow => cts::Color::Yellow,
        Color::Blue => cts::Color::Blue,
        Color::Magenta => cts::Color::Magenta,
        Color::Cyan => cts::Color::Cyan,
        Color::White => cts::Color::White,
        Color::Rgb { r, g, b } => cts::Color::Rgb { r, g, b },
    })
}

/// Convert renderer style booleans into crossterm attributes.
fn modifiers_to_crossterm(style: CellStyle) -> cts::Attributes {
    let mut attributes = cts::Attributes::none();
    if style.bold {
        attributes = attributes.with(cts::Attribute::Bold);
    }
    if style.italic {
        attributes = attributes.with(cts::Attribute::Italic);
    }
    if style.underlined {
        attributes = attributes.with(cts::Attribute::Underlined);
    }
    if style.dim {
        attributes = attributes.with(cts::Attribute::Dim);
    }
    attributes
}

/// Enter raw mode on the real terminal.
pub fn enter_raw_mode() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()
}

/// Leave raw mode on the real terminal.
pub fn leave_raw_mode() -> io::Result<()> {
    crossterm::terminal::disable_raw_mode()
}

/// Read the current terminal size from crossterm, falling back to 80×24.
pub fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend(width: u16, height: u16) -> TerminalBackend<Vec<u8>> {
        TerminalBackend::new(Vec::new(), width, height)
    }

    #[test]
    fn write_row_outputs_styled_text_and_padding() {
        let mut b = backend(10, 5);
        let row = Row::padded(
            vec![Span::styled("hi", CellStyle::new().fg(Color::Red).bg(Color::Blue))],
            6,
            CellStyle::default(),
        );
        b.write_row(0, &row).unwrap();
        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("hi"));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn write_row_pads_to_width() {
        let mut b = backend(10, 5);
        let row = Row::padded(vec![Span::plain("ab")], 6, CellStyle::new().bg(Color::Blue));
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("ab"));
        assert!(out.ends_with("\x1b[0m"));
    }

    #[test]
    fn write_committed_appends_newlines() {
        let mut b = backend(20, 10);
        let rows = vec![
            Row::padded(vec![Span::plain("hello")], 10, CellStyle::default()),
            Row::padded(vec![Span::plain("world")], 10, CellStyle::default()),
        ];
        b.write_committed(&rows).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
        assert_eq!(out.matches('\n').count(), 2);
    }

    #[test]
    fn render_frame_clears_writes_and_moves_cursor() {
        let mut b = backend(20, 10);
        let mut frame = Frame::new(20);
        frame.push(Row::padded(vec![Span::plain("hi")], 5, CellStyle::default()));
        frame.set_cursor(CursorCoord::new(0, 2));
        b.render_frame(&frame, 0).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("hi"));
        assert!(out.contains("\x1b[1;3H"));
    }

    #[test]
    fn clear_rows_emits_clear_current_line() {
        let mut b = backend(10, 5);
        b.clear_rows(0, 2).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[2K"));
    }

    #[test]
    fn hide_and_show_cursor_queue_sequences() {
        let mut b = backend(10, 5);
        b.hide_cursor().unwrap();
        b.show_cursor().unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[?25l"));
        assert!(out.contains("\x1b[?25h"));
    }

    #[test]
    fn style_to_crossterm_maps_colors_and_attrs() {
        let style = CellStyle::new().fg(Color::Red).bg(Color::Blue).bold().italic();
        let cs = style_to_crossterm(style);
        assert_eq!(cs.foreground_color, Some(cts::Color::Red));
        assert_eq!(cs.background_color, Some(cts::Color::Blue));
        assert!(cs.attributes.has(cts::Attribute::Bold));
        assert!(cs.attributes.has(cts::Attribute::Italic));
    }

    #[test]
    fn color_to_crossterm_rgb() {
        let c = color_to_crossterm(Color::Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(c, Some(cts::Color::Rgb { r: 1, g: 2, b: 3 }));
    }

    #[test]
    fn set_size_updates_dimensions() {
        let mut b = backend(80, 24);
        b.set_size(100, 30);
        assert_eq!(b.width(), 100);
        assert_eq!(b.height(), 30);
    }

    #[test]
    fn write_spans_truncates_to_width() {
        let mut buf = Vec::new();
        let spans = vec![Span::plain("hello world")];
        write_spans(&mut buf, &spans, 5).unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("hello"));
        assert!(!out.contains("world"));
    }
}
