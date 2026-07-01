//! Live region state for the direct renderer.
//!
//! [`LiveRegion`] tracks which transcript entries have been committed into
//! native terminal scrollback and builds the [`Frame`] for the live region
//! that is cleared and redrawn each tick.
//!
//! The live region contains, from top to bottom:
//! 1. optional active streaming block;
//! 2. dynamic status row (session + status icon);
//! 3. prompt input rows;
//! 4. optional accessory rows (help/commands/files);
//! 5. static status row (model/search/tokens/cwd).
//!
//! Committed transcript content is printed once into scrollback and never
//! redrawn. Only the live region is cleared and rewritten. This matches the
//! two-zone layout contract from the v0 spec.

#![allow(dead_code)]

use std::io;

use crate::app::{App, Entry, ToolStatus};
use crate::renderer::backend::TerminalBackend;
use crate::renderer::live as rows;
use crate::renderer::row::{CursorCoord, Frame, Row};
use crate::renderer::style::{CellStyle, Color, Span};
use crate::ui::style as ui_style;

/// Maximum rows the prompt input can occupy before scrolling within the live
/// region.
const MAX_PROMPT_ROWS: usize = 8;

/// Maximum accessory rows (help/commands/files) shown in the live region.
const MAX_ACCESSORY_ROWS: usize = 8;

/// State tracking which transcript entries have been committed to scrollback
/// and how many rows the last live-region render used.
#[derive(Debug)]
pub struct LiveRegion {
    /// Number of transcript entries already written to scrollback.
    committed_count: usize,
    /// Terminal width at the last render. When it changes, scrollback is
    /// invalidated and all stable entries are re-committed.
    rendered_width: Option<usize>,
    /// Number of rows the last live frame used. Used to clear before redraw.
    last_frame_rows: usize,
    /// Whether the banner has been emitted to scrollback.
    emitted_banner: bool,
}

impl Default for LiveRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveRegion {
    /// Create a fresh live region with nothing committed.
    pub fn new() -> Self {
        LiveRegion { committed_count: 0, rendered_width: None, last_frame_rows: 0, emitted_banner: false }
    }

    /// Commit newly-stable transcript entries into native scrollback.
    ///
    /// Stable entries are those that will not change: completed assistant text,
    /// completed reasoning, finished tools, user messages, status, and errors.
    /// Streaming entries stay in the live region until they complete.
    ///
    /// On width change, the entire committed set is invalidated and re-emitted
    /// at the new width.
    pub fn commit_transcript<W: io::Write>(
        &mut self, app: &App, backend: &mut TerminalBackend<W>, width: usize,
    ) -> io::Result<()> {
        if self.rendered_width.is_some_and(|w| w != width) {
            self.committed_count = 0;
            self.emitted_banner = false;
        }
        self.rendered_width = Some(width);

        if !self.emitted_banner {
            let banner = banner_rows(app, width);
            if !banner.is_empty() {
                backend.write_committed(&banner)?;
            }
            self.emitted_banner = true;
        }

        if self.committed_count > app.transcript.len() {
            self.committed_count = 0;
        }

        let start = self.committed_count;
        let stable_end = app.transcript[start..]
            .iter()
            .take_while(|e| entry_is_stable(e))
            .count()
            + start;

        if stable_end <= start {
            return Ok(());
        }

        let committed = transcript_rows(&app.transcript[start..stable_end], &app.user_label, width);
        if !committed.is_empty() {
            backend.write_committed(&committed)?;
        }
        self.committed_count = stable_end;
        Ok(())
    }

    /// Build the live-region frame from app state.
    ///
    /// The frame contains active streaming rows, dynamic status, prompt input,
    /// accessories, and static status — everything that is cleared and redrawn
    /// each tick.
    pub fn build_frame(&self, app: &App, width: usize, height: usize) -> Frame {
        let mut frame = Frame::new(width);
        let streaming = rows::active_streaming_rows(app, width);

        frame.rows.extend(streaming);
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

    /// Render the live region: clear the previous frame, write the new frame,
    /// and place the cursor.
    ///
    /// `top_row` is the 0-based row where the live region starts (typically
    /// `terminal_height - frame_height`).
    pub fn render_frame<W: io::Write>(
        &mut self, app: &App, backend: &mut TerminalBackend<W>, width: usize, height: usize,
    ) -> io::Result<()> {
        let frame = self.build_frame(app, width, height);
        let frame_height = frame.len();
        let top_row = height.saturating_sub(frame_height) as u16;

        if self.last_frame_rows > frame_height {
            backend.clear_rows(top_row, self.last_frame_rows as u16)?;
        }

        backend.render_frame(&frame, top_row)?;
        self.last_frame_rows = frame_height;
        Ok(())
    }

    /// Number of transcript entries committed to scrollback.
    pub fn committed_count(&self) -> usize {
        self.committed_count
    }

    /// Whether the banner has been emitted.
    pub fn banner_emitted(&self) -> bool {
        self.emitted_banner
    }

    /// Reset all committed state (e.g. on `/clear`).
    pub fn reset(&mut self) {
        self.committed_count = 0;
        self.emitted_banner = false;
        self.last_frame_rows = 0;
    }
}

/// Determine whether a transcript entry is stable (will not change further).
fn entry_is_stable(entry: &Entry) -> bool {
    match entry {
        Entry::Assistant { streaming, .. } | Entry::Reasoning { streaming, .. } => !streaming,
        Entry::Tool { status, .. } => *status != ToolStatus::Running,
        Entry::User { .. } | Entry::Status { .. } | Entry::Error { .. } => true,
    }
}

/// Build banner rows for scrollback.
fn banner_rows(app: &App, width: usize) -> Vec<Row> {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let title_style = CellStyle::new().fg(ratatui_color(p.accent)).bg(bg).bold();

    let mut rows = Vec::new();
    rows.push(Row::blank(width, bg_style(bg)));

    let title = format!("thndrs — {}", app.model);
    rows.push(Row::padded(vec![Span::styled(title, title_style)], width, bg_style(bg)));

    if !app.cwd.as_os_str().is_empty() {
        let cwd_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg);
        rows.push(Row::padded(
            vec![Span::styled(format!("cwd: {}", app.cwd.display()), cwd_style)],
            width,
            bg_style(bg),
        ));
    }

    rows.push(Row::blank(width, bg_style(bg)));
    rows
}

/// Build committed transcript rows from entries.
fn transcript_rows(entries: &[Entry], user_label: &str, width: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 && is_group_boundary(&entries[i - 1], entry) {
            rows.push(Row::blank(width, CellStyle::default()));
        }
        rows.extend(entry_to_rows(entry, user_label, width));
    }
    rows
}

/// Map a Ratatui Color to a renderer Color (re-exported for this module).
fn ratatui_color(c: ratatui::style::Color) -> Color {
    match c {
        ratatui::style::Color::Reset => Color::Reset,
        ratatui::style::Color::Black => Color::Black,
        ratatui::style::Color::Red => Color::DarkRed,
        ratatui::style::Color::Green => Color::DarkGreen,
        ratatui::style::Color::Yellow => Color::DarkYellow,
        ratatui::style::Color::Blue => Color::DarkBlue,
        ratatui::style::Color::Magenta => Color::DarkMagenta,
        ratatui::style::Color::Cyan => Color::DarkCyan,
        ratatui::style::Color::Gray => Color::Grey,
        ratatui::style::Color::DarkGray => Color::DarkGrey,
        ratatui::style::Color::LightRed => Color::Red,
        ratatui::style::Color::LightGreen => Color::Green,
        ratatui::style::Color::LightYellow => Color::Yellow,
        ratatui::style::Color::LightBlue => Color::Blue,
        ratatui::style::Color::LightMagenta => Color::Magenta,
        ratatui::style::Color::LightCyan => Color::Cyan,
        ratatui::style::Color::White => Color::White,
        ratatui::style::Color::Rgb(r, g, b) => Color::Rgb { r, g, b },
        ratatui::style::Color::Indexed(i) => {
            Color::Rgb { r: ((i >> 5) & 0x7) * 36, g: ((i >> 2) & 0x7) * 36, b: (i & 0x3) * 85 }
        }
    }
}

/// Build a [`CellStyle`] with only a background color.
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

/// Convert a single transcript entry to padded rows for scrollback.
fn entry_to_rows(entry: &Entry, user_label: &str, width: usize) -> Vec<Row> {
    let p = ui_style::palette();
    let bg = ratatui_color(p.surface0);
    let body_width = crate::renderer::layout::content_width(width);

    match entry {
        Entry::User { text } => {
            let surface1 = ratatui_color(p.surface1);
            let label_style = CellStyle::new().fg(ratatui_color(p.blue)).bg(surface1).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.text)).bg(surface1);
            build_labeled_block(user_label, label_style, text_style, text, width, body_width, surface1)
        }
        Entry::Assistant { text, .. } => {
            let label_style = CellStyle::new().fg(ratatui_color(p.green)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg);
            build_labeled_block("Assistant", label_style, text_style, text, width, body_width, bg)
        }
        Entry::Reasoning { text, .. } => {
            let label_style = CellStyle::new().fg(ratatui_color(p.mauve)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.subtext0)).bg(bg).italic();
            build_labeled_block("Thinking ✓", label_style, text_style, text, width, body_width, bg)
        }
        Entry::Tool { name, status, .. } => {
            let (status_label, status_color, icon) = match status {
                ToolStatus::Running => ("running", ratatui_color(p.yellow), "·"),
                ToolStatus::Ok => ("ok", ratatui_color(p.green), "✓"),
                ToolStatus::Failed => ("failed", ratatui_color(p.red), "✕"),
            };
            let header_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg).bold();
            let status_style = CellStyle::new().fg(status_color).bg(bg);
            vec![
                Row::blank(width, bg_style(bg)),
                Row::padded(
                    vec![
                        Span::styled(format!("{icon} "), status_style),
                        Span::styled(name.to_string(), header_style),
                        Span::styled(format!(" [{status_label}]"), status_style),
                    ],
                    width,
                    bg_style(bg),
                ),
                Row::blank(width, bg_style(bg)),
            ]
        }
        Entry::Status { text } => {
            let label_style = CellStyle::new().fg(ratatui_color(p.overlay1)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg);
            build_labeled_block("Notice", label_style, text_style, text, width, body_width, bg)
        }
        Entry::Error { text } => {
            let label_style = CellStyle::new().fg(ratatui_color(p.red)).bg(bg).bold();
            let text_style = CellStyle::new().fg(ratatui_color(p.text)).bg(bg);
            build_labeled_block("⚠ Error", label_style, text_style, text, width, body_width, bg)
        }
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
        assert_eq!(lr.committed_count(), 0);
        assert!(!lr.banner_emitted());
    }

    #[test]
    fn commit_emits_banner_first() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert!(lr.banner_emitted(), "banner should be emitted on first commit");

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("thndrs"), "banner should contain app name");
        assert!(out.contains("test-model"), "banner should contain model");
    }

    #[test]
    fn commit_writes_stable_entries_to_scrollback() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "hi there".to_string(), streaming: false });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 2, "both stable entries should be committed");

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("hello"), "user text should be in scrollback");
        assert!(out.contains("hi there"), "assistant text should be in scrollback");
    }

    #[test]
    fn commit_skips_streaming_entries() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "thinking...".to_string(), streaming: true });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 1, "streaming entry should not be committed");

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("hello"), "user text should be in scrollback");
        assert!(
            !out.contains("thinking..."),
            "streaming text should NOT be in scrollback"
        );
    }

    #[test]
    fn commit_completes_streaming_when_it_finishes() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "thinking...".to_string(), streaming: true });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 1);

        if let Entry::Assistant { streaming, .. } = app.transcript.last_mut().unwrap() {
            *streaming = false;
        }

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 2, "finished entry should be committed");

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("thinking..."), "now-stable text should be in scrollback");
    }

    #[test]
    fn width_change_invalidates_committed() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 1);

        lr.commit_transcript(&app, &mut backend, 120).unwrap();
        assert_eq!(lr.committed_count(), 1, "entry should be re-committed at new width");
    }

    #[test]
    fn transcript_shrink_resets_committed() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "hi".to_string(), streaming: false });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 2);

        app.transcript.clear();
        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 0, "committed count should reset on shrink");
    }

    #[test]
    fn build_frame_has_status_and_prompt() {
        let app = test_app();
        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        assert!(!frame.is_empty(), "frame should have rows");
        assert!(frame.len() >= 3, "frame should have status, prompt, and footer");
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
    fn build_frame_cursor_set_for_editable_prompt() {
        let mut app = test_app();
        app.input.set_text("hello");

        let lr = LiveRegion::new();
        let frame = lr.build_frame(&app, 80, 24);
        assert!(frame.cursor.is_some(), "cursor should be set for editable prompt");
    }

    #[test]
    fn render_frame_clears_and_wrows() {
        let app = test_app();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("\x1b[2K"), "should clear live rows");
    }

    #[test]
    fn reset_clears_state() {
        let mut lr = LiveRegion::new();
        lr.committed_count = 5;
        lr.emitted_banner = true;
        lr.last_frame_rows = 10;

        lr.reset();

        assert_eq!(lr.committed_count(), 0);
        assert!(!lr.banner_emitted());
    }

    #[test]
    fn committed_output_is_scrollback_friendly() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "test".to_string() });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.commit_transcript(&app, &mut backend, 80).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains('\n'), "committed output must have newlines for scrollback");
        assert!(!out.is_empty(), "should have committed output");
    }

    #[test]
    fn committed_rows_do_not_contain_clear_sequences() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "world".to_string(), streaming: false });

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();
        lr.commit_transcript(&app, &mut backend, 80).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(!out.contains("\x1b[2K"), "committed rows must not clear lines");
    }

    #[test]
    fn full_render_cycle_commit_then_frame() {
        let mut app = test_app();
        app.transcript.push(Entry::User { text: "hello".to_string() });
        app.transcript
            .push(Entry::Assistant { text: "hi".to_string(), streaming: false });
        app.input.set_text("next question");

        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.commit_transcript(&app, &mut backend, 80).unwrap();
        assert_eq!(lr.committed_count(), 2);

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();

        let out = String::from_utf8(backend.writer().clone()).unwrap();
        assert!(out.contains("hello"), "committed user text should be present");
        assert!(out.contains("hi"), "committed assistant text should be present");
        assert!(out.contains("next question"), "live prompt should be present");
    }

    #[test]
    fn resize_reflows_live_frame() {
        let mut app = test_app();
        app.input.set_text("some prompt text here");
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        let mut lr = LiveRegion::new();

        lr.render_frame(&app, &mut backend, 80, 24).unwrap();
        let first_frame_rows = lr.last_frame_rows;

        backend.set_size(40, 24);
        lr.render_frame(&app, &mut backend, 40, 24).unwrap();

        let second_frame_rows = lr.last_frame_rows;
        assert!(
            second_frame_rows >= first_frame_rows,
            "narrower width should produce at least as many rows"
        );
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
}
