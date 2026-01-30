use crate::syntax::SyntaxHighlighter;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

const CODE_BLOCK_START: &str = "```";
const CODE_BLOCK_END: &str = "```";

impl<'a> super::TranscriptRenderer<'a> {
    /// Render text with code block highlighting
    pub(super) fn render_with_code_highlighting(
        &self, text: &str, default_color: ratatui::style::Color, max_width: usize, lines: &mut Vec<Line<'static>>,
    ) {
        let default_style = Style::default().fg(default_color).bg(self.theme.panel_bg);
        let mut remaining = text;

        while let Some(start_idx) = remaining.find(CODE_BLOCK_START) {
            if start_idx > 0 {
                let before_code = &remaining[..start_idx];
                self.wrap_text_styled(before_code, default_style, max_width, lines);
            }

            let after_start = &remaining[start_idx + CODE_BLOCK_START.len()..];

            let lang_end_idx = after_start.find('\n').unwrap_or(after_start.len());
            let lang = after_start[..lang_end_idx].trim();

            let code_start = if lang_end_idx < after_start.len() { lang_end_idx + 1 } else { lang_end_idx };
            let code_block = &after_start[code_start..];

            if let Some(end_idx) = code_block.find(CODE_BLOCK_END) {
                let code = &code_block[..end_idx];
                remaining = &code_block[end_idx + CODE_BLOCK_END.len()..];

                self.render_code_block(code, lang, max_width, lines);
            } else {
                self.wrap_text_styled(
                    after_start,
                    Style::default().fg(self.theme.cyan).bg(self.theme.panel_bg),
                    max_width,
                    lines,
                );
                break;
            }
        }

        if !remaining.is_empty() {
            self.wrap_text_styled(remaining, default_style, max_width, lines);
        }
    }

    /// Render a code block with syntax highlighting and boxed frame
    fn render_code_block(&self, code: &str, lang: &str, width: usize, lines: &mut Vec<Line<'static>>) {
        let highlighter = SyntaxHighlighter::new();
        let highlighted = highlighter.highlight_code(code, lang.trim());
        let mut code_lines = Vec::new();

        for span in &highlighted {
            let styled = Span::styled(span.content.to_string(), span.style.bg(self.theme.panel_bg));
            code_lines.push(Line::from(vec![styled]));
        }

        let title = if lang.is_empty() { " Code ".to_string() } else { format!(" {} ", lang.to_uppercase()) };

        self.render_card(&title, self.theme.muted, width, code_lines, lines);
    }
}
