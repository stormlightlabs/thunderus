//! Tool execution visualization components
//!
//! Provides:
//! - Diff rendering with red/green lines
//! - Task progress list with status indicators
//! - Bash output display

use super::colors;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap},
};

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

/// Draw a task progress list
pub fn draw_task_progress(frame: &mut Frame, area: Rect, tasks: &[TaskItem], title: &str) {
    let block = Block::default()
        .title(title)
        .style(Style::default().bg(colors::BG_SECONDARY).fg(colors::TEXT_SECONDARY));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);

    let mut constraints: Vec<Constraint> = tasks.iter().map(|_| Constraint::Length(1)).collect();
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (idx, task) in tasks.iter().enumerate() {
        if idx < layout.len() {
            draw_task_row(frame, layout[idx], task);
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

/// Draw a diff with red/green highlighting
pub fn draw_diff(frame: &mut Frame, area: Rect, diff_lines: &[DiffLine]) {
    let block = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);

    let mut constraints: Vec<Constraint> = diff_lines.iter().map(|_| Constraint::Length(1)).collect();
    constraints.push(Constraint::Min(0));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (idx, line) in diff_lines.iter().enumerate() {
        if idx < layout.len() {
            draw_diff_line(frame, layout[idx], line);
        }
    }
}

/// A line in a diff display
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_type: DiffLineType,
    pub line_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineType {
    Context,
    Added,
    Removed,
    Header,
}

fn draw_diff_line(frame: &mut Frame, area: Rect, line: &DiffLine) {
    let (prefix, style, bg) = match line.line_type {
        DiffLineType::Context => (" ", Style::default().fg(colors::TEXT_SECONDARY), colors::BG_TERMINAL),
        DiffLineType::Added => ("+", Style::default().fg(colors::ACCENT_GREEN), colors::BG_TERMINAL),
        DiffLineType::Removed => ("-", Style::default().fg(colors::ACCENT_RED), colors::BG_TERMINAL),
        DiffLineType::Header => (
            "@",
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
            colors::BG_TERMINAL,
        ),
    };

    let line_num = line
        .line_number
        .map(|n| format!("{:4} ", n))
        .unwrap_or_else(|| "     ".to_string());

    let text = Text::from(vec![Line::from(vec![
        Span::styled(line_num, Style::default().fg(colors::TEXT_MUTED)),
        Span::styled(prefix, style),
        Span::styled(" ", Style::default()),
        Span::styled(&line.content, style),
    ])]);

    let paragraph = Paragraph::new(text)
        .style(Style::default().bg(bg))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// Parse a unified diff string into displayable lines
pub fn parse_diff(diff_text: &str) -> Vec<DiffLine> {
    diff_text
        .lines()
        .map(|line| {
            let mut chars = line.chars();
            let (content, line_type) = match chars.next() {
                Some('@') if line.starts_with("@@") => (line.to_string(), DiffLineType::Header),
                Some('+') => (chars.as_str().to_string(), DiffLineType::Added),
                Some('-') => (chars.as_str().to_string(), DiffLineType::Removed),
                Some(' ') => (chars.as_str().to_string(), DiffLineType::Context),
                _ => (line.to_string(), DiffLineType::Context),
            };
            DiffLine { content, line_type, line_number: None }
        })
        .collect()
}

/// Draw bash command output
pub fn draw_bash_output(frame: &mut Frame, area: Rect, command: &str, output: &str, exit_code: i32) {
    let block = Block::default().style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let cmd_line = Line::from(vec![
        Span::styled("$ ", Style::default().fg(colors::ACCENT_CYAN)),
        Span::styled(command, Style::default().fg(colors::TEXT_PRIMARY)),
    ]);
    let cmd_para = Paragraph::new(cmd_line).style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(cmd_para, layout[0]);

    let output_style = if exit_code == 0 {
        Style::default().fg(colors::TEXT_SECONDARY)
    } else {
        Style::default().fg(colors::ACCENT_RED)
    };

    let output_text = Text::from(output).style(output_style);
    let output_para = Paragraph::new(output_text)
        .style(Style::default().bg(colors::BG_SECONDARY))
        .wrap(Wrap { trim: false });
    frame.render_widget(output_para, layout[1]);
}

/// Draw a collapsible section
pub fn draw_collapsible(frame: &mut Frame, area: Rect, title: &str, expanded: bool, content: Text<'_>) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff() {
        let diff = "@@ -1,3 +1,4 @@\n context\n-removed\n+added\n context";
        let lines = parse_diff(diff);

        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].line_type, DiffLineType::Header);
        assert_eq!(lines[1].line_type, DiffLineType::Context);
        assert_eq!(lines[2].line_type, DiffLineType::Removed);
        assert_eq!(lines[3].line_type, DiffLineType::Added);
        assert_eq!(lines[4].line_type, DiffLineType::Context);
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
}
