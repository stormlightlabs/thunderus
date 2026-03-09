//! Tool execution visualization components
//!
//! Provides:
//! - Diff rendering with red/green lines
//! - Task progress list with status indicators
//! - Bash output display

use super::colors;
use super::layout::{ConstraintSpec, split as split_rects};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};
use thndrs_ui_macros::AreaSpec;

const MAX_BASH_VISIBLE_LINES: usize = 50;

/// A task in a progress list
#[derive(Debug, Clone)]
pub struct TaskItem {
    pub label: String,
    pub state: TaskState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    Pending,
    Running,
    Done,
}

impl TaskItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), state: TaskState::Pending }
    }

    pub fn running(mut self) -> Self {
        self.state = TaskState::Running;
        self
    }

    pub fn done(mut self) -> Self {
        self.state = TaskState::Done;
        self
    }
}

#[derive(AreaSpec)]
pub struct TaskProgressList;

impl TaskProgressList {
    pub fn render(self, frame: &mut Frame, area: Rect, tasks: &[TaskItem], title: &str) {
        let block = Block::default()
            .title(title)
            .style(Style::default().bg(colors::BG_SECONDARY).fg(colors::TEXT_SECONDARY));
        frame.render_widget(block.clone(), area);

        let inner = block.inner(area);
        let layout = split_rects(inner, Direction::Vertical, list_row_constraints(tasks.len()));

        for (idx, task) in tasks.iter().enumerate() {
            if idx < layout.len() {
                draw_task_row(frame, layout[idx], task);
            }
        }
    }
}

fn draw_task_row(frame: &mut Frame, area: Rect, task: &TaskItem) {
    let (indicator, style) = match task.state {
        TaskState::Pending => ("○", Style::default().fg(colors::TEXT_MUTED)),
        TaskState::Running => ("◐", Style::default().fg(colors::ACCENT_CYAN)),
        TaskState::Done => ("✓", Style::default().fg(colors::ACCENT_GREEN)),
    };

    let line = Line::from(vec![
        Span::styled(format!(" {} ", indicator), style),
        Span::styled(
            &task.label,
            if task.state == TaskState::Pending {
                Style::default().fg(colors::TEXT_MUTED)
            } else {
                Style::default().fg(colors::TEXT_SECONDARY)
            },
        ),
    ]);

    let paragraph = Paragraph::new(line).style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(paragraph, area);
}

#[derive(AreaSpec)]
pub struct DiffView;

impl DiffView {
    pub fn render(self, frame: &mut Frame, area: Rect, diff_lines: &[DiffLine]) {
        let block = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
        frame.render_widget(block.clone(), area);

        let inner = block.inner(area);
        let layout = split_rects(inner, Direction::Vertical, list_row_constraints(diff_lines.len()));

        for (idx, line) in diff_lines.iter().enumerate() {
            if idx < layout.len() {
                line.draw(frame, layout[idx])
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    Header,
}

/// A line in a diff display
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_kind: DiffLineKind,
    pub line_number: Option<u32>,
}

impl DiffLine {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let (prefix, style, bg) = match self.line_kind {
            DiffLineKind::Context => (" ", Style::default().fg(colors::TEXT_SECONDARY), colors::BG_TERMINAL),
            DiffLineKind::Added => ("+", Style::default().fg(colors::ACCENT_GREEN), colors::BG_TERMINAL),
            DiffLineKind::Removed => ("-", Style::default().fg(colors::ACCENT_RED), colors::BG_TERMINAL),
            DiffLineKind::Header => (
                "@",
                Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
                colors::BG_TERMINAL,
            ),
        };

        let line_num = self
            .line_number
            .map(|n| format!("{:4} ", n))
            .unwrap_or_else(|| "     ".to_string());

        let text = Text::from(vec![Line::from(vec![
            Span::styled(line_num, Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(prefix, style),
            Span::styled(" ", Style::default()),
            Span::styled(&self.content, style),
        ])]);

        let paragraph = Paragraph::new(text)
            .style(Style::default().bg(bg))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }
}

/// Parse a unified diff string into displayable lines
pub fn parse_diff(diff_text: &str) -> Vec<DiffLine> {
    diff_text
        .lines()
        .map(|line| {
            let mut chars = line.chars();
            let (content, line_type) = match chars.next() {
                Some('@') if line.starts_with("@@") => (line.to_string(), DiffLineKind::Header),
                Some('+') => (chars.as_str().to_string(), DiffLineKind::Added),
                Some('-') => (chars.as_str().to_string(), DiffLineKind::Removed),
                Some(' ') => (chars.as_str().to_string(), DiffLineKind::Context),
                _ => (line.to_string(), DiffLineKind::Context),
            };
            DiffLine { content, line_kind: line_type, line_number: None }
        })
        .collect()
}

#[derive(AreaSpec)]
pub struct BashOutputView;

impl ConstraintSpec for BashOutputView {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![Constraint::Length(1), Constraint::Min(0)]
    }
}

impl BashOutputView {
    pub fn render(self, frame: &mut Frame, area: Rect, command: &str, output: &str, exit_code: i32) {
        let block = Block::default().style(Style::default().bg(colors::BG_SECONDARY));
        frame.render_widget(block.clone(), area);

        let inner = block.inner(area);
        let layout = self.split(inner);
        if layout.len() < 2 {
            return;
        }

        let exit_style = if exit_code == 0 {
            Style::default().fg(colors::ACCENT_GREEN)
        } else {
            Style::default().fg(colors::ACCENT_RED)
        };

        let cmd_line = Line::from(vec![
            Span::styled("$ ", Style::default().fg(colors::ACCENT_CYAN)),
            Span::styled(command, Style::default().fg(colors::TEXT_PRIMARY)),
            Span::styled(" ", Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(format!("[exit {exit_code}]"), exit_style),
        ]);
        let cmd_para = Paragraph::new(cmd_line).style(Style::default().bg(colors::BG_SECONDARY));
        frame.render_widget(cmd_para, layout[0]);

        let output_style = if exit_code == 0 {
            Style::default().fg(colors::TEXT_SECONDARY)
        } else {
            Style::default().fg(colors::ACCENT_RED)
        };

        let output = truncate_bash_output(output, MAX_BASH_VISIBLE_LINES);
        let output_text = Text::from(output).style(output_style);
        let output_para = Paragraph::new(output_text)
            .style(Style::default().bg(colors::BG_SECONDARY))
            .wrap(Wrap { trim: false });
        frame.render_widget(output_para, layout[1]);
    }
}

fn truncate_bash_output(output: &str, max_visible_lines: usize) -> String {
    if max_visible_lines == 0 {
        return String::new();
    }

    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_visible_lines {
        return output.to_string();
    }

    let hidden = lines.len() - max_visible_lines;
    let mut visible = lines[..max_visible_lines].join("\n");
    if !visible.is_empty() {
        visible.push('\n');
    }
    visible.push_str(&format!("[{hidden} lines hidden]"));
    visible
}

#[derive(AreaSpec)]
pub struct CollapsibleSection;

impl CollapsibleSection {
    pub fn render(self, frame: &mut Frame, area: Rect, title: &str, expanded: bool, content: Text<'_>) -> Rect {
        let indicator = if expanded { "▼" } else { "▶" };

        let block = Block::default()
            .title(format!("{} {}", indicator, title))
            .style(Style::default().bg(colors::BG_SECONDARY).fg(colors::TEXT_SECONDARY));
        frame.render_widget(block.clone(), area);

        if expanded {
            let inner = block.inner(area);
            let content_para = Paragraph::new(content)
                .style(Style::default().fg(colors::TEXT_SECONDARY))
                .wrap(Wrap { trim: false });
            frame.render_widget(content_para, inner);
            inner
        } else {
            area
        }
    }
}

fn list_row_constraints(row_count: usize) -> Vec<Constraint> {
    let mut constraints = vec![Constraint::Length(1); row_count];
    constraints.push(Constraint::Min(0));
    constraints
}

/// Backward-compatible wrappers.
pub fn draw_task_progress(frame: &mut Frame, area: Rect, tasks: &[TaskItem], title: &str) {
    TaskProgressList.render(frame, area, tasks, title);
}

pub fn draw_diff(frame: &mut Frame, area: Rect, diff_lines: &[DiffLine]) {
    DiffView.render(frame, area, diff_lines);
}

pub fn draw_bash_output(frame: &mut Frame, area: Rect, command: &str, output: &str, exit_code: i32) {
    BashOutputView.render(frame, area, command, output, exit_code);
}

pub fn draw_collapsible(frame: &mut Frame, area: Rect, title: &str, expanded: bool, content: Text<'_>) -> Rect {
    CollapsibleSection.render(frame, area, title, expanded, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff() {
        let diff = "@@ -1,3 +1,4 @@\n context\n-removed\n+added\n context";
        let lines = parse_diff(diff);

        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].line_kind, DiffLineKind::Header);
        assert_eq!(lines[1].line_kind, DiffLineKind::Context);
        assert_eq!(lines[2].line_kind, DiffLineKind::Removed);
        assert_eq!(lines[3].line_kind, DiffLineKind::Added);
        assert_eq!(lines[4].line_kind, DiffLineKind::Context);
    }

    #[test]
    fn test_task_item_states() {
        let pending = TaskItem::new("Pending task");
        assert_eq!(pending.state, TaskState::Pending);

        let running = TaskItem::new("Running task").running();
        assert_eq!(running.state, TaskState::Running);

        let done = TaskItem::new("Done task").done();
        assert_eq!(done.state, TaskState::Done);
    }

    #[test]
    fn test_list_row_constraints_has_spacer() {
        let constraints = list_row_constraints(2);
        assert_eq!(constraints.len(), 3);
        assert_eq!(constraints[2], Constraint::Min(0));
    }

    #[test]
    fn test_truncate_bash_output_adds_hidden_line_indicator() {
        let output = (0..55).map(|idx| format!("line {idx}")).collect::<Vec<_>>().join("\n");
        let truncated = truncate_bash_output(&output, 50);

        assert!(truncated.contains("[5 lines hidden]"));
        assert!(truncated.contains("line 0"));
        assert!(!truncated.contains("line 54"));
    }
}
