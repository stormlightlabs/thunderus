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

/// API-key providers supported by first-run setup.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ApiKeyProviderArg {
    /// Umans Code provider.
    Umans,
    /// OpenCode Go provider.
    OpencodeGo,
    /// OpenCode Zen provider.
    OpencodeZen,
    /// ChatGPT subscription-backed Codex provider.
    ChatgptCodex,
}

impl ApiKeyProviderArg {
    /// Provider display label.
    pub fn label(self) -> &'static str {
        match self {
            ApiKeyProviderArg::Umans => "umans",
            ApiKeyProviderArg::OpencodeGo => "opencode-go",
            ApiKeyProviderArg::OpencodeZen => "opencode-zen",
            ApiKeyProviderArg::ChatgptCodex => "chatgpt-codex",
        }
    }

    /// Required API-key environment variable for this provider.
    pub fn env_var(self) -> &'static str {
        match self {
            ApiKeyProviderArg::Umans => auth::UMANS_API_KEY_ENV,
            ApiKeyProviderArg::OpencodeGo => auth::OPENCODE_GO_KEY_ENV,
            ApiKeyProviderArg::OpencodeZen => auth::OPENCODE_ZEN_KEY_ENV,
            ApiKeyProviderArg::ChatgptCodex => auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV,
        }
    }

    /// Default model used when setup writes a new config model key.
    pub fn default_model(self) -> &'static str {
        match self {
            ApiKeyProviderArg::Umans => "umans-coder",
            ApiKeyProviderArg::OpencodeGo => "opencode-go/kimi-k2.7-code",
            ApiKeyProviderArg::OpencodeZen => "opencode/big-pickle",
            ApiKeyProviderArg::ChatgptCodex => "chatgpt-codex/gpt-5.5",
        }
    }
}

/// First-run setup options.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct SetupCommand {
    /// Provider to configure.
    #[arg(long, value_enum)]
    pub provider: Option<ApiKeyProviderArg>,
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
    let credential_source = auth::credential_source(provider.env_var(), &workspace);
    let scope = command_scope(command);

    writeln!(writer, "workspace: {}", workspace.display())?;
    writeln!(writer, "provider: {}", provider.label())?;
    writeln!(
        writer,
        "credential: {}",
        credential_source
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| String::from("missing"))
    )?;

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
                    provider.env_var(),
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
                auth::set_credential(&path, provider.env_var(), api_key).map_err(io::Error::other)?;
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

fn provider_for_model(model: &str) -> ApiKeyProviderArg {
    if crate::providers::opencode::is_zen_model_id(model) {
        ApiKeyProviderArg::OpencodeZen
    } else if crate::providers::opencode::is_go_model_id(model) {
        ApiKeyProviderArg::OpencodeGo
    } else if crate::providers::chatgpt_codex::is_model_id(model) {
        ApiKeyProviderArg::ChatgptCodex
    } else {
        ApiKeyProviderArg::Umans
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
    workspace: &std::path::Path, provider: ApiKeyProviderArg, scope: CredentialScope, writer: &mut W,
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
        assert_eq!(provider_for_model("umans-coder"), ApiKeyProviderArg::Umans);
        assert_eq!(
            provider_for_model("opencode/big-pickle"),
            ApiKeyProviderArg::OpencodeZen
        );
        assert_eq!(
            provider_for_model("opencode-go/kimi-k2.7-code"),
            ApiKeyProviderArg::OpencodeGo
        );
        assert_eq!(
            provider_for_model("chatgpt-codex/gpt-5.5"),
            ApiKeyProviderArg::ChatgptCodex
        );
    }

    #[test]
    fn opencode_zen_is_the_setup_default_model_for_big_pickle() {
        assert_eq!(ApiKeyProviderArg::OpencodeZen.default_model(), "opencode/big-pickle");
        assert_eq!(ApiKeyProviderArg::OpencodeZen.env_var(), auth::OPENCODE_ZEN_KEY_ENV);
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
