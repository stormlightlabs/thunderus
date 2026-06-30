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
}
