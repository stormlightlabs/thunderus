//! Live region state for the direct renderer.
//!
//! [`LiveRegion`] builds one logical terminal viewport from history rows and
//! live prompt chrome, then redraws that bounded viewport each tick.
//!
//! The viewport contains, from top to bottom:
//! 1. banner and transcript history clipped to the available history area;
//! 2. blank canvas rows if history does not fill that area;
//! 3. dynamic status row (session + status icon);
//! 4. prompt input rows;
//! 5. optional accessory rows (help/commands/files);
//! 6. static status row (model/search/tokens/cwd).
//!
//! This mirrors the Codex/Pi/Goose pattern: every render owns a bounded width
//! and height, rebuilds rows for the current terminal size, and clips/pads
//! before writing to the terminal.

use std::io;

use crate::app::{App, Entry, ToolStatus};
use crate::renderer::backend::TerminalBackend;
use crate::renderer::live as rows;
use crate::renderer::row::{Frame, Row};
use crate::renderer::style as renderer_style;
use crate::renderer::style::{CellStyle, Color, Span};

/// Maximum rows the prompt input can occupy before scrolling within the live
/// region.
const MAX_PROMPT_ROWS: usize = 8;

/// Maximum accessory rows (help/commands/files) shown in the live region.
const MAX_ACCESSORY_ROWS: usize = 8;

/// State tracking the last viewport render.
#[derive(Debug)]
pub struct LiveRegion {
    /// The last frame rendered to the screen, used for diff-based rendering.
    rendered_frame: Option<Frame>,
    /// Terminal width at the last render.
    rendered_width: Option<usize>,
    /// Terminal height at the last render.
    rendered_height: Option<usize>,
}

impl Default for LiveRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRegion {
    /// Create a fresh live region with nothing committed.
    pub fn new() -> Self {
        LiveRegion { rendered_frame: None, rendered_width: None, rendered_height: None }
    }

    /// Build the full terminal viewport from app state.
    ///
    /// The frame is exactly `height` rows tall when `height > 0`. History rows
    /// are clipped from the bottom when they overflow their area, while the
    /// live prompt/status rows stay anchored at the bottom.
    pub fn build_frame(&self, app: &App, width: usize, height: usize) -> Frame {
        let mut frame = Frame::new(width);
        if width == 0 || height == 0 {
            return frame;
        }

        let live = self.build_live_frame(app, width, height);
        let live_height = live.rows.len().min(height);
        let history_height = height.saturating_sub(live_height);

        let mut history_rows = banner_rows(app, width);
        history_rows.extend(transcript_rows(&app.transcript, &app.user_label, width));
        let history_start = history_rows.len().saturating_sub(history_height);
        let visible_history = &history_rows[history_start..];

        frame.rows.extend(visible_history.iter().cloned());

        let p = renderer_style::palette();
        while frame.rows.len() < history_height {
            frame.push(Row::blank(width, bg_style(p.panel_bg)));
        }

        let live_start = live.rows.len().saturating_sub(live_height);
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

        while frame.rows.len() < height {
            frame.push(Row::blank(width, bg_style(p.panel_bg)));
        }

        frame
    }

    /// Build bottom-anchored live prompt/status rows.
    fn build_live_frame(&self, app: &App, width: usize, height: usize) -> Frame {
        let mut frame = Frame::new(width);
        frame.push(rows::dynamic_status_row(app, width));
        let (prompt_rows, cursor) = rows::prompt_rows_for(app, width);
        let prompt_count = prompt_rows.len().min(MAX_PROMPT_ROWS);
        let remaining_after_prompt = height.saturating_sub(frame.len() + prompt_count + 1);
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

        frame
    }

    /// Render the full logical viewport.
    ///
    /// Uses diff-based rendering: only rows that differ from the previous
    /// frame are written to the terminal. This eliminates full-screen
    /// redraws on every tick, which was a primary cause of flickering.
    pub fn render_frame<W: io::Write>(
        &mut self, app: &App, backend: &mut TerminalBackend<W>, width: usize, height: usize,
    ) -> io::Result<()> {
        let frame = self.build_frame(app, width, height);

        // If the terminal size changed since the last render, we cannot rely
        // on the diff — the old frame's rows may have different content at
        // different positions. Fall back to a full redraw.
        let size_changed = self.rendered_width != Some(width) || self.rendered_height != Some(height);

        if size_changed {
            backend.render_frame(&frame, 0)?;
        } else {
            backend.render_frame_diff(&frame, self.rendered_frame.as_ref(), 0)?;
        }

        self.rendered_frame = Some(frame);
        self.rendered_width = Some(width);
        self.rendered_height = Some(height);
        Ok(())
    }

    /// Reset all committed state (e.g. on `/clear`).
    pub fn reset(&mut self) {
        self.rendered_frame = None;
        self.rendered_width = None;
        self.rendered_height = None;
    }
}

/// Build banner rows for scrollback.
fn banner_rows(app: &App, width: usize) -> Vec<Row> {
    let p = renderer_style::palette();
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
        push_wrapped_banner_row(&mut rows, vec![Span::styled(line, title_style)], width, bg);
    }
    if banner_is_art {
        push_wrapped_banner_row(
            &mut rows,
            vec![Span::styled(
                "─".repeat(width.saturating_sub(4)),
                CellStyle::new().fg(p.overlay0).bg(bg),
            )],
            width,
            bg,
        );
    }

    let title = String::from("thndrs  coding agent");
    push_wrapped_banner_row(&mut rows, vec![Span::styled(title, title_style)], width, bg);
    push_wrapped_banner_row(
        &mut rows,
        vec![
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
            vec![Span::styled(format!("cwd: {}", app.cwd.display()), muted_style)],
            width,
            bg,
        );
    }

    push_wrapped_banner_row(
        &mut rows,
        vec![
            Span::styled("›  ", title_style),
            Span::styled("Ask for a change, run a command, or inspect this repo.", muted_style),
        ],
        width,
        bg,
    );
    push_wrapped_banner_row(
        &mut rows,
        vec![
            Span::styled("?  ", title_style),
            Span::styled("help", muted_style),
            Span::styled("   Ctrl+P ", title_style),
            Span::styled("files", muted_style),
        ],
        width,
        bg,
    );

    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

fn push_wrapped_banner_row(rows: &mut Vec<Row>, spans: Vec<Span>, width: usize, bg: Color) {
    let body_width = crate::renderer::layout::content_width(width);
    for line in crate::renderer::layout::wrap_spans(&spans, body_width) {
        rows.push(Row::padded(line, width, bg_style(bg)));
    }
}

/// Build committed transcript rows from entries.
fn transcript_rows(entries: &[Entry], user_label: &str, width: usize) -> Vec<Row> {
    let p = renderer_style::palette();
    let mut rows = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 && is_group_boundary(&entries[i - 1], entry) {
            rows.push(Row::blank(width, bg_style(p.surface_dim)));
        }
        rows.extend(entry_to_rows(entry, user_label, width));
    }
    rows
}

/// Build a [`CellStyle`] with only a background color.
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

/// Maximum tool output lines rendered before a truncation marker is shown.
const MAX_TOOL_OUTPUT_LINES: usize = 6;

/// Gutter prefix for tool output lines.
const GUTTER: &str = "   │ ";

/// Convert a single transcript entry to padded rows for scrollback.
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize) -> Vec<Row> {
    let p = renderer_style::palette();
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
            tool_block_rows(name, arguments, *status, output, width, body_width, bg)
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
    let p = renderer_style::palette();
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
        for line in crate::renderer::layout::wrap_text(text, body_width) {
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
    rows.push(Row::blank(width, bg_style(bg)));
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
    let p = renderer_style::palette();
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
            for wrapped in crate::renderer::layout::wrap_text(line, body_width) {
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
        let highlighted = crate::renderer::highlight::highlight_lines(&code_buf, lang);
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

/// Build a tool block: header row + args summary + output lines + vertical
/// padding.
fn tool_block_rows(
    name: &str, args: &str, status: ToolStatus, output: &[String], width: usize, body_width: usize, bg: Color,
) -> Vec<Row> {
    let p = renderer_style::palette();
    let (status_label, status_color, icon) = match status {
        ToolStatus::Running => ("running", p.peach, "·"),
        ToolStatus::Ok => ("ok", p.green, "✓"),
        ToolStatus::Failed => ("failed", p.red, "✕"),
    };
    let header_style = CellStyle::new().fg(p.text).bg(bg).bold();
    let status_style = CellStyle::new().fg(status_color).bg(bg);
    let muted_style = CellStyle::new().fg(p.subtext0).bg(bg);
    let gutter_style = CellStyle::new().fg(p.overlay0).bg(bg);

    let args_summary = summarize_tool_args(args);
    let base_name = name.split('#').next().unwrap_or(name);
    let lang = crate::renderer::highlight::tool_output_language(base_name, args);

    let mut rows = vec![Row::blank(width, bg_style(bg))];

    let mut header_spans = vec![
        Span::styled(format!("{icon} "), status_style),
        Span::styled(name.to_string(), header_style),
        Span::styled(format!(" [{status_label}]"), status_style),
    ];

    if !args_summary.is_empty() {
        let header_width: usize = header_spans.iter().map(|s| s.text.chars().count()).sum();
        if header_width + 2 + args_summary.chars().count() <= body_width {
            header_spans.push(Span::styled("  ", CellStyle::new().bg(bg)));
            header_spans.push(Span::styled(args_summary, muted_style));
            rows.push(Row::padded(header_spans, width, bg_style(bg)));
        } else {
            rows.push(Row::padded(header_spans, width, bg_style(bg)));
            for wrapped in crate::renderer::layout::wrap_text(&args_summary, body_width.saturating_sub(2)) {
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
                .map(|l| format!("{l}\n"))
                .collect();
            let highlighted = crate::renderer::highlight::highlight_lines(&joined, Some(lang_str));
            for hl_row in highlighted {
                let mut spans = vec![Span::styled(GUTTER, gutter_style)];
                spans.extend(hl_row.into_iter().map(|s| Span { text: s.text, style: s.style.bg(bg) }));
                rows.push(Row::padded(spans, width, bg_style(bg)));
            }
        }
        None => {
            for line in output.iter().take(MAX_TOOL_OUTPUT_LINES) {
                let content_style = if is_section_header(line) {
                    CellStyle::new().fg(p.overlay1).bg(bg).bold()
                } else {
                    CellStyle::new().fg(p.subtext0).bg(bg)
                };
                for wrapped in
                    crate::renderer::layout::wrap_text(line, body_width.saturating_sub(GUTTER.chars().count()))
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
                format!("   │ …({} more lines)", output.len() - MAX_TOOL_OUTPUT_LINES),
                muted_style,
            )],
            width,
            bg_style(bg),
        ));
    }

    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

/// Detect whether a tool output line is a section header.
fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("── ") || trimmed.starts_with("$ ")
}

/// Produce a short summary of a tool's arguments for the transcript line.
fn summarize_tool_args(arguments: &str) -> String {
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
            return format!("{}: {}", key, crate::utils::truncate_ellipsis(val, 40));
        }
    }
    for (k, val) in obj {
        if let Some(s) = val.as_str() {
            return format!("{k}: {}", crate::utils::truncate_ellipsis(s, 40));
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
        "Notice"
    }
}

/// Build a labeled text block: blank padding row, label row, wrapped text rows,
/// blank padding row.
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
    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

/// Semantic group classification for transcript spacing.
fn entry_group(entry: &Entry) -> u8 {
    match entry {
        Entry::User { .. } => 0,
        Entry::Assistant { .. } => 1,
        Entry::Reasoning { .. } => 2,
        Entry::Tool { .. } => 3,
        Entry::Status { .. } | Entry::Error { .. } => 4,
    }
}

fn is_group_boundary(prev: &Entry, curr: &Entry) -> bool {
    let prev_group = entry_group(prev);
    let curr_group = entry_group(curr);
    if prev_group == 4 || curr_group == 4 { false } else { prev_group != curr_group }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Entry, ToolStatus};
    use crate::cli::{Cli, Theme, WebSearchMode};
    use std::path::PathBuf;

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
    fn build_frame_contains_banner_history_and_prompt() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "hi there".to_string(), streaming: false });
        app.input.set_text("next question");

        let frame = LiveRegion::new().build_frame(&app, 80, 24);
        let combined = frame.render_text();

        assert!(combined.contains("thndrs"), "banner should be part of the viewport");
        assert!(combined.contains("hello"), "user text should be part of the viewport");
        assert!(
            combined.contains("hi there"),
            "assistant text should be part of the viewport"
        );
        assert!(
            combined.contains("next question"),
            "prompt should be part of the viewport"
        );
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
        assert!(frame.rows[frame.len() - 1].text().contains("model:"));
        assert!(frame.rows[frame.len() - 2].text().contains("hello"));
        assert!(frame.rows[frame.len() - 3].text().contains("test-session"));
    }

    #[test]
    fn build_frame_clips_old_history_first() {
        let mut app = test_app();
        for i in 0..12 {
            app.transcript.push(Entry::User { text: format!("message-{i}") });
        }

        let frame = LiveRegion::new().build_frame(&app, 80, 10);
        let combined = frame.render_text();

        assert_eq!(frame.len(), 10);
        assert!(!combined.contains("message-0"), "old history should be clipped first");
        assert!(combined.contains("message-11"), "latest history should remain visible");
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
        assert!(lr.rendered_frame.is_some(), "should store the rendered frame for diffing");
    }

    #[test]
    fn render_frame_diff_skips_unchanged_rows() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        // First render: full write (no previous frame to diff against).
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let first_output_len = String::from_utf8(backend.writer().clone()).unwrap().len();

        // Second render with identical state: diff should produce no row writes.
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let second_new_bytes = second_output.len() - first_output_len;

        assert_eq!(
            second_new_bytes, 0,
            "identical frame should produce no output, got: {:?}",
            &second_output[first_output_len..]
        );
    }

    #[test]
    fn render_frame_diff_writes_only_changed_rows() {
        let mut app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        // First render.
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        // Change app state so at least one row differs.
        app.ui_tick = app.ui_tick.wrapping_add(1);
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        // The second render should have produced some output (the changed row).
        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(
            !out.is_empty(),
            "changed frame should produce output"
        );
    }

    #[test]
    fn render_frame_full_redraw_on_resize() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        // Initial render at 80x24.
        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

        // Resize: should trigger a full redraw, not a diff.
        lr.render_frame(&app, &mut backend, 100, 30).unwrap();
        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let second_new_bytes = second_output.len() - first_len;

        assert!(
            second_new_bytes > 0,
            "resize should trigger a full redraw with output"
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

    fn render_entry_styled(entry: &Entry, width: usize) -> String {
        let rows = entry_to_rows(entry, "User", width);
        let frame = crate::renderer::row::Frame { rows, width, cursor: None };
        frame.render_styled()
    }

    #[test]
    fn snapshot_startup_banner() {
        let app = test_app();
        let rows = banner_rows(&app, 80);
        let frame = crate::renderer::row::Frame { rows, width: 80, cursor: None };
        insta::assert_snapshot!("startup_banner", frame.render_styled());
    }

    #[test]
    fn snapshot_narrow_startup_banner() {
        let app = test_app();
        let rows = banner_rows(&app, 40);
        let frame = crate::renderer::row::Frame { rows, width: 40, cursor: None };
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
