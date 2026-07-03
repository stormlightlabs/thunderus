//! Crossterm terminal backend for the direct renderer.
//!
//! Generic over `W: Write` so the row-writing logic can be unit-tested against
//! a [`Vec<u8>`] buffer. The backend owns raw-mode setup/teardown, cursor
//! hide/show, clearing viewport rows, cursor movement, and writing styled rows.
//!
//! The row model is independent of crossterm I/O; this module is the only place
//! that translates [`super::Row`] into ANSI escape sequences.

use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::queue;
use crossterm::style as cts;
use crossterm::terminal::{Clear as CtClear, ClearType};

use crate::utils;

use super::row::{CursorCoord, Frame, Row};
use super::style::{CellStyle, Color, Span};
use unicode_segmentation::UnicodeSegmentation;

/// Set the terminal scroll region (DECSTBM) to constrain scrolling to a
/// range of rows. Rows are 1-based per the ANSI spec.
struct SetScrollRegion(pub std::ops::Range<u16>);

impl crossterm::Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "SetScrollRegion not supported via WinAPI",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

/// Reset the terminal scroll region to the full screen.
struct ResetScrollRegion;

impl crossterm::Command for ResetScrollRegion {
    fn write_ansi(&self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        f.write_str("\x1b[r")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ResetScrollRegion not supported via WinAPI",
        ))
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

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

    /// Access the underlying writer for assertions.
    #[cfg(test)]
    pub fn writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Show the cursor.
    pub fn show_cursor(&mut self) -> io::Result<()> {
        queue!(self.writer, Show)
    }

    /// Clear `count` rows starting at `top_row` (0-based, from the top of the
    /// live region).
    ///
    /// Only queues escape sequences; the caller is responsible for flushing
    /// after all rows (cleared and written) are queued so the terminal never
    /// sees an intermediate blank frame.
    pub fn clear_rows(&mut self, top_row: u16, count: u16) -> io::Result<()> {
        for i in 0..count {
            let row = top_row + i;
            queue!(self.writer, MoveTo(0, row), CtClear(ClearType::CurrentLine))?;
        }
        Ok(())
    }

    /// Clear the visible screen.
    pub fn clear_all(&mut self) -> io::Result<()> {
        queue!(self.writer, MoveTo(0, 0), CtClear(ClearType::All))?;
        self.writer.flush()
    }

    /// Clear the visible screen and purge terminal scrollback where supported.
    pub fn clear_all_and_scrollback(&mut self) -> io::Result<()> {
        queue!(
            self.writer,
            MoveTo(0, 0),
            CtClear(ClearType::All),
            CtClear(ClearType::Purge)
        )?;
        self.writer.flush()
    }

    /// Insert history rows above the viewport into the terminal's native
    /// scrollback.
    ///
    /// Uses scroll-region escape sequences: sets a scroll region from the top
    /// of the screen to `viewport_top`, positions the cursor at the bottom of
    /// that region, and writes each row with `\r\n` + styled content +
    /// `Clear(UntilNewLine)`. This pushes existing content up into the
    /// terminal's scrollback buffer, where the user can scroll back to it
    /// natively (Shift+PageUp, mouse wheel when not captured, etc.).
    ///
    /// After insertion, the scroll region is reset and the cursor is returned
    /// to its previous position.
    pub fn insert_history_lines(&mut self, rows: &[Row], viewport_top: u16) -> io::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }

        if viewport_top == 0 {
            self.write_rows(0, rows)?;
            return Ok(());
        }

        queue!(self.writer, SetScrollRegion(1..viewport_top))?;

        let cursor_top = viewport_top.saturating_sub(1);
        queue!(self.writer, MoveTo(0, cursor_top))?;

        for row in rows {
            queue!(self.writer, crossterm::style::Print("\r\n"))?;
            write_screen_row(&mut self.writer, row)?;
        }

        queue!(self.writer, ResetScrollRegion)?;
        self.writer.flush()
    }

    /// Move the cursor to a coordinate within the live region.
    pub fn move_cursor(&mut self, coord: CursorCoord) -> io::Result<()> {
        queue!(self.writer, MoveTo(coord.col as u16, coord.row as u16))
    }

    /// Write a single styled row at `row` (0-based).
    ///
    /// Does not move to a new line afterward.
    #[cfg(test)]
    pub fn write_row(&mut self, row: usize, styled: &Row) -> io::Result<()> {
        queue!(self.writer, MoveTo(0, row as u16))?;
        write_screen_row(&mut self.writer, styled)?;
        self.writer.flush()
    }

    /// Write multiple rows starting at `top_row`.
    ///
    /// Only queues escape sequences; the caller is responsible for flushing
    /// after all writes are complete so the terminal never sees an
    /// intermediate partial frame.
    pub fn write_rows(&mut self, top_row: u16, rows: &[Row]) -> io::Result<()> {
        for (i, row) in rows.iter().enumerate() {
            let y = top_row + i as u16;
            queue!(self.writer, MoveTo(0, y))?;
            write_screen_row(&mut self.writer, row)?;
        }
        Ok(())
    }

    /// Render a complete frame: write all rows (each row clears its own line
    /// via `Clear(UntilNewLine)`), then place the cursor (if any).
    ///
    /// A separate "clear all rows first" pass is intentionally avoided: it
    /// would send a blank frame to the terminal before content arrives,
    /// causing visible flicker. Each row's `write_screen_row` clears stale
    /// content and then paints the padded row background explicitly.
    pub fn render_frame(&mut self, frame: &Frame, top_row: u16) -> io::Result<()> {
        self.write_rows(top_row, &frame.rows)?;
        if frame.cursor_visible {
            if let Some(cursor) = frame.cursor {
                self.move_cursor(CursorCoord { row: top_row as usize + cursor.row, col: cursor.col })?;
            }
            queue!(self.writer, Show)?;
        } else {
            queue!(self.writer, Hide)?;
        }
        self.writer.flush()
    }

    /// Render only rows that differ from `prev`, leaving unchanged rows on
    /// screen untouched.
    ///
    /// This is the core anti-flicker mechanism: on a typical tick the vast
    /// majority of rows are identical, so only a handful of escape sequences
    /// are emitted. Rows beyond the new frame's length (i.e. rows present in
    /// `prev` but not in `frame`) are cleared.
    pub fn render_frame_diff(&mut self, frame: &Frame, prev: Option<&Frame>, top_row: u16) -> io::Result<()> {
        let prev_rows = prev.map_or(&[][..], |p| &p.rows);
        let new_rows = &frame.rows;
        let mut wrote_rows = false;

        for (i, row) in new_rows.iter().enumerate() {
            let needs_write = match prev_rows.get(i) {
                Some(prev_row) => prev_row != row,
                None => true,
            };
            if needs_write {
                let y = top_row + i as u16;
                queue!(self.writer, MoveTo(0, y))?;
                write_screen_row(&mut self.writer, row)?;
                wrote_rows = true;
            }
        }

        if new_rows.len() < prev_rows.len() {
            self.clear_rows(
                top_row + new_rows.len() as u16,
                (prev_rows.len() - new_rows.len()) as u16,
            )?;
            wrote_rows = true;
        }

        if frame.cursor_visible {
            if let Some(cursor) = frame.cursor {
                let cursor_changed = prev.is_none_or(|p| p.cursor != Some(cursor) || !p.cursor_visible);
                if wrote_rows || cursor_changed {
                    queue!(self.writer, Show)?;
                    self.move_cursor(CursorCoord::new(top_row as usize + cursor.row, cursor.col))?;
                }
            } else {
                let was_hidden = prev.is_none_or(|p| !p.cursor_visible);
                if was_hidden {
                    queue!(self.writer, Show)?;
                }
            }
        } else {
            let was_visible = prev.is_none_or(|p| p.cursor_visible);
            if was_visible {
                queue!(self.writer, Hide)?;
            }
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
#[cfg(test)]
fn write_spans(writer: &mut impl Write, spans: &[Span], width: usize) -> io::Result<()> {
    let mut used = 0usize;
    let mut last_bg = Color::Reset;

    for span in spans {
        use crate::utils;

        if used >= width {
            break;
        }
        let remaining = width - used;
        let text = take_display_width(&sanitize_terminal_text(&span.text), remaining);
        let taken = utils::text_width(&text);
        used += taken;
        last_bg = span.style.bg;

        let style = style_to_crossterm(span.style);
        if !text.is_empty() {
            queue!(writer, cts::PrintStyledContent(style.apply(text)))?;
        }
    }

    if used < width {
        let mut pad_style = cts::ContentStyle::new();
        pad_style.background_color = Some(last_bg);
        queue!(
            writer,
            cts::PrintStyledContent(pad_style.apply(" ".repeat(width - used)))
        )?;
    }

    queue!(writer, cts::ResetColor)?;
    Ok(())
}

/// Write a row for the live screen without touching the terminal's last
/// column. The row is still cleared first, but padded cells are printed
/// explicitly because not every terminal applies the active background color
/// to `Clear(UntilNewLine)`.
fn write_screen_row(writer: &mut impl Write, row: &Row) -> io::Result<()> {
    clear_to_end_with_bg(writer, trailing_bg(row))?;
    let printable_width = row.width.saturating_sub(1);
    if printable_width > 0 {
        write_spans_unpadded(writer, &row.spans, printable_width)?;
    }
    queue!(writer, cts::ResetColor)?;
    Ok(())
}

fn clear_to_end_with_bg(writer: &mut impl Write, bg: Color) -> io::Result<()> {
    let mut clear_style = cts::ContentStyle::new();
    clear_style.background_color = Some(bg);
    queue!(writer, cts::SetStyle(clear_style), CtClear(ClearType::UntilNewLine))?;
    Ok(())
}

fn write_spans_unpadded(writer: &mut impl Write, spans: &[Span], width: usize) -> io::Result<()> {
    let mut used = 0usize;
    for span in spans {
        if used >= width {
            break;
        }
        let remaining = width - used;
        let text = take_display_width(&sanitize_terminal_text(&span.text), remaining);
        used += utils::text_width(&text);
        if !text.is_empty() {
            queue!(
                writer,
                cts::PrintStyledContent(style_to_crossterm(span.style).apply(text))
            )?;
        }
    }
    Ok(())
}

fn take_display_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let g_width = utils::grapheme_width(grapheme);
        if used + g_width > width {
            break;
        }
        out.push_str(grapheme);
        used += g_width;
    }
    out
}

fn sanitize_terminal_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\x00' => out.push('␀'),
            '\x01' => out.push('␁'),
            '\x02' => out.push('␂'),
            '\x03' => out.push('␃'),
            '\x04' => out.push('␄'),
            '\x05' => out.push('␅'),
            '\x06' => out.push('␆'),
            '\x07' => out.push('␇'),
            '\x08' => out.push('␈'),
            '\t' => out.push_str("    "),
            '\n' => out.push('␊'),
            '\x0b' => out.push('␋'),
            '\x0c' => out.push('␌'),
            '\r' => out.push('␍'),
            '\x0e' => out.push('␎'),
            '\x0f' => out.push('␏'),
            '\x10' => out.push('␐'),
            '\x11' => out.push('␑'),
            '\x12' => out.push('␒'),
            '\x13' => out.push('␓'),
            '\x14' => out.push('␔'),
            '\x15' => out.push('␕'),
            '\x16' => out.push('␖'),
            '\x17' => out.push('␗'),
            '\x18' => out.push('␘'),
            '\x19' => out.push('␙'),
            '\x1a' => out.push('␚'),
            '\x1b' => out.push('␛'),
            '\x1c' => out.push('␜'),
            '\x1d' => out.push('␝'),
            '\x1e' => out.push('␞'),
            '\x1f' => out.push('␟'),
            '\x7f' => out.push('␡'),
            _ => out.push(ch),
        }
    }
    out
}

fn trailing_bg(row: &Row) -> Color {
    row.spans.last().map(|span| span.style.bg).unwrap_or(Color::Reset)
}

/// Convert a renderer [`CellStyle`] to a crossterm [`cts::ContentStyle`].
pub fn style_to_crossterm(style: CellStyle) -> cts::ContentStyle {
    let mut out = cts::ContentStyle::new();
    out.foreground_color = Some(style.fg);
    out.background_color = Some(style.bg);
    out.attributes = modifiers_to_crossterm(style);
    out
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
    fn write_row_paints_padding_without_touching_last_column() {
        let mut b = backend(10, 5);
        let row = Row::padded(vec![Span::plain("ab")], 10, CellStyle::default());
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[K"));
        assert!(out.contains("ab"));
        assert!(
            out.contains("    "),
            "live writer should paint row padding instead of only relying on clear-to-EOL: {out:?}"
        );
        assert!(
            !out.contains("        \x1b[0m"),
            "live writer should not touch the terminal's final column: {out:?}"
        );
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
    fn render_frame_diff_restores_cursor_after_row_writes() {
        let mut b = backend(20, 10);
        let mut prev = Frame::new(20);
        prev.push(Row::padded(vec![Span::plain("old")], 20, CellStyle::default()));
        prev.set_cursor(CursorCoord::new(0, 4));

        let mut frame = Frame::new(20);
        frame.push(Row::padded(vec![Span::plain("new")], 20, CellStyle::default()));
        frame.set_cursor(CursorCoord::new(0, 4));

        b.render_frame_diff(&frame, Some(&prev), 0).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("new"));
        assert!(
            out.contains("\x1b[1;5H"),
            "cursor should be restored after changed row writes: {out:?}"
        );
    }

    #[test]
    fn clear_rows_emits_clear_current_line() {
        let mut b = backend(10, 5);
        b.clear_rows(0, 2).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[2K"));
    }

    #[test]
    fn clear_all_emits_visible_screen_clear_without_purge() {
        let mut b = backend(10, 5);
        b.clear_all().unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[2J"));
        assert!(
            !out.contains("\x1b[3J"),
            "visible-screen clear should not purge native scrollback"
        );
    }

    #[test]
    fn insert_history_lines_scrolls_before_first_row() {
        let mut b = backend(20, 10);
        let row = Row::padded(vec![Span::plain("history")], 20, CellStyle::default());
        b.insert_history_lines(&[row], 8).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(
            out.contains("\x1b[1;8r"),
            "should constrain scrolling above live viewport"
        );
        assert!(
            out.contains("\x1b[8;1H\r\n"),
            "first history row should be inserted by scrolling, not by overwriting: {out:?}"
        );
        assert!(out.contains("history"));
        assert!(out.contains("\x1b[r"), "scroll region should be reset");
    }

    #[test]
    fn insert_history_lines_at_top_writes_without_scroll_region() {
        let mut b = backend(20, 10);
        let row = Row::padded(vec![Span::plain("history")], 20, CellStyle::default());
        b.insert_history_lines(&[row], 0).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("history"));
        assert!(!out.contains("\x1b[1;0r"));
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

    #[test]
    fn write_spans_truncates_by_display_width() {
        let mut buf = Vec::new();
        let spans = vec![Span::plain("a中b")];
        write_spans(&mut buf, &spans, 3).unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("a中"));
        assert!(!out.contains("b"));
    }

    #[test]
    fn write_spans_truncates_without_splitting_grapheme() {
        let mut buf = Vec::new();
        let family = "👨\u{200d}👩\u{200d}👧";
        let spans = vec![Span::plain(format!("a{family}b"))];
        write_spans(&mut buf, &spans, 3).unwrap();

        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains(&format!("a{family}")));
        assert!(!out.contains('b'));
    }

    #[test]
    fn write_row_sanitizes_terminal_control_sequences() {
        let mut b = backend(80, 5);
        let row = Row::padded(
            vec![Span::plain("before \x1b]52;c;clipboard\x07 after \x1b[2J")],
            80,
            CellStyle::default(),
        );
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("before ␛]52;c;clipboard␇ after ␛[2J"));
        assert!(
            !out.contains("\x1b]52;c;clipboard"),
            "OSC clipboard sequence should be rendered inertly: {out:?}"
        );
        assert!(
            !out.contains("\x1b[2J after"),
            "embedded CSI clear sequence should be rendered inertly: {out:?}"
        );
    }

    #[test]
    fn insert_history_lines_multiple_rows() {
        let mut b = backend(20, 10);
        let rows = [
            Row::padded(vec![Span::plain("one")], 20, CellStyle::default()),
            Row::padded(vec![Span::plain("two")], 20, CellStyle::default()),
            Row::padded(vec![Span::plain("three")], 20, CellStyle::default()),
        ];
        b.insert_history_lines(&rows, 8).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert_eq!(
            out.matches("\r\n").count(),
            3,
            "each history row should be inserted with a newline"
        );
        assert!(out.contains("one"));
        assert!(out.contains("two"));
        assert!(out.contains("three"));
        assert!(out.contains("\x1b[r"), "scroll region should be reset after insertion");
    }

    #[test]
    fn insert_history_lines_resets_scroll_region() {
        let mut b = backend(20, 10);
        let row = Row::padded(vec![Span::plain("history")], 20, CellStyle::default());
        b.insert_history_lines(&[row], 8).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("\x1b[r"), "scroll region should be reset after insertion");
    }

    #[test]
    fn insert_history_lines_empty_is_noop() {
        let mut b = backend(20, 10);
        b.insert_history_lines(&[], 8).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.is_empty(), "empty history insert should produce no output");
    }

    #[test]
    fn history_insert_then_live_diff_renders_viewport() {
        let mut b = backend(20, 10);

        let mut prev = Frame::new(20);
        prev.push(Row::padded(vec![Span::plain("live old")], 20, CellStyle::default()));
        prev.set_cursor(CursorCoord::new(0, 8));
        b.render_frame(&prev, 0).unwrap();
        b.writer().clear();

        let history_row = Row::padded(vec![Span::plain("history")], 20, CellStyle::default());
        b.insert_history_lines(&[history_row], 10).unwrap();
        let after_insert = String::from_utf8(b.writer().clone()).unwrap().len();

        let mut frame = Frame::new(20);
        frame.push(Row::padded(vec![Span::plain("live new")], 20, CellStyle::default()));
        frame.set_cursor(CursorCoord::new(0, 8));
        b.render_frame_diff(&frame, Some(&prev), 0).unwrap();
        let total = String::from_utf8(b.writer().clone()).unwrap();
        let diff_bytes = &total[after_insert..];

        assert!(
            total.contains("\x1b[1;10r"),
            "history insert should use a constrained scroll region"
        );
        assert!(
            diff_bytes.contains("live new"),
            "live diff should render the changed row: {diff_bytes:?}"
        );
        assert!(
            diff_bytes.contains("\x1b[1;9H"),
            "cursor should be placed after the diffed row: {diff_bytes:?}"
        );
    }

    #[test]
    fn write_row_cjk() {
        let mut b = backend(20, 5);
        let row = Row::padded(vec![Span::plain("日本語")], 20, CellStyle::default());
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(out.contains("日本語"), "CJK text should be written intact: {out:?}");
        assert!(out.ends_with("\x1b[0m"), "row write should end with reset: {out:?}");
    }

    #[test]
    fn write_row_emoji() {
        let mut b = backend(20, 5);
        let family = "👨\u{200d}👩\u{200d}👧";
        let row = Row::padded(vec![Span::plain(family.to_string())], 20, CellStyle::default());
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        assert!(
            out.contains(family),
            "ZWJ emoji should be written as one grapheme: {out:?}"
        );
    }

    #[test]
    fn write_row_regional_indicators() {
        let flag = "🇺🇸";
        let mut b = backend(20, 5);
        let row = Row::padded(vec![Span::plain(flag.to_string())], 20, CellStyle::default());
        b.write_row(0, &row).unwrap();
        let wide = String::from_utf8(b.writer().clone()).unwrap();
        assert!(wide.contains(flag), "flag should render when it fits: {wide:?}");

        let mut b = backend(20, 5);
        let row = Row::padded(vec![Span::plain(flag.to_string())], 2, CellStyle::default());
        b.write_row(0, &row).unwrap();
        let narrow = String::from_utf8(b.writer().clone()).unwrap();
        assert!(
            !narrow.contains(flag),
            "flag should be omitted entirely when it does not fit: {narrow:?}"
        );
        assert!(
            !narrow.contains('\u{1f1fa}') && !narrow.contains('\u{1f1f8}'),
            "flag codepoints should not be split across the clip boundary: {narrow:?}"
        );
    }

    #[test]
    fn write_row_long_unbroken_text() {
        let mut b = backend(10, 5);
        let row = Row::padded(vec![Span::plain("a".repeat(100))], 10, CellStyle::default());
        b.write_row(0, &row).unwrap();

        let out = String::from_utf8(b.writer().clone()).unwrap();
        let a_count = out.matches('a').count();
        assert!(
            a_count > 0 && a_count <= 9,
            "long unbroken text should be clipped to the printable width without splitting: {a_count}"
        );
    }
}
