//! Small `thndrs` banner component using [`figlet_rs`]
//!
//! The banner is rendered from a committed `.flf` font loaded at compile time
//! via `include_str!`, so the app does not depend on runtime font paths.
//!
//! If the font cannot parse, or the terminal is too narrow for the banner, the
//! banner falls back to plain `thndrs` text.

use figlet_rs::FIGlet;

/// The committed FIGlet font file, loaded at compile time via `include_str!`
/// so the app does not depend on runtime font paths.
const FONT_CONTENT: &str = include_str!("fonts/ansi_shadow.flf");

/// Minimum terminal width required to render the FIGlet banner.
///
/// The ANSI Shadow font renders "thndrs" at ~40 columns. Below this width we
/// fall back to plain text so the output is not garbled.
pub const BANNER_MIN_WIDTH: u16 = 42;

/// Render the `thndrs` banner as a multi-line string.
///
/// Returns the FIGlet art when the font parses and the width is sufficient;
/// otherwise returns the plain text `thndrs`.
pub fn render_banner(width: u16) -> String {
    if width < BANNER_MIN_WIDTH {
        String::from("thndrs")
    } else {
        match FIGlet::from_content(FONT_CONTENT) {
            Ok(font) => match font.convert("thndrs") {
                Some(figure) => figure.to_string(),
                None => String::from("thndrs"),
            },
            Err(_) => String::from("thndrs"),
        }
    }
}

/// Split the banner into lines for rendering, trimming trailing whitespace.
pub fn banner_lines(width: u16) -> Vec<String> {
    render_banner(width)
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_renders_figlet_at_normal_width() {
        let lines = banner_lines(80);
        assert!(!lines.is_empty(), "banner should produce lines");
        assert!(
            lines.len() <= 7,
            "ansi shadow height should be <= 7, got {}",
            lines.len()
        );
    }

    #[test]
    fn banner_falls_back_to_plain_text_when_narrow() {
        let lines = banner_lines(20);
        assert_eq!(lines, vec!["thndrs"]);
    }

    #[test]
    fn banner_falls_back_at_threshold_boundary() {
        assert_eq!(banner_lines(BANNER_MIN_WIDTH - 1), vec!["thndrs"]);
        let lines = banner_lines(BANNER_MIN_WIDTH);
        assert!(lines.len() > 1, "at threshold should render multi-line figlet");
    }

    #[test]
    fn render_banner_plain_when_too_narrow() {
        let banner = render_banner(10);
        assert_eq!(banner, "thndrs");
    }

    #[test]
    fn committed_font_parses_successfully() {
        let font = FIGlet::from_content(FONT_CONTENT);
        assert!(font.is_ok(), "committed ansi_shadow.flf should parse");
    }
}
