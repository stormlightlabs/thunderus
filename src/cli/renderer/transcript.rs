//! Semantic transcript row construction for the direct renderer.
//!
//! This module owns turning transcript [`Entry`] values into [`Row`] blocks.
//! Viewport policy (scrollback commits, width epochs, live-region clipping) stays
//! in [`super::region::LiveRegion`].

use std::path::Path;

use crate::app::{App, Entry, ToolStatus};
use crate::internals::{self, StartupSection};
use crate::renderer::row::Row;
use crate::renderer::style::{CellStyle, Color, Span};
use crate::{renderer, utils};

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Content width where the startup workbench can use two columns.
const WIDE_STARTUP_WORKBENCH_WIDTH: usize = 72;

/// Maximum skill-list rows shown under the startup workbench loaded heading.
const MAX_STARTUP_LOADED_SKILL_ROWS: usize = 4;

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

#[derive(Clone, Copy)]
struct StartupBannerTheme {
    width: usize,
    bg: Color,
    rail_style: CellStyle,
    marker_style: CellStyle,
    heading_style: CellStyle,
    muted_style: CellStyle,
}

impl StartupBannerTheme {
    fn body_width(self) -> usize {
        super::layout::content_width(self.width).saturating_sub(3)
    }
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
        let group_id = renderer::row::RowGroupId { entry_index: index };
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
    let theme = StartupBannerTheme {
        width,
        bg,
        rail_style: CellStyle::new().fg(p.overlay0).bg(bg),
        marker_style: CellStyle::new().fg(p.accent).bg(bg).bold(),
        heading_style: CellStyle::new().fg(p.text).bg(bg).bold(),
        muted_style: CellStyle::new().fg(p.subtext0).bg(bg),
    };
    let snapshot = app.self_knowledge_snapshot();
    let sections = snapshot.startup_sections();

    let mut rows = Vec::new();
    rows.push(Row::blank(width, bg_style(bg)));

    push_rail_text_row(&mut rows, "THNDRS", theme, theme.marker_style);
    if !app.cwd.as_os_str().is_empty() {
        let cwd_line = super::path_display::cwd_segment(&app.cwd, theme.body_width());
        let cwd_line = cwd_line.strip_prefix("cwd: ").unwrap_or(&cwd_line);
        push_rail_text_row(&mut rows, &format!("cwd {cwd_line}"), theme, theme.muted_style);
    }
    push_rail_empty_row(&mut rows, theme);

    push_startup_workbench(&mut rows, &snapshot, &sections, app, theme);

    push_rail_marker_row(&mut rows, "search", theme);
    for line in startup_section_lines(&sections, "Search", app) {
        push_wrapped_rail_text(&mut rows, &line, 3, 3, theme, theme.muted_style);
    }

    push_rail_empty_row(&mut rows, theme);
    push_rail_marker_row(&mut rows, "ready", theme);
    push_rail_command_row(
        &mut rows,
        ">",
        "Ask for a change, run a command, or inspect this repo.",
        theme,
    );
    push_rail_command_row(&mut rows, "?", "help", theme);
    push_rail_command_row(&mut rows, "/model", "switch models", theme);

    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

fn push_startup_workbench(
    rows: &mut Vec<Row>, snapshot: &internals::SelfKnowledgeSnapshot, sections: &[StartupSection], app: &App,
    theme: StartupBannerTheme,
) {
    push_rail_marker_row(rows, "workbench", theme);
    let body_width = theme.body_width();
    if body_width >= WIDE_STARTUP_WORKBENCH_WIDTH {
        push_wide_startup_workbench(rows, snapshot, sections, app, theme);
    } else {
        push_narrow_startup_workbench(rows, snapshot, sections, app, theme);
    }
}

fn push_wide_startup_workbench(
    rows: &mut Vec<Row>, snapshot: &internals::SelfKnowledgeSnapshot, sections: &[StartupSection], app: &App,
    theme: StartupBannerTheme,
) {
    let body_width = theme.body_width();
    let gap_width = 3usize;
    let left_width = 31usize.min(body_width.saturating_sub(gap_width));
    let right_width = body_width.saturating_sub(left_width + gap_width);

    let mut left = vec![
        "  system".to_string(),
        format!("     provider  {}", snapshot.runtime.provider.provider),
        format!("     model     {}", snapshot.runtime.provider.model),
        format!("     search    {}", snapshot.runtime.provider.search.mode),
        String::new(),
        "  loaded".to_string(),
    ];
    left.extend(startup_loaded_skill_lines(snapshot, left_width));

    let mut right = vec!["project".to_string()];
    for line in startup_section_lines(sections, "Context", app) {
        right.extend(wrap_labeled_value("context", &line, 3, 9, right_width));
    }
    right.push(String::new());
    right.push("attention".to_string());
    for line in startup_section_lines(sections, "Diagnostics", app) {
        right.extend(wrap_with_indent(&line, 3, 3, right_width));
    }

    let count = left.len().max(right.len());
    for index in 0..count {
        let left_line = left.get(index).map_or("", String::as_str);
        let right_line = right.get(index).map_or("", String::as_str);
        push_wide_workbench_row(rows, left_line, left_width, gap_width, right_line, theme);
    }
}

fn push_narrow_startup_workbench(
    rows: &mut Vec<Row>, snapshot: &internals::SelfKnowledgeSnapshot, sections: &[StartupSection], app: &App,
    theme: StartupBannerTheme,
) {
    push_rail_text_row(rows, "  system", theme, theme.heading_style);
    push_rail_text_row(
        rows,
        &format!("     provider  {}", snapshot.runtime.provider.provider),
        theme,
        theme.muted_style,
    );
    push_rail_text_row(
        rows,
        &format!("     model     {}", snapshot.runtime.provider.model),
        theme,
        theme.muted_style,
    );
    push_rail_text_row(
        rows,
        &format!("     search    {}", snapshot.runtime.provider.search.mode),
        theme,
        theme.muted_style,
    );

    push_rail_empty_row(rows, theme);
    push_rail_text_row(rows, "  project", theme, theme.heading_style);
    for line in startup_section_lines(sections, "Context", app) {
        push_wrapped_rail_text(rows, &format!("context   {line}"), 5, 5, theme, theme.muted_style);
    }

    push_rail_empty_row(rows, theme);
    push_rail_text_row(rows, "  attention", theme, theme.heading_style);
    for line in startup_section_lines(sections, "Diagnostics", app) {
        push_wrapped_rail_text(rows, &line, 5, 5, theme, theme.muted_style);
    }
}

fn startup_section_lines(sections: &[StartupSection], heading: &str, app: &crate::app::App) -> Vec<String> {
    sections
        .iter()
        .find(|section| section.heading == heading)
        .map(|section| {
            section
                .lines
                .iter()
                .map(|line| match section.heading {
                    "Context" | "Diagnostics" => super::path_display::transcript_line(line, &app.cwd),
                    _ => line.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn startup_loaded_skill_lines(snapshot: &crate::internals::SelfKnowledgeSnapshot, width: usize) -> Vec<String> {
    let skill_names = snapshot
        .inventory
        .references
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<Vec<_>>();
    if skill_names.is_empty() {
        return vec!["     (none)".to_string()];
    }

    let indent = "     ";
    let content_width = width.saturating_sub(indent.len()).max(1);
    let mut shown = 0usize;
    let mut rows = Vec::new();
    for count in 1..=skill_names.len() {
        let hidden = skill_names.len() - count;
        let candidate = loaded_skill_rows(&skill_names[..count], hidden > 0, content_width);
        let available_rows = if hidden > 0 {
            MAX_STARTUP_LOADED_SKILL_ROWS.saturating_sub(1)
        } else {
            MAX_STARTUP_LOADED_SKILL_ROWS
        };
        if candidate.len() > available_rows {
            break;
        }
        shown = count;
        rows = candidate;
    }

    let hidden = skill_names.len() - shown;
    if hidden > 0 {
        rows.push(format!("...{hidden} skills hidden"));
    }

    rows.into_iter().map(|line| format!("{indent}{line}")).collect()
}

fn loaded_skill_rows(skill_names: &[&str], has_hidden: bool, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    for (index, name) in skill_names.iter().enumerate() {
        let suffix = if has_hidden || index + 1 < skill_names.len() { "," } else { "" };
        add_loaded_skill(&mut rows, &format!("{name}{suffix}"), width);
    }
    rows.into_iter().filter(|line| !line.is_empty()).collect()
}

fn add_loaded_skill(rows: &mut Vec<String>, token: &str, width: usize) {
    for part in split_hyphenated_token(token, width) {
        let current = rows.last_mut().expect("loaded skill rows should keep one active row");
        let separator_width = usize::from(!current.is_empty());
        if current.is_empty() {
            current.push_str(&part);
        } else if utils::text_width(current) + separator_width + utils::text_width(&part) <= width {
            current.push(' ');
            current.push_str(&part);
        } else {
            rows.push(part);
        }
    }
}

fn split_hyphenated_token(token: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut remaining = token;
    let mut parts = Vec::new();
    while utils::text_width(remaining) > width {
        let split = hyphen_split_index(remaining, width);
        parts.push(remaining[..split].to_string());
        remaining = &remaining[split..];
    }
    if !remaining.is_empty() {
        parts.push(remaining.to_string());
    }
    parts
}

fn hyphen_split_index(text: &str, width: usize) -> usize {
    let mut fallback = text.len();
    for (index, _) in text.char_indices() {
        if index > 0 && utils::text_width(&text[..index]) > width {
            fallback = index;
            break;
        }
    }

    text.char_indices()
        .filter_map(|(index, ch)| {
            let split = index + ch.len_utf8();
            (ch == '-' && split <= fallback && utils::text_width(&text[..split]) <= width).then_some(split)
        })
        .next_back()
        .unwrap_or_else(|| fallback.max(1))
}

fn startup_workbench_heading(line: &str) -> bool {
    matches!(line.trim(), "system" | "loaded" | "project" | "attention")
}

fn wrap_labeled_value(label: &str, value: &str, first_indent: usize, value_indent: usize, width: usize) -> Vec<String> {
    let first_prefix = format!("{}{}  ", " ".repeat(first_indent), label);
    let continuation_prefix = " ".repeat(value_indent);
    let first_width = width.saturating_sub(utils::text_width(&first_prefix));
    let continuation_width = width.saturating_sub(value_indent);
    let wrapped = super::layout::wrap_text(value, first_width.max(1));
    let mut out = Vec::with_capacity(wrapped.len());
    for (index, line) in wrapped.into_iter().enumerate() {
        if index == 0 {
            out.push(format!("{first_prefix}{line}"));
        } else {
            for continued in super::layout::wrap_text(&line, continuation_width.max(1)) {
                out.push(format!("{continuation_prefix}{continued}"));
            }
        }
    }
    out
}

fn wrap_with_indent(text: &str, first_indent: usize, continuation_indent: usize, width: usize) -> Vec<String> {
    let first_width = width.saturating_sub(first_indent).max(1);
    let continuation_width = width.saturating_sub(continuation_indent).max(1);
    let mut out = Vec::new();
    for (index, line) in super::layout::wrap_text(text, first_width).into_iter().enumerate() {
        if index == 0 {
            out.push(format!("{}{}", " ".repeat(first_indent), line));
        } else {
            for continued in super::layout::wrap_text(&line, continuation_width) {
                out.push(format!("{}{}", " ".repeat(continuation_indent), continued));
            }
        }
    }
    out
}

fn pad_display_width(text: &str, width: usize) -> String {
    let used = utils::text_width(text);
    if used >= width {
        return text.to_string();
    }
    format!("{text}{}", " ".repeat(width - used))
}

fn push_wide_workbench_row(
    rows: &mut Vec<Row>, left: &str, left_width: usize, gap_width: usize, right: &str, theme: StartupBannerTheme,
) {
    let left_style = if startup_workbench_heading(left) { theme.heading_style } else { theme.muted_style };
    let right_style = if startup_workbench_heading(right) { theme.heading_style } else { theme.muted_style };

    rows.push(Row::padded(
        vec![
            Span::styled(" | ", theme.rail_style),
            Span::styled(pad_display_width(left, left_width), left_style),
            Span::styled(" ".repeat(gap_width), theme.muted_style),
            Span::styled(right.to_string(), right_style),
        ],
        theme.width,
        bg_style(theme.bg),
    ));
}

fn push_rail_marker_row(rows: &mut Vec<Row>, label: &str, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![
            Span::styled(" + ", theme.marker_style),
            Span::styled(label.to_string(), theme.heading_style),
        ],
        theme.width,
        bg_style(theme.bg),
    ));
}

fn push_rail_empty_row(rows: &mut Vec<Row>, theme: StartupBannerTheme) {
    rows.push(Row::padded(
        vec![Span::styled(" |", theme.rail_style)],
        theme.width,
        bg_style(theme.bg),
    ));
}

fn push_rail_text_row(rows: &mut Vec<Row>, text: &str, theme: StartupBannerTheme, text_style: CellStyle) {
    rows.push(Row::padded(
        vec![
            Span::styled(" | ", theme.rail_style),
            Span::styled(text.to_string(), text_style),
        ],
        theme.width,
        bg_style(theme.bg),
    ));
}

fn push_wrapped_rail_text(
    rows: &mut Vec<Row>, text: &str, first_indent: usize, continuation_indent: usize, theme: StartupBannerTheme,
    text_style: CellStyle,
) {
    let wrapped = wrap_with_indent(text, first_indent, continuation_indent, theme.body_width());
    for line in wrapped {
        push_rail_text_row(rows, &line, theme, text_style);
    }
}

fn push_rail_command_row(rows: &mut Vec<Row>, command: &str, description: &str, theme: StartupBannerTheme) {
    let body_width = theme.body_width();
    let command_width = 7usize.min(body_width);
    let first_prefix = format!("  {command:<command_width$}");
    let continuation_prefix = " ".repeat(2 + command_width);
    let first_width = body_width.saturating_sub(utils::text_width(&first_prefix)).max(1);
    let continuation_width = body_width
        .saturating_sub(utils::text_width(&continuation_prefix))
        .max(1);

    for (index, line) in super::layout::wrap_text(description, first_width)
        .into_iter()
        .enumerate()
    {
        if index == 0 {
            rows.push(Row::padded(
                vec![
                    Span::styled(" | ", theme.rail_style),
                    Span::styled(first_prefix.clone(), theme.marker_style),
                    Span::styled(line, theme.muted_style),
                ],
                theme.width,
                bg_style(theme.bg),
            ));
        } else {
            for continued in super::layout::wrap_text(&line, continuation_width) {
                push_rail_text_row(
                    rows,
                    &format!("{continuation_prefix}{continued}"),
                    theme,
                    theme.muted_style,
                );
            }
        }
    }
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
        let tool_content_width = body_width.saturating_sub(utils::text_width(GUTTER));

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
            let header_width: usize = header_spans.iter().map(|s| utils::text_width(&s.text)).sum();
            if header_width + 2 + utils::text_width(&args_summary) <= body_width {
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
                    let content_spans: Vec<_> = hl_row
                        .into_iter()
                        .map(|s| Span { text: s.text, style: s.style.bg(bg) })
                        .collect();
                    spans.extend(super::layout::truncate_spans(
                        &content_spans,
                        tool_content_width,
                        muted_style,
                    ));
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
                    for wrapped in super::layout::wrap_text_preserving_whitespace(&line, tool_content_width) {
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
            let mut rows = build_labeled_block(user_label, label_style, text_style, text, width, body_width, surface1);
            rows.push(Row::blank(width, bg_style(surface1)));
            rows
        }
        Entry::Agent { text, .. } => {
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
        vec![Span::styled("Agent".to_string(), label_style)],
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
        Err(_) => return utils::truncate_ellipsis(trimmed, 48),
    };
    let Some(obj) = v.as_object() else {
        return utils::truncate_ellipsis(trimmed, 48);
    };
    for key in &["pattern", "path", "query", "root", "glob", "file", "program", "url"] {
        if let Some(val) = obj.get(*key).and_then(|f| f.as_str()) {
            let val = super::path_display::transcript_line(val, cwd);
            return format!("{}: {}", key, utils::truncate_ellipsis(&val, 40));
        }
    }
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            let s = super::path_display::transcript_line(s, cwd);
            return format!("{k}: {}", utils::truncate_ellipsis(&s, 40));
        }
    }
    utils::truncate_ellipsis(trimmed, 48)
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
