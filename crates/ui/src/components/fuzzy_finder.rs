use crate::fuzzy_finder::FuzzyFinder;
use crate::state::AppState;
use crate::theme::{Theme, ThemePalette};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

/// Fuzzy finder UI component
pub struct FuzzyFinderComponent<'a> {
    state: &'a AppState,
}

impl<'a> FuzzyFinderComponent<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// Render the fuzzy finder as an overlay
    pub fn render(&self, frame: &mut Frame<'_>) {
        let size = frame.area();
        let theme = Theme::palette(self.state.theme_variant());

        let overlay_size = Rect {
            x: size.x + size.width / 4,
            y: size.y + size.height / 8,
            width: size.width / 2,
            height: size.height * 3 / 4,
        };

        let layout = Layout::new(
            Direction::Vertical,
            [Constraint::Length(3), Constraint::Min(1), Constraint::Length(8)],
        )
        .split(overlay_size);

        let input_area = layout[0];
        let list_area = layout[1];
        let preview_area = layout[2];

        frame.render_widget(Clear, overlay_size);

        if let Some(finder) = self.state.fuzzy_finder() {
            self.render_input(frame, finder, input_area, theme);
            self.render_list(frame, finder, list_area, theme);
            self.render_preview(frame, finder, preview_area, theme);
        }
    }

    fn render_input(&self, frame: &mut Frame<'_>, finder: &FuzzyFinder, area: Rect, theme: ThemePalette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.blue))
            .title(Span::styled("File Finder", Style::default().fg(theme.blue).bold()))
            .bg(theme.panel_bg);

        let input_text = finder.pattern();
        let prompt = "> ";

        let mut spans = vec![Span::styled(prompt, Style::default().fg(theme.blue).bg(theme.panel_bg))];
        spans.push(Span::styled(
            input_text,
            Style::default().fg(theme.fg).bg(theme.panel_bg),
        ));
        spans.push(Span::styled("█", Style::default().bg(theme.fg).fg(theme.fg)));

        let paragraph = Paragraph::new(Line::from(spans)).block(block);

        frame.render_widget(paragraph, area);
    }

    fn render_list(&self, frame: &mut Frame<'_>, finder: &FuzzyFinder, area: Rect, theme: ThemePalette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .title(Span::styled(
                format!("Results ({}/{})", finder.match_count(), finder.total_file_count()),
                Style::default().fg(theme.muted),
            ))
            .bg(theme.panel_bg);

        let results: Vec<ListItem> = finder
            .results()
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                let is_selected = idx == finder.selected_index();
                let style = if is_selected {
                    Style::default()
                        .fg(theme.fg)
                        .bg(theme.highlight)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.fg)
                };

                let language = self.get_language_from_extension(file.extension().unwrap_or(""));
                let language_span = if let Some(lang) = language {
                    Span::styled(format!(" [{}]", lang), Style::default().fg(theme.cyan))
                } else {
                    Span::raw("")
                };

                let line = Line::from(vec![
                    Span::raw("  "),
                    Span::styled(&file.relative_path, style),
                    language_span,
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(results)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(list, area);
    }

    fn render_preview(&self, frame: &mut Frame<'_>, finder: &FuzzyFinder, area: Rect, theme: ThemePalette) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.muted))
            .bg(theme.panel_bg);

        if let Some(selected) = finder.selected() {
            let title = format!(
                "Preview: {} [{}/{}]",
                selected.relative_path,
                self.get_line_count(&selected.path),
                self.get_total_lines(&selected.path)
            );

            let block = block.title(Span::styled(title, Style::default().fg(theme.muted)));

            let content = if let Ok(text) = std::fs::read_to_string(&selected.path) {
                let lines: Vec<String> = text
                    .lines()
                    .take(6)
                    .enumerate()
                    .map(|(i, line)| format!("{}  {}", i + 1, line))
                    .collect();

                Text::from(lines.join("\n"))
            } else {
                Text::from("Unable to read file")
            };

            let paragraph = Paragraph::new(content).block(block).wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new("No file selected")
                .block(block)
                .alignment(Alignment::Center);

            frame.render_widget(paragraph, area);
        }
    }

    fn get_language_from_extension(&self, ext: &str) -> Option<&'static str> {
        let lang = match ext {
            "rs" => "Rust",
            "toml" => "TOML",
            "md" => "Markdown",
            "txt" => "Text",
            "json" => "JSON",
            "yaml" | "yml" => "YAML",
            "py" => "Python",
            "js" => "JavaScript",
            "ts" => "TypeScript",
            "tsx" => "TSX",
            "jsx" => "JSX",
            "go" => "Go",
            "c" | "h" => "C",
            "cpp" | "hpp" | "cc" | "cxx" => "C++",
            "java" => "Java",
            "rb" => "Ruby",
            "php" => "PHP",
            "sh" | "bash" => "Shell",
            "sql" => "SQL",
            "html" => "HTML",
            "css" | "scss" | "sass" => "CSS",
            "xml" => "XML",
            "svg" => "SVG",
            _ => return None,
        };
        Some(lang)
    }

    fn get_line_count(&self, path: &std::path::Path) -> usize {
        if let Ok(text) = std::fs::read_to_string(path) { text.lines().take(6).count() } else { 0 }
    }

    fn get_total_lines(&self, path: &std::path::Path) -> usize {
        if let Ok(text) = std::fs::read_to_string(path) { text.lines().count() } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn create_test_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap().write_all(content.as_bytes()).unwrap();
    }

    fn create_test_state_with_finder(temp_dir: &TempDir) -> AppState {
        let mut state = AppState::new(
            temp_dir.path().to_path_buf(),
            "test".to_string(),
            thunderus_core::ProviderConfig::Glm {
                api_key: "test".to_string(),
                model: "glm-4.7".to_string(),
                base_url: "https://api.example.com".to_string(),
            },
            thunderus_core::ApprovalMode::Auto,
            thunderus_core::SandboxMode::Policy,
        );

        state.enter_fuzzy_finder(String::new(), 0);

        state
    }

    #[test]
    fn test_fuzzy_finder_component_new() {
        let state = AppState::default();
        let component = FuzzyFinderComponent::new(&state);

        assert_eq!(component.state.config.profile, "default");
    }

    #[test]
    fn test_get_language_from_extension() {
        let temp = TempDir::new().unwrap();
        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        assert_eq!(component.get_language_from_extension("rs"), Some("Rust"));
        assert_eq!(component.get_language_from_extension("py"), Some("Python"));
        assert_eq!(component.get_language_from_extension("js"), Some("JavaScript"));
        assert_eq!(component.get_language_from_extension("md"), Some("Markdown"));
        assert_eq!(component.get_language_from_extension("unknown"), None);
    }

    #[test]
    fn test_get_line_count() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        create_test_file(&test_file, "line1\nline2\nline3\nline4\nline5\nline6\nline7");

        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        let count = component.get_line_count(&test_file);
        assert_eq!(count, 6);
    }

    #[test]
    fn test_get_total_lines() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test.txt");
        create_test_file(&test_file, "line1\nline2\nline3");

        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        let total = component.get_total_lines(&test_file);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_get_total_lines_empty() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("empty.txt");
        create_test_file(&test_file, "");

        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        let total = component.get_total_lines(&test_file);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_get_language_from_extension_case_insensitive() {
        let temp = TempDir::new().unwrap();
        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        assert_eq!(component.get_language_from_extension("rs"), Some("Rust"));
        assert_eq!(component.get_language_from_extension("js"), Some("JavaScript"));

        assert_eq!(component.get_language_from_extension("RS"), None);
        assert_eq!(component.get_language_from_extension("JS"), None);
    }

    #[test]
    fn test_get_line_count_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        let state = create_test_state_with_finder(&temp);
        let component = FuzzyFinderComponent::new(&state);

        let count = component.get_line_count(PathBuf::from("/nonexistent/file.txt").as_path());
        assert_eq!(count, 0);
    }
}
