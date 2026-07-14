//! `thndrs setup` command definitions.

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use clap::{Args, ValueEnum};

use super::auth::{
    CredentialScope, ProviderCredentialHealth, check_chatgpt_codex_auth, check_provider_key, confirm, credential_path,
    credential_rejected_error, credential_rejected_error_for_source, prompt_scope, read_hidden_api_key,
    run_chatgpt_codex_login,
};
use crate::cli::Cli;
use crate::context;
use crate::thndrs_core::auth;
use crate::{config, providers};

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

impl SetupProviderArg {
    fn for_model(model: &str) -> Self {
        if providers::opencode::is_zen_model_id(model) {
            Self::OpencodeZen
        } else if providers::opencode::is_go_model_id(model) {
            Self::OpencodeGo
        } else if providers::chatgpt_codex::is_model_id(model) {
            Self::ChatgptCodex
        } else {
            Self::Umans
        }
    }
}

/// Authentication mechanism used by a setup provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAuthKind {
    /// Provider uses a single API key stored in a credential env file.
    ApiKey { env_var: &'static str },
    /// Provider uses ChatGPT OAuth credentials stored in `~/.thndrs/auth.json`.
    ChatGptOAuth { env_override: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChatGptSetupAuthStatus {
    EnvironmentOverride,
    StoredCredential,
    MissingCredential,
    InvalidStore,
}

impl ChatGptSetupAuthStatus {
    fn label(self) -> &'static str {
        match self {
            Self::EnvironmentOverride => "environment override",
            Self::StoredCredential => "~/.thndrs/auth.json",
            Self::MissingCredential => "missing OAuth credential",
            Self::InvalidStore => "invalid auth store",
        }
    }
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
                setup_summary: "Umans Code uses a UMANS_API_KEY from app.umans.ai, stored only in a thndrs credential store.",
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
                default_model: "chatgpt-codex/gpt-5.6-sol",
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

impl SetupCommand {
    fn scope(&self) -> Option<CredentialScope> {
        if self.global {
            Some(CredentialScope::Global)
        } else if self.project {
            Some(CredentialScope::Project)
        } else {
            None
        }
    }
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
        check_provider_key,
        check_chatgpt_codex_auth,
    )
}

fn run_with_writer<W, F, H, O>(
    cli: &Cli, command: &SetupCommand, interactive: bool, writer: &mut W, chatgpt_login: F, check_key: H,
    check_chatgpt_auth: O,
) -> io::Result<()>
where
    W: Write,
    F: FnOnce(&mut W) -> io::Result<()>,
    H: Fn(SetupProviderArg, &str) -> ProviderCredentialHealth,
    O: Fn(&Path) -> ProviderCredentialHealth,
{
    let workspace = context::discover_workspace_root(&cli.cwd);
    let inferred_provider = (!cli.model.trim().is_empty()).then(|| SetupProviderArg::for_model(&cli.model));
    let provider = match command.provider {
        Some(provider) => provider,
        None if interactive => prompt_provider(writer, inferred_provider)?,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "non-interactive setup needs --provider <chatgpt-codex|umans|opencode-zen|opencode-go> or --model <model-id>",
            ));
        }
    };
    let chatgpt_auth_status = (provider == SetupProviderArg::ChatgptCodex).then(chatgpt_setup_auth_status);
    let auth_status = chatgpt_auth_status
        .map(|status| status.label().to_string())
        .unwrap_or_else(|| auth_status(provider, &workspace));
    let scope = command.scope();

    writeln!(writer, "workspace: {}", workspace.display())?;
    writeln!(writer, "model: {}", provider.default_model())?;
    writeln!(writer, "provider: {}", provider.label())?;
    writeln!(writer, "auth: {auth_status}")?;
    writeln!(writer, "setup: {}", provider.metadata().setup_summary)?;

    if let Some(chatgpt_auth_status) = chatgpt_auth_status {
        run_chatgpt_setup(
            provider,
            chatgpt_auth_status,
            &workspace,
            interactive,
            writer,
            chatgpt_login,
            check_chatgpt_auth,
        )?;
        maybe_write_oauth_model_config(&workspace, provider, scope, interactive, writer)?;
        write_chatgpt_setup_next(provider, chatgpt_auth_status, writer)?;
        return Ok(());
    }

    let env_var = provider
        .api_key_env_var()
        .expect("API-key setup branch only handles API-key providers");
    let credential = auth::resolve_credential(env_var, &workspace);
    let credential_source = credential.as_ref().map(|(_, source)| *source);
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

    if let Some((api_key, source)) = credential {
        match check_key(provider, &api_key) {
            ProviderCredentialHealth::Verified => {}
            ProviderCredentialHealth::Rejected => return Err(credential_rejected_error_for_source(provider, source)),
            ProviderCredentialHealth::Unavailable => {
                return Err(verification_unavailable_error(provider));
            }
        }
    } else {
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
                let health = check_key(provider, api_key);
                if health == ProviderCredentialHealth::Rejected {
                    return Err(credential_rejected_error(provider));
                }
                let path = credential_path(scope, &workspace)?;
                auth::set_credential(&path, env_var, api_key).map_err(io::Error::other)?;
                if scope == CredentialScope::Project {
                    auth::ensure_git_exclude(&workspace).map_err(io::Error::other)?;
                }
                writeln!(writer, "{} credential stored in {}", provider.label(), scope.label())?;
                match health {
                    ProviderCredentialHealth::Verified => writeln!(writer, "validation: ok")?,
                    ProviderCredentialHealth::Unavailable => {
                        writeln!(
                            writer,
                            "validation: unavailable; credential stored but not verified. Retry `thndrs setup --provider {}` before coding.",
                            provider.label()
                        )?;
                        return Err(verification_unavailable_error(provider));
                    }
                    ProviderCredentialHealth::Rejected => return Err(credential_rejected_error(provider)),
                }
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "credential was not stored; run `thndrs login {}` before coding",
                        provider.label()
                    ),
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "credential setup skipped; run `thndrs login {}` before coding",
                    provider.label()
                ),
            ));
        }
    }

    maybe_write_model_config(&workspace, provider, scope, interactive, true, writer)?;
    writeln!(writer, "next: thndrs")?;
    Ok(())
}

fn verification_unavailable_error(provider: SetupProviderArg) -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        format!(
            "could not verify {} credentials; retry `thndrs setup --provider {}` before coding",
            provider.label(),
            provider.label()
        ),
    )
}

fn prompt_provider<W: Write>(
    writer: &mut W, default_provider: Option<SetupProviderArg>,
) -> io::Result<SetupProviderArg> {
    write_provider_choices(writer, default_provider)?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty()
        && let Some(default_provider) = default_provider
    {
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

fn write_provider_choices<W: Write>(writer: &mut W, default_provider: Option<SetupProviderArg>) -> io::Result<()> {
    writeln!(writer, "Choose provider:")?;
    for (index, provider) in SetupProviderArg::ALL.iter().enumerate() {
        let default = if Some(*provider) == default_provider { " (default)" } else { "" };
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
    match default_provider {
        Some(provider) => write!(writer, "Provider [{}]: ", provider.label())?,
        None => write!(writer, "Provider: ")?,
    }
    writer.flush()
}

fn auth_status(provider: SetupProviderArg, workspace: &std::path::Path) -> String {
    match provider.metadata().auth_kind {
        ProviderAuthKind::ApiKey { env_var } => auth::credential_source(env_var, workspace)
            .map(|source| source.label().to_string())
            .unwrap_or_else(|| "missing".to_string()),
        ProviderAuthKind::ChatGptOAuth { .. } => chatgpt_setup_auth_status().label().to_string(),
    }
}

fn chatgpt_setup_auth_status() -> ChatGptSetupAuthStatus {
    if std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV).is_ok_and(|value| !value.trim().is_empty()) {
        return ChatGptSetupAuthStatus::EnvironmentOverride;
    }
    match auth::read_chatgpt_codex_credentials() {
        Ok(Some(_)) => ChatGptSetupAuthStatus::StoredCredential,
        Ok(None) => ChatGptSetupAuthStatus::MissingCredential,
        Err(_) => ChatGptSetupAuthStatus::InvalidStore,
    }
}

fn verify_chatgpt_setup_auth(status: ChatGptSetupAuthStatus, health: ProviderCredentialHealth) -> io::Result<()> {
    match health {
        ProviderCredentialHealth::Verified => Ok(()),
        ProviderCredentialHealth::Rejected => match status {
            ChatGptSetupAuthStatus::EnvironmentOverride => Err(credential_rejected_error_for_source(
                SetupProviderArg::ChatgptCodex,
                auth::CredentialSource::Environment,
            )),
            ChatGptSetupAuthStatus::StoredCredential | ChatGptSetupAuthStatus::InvalidStore => {
                Err(credential_rejected_error(SetupProviderArg::ChatgptCodex))
            }
            ChatGptSetupAuthStatus::MissingCredential => Ok(()),
        },
        ProviderCredentialHealth::Unavailable => Err(verification_unavailable_error(SetupProviderArg::ChatgptCodex)),
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

fn run_chatgpt_setup<W, F, O>(
    provider: SetupProviderArg, auth_status: ChatGptSetupAuthStatus, workspace: &Path, interactive: bool,
    writer: &mut W, chatgpt_login: F, check_chatgpt_auth: O,
) -> io::Result<()>
where
    W: Write,
    F: FnOnce(&mut W) -> io::Result<()>,
    O: Fn(&Path) -> ProviderCredentialHealth,
{
    match auth_status {
        ChatGptSetupAuthStatus::EnvironmentOverride => {
            verify_chatgpt_setup_auth(auth_status, check_chatgpt_auth(workspace))?;
            if interactive && confirm(writer, "Create or update stored ChatGPT OAuth credentials now?")? {
                chatgpt_login(writer)?;
            }
        }
        ChatGptSetupAuthStatus::StoredCredential | ChatGptSetupAuthStatus::InvalidStore => {
            verify_chatgpt_setup_auth(auth_status, check_chatgpt_auth(workspace))?;
        }
        ChatGptSetupAuthStatus::MissingCredential => {
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
            verify_chatgpt_setup_auth(ChatGptSetupAuthStatus::StoredCredential, check_chatgpt_auth(workspace))?;
        }
    }
    Ok(())
}

fn write_chatgpt_setup_next<W: Write>(
    provider: SetupProviderArg, auth_status: ChatGptSetupAuthStatus, writer: &mut W,
) -> io::Result<()> {
    match auth_status {
        ChatGptSetupAuthStatus::EnvironmentOverride => writeln!(
            writer,
            "next: thndrs (using {}); run `thndrs login {}` later to create or update stored OAuth credentials",
            auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV,
            provider.label(),
        ),
        _ => writeln!(writer, "next: thndrs"),
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

    fn with_chatgpt_access_token<T>(token: Option<&str>, f: impl FnOnce() -> T) -> T {
        let old_token = std::env::var_os(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        unsafe {
            match token {
                Some(token) => std::env::set_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV, token),
                None => std::env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV),
            }
        }
        let result = f();
        unsafe {
            if let Some(token) = old_token {
                std::env::set_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV, token);
            } else {
                std::env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
            }
        }
        result
    }

    #[test]
    fn provider_defaults_from_model() {
        assert_eq!(SetupProviderArg::for_model("umans-coder"), SetupProviderArg::Umans);
        assert_eq!(
            SetupProviderArg::for_model("opencode/big-pickle"),
            SetupProviderArg::OpencodeZen
        );
        assert_eq!(
            SetupProviderArg::for_model("opencode-go/kimi-k2.7-code"),
            SetupProviderArg::OpencodeGo
        );
        assert_eq!(
            SetupProviderArg::for_model("chatgpt-codex/gpt-5.5"),
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
        write_provider_choices(&mut output, Some(SetupProviderArg::OpencodeZen)).expect("choices");
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
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::OpencodeZen), global: false, project: false };
        let mut output = Vec::new();

        with_home(&home, || {
            run_with_writer(
                &cli,
                &command,
                false,
                &mut output,
                |_| Ok(()),
                |_, _| ProviderCredentialHealth::Verified,
                |_| ProviderCredentialHealth::Verified,
            )
            .expect("setup");
        });
        let output = String::from_utf8(output).expect("utf8");

        assert!(output.contains("provider: opencode-zen"));
        assert!(output.contains("model: opencode/big-pickle"));
        assert!(output.contains("auth: project credentials"));
        assert!(output.contains("OpenCode Zen Big Pickle is the default model"));
    }

    #[test]
    fn rejected_existing_credential_keeps_setup_incomplete() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        auth::set_credential(
            &auth::project_credentials_path(&workspace),
            auth::UMANS_API_KEY_ENV,
            "rejected-key",
        )
        .expect("credential");
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::Umans), global: false, project: false };
        let mut output = Vec::new();

        let error = with_home(&home, || {
            run_with_writer(
                &cli,
                &command,
                false,
                &mut output,
                |_| Ok(()),
                |_, _| ProviderCredentialHealth::Rejected,
                |_| ProviderCredentialHealth::Verified,
            )
            .expect_err("rejected credential")
        });
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("thndrs login umans"));
        assert!(!output.contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
    }

    #[test]
    fn unavailable_existing_credential_keeps_setup_incomplete() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        auth::set_credential(
            &auth::project_credentials_path(&workspace),
            auth::UMANS_API_KEY_ENV,
            "unverified-key",
        )
        .expect("credential");
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::Umans), global: false, project: false };
        let mut output = Vec::new();

        let error = with_home(&home, || {
            run_with_writer(
                &cli,
                &command,
                false,
                &mut output,
                |_| Ok(()),
                |_, _| ProviderCredentialHealth::Unavailable,
                |_| ProviderCredentialHealth::Verified,
            )
            .expect_err("unavailable verification")
        });
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(error.to_string().contains("could not verify umans credentials"));
        assert!(!output.contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
    }

    #[test]
    fn rejected_environment_credential_explains_precedence() {
        let _guard = crate::test_env::lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let old_home = std::env::var_os("HOME");
        let old_key = std::env::var_os(auth::UMANS_API_KEY_ENV);
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var(auth::UMANS_API_KEY_ENV, "rejected-environment-key");
        }
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::Umans), global: false, project: false };
        let mut output = Vec::new();

        let error = run_with_writer(
            &cli,
            &command,
            false,
            &mut output,
            |_| Ok(()),
            |_, _| ProviderCredentialHealth::Rejected,
            |_| ProviderCredentialHealth::Verified,
        )
        .expect_err("rejected environment credential");

        unsafe {
            if let Some(home) = old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
            if let Some(key) = old_key {
                std::env::set_var(auth::UMANS_API_KEY_ENV, key);
            } else {
                std::env::remove_var(auth::UMANS_API_KEY_ENV);
            }
        }

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("replace or unset UMANS_API_KEY"));
        assert!(error.to_string().contains("thndrs login umans"));
        assert!(!String::from_utf8(output).expect("utf8").contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
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
            with_chatgpt_access_token(None, || {
                run_with_writer(
                    &cli,
                    &command,
                    false,
                    &mut output,
                    |_| Ok(()),
                    |_, _| ProviderCredentialHealth::Verified,
                    |_| ProviderCredentialHealth::Verified,
                )
                .expect_err("missing auth")
            })
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
    fn setup_without_provider_or_model_explains_noninteractive_route() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let cli = Cli { cwd: workspace, ..Cli::default() };
        let command = SetupCommand { provider: None, global: false, project: false };
        let mut output = Vec::new();

        let err = with_home(&home, || {
            run_with_writer(
                &cli,
                &command,
                false,
                &mut output,
                |_| Ok(()),
                |_, _| ProviderCredentialHealth::Verified,
                |_| ProviderCredentialHealth::Verified,
            )
            .expect_err("missing route")
        });

        assert!(output.is_empty());
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("--provider"));
        assert!(err.to_string().contains("--model"));
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
            with_chatgpt_access_token(None, || {
                run_with_writer(
                    &cli,
                    &command,
                    true,
                    &mut output,
                    |writer| {
                        called = true;
                        writeln!(writer, "fake ChatGPT OAuth login")
                    },
                    |_, _| ProviderCredentialHealth::Verified,
                    |_| ProviderCredentialHealth::Verified,
                )
                .expect("setup");
            });
        });
        let output = String::from_utf8(output).expect("utf8");

        assert!(called);
        assert!(output.contains("fake ChatGPT OAuth login"));
        assert!(!output.contains("Enter chatgpt-codex API key"));
        assert!(!output.contains("Choose credential store"));
    }

    #[test]
    fn rejected_chatgpt_environment_override_explains_precedence_before_setup_writes_config() {
        let _guard = crate::test_env::lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let old_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", &home);
        }
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::ChatgptCodex), global: false, project: true };
        let mut output = Vec::new();

        let error = with_chatgpt_access_token(Some("rejected-environment-token"), || {
            run_with_writer(
                &cli,
                &command,
                true,
                &mut output,
                |_| panic!("rejected environment override must not start OAuth login"),
                |_, _| ProviderCredentialHealth::Verified,
                |_| ProviderCredentialHealth::Rejected,
            )
            .expect_err("rejected environment override")
        });

        unsafe {
            if let Some(home) = old_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            error
                .to_string()
                .contains("replace or unset CHATGPT_CODEX_ACCESS_TOKEN")
        );
        assert!(error.to_string().contains("thndrs login chatgpt-codex"));
        assert!(!String::from_utf8(output).expect("utf8").contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
    }

    #[test]
    fn unavailable_stored_chatgpt_credential_keeps_setup_incomplete() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::ChatgptCodex), global: false, project: true };
        let mut output = Vec::new();

        let error = with_home(&home, || {
            with_chatgpt_access_token(None, || {
                auth::write_chatgpt_codex_credentials(&auth::ChatGptCodexCredentials {
                    access_token: "access-token".to_string(),
                    refresh_token: "refresh-token".to_string(),
                    expires_at_ms: u64::MAX,
                    account_id: "acct_test".to_string(),
                })
                .expect("stored OAuth credential");
                run_with_writer(
                    &cli,
                    &command,
                    true,
                    &mut output,
                    |_| panic!("unavailable stored credential must not start OAuth login"),
                    |_, _| ProviderCredentialHealth::Verified,
                    |_| ProviderCredentialHealth::Unavailable,
                )
                .expect_err("unavailable OAuth verification")
            })
        });
        let output = String::from_utf8(output).expect("utf8");

        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(error.to_string().contains("could not verify chatgpt-codex credentials"));
        assert!(!error.to_string().contains("thndrs login"));
        assert!(!output.contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
    }

    #[test]
    fn fresh_chatgpt_oauth_is_verified_before_setup_writes_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("workspace");
        let home = tmp.path().join("home");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::create_dir_all(&home).expect("home");
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let command = SetupCommand { provider: Some(SetupProviderArg::ChatgptCodex), global: false, project: true };
        let mut output = Vec::new();
        let mut login_called = false;

        let error = with_home(&home, || {
            with_chatgpt_access_token(None, || {
                run_with_writer(
                    &cli,
                    &command,
                    true,
                    &mut output,
                    |_| {
                        login_called = true;
                        Ok(())
                    },
                    |_, _| ProviderCredentialHealth::Verified,
                    |_| ProviderCredentialHealth::Rejected,
                )
                .expect_err("post-login OAuth verification should reject unusable credentials")
            })
        });
        let output = String::from_utf8(output).expect("utf8");

        assert!(login_called);
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("thndrs login chatgpt-codex"));
        assert!(!output.contains("next: thndrs"));
        assert!(!workspace.join(".thndrs/config.toml").exists());
    }

    #[test]
    fn setup_scope_uses_flags() {
        assert_eq!(
            SetupCommand { provider: None, global: true, project: false }.scope(),
            Some(CredentialScope::Global)
        );
        assert_eq!(
            SetupCommand { provider: None, global: false, project: true }.scope(),
            Some(CredentialScope::Project)
        );
        assert_eq!(
            SetupCommand { provider: None, global: false, project: false }.scope(),
            None
        );
    }
}
