//! `thndrs setup` command definitions.

use std::fs;
use std::io::{self, IsTerminal, Write};

use clap::{Args, ValueEnum};

use super::auth::{
    CredentialScope, confirm, credential_path, prompt_scope, read_hidden_api_key, validate_provider_key,
};
use crate::cli::Cli;
use crate::thndrs_core::auth;
use crate::{config, context};

/// Providers supported by first-run setup and login commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SetupProviderArg {
    /// Umans Code provider.
    Umans,
    /// OpenCode Go provider.
    OpencodeGo,
    /// OpenCode Zen provider.
    OpencodeZen,
    /// ChatGPT subscription-backed Codex provider.
    ChatgptCodex,
}

/// Authentication mechanism used by a setup provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAuthKind {
    /// Provider uses a single API key stored in a credential env file.
    ApiKey { env_var: &'static str },
    /// Provider uses ChatGPT OAuth credentials stored in `~/.thndrs/auth.json`.
    ChatGptOAuth { env_override: &'static str },
}

/// Static metadata used by setup, login, and auth status commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderMetadata {
    /// Public provider argument value.
    pub label: &'static str,
    /// Default model written by setup when requested.
    pub default_model: &'static str,
    /// Provider auth storage behavior.
    pub auth_kind: ProviderAuthKind,
    /// Short setup summary copy.
    pub setup_summary: &'static str,
}

impl SetupProviderArg {
    /// All built-in setup providers in default prompt order.
    pub const ALL: [Self; 4] = [Self::OpencodeZen, Self::ChatgptCodex, Self::Umans, Self::OpencodeGo];

    /// Provider metadata.
    pub const fn metadata(self) -> ProviderMetadata {
        match self {
            Self::Umans => ProviderMetadata {
                label: "umans",
                default_model: "umans-coder",
                auth_kind: ProviderAuthKind::ApiKey { env_var: auth::UMANS_API_KEY_ENV },
                setup_summary: "Umans Code uses an API key stored in a thndrs credential store.",
            },
            Self::OpencodeGo => ProviderMetadata {
                label: "opencode-go",
                default_model: "opencode-go/kimi-k2.7-code",
                auth_kind: ProviderAuthKind::ApiKey { env_var: auth::OPENCODE_GO_KEY_ENV },
                setup_summary: "OpenCode Go uses OPENCODE_GO_KEY stored in a thndrs credential store.",
            },
            Self::OpencodeZen => ProviderMetadata {
                label: "opencode-zen",
                default_model: "opencode/big-pickle",
                auth_kind: ProviderAuthKind::ApiKey { env_var: auth::OPENCODE_ZEN_KEY_ENV },
                setup_summary: "OpenCode Zen Big Pickle is the default model; it requires OPENCODE_ZEN_KEY and is free for a limited time.",
            },
            Self::ChatgptCodex => ProviderMetadata {
                label: "chatgpt-codex",
                default_model: "chatgpt-codex/gpt-5.5",
                auth_kind: ProviderAuthKind::ChatGptOAuth { env_override: auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV },
                setup_summary: "ChatGPT Codex uses ChatGPT OAuth stored in ~/.thndrs/auth.json.",
            },
        }
    }

    /// Provider display label.
    pub fn label(self) -> &'static str {
        self.metadata().label
    }

    /// Required API-key environment variable for API-key providers.
    pub fn api_key_env_var(self) -> Option<&'static str> {
        match self.metadata().auth_kind {
            ProviderAuthKind::ApiKey { env_var } => Some(env_var),
            ProviderAuthKind::ChatGptOAuth { .. } => None,
        }
    }

    /// Default model used when setup writes a new config model key.
    pub fn default_model(self) -> &'static str {
        self.metadata().default_model
    }
}

/// First-run setup options.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct SetupCommand {
    /// Provider to configure.
    #[arg(long, value_enum)]
    pub provider: Option<SetupProviderArg>,
    /// Write global setup files under the user's home directory.
    #[arg(long, conflicts_with = "project")]
    pub global: bool,
    /// Write project setup files under the current workspace.
    #[arg(long, conflicts_with = "global")]
    pub project: bool,
}

/// Run the first-run setup workflow.
pub fn run(cli: &Cli, command: &SetupCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let workspace = context::discover_workspace_root(&cli.cwd);
    let provider = command.provider.unwrap_or_else(|| provider_for_model(&cli.model));
    let auth_status = auth_status(provider, &workspace);
    let scope = command_scope(command);

    writeln!(writer, "workspace: {}", workspace.display())?;
    writeln!(writer, "model: {}", provider.default_model())?;
    writeln!(writer, "provider: {}", provider.label())?;
    writeln!(writer, "auth: {auth_status}")?;
    writeln!(writer, "setup: {}", provider.metadata().setup_summary)?;

    if let ProviderAuthKind::ChatGptOAuth { .. } = provider.metadata().auth_kind {
        return run_chatgpt_setup(provider, auth_status.as_str(), &mut writer);
    }

    let env_var = provider
        .api_key_env_var()
        .expect("API-key setup branch only handles API-key providers");
    let credential_source = auth::credential_source(env_var, &workspace);
    if scope.is_none() && credential_source.is_none() && !io::stdin().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "setup needs an interactive terminal to choose a credential store and read a hidden API key; pass provider env vars for non-interactive use",
        ));
    }
    let scope = match scope {
        Some(scope) => scope,
        None if io::stdin().is_terminal() => prompt_scope(&mut writer)?,
        None => CredentialScope::Project,
    };

    maybe_write_model_config(&workspace, provider, scope, &mut writer)?;

    if credential_source.is_none() {
        if !io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is missing; run `thndrs login {}` interactively",
                    env_var,
                    provider.label()
                ),
            ));
        }
        if confirm(&mut writer, &format!("Enter {} API key now?", provider.label()))? {
            let api_key = read_hidden_api_key(&mut writer, provider)?;
            let api_key = api_key.trim();
            if api_key.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "API key cannot be empty"));
            }
            if confirm(
                &mut writer,
                &format!("Store {} credential in {} store?", provider.label(), scope.label()),
            )? {
                let path = credential_path(scope, &workspace)?;
                auth::set_credential(&path, env_var, api_key).map_err(io::Error::other)?;
                if scope == CredentialScope::Project {
                    auth::ensure_git_exclude(&workspace).map_err(io::Error::other)?;
                }
                writeln!(writer, "{} credential stored in {}", provider.label(), scope.label())?;
                match validate_provider_key(provider, api_key) {
                    Ok(()) => writeln!(writer, "validation: ok")?,
                    Err(err) => writeln!(writer, "validation: stored but unverified ({err})")?,
                }
            }
        } else {
            writeln!(writer, "credential setup skipped")?;
        }
    }

    writeln!(writer, "next: thndrs")?;
    Ok(())
}

fn auth_status(provider: SetupProviderArg, workspace: &std::path::Path) -> String {
    match provider.metadata().auth_kind {
        ProviderAuthKind::ApiKey { env_var } => auth::credential_source(env_var, workspace)
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        ProviderAuthKind::ChatGptOAuth { env_override } => {
            if std::env::var(env_override).is_ok_and(|value| !value.trim().is_empty()) {
                "environment override".to_string()
            } else {
                match auth::read_chatgpt_codex_credentials() {
                    Ok(Some(_)) => "~/.thndrs/auth.json".to_string(),
                    Ok(None) => "missing OAuth credential".to_string(),
                    Err(_) => "invalid auth store".to_string(),
                }
            }
        }
    }
}

fn run_chatgpt_setup<W: Write>(provider: SetupProviderArg, auth_status: &str, writer: &mut W) -> io::Result<()> {
    if auth_status == "environment override" {
        writeln!(
            writer,
            "next: run `thndrs login {}` interactively to create or update stored OAuth credentials",
            provider.label()
        )?;
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "{} setup uses ChatGPT OAuth, not an API key; run `thndrs login {}` interactively",
            provider.label(),
            provider.label()
        ),
    ))
}

fn provider_for_model(model: &str) -> SetupProviderArg {
    if crate::providers::opencode::is_zen_model_id(model) {
        SetupProviderArg::OpencodeZen
    } else if crate::providers::opencode::is_go_model_id(model) {
        SetupProviderArg::OpencodeGo
    } else if crate::providers::chatgpt_codex::is_model_id(model) {
        SetupProviderArg::ChatgptCodex
    } else {
        SetupProviderArg::Umans
    }
}

fn command_scope(command: &SetupCommand) -> Option<CredentialScope> {
    if command.global {
        Some(CredentialScope::Global)
    } else if command.project {
        Some(CredentialScope::Project)
    } else {
        None
    }
}

fn maybe_write_model_config<W: Write>(
    workspace: &std::path::Path, provider: SetupProviderArg, scope: CredentialScope, writer: &mut W,
) -> io::Result<()> {
    let path = match scope {
        CredentialScope::Global => config::global_config_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not available"))?,
        CredentialScope::Project => config::project_config_path(workspace),
    };

    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|line| line.trim_start().starts_with("model")) {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Ok(());
    }
    if !confirm(writer, &format!("Write default model to {} config?", scope.label()))? {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(&format!("model = \"{}\"\n", provider.default_model()));
    fs::write(path, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_from_model() {
        assert_eq!(provider_for_model("umans-coder"), SetupProviderArg::Umans);
        assert_eq!(provider_for_model("opencode/big-pickle"), SetupProviderArg::OpencodeZen);
        assert_eq!(
            provider_for_model("opencode-go/kimi-k2.7-code"),
            SetupProviderArg::OpencodeGo
        );
        assert_eq!(
            provider_for_model("chatgpt-codex/gpt-5.5"),
            SetupProviderArg::ChatgptCodex
        );
    }

    #[test]
    fn opencode_zen_is_the_setup_default_model_for_big_pickle() {
        assert_eq!(SetupProviderArg::OpencodeZen.default_model(), "opencode/big-pickle");
        assert_eq!(
            SetupProviderArg::OpencodeZen.api_key_env_var(),
            Some(auth::OPENCODE_ZEN_KEY_ENV)
        );
    }

    #[test]
    fn provider_metadata_models_auth_kinds() {
        assert_eq!(SetupProviderArg::ALL[0], SetupProviderArg::OpencodeZen);
        assert_eq!(
            SetupProviderArg::Umans.metadata().auth_kind,
            ProviderAuthKind::ApiKey { env_var: auth::UMANS_API_KEY_ENV }
        );
        assert_eq!(
            SetupProviderArg::ChatgptCodex.metadata().auth_kind,
            ProviderAuthKind::ChatGptOAuth { env_override: auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV }
        );
        assert_eq!(SetupProviderArg::ChatgptCodex.api_key_env_var(), None);
    }

    #[test]
    fn setup_scope_uses_flags() {
        assert_eq!(
            command_scope(&SetupCommand { provider: None, global: true, project: false }),
            Some(CredentialScope::Global)
        );
        assert_eq!(
            command_scope(&SetupCommand { provider: None, global: false, project: true }),
            Some(CredentialScope::Project)
        );
        assert_eq!(
            command_scope(&SetupCommand { provider: None, global: false, project: false }),
            None
        );
    }
}
