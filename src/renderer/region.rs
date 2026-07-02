//! Live region state for the direct renderer.
//!
//! [`LiveRegion`] builds one logical terminal viewport from recent history rows
//! and live prompt chrome, then redraws that bounded viewport each tick.
//!
//! The viewport contains, from top to bottom:
//! 1. banner before the first transcript rows have been committed;
//! 2. recent committed transcript rows;
//! 3. the mutable transcript tail, if any;
//! 4. dynamic status row (session + status icon);
//! 5. prompt input rows;
//! 6. optional accessory rows (help/commands/files);
//! 7. static status row (model/search/tokens/cwd);
//! 8. blank canvas rows after the compact transcript/live content.

use std::{io, path::Path};

use crate::app::{App, Entry, ToolStatus};
use crate::renderer::backend::TerminalBackend;
use crate::renderer::live as rows;
use crate::renderer::row::{Frame, Row};
use crate::renderer::style::{self, CellStyle, Color, Span};
use crate::utils;

/// Maximum rows the prompt input can occupy before scrolling within the live region.
const MAX_PROMPT_ROWS: usize = 8;

/// Maximum accessory rows (help/commands/files) shown in the live region.
const MAX_ACCESSORY_ROWS: usize = 8;

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Gutter prefix for tool output lines.
const GUTTER: &str = "   │ ";

/// State tracking the live region render and committed history.
///
/// Transcript rows that are stable are written to the terminal's native
/// scrollback via [`insert_history_lines`](TerminalBackend::insert_history_lines).
/// The viewport redraw includes the visible tail of committed transcript rows,
/// plus live chrome (prompt/status/accessories) and any mutable transcript tail
/// that cannot be safely appended yet.
#[derive(Debug)]
pub struct LiveRegion {
    /// The last frame rendered to the screen, used for diff-based rendering.
    rendered_frame: Option<Frame>,
    /// Terminal width at the last render.
    rendered_width: Option<usize>,
    /// Terminal height at the last render.
    rendered_height: Option<usize>,
    /// Top row used for the last diff-rendered frame.
    rendered_top_row: Option<u16>,
    /// Number of stable transcript rows already committed to terminal scrollback.
    committed_row_count: usize,
    /// Terminal width used for committed rows.
    ///
    /// Width changes require replay so wrapped rows do not duplicate or stale.
    committed_width: Option<usize>,
}

impl Default for LiveRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRegion {
    /// Create a fresh live region with nothing committed.
    pub fn new() -> Self {
        LiveRegion {
            rendered_frame: None,
            rendered_width: None,
            rendered_height: None,
            rendered_top_row: None,
            committed_row_count: 0,
            committed_width: None,
        }
    }

    /// Build the live-region frame.
    ///
    /// Before the banner has been committed to scrollback (i.e. before the
    /// first transcript entry arrives), the banner rows are included in the
    /// frame so they stay visible on screen at startup.
    ///
    /// Once the banner is committed, the frame contains only the live chrome.
    pub fn build_frame(&self, app: &App, width: usize, height: usize) -> Frame {
        let mut frame = Frame::new(width);
        if width == 0 || height == 0 {
            return frame;
        }

        let plan = TranscriptRenderPlan::new(app, width);
        let live_tail = plan.live_rows.clone();
        let live = self.build_live_frame(app, width, height, live_tail);
        let live_height = live.rows.len().min(height);

        let history_rows = if app.transcript.is_empty() { banner_rows(app, width) } else { plan.stable_rows };
        let available_history = height.saturating_sub(live_height);
        let history_start = history_rows.len().saturating_sub(available_history);

        frame
            .rows
            .extend(history_rows.into_iter().skip(history_start).take(available_history));

        let p = style::palette();
        let live_start = live.rows.len().saturating_sub(live_height);

        while frame.rows.len() < height.saturating_sub(live_height) {
            frame.push(Row::blank(width, bg_style(p.panel_bg)));
        }

        let live_offset = frame.rows.len();
        let cursor = live.cursor.and_then(|mut cursor| {
            if cursor.row < live_start {
                return None;
            }
            cursor.row = cursor.row - live_start + live_offset;
            Some(cursor)
        });

        frame
            .rows
            .extend(live.rows.into_iter().skip(live_start).take(live_height));
        frame.cursor = cursor;

        let prompt_editable = matches!(app.prompt_state(), crate::app::PromptState::Editable);
        frame.cursor_visible = prompt_editable;

        while frame.rows.len() < height {
            frame.push(Row::blank(width, bg_style(p.panel_bg)));
        }

        frame
    }

    /// Build bottom-anchored live prompt/status rows.
    fn build_live_frame(&self, app: &App, width: usize, height: usize, live_tail: Vec<Row>) -> Frame {
        let mut frame = Frame::new(width);

        for row in clip_live_tail_rows(live_tail, height) {
            frame.push(row);
        }

        frame.push(Row::blank(width, bg_style(style::palette().surface0)));
        frame.push(rows::dynamic_status_row(app, width));

        let (prompt_rows, cursor) = rows::prompt_rows_for(app, width);
        let prompt_count = prompt_rows.len().min(MAX_PROMPT_ROWS);
        let prompt_block_count = prompt_count + 1;
        let remaining_after_prompt = height.saturating_sub(frame.len() + prompt_block_count + 1);
        let accessory_height = remaining_after_prompt.min(MAX_ACCESSORY_ROWS);
        let accessory = rows::accessory_rows(app, width, accessory_height);

        let prompt_offset = frame.len();
        for row in prompt_rows.into_iter().take(prompt_count) {
            frame.push(row);
        }

        if let Some(mut c) = cursor {
            c.row += prompt_offset;
            frame.set_cursor(c);
        }

        for row in accessory {
            frame.push(row);
        }

        frame.push(rows::static_status_row(app, width));
        frame.push(Row::blank(width, bg_style(style::palette().surface0)));
        frame
    }

    /// Render the live-region viewport, committing stable transcript rows to
    /// terminal scrollback first.
    ///
    /// 1. Render the transcript into stable scrollback rows plus mutable live
    ///    tail rows.
    /// 2. Append only the stable rows that have not yet been inserted into
    ///    native scrollback.
    /// 3. Use
    ///    [`insert_history_lines`](TerminalBackend::insert_history_lines) to
    ///    push them into native terminal scrollback above the viewport.
    /// 4. Render the live region (mutable tail, prompt/status/accessories) via
    ///    diff rendering.
    pub fn render_frame<W: io::Write>(
        &mut self, app: &App, backend: &mut TerminalBackend<W>, width: usize, height: usize,
    ) -> io::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }

        if self
            .committed_width
            .is_some_and(|committed_width| committed_width != width)
        {
            backend.clear_all()?;
            self.rendered_frame = None;
            self.committed_row_count = 0;
        }

        let plan = TranscriptRenderPlan::new(app, width);
        if self.committed_row_count > plan.stable_rows.len() {
            backend.clear_all()?;
            self.rendered_frame = None;
            self.committed_row_count = 0;
        }

        let rows_to_commit = &plan.stable_rows[self.committed_row_count..];
        if !rows_to_commit.is_empty() {
            backend.insert_history_lines(rows_to_commit, height as u16)?;
            self.rendered_frame = None;
            self.committed_row_count = plan.stable_rows.len();
            self.committed_width = Some(width);
        }

        let frame = self.build_frame(app, width, height);
        let top_row = 0;

        if self.rendered_top_row.is_some_and(|prev_top| prev_top != top_row) {
            if let Some(prev) = self.rendered_frame.as_ref() {
                backend.clear_rows(self.rendered_top_row.unwrap_or(0), prev.rows.len() as u16)?;
            }
            self.rendered_frame = None;
        }

        if self.rendered_width != Some(width)
            || self.rendered_height != Some(height)
            || self.rendered_top_row != Some(top_row)
        {
            backend.render_frame(&frame, top_row)?;
        } else {
            backend.render_frame_diff(&frame, self.rendered_frame.as_ref(), top_row)?;
        }

        self.rendered_frame = Some(frame);
        self.rendered_width = Some(width);
        self.rendered_height = Some(height);
        self.rendered_top_row = Some(top_row);
        Ok(())
    }

    /// Reset all committed state (e.g. on `/clear`).
    pub fn reset(&mut self) {
        self.rendered_frame = None;
        self.rendered_width = None;
        self.rendered_height = None;
        self.rendered_top_row = None;
        self.committed_row_count = 0;
        self.committed_width = None;
    }
}

#[derive(Clone, Debug)]
struct TranscriptRenderPlan {
    stable_rows: Vec<Row>,
    live_rows: Vec<Row>,
}

impl TranscriptRenderPlan {
    fn new(app: &App, width: usize) -> TranscriptRenderPlan {
        let mut stable_rows = Vec::new();
        let mut live_rows = Vec::new();

        if app.transcript.is_empty() {
            return TranscriptRenderPlan { stable_rows, live_rows };
        }

        stable_rows.extend(banner_rows(app, width));

        for entry in &app.transcript {
            let (entry_stable, entry_live) = entry_stable_and_live_rows(entry, &app.user_label, width, &app.cwd);
            if entry_stable.is_empty() {
                live_rows.extend(entry_live);
            } else {
                stable_rows.extend(entry_stable);
                live_rows.extend(entry_live);
            }
        }

        TranscriptRenderPlan { stable_rows, live_rows }
    }
}

fn clip_live_tail_rows(mut active: Vec<Row>, height: usize) -> Vec<Row> {
    let max_active = height.saturating_sub(4).max(1);
    if active.len() > max_active { active.split_off(active.len() - max_active) } else { active }
}

fn entry_stable_and_live_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path) -> (Vec<Row>, Vec<Row>) {
    match entry {
        Entry::Assistant { streaming: true, .. } | Entry::Reasoning { streaming: true, .. } => {
            split_streaming_text_rows(entry, user_label, width, cwd)
        }
        Entry::Tool { status: ToolStatus::Running, .. } => (Vec::new(), entry_to_rows(entry, user_label, width, cwd)),
        _ => (entry_to_rows(entry, user_label, width, cwd), Vec::new()),
    }
}

fn split_streaming_text_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path) -> (Vec<Row>, Vec<Row>) {
    let rows = entry_to_rows(entry, user_label, width, cwd);
    if rows.len() <= 3 {
        return (Vec::new(), rows);
    }

    let stable_len = rows.len().saturating_sub(2);
    let stable_rows = rows[..stable_len].to_vec();
    let live_rows = rows[stable_len..].to_vec();
    (stable_rows, live_rows)
}

/// Build banner rows for scrollback.
fn banner_rows(app: &App, width: usize) -> Vec<Row> {
    let p = style::palette();
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

fn push_wrapped_banner_row(rows: &mut Vec<Row>, spans: &[Span], width: usize, bg: Color) {
    let body_width = crate::renderer::layout::content_width(width);
    for line in crate::renderer::layout::wrap_spans(spans, body_width) {
        rows.push(Row::padded(line, width, bg_style(bg)));
    }
}

fn push_banner_art_row(rows: &mut Vec<Row>, span: Span, width: usize, bg: Color) {
    rows.push(Row::padded(vec![span], width, bg_style(bg)));
}

/// Build a [`CellStyle`] with only a background color.
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

/// Convert a single transcript entry to padded rows for scrollback.
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize, cwd: &Path) -> Vec<Row> {
    let p = style::palette();
    let bg = p.surface_dim;
    let body_width = crate::renderer::layout::content_width(width);

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
            tool_block_rows(name, arguments, *status, output, width, body_width, bg, cwd)
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
    let p = style::palette();
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
    let p = style::palette();
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
                let highlighted = crate::renderer::highlight::highlight_lines(&code_buf, lang);
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

/// Build a tool block: header row + args summary + output lines + vertical padding.
fn tool_block_rows(
    name: &str, args: &str, status: ToolStatus, output: &[String], width: usize, body_width: usize, bg: Color,
    cwd: &Path,
) -> Vec<Row> {
    let p = style::palette();
    let (status_label, status_color, icon) = match status {
        ToolStatus::Running => ("running", p.peach, "·"),
        ToolStatus::Ok => ("ok", p.green, "✓"),
        ToolStatus::Failed => ("failed", p.red, "✕"),
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
        let header_width: usize = header_spans
            .iter()
            .map(|s| crate::renderer::layout::display_width(&s.text))
            .sum();
        if header_width + 2 + crate::renderer::layout::display_width(&args_summary) <= body_width {
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
                    let line = crate::renderer::path_display::transcript_line(line, cwd);
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
                let line = crate::renderer::path_display::transcript_line(line, cwd);
                let content_style = if is_section_header(&line) {
                    CellStyle::new().fg(p.overlay1).bg(bg).bold()
                } else {
                    CellStyle::new().fg(p.subtext0).bg(bg)
                };
                for wrapped in crate::renderer::layout::wrap_text(
                    &line,
                    body_width.saturating_sub(crate::renderer::layout::display_width(GUTTER)),
                ) {
                    let spans = vec![Span::styled(GUTTER, gutter_style), Span::styled(wrapped, content_style)];
                    rows.push(Row::padded(spans, width, bg_style(bg)));
                }
            }
        }
    }

    if output.len() > MAX_TOOL_OUTPUT_LINES {
        rows.push(Row::padded(
            vec![Span::styled(
                format!("   │ …({} more lines)", output.len() - MAX_TOOL_OUTPUT_LINES),
                muted_style,
            )],
            width,
            bg_style(bg),
        ));
    }

    rows
}

/// Detect whether a tool output line is a section header.
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("── ") || trimmed.starts_with("$ ")
}

/// Produce a short summary of a tool's arguments for the transcript line.
fn summarize_tool_args(arguments: &str, cwd: &Path) -> String {
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
            let val = crate::renderer::path_display::transcript_line(val, cwd);
            return format!("{}: {}", key, utils::truncate_ellipsis(&val, 40));
        }
    }
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            let s = crate::renderer::path_display::transcript_line(s, cwd);
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

    for line in crate::renderer::layout::wrap_text(text, body_width) {
        if line.is_empty() {
            rows.push(Row::blank(width, bg_style(bg)));
        } else {
            rows.push(Row::padded(vec![Span::styled(line, text_style)], width, bg_style(bg)));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::{App, Entry, RunState, ToolStatus};
    use crate::cli::{Cli, Theme, WebSearchMode};
    use crate::renderer::row;

    fn vt100_contents(bytes: &[u8], width: u16, height: u16) -> String {
        let mut parser = vt100::Parser::new(height, width, 200);
        parser.process(bytes);
        parser.screen().contents()
    }

    fn nonblank_lines(contents: &str) -> Vec<&str> {
        contents.lines().filter(|line| !line.trim().is_empty()).collect()
    }

    fn render_entry_styled(entry: &Entry, width: usize) -> String {
        let rows = entry_to_rows(entry, "User", width, Path::new("."));
        let frame = row::Frame { rows, width, cursor: None, cursor_visible: true };
        frame.render_styled()
    }

    fn test_app() -> App {
        let mut app = App::from_cli(&Cli {
            cwd: PathBuf::from("."),
            model: "test-model".to_string(),
            websearch: WebSearchMode::Native,
            tick_rate_ms: 100,
            no_alt_screen: true,
            no_mouse: false,
            mouse: false,
            verbose: false,
            theme: Theme::EldritchMinimal,
            print_prompt: false,
        });
        app.session_id = "test-session".to_string();
        app.git_status = Some(crate::renderer::git::GitStatusSummary {
            branch: Some("main".to_string()),
            added: 0,
            modified: 0,
            deleted: 0,
        });
        app
    }

    #[test]
    fn live_region_starts_empty() {
        let lr = LiveRegion::new();
        assert_eq!(lr.rendered_width, None);
        assert_eq!(lr.rendered_height, None);
    }

    #[test]
    fn build_frame_has_terminal_height() {
        let app = test_app();
        let frame = LiveRegion::new().build_frame(&app, 80, 24);
        assert_eq!(frame.len(), 24);
        assert!(frame.rows.iter().all(|row| row.width == 80));
    }

    #[test]
    fn build_frame_contains_live_prompt_and_status() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "hi there".to_string(), streaming: false });
        app.input.set_text("next question");

        let frame = LiveRegion::new().build_frame(&app, 80, 24);
        let combined = frame.render_text();
        assert!(
            combined.contains("next question"),
            "prompt should be part of the viewport"
        );
        assert!(combined.contains("model:"), "footer should be part of the viewport");
    }

    #[test]
    fn build_frame_includes_streaming_when_active() {
        let mut app = test_app();
        app.transcript
            .push(Entry::Assistant { text: "streaming text".to_string(), streaming: true });
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);

        let combined: String = frame.rows.iter().map(|r| r.text()).collect();
        assert!(
            combined.contains("streaming text"),
            "streaming text should be in live frame"
        );
        assert!(
            combined.contains("Assistant"),
            "assistant label should be in live frame"
        );
    }

    #[test]
    fn build_frame_keeps_live_rows_at_bottom() {
        let mut app = test_app();
        app.input.set_text("hello");

        let frame = LiveRegion::new().build_frame(&app, 80, 12);
        assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
        assert!(frame.rows[frame.len() - 2].text().contains("model:"));
        assert!(frame.rows[frame.len() - 3].text().contains("hello"));
        assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
        assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
    }

    #[test]
    fn build_frame_keeps_live_rows_at_bottom_with_status_notice() {
        let mut app = test_app();
        app.transcript
            .push(Entry::Status { text: "Press CTRL+D again to quit.".to_string() });

        let frame = LiveRegion::new().build_frame(&app, 80, 16);
        assert!(
            frame.render_text().contains("Press CTRL+D again to quit."),
            "status notice should still render"
        );
        assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
        assert!(frame.rows[frame.len() - 2].text().contains("model:"));
        assert!(frame.rows[frame.len() - 3].text().contains("›"));
        assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
        assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
        assert_eq!(
            frame.cursor,
            Some(crate::renderer::row::CursorCoord::new(frame.len() - 3, 6)),
            "cursor should be on the bottom-pinned prompt row"
        );
    }

    #[test]
    fn build_frame_height_matches_viewport() {
        let mut app = test_app();
        for _ in 0..12 {
            app.transcript.push(Entry::User { text: "message".to_string() });
        }

        let frame = LiveRegion::new().build_frame(&app, 80, 10);
        let combined = frame.render_text();
        assert_eq!(frame.len(), 10, "frame should fill the viewport height");
        assert!(combined.contains("model:"), "footer should remain visible");
    }

    #[test]
    fn build_frame_cursor_set_for_editable_prompt() {
        let mut app = test_app();
        app.input.set_text("hello");

        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        assert!(frame.cursor.is_some(), "cursor should be set for editable prompt");
    }

    #[test]
    fn build_frame_cursor_visible_for_editable_prompt_across_ticks() {
        let mut app = test_app();
        app.input.set_text("hello");

        let lr = LiveRegion::new();
        for tick in 0..20u64 {
            app.ui_tick = tick;
            let frame = lr.build_frame(&app, 80, 24);
            assert!(
                frame.cursor_visible,
                "cursor should be visible on every tick for editable prompt (tick={tick})"
            );
        }
    }

    #[test]
    fn build_frame_cursor_hidden_for_non_editable_prompt() {
        let mut app = test_app();
        app.input.set_text("hello");
        app.run_state = RunState::Working;
        app.transcript.push(Entry::User { text: "go".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "working...".to_string(), streaming: false });

        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        assert!(
            !frame.cursor_visible,
            "cursor should be hidden when prompt is not editable"
        );
    }

    #[test]
    fn render_frame_diff_emits_no_hide_when_cursor_stays_visible() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let new_bytes = &second_output[first_len..];

        assert!(
            !new_bytes.contains("\x1b[?25l"),
            "re-render of identical visible-cursor frame should not emit Hide: {new_bytes:?}"
        );
    }

    #[test]
    fn render_frame_diff_emits_no_show_on_unchanged_visible_cursor() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let new_bytes = &second_output[first_len..];

        assert_eq!(
            new_bytes.len(),
            0,
            "identical frame with unchanged cursor should produce zero output, got: {new_bytes:?}"
        );
    }

    #[test]
    fn render_frame_writes_from_top() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("\x1b[1;1H"), "viewport render should start at top-left");
        assert!(out.contains("\x1b[K"), "should clear each row to end-of-line");
        assert_eq!(lr.rendered_width, Some(80));
        assert_eq!(lr.rendered_height, Some(24));
        assert!(
            lr.rendered_frame.is_some(),
            "should store the rendered frame for diffing"
        );
    }

    #[test]
    fn render_frame_diff_skips_unchanged_rows() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let first_output_len = String::from_utf8(backend.writer().clone()).unwrap().len();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let second_new_bytes = second_output.len() - first_output_len;

        assert_eq!(
            second_new_bytes,
            0,
            "identical frame should produce no output, got: {:?}",
            &second_output[first_output_len..]
        );
    }

    #[test]
    fn render_frame_diff_writes_only_changed_rows() {
        let mut app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        app.ui_tick = app.ui_tick.wrapping_add(1);
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(!out.is_empty(), "changed frame should produce output");
    }

    #[test]
    fn render_frame_full_redraw_on_resize() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();
        lr.render_frame(&app, &mut backend, 100, 30).unwrap();

        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let second_new_bytes = second_output.len() - first_len;

        assert!(second_new_bytes > 0, "resize should trigger a full redraw with output");
    }

    #[test]
    fn render_frame_commits_submitted_user_to_scrollback() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript.push(Entry::User { text: "start the task".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(
            out.contains("\x1b[1;"),
            "history rows should be inserted through a constrained scroll region"
        );
        assert!(
            out.contains("start the task"),
            "submitted prompt should be appended to native scrollback immediately"
        );
        assert!(lr.committed_row_count > 0);
    }

    #[test]
    fn build_frame_places_streaming_output_above_status_line() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::Assistant { text: "streaming response text".to_string(), streaming: true });

        let frame = LiveRegion::new().build_frame(&app, 80, 24);
        let output_row = frame
            .rows
            .iter()
            .position(|row| row.text().contains("streaming response text"))
            .expect("streaming output should render");
        let status_row = frame
            .rows
            .iter()
            .position(|row| row.text().contains("test-session"))
            .expect("status line should render");

        assert!(
            output_row < status_row,
            "mutable transcript output should stay above the running/status line"
        );
    }

    #[test]
    fn build_frame_keeps_user_text_visible_when_live_tail_grows() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: "consolidate the renderer milestones".to_string() });
        app.transcript.push(Entry::Reasoning {
            text: "reading TODO.md before summarizing the requested renderer milestones".to_string(),
            streaming: true,
        });
        app.transcript.push(Entry::Tool {
            name: "read_file_range".to_string(),
            arguments: r#"{"path": "TODO.md"}"#.to_string(),
            status: ToolStatus::Running,
            output: vec![
                "1: # TODO".to_string(),
                "2:".to_string(),
                "3: ## Completed Summary".to_string(),
                "4:".to_string(),
                "5: ### Harness, Provider, And Event Loop".to_string(),
                "6:".to_string(),
            ],
        });

        let frame = LiveRegion::new().build_frame(&app, 120, 32);
        let lines: Vec<String> = frame.rows.iter().map(|row| row.text()).collect();
        let user_label = lines
            .iter()
            .position(|line| line.contains("User"))
            .expect("user label should remain visible");
        let user_text = lines
            .iter()
            .position(|line| line.contains("consolidate the renderer milestones"))
            .expect("user text should remain visible");
        let thinking = lines
            .iter()
            .position(|line| line.contains("Thinking"))
            .expect("live reasoning should render");

        assert!(
            user_label < user_text && user_text < thinking,
            "live rows should not overwrite the bottom of the user block:\n{}",
            frame.render_text()
        );
    }

    #[test]
    fn build_frame_keeps_done_prompt_bottom_anchored_after_latest_assistant_message() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "summarize TODO".to_string() });
        app.transcript.push(Entry::Assistant {
            text: "Here is the consolidated renderer summary.".to_string(),
            streaming: false,
        });
        app.input.set_text("Please update @TODO.md with the updated content");

        let frame = LiveRegion::new().build_frame(&app, 120, 32);
        let lines: Vec<String> = frame.rows.iter().map(|row| row.text()).collect();
        let assistant_body = lines
            .iter()
            .position(|line| line.contains("Here is the consolidated renderer summary."))
            .expect("assistant body should render");
        let status = lines
            .iter()
            .position(|line| line.contains("test-session"))
            .expect("status row should render");

        assert!(
            assistant_body < status,
            "transcript should stay above live prompt/status:\n{}",
            frame.render_text()
        );
        assert!(frame.rows[frame.len() - 1].text().trim().is_empty());
        assert!(frame.rows[frame.len() - 2].text().contains("model:"));
        assert!(frame.rows[frame.len() - 3].text().contains("Please update @TODO.md"));
        assert!(frame.rows[frame.len() - 4].text().contains("test-session"));
        assert!(frame.rows[frame.len() - 5].text().trim().is_empty());
    }

    #[test]
    fn render_frame_commits_streaming_stable_prefix_and_keeps_tail_live() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::Assistant { text: "first stable line second mutable tail".to_string(), streaming: true });

        let mut backend = TerminalBackend::new(Vec::new(), 24, 16);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 24, 16).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(
            out.contains("Assistant"),
            "stable streaming block header should be committed to scrollback"
        );
        let frame_text = lr.rendered_frame.as_ref().unwrap().render_text();
        assert!(
            frame_text.contains("mutable tail") || frame_text.contains("tail"),
            "the mutable streaming tail should remain in the live frame"
        );
    }

    #[test]
    fn vt100_submitted_prompt_survives_first_render_scrollback_round_trip() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript.push(Entry::User { text: "start the task".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 18);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 18).unwrap();

        let contents = vt100_contents(backend.writer(), 80, 18);
        assert!(
            contents.contains("start the task"),
            "vt100 should interpret the scroll-region insert as visible/history content:\n{contents}"
        );
        assert!(
            contents.contains("sending") || contents.contains("submitted"),
            "live chrome should still render after the prompt commit:\n{contents}"
        );
    }

    #[test]
    fn vt100_streaming_tail_moves_above_status_after_stable_prefix_commit() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: "describe the renderer".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 32, 14);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 32, 14).unwrap();

        app.transcript.push(Entry::Assistant {
            text: "stable prefix keeps moving into scrollback while mutable tail stays live".to_string(),
            streaming: true,
        });
        lr.render_frame(&app, &mut backend, 32, 14).unwrap();

        let contents = vt100_contents(backend.writer(), 32, 14);
        let tail_line = contents
            .lines()
            .position(|line| line.contains("tail") || line.contains("live"))
            .expect("streaming tail should be visible in vt100 output");
        let status_line = contents
            .lines()
            .position(|line| line.contains("test-session"))
            .expect("status line should be visible in vt100 output");

        assert!(
            tail_line < status_line,
            "streaming tail should render above the status line after vt100 parsing:\n{contents}"
        );
    }

    #[test]
    fn vt100_resize_replays_committed_rows_without_duplicate_prompt() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: "resize preserves exactly one submitted prompt".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 48, 16);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 48, 16).unwrap();

        app.transcript.push(Entry::Assistant {
            text: "stable response rows should reflow when the terminal changes size".to_string(),
            streaming: false,
        });
        lr.render_frame(&app, &mut backend, 48, 16).unwrap();

        backend.set_size(30, 16);
        lr.render_frame(&app, &mut backend, 30, 16).unwrap();

        backend.set_size(48, 16);
        lr.render_frame(&app, &mut backend, 48, 16).unwrap();

        let contents = vt100_contents(backend.writer(), 48, 16);
        let prompt_count = nonblank_lines(&contents)
            .into_iter()
            .filter(|line| line.contains("resize preserves exactly one"))
            .count();
        assert_eq!(
            prompt_count, 1,
            "prompt should be replayed once after narrow/wide round trip:\n{contents}"
        );
        assert!(
            contents.contains("stable response rows"),
            "committed assistant rows should survive resize replay:\n{contents}"
        );
    }

    #[test]
    fn vt100_resize_replays_startup_banner_with_committed_scrollback() {
        let mut app = test_app();
        app.run_state = RunState::Working;
        app.transcript
            .push(Entry::User { text: "trigger scrollback replay".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 20);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 80, 20).unwrap();

        backend.set_size(40, 20);
        lr.render_frame(&app, &mut backend, 40, 20).unwrap();

        backend.set_size(80, 20);
        lr.render_frame(&app, &mut backend, 80, 20).unwrap();

        let contents = vt100_contents(backend.writer(), 80, 20);
        assert!(
            contents.contains("thndrs  coding agent"),
            "startup banner metadata should be replayed with committed scrollback after resize:\n{contents}"
        );
        assert!(
            contents.contains("trigger scrollback replay"),
            "committed prompt should still be present with replayed banner:\n{contents}"
        );
    }

    #[test]
    fn vt100_resize_keeps_latest_git_statusline_without_duplicates() {
        let mut app = test_app();
        app.git_status = Some(crate::renderer::git::GitStatusSummary {
            branch: Some("main".to_string()),
            added: 1,
            modified: 0,
            deleted: 0,
        });

        let mut backend = TerminalBackend::new(Vec::new(), 100, 18);
        let mut lr = LiveRegion::new();
        lr.render_frame(&app, &mut backend, 100, 18).unwrap();

        app.git_status = Some(crate::renderer::git::GitStatusSummary {
            branch: Some("main".to_string()),
            added: 1,
            modified: 2,
            deleted: 1,
        });
        lr.render_frame(&app, &mut backend, 100, 18).unwrap();

        backend.set_size(72, 18);
        lr.render_frame(&app, &mut backend, 72, 18).unwrap();

        backend.set_size(100, 18);
        lr.render_frame(&app, &mut backend, 100, 18).unwrap();

        let contents = vt100_contents(backend.writer(), 100, 18);
        let git_lines: Vec<&str> = contents.lines().filter(|line| line.contains("git: main")).collect();
        assert_eq!(
            git_lines.len(),
            1,
            "exactly one git statusline should remain after resize replay:\n{contents}"
        );
        assert!(
            git_lines[0].contains("git: main +1 ~2 -1"),
            "latest git summary should survive resize replay:\n{contents}"
        );
    }

    #[test]
    fn reset_clears_state() {
        let mut lr = LiveRegion::new();
        lr.rendered_frame = Some(Frame::new(80));
        lr.rendered_width = Some(80);
        lr.rendered_height = Some(24);

        lr.reset();

        assert!(lr.rendered_frame.is_none());
        assert_eq!(lr.rendered_width, None);
        assert_eq!(lr.rendered_height, None);
    }

    #[test]
    fn resize_reflows_viewport() {
        let mut app = test_app();
        app.input
            .set_text("some prompt text here that should occupy more rows when the viewport narrows");
        let lr = LiveRegion::new();

        let wide = lr.build_frame(&app, 80, 16);
        let narrow = lr.build_frame(&app, 32, 16);

        assert_eq!(wide.len(), 16);
        assert_eq!(narrow.len(), 16);
        assert_ne!(wide.render_text(), narrow.render_text());
        assert!(narrow.cursor.is_some());
    }

    #[test]
    fn snapshot_empty_live_frame() {
        let app = test_app();
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        insta::assert_snapshot!("empty_live_frame", frame.render_styled());
    }

    #[test]
    fn snapshot_streaming_live_frame() {
        let mut app = test_app();
        app.transcript
            .push(Entry::Assistant { text: "streaming response text".to_string(), streaming: true });
        app.input.set_text("follow up");
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        insta::assert_snapshot!("streaming_live_frame", frame.render_styled());
    }

    #[test]
    fn snapshot_narrow_live_frame() {
        let mut app = test_app();
        app.input.set_text("a longer prompt that should wrap at narrow width");
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 40, 20);
        insta::assert_snapshot!("narrow_live_frame", frame.render_styled());
    }

    #[test]
    fn snapshot_startup_banner() {
        let app = test_app();
        let rows = banner_rows(&app, 80);
        let frame = row::Frame { rows, width: 80, cursor: None, cursor_visible: true };
        insta::assert_snapshot!("startup_banner", frame.render_styled());
    }

    #[test]
    fn snapshot_narrow_startup_banner() {
        let app = test_app();
        let rows = banner_rows(&app, 40);
        let frame = row::Frame { rows, width: 40, cursor: None, cursor_visible: true };
        insta::assert_snapshot!("narrow_startup_banner", frame.render_styled());
    }

    #[test]
    fn snapshot_user_message() {
        let entry = Entry::User { text: "Hello, can you help me with this?".to_string() };
        insta::assert_snapshot!("user_message", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_assistant_text() {
        let entry =
            Entry::Assistant { text: "Sure! I can help with that. Let me take a look.".to_string(), streaming: false };
        insta::assert_snapshot!("assistant_text", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_assistant_with_code_fence() {
        let entry = Entry::Assistant {
            text: "````md\nHere is the code:\n\n```rs\nfn main() {\n    println!(\"hello\");\n}\n```\n````".to_string(),
            streaming: false,
        };
        insta::assert_snapshot!("assistant_code_fence", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_reasoning() {
        let entry =
            Entry::Reasoning { text: "I need to check the file structure first.".to_string(), streaming: false };
        insta::assert_snapshot!("reasoning_block", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_tool_ok() {
        let entry = Entry::Tool {
            name: "search_text".to_string(),
            arguments: r#"{"pattern": "fn main", "path": "src/main.rs"}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "src/main.rs:1:fn main() {".to_string(),
                "src/main.rs:2:    println!(\"hello\");".to_string(),
                "src/main.rs:3:}".to_string(),
            ],
        };
        insta::assert_snapshot!("tool_ok", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_tool_failed() {
        let entry = Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"program": "cargo build"}"#.to_string(),
            status: ToolStatus::Failed,
            output: vec![
                "error[E0308]: mismatched types".to_string(),
                "  --> src/main.rs:5:14".to_string(),
                "   |".to_string(),
                "5 |     let x: i32 = \"hello\";".to_string(),
                "   |               ^^^^^^^^".to_string(),
            ],
        };
        insta::assert_snapshot!("tool_failed", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_error_message() {
        let entry = Entry::Error { text: "Provider request failed: connection refused".to_string() };
        insta::assert_snapshot!("error_message", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_rust_compiler_output() {
        let entry = Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"program": "cargo build"}"#.to_string(),
            status: ToolStatus::Failed,
            output: vec![
                "   Compiling thndrs v0.1.0".to_string(),
                "error[E0277]: the trait bound `X: Y` is not satisfied".to_string(),
                "  --> src/lib.rs:42:10".to_string(),
                "   |".to_string(),
                "42 |     fn foo() -> impl Y {".to_string(),
                "   |                    ^^^^^".to_string(),
            ],
        };
        insta::assert_snapshot!("rust_compiler_output", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_json_output() {
        let entry = Entry::Tool {
            name: "read_file_range".to_string(),
            arguments: r#"{"path": "config.json"}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "{".to_string(),
                "  \"name\": \"thndrs\",".to_string(),
                "  \"version\": \"0.1.0\"".to_string(),
                "}".to_string(),
            ],
        };
        insta::assert_snapshot!("json_output", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_plain_prose() {
        let entry = Entry::Assistant {
            text: "This is a plain prose response without any code or special formatting. It should wrap nicely across multiple lines when the terminal is narrow enough.".to_string(),
            streaming: false,
        };
        insta::assert_snapshot!("plain_prose", render_entry_styled(&entry, 60));
    }

    #[test]
    fn snapshot_diff_output() {
        let entry = Entry::Tool {
            name: "replace_range".to_string(),
            arguments: r#"{"path": "src/main.rs"}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "--- src/main.rs".to_string(),
                "+++ src/main.rs".to_string(),
                "@@ -1,3 +1,3 @@".to_string(),
                " fn main() {".to_string(),
                "-    println!(\"old\");".to_string(),
                "+    println!(\"new\");".to_string(),
                " }".to_string(),
            ],
        };
        insta::assert_snapshot!("diff_output", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_tool_with_truncated_output() {
        let entry = Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"program": "ls"}"#.to_string(),
            status: ToolStatus::Ok,
            output: (0..20).map(|i| format!("file_{i}.rs")).collect(),
        };
        insta::assert_snapshot!("tool_truncated_output", render_entry_styled(&entry, 80));
    }

    #[test]
    fn snapshot_status_entry() {
        let entry = Entry::Status { text: "context  AGENTS.md (scope: .)".to_string() };
        insta::assert_snapshot!("status_entry", render_entry_styled(&entry, 80));
    }

    #[test]
    fn plain_status_entries_render_as_system() {
        let entry = Entry::Status { text: "manual status".to_string() };
        let rendered = render_entry_styled(&entry, 80);

        assert!(
            rendered.contains("System"),
            "plain status label should be System:\n{rendered}"
        );
        assert!(
            !rendered.contains("Notice"),
            "plain status label should not be Notice:\n{rendered}"
        );
    }

    #[test]
    fn tool_output_shortens_workspace_absolute_paths() {
        let cwd = Path::new("/Users/owais/Projects/StormlightLabs/OpenSource/thndrs");
        let entry = Entry::Tool {
            name: "search_text".to_string(),
            arguments: r#"{"pattern":"Entry::Status"}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec![
                "/Users/owais/Projects/StormlightLabs/OpenSource/thndrs/src/session/tests.rs:1420: Entry::Status"
                    .to_string(),
            ],
        };
        let rows = entry_to_rows(&entry, "User", 120, cwd);
        let frame = row::Frame { rows, width: 120, cursor: None, cursor_visible: true };
        let rendered = frame.render_text();

        assert!(
            rendered.contains("src/session/tests.rs:1420:"),
            "path should be project-relative:\n{rendered}"
        );
        assert!(
            !rendered.contains("/Users/owais/Projects/StormlightLabs/OpenSource/thndrs"),
            "workspace prefix should be hidden:\n{rendered}"
        );
    }

    #[test]
    fn snapshot_streaming_tool_with_output() {
        let mut app = test_app();
        app.transcript.push(Entry::Tool {
            name: "run_shell".to_string(),
            arguments: r#"{"program": "cargo test"}"#.to_string(),
            status: ToolStatus::Running,
            output: vec![
                "running 3 tests".to_string(),
                "test tests::foo ... ok".to_string(),
                "test tests::bar ... ok".to_string(),
            ],
        });
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        insta::assert_snapshot!("streaming_tool_with_output", frame.render_styled());
    }
}
