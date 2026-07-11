//! `thndrs` library entrypoint with terminal setup, draw loop, event polling,
//! and cleanup.
//!
//! The `thndrs` bin just calls [`run`].

pub mod cli;
pub mod server;

#[path = "core/agent.rs"]
pub mod agent;
#[path = "core/session/mod.rs"]
pub mod session;

#[path = "core/mod.rs"]
mod thndrs_core;

pub use cli::{app, input, renderer};

pub use prelude::*;
pub use thndrs_core::{
    acp, config, fuzzy, harness, internals, mcp, prelude, prompt, providers, search, skills, tools, utils,
};

#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("test environment lock")
    }
}

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent};

use acp::config::provider_label;
use app::PromptAccessory;
use app::{App, Msg, RunState, start_auto_compaction, update};
use cli::{
    Cli, Command,
    commands::acp::AcpCommand,
    commands::debug::DebugCommand,
    commands::mcp::McpCommand,
    commands::session::{SessionCommand, SessionDataFormat},
};
use mcp::manager::McpManager;
use prompt::PromptBundle;
use renderer::backend::TerminalBackend;
use renderer::region::LiveRegion;
use utils::datetime;

use thndrs_agent::CancelToken;
use thndrs_context::context;

enum AcpEventWrite {
    Continue,
    Finished,
    Cancelled,
    Failed(String),
}

/// State carried by the main loop for a single agent run.
struct AgentSlot {
    receiver: mpsc::Receiver<app::AgentEvent>,
    cancel: CancelToken,
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

#[derive(Clone, Debug)]
struct Observability {
    session_log_path: PathBuf,
    daily_log_path: PathBuf,
}

/// Run the TUI to completion using the given CLI configuration.
///
/// Sets up the terminal, drives the draw loop, polls events on a tick, and
/// restores the terminal on exit.
pub fn run(cli: &Cli) -> io::Result<()> {
    if let Some(command) = &cli.command {
        return run_command(cli, command);
    }
    if cli.print_prompt {
        return run_print_prompt(cli);
    }
    let tick = Duration::from_millis(cli.tick_rate_ms);
    run_inline(tick, cli)
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
    out.push_str(&serde_json::to_string_pretty(&tools::sorted_json_value(&tool_catalog)).unwrap_or_default());
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

fn run_command(cli: &Cli, command: &Command) -> io::Result<()> {
    match command {
        Command::Setup(command) => run_setup_command(cli, command),
        Command::Login(command) => run_login_command(cli, command),
        Command::Logout(command) => run_logout_command(cli, command),
        Command::Auth { command } => run_auth_command(cli, command),
        Command::Doctor(command) => run_doctor_command(cli, command),
        Command::Config { command } => run_config_command(cli, command),
        Command::Acp { command } => run_acp_command(cli, command),
        Command::Mcp { command } => run_mcp_command(cli, command),
        Command::Session { command } => run_session_command(cli, command),
        Command::Debug { command } => run_debug_command(cli, command),
    }
}

fn run_setup_command(_cli: &Cli, _command: &cli::commands::setup::SetupCommand) -> io::Result<()> {
    cli::commands::setup::run(_cli, _command)
}

fn run_login_command(_cli: &Cli, _command: &cli::commands::auth::LoginCommand) -> io::Result<()> {
    cli::commands::auth::run_login(_cli, _command)
}

fn run_logout_command(_cli: &Cli, _command: &cli::commands::auth::LogoutCommand) -> io::Result<()> {
    cli::commands::auth::run_logout(_cli, _command)
}

fn run_auth_command(_cli: &Cli, _command: &cli::commands::auth::AuthCommand) -> io::Result<()> {
    cli::commands::auth::run_auth(_cli, _command)
}

fn run_doctor_command(_cli: &Cli, _command: &cli::commands::doctor::DoctorCommand) -> io::Result<()> {
    cli::commands::doctor::run(_cli, _command)
}

fn run_config_command(_cli: &Cli, _command: &cli::commands::config::ConfigCommand) -> io::Result<()> {
    cli::commands::config::run(_cli, _command)
}

fn load_mcp_manager_for_workspace(workspace: &Path) -> io::Result<Arc<McpManager>> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    let effective = mcp::config::load_effective_mcp(workspace, &env_vars).map_err(io::Error::other)?;
    let mut manager = McpManager::from_config(&effective.config);
    manager.extend_diagnostics(effective.diagnostics);
    Ok(Arc::new(manager))
}

fn load_effective_mcp_for_workspace(workspace: &Path) -> io::Result<mcp::config::EffectiveMcpConfig> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    mcp::config::load_effective_mcp(workspace, &env_vars).map_err(io::Error::other)
}

fn run_mcp_command(cli: &Cli, command: &McpCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match command {
        McpCommand::List => run_mcp_list(cli, &mut lock),
        McpCommand::Test { name } => run_mcp_test(cli, name, &mut lock),
        McpCommand::Tools { name } => run_mcp_tools(cli, name, &mut lock),
        McpCommand::Call { server, tool, json } => run_mcp_call(cli, server, tool, json, &mut lock),
    }
}

fn run_session_command(cli: &Cli, command: &SessionCommand) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(&workspace));
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match command {
        SessionCommand::List => run_session_list(&dir, &mut lock),
        SessionCommand::Latest => run_session_latest(&dir, &mut lock),
        SessionCommand::Titles => run_session_titles(&dir, &mut lock),
        SessionCommand::Show { session_id } => run_session_show(&dir, session_id, &mut lock),
        SessionCommand::Resume { session_id } => run_session_resume(&dir, session_id, &mut lock),
        SessionCommand::Inspect { session_id, format } => run_session_inspect(&dir, session_id, *format, &mut lock),
        SessionCommand::Export { session_id, format } => run_session_export(&dir, session_id, *format, &mut lock),
    }
}

fn run_session_list<W: io::Write>(dir: &Path, writer: &mut W) -> io::Result<()> {
    let files = session::list_session_files(dir);
    if files.is_empty() {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    }
    let summaries = session::list_session_summaries(dir);

    for (file, summary) in files.into_iter().zip(summaries) {
        let id = file.file_stem().and_then(|stem| stem.to_str()).unwrap_or("session");
        writeln!(
            writer,
            "{id}\t{}\t{}\t{}",
            summary.title,
            summary.model,
            summary.sidebar_label().replace('\n', " ")
        )?;
    }
    Ok(())
}

fn run_session_latest<W: io::Write>(dir: &Path, writer: &mut W) -> io::Result<()> {
    let Some(file) = session::latest_session_file(dir) else {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    };
    let summary = session::SessionReader::read_summary(&file);
    let id = file.file_stem().and_then(|stem| stem.to_str()).unwrap_or("session");
    writeln!(writer, "id: {id}")?;
    writeln!(writer, "path: {}", file.display())?;
    writeln!(writer, "title: {}", summary.title)?;
    writeln!(writer, "model: {}", summary.model)?;
    writeln!(
        writer,
        "tokens: in {} out {}",
        summary.input_tokens, summary.output_tokens
    )?;
    Ok(())
}

fn run_session_titles<W: io::Write>(dir: &Path, writer: &mut W) -> io::Result<()> {
    let titles = session::list_session_titles(dir);
    if titles.is_empty() {
        writeln!(writer, "no sessions found")?;
        return Ok(());
    }

    for title in titles {
        writeln!(writer, "{title}")?;
    }
    Ok(())
}

fn run_session_show<W: io::Write>(dir: &Path, session_id: &str, writer: &mut W) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;

    let _reader = session::SessionReader;
    let transcript = session::SessionReader::read_transcript(&path);
    if transcript.is_empty() {
        writeln!(writer, "session `{session_id}` has no replayable transcript")?;
        return Ok(());
    }

    for entry in transcript {
        match entry {
            app::Entry::User { text } => writeln!(writer, "user: {text}")?,
            app::Entry::Agent { text, .. } => writeln!(writer, "assistant: {text}")?,
            app::Entry::Reasoning { text, .. } => writeln!(writer, "reasoning: {text}")?,
            app::Entry::Tool { name, status, output, .. } => {
                writeln!(writer, "tool {name} {:?}: {}", status, output.join("\n"))?;
            }
            app::Entry::Status { text } => writeln!(writer, "status: {text}")?,
            app::Entry::Error { text } => writeln!(writer, "error: {text}")?,
        }
    }
    Ok(())
}

fn run_session_resume<W: io::Write>(dir: &Path, session_id: &str, writer: &mut W) -> io::Result<()> {
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let session_writer = session::SessionWriter::resume(&path, id)?;
    writeln!(writer, "resumed: {id}")?;
    writeln!(writer, "path: {}", session_writer.path().display())?;
    Ok(())
}

fn run_session_inspect<W: io::Write>(
    dir: &Path, session_id: &str, format: SessionDataFormat, writer: &mut W,
) -> io::Result<()> {
    if format != SessionDataFormat::Json {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inspect only supports --format json",
        ));
    }
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let summary = session::SessionReader::read_summary(&path);
    let records = session::SessionReader::read_redacted_records(&path);
    let projection = serde_json::json!({
        "schema_version": session::SCHEMA_VERSION,
        "session": {
            "id": id,
            "path": path,
            "title": summary.title,
            "model": summary.model,
            "usage": { "input_tokens": summary.input_tokens, "output_tokens": summary.output_tokens },
        },
        "records": records,
    });
    serde_json::to_writer_pretty(&mut *writer, &projection).map_err(io::Error::other)?;
    writeln!(writer)?;
    Ok(())
}

fn run_session_export<W: io::Write>(
    dir: &Path, session_id: &str, format: SessionDataFormat, writer: &mut W,
) -> io::Result<()> {
    if format != SessionDataFormat::Jsonl {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "export only supports --format jsonl",
        ));
    }
    let path = session::resolve_session_file(dir, session_id).map_err(io::Error::other)?;
    for record in session::SessionReader::read_redacted_records(&path) {
        serde_json::to_writer(&mut *writer, &record).map_err(io::Error::other)?;
        writeln!(writer)?;
    }
    Ok(())
}

fn run_debug_command(cli: &Cli, command: &DebugCommand) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match command {
        DebugCommand::Tail { lines } => run_debug_tail(&workspace, *lines, &mut lock),
        DebugCommand::SessionLog { session_id, lines } => {
            run_debug_session_log(&workspace, session_id, *lines, &mut lock)
        }
    }
}

fn run_debug_tail<W: io::Write>(workspace: &Path, lines: usize, writer: &mut W) -> io::Result<()> {
    let daily_dir = workspace.join(".thndrs").join("logs").join("daily");
    let Some(path) = newest_log_file(&daily_dir) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no daily debug log found"));
    };
    write_log_tail(&path, lines, writer)
}

fn run_debug_session_log<W: io::Write>(
    workspace: &Path, session_id: &str, lines: usize, writer: &mut W,
) -> io::Result<()> {
    let sessions = session::sessions_dir(workspace);
    let session_path = session::resolve_session_file(&sessions, session_id).map_err(io::Error::other)?;
    let id = session_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(session_id);
    let log_path = workspace
        .join(".thndrs")
        .join("logs")
        .join("sessions")
        .join(format!("thndrs-{id}.log"));
    write_log_tail(&log_path, lines, writer)
}

fn write_log_tail<W: io::Write>(path: &Path, lines: usize, writer: &mut W) -> io::Result<()> {
    let tail = session::read_redacted_log_tail(path, lines.min(2_000));
    if tail.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("debug log `{}` is empty or missing", path.display()),
        ));
    }
    for line in tail {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

fn newest_log_file(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .collect();
    files.sort_by(|left, right| {
        let left_time = std::fs::metadata(left).and_then(|metadata| metadata.modified()).ok();
        let right_time = std::fs::metadata(right).and_then(|metadata| metadata.modified()).ok();
        right_time.cmp(&left_time).then_with(|| right.cmp(left))
    });
    files.into_iter().next()
}

fn run_mcp_list<W: io::Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    if effective.config.servers.is_empty() {
        writeln!(writer, "no MCP servers configured")?;
        return Ok(());
    }

    for (name, server) in &effective.config.servers {
        let status = if server.enabled { "enabled" } else { "disabled" };
        writeln!(writer, "{name}\t{status}\t{:?}", server.transport)?;
    }
    for diagnostic in effective.diagnostics {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    Ok(())
}

fn run_mcp_test<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective.config, name)?;
    let client = mcp::manager::McpClient::connect(name.to_string(), &server).map_err(io::Error::other)?;
    writeln!(writer, "{name}\tready\t{} tools", client.tool_definitions().len())?;
    for diagnostic in client.diagnostics() {
        writeln!(writer, "diagnostic: {diagnostic}")?;
    }
    Ok(())
}

fn run_mcp_tools<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective.config, name)?;
    let client = mcp::manager::McpClient::connect(name.to_string(), &server).map_err(io::Error::other)?;
    for tool in client.tool_definitions() {
        writeln!(writer, "{}\t{}", tool.name, tool.description)?;
    }
    Ok(())
}

fn run_mcp_call<W: io::Write>(
    cli: &Cli, server_name: &str, tool_name: &str, json: &str, writer: &mut W,
) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let effective = load_effective_mcp_for_workspace(&workspace)?;
    let server = configured_mcp_server(&effective.config, server_name)?;
    let client = mcp::manager::McpClient::connect(server_name.to_string(), &server).map_err(io::Error::other)?;
    let namespaced = mcp::adapter::namespaced_tool_name(server_name, tool_name);
    let request = tools::ToolUseRequest::new(namespaced, json.to_string(), "cli".to_string());
    let output = client.call_tool(&request);
    match output.status {
        app::ToolStatus::Failed => writeln!(
            writer,
            "failed: {}",
            output.error.unwrap_or_else(|| "MCP tool failed".to_string())
        ),
        _ => {
            for line in output.output {
                writeln!(writer, "{line}")?;
            }
            Ok(())
        }
    }
}

fn configured_mcp_server(config: &mcp::config::McpConfig, name: &str) -> io::Result<mcp::config::McpServerConfig> {
    let server = config.servers.get(name).cloned().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("MCP server `{name}` is not configured"),
        )
    })?;
    if !server.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("MCP server `{name}` is disabled"),
        ));
    }
    Ok(server)
}

fn run_acp_command(cli: &Cli, command: &AcpCommand) -> io::Result<()> {
    match command {
        AcpCommand::List => {
            print!("{}", render_acp_list(cli));
            Ok(())
        }
        AcpCommand::Inspect { name } => {
            print!("{}", render_acp_inspect(cli, name)?);
            Ok(())
        }
        AcpCommand::Smoke { name, prompt } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_smoke(cli, name, prompt, &mut lock)
        }
        AcpCommand::Logout { name } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_logout(cli, name, &mut lock)
        }
        AcpCommand::ListSessions { name } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_list_sessions(cli, name, &mut lock)
        }
        AcpCommand::LoadSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_load_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::ResumeSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_resume_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::CloseSession { name, session_id } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_close_session(cli, name, session_id, &mut lock)
        }
        AcpCommand::Registry { file } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_registry(file.as_deref(), &mut lock)
        }
        AcpCommand::Install { agent_id, name, file, yes } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_install(cli, agent_id, name.clone(), file.as_deref(), *yes, &mut lock)
        }
        AcpCommand::Update { name, file, yes } => {
            let stdout = io::stdout();
            let mut lock = stdout.lock();
            run_acp_update(cli, name, file.as_deref(), *yes, &mut lock)
        }
    }
}

fn render_acp_list(cli: &Cli) -> String {
    let mut out = String::new();
    if cli.acp_agents.is_empty() {
        out.push_str("no ACP agents configured\n");
        return out;
    }

    for (name, agent) in &cli.acp_agents {
        let status = if agent.enabled { "enabled" } else { "disabled" };
        out.push_str(&format!(
            "{name}\t{status}\t{}\n",
            acp::config::redacted_command_display(agent)
        ));
    }
    out
}

fn render_acp_inspect(cli: &Cli, name: &str) -> io::Result<String> {
    let agent = cli
        .acp_agents
        .get(name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    let source = cli
        .config_origins
        .get("acp_agents")
        .map(|origin| format!("{}:{}", origin.source.as_str(), origin.detail))
        .unwrap_or_else(|| "default:default".to_string());
    let env_keys = if agent.env.is_empty() {
        String::from("none")
    } else {
        agent.env.keys().cloned().collect::<Vec<_>>().join(", ")
    };
    let args = if agent.args.is_empty() { String::from("none") } else { agent.args.join(" ") };

    Ok(format!(
        "name: {name}\nstatus: {}\ncommand: {}\nargs: {args}\nenv_keys: {env_keys}\ntimeout_secs: {}\nsource: {source}\n",
        if agent.enabled { "enabled" } else { "disabled" },
        acp::config::redacted_command_display(agent),
        agent.timeout_secs,
    ))
}

fn run_acp_smoke<W: io::Write>(cli: &Cli, name: &str, prompt: &str, writer: &mut W) -> io::Result<()> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    if !agent.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ACP agent `{name}` is disabled"),
        ));
    }

    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let mut handle = acp::runner::RunHandle::new(
        workspace_root.clone(),
        name.to_string(),
        Some(agent),
        prompt.to_string(),
    );
    if let Ok(effective_mcp) = load_effective_mcp_for_workspace(&workspace_root) {
        handle = handle
            .with_mcp_config(effective_mcp.config)
            .with_mcp_diagnostics(effective_mcp.diagnostics);
    }
    let rx = handle.spawn();
    for event in rx {
        match write_acp_event(writer, event)? {
            AcpEventWrite::Continue => {}
            AcpEventWrite::Finished => {
                writeln!(writer, "\nfinished")?;
                return Ok(());
            }
            AcpEventWrite::Cancelled => {
                writeln!(writer, "\ncancelled")?;
                return Ok(());
            }
            AcpEventWrite::Failed(message) => {
                writeln!(writer, "\nfailed: {message}")?;
                return Err(io::Error::other(message));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "ACP smoke stream ended before a terminal event",
    ))
}

fn run_acp_logout<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    for line in acp::runner::logout(name, agent).map_err(io::Error::other)? {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

fn run_acp_list_sessions<W: io::Write>(cli: &Cli, name: &str, writer: &mut W) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let sessions = acp::runner::list_sessions(name, agent, workspace_root).map_err(io::Error::other)?;
    if sessions.is_empty() {
        writeln!(writer, "no ACP sessions found")?;
        return Ok(());
    }
    for session in sessions {
        let title = session.title.unwrap_or_else(|| "-".to_string());
        let updated_at = session.updated_at.unwrap_or_else(|| "-".to_string());
        writeln!(
            writer,
            "{}\t{}\t{}\t{}",
            session.session_id,
            session.cwd.display(),
            title,
            updated_at
        )?;
        if !session.additional_directories.is_empty() {
            let dirs = session
                .additional_directories
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(writer, "  additional_directories: {dirs}")?;
        }
    }
    Ok(())
}

fn run_acp_load_session<W: io::Write>(cli: &Cli, name: &str, session_id: &str, writer: &mut W) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let events =
        acp::runner::load_session(name, agent, workspace_root, session_id.to_string()).map_err(io::Error::other)?;
    for event in events {
        match write_acp_event(writer, event)? {
            AcpEventWrite::Continue | AcpEventWrite::Finished | AcpEventWrite::Cancelled | AcpEventWrite::Failed(_) => {
            }
        }
    }
    writeln!(writer, "\nloaded: {name} {session_id}")?;
    Ok(())
}

fn run_acp_resume_session<W: io::Write>(cli: &Cli, name: &str, session_id: &str, writer: &mut W) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let metadata =
        acp::runner::resume_session(name, agent, workspace_root, session_id.to_string()).map_err(io::Error::other)?;
    writeln!(
        writer,
        "acp_session: {} {}",
        metadata.agent_name, metadata.acp_session_id
    )?;
    writeln!(writer, "resumed: {name} {session_id}")?;
    Ok(())
}

fn run_acp_close_session<W: io::Write>(cli: &Cli, name: &str, session_id: &str, writer: &mut W) -> io::Result<()> {
    let agent = configured_acp_agent(cli, name)?;
    for line in acp::runner::close_session(name, agent, session_id.to_string()).map_err(io::Error::other)? {
        writeln!(writer, "{line}")?;
    }
    Ok(())
}

fn run_acp_registry<W: io::Write>(file: Option<&Path>, writer: &mut W) -> io::Result<()> {
    let registry = match file {
        Some(path) => acp::registry::read_file(path),
        None => acp::registry::fetch_official(),
    }
    .map_err(io::Error::other)?;
    write!(writer, "{registry}")
}

fn run_acp_install<W: io::Write>(
    cli: &Cli, agent_id: &str, name: Option<String>, file: Option<&Path>, yes: bool, writer: &mut W,
) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let request = acp::registry::InstallRequest {
        agent_id: agent_id.to_string(),
        name,
        source: registry_source(file),
        confirmed: yes,
        timestamp: datetime::now_iso8601(),
    };
    let outcome = acp::registry::install(&workspace, &request).map_err(io::Error::other)?;
    writeln!(
        writer,
        "installed: {} {} {}",
        outcome.name, outcome.agent_id, outcome.agent_version
    )?;
    writeln!(writer, "model: {}", outcome.model)?;
    writeln!(writer, "config: {}", outcome.config_path.display())?;
    writeln!(writer, "metadata: {}", outcome.metadata_path.display())
}

fn run_acp_update<W: io::Write>(
    cli: &Cli, name: &str, file: Option<&Path>, yes: bool, writer: &mut W,
) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let request = acp::registry::UpdateRequest {
        name: name.to_string(),
        source: registry_source(file),
        confirmed: yes,
        timestamp: datetime::now_iso8601(),
    };
    let outcome = acp::registry::update(&workspace, &request).map_err(io::Error::other)?;
    writeln!(
        writer,
        "updated: {} {} {}",
        outcome.name, outcome.agent_id, outcome.agent_version
    )?;
    writeln!(writer, "model: {}", outcome.model)?;
    writeln!(writer, "config: {}", outcome.config_path.display())?;
    writeln!(writer, "metadata: {}", outcome.metadata_path.display())
}

fn registry_source(file: Option<&Path>) -> acp::registry::RegistrySource {
    match file {
        Some(path) => acp::registry::RegistrySource::File(path.to_path_buf()),
        None => acp::registry::RegistrySource::Official,
    }
}

fn write_acp_event<W: io::Write>(writer: &mut W, event: app::AgentEvent) -> io::Result<AcpEventWrite> {
    match event {
        app::AgentEvent::Started => writeln!(writer, "started")?,
        app::AgentEvent::Status(text) => writeln!(writer, "status: {text}")?,
        app::AgentEvent::AssistantDelta(text) => write!(writer, "{text}")?,
        app::AgentEvent::ReasoningDelta(text) => writeln!(writer, "reasoning: {text}")?,
        app::AgentEvent::Usage { input_tokens, output_tokens } => {
            writeln!(writer, "usage: input={input_tokens} output={output_tokens}")?
        }
        app::AgentEvent::ToolStarted { id, name, arguments } => {
            writeln!(writer, "tool_started: {name}#{id} {arguments}")?
        }
        app::AgentEvent::ToolFinished { id, status, output, .. } => {
            writeln!(writer, "tool_finished: {id} {status:?} {}", output.join("\n"))?
        }
        app::AgentEvent::PermissionRequest(permission) => {
            writeln!(writer, "permission: {} ({})", permission.title, permission.tool_call_id)?;
            let _ = permission.cancel();
        }
        app::AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            writeln!(writer, "permission_resolved: {tool_call_id} {outcome}")?
        }
        app::AgentEvent::AcpSession(metadata) => writeln!(
            writer,
            "acp_session: {} {}",
            metadata.agent_name, metadata.acp_session_id
        )?,
        app::AgentEvent::ModelMetadataLoaded(_) | app::AgentEvent::Retrying { .. } => {}
        app::AgentEvent::Finished => return Ok(AcpEventWrite::Finished),
        app::AgentEvent::Cancelled => return Ok(AcpEventWrite::Cancelled),
        app::AgentEvent::Failed(message) => return Ok(AcpEventWrite::Failed(message)),
    }
    Ok(AcpEventWrite::Continue)
}

fn configured_acp_agent(cli: &Cli, name: &str) -> io::Result<config::AcpAgentConfig> {
    let agent = cli
        .acp_agents
        .get(name)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("ACP agent `{name}` is not configured")))?;
    if !agent.enabled {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("ACP agent `{name}` is disabled"),
        ));
    }
    Ok(agent)
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
    let mcp_manager = load_mcp_manager_for_workspace(&workspace_root).ok();
    let tool_catalog = tools::runtime_tool_definitions(mcp_manager.as_deref());

    let user_turn = String::from("(no user prompt — print-prompt debug mode)");
    let bundle = PromptBundle::new_with_skills(
        &workspace_root,
        &cli.model,
        cli.websearch,
        &context_sources,
        &skill_inventory.skills,
        &[],
        &user_turn,
    )
    .with_tool_catalog(tool_catalog);

    let mut output = render_print_prompt(&bundle);
    output.push_str(&render_print_prompt_config(cli, &workspace_root));
    print!("{output}");
    Ok(())
}

fn render_print_prompt_config(cli: &Cli, workspace_root: &Path) -> String {
    let session_dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(workspace_root));
    let mut out = String::new();

    out.push_str("\n\n=== Effective Config ===\n");
    out.push_str(&format!("  provider: {}\n", provider_label(&cli.model)));
    out.push_str(&format!("  model: {}\n", cli.model));
    out.push_str(&format!("  search: {}\n", cli.websearch.label()));
    out.push_str(&format!("  workspace: {}\n", workspace_root.display()));
    out.push_str(&format!("  session_dir: {}\n", session_dir.display()));

    out.push_str("  files:\n");
    if cli.config_layers.is_empty() {
        out.push_str("    none\n");
    } else {
        for layer in &cli.config_layers {
            let path = layer.display_path.as_deref().unwrap_or("<unknown>");
            let hash = layer.hash.as_deref().unwrap_or("");
            out.push_str(&format!("    {} {} {}\n", layer.source.as_str(), path, hash));
        }
    }

    out.push_str("  origins:\n");
    if cli.config_origins.is_empty() {
        out.push_str("    none\n");
    } else {
        for (key, origin) in &cli.config_origins {
            out.push_str(&format!("    {key}: {}:{}\n", origin.source.as_str(), origin.detail));
        }
    }

    out.push_str("  diagnostics:\n");
    if cli.config_diagnostics.is_empty() {
        out.push_str("    none");
    } else {
        for diagnostic in &cli.config_diagnostics {
            out.push_str(&format!("    {}\n", redact_secret(diagnostic)));
        }
    }

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
    let mut live = LiveRegion::new();
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
    backend: &mut TerminalBackend<W>, live: &mut LiveRegion, tick: Duration, cli: &Cli, mouse_enabled: bool,
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
            maybe_spawn_agent(&mut app, &mut agent);
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

            maybe_spawn_agent(&mut app, &mut agent);
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
    backend: &mut TerminalBackend<W>, live: &mut LiveRegion, app: &mut App, width: usize, height: usize,
) -> io::Result<()> {
    renderer::style::set_theme(app.theme);
    live.render_frame(app, backend, width, height)?;
    backend.flush()
}

/// Process a key in the direct renderer path.
fn handle_direct_key<W: io::Write>(
    app: &mut App, key: KeyEvent, agent: &mut Option<AgentSlot>, backend: &mut TerminalBackend<W>,
    live: &mut LiveRegion,
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
    app: &mut App, msg: Msg, backend: &mut TerminalBackend<W>, live: &mut LiveRegion,
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
    app: &mut App, agent: &mut Option<AgentSlot>, backend: &mut TerminalBackend<W>, live: &mut LiveRegion,
    observability: &Option<Observability>,
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
                if app.run_state == RunState::Stopping {
                    handle_direct_msg(app, Msg::Agent(app::AgentEvent::Cancelled), backend, live)?;
                }
                break;
            }
        }
    }
    Ok(())
}

fn drain_git_status_watcher<W: io::Write>(
    app: &mut App, watcher: &GitStatusWatcher, backend: &mut TerminalBackend<W>, live: &mut LiveRegion,
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
fn maybe_spawn_agent(app: &mut App, agent: &mut Option<AgentSlot>) {
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
    let cli = app.cli.clone();
    let workspace_root = context::discover_workspace_root(&cli.cwd);
    let resolved_websearch = cli.websearch.resolve_for_prompt(&prompt);
    let mut config = tools::AgentRunConfig::new(workspace_root, cli.model.clone(), resolved_websearch);
    if let Some(acp_name) = acp::config::parse_model_id(&cli.model) {
        tracing::info!(
            cwd = %config.root.display(),
            model = %config.model,
            acp_agent = %acp_name,
            "spawning ACP agent run"
        );
        let (steering_tx, _steering_rx) = mpsc::channel();
        let mut handle = acp::runner::RunHandle::new(
            config.root,
            acp_name.to_string(),
            cli.acp_agents.get(acp_name).cloned(),
            prompt,
        );
        if let Ok(effective_mcp) = load_effective_mcp_for_workspace(&handle.root) {
            handle = handle
                .with_mcp_config(effective_mcp.config)
                .with_mcp_diagnostics(effective_mcp.diagnostics);
        }
        let cancel = handle.cancel.clone();
        let receiver = handle.spawn();
        *agent = Some(AgentSlot { receiver, cancel, steering: steering_tx });
        return;
    }
    let mcp_manager = load_mcp_manager_for_workspace(&config.root).ok();
    if let Some(manager) = mcp_manager.clone() {
        config = config.with_mcp_manager(manager);
    }
    tracing::info!(
        cwd = %config.root.display(),
        model = %config.model,
        requested_websearch = %cli.websearch.label(),
        resolved_websearch = %config.search_mode.header_value(),
        "spawning agent run"
    );

    let tool_catalog = tools::runtime_tool_definitions(mcp_manager.as_deref());
    let bundle = PromptBundle::new_with_skills(
        &config.root,
        &config.model,
        config.search_mode,
        &app.context_sources,
        &app.skills,
        &app.transcript,
        &prompt,
    )
    .with_tool_catalog(tool_catalog);

    if !app.compaction_in_flight() && preflight_requires_auto_compaction(app, &bundle) {
        start_auto_compaction(app, prompt);
        return;
    }

    if let Some(ref mut writer) = app.session_writer {
        let turn_id = format!("turn_{}", app.turn_count);
        if let Some(ledger) = &bundle.context_ledger {
            let _ = writer.append_context_ledger(&turn_id, ledger);
        }
        let metadata = session::PromptMetadata::from_bundle(&bundle);
        let _ = writer.append_prompt_metadata(&turn_id, &metadata);
    }
    let messages = prompt::lower_to_umans_messages(&bundle);
    let expects_write = agent::prompt_expects_workspace_write(&prompt);
    let (steering_tx, steering_rx) = mpsc::channel();
    let turn = harness::HarnessTurn::provider_with_steering(config, messages, expects_write, steering_rx).start();
    *agent = Some(AgentSlot { receiver: turn.events, cancel: turn.cancel, steering: steering_tx });
}

/// Whether the upcoming provider request is oversized and needs auto-compaction.
///
/// Resolves model limits conservatively (static provider metadata or fallback;
/// live metadata loads inside the agent thread), estimates the full prompt
/// token cost from the lowered provider messages (which include the rendered
/// system prompt as the first message), and runs the pure
/// [`context::preflight_auto_compaction`] decision.
///
/// Returns `false` when auto mode is disabled or the estimate fits the policy.
fn preflight_requires_auto_compaction(app: &App, bundle: &PromptBundle) -> bool {
    let policy = app::effective_compaction_policy(app);
    if !matches!(policy.mode, context::CompactionMode::Auto) {
        return false;
    }
    let provider = acp::config::provider_label(&app.model);
    let (limits, _) = context::ModelContextLimits::resolve(provider, &app.model, None, None);
    let messages = prompt::lower_to_umans_messages(bundle);
    let bytes = messages.iter().map(|message| message.as_text().len()).sum::<usize>();
    let estimate = context::estimate_tokens(bytes) as u64;
    matches!(
        context::preflight_auto_compaction(policy, &limits, estimate),
        context::AutoCompactionDecision::Compact
    )
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

/// Keep the active agent slot aligned with the app lifecycle.
///
/// `Stopping` is a pending terminal state: keep the receiver alive so the app
/// can observe `Cancelled`/`Finished`/`Failed` and transition back to idle.
fn manage_agent_lifecycle(app: &App, agent: &mut Option<AgentSlot>) {
    match app.run_state {
        RunState::Working => {}
        RunState::Stopping => {
            if let Some(slot) = agent {
                slot.cancel.cancel();
            }
        }
        RunState::Idle | RunState::Error(_) => {
            if let Some(slot) = agent.take() {
                tracing::info!("cancelling dropped agent slot");
                slot.cancel.cancel();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context::ContextSource;
    use prompt::PromptBundle;
    use std::path::{Path, PathBuf};
    use std::process::Command;

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
            context_ledger: None,
        }
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

    // FIXME: should be a include_str!
    fn fake_agent_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("fake_acp_agent.py")
    }

    fn acp_fixture_cli(cwd: &Path, script: &str) -> Cli {
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "python3".to_string(),
                args: vec![fake_agent_fixture().display().to_string(), script.to_string()],
                timeout_secs: 2,
                ..config::AcpAgentConfig::default()
            },
        );
        Cli { cwd: cwd.to_path_buf(), acp_agents: agents, ..Cli::default() }
    }

    fn write_registry_fixture(path: &Path, version: &str) {
        std::fs::write(
            path,
            format!(
                r#"{{
                    "version": "1.0.0",
                    "agents": [{{
                        "id": "codex-acp",
                        "name": "Codex",
                        "version": "{version}",
                        "description": "ACP adapter",
                        "distribution": {{
                            "npx": {{
                                "package": "@agentclientprotocol/codex-acp@{version}",
                                "args": ["--acp"]
                            }}
                        }}
                    }}]
                }}"#
            ),
        )
        .expect("write registry");
    }

    fn write_fake_mcp_server(dir: &Path) -> PathBuf {
        let path = dir.join("fake_mcp_cli.py");
        std::fs::write(
            &path,
            r#"#!/usr/bin/env python3
import json
import sys

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "fake", "version": "0.1.0"}
            }
        }), flush=True)
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "Echo text",
                    "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}
                }]
            }
        }), flush=True)
    elif method == "tools/call":
        args = msg.get("params", {}).get("arguments", {})
        print(json.dumps({
            "jsonrpc": "2.0",
            "id": msg["id"],
            "result": {"content": [{"type": "text", "text": args.get("text", "")}], "isError": False}
        }), flush=True)
"#,
        )
        .expect("write fake mcp server");
        path
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
    fn render_print_prompt_config_includes_effective_config_metadata() {
        let mut origins = std::collections::BTreeMap::new();
        origins.insert(
            "model".to_string(),
            config::ConfigOrigin { source: config::ConfigSource::CliFlag, detail: "--model".to_string() },
        );
        let cli = Cli {
            model: "umans-glm-5.2".to_string(),
            websearch: cli::WebSearchMode::Exa,
            session_dir: Some(PathBuf::from("/repo/custom-sessions")),
            config_layers: vec![config::LoadedConfigLayer {
                source: config::ConfigSource::ProjectFile,
                config: config::Config::default(),
                path: None,
                display_path: Some(".thndrs/config.toml".to_string()),
                hash: Some("abc123".to_string()),
            }],
            config_origins: origins,
            config_diagnostics: vec!["diagnostic with sk-secret".to_string()],
            ..Cli::default()
        };

        let output = render_print_prompt_config(&cli, Path::new("/repo"));

        assert!(output.contains("=== Effective Config ==="));
        assert!(output.contains("provider: umans"));
        assert!(output.contains("model: umans-glm-5.2"));
        assert!(output.contains("search: exa"));
        assert!(output.contains("workspace: /repo"));
        assert!(output.contains("session_dir: /repo/custom-sessions"));
        assert!(output.contains("project .thndrs/config.toml abc123"));
        assert!(output.contains("model: cli:--model"));
        assert!(output.contains("sk-[REDACTED]secret"));

        insta::assert_snapshot!(output, @r###"


=== Effective Config ===
  provider: umans
  model: umans-glm-5.2
  search: exa
  workspace: /repo
  session_dir: /repo/custom-sessions
  files:
    project .thndrs/config.toml abc123
  origins:
    model: cli:--model
  diagnostics:
    diagnostic with sk-[REDACTED]secret
"###);
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
    fn acp_list_renders_enabled_and_disabled_agents() {
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "disabled".to_string(),
            config::AcpAgentConfig {
                command: "agent-off".to_string(),
                enabled: false,
                ..config::AcpAgentConfig::default()
            },
        );
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "agent".to_string(),
                args: vec!["--acp".to_string()],
                ..config::AcpAgentConfig::default()
            },
        );
        let cli = Cli { acp_agents: agents, ..Cli::default() };

        let output = render_acp_list(&cli);

        assert!(output.contains("disabled\tdisabled\tagent-off"));
        assert!(output.contains("local\tenabled\tagent --acp"));
    }

    #[test]
    fn acp_inspect_renders_redacted_config_details() {
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "agent".to_string(),
                args: vec!["--acp".to_string()],
                env: std::collections::BTreeMap::from([
                    ("TOKEN".to_string(), "secret-value".to_string()),
                    ("PLAIN".to_string(), "visible-value".to_string()),
                ]),
                timeout_secs: 12,
                ..config::AcpAgentConfig::default()
            },
        );
        let mut origins = std::collections::BTreeMap::new();
        origins.insert(
            "acp_agents".to_string(),
            config::ConfigOrigin {
                source: config::ConfigSource::ProjectFile,
                detail: ".thndrs/config.toml".to_string(),
            },
        );
        let cli = Cli { acp_agents: agents, config_origins: origins, ..Cli::default() };

        let output = render_acp_inspect(&cli, "local").expect("inspect");

        assert!(output.contains("name: local"));
        assert!(output.contains("status: enabled"));
        assert!(output.contains("command: agent --acp"));
        assert!(output.contains("args: --acp"));
        assert!(output.contains("env_keys: PLAIN, TOKEN"));
        assert!(output.contains("timeout_secs: 12"));
        assert!(output.contains("source: project:.thndrs/config.toml"));
        assert!(!output.contains("secret-value"));
        assert!(!output.contains("visible-value"));
    }

    #[test]
    fn acp_smoke_runs_fake_agent_and_prints_events() {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "python3".to_string(),
                args: vec![fake_agent_fixture().display().to_string(), "lifecycle".to_string()],
                timeout_secs: 2,
                ..config::AcpAgentConfig::default()
            },
        );
        let cli = Cli { cwd: temp.path().to_path_buf(), acp_agents: agents, ..Cli::default() };
        let mut output = Vec::new();

        run_acp_smoke(&cli, "local", "ping", &mut output).expect("smoke run");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("started"));
        assert!(output.contains("status: acp: connected to fake-acp-agent 0.0.0"));
        assert!(output.contains("acp_session: local fake-session-1"));
        assert!(output.contains("pong from fake ACP agent"));
        assert!(output.contains("finished"));
    }

    #[test]
    fn acp_logout_runs_fake_agent_and_prints_result() {
        let mut agents = config::AcpAgentsConfig::new();
        agents.insert(
            "local".to_string(),
            config::AcpAgentConfig {
                command: "python3".to_string(),
                args: vec![fake_agent_fixture().display().to_string(), "auth-success".to_string()],
                timeout_secs: 2,
                ..config::AcpAgentConfig::default()
            },
        );
        let cli = Cli { acp_agents: agents, ..Cli::default() };
        let mut output = Vec::new();

        run_acp_logout(&cli, "local", &mut output).expect("logout run");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("acp: logged out `local`"));
    }

    #[test]
    fn acp_list_sessions_runs_fake_agent_and_prints_sessions() {
        let temp = tempfile::tempdir().expect("temp dir");
        let cli = acp_fixture_cli(temp.path(), "sessions");
        let mut output = Vec::new();

        run_acp_list_sessions(&cli, "local", &mut output).expect("list sessions");
        let output = String::from_utf8(output).expect("utf8");
        assert!(output.contains("external-session-1"));
        assert!(output.contains("Fixture Session"));
        assert!(output.contains("2026-07-04T00:00:00Z"));
    }

    #[test]
    fn acp_load_session_runs_fake_agent_and_prints_replay() {
        let temp = tempfile::tempdir().expect("temp dir");
        let cli = acp_fixture_cli(temp.path(), "sessions");
        let mut output = Vec::new();

        run_acp_load_session(&cli, "local", "external-session-1", &mut output).expect("load session");
        let output = String::from_utf8(output).expect("utf8");
        assert!(output.contains("replayed external-session-1"));
        assert!(output.contains("loaded: local external-session-1"));
    }

    #[test]
    fn acp_resume_and_close_session_run_fake_agent() {
        let temp = tempfile::tempdir().expect("temp dir");
        let cli = acp_fixture_cli(temp.path(), "sessions");
        let mut resume_output = Vec::new();
        let mut close_output = Vec::new();

        run_acp_resume_session(&cli, "local", "external-session-1", &mut resume_output).expect("resume session");
        run_acp_close_session(&cli, "local", "external-session-1", &mut close_output).expect("close session");
        let resume_output = String::from_utf8(resume_output).expect("utf8");
        let close_output = String::from_utf8(close_output).expect("utf8");
        assert!(resume_output.contains("acp_session: local external-session-1"));
        assert!(resume_output.contains("resumed: local external-session-1"));
        assert!(close_output.contains("acp: closed `local` session external-session-1"));
    }

    #[test]
    fn session_list_and_latest_print_local_session_summaries() {
        let temp = tempfile::tempdir().expect("temp dir");
        let session_dir = temp.path().join("sessions");
        let mut writer = session::SessionWriter::create(
            &session_dir,
            "session-new",
            "/repo",
            "Latest Work",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create session");
        writer.append_usage(7, 11).expect("append usage");

        let mut list_output = Vec::new();
        let mut latest_output = Vec::new();

        run_session_list(&session_dir, &mut list_output).expect("list sessions");
        run_session_latest(&session_dir, &mut latest_output).expect("latest session");
        let list_output = String::from_utf8(list_output).expect("utf8");
        let latest_output = String::from_utf8(latest_output).expect("utf8");

        assert!(list_output.contains("session-new"));
        assert!(list_output.contains("Latest Work"));
        assert!(list_output.contains("umans-coder"));
        assert!(list_output.contains("in 7 out 11"));
        assert!(latest_output.contains("id: session-new"));
        assert!(latest_output.contains("title: Latest Work"));
        assert!(latest_output.contains("tokens: in 7 out 11"));
    }

    #[test]
    fn session_titles_prints_titles_newest_first() {
        let temp = tempfile::tempdir().expect("temp dir");
        let session_dir = temp.path().join("sessions");
        session::SessionWriter::create(
            &session_dir,
            "session-old",
            "/repo",
            "Old",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create old session");
        std::thread::sleep(Duration::from_millis(5));
        session::SessionWriter::create(
            &session_dir,
            "session-new",
            "/repo",
            "New",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create new session");

        let mut output = Vec::new();

        run_session_titles(&session_dir, &mut output).expect("titles");
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(output.lines().collect::<Vec<_>>(), vec!["New", "Old"]);
    }

    #[test]
    fn session_show_prints_replayable_transcript() {
        let temp = tempfile::tempdir().expect("temp dir");
        let session_dir = temp.path().join("sessions");
        let mut writer = session::SessionWriter::create(
            &session_dir,
            "session-show",
            "/repo",
            "Show",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create session");
        writer
            .append_entry(&app::Entry::User { text: "hello".to_string() }, "turn_1")
            .expect("append user");
        writer
            .append_entry(
                &app::Entry::Agent { text: "hi".to_string(), streaming: false },
                "turn_1",
            )
            .expect("append assistant");
        let mut output = Vec::new();

        run_session_show(&session_dir, "session-show", &mut output).expect("show session");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("user: hello"));
        assert!(output.contains("assistant: hi"));
    }

    #[test]
    fn session_inspect_and_export_are_redacted_and_sequence_ordered() {
        let temp = tempfile::tempdir().expect("temp dir");
        let session_dir = temp.path().join("sessions");
        let mut writer = session::SessionWriter::create(
            &session_dir,
            "session-inspect",
            "/repo",
            "Inspect",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create session");
        writer
            .append_entry(
                &app::Entry::User { text: "api_key=sk-secretvalue123".to_string() },
                "turn_1",
            )
            .expect("append user");
        writer.append_usage(2, 3).expect("append usage");
        drop(writer);

        let mut inspect = Vec::new();
        let mut export = Vec::new();
        run_session_inspect(&session_dir, "session-ins", SessionDataFormat::Json, &mut inspect).expect("inspect");
        run_session_export(&session_dir, "session-inspect", SessionDataFormat::Jsonl, &mut export).expect("export");
        let inspect = String::from_utf8(inspect).expect("utf8");
        let export = String::from_utf8(export).expect("utf8");

        assert!(inspect.contains("\"input_tokens\": 2"));
        assert!(inspect.contains("api_key=[REDACTED]"));
        assert!(!inspect.contains("sk-secretvalue123"));
        let lines = export.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("session_meta"));
        assert!(lines[1].contains("user"));
        assert!(lines[2].contains("usage"));
    }

    #[test]
    fn debug_session_log_reads_a_bounded_redacted_tail() {
        let temp = tempfile::tempdir().expect("temp dir");
        let sessions_dir = session::sessions_dir(temp.path());
        let writer = session::SessionWriter::create(
            &sessions_dir,
            "session-log",
            "/repo",
            "Log",
            "umans",
            "umans-coder",
            "none",
            "0.1.0",
            None,
        )
        .expect("create session");
        drop(writer);
        let log_dir = temp.path().join(".thndrs").join("logs").join("sessions");
        std::fs::create_dir_all(&log_dir).expect("create log dir");
        std::fs::write(
            log_dir.join("thndrs-session-log.log"),
            "first\napi_key=sk-secretvalue123\nlast\n",
        )
        .expect("write log");

        let mut output = Vec::new();
        run_debug_session_log(temp.path(), "session-l", 2, &mut output).expect("read log");
        let output = String::from_utf8(output).expect("utf8");

        assert!(!output.contains("first"));
        assert!(output.contains("api_key=[REDACTED]"));
        assert!(!output.contains("sk-secretvalue123"));
        assert!(output.contains("last"));
    }

    #[test]
    fn acp_registry_reads_file_and_prints_review_gate() {
        let temp = tempfile::tempdir().expect("temp dir");
        let registry_path = temp.path().join("registry.json");
        std::fs::write(
            &registry_path,
            r#"{
                "version": "1.0.0",
                "agents": [{
                    "id": "codex-acp",
                    "name": "Codex",
                    "version": "1.1.0",
                    "description": "ACP adapter for OpenAI's coding assistant",
                    "repository": "https://github.com/agentclientprotocol/codex-acp",
                    "distribution": {
                        "npx": {
                            "package": "@agentclientprotocol/codex-acp@1.1.0",
                            "env": {"OPENAI_API_KEY": "sk-secret"}
                        }
                    }
                }]
            }"#,
        )
        .expect("write registry");
        let mut output = Vec::new();

        run_acp_registry(Some(&registry_path), &mut output).expect("registry");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("ACP registry v1.0.0"));
        assert!(output.contains("codex-acp\tCodex\t1.1.0\tnpx:@agentclientprotocol/codex-acp@1.1.0"));
        assert!(output.contains("install/update: use `thndrs acp install"));
        assert!(!output.contains("OPENAI_API_KEY"));
        assert!(!output.contains("sk-secret"));
    }

    #[test]
    fn acp_install_and_update_registry_agent() {
        let temp = tempfile::tempdir().expect("temp dir");
        let registry_path = temp.path().join("registry.json");
        write_registry_fixture(&registry_path, "1.1.0");
        let cli = Cli { cwd: temp.path().to_path_buf(), ..Cli::default() };
        let mut install_output = Vec::new();

        run_acp_install(
            &cli,
            "codex-acp",
            Some("codex".to_string()),
            Some(&registry_path),
            true,
            &mut install_output,
        )
        .expect("install");
        let install_output = String::from_utf8(install_output).expect("utf8");

        assert!(install_output.contains("installed: codex codex-acp 1.1.0"));
        assert!(install_output.contains("model: acp:codex"));

        write_registry_fixture(&registry_path, "1.2.0");
        let mut update_output = Vec::new();
        run_acp_update(&cli, "codex", Some(&registry_path), true, &mut update_output).expect("update");
        let update_output = String::from_utf8(update_output).expect("utf8");

        assert!(update_output.contains("updated: codex codex-acp 1.2.0"));
        let config = std::fs::read_to_string(temp.path().join(".thndrs/config.toml")).expect("config");
        assert!(config.contains("@agentclientprotocol/codex-acp@1.2.0"));
    }

    #[test]
    fn mcp_list_tools_and_call_use_fake_server() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        std::fs::create_dir_all(workspace.join(".thndrs")).expect("create config dir");
        let script = write_fake_mcp_server(tmp.path());
        std::fs::write(
            workspace.join(".thndrs").join("mcp.toml"),
            format!(
                r#"
                [servers.docs]
                command = "python3"
                args = [{:?}]
                timeout_secs = 5
                "#,
                script.display().to_string()
            ),
        )
        .expect("write mcp config");
        let cli = Cli { cwd: workspace, ..Cli::default() };

        let mut list_output = Vec::new();
        run_mcp_list(&cli, &mut list_output).expect("list mcp");
        let list = String::from_utf8(list_output).expect("utf8");
        assert!(list.contains("docs"));
        assert!(list.contains("enabled"));

        let mut tools_output = Vec::new();
        run_mcp_tools(&cli, "docs", &mut tools_output).expect("tools mcp");
        let tools = String::from_utf8(tools_output).expect("utf8");
        assert!(tools.contains("mcp__docs__echo"));

        let mut call_output = Vec::new();
        run_mcp_call(&cli, "docs", "echo", r#"{"text":"hello"}"#, &mut call_output).expect("call mcp");
        let call = String::from_utf8(call_output).expect("utf8");
        assert!(call.contains("hello"));
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
        let slot = AgentSlot { receiver: event_rx, cancel: CancelToken::new(), steering: steering_tx };

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
    fn manage_agent_lifecycle_keeps_stopping_slot_until_terminal_event() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Stopping;
        let (_event_tx, event_rx) = mpsc::channel();
        let (steering_tx, _steering_rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let mut agent = Some(AgentSlot { receiver: event_rx, cancel: cancel.clone(), steering: steering_tx });

        manage_agent_lifecycle(&app, &mut agent);

        assert!(agent.is_some(), "stopping should keep the receiver for terminal events");
        assert!(
            cancel.is_cancelled(),
            "stopping should still signal cooperative cancellation"
        );
    }

    #[test]
    fn disconnected_stopping_agent_returns_app_to_idle() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Stopping;
        let (event_tx, event_rx) = mpsc::channel();
        drop(event_tx);
        let (steering_tx, _steering_rx) = mpsc::channel();
        let mut agent = Some(AgentSlot { receiver: event_rx, cancel: CancelToken::new(), steering: steering_tx });
        let mut live = renderer::region::LiveRegion::new();
        let mut backend = TerminalBackend::new(Vec::new(), 80, 24);

        drain_direct_agent_events(&mut app, &mut agent, &mut backend, &mut live, &None).expect("drain events");

        assert!(agent.is_none(), "disconnected slot should be cleared");
        assert_eq!(app.run_state, RunState::Idle);
    }

    #[test]
    fn maybe_spawn_agent_uses_current_app_model_after_picker_switch() {
        let cli = Cli::default();
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.model = "fake-agent".to_string();
        app.cli.model = app.model.clone();
        app.run_state = RunState::Working;
        app.transcript
            .push(app::Entry::User { text: "hello with switched model".to_string() });

        let mut agent = None;
        maybe_spawn_agent(&mut app, &mut agent);
        let slot = agent.as_ref().expect("agent spawned");

        let first_model_event = loop {
            let event = slot
                .receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("agent event");
            if !matches!(event, app::AgentEvent::Started) {
                break event;
            }
        };
        slot.cancel.cancel();

        assert!(
            matches!(first_model_event, app::AgentEvent::ReasoningDelta(_)),
            "switched fake model should run fake provider, got {first_model_event:?}"
        );
    }

    #[test]
    fn maybe_spawn_agent_auto_compacts_oversized_turn_instead_of_spawning() {
        let mut config = config::Config::default();
        config.context.compaction.mode = context::CompactionMode::Auto;
        let cli = Cli {
            model: "fake-agent".to_string(),
            config_layers: vec![config::LoadedConfigLayer {
                source: config::ConfigSource::ProjectFile,
                config,
                path: None,
                display_path: None,
                hash: None,
            }],
            ..Cli::default()
        };
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Working;
        let big = "x".repeat(5_000);
        for _ in 0..20 {
            app.transcript.push(app::Entry::User { text: big.clone() });
            app.transcript
                .push(app::Entry::Agent { text: big.clone(), streaming: false });
        }
        app.transcript
            .push(app::Entry::User { text: "final oversized turn".to_string() });

        let mut agent = None;
        maybe_spawn_agent(&mut app, &mut agent);

        assert!(
            agent.is_none(),
            "the known-oversized request must never be sent to the main provider"
        );
        assert!(
            app.compaction_in_flight(),
            "auto-compaction should be triggered instead"
        );
        assert!(
            matches!(app.transcript.last(), Some(app::Entry::User { text }) if text.contains("Summarize")),
            "the compaction prompt should be the active turn"
        );
    }

    #[test]
    fn maybe_spawn_agent_does_not_auto_compact_when_mode_is_manual() {
        let mut config = config::Config::default();
        config.context.compaction.mode = context::CompactionMode::Manual;
        let cli = Cli {
            model: "fake-agent".to_string(),
            config_layers: vec![config::LoadedConfigLayer {
                source: config::ConfigSource::ProjectFile,
                config,
                path: None,
                display_path: None,
                hash: None,
            }],
            ..Cli::default()
        };
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Working;
        let big = "x".repeat(5_000);
        for _ in 0..20 {
            app.transcript.push(app::Entry::User { text: big.clone() });
            app.transcript
                .push(app::Entry::Agent { text: big.clone(), streaming: false });
        }
        app.transcript
            .push(app::Entry::User { text: "final oversized turn".to_string() });

        let mut agent = None;
        maybe_spawn_agent(&mut app, &mut agent);

        assert!(!app.compaction_in_flight(), "manual mode must not auto-compact");
        assert!(agent.is_some(), "manual mode sends the request to the provider");
        if let Some(slot) = agent {
            slot.cancel.cancel();
        }
    }

    #[test]
    fn maybe_spawn_agent_does_not_run_preflight_while_agent_in_flight() {
        let mut config = config::Config::default();
        config.context.compaction.mode = context::CompactionMode::Auto;
        let cli = Cli {
            model: "fake-agent".to_string(),
            config_layers: vec![config::LoadedConfigLayer {
                source: config::ConfigSource::ProjectFile,
                config,
                path: None,
                display_path: None,
                hash: None,
            }],
            ..Cli::default()
        };
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.run_state = RunState::Working;
        let big = "x".repeat(5_000);
        for _ in 0..20 {
            app.transcript.push(app::Entry::User { text: big.clone() });
            app.transcript
                .push(app::Entry::Agent { text: big.clone(), streaming: false });
        }
        app.transcript
            .push(app::Entry::User { text: "in-flight turn".to_string() });

        let (steering_tx, _steering_rx) = mpsc::channel();
        let existing = AgentSlot { receiver: mpsc::channel().1, cancel: CancelToken::new(), steering: steering_tx };
        let mut agent = Some(existing);
        maybe_spawn_agent(&mut app, &mut agent);

        assert!(
            !app.compaction_in_flight(),
            "in-flight requests must never be interrupted for compaction"
        );
        assert!(agent.is_some(), "the existing agent slot must be preserved");
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
    #[ignore = "this test is flaky and should either be fixed or run on its own"]
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
}
