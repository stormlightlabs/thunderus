//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The bin in [`main.rs`] just calls [`run`].

pub mod cli;

mod agent;
mod app;
mod context;
mod tools;
mod ui;

use std::io;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent};
use ratatui::init::DefaultTerminal;

use crate::app::{App, Msg, RunState, update};
use crate::cli::Cli;

/// Run the TUI to completion using the given CLI configuration.
///
/// Sets up the terminal (alternate screen unless `--no-alt-screen`), drives the
/// draw loop, polls events on a tick, and restores the terminal on exit.
///
/// If the `--no-alt-screen` flag is set, the alternate screen buffer is skipped
/// so the app draws inline. This is useful for debugging and terminal-capture tests.
pub fn run(cli: &Cli) -> io::Result<()> {
    let tick = Duration::from_millis(cli.tick_rate_ms);
    if cli.no_alt_screen { run_inline(tick, cli) } else { run_alt_screen(tick, cli) }
}

/// [`ratatui::init`] enables raw mode, enters the alternate screen, installs a
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
///    Between event polls, drain any pending agent stream events.
/// 3. Tick.
fn main_loop(terminal: &mut DefaultTerminal, tick: Duration, cli: &Cli) -> io::Result<()> {
    let mut app = App::from_cli(cli);
    let mut agent_rx: Option<Receiver<crate::app::AgentEvent>> = None;
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        let deadline = Instant::now() + tick;
        while Instant::now() < deadline {
            drain_agent_events(&mut app, &mut agent_rx, terminal)?;
            manage_agent_lifecycle(&app, &mut agent_rx);

            if app.quit {
                return Ok(());
            }

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

            maybe_spawn_agent(&app, &mut agent_rx);

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

/// Spawn the fake agent stream if the app is in [`RunState::Working`] state and
/// no stream receiver exists yet.
fn maybe_spawn_agent(app: &App, agent_rx: &mut Option<Receiver<crate::app::AgentEvent>>) {
    if app.run_state == RunState::Working && agent_rx.is_none() {
        *agent_rx = Some(crate::agent::spawn_fake_stream());
    }
}

/// Drain all pending agent stream events from the channel and dispatch them as
/// [`Msg::Agent`].
fn drain_agent_events(
    app: &mut App, agent_rx: &mut Option<Receiver<crate::app::AgentEvent>>, term: &mut DefaultTerminal,
) -> io::Result<()> {
    let Some(rx) = agent_rx else { return Ok(()) };

    loop {
        match rx.try_recv() {
            Ok(event) => {
                handle_msg(app, Msg::Agent(event), term)?;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                *agent_rx = None;
                break;
            }
        }
    }
    Ok(())
}

/// If the app is no longer in `Working` state but a receiver still exists,
/// drop it (user cancelled via Escape).
fn manage_agent_lifecycle(app: &App, agent_rx: &mut Option<Receiver<crate::app::AgentEvent>>) {
    if app.run_state != RunState::Working && agent_rx.is_some() {
        *agent_rx = None;
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
