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
//!
//! Transcript row construction lives in [`super::transcript`]; the pure view
//! projection built from app state lives in [`super::view`]. This module keeps
//! viewport policy, scrollback commit bookkeeping, and width epochs.

use std::io;

use crate::app::App;
use crate::renderer::backend::TerminalBackend;
use crate::renderer::row::{Frame, Row};
use crate::renderer::style::{self, CellStyle, Color};
use crate::renderer::view::{self, RendererView};

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

        let view = view::build_view(app, width, height);
        let live = self.build_live_frame(&view);
        let live_height = live.rows.len().min(height);

        let history_rows =
            if app.transcript.is_empty() { view.transcript.banner_rows } else { view.transcript.stable_rows };
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

    /// Build bottom-anchored live prompt/status rows from the view projection.
    fn build_live_frame(&self, view: &RendererView) -> Frame {
        let width = view.width;
        let live = &view.live;
        let mut frame = Frame::new(width);

        for row in live.live_tail.clone() {
            frame.push(row);
        }

        frame.push(Row::blank(width, bg_style(style::palette().surface0)));
        frame.push(live.dynamic_status.clone());

        let prompt_offset = frame.len();
        for row in live.prompt_rows.clone() {
            frame.push(row);
        }

        if let Some(mut c) = live.prompt_cursor {
            c.row += prompt_offset;
            frame.set_cursor(c);
        }

        for row in live.accessory_rows.clone() {
            frame.push(row);
        }

        frame.push(live.static_status.clone());
        frame.push(Row::blank(width, bg_style(style::palette().surface0)));
        frame
    }

    /// Render the live-region viewport, committing stable transcript rows to
    /// terminal scrollback first.
    ///
    /// 1. Build the pure view projection from app state.
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

        let view = view::build_view(app, width, height);
        if self.committed_row_count > view.transcript.stable_rows.len() {
            backend.clear_all()?;
            self.rendered_frame = None;
            self.committed_row_count = 0;
        }

        let rows_to_commit = &view.transcript.stable_rows[self.committed_row_count..];
        if !rows_to_commit.is_empty() {
            backend.insert_history_lines(rows_to_commit, height as u16)?;
            self.rendered_frame = None;
            self.committed_row_count = view.transcript.stable_rows.len();
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

/// Clip the mutable transcript tail to a reasonable share of the viewport.
/// Build a [`CellStyle`] with only a background color.
fn bg_style(color: Color) -> CellStyle {
    CellStyle::new().bg(color)
}

#[cfg(test)]
mod tests;
