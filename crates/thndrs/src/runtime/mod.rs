//! Runtime routing for commands, prompt printing, and the interactive terminal.

use super::*;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event;
use ratatui::backend::CrosstermBackend;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::prompt::PromptBundle;
use acp::config::provider_label;
use app::{
    Action, App, Effect, EffectRequest, EffectResult, Msg, RunState, start_auto_compaction, translate_input,
    update_with_effects,
};
use cli::{
    Cli, Command, MIN_TICK_RATE_MS, commands as cli_commands,
    commands::context::{ContextCommand, ContextSubcommand, UsageCommand},
    commands::debug::DebugCommand,
    commands::mcp::McpCommand,
    commands::session::{SessionCommand, SessionDataFormat, SessionReportFormat},
};
use mcp::manager::McpManager;
use utils::datetime;

use crate::input::TerminalInput;

use thndrs_agent::CancelToken;
use thndrs_agent::context as agent_context;

/// Smallest interval at which the TUI applies periodic agent-driven updates.
const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(MIN_TICK_RATE_MS);
/// Largest event burst applied before yielding back to the render/event loop.
const MAX_AGENT_EVENTS_PER_RENDER: usize = 256;
/// Buffer one complete terminal transaction so CRLF scrollback commits cannot
/// be line-buffered and shown before their replacement frame.
const TERMINAL_WRITE_BUFFER_CAPACITY: usize = 64 * 1024;

/// Session state to load before entering the interactive terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitialSession<'a> {
    New,
    Resume(&'a str),
}

enum AcpEventWrite {
    Continue,
    Finished,
    Cancelled,
    Failed(String),
}

/// State carried by the main loop for a single agent run.
pub(crate) struct AgentSlot {
    pub(crate) request: EffectRequest,
    pub(crate) receiver: thndrs_agent::AgentRun<app::AgentEvent>,
    pub(crate) cancel: CancelToken,
    pub(crate) steering: mpsc::Sender<String>,
}

pub(crate) struct GitStatusWatcher {
    receiver: mpsc::Receiver<Option<cli::git::GitStatusSummary>>,
    _initialized: mpsc::Receiver<()>,
    stop: mpsc::Sender<()>,
}

impl GitStatusWatcher {
    fn spawn(cwd: PathBuf) -> Self {
        Self::spawn_with_interval(cwd, Duration::from_millis(1000))
    }

    fn spawn_with_interval(cwd: PathBuf, interval: Duration) -> Self {
        let (status_tx, status_rx) = mpsc::channel();
        let (initialized_tx, initialized_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut last = cli::git::collect(&cwd);
            let _ = initialized_tx.send(());
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                let next = cli::git::collect(&cwd);
                if next != last {
                    last = next.clone();
                    if status_tx.send(next).is_err() {
                        break;
                    }
                }
            }
        });
        Self { receiver: status_rx, _initialized: initialized_rx, stop: stop_tx }
    }

    #[cfg(test)]
    fn wait_until_initialized(&self) {
        self._initialized
            .recv()
            .expect("git status watcher should report initialization");
    }
}

impl Drop for GitStatusWatcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

/// Coalesces background updates while allowing interaction-driven frames through immediately.
#[derive(Debug)]
pub(crate) struct PresentationScheduler {
    frame_interval: Duration,
    dirty: bool,
    full_repaint: bool,
    present_immediately: bool,
    last_presented: Option<Instant>,
    deadline: Option<Instant>,
}

impl PresentationScheduler {
    fn new(frame_interval: Duration) -> Self {
        Self {
            frame_interval,
            dirty: false,
            full_repaint: false,
            present_immediately: false,
            last_presented: None,
            deadline: None,
        }
    }

    fn request_immediate(&mut self) {
        self.dirty = true;
        self.present_immediately = true;
        self.deadline = None;
    }

    fn request_throttled(&mut self, now: Instant) {
        self.dirty = true;
        if self.present_immediately {
            return;
        }

        let deadline = self
            .last_presented
            .and_then(|last_presented| last_presented.checked_add(self.frame_interval))
            .unwrap_or(now)
            .max(now);
        self.deadline = Some(self.deadline.map_or(deadline, |scheduled| scheduled.min(deadline)));
    }

    fn request_full_repaint(&mut self) {
        self.full_repaint = true;
        self.request_immediate();
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn should_present(&self, now: Instant) -> bool {
        self.dirty && (self.present_immediately || self.deadline.is_some_and(|deadline| now >= deadline))
    }

    fn full_repaint_required(&self) -> bool {
        self.full_repaint
    }

    fn mark_presented(&mut self, now: Instant) {
        self.dirty = false;
        self.full_repaint = false;
        self.present_immediately = false;
        self.last_presented = Some(now);
        self.deadline = None;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Observability {
    session_log_path: PathBuf,
    daily_log_path: PathBuf,
}

#[path = "commands.rs"]
mod commands;
#[path = "interactive.rs"]
mod interactive;
#[path = "prompt.rs"]
mod prompt;
#[path = "terminal.rs"]
mod terminal;
#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
pub(crate) use commands::{
    SessionPruneRequest, SessionPurgeRequest, render_acp_inspect, render_acp_list, run_acp_close_session,
    run_acp_install, run_acp_list_sessions, run_acp_load_session, run_acp_logout, run_acp_registry,
    run_acp_resume_session, run_acp_smoke, run_acp_update, run_debug_session_log, run_mcp_add, run_mcp_call,
    run_mcp_catalog_configure, run_mcp_catalog_list, run_mcp_catalog_search, run_mcp_catalog_show, run_mcp_list,
    run_mcp_remove, run_mcp_resource, run_mcp_resources, run_mcp_revoke, run_mcp_tools, run_mcp_trust,
    run_session_export, run_session_inspect, run_session_latest, run_session_list, run_session_prune,
    run_session_purge, run_session_rename, run_session_show, run_session_storage, run_session_titles,
};
pub(crate) use commands::{load_effective_mcp_for_workspace, load_mcp_manager_for_workspace, run_command};
pub(crate) use interactive::*;
pub(crate) use prompt::{append_daily_log, daily_detail_value, init_tracing, session_resume_message};
#[cfg(test)]
pub(crate) use prompt::{redact_secret, render_print_prompt_config};
pub(crate) use terminal::{InlineTerminalSession, InteractiveSurface, RatatuiSurface};

/// Process exit code for a [`run`] error.
pub fn exit_code(error: &io::Error) -> i32 {
    headless::exit_code(error)
}

/// Run the TUI or one of the non-interactive commands.
pub fn run(cli: &Cli) -> io::Result<()> {
    if let Some(Command::Session { command: SessionCommand::Resume { session_id } }) = &cli.command {
        let tick = Duration::from_millis(cli.tick_rate_ms);
        return interactive::run_inline(tick, cli, InitialSession::Resume(session_id));
    }
    if let Some(command) = &cli.command {
        return run_command(cli, command);
    }
    if cli.print_prompt {
        return prompt::run_print_prompt(cli);
    }
    let tick = Duration::from_millis(cli.tick_rate_ms);
    interactive::run_inline(tick, cli, InitialSession::New)
}

/// Render the `--print-prompt` debug view as a string.
pub fn render_print_prompt(bundle: &PromptBundle) -> String {
    prompt::render_print_prompt(bundle)
}
