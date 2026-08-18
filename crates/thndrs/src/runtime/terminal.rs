//! Inline Ratatui terminal coordinator used by the interactive runtime.

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::Duration;

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, CrosstermBackend};
use ratatui::layout::Position;
use ratatui::{TerminalOptions, Viewport};

use super::*;
use crate::renderer::inline::ScrollbackCommitter;
use crate::renderer::live_surface::LiveSurfaceLayout;
use crate::renderer::ratatui::{render_logical_frame, render_rows_to_buffer};
use crate::renderer::row::Frame;
use crate::renderer::view::{LiveView, SemanticUiView};

pub(crate) trait InteractiveSurface {
    fn draw(&mut self, app: &mut App, full_repaint: bool) -> io::Result<()>;
    fn clear(&mut self) -> io::Result<()>;
    fn suspend(&mut self) -> io::Result<()>;
    fn handle_navigation(&mut self, app: &mut App, action: &Action) -> bool;
}

/// Owns the terminal modes required by normal-screen inline interaction.
///
/// Mouse capture and the alternate screen are intentionally absent so the
/// terminal keeps native scrollback, selection, and copy behavior.
#[derive(Debug)]
pub(crate) struct InlineTerminalSession {
    active: bool,
}

impl InlineTerminalSession {
    pub(crate) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let result = execute!(
            io::stdout(),
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
            SetCursorStyle::BlinkingBlock
        );
        if let Err(error) = result {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self { active: true })
    }

    pub(crate) fn suspend(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        drain_terminal_events();
        restore_terminal()?;
        self.active = false;
        Ok(())
    }

    fn resume(&mut self) -> io::Result<()> {
        if self.active {
            return Ok(());
        }
        enable_raw_mode()?;
        let result = execute!(
            io::stdout(),
            EnableBracketedPaste,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES),
            SetCursorStyle::BlinkingBlock
        );
        if result.is_ok() {
            self.active = true;
        } else {
            let _ = disable_raw_mode();
        }
        result
    }
}

impl Drop for InlineTerminalSession {
    fn drop(&mut self) {
        if self.active {
            drain_terminal_events();
            let _ = restore_terminal();
            self.active = false;
        }
    }
}

/// Cloneable writer handle used to recreate Ratatui's inline viewport at its
/// current height. Ratatui fixes an inline viewport's height at construction,
/// while this application needs the height to follow its mutable bottom pane.
struct SharedWriter<W>(Rc<RefCell<W>>);

impl<W> Clone for SharedWriter<W> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<W: Write> Write for SharedWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .try_borrow_mut()
            .map_err(|_| io::Error::other("terminal writer is already borrowed"))?
            .write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .try_borrow_mut()
            .map_err(|_| io::Error::other("terminal writer is already borrowed"))?
            .flush()
    }
}

/// One terminal coordinator owns native-scrollback insertion, dynamic live
/// viewport reservation, cursor placement, and flush order.
pub(crate) struct RatatuiSurface<W: io::Write> {
    writer: SharedWriter<W>,
    terminal: Option<Terminal<CrosstermBackend<SharedWriter<W>>>>,
    scrollback: ScrollbackCommitter,
    live_height: u16,
    pub(crate) terminal_session: InlineTerminalSession,
}

impl<W: io::Write> RatatuiSurface<W> {
    pub(crate) fn new(writer: W, terminal_session: InlineTerminalSession) -> Self {
        Self {
            writer: SharedWriter(Rc::new(RefCell::new(writer))),
            terminal: None,
            scrollback: ScrollbackCommitter::default(),
            live_height: 1,
            terminal_session,
        }
    }

    fn terminal_mut(&mut self, height: u16) -> io::Result<&mut Terminal<CrosstermBackend<SharedWriter<W>>>> {
        let height = height.max(1);
        if self.terminal.is_none() || self.live_height != height {
            // Ratatui fixes an inline viewport's height at construction. Clear
            // only the old mutable pane before replacing it so stale composer,
            // picker, or streaming rows cannot become transcript history.
            // `Terminal::clear` would erase the screen below the old viewport,
            // including visible native history.
            if let Some(terminal) = self.terminal.as_mut() {
                clear_live_viewport(terminal)?;
            }
            self.terminal = Some(Terminal::with_options(
                CrosstermBackend::new(self.writer.clone()),
                TerminalOptions { viewport: Viewport::Inline(height) },
            )?);
            self.live_height = height;
        }
        // The branch above guarantees the terminal is present.
        Ok(self.terminal.as_mut().expect("inline terminal is initialized"))
    }

    /// Leave a blank terminal row for the shell after the live inline surface.
    pub(crate) fn finish(&mut self) -> io::Result<()> {
        let height = self.live_height;
        finish_inline_viewport(self.terminal_mut(height)?)
    }
}

/// Scroll the fully rendered live surface out of the shell's input row.
///
/// An inline viewport's cursor remains in the composer. Moving it to the last
/// row and appending a line keeps the next shell prompt separate from that
/// composer when the application exits.
fn clear_live_viewport<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), B::Error> {
    let area = terminal.get_frame().area();
    let cursor = terminal.backend_mut().get_cursor_position()?;
    for y in area.top()..area.bottom() {
        terminal.backend_mut().set_cursor_position(Position::new(area.x, y))?;
        terminal.backend_mut().clear_region(ClearType::CurrentLine)?;
    }
    terminal.backend_mut().set_cursor_position(cursor)
}

fn finish_inline_viewport<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), B::Error> {
    let area = terminal.get_frame().area();
    if area.height == 0 {
        return Ok(());
    }
    terminal.set_cursor_position(Position::new(area.x, area.bottom() - 1))?;
    terminal.backend_mut().append_lines(1)
}

impl<W: io::Write> InteractiveSurface for RatatuiSurface<W> {
    fn draw(&mut self, app: &mut App, full_repaint: bool) -> io::Result<()> {
        let projection_started = Instant::now();
        renderer::style::set_theme(app.runtime.theme);
        let (width, _) = crossterm::terminal::size()?;
        let plan = self.scrollback.newly_stable(app, width as usize);
        let mutable_tail = self.scrollback.mutable_tail_rows(app, width as usize);
        let logical = bottom_pane_frame(app, width as usize, mutable_tail);
        let desired_height = logical.rows.len().clamp(1, u16::MAX as usize) as u16;
        let projection_elapsed = projection_started.elapsed();
        let terminal = self.terminal_mut(desired_height)?;

        if full_repaint {
            terminal.clear()?;
        }
        let committed_rows = plan
            .commits
            .iter()
            .flat_map(|commit| commit.rows.iter().cloned())
            .collect::<Vec<_>>();
        if !committed_rows.is_empty() {
            terminal.insert_before(committed_rows.len() as u16, |buffer| {
                render_rows_to_buffer(&committed_rows, buffer);
            })?;
        }
        let draw_started = Instant::now();
        terminal.draw(|frame| render_logical_frame(frame, &logical))?;
        self.scrollback.mark_committed(&plan.commits);
        tracing::trace!(
            projection_us = projection_elapsed.as_micros(),
            draw_us = draw_started.elapsed().as_micros(),
            width,
            live_height = self.live_height,
            committed_blocks = plan.commits.len(),
            "inline ratatui frame timing"
        );
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.scrollback.clear();
        let height = self.live_height;
        self.terminal_mut(height)?.clear()
    }

    fn suspend(&mut self) -> io::Result<()> {
        // Clear only Ratatui's mutable inline viewport before the shell or a
        // child process takes over. Committed transcript blocks stay in native
        // history and are never hydrated or replayed on resume.
        let height = self.live_height;
        self.terminal_mut(height)?.clear()?;
        self.terminal_session.suspend()?;
        let status = std::process::Command::new("kill")
            .args(["-TSTP", &std::process::id().to_string()])
            .status();
        let resume = self.terminal_session.resume();
        resume?;
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => Err(io::Error::other("failed to suspend process")),
            Err(error) => Err(error),
        }
    }

    fn handle_navigation(&mut self, _app: &mut App, _action: &Action) -> bool {
        false
    }
}

fn bottom_pane_frame(app: &App, width: usize, mut mutable_tail: Vec<crate::renderer::row::Row>) -> Frame {
    if app.transcript.entries.is_empty() {
        mutable_tail = app.render_banner_rows(width);
        if !matches!(app.overlay.accessory(), crate::app::PromptAccessory::None) {
            mutable_tail.truncate(1);
        }
    }

    let semantic = SemanticUiView::from(app);
    let live = LiveView::build(app, width, u16::MAX as usize, &semantic);
    let chrome = LiveSurfaceLayout::build(&live, width).into_frame();

    let mut frame = Frame::new(width);
    frame.rows.append(&mut mutable_tail);
    let chrome_offset = frame.rows.len();
    frame.rows.extend(chrome.rows);
    frame.cursor = chrome.cursor.map(|mut cursor| {
        cursor.row += chrome_offset;
        cursor
    });
    frame.cursor_visible = !matches!(
        app.prompt_state(),
        crate::app::PromptState::Stopped | crate::app::PromptState::Errored
    );
    frame
}

fn drain_terminal_events() {
    while crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
        if crossterm::event::read().is_err() {
            break;
        }
    }
}

fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        Show,
        SetCursorStyle::DefaultUserShape,
        PopKeyboardEnhancementFlags,
        DisableBracketedPaste
    )?;
    stdout.flush()?;
    disable_raw_mode()
}

#[cfg(test)]
mod tests {
    use ratatui::TerminalOptions;
    use ratatui::Viewport;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::app::Entry;
    use crate::renderer::row::Row;
    use crate::renderer::style::{CellStyle, Span};

    #[test]
    fn inline_viewport_inserts_completed_rows_without_retaining_them_in_the_live_frame() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries.clear();
        app.transcript
            .entries
            .push(Entry::User { text: "committed once".to_string() });

        let mut coordinator = ScrollbackCommitter::default();
        let plan = coordinator.newly_stable(&app, 40);
        let frame = bottom_pane_frame(&app, 40, coordinator.mutable_tail_rows(&app, 40));
        assert!(frame.rows.iter().all(|row| !row.text().contains("committed once")));

        let mut terminal = Terminal::with_options(
            TestBackend::new(40, 8),
            TerminalOptions { viewport: Viewport::Inline(3) },
        )
        .expect("inline terminal");
        let rows = plan
            .commits
            .iter()
            .flat_map(|commit| commit.rows.iter().cloned())
            .collect::<Vec<_>>();
        terminal
            .insert_before(rows.len() as u16, |buffer| render_rows_to_buffer(&rows, buffer))
            .expect("insert transcript rows");
        terminal
            .draw(|ratatui_frame| render_logical_frame(ratatui_frame, &frame))
            .expect("draw mutable surface");
        coordinator.mark_committed(&plan.commits);

        assert!(coordinator.newly_stable(&app, 20).commits.is_empty());
    }

    #[test]
    fn inline_viewport_leaves_room_for_recent_history_on_tall_terminals() {
        let mut terminal = Terminal::with_options(
            TestBackend::new(40, 30),
            TerminalOptions { viewport: Viewport::Inline(3) },
        )
        .expect("inline terminal");
        let rows = vec![Row::padded(
            vec![Span::plain("recent transcript")],
            40,
            CellStyle::default(),
        )];

        terminal
            .insert_before(1, |buffer| render_rows_to_buffer(&rows, buffer))
            .expect("insert transcript row");

        let visible = terminal
            .backend()
            .buffer()
            .content
            .chunks(40)
            .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        assert!(visible.iter().any(|line| line.contains("recent transcript")));
    }

    #[test]
    fn ctrl_d_confirmation_keeps_the_welcome_and_stays_visible() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.show_status_toast("Press CTRL+D again to quit.", crate::app::StatusToastKind::Warning);
        let committer = ScrollbackCommitter::default();
        let plan = committer.newly_stable(&app, 80);
        let frame = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));
        let text = frame.rows.iter().map(|row| row.text()).collect::<Vec<_>>().join("\n");

        assert!(
            plan.commits.is_empty(),
            "exit confirmation must not commit a transcript block"
        );
        assert!(text.contains("thndrs / ready"));
        assert!(text.contains("Press CTRL+D again to quit."));
    }

    #[test]
    fn empty_picker_keeps_welcome_identity_and_surface_room() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries.clear();
        app.overlay
            .show_picker(
                crate::app::PromptAccessory::Models,
                crate::app::PickerState::new(
                    vec![
                        crate::app::PickerItem::new("model/a", "first"),
                        crate::app::PickerItem::new("model/b", "second"),
                    ],
                    10,
                ),
            )
            .expect("model picker opens");

        let committer = ScrollbackCommitter::default();
        let frame = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));
        let text = frame.rows.iter().map(|row| row.text()).collect::<Vec<_>>().join("\n");

        assert!(text.contains("thndrs / ready"));
        assert!(text.contains("MODELS"));
        assert!(text.contains("model/a"));
        assert!(text.contains("model/b"));
    }

    #[test]
    fn autocomplete_is_rendered_below_the_composer_and_never_committed() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.transcript.entries.clear();
        app.composer.input.set_text("/");
        app.overlay.show_commands();

        let committer = ScrollbackCommitter::default();
        let frame = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));
        let prompt = frame
            .rows
            .iter()
            .position(|row| row.text().contains('❯'))
            .expect("command draft row");
        let suggestions = frame
            .rows
            .iter()
            .position(|row| row.text().contains("COMMANDS"))
            .expect("command suggestions");

        assert!(suggestions > prompt, "suggestions must follow the composer");

        app.overlay.close();
        app.composer.input.set_text("@src");
        app.overlay
            .show_picker(
                crate::app::PromptAccessory::Files(crate::app::FilePickerSource::Forced),
                crate::app::PickerState::new(vec![crate::app::PickerItem::new("src/main.rs", "main entry")], 10),
            )
            .expect("file picker opens");
        let file_frame = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));
        let file_prompt = file_frame
            .rows
            .iter()
            .position(|row| row.text().contains('❯'))
            .expect("file draft row");
        let files = file_frame
            .rows
            .iter()
            .position(|row| row.text().contains("PATHS"))
            .expect("file suggestions");

        assert!(files > file_prompt, "file suggestions must follow the composer");
        assert!(committer.newly_stable(&app, 80).commits.is_empty());
    }

    #[test]
    fn picker_growth_only_changes_the_bottom_pane_height() {
        let mut app = App::from_cli(&Cli::default());
        app.session.writer = None;
        app.overlay.close();
        app.transcript.entries.clear();
        app.transcript
            .entries
            .push(Entry::User { text: "committed".to_string() });
        let mut committer = ScrollbackCommitter::default();
        let commits = committer.newly_stable(&app, 80);
        committer.mark_committed(&commits.commits);
        let idle = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));

        app.overlay
            .show_picker(
                crate::app::PromptAccessory::Models,
                crate::app::PickerState::new(
                    vec![
                        crate::app::PickerItem::new("model/a", "first"),
                        crate::app::PickerItem::new("model/b", "second"),
                    ],
                    10,
                ),
            )
            .expect("model picker opens");
        let picker = bottom_pane_frame(&app, 80, committer.mutable_tail_rows(&app, 80));

        assert!(
            picker.rows.len() > idle.rows.len(),
            "picker should request temporary live-region height"
        );
        assert!(committer.newly_stable(&app, 80).commits.is_empty());
    }

    #[test]
    fn clearing_a_replaced_live_viewport_preserves_visible_history() {
        let mut terminal = Terminal::with_options(
            TestBackend::with_lines(["history", "live 0", "live 1", "shell"]),
            TerminalOptions { viewport: Viewport::Fixed(ratatui::layout::Rect::new(0, 1, 7, 2)) },
        )
        .expect("fixed terminal");

        clear_live_viewport(&mut terminal).expect("clear live viewport");

        terminal
            .backend()
            .assert_buffer_lines(["history", "       ", "       ", "shell  "]);
    }

    #[test]
    fn finishing_inline_viewport_leaves_a_blank_shell_row() {
        let mut terminal = Terminal::with_options(
            TestBackend::with_lines(["live 0", "live 1", "live 2"]),
            TerminalOptions { viewport: Viewport::Inline(3) },
        )
        .expect("inline terminal");

        finish_inline_viewport(&mut terminal).expect("finish inline viewport");

        terminal.backend().assert_buffer_lines(["live 1", "live 2", "      "]);
        terminal.backend().assert_scrollback_lines(["live 0"]);
    }
}
