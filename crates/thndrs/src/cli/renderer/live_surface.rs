//! Ratatui-owned layout for the mutable composer and application views.
//!
//! This module accepts only [`super::view::LiveView`]. It deliberately has no
//! transcript input: transcript projection and navigation remain outside the
//! live surface so inline mode can place this layout below native history.

use super::row::{CursorCoord, Frame};
#[cfg(test)]
use super::style::CellStyle;
use super::view::LiveView;

/// Bottom-pinned layout for the composer and every focused surface.
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
    /// The containing terminal coordinator reserves exactly this many rows.
    /// Focused content is already restrained by the view projection, so this
    /// layout never needs to know about terminal scrollback or a fixed viewport.
    pub fn build(live: &LiveView, width: usize) -> Self {
        let accessory =
            if live.detail_pane.is_empty() { live.accessory_rows.clone() } else { live.detail_pane.clone() };

        let mut frame = Frame::new(width);
        let prompt_offset = frame.len();
        frame.rows.extend(live.prompt_rows.iter().cloned());
        if let Some(mut cursor) = live.prompt_cursor {
            cursor.row += prompt_offset;
            frame.set_cursor(cursor);
        }
        // Autocomplete belongs immediately below the composer, like Pi and
        // Codex. Larger focused surfaces use the same bottom pane, never
        // terminal history.
        frame.rows.extend(accessory);
        frame.rows.extend(live.queued_summary.clone());
        frame.push(live.static_status.clone());
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

    /// Borrow the Ratatui-ready rows for this surface.
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Consume the layout into its Ratatui-ready logical frame.
    pub fn into_frame(self) -> Frame {
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::renderer::ratatui::render_logical_frame;
    use crate::renderer::row::Row;
    use crate::renderer::style::Span;
    use crate::renderer::view::LiveView;

    fn live_view() -> LiveView {
        LiveView {
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
        let layout = LiveSurfaceLayout::build(&live_view(), 16);
        let rows = &layout.frame().rows;

        assert!(rows[0].text().contains("draft"));
        assert!(rows[1].text().contains("first accessory"));
        assert!(rows[2].text().contains("focused accessory"));
        assert!(rows.last().is_some_and(|row| row.text().contains("Ready")));
        assert_eq!(layout.cursor(), Some(CursorCoord::new(0, 3)));
    }

    #[test]
    fn places_the_cursor_using_the_rows_that_paint_the_prompt() {
        let layout = LiveSurfaceLayout::build(&live_view(), 16);
        let cursor = layout.cursor().expect("editable prompt has a cursor");
        let row = &layout.frame().rows[cursor.row];

        assert!(row.text().contains("draft"));
        assert_eq!(cursor.col, 3);
    }

    #[test]
    fn detail_replaces_accessory_in_the_shared_live_surface() {
        let mut live = live_view();
        live.detail_pane = vec![Row::padded(vec![Span::plain("detail")], 16, CellStyle::new())];
        let layout = LiveSurfaceLayout::build(&live, 16);
        let text = layout.frame().rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("detail"));
        assert!(!text.contains("focused accessory"));
    }

    #[test]
    fn ratatui_test_backend_paints_the_same_prompt_row_as_the_cursor_layout() {
        let layout = LiveSurfaceLayout::build(&live_view(), 16);
        let mut terminal = Terminal::new(TestBackend::new(16, 6)).expect("test terminal");
        terminal
            .draw(|frame| render_logical_frame(frame, layout.frame()))
            .expect("render live surface");

        let buffer = terminal.backend().buffer();
        let cursor = layout.cursor().expect("cursor");
        let source_start = layout.frame().rows.len().saturating_sub(6);
        let destination_start = 6usize.saturating_sub(layout.frame().rows.len().min(6));
        let painted_row = cursor.row.saturating_sub(source_start) + destination_start;
        let painted = (0..16)
            .map(|x| buffer[(x, painted_row as u16)].symbol())
            .collect::<String>();
        assert!(
            painted.contains("draft"),
            "cursor should target the painted prompt row: {painted:?}"
        );
    }
}
