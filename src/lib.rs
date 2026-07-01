//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The bin in [`main.rs`] just calls [`run`].

pub mod cli;
pub mod session;

mod agent;
mod app;
mod banner;
mod config;
mod context;
mod datetime;
mod fuzzy;
pub mod input;
mod prompt;
mod providers;
mod search;
mod tools;
mod ui;
mod utils;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::cursor::MoveTo;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent};
use crossterm::style::{
    Attribute as CtAttribute, Attributes as CtAttributes, Color as CtColor, ContentStyle, PrintStyledContent,
    ResetColor,
};
use crossterm::terminal::{Clear, ClearType, DisableLineWrap, EnableLineWrap};
use crossterm::{QueueableCommand, queue};
use ratatui::init::DefaultTerminal;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::{TerminalOptions, Viewport};

use app::{App, Entry, Msg, RunState, ToolStatus, update};
use cli::Cli;
use prompt::PromptBundle;
use tools::AgentRunConfig;

/// State carried by the main loop for a single agent run.
struct AgentSlot {
    receiver: mpsc::Receiver<app::AgentEvent>,
    cancel: agent::CancelToken,
    steering: mpsc::Sender<String>,
}

#[derive(Default)]
struct ScrollbackState {
    emitted_banner: bool,
    emitted_entries: usize,
    rendered_width: Option<u16>,
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
    if cli.no_alt_screen { run_inline(tick, cli) } else { run_alt_screen(tick, cli) }
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

    let user_turn = String::from("(no user prompt — print-prompt debug mode)");
    let bundle = PromptBundle::new(
        &workspace_root,
        &cli.model,
        cli.websearch,
        &context_sources,
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

    out.push_str("=== System Prompt ===\n");
    out.push_str(&system_prompt);
    out.push_str("\n\n");
    out.push_str(&format!("=== Tool Catalog ({} tools) ===\n", bundle.tool_catalog.len()));
    out.push_str(&serde_json::to_string_pretty(&tool_catalog).unwrap_or_default());
    out.push_str("\n\n");
    out.push_str(&format!(
        "=== Lowered Umans Messages ({} messages) ===\n",
        messages.len()
    ));
    for (i, msg) in messages.iter().enumerate() {
        let redacted = redact_secret(&msg.as_text());
        let truncated = if redacted.len() > 200 { format!("{}...", &redacted[..200]) } else { redacted };
        out.push_str(&format!("[{i}] {}: {truncated}\n", msg.role));
    }
    out.push('\n');
    out.push_str("=== Environment ===\n");
    out.push_str(&format!("  cwd: {}\n", bundle.environment.cwd));
    out.push_str(&format!("  model: {}\n", bundle.environment.model));
    out.push_str(&format!("  search: {}\n", bundle.environment.search_mode));
    out.push_str("  date: [date]\n");
    out.push_str(&format!("  context_sources: {}\n", bundle.project_context.len()));

    out
}

/// Redact secret-like values from prompt content for debug display.
fn redact_secret(text: &str) -> String {
    text.replace("sk-", "sk-[REDACTED]")
}

/// [`ratatui::init`] enables raw mode, enters the alternate screen, installs a
/// panic hook that restores the terminal, and returns a [`DefaultTerminal`].
///
/// We always restore the terminal, even on error.
fn run_alt_screen(tick: Duration, cli: &Cli) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let mouse_enabled = cli.mouse && !cli.no_mouse;
    if mouse_enabled && let Err(err) = crossterm::execute!(io::stdout(), EnableMouseCapture) {
        ratatui::restore();
        return Err(err);
    }
    let result = main_loop(&mut terminal, tick, cli, None);
    if mouse_enabled {
        let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

/// Inline keeps the prompt/status UI in a small Ratatui viewport and inserts
/// completed transcript entries above it, preserving native terminal scrollback.
fn run_inline(tick: Duration, cli: &Cli) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::with_options(
        backend,
        TerminalOptions { viewport: Viewport::Inline(ui::INLINE_VIEWPORT_HEIGHT) },
    )?;
    let mouse_enabled = cli.mouse && !cli.no_mouse;
    if mouse_enabled && let Err(err) = crossterm::execute!(io::stdout(), EnableMouseCapture) {
        crossterm::terminal::disable_raw_mode()?;
        return Err(err);
    }
    let mut scrollback = ScrollbackState::default();
    let result = main_loop(&mut terminal, tick, cli, Some(&mut scrollback));
    if mouse_enabled {
        let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    }
    crossterm::terminal::disable_raw_mode()?;
    result
}

/// 1. Initial draw so the shell is visible before the first event.
/// 2. Poll for events until the tick deadline, draining all pending events.
///    Between event polls, drain any pending agent stream events.
/// 3. Tick.
fn main_loop(
    terminal: &mut DefaultTerminal, tick: Duration, cli: &Cli, mut scrollback: Option<&mut ScrollbackState>,
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
        "starting thndrs"
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
    redraw(terminal, &app, scrollback.as_deref_mut())?;

    loop {
        let deadline = Instant::now() + tick;
        while Instant::now() < deadline {
            drain_agent_events(
                &mut app,
                &mut agent,
                terminal,
                &observability,
                scrollback.as_deref_mut(),
            )?;
            manage_agent_lifecycle(&app, &mut agent);
            maybe_spawn_agent(&app, cli, &mut agent);
            flush_steering(&mut app, &agent);

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
                Event::Key(key) => handle_key(&mut app, key, terminal, &mut agent, scrollback.as_deref_mut())?,
                Event::Mouse(mouse) => handle_msg(&mut app, Msg::Mouse(mouse), terminal, scrollback.as_deref_mut())?,
                Event::Resize(_, _) => {
                    redraw(terminal, &app, scrollback.as_deref_mut())?;
                }
                _ => {}
            }

            maybe_spawn_agent(&app, cli, &mut agent);
            flush_steering(&mut app, &agent);

            if app.quit {
                tracing::info!("quitting thndrs");
                append_daily_log(&observability, &app.session_id, "session_end", "reason=quit");
                return Ok(());
            }
        }
        handle_msg(&mut app, Msg::Tick, terminal, scrollback.as_deref_mut())?;
        if app.quit {
            tracing::info!("quitting thndrs");
            append_daily_log(&observability, &app.session_id, "session_end", "reason=quit");
            return Ok(());
        }
    }
}

/// Spawn the unified agent stream if the app is in [`RunState::Working`] state
/// and no agent slot exists yet.
///
/// The run uses the Umans provider. The [`agent::CancelToken`] is retained so
/// `Escape` can signal cooperative cancellation.
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

    let bundle = PromptBundle::new(
        &config.root,
        &config.model,
        config.search_mode,
        &app.context_sources,
        &app.transcript,
        &prompt,
    );
    let messages = prompt::lower_to_umans_messages(&bundle);
    let expects_write = agent::prompt_expects_workspace_write(&prompt);
    let (steering_tx, steering_rx) = mpsc::channel();
    let handle = agent::RunHandle::umans_with_steering(config, messages, expects_write, steering_rx);
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

/// Drain all pending agent stream events from the channel and dispatch them as
/// [`Msg::Agent`].
fn drain_agent_events(
    app: &mut App, agent: &mut Option<AgentSlot>, term: &mut DefaultTerminal, observability: &Option<Observability>,
    scrollback: Option<&mut ScrollbackState>,
) -> io::Result<()> {
    let Some(slot) = agent else { return Ok(()) };
    let mut scrollback = scrollback;

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
                handle_msg(app, Msg::Agent(event), term, scrollback.as_deref_mut())?;
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

fn handle_key(
    app: &mut App, key: KeyEvent, term: &mut DefaultTerminal, agent: &mut Option<AgentSlot>,
    scrollback: Option<&mut ScrollbackState>,
) -> io::Result<()> {
    if key.code == crossterm::event::KeyCode::Esc
        && app.run_state == RunState::Working
        && let Some(slot) = agent
    {
        slot.cancel.cancel();
    }
    handle_msg(app, Msg::Key(key), term, scrollback)
}

/// Process the message and any chained follow-ups.
fn handle_msg(
    app: &mut App, msg: Msg, terminal: &mut DefaultTerminal, scrollback: Option<&mut ScrollbackState>,
) -> io::Result<()> {
    let mut next = Some(msg);
    while let Some(m) = next {
        next = update(app, &m);
        if app.quit {
            return Ok(());
        }
    }
    redraw(terminal, app, scrollback)?;
    Ok(())
}

fn redraw(terminal: &mut DefaultTerminal, app: &App, scrollback: Option<&mut ScrollbackState>) -> io::Result<()> {
    if let Some(scrollback) = scrollback {
        let mut viewport = terminal.draw(|f| ui::render_inline(f, app))?.area;
        viewport = pin_inline_viewport_to_bottom(terminal, viewport, app)?;
        append_stable_transcript(terminal, app, scrollback, viewport)?;
        terminal.draw(|f| ui::render_inline(f, app))?;
    } else {
        terminal.draw(|f| ui::render(f, app))?;
    }
    Ok(())
}

fn pin_inline_viewport_to_bottom(
    terminal: &mut DefaultTerminal, viewport: ratatui::layout::Rect, app: &App,
) -> io::Result<ratatui::layout::Rect> {
    let (_, term_height) = crossterm::terminal::size().unwrap_or((viewport.width, viewport.height));
    let gap = term_height.saturating_sub(viewport.bottom());
    if gap == 0 {
        return Ok(viewport);
    }

    terminal.insert_before(gap, |_| {})?;
    Ok(terminal.draw(|f| ui::render_inline(f, app))?.area)
}

fn append_stable_transcript(
    terminal: &mut DefaultTerminal, app: &App, scrollback: &mut ScrollbackState, viewport: ratatui::layout::Rect,
) -> io::Result<()> {
    let width = viewport.width;
    let viewport_top = viewport.y;

    if scrollback.rendered_width.is_some_and(|last| last != width) {
        clear_inline_scrollback(terminal)?;
        scrollback.emitted_banner = false;
        scrollback.emitted_entries = 0;
    }
    scrollback.rendered_width = Some(width);

    if !scrollback.emitted_banner {
        let lines = ui::startup_banner_lines(app, width as usize);
        insert_history_lines(terminal, &lines, width, viewport_top)?;
        scrollback.emitted_banner = true;
    }

    if scrollback.emitted_entries > app.transcript.len() {
        scrollback.emitted_entries = app.transcript.len();
    }

    let start = scrollback.emitted_entries;
    let stable_end = app.transcript[start..]
        .iter()
        .take_while(|entry| transcript_entry_is_stable(entry))
        .count()
        + start;
    if stable_end == start {
        return Ok(());
    }

    let lines = ui::transcript_lines(&app.transcript[start..stable_end], &app.user_label, width as usize);
    if lines.is_empty() {
        scrollback.emitted_entries = stable_end;
        return Ok(());
    }

    insert_history_lines(terminal, &lines, width, viewport_top)?;
    scrollback.emitted_entries = stable_end;
    Ok(())
}

fn clear_inline_scrollback(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let backend = terminal.backend_mut();
    queue!(backend, Clear(ClearType::All), MoveTo(0, 0), Clear(ClearType::Purge))?;
    backend.flush()
}

fn insert_history_lines(
    terminal: &mut DefaultTerminal, lines: &[Line<'static>], width: u16, viewport_top: u16,
) -> io::Result<()> {
    if lines.is_empty() || width == 0 || viewport_top == 0 {
        return Ok(());
    }

    let backend = terminal.backend_mut();
    backend.queue(DisableLineWrap)?;
    for chunk in lines.chunks(viewport_top as usize) {
        let rows = chunk.len() as u16;
        write_scroll_region_up(backend, 0, viewport_top, rows)?;
        let start_row = viewport_top.saturating_sub(rows);
        for (idx, line) in chunk.iter().enumerate() {
            backend.queue(MoveTo(0, start_row + idx as u16))?;
            write_history_line(backend, line, width as usize)?;
        }
    }
    queue!(backend, EnableLineWrap)?;
    backend.flush()
}

fn write_scroll_region_up(
    writer: &mut impl Write, first_row: u16, end_row_exclusive: u16, amount: u16,
) -> io::Result<()> {
    if amount == 0 || first_row >= end_row_exclusive {
        return Ok(());
    }
    let first = first_row + 1;
    let last = end_row_exclusive;
    write!(writer, "\x1b[{first};{last}r\x1b[{amount}S\x1b[r")
}

fn write_history_line(writer: &mut impl Write, line: &Line<'static>, width: usize) -> io::Result<()> {
    let mut used = 0usize;
    let line_style = line.style;
    for span in &line.spans {
        if used >= width {
            break;
        }
        let style = line_style.patch(span.style);
        let content = span_content_to_width(span, width - used);
        if !content.is_empty() {
            used += content.chars().count();
            writer.queue(PrintStyledContent(style_to_crossterm(style).apply(content)))?;
        }
    }

    if used < width {
        let fill_style = style_to_crossterm(line_style);
        writer.queue(PrintStyledContent(fill_style.apply(" ".repeat(width - used))))?;
    }
    writer.queue(ResetColor)?;
    Ok(())
}

fn span_content_to_width(span: &Span<'static>, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    span.content.chars().take(width).collect()
}

fn style_to_crossterm(style: Style) -> ContentStyle {
    let mut out = ContentStyle::new();
    out.foreground_color = style.fg.and_then(color_to_crossterm);
    out.background_color = style.bg.and_then(color_to_crossterm);
    out.attributes = modifiers_to_crossterm(style.add_modifier);
    out
}

fn color_to_crossterm(color: Color) -> Option<CtColor> {
    Some(match color {
        Color::Reset => CtColor::Reset,
        Color::Black => CtColor::Black,
        Color::Red => CtColor::DarkRed,
        Color::Green => CtColor::DarkGreen,
        Color::Yellow => CtColor::DarkYellow,
        Color::Blue => CtColor::DarkBlue,
        Color::Magenta => CtColor::DarkMagenta,
        Color::Cyan => CtColor::DarkCyan,
        Color::Gray => CtColor::Grey,
        Color::DarkGray => CtColor::DarkGrey,
        Color::LightRed => CtColor::Red,
        Color::LightGreen => CtColor::Green,
        Color::LightYellow => CtColor::Yellow,
        Color::LightBlue => CtColor::Blue,
        Color::LightMagenta => CtColor::Magenta,
        Color::LightCyan => CtColor::Cyan,
        Color::White => CtColor::White,
        Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
        Color::Indexed(i) => CtColor::AnsiValue(i),
    })
}

fn modifiers_to_crossterm(modifier: Modifier) -> CtAttributes {
    let mut attributes = CtAttributes::none();
    if modifier.contains(Modifier::BOLD) {
        attributes = attributes.with(CtAttribute::Bold);
    }
    if modifier.contains(Modifier::ITALIC) {
        attributes = attributes.with(CtAttribute::Italic);
    }
    if modifier.contains(Modifier::UNDERLINED) {
        attributes = attributes.with(CtAttribute::Underlined);
    }
    if modifier.contains(Modifier::DIM) {
        attributes = attributes.with(CtAttribute::Dim);
    }
    attributes
}

fn transcript_entry_is_stable(entry: &Entry) -> bool {
    match entry {
        Entry::Assistant { streaming, .. } | Entry::Reasoning { streaming, .. } => !streaming,
        Entry::Tool { status, .. } => *status != ToolStatus::Running,
        Entry::User { .. } | Entry::Status { .. } | Entry::Error { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context::ContextSource;
    use prompt::PromptBundle;
    use std::path::PathBuf;

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
                search_mode: "native".to_string(),
                date: "2026-06-29".to_string(),
            },
            project_context: vec![source],
            tool_catalog: tools::tool_definitions(),
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
            output.contains("=== Lowered Umans Messages"),
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
    fn write_scroll_region_up_uses_half_open_rows_as_ansi_region() {
        let mut out = Vec::new();

        write_scroll_region_up(&mut out, 0, 9, 3).expect("write scroll region");

        assert_eq!(String::from_utf8(out).expect("utf8"), "\x1b[1;9r\x1b[3S\x1b[r");
    }

    #[test]
    fn write_history_line_pads_to_terminal_width() {
        let mut out = Vec::new();
        let mut line = Line::from(vec![Span::styled(
            "hi",
            Style::default().fg(Color::Red).bg(Color::Blue),
        )]);
        line.style = Style::default().bg(Color::Blue);

        write_history_line(&mut out, &line, 5).expect("write history line");

        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("hi"));
        assert!(rendered.contains("   "));
        assert!(rendered.ends_with("\x1b[0m"));
    }
}
