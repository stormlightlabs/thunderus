//! Width-aware layout helpers for the row model.
//!
//! These functions operate on the renderer's own [`Span`] and [`CellStyle`]
//! types. They are the single source of truth for wrapping, padding, and
//! truncation so that cursor placement and snapshots stay deterministic.

#![allow(dead_code)]

use super::style::{CellStyle, Span};

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
        for word in raw_line.split_whitespace() {
            let word_len = word.chars().count();
            let current_len = current.chars().count();

            if current_len == 0 {
                if word_len <= width {
                    current.push_str(word);
                } else {
                    rows.extend(split_long_word(word, width));
                }
            } else if current_len + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                rows.push(std::mem::take(&mut current));
                if word_len <= width {
                    current.push_str(word);
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
    for ch in word.chars() {
        if current.chars().count() == width {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
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
        if let Some(last) = current.last_mut()
            && last.style == span.style
            && !span.text.contains('\n')
        {
            last.text.push_str(&span.text);
            current_width += span.text.chars().count();
            if current_width >= width {
                flush_wrapped_row(&mut rows, &mut current, &mut current_width, width);
            }
            continue;
        }

        for ch in span.text.chars() {
            if ch == '\n' {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
                continue;
            }
            if current_width == width {
                rows.push(std::mem::take(&mut current));
                current_width = 0;
            }
            if let Some(last) = current.last_mut()
                && last.style == span.style
            {
                last.text.push(ch);
            } else {
                current.push(Span { text: ch.to_string(), style: span.style });
            }
            current_width += 1;
        }
    }

    if !current.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    rows
}

/// Move completed content from `current` into `rows`, splitting a span that
/// overflows `width` so the remainder starts the next row.
fn flush_wrapped_row(rows: &mut Vec<Vec<Span>>, current: &mut Vec<Span>, current_width: &mut usize, width: usize) {
    let mut working = std::mem::take(current);
    *current_width = 0;

    while working_width(&working) > width {
        let mut row = Vec::new();
        let mut row_width = 0usize;

        while !working.is_empty() {
            let span = working.first_mut().unwrap();
            let span_len = span.text.chars().count();
            let remaining = width - row_width;
            if span_len <= remaining {
                row_width += span_len;
                row.push(working.remove(0));
            } else {
                let taken: String = span.text.chars().take(remaining).collect();
                let rest: String = span.text.chars().skip(remaining).collect();
                row.push(Span { text: taken, style: span.style });
                span.text = rest;
                break;
            }
        }
        rows.push(row);
    }

    if !working.is_empty() {
        *current_width = working_width(&working);
        *current = working;
    }
}

fn working_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
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
/// Left pad is `min(2)`, right pad absorbs the remainder. Matches the existing
/// transcript block padding so rows align with `ui::transcript::block_line`.
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
pub fn truncate_spans(spans: &[Span], width: usize, ellipsis_style: CellStyle) -> Vec<Span> {
    if width == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut used = 0usize;

    for span in spans {
        if used >= width {
            break;
        }
        let span_len = span.text.chars().count();
        let remaining = width - used;
        if span_len <= remaining {
            used += span_len;
            out.push(span.clone());
        } else {
            let taken: String = span.text.chars().take(remaining).collect();
            out.push(Span { text: taken, style: span.style });
            break;
        }
    }

    let original: usize = spans_width(spans);
    if original > width && width > 0 {
        if let Some(last) = out.last_mut() {
            let last_len = last.text.chars().count();
            if last_len > 1 {
                let kept: String = last.text.chars().take(last_len - 1).collect();
                last.text = kept;
            } else {
                out.pop();
            }
        }
        out.push(Span::styled("…".to_string(), ellipsis_style));
    }

    out
}

/// Width (column count) of a span slice.
pub fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
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
}
