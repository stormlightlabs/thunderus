use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

impl<'a> super::TranscriptRenderer<'a> {
    pub(super) fn wrap_text_styled(&self, text: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if max_width == 0 {
            return;
        }

        for source_line in text.lines() {
            if source_line.is_empty() {
                lines.push(Line::default());
                continue;
            }

            if self.is_path_or_url(source_line) {
                self.smart_wrap_path_styled(source_line, style, max_width, lines);
            } else {
                self.wrap_normal_text_styled(source_line, style, max_width, lines);
            }
        }
    }

    fn smart_wrap_path_styled(&self, path: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
        if path.width() <= max_width {
            lines.push(Line::from(vec![Span::styled(path.to_string(), style)]));
            return;
        }

        let mut remaining = path;
        while remaining.width() > max_width {
            if let Some(idx) = self.find_break_point(remaining, max_width) {
                let chunk = &remaining[..idx];
                lines.push(Line::from(vec![Span::styled(chunk.to_string(), style)]));
                remaining = &remaining[idx..];
            } else {
                lines.push(Line::from(vec![Span::styled(remaining.to_string(), style)]));
                break;
            }
        }

        if !remaining.is_empty() {
            lines.push(Line::from(vec![Span::styled(remaining.to_string(), style)]));
        }
    }

    fn wrap_normal_text_styled(&self, text: &str, style: Style, max_width: usize, lines: &mut Vec<Line<'static>>) {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            lines.push(Line::default());
            return;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in words {
            let word_width = word.width();
            let space_width = if current_line.is_empty() { 0 } else { 1 };

            if current_width + space_width + word_width > max_width {
                if !current_line.is_empty() {
                    lines.push(Line::from(vec![Span::styled(current_line.clone(), style)]));
                    current_line = String::new();
                    current_width = 0;
                }

                if word_width > max_width {
                    let chars = word.chars().peekable();
                    let mut chunk_width = 0;
                    let mut chunk = String::new();

                    for ch in chars {
                        let ch_width = ch.width().unwrap_or(0);

                        if chunk_width + ch_width > max_width {
                            lines.push(Line::from(vec![Span::styled(chunk.clone(), style)]));
                            chunk.clear();
                            chunk_width = 0;
                        }

                        chunk.push(ch);
                        chunk_width += ch_width;
                    }

                    if !chunk.is_empty() {
                        lines.push(Line::from(vec![Span::styled(chunk.clone(), style)]));
                    }
                    continue;
                }
            }
            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += 1;
            }
            current_line.push_str(word);
            current_width += word_width;
        }

        if !current_line.is_empty() {
            lines.push(Line::from(vec![Span::styled(current_line, style)]));
        }
    }

    /// Wrap text to a specific width, returning Vec<String>
    pub(super) fn wrap_text_to_width(&self, text: &str, max_width: usize) -> Vec<String> {
        let mut result = Vec::new();
        if max_width == 0 {
            return result;
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return result;
        }

        let mut current_line = String::new();
        let mut current_width = 0;

        for word in words {
            let word_width = word.width();
            let space_width = if current_line.is_empty() { 0 } else { 1 };

            if current_width + space_width + word_width > max_width {
                if !current_line.is_empty() {
                    result.push(current_line.clone());
                    current_line = String::new();
                    current_width = 0;
                }

                if word_width > max_width {
                    let chars = word.chars().peekable();
                    let mut chunk_width = 0;
                    let mut chunk = String::new();

                    for ch in chars {
                        let ch_width = ch.width().unwrap_or(0);

                        if chunk_width + ch_width > max_width {
                            result.push(chunk.clone());
                            chunk.clear();
                            chunk_width = 0;
                        }

                        chunk.push(ch);
                        chunk_width += ch_width;
                    }

                    if !chunk.is_empty() {
                        result.push(chunk);
                    }
                    continue;
                }
            }

            if !current_line.is_empty() {
                current_line.push(' ');
                current_width += 1;
            }
            current_line.push_str(word);
            current_width += word_width;
        }

        if !current_line.is_empty() {
            result.push(current_line);
        }

        result
    }

    /// Check if text looks like a file path or URL
    fn is_path_or_url(&self, text: &str) -> bool {
        text.starts_with('/')
            || text.starts_with("./")
            || text.starts_with("../")
            || text.starts_with("http://")
            || text.starts_with("https://")
            || text.starts_with("git@")
            || text.starts_with("file://")
    }

    /// Find a good break point in path/URL (prefer /, ., etc.)
    fn find_break_point(&self, text: &str, max_width: usize) -> Option<usize> {
        let mut break_idx = None;
        for (i, ch) in text.char_indices() {
            if i > 0
                && i % max_width == 0
                && let Some(idx) = break_idx
            {
                return Some(idx);
            }
            if matches!(ch, '/' | '.' | '-' | '_') {
                break_idx = Some(i + ch.len_utf8());
            }
        }
        break_idx
    }
}
