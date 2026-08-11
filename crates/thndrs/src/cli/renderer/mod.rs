//! Terminal rendering for `thndrs`.
//!
//! The row model
//! ([`style`], [`layout`], [`row`], [`cursor`]) is independent of crossterm I/O
//! so wrapping, padding, truncation, cursor coordinates, and snapshots remain
//! unit-testable. [`alternate`] owns the production Ratatui surface and terminal
//! lifecycle.

pub mod alternate;
pub mod cursor;
pub mod git;
pub mod highlight;
pub mod layout;
pub mod live;
pub mod path_display;
pub mod row;
pub mod style;
pub mod surface;
pub mod transcript;
pub mod view;

#[cfg(test)]
mod tests {
    use super::cursor;
    use super::cursor::prompt_cursor;
    use super::layout::{content_width, wrap_text};
    use super::row::{CursorCoord, Frame, Row};
    use super::style::{CellStyle, Color, Span};

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

        let rows = vec![
            Row::padded(
                vec![Span::styled("User", CellStyle::new().fg(Color::Blue).bg(bg).bold())],
                40,
                surface,
            ),
            Row::padded(
                vec![Span::styled("hello there", CellStyle::default().bg(bg))],
                40,
                surface,
            ),
        ];
        frame.rows.extend(rows);
        insta::assert_snapshot!(frame.render_styled());
    }

    #[test]
    fn snapshot_prompt_with_cursor() {
        let mut frame = Frame::new(30);
        let prompt_style = CellStyle::new().fg(Color::Yellow);
        frame.push(Row::padded(
            vec![Span::styled("❯  hello", prompt_style)],
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
            let spans = vec![Span::styled("❯  ", style), Span::styled(line, CellStyle::default())];
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
}
