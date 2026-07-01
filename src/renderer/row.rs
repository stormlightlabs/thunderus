//! Row, block, and frame primitives for the direct renderer.
//!
//! A [`Row`] is one terminal row after wrapping and padding decisions. A
//! [`Frame`] is the complete live-region render.

use super::style::{CellStyle, Span};

/// One terminal row: a sequence of styled spans padded to a known width.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Row {
    pub spans: Vec<Span>,
    /// Width in display columns the row is padded/truncated to.
    pub width: usize,
}

impl Row {
    /// Create a row from spans, padded to `width` using `pad_style`.
    pub fn padded(spans: Vec<Span>, width: usize, pad_style: CellStyle) -> Self {
        let spans = crate::renderer::layout::pad_row(spans, width, pad_style);
        Row { spans, width }
    }

    /// Create an empty (blank) row of `width` columns.
    pub fn blank(width: usize, style: CellStyle) -> Self {
        if width == 0 {
            return Row { spans: Vec::new(), width: 0 };
        }
        Row { spans: vec![Span::styled(" ".repeat(width), style)], width }
    }

    /// Visible width (column count) of the row's spans.
    #[cfg(test)]
    pub fn text_width(&self) -> usize {
        crate::renderer::layout::spans_width(&self.spans)
    }

    /// Plain-text rendering of the row (for snapshot readability).
    pub fn text(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }
}

/// The complete live-region render.
///
/// A frame is an ordered list of rows representing the current live region:
/// active streaming block, dynamic status, prompt input, accessories, and
/// static status. Frames are diffed and redrawn each tick.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Frame {
    pub rows: Vec<Row>,
    pub width: usize,
    pub cursor: Option<CursorCoord>,
}

/// A (row, column) coordinate within a frame, used for cursor placement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CursorCoord {
    pub row: usize,
    pub col: usize,
}

impl CursorCoord {
    pub fn new(row: usize, col: usize) -> Self {
        CursorCoord { row, col }
    }
}

impl Frame {
    /// Create an empty frame at `width`.
    pub fn new(width: usize) -> Self {
        Frame { rows: Vec::new(), width, cursor: None }
    }

    /// Append a row.
    pub fn push(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// Set the cursor coordinate.
    pub fn set_cursor(&mut self, cursor: CursorCoord) {
        self.cursor = Some(cursor);
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Plain-text rendering, one line per row.
    #[cfg(test)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(&row.text());
            out.push('\n');
        }
        out
    }

    /// Debug rendering showing styled spans per row.
    ///
    /// Each row is rendered as `│ <text>` followed by inline style annotations.
    /// This is the snapshot format that asserts styled rows without using
    /// Ratatui as the layout engine.
    #[cfg(test)]
    pub fn render_styled(&self) -> String {
        let mut out = String::new();
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str("│ ");
            for span in &row.spans {
                out.push_str(&span.text);
            }

            out.push_str("  #");
            for span in &row.spans {
                if !span.text.is_empty() {
                    out.push_str(&format!(" [{}]={}", span.style, span.text.replace('\n', "\\n")));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::style::Color;

    #[test]
    fn row_padded_fills_width() {
        let row = Row::padded(vec![Span::plain("hi")], 10, CellStyle::default());
        assert_eq!(row.text_width(), 10);
        assert_eq!(row.width, 10);
    }

    #[test]
    fn frame_render_text_one_line_per_row() {
        let mut frame = Frame::new(10);
        frame.push(Row::padded(vec![Span::plain("a")], 5, CellStyle::default()));
        frame.push(Row::padded(vec![Span::plain("b")], 5, CellStyle::default()));
        let text = frame.render_text();
        assert!(text.contains("a"));
        assert!(text.contains("b"));
        assert_eq!(text.matches('\n').count(), 2);
    }

    #[test]
    fn frame_render_styled_includes_style_annotations() {
        let mut frame = Frame::new(10);
        frame.push(Row::padded(
            vec![Span::styled("hi", CellStyle::new().fg(Color::Red))],
            6,
            CellStyle::default(),
        ));
        let styled = frame.render_styled();
        assert!(styled.contains("fg=red"));
        assert!(styled.contains("hi"));
    }

    #[test]
    fn frame_set_cursor_stores_coord() {
        let mut frame = Frame::new(10);
        frame.set_cursor(CursorCoord::new(2, 3));
        assert_eq!(frame.cursor, Some(CursorCoord { row: 2, col: 3 }));
    }
}
