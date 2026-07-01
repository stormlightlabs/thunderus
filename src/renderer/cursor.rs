//! Prompt cursor coordinate calculation.
//!
//! Given prompt text, a char-index cursor position, the body width (columns
//! available for text), and an optional prompt indent, compute the `(row, col)`
//! coordinate of the cursor within the prompt's visual rows.
//!
//! Handles:
//! - single-line input;
//! - word/char wrapping at `body_width`;
//! - explicit `\n` newlines;
//! - a leading prompt indent applied to every visual row;
//! - multibyte (UTF-8) text, since the cursor is a char index.

#![allow(dead_code)]

use super::row::CursorCoord;

/// Compute the `(row, col)` coordinate of the cursor in the prompt's visual
/// rows.
///
/// `text` is the full prompt string, `cursor` is the char index of the cursor,
/// `body_width` is the number of columns available for prompt text (excluding
/// any indent), and `indent` is the number of leading columns added to every
/// visual row.
///
/// The returned column is relative to the start of the visual row including the
/// indent, i.e. it is the absolute column where the cursor should be placed.
pub fn prompt_cursor(text: &str, cursor: usize, body_width: usize, indent: usize) -> CursorCoord {
    let body_width = body_width.max(1);
    let mut row = 0usize;
    let mut col = indent;
    let mut pos = 0usize;

    for ch in text.chars() {
        if pos == cursor {
            return CursorCoord { row, col };
        }

        if ch == '\n' {
            row += 1;
            col = indent;
            pos += 1;
            continue;
        }

        if col - indent >= body_width {
            row += 1;
            col = indent;
        }
        col += 1;
        pos += 1;
    }

    CursorCoord { row, col }
}

/// Decompose prompt text into visual rows of text (without the indent) for
/// rendering.
///
/// Each returned string is the content of one visual row; the caller adds the
/// indent prefix. Explicit `\n` characters are not included in the output.
pub fn prompt_rows(text: &str, body_width: usize) -> Vec<String> {
    let body_width = body_width.max(1);
    let mut rows = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if ch == '\n' {
            rows.push(std::mem::take(&mut current));
            continue;
        }
        if current.chars().count() >= body_width {
            rows.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    rows.push(current);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_line_cursor_at_end() {
        let coord = prompt_cursor("hello", 5, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 5 });
    }

    #[test]
    fn single_line_cursor_in_middle() {
        let coord = prompt_cursor("hello", 2, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn single_line_cursor_at_start() {
        let coord = prompt_cursor("hello", 0, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 0 });
    }

    #[test]
    fn empty_input_cursor_at_origin() {
        let coord = prompt_cursor("", 0, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 0 });
    }

    #[test]
    fn wrapped_line_cursor_on_second_row() {
        let coord = prompt_cursor("abcdef", 4, 3, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 1 });
    }

    #[test]
    fn wrapped_line_cursor_at_wrap_boundary() {
        let coord = prompt_cursor("abcdef", 3, 3, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 3 });
    }

    #[test]
    fn explicit_newline_cursor_on_second_line() {
        let coord = prompt_cursor("line1\nline2", 7, 80, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 1 });
    }

    #[test]
    fn explicit_newline_cursor_at_line_start() {
        let coord = prompt_cursor("line1\nline2", 6, 80, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 0 });
    }

    #[test]
    fn explicit_newline_cursor_at_newline_char() {
        let coord = prompt_cursor("line1\nline2", 5, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 5 });
    }

    #[test]
    fn indented_prompt_offsets_column() {
        let coord = prompt_cursor("hello", 2, 80, 3);
        assert_eq!(coord, CursorCoord { row: 0, col: 5 });
    }

    #[test]
    fn indented_wrapped_line_resets_indent() {
        let coord = prompt_cursor("abcdef", 4, 3, 3);
        assert_eq!(coord, CursorCoord { row: 1, col: 4 });
    }

    #[test]
    fn indented_multiline_resets_indent() {
        let coord = prompt_cursor("ab\ncd", 4, 80, 3);
        assert_eq!(coord, CursorCoord { row: 1, col: 4 });
    }

    #[test]
    fn multibyte_cursor_counts_chars_not_bytes() {
        let coord = prompt_cursor("héllo", 2, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn multibyte_wrapped_cursor() {
        let coord = prompt_cursor("éééé", 3, 2, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 1 });
    }

    #[test]
    fn cursor_beyond_end_clamps_to_final_row() {
        let coord = prompt_cursor("ab", 99, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn cursor_beyond_end_multiline() {
        let coord = prompt_cursor("ab\ncd", 99, 80, 0);
        assert_eq!(coord, CursorCoord { row: 1, col: 2 });
    }

    #[test]
    fn prompt_rows_single_line() {
        let rows = prompt_rows("hello", 80);
        assert_eq!(rows, vec!["hello"]);
    }

    #[test]
    fn prompt_rows_explicit_newline() {
        let rows = prompt_rows("ab\ncd", 80);
        assert_eq!(rows, vec!["ab", "cd"]);
    }

    #[test]
    fn prompt_rows_wrapped() {
        let rows = prompt_rows("abcdef", 3);
        assert_eq!(rows, vec!["abc", "def"]);
    }

    #[test]
    fn prompt_rows_empty() {
        let rows = prompt_rows("", 80);
        assert_eq!(rows, vec![""]);
    }

    #[test]
    fn combined_wrap_and_newline() {
        let rows = prompt_rows("abcd\nef", 2);
        assert_eq!(rows, vec!["ab", "cd", "ef"]);

        let coord = prompt_cursor("abcd\nef", 6, 2, 0);
        assert_eq!(coord, CursorCoord { row: 2, col: 1 });
    }
}
