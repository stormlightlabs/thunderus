//! `thndrs setup` command definitions.

use std::io::{self, IsTerminal, Write};

use clap::{Args, ValueEnum};

use super::auth::{
    CredentialScope, confirm, credential_path, prompt_scope, read_hidden_api_key, run_chatgpt_codex_login,
    validate_provider_key,
};
use crate::cli::Cli;
use crate::config;
use crate::thndrs_core::auth;
use thndrs_context::context;

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
                setup_summary: "OpenCode Zen Big Pickle is the default model. It requires OPENCODE_ZEN_KEY; OpenCode describes Big Pickle as free for a limited time, and free-period prompts may be used to improve the model.",
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
    run_with_writer(
        cli,
        command,
        io::stdin().is_terminal(),
        &mut writer,
        run_chatgpt_codex_login,
    )
}

fn run_with_writer<W, F>(
    cli: &Cli, command: &SetupCommand, interactive: bool, writer: &mut W, chatgpt_login: F,
) -> io::Result<()>
where
    W: Write,
    F: FnOnce(&mut W) -> io::Result<()>,
{
    let workspace = context::discover_workspace_root(&cli.cwd);
    let inferred_provider = provider_for_model(&cli.model);
    let provider = match command.provider {
        Some(provider) => provider,
        None if interactive => prompt_provider(writer, inferred_provider)?,
        None => inferred_provider,
    };
    let auth_status = auth_status(provider, &workspace);
    let scope = command_scope(command);

    writeln!(writer, "workspace: {}", workspace.display())?;
    writeln!(writer, "model: {}", provider.default_model())?;
    writeln!(writer, "provider: {}", provider.label())?;
    writeln!(writer, "auth: {auth_status}")?;
    writeln!(writer, "setup: {}", provider.metadata().setup_summary)?;

    if let ProviderAuthKind::ChatGptOAuth { .. } = provider.metadata().auth_kind {
        maybe_write_oauth_model_config(&workspace, provider, scope, interactive, writer)?;
        return run_chatgpt_setup(provider, auth_status.as_str(), interactive, writer, chatgpt_login);
    }

    let env_var = provider
        .api_key_env_var()
        .expect("API-key setup branch only handles API-key providers");
    let credential_source = auth::credential_source(env_var, &workspace);
    if scope.is_none() && credential_source.is_none() && !interactive {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "setup needs an interactive terminal to choose a credential store and read a hidden API key; pass provider env vars for non-interactive use",
        ));
    }
    let scope = match scope {
        Some(scope) => scope,
        None if interactive => prompt_scope(writer)?,
        None => CredentialScope::Project,
    };

    maybe_write_model_config(&workspace, provider, scope, interactive, true, writer)?;

    if credential_source.is_none() {
        if !interactive {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is missing; run `thndrs login {}` interactively",
                    env_var,
                    provider.label()
                ),
            ));
        }
        if confirm(writer, &format!("Enter {} API key now?", provider.label()))? {
            let api_key = read_hidden_api_key(writer, provider)?;
            let api_key = api_key.trim();
            if api_key.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "API key cannot be empty"));
            }
            if confirm(
                writer,
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

fn prompt_provider<W: Write>(writer: &mut W, default_provider: SetupProviderArg) -> io::Result<SetupProviderArg> {
    write_provider_choices(writer, default_provider)?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() {
        return Ok(default_provider);
    }
    match answer {
        "1" | "opencode-zen" => Ok(SetupProviderArg::OpencodeZen),
        "2" | "chatgpt-codex" => Ok(SetupProviderArg::ChatgptCodex),
        "3" | "umans" => Ok(SetupProviderArg::Umans),
        "4" | "opencode-go" => Ok(SetupProviderArg::OpencodeGo),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected provider number or provider name",
        )),
    }
}

fn write_provider_choices<W: Write>(writer: &mut W, default_provider: SetupProviderArg) -> io::Result<()> {
    writeln!(writer, "Choose provider:")?;
    for (index, provider) in SetupProviderArg::ALL.iter().enumerate() {
        let default = if *provider == default_provider { " (default)" } else { "" };
        writeln!(
            writer,
            "  {}) {}{} [{}] - {}",
            index + 1,
            provider.label(),
            default,
            provider.default_model(),
            provider.metadata().setup_summary
        )?;
    }
    write!(writer, "Provider [{}]: ", default_provider.label())?;
    writer.flush()
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

fn maybe_write_oauth_model_config<W: Write>(
    workspace: &std::path::Path, provider: SetupProviderArg, scope: Option<CredentialScope>, interactive: bool,
    writer: &mut W,
) -> io::Result<()> {
    if let Some(scope) = scope {
        maybe_write_model_config(workspace, provider, scope, interactive, true, writer)?;
    } else if interactive && confirm(writer, "Write default model to a config file?")? {
        let scope = prompt_config_scope(writer)?;
        maybe_write_model_config(workspace, provider, scope, interactive, false, writer)?;
    }
    Ok(())
}

fn prompt_config_scope<W: Write>(writer: &mut W) -> io::Result<CredentialScope> {
    writeln!(writer, "Choose config scope:")?;
    writeln!(writer, "  1) global (~/.thndrs/config.toml)")?;
    writeln!(writer, "  2) project (.thndrs/config.toml)")?;
    write!(writer, "Config [global/project]: ")?;
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

fn run_chatgpt_setup<W, F>(
    provider: SetupProviderArg, auth_status: &str, interactive: bool, writer: &mut W, chatgpt_login: F,
) -> io::Result<()>
where
    W: Write,
    F: FnOnce(&mut W) -> io::Result<()>,
{
    if auth_status == "environment override" {
        if interactive && confirm(writer, "Create or update stored ChatGPT OAuth credentials now?")? {
            chatgpt_login(writer)?;
            writeln!(writer, "next: thndrs")?;
            return Ok(());
        }
        writeln!(
            writer,
            "next: run `thndrs login {}` interactively to create or update stored OAuth credentials",
            provider.label()
        )?;
        return Ok(());
    }
    if auth_status == "~/.thndrs/auth.json" {
        writeln!(writer, "next: thndrs")?;
        return Ok(());
    }
    if !interactive {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} setup uses interactive ChatGPT OAuth, not an API key; run `thndrs setup --provider {}` or `thndrs login {}` in a terminal",
                provider.label(),
                provider.label(),
                provider.label()
            ),
        ));
    }
    chatgpt_login(writer)?;
    writeln!(writer, "next: thndrs")?;
    Ok(())
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
    workspace: &std::path::Path, provider: SetupProviderArg, scope: CredentialScope, interactive: bool, ask: bool,
    writer: &mut W,
) -> io::Result<()> {
    let path = match scope {
        CredentialScope::Global => config::global_config_path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not available"))?,
        CredentialScope::Project => config::project_config_path(workspace),
    };

    if config::model_config_has_model(&path)? {
        return Ok(());
    }
    if !interactive {
        return Ok(());
    }
    if ask && !confirm(writer, &format!("Write default model to {} config?", scope.label()))? {
        return Ok(());
    }
    config::write_model_config_if_missing(&path, provider.default_model())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn with_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", home);
        }
        let result = f();
        unsafe {
            if let Some(old_home) = old_home {
                std::env::set_var("HOME", old_home);
            } else {
                std::env::remove_var("HOME");
            }
        }
        result
    }

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
    fn provider_choice_copy_marks_opencode_zen_default_and_caveats() {
        let mut output = Vec::new();
        write_provider_choices(&mut output, SetupProviderArg::OpencodeZen).expect("choices");
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("opencode-zen (default)"));
        assert!(output.contains("opencode/big-pickle"));
        assert!(output.contains(auth::OPENCODE_ZEN_KEY_ENV));
        assert!(output.contains("free for a limited time"));
        assert!(output.contains("prompts may be used to improve the model"));
    }

    #[test]
    fn opencode_zen_noninteractive_summary_uses_provider_copy() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        auth::set_credential(
            &auth::project_credentials_path(&workspace),
            auth::OPENCODE_ZEN_KEY_ENV,
            "secret",
        )
        .expect("credential");
        let cli = Cli { cwd: workspace, ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::OpencodeZen), global: false, project: false };
        let mut output = Vec::new();

        with_home(&home, || {
            run_with_writer(&cli, &command, false, &mut output, |_| Ok(())).expect("setup");
        });
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("provider: opencode-zen"));
        assert!(output.contains("model: opencode/big-pickle"));
        assert!(output.contains("auth: project credentials"));
        assert!(output.contains("OpenCode Zen Big Pickle is the default model"));
    }

    #[test]
    fn chatgpt_setup_noninteractive_fails_without_api_key_prompt() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let cli = Cli { cwd: workspace, ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::ChatgptCodex), global: false, project: false };
        let mut output = Vec::new();

        let err = with_home(&home, || {
            run_with_writer(&cli, &command, false, &mut output, |_| Ok(())).expect_err("missing auth")
        });
        let output = String::from_utf8(output).expect("utf8");
        let err = err.to_string();

        assert!(output.contains("provider: chatgpt-codex"));
        assert!(output.contains("auth: missing OAuth credential"));
        assert!(err.contains("interactive ChatGPT OAuth"));
        assert!(!output.contains("Enter chatgpt-codex API key"));
        assert!(!err.contains("CHATGPT_CODEX_ACCESS_TOKEN is missing"));
    }

    #[test]
    fn chatgpt_setup_interactive_uses_oauth_runner_not_api_key_input() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(workspace.join(".thndrs")).expect("workspace config dir");
        fs::create_dir_all(&home).expect("home");
        fs::write(
            config::project_config_path(&workspace),
            "model = \"chatgpt-codex/gpt-5.5\"\n",
        )
        .expect("project config");
        let cli = Cli { cwd: workspace, ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::ChatgptCodex), global: false, project: true };
        let mut output = Vec::new();
        let mut called = false;

        with_home(&home, || {
            run_with_writer(&cli, &command, true, &mut output, |writer| {
                called = true;
                writeln!(writer, "fake ChatGPT OAuth login")
            })
            .expect("setup");
        });
        let output = String::from_utf8(output).expect("utf8");

        assert!(called);
        assert!(output.contains("fake ChatGPT OAuth login"));
        assert!(!output.contains("Enter chatgpt-codex API key"));
        assert!(!output.contains("Choose credential store"));
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
