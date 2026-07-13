//! Slash-command routing and command output projection.
//!
//! This module handles text entered after `/` or `:`. It dispatches session,
//! context, setup/auth, model, skill, MCP, background-process, and quit
//! commands, and appends their redacted status or error output to the
//! transcript. Commands that need another agent turn return a [`Msg`].

use super::*;

/// Route a slash command (the part after `/` or the text after `:`).
pub fn handle_command(app: &mut App, command: &str) -> Option<Msg> {
    if command_contains_api_key_like_argument(command) {
        app.transcript.push(Entry::Error {
            text: String::from("slash commands do not accept API keys as arguments; use /login <provider>"),
        });
        app.input.clear();
        return None;
    }

    if command == "history" {
        return run_history_command(app);
    }
    if command == "tokens" {
        app.transcript.push(Entry::Status {
            text: format!("tokens: in {} out {}", app.session_tokens_in, app.session_tokens_out),
        });
        app.input.clear();
        return None;
    }
    if let Some(session_id) = command.strip_prefix("resume ") {
        return resume_session_command(app, session_id.trim());
    }
    if command == "resume" {
        app.transcript
            .push(Entry::Error { text: String::from("usage: /resume <session-id>") });
        return None;
    }
    if let Some(session_id) = command.strip_prefix("session ") {
        return show_session_command(app, session_id.trim());
    }
    if command == "session" {
        app.transcript
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
        return handle_context_command(app, rest.trim());
    }
    if let Some((action, rest)) = command.split_once(' ')
        && matches!(action, "pin" | "drop" | "recover")
    {
        return handle_context_command(app, &format!("{action} {rest}"));
    }
    if matches!(command, "pin" | "drop" | "recover") {
        return handle_context_command(app, command);
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
        app.input.clear();
        match parse_api_key_provider(rest.trim()) {
            Some(provider) => {
                app.first_run_recovery = Some(FirstRunRecovery::login(provider));
            }
            None => app.transcript.push(Entry::Error {
                text: String::from("usage: /login <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            }),
        }
        return None;
    }
    if let Some(rest) = command.strip_prefix("logout ") {
        app.input.clear();
        match parse_api_key_provider(rest.trim()) {
            Some(SetupProviderArg::ChatgptCodex) => {
                app.transcript.push(Entry::Status {
                    text: String::from(
                        "ChatGPT Codex logout is CLI-only; run `thndrs logout chatgpt-codex` outside the TUI",
                    ),
                });
            }
            Some(provider) => {
                app.first_run_recovery = Some(FirstRunRecovery::logout(provider));
            }
            None => app.transcript.push(Entry::Error {
                text: String::from("usage: /logout <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            }),
        }
        return None;
    }

    match command {
        "compact" => start_manual_compaction(app),
        "clear" => {
            app.transcript.clear();
            app.input.clear();
            app.queued_steering.clear();
            app.queued_followups.clear();
            Some(Msg::Clear)
        }
        "quit" | "exit" => {
            app.input.clear();
            app.quit = true;
            Some(Msg::Quit)
        }
        "help" => {
            app.prompt_accessory = PromptAccessory::Help;
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
            run_doctor_slash(app);
            app.input.clear();
            None
        }
        "auth status" => {
            run_auth_status_slash(app);
            app.input.clear();
            None
        }
        "config path" => {
            run_config_slash(app, &crate::cli::commands::config::ConfigCommand::Path);
            app.input.clear();
            None
        }
        "config show" => {
            run_config_slash(
                app,
                &crate::cli::commands::config::ConfigCommand::Show(crate::cli::commands::config::ConfigShowCommand {
                    redacted: true,
                }),
            );
            app.input.clear();
            None
        }
        "config edit" => {
            app.transcript.push(Entry::Status {
                text: String::from(
                    "config edit is CLI-only; run `thndrs config edit --global` or `thndrs config edit --project` outside the TUI",
                ),
            });
            app.input.clear();
            None
        }
        "setup" => {
            let provider = provider_for_model(&app.model);
            app.first_run_recovery = Some(FirstRunRecovery::setup(provider));
            app.input.clear();
            None
        }
        "login" => {
            app.transcript.push(Entry::Error {
                text: String::from("usage: /login <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            });
            app.input.clear();
            None
        }
        "logout" => {
            app.transcript.push(Entry::Error {
                text: String::from("usage: /logout <umans|opencode-go|opencode-zen|chatgpt-codex>"),
            });
            app.input.clear();
            None
        }
        _ => None,
    }
}
pub fn command_suggestions_for_app(app: &App) -> Vec<(&'static str, &'static str)> {
    let query = super::input::command_query(app);
    let commands = [
        ("clear", "clear transcript"),
        ("quit", "exit app"),
        ("exit", "exit app"),
        ("help", "show help"),
        ("bg", "list background processes"),
        ("model", "switch model"),
        ("reasoning", "set reasoning effort"),
        ("skills", "browse loaded skills"),
        ("doctor", "show context health"),
        ("history", "list recent sessions"),
        ("resume", "resume a local session"),
        ("session", "show a local session summary"),
        ("tokens", "show current session token totals"),
        ("debug log", "read the current session log"),
        ("auth status", "show credential sources"),
        ("config path", "show config paths"),
        ("config show", "show redacted config"),
        ("setup", "open setup"),
        ("login", "provider login"),
        ("logout", "remove provider credential"),
    ];
    commands
        .into_iter()
        .filter(|(cmd, _)| cmd.starts_with(&query))
        .collect()
}
fn parse_api_key_provider(input: &str) -> Option<SetupProviderArg> {
    match input {
        "umans" => Some(SetupProviderArg::Umans),
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
    if lower.starts_with("ctx_") {
        return false;
    }
    value.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("opencode_go_key=")
        || lower.contains("opencode_zen_key=")
        || lower.contains("umans_api_key=")
        || lower.contains("access_token=")
        || lower.contains("refresh_token=")
        || lower.contains("device_auth_id=")
        || lower.contains("device_code=")
        || (value.len() >= 32
            && value.chars().any(|ch| ch.is_ascii_digit())
            && value.chars().any(|ch| ch.is_ascii_alphabetic()))
}

fn run_doctor_slash(app: &mut App) {
    app.refresh_context_ledger(None);
    let Some(ledger) = app.context_ledger.as_ref() else {
        app.transcript
            .push(Entry::Error { text: "context health is unavailable".to_string() });
        return;
    };
    let counts = ledger.counts();
    let review = app
        .last_compaction_review
        .map(compaction_review_label)
        .unwrap_or("none");
    app.transcript.push(Entry::Status {
        text: format!(
            "thndrs doctor (context health)\nsources: {} ({} discovery diagnostics)\npins: {} dropped: {}\nbudget: {} / {} used, {} available, {} auto threshold\nlimits: {} ({})\ncompaction: {} review {}",
            app.context_sources.len(),
            app.context_diagnostics.len(),
            counts.pinned,
            counts.dropped,
            ledger.budget.used,
            ledger.budget.target,
            ledger.budget.available_input,
            ledger.budget.auto_compaction_threshold,
            ledger.budget.limits.source.label(),
            ledger.budget.limits.confidence.label(),
            compaction_mode_label(app),
            review,
        ),
    });
}

fn handle_context_command(app: &mut App, command: &str) -> Option<Msg> {
    let Some((action, reference)) = command.split_once(' ') else {
        return match command {
            "show" => {
                app.open_context_surface();
                None
            }
            "drop --reset" => {
                match app.reset_context_drops() {
                    Ok(()) => app.input.clear(),
                    Err(error) => app.transcript.push(Entry::Error { text: error }),
                }
                None
            }
            "review" => {
                let state = app
                    .last_compaction_review
                    .map(compaction_review_label)
                    .unwrap_or("none");
                app.transcript
                    .push(Entry::Status { text: format!("compaction review: {state}") });
                app.input.clear();
                None
            }
            "pin" | "drop" | "recover" => {
                app.transcript
                    .push(Entry::Error { text: format!("usage: /context {command} <id-or-path>") });
                None
            }
            _ => {
                app.transcript
                    .push(Entry::Error { text: "usage: /context [show|pin|drop|recover|review]".to_string() });
                None
            }
        };
    };

    let result = match action {
        "pin" => app.pin_context_reference(reference.trim()),
        "drop" if reference.trim() == "--reset" => app.reset_context_drops(),
        "drop" => app.drop_context_reference(reference.trim()),
        "recover" => app.recover_context_reference(reference.trim()),
        "review" => return handle_context_review(app, reference.trim()),
        _ => Err("usage: /context [show|pin|drop|recover|review]".to_string()),
    };
    match result {
        Ok(()) => {
            app.input.clear();
            if matches!(action, "pin" | "drop" | "recover") {
                app.prompt_accessory = PromptAccessory::Context;
            }
        }
        Err(error) => app.transcript.push(Entry::Error { text: error }),
    }
    None
}

fn handle_context_review(app: &mut App, action: &str) -> Option<Msg> {
    let Some(pending) = app.pending_compaction_review.as_ref() else {
        app.transcript
            .push(Entry::Error { text: "no compaction summary is awaiting review".to_string() });
        return None;
    };
    let review = match action {
        "approve" => session::CompactionReviewResult::Approved,
        "reject" => session::CompactionReviewResult::Rejected,
        _ => {
            app.transcript
                .push(Entry::Error { text: "usage: /context review <approve|reject>".to_string() });
            return None;
        }
    };
    if let Some(writer) = app.session_writer.as_mut()
        && let Err(error) = writer.append_compaction_review(&pending.pending.recovery_handle, review)
    {
        app.transcript
            .push(Entry::Error { text: format!("failed to record compaction review: {error}") });
        return None;
    }
    let pending = app
        .pending_compaction_review
        .take()
        .expect("review state checked above");
    app.last_compaction_review = Some(review);
    app.input.clear();
    if review == session::CompactionReviewResult::Rejected {
        let recovery_handle = pending.pending.recovery_handle.clone();
        let original_user_turn = pending.pending.original_user_turn.clone();
        restore_failed_compaction(app, pending.pending);
        if let Some(turn) = original_user_turn {
            app.input.set_text(&turn);
        }
        app.transcript
            .push(Entry::Status { text: format!("compaction rejected  {recovery_handle}") });
        return None;
    }
    apply_compaction(app, pending.pending, pending.summary).flatten()
}

fn run_auth_status_slash(app: &mut App) {
    let mut output = Vec::new();
    let result = crate::cli::commands::auth::write_auth_status(&app.cwd, &mut output);
    push_command_output(app, "auth status", &output, result);
}

fn run_config_slash(app: &mut App, command: &crate::cli::commands::config::ConfigCommand) {
    let mut output = Vec::new();
    let result = crate::cli::commands::config::run_with_writer(&app.cli, command, &mut output);
    push_command_output(app, "config", &output, result);
}

fn push_command_output(app: &mut App, label: &str, output: &[u8], result: std::io::Result<()>) {
    let text = String::from_utf8_lossy(output).trim_end().to_string();
    if !text.is_empty() {
        app.transcript.push(Entry::Status { text });
    }
    if let Err(err) = result {
        app.transcript
            .push(Entry::Error { text: format!("{label} exited with {}: {err}", err.kind()) });
    }
}

/// Handle a slash command submitted while the agent is working.
///
/// Safe commands (`quit`, `exit`, `help`, `bg`) execute immediately. Commands
/// that mutate idle-only UI state are rejected instead of being queued as text.
/// Prefix with `//` to queue a literal slash-prefixed follow-up.
pub fn handle_running_command(app: &mut App, command: &str) -> Option<Msg> {
    let is_read_only = matches!(command, "quit" | "exit" | "help" | "bg")
        || matches!(command, "history" | "tokens" | "debug log")
        || matches!(command, "context" | "context show" | "doctor")
        || command.starts_with("session ")
        || command.starts_with("debug log ");
    if is_read_only {
        return handle_command(app, command);
    }
    app.transcript.push(Entry::Status {
        text: format!("/{command} is not available while the agent is working; use //{command} to queue it as text"),
    });
    None
}
