//! Slash-command routing and command output projection.
//!
//! This module handles text entered after `/` or `:`.
//!
//! It dispatches session, context, setup/auth, model, skill, MCP,
//! background-process, and quit commands, and appends their redacted
//! status  or error output to the transcript.
//!
//! Commands that need another agent turn return a [`Msg`].

use super::*;
use crate::{cli::commands::config::ConfigCommand, mcp};

const COMMANDS: &[(&str, &str)] = &[
    ("clear", "clear transcript"),
    ("quit", "exit app"),
    ("exit", "exit app"),
    ("help", "show help"),
    ("bg", "list background processes"),
    ("model", "switch model"),
    ("reasoning", "set reasoning effort"),
    ("skills", "browse loaded skills"),
    ("mcp", "inspect configured MCP servers"),
    ("mcp trust", "review and trust project MCP configuration"),
    ("mcp revoke", "revoke project MCP trust"),
    ("context", "inspect context lifecycle"),
    ("context request", "inspect a provider request"),
    ("context verify", "review a verification relation"),
    ("context release", "explicitly release context protection"),
    ("doctor", "show context health"),
    ("history", "list recent sessions"),
    ("new", "start a new session"),
    ("resume", "resume a local session"),
    ("name", "name the current session"),
    ("session", "show a local session summary"),
    ("status", "inspect runtime status and telemetry"),
    ("usage", "show provider consumption and account capacity"),
    ("debug log", "read the current session log"),
    ("auth status", "show credential sources"),
    ("config path", "show config paths"),
    ("config show", "show redacted config"),
    ("setup", "open setup"),
    ("login", "provider login"),
    ("logout", "remove provider credential"),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSuggestion {
    /// Text inserted into command or slash mode.
    pub name: String,
    /// Description and optional argument hint shown beside the name.
    pub detail: String,
}

/// Route a slash command (the part after `/` or the text after `:`).
pub fn handle_command(app: &mut App, command: &str) -> Option<Msg> {
    if command_contains_api_key_like_argument(command) {
        app.transcript.entries.push(Entry::Error {
            text: String::from("slash commands do not accept API keys as arguments; use /login <provider>"),
        });
        app.composer.input.clear();
        return None;
    }

    if command == "history" {
        return run_history_command(app);
    }
    if command == "usage" {
        app.transcript.entries.push(Entry::Status { text: app.usage_status() });
        app.composer.input.clear();
        return None;
    }
    if command == "status" {
        app.transcript
            .entries
            .push(Entry::Status { text: app.runtime_status() });
        app.composer.input.clear();
        return None;
    }
    if let Some(session_id) = command.strip_prefix("resume ") {
        return resume_session_command(app, session_id.trim());
    }
    if command == "resume" {
        open_session_picker(app);
        return None;
    }
    if let Some(name) = command.strip_prefix("name ") {
        return rename_session_command(app, name);
    }
    if command == "name" {
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("usage: /name <session-name>") });
        return None;
    }
    if let Some(session_id) = command.strip_prefix("session ") {
        return show_session_command(app, session_id.trim());
    }
    if command == "session" {
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("usage: /session <session-id>") });
        return None;
    }
    if command == "debug log" {
        return read_session_log_command(app, None);
    }
    if let Some(session_id) = command.strip_prefix("debug log ") {
        return read_session_log_command(app, Some(session_id.trim()));
    }

    if command == "context" || command == "context show" {
        app.open_context_surface();
        return None;
    }
    if let Some(rest) = command.strip_prefix("context ") {
        return super::context::handle_context_command(app, rest.trim());
    }
    if let Some((action, rest)) = command.split_once(' ')
        && matches!(
            action,
            "pin" | "drop" | "recover" | "verify" | "verification" | "release"
        )
    {
        return super::context::handle_context_command(app, &format!("{action} {rest}"));
    }
    if matches!(
        command,
        "pin" | "drop" | "recover" | "verify" | "verification" | "release"
    ) {
        return super::context::handle_context_command(app, command);
    }

    if command == "mcp" {
        list_mcp_servers(app);
        return None;
    }
    if command == "mcp trust" {
        open_mcp_trust_surface(app);
        return None;
    }
    if command == "mcp revoke" {
        revoke_mcp_trust(app);
        return None;
    }
    if command == "mcp tools" {
        list_mcp_tools(app, "");
        return None;
    }
    if let Some(name) = command.strip_prefix("mcp tools ") {
        list_mcp_tools(app, name.trim());
        return None;
    }
    if let Some(rest) = command.strip_prefix("login ") {
        app.composer.input.clear();
        match rest.trim() {
            "umans" => app.transcript.entries.push(Entry::Error {
                text: crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE.to_string(),
            }),
            provider => match parse_api_key_provider(provider) {
                Some(provider) => {
                    app.overlay.show_setup(FirstRunRecovery::login(provider));
                }
                None => app.transcript.entries.push(Entry::Error {
                    text: String::from("usage: /login <opencode-go|opencode-zen|chatgpt-codex>"),
                }),
            },
        }
        return None;
    }
    if let Some(rest) = command.strip_prefix("logout ") {
        app.composer.input.clear();
        match rest.trim() {
            "umans" => app.transcript.entries.push(Entry::Error {
                text: crate::cli::commands::setup::UNSUPPORTED_PROVIDER_ROUTE_MESSAGE.to_string(),
            }),
            provider => match parse_api_key_provider(provider) {
                Some(SetupProviderArg::ChatgptCodex) => {
                    app.transcript.entries.push(Entry::Status {
                        text: String::from(
                            "ChatGPT Codex logout is CLI-only; run `thndrs logout chatgpt-codex` outside the TUI",
                        ),
                    });
                }
                Some(provider) => {
                    app.overlay.show_setup(FirstRunRecovery::logout(provider));
                }
                None => app.transcript.entries.push(Entry::Error {
                    text: String::from("usage: /logout <opencode-go|opencode-zen|chatgpt-codex>"),
                }),
            },
        }
        return None;
    }

    if let Some(rest) = command.strip_prefix("bg cancel") {
        return cancel_background_process(app, rest.trim());
    }

    match command {
        "compact" => super::context::start_compaction(app, session::CompactionTrigger::Manual, None),
        "new" => {
            app.start_new_session();
            None
        }
        "clear" => {
            app.transcript.entries.clear();
            app.composer.input.clear();
            app.composer.queue = QueueState::default();
            Some(Msg::Clear)
        }
        "quit" | "exit" => {
            app.composer.input.clear();
            app.runtime.quit = true;
            Some(Msg::Quit)
        }
        "help" => {
            app.overlay.show_help();
            None
        }
        "bg" => {
            list_background_processes(app);
            None
        }
        "model" => {
            open_model_picker(app);
            None
        }
        "reasoning" => {
            open_reasoning_effort_picker(app);
            None
        }
        "skills" => {
            open_skill_picker(app);
            None
        }
        "doctor" => {
            super::context::run_doctor_slash(app);
            app.composer.input.clear();
            None
        }
        "auth status" => {
            run_auth_status_slash(app);
            app.composer.input.clear();
            None
        }
        "config path" => {
            run_config_slash(app, &crate::cli::commands::config::ConfigCommand::Path);
            app.composer.input.clear();
            None
        }
        "config show" => {
            run_config_slash(
                app,
                &crate::cli::commands::config::ConfigCommand::Show(crate::cli::commands::config::ConfigShowCommand {
                    redacted: true,
                }),
            );
            app.composer.input.clear();
            None
        }
        "config edit" => {
            app.transcript.entries.push(Entry::Status {
                text: String::from(
                    "config edit is CLI-only; run `thndrs config edit --global` or `thndrs config edit --project` outside the TUI",
                ),
            });
            app.composer.input.clear();
            None
        }
        "setup" => {
            let provider = provider_for_model(&app.runtime.model);
            app.overlay.show_setup(FirstRunRecovery::setup(provider));
            app.composer.input.clear();
            None
        }
        "login" => {
            app.transcript
                .entries
                .push(Entry::Error { text: String::from("usage: /login <opencode-go|opencode-zen|chatgpt-codex>") });
            app.composer.input.clear();
            None
        }
        "logout" => {
            app.transcript
                .entries
                .push(Entry::Error { text: String::from("usage: /logout <opencode-go|opencode-zen|chatgpt-codex>") });
            app.composer.input.clear();
            None
        }
        _ => submit_prompt_template(app, command),
    }
}

pub fn command_suggestions_for_app(app: &App) -> Vec<CommandSuggestion> {
    let query = super::input::command_query(app);
    let mut suggestions = COMMANDS
        .iter()
        .filter(|(command, _)| command.starts_with(&query))
        .map(|(command, description)| CommandSuggestion {
            name: (*command).to_string(),
            detail: (*description).to_string(),
        })
        .collect::<Vec<_>>();
    suggestions.extend(
        app.transcript
            .prompt_templates
            .iter()
            .filter(|template| {
                template.name.starts_with(&query) && !COMMANDS.iter().any(|(command, _)| *command == template.name)
            })
            .map(|template| {
                let detail = template.argument_hint.as_ref().map_or_else(
                    || template.description.clone(),
                    |hint| format!("{hint} — {}", template.description),
                );
                CommandSuggestion { name: template.name.clone(), detail }
            }),
    );
    suggestions
}

/// Handle a slash command submitted while the agent is working.
///
/// Safe commands (`quit`, `exit`, `help`, `bg`) execute immediately.
///
/// Commands that mutate idle-only UI state are rejected instead of being queued as text.
///
/// Prefix with `//` to queue a literal slash-prefixed follow-up.
pub fn handle_running_command(app: &mut App, command: &str) -> Option<Msg> {
    if let Some(rendered) = render_prompt_template(app, command) {
        match rendered {
            Ok(prompt) => super::input::queue_running_input(app, &prompt),
            Err(error) => {
                app.transcript.entries.push(Entry::Error { text: error });
                app.composer.input.set_text(&format!("/{command}"));
            }
        }
        return None;
    }
    let is_read_only = matches!(command, "quit" | "exit" | "help" | "bg" | "bg cancel")
        || command.starts_with("bg cancel ")
        || matches!(command, "history" | "usage" | "debug log")
        || matches!(command, "context" | "context show" | "doctor")
        || command.starts_with("context export ")
        || command.starts_with("session ")
        || command.starts_with("debug log ");
    if is_read_only {
        return handle_command(app, command);
    }
    app.transcript.entries.push(Entry::Status {
        text: format!("/{command} is not available while the agent is working; use //{command} to queue it as text"),
    });
    None
}

fn submit_prompt_template(app: &mut App, command: &str) -> Option<Msg> {
    let rendered = render_prompt_template(app, command)?;
    match rendered {
        Ok(prompt) => super::input::submit_user_turn(app, prompt),
        Err(error) => {
            app.transcript.entries.push(Entry::Error { text: error });
            app.composer.input.set_text(&format!("/{command}"));
            None
        }
    }
}

fn render_prompt_template(app: &App, command: &str) -> Option<Result<String, String>> {
    let name_end = command.find(char::is_whitespace).unwrap_or(command.len());
    let name = &command[..name_end];
    let arguments = command[name_end..].trim_start();
    let template = app
        .transcript
        .prompt_templates
        .iter()
        .find(|template| template.name == name)?;
    Some(prompt::templates::render(template, arguments))
}

fn parse_api_key_provider(input: &str) -> Option<SetupProviderArg> {
    match input {
        "opencode-go" => Some(SetupProviderArg::OpencodeGo),
        "opencode-zen" => Some(SetupProviderArg::OpencodeZen),
        "chatgpt-codex" => Some(SetupProviderArg::ChatgptCodex),
        _ => None,
    }
}

fn command_contains_api_key_like_argument(command: &str) -> bool {
    let mut parts = command.split_whitespace();
    let Some(head) = parts.next() else {
        return false;
    };
    let skip = match head {
        "login" | "logout" => 1,
        _ => 0,
    };
    parts.skip(skip).any(is_api_key_like)
}

fn is_api_key_like(value: &str) -> bool {
    let value = value.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == '`' || ch == ',' || ch == ';');
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("ctx_") || lower.starts_with("rel_") {
        return false;
    }
    value.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("opencode_go_key=")
        || lower.contains("opencode_zen_key=")
        || lower.contains("access_token=")
        || lower.contains("refresh_token=")
        || lower.contains("device_auth_id=")
        || lower.contains("device_code=")
        || (value.len() >= 32
            && value.chars().any(|ch| ch.is_ascii_digit())
            && value.chars().any(|ch| ch.is_ascii_alphabetic()))
}

fn run_auth_status_slash(app: &mut App) {
    let mut output = Vec::new();
    let result = crate::cli::commands::auth::write_auth_status(&app.runtime.cwd, &mut output);
    push_command_output(app, "auth status", &output, result);
}

fn run_config_slash(app: &mut App, command: &ConfigCommand) {
    let mut output = Vec::new();
    let result = crate::cli::commands::config::run_with_writer(&app.runtime.cli, command, &mut output);
    push_command_output(app, "config", &output, result);
}

fn push_command_output(app: &mut App, label: &str, output: &[u8], result: std::io::Result<()>) {
    let text = String::from_utf8_lossy(output).trim_end().to_string();
    if !text.is_empty() {
        app.transcript.entries.push(Entry::Status { text });
    }
    if let Err(err) = result {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("{label} exited with {}: {err}", err.kind()) });
    }
}

fn run_history_command(app: &mut App) -> Option<Msg> {
    let dir = app.session_directory();
    let files = session::list_session_files(&dir);
    if files.is_empty() {
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("no sessions found") });
    } else {
        let rows = files
            .into_iter()
            .take(20)
            .map(|path| {
                let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("session");
                let summary = session::SessionReader::read_summary(&path);
                format!(
                    "{id}\t{}\t{}\tin {} out {}",
                    summary.title, summary.model, summary.input_tokens, summary.output_tokens
                )
            })
            .collect::<Vec<_>>();
        app.transcript
            .entries
            .push(Entry::Status { text: format!("sessions:\n{}", rows.join("\n")) });
    }
    app.composer.input.clear();
    None
}

fn show_session_command(app: &mut App, session_id: &str) -> Option<Msg> {
    let path = match session::resolve_session_file(&app.session_directory(), session_id) {
        Ok(path) => path,
        Err(error) => {
            app.transcript.entries.push(Entry::Error { text: error.to_string() });
            return None;
        }
    };
    let id = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(session_id);
    let summary = session::SessionReader::read_summary(&path);
    app.transcript.entries.push(Entry::Status {
        text: format!(
            "session: {id}\ntitle: {}\nmodel: {}\ntokens: in {} out {}\npath: {}",
            summary.title,
            summary.model,
            summary.input_tokens,
            summary.output_tokens,
            path.display()
        ),
    });
    app.composer.input.clear();
    None
}

fn resume_session_command(app: &mut App, session_id: &str) -> Option<Msg> {
    if let Err(error) = app.resume_session(session_id) {
        app.transcript.entries.push(Entry::Error { text: error.to_string() });
    }
    None
}

fn rename_session_command(app: &mut App, name: &str) -> Option<Msg> {
    if let Err(error) = app.rename_session(name) {
        app.transcript.entries.push(Entry::Error { text: error.to_string() });
    } else {
        app.composer.input.clear();
    }
    None
}

fn read_session_log_command(app: &mut App, requested_session_id: Option<&str>) -> Option<Msg> {
    let id = match requested_session_id {
        Some(query) => match session::resolve_session_file(&app.session_directory(), query) {
            Ok(path) => path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(query)
                .to_string(),
            Err(error) => {
                app.transcript.entries.push(Entry::Error { text: error.to_string() });
                return None;
            }
        },
        None => app.session.id.clone(),
    };
    let path = app
        .runtime
        .cwd
        .join(".thndrs")
        .join("logs")
        .join("sessions")
        .join(format!("thndrs-{id}.log"));
    let lines = session::read_redacted_log_tail(&path, 100);
    if lines.is_empty() {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("debug log `{}` is empty or missing", path.display()) });
        return None;
    }
    app.transcript
        .entries
        .push(Entry::Status { text: format!("debug log {id}:\n{}", lines.join("\n")) });
    app.composer.input.clear();
    None
}

/// List background processes in the transcript.
fn list_background_processes(app: &mut App) {
    super::agent_lifecycle::drain_background_processes(app);
    let bg_ids: Vec<u64> = app.runtime.process_registry.background_ids().collect();
    if bg_ids.is_empty() {
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("no background processes") });
    } else {
        let lines: Vec<String> = bg_ids
            .iter()
            .filter_map(|id| {
                app.runtime.process_registry.get(*id).map(|p| {
                    let elapsed = p.elapsed().as_secs();
                    let cmd = p.command.join(" ");
                    format!("[{id}] {cmd} cwd={} ({elapsed}s)", p.cwd.display())
                })
            })
            .collect();
        app.transcript
            .entries
            .push(Entry::Status { text: format!("background processes:\n{}", lines.join("\n")) });
    }
}

fn cancel_background_process(app: &mut App, id_text: &str) -> Option<Msg> {
    super::agent_lifecycle::drain_background_processes(app);
    if id_text.is_empty() {
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("usage: :bg cancel <id>") });
        return None;
    }
    let Ok(id) = id_text.parse::<u64>() else {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("invalid background process id: {id_text}") });
        return None;
    };
    if app.runtime.process_registry.cancel(id) {
        app.transcript
            .entries
            .push(Entry::Status { text: format!("cancellation requested for background process [{id}]") });
    } else {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("background process [{id}] is not running") });
    }
    None
}

fn open_mcp_trust_surface(app: &mut App) {
    let effective = match load_mcp_for_tui(app) {
        Ok(effective) => effective,
        Err(error) => {
            app.transcript.entries.push(Entry::Error { text: error });
            return;
        }
    };
    let Some(surface) = mcp_trust_surface(&effective, &app.runtime.cwd, McpTrustAction::Trust) else {
        app.transcript
            .entries
            .push(Entry::Error { text: "project MCP configuration `.thndrs/mcp.toml` not found".to_string() });
        return;
    };
    match effective.project_trust {
        Some(crate::trust::ProjectMcpTrust::Trusted) => app.transcript.entries.push(Entry::Status {
            text: "project MCP configuration is already trusted; use /mcp revoke to deactivate it".to_string(),
        }),
        Some(crate::trust::ProjectMcpTrust::Untrusted | crate::trust::ProjectMcpTrust::Stale { .. }) => {
            app.overlay.show_mcp_trust(surface);
        }
        None => app
            .transcript
            .entries
            .push(Entry::Error { text: "project MCP configuration `.thndrs/mcp.toml` not found".to_string() }),
    }
}

fn revoke_mcp_trust(app: &mut App) {
    let effective = match load_mcp_for_tui(app) {
        Ok(effective) => effective,
        Err(error) => {
            app.transcript.entries.push(Entry::Error { text: error });
            return;
        }
    };
    let Some(surface) = mcp_trust_surface(&effective, &app.runtime.cwd, McpTrustAction::Revoke) else {
        app.transcript
            .entries
            .push(Entry::Error { text: "project MCP configuration `.thndrs/mcp.toml` not found".to_string() });
        return;
    };
    match effective.project_trust {
        Some(crate::trust::ProjectMcpTrust::Trusted) if !surface.servers.is_empty() => {
            app.overlay.show_mcp_trust(surface);
        }
        Some(crate::trust::ProjectMcpTrust::Trusted) => finish_mcp_revoke(app),
        Some(crate::trust::ProjectMcpTrust::Untrusted | crate::trust::ProjectMcpTrust::Stale { .. }) => {
            finish_mcp_revoke(app);
        }
        None => app
            .transcript
            .entries
            .push(Entry::Error { text: "project MCP configuration `.thndrs/mcp.toml` not found".to_string() }),
    }
}

pub(super) fn handle_mcp_trust_action(app: &mut App, action: Action) -> Option<Msg> {
    match action {
        Action::SelectPrevious => {
            if let Some(surface) = app.overlay.mcp_trust_mut() {
                surface.selected = surface.selected.saturating_sub(1);
            }
        }
        Action::SelectNext => {
            if let Some(surface) = app.overlay.mcp_trust_mut() {
                surface.selected = (surface.selected + 1).min(1);
            }
        }
        Action::Cancel | Action::CloseOverlay => app.overlay.close(),
        Action::Confirm => {
            let Some(surface) = app.overlay.mcp_trust().cloned() else {
                return None;
            };
            app.overlay.close();
            if surface.selected == 0 {
                match surface.action {
                    McpTrustAction::Trust => finish_mcp_trust(app, &surface),
                    McpTrustAction::Revoke => finish_mcp_revoke(app),
                }
            }
        }
        _ => {}
    }
    None
}

fn finish_mcp_trust(app: &mut App, surface: &McpTrustSurface) {
    let current_hash = match mcp::config::project_mcp_config_hash(&app.runtime.cwd) {
        Ok(Some(hash)) => hash,
        Ok(None) => {
            app.transcript
                .entries
                .push(Entry::Error { text: "project MCP configuration was removed before approval".to_string() });
            return;
        }
        Err(error) => {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("could not re-read project MCP configuration: {error}") });
            return;
        }
    };
    if current_hash != surface.hash {
        app.transcript.entries.push(Entry::Error {
            text:
                "project MCP configuration changed since review and remains blocked; run /mcp trust to review it again"
                    .to_string(),
        });
        return;
    }
    if let Err(error) = crate::trust::trust_project_mcp(&app.runtime.cwd, &surface.hash) {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("could not trust project MCP configuration: {error}") });
        return;
    }
    match load_mcp_for_tui(app) {
        Ok(effective) => {
            let count = mcp::config::server_statuses(&effective)
                .into_iter()
                .filter(|server| server.source == crate::config::ConfigSource::ProjectFile)
                .count();
            app.transcript.entries.push(Entry::Status {
                text: format!(
                    "trusted project MCP configuration ({}) and reloaded {count} project server(s); servers remain stopped until needed",
                    short_hash(&surface.hash)
                ),
            });
        }
        Err(error) => app.transcript.entries.push(Entry::Error {
            text: format!(
                "trusted project MCP configuration ({}) but could not reload it: {error}",
                short_hash(&surface.hash)
            ),
        }),
    }
}

fn finish_mcp_revoke(app: &mut App) {
    match crate::trust::revoke_project_mcp_trust(&app.runtime.cwd) {
        Ok(true) => match load_mcp_for_tui(app) {
            Ok(effective) => {
                let blocked = mcp::config::server_statuses(&effective)
                    .into_iter()
                    .filter(|server| server.state == mcp::config::McpLifecycleState::BlockedByTrust)
                    .count();
                app.transcript.entries.push(Entry::Status {
                    text: format!("revoked project MCP trust; {blocked} project server(s) are blocked by trust"),
                });
            }
            Err(error) => app.transcript.entries.push(Entry::Error {
                text: format!("revoked project MCP trust but could not reload configuration: {error}"),
            }),
        },
        Ok(false) => app
            .transcript
            .entries
            .push(Entry::Status { text: "project MCP trust was not set".to_string() }),
        Err(error) => app
            .transcript
            .entries
            .push(Entry::Error { text: format!("could not revoke project MCP trust: {error}") }),
    }
}

fn load_mcp_for_tui(app: &App) -> Result<mcp::config::EffectiveMcpConfig, String> {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    mcp::config::load_effective_mcp(&app.runtime.cwd, &env_vars)
        .map_err(|error| format!("failed to load MCP config: {error}"))
}

fn mcp_trust_surface(
    effective: &mcp::config::EffectiveMcpConfig, workspace: &std::path::Path, action: McpTrustAction,
) -> Option<McpTrustSurface> {
    let layer = effective
        .layers
        .iter()
        .find(|layer| layer.source == crate::config::ConfigSource::ProjectFile)?;
    let hash = layer.hash.clone()?;
    let mut servers = if effective.blocked_project_servers.is_empty() {
        effective
            .config
            .servers
            .iter()
            .filter(|(name, _)| effective.server_sources.get(*name) == Some(&crate::config::ConfigSource::ProjectFile))
            .map(|(name, server)| McpTrustServer {
                name: name.clone(),
                transport: server.transport,
                replaces_global: effective.project_overrides_global.contains(name),
            })
            .collect::<Vec<_>>()
    } else {
        effective
            .blocked_project_servers
            .iter()
            .map(|(name, server)| McpTrustServer {
                name: name.clone(),
                transport: server.transport,
                replaces_global: server.overrides_global,
            })
            .collect()
    };
    servers.sort_by(|left, right| left.name.cmp(&right.name));
    Some(McpTrustSurface {
        action,
        workspace: workspace.display().to_string(),
        config_path: layer
            .display_path
            .clone()
            .unwrap_or_else(|| ".thndrs/mcp.toml".to_string()),
        hash,
        servers,
        selected: 1,
    })
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

fn list_mcp_servers(app: &mut App) {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(&app.runtime.cwd, &env_vars) {
        Ok(effective) if effective.config.servers.is_empty() && effective.blocked_project_servers.is_empty() => {
            app.transcript
                .entries
                .push(Entry::Status { text: String::from("no MCP servers configured") });
        }
        Ok(effective) => {
            let mut lines = mcp::config::server_statuses(&effective)
                .into_iter()
                .map(|server| {
                    let precedence = if server.overrides_global { "\twould override global" } else { "" };
                    format!(
                        "{}\t{}\t{:?}\tsource={}{}",
                        server.name,
                        server.state.label(),
                        server.transport,
                        server.source.as_str(),
                        precedence,
                    )
                })
                .collect::<Vec<_>>();
            lines.extend(
                effective
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| format!("diagnostic: {diagnostic}")),
            );
            app.transcript
                .entries
                .push(Entry::Status { text: format!("MCP servers:\n{}", lines.join("\n")) });
        }
        Err(err) => app
            .transcript
            .entries
            .push(Entry::Error { text: format!("failed to load MCP config: {err}") }),
    }
}

fn list_mcp_tools(app: &mut App, name: &str) {
    if name.is_empty() {
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("usage: /mcp tools <name>") });
        return;
    }

    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    let effective = match mcp::config::load_effective_mcp(&app.runtime.cwd, &env_vars) {
        Ok(effective) => effective,
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("failed to load MCP config: {err}") });
            return;
        }
    };
    let Some(server) = effective.config.servers.get(name) else {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("MCP server `{name}` is not configured") });
        return;
    };
    if !server.enabled {
        app.transcript
            .entries
            .push(Entry::Error { text: format!("MCP server `{name}` is disabled") });
        return;
    }

    match mcp::manager::McpClient::connect(name.to_string(), server) {
        Ok(client) => {
            let lines: Vec<String> = client
                .tool_definitions()
                .into_iter()
                .map(|tool| format!("{}\t{}", tool.name, tool.description))
                .collect();
            app.transcript.entries.push(Entry::Status {
                text: if lines.is_empty() {
                    format!("MCP server `{name}` exposes no tools")
                } else {
                    format!("MCP tools for `{name}`:\n{}", lines.join("\n"))
                },
            });
        }
        Err(err) => app.transcript.entries.push(Entry::Error { text: err.to_string() }),
    }
}
