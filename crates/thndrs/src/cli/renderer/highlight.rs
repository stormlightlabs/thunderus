//! Renderer-native syntax highlighting using `syntect`.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::renderer::style::{CellStyle, Color, Span};

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
pub fn syntax_for(lang_or_ext: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    let syn_set = syntax_set();
    syn_set
        .find_syntax_by_extension(lang_or_ext)
        .or_else(|| syn_set.find_syntax_by_token(lang_or_ext))
}

/// Highlight a code string into rows of renderer spans.
///
/// If `lang` is provided and matches a known syntax, the code is highlighted
/// with colors from the default theme. If no match is found or highlighting
/// fails, the code is returned as plain unstyled spans.
pub fn highlight_lines(code: &str, lang: Option<&str>) -> Vec<Vec<Span>> {
    let syntax = lang.and_then(syntax_for);

    let Some(syntax) = syntax else {
        return code.lines().map(|l| vec![Span::plain(l.to_string())]).collect();
    };

    let mut highlighter = HighlightLines::new(syntax, theme());
    code.lines()
        .map(|line| match highlighter.highlight_line(line, syntax_set()) {
            Ok(regions) => regions
                .into_iter()
                .map(|(style, text)| Span::styled(text.to_string(), syntect_style_to_renderer(&style)))
                .collect(),
            Err(_) => vec![Span::plain(line.to_string())],
        })
        .collect()
}

/// Convert a syntect `Style` into a renderer [`CellStyle`].
fn syntect_style_to_renderer(style: &syntect::highlighting::Style) -> CellStyle {
    let fg = syntect_color_to_renderer(style.foreground);
    let mut cell = CellStyle::new().fg(fg);

    if style.font_style.contains(FontStyle::BOLD) {
        cell = cell.bold();
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        cell = cell.italic();
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        cell = cell.underlined();
    }

    cell
}

/// Convert a syntect `Color` into a renderer [`Color`].
fn syntect_color_to_renderer(color: syntect::highlighting::Color) -> Color {
    Color::Rgb { r: color.r, g: color.g, b: color.b }
}

/// Map a file path extension to a syntect language token.
pub fn path_extension_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rs"),
        "py" => Some("py"),
        "js" | "jsx" => Some("js"),
        "ts" | "tsx" => Some("ts"),
        "json" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "sh" | "bash" => Some("bash"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "hpp" | "cc" => Some("cpp"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "md" => Some("md"),
        "sql" => Some("sql"),
        _ => None,
    }
}

/// Determine the source language for a tool that returns file contents.
pub fn tool_output_language(tool_name: &str, arguments: &str) -> Option<&'static str> {
    match tool_name {
        "read_file_range" => {
            let v: serde_json::Value = serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
            let path = v.get("path").and_then(|p| p.as_str())?;
            path_extension_language(path)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_for_rust_extension() {
        assert!(syntax_for("rs").is_some());
    }

    #[test]
    fn syntax_for_json_extension() {
        assert!(syntax_for("json").is_some());
    }

    #[test]
    fn syntax_for_unknown_returns_none() {
        assert!(syntax_for("totallymadeup").is_none());
    }

    #[test]
    fn highlight_rust_code_produces_spans() {
        let code = "fn main() {\n    println!(\"hello\");\n}";
        let rows = highlight_lines(code, Some("rs"));
        assert_eq!(rows.len(), 3, "should produce one row per input line");
        assert!(!rows[0].is_empty(), "each row should have spans");
    }

    #[test]
    fn highlight_unknown_language_produces_plain_spans() {
        let rows = highlight_lines("some unknown code", Some("totallymadeup"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].style.fg, Color::Reset);
    }

    #[test]
    fn highlight_no_language_produces_plain_spans() {
        let rows = highlight_lines("plain text\nsecond line", None);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn highlight_rust_spans_have_rgb_colors() {
        let rows = highlight_lines("fn main() {}", Some("rs"));
        let has_color = rows.iter().flatten().any(|s| s.style.fg != Color::Reset);
        assert!(has_color, "highlighted code should have colored spans");
    }

    #[test]
    fn path_extension_language_maps_common_types() {
        assert_eq!(path_extension_language("main.rs"), Some("rs"));
        assert_eq!(path_extension_language("config.json"), Some("json"));
        assert_eq!(path_extension_language("script.sh"), Some("bash"));
        assert_eq!(path_extension_language("unknown.xyz"), None);
    }

    #[test]
    fn tool_output_language_detects_from_args() {
        let args = r#"{"path": "main.rs"}"#;
        assert_eq!(tool_output_language("read_file_range", args), Some("rs"));
        assert_eq!(tool_output_language("run_shell", "{}"), None);
        assert_eq!(tool_output_language("search_text", "{}"), None);
    }
}
