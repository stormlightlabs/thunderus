use crate::tools::Cap;

/// Truncate a string to [`caps::MAX_LINE_LENGTH`] chars, adding `...` if truncated.
pub fn truncate_line(s: &str) -> String {
    if s.chars().count() <= Cap::MaxLineLen.into() {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(Cap::MaxLineLen.into()).collect();
        format!("{truncated}...")
    }
}

/// Truncate a string to `max_chars` visible chars, appending `…` if truncated.
///
/// Unlike [`truncate_line`] which uses a fixed cap and `...`, this is a
/// general-purpose helper for width-aware UI truncation: it takes an explicit
/// max, uses a single `…` ellipsis, and counts chars (not bytes).
pub fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let keep = max_chars - 1;
    let truncated: String = s.chars().take(keep).collect();
    format!("{truncated}…")
}

/// Truncate from the start (keeping the end), prefixing `…` if truncated.
///
/// Useful for paths and URLs where the end is more informative.
pub fn truncate_ellipsis_start(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let skip = count - (max_chars - 1);
    let kept: String = s.chars().skip(skip).collect();
    format!("…{kept}")
}

pub fn text_width(text: &str) -> usize {
    text.chars().count()
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
        let c = usize::from(Cap::MaxLineLen);
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
    fn truncate_ellipsis_start_short_unchanged() {
        assert_eq!(truncate_ellipsis_start("short", 10), "short");
    }
}
