//! Row, block, and frame primitives for the direct renderer.
//!
//! A [`Row`] is one terminal row after wrapping and padding decisions. A
//! [`Block`] groups rows belonging to one semantic transcript entry. A
//! [`Frame`] is the complete live-region render and supports debug rendering
//! for snapshot tests without any terminal I/O.

#![allow(dead_code)]

use std::fmt;

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

    /// Create a row from spans, truncated to `width`.
    pub fn truncated(spans: &[Span], width: usize, ellipsis_style: CellStyle) -> Self {
        let spans = super::layout::truncate_spans(spans, width, ellipsis_style);
        let used = super::layout::spans_width(&spans);
        Row { spans, width: used }
    }

    /// Create an empty (blank) row of `width` columns.
    pub fn blank(width: usize, style: CellStyle) -> Self {
        if width == 0 {
            return Row { spans: Vec::new(), width: 0 };
        }
        Row { spans: vec![Span::styled(" ".repeat(width), style)], width }
    }

    /// Visible width (column count) of the row's spans.
    pub fn text_width(&self) -> usize {
        crate::renderer::layout::spans_width(&self.spans)
    }

    /// Whether the row is visually empty (no non-space content).
    pub fn is_blank(&self) -> bool {
        self.spans.iter().all(|s| s.text.chars().all(|c| c == ' '))
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

/// A semantic group of rows, e.g. one assistant message or one tool block.
///
/// Blocks carry an optional label and background so the renderer can add
/// vertical padding and full-width backgrounds consistently.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Block {
    pub rows: Vec<Row>,
}

impl Block {
    /// Create an empty block.
    pub fn new() -> Self {
        Block { rows: Vec::new() }
    }

    /// Create a block from rows.
    pub fn from_rows(rows: Vec<Row>) -> Self {
        Block { rows }
    }

    /// Append a row.
    pub fn push(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the block is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
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

    /// Append all rows from a block.
    pub fn push_block(&mut self, block: Block) {
        self.rows.extend(block.rows);
    }

    /// Set the cursor coordinate.
    pub fn set_cursor(&mut self, cursor: CursorCoord) {
        self.cursor = Some(cursor);
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the frame has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Plain-text rendering, one line per row.
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

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render_text())
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
    fn row_blank_is_blank() {
        let row = Row::blank(5, CellStyle::default());
        assert!(row.is_blank());
        assert_eq!(row.text_width(), 5);
    }

    #[test]
    fn row_truncated_marks_width() {
        let row = Row::truncated(&[Span::plain("hello world")], 8, CellStyle::default());
        assert_eq!(row.width, 8);
    }

    #[test]
    fn block_push_increments_len() {
        let mut block = Block::new();
        assert!(block.is_empty());
        block.push(Row::blank(5, CellStyle::default()));
        assert_eq!(block.len(), 1);
        assert!(!block.is_empty());
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
    fn frame_push_block_appends_all_rows() {
        let mut frame = Frame::new(10);
        let mut block = Block::new();
        block.push(Row::blank(3, CellStyle::default()));
        block.push(Row::blank(3, CellStyle::default()));
        frame.push_block(block);
        assert_eq!(frame.len(), 2);
    }

    #[test]
    fn frame_set_cursor_stores_coord() {
        let mut frame = Frame::new(10);
        frame.set_cursor(CursorCoord::new(2, 3));
        assert_eq!(frame.cursor, Some(CursorCoord { row: 2, col: 3 }));
    }
}
