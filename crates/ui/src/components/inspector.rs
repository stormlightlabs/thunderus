use crate::state::AppState;
use crate::syntax::SyntaxHighlighter;
use crate::theme::Theme;

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use thunderus_core::Event;

/// Inspector component for viewing provenance and trajectory
pub struct Inspector<'a> {
    state: &'a AppState,
}

impl<'a> Inspector<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn render(&self, frame: &mut Frame, list_area: Rect, detail_area: Rect) {
        let theme = Theme::palette(self.state.theme_variant());
        let evidence = &self.state.evidence;
        let items: Vec<ListItem> = evidence
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let is_selected = i == evidence.selected_index;
                let bg = if is_selected { theme.active } else { theme.panel_bg };
                let fg = if is_selected { theme.blue } else { theme.fg };

                let content = format!("[{}] {}", node.impact, self.summarize_event(&node.event.event));
                ListItem::new(content).style(Style::default().fg(fg).bg(bg))
            })
            .collect();

        let list_block = Block::default()
            .borders(Borders::ALL)
            .title(" Chain of Evidence ")
            .border_style(Style::default().fg(theme.blue));

        let list = List::new(items)
            .block(list_block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(list, list_area);

        let detail_block = Block::default()
            .borders(Borders::ALL)
            .title(" Evidence Detail ")
            .border_style(Style::default().fg(theme.blue));

        if let Some(selected_node) = evidence.selected_node() {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Impact: ", Style::default().fg(theme.muted)),
                    Span::styled(&selected_node.impact, Style::default().fg(theme.blue).bold()),
                ]),
                Line::from(vec![
                    Span::styled("Sequence: ", Style::default().fg(theme.muted)),
                    Span::styled(selected_node.event.seq.to_string(), Style::default().fg(theme.fg)),
                ]),
                Line::from(vec![
                    Span::styled("Timestamp: ", Style::default().fg(theme.muted)),
                    Span::styled(&selected_node.event.timestamp, Style::default().fg(theme.fg)),
                ]),
                Line::from(""),
                Line::from(Span::styled("Content:", Style::default().fg(theme.muted))),
            ];

            let highlighter = SyntaxHighlighter::new();
            let event_lines = self.highlight_event_details(&selected_node.event.event, &highlighter);
            lines.extend(event_lines);

            let paragraph = Paragraph::new(Text::from(lines))
                .block(detail_block)
                .wrap(Wrap { trim: false })
                .scroll((evidence.detail_scroll, 0));
            frame.render_widget(paragraph, detail_area);
        } else {
            frame.render_widget(
                Paragraph::new(if evidence.nodes.is_empty() {
                    "No evidence data available for this document."
                } else {
                    "Select an event to see details."
                })
                .block(detail_block)
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted)),
                detail_area,
            );
        }
    }

    fn summarize_event(&self, event: &Event) -> String {
        match event {
            Event::UserMessage { content } => {
                if content.len() > 30 { format!("{}...", &content[..27]) } else { content.clone() }.replace("\n", " ")
            }
            Event::ToolCall { tool, .. } => format!("Called {}", tool),
            Event::ToolResult { tool, success, .. } => {
                format!("{} {}", tool, if *success { "succeeded" } else { "failed" })
            }
            Event::Patch { name, .. } => format!("Patch: {}", name),
            _ => "Event".to_string(),
        }
    }

    fn highlight_event_details(&self, event: &Event, highlighter: &SyntaxHighlighter) -> Vec<Line<'static>> {
        match event {
            Event::UserMessage { content } => vec![Line::from(highlighter.highlight_code(content, "markdown"))],
            Event::ToolCall { tool, arguments } => vec![
                Line::from(format!("Tool: {}", tool)),
                Line::from("Arguments:"),
                Line::from(
                    highlighter.highlight_code(&serde_json::to_string_pretty(arguments).unwrap_or_default(), "json"),
                ),
            ],
            Event::ToolResult { tool, result, .. } => vec![
                Line::from(format!("Tool: {}", tool)),
                Line::from("Result:"),
                Line::from(
                    highlighter.highlight_code(&serde_json::to_string_pretty(result).unwrap_or_default(), "json"),
                ),
            ],
            Event::Patch { name, diff, .. } => vec![
                Line::from(format!("Patch: {}", name)),
                Line::from("Diff:"),
                Line::from(highlighter.highlight_code(diff, "diff")),
            ],
            _ => vec![Line::from(format!("{:?}", event))],
        }
    }

    /// Get list of affected files for the currently selected event
    pub fn affected_files(&self) -> Vec<String> {
        self.state
            .evidence
            .selected_node()
            .map(|node| match &node.event.event {
                Event::Patch { files, .. } => files.clone(),
                Event::FileRead { file_path, .. } => vec![file_path.clone()],
                _ => Vec::new(),
            })
            .unwrap_or_default()
    }
}
