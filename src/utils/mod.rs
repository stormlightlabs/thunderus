use crate::tools::MAX_LINE_LEN;

use std::path::PathBuf;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Return the current user's home directory from common platform env vars.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Escape text for XML element content.
pub fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Truncate a string to [`MAX_LINE_LEN`] chars, adding `...` if truncated.
pub fn truncate_line(s: &str) -> String {
    if s.chars().count() <= MAX_LINE_LEN {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(MAX_LINE_LEN).collect::<String>())
    }
}

/// Truncate a string to `max_width` terminal columns, appending `…` if truncated.
///
/// Unlike [`truncate_line`] which uses a fixed cap and `...`, this is a
/// general-purpose helper for width-aware UI truncation: it takes an explicit
/// max, uses a single `…` ellipsis, and counts display columns (not bytes).
pub fn truncate_ellipsis(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text_width(s) <= max_width {
        return s.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }
    format!("{}…", take_display_width(s, max_width - 1))
}

/// Truncate from the start (keeping the end), prefixing `…` if truncated.
///
/// Useful for paths and URLs where the end is more informative.
pub fn truncate_ellipsis_start(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text_width(s) <= max_width {
        return s.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }

    format!("…{}", take_display_width_from_end(s, max_width - 1))
}

/// Display width of a string in terminal columns.
pub fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Display width of one grapheme cluster in terminal columns.
pub fn grapheme_width(grapheme: &str) -> usize {
    UnicodeWidthStr::width(grapheme)
}

fn take_display_width(text: &str, max_width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true) {
        let width = text_width(grapheme);
        if used + width > max_width {
            break;
        }
        out.push_str(grapheme);
        used += width;
    }
    out
}

fn take_display_width_from_end(text: &str, max_width: usize) -> String {
    let mut chunks = Vec::new();
    let mut used = 0usize;
    for grapheme in text.graphemes(true).rev() {
        let width = text_width(grapheme);
        if used + width > max_width {
            break;
        }
        chunks.push(grapheme);
        used += width;
    }
    chunks.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_line_short_unchanged() {
        assert_eq!(truncate_line("hello"), "hello");
    }

    #[test]
    fn truncate_line_long_truncated() {
        let c = MAX_LINE_LEN;
        let long = "x".repeat(c + 100);
        let result = truncate_line(&long);
        assert!(result.ends_with("..."));
        assert!(result.chars().count() <= c + 3);
    }

    #[test]
    fn truncate_ellipsis_short_unchanged() {
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_ellipsis_exact_fit() {
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_ellipsis_truncates_with_ellipsis() {
        assert_eq!(truncate_ellipsis("hello world", 8), "hello w…");
    }

    #[test]
    fn truncate_ellipsis_uses_display_width() {
        assert_eq!(truncate_ellipsis("ab中cd", 5), "ab中…");
    }

    #[test]
    fn truncate_ellipsis_keeps_grapheme_together() {
        let family = "👨\u{200d}👩\u{200d}👧";
        assert_eq!(truncate_ellipsis(&format!("{family}abc"), 4), format!("{family}a…"));
    }

    #[test]
    fn truncate_ellipsis_zero_max() {
        assert_eq!(truncate_ellipsis("hello", 0), "");
    }

    #[test]
    fn truncate_ellipsis_one_max() {
        assert_eq!(truncate_ellipsis("hello", 1), "…");
    }

    #[test]
    fn truncate_ellipsis_start_keeps_end() {
        assert_eq!(truncate_ellipsis_start("/long/path/to/file.rs", 15), "…ath/to/file.rs");
    }

    #[test]
    fn truncate_ellipsis_start_uses_display_width() {
        assert_eq!(truncate_ellipsis_start("ab中cd", 5), "…中cd");
    }

    #[test]
    fn truncate_ellipsis_start_short_unchanged() {
        assert_eq!(truncate_ellipsis_start("short", 10), "short");
    }
}
