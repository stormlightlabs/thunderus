//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The bin in [`main.rs`] just calls [`run`].

pub mod cli;
pub mod input;
pub mod session;

mod agent;
mod app;
mod config;
mod context;
mod fuzzy;
mod internals;
mod prompt;
mod providers;
mod renderer;
mod search;
mod skills;
mod tools;
mod utils;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent};

use app::{App, Msg, RunState, update};
use cli::Cli;
use prompt::PromptBundle;
use renderer::backend::TerminalBackend;
use tools::AgentRunConfig;

use crate::app::PromptAccessory;
use crate::utils::datetime;

/// State carried by the main loop for a single agent run.
struct AgentSlot {
    receiver: mpsc::Receiver<app::AgentEvent>,
    cancel: agent::CancelToken,
    steering: mpsc::Sender<String>,
}

struct GitStatusWatcher {
    receiver: mpsc::Receiver<Option<renderer::git::GitStatusSummary>>,
    stop: mpsc::Sender<()>,
}

impl GitStatusWatcher {
    fn spawn(cwd: PathBuf) -> Self {
        Self::spawn_with_interval(cwd, Duration::from_millis(1000))
    }

    fn spawn_with_interval(cwd: PathBuf, interval: Duration) -> Self {
        let (status_tx, status_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut last = renderer::git::collect(&cwd);
            loop {
                match stop_rx.recv_timeout(interval) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                let next = renderer::git::collect(&cwd);
                if next != last {
                    last = next.clone();
                    if status_tx.send(next).is_err() {
                        break;
                    }
                }
            }
        });
        Self { receiver: status_rx, stop: stop_tx }
    }
}

impl Drop for GitStatusWatcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

/// Run the TUI to completion using the given CLI configuration.
///
/// Sets up the terminal, drives the draw loop, polls events on a tick, and
/// restores the terminal on exit.
pub fn run(cli: &Cli) -> io::Result<()> {
    if cli.print_prompt {
        return run_print_prompt(cli);
    }
    let tick = Duration::from_millis(cli.tick_rate_ms);
    run_inline(tick, cli)
}

#[derive(Clone, Debug)]
struct Observability {
    session_log_path: PathBuf,
    daily_log_path: PathBuf,
}

fn init_tracing(workspace_root: &Path, session_id: &str) -> Option<Observability> {
    let session_log_dir = workspace_root.join(".thndrs").join("logs").join("sessions");
    let daily_log_dir = workspace_root.join(".thndrs").join("logs").join("daily");
    let session_log_path = session_log_dir.join(format!("thndrs-{session_id}.log"));
    let daily_log_path = daily_log_dir.join(format!("{}.log", datetime::rounded_date()));
    std::fs::create_dir_all(&session_log_dir).ok()?;
    std::fs::create_dir_all(&daily_log_dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session_log_path)
        .ok()?;

    if tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .try_init()
        .is_ok()
    {
        Some(Observability { session_log_path, daily_log_path })
    } else {
        None
    }
}

fn daily_detail_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\n' && *c != '\r').take(300).collect()
}

fn append_daily_log(observability: &Option<Observability>, session_id: &str, event: &str, details: &str) {
    let Some(obs) = observability else {
        return;
    };

    if let Some(parent) = obs.daily_log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    use std::io::Write;
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&obs.daily_log_path)
    else {
        return;
    };
    let _ = writeln!(
        file,
        "{} session={} event={} {}",
        datetime::now_iso8601(),
        session_id,
        event,
        details
    );
}

/// Print the assembled prompt bundle with secrets redacted, without calling
/// the provider. This is the `--print-prompt` debug path.
fn run_print_prompt(cli: &Cli) -> io::Result<()> {
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let context_sources = match context::load_agents_md(&workspace_root) {
        Some(source) => vec![source],
        None => Vec::new(),
    };
    let skill_inventory = skills::discover(&workspace_root, &cli.skill_dirs);

    let user_turn = String::from("(no user prompt — print-prompt debug mode)");
    let bundle = PromptBundle::new_with_skills(
        &workspace_root,
        &cli.model,
        cli.websearch,
        &context_sources,
        &skill_inventory.skills,
        &[],
        &user_turn,
    );

    let output = render_print_prompt(&bundle);
    print!("{output}");
    Ok(())
}

/// Render the `--print-prompt` debug view as a string.
///
/// Produces a human-readable dump of the assembled prompt bundle: system prompt,
/// tool catalog, lowered Umans messages, and environment metadata. Secrets
/// (`sk-` prefixed values) are redacted. The date is replaced with `[date]` so
/// the output is stable for snapshot testing.
pub fn render_print_prompt(bundle: &PromptBundle) -> String {
    let system_prompt = prompt::render_system_prompt(bundle);
    let messages = prompt::lower_to_umans_messages(bundle);
    let tool_catalog = prompt::render_tool_catalog(bundle);
    let mut out = String::new();

    out.push_str(&format!("=== System Prompt ===\n{}\n\n", &system_prompt));
    out.push_str(&format!("=== Tool Catalog ({} tools) ===\n", bundle.tool_catalog.len()));
    out.push_str(&serde_json::to_string_pretty(&tool_catalog).unwrap_or_default());
    out.push_str(&format!(
        "\n\n=== Lowered Provider Messages ({} messages) ===\n",
        messages.len()
    ));

    for (i, msg) in messages.iter().enumerate() {
        let redacted = redact_secret(&msg.as_text());
        let truncated = if redacted.len() > 200 { format!("{}...", &redacted[..200]) } else { redacted };
        out.push_str(&format!("[{i}] {}: {truncated}\n", msg.role));
    }

    out.push_str("\n=== Environment ===\n");
    out.push_str(&format!("  cwd: {}\n", bundle.environment.cwd));
    out.push_str(&format!("  model: {}\n", bundle.environment.model));
    out.push_str(&format!("  search: {}\n", bundle.environment.search_mode.label()));
    out.push_str("  date: [date]\n");
    out.push_str(&format!("  context_sources: {}\n", bundle.project_context.len()));
    out.push_str(&format!("  skills: {}\n", bundle.available_skills.len()));

    out
}

/// Redact secret-like values from prompt content for debug display.
fn redact_secret(text: &str) -> String {
    text.replace("sk-", "sk-[REDACTED]")
}

/// Inline mode using the direct renderer: a logical viewport owns the visible
/// terminal area and is rebuilt for the current terminal size each tick.
fn run_inline(tick: Duration, cli: &Cli) -> io::Result<()> {
    renderer::enter_raw_mode()?;
    let mouse_enabled = cli.mouse && !cli.no_mouse;
    let stdout = io::stdout();
    let mut backend = TerminalBackend::new(stdout, renderer::terminal_size().0, renderer::terminal_size().1);
    let mut live = renderer::region::LiveRegion::new();
    let result = direct_loop(&mut backend, &mut live, tick, cli, mouse_enabled);

    let _ = backend.show_cursor();
    renderer::leave_raw_mode()?;

    let _ = crossterm::execute!(
        io::stdout(),
        crossterm::cursor::MoveTo(0, renderer::terminal_size().1.saturating_sub(1))
    );
    println!();
    result
}

/// Direct renderer event loop: drives the agent, polls events, and renders via
/// [`TerminalBackend`] + [`LiveRegion`].
///
/// History and live prompt chrome are rendered as one terminal-sized frame so
/// resize and wrapping behavior is owned by the renderer instead of native
/// terminal scrollback side effects.
fn direct_loop<W: io::Write>(
    backend: &mut TerminalBackend<W>, live: &mut renderer::region::LiveRegion, tick: Duration, cli: &Cli,
    mouse_enabled: bool,
) -> io::Result<()> {
    let mut app = App::from_cli(cli);
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let observability = init_tracing(&workspace_root, &app.session_id);
    if cli.verbose
        && let Some(obs) = &observability
    {
        app.transcript
            .push(app::Entry::Status { text: format!("logs  {}", obs.session_log_path.display()) });
    }
    tracing::info!(
        session = %app.session_id,
        cwd = %workspace_root.display(),
        model = %cli.model,
        websearch = %cli.websearch.label(),
        "starting thndrs (direct renderer)"
    );
    append_daily_log(
        &observability,
        &app.session_id,
        "session_start",
        &format!(
            "cwd={} model={} websearch={}",
            workspace_root.display(),
            cli.model,
            cli.websearch.label()
        ),
    );

    let mut agent: Option<AgentSlot> = None;
    let git_watcher = GitStatusWatcher::spawn(workspace_root);
    let mut mouse_captured = false;
    let (width, height) = backend_size(backend);
    direct_render(backend, live, &mut app, width, height)?;

    loop {
        let deadline = Instant::now() + tick;
        while Instant::now() < deadline {
            drain_direct_agent_events(&mut app, &mut agent, backend, live, &observability)?;
            drain_git_status_watcher(&mut app, &git_watcher, backend, live)?;
            manage_agent_lifecycle(&app, &mut agent);
            maybe_spawn_agent(&app, cli, &mut agent);
            flush_steering(&mut app, &agent);
            sync_mouse_capture(&app, &mut mouse_captured, mouse_enabled);
            let (w, h) = backend_size(backend);
            direct_render(backend, live, &mut app, w, h)?;

            if app.quit {
                tracing::info!("quitting thndrs");
                append_daily_log(&observability, &app.session_id, "session_end", "reason=quit");
                return Ok(());
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if !event::poll(remaining)? {
                break;
            }
            match event::read()? {
                Event::Key(key) => {
                    handle_direct_key(&mut app, key, &mut agent, backend, live)?;
                    sync_mouse_capture(&app, &mut mouse_captured, mouse_enabled);
                    let (w, h) = backend_size(backend);
                    direct_render(backend, live, &mut app, w, h)?;
                }
                Event::Mouse(mouse) => {
                    handle_direct_msg(&mut app, Msg::Mouse(mouse), backend, live)?;
                    let (w, h) = backend_size(backend);
                    direct_render(backend, live, &mut app, w, h)?;
                }
                Event::Resize(_, _) => {
                    let (w, h) = renderer::terminal_size();
                    backend.set_size(w, h);
                    let (w, h) = backend_size(backend);
                    direct_render(backend, live, &mut app, w, h)?;
                }
                _ => {}
            }

            maybe_spawn_agent(&app, cli, &mut agent);
            flush_steering(&mut app, &agent);
            sync_mouse_capture(&app, &mut mouse_captured, mouse_enabled);

            if app.quit {
                tracing::info!("quitting thndrs");
                append_daily_log(&observability, &app.session_id, "session_end", "reason=quit");
                return Ok(());
            }
        }
        handle_direct_msg(&mut app, Msg::Tick, backend, live)?;
        drain_git_status_watcher(&mut app, &git_watcher, backend, live)?;
        sync_mouse_capture(&app, &mut mouse_captured, mouse_enabled);
        let (w, h) = backend_size(backend);
        direct_render(backend, live, &mut app, w, h)?;
        if app.quit {
            tracing::info!("quitting thndrs");
            append_daily_log(&observability, &app.session_id, "session_end", "reason=quit");
            return Ok(());
        }
    }
}

/// Toggle terminal mouse capture based on whether the file picker is open.
///
/// When the file picker is active, mouse capture is enabled so scroll-wheel
/// navigation works inside the picker. At all other times mouse capture is
/// disabled so the user can select and copy transcript/input text using
/// native terminal selection.
fn sync_mouse_capture(app: &App, captured: &mut bool, mouse_enabled: bool) {
    if !mouse_enabled {
        return;
    }
    let picker_open = matches!(
        app.prompt_accessory,
        PromptAccessory::Files(_) | PromptAccessory::Skills
    );
    if picker_open && !*captured {
        let _ = crossterm::execute!(io::stdout(), EnableMouseCapture);
        *captured = true;
    } else if !picker_open && *captured {
        let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
        *captured = false;
    }
}

/// Read the current size from the backend as `usize` tuples.
fn backend_size<W: io::Write>(backend: &TerminalBackend<W>) -> (usize, usize) {
    (backend.width() as usize, backend.height() as usize)
}

/// Render the complete logical viewport.
///
/// The live region handles committing finalized transcript entries to
/// terminal scrollback and rendering only the live chrome (prompt, status,
/// streaming content) via diff-based rendering.
fn direct_render<W: io::Write>(
    backend: &mut TerminalBackend<W>, live: &mut renderer::region::LiveRegion, app: &mut App, width: usize,
    height: usize,
) -> io::Result<()> {
    renderer::style::set_theme(app.theme);
    live.render_frame(app, backend, width, height)?;
    backend.flush()
}

/// Process a key in the direct renderer path.
fn handle_direct_key<W: io::Write>(
    app: &mut App, key: KeyEvent, agent: &mut Option<AgentSlot>, backend: &mut TerminalBackend<W>,
    live: &mut renderer::region::LiveRegion,
) -> io::Result<()> {
    if key.code == crossterm::event::KeyCode::Esc
        && app.run_state == RunState::Working
        && let Some(slot) = agent
    {
        slot.cancel.cancel();
    }
    handle_direct_msg(app, Msg::Key(key), backend, live)
}

/// Process a message and chain follow-ups, then render.
fn handle_direct_msg<W: io::Write>(
    app: &mut App, msg: Msg, backend: &mut TerminalBackend<W>, live: &mut renderer::region::LiveRegion,
) -> io::Result<()> {
    let mut next = Some(msg);
    while let Some(m) = next {
        let is_clear = matches!(m, Msg::Clear);
        next = update(app, &m);
        if is_clear {
            live.reset();
            backend.clear_all_and_scrollback()?;
        }
        if app.quit {
            return Ok(());
        }
    }
    Ok(())
}

/// Drain agent events in the direct renderer path.
fn drain_direct_agent_events<W: io::Write>(
    app: &mut App, agent: &mut Option<AgentSlot>, backend: &mut TerminalBackend<W>,
    live: &mut renderer::region::LiveRegion, observability: &Option<Observability>,
) -> io::Result<()> {
    let Some(slot) = agent else {
        return Ok(());
    };

    loop {
        match slot.receiver.try_recv() {
            Ok(event) => {
                match &event {
                    app::AgentEvent::Failed(msg) => {
                        tracing::error!(error = %msg, "agent failed");
                        append_daily_log(
                            observability,
                            &app.session_id,
                            "agent_failed",
                            &format!("error={}", daily_detail_value(msg)),
                        );
                    }
                    app::AgentEvent::Cancelled => {
                        tracing::warn!("agent cancelled");
                        append_daily_log(observability, &app.session_id, "agent_cancelled", "");
                    }
                    app::AgentEvent::Finished => {
                        tracing::info!("agent finished");
                        append_daily_log(observability, &app.session_id, "agent_finished", "");
                    }
                    _ => {}
                }
                handle_direct_msg(app, Msg::Agent(event), backend, live)?;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => {
                *agent = None;
                break;
            }
        }
    }
    Ok(())
}

fn drain_git_status_watcher<W: io::Write>(
    app: &mut App, watcher: &GitStatusWatcher, backend: &mut TerminalBackend<W>,
    live: &mut renderer::region::LiveRegion,
) -> io::Result<()> {
    while let Ok(status) = watcher.receiver.try_recv() {
        handle_direct_msg(app, Msg::GitStatusChanged(status), backend, live)?;
    }
    Ok(())
}

/// Spawn the unified agent stream if the app is in [`RunState::Working`] state
/// and no agent slot exists yet.
///
/// The run chooses a provider from the selected model id. The
/// [`agent::CancelToken`] is retained so `Escape` can signal cooperative
/// cancellation.
fn maybe_spawn_agent(app: &App, cli: &Cli, agent: &mut Option<AgentSlot>) {
    if app.run_state != RunState::Working {
        return;
    }
    if agent.is_some() {
        return;
    }

    let prompt = app
        .transcript
        .iter()
        .rev()
        .find_map(|e| match e {
            app::Entry::User { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let resolved_websearch = cli.websearch.resolve_for_prompt(&prompt);
    let config = AgentRunConfig::new(workspace_root, cli.model.clone(), resolved_websearch);
    tracing::info!(
        cwd = %config.root.display(),
        model = %config.model,
        requested_websearch = %cli.websearch.label(),
        resolved_websearch = %config.search_mode.header_value(),
        "spawning agent run"
    );

    let bundle = PromptBundle::new_with_skills(
        &config.root,
        &config.model,
        config.search_mode,
        &app.context_sources,
        &app.skills,
        &app.transcript,
        &prompt,
    );
    let messages = prompt::lower_to_umans_messages(&bundle);
    let expects_write = agent::prompt_expects_workspace_write(&prompt);
    let (steering_tx, steering_rx) = mpsc::channel();
    let handle = agent::RunHandle::provider_with_steering(config, messages, expects_write, steering_rx);
    let cancel = handle.cancel.clone();
    let receiver = agent::spawn_run(handle);
    *agent = Some(AgentSlot { receiver, cancel, steering: steering_tx });
}

fn flush_steering(app: &mut App, agent: &Option<AgentSlot>) {
    let Some(slot) = agent else {
        return;
    };
    let mut unsent = Vec::new();
    for message in app.queued_steering.drain(..) {
        if slot.steering.send(message.clone()).is_err() {
            unsent.push(message);
        }
    }
    app.queued_steering = unsent;
}

/// If the app is no longer in `Working` state but an agent slot still exists,
/// cancel it and drop the slot (user cancelled via Escape or the run finished).
fn manage_agent_lifecycle(app: &App, agent: &mut Option<AgentSlot>) {
    if app.run_state != RunState::Working
        && let Some(slot) = agent.take()
    {
        tracing::info!("cancelling dropped agent slot");
        slot.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context::ContextSource;
    use prompt::PromptBundle;
    use std::path::PathBuf;
    use std::process::Command;

    /// Build a deterministic bundle for snapshot testing — no workspace
    /// discovery, no live date, fixed context.
    fn snapshot_bundle() -> PromptBundle {
        let source = ContextSource {
            path: PathBuf::from("/repo/AGENTS.md"),
            scope: ".".to_string(),
            content: "# Project\n\nBuild with cargo. Run tests with cargo test.\n".to_string(),
            content_hash: 12345,
            truncated: false,
            byte_count: 50,
        };
        PromptBundle {
            fragments: prompt::default_fragments(),
            environment: prompt::EnvironmentMetadata {
                cwd: "/repo".to_string(),
                model: "umans-coder".to_string(),
                search_mode: cli::WebSearchMode::Native,
                date: "2026-06-29".to_string(),
            },
            project_context: vec![source],
            tool_catalog: tools::tool_definitions(),
            available_skills: Vec::new(),
            transcript_tail: Vec::new(),
            user_turn: "explain this repo".to_string(),
            history_reuse: prompt::HistoryReuse::Unavailable,
            prev_context_hash: None,
        }
    }

    #[test]
    fn render_print_prompt_snapshot() {
        let bundle = snapshot_bundle();
        let output = render_print_prompt(&bundle);
        insta::assert_snapshot!(output);
    }

    #[test]
    fn render_print_prompt_redacts_secrets() {
        let mut bundle = snapshot_bundle();
        bundle.user_turn = "my key is sk-test123".to_string();
        let output = render_print_prompt(&bundle);
        assert!(
            !output.contains("sk-test123"),
            "secrets should be redacted in print-prompt output"
        );
        assert!(output.contains("sk-[REDACTED]"), "redacted marker should appear");
    }

    #[test]
    fn render_print_prompt_includes_all_sections() {
        let bundle = snapshot_bundle();
        let output = render_print_prompt(&bundle);
        assert!(
            output.contains("=== System Prompt ==="),
            "should have system prompt section"
        );
        assert!(output.contains("=== Tool Catalog"), "should have tool catalog section");
        assert!(
            output.contains("=== Lowered Provider Messages"),
            "should have messages section"
        );
        assert!(
            output.contains("=== Environment ==="),
            "should have environment section"
        );
    }

    #[test]
    fn render_print_prompt_date_is_redacted() {
        let bundle = snapshot_bundle();
        let output = render_print_prompt(&bundle);
        let env_section = output.split("=== Environment ===").nth(1).unwrap_or("");
        assert!(
            env_section.contains("date: [date]"),
            "date in env section should be redacted to [date] for snapshot stability"
        );
    }

    #[test]
    fn redact_secret_replaces_sk_prefix() {
        let result = redact_secret("token: sk-abc123 rest");
        assert_eq!(result, "token: sk-[REDACTED]abc123 rest");
    }

    #[test]
    fn clear_resets_direct_renderer_and_purges_terminal() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.transcript.push(app::Entry::User { text: "hello".to_string() });

        let mut live = renderer::region::LiveRegion::new();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);
        direct_render(&mut backend, &mut live, &mut app, 80, 24).expect("initial render");

        handle_direct_msg(&mut app, Msg::Clear, &mut backend, &mut live).expect("clear");
        assert!(app.transcript.is_empty());

        let output = String::from_utf8_lossy(backend.writer());
        assert!(output.contains("\u{1b}[2J"), "visible screen should be cleared");
        assert!(output.contains("\u{1b}[3J"), "terminal scrollback should be purged");
    }

    #[test]
    fn flush_steering_sends_queued_messages_to_active_agent() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Working;
        app.queued_steering.push("use the failing test first".to_string());
        let (event_tx, event_rx) = mpsc::channel();
        drop(event_tx);
        let (steering_tx, steering_rx) = mpsc::channel();
        let slot = AgentSlot { receiver: event_rx, cancel: agent::CancelToken::new(), steering: steering_tx };

        flush_steering(&mut app, &Some(slot));

        assert!(
            app.queued_steering.is_empty(),
            "sent steering should leave the app queue"
        );
        assert_eq!(
            steering_rx.try_recv().expect("active run should receive steering"),
            "use the failing test first"
        );
    }

    #[test]
    fn direct_render_does_not_emit_redundant_hide_cursor() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;

        let mut live = renderer::region::LiveRegion::new();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

        direct_render(&mut backend, &mut live, &mut app, 80, 24).expect("initial render");
        let first_len = String::from_utf8(backend.writer().clone()).unwrap().len();

        direct_render(&mut backend, &mut live, &mut app, 80, 24).expect("second render");
        let second_output = String::from_utf8(backend.writer().clone()).unwrap();
        let new_bytes = &second_output[first_len..];

        assert!(
            !new_bytes.contains("\x1b[?25l"),
            "direct_render should not emit Hide cursor on re-render of identical frame: {new_bytes:?}"
        );
    }

    #[test]
    fn git_status_watcher_reports_external_change() {
        let dir = tempfile::tempdir().expect("temp git dir");
        git(dir.path(), &["init"]);
        git(dir.path(), &["config", "user.email", "test@example.com"]);
        git(dir.path(), &["config", "user.name", "Test User"]);
        std::fs::write(dir.path().join("tracked.txt"), "clean\n").expect("write tracked file");
        git(dir.path(), &["add", "tracked.txt"]);
        git(dir.path(), &["commit", "-m", "initial"]);

        let watcher = GitStatusWatcher::spawn_with_interval(dir.path().to_path_buf(), Duration::from_millis(50));
        thread::sleep(Duration::from_millis(100));
        std::fs::write(dir.path().join("tracked.txt"), "dirty\n").expect("modify tracked file");

        let status = watcher
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("watcher should report dirty git status")
            .expect("repo status should be available");
        assert_eq!(status.modified, 1);
        assert!(
            status.display().ends_with("+0 ~1 -0"),
            "dirty summary should show one modified file: {}",
            status.display()
        );
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap_or_else(|err| panic!("git {args:?} failed to start: {err}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
