//! Ratatui-owned layout for the mutable composer and bounded application views.
//!
//! This module accepts only [`super::view::LiveView`]. It deliberately has no
//! transcript input: transcript projection and navigation remain outside the
//! live surface so inline mode can place this layout below native history.

use super::row::{CursorCoord, Frame, Row};
use super::style::CellStyle;
use super::view::LiveView;

/// Bottom-pinned layout for the composer and every bounded focused surface.
///
/// The result is a logical frame so Ratatui remains the terminal-cell writer,
/// while wrapping and cursor coordinates can be tested without a terminal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveSurfaceLayout {
    frame: Frame,
}

impl LiveSurfaceLayout {
    /// Lay out the composer, focused surface, queue summary, and status footer.
    ///
    /// `height` is the available live-surface height. The layout clips its
    /// focused surface first, retaining the active composer and its cursor.
    pub fn build(live: &LiveView, width: usize, height: usize) -> Self {
        let min_prompt_chrome = live.prompt_rows.len() + 1;
        let keep_prompt_gutters = height >= min_prompt_chrome + 3;

        let mut footer = vec![live.static_status.clone()];
        if keep_prompt_gutters {
            footer.push(Row::blank(width, CellStyle::new()));
        }
        let prompt_gutter = keep_prompt_gutters.then(|| Row::blank(width, CellStyle::new()));
        let accessory =
            if live.detail_pane.is_empty() { live.accessory_rows.clone() } else { live.detail_pane.clone() };
        let queued: Vec<Row> = live.queued_summary.clone().into_iter().collect();
        let reserved = footer.len() + live.prompt_rows.len() + usize::from(prompt_gutter.is_some());
        let remaining = height.saturating_sub(reserved);
        let accessory_budget = accessory.len().min(remaining);
        let queued_budget = queued.len().min(remaining.saturating_sub(accessory_budget));

        let mut frame = Frame::new(width);
        frame.rows.extend(clip_from_top(queued, queued_budget));
        frame.rows.extend(clip_from_top(accessory, accessory_budget));
        if let Some(row) = prompt_gutter {
            frame.push(row);
        }
        let prompt_offset = frame.len();
        frame.rows.extend(live.prompt_rows.iter().cloned());
        if let Some(mut cursor) = live.prompt_cursor {
            cursor.row += prompt_offset;
            frame.set_cursor(cursor);
        }
        frame.rows.extend(footer);
        Self { frame }
    }

    /// Number of rows requested from the containing live surface.
    pub fn height(&self) -> usize {
        self.frame.rows.len()
    }

    /// Cursor coordinate relative to the live surface's top-left corner.
    pub fn cursor(&self) -> Option<CursorCoord> {
        self.frame.cursor
    }

    /// Borrow the Ratatui-ready rows for this bounded surface.
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Consume the layout into its Ratatui-ready logical frame.
    pub fn into_frame(self) -> Frame {
        self.frame
    }
}

fn clip_from_top(mut rows: Vec<Row>, budget: usize) -> Vec<Row> {
    if budget == 0 {
        return Vec::new();
    }
    if rows.len() > budget {
        rows = rows.split_off(rows.len() - budget);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::renderer::alternate::render_logical_frame;
    use crate::renderer::row::Row;
    use crate::renderer::style::Span;
    use crate::renderer::view::LiveView;

    fn live_view() -> LiveView {
        LiveView {
            live_tail: Vec::new(),
            prompt_rows: vec![Row::padded(vec![Span::plain("draft")], 16, CellStyle::new())],
            prompt_cursor: Some(CursorCoord::new(0, 3)),
            accessory_rows: vec![
                Row::padded(vec![Span::plain("first accessory")], 16, CellStyle::new()),
                Row::padded(vec![Span::plain("focused accessory")], 16, CellStyle::new()),
            ],
            queued_summary: Some(Row::padded(vec![Span::plain("queued")], 16, CellStyle::new())),
            detail_pane: Vec::new(),
            static_status: Row::padded(vec![Span::plain("Ready")], 16, CellStyle::new()),
        }
    }

    #[test]
    fn clips_focused_content_before_the_active_composer() {
        let layout = LiveSurfaceLayout::build(&live_view(), 16, 3);
        let rows = &layout.frame().rows;

        assert_eq!(rows.len(), 3);
        assert!(rows[0].text().contains("focused accessory"));
        assert!(rows[1].text().contains("draft"));
        assert!(rows[2].text().contains("Ready"));
        assert_eq!(layout.cursor(), Some(CursorCoord::new(1, 3)));
    }

    #[test]
    fn places_the_cursor_using_the_rows_that_paint_the_prompt() {
        let layout = LiveSurfaceLayout::build(&live_view(), 16, 6);
        let cursor = layout.cursor().expect("editable prompt has a cursor");
        let row = &layout.frame().rows[cursor.row];

        assert!(row.text().contains("draft"));
        assert_eq!(cursor.col, 3);
    }

    #[test]
    fn transcript_rows_cannot_enter_the_live_surface_layout() {
        let mut live = live_view();
        live.live_tail = vec![Row::padded(vec![Span::plain("transcript row")], 16, CellStyle::new())];
        let layout = LiveSurfaceLayout::build(&live, 16, 6);
        let text = layout.frame().rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(!text.contains("transcript row"));
    }

    #[test]
    fn detail_replaces_accessory_in_the_shared_live_surface() {
        let mut live = live_view();
        live.detail_pane = vec![Row::padded(vec![Span::plain("detail")], 16, CellStyle::new())];
        let layout = LiveSurfaceLayout::build(&live, 16, 6);
        let text = layout.frame().rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("detail"));
        assert!(!text.contains("focused accessory"));
    }

    #[test]
    fn ratatui_test_backend_paints_the_same_prompt_row_as_the_cursor_layout() {
        let layout = LiveSurfaceLayout::build(&live_view(), 16, 6);
        let mut terminal = Terminal::new(TestBackend::new(16, 6)).expect("test terminal");
        terminal
            .draw(|frame| render_logical_frame(frame, layout.frame()))
            .expect("render live surface");

        let buffer = terminal.backend().buffer();
        let painted = (0..16)
            .map(|x| buffer[(x, layout.cursor().expect("cursor").row as u16)].symbol())
            .collect::<String>();
        assert!(
            painted.contains("draft"),
            "cursor should target the painted prompt row: {painted:?}"
        );
    }
}
