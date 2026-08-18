//! Inline Ratatui terminal coordinator used by the interactive runtime.

use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Position;

use super::*;
use crate::renderer::inline::{InlineTranscript, InlineTranscriptPlan};
use crate::renderer::live_surface::LiveSurfaceLayout;
use crate::renderer::ratatui::{render_logical_frame, render_rows_to_buffer};
use crate::renderer::row::Frame;
use crate::renderer::view::{LiveView, SemanticUiView, TranscriptView};

/// Height of the permanent inline live region.
///
/// Sized to the composer at its largest (including its border chrome), the
/// status footer, and one blank gutter row, so normal operation does not
/// reserve setup/auth-scale blank space. Setup, detail, and picker surfaces
/// clip within this region.
pub(crate) const INLINE_VIEWPORT_HEIGHT: u16 = crate::renderer::live::LIVE_REGION_HEIGHT as u16;

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

/// One terminal coordinator owns viewport reservation, transcript insertion,
/// live drawing, cursor placement, and flush order.
pub(crate) struct RatatuiSurface<W: io::Write> {
    terminal: Terminal<CrosstermBackend<W>>,
    transcript: InlineTranscript,
    pub(crate) terminal_session: InlineTerminalSession,
}

impl<W: io::Write> RatatuiSurface<W> {
    pub(crate) fn new(terminal: Terminal<CrosstermBackend<W>>, terminal_session: InlineTerminalSession) -> Self {
        Self { terminal, transcript: InlineTranscript::default(), terminal_session }
    }

    /// Leave a blank terminal row for the shell after the live inline surface.
    pub(crate) fn finish(&mut self) -> io::Result<()> {
        finish_inline_viewport(&mut self.terminal)
    }
}

/// Scroll the fully rendered live surface out of the shell's input row.
///
/// An inline viewport's cursor remains in the composer. Moving it to the last
/// row and appending a line keeps the next shell prompt separate from that
/// composer when the application exits.
fn finish_inline_viewport<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), B::Error> {
    let area = terminal.size()?;
    if area.height == 0 {
        return Ok(());
    }
    terminal.set_cursor_position(Position::new(0, area.height - 1))?;
    terminal.backend_mut().append_lines(1)
}

impl<W: io::Write> InteractiveSurface for RatatuiSurface<W> {
    fn draw(&mut self, app: &mut App, full_repaint: bool) -> io::Result<()> {
        if full_repaint {
            self.terminal.clear()?;
        }
        let projection_started = Instant::now();
        renderer::style::set_theme(app.runtime.theme);
        let size = self.terminal.size()?;
        let plan = self.transcript.plan(app, size.width as usize);
        let projection_elapsed = projection_started.elapsed();

        let committed_rows = plan
            .commits
            .iter()
            .flat_map(|commit| commit.rows.iter().cloned())
            .collect::<Vec<_>>();
        if !committed_rows.is_empty() {
            self.terminal.insert_before(committed_rows.len() as u16, |buffer| {
                render_rows_to_buffer(&committed_rows, buffer);
            })?;
        }
        let draw_started = Instant::now();
        self.terminal.draw(|frame| {
            let area = frame.area();
            let logical = inline_frame(app, area.width as usize, area.height as usize, &plan);
            render_logical_frame(frame, &logical);
        })?;
        self.transcript.mark_committed(&plan.commits);
        tracing::trace!(
            projection_us = projection_elapsed.as_micros(),
            draw_us = draw_started.elapsed().as_micros(),
            width = size.width,
            height = size.height,
            committed_blocks = plan.commits.len(),
            "inline ratatui frame timing"
        );
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        self.transcript.reset();
        self.terminal.clear()
    }

    fn suspend(&mut self) -> io::Result<()> {
        // Clear only Ratatui's mutable inline viewport before the shell or a
        // child process takes over. Committed transcript blocks stay in native
        // history and are never hydrated or replayed on resume.
        self.terminal.clear()?;
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

fn inline_frame(app: &App, width: usize, height: usize, plan: &InlineTranscriptPlan) -> Frame {
    let semantic = SemanticUiView::from(app);
    let transcript = TranscriptView {
        rows: Vec::new(),
        banner_rows: Vec::new(),
        stable_rows: Vec::new(),
        live_rows: plan.live_rows.clone(),
    };
    let live = LiveView::build(app, width, height, &transcript, &semantic);
    let chrome = LiveSurfaceLayout::build(&live, width, height).into_frame();

    let mut frame = Frame::new(width);
    frame.rows.extend(plan.live_rows.iter().cloned());
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

        let mut coordinator = InlineTranscript::default();
        let plan = coordinator.plan(&app, 40);
        let frame = inline_frame(&app, 40, 8, &plan);
        assert!(frame.rows.iter().all(|row| !row.text().contains("committed once")));

        let mut terminal = Terminal::with_options(
            TestBackend::new(40, 8),
            TerminalOptions { viewport: Viewport::Inline(INLINE_VIEWPORT_HEIGHT) },
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

        assert!(coordinator.plan(&app, 20).commits.is_empty());
    }

    #[test]
    fn inline_viewport_leaves_room_for_recent_history_on_tall_terminals() {
        let mut terminal = Terminal::with_options(
            TestBackend::new(40, 30),
            TerminalOptions { viewport: Viewport::Inline(INLINE_VIEWPORT_HEIGHT) },
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
        let plan = InlineTranscript::default().plan(&app, 80);
        let frame = inline_frame(&app, 80, 12, &plan);
        let text = frame.rows.iter().map(|row| row.text()).collect::<Vec<_>>().join("\n");

        assert!(
            plan.commits.is_empty(),
            "exit confirmation must not commit a transcript block"
        );
        assert!(text.contains("thndrs / ready"));
        assert!(text.contains("Press CTRL+D again to quit."));
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
