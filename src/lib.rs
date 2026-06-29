//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The bin in [`main.rs`] just calls [`run`].

pub mod cli;

mod agent;
mod app;
mod banner;
mod context;
mod tools;
mod ui;

#[allow(dead_code)]
// TODO: Wire into app loop
mod providers;

use std::io;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent};
use ratatui::init::DefaultTerminal;

use crate::app::{App, Msg, RunState, update};
use crate::cli::Cli;
use crate::tools::AgentRunConfig;

/// State carried by the main loop for a single agent run.
struct AgentSlot {
    receiver: Receiver<crate::app::AgentEvent>,
    cancel: crate::agent::CancelToken,
}

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
    let mut agent: Option<AgentSlot> = None;
    terminal.draw(|f| ui::render(f, &app))?;

    loop {
        let deadline = Instant::now() + tick;
        while Instant::now() < deadline {
            drain_agent_events(&mut app, &mut agent, terminal)?;
            manage_agent_lifecycle(&app, &mut agent);

            if app.quit {
                return Ok(());
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if !event::poll(remaining)? {
                break;
            }
            match event::read()? {
                Event::Key(key) => handle_key(&mut app, key, terminal, &mut agent)?,
                Event::Resize(_, _) => {
                    terminal.draw(|f| ui::render(f, &app))?;
                }
                _ => {}
            }

            maybe_spawn_agent(&app, cli, &mut agent);

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

/// Spawn the unified agent stream if the app is in [`RunState::Working`] state
/// and no agent slot exists yet.
///
/// The run uses the fake provider for now; the Umans provider is wired but
/// gated on `UMANS_API_KEY` and will be selected once the provider trait is
/// connected. The [`crate::agent::CancelToken`] is retained so `Escape` can
/// signal cooperative cancellation.
fn maybe_spawn_agent(app: &App, cli: &Cli, agent: &mut Option<AgentSlot>) {
    if app.run_state != RunState::Working {
        return;
    }
    if agent.is_some() {
        return;
    }

    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let config = AgentRunConfig::new(workspace_root, cli.model.clone(), cli.websearch);

    // Derive the prompt from the last user entry in the transcript.
    let prompt = app
        .transcript
        .iter()
        .rev()
        .find_map(|e| match e {
            crate::app::Entry::User { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let handle = crate::agent::RunHandle::fake(config, prompt);
    let cancel = handle.cancel.clone();
    let receiver = crate::agent::spawn_run(handle);
    *agent = Some(AgentSlot { receiver, cancel });
}

/// Drain all pending agent stream events from the channel and dispatch them as
/// [`Msg::Agent`].
fn drain_agent_events(app: &mut App, agent: &mut Option<AgentSlot>, term: &mut DefaultTerminal) -> io::Result<()> {
    let Some(slot) = agent else { return Ok(()) };

    loop {
        match slot.receiver.try_recv() {
            Ok(event) => {
                handle_msg(app, Msg::Agent(event), term)?;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                *agent = None;
                break;
            }
        }
    }
    Ok(())
}

/// If the app is no longer in `Working` state but an agent slot still exists,
/// cancel it and drop the slot (user cancelled via Escape or the run finished).
fn manage_agent_lifecycle(app: &App, agent: &mut Option<AgentSlot>) {
    if app.run_state != RunState::Working
        && let Some(slot) = agent.take()
    {
        slot.cancel.cancel();
    }
}

fn handle_key(
    app: &mut App, key: KeyEvent, term: &mut DefaultTerminal, agent: &mut Option<AgentSlot>,
) -> io::Result<()> {
    if key.code == crossterm::event::KeyCode::Esc
        && app.run_state == RunState::Working
        && let Some(slot) = agent
    {
        slot.cancel.cancel();
    }
    handle_msg(app, Msg::Key(key), term)
}

/// Process the message and any chained follow-ups.
fn handle_msg(app: &mut App, msg: Msg, terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut next = Some(msg);
    while let Some(m) = next {
        next = update(app, &m);
        if app.quit {
            return Ok(());
        }
    }
    terminal.draw(|f| ui::render(f, app))?;
    Ok(())
}
