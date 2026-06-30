use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{Entry, ToolStatus};
use crate::ui::MAX_TOOL_OUTPUT_LINES;
use crate::ui::style::{self, P};

const ROLE_WIDTH: usize = 27;

pub fn entry_lines(entry: &Entry, tick: u64, user_label: &str) -> Vec<Line<'static>> {
    match entry {
        Entry::User { text } => vec![message_line(user_label, P.blue, text)],
        Entry::Assistant { text, streaming } => {
            let label = if *streaming { style::spinner_frame(tick) } else { "Assistant" };
            vec![message_line(label, P.green, text)]
        }
        Entry::Reasoning { text, streaming } => {
            let label = if *streaming { style::spinner_frame(tick) } else { "thought" };
            vec![Line::from(vec![
                chip(label, P.mauve, P.surface0),
                Span::styled(" │ ", Style::default().fg(P.overlay0).bg(P.panel_bg)),
                Span::styled(text.clone(), style::subtle_style().add_modifier(Modifier::ITALIC)),
            ])]
        }
        Entry::Tool { name, arguments, status, output } => tool_lines(name, arguments, *status, output, tick),
        Entry::Status { text } => vec![message_line("Status", P.overlay1, text)],
        Entry::Error { text } => vec![Line::from(vec![
            role_label("Error", P.red),
            Span::styled("⚠ ", Style::default().fg(P.red).bg(P.panel_bg)),
            Span::styled(text.clone(), Style::default().fg(P.red).bg(P.panel_bg)),
        ])],
    }
}

fn message_line(label: &str, fg: Color, text: &str) -> Line<'static> {
    Line::from(vec![
        role_label(label, fg),
        Span::styled(text.to_string(), style::text_style()),
    ])
}

fn role_label(label: &str, fg: Color) -> Span<'static> {
    Span::styled(
        format!("{label:<ROLE_WIDTH$}"),
        Style::default().fg(fg).bg(P.panel_bg).add_modifier(Modifier::BOLD),
    )
}

fn tool_lines(name: &str, arguments: &str, status: ToolStatus, output: &[String], tick: u64) -> Vec<Line<'static>> {
    let (status_label, status_color, icon) = match status {
        ToolStatus::Running => ("running", P.yellow, style::spinner_frame(tick)),
        ToolStatus::Ok => ("ok", P.green, "✓"),
        ToolStatus::Failed => ("failed", P.red, "✕"),
    };
    let args_summary = summarize_tool_args(arguments);
    let mut spans = vec![
        chip("tool", P.peach, P.surface0),
        Span::styled(" ", style::text_style()),
        Span::styled(icon.to_string(), Style::default().fg(status_color).bg(P.panel_bg)),
        Span::styled(" ", style::text_style()),
        Span::styled(name.to_string(), style::text_style().add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" [{status_label}]"),
            Style::default().fg(status_color).bg(P.panel_bg),
        ),
    ];

    if !args_summary.is_empty() {
        spans.push(Span::styled(format!("  {args_summary}"), style::muted_style()));
    }

    let base_name = name.split('#').next().unwrap_or(name);
    let lang = tool_output_language(base_name, arguments);

    let gutter = "      │ ";
    let gutter_style = Style::default().fg(P.overlay0).bg(P.panel_bg);

    let mut lines = vec![Line::from(spans)];

    match lang {
        Some(lang_str) => {
            let joined: String = output
                .iter()
                .take(MAX_TOOL_OUTPUT_LINES)
                .map(|l| format!("{l}\n"))
                .collect();
            let highlighted = crate::ui::highlight_lines(&joined, Some(lang_str));
            for hl in highlighted {
                let mut line_spans = vec![Span::styled(gutter, gutter_style)];
                line_spans.extend(hl.spans);
                lines.push(Line::from(line_spans));
            }
        }
        None => {
            for line in output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                lines.push(Line::from(vec![
                    Span::styled(gutter, gutter_style),
                    Span::styled(line.clone(), style::subtle_style()),
                ]));
            }
        }
    };

    if output.len() > MAX_TOOL_OUTPUT_LINES {
        lines.push(Line::from(vec![Span::styled(
            format!("      │ …({} more lines)", output.len() - MAX_TOOL_OUTPUT_LINES),
            style::muted_style(),
        )]));
    }

    lines
}

/// Determine the syntax highlighting language for a tool's output.
///
/// Returns `Some(lang)` for code-oriented tools, `None` for plain text.
///
/// - `read_file_range` / `create_file` / `replace_range` / `write_patch`:
///   detect from the file path extension in arguments.
/// - `run_shell`: highlight as shell script when output looks like compiler
///   output or command output. For simplicity, use `bash` for shell output.
/// - `search_text` / `find_files` / `web_search` / `read_url` / others:
///   no highlighting (plain text results).
fn tool_output_language(tool_name: &str, arguments: &str) -> Option<&'static str> {
    match tool_name {
        "read_file_range" | "create_file" | "replace_range" | "write_patch" => {
            let v: serde_json::Value = serde_json::from_str(arguments).unwrap_or(serde_json::Value::Null);
            let path = v.get("path").and_then(|p| p.as_str())?;
            path_extension_language(path)
        }
        "run_shell" => Some("bash"),
        _ => None,
    }
}

/// Map a file path extension to a syntect language token.
///
/// TODO: this could be more exhaustive
fn path_extension_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rs"),
        "py" => Some("py"),
        "js" | "jsx" => Some("js"),
        "ts" | "tsx" => Some("ts"),
        "json" => Some("json"),
        "toml" => Some("toml"),
        "yaml" | "yml" => Some("yaml"),
        "sh" | "bash" => Some("bash"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "hpp" | "cc" => Some("cpp"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "md" => Some("md"),
        "sql" => Some("sql"),
        _ => None,
    }
}

fn chip(label: &str, fg: Color, bg: Color) -> Span<'static> {
    style::label_chip(label, fg, bg)
}

/// Produce a short, single-line summary of a tool's arguments for the
/// transcript line. Returns an empty string when there is nothing useful to
/// show.
fn summarize_tool_args(arguments: &str) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return String::new();
    }

    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return truncate_chars(trimmed, 48),
    };

    let Some(obj) = v.as_object() else {
        return truncate_chars(trimmed, 48);
    };

    for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
        if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
            return format!("{key}: {}", truncate_chars(val, 40));
        }
    }

    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            return format!("{k}: {}", truncate_chars(s, 40));
        }
    }

    truncate_chars(trimmed, 48)
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}...")
}
