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
mod tests;
