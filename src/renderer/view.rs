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

    let ctx = super::transcript::TranscriptRowContext { user_label: &app.user_label, cwd: &app.cwd, width };

    for entry in &app.transcript {
        let (entry_stable, entry_live) = entry_stable_and_live_rows(entry, &ctx);
        if entry_stable.is_empty() {
            live_rows.extend(entry_live);
        } else {
            stable_rows.extend(entry_stable);
            live_rows.extend(entry_live);
        }
    }

    TranscriptView { banner_rows: Vec::new(), stable_rows, live_rows }
}

fn build_live_view(app: &App, width: usize, height: usize, transcript: &TranscriptView) -> LiveView {
    let live_tail = clip_live_tail_rows(transcript.live_rows.clone(), height);
    let dynamic_status = super::live::dynamic_status_row(app, width);
    let (prompt_rows, prompt_cursor) = super::live::prompt_rows_for(app, width);

    let prompt_count = prompt_rows.len().min(super::live::MAX_PROMPT_ROWS);
    let base_height = live_tail.len() + 1 + 1; // live_tail + blank + dynamic_status
    let prompt_block_count = prompt_count + 1;
    let remaining_after_prompt = height.saturating_sub(base_height + prompt_block_count + 1);
    let accessory_height = remaining_after_prompt.min(super::live::MAX_ACCESSORY_ROWS);

    let accessory_rows = super::live::accessory_rows(app, width, accessory_height);
    let static_status = super::live::static_status_row(app, width);

    LiveView {
        live_tail,
        dynamic_status,
        prompt_rows: prompt_rows.into_iter().take(prompt_count).collect(),
        prompt_cursor,
        accessory_rows,
        static_status,
    }
}

/// Split a single entry into stable and live rows.
///
/// Streaming assistant/reasoning blocks keep the last two rows live when the
/// block exceeds three rows so the stable prefix can be committed to scrollback
/// while the mutable tail stays visible. Running tools are entirely live until
/// they finish. All other entries are fully stable.
fn entry_stable_and_live_rows(entry: &Entry, ctx: &super::transcript::TranscriptRowContext) -> (Vec<Row>, Vec<Row>) {
    let rows = super::transcript::entry_rows(entry, ctx);

    match entry {
        Entry::Assistant { streaming: true, .. } | Entry::Reasoning { streaming: true, .. } => {
            split_streaming_rows(rows)
        }
        Entry::Tool { status: ToolStatus::Running, .. } => (Vec::new(), rows),
        _ => (rows, Vec::new()),
    }
}

fn split_streaming_rows(rows: Vec<Row>) -> (Vec<Row>, Vec<Row>) {
    if rows.len() <= 3 {
        return (Vec::new(), rows);
    }

    let stable_len = rows.len().saturating_sub(2);
    let stable_rows = rows[..stable_len].to_vec();
    let live_rows = rows[stable_len..].to_vec();
    (stable_rows, live_rows)
}

/// Clip the mutable transcript tail to a reasonable share of the viewport.
///
/// This is a viewport policy detail duplicated here so the view projection can
/// stay self-contained. The same clipping is applied again by
/// [`super::region::LiveRegion`] when composing the final frame.
fn clip_live_tail_rows(mut active: Vec<Row>, height: usize) -> Vec<Row> {
    let max_active = height.saturating_sub(4).max(1);
    if active.len() > max_active { active.split_off(active.len() - max_active) } else { active }
}

#[cfg(test)]
mod tests;
