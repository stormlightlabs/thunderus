use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{Entry, ToolStatus};
use crate::ui::MAX_TOOL_OUTPUT_LINES;
use crate::ui::style::{self, P};
use crate::utils::truncate_ellipsis;

const ROLE_WIDTH: usize = 16;

const GUTTER: &str = "   │ ";

/// Render entry lines with an optional max width for truncation.
///
/// When `max_width > 0`, text content is truncated with `…` to fit.
pub fn entry_lines_with_width(entry: &Entry, tick: u64, user_label: &str, max_width: usize) -> Vec<Line<'static>> {
    let avail = max_width.saturating_sub(ROLE_WIDTH);
    match entry {
        Entry::User { text } => vec![user_message_line(user_label, text, avail)],
        Entry::Assistant { text, streaming } => vec![message_line(
            if *streaming { style::spinner_frame(tick) } else { "Assistant" },
            P.green,
            text,
            avail,
        )],
        Entry::Reasoning { text, streaming } => reasoning_lines(text, *streaming, tick, avail),
        Entry::Tool { name, arguments, status, output } => tool_lines(name, arguments, *status, output, tick, avail),
        Entry::Status { text } => vec![message_line("Status", P.overlay1, text, avail)],
        Entry::Error { text } => error_lines(text, avail),
    }
}

fn message_line(label: &str, fg: Color, text: &str, avail: usize) -> Line<'static> {
    let display = if avail > 0 { truncate_ellipsis(text, avail) } else { text.to_string() };
    Line::from(vec![role_label(label, fg), Span::styled(display, style::text_style())])
}

/// Render a user prompt with a visually distinct bounded block.
///
/// User messages get a colored left border bar (▌) and the user label as a
/// chip, making them stand out from assistant/tool rows which use plain
/// left-aligned labels.
fn user_message_line(label: &str, text: &str, avail: usize) -> Line<'static> {
    let display = if avail > 0 { truncate_ellipsis(text, avail) } else { text.to_string() };
    let border = Span::styled(
        "▌",
        Style::default().fg(P.blue).bg(P.panel_bg).add_modifier(Modifier::BOLD),
    );
    let label_display = truncate_ellipsis(label, ROLE_WIDTH.saturating_sub(2));
    Line::from(vec![
        Span::styled(" ", style::text_style()),
        border,
        Span::styled(" ", style::text_style()),
        Span::styled(
            format!("{label_display:<ROLE_WIDTH$}"),
            Style::default().fg(P.blue).bg(P.panel_bg).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", style::text_style()),
        Span::styled(display, style::text_style()),
    ])
}

/// Render a reasoning block with a stable header/status line.
///
/// The header shows `Thinking` (with spinner) while streaming or `Thought`
/// (with ✓) when done. The body is italic and indented under the header,
/// matching the Gridland sibling-block pattern without nesting inside
/// assistant text.
fn reasoning_lines(text: &str, streaming: bool, tick: u64, avail: usize) -> Vec<Line<'static>> {
    let (header, icon) = if streaming { ("Thinking", style::spinner_frame(tick)) } else { ("Thought", "✓") };

    let body = if avail > 0 { truncate_ellipsis(text, avail) } else { text.to_string() };

    vec![Line::from(vec![
        role_label(header, P.mauve),
        Span::styled(format!("{icon} "), Style::default().fg(P.mauve).bg(P.panel_bg)),
        Span::styled(body, style::subtle_style().add_modifier(Modifier::ITALIC)),
    ])]
}

fn role_label(label: &str, fg: Color) -> Span<'static> {
    Span::styled(
        format!("{label:<ROLE_WIDTH$}"),
        Style::default().fg(fg).bg(P.panel_bg).add_modifier(Modifier::BOLD),
    )
}

/// Render an error entry with the `⚠` icon as part of the label and the
/// error text aligned in the message body.
///
/// The icon sits in the label column so the error text aligns under the
/// message body of other rows. Long text is truncated with `…`.
fn error_lines(text: &str, avail: usize) -> Vec<Line<'static>> {
    let err_style = Style::default().fg(P.red).bg(P.panel_bg);
    let display = if avail > 0 { truncate_ellipsis(text, avail) } else { text.to_string() };
    vec![Line::from(vec![
        role_label("⚠ Error", P.red),
        Span::styled(display, err_style),
    ])]
}

fn tool_lines(
    name: &str, args: &str, status: ToolStatus, output: &[String], tick: u64, avail: usize,
) -> Vec<Line<'static>> {
    let (status_label, status_color, icon) = match status {
        ToolStatus::Running => ("running", P.yellow, style::spinner_frame(tick)),
        ToolStatus::Ok => ("ok", P.green, "✓"),
        ToolStatus::Failed => ("failed", P.red, "✕"),
    };
    let args_summary = summarize_tool_args(args);

    let mut spans = vec![role_label("tool", P.peach)];
    spans.push(Span::styled(
        format!("{icon} "),
        Style::default().fg(status_color).bg(P.panel_bg),
    ));
    spans.push(Span::styled(
        name.to_string(),
        style::text_style().add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" [{status_label}]"),
        Style::default().fg(status_color).bg(P.panel_bg),
    ));

    if !args_summary.is_empty() {
        spans.push(Span::styled(format!("  {args_summary}"), style::muted_style()));
    }

    let base_name = name.split('#').next().unwrap_or(name);
    let lang = tool_output_language(base_name, args);

    let gutter_style = Style::default().fg(P.overlay0).bg(P.panel_bg);
    let gutter_len = GUTTER.chars().count();

    let mut lines = vec![Line::from(spans)];

    match lang {
        Some(lang_str) => {
            let joined: String = output
                .iter()
                .take(MAX_TOOL_OUTPUT_LINES)
                .map(|l| format!("{l}\n"))
                .collect();
            let display_path = format!("output.{lang_str}");
            let highlighted = super::highlight::highlight_code(&joined, Some(&display_path));
            for hl in highlighted {
                let mut line_spans = vec![Span::styled(GUTTER, gutter_style)];
                line_spans.extend(hl.spans);
                lines.push(Line::from(line_spans));
            }
        }
        None => {
            for line in output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                let display = if avail > gutter_len {
                    truncate_ellipsis(line, avail.saturating_sub(gutter_len))
                } else {
                    line.clone()
                };
                let content_style = if is_section_header(line) {
                    style::text_style().add_modifier(Modifier::BOLD).fg(P.overlay1)
                } else {
                    style::subtle_style()
                };
                lines.push(Line::from(vec![
                    Span::styled(GUTTER, gutter_style),
                    Span::styled(display, content_style),
                ]));
            }
        }
    };

    if output.len() > MAX_TOOL_OUTPUT_LINES {
        lines.push(Line::from(vec![Span::styled(
            format!("   │ …({} more lines)", output.len() - MAX_TOOL_OUTPUT_LINES),
            style::muted_style(),
        )]));
    }

    lines
}

/// Detect whether a tool output line is a section header.
///
/// Section headers are lines like `── stdout ──`, `── stderr ──`, or
/// command summaries starting with `$`. These are rendered with distinct
/// styling to separate them from content lines.
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("── ") || trimmed.starts_with("$ ")
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
        Err(_) => return truncate_ellipsis(trimmed, 48),
    };

    let Some(obj) = v.as_object() else {
        return truncate_ellipsis(trimmed, 48);
    };

    for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
        if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
            return format!("{key}: {}", truncate_ellipsis(val, 40));
        }
    }

    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            return format!("{k}: {}", truncate_ellipsis(s, 40));
        }
    }

    truncate_ellipsis(trimmed, 48)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_section_header_detects_stdout() {
        assert!(is_section_header("── stdout ──"));
    }

    #[test]
    fn is_section_header_detects_stderr() {
        assert!(is_section_header("── stderr ──"));
    }

    #[test]
    fn is_section_header_detects_command() {
        assert!(is_section_header("$ cargo build [one-shot ok 120ms]"));
    }

    #[test]
    fn is_section_header_rejects_content() {
        assert!(!is_section_header("running 3 tests"));
        assert!(!is_section_header("error[E0308]: mismatched types"));
    }
}
