//! Width-aware layout helpers for the row model.
//!
//! These functions operate on the renderer's own [`Span`] and [`CellStyle`]
//! types. They are the single source of truth for wrapping, padding, and
//! truncation so that cursor placement and snapshots stay deterministic.

use super::style::{CellStyle, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Maximum width usable for body content inside a padded block.
///
/// Uses two-cell left / two-cell right block padding used so
/// transcript rows line up with the live region.
pub fn content_width(max_width: usize) -> usize {
    let left_pad = max_width.min(2);
    let right_pad = max_width.saturating_sub(left_pad).min(2);
    max_width.saturating_sub(left_pad + right_pad)
}

/// Word-wrap plain text into rows no wider than `width`.
///
/// Existing `\n` line breaks are preserved. Overlong words are hard-split.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();

    for raw_line in text.split('\n') {
        if raw_line.is_empty() {
            rows.push(String::new());
            continue;
        }

        let mut current = String::new();
        let mut current_width = 0usize;
        for word in raw_line.split_whitespace() {
            let word_len = display_width(word);

            if current_width == 0 {
                if word_len <= width {
                    current.push_str(word);
                    current_width = word_len;
                } else {
                    rows.extend(split_long_word(word, width));
                }
            } else if current_width + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_len;
            } else {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
                if word_len <= width {
                    current.push_str(word);
                    current_width = word_len;
                } else {
                    rows.extend(split_long_word(word, width));
                }
            }
        }

        if !current.is_empty() {
            rows.push(current);
        }
    }

    rows
}

/// Hard-split a single word into chunks no wider than `width`.
fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for grapheme in word.graphemes(true) {
        let g_width = grapheme_width(grapheme);
        if current_width > 0 && current_width + g_width > width {
            chunks.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push_str(grapheme);
        current_width += g_width;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Wrap styled spans into rows no wider than `width`, preserving explicit
/// `\n` newlines inside span text.
///
/// Spans are split at character boundaries; adjacent spans keep their
/// individual styles. A single logical row can contain multiple spans. Blank
/// (empty-text) input produces a single empty row so callers always get at
/// least one row.
pub fn wrap_spans(spans: &[Span], width: usize) -> Vec<Vec<Span>> {
    let width = width.max(1);
    let mut rows: Vec<Vec<Span>> = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let mut current_width = 0usize;

    for span in spans {
        for grapheme in span.text.graphemes(true) {
            if grapheme.contains('\n') {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
                continue;
            }
            let g_width = grapheme_width(grapheme);
            if current_width > 0 && current_width + g_width > width {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            if let Some(last) = current.last_mut()
                && last.style == span.style
            {
                last.text.push_str(grapheme);
            } else {
                current.push(Span { text: grapheme.to_string(), style: span.style });
            }
            current_width += g_width;
        }
    }

    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}

/// Left-pad a row's spans with `count` cells using `pad_style`.
pub fn pad_left(spans: Vec<Span>, count: usize, pad_style: CellStyle) -> Vec<Span> {
    if count == 0 {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len() + 1);
    out.push(Span::styled(" ".repeat(count), pad_style));
    out.extend(spans);
    out
}

/// Right-pad a row's spans with `count` cells using `pad_style`.
pub fn pad_right(spans: Vec<Span>, count: usize, pad_style: CellStyle) -> Vec<Span> {
    if count == 0 {
        return spans;
    }
    let mut out = spans;
    out.push(Span::styled(" ".repeat(count), pad_style));
    out
}

/// Pad a row on both sides to reach exactly `width` columns.
///
/// Left pad is `min(2)`, right pad absorbs the remainder. Matches the
/// renderer's transcript block padding so committed rows align with the live
/// region.
pub fn pad_row(spans: Vec<Span>, width: usize, pad_style: CellStyle) -> Vec<Span> {
    if width == 0 {
        return Vec::new();
    }
    let used = spans_width(&spans);
    let left_pad = width.min(2);
    let right_pad = width.saturating_sub(left_pad + used).min(2);
    let mut out = pad_left(spans, left_pad, pad_style);
    let body = width.saturating_sub(left_pad + right_pad);
    let fill = body.saturating_sub(used);
    if fill > 0 {
        out.push(Span::styled(" ".repeat(fill), pad_style));
    }
    out = pad_right(out, right_pad, pad_style);
    out
}

/// Truncate spans to `width` columns, appending `…` if anything was cut.
///
/// The ellipsis occupies one column, so the maximum retained content is
/// `width - 1` when truncation occurs.
#[cfg(test)]
pub fn truncate_spans(spans: &[Span], width: usize, ellipsis_style: CellStyle) -> Vec<Span> {
    if width == 0 {
        return Vec::new();
    }
    if spans_width(spans) <= width {
        return spans.to_vec();
    }

    let keep_width = width.saturating_sub(1);
    let mut out = Vec::new();
    let mut used = 0usize;

    for span in spans {
        if used >= keep_width {
            break;
        }
        let mut taken = String::new();
        for grapheme in span.text.graphemes(true) {
            let g_width = grapheme_width(grapheme);
            if used + g_width > keep_width {
                break;
            }
            taken.push_str(grapheme);
            used += g_width;
        }
        if !taken.is_empty() {
            out.push(Span { text: taken, style: span.style });
        }
    }

    out.push(Span::styled("…".to_string(), ellipsis_style));
    out
}

/// Width (column count) of a span slice.
pub fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| display_width(&s.text)).sum()
}

/// Display width of a string in terminal columns.
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Display width of one grapheme cluster in terminal columns.
pub fn grapheme_width(grapheme: &str) -> usize {
    UnicodeWidthStr::width(grapheme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::style::Color;

    #[test]
    fn content_width_subtracts_block_padding() {
        assert_eq!(content_width(80), 76);
        assert_eq!(content_width(4), 0);
        assert_eq!(content_width(0), 0);
        assert_eq!(content_width(6), 2);
    }

    #[test]
    fn wrap_text_preserves_newlines() {
        let rows = wrap_text("hello\nworld", 80);
        assert_eq!(rows, vec!["hello", "world"]);
    }

    #[test]
    fn wrap_text_empty_line_preserved() {
        let rows = wrap_text("a\n\nb", 80);
        assert_eq!(rows, vec!["a", "", "b"]);
    }

    #[test]
    fn wrap_text_word_wraps() {
        let rows = wrap_text("aa bb cc", 5);
        assert_eq!(rows, vec!["aa bb", "cc"]);
    }

    #[test]
    fn wrap_text_hard_splits_long_words() {
        let rows = wrap_text("abcdef", 3);
        assert_eq!(rows, vec!["abc", "def"]);
    }

    #[test]
    fn wrap_text_uses_display_width() {
        let rows = wrap_text("ab中cd", 4);
        assert_eq!(rows, vec!["ab中", "cd"]);
    }

    #[test]
    fn wrap_text_keeps_emoji_zwj_grapheme_together() {
        let family = "👨\u{200d}👩\u{200d}👧";
        let rows = wrap_text(&format!("a{family}b"), 3);
        assert_eq!(rows, vec![format!("a{family}"), "b".to_string()]);
    }

    #[test]
    fn wrap_spans_preserves_styles() {
        let spans = vec![
            Span::styled("red", CellStyle::new().fg(Color::Red)),
            Span::styled("blue", CellStyle::new().fg(Color::Blue)),
        ];
        let rows = wrap_spans(&spans, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 2);
        assert_eq!(rows[0][0].text, "red");
        assert_eq!(rows[0][1].text, "blue");
    }

    #[test]
    fn wrap_spans_preserves_newlines() {
        let spans = vec![Span::plain("ab\ncd")];
        let rows = wrap_spans(&spans, 80);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, "ab");
        assert_eq!(rows[1][0].text, "cd");
    }

    #[test]
    fn wrap_spans_empty_input_produces_empty_row() {
        let rows = wrap_spans(&[], 10);
        assert_eq!(rows, vec![vec![]]);
    }

    #[test]
    fn wrap_spans_splits_at_width() {
        let spans = vec![Span::plain("abcdef")];
        let rows = wrap_spans(&spans, 3);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, "abc");
        assert_eq!(rows[1][0].text, "def");
    }

    #[test]
    fn wrap_spans_uses_display_width() {
        let spans = vec![Span::plain("a中b")];
        let rows = wrap_spans(&spans, 3);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, "a中");
        assert_eq!(rows[1][0].text, "b");
    }

    #[test]
    fn wrap_spans_keeps_combining_mark_with_base() {
        let spans = vec![Span::plain("ab\u{0327}")];
        let rows = wrap_spans(&spans, 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].text, "ab\u{0327}");
    }

    #[test]
    fn wrap_spans_keeps_emoji_zwj_grapheme_together() {
        let family = "👨\u{200d}👩\u{200d}👧";
        let spans = vec![Span::plain(format!("a{family}b"))];
        let rows = wrap_spans(&spans, 3);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].text, format!("a{family}"));
        assert_eq!(rows[1][0].text, "b");
    }

    #[test]
    fn pad_row_adds_left_and_right_padding() {
        let spans = vec![Span::plain("hi")];
        let padded = pad_row(spans, 10, CellStyle::new());
        assert_eq!(spans_width(&padded), 10);
        assert_eq!(padded[0].text, "  ");
        assert_eq!(padded.last().unwrap().text, "  ");
    }

    #[test]
    fn pad_row_zero_width_returns_empty() {
        let spans = vec![Span::plain("hi")];
        let padded = pad_row(spans, 0, CellStyle::new());
        assert!(padded.is_empty());
    }

    #[test]
    fn truncate_spans_adds_ellipsis() {
        let spans = vec![Span::plain("hello world")];
        let out = truncate_spans(&spans, 8, CellStyle::default());
        assert_eq!(spans_width(&out), 8);
        assert!(out.last().unwrap().text.contains('…'));
    }

    #[test]
    fn truncate_spans_short_unchanged() {
        let spans = vec![Span::plain("hi")];
        let out = truncate_spans(&spans, 10, CellStyle::default());
        assert_eq!(spans_width(&out), 2);
        assert_eq!(out[0].text, "hi");
    }

    #[test]
    fn truncate_spans_zero_width_empty() {
        let spans = vec![Span::plain("hi")];
        let out = truncate_spans(&spans, 0, CellStyle::default());
        assert!(out.is_empty());
    }

    #[test]
    fn truncate_spans_keeps_emoji_zwj_grapheme_together() {
        let family = "👨\u{200d}👩\u{200d}👧";
        let spans = vec![Span::plain(format!("{family}abc"))];
        let out = truncate_spans(&spans, 4, CellStyle::default());
        assert_eq!(spans_width(&out), 4);
        assert_eq!(out[0].text, format!("{family}a"));
        assert_eq!(out.last().unwrap().text, "…");
    }

    #[test]
    fn wrap_text_wraps_long_url() {
        let url = "https://github.com/stormlight-labs/thndrs/blob/main/src/renderer/layout.rs";
        let rows = wrap_text(url, 30);
        assert!(rows.len() > 1, "long URL should wrap to multiple rows");
        assert!(
            rows.iter().all(|r| display_width(r) <= 30),
            "no wrapped row should exceed width 30"
        );

        let joined = rows.join("");
        assert!(joined.contains("thndrs"), "URL content should survive wrapping");
    }

    #[test]
    fn wrap_text_hard_splits_unbroken_url() {
        let url = "https://example.com/very/long/path/that/exceeds/the/width";
        let rows = wrap_text(url, 20);
        assert!(rows.len() > 1, "long unbroken URL should hard-split");
        assert!(
            rows.iter().all(|r| display_width(r) <= 20),
            "no row should exceed width 20"
        );
    }

    #[test]
    fn wrap_text_wraps_prose_paragraph() {
        let prose = "The renderer must keep display width and text boundaries separate. \
                     Display width decides cell budgets and cursor columns. Grapheme \
                     boundaries decide edit, wrap, truncate, and backend clipping steps.";
        let rows = wrap_text(prose, 40);
        assert!(rows.len() > 2, "prose paragraph should wrap to multiple rows");
        assert!(
            rows.iter().all(|r| display_width(r) <= 40),
            "no wrapped row should exceed width 40"
        );
        assert!(
            rows[0].contains("renderer"),
            "first row should start with the beginning of the prose"
        );
    }

    #[test]
    fn wrap_spans_wraps_mixed_styled_spans() {
        let spans = vec![
            Span::styled("error: ", CellStyle::new().fg(Color::Red).bold()),
            Span::styled("mismatched types", CellStyle::new().fg(Color::Yellow)),
            Span::plain(" in src/main.rs at line 42"),
        ];
        let rows = wrap_spans(&spans, 20);
        assert!(rows.len() > 1, "mixed styled spans should wrap to multiple rows");
        assert!(
            rows.iter().all(|r| spans_width(r) <= 20),
            "no wrapped row should exceed width 20"
        );
        assert!(rows[0][0].text.contains("error"));
    }

    #[test]
    fn wrap_spans_preserves_style_boundaries_across_wrap() {
        let spans = vec![
            Span::styled("red-text-here", CellStyle::new().fg(Color::Red)),
            Span::styled("blue-text-here", CellStyle::new().fg(Color::Blue)),
        ];
        let rows = wrap_spans(&spans, 14);
        assert!(rows.len() > 1, "should wrap at width 14");
        for row in &rows {
            let styles: Vec<_> = row.iter().map(|s| s.style).collect();
            for window in styles.windows(2) {
                assert_ne!(window[0], window[1], "adjacent spans should have distinct styles");
            }
        }
    }

    #[test]
    fn wrap_text_terminal_cell_clipping_cjk_at_boundary() {
        let rows = wrap_text("ab中c", 3);
        assert_eq!(rows, vec!["ab".to_string(), "中c".to_string()]);
    }

    #[test]
    fn wrap_spans_clips_wide_grapheme_at_boundary() {
        let flag = "🇺🇸";
        let spans = vec![Span::plain(format!("a{flag}b"))];
        let rows = wrap_spans(&spans, 2);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0].text, "a");
        assert_eq!(rows[1][0].text, flag);
        assert_eq!(rows[2][0].text, "b");
    }

    /// Confirm that `wrap_text` and `wrap_spans` remain renderer-owned
    /// (no Textwrap dependency). If someone adds Textwrap, the Cargo.toml
    /// dependency check below will fail.
    #[test]
    fn wrap_text_and_wrap_spans_are_renderer_owned() {
        let text_rows = wrap_text("hello world test", 10);
        assert_eq!(text_rows, vec!["hello", "world test"]);

        let spans = vec![
            Span::styled("red", CellStyle::new().fg(Color::Red)),
            Span::styled("blue", CellStyle::new().fg(Color::Blue)),
        ];
        let span_rows = wrap_spans(&spans, 10);
        assert_eq!(span_rows.len(), 1, "renderer-owned wrap_spans should work");
    }

    /// Confirm that the project does not depend on the `textwrap` crate.
    #[test]
    fn no_textwrap_dependency() {
        let rows = wrap_text("a b c d e f", 5);
        assert!(rows.iter().all(|r| display_width(r) <= 5));
    }
}
