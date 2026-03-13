use ::iocraft::prelude::Color;
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

const MAX_FILE_LINES: usize = 2_000;

#[derive(Debug, Clone)]
pub struct HighlightSegment {
    pub text: String,
    pub fg: Color,
    pub bold: bool,
}

#[derive(Debug, Clone)]
pub struct HighlightedLine {
    pub line_number: usize,
    pub segments: Vec<HighlightSegment>,
}

pub fn highlight_file(path: &Path, content: &str) -> Vec<HighlightedLine> {
    let syntax_set = SyntaxSet::load_defaults_newlines();
    let theme_set = ThemeSet::load_defaults();
    let theme = choose_theme(&theme_set);

    let syntax = syntax_set
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for (idx, line) in content.lines().take(MAX_FILE_LINES).enumerate() {
        let ranges = highlighter
            .highlight_line(line, &syntax_set)
            .unwrap_or_else(|_| vec![(syntect::highlighting::Style::default(), line)]);

        let segments = ranges
            .into_iter()
            .map(|(style, text)| HighlightSegment {
                text: text.to_string(),
                fg: Color::Rgb { r: style.foreground.r, g: style.foreground.g, b: style.foreground.b },
                bold: style.font_style.contains(syntect::highlighting::FontStyle::BOLD),
            })
            .collect::<Vec<_>>();

        lines.push(HighlightedLine { line_number: idx + 1, segments });
    }

    if lines.is_empty() {
        lines.push(HighlightedLine { line_number: 1, segments: Vec::new() });
    }

    lines
}

fn choose_theme(themes: &ThemeSet) -> &Theme {
    if let Some(theme) = themes.themes.get("base16-mocha.dark") {
        return theme;
    }

    if let Some((_, theme)) = themes.themes.iter().next() {
        return theme;
    }

    panic!("syntect theme set is unexpectedly empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_file_returns_line_numbers() {
        let lines = highlight_file(Path::new("src/main.rs"), "fn main() {}\n");
        assert_eq!(lines[0].line_number, 1);
    }
}
