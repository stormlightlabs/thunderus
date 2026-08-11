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
    ("context", "inspect context lifecycle"),
    ("context verify", "review a verification relation"),
    ("context release", "explicitly release context protection"),
    ("doctor", "show context health"),
    ("history", "list recent sessions"),
    ("resume", "resume a local session"),
    ("name", "name the current session"),
    ("session", "show a local session summary"),
    ("status", "inspect runtime status and telemetry"),
    ("tokens", "show current session token totals"),
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
    if command == "tokens" {
        app.transcript
            .entries
            .push(Entry::Status { text: app.token_accounting_status() });
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
        "clear" => {
            app.transcript.entries.clear();
            app.composer.input.clear();
            app.composer.queued_steering.clear();
            app.composer.queued_followups.clear();
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
        || matches!(command, "history" | "tokens" | "debug log")
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

fn list_mcp_servers(app: &mut App) {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(&app.runtime.cwd, &env_vars) {
        Ok(effective) if effective.config.servers.is_empty() => {
            app.transcript
                .entries
                .push(Entry::Status { text: String::from("no MCP servers configured") });
        }
        Ok(effective) => {
            let mut lines = Vec::new();
            for (name, server) in &effective.config.servers {
                let status = if server.enabled { "enabled" } else { "disabled" };
                lines.push(format!("{name}\t{status}\t{:?}", server.transport));
            }
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
