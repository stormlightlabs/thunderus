use ratatui::style::{Color, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Syntax highlighter for code blocks
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    /// Create a new syntax highlighter with default settings
    pub fn new() -> Self {
        let theme_set = ThemeSet::load_defaults();
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme: theme_set.themes["base16-ocean.dark"].clone(),
        }
    }

    /// Highlight a code block and return styled spans
    pub fn highlight_code(&self, code: &str, lang: &str) -> Vec<Span<'static>> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| self.syntax_set.find_syntax_by_name(lang))
            .or_else(|| self.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);

        let mut spans = Vec::new();
        let mut current_color = self.text_color();

        for line in LinesWithEndings::from(code) {
            if let Ok(ranges) = highlighter.highlight_line(line, &self.syntax_set) {
                for (style, text) in ranges {
                    let color = self.syntect_to_ratatui_color(&style.foreground);
                    let span_text = text.to_string();

                    if color != current_color {
                        spans.push(Span::styled(span_text, Style::default().fg(color)));
                        current_color = color;
                    } else if let Some(last) = spans.last_mut() {
                        let mut new_content = String::new();
                        new_content.push_str(&last.content);
                        new_content.push_str(&span_text);
                        last.content = new_content.into();
                    } else {
                        spans.push(Span::styled(span_text, Style::default().fg(color)));
                    }
                }
            }
        }

        spans
    }

    /// Convert syntect color to ratatui color
    fn syntect_to_ratatui_color(&self, color: &syntect::highlighting::Color) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }

    /// Get default text color from theme
    fn text_color(&self) -> Color {
        let settings = &self.theme.settings;
        let text_color =
            settings
                .foreground
                .as_ref()
                .unwrap_or(&syntect::highlighting::Color { r: 198, g: 200, b: 209, a: 255 });
        Color::Rgb(text_color.r, text_color.g, text_color.b)
    }

    /// Get a color for a simple fallback (when language detection fails)
    pub fn fallback_color(&self) -> Color {
        self.text_color()
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntax_highlighter_new() {
        let highlighter = SyntaxHighlighter::new();
        assert!(!highlighter.syntax_set.syntaxes().is_empty());
    }

    #[test]
    fn test_highlight_rust() {
        let highlighter = SyntaxHighlighter::new();
        let code = r#"fn main() {
    println!("Hello, world!");
}"#;
        let spans = highlighter.highlight_code(code, "rust");
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_python() {
        let highlighter = SyntaxHighlighter::new();
        let code = "def hello():\n    print('Hello')";
        let spans = highlighter.highlight_code(code, "python");
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_unknown_lang() {
        let highlighter = SyntaxHighlighter::new();
        let code = "some code here";
        let spans = highlighter.highlight_code(code, "unknownlangxyz");
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_fallback_color() {
        let highlighter = SyntaxHighlighter::new();
        let color = highlighter.fallback_color();
        assert!(matches!(color, Color::Rgb(_, _, _)));
    }
}
