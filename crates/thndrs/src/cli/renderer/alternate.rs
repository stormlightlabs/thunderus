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

use crossterm::cursor::Show;
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Frame as RatatuiFrame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color as RatatuiColor, Modifier, Style};
use ratatui::text::{Line, Span as RatatuiSpan};
use ratatui::widgets::{Clear, Paragraph};

use crate::app::{Action, App, Entry, PromptAccessory, PromptState};

use super::row::{Frame, Row};
use super::style::{CellStyle, Color, Span};
use super::view::{LiveView, RendererView, SemanticUiView, TranscriptView, project_transcript_entry};

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
                EnableMouseCapture
            )
        } else {
            execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)
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
                EnableMouseCapture
            )
        } else {
            execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)
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
            DisableMouseCapture,
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
    } else {
        execute!(stdout, Show, DisableBracketedPaste, LeaveAlternateScreen)?;
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
    transcript_cache: TranscriptProjectionCache,
    last_positions: Vec<TranscriptPosition>,
    last_top: usize,
    last_page_rows: usize,
}

#[derive(Clone, Debug)]
struct CachedEntryProjection {
    source: Entry,
    width: usize,
    tool_group_start: bool,
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
            let tool_group_start = entry_index
                .checked_sub(1)
                .and_then(|index| app.transcript.entries.get(index))
                .is_none_or(|entry| !matches!(entry, Entry::Tool { .. }));
            let reusable = self.entries.get(entry_index).is_some_and(|cached| {
                cached.width == width && cached.tool_group_start == tool_group_start && cached.source == *entry
            });
            if reusable {
                continue;
            }
            let (stable_rows, live_rows) = project_transcript_entry(app, entry_index, width);
            let projection =
                CachedEntryProjection { source: entry.clone(), width, tool_group_start, stable_rows, live_rows };
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
            Action::ScrollTranscriptUp => self.line_up(),
            Action::ScrollTranscriptDown => self.line_down(),
            Action::ScrollTranscriptHalfUp => self.half_page_up(),
            Action::ScrollTranscriptHalfDown => self.half_page_down(),
            Action::TranscriptTop => self.top(),
            Action::TranscriptFollowTail => self.follow_tail(),
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

    /// Build one complete terminal-sized logical frame.
    pub fn build_frame(&mut self, app: &App, width: usize, height: usize) -> Frame {
        let semantic = SemanticUiView::from(app);
        let transcript = self.transcript_cache.project(app, width);
        let live = LiveView::build(app, width, height, &transcript, &semantic);
        let view = RendererView { semantic, transcript, live, width, height };
        let anchored = matches!(self.state, ViewportState::Anchored(_));
        let chrome = build_chrome_frame(&view, anchored);
        let chrome_height = chrome.rows.len().min(height);
        let transcript_height = height.saturating_sub(chrome_height);
        let rows = &view.transcript.rows;
        let positions = transcript_positions(rows);
        let max_top = rows.len().saturating_sub(transcript_height);
        let top = match self.state {
            ViewportState::FollowingTail => max_top,
            ViewportState::Anchored(anchor) => resolve_position(&positions, anchor).unwrap_or(max_top).min(max_top),
        };

        self.last_positions = positions;
        self.last_top = top;
        self.last_page_rows = transcript_height.max(1);

        let mut frame = Frame::new(width);
        let visible_end = top.saturating_add(transcript_height).min(rows.len());
        let visible = &rows[top.min(rows.len())..visible_end];
        for _ in 0..transcript_height.saturating_sub(visible.len()) {
            frame.push(Row::blank(width, CellStyle::new()));
        }
        frame.rows.extend(visible.iter().cloned());

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
        }
    }

    fn scroll_down(&mut self, amount: usize) {
        let max_top = self.last_positions.len().saturating_sub(self.last_page_rows);
        let target = self.last_top.saturating_add(amount);
        if target >= max_top {
            self.follow_tail();
        } else if let Some(position) = self.last_positions.get(target).copied() {
            self.state = ViewportState::Anchored(position);
        }
    }
}

/// Build the bottom-pinned prompt, focused surface, queue summary, and footer.
fn build_chrome_frame(view: &RendererView, anchored: bool) -> Frame {
    let width = view.width;
    let height = view.height;
    let live = &view.live;
    let surface_bg = CellStyle::new().bg(super::style::palette().panel_bg);
    let min_prompt_chrome = live.prompt_rows.len() + 1;
    let keep_prompt_gutters = height >= min_prompt_chrome + 3;

    let mut footer = Vec::new();
    if anchored {
        let p = super::style::palette();
        footer.push(Row::padded(
            vec![Span::styled(
                "  ↑ Viewing earlier output · Ctrl+End to follow",
                CellStyle::new().fg(p.teal).bg(p.panel_bg),
            )],
            width,
            CellStyle::new().bg(p.panel_bg),
        ));
    }
    footer.push(live.static_status.clone());
    if keep_prompt_gutters {
        footer.push(Row::blank(width, surface_bg));
    }
    let prompt_gutter =
        keep_prompt_gutters.then(|| Row::blank(width, CellStyle::new().bg(super::style::palette().panel_bg)));
    let accessory = if live.detail_pane.is_empty() { live.accessory_rows.clone() } else { live.detail_pane.clone() };
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
    frame
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

fn resolve_position(positions: &[TranscriptPosition], anchor: TranscriptPosition) -> Option<usize> {
    positions.iter().position(|position| *position == anchor).or_else(|| {
        match anchor {
        TranscriptPosition::Entry { entry_index, .. } => positions.iter().position(|position| {
            matches!(position, TranscriptPosition::Entry { entry_index: candidate, .. } if *candidate == entry_index)
        }),
        TranscriptPosition::Banner { .. } => positions.first().map(|_| 0),
    }
    })
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
    use crate::app::Entry;
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
    fn snapshot_borderless_full_frames_through_ratatui() {
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

        let mut rendered = String::new();
        for (label, width, height) in [("normal", 80, 12), ("narrow", 32, 9), ("monochrome", 48, 10)] {
            let logical = AlternateViewport::default().build_frame(&app, width, height);
            let text = test_backend_text(&logical, width, height);
            assert!(
                !text.contains(['╭', '╮', '╰', '╯', '│', '─']),
                "{label} full frame should be borderless:\n{text}"
            );
            rendered.push_str(&format!("{label} ({width}x{height}):\n{text}\n"));
        }

        insta::assert_snapshot!("borderless_full_frames", rendered);
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
        assert!(
            anchored
                .rows
                .iter()
                .any(|row| row.text().contains("Ctrl+End to follow"))
        );

        viewport.follow_tail();
        let following = viewport.build_frame(&app, 80, 12);
        assert!(!following.rows.iter().any(|row| row.text().contains("End to follow")));
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
        let action = Action::ScrollTranscriptUp;

        assert!(viewport.handle_navigation(&app, &action));
        assert!(matches!(viewport.state(), ViewportState::Anchored(_)));
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
