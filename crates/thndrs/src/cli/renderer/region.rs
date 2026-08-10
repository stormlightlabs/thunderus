//! Live region state for the direct renderer.
//!
//! [`LiveRegion`] builds one logical terminal viewport from recent history rows
//! and live prompt chrome, then redraws that bounded viewport each tick.
//!
//! The viewport contains, from top to bottom:
//! 1. banner before the first transcript rows have been committed;
//! 2. recent committed transcript rows;
//! 3. the mutable transcript tail, if any;
//! 4. prompt input rows;
//! 5. optional accessory rows (help/commands/files);
//! 6. static status row (model/search/tokens/cwd);
//! 7. blank terminal-default rows after the compact transcript/live content.
//!
//! Transcript row construction lives in [`super::transcript`]; the pure view
//! projection built from app state lives in [`super::view`]. This module keeps
//! viewport policy and scrollback commit bookkeeping.

use std::io;

use crate::app::{App, PromptState};
use crate::renderer::backend::TerminalBackend;
use crate::renderer::row::{Frame, Row};
use crate::renderer::style::{self, CellStyle, Span};
use crate::renderer::transcript::ENTRY_RAIL;
use crate::renderer::view::RendererView;

trait RowPolicyText {
    fn text_for_policy(&self) -> String;
}

impl RowPolicyText for Row {
    fn text_for_policy(&self) -> String {
        let mut out = String::new();
        for span in &self.spans {
            out.push_str(&span.text);
        }
        out
    }
}

/// State tracking the live region render and committed history.
///
/// Transcript rows that are stable are written to the terminal's native
/// scrollback via [`TerminalBackend::insert_history_lines`].
///
/// The renderer's pure [`Self::build_frame`] projection can include the
/// visible tail of committed transcript rows. Terminal I/O instead redraws
/// only the live chrome and any mutable transcript tail that cannot yet be
/// appended safely.
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
    /// Whether the startup banner has been committed with the first transcript entry.
    banner_committed: bool,
    /// Number of stable transcript entries already committed to terminal scrollback.
    committed_entry_count: usize,
}

/// One prepared direct-render update.
///
/// The projection and frame are built before terminal writes so the backend can
/// bracket the complete update atomically with synchronized-update control
/// sequences.
struct RenderPlan<'a> {
    frame: Frame,
    rows_to_commit: &'a [Row],
    width: usize,
    height: usize,
    top_row: u16,
    relocate_live_region: bool,
    promote_visible_tail: bool,
    stable_entry_count: usize,
    commits_banner: bool,
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
            banner_committed: false,
            committed_entry_count: 0,
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
        let view = RendererView::build(app, width, height);
        self.build_frame_from_view(app, &view)
    }

    /// Compose a terminal-sized frame from a projection that was already
    /// built for the current render pass.
    ///
    /// Keeping this separate from [`Self::build_frame`] lets the hot render
    /// path reuse its projection for both scrollback commits and frame layout
    /// instead of rebuilding and re-highlighting the transcript twice.
    fn build_frame_from_view(&self, app: &App, view: &RendererView) -> Frame {
        let width = view.width;
        let height = view.height;
        let mut frame = Frame::new(width);
        if width == 0 || height == 0 {
            return frame;
        }

        let live = self.build_live_frame(view);
        let live_height = live.rows.len().min(height);

        let startup = app.transcript.is_empty();
        let available_history = height.saturating_sub(live_height);
        let history_rows = if startup {
            startup_history_rows(view.transcript.banner_rows.clone(), width, available_history)
        } else {
            clip_transcript_rows_from_top(&view.transcript.stable_rows, available_history, width)
        };

        frame.rows.extend(history_rows);

        let live_start = live.rows.len().saturating_sub(live_height);

        while frame.rows.len() < height.saturating_sub(live_height) {
            frame.push(Row::blank(width, CellStyle::new()));
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

        let cursor_visible = !matches!(app.prompt_state(), PromptState::Stopped | PromptState::Errored);
        frame.cursor_visible = cursor_visible;

        while frame.rows.len() < height {
            frame.push(Row::blank(width, CellStyle::new()));
        }

        frame
    }

    /// Build bottom-anchored live prompt/status rows from the view projection.
    ///
    /// Surfaces are composed in explicit priority order. The priority
    /// determines which surfaces are clipped first when the terminal is too
    /// short (lowest priority = clipped first):
    ///
    /// 1. static footer (static status + trailing blank) — always kept
    /// 2. prompt input rows + cursor — always kept
    /// 3. accessory or detail pane — clipped last of the optional surfaces
    /// 4. queued prompt summary — clipped before accessory
    /// 5. live tail (mutable transcript rows) — clipped first
    ///
    /// Vertical order (top to bottom):
    ///   live_tail → queued → accessory → framed prompt → footer
    fn build_live_frame(&self, view: &RendererView) -> Frame {
        let width = view.width;
        let height = view.height;
        let live = &view.live;
        let surface_bg = CellStyle::new().bg(super::style::palette().panel_bg);

        let min_prompt_chrome = live.prompt_rows.len() + 1;
        let keep_prompt_gutters = height >= min_prompt_chrome + 3;

        let mut footer = vec![live.static_status.clone()];
        if keep_prompt_gutters {
            footer.push(Row::blank(width, surface_bg));
        }
        let prompt_gutter =
            keep_prompt_gutters.then(|| Row::blank(width, CellStyle::new().bg(super::style::palette().panel_bg)));
        let prompt = live.prompt_rows.clone();

        let accessory =
            if !live.detail_pane.is_empty() { live.detail_pane.clone() } else { live.accessory_rows.clone() };

        let queued: Vec<Row> = live.queued_summary.clone().into_iter().collect();
        let tail = &live.live_tail;
        let reserved = footer.len() + prompt.len() + usize::from(prompt_gutter.is_some());
        let remaining = height.saturating_sub(reserved);

        let accessory_budget = accessory.len().min(remaining);
        let after_accessory = remaining.saturating_sub(accessory_budget);
        let queued_budget = queued.len().min(after_accessory);
        let after_queued = after_accessory.saturating_sub(queued_budget);
        let tail_budget = tail.len().min(after_queued);

        let tail_rows = clip_transcript_rows_from_top(tail, tail_budget, width);
        let queued_rows = clip_from_top(queued, queued_budget);
        let accessory_rows = clip_from_top(accessory, accessory_budget);

        let mut frame = Frame::new(width);

        for row in tail_rows {
            frame.push(row);
        }
        for row in queued_rows {
            frame.push(row);
        }
        for row in accessory_rows {
            frame.push(row);
        }

        if let Some(row) = prompt_gutter {
            frame.push(row);
        }

        let prompt_offset = frame.len();
        for row in prompt {
            frame.push(row);
        }

        if let Some(mut c) = live.prompt_cursor {
            c.row += prompt_offset;
            frame.set_cursor(c);
        }

        for row in footer {
            frame.push(row);
        }

        frame
    }

    /// Render the live-region viewport, committing stable transcript rows to
    /// terminal scrollback first.
    ///
    /// 1. Build the pure view projection from app state.
    /// 2. Append only the stable rows that have not yet been inserted into
    ///    native scrollback.
    /// 3. Use [`insert_history_lines`](TerminalBackend::insert_history_lines)
    ///    to push them into native terminal scrollback above the viewport.
    /// 4. Render the live region (mutable tail, prompt/status/accessories) via
    ///    diff rendering.
    pub fn render_frame<W: io::Write>(
        &mut self, app: &App, backend: &mut TerminalBackend<W>, width: usize, height: usize,
    ) -> io::Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }

        let view = RendererView::build(app, width, height);
        let mut live = self.build_live_frame(&view);
        if app.transcript.is_empty() {
            let banner_budget = height.saturating_sub(live.rows.len());
            let banner = startup_history_rows(view.transcript.banner_rows.clone(), width, banner_budget);
            if !banner.is_empty() {
                let banner_len = banner.len();
                live.rows.splice(0..0, banner);
                if let Some(cursor) = live.cursor.as_mut() {
                    cursor.row += banner_len;
                }
            }
        }
        live.cursor_visible = !matches!(app.prompt_state(), PromptState::Stopped | PromptState::Errored);
        // Keep one physical row above the mutable region whenever history is
        // ready to commit. Full-screen terminal scrolling can then move old
        // content into native scrollback before the row is painted.
        let live_budget = height.saturating_sub(usize::from(!view.transcript.stable_rows.is_empty()));
        let live_start = live.rows.len().saturating_sub(live_budget);
        if live_start > 0 {
            live.rows = live.rows.split_off(live_start);
            if let Some(cursor) = live.cursor.as_mut() {
                if cursor.row < live_start {
                    live.cursor = None;
                } else {
                    cursor.row -= live_start;
                }
            }
        }
        let top_row = height.saturating_sub(live.rows.len()) as u16;
        let live_region_moved = self.rendered_top_row.is_some_and(|previous| previous != top_row);
        let height_changed = self
            .rendered_height
            .is_some_and(|rendered_height| rendered_height != height);
        let width_changed = self
            .rendered_width
            .is_some_and(|rendered_width| rendered_width != width);
        let stable_entry_count = view
            .transcript
            .stable_rows
            .iter()
            .filter_map(|row| row.group_id)
            .map(|group| group.entry_index + 1)
            .max()
            .unwrap_or(0);
        let commits_banner = !self.banner_committed && !view.transcript.stable_rows.is_empty();
        let commit_start = view
            .transcript
            .stable_rows
            .iter()
            .position(|row| {
                (!self.banner_committed && row.group_id.is_none())
                    || row
                        .group_id
                        .is_some_and(|group| group.entry_index >= self.committed_entry_count)
            })
            .unwrap_or(view.transcript.stable_rows.len());
        let rows_to_commit = &view.transcript.stable_rows[commit_start..];
        let relocate_live_region = width_changed || height_changed || live_region_moved;
        let promote_visible_tail = view.transcript.live_rows.is_empty()
            && self
                .rendered_frame
                .as_ref()
                .is_some_and(|previous| rendered_prefix_matches(rows_to_commit, previous));
        let frame_changed = self.rendered_frame.as_ref() != Some(&live);
        let viewport_changed = self.rendered_width != Some(width)
            || self.rendered_height != Some(height)
            || self.rendered_top_row != Some(top_row);
        let needs_output = !rows_to_commit.is_empty() || viewport_changed || frame_changed;
        if !needs_output {
            return Ok(());
        }

        let plan = RenderPlan {
            frame: live,
            rows_to_commit,
            width,
            height,
            top_row,
            relocate_live_region,
            promote_visible_tail,
            stable_entry_count,
            commits_banner,
        };
        backend.begin_synchronized_update()?;
        let result = self.render_frame_inner(backend, plan);
        let end_result = backend.end_synchronized_update();
        result.and(end_result)
    }

    fn render_frame_inner<W: io::Write>(
        &mut self, backend: &mut TerminalBackend<W>, plan: RenderPlan<'_>,
    ) -> io::Result<()> {
        let RenderPlan {
            frame,
            rows_to_commit,
            width,
            height,
            top_row,
            relocate_live_region,
            promote_visible_tail,
            stable_entry_count,
            commits_banner,
        } = plan;
        let promoted_visible_tail = if promote_visible_tail && let Some(previous_top) = self.rendered_top_row {
            backend.write_rows(previous_top, rows_to_commit)?;
            self.rendered_frame = None;
            true
        } else {
            false
        };
        if !promoted_visible_tail && relocate_live_region {
            let live_grew_upward = self.rendered_width == Some(width)
                && self.rendered_height == Some(height)
                && self.rendered_top_row.is_some_and(|previous_top| previous_top > top_row);
            if live_grew_upward && let Some(previous_top) = self.rendered_top_row {
                if let Some(previous_frame) = self.rendered_frame.as_ref() {
                    backend.clear_rows(previous_top, previous_frame.rows.len() as u16)?;
                }
                backend.scroll_up_preserving_history(previous_top - top_row)?;
                self.rendered_frame = None;
            } else {
                self.clear_abandoned_live_rows(backend, top_row, frame.rows.len(), height)?;
            }
        }

        if !rows_to_commit.is_empty() {
            // The terminal will preserve every row displaced by the full-screen
            // scroll. Remove the previous mutable region first so prompt and
            // streaming content cannot become transcript history.
            if !promoted_visible_tail {
                if let (Some(previous_top), Some(previous_frame)) =
                    (self.rendered_top_row, self.rendered_frame.as_ref())
                {
                    backend.clear_rows(previous_top, previous_frame.rows.len() as u16)?;
                }
                // Keep the mutable composer below the terminal-native transcript
                // region. When it fills the screen, still insert into full-screen
                // scrollback and immediately restore the live frame below.
                let transcript_bottom = if top_row == 0 { height as u16 } else { top_row };
                backend.insert_history_lines(rows_to_commit, transcript_bottom)?;
            }
            self.rendered_frame = None;
            self.committed_entry_count = stable_entry_count;
            self.banner_committed |= commits_banner;
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

    fn clear_abandoned_live_rows<W: io::Write>(
        &mut self, backend: &mut TerminalBackend<W>, new_top: u16, new_len: usize, height: usize,
    ) -> io::Result<()> {
        let (Some(old_top), Some(old_frame)) = (self.rendered_top_row, self.rendered_frame.as_ref()) else {
            self.rendered_frame = None;
            return Ok(());
        };
        let terminal_bottom = height as u16;
        let old_end = old_top.saturating_add(old_frame.rows.len() as u16).min(terminal_bottom);
        let new_end = new_top.saturating_add(new_len as u16).min(terminal_bottom);

        if old_top < new_top {
            let end = old_end.min(new_top);
            backend.clear_rows(old_top, end.saturating_sub(old_top))?;
        }
        if new_end < old_end {
            let start = new_end.max(old_top);
            backend.clear_rows(start, old_end.saturating_sub(start))?;
        }
        self.rendered_frame = None;
        Ok(())
    }

    /// Reset all committed state (e.g. on `/clear`).
    pub fn reset(&mut self) {
        self.rendered_frame = None;
        self.rendered_width = None;
        self.rendered_height = None;
        self.rendered_top_row = None;
        self.banner_committed = false;
        self.committed_entry_count = 0;
    }

    /// Start projecting a replacement transcript without clearing terminal history.
    ///
    /// The next render commits the replacement transcript from its first
    /// stable entry. Keeping the painted frame lets that render erase the old
    /// mutable region before inserting the new segment, while retaining the
    /// banner watermark prevents the startup banner from being replayed.
    pub fn begin_transcript_segment(&mut self) {
        self.committed_entry_count = 0;
    }

    /// Forget only the last painted live frame.
    ///
    /// External programs may overwrite the visible terminal while suspended.
    /// The committed transcript remains valid in native scrollback.
    pub fn invalidate_frame(&mut self) {
        self.rendered_frame = None;
    }
}

fn rendered_prefix_matches(rows: &[Row], previous: &Frame) -> bool {
    if rows.is_empty() || rows.len() > previous.rows.len() || rows.iter().any(|row| row.group_id.is_none()) {
        return false;
    }

    rows.iter()
        .zip(&previous.rows)
        .all(|(settled, rendered)| settled.group_id == rendered.group_id)
        && previous
            .rows
            .get(rows.len())
            .is_none_or(|next| next.group_id != rows.last().and_then(|row| row.group_id))
}

struct StartupSectionGroup {
    priority: u8,
    rows: Vec<Row>,
}

/// Keep the last `budget` rows, dropping older rows from the top.
///
/// When `budget` exceeds the row count, all rows are returned unchanged.
/// A zero budget returns an empty vec.
fn clip_from_top(mut rows: Vec<Row>, budget: usize) -> Vec<Row> {
    if budget == 0 {
        return Vec::new();
    }
    if rows.len() <= budget {
        return rows;
    }
    rows.split_off(rows.len() - budget)
}

/// Keep the latest transcript rows with an explicit partial-entry marker.
///
/// Group identity is semantic row metadata, unlike the rendered code and tool
/// gutters. A marker therefore remains aligned with its entry even when the
/// first retained row is nested code or tool output. When an entry heading is
/// still visible, it remains more useful than a continuation marker.
fn clip_transcript_rows_from_top(rows: &[Row], budget: usize, width: usize) -> Vec<Row> {
    if budget == 0 || rows.is_empty() {
        return Vec::new();
    }
    if rows.len() <= budget {
        return rows.to_vec();
    }

    let first_visible = rows.len() - budget;
    let Some(group_id) = rows[first_visible].group_id else {
        return rows[first_visible..].to_vec();
    };

    let group_start = rows[..first_visible]
        .iter()
        .rposition(|row| row.group_id != Some(group_id))
        .map_or(0, |index| index + 1);
    if group_start == first_visible {
        return rows[first_visible..].to_vec();
    }

    let first_nonblank = rows[group_start..=first_visible]
        .iter()
        .position(|row| !row.text_for_policy().trim().is_empty())
        .map(|index| group_start + index);
    if first_nonblank == Some(first_visible) {
        return rows[first_visible..].to_vec();
    }

    // The marker replaces `first_visible`, so that row is hidden along with
    // the earlier rows from this entry.
    let hidden_rows = first_visible - group_start + 1;
    let entry_rows = &rows[group_start..=first_visible];
    // `Row::padded` places layout padding before the rendered entry rail. Pick
    // the rail explicitly so a continuation retains the entry's color instead
    // of inheriting the padding style. The fallback keeps custom transcript
    // rows without a rail readable.
    let rail_style = entry_rows
        .iter()
        .flat_map(|row| row.spans.iter())
        .find(|span| span.text == ENTRY_RAIL)
        .or_else(|| {
            entry_rows
                .iter()
                .flat_map(|row| row.spans.iter())
                .find(|span| !span.text.trim().is_empty())
        })
        .map(|span| span.style)
        .unwrap_or_default();
    let marker_style = CellStyle::new().fg(style::palette().overlay1).bg(rail_style.bg);
    let mut marker = Row::padded(
        vec![
            Span::styled(ENTRY_RAIL, rail_style),
            Span::styled(
                format!("… {hidden_rows} earlier row(s) in this entry hidden"),
                marker_style,
            ),
        ],
        width,
        CellStyle::new().bg(marker_style.bg),
    );
    marker.group_id = Some(group_id);

    if budget == 1 {
        return vec![marker];
    }

    let visible_tail = rows[rows.len() - (budget - 1)..].to_vec();
    let mut clipped = Vec::with_capacity(budget);
    clipped.push(marker);
    clipped.extend(visible_tail);
    clipped
}

/// Apply startup-specific clipping instead of blindly taking the bottom rows.
///
/// Short terminals should keep the compact identity and the most actionable
/// context/diagnostic/help rows when possible.
///
/// If rows are omitted, a visible marker replaces the silent gap so the user knows
/// the startup shell was constrained by height.
fn startup_history_rows(rows: Vec<Row>, width: usize, budget: usize) -> Vec<Row> {
    match budget {
        0 => Vec::new(),
        budget if rows.len() <= budget => {
            // Keep the compact welcome above the viewport midpoint, with the
            // remaining space flowing naturally toward the composer.
            let top_padding = budget.saturating_sub(rows.len()) / 3;
            let mut positioned = vec![Row::blank(width, CellStyle::new()); top_padding];
            positioned.extend(rows);
            positioned
        }
        1 => vec![hidden_startup_row(width, rows.len())],
        _ => clip_startup_sections(&rows, width, budget),
    }
}

/// Clip the startup banner by complete semantic groups.
fn clip_startup_sections(rows: &[Row], width: usize, budget: usize) -> Vec<Row> {
    let keep_budget = budget - 1;
    let mut groups = startup_section_groups(rows);
    if keep_budget < 14
        && let Some(identity) = groups.first_mut()
    {
        identity.rows = compact_identity_section(&identity.rows);
    }
    let compact_identity = groups
        .first()
        .and_then(|group| group.rows.iter().find(|row| row.text_for_policy().contains("thndrs")))
        .cloned();
    let mut selected = vec![false; groups.len()];
    let mut remaining = keep_budget;

    let mut candidates: Vec<(u8, usize)> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| (group.priority, index))
        .collect();
    candidates.sort_unstable();

    for (_, index) in candidates {
        let group = &groups[index];
        if group.rows.len() <= remaining {
            selected[index] = true;
            remaining -= group.rows.len();
        }
    }

    let hidden = groups
        .iter()
        .enumerate()
        .filter(|(index, _)| !selected[*index])
        .map(|(_, group)| group.rows.len())
        .sum();
    let mut out: Vec<Row> = groups
        .into_iter()
        .enumerate()
        .filter(|(index, _)| selected[*index])
        .flat_map(|(_, group)| group.rows)
        .collect();

    if out.is_empty() {
        // At extreme heights, preserve the application identity instead of a
        // detached metadata row. The next height tier restores complete groups.
        if let Some(row) = compact_identity {
            out.push(row);
        }
    }
    out.push(hidden_startup_row(width, hidden));
    out
}

/// Keeps the startup identity legible when there is not enough height for
/// the full runtime summary.
fn compact_identity_section(rows: &[Row]) -> Vec<Row> {
    rows.iter()
        .filter(|row| {
            let text = row.text_for_policy();
            let trimmed = text.trim();
            trimmed.starts_with("thndrs")
        })
        .cloned()
        .collect()
}

fn startup_section_groups(rows: &[Row]) -> Vec<StartupSectionGroup> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for row in rows {
        if row.text_for_policy().trim().is_empty() {
            if !current.is_empty() {
                groups.push(startup_section_group(std::mem::take(&mut current)));
            }
        } else {
            current.push(row.clone());
        }
    }
    if !current.is_empty() {
        groups.push(startup_section_group(current));
    }
    groups
}

fn startup_section_group(rows: Vec<Row>) -> StartupSectionGroup {
    let text = rows
        .iter()
        .map(RowPolicyText::text_for_policy)
        .collect::<Vec<_>>()
        .join("\n");
    let priority = if text.contains("thndrs") {
        0
    } else if text.contains("│") {
        1
    } else if text.contains("ATTENTION") {
        2
    } else {
        3
    };
    StartupSectionGroup { priority, rows }
}

fn hidden_startup_row(width: usize, hidden: usize) -> Row {
    let p = style::palette();
    Row::padded(
        vec![Span::styled(
            format!("... {hidden} rows hidden"),
            CellStyle::new().fg(p.subtext0),
        )],
        width,
        CellStyle::new(),
    )
}

#[cfg(test)]
mod tests;
