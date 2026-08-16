//! Ratatui alternate-screen renderer.
//!
//! The production terminal surface has one lifecycle owner and one complete
//! frame per dirty application update. Pure viewport projection remains
//! separate from terminal I/O so navigation and layout can use `TestBackend`.
//!
//! - [`AlternateLayout`] reserves a bottom-pinned composer and gives the
//!   remaining rows to an application-owned transcript viewport;
//! - [`AlternateViewport`] owns semantic transcript navigation;
//! - [`AlternateScreenSession`] owns raw and alternate-screen modes;
//! - [`render_logical_frame`] adapts renderer-owned rows to one Ratatui frame.

use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Frame as RatatuiFrame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color as RatatuiColor, Modifier, Style};
use ratatui::text::{Line, Span as RatatuiSpan};
use ratatui::widgets::{Clear, Paragraph};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{Action, App, Entry, PromptAccessory, PromptState};

use super::live_surface::LiveSurfaceLayout;
use super::row::{Frame, Row};
use super::style::{CellStyle, Color, Span};
use super::view::{
    LiveView, RendererView, SemanticUiView, TranscriptProjectionKey, TranscriptView, project_transcript_entry,
    transcript_projection_key,
};

const MOUSE_WHEEL_ROWS: usize = 3;

/// Owns terminal modes for one alternate-screen application session.
///
/// Cleanup is best-effort in `Drop`, so normal return, errors, and unwinding all
/// restore the shell-facing terminal state through the same owner.
#[derive(Debug)]
pub struct AlternateScreenSession {
    mouse_capture: bool,
    active: bool,
}

impl AlternateScreenSession {
    /// Enter raw mode and the alternate screen.
    pub fn enter(mouse_capture: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let result = if mouse_capture {
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                EnableBracketedPaste,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
                EnableMouseCapture,
                SetCursorStyle::BlinkingBlock
            )
        } else {
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                EnableBracketedPaste,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
                SetCursorStyle::BlinkingBlock
            )
        };
        if let Err(error) = result {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { mouse_capture, active: true })
    }

    /// Temporarily restore the shell terminal for an external interactive app.
    pub fn suspend(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        drain_terminal_events();
        restore_terminal(self.mouse_capture)?;
        self.active = false;
        Ok(())
    }

    /// Re-enter the owned alternate-screen terminal after suspension.
    pub fn resume(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        enable_raw_mode()?;
        let result = if self.mouse_capture {
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                EnableBracketedPaste,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
                EnableMouseCapture,
                SetCursorStyle::BlinkingBlock
            )
        } else {
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                EnableBracketedPaste,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
                SetCursorStyle::BlinkingBlock
            )
        };
        if result.is_ok() {
            self.active = true;
        } else {
            let _ = disable_raw_mode();
        }
        result
    }
}

impl Drop for AlternateScreenSession {
    fn drop(&mut self) {
        if self.active {
            drain_terminal_events();
            let _ = restore_terminal(self.mouse_capture);
            self.active = false;
        }
    }
}

fn drain_terminal_events() {
    while crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
        if crossterm::event::read().is_err() {
            break;
        }
    }
}

fn restore_terminal(mouse_capture: bool) -> io::Result<()> {
    let mut stdout = io::stdout();
    if mouse_capture {
        execute!(
            stdout,
            Show,
            SetCursorStyle::DefaultUserShape,
            DisableMouseCapture,
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
    } else {
        execute!(
            stdout,
            Show,
            SetCursorStyle::DefaultUserShape,
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
    }
    stdout.flush()?;
    disable_raw_mode()
}

/// Stable transcript position used while the reader is away from the tail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptPosition {
    /// A row in the startup banner, before transcript entries exist.
    Banner { row: usize },
    /// A logical rendered row within an append-only transcript entry.
    Entry { entry_index: usize, row_in_entry: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TranscriptPoint {
    position: TranscriptPosition,
    grapheme: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TranscriptSelection {
    Rows {
        anchor: TranscriptPosition,
        head: TranscriptPosition,
    },
    Text {
        anchor: TranscriptPoint,
        head: TranscriptPoint,
    },
}

/// Transcript navigation mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewportState {
    /// Keep the newest transcript row visible as content arrives.
    #[default]
    FollowingTail,
    /// Preserve the selected transcript position while content changes below it.
    Anchored(TranscriptPosition),
}

/// Application-owned transcript viewport for the alternate-screen renderer.
#[derive(Debug, Default)]
pub struct AlternateViewport {
    state: ViewportState,
    search_restore: Option<ViewportState>,
    last_search_match: Option<(usize, usize, usize)>,
    transcript_cache: TranscriptProjectionCache,
    last_positions: Vec<TranscriptPosition>,
    last_rows: Vec<Row>,
    selection: Option<TranscriptSelection>,
    mouse_anchor: Option<TranscriptPoint>,
    anchor_excerpt: Option<String>,
    last_top: usize,
    last_page_rows: usize,
    last_content_top: usize,
    last_visible_rows: usize,
}

#[derive(Clone, Debug)]
struct CachedEntryProjection {
    source: Entry,
    width: usize,
    key: TranscriptProjectionKey,
    stable_rows: Vec<Row>,
    live_rows: Vec<Row>,
}

#[derive(Debug, Default)]
struct TranscriptProjectionCache {
    banner_width: Option<usize>,
    banner_rows: Vec<Row>,
    entries: Vec<CachedEntryProjection>,
}

impl TranscriptProjectionCache {
    fn project(&mut self, app: &App, width: usize) -> TranscriptView {
        if self.banner_width != Some(width) {
            self.banner_rows = app.render_banner_rows(width);
            self.banner_width = Some(width);
        }
        self.entries.truncate(app.transcript.entries.len());
        for (entry_index, entry) in app.transcript.entries.iter().enumerate() {
            let key = transcript_projection_key(app, entry_index);
            let reusable = self
                .entries
                .get(entry_index)
                .is_some_and(|cached| cached.width == width && cached.key == key && cached.source == *entry);
            if reusable {
                continue;
            }
            let (stable_rows, live_rows) = project_transcript_entry(app, entry_index, width);
            let projection = CachedEntryProjection { source: entry.clone(), width, key, stable_rows, live_rows };
            if entry_index < self.entries.len() {
                self.entries[entry_index] = projection;
            } else {
                self.entries.push(projection);
            }
        }

        if app.transcript.entries.is_empty() {
            return TranscriptView {
                rows: self.banner_rows.clone(),
                banner_rows: self.banner_rows.clone(),
                stable_rows: Vec::new(),
                live_rows: Vec::new(),
            };
        }

        let mut rows = self.banner_rows.clone();
        let mut stable_rows = self.banner_rows.clone();
        let mut live_rows = Vec::new();
        for entry in &self.entries {
            rows.extend(entry.stable_rows.iter().cloned());
            rows.extend(entry.live_rows.iter().cloned());
            stable_rows.extend(entry.stable_rows.iter().cloned());
            live_rows.extend(entry.live_rows.iter().cloned());
        }
        TranscriptView { rows, banner_rows: Vec::new(), stable_rows, live_rows }
    }
}

impl AlternateViewport {
    /// Current navigation state.
    pub fn state(&self) -> ViewportState {
        self.state
    }

    /// Return to the newest transcript content.
    pub fn follow_tail(&mut self) {
        self.state = ViewportState::FollowingTail;
    }

    /// Forget navigation state after a transcript clear.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Apply transcript navigation when no higher-priority surface owns input.
    pub fn handle_navigation(&mut self, app: &App, action: &Action) -> bool {
        let transcript_focused = app.overlay.setup().is_none()
            && !app.overlay.is_detail()
            && app.overlay.permission().is_none()
            && matches!(app.overlay.accessory(), PromptAccessory::None);
        if !transcript_focused {
            return false;
        }
        match action {
            Action::ScrollTranscriptWheelUp => self.scroll_up(MOUSE_WHEEL_ROWS),
            Action::ScrollTranscriptWheelDown => self.scroll_down(MOUSE_WHEEL_ROWS),
            Action::ScrollTranscriptUp => self.line_up(),
            Action::ScrollTranscriptDown => self.line_down(),
            Action::ScrollTranscriptPageUp => self.page_up(),
            Action::ScrollTranscriptPageDown => self.page_down(),
            Action::ScrollTranscriptHalfUp => self.half_page_up(),
            Action::ScrollTranscriptHalfDown => self.half_page_down(),
            Action::TranscriptTop => self.top(),
            Action::TranscriptFollowTail => self.follow_tail(),
            Action::ExtendTranscriptSelectionUp => self.extend_selection(-1),
            Action::ExtendTranscriptSelectionDown => self.extend_selection(1),
            Action::ClearTranscriptSelection => {
                self.selection = None;
                self.mouse_anchor = None;
            }
            Action::BeginTranscriptSelection { column, row } => {
                self.begin_mouse_selection(usize::from(*column), usize::from(*row));
            }
            Action::UpdateTranscriptSelection { column, row } => {
                self.update_mouse_selection(usize::from(*column), usize::from(*row));
            }
            Action::EndTranscriptSelection { column, row } => {
                self.end_mouse_selection(usize::from(*column), usize::from(*row));
            }
            Action::CopyTranscriptSelection => return false,
            _ => return false,
        }
        true
    }

    /// Scroll one visual row toward older content.
    pub fn line_up(&mut self) {
        self.scroll_up(1);
    }

    /// Scroll one visual row toward newer content.
    pub fn line_down(&mut self) {
        self.scroll_down(1);
    }

    /// Scroll half a visible transcript page toward older content.
    pub fn half_page_up(&mut self) {
        self.scroll_up(self.last_page_rows.div_ceil(2).max(1));
    }

    /// Scroll half a visible transcript page toward newer content.
    pub fn half_page_down(&mut self) {
        self.scroll_down(self.last_page_rows.div_ceil(2).max(1));
    }

    /// Scroll one visible transcript page toward older content.
    pub fn page_up(&mut self) {
        self.scroll_up(self.last_page_rows.max(1));
    }

    /// Scroll one visible transcript page toward newer content.
    pub fn page_down(&mut self) {
        self.scroll_down(self.last_page_rows.max(1));
    }

    /// Jump to the oldest projected transcript row.
    pub fn top(&mut self) {
        if let Some(position) = self.last_positions.first().copied() {
            self.state = ViewportState::Anchored(position);
        }
    }

    /// Return the selected visual transcript rows as Unicode text.
    pub fn selected_text(&self) -> Option<String> {
        let ranges = selection_ranges(self.selection?, &self.last_positions, &self.last_rows)?;
        let text = ranges
            .into_iter()
            .map(|(row, start, end)| row_graphemes(&self.last_rows[row])[start..end].concat())
            .collect::<Vec<_>>()
            .join("\n");
        (!text.is_empty()).then_some(text)
    }

    fn extend_selection(&mut self, direction: isize) {
        if self.last_positions.is_empty() {
            return;
        }
        let initial = self.last_top.min(self.last_positions.len() - 1);
        let (anchor, head) = match self.selection {
            Some(TranscriptSelection::Rows { anchor, head }) => (anchor, head),
            _ => (self.last_positions[initial], self.last_positions[initial]),
        };
        let head_index = self
            .last_positions
            .iter()
            .position(|position| *position == head)
            .unwrap_or(initial);
        let next = head_index
            .saturating_add_signed(direction)
            .min(self.last_positions.len() - 1);
        let next_position = self.last_positions[next];
        self.selection = Some(TranscriptSelection::Rows { anchor, head: next_position });
        self.mouse_anchor = None;
        self.state = ViewportState::Anchored(next_position);
    }

    fn begin_mouse_selection(&mut self, column: usize, row: usize) {
        self.selection = None;
        self.mouse_anchor = self.mouse_point(column, row, false);
    }

    fn update_mouse_selection(&mut self, column: usize, row: usize) {
        let Some(anchor) = self.mouse_anchor else {
            return;
        };
        if let Some(head) = self.mouse_point(column, row, true) {
            self.selection = Some(TranscriptSelection::Text { anchor, head });
        }
    }

    fn end_mouse_selection(&mut self, column: usize, row: usize) {
        if self.selection.is_some() {
            self.update_mouse_selection(column, row);
        }
        self.mouse_anchor = None;
    }

    fn mouse_point(&self, column: usize, row: usize, clamp: bool) -> Option<TranscriptPoint> {
        if self.last_visible_rows == 0 {
            return None;
        }
        let first_screen_row = self.last_content_top;
        let last_screen_row = first_screen_row + self.last_visible_rows - 1;
        if !clamp && !(first_screen_row..=last_screen_row).contains(&row) {
            return None;
        }
        let screen_row = row.clamp(first_screen_row, last_screen_row);
        let row_index = self.last_top + screen_row - first_screen_row;
        let row = self.last_rows.get(row_index)?;
        let graphemes = row_graphemes(row);
        let last_grapheme = graphemes.len().checked_sub(1)?;
        let text_width = UnicodeWidthStr::width(graphemes.concat().as_str());
        if !clamp && column >= text_width {
            return None;
        }
        let mut display_column = 0;
        let grapheme = graphemes
            .iter()
            .position(|grapheme| {
                display_column += UnicodeWidthStr::width(grapheme.as_str()).max(1);
                column < display_column
            })
            .unwrap_or(last_grapheme);
        Some(TranscriptPoint { position: self.last_positions[row_index], grapheme })
    }

    /// Build one complete terminal-sized logical frame.
    pub fn build_frame(&mut self, app: &App, width: usize, height: usize) -> Frame {
        let search_match = app.overlay.transcript_search().and_then(|search| {
            search
                .current()
                .map(|found| (search.selected, found.entry_index, found.start))
        });
        let search_changed = search_match != self.last_search_match;
        if app.overlay.transcript_search().is_some() {
            if self.search_restore.is_none() {
                self.search_restore = Some(self.state);
            }
            if search_match != self.last_search_match
                && let Some((_, entry_index, _)) = search_match
            {
                self.state = ViewportState::Anchored(TranscriptPosition::Entry { entry_index, row_in_entry: 0 });
                self.anchor_excerpt = None;
            }
            self.last_search_match = search_match;
        } else if let Some(previous) = self.search_restore.take() {
            self.state = previous;
            self.last_search_match = None;
            self.anchor_excerpt = None;
        }
        let semantic = SemanticUiView::from(app);
        let transcript = self.transcript_cache.project(app, width);
        let anchored = matches!(self.state, ViewportState::Anchored(_));
        let live = LiveView::build(app, width, height, &transcript, &semantic, anchored);
        let view = RendererView { semantic, transcript, live, width, height };
        let chrome = LiveSurfaceLayout::build(&view.live, width, height).into_frame();
        let chrome_height = chrome.rows.len().min(height);
        let transcript_height = height.saturating_sub(chrome_height);
        let rows = &view.transcript.rows;
        let positions = transcript_positions(rows);
        let search_row = transcript_search_row(app, rows, &positions);
        if search_changed && let Some(row) = search_row {
            self.state = ViewportState::Anchored(positions[row]);
            self.anchor_excerpt = Some(row_excerpt(&rows[row]));
        }
        let max_top = rows.len().saturating_sub(transcript_height);
        let top = match self.state {
            ViewportState::FollowingTail => max_top,
            ViewportState::Anchored(anchor) => {
                resolve_position(&positions, rows, anchor, self.anchor_excerpt.as_deref())
                    .unwrap_or(max_top)
                    .min(max_top)
            }
        };

        self.last_positions = positions;
        self.last_rows = rows.clone();
        self.last_top = top;
        self.last_page_rows = transcript_height.max(1);

        let mut frame = Frame::new(width);
        let visible_end = top.saturating_add(transcript_height).min(rows.len());
        let mut visible = rows[top.min(rows.len())..visible_end].to_vec();
        self.last_visible_rows = visible.len();
        self.last_content_top = transcript_height.saturating_sub(visible.len());
        if let Some(search_row) = search_row
            && (top..visible_end).contains(&search_row)
        {
            for span in &mut visible[search_row - top].spans {
                span.style = span.style.underlined();
            }
        }
        if let Some(selection) = self.selection
            && let Some(ranges) = selection_ranges(selection, &self.last_positions, &self.last_rows)
        {
            let palette = super::style::palette();
            for (row_index, start, end) in ranges {
                if (top..visible_end).contains(&row_index) {
                    style_grapheme_range(&mut visible[row_index - top], start, end, palette.border);
                }
            }
        }
        for _ in 0..transcript_height.saturating_sub(visible.len()) {
            frame.push(Row::blank(width, CellStyle::new()));
        }
        frame.rows.extend(visible);

        let chrome_start = chrome.rows.len().saturating_sub(chrome_height);
        let chrome_offset = frame.rows.len();
        frame
            .rows
            .extend(chrome.rows.into_iter().skip(chrome_start).take(chrome_height));
        frame.cursor = chrome.cursor.and_then(|mut cursor| {
            if cursor.row < chrome_start {
                return None;
            }
            cursor.row = cursor.row - chrome_start + chrome_offset;
            Some(cursor)
        });
        frame.cursor_visible = !matches!(app.prompt_state(), PromptState::Stopped | PromptState::Errored);
        while frame.rows.len() < height {
            frame.push(Row::blank(width, CellStyle::new()));
        }
        frame
    }

    fn scroll_up(&mut self, amount: usize) {
        let target = self.last_top.saturating_sub(amount);
        if let Some(position) = self.last_positions.get(target).copied() {
            self.state = ViewportState::Anchored(position);
            self.anchor_excerpt = self
                .last_rows
                .get(target)
                .map(row_excerpt)
                .filter(|text| !text.is_empty());
        }
    }

    fn scroll_down(&mut self, amount: usize) {
        let max_top = self.last_positions.len().saturating_sub(self.last_page_rows);
        let target = self.last_top.saturating_add(amount);
        if target >= max_top {
            self.follow_tail();
        } else if let Some(position) = self.last_positions.get(target).copied() {
            self.state = ViewportState::Anchored(position);
            self.anchor_excerpt = self
                .last_rows
                .get(target)
                .map(row_excerpt)
                .filter(|text| !text.is_empty());
        }
    }
}

fn transcript_positions(rows: &[Row]) -> Vec<TranscriptPosition> {
    let mut banner_row = 0;
    let mut previous_entry = None;
    let mut row_in_entry = 0;
    rows.iter()
        .map(|row| match row.group_id {
            Some(group) => {
                if previous_entry == Some(group.entry_index) {
                    row_in_entry += 1;
                } else {
                    previous_entry = Some(group.entry_index);
                    row_in_entry = 0;
                }
                TranscriptPosition::Entry { entry_index: group.entry_index, row_in_entry }
            }
            None => {
                let position = TranscriptPosition::Banner { row: banner_row };
                banner_row += 1;
                position
            }
        })
        .collect()
}

fn resolve_position(
    positions: &[TranscriptPosition], rows: &[Row], anchor: TranscriptPosition, excerpt: Option<&str>,
) -> Option<usize> {
    positions.iter().position(|position| *position == anchor).or_else(|| {
        match anchor {
        TranscriptPosition::Entry { entry_index, .. } => excerpt
            .and_then(|needle| {
                positions.iter().zip(rows).position(|(position, row)| {
                    matches!(position, TranscriptPosition::Entry { entry_index: candidate, .. } if *candidate == entry_index)
                        && {
                            let candidate = row_excerpt(row);
                            candidate.contains(needle) || needle.contains(&candidate)
                        }
                })
            })
            .or_else(|| positions.iter().position(|position| {
                matches!(position, TranscriptPosition::Entry { entry_index: candidate, .. } if *candidate == entry_index)
            })),
        TranscriptPosition::Banner { .. } => positions.first().map(|_| 0),
    }
    })
}

fn row_excerpt(row: &Row) -> String {
    row.spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>()
        .trim()
        .to_string()
}

fn row_graphemes(row: &Row) -> Vec<String> {
    let text = row.spans.iter().map(|span| span.text.as_str()).collect::<String>();
    UnicodeSegmentation::graphemes(text.trim_end(), true)
        .map(str::to_string)
        .collect()
}

fn selection_ranges(
    selection: TranscriptSelection, positions: &[TranscriptPosition], rows: &[Row],
) -> Option<Vec<(usize, usize, usize)>> {
    let (start_row, start_grapheme, end_row, end_grapheme) = match selection {
        TranscriptSelection::Rows { anchor, head } => {
            let anchor = positions.iter().position(|position| *position == anchor)?;
            let head = positions.iter().position(|position| *position == head)?;
            let (start, end) = if anchor <= head { (anchor, head) } else { (head, anchor) };
            (start, 0, end, row_graphemes(&rows[end]).len())
        }
        TranscriptSelection::Text { anchor, head } => {
            let anchor_row = positions.iter().position(|position| *position == anchor.position)?;
            let head_row = positions.iter().position(|position| *position == head.position)?;
            if (anchor_row, anchor.grapheme) <= (head_row, head.grapheme) {
                (anchor_row, anchor.grapheme, head_row, head.grapheme + 1)
            } else {
                (head_row, head.grapheme, anchor_row, anchor.grapheme + 1)
            }
        }
    };

    Some(
        (start_row..=end_row)
            .map(|row| {
                let length = row_graphemes(&rows[row]).len();
                let start = if row == start_row { start_grapheme.min(length) } else { 0 };
                let end = if row == end_row { end_grapheme.min(length) } else { length };
                (row, start, end.max(start))
            })
            .collect(),
    )
}

fn style_grapheme_range(row: &mut Row, start: usize, end: usize, selection_bg: Color) {
    if start >= end {
        return;
    }
    let mut grapheme_index = 0;
    let mut styled = Vec::<Span>::new();
    for span in &row.spans {
        for grapheme in UnicodeSegmentation::graphemes(span.text.as_str(), true) {
            let style =
                if (start..end).contains(&grapheme_index) { span.style.with_bg(selection_bg) } else { span.style };
            if let Some(previous) = styled.last_mut()
                && previous.style == style
            {
                previous.text.push_str(grapheme);
            } else {
                styled.push(Span::styled(grapheme, style));
            }
            grapheme_index += 1;
        }
    }
    row.spans = styled;
}

fn transcript_search_row(app: &App, rows: &[Row], positions: &[TranscriptPosition]) -> Option<usize> {
    let search = app.overlay.transcript_search()?;
    let current = search.current()?;
    let query = search.query.as_str();
    let ordinal = search.matches[..=search.selected]
        .iter()
        .filter(|found| found.entry_index == current.entry_index)
        .count()
        .saturating_sub(1);
    let mut seen = 0;
    let mut first_entry_row = None;
    for (index, (row, position)) in rows.iter().zip(positions).enumerate() {
        if !matches!(position, TranscriptPosition::Entry { entry_index, .. } if *entry_index == current.entry_index) {
            continue;
        }
        first_entry_row.get_or_insert(index);
        for _ in row_excerpt(row).match_indices(query) {
            if seen == ordinal {
                return Some(index);
            }
            seen += 1;
        }
    }
    first_entry_row
}

/// Rectangles for an app-owned transcript above a bottom-pinned composer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AlternateLayout {
    /// Scrollable transcript viewport.
    pub transcript: Rect,
    /// Bottom-pinned composer, status, and focused prompt surfaces.
    pub composer: Rect,
}

impl AlternateLayout {
    /// Allocate a bottom-pinned composer and give all preceding rows to the
    /// transcript.
    ///
    /// A non-empty terminal always retains at least one composer row. Requested
    /// composer height is clamped to the available height, so tiny terminals
    /// cannot place the composer outside the frame.
    pub fn new(area: Rect, requested_composer_height: u16) -> Self {
        if area.height == 0 {
            return Self { transcript: Rect::new(area.x, area.y, area.width, 0), composer: area };
        }

        let composer_height = requested_composer_height.max(1).min(area.height);
        let transcript_height = area.height.saturating_sub(composer_height);
        let transcript = Rect::new(area.x, area.y, area.width, transcript_height);
        let composer = Rect::new(
            area.x,
            area.y.saturating_add(transcript_height),
            area.width,
            composer_height,
        );
        Self { transcript, composer }
    }
}

/// Render one complete renderer-owned frame through Ratatui.
///
/// The logical frame is bottom-aligned when its height differs from the
/// terminal height. This keeps its composer and cursor pinned while the future
/// alternate-screen projection is being separated into transcript and composer
/// surfaces. Ratatui remains the only writer during the draw.
pub fn render_logical_frame(frame: &mut RatatuiFrame<'_>, logical: &Frame) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if area.width == 0 || area.height == 0 {
        return;
    }

    let visible_height = area.height as usize;
    let source_start = logical.rows.len().saturating_sub(visible_height);
    let visible_rows = &logical.rows[source_start..];
    let destination_start = visible_height.saturating_sub(visible_rows.len());

    for (index, row) in visible_rows.iter().enumerate() {
        let y = area.y.saturating_add((destination_start + index) as u16);
        let row_area = Rect::new(area.x, y, area.width, 1);
        frame.render_widget(Paragraph::new(line_from_row(row)), row_area);
    }

    if !logical.cursor_visible {
        return;
    }
    let Some(cursor) = logical.cursor else {
        return;
    };
    if cursor.row < source_start {
        return;
    }

    let visible_row = cursor.row - source_start + destination_start;
    if visible_row >= visible_height {
        return;
    }
    let x = area
        .x
        .saturating_add((cursor.col as u16).min(area.width.saturating_sub(1)));
    let y = area.y.saturating_add(visible_row as u16);
    frame.set_cursor_position(Position::new(x, y));
}

fn line_from_row(row: &Row) -> Line<'static> {
    let spans = row
        .spans
        .iter()
        .map(|span| RatatuiSpan::styled(span.text.clone(), ratatui_style(span.style)))
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn ratatui_style(style: CellStyle) -> Style {
    let mut result = Style::default().fg(ratatui_color(style.fg)).bg(ratatui_color(style.bg));
    let mut modifiers = Modifier::empty();
    if style.bold {
        modifiers.insert(Modifier::BOLD);
    }
    if style.italic {
        modifiers.insert(Modifier::ITALIC);
    }
    if style.underlined {
        modifiers.insert(Modifier::UNDERLINED);
    }
    if style.dim {
        modifiers.insert(Modifier::DIM);
    }
    result = result.add_modifier(modifiers);
    result
}

fn ratatui_color(color: Color) -> RatatuiColor {
    match color {
        Color::Reset => RatatuiColor::Reset,
        Color::Black => RatatuiColor::Black,
        Color::DarkGrey => RatatuiColor::DarkGray,
        Color::Red => RatatuiColor::LightRed,
        Color::DarkRed => RatatuiColor::Red,
        Color::Green => RatatuiColor::LightGreen,
        Color::DarkGreen => RatatuiColor::Green,
        Color::Yellow => RatatuiColor::LightYellow,
        Color::DarkYellow => RatatuiColor::Yellow,
        Color::Blue => RatatuiColor::LightBlue,
        Color::DarkBlue => RatatuiColor::Blue,
        Color::Magenta => RatatuiColor::LightMagenta,
        Color::DarkMagenta => RatatuiColor::Magenta,
        Color::Cyan => RatatuiColor::LightCyan,
        Color::DarkCyan => RatatuiColor::Cyan,
        Color::White => RatatuiColor::White,
        Color::Grey => RatatuiColor::Gray,
        Color::Rgb { r, g, b } => RatatuiColor::Rgb(r, g, b),
        Color::AnsiValue(value) => RatatuiColor::Indexed(value),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::{Backend, TestBackend};

    use super::*;
    use crate::app::{Entry, ToolStatus};
    use crate::cli::Cli;
    use crate::renderer::row::{CursorCoord, Row};
    use crate::renderer::style::Span;

    #[test]
    fn layout_pins_composer_to_bottom() {
        let area = Rect::new(2, 3, 80, 24);
        let layout = AlternateLayout::new(area, 5);

        assert_eq!(layout.transcript, Rect::new(2, 3, 80, 19));
        assert_eq!(layout.composer, Rect::new(2, 22, 80, 5));
        assert_eq!(layout.composer.bottom(), area.bottom());
    }

    #[test]
    fn growing_composer_only_shrinks_transcript() {
        let area = Rect::new(0, 0, 80, 24);
        let one_row = AlternateLayout::new(area, 1);
        let eight_rows = AlternateLayout::new(area, 8);

        assert_eq!(one_row.composer.bottom(), eight_rows.composer.bottom());
        assert_eq!(one_row.transcript.height, 23);
        assert_eq!(eight_rows.transcript.height, 16);
    }

    #[test]
    fn tiny_terminal_keeps_one_clamped_composer() {
        let area = Rect::new(0, 0, 20, 1);
        let layout = AlternateLayout::new(area, 8);

        assert_eq!(layout.transcript.height, 0);
        assert_eq!(layout.composer, area);
    }

    #[test]
    fn zero_height_terminal_stays_in_bounds() {
        let area = Rect::new(4, 7, 20, 0);
        let layout = AlternateLayout::new(area, 3);

        assert_eq!(layout.transcript, area);
        assert_eq!(layout.composer, area);
    }

    #[test]
    fn adapter_bottom_aligns_clipped_frame_and_translates_cursor() {
        let backend = TestBackend::new(8, 2);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut logical = Frame::new(8);
        logical.push(Row::padded(vec![Span::plain("old")], 8, CellStyle::default()));
        logical.push(Row::padded(vec![Span::plain("prompt")], 8, CellStyle::default()));
        logical.push(Row::padded(
            vec![Span::styled("status", CellStyle::new().fg(Color::Green).bold())],
            8,
            CellStyle::default(),
        ));
        logical.set_cursor(CursorCoord::new(1, 6));

        terminal
            .draw(|frame| render_logical_frame(frame, &logical))
            .expect("draw logical frame");

        let buffer = terminal.backend().buffer();
        let first_row = (0..8).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
        let second_row = (0..8).map(|x| buffer[(x, 1)].symbol()).collect::<String>();
        assert_eq!(first_row, "  prompt");
        assert_eq!(second_row, "  status");
        assert_eq!(buffer[(2, 1)].fg, RatatuiColor::LightGreen);
        assert!(buffer[(2, 1)].modifier.contains(Modifier::BOLD));
        assert_eq!(terminal.backend_mut().get_cursor_position(), Ok(Position::new(6, 0)));
    }

    #[test]
    fn snapshot_composer_full_frames_through_ratatui() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.session.id = "test-session".to_string();
        app.overlay.close();
        app.transcript.entries = vec![
            Entry::User { text: "Explain the renderer migration.".to_string() },
            Entry::Agent { text: "Ratatui now owns the bounded terminal frame.".to_string(), streaming: false },
        ]
        .into();
        app.composer.input.insert_str("Keep the interface quiet");

        let logical = AlternateViewport::default().build_frame(&app, 80, 12);
        let session_row = logical
            .rows
            .iter()
            .position(|row| row.text().contains("test-session"))
            .expect("session row");
        assert!(
            logical.rows[session_row - 1]
                .spans
                .iter()
                .all(|span| span.style.bg == Color::Reset),
            "space above the session label should use the terminal background"
        );
        assert!(
            logical.rows[session_row]
                .spans
                .iter()
                .all(|span| span.style.bg == Color::Reset),
            "session label should use the terminal background"
        );
        for (border_row, corners) in [(session_row + 1, ('╭', '╮')), (session_row + 3, ('╰', '╯'))] {
            assert_eq!(
                logical.rows[border_row].spans.first().map(|span| span.style.bg),
                Some(Color::Reset)
            );
            assert_eq!(
                logical.rows[border_row].spans.last().map(|span| span.style.bg),
                Some(Color::Reset)
            );
            let text = logical.rows[border_row].text();
            assert!(text.contains(corners.0));
            assert!(text.contains(corners.1));
            let border = logical.rows[border_row]
                .spans
                .iter()
                .find(|span| span.text.contains('─'))
                .expect("composer border should contain a horizontal rule");
            assert_eq!(border.style.fg, crate::renderer::style::palette().focus);
        }
        assert!(
            logical.rows[session_row + 2]
                .spans
                .iter()
                .all(|span| span.style.bg == Color::Reset),
            "input row should use the terminal background"
        );

        let mut rendered = String::new();
        for (label, width, height) in [("normal", 80, 12), ("narrow", 32, 9), ("monochrome", 48, 10)] {
            let logical = AlternateViewport::default().build_frame(&app, width, height);
            let text = test_backend_text(&logical, width, height);
            assert!(
                text.contains(['╭', '╮', '╰', '╯', '│']),
                "{label} full frame should preserve the rounded composer:\n{text}"
            );
            rendered.push_str(&format!("{label} ({width}x{height}):\n{text}\n"));
        }

        insta::assert_snapshot!("composer_full_frames", rendered);
    }

    fn test_backend_text(logical: &Frame, width: usize, height: usize) -> String {
        let backend = TestBackend::new(width as u16, height as u16);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render_logical_frame(frame, logical))
            .expect("render logical frame");
        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..height as u16 {
            let line = (0..width as u16).map(|x| buffer[(x, y)].symbol()).collect::<String>();
            rendered.push_str(line.trim_end());
            rendered.push('\n');
        }
        rendered
    }

    #[test]
    fn chronological_projection_does_not_partition_later_settled_entries_first() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries = vec![
            Entry::Agent { text: "first mutable".to_string(), streaming: true },
            Entry::Status { text: "second settled".to_string() },
        ]
        .into();

        let view = RendererView::build(&app, 80, 24);
        let text = view
            .transcript
            .rows
            .iter()
            .flat_map(|row| row.spans.iter().map(|span| span.text.as_str()))
            .collect::<String>();
        let first = text.find("first mutable").expect("mutable entry");
        let second = text.find("second settled").expect("settled entry");
        assert!(first < second, "transcript rows must remain chronological");
    }

    #[test]
    fn projection_cache_refreshes_a_growing_activity_summary() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries.push(Entry::Tool {
            name: "find_files".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["first".to_string()],
        });
        let mut cache = TranscriptProjectionCache::default();
        let first = cache.project(&app, 80);
        assert!(first.rows.iter().any(|row| row.text().contains("1 search")));

        app.transcript.entries.push(Entry::Tool {
            name: "search_text".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["second".to_string()],
        });
        let grown = cache.project(&app, 80);
        assert!(grown.rows.iter().any(|row| row.text().contains("2 searches")));
        assert!(!grown.rows.iter().any(|row| row.text().contains("find_files")));
        assert!(!grown.rows.iter().any(|row| row.text().contains("search_text")));
    }

    #[test]
    fn anchored_reader_stays_on_same_entry_while_new_content_arrives() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries = (0..12)
            .map(|index| Entry::Status { text: format!("entry {index}") })
            .collect();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 48, 12);
        viewport.page_up();
        let anchored = viewport.state();

        app.transcript
            .entries
            .push(Entry::Agent { text: "streaming tail".to_string(), streaming: true });
        viewport.build_frame(&app, 48, 12);

        assert_eq!(viewport.state(), anchored);
        assert!(matches!(
            anchored,
            ViewportState::Anchored(TranscriptPosition::Entry { .. })
        ));
    }

    #[test]
    fn anchored_hint_disappears_after_following_latest() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries = (0..12)
            .map(|index| Entry::Status { text: format!("entry {index}") })
            .collect();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 80, 12);
        viewport.page_up();

        let anchored = viewport.build_frame(&app, 80, 12);
        assert!(anchored.rows.iter().any(|row| row.text().contains("↑ away")));

        viewport.follow_tail();
        let following = viewport.build_frame(&app, 80, 12);
        assert!(!following.rows.iter().any(|row| row.text().contains("↑ away")));
        assert_eq!(viewport.state(), ViewportState::FollowingTail);
    }

    #[test]
    fn anchored_entry_identity_survives_width_changes() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries = (0..8)
            .map(|index| Entry::Status {
                text: format!("entry {index} with enough text to wrap across a narrow viewport"),
            })
            .collect();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 32, 10);
        viewport.half_page_up();
        let before = viewport.state();

        viewport.build_frame(&app, 100, 20);

        let (
            ViewportState::Anchored(TranscriptPosition::Entry { entry_index: before_entry, .. }),
            ViewportState::Anchored(TranscriptPosition::Entry { entry_index: after_entry, .. }),
        ) = (before, viewport.state())
        else {
            panic!("expected an entry anchor before and after resize");
        };
        assert_eq!(before_entry, after_entry);
    }

    #[test]
    fn following_tail_frame_keeps_composer_cursor_in_terminal_bounds() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.composer.input.insert_str("one\ntwo\nthree\nwide 🦀 text");
        let frame = AlternateViewport::default().build_frame(&app, 24, 9);

        assert_eq!(frame.rows.len(), 9);
        let cursor = frame.cursor.expect("prompt cursor");
        assert!(cursor.row < 9);
        assert!(cursor.col < 24);
    }

    #[test]
    fn keyboard_selection_crosses_wrapped_entry_boundaries_and_preserves_unicode() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries = vec![
            Entry::User { text: "first wrapped 🦀 transcript line".to_string() },
            Entry::Agent { text: "second block".to_string(), streaming: false },
        ]
        .into();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 14, 40);

        for _ in 0..50 {
            viewport.handle_navigation(&app, &Action::ExtendTranscriptSelectionDown);
        }

        let selected = viewport.selected_text().expect("selected transcript text");
        assert!(selected.contains('🦀'));
        assert!(
            selected.contains("second"),
            "selection should cross into the next transcript block"
        );

        let frame = viewport.build_frame(&app, 14, 40);
        let selection_style = super::super::style::palette().border;
        assert!(frame.rows.iter().any(|row| {
            row.spans
                .iter()
                .any(|span| span.style.bg == selection_style && !span.style.underlined)
        }));
        assert!(
            frame
                .rows
                .iter()
                .filter(|row| { row.spans.iter().any(|span| span.style.bg == selection_style) })
                .any(|row| row.spans.iter().any(|span| span.style.bg != selection_style))
        );
    }

    #[test]
    fn mouse_drag_selects_exact_unicode_text_with_native_style() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries =
            vec![Entry::Agent { text: "before 🦀 native after".to_string(), streaming: false }].into();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 80, 40);

        let phrase = "🦀 native";
        let row_index = viewport
            .last_rows
            .iter()
            .position(|row| row.text().contains(phrase))
            .expect("phrase row");
        let row_text = viewport.last_rows[row_index].text();
        let phrase_byte = row_text.find(phrase).expect("phrase column");
        let start_column = UnicodeWidthStr::width(&row_text[..phrase_byte]);
        let end_column = start_column + UnicodeWidthStr::width(phrase) - 1;
        let screen_row = viewport.last_content_top + row_index - viewport.last_top;

        viewport.handle_navigation(
            &app,
            &Action::BeginTranscriptSelection { column: end_column as u16, row: screen_row as u16 },
        );
        viewport.handle_navigation(
            &app,
            &Action::UpdateTranscriptSelection { column: start_column as u16, row: screen_row as u16 },
        );
        viewport.handle_navigation(
            &app,
            &Action::EndTranscriptSelection { column: start_column as u16, row: screen_row as u16 },
        );

        assert_eq!(viewport.selected_text().as_deref(), Some(phrase));
        let frame = viewport.build_frame(&app, 80, 40);
        let selection_bg = super::super::style::palette().border;
        let selected_spans = frame.rows[screen_row]
            .spans
            .iter()
            .filter(|span| span.style.bg == selection_bg)
            .collect::<Vec<_>>();
        assert_eq!(
            selected_spans.iter().map(|span| span.text.as_str()).collect::<String>(),
            phrase
        );
        assert!(selected_spans.iter().all(|span| !span.style.underlined));
        assert!(
            frame.rows[screen_row]
                .spans
                .iter()
                .any(|span| span.style.bg != selection_bg)
        );
    }

    #[test]
    fn focused_overlay_keeps_page_navigation_for_itself() {
        let cli = Cli { model: "fake-agent".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.overlay.show_help();
        let mut viewport = AlternateViewport::default();
        let action = Action::ScrollTranscriptUp;

        assert!(!viewport.handle_navigation(&app, &action));
        assert_eq!(viewport.state(), ViewportState::FollowingTail);

        app.overlay.close();
        viewport.build_frame(&app, 80, 12);
        assert!(viewport.handle_navigation(&app, &action));
    }

    #[test]
    fn mouse_wheel_scrolls_transcript_without_recalling_prompt_history() {
        let cli = Cli { model: "fake-agent".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.composer.input_history.push("previous prompt".to_string());
        app.composer.input.insert_str("current draft");
        app.transcript.entries = (0..20)
            .map(|index| Entry::Status { text: format!("entry {index}") })
            .collect();

        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 80, 12);
        let initial_top = viewport.last_top;
        let action = Action::ScrollTranscriptWheelUp;

        assert!(viewport.handle_navigation(&app, &action));
        viewport.build_frame(&app, 80, 12);
        assert!(matches!(viewport.state(), ViewportState::Anchored(_)));
        assert_eq!(viewport.last_top, initial_top.saturating_sub(MOUSE_WHEEL_ROWS));
        assert_eq!(app.composer.input.as_str(), "current draft");
        assert_eq!(app.composer.history_cursor, None);
    }

    #[test]
    fn submitted_entry_does_not_move_an_anchored_reader() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries = (0..10)
            .map(|index| Entry::Status { text: format!("entry {index}") })
            .collect();
        let mut viewport = AlternateViewport::default();
        viewport.build_frame(&app, 40, 10);
        viewport.page_up();
        let anchor = viewport.state();

        app.transcript
            .entries
            .push(Entry::User { text: "new submission".to_string() });
        viewport.build_frame(&app, 40, 10);

        assert_eq!(viewport.state(), anchor);
    }
}
