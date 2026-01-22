//! Memory hits panel component
//!
//! Displays memory search results with interactive navigation and pin controls.

use crate::{ThemeVariant, state::MemoryHitsState, theme::Theme};

use ratatui::style::Color;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem},
};
use thunderus_core::memory::MemoryKind;

/// Memory hits panel component
///
/// Displays search results from the memory store with:
/// - Title, kind, path, and snippet for each hit
/// - BM25 score and execution time
/// - Keyboard navigation (up/down, Enter to open, 'p' to pin)
pub struct MemoryHitsPanel<'a> {
    state: &'a MemoryHitsState,
}

impl<'a> MemoryHitsPanel<'a> {
    /// Create a new memory hits panel
    pub fn new(state: &'a MemoryHitsState) -> Self {
        Self { state }
    }

    /// Render the memory hits panel
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        if !self.state.is_visible() {
            return;
        }

        let theme = Theme::palette(
            std::env::var("THUNDERUS_THEME")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(ThemeVariant::Iceberg),
        );

        let title = format!(
            "Memory Hits ({}) - {:.0}ms",
            self.state.hits.len(),
            self.state.search_time_ms
        );

        let mut items = Vec::new();

        for (idx, hit) in self.state.hits.iter().enumerate() {
            let is_selected = idx == self.state.selected_index;
            let is_pinned = self.state.is_pinned(&hit.id);

            let kind_display = self.format_kind(hit.kind);
            let kind_style = self.kind_style(hit.kind);

            let pin_indicator = if is_pinned {
                Span::styled("📌 ", Style::default().fg(theme.yellow))
            } else {
                Span::styled("   ", Style::default())
            };

            let select_indicator = if is_selected {
                Span::styled("▸ ", Style::default().fg(theme.cyan).bold())
            } else {
                Span::styled("  ", Style::default())
            };

            let score_display = format!("{:.1}", hit.score);

            let title_line = Line::from(vec![
                pin_indicator,
                select_indicator,
                Span::styled(kind_display, kind_style),
                Span::styled(" ", Style::default()),
                Span::styled(&hit.title, Style::default().fg(theme.blue).bold()),
                Span::styled(format!(" [{}]", score_display), Style::default().fg(theme.muted)),
            ]);

            let path_line = Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(&hit.path, Style::default().fg(theme.muted).italic()),
            ]);

            let snippet_lines: Vec<Line> = textwrap::wrap(&hit.snippet, 60)
                .iter()
                .map(|line| {
                    Line::from(vec![
                        Span::styled("   ", Style::default()),
                        Span::styled(line.to_string(), Style::default().fg(theme.fg)),
                    ])
                })
                .collect();

            items.push(ListItem::new(Text::from(
                std::iter::once(title_line)
                    .chain(std::iter::once(path_line))
                    .chain(snippet_lines)
                    .chain(std::iter::once(Line::raw("")))
                    .collect::<Vec<_>>(),
            )));
        }

        let list = List::new(items)
            .block(
                Block::default()
                    .title(Line::from(vec![
                        Span::styled(title, Style::default().fg(theme.blue).bold()),
                        Span::styled(
                            " [↑↓]Nav [Enter]Open [p]Pin [Esc]Close",
                            Style::default().fg(theme.muted),
                        ),
                    ]))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .bg(theme.panel_bg),
            )
            .highlight_style(Style::default().bg(theme.highlight));

        frame.render_widget(list, area);
    }

    /// Format memory kind for display
    fn format_kind(&self, kind: MemoryKind) -> String {
        match kind {
            MemoryKind::Core => "CORE".to_string(),
            MemoryKind::Fact => "FACT".to_string(),
            MemoryKind::Adr => "ADR".to_string(),
            MemoryKind::Playbook => "PLAYBOOK".to_string(),
            MemoryKind::Recap => "RECAP".to_string(),
        }
    }

    /// Get color style for memory kind
    fn kind_style(&self, kind: MemoryKind) -> Style {
        let color = match kind {
            MemoryKind::Core => Color::Red,
            MemoryKind::Fact => Color::Green,
            MemoryKind::Adr => Color::Yellow,
            MemoryKind::Playbook => Color::Blue,
            MemoryKind::Recap => Color::Magenta,
        };

        Style::default().fg(color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderus_store::SearchHit;

    fn create_test_hit(id: &str, title: &str, kind: MemoryKind) -> SearchHit {
        SearchHit {
            id: id.to_string(),
            kind,
            title: title.to_string(),
            path: format!("memory/{}.md", id),
            anchor: None,
            snippet: format!("Test snippet for {}", title),
            score: -5.0,
            event_ids: vec![],
        }
    }

    fn create_test_state() -> MemoryHitsState {
        let mut state = MemoryHitsState::new();
        let hits = vec![
            create_test_hit("test-1", "Test Fact", MemoryKind::Fact),
            create_test_hit("test-2", "Test ADR", MemoryKind::Adr),
        ];
        state.set_hits(hits, "test query".to_string(), 15);
        state
    }

    #[test]
    fn test_memory_hits_panel_new() {
        let state = MemoryHitsState::new();
        let panel = MemoryHitsPanel::new(&state);
        assert_eq!(panel.state.hits.len(), 0);
    }

    #[test]
    fn test_memory_hits_panel_with_hits() {
        let state = create_test_state();
        let panel = MemoryHitsPanel::new(&state);
        assert_eq!(panel.state.hits.len(), 2);
    }
}
