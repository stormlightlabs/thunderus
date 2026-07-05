//! Pure renderer view projection.
//!
//! [`RendererView`] is a data-only staging area built from [`App`] plus
//! terminal dimensions. It contains no crossterm types and performs no terminal
//! writes. The view separates semantic row construction from viewport policy so
//! that [`super::region::LiveRegion`] can focus on scrollback commits, width
//! epochs, and frame composition.

use crate::app::{App, Entry, ToolStatus};
use crate::renderer::row::{CursorCoord, Row};

/// Complete view of what the renderer should draw this tick.
pub struct RendererView {
    pub transcript: TranscriptView,
    pub live: LiveView,
    pub width: usize,
    #[allow(dead_code)]
    pub height: usize,
}

/// Transcript portion of the view: committed banner rows, stable rows, and rows
/// that are still mutable (streaming or running).
pub struct TranscriptView {
    /// Banner rows shown before the first transcript entry is committed.
    pub banner_rows: Vec<Row>,
    /// Rows that can be safely committed to native scrollback.
    pub stable_rows: Vec<Row>,
    /// Rows that must remain in the live viewport until the entry settles.
    pub live_rows: Vec<Row>,
}

/// Live chrome portion of the view: prompt, status, and accessory rows.
pub struct LiveView {
    /// Clipped mutable transcript tail rows.
    pub live_tail: Vec<Row>,
    /// Dynamic status row above the prompt.
    pub dynamic_status: Row,
    /// Prompt input rows.
    pub prompt_rows: Vec<Row>,
    /// Cursor coordinate relative to the first prompt row.
    pub prompt_cursor: Option<CursorCoord>,
    /// Optional accessory rows (help, commands, file picker, etc.).
    pub accessory_rows: Vec<Row>,
    /// Summary row for queued steering/follow-up prompts, shown when non-empty.
    pub queued_summary: Option<Row>,
    /// Scrollable detail pane rows for the expanded tool entry.
    pub detail_pane: Vec<Row>,
    /// Static status row below the prompt.
    pub static_status: Row,
}

/// Build a pure data view from app state and terminal dimensions.
///
/// No crossterm types or terminal writes appear here. The returned view is the
/// input to [`super::region::LiveRegion::build_frame`].
pub fn build_view(app: &App, width: usize, height: usize) -> RendererView {
    let transcript = build_transcript_view(app, width);
    let live = build_live_view(app, width, height, &transcript);

    RendererView { transcript, live, width, height }
}

fn build_transcript_view(app: &App, width: usize) -> TranscriptView {
    let banner_rows = super::transcript::banner_rows(app, width);

    if app.transcript.is_empty() {
        return TranscriptView { banner_rows, stable_rows: Vec::new(), live_rows: Vec::new() };
    }

    let mut stable_rows = Vec::new();
    let mut live_rows = Vec::new();

    stable_rows.extend(banner_rows);

    let ctx = super::transcript::TranscriptRowContext {
        user_label: &app.user_label,
        cwd: &app.cwd,
        width,
        entry_index: None,
    };

    for (index, entry) in app.transcript.iter().enumerate() {
        let mut entry_ctx = ctx.clone();
        entry_ctx.entry_index = Some(index);
        let (entry_stable, entry_live) = entry_stable_and_live_rows(entry, &entry_ctx);
        if entry_stable.is_empty() {
            live_rows.extend(entry_live);
        } else {
            stable_rows.extend(entry_stable);
            live_rows.extend(entry_live);
        }
    }

    TranscriptView { banner_rows: Vec::new(), stable_rows, live_rows }
}

fn build_live_view(app: &App, width: usize, _height: usize, transcript: &TranscriptView) -> LiveView {
    let live_tail = transcript.live_rows.clone();
    let dynamic_status = super::live::dynamic_status_row(app, width);
    let (prompt_rows, prompt_cursor) = super::live::prompt_rows_for(app, width);
    let (prompt_rows, prompt_cursor) =
        clip_prompt_rows_around_cursor(prompt_rows, prompt_cursor, super::live::MAX_PROMPT_ROWS);

    let accessory_rows = super::live::accessory_rows(app, width, super::live::MAX_ACCESSORY_ROWS);
    let queued_summary = super::live::queued_summary_row(app, width);
    let detail_pane = if app.detail_pane.open {
        super::live::detail_pane_rows(app, width, super::live::MAX_ACCESSORY_ROWS)
    } else {
        Vec::new()
    };
    let static_status = super::live::static_status_row(app, width);

    LiveView {
        live_tail,
        dynamic_status,
        prompt_rows,
        prompt_cursor,
        accessory_rows,
        queued_summary,
        detail_pane,
        static_status,
    }
}

/// Split a single entry into stable and live rows.
///
/// Streaming assistant/reasoning blocks and running tools are entirely live
/// until they finish. All other entries are fully stable.
fn entry_stable_and_live_rows(entry: &Entry, ctx: &super::transcript::TranscriptRowContext) -> (Vec<Row>, Vec<Row>) {
    let rows = super::transcript::entry_rows(entry, ctx);

    match entry {
        Entry::Agent { streaming: true, .. }
        | Entry::Reasoning { streaming: true, .. }
        | Entry::Tool { status: ToolStatus::Running, .. } => (Vec::new(), rows),
        _ => (rows, Vec::new()),
    }
}

fn clip_prompt_rows_around_cursor(
    rows: Vec<Row>, cursor: Option<CursorCoord>, max_rows: usize,
) -> (Vec<Row>, Option<CursorCoord>) {
    if rows.len() <= max_rows || max_rows == 0 {
        return (rows, cursor);
    }

    let cursor_row = cursor.map_or_else(
        || rows.len().saturating_sub(1),
        |cursor| cursor.row.min(rows.len().saturating_sub(1)),
    );
    let start = cursor_row.saturating_add(1).saturating_sub(max_rows);
    let clipped_rows = rows.into_iter().skip(start).take(max_rows).collect();
    let clipped_cursor = cursor.map(|mut cursor| {
        cursor.row = cursor.row.saturating_sub(start);
        cursor
    });

    (clipped_rows, clipped_cursor)
}

#[cfg(test)]
mod tests;
