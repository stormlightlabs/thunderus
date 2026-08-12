//! Prompt cursor coordinate calculation.
//!
//! Given prompt text, a grapheme-cluster cursor position, the body width
//! (columns available for text), and an optional prompt indent, compute the
//! `(row, col)` coordinate of the cursor within the prompt's visual rows.
//!
//! Uses [`unicode_width`] for display-width measurement so that wide
//! characters (CJK), zero-width characters (combining marks), and emoji are
//! handled correctly. Cursor position is expressed as a grapheme-cluster index
//! (matching [`crate::input::PromptInput`]).
//!
//! Handles:
//! - single-line input;
//! - word/char wrapping at `body_width` using **display width**, not char count;
//! - explicit `\n` newlines;
//! - a leading prompt indent applied to every visual row;
//! - multibyte and grapheme clusters.

use crate::utils;

use super::row::CursorCoord;
use unicode_segmentation::UnicodeSegmentation;

struct PromptLayout {
    rows: Vec<String>,
    cursor_positions: Vec<CursorCoord>,
}

/// Compute the `(row, col)` coordinate of the cursor in the prompt's visual
/// rows.
///
/// `text` is the full prompt string, `cursor` is the grapheme-cluster index of
/// the cursor, `body_width` is the number of display columns available for
/// prompt text (excluding any indent), and `indent` is the number of leading
/// columns added to every visual row.
///
/// The returned column is relative to the start of the visual row including the
/// indent, i.e. it is the absolute column where the cursor should be placed.
pub fn prompt_cursor(text: &str, cursor: usize, body_width: usize, indent: usize) -> CursorCoord {
    let layout = prompt_layout(text, body_width, indent);
    let cursor = cursor.min(layout.cursor_positions.len().saturating_sub(1));
    layout.cursor_positions[cursor]
}

/// Decompose prompt text into visual rows of text (without the indent) for
/// rendering. Uses display width for wrapping so wide characters occupy the
/// correct number of columns.
///
/// Each returned string is the content of one visual row; the caller adds the
/// indent prefix. Explicit `\n` characters are not included in the output.
pub fn prompt_rows(text: &str, body_width: usize) -> Vec<String> {
    prompt_layout(text, body_width, 0).rows
}

fn prompt_layout(text: &str, body_width: usize, indent: usize) -> PromptLayout {
    let body_width = body_width.max(1);
    let graphemes = text.graphemes(true).collect::<Vec<_>>();
    let mut rows = vec![String::new()];
    let mut cursor_positions = vec![CursorCoord { row: 0, col: indent }; graphemes.len() + 1];
    let mut row = 0usize;
    let mut row_width = 0usize;
    let mut index = 0usize;

    while index < graphemes.len() {
        let grapheme = graphemes[index];
        if grapheme.contains('\n') {
            row += 1;
            rows.push(String::new());
            row_width = 0;
            index += 1;
            cursor_positions[index] = CursorCoord { row, col: indent };
            continue;
        }

        if grapheme.chars().all(char::is_whitespace) {
            append_grapheme(
                grapheme,
                index,
                body_width,
                indent,
                &mut rows,
                &mut cursor_positions,
                &mut row,
                &mut row_width,
            );
            index += 1;
            continue;
        }

        let word_end = graphemes[index..]
            .iter()
            .position(|part| part.contains('\n') || part.chars().all(char::is_whitespace))
            .map_or(graphemes.len(), |offset| index + offset);
        let word_width = graphemes[index..word_end]
            .iter()
            .map(|part| utils::grapheme_width(part))
            .sum::<usize>();

        if row_width > 0 && row_width + word_width > body_width {
            row += 1;
            rows.push(String::new());
            row_width = 0;
            cursor_positions[index] = CursorCoord { row, col: indent };
        }

        while index < word_end {
            append_grapheme(
                graphemes[index],
                index,
                body_width,
                indent,
                &mut rows,
                &mut cursor_positions,
                &mut row,
                &mut row_width,
            );
            index += 1;
        }
    }

    PromptLayout { rows, cursor_positions }
}

#[allow(clippy::too_many_arguments)]
fn append_grapheme(
    grapheme: &str, index: usize, body_width: usize, indent: usize, rows: &mut Vec<String>,
    cursor_positions: &mut [CursorCoord], row: &mut usize, row_width: &mut usize,
) {
    let grapheme_width = utils::grapheme_width(grapheme);
    if *row_width > 0 && *row_width + grapheme_width > body_width {
        *row += 1;
        rows.push(String::new());
        *row_width = 0;
    }

    rows[*row].push_str(grapheme);
    *row_width += grapheme_width;
    cursor_positions[index + 1] = CursorCoord { row: *row, col: indent + *row_width };
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
    fn multibyte_cursor_counts_graphemes_not_bytes() {
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
    fn prompt_rows_wrap_at_word_boundaries() {
        let rows = prompt_rows("hello brave world", 11);
        assert_eq!(rows, vec!["hello brave", " world"]);
    }

    #[test]
    fn cursor_follows_word_wrapping() {
        let coord = prompt_cursor("hello world", 8, 8, 3);
        assert_eq!(coord, CursorCoord { row: 1, col: 5 });
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

    #[test]
    fn cjk_wide_char_cursor_column() {
        let coord = prompt_cursor("中", 1, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn cjk_wide_char_wraps_correctly() {
        let rows = prompt_rows("中中", 3);
        assert_eq!(rows, vec!["中", "中"]);
    }

    #[test]
    fn cjk_wide_char_cursor_after_wrap() {
        let coord = prompt_cursor("中中", 1, 3, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn cjk_mixed_with_ascii_wraps_by_width() {
        let rows = prompt_rows("a中b", 3);
        assert_eq!(rows, vec!["a中", "b"]);
    }

    #[test]
    fn combining_mark_does_not_advance_column() {
        let text = "e\u{0301}";
        let coord = prompt_cursor(text, 1, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 1 });
    }

    #[test]
    fn combining_mark_does_not_cause_extra_wrap() {
        let text = "ab\u{0327}";
        let rows = prompt_rows(text, 2);
        assert_eq!(rows, vec!["ab\u{0327}"], "combining mark should not force a wrap");
    }

    #[test]
    fn emoji_width_2_cursor() {
        let coord = prompt_cursor("\u{1F600}", 1, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 2 });
    }

    #[test]
    fn emoji_zwj_one_grapheme_cursor() {
        let text = "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}b";
        let coord = prompt_cursor(text, 1, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 1 });

        let coord = prompt_cursor(text, 2, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 3 });
    }

    #[test]
    fn emoji_wraps_by_width() {
        let rows = prompt_rows("\u{1F600}\u{1F600}", 3);
        assert_eq!(rows, vec!["\u{1F600}", "\u{1F600}"]);
    }

    #[test]
    fn zwnj_does_not_advance_width() {
        let text = "ab\u{200C}cd";
        let coord = prompt_cursor(text, 3, 80, 0);
        assert_eq!(coord, CursorCoord { row: 0, col: 3 });
    }
}
