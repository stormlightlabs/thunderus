use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::{Entry, ToolStatus};
use crate::ui::MAX_TOOL_OUTPUT_LINES;
use crate::ui::style;
use crate::utils::truncate_ellipsis;

const GUTTER: &str = "   │ ";

/// Rendered transcript block.
///
/// Blocks keep one semantic entry together while still allowing the final
/// transcript output to scroll by line. When the viewport starts inside a
/// block, rendering can show a continuation marker instead of a detached
/// wrapped line with no context.
#[derive(Clone)]
pub struct TranscriptBlock {
    lines: Vec<Line<'static>>,
}

impl TranscriptBlock {
    pub fn into_lines(self) -> Vec<Line<'static>> {
        self.lines
    }
}

/// Build transcript blocks from entries, including semantic spacing.
pub fn entry_blocks(entries: &[Entry], user_label: &str, max_width: usize) -> Vec<TranscriptBlock> {
    let mut blocks = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        if i > 0 && is_group_boundary(&entries[i - 1], entry) {
            blocks.push(TranscriptBlock { lines: vec![Line::styled("", style::panel_style())] });
        }
        blocks.push(TranscriptBlock { lines: entry_lines_with_width(entry, user_label, max_width) });
    }

    blocks
}

/// Render entry lines with an optional max width for truncation.
///
/// When `max_width > 0`, text content is wrapped to fit.
pub fn entry_lines_with_width(entry: &Entry, user_label: &str, max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    match entry {
        Entry::User { text } => user_message_lines(user_label, text, max_width),
        Entry::Assistant { text, .. } => assistant_message_lines("Assistant", text, max_width),
        Entry::Reasoning { text, streaming } => reasoning_lines(text, *streaming, max_width),
        Entry::Tool { name, arguments, status, output } => tool_lines(name, arguments, *status, output, max_width),
        Entry::Status { text } => message_lines(status_label(text), p.overlay1, text, max_width),
        Entry::Error { text } => error_lines(text, max_width),
    }
}

fn message_lines(label: &str, fg: Color, text: &str, max_width: usize) -> Vec<Line<'static>> {
    block_text_lines(
        label,
        fg,
        style::palette().surface0,
        text,
        style::text_style(),
        max_width,
    )
}

fn assistant_message_lines(label: &str, text: &str, max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let Some(markdown) = assistant_markdown_body(text) else {
        return message_lines(label, p.green, text, max_width);
    };

    let highlighted = super::highlight::highlight_code(markdown, Some("assistant.md"));
    let width = block_content_width(max_width);
    let mut lines = vec![
        block_line(Vec::new(), p.surface0, width, max_width),
        block_label_line(label, p.green, p.surface0, max_width),
    ];
    for line in highlighted {
        lines.extend(block_spans_lines(line.spans, p.surface0, max_width));
    }
    if lines.len() == 2 {
        lines.extend(block_text_body_lines("", style::text_style(), p.surface0, max_width));
    }
    lines.push(block_line(Vec::new(), p.surface0, width, max_width));
    lines
}

fn assistant_markdown_body(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix("````md\n")
        .or_else(|| text.strip_prefix("````markdown\n"))?;
    Some(rest.strip_suffix("\n````").unwrap_or(rest))
}

/// Render a user prompt with a visually distinct bounded block.
///
/// User messages get a colored left border bar (▌) and the user label as a
/// chip, making them stand out from assistant/tool rows which use plain
/// left-aligned labels.
fn user_message_lines(label: &str, text: &str, max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    block_text_lines(
        label,
        p.blue,
        p.surface1,
        text,
        Style::default().fg(p.text).bg(p.surface1),
        max_width,
    )
}

/// Render a reasoning block with a stable header/status line.
///
/// The header stays `Thinking` while streaming and after completion. The body
/// is italic and indented under the header,
/// matching the Gridland sibling-block pattern without nesting inside
/// assistant text.
fn reasoning_lines(text: &str, streaming: bool, max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let icon = if streaming { "·" } else { "✓" };
    let label = format!("Thinking {icon}");
    block_text_lines(
        &label,
        p.mauve,
        p.surface0,
        text,
        style::subtle_style().add_modifier(Modifier::ITALIC),
        max_width,
    )
}

// FIXME: I hate this
fn status_label(text: &str) -> &'static str {
    if text.starts_with("context  ") {
        "Context"
    } else if text.starts_with("logs  ") {
        "Session log"
    } else if text.starts_with("provider:") || text.starts_with("tool budget:") {
        "Diagnostic"
    } else if text.starts_with("queued ") {
        "Queued"
    } else if text.starts_with("queue target:") {
        "Queue"
    } else if text.starts_with("background ") || text == "no background processes" {
        "Background"
    } else if text == "cancelled" {
        "Cancelled"
    } else {
        "Notice"
    }
}

/// Render an error entry with the `⚠` icon as part of the label and the
/// error text aligned in the message body.
///
/// The icon sits in the label column so the error text aligns under the
/// message body of other rows. Long text is truncated with `…`.
fn error_lines(text: &str, max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let err_style = Style::default().fg(p.text).bg(p.surface0);
    block_text_lines("⚠ Error", p.red, p.surface0, text, err_style, max_width)
}

fn tool_lines(name: &str, args: &str, status: ToolStatus, output: &[String], max_width: usize) -> Vec<Line<'static>> {
    let p = style::palette();
    let (status_label, status_color, icon) = match status {
        ToolStatus::Running => ("running", p.yellow, "·"),
        ToolStatus::Ok => ("ok", p.green, "✓"),
        ToolStatus::Failed => ("failed", p.red, "✕"),
    };
    let args_summary = summarize_tool_args(args);

    let mut header_spans = Vec::new();
    header_spans.push(Span::styled(
        format!("{icon} "),
        Style::default().fg(status_color).bg(p.panel_bg),
    ));
    header_spans.push(Span::styled(
        name.to_string(),
        style::text_style().add_modifier(Modifier::BOLD),
    ));
    header_spans.push(Span::styled(
        format!(" [{status_label}]"),
        Style::default().fg(status_color).bg(p.panel_bg),
    ));

    let base_name = name.split('#').next().unwrap_or(name);
    let lang = tool_output_language(base_name, args);

    let gutter_style = Style::default().fg(p.overlay0).bg(p.panel_bg);

    let mut lines = if args_summary.is_empty() {
        vec![Line::from(header_spans)]
    } else if spans_width(&header_spans) + 2 + args_summary.chars().count() <= max_width {
        header_spans.push(Span::styled("  ", style::text_style()));
        header_spans.push(Span::styled(args_summary, style::muted_style()));
        vec![Line::from(header_spans)]
    } else {
        let mut lines = vec![Line::from(header_spans)];
        lines.extend(wrapped_lines(
            vec![Span::styled("  ", style::text_style())],
            &args_summary,
            style::muted_style(),
            max_width,
        ));
        lines
    };

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
                lines.extend(wrapped_spans(
                    &[Span::styled(GUTTER, gutter_style)],
                    hl.spans,
                    max_width,
                ));
            }
        }
        None => {
            for line in output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                let content_style = if is_section_header(line) {
                    style::text_style().add_modifier(Modifier::BOLD).fg(p.overlay1)
                } else {
                    style::subtle_style()
                };
                lines.extend(wrapped_lines(
                    vec![Span::styled(GUTTER, gutter_style)],
                    line,
                    content_style,
                    max_width,
                ));
            }
        }
    };

    if output.len() > MAX_TOOL_OUTPUT_LINES {
        lines.push(Line::from(vec![Span::styled(
            format!("   │ …({} more lines)", output.len() - MAX_TOOL_OUTPUT_LINES),
            style::muted_style(),
        )]));
    }

    block_existing_lines("tool", p.peach, p.surface0, lines, max_width)
}

fn block_text_lines(
    label: &str, label_color: Color, bg: Color, text: &str, text_style: Style, max_width: usize,
) -> Vec<Line<'static>> {
    let width = block_content_width(max_width);
    let mut lines = vec![
        block_line(Vec::new(), bg, width, max_width),
        block_label_line(label, label_color, bg, max_width),
    ];
    lines.extend(block_text_body_lines(text, text_style.bg(bg), bg, max_width));
    lines.push(block_line(Vec::new(), bg, width, max_width));
    lines
}

fn block_label_line(label: &str, fg: Color, bg: Color, max_width: usize) -> Line<'static> {
    let width = block_content_width(max_width);
    block_line(
        vec![Span::styled(
            label.to_string(),
            Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
        )],
        bg,
        width,
        max_width,
    )
}

fn block_text_body_lines(text: &str, text_style: Style, bg: Color, max_width: usize) -> Vec<Line<'static>> {
    let width = block_content_width(max_width);
    if width == 0 {
        return vec![block_line(Vec::new(), bg, width, max_width)];
    }
    wrap_text(text, width.max(1))
        .into_iter()
        .map(|part| block_line(vec![Span::styled(part, text_style.bg(bg))], bg, width, max_width))
        .collect()
}

fn block_spans_lines(spans: Vec<Span<'static>>, bg: Color, max_width: usize) -> Vec<Line<'static>> {
    let width = block_content_width(max_width);
    if width == 0 {
        return vec![block_line(Vec::new(), bg, width, max_width)];
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;

    for span in spans {
        let span_style = span.style.bg(bg);
        for ch in span.content.chars() {
            if current_width == width {
                lines.push(block_line(current, bg, width, max_width));
                current = Vec::new();
                current_width = 0;
            }
            current.push(Span::styled(ch.to_string(), span_style));
            current_width += 1;
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(block_line(current, bg, width, max_width));
    }
    lines
}

fn block_existing_lines(
    label: &str, label_color: Color, bg: Color, lines: Vec<Line<'static>>, max_width: usize,
) -> Vec<Line<'static>> {
    let width = block_content_width(max_width);
    let mut out = vec![
        block_line(Vec::new(), bg, width, max_width),
        block_label_line(label, label_color, bg, max_width),
    ];
    for line in lines {
        let mut spans = Vec::new();
        let mut used = 0usize;
        for span in line.spans {
            let span_width = span.content.chars().count();
            if used >= width {
                break;
            }
            let take = width.saturating_sub(used);
            let content: String = span.content.chars().take(take).collect();
            used += content.chars().count();
            spans.push(Span::styled(content, span.style.bg(bg)));
            if span_width > take {
                break;
            }
        }
        out.push(block_line(spans, bg, width, max_width));
    }
    out.push(block_line(Vec::new(), bg, width, max_width));
    out
}

fn block_content_width(max_width: usize) -> usize {
    let left_pad = max_width.min(2);
    let right_pad = max_width.saturating_sub(left_pad).min(2);
    max_width.saturating_sub(left_pad + right_pad)
}

fn block_line(mut body: Vec<Span<'static>>, bg: Color, width: usize, max_width: usize) -> Line<'static> {
    let mut trimmed = Vec::new();
    let mut used = 0usize;
    for span in body.drain(..) {
        if used >= width {
            break;
        }
        let take = width - used;
        let content: String = span.content.chars().take(take).collect();
        used += content.chars().count();
        trimmed.push(Span::styled(content, span.style.bg(bg)));
    }
    body = trimmed;

    if used < width {
        body.push(Span::styled(" ".repeat(width - used), Style::default().bg(bg)));
    }
    let left_pad = max_width.min(2);
    let right_pad = max_width.saturating_sub(left_pad + width).min(2);
    let mut spans = Vec::new();
    if left_pad > 0 {
        spans.push(Span::styled(" ".repeat(left_pad), Style::default().bg(bg)));
    }
    spans.extend(body);
    if right_pad > 0 {
        spans.push(Span::styled(" ".repeat(right_pad), Style::default().bg(bg)));
    }
    let mut line = Line::from(spans);
    line.style = Style::default().bg(bg);
    line
}

fn wrapped_lines(prefix: Vec<Span<'static>>, text: &str, text_style: Style, max_width: usize) -> Vec<Line<'static>> {
    let prefix_width = spans_width(&prefix);
    let body_width = max_width.saturating_sub(prefix_width).max(1);
    let mut lines = Vec::new();

    for part in wrap_text(text, body_width) {
        if lines.is_empty() {
            let mut spans = prefix.clone();
            spans.push(Span::styled(part, text_style));
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(vec![
                Span::styled(" ".repeat(prefix_width), style::text_style()),
                Span::styled(part, text_style),
            ]));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(prefix));
    }

    lines
}

fn wrapped_spans(prefix: &[Span<'static>], spans: Vec<Span<'static>>, max_width: usize) -> Vec<Line<'static>> {
    let prefix_width = spans_width(prefix);
    let body_width = max_width.saturating_sub(prefix_width).max(1);
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut current_width = 0usize;

    for span in spans {
        let style = span.style;
        for ch in span.content.chars() {
            if current_width == body_width {
                lines.push(line_with_prefix(prefix, &current, lines.is_empty(), prefix_width));
                current.clear();
                current_width = 0;
            }
            current.push(Span::styled(ch.to_string(), style));
            current_width += 1;
        }
    }

    if !current.is_empty() || lines.is_empty() {
        lines.push(line_with_prefix(prefix, &current, lines.is_empty(), prefix_width));
    }

    lines
}

fn line_with_prefix(
    prefix: &[Span<'static>], body: &[Span<'static>], first: bool, prefix_width: usize,
) -> Line<'static> {
    let mut spans = if first {
        prefix.to_vec()
    } else {
        vec![Span::styled(" ".repeat(prefix_width), style::text_style())]
    };
    spans.extend(body.iter().cloned());
    Line::from(spans)
}

fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();

    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let word_len = word.chars().count();
            let current_len = current.chars().count();

            if current_len == 0 {
                if word_len <= width {
                    current.push_str(word);
                } else {
                    lines.extend(split_long_word(word, width));
                }
            } else if current_len + 1 + word_len <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                if word_len <= width {
                    current.push_str(word);
                } else {
                    lines.extend(split_long_word(word, width));
                }
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        if current.chars().count() == width {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
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

/// Semantic group classification for transcript spacing.
#[derive(PartialEq)]
enum EntryGroup {
    User,
    Assistant,
    Reasoning,
    Tool,
    Transient,
}

impl From<&Entry> for EntryGroup {
    fn from(e: &Entry) -> Self {
        match e {
            Entry::User { .. } => EntryGroup::User,
            Entry::Assistant { .. } => EntryGroup::Assistant,
            Entry::Reasoning { .. } => EntryGroup::Reasoning,
            Entry::Tool { .. } => EntryGroup::Tool,
            Entry::Status { .. } | Entry::Error { .. } => EntryGroup::Transient,
        }
    }
}

fn is_group_boundary(prev: &Entry, curr: &Entry) -> bool {
    let prev_type = EntryGroup::from(prev);
    let curr_type = EntryGroup::from(curr);

    if prev_type == EntryGroup::Transient || curr_type == EntryGroup::Transient {
        false
    } else {
        prev_type != curr_type
    }
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

    #[test]
    fn assistant_markdown_body_strips_outer_four_tick_fence() {
        assert_eq!(
            assistant_markdown_body("````md\n# Done\n\n```rs\nfn main() {}\n```\n````"),
            Some("# Done\n\n```rs\nfn main() {}\n```")
        );
    }

    #[test]
    fn assistant_markdown_body_allows_streaming_partial_fence() {
        assert_eq!(assistant_markdown_body("````md\n# Done"), Some("# Done"));
    }

    #[test]
    fn entry_blocks_insert_separator_between_user_and_assistant() {
        let entries = vec![
            Entry::User { text: String::from("hello") },
            Entry::Assistant { text: String::from("hi"), streaming: false },
        ];

        let blocks = entry_blocks(&entries, "User", 80);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[1].lines[0].spans.is_empty());
    }

    #[test]
    fn block_line_paints_full_row_with_internal_padding() {
        let bg = Color::Blue;
        let line = block_line(vec![Span::styled("hi", Style::default().bg(bg))], bg, 6, 10);

        assert_eq!(line.style.bg, Some(bg));
        assert_eq!(spans_width(&line.spans), 10);
        assert_eq!(line.spans.first().unwrap().content.as_ref(), "  ");
        assert_eq!(line.spans.last().unwrap().content.as_ref(), "  ");
        assert!(line.spans.iter().all(|span| span.style.bg == Some(bg)));
    }

    #[test]
    fn block_line_does_not_exceed_tiny_width() {
        let bg = Color::Blue;
        let line = block_line(vec![Span::styled("hello", Style::default().bg(bg))], bg, 0, 3);

        assert_eq!(spans_width(&line.spans), 3);
        assert_eq!(line.style.bg, Some(bg));
        assert!(line.spans.iter().all(|span| span.style.bg == Some(bg)));
    }
}
