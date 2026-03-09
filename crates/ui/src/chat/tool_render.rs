use super::formatters;
use super::{ToolCallDisplay, ToolCallStatus};
use crate::colors;
use crate::components::{ToolCallCard, ToolCallState, wrapped_line_count};
use crate::layout::split as split_rects;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};

pub const BASH_MAX_VISIBLE_LINES: usize = 50;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
    Header,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub content: String,
    pub line_kind: DiffLineKind,
    pub line_number: Option<u32>,
}

impl DiffLine {
    fn to_line(&self) -> Line<'static> {
        let (prefix, style) = match self.line_kind {
            DiffLineKind::Context => (" ", Style::default().fg(colors::TEXT_SECONDARY)),
            DiffLineKind::Added => ("+", Style::default().fg(colors::ACCENT_GREEN)),
            DiffLineKind::Removed => ("-", Style::default().fg(colors::ACCENT_RED)),
            DiffLineKind::Header => (
                "@",
                Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
            ),
        };

        let line_num = self
            .line_number
            .map(|number| format!("{:4} ", number))
            .unwrap_or_else(|| "     ".to_string());

        Line::from(vec![
            Span::styled(line_num, Style::default().fg(colors::TEXT_MUTED)),
            Span::styled(prefix, style),
            Span::styled(" ", Style::default()),
            Span::styled(self.content.clone(), style),
        ])
    }
}

pub fn draw_tool_call_widget(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let state = tool_call.to_ui_state();
    let formatted_args = formatters::tool_args(&tool_call.name, &tool_call.arguments, area.width);
    let summary = Text::from(formatted_args.clone());

    if !tool_call.expanded {
        ToolCallCard.render(frame, area, &tool_call.name, &formatted_args, state, summary);
        return;
    }

    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(ToolCallCard::collapsed_height()), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        ToolCallCard.render(frame, area, &tool_call.name, &formatted_args, state, summary);
        return;
    }

    ToolCallCard.render(frame, layout[0], &tool_call.name, &formatted_args, state, summary);
    draw_expanded_tool_details(frame, layout[1], tool_call);
}

fn draw_expanded_tool_details(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        draw_task_progress(frame, area, &tool_call.progress_tasks(), "Tool progress");
        return;
    }

    match tool_call.name.as_str() {
        "read" => {
            draw_read_tool_output(frame, area, tool_call);
            return;
        }
        "write" => {
            draw_write_tool_output(frame, area, tool_call);
            return;
        }
        "edit" => {
            if let Some(diff_text) = formatters::edit_diff(&tool_call.arguments) {
                let diff_lines = parse_diff(&diff_text);
                if !diff_lines.is_empty() {
                    let path = formatters::diff_path(&diff_text)
                        .or_else(|| formatters::path_arg(&tool_call.arguments))
                        .unwrap_or_else(|| "edited file".to_string());
                    let layout = split_rects(
                        area,
                        Direction::Vertical,
                        vec![Constraint::Length(1), Constraint::Min(0)],
                    );
                    if layout.len() == 2 {
                        frame.render_widget(
                            Paragraph::new(Line::from(vec![Span::styled(
                                path,
                                Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
                            )]))
                            .style(Style::default().bg(colors::BG_TERMINAL)),
                            layout[0],
                        );
                        draw_diff(frame, layout[1], &diff_lines);
                        return;
                    }
                }
            }
        }
        "bash" => {
            let command = formatters::bash_cmd(&tool_call.arguments).unwrap_or_else(|| "bash".to_string());
            let (output, exit_code) = parse_bash_output(
                tool_call.output.as_deref().unwrap_or_default(),
                tool_call.status == ToolCallStatus::Error,
            );
            let normalized = formatters::tool_output("bash", &output);
            draw_bash_output(frame, area, &command, &normalized, exit_code);
            return;
        }
        "research" => {
            draw_research_tool_output(frame, area, tool_call);
            return;
        }
        _ => {}
    }

    let normalized = formatters::tool_output(&tool_call.name, tool_call.output.as_deref().unwrap_or_default());
    draw_collapsible(frame, area, "Tool output", true, Text::from(normalized));
}

fn draw_read_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let path = formatters::path_arg(&tool_call.arguments).unwrap_or_else(|| "read".to_string());
    let output = formatters::tool_output("read", tool_call.output.as_deref().unwrap_or_default());
    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            path,
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
        )]))
        .style(Style::default().bg(colors::BG_TERMINAL)),
        layout[0],
    );

    let lines = if tool_call.status == ToolCallStatus::Error {
        vec![Line::from(vec![Span::styled(
            output,
            Style::default().fg(colors::ACCENT_RED),
        )])]
    } else {
        build_read_output_lines(&output)
    };
    draw_lines_with_truncation(frame, layout[1], lines, colors::TEXT_MUTED);
}

fn draw_write_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let output = formatters::tool_output("write", tool_call.output.as_deref().unwrap_or_default());
    let line_text = if tool_call.status == ToolCallStatus::Success {
        formatters::write_success_ln(&output).unwrap_or_else(|| output.lines().next().unwrap_or_default().to_string())
    } else {
        output.lines().next().unwrap_or_default().to_string()
    };
    let style = if tool_call.status == ToolCallStatus::Success {
        Style::default().fg(colors::ACCENT_GREEN)
    } else {
        Style::default().fg(colors::ACCENT_RED)
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(line_text, style)]))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_research_tool_output(frame: &mut Frame, area: Rect, tool_call: &ToolCallDisplay) {
    let url = formatters::research_url(&tool_call.arguments).unwrap_or_else(|| "research".to_string());
    let output = formatters::tool_output("research", tool_call.output.as_deref().unwrap_or_default());

    let layout = split_rects(
        area,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
    if layout.len() < 2 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            url,
            Style::default()
                .fg(colors::ACCENT_CYAN)
                .add_modifier(Modifier::UNDERLINED),
        )]))
        .style(Style::default().bg(colors::BG_TERMINAL)),
        layout[0],
    );

    let wrapped = wrap_text_lines(&output, layout[1].width);
    let lines = wrapped
        .into_iter()
        .map(|line| Line::from(vec![Span::styled(line, Style::default().fg(colors::TEXT_SECONDARY))]))
        .collect::<Vec<_>>();
    draw_lines_with_truncation(frame, layout[1], lines, colors::TEXT_MUTED);
}

pub fn build_read_output_lines(output: &str) -> Vec<Line<'static>> {
    if let Some(placeholder) = parse_read_image_placeholder(output) {
        return vec![Line::from(vec![Span::styled(
            placeholder,
            Style::default().fg(colors::TEXT_MUTED),
        )])];
    }

    let number_width = output
        .lines()
        .filter_map(|line| split_numbered_line(line).map(|(number, _)| number.len()))
        .max()
        .unwrap_or(1);

    let mut lines = Vec::new();
    for raw_line in output.lines() {
        if let Some((number, content)) = split_numbered_line(raw_line) {
            let number_span = Span::styled(
                format!("{number:>width$} ", width = number_width),
                Style::default().fg(colors::TEXT_MUTED),
            );

            if let Some(name) = content.strip_prefix("▶ ") {
                lines.push(Line::from(vec![
                    number_span,
                    Span::styled("▶ ", Style::default().fg(colors::ACCENT_YELLOW)),
                    Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ]));
            } else if let Some(name) = content.strip_prefix("⌸ ") {
                lines.push(Line::from(vec![
                    number_span,
                    Span::styled("⌸ ", Style::default().fg(colors::TEXT_SECONDARY)),
                    Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ]));
            } else {
                lines.push(Line::from(vec![
                    number_span,
                    Span::styled(content.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
                ]));
            }
            continue;
        }

        if let Some(name) = raw_line.strip_prefix("▶ ") {
            lines.push(Line::from(vec![
                Span::styled("▶ ", Style::default().fg(colors::ACCENT_YELLOW)),
                Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
            ]));
            continue;
        }

        if let Some(name) = raw_line.strip_prefix("⌸ ") {
            lines.push(Line::from(vec![
                Span::styled("⌸ ", Style::default().fg(colors::TEXT_SECONDARY)),
                Span::styled(name.to_string(), Style::default().fg(colors::TEXT_SECONDARY)),
            ]));
            continue;
        }

        if raw_line.is_empty() {
            lines.push(Line::from(vec![Span::raw("")]));
            continue;
        }

        lines.push(Line::from(vec![Span::styled(
            raw_line.to_string(),
            Style::default().fg(colors::TEXT_SECONDARY),
        )]));
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::raw("")]));
    }

    lines
}

fn draw_lines_with_truncation(
    frame: &mut Frame, area: Rect, mut lines: Vec<Line<'static>>, indicator_color: ratatui::style::Color,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if lines.is_empty() {
        lines.push(Line::from(vec![Span::raw("")]));
    }

    let max_lines = area.height as usize;
    if lines.len() > max_lines {
        let keep = max_lines.saturating_sub(1);
        let hidden = lines.len().saturating_sub(keep);
        lines.truncate(keep);
        lines.push(Line::from(vec![Span::styled(
            format!("[{hidden} lines hidden]"),
            Style::default().fg(indicator_color),
        )]));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(Style::default().bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

pub fn parse_bash_output(raw_output: &str, is_error: bool) -> (String, i32) {
    if !is_error {
        return (raw_output.to_string(), 0);
    }

    let Some(stripped) = raw_output.strip_prefix("Command exited with code ") else {
        return (raw_output.to_string(), 1);
    };

    let mut parts = stripped.splitn(2, '\n');
    let Some(code) = parts.next().unwrap_or_default().trim().parse::<i32>().ok() else {
        return (raw_output.to_string(), 1);
    };

    let output = parts.next().unwrap_or_default().to_string();
    (output, code)
}

pub fn bash_visible_line_count(output: &str, max_visible_lines: usize) -> usize {
    if max_visible_lines == 0 {
        return 1;
    }

    let lines = output.lines().count().max(1);
    if lines <= max_visible_lines { lines } else { max_visible_lines + 1 }
}

pub fn parse_diff(diff_text: &str) -> Vec<DiffLine> {
    let mut result = Vec::new();
    let mut new_line = 0u32;

    for line in diff_text.lines() {
        let mut chars = line.chars();
        match chars.next() {
            Some('@') if line.starts_with("@@") => {
                if let Some(parsed) = parse_new_hunk_line(line) {
                    new_line = parsed;
                }
                result.push(DiffLine { content: line.to_string(), line_kind: DiffLineKind::Header, line_number: None });
            }
            Some('+') if !line.starts_with("+++") => {
                let line_number = if new_line == 0 { None } else { Some(new_line) };
                result.push(DiffLine {
                    content: chars.as_str().to_string(),
                    line_kind: DiffLineKind::Added,
                    line_number,
                });
                if new_line > 0 {
                    new_line += 1;
                }
            }
            Some('-') if !line.starts_with("---") => {
                result.push(DiffLine {
                    content: chars.as_str().to_string(),
                    line_kind: DiffLineKind::Removed,
                    line_number: None,
                });
            }
            Some(' ') => {
                let line_number = if new_line == 0 { None } else { Some(new_line) };
                result.push(DiffLine {
                    content: chars.as_str().to_string(),
                    line_kind: DiffLineKind::Context,
                    line_number,
                });
                if new_line > 0 {
                    new_line += 1;
                }
            }
            _ => {
                result.push(DiffLine {
                    content: line.to_string(),
                    line_kind: DiffLineKind::Context,
                    line_number: None,
                });
            }
        }
    }

    result
}

pub fn draw_task_progress(frame: &mut Frame, area: Rect, tasks: &[TaskItem], title: &str) {
    let block = Block::default()
        .title(title)
        .style(Style::default().bg(colors::BG_TERMINAL).fg(colors::TEXT_SECONDARY));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let layout = split_rects(inner, Direction::Vertical, list_row_constraints(tasks.len()));

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

    let paragraph = Paragraph::new(line).style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(paragraph, area);
}

pub fn draw_diff(frame: &mut Frame, area: Rect, diff_lines: &[DiffLine]) {
    let lines = diff_lines.iter().map(DiffLine::to_line).collect();
    draw_lines_with_truncation(frame, area, lines, colors::TEXT_MUTED);
}

pub fn draw_bash_output(frame: &mut Frame, area: Rect, command: &str, output: &str, exit_code: i32) {
    let block = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let layout = split_rects(
        inner,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0)],
    );
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
    let cmd_para = Paragraph::new(cmd_line).style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(cmd_para, layout[0]);

    let output_style = if exit_code == 0 {
        Style::default().fg(colors::TEXT_SECONDARY)
    } else {
        Style::default().fg(colors::ACCENT_RED)
    };

    let output = truncate_bash_output(output, BASH_MAX_VISIBLE_LINES);
    let output_text = Text::from(output).style(output_style);
    let output_para = Paragraph::new(output_text)
        .style(Style::default().bg(colors::BG_TERMINAL))
        .wrap(Wrap { trim: false });
    frame.render_widget(output_para, layout[1]);
}

pub fn draw_collapsible(frame: &mut Frame, area: Rect, title: &str, expanded: bool, content: Text<'_>) -> Rect {
    let indicator = if expanded { "▼" } else { "▶" };

    let block = Block::default()
        .title(format!("{} {}", indicator, title))
        .style(Style::default().bg(colors::BG_TERMINAL).fg(colors::TEXT_SECONDARY));
    frame.render_widget(block.clone(), area);

    if expanded {
        let inner = block.inner(area);
        let content_para = Paragraph::new(content)
            .style(Style::default().fg(colors::TEXT_SECONDARY).bg(colors::BG_TERMINAL))
            .wrap(Wrap { trim: false });
        frame.render_widget(content_para, inner);
        inner
    } else {
        area
    }
}

fn list_row_constraints(row_count: usize) -> Vec<Constraint> {
    let mut constraints = vec![Constraint::Length(1); row_count];
    constraints.push(Constraint::Min(0));
    constraints
}

fn split_numbered_line(line: &str) -> Option<(&str, &str)> {
    let (number_part, content) = line.split_once('\t')?;
    let number = number_part.trim();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((number, content))
}

fn parse_read_image_placeholder(output: &str) -> Option<String> {
    let header = output.lines().next()?.trim();
    let body = header.strip_prefix("[Image: ")?.strip_suffix(']')?;

    let mut size = "unknown size".to_string();
    let mut mime = "unknown".to_string();
    for part in body.split(',').map(str::trim) {
        if part.ends_with("bytes") {
            size = part.to_string();
        } else if let Some(value) = part.strip_prefix("mime: ") {
            mime = value.to_string();
        }
    }

    Some(format!("[image: {mime}, {size}]"))
}

fn wrap_text_lines(content: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut wrapped = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in line.split_whitespace() {
            let word_len = word.chars().count();

            if current.is_empty() {
                if word_len <= width {
                    current.push_str(word);
                } else {
                    for chunk in word.chars().collect::<Vec<_>>().chunks(width) {
                        wrapped.push(chunk.iter().collect());
                    }
                }
                continue;
            }

            if current.chars().count() + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                wrapped.push(current);
                current = String::new();
                if word_len <= width {
                    current.push_str(word);
                } else {
                    for chunk in word.chars().collect::<Vec<_>>().chunks(width) {
                        wrapped.push(chunk.iter().collect());
                    }
                }
            }
        }

        if !current.is_empty() {
            wrapped.push(current);
        }
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
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

pub fn tool_call_expanded_height(tool_call: &ToolCallDisplay, width: u16) -> u16 {
    if matches!(tool_call.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
        return 5;
    }

    match tool_call.name.as_str() {
        "read" => {
            let output = formatters::tool_output("read", tool_call.output.as_deref().unwrap_or_default());
            let line_count = if tool_call.status == ToolCallStatus::Error {
                output.lines().count().max(1)
            } else {
                build_read_output_lines(&output).len().max(1)
            } as u16;
            line_count.clamp(1, 20) + 1
        }
        "write" => 1,
        "edit" => formatters::edit_diff(&tool_call.arguments)
            .map(|diff| parse_diff(&diff).len() as u16)
            .map(|line_count: u16| line_count.clamp(1, 15) + 1)
            .unwrap_or(3),
        "bash" => {
            let (output, _) = parse_bash_output(
                tool_call.output.as_deref().unwrap_or_default(),
                tool_call.status == ToolCallStatus::Error,
            );
            let output = formatters::tool_output("bash", &output);
            (bash_visible_line_count(&output, BASH_MAX_VISIBLE_LINES) as u16).clamp(1, 15) + 1
        }
        "research" => {
            let output = formatters::tool_output("research", tool_call.output.as_deref().unwrap_or_default());
            (wrapped_line_count(&output, width.saturating_sub(4)) as u16).clamp(1, 15) + 1
        }
        _ => {
            let output = formatters::tool_output(&tool_call.name, tool_call.output.as_deref().unwrap_or_default());
            (wrapped_line_count(&output, width.saturating_sub(4)) as u16).clamp(1, 10) + 2
        }
    }
}

pub fn tool_call_state(status: ToolCallStatus) -> ToolCallState {
    status.into()
}

fn parse_new_hunk_line(line: &str) -> Option<u32> {
    let plus_idx = line.find('+')?;
    let after_plus = &line[plus_idx + 1..];
    let digits = after_plus
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_read_image_placeholder() {
        let output = "[Image: 123 bytes, mime: image/png]\nbase64:AAAA";
        let placeholder = parse_read_image_placeholder(output).expect("placeholder should parse");
        assert_eq!(placeholder, "[image: image/png, 123 bytes]");
    }

    #[test]
    fn test_parse_bash_output_with_error_prefix() {
        let (output, code) = parse_bash_output("Command exited with code 2\nstderr text", true);
        assert_eq!(code, 2);
        assert_eq!(output, "stderr text");
    }

    #[test]
    fn test_parse_bash_output_success_keeps_literal_prefix_text() {
        let literal = "Command exited with code 2\nbut this is command output";
        let (output, code) = parse_bash_output(literal, false);
        assert_eq!(code, 0);
        assert_eq!(output, literal);
    }

    #[test]
    fn test_parse_bash_output_error_without_prefix_defaults_to_non_zero_exit() {
        let (output, code) = parse_bash_output("Command timed out after 120 seconds", true);
        assert_eq!(code, 1);
        assert_eq!(output, "Command timed out after 120 seconds");
    }

    #[test]
    fn test_parse_diff() {
        let diff = "@@ -1,3 +1,4 @@\n context\n-removed\n+added\n context";
        let lines = parse_diff(diff);

        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].line_kind, DiffLineKind::Header);
        assert_eq!(lines[0].line_number, None);
        assert_eq!(lines[1].line_kind, DiffLineKind::Context);
        assert_eq!(lines[1].line_number, Some(1));
        assert_eq!(lines[2].line_kind, DiffLineKind::Removed);
        assert_eq!(lines[2].line_number, None);
        assert_eq!(lines[3].line_kind, DiffLineKind::Added);
        assert_eq!(lines[3].line_number, Some(2));
        assert_eq!(lines[4].line_kind, DiffLineKind::Context);
        assert_eq!(lines[4].line_number, Some(3));
    }

    #[test]
    fn test_wrap_text_lines_wraps_on_word_boundaries() {
        let wrapped = wrap_text_lines("alpha beta gamma", 10);
        assert_eq!(wrapped, vec!["alpha beta".to_string(), "gamma".to_string()]);
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
