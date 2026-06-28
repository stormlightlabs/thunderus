//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The bin in [`main.rs`] just calls [`run`].

mod app;
pub mod cli;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent};
use ratatui::init::DefaultTerminal;

use crate::app::{App, Msg, update};
use crate::cli::Cli;

/// Run the TUI to completion using the given CLI configuration.
///
/// Sets up the terminal (alternate screen unless `--no-alt-screen`), drives the
/// draw loop, polls events on a tick, and restores the terminal on exit.
///
/// If the `--no-alt-screen` flag is set, the alternate screen buffer is skipped
/// so the app draws inline — useful for debugging and terminal-capture tests.
pub fn run(cli: &Cli) -> io::Result<()> {
    let tick = Duration::from_millis(cli.tick_rate_ms);
    if cli.no_alt_screen { run_inline(tick, cli) } else { run_alt_screen(tick, cli) }
}

/// `ratatui::init` enables raw mode, enters the alternate screen, installs a
/// panic hook that restores the terminal, and returns a [`DefaultTerminal`].
///
/// We always restore the terminal, even on error.
fn run_alt_screen(tick: Duration, cli: &Cli) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = main_loop(&mut terminal, tick, cli);
    ratatui::restore();
    result
}

/// Inline has no alternate screen so we enable raw mode only & restore manually.
fn run_inline(tick: Duration, cli: &Cli) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;
    let result = main_loop(&mut terminal, tick, cli);
    crossterm::terminal::disable_raw_mode()?;
    result
}

/// 1. Initial draw so the shell is visible before the first event.
/// 2. Poll for events until the tick deadline, draining all pending events.
/// 3. Tick.
fn main_loop(terminal: &mut DefaultTerminal, tick: Duration, cli: &Cli) -> io::Result<()> {
    let mut app = App::from_cli(cli);
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        let deadline = Instant::now() + tick;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !event::poll(remaining)? {
                break;
            }
            match event::read()? {
                Event::Key(key) => handle_key(&mut app, key, terminal)?,
                Event::Resize(_, _) => {
                    terminal.draw(|f| ui::render(f, &app))?;
                }
                _ => {}
            }
            if app.quit {
                return Ok(());
            }
        }
        handle_msg(&mut app, Msg::Tick, terminal)?;
        if app.quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent, terminal: &mut DefaultTerminal) -> io::Result<()> {
    handle_msg(app, Msg::Key(key), terminal)
}

/// Process the message and any chained follow-ups.
fn handle_msg(app: &mut App, msg: Msg, terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut next = Some(msg);
    while let Some(m) = next {
        next = update(app, m);
        if app.quit {
            return Ok(());
        }
    }
    terminal.draw(|f| ui::render(f, app))?;
    Ok(())
}
