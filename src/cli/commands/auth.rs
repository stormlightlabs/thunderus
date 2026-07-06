//! `thndrs login`, `thndrs logout`, and `thndrs auth` command definitions.

use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use crossterm::event::{Event, KeyCode, KeyEventKind, read};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use super::setup::{ProviderAuthKind, SetupProviderArg};
use crate::cli::Cli;
use crate::context;
use crate::thndrs_core::auth;

/// Store one provider credential.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct LoginCommand {
    /// Provider whose API key should be stored.
    pub provider: SetupProviderArg,
}

/// Remove one stored provider credential.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct LogoutCommand {
    /// Provider whose stored API key should be removed.
    pub provider: SetupProviderArg,
}

/// Authentication inspection commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum AuthCommand {
    /// Show provider credential sources without values.
    Status,
}

/// Scope for a stored credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialScope {
    /// Store in `~/.thndrs/credentials.env`.
    Global,
    /// Store in `<workspace>/.thndrs/credentials.env`.
    Project,
}

impl CredentialScope {
    /// Human-readable storage label.
    pub fn label(self) -> &'static str {
        match self {
            CredentialScope::Global => "global",
            CredentialScope::Project => "project",
        }
    }
}

/// Run `thndrs login`.
pub fn run_login(cli: &Cli, command: &LoginCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    require_interactive("login")?;

    if command.provider == SetupProviderArg::ChatgptCodex {
        return run_chatgpt_codex_login(&mut writer);
    }

    let workspace = context::discover_workspace_root(&cli.cwd);
    let env_var = command
        .provider
        .api_key_env_var()
        .expect("non-ChatGPT login providers use API keys");
    if matches!(
        auth::credential_source(env_var, &workspace),
        Some(auth::CredentialSource::Environment)
    ) {
        writeln!(
            writer,
            "{} is set in the environment and takes precedence over stored credentials.",
            env_var
        )?;
        if !confirm(&mut writer, "Store a credential anyway?")? {
            writeln!(writer, "login cancelled")?;
            return Ok(());
        }
    }

    let scope = prompt_scope(&mut writer)?;
    let api_key = read_hidden_api_key(&mut writer, command.provider)?;
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "API key cannot be empty"));
    }
    if !confirm(
        &mut writer,
        &format!(
            "Store {} credential in {} store?",
            command.provider.label(),
            scope.label()
        ),
    )? {
        writeln!(writer, "login cancelled")?;
        return Ok(());
    }

    let path = credential_path(scope, &workspace)?;
    auth::set_credential(&path, env_var, api_key).map_err(io::Error::other)?;
    if scope == CredentialScope::Project {
        auth::ensure_git_exclude(&workspace).map_err(io::Error::other)?;
    }
    writeln!(
        writer,
        "{} credential stored in {}",
        command.provider.label(),
        scope.label()
    )?;
    match validate_provider_key(command.provider, api_key) {
        Ok(()) => writeln!(writer, "validation: ok")?,
        Err(err) => writeln!(writer, "validation: stored but unverified ({err})")?,
    }
    Ok(())
}

/// Run `thndrs logout`.
pub fn run_logout(cli: &Cli, command: &LogoutCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    require_interactive("logout")?;

    if command.provider == SetupProviderArg::ChatgptCodex {
        if !confirm(
            &mut writer,
            "Remove ChatGPT Codex credentials from ~/.thndrs/auth.json?",
        )? {
            writeln!(writer, "logout cancelled")?;
            return Ok(());
        }
        auth::remove_chatgpt_codex_credentials().map_err(io::Error::other)?;
        writeln!(writer, "chatgpt-codex credential removed")?;
        if std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV).is_ok_and(|value| !value.is_empty()) {
            writeln!(
                writer,
                "{} is still set in the environment, so chatgpt-codex remains authenticated through the environment.",
                auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV
            )?;
        }
        return Ok(());
    }

    let workspace = context::discover_workspace_root(&cli.cwd);
    let scope = prompt_scope(&mut writer)?;
    if !confirm(
        &mut writer,
        &format!(
            "Remove {} credential from {} store?",
            command.provider.label(),
            scope.label()
        ),
    )? {
        writeln!(writer, "logout cancelled")?;
        return Ok(());
    }

    let path = credential_path(scope, &workspace)?;
    let env_var = command
        .provider
        .api_key_env_var()
        .expect("non-ChatGPT logout providers use API keys");
    auth::remove_credential(&path, env_var).map_err(io::Error::other)?;
    writeln!(
        writer,
        "{} credential removed from {}",
        command.provider.label(),
        scope.label()
    )?;
    if matches!(
        auth::credential_source(env_var, &workspace),
        Some(auth::CredentialSource::Environment)
    ) {
        writeln!(
            writer,
            "{} is still set in the environment, so {} remains authenticated through the environment.",
            env_var,
            command.provider.label()
        )?;
    }
    Ok(())
}

/// Run `thndrs auth`.
pub fn run_auth(cli: &Cli, command: &AuthCommand) -> io::Result<()> {
    match command {
        AuthCommand::Status => {
            let stdout = io::stdout();
            let mut writer = stdout.lock();
            let workspace = context::discover_workspace_root(&cli.cwd);
            write_auth_status(&workspace, &mut writer)
        }
    }
}

/// Write redacted provider credential status.
pub fn write_auth_status<W: Write>(workspace: &Path, writer: &mut W) -> io::Result<()> {
    for provider in SetupProviderArg::ALL {
        let source = match provider.metadata().auth_kind {
            ProviderAuthKind::ApiKey { env_var } => auth::credential_source(env_var, workspace)
                .map(|source| source.label().to_string())
                .unwrap_or_else(|| String::from("missing")),
            ProviderAuthKind::ChatGptOAuth { .. } => chatgpt_codex_status(),
        };
        writeln!(writer, "{}\t{}", provider.label(), source)?;
    }
    Ok(())
}

/// Resolve the credential file path for a storage scope.
pub fn credential_path(scope: CredentialScope, workspace: &Path) -> io::Result<PathBuf> {
    match scope {
        CredentialScope::Global => auth::global_credentials_path().map_err(io::Error::other),
        CredentialScope::Project => Ok(auth::project_credentials_path(workspace)),
    }
}

/// Validate a provider key without persisting provider payloads.
pub fn validate_provider_key(provider: SetupProviderArg, api_key: &str) -> Result<(), String> {
    match provider {
        SetupProviderArg::Umans => crate::providers::umans::validate_api_key(api_key),
        SetupProviderArg::OpencodeGo => crate::providers::opencode::validate_go_api_key(api_key),
        SetupProviderArg::OpencodeZen => crate::providers::opencode::validate_zen_api_key(api_key),
        SetupProviderArg::ChatgptCodex => Err("ChatGPT Codex uses OAuth login, not API-key validation".to_string()),
    }
}

/// Ask for yes/no confirmation.
pub fn confirm<W: Write>(writer: &mut W, prompt: &str) -> io::Result<bool> {
    write!(writer, "{prompt} [y/N]: ")?;
    writer.flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Prompt for the credential storage scope.
pub fn prompt_scope<W: Write>(writer: &mut W) -> io::Result<CredentialScope> {
    writeln!(writer, "Choose credential store:")?;
    writeln!(writer, "  1) global (~/.thndrs/credentials.env)")?;
    writeln!(writer, "  2) project (.thndrs/credentials.env)")?;
    write!(writer, "Store [global/project]: ")?;
    writer.flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    match answer.trim().to_ascii_lowercase().as_str() {
        "" | "g" | "global" | "1" => Ok(CredentialScope::Global),
        "p" | "project" | "2" => Ok(CredentialScope::Project),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected `global` or `project`",
        )),
    }
}

/// Read an API key from the terminal without echoing typed characters.
pub fn read_hidden_api_key<W: Write>(writer: &mut W, provider: SetupProviderArg) -> io::Result<String> {
    write!(writer, "Enter {} API key (input hidden): ", provider.label())?;
    writer.flush()?;
    enable_raw_mode()?;
    let result = read_hidden_line();
    let _ = disable_raw_mode();
    writeln!(writer)?;
    result
}

fn require_interactive(command: &str) -> io::Result<()> {
    if io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{command} requires an interactive terminal; use provider env vars for non-interactive setup"),
        ))
    }
}

fn read_hidden_line() -> io::Result<String> {
    let mut value = String::new();
    loop {
        match read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => return Ok(value),
                KeyCode::Esc => return Err(io::Error::new(io::ErrorKind::Interrupted, "API key entry cancelled")),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char(ch) => value.push(ch),
                _ => {}
            },
            _ => {}
        }
    }
}

/// Run the shared ChatGPT Codex OAuth login flow.
pub fn run_chatgpt_codex_login<W: Write>(writer: &mut W) -> io::Result<()> {
    writeln!(
        writer,
        "ChatGPT Codex login uses ChatGPT OAuth and stores credentials in ~/.thndrs/auth.json"
    )?;
    let credentials = match auth::request_chatgpt_codex_device_code() {
        Ok(code) => {
            writeln!(
                writer,
                "Open {} and enter code {}",
                code.verification_uri
                    .as_deref()
                    .unwrap_or("https://auth.openai.com/codex/device"),
                code.user_code
            )?;
            writer.flush()?;
            auth::poll_chatgpt_codex_device_code(&code).map_err(io::Error::other)?
        }
        Err(err) => {
            writeln!(
                writer,
                "device-code login unavailable ({err}); falling back to browser PKCE"
            )?;
            auth::login_chatgpt_codex_with_browser_pkce().map_err(io::Error::other)?
        }
    };
    auth::write_chatgpt_codex_credentials(&credentials).map_err(io::Error::other)?;
    writeln!(writer, "chatgpt-codex credential stored in global auth store")?;
    Ok(())
}

fn chatgpt_codex_status() -> String {
    if std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV).is_ok_and(|value| !value.is_empty()) {
        return "environment".to_string();
    }
    match auth::read_chatgpt_codex_credentials() {
        Ok(Some(_)) => "global auth".to_string(),
        Ok(None) => "missing".to_string(),
        Err(_) => "invalid auth store".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_status_does_not_print_values() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path();
        auth::set_credential(
            &auth::project_credentials_path(workspace),
            auth::UMANS_API_KEY_ENV,
            "sk-secret-value",
        )
        .expect("store credential");

        let mut output = Vec::new();
        write_auth_status(workspace, &mut output).expect("status");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("umans\tproject credentials"));
        assert!(output.contains("opencode-go\tmissing"));
        assert!(output.contains("opencode-zen\tmissing"));
        assert!(!output.contains("sk-secret-value"));
    }

    #[test]
    fn credential_scope_labels_are_stable() {
        assert_eq!(CredentialScope::Global.label(), "global");
        assert_eq!(CredentialScope::Project.label(), "project");
    }
}
