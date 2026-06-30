//! Syntax highlighting for code-oriented transcript blocks using `syntect`.
//!
//! Provides cached syntax/theme sets, extension/language detection, and a
//! mapping from syntect color/style data into Ratatui spans.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Cached syntax set for parsing code by file extension/language.
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Cached theme set for syntax highlighting colors.
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

/// Get the cached syntax set (built-in defaults).
fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Get the cached theme set (built-in defaults).
fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Get the default highlighting theme.
fn theme() -> &'static syntect::highlighting::Theme {
    theme_set().themes.get("base16-ocean.dark").expect("default theme")
}

/// Guess a syntax definition from a file extension or language name.
///
/// Returns `None` if no matching syntax is found.
pub fn syntax_for(lang_or_ext: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    let syn_set = syntax_set();
    syn_set
        .find_syntax_by_extension(lang_or_ext)
        .or_else(|| syn_set.find_syntax_by_token(lang_or_ext))
}

/// Highlight a code string into Ratatui lines.
///
/// If `lang` is provided and matches a known syntax, the code is highlighted
/// with colors from the default theme. If no match is found or highlighting
/// fails, the code is returned as plain unstyled lines.
pub fn highlight_lines(code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
    let syntax = lang.and_then(syntax_for);

    let Some(syntax) = syntax else {
        return code
            .lines()
            .map(|l| Line::from(vec![Span::styled(l.to_string(), Style::default())]))
            .collect();
    };

    let mut highlighter = HighlightLines::new(syntax, theme());
    code.lines()
        .map(|line| match highlighter.highlight_line(line, syntax_set()) {
            Ok(regions) => {
                let spans: Vec<Span<'static>> = regions
                    .into_iter()
                    .map(|(style, text)| {
                        let ratatui_style = syntect_style_to_ratatui(&style);
                        Span::styled(text.to_string(), ratatui_style)
                    })
                    .collect();
                Line::from(spans)
            }
            Err(_) => Line::from(vec![Span::styled(line.to_string(), Style::default())]),
        })
        .collect()
}

/// Highlight a code string with detected language from a file path.
#[allow(dead_code)]
pub fn highlight_code(code: &str, path: Option<&str>) -> Vec<Line<'static>> {
    let lang = path.and_then(|p| {
        let ext = p.rsplit('.').next()?;
        Some(ext)
    });
    highlight_lines(code, lang)
}

/// Convert a syntect `Style` into a Ratatui `Style`.
fn syntect_style_to_ratatui(style: &syntect::highlighting::Style) -> Style {
    let fg = syntect_color_to_ratatui(style.foreground);
    let mut ratatui_style = Style::default().fg(fg);

    if style.font_style.contains(FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }

    ratatui_style
}

/// Convert a syntect `Color` into a Ratatui `Color`.
fn syntect_color_to_ratatui(color: syntect::highlighting::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_for_rust_extension() {
        let syntax = syntax_for("rs");
        assert!(syntax.is_some(), "should detect .rs extension");
    }

    #[test]
    fn syntax_for_json_extension() {
        let syntax = syntax_for("json");
        assert!(syntax.is_some(), "should detect .json extension");
    }

    #[test]
    fn syntax_for_unknown_returns_none() {
        let syntax = syntax_for("totallymadeup");
        assert!(syntax.is_none(), "unknown language should return None");
    }

    #[test]
    fn highlight_rust_code_produces_styled_lines() {
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let lines = highlight_lines(code, Some("rs"));
        assert_eq!(lines.len(), 3, "should produce one line per input line");
    }

    #[test]
    fn highlight_unknown_language_produces_plain_lines() {
        let code = "some unknown code";
        let lines = highlight_lines(code, Some("totallymadeup"));
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn highlight_no_language_produces_plain_lines() {
        let code = "plain text\nsecond line";
        let lines = highlight_lines(code, None);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn highlight_code_with_path_detects_extension() {
        let code = "x = 1";
        let lines = highlight_code(code, Some("script.py"));
        assert!(!lines.is_empty());
    }
}
