//! Direct terminal renderer for `thndrs`.
//!
//! Replaces Ratatui as the source of truth for inline rendering. The row model
//! ([`style`], [`layout`], [`row`], [`cursor`]) is independent of crossterm I/O
//! so wrapping, padding, truncation, cursor coordinates, and snapshots can be
//! unit-tested. The [`backend`] module is the only place that translates rows
//! into ANSI escape sequences.

#![allow(unused_imports)]

pub mod backend;
pub mod cursor;
pub mod layout;
pub mod row;
pub mod style;

pub use backend::{TerminalBackend, enter_raw_mode, leave_raw_mode, terminal_size};
pub use cursor::prompt_cursor;
pub use layout::{content_width, pad_row, truncate_spans, wrap_spans, wrap_text};
pub use row::{Block, CursorCoord, Frame, Row};
pub use style::{CellStyle, Color, Span};

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a helper that wraps styled text into padded rows.
    fn padded_rows(text: &str, style: CellStyle, width: usize, pad_style: CellStyle) -> Vec<Row> {
        let content_w = content_width(width);
        let wrapped = wrap_text(text, content_w);
        wrapped
            .into_iter()
            .map(|line| {
                let spans = if line.is_empty() { Vec::new() } else { vec![Span::styled(line, style)] };
                Row::padded(spans, width, pad_style)
            })
            .collect()
    }

    #[test]
    fn row_model_narrow_width() {
        let rows = padded_rows(
            "hello world",
            CellStyle::new().fg(Color::Green),
            10,
            CellStyle::default(),
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].text_width(), 10);
        assert!(rows[0].text().contains("hello"));
        assert!(rows[1].text().contains("world"));
    }

    #[test]
    fn row_model_normal_width() {
        let rows = padded_rows(
            "hello world this is a test",
            CellStyle::default(),
            80,
            CellStyle::default(),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text_width(), 80);
        assert!(rows[0].text().contains("hello world this is a test"));
    }

    #[test]
    fn row_model_wide_width() {
        let rows = padded_rows("hello world", CellStyle::default(), 200, CellStyle::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text_width(), 200);
    }

    #[test]
    fn row_model_tiny_width_no_panic() {
        let rows = padded_rows("hello", CellStyle::default(), 1, CellStyle::default());
        assert!(!rows.is_empty());
    }

    #[test]
    fn row_model_zero_width_does_not_panic() {
        let rows = padded_rows("hello", CellStyle::default(), 0, CellStyle::default());
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r.width == 0));
    }

    #[test]
    fn prompt_cursor_single_line() {
        let coord = prompt_cursor("hello world", 5, 76, 3);
        assert_eq!(coord, CursorCoord { row: 0, col: 8 });
    }

    #[test]
    fn prompt_cursor_wrapped_line() {
        let coord = prompt_cursor("abcdefghij", 7, 3, 0);
        assert_eq!(coord, CursorCoord { row: 2, col: 1 });
    }

    #[test]
    fn prompt_cursor_multiline_explicit_newline() {
        let coord = prompt_cursor("line1\nline2\nline3", 8, 76, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 2 });
    }

    #[test]
    fn prompt_cursor_indented_multiline() {
        let coord = prompt_cursor("ab\ncd", 4, 76, 3);
        assert_eq!(coord, CursorCoord { row: 1, col: 4 });
    }

    #[test]
    fn prompt_cursor_multibyte() {
        let coord = prompt_cursor("héllo wörld", 6, 76, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 6 });
    }

    #[test]
    fn prompt_cursor_multibyte_wrapped() {
        let coord = prompt_cursor("ééééé", 4, 2, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 2 });
    }

    #[test]
    fn snapshot_simple_styled_frame() {
        let mut frame = Frame::new(20);
        let blue = CellStyle::new().fg(Color::Blue).bg(Color::Rgb { r: 30, g: 33, b: 50 });
        frame.push(Row::padded(vec![Span::styled("Assistant", blue.bold())], 20, blue));
        frame.push(Row::padded(
            vec![Span::styled("hello world", CellStyle::default().bg(blue.bg))],
            20,
            blue,
        ));
        frame.set_cursor(CursorCoord::new(2, 3));
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn snapshot_transcript_block() {
        let mut frame = Frame::new(40);
        let bg = Color::Rgb { r: 30, g: 33, b: 50 };
        let surface = CellStyle::default().bg(bg);

        let mut block = Block::new();
        block.push(Row::padded(
            vec![Span::styled("User", CellStyle::new().fg(Color::Blue).bg(bg).bold())],
            40,
            surface,
        ));
        block.push(Row::padded(
            vec![Span::styled("hello there", CellStyle::default().bg(bg))],
            40,
            surface,
        ));
        frame.push_block(block);
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn snapshot_prompt_with_cursor() {
        let mut frame = Frame::new(30);
        let prompt_style = CellStyle::new().fg(Color::Yellow);
        frame.push(Row::padded(
            vec![Span::styled("›  hello", prompt_style)],
            30,
            CellStyle::default(),
        ));
        frame.set_cursor(prompt_cursor("hello", 5, 27, 3));
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn snapshot_multiline_prompt() {
        let text = "line one\nline two";
        let mut frame = Frame::new(30);
        let style = CellStyle::new().fg(Color::Yellow);
        let indent = 3;
        let body_width = 27;
        for line in cursor::prompt_rows(text, body_width) {
            let spans = vec![Span::styled("›  ", style), Span::styled(line, CellStyle::default())];
            frame.push(Row::padded(spans, 30, CellStyle::default()));
        }
        frame.set_cursor(prompt_cursor(text, 9, body_width, indent));
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn snapshot_narrow_width_block() {
        let mut frame = Frame::new(10);
        let bg = Color::Rgb { r: 30, g: 33, b: 50 };
        let surface = CellStyle::default().bg(bg);
        frame.push(Row::padded(
            vec![Span::styled("Err", CellStyle::new().fg(Color::Red).bg(bg).bold())],
            10,
            surface,
        ));
        frame.push(Row::padded(
            vec![Span::styled("bad input value here", CellStyle::default().bg(bg))],
            10,
            surface,
        ));
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn backend_writes_styled_row_to_buffer() {
        let mut backend = TerminalBackend::new(Vec::new(), 20, 10);
        let row = Row::padded(
            vec![Span::styled("hi", CellStyle::new().fg(Color::Green).bg(Color::Blue))],
            6,
            CellStyle::default(),
        );
        backend.write_row(0, &row).expect("write row");
        let out = String::from_utf8(backend.writer().clone()).expect("utf8");
        assert!(out.contains("hi"), "row text should be in output");
        assert!(out.ends_with("\x1b[0m"), "should reset color at end");
    }

    #[test]
    fn backend_render_frame_writes_all_rows_and_cursor() {
        let mut backend = TerminalBackend::new(Vec::new(), 20, 10);
        let mut frame = Frame::new(20);
        frame.push(Row::padded(vec![Span::plain("row1")], 10, CellStyle::default()));
        frame.push(Row::padded(vec![Span::plain("row2")], 10, CellStyle::default()));
        frame.set_cursor(CursorCoord::new(1, 4));
        backend.render_frame(&frame, 0).expect("render frame");

        let out = String::from_utf8(backend.writer().clone()).expect("utf8");
        assert!(out.contains("row1"));
        assert!(out.contains("row2"));
        assert!(out.contains("\x1b[2;5H"));
    }
}
