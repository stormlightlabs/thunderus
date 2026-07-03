//! Semantic transcript row construction for the direct renderer.
//!
//! This module owns turning transcript [`Entry`] values into [`Row`] blocks.
//! Viewport policy (scrollback commits, width epochs, live-region clipping) stays
//! in [`super::region::LiveRegion`].

use std::path::Path;

use crate::app::{Entry, ToolStatus};
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Gutter prefix for tool output lines.
pub const GUTTER: &str = "   │ ";

/// Context needed to render a single transcript entry into rows.
#[derive(Clone)]
pub struct TranscriptRowContext<'a> {
    pub user_label: &'a str,
    pub cwd: &'a Path,
    pub width: usize,
    /// Index of the entry in the transcript. When present, rows are tagged with
    /// a [`RowGroupId`] so native scrollback navigation can correlate rows to
    /// the originating entry.
    pub entry_index: Option<usize>,
}

impl<'a> TranscriptRowContext<'a> {
    /// Build a context without entry grouping. Useful for tests that only need
    /// row snapshots and do not exercise scrollback navigation.
    #[cfg(test)]
    pub fn for_test(user_label: &'a str, cwd: &'a Path, width: usize) -> Self {
        Self { user_label, cwd, width, entry_index: None }
    }
}

/// Build all rows for a single transcript entry.
///
/// The returned rows are the full block for the entry. Callers that need to
/// split streaming or running content into stable/live portions do that on top
/// of this merged result.
pub fn entry_rows(entry: &Entry, ctx: &TranscriptRowContext) -> Vec<Row> {
    let mut rows = entry_to_rows(entry, ctx.user_label, ctx.width, ctx.cwd);
    if let Some(index) = ctx.entry_index {
        let group_id = crate::renderer::row::RowGroupId { entry_index: index };
        for row in &mut rows {
            row.group_id = Some(group_id);
        }
    }
    rows
}

/// Build startup banner rows from app state.
pub fn banner_rows(app: &crate::app::App, width: usize) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.panel_bg;
    let title_style = CellStyle::new().fg(p.accent).bg(bg).bold();
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);

    let mut rows = Vec::new();
    rows.push(Row::blank(width, bg_style(bg)));

    let banner_width = width.saturating_sub(4) as u16;
    let banner_lines = crate::banner::banner_lines(banner_width);
    let banner_is_art = banner_lines.len() > 1;
    for line in banner_lines {
        push_banner_art_row(&mut rows, Span::styled(line, title_style), width, bg);
    }
    if banner_is_art {
        push_banner_art_row(
            &mut rows,
            Span::styled(
                "─".repeat(width.saturating_sub(4)),
                CellStyle::new().fg(p.overlay0).bg(bg),
            ),
            width,
            bg,
        );
    }

    let title = String::from("thndrs  coding agent");
    push_wrapped_banner_row(&mut rows, &[Span::styled(title, title_style)], width, bg);
    push_wrapped_banner_row(
        &mut rows,
        &[
            Span::styled("model: ", muted_style),
            Span::styled(app.model.clone(), text_style),
            Span::styled("   search: ", muted_style),
            Span::styled(app.websearch.label(), text_style),
        ],
        width,
        bg,
    );

    if !app.cwd.as_os_str().is_empty() {
        push_wrapped_banner_row(
            &mut rows,
            &[Span::styled(format!("cwd: {}", app.cwd.display()), muted_style)],
            width,
            bg,
        );
    }

    for section in app.self_knowledge_snapshot().startup_sections() {
        push_wrapped_banner_row(
            &mut rows,
            &[
                Span::styled(format!("[{}] ", section.heading), title_style),
                Span::styled(section.body, muted_style),
            ],
            width,
            bg,
        );
    }

    push_wrapped_banner_row(
        &mut rows,
        &[
            Span::styled("›  ", title_style),
            Span::styled("Ask for a change, run a command, or inspect this repo.", muted_style),
        ],
        width,
        bg,
    );
    push_wrapped_banner_row(
        &mut rows,
        &[Span::styled("?  ", title_style), Span::styled("help", muted_style)],
        width,
        bg,
    );
    push_wrapped_banner_row(
        &mut rows,
        &[
            Span::styled("/model ", title_style),
            Span::styled("switch models", muted_style),
        ],
        width,
        bg,
    );

    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

/// Build a tool block: header row + args summary + output lines + vertical padding.
struct ToolBlockView<'a> {
    name: &'a str,
    args: &'a str,
    status: ToolStatus,
    output: &'a [String],
    width: usize,
    body_width: usize,
    bg: Color,
    cwd: &'a Path,
}

impl ToolBlockView<'_> {
    fn rows(&self) -> Vec<Row> {
        let name = self.name;
        let args = self.args;
        let status = self.status;
        let output = self.output;
        let width = self.width;
        let body_width = self.body_width;
        let bg = self.bg;
        let cwd = self.cwd;
        let p = super::style::palette();
        let (status_label, status_color, icon) = match status {
            ToolStatus::Running => ("running", p.peach, "·"),
            ToolStatus::Ok => ("ok", p.green, "✓"),
            ToolStatus::Failed => ("failed", p.red, "✕"),
            ToolStatus::Cancelled => ("cancelled", p.peach, "○"),
        };
        let header_style = CellStyle::new().fg(p.text).bg(bg).bold();
        let status_style = CellStyle::new().fg(status_color).bg(bg);
        let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
        let gutter_style = CellStyle::new().fg(p.overlay0).bg(bg);

        let args_summary = summarize_tool_args(args, cwd);
        let base_name = name.split('#').next().unwrap_or(name);
        let lang = super::highlight::tool_output_language(base_name, args);

        let mut rows = vec![Row::blank(width, bg_style(bg))];

        let mut header_spans = vec![
            Span::styled(format!("{icon} "), status_style),
            Span::styled(name.to_string(), header_style),
            Span::styled(format!(" [{status_label}]"), status_style),
        ];

        if !args_summary.is_empty() {
            let header_width: usize = header_spans.iter().map(|s| super::layout::display_width(&s.text)).sum();
            if header_width + 2 + super::layout::display_width(&args_summary) <= body_width {
                header_spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
                header_spans.push(Span::styled(args_summary, muted_style));
                rows.push(Row::padded(header_spans, width, bg_style(bg)));
            } else {
                rows.push(Row::padded(header_spans, width, bg_style(bg)));
                for wrapped in super::layout::wrap_text(&args_summary, body_width.saturating_sub(2)) {
                    let spans = vec![
                        Span::styled("  ", CellStyle::new().bg(bg)),
                        Span::styled(wrapped, muted_style),
                    ];
                    rows.push(Row::padded(spans, width, bg_style(bg)));
                }
            }
        } else {
            rows.push(Row::padded(header_spans, width, bg_style(bg)));
        }

        match lang {
            Some(lang_str) => {
                let joined: String = output
                    .iter()
                    .take(MAX_TOOL_OUTPUT_LINES)
                    .map(|line| {
                        let line = super::path_display::transcript_line(line, cwd);
                        format!("{line}\n")
                    })
                    .collect();
                let highlighted = super::highlight::highlight_lines(&joined, Some(lang_str));
                for hl_row in highlighted {
                    let mut spans = vec![Span::styled(GUTTER, gutter_style)];
                    spans.extend(hl_row.into_iter().map(|s| Span { text: s.text, style: s.style.bg(bg) }));
                    rows.push(Row::padded(spans, width, bg_style(bg)));
                }
            }
            None => {
                for line in output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                    let line = super::path_display::transcript_line(line, cwd);
                    let content_style = if is_section_header(&line) {
                        CellStyle::new().fg(p.overlay1).bg(bg).bold()
                    } else {
                        CellStyle::new().fg(p.subtext0).bg(bg)
                    };
                    for wrapped in
                        super::layout::wrap_text(&line, body_width.saturating_sub(super::layout::display_width(GUTTER)))
                    {
                        let spans = vec![Span::styled(GUTTER, gutter_style), Span::styled(wrapped, content_style)];
                        rows.push(Row::padded(spans, width, bg_style(bg)));
                    }
                }
            }
        }

        if output.len() > MAX_TOOL_OUTPUT_LINES {
            rows.push(Row::padded(
                vec![Span::styled(
                    format!(
                        "   │ … ({} lines stored, {} shown here)",
                        output.len(),
                        MAX_TOOL_OUTPUT_LINES
                    ),
                    muted_style,
                )],
                width,
                bg_style(bg),
            ));
        }

        rows
    }
}

/// Convert a single transcript entry to padded rows for scrollback.
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path) -> Vec<Row> {
    let p = super::style::palette();
    let bg = p.surface_dim;
    let body_width = super::layout::content_width(width);

    match entry {
        Entry::User { text } => {
            let surface1 = p.surface1;
            let label_style = CellStyle::new().fg(p.blue).bg(surface1).bold();
            let text_style = CellStyle::new().fg(p.text).bg(surface1);
            build_labeled_block(user_label, label_style, text_style, text, width, body_width, surface1)
        }
        Entry::Assistant { text, .. } => {
            let label_style = CellStyle::new().fg(p.green).bg(bg).bold();
            assistant_block_rows(text, label_style, bg, width, body_width)
        }
        Entry::Reasoning { text, streaming } => {
            let label_style = CellStyle::new().fg(p.mauve).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.subtext0).bg(bg).italic();
            let label = if *streaming { "Thinking ·" } else { "Thinking ✓" };
            build_labeled_block(label, label_style, text_style, text, width, body_width, bg)
        }
        Entry::Tool { name, arguments, status, output } => {
            ToolBlockView { name, args: arguments, status: *status, output, width, body_width, bg, cwd }.rows()
        }
        Entry::Status { text } => {
            let label_style = CellStyle::new().fg(p.overlay1).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.text).bg(bg);
            build_labeled_block(
                status_label_for(text),
                label_style,
                text_style,
                text,
                width,
                body_width,
                bg,
            )
        }
        Entry::Error { text } => {
            let label_style = CellStyle::new().fg(p.red).bg(bg).bold();
            let text_style = CellStyle::new().fg(p.text).bg(bg);
            build_labeled_block("⚠ Error", label_style, text_style, text, width, body_width, bg)
        }
    }
}

/// Build an assistant message block, detecting markdown code fences for
/// syntax highlighting.
fn assistant_block_rows(text: &str, label_style: CellStyle, bg: Color, width: usize, body_width: usize) -> Vec<Row> {
    let p = super::style::palette();
    let text_style = CellStyle::new().fg(p.text).bg(bg);
    let mut rows = vec![Row::blank(width, bg_style(bg))];
    rows.push(Row::padded(
        vec![Span::styled("Assistant".to_string(), label_style)],
        width,
        bg_style(bg),
    ));

    if let Some(markdown) = assistant_markdown_body(text) {
        rows.extend(render_markdown_body(markdown, text_style, bg, width, body_width));
    } else {
        for line in super::layout::wrap_text(text, body_width) {
            if line.is_empty() {
                rows.push(Row::blank(width, bg_style(bg)));
            } else {
                rows.push(Row::padded(vec![Span::styled(line, text_style)], width, bg_style(bg)));
            }
        }
    }

    if rows.len() == 2 {
        rows.push(Row::blank(width, bg_style(bg)));
    }
    rows
}

/// Extract the body from a four-tick markdown fence wrapper.
fn assistant_markdown_body(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix("````md\n")
        .or_else(|| text.strip_prefix("````markdown\n"))?;
    Some(rest.strip_suffix("\n````").unwrap_or(rest))
}

/// Render markdown body with code fence detection and syntax highlighting.
fn render_markdown_body(markdown: &str, text_style: CellStyle, bg: Color, width: usize, body_width: usize) -> Vec<Row> {
    let p = super::style::palette();
    let mut rows = Vec::new();
    let mut in_code_fence = false;
    let mut code_lang: Option<String> = None;
    let mut code_buf = String::new();

    for line in markdown.lines() {
        if line.starts_with("```") {
            if !in_code_fence {
                in_code_fence = true;
                let lang_str = line.trim_start_matches('`').trim();
                code_lang = if lang_str.is_empty() { None } else { Some(lang_str.to_string()) };
                code_buf.clear();
            } else {
                let lang = code_lang.as_deref();
                let highlighted = super::highlight::highlight_lines(&code_buf, lang);
                for hl_row in highlighted {
                    let mut spans = vec![Span::styled(GUTTER, CellStyle::new().fg(p.overlay0).bg(bg))];
                    spans.extend(hl_row.into_iter().map(|s| Span { text: s.text, style: s.style.bg(bg) }));
                    rows.push(Row::padded(spans, width, bg_style(bg)));
                }
                in_code_fence = false;
                code_lang = None;
                code_buf.clear();
            }
            continue;
        }

        if in_code_fence {
            code_buf.push_str(line);
            code_buf.push('\n');
            continue;
        }

        if line.is_empty() {
            rows.push(Row::blank(width, bg_style(bg)));
        } else {
            for wrapped in super::layout::wrap_text(line, body_width) {
                rows.push(Row::padded(
                    vec![Span::styled(wrapped, text_style)],
                    width,
                    bg_style(bg),
                ));
            }
        }
    }

    if in_code_fence && !code_buf.is_empty() {
        let lang = code_lang.as_deref();
        let highlighted = super::highlight::highlight_lines(&code_buf, lang);
        for hl_row in highlighted {
            let mut spans = vec![Span::styled(GUTTER, CellStyle::new().fg(p.overlay0).bg(bg))];
            spans.extend(hl_row.into_iter().map(|s| Span { text: s.text, style: s.style.bg(bg) }));
            rows.push(Row::padded(spans, width, bg_style(bg)));
        }
    }

    if rows.is_empty() {
        rows.push(Row::blank(width, bg_style(bg)));
    }

    rows
}

/// Build a labeled text block with a single leading spacer row.
fn build_labeled_block(
    label: &str, label_style: CellStyle, text_style: CellStyle, text: &str, width: usize, body_width: usize, bg: Color,
) -> Vec<Row> {
    let mut rows = vec![Row::blank(width, bg_style(bg))];
    rows.push(Row::padded(
        vec![Span::styled(label.to_string(), label_style)],
        width,
        bg_style(bg),
    ));

    for line in super::layout::wrap_text(text, body_width) {
        if line.is_empty() {
            rows.push(Row::blank(width, bg_style(bg)));
        } else {
            rows.push(Row::padded(vec![Span::styled(line, text_style)], width, bg_style(bg)));
        }
    }
    rows
}

fn push_wrapped_banner_row(rows: &mut Vec<Row>, spans: &[Span], width: usize, bg: Color) {
    let body_width = super::layout::content_width(width);
    for line in super::layout::wrap_spans(spans, body_width) {
        rows.push(Row::padded(line, width, bg_style(bg)));
    }
}

fn push_banner_art_row(rows: &mut Vec<Row>, span: Span, width: usize, bg: Color) {
    rows.push(Row::padded(vec![span], width, bg_style(bg)));
}

/// Detect whether a tool output line is a section header.
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("── ") || trimmed.starts_with("$ ")
}

/// Produce a short summary of a tool's arguments for the transcript line.
pub fn summarize_tool_args(arguments: &str, cwd: &Path) -> String {
    let trimmed = arguments.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return String::new();
    }
    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return crate::utils::truncate_ellipsis(trimmed, 48),
    };
    let Some(obj) = v.as_object() else {
        return crate::utils::truncate_ellipsis(trimmed, 48);
    };
    for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
        if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
            let val = super::path_display::transcript_line(val, cwd);
            return format!("{}: {}", key, crate::utils::truncate_ellipsis(&val, 40));
        }
    }
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            let s = super::path_display::transcript_line(s, cwd);
            return format!("{k}: {}", crate::utils::truncate_ellipsis(&s, 40));
        }
    }
    crate::utils::truncate_ellipsis(trimmed, 48)
}

/// Derive a label for status entries based on text content.
fn status_label_for(text: &str) -> &'static str {
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
        "System"
    }
}

/// Build a [`CellStyle`] with only a background color.
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

#[cfg(test)]
mod tests;
