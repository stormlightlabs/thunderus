//! First-run provider setup, authentication, and credential recovery.
//!
//! This module handles the setup flow for a selected model:
//!
//! 1. choosing a provider
//! 2. selecting credential and model-config scope
//! 3. collecting an API key
//! 4. writing or removing the resulting credential.
//!
//! ChatGPT Codex uses browser-first PKCE OAuth instead of API-key entry. Device
//! code remains an explicit headless/remote alternative.
//!
//! API-key input stays in [`FirstRunRecovery::secret_input`] until it is
//! written to the provider credential store.
//!
//! It is not copied into transcript, prompt, or session metadata.

use std::io;

use super::*;

/// Focused first-run and credential recovery surface.
#[derive(Clone, Eq, PartialEq)]
pub struct FirstRunRecovery {
    /// Provider being configured or diagnosed.
    pub provider: Option<SetupProviderArg>,
    /// Current recovery step.
    pub stage: RecoveryStage,
    /// Whether a prompt submit is waiting on this recovery.
    pub pending_provider_prompt: bool,
    /// Selected action row.
    pub selected: usize,
    /// Hidden API-key buffer. This is never rendered or written to transcripts.
    pub secret_input: String,
    /// ChatGPT OAuth state. Token material is never rendered.
    pub chatgpt_oauth: Option<ChatGptOAuthRecovery>,
}

impl std::fmt::Debug for FirstRunRecovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FirstRunRecovery")
            .field("provider", &self.provider)
            .field("stage", &self.stage)
            .field("pending_provider_prompt", &self.pending_provider_prompt)
            .field("selected", &self.selected)
            .field(
                "secret_input",
                &if self.secret_input.is_empty() { "<empty>" } else { "[redacted]" },
            )
            .field("chatgpt_oauth", &self.chatgpt_oauth)
            .finish()
    }
}

impl FirstRunRecovery {
    pub fn missing_label(&self) -> &'static str {
        match self.stage {
            RecoveryStage::ChooseProvider | RecoveryStage::ModelSelection | RecoveryStage::ModelConfigScope => "none",
            _ => match self.provider {
                Some(crate::cli::commands::setup::SetupProviderArg::ChatgptCodex) => "ChatGPT OAuth credential",
                Some(provider) => provider.api_key_env_var().unwrap_or("credential"),
                None => "ACP agent config",
            },
        }
    }

    pub fn setup(default_provider: SetupProviderArg) -> Self {
        let selected = match default_provider {
            SetupProviderArg::ChatgptCodex => 0,
            SetupProviderArg::OpencodeZen => 1,
            SetupProviderArg::OpencodeGo => 2,
            SetupProviderArg::Umans => 0,
        };
        Self {
            provider: Some(default_provider),
            stage: RecoveryStage::ChooseProvider,
            pending_provider_prompt: false,
            selected,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn missing_provider(provider: SetupProviderArg, pending_provider_prompt: bool) -> Self {
        Self {
            provider: Some(provider),
            stage: RecoveryStage::MissingCredential,
            pending_provider_prompt,
            selected: 0,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn unsupported_route(pending_provider_prompt: bool) -> Self {
        Self {
            provider: Some(SetupProviderArg::Umans),
            stage: RecoveryStage::UnsupportedRoute,
            pending_provider_prompt,
            selected: 0,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn acp_missing(pending_provider_prompt: bool) -> Self {
        Self {
            provider: None,
            stage: RecoveryStage::AcpMissing,
            pending_provider_prompt,
            selected: 0,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn login(provider: SetupProviderArg) -> Self {
        Self {
            provider: Some(provider),
            stage: if provider == SetupProviderArg::ChatgptCodex {
                RecoveryStage::MissingCredential
            } else {
                RecoveryStage::EnterKey
            },
            pending_provider_prompt: false,
            selected: 0,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn logout(provider: SetupProviderArg) -> Self {
        Self {
            provider: Some(provider),
            stage: RecoveryStage::LogoutConfirm,
            pending_provider_prompt: false,
            selected: 0,
            secret_input: String::new(),
            chatgpt_oauth: None,
        }
    }

    pub fn action_count(&self) -> usize {
        match self.stage {
            RecoveryStage::ChooseProvider => 4,
            RecoveryStage::UnsupportedRoute => 2,
            RecoveryStage::ModelSelection => self.app_model_selection_count(),
            RecoveryStage::ModelConfigScope => 4,
            RecoveryStage::MissingCredential => {
                if self.provider == Some(SetupProviderArg::ChatgptCodex) {
                    6
                } else {
                    5
                }
            }
            RecoveryStage::EnterKey => 1,
            RecoveryStage::ConfirmStore | RecoveryStage::LogoutConfirm => 3,
            RecoveryStage::Instructions => 2,
            RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPasteRedirect => 1,
            RecoveryStage::ChatGptOAuthPolling => self
                .chatgpt_oauth
                .as_ref()
                .filter(|oauth| oauth.method == ChatGptOAuthMethod::Browser)
                .map_or(1, |_| 2),
            RecoveryStage::ChatGptOAuthFailed => 3,
            RecoveryStage::AcpMissing => 4,
        }
    }

    fn app_model_selection_count(&self) -> usize {
        self.provider
            .map(setup_model_options)
            .map(|options| options.len())
            .unwrap_or(0)
    }
}

/// ChatGPT OAuth method selected by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatGptOAuthMethod {
    /// Browser PKCE with a loopback callback.
    Browser,
    /// Device code for headless or remote environments.
    DeviceCode,
}

/// ChatGPT OAuth state shown in the focused recovery surface.
#[derive(Clone, Eq, PartialEq)]
pub struct ChatGptOAuthRecovery {
    /// OAuth method selected by the user.
    pub method: ChatGptOAuthMethod,
    /// Browser authorization URL, when browser PKCE is active.
    pub authorization_url: Option<String>,
    /// Device-code response used for polling, when device code is active. Its
    /// debug output redacts the device token.
    pub code: Option<auth::ChatGptCodexDeviceCode>,
    /// UI tick when the next single device-code poll is allowed.
    pub next_poll_tick: u64,
    /// OAuth expiry tick.
    pub expires_at_tick: u64,
    /// Redacted status text for the recovery surface.
    pub status: String,
}

impl std::fmt::Debug for ChatGptOAuthRecovery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ChatGptOAuthRecovery")
            .field("method", &self.method)
            .field(
                "authorization_url",
                &self.authorization_url.as_ref().map(|_| "[redacted]"),
            )
            .field("code", &self.code)
            .field("next_poll_tick", &self.next_poll_tick)
            .field("expires_at_tick", &self.expires_at_tick)
            .field("status", &self.status)
            .finish()
    }
}

/// Step within the first-run recovery surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryStage {
    /// Choose which built-in provider to configure.
    ChooseProvider,
    /// Choose whether and where to persist the selected provider's default model.
    ModelConfigScope,
    /// Choose a model after provider authentication succeeds.
    ModelSelection,
    /// A configured model belongs to a retired or unknown built-in route.
    UnsupportedRoute,
    /// Selected provider is missing an API-key credential.
    MissingCredential,
    /// Hidden API-key entry is active.
    EnterKey,
    /// Select global/project storage before writing the key.
    ConfirmStore,
    /// Show setup instructions in a focused surface.
    Instructions,
    /// Starting a ChatGPT OAuth method.
    ChatGptOAuthRequesting,
    /// Waiting for browser callback or device-code authorization.
    ChatGptOAuthPolling,
    /// Pasting the full browser redirect URL into a hidden input.
    ChatGptOAuthPasteRedirect,
    /// ChatGPT OAuth failed with a redacted, user-readable error.
    ChatGptOAuthFailed,
    /// Confirm logout and storage scope.
    LogoutConfirm,
    /// ACP model recovery, separate from provider API-key setup.
    AcpMissing,
}

impl RecoveryStage {
    pub fn label(self) -> &'static str {
        match self {
            RecoveryStage::ChooseProvider => "choose provider",
            RecoveryStage::UnsupportedRoute => "unsupported provider route",
            RecoveryStage::ModelSelection => "choose model",
            RecoveryStage::ModelConfigScope => "model scope",
            RecoveryStage::MissingCredential => "authentication required",
            RecoveryStage::EnterKey => "credential entry",
            RecoveryStage::ConfirmStore => "credential scope",
            RecoveryStage::Instructions => "setup instructions",
            RecoveryStage::ChatGptOAuthRequesting => "starting OAuth",
            RecoveryStage::ChatGptOAuthPolling => "OAuth in progress",
            RecoveryStage::ChatGptOAuthPasteRedirect => "paste redirect",
            RecoveryStage::ChatGptOAuthFailed => "OAuth failed",
            RecoveryStage::LogoutConfirm => "remove credential",
            RecoveryStage::AcpMissing => "ACP setup required",
        }
    }
}

/// Small seam for testing TUI OAuth without real network calls.
///
/// FIXME: what on earth is this
#[derive(Clone, Copy, Debug)]
pub struct ChatGptOAuthDriver {
    pub start_browser_login: fn() -> Result<auth::ChatGptCodexBrowserLogin, auth::AuthError>,
    pub open_browser: fn(&str) -> Result<(), auth::AuthError>,
    pub poll_browser_login:
        fn(&mut auth::ChatGptCodexBrowserLogin) -> Result<auth::ChatGptCodexBrowserPoll, auth::AuthError>,
    pub complete_browser_redirect:
        fn(&auth::ChatGptCodexBrowserLogin, &str) -> Result<auth::ChatGptCodexCredentials, auth::AuthError>,
    pub request_device_code: fn() -> Result<auth::ChatGptCodexDeviceCode, auth::AuthError>,
    pub poll_device_code_once:
        fn(&auth::ChatGptCodexDeviceCode) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError>,
    pub write_credentials: fn(&auth::ChatGptCodexCredentials) -> Result<(), auth::AuthError>,
}

impl Default for ChatGptOAuthDriver {
    fn default() -> Self {
        Self {
            start_browser_login: auth::start_chatgpt_codex_browser_login,
            open_browser: auth::open_chatgpt_codex_authorization_url,
            poll_browser_login: auth::poll_chatgpt_codex_browser_login_once,
            complete_browser_redirect: auth::ChatGptCodexBrowserLogin::complete_redirect,
            request_device_code: auth::request_chatgpt_codex_device_code,
            poll_device_code_once: auth::poll_chatgpt_codex_device_code_once,
            write_credentials: auth::write_chatgpt_codex_credentials,
        }
    }
}

/// Setup state held while the reasoning picker is open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingSetupReasoningEffort {
    pub provider: SetupProviderArg,
    pub scope: CredentialScope,
}

pub fn provider_for_model(model: &str) -> SetupProviderArg {
    if opencode::is_zen_model_id(model) {
        SetupProviderArg::OpencodeZen
    } else if opencode::is_go_model_id(model) {
        SetupProviderArg::OpencodeGo
    } else if codex::is_model_id(model) {
        SetupProviderArg::ChatgptCodex
    } else {
        SetupProviderArg::Umans
    }
}

pub fn provider_authenticated(provider: SetupProviderArg, cwd: &std::path::Path) -> bool {
    match provider {
        SetupProviderArg::Umans => false,
        SetupProviderArg::ChatgptCodex => chatgpt_codex_auth_available_locally(),
        _ => match provider.api_key_env_var() {
            Some(env_var) => auth::credential_source(env_var, cwd).is_some(),
            _ => false,
        },
    }
}

pub fn chatgpt_codex_auth_available_locally() -> bool {
    if let Ok(token) = std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return auth::chatgpt_account_id_from_jwt(&token).is_ok();
    }

    matches!(
        auth::read_chatgpt_codex_credentials(),
        Ok(Some(credentials))
            if !credentials.access_token.trim().is_empty()
                && !credentials.refresh_token.trim().is_empty()
                && !credentials.account_id.trim().is_empty()
    )
}

pub fn selected_provider_missing(app: &App) -> Option<FirstRunRecovery> {
    if app.runtime.model.trim().is_empty() {
        return Some(FirstRunRecovery::setup(SetupProviderArg::ChatgptCodex));
    }

    if app.runtime.model.starts_with("fake-agent") {
        return None;
    }

    if let Some(acp_name) = crate::acp::config::parse_model_id(&app.runtime.model) {
        if app.runtime.cli.acp_agents.contains_key(acp_name) {
            return None;
        }
        return Some(FirstRunRecovery::acp_missing(true));
    }

    if crate::cli::commands::setup::model_uses_unsupported_route(&app.runtime.model) {
        return Some(FirstRunRecovery::unsupported_route(true));
    }

    let provider = provider_for_model(&app.runtime.model);
    if !provider_authenticated(provider, &app.runtime.cwd) {
        Some(FirstRunRecovery::missing_provider(provider, true))
    } else {
        None
    }
}

pub fn handle_first_run_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    let recovery = app.overlay.setup_mut()?;

    if recovery.stage == RecoveryStage::EnterKey || recovery.stage == RecoveryStage::ChatGptOAuthPasteRedirect {
        match key.code {
            KeyCode::Esc => {
                recovery.secret_input.clear();
                recovery.stage = if recovery.stage == RecoveryStage::ChatGptOAuthPasteRedirect {
                    RecoveryStage::ChatGptOAuthPolling
                } else {
                    RecoveryStage::MissingCredential
                };
                recovery.selected = 0;
            }
            KeyCode::Backspace => {
                recovery.secret_input.pop();
            }
            KeyCode::Enter => {
                if recovery.secret_input.trim().is_empty() {
                    app.transcript.entries.push(Entry::Error {
                        text: if recovery.stage == RecoveryStage::ChatGptOAuthPasteRedirect {
                            String::from("paste the full ChatGPT redirect URL or press Esc to cancel")
                        } else {
                            String::from("API key cannot be empty")
                        },
                    });
                } else if recovery.stage == RecoveryStage::ChatGptOAuthPasteRedirect {
                    let redirect = recovery.secret_input.clone();
                    recovery.secret_input.clear();
                    complete_chatgpt_oauth_redirect(app, &redirect);
                } else {
                    recovery.stage = RecoveryStage::ConfirmStore;
                    recovery.selected = 0;
                }
            }
            KeyCode::Char(ch) => recovery.secret_input.push(ch),
            _ => {}
        }
        return None;
    }

    if matches!(
        recovery.stage,
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPolling
    ) && key.code == KeyCode::Esc
    {
        recovery.stage = RecoveryStage::MissingCredential;
        recovery.selected = 0;
        recovery.chatgpt_oauth = None;
        app.overlay.set_browser_login(None);
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            app.overlay.close();
            None
        }
        KeyCode::Up => {
            recovery.selected = recovery.selected.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            let max = recovery.action_count().saturating_sub(1);
            recovery.selected = (recovery.selected + 1).min(max);
            None
        }
        KeyCode::Enter => accept_recovery_action(app),
        _ => None,
    }
}

pub fn accept_recovery_action(app: &mut App) -> Option<Msg> {
    let recovery = app.overlay.setup().cloned()?;

    match recovery.stage {
        RecoveryStage::ChooseProvider => {
            let Some(provider) = first_run_provider(recovery.selected) else {
                app.overlay.show_setup(FirstRunRecovery {
                    provider: None,
                    stage: RecoveryStage::Instructions,
                    pending_provider_prompt: recovery.pending_provider_prompt,
                    selected: 0,
                    secret_input: String::new(),
                    chatgpt_oauth: None,
                });
                return None;
            };
            let stage = if provider_authenticated(provider, &app.runtime.cwd) {
                RecoveryStage::ModelSelection
            } else {
                RecoveryStage::MissingCredential
            };
            app.overlay.show_setup(FirstRunRecovery {
                provider: Some(provider),
                stage,
                pending_provider_prompt: recovery.pending_provider_prompt,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: None,
            });
        }
        RecoveryStage::ModelSelection => select_setup_model(app, &recovery),
        RecoveryStage::ModelConfigScope => configure_setup_model_scope(app, &recovery),
        RecoveryStage::UnsupportedRoute => match recovery.selected {
            0 => app
                .overlay
                .show_setup(FirstRunRecovery::setup(SetupProviderArg::ChatgptCodex)),
            1 => {
                app.runtime.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::MissingCredential if recovery.provider == Some(SetupProviderArg::ChatgptCodex) => {
            match recovery.selected {
                0 => start_chatgpt_browser_oauth_recovery(app),
                1 => start_chatgpt_device_oauth_recovery(app),
                2 => {
                    app.overlay.close();
                    open_model_picker(app);
                }
                3 => {
                    if let Some(active) = app.overlay.setup_mut() {
                        active.stage = RecoveryStage::Instructions;
                        active.selected = 0;
                        active.chatgpt_oauth = None;
                    }
                }
                4 => {
                    if recovery.pending_provider_prompt {
                        app.transcript.entries.push(Entry::Status {
                            text: String::from(
                                "setup required before submitting this ChatGPT Codex prompt; start OAuth login or switch model",
                            ),
                        });
                    } else {
                        app.overlay.close();
                        app.transcript
                            .entries
                            .push(Entry::Status { text: String::from("setup skipped") });
                    }
                }
                5 => {
                    app.runtime.quit = true;
                    return Some(Msg::Quit);
                }
                _ => {}
            }
        }
        RecoveryStage::MissingCredential => match recovery.selected {
            0 => {
                if let Some(active) = app.overlay.setup_mut() {
                    active.stage = RecoveryStage::EnterKey;
                    active.selected = 0;
                    active.secret_input.clear();
                }
            }
            1 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if let Some(active) = app.overlay.setup_mut() {
                        active.stage = RecoveryStage::Instructions;
                        active.selected = 0;
                    }
                    return None;
                }
                app.overlay.close();
                open_model_picker(app);
            }
            2 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if recovery.pending_provider_prompt {
                        app.transcript.entries.push(Entry::Status {
                            text: String::from(
                                "setup required before submitting this ChatGPT Codex prompt; start ChatGPT OAuth login or switch model",
                            ),
                        });
                    } else {
                        app.overlay.close();
                        app.transcript
                            .entries
                            .push(Entry::Status { text: String::from("setup skipped") });
                    }
                    return None;
                }
                if let Some(active) = app.overlay.setup_mut() {
                    active.stage = RecoveryStage::Instructions;
                    active.selected = 0;
                }
            }
            3 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    app.runtime.quit = true;
                    return Some(Msg::Quit);
                }
                if recovery.pending_provider_prompt {
                    app.transcript.entries.push(Entry::Status {
                        text: String::from(
                            "setup required before submitting this provider-backed prompt; enter a key or switch model",
                        ),
                    });
                } else {
                    app.overlay.close();
                    app.transcript
                        .entries
                        .push(Entry::Status { text: String::from("setup skipped") });
                }
            }
            4 => {
                app.runtime.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::ConfirmStore => store_recovery_credential(app, &recovery),
        RecoveryStage::Instructions => match recovery.selected {
            0 => {
                if let Some(active) = app.overlay.setup_mut() {
                    active.stage = if active.provider.is_none() {
                        RecoveryStage::ChooseProvider
                    } else {
                        RecoveryStage::MissingCredential
                    };
                    active.selected = 0;
                }
            }
            1 => app.overlay.close(),
            _ => {}
        },
        RecoveryStage::ChatGptOAuthRequesting => {}
        RecoveryStage::ChatGptOAuthPolling => {
            if recovery
                .chatgpt_oauth
                .as_ref()
                .is_some_and(|oauth| oauth.method == ChatGptOAuthMethod::Browser && recovery.selected == 1)
            {
                if let Some(active) = app.overlay.setup_mut() {
                    active.stage = RecoveryStage::ChatGptOAuthPasteRedirect;
                    active.selected = 0;
                    active.secret_input.clear();
                }
            } else if let Some(active) = app.overlay.setup_mut() {
                active.stage = RecoveryStage::MissingCredential;
                active.selected = 0;
                active.chatgpt_oauth = None;
                app.overlay.set_browser_login(None);
            }
        }
        RecoveryStage::ChatGptOAuthPasteRedirect => {}
        RecoveryStage::ChatGptOAuthFailed => match recovery.selected {
            0 => match recovery.chatgpt_oauth.as_ref().map(|oauth| oauth.method) {
                Some(ChatGptOAuthMethod::DeviceCode) => start_chatgpt_device_oauth_recovery(app),
                _ => start_chatgpt_browser_oauth_recovery(app),
            },
            1 => {
                start_chatgpt_device_oauth_recovery(app);
            }
            2 => {
                if let Some(active) = app.overlay.setup_mut() {
                    active.stage = RecoveryStage::MissingCredential;
                    active.selected = 0;
                    active.chatgpt_oauth = None;
                }
                app.overlay.set_browser_login(None);
            }
            _ => {}
        },
        RecoveryStage::LogoutConfirm => remove_recovery_credential(app, &recovery),
        RecoveryStage::AcpMissing => match recovery.selected {
            0 => {
                app.overlay.close();
                open_model_picker(app);
            }
            1 => {
                app.transcript.entries.push(Entry::Status {
                    text: String::from("ACP setup: run `thndrs acp list` or `thndrs acp registry` outside the TUI"),
                });
            }
            2 => {
                if recovery.pending_provider_prompt {
                    app.transcript.entries.push(Entry::Status {
                        text: String::from(
                            "ACP agent config is required before submitting this prompt; switch model or configure ACP",
                        ),
                    });
                } else {
                    app.overlay.close();
                }
            }
            3 => {
                app.runtime.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::EnterKey => {}
    }

    None
}

pub fn configure_setup_model_scope(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.overlay.close();
        return;
    };

    match recovery.selected {
        0 => {
            if write_setup_model_config(app, provider, CredentialScope::Project).is_ok() {
                after_setup_model_config(app, provider, CredentialScope::Project);
            }
        }
        1 => {
            if write_setup_model_config(app, provider, CredentialScope::Global).is_ok() {
                after_setup_model_config(app, provider, CredentialScope::Global);
            }
        }
        2 => {
            app.transcript
                .entries
                .push(Entry::Status { text: String::from("model config skipped") });
            advance_after_setup_model_config(app, provider);
        }
        _ => {
            app.overlay.close();
            app.transcript
                .entries
                .push(Entry::Status { text: String::from("setup skipped") });
        }
    }
}

pub fn after_setup_model_config(app: &mut App, provider: SetupProviderArg, scope: CredentialScope) {
    if codex::supports_reasoning_effort(&app.runtime.model) {
        app.overlay
            .set_pending_setup_reasoning_effort(PendingSetupReasoningEffort { provider, scope });
        open_reasoning_effort_picker(app);
    } else {
        advance_after_setup_model_config(app, provider);
    }
}

pub fn write_setup_model_config(app: &mut App, _provider: SetupProviderArg, scope: CredentialScope) -> io::Result<()> {
    let model = app.runtime.model.trim();
    if model.is_empty() {
        let err = io::Error::new(io::ErrorKind::InvalidInput, "choose a model before saving setup");
        app.transcript
            .entries
            .push(Entry::Error { text: format!("failed to save selected model to config: {err}") });
        return Err(err);
    }
    let path = match scope {
        CredentialScope::Global => match config::global_config_path() {
            Some(path) => path,
            None => {
                let err = io::Error::new(io::ErrorKind::NotFound, "HOME is not available");
                app.transcript
                    .entries
                    .push(Entry::Error { text: format!("failed to save selected model to global config: {err}") });
                return Err(err);
            }
        },
        CredentialScope::Project => config::project_config_path(&app.runtime.cwd),
    };

    match config::write_model_config(&path, model) {
        Ok(()) => {
            let display = match scope {
                CredentialScope::Global => config::global_config_path_display(&path),
                CredentialScope::Project => config::project_config_path_display(&path, &app.runtime.cwd),
            };
            app.transcript
                .entries
                .push(Entry::Status { text: format!("model: {model} (saved to {display})") });
            Ok(())
        }
        Err(err) => {
            app.transcript.entries.push(Entry::Error {
                text: format!("failed to save selected model to {} config: {err}", scope.label()),
            });
            Err(err)
        }
    }
}

pub fn advance_after_setup_model_config(app: &mut App, provider: SetupProviderArg) {
    if provider_authenticated(provider, &app.runtime.cwd) {
        app.overlay.close();
        app.transcript.entries.push(Entry::Status {
            text: format!(
                "setup saved for {}; thndrs will verify the credential on the first provider request",
                provider.label()
            ),
        });
    } else if provider == SetupProviderArg::ChatgptCodex {
        app.overlay
            .show_setup(FirstRunRecovery::missing_provider(provider, false));
    } else {
        app.overlay.show_setup(FirstRunRecovery::login(provider));
    }
}

pub fn setup_model_options(provider: SetupProviderArg) -> Vec<PickerItem> {
    let mut options: Vec<PickerItem> = offline_model_picker_items()
        .into_iter()
        .filter(|item| provider_for_model(&item.label) == provider)
        .collect();
    let default_model = provider.default_model();
    if let Some(index) = options.iter().position(|item| item.label == default_model) {
        options.swap(0, index);
    } else {
        options.insert(0, PickerItem::new(default_model, "provider setup model"));
    }
    options
}

/// Start the browser-first ChatGPT Codex OAuth recovery.
pub fn start_chatgpt_browser_oauth_recovery(app: &mut App) {
    let pending_provider_prompt = app
        .overlay
        .setup()
        .as_ref()
        .is_some_and(|recovery| recovery.pending_provider_prompt);
    if let Some(active) = app.overlay.setup_mut() {
        active.stage = RecoveryStage::ChatGptOAuthRequesting;
        active.selected = 0;
        active.chatgpt_oauth = None;
    }
    app.overlay.set_browser_login(None);

    match (app.overlay.oauth_driver().start_browser_login)() {
        Ok(login) => {
            let authorization_url = login.authorization_url().to_string();
            let status = match (app.overlay.oauth_driver().open_browser)(&authorization_url) {
                Ok(()) => String::from("Browser opened. Waiting for the ChatGPT callback."),
                Err(_) => String::from("Browser did not open; copy the authorization URL below."),
            };
            let expires_at_tick = app.runtime.ui_tick.wrapping_add(seconds_to_ticks(app, 5 * 60));
            app.overlay.set_browser_login(Some(login));
            app.overlay.show_setup(FirstRunRecovery {
                provider: Some(SetupProviderArg::ChatgptCodex),
                stage: RecoveryStage::ChatGptOAuthPolling,
                pending_provider_prompt,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: Some(ChatGptOAuthRecovery {
                    method: ChatGptOAuthMethod::Browser,
                    authorization_url: Some(authorization_url),
                    code: None,
                    next_poll_tick: app.runtime.ui_tick,
                    expires_at_tick,
                    status,
                }),
            });
        }
        Err(err) => set_chatgpt_oauth_failure(
            app,
            ChatGptOAuthMethod::Browser,
            pending_provider_prompt,
            format!(
                "ChatGPT browser OAuth could not start: {}",
                redact_auth_error(&err.to_string())
            ),
        ),
    }
}

/// Start the explicitly selected headless ChatGPT Codex device-code recovery.
pub fn start_chatgpt_device_oauth_recovery(app: &mut App) {
    let pending_provider_prompt = app
        .overlay
        .setup()
        .as_ref()
        .is_some_and(|recovery| recovery.pending_provider_prompt);
    if let Some(active) = app.overlay.setup_mut() {
        active.stage = RecoveryStage::ChatGptOAuthRequesting;
        active.selected = 0;
        active.chatgpt_oauth = None;
    }
    app.overlay.set_browser_login(None);

    match (app.overlay.oauth_driver().request_device_code)() {
        Ok(code) => {
            let next_poll_tick = app
                .runtime
                .ui_tick
                .wrapping_add(seconds_to_ticks(app, code.interval.unwrap_or(5).max(1)));
            let expires_at_tick = app
                .runtime
                .ui_tick
                .wrapping_add(seconds_to_ticks(app, code.expires_in.unwrap_or(900).max(1)));
            app.overlay.show_setup(FirstRunRecovery {
                provider: Some(SetupProviderArg::ChatgptCodex),
                stage: RecoveryStage::ChatGptOAuthPolling,
                pending_provider_prompt,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: Some(ChatGptOAuthRecovery {
                    method: ChatGptOAuthMethod::DeviceCode,
                    authorization_url: None,
                    code: Some(code),
                    next_poll_tick,
                    expires_at_tick,
                    status: String::from("Waiting for ChatGPT authorization."),
                }),
            });
        }
        Err(err) => {
            set_chatgpt_oauth_failure(
                app,
                ChatGptOAuthMethod::DeviceCode,
                pending_provider_prompt,
                format!(
                    "ChatGPT device-code login could not start: {}",
                    redact_auth_error(&err.to_string())
                ),
            );
        }
    }
}

fn set_chatgpt_oauth_failure(app: &mut App, method: ChatGptOAuthMethod, pending_provider_prompt: bool, status: String) {
    app.overlay.set_browser_login(None);
    app.overlay.show_setup(FirstRunRecovery {
        provider: Some(SetupProviderArg::ChatgptCodex),
        stage: RecoveryStage::ChatGptOAuthFailed,
        pending_provider_prompt,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: Some(ChatGptOAuthRecovery {
            method,
            authorization_url: None,
            code: None,
            next_poll_tick: app.runtime.ui_tick,
            expires_at_tick: app.runtime.ui_tick,
            status: status.clone(),
        }),
    });
    app.transcript.entries.push(Entry::Error { text: status });
}

fn finish_chatgpt_oauth(
    app: &mut App, credentials: &auth::ChatGptCodexCredentials, pending_provider_prompt: bool,
    method: ChatGptOAuthMethod,
) {
    match (app.overlay.oauth_driver().write_credentials)(credentials) {
        Ok(()) => {
            app.overlay.set_browser_login(None);
            let needs_model = app.runtime.model.trim().is_empty();
            app.transcript.entries.push(Entry::Status {
                text: String::from("chatgpt-codex OAuth credential stored in global auth store"),
            });
            if needs_model {
                app.overlay.show_setup(FirstRunRecovery {
                    provider: Some(SetupProviderArg::ChatgptCodex),
                    stage: RecoveryStage::ModelSelection,
                    pending_provider_prompt,
                    selected: 0,
                    secret_input: String::new(),
                    chatgpt_oauth: None,
                });
            } else {
                app.overlay.close();
            }
        }
        Err(err) => set_chatgpt_oauth_failure(
            app,
            method,
            pending_provider_prompt,
            format!(
                "ChatGPT OAuth credential write failed: {}",
                redact_auth_error(&err.to_string())
            ),
        ),
    }
}

fn complete_chatgpt_oauth_redirect(app: &mut App, redirect: &str) {
    let pending_provider_prompt = app
        .overlay
        .setup()
        .as_ref()
        .is_some_and(|recovery| recovery.pending_provider_prompt);
    let result = app
        .overlay
        .browser_login()
        .as_ref()
        .ok_or_else(|| auth::AuthError::ChatGptCodex("browser OAuth session is no longer active".to_string()))
        .and_then(|login| (app.overlay.oauth_driver().complete_browser_redirect)(login, redirect));
    match result {
        Ok(credentials) => {
            finish_chatgpt_oauth(app, &credentials, pending_provider_prompt, ChatGptOAuthMethod::Browser)
        }
        Err(err) => {
            let status = format!("ChatGPT redirect was rejected: {}", redact_auth_error(&err.to_string()));
            if let Some(recovery) = app.overlay.setup_mut() {
                recovery.stage = RecoveryStage::ChatGptOAuthPolling;
                recovery.selected = 0;
                if let Some(oauth) = recovery.chatgpt_oauth.as_mut() {
                    oauth.status = status.clone();
                }
            }
            app.transcript.entries.push(Entry::Error { text: status });
        }
    }
}

pub fn poll_chatgpt_oauth_on_tick(app: &mut App) {
    let tick_ms = app.runtime.cli.tick_rate_ms.max(1);
    let Some(recovery) = app.overlay.setup() else {
        return;
    };
    if recovery.stage != RecoveryStage::ChatGptOAuthPolling {
        return;
    }
    let Some(oauth) = recovery.chatgpt_oauth.as_ref() else {
        return;
    };
    let method = oauth.method;
    let pending_provider_prompt = recovery.pending_provider_prompt;
    let expires_at_tick = oauth.expires_at_tick;
    let next_poll_tick = oauth.next_poll_tick;
    if super::agent_lifecycle::now_or_after_deadline(app.runtime.ui_tick, expires_at_tick) {
        set_chatgpt_oauth_failure(
            app,
            method,
            pending_provider_prompt,
            match method {
                ChatGptOAuthMethod::Browser => String::from("ChatGPT browser OAuth callback expired."),
                ChatGptOAuthMethod::DeviceCode => String::from("ChatGPT device-code login expired."),
            },
        );
        return;
    }
    if method == ChatGptOAuthMethod::DeviceCode
        && !super::agent_lifecycle::now_or_after_deadline(app.runtime.ui_tick, next_poll_tick)
    {
        return;
    }

    match method {
        ChatGptOAuthMethod::Browser => {
            let poll_browser_login = app.overlay.oauth_driver().poll_browser_login;
            let result = app
                .overlay
                .browser_login_mut()
                .ok_or_else(|| auth::AuthError::ChatGptCodex("browser OAuth session is no longer active".to_string()))
                .and_then(poll_browser_login);
            match result {
                Ok(auth::ChatGptCodexBrowserPoll::Pending) => {}
                Ok(auth::ChatGptCodexBrowserPoll::Authorized(credentials)) => {
                    finish_chatgpt_oauth(app, &credentials, pending_provider_prompt, method);
                }
                Err(err) => set_chatgpt_oauth_failure(
                    app,
                    method,
                    pending_provider_prompt,
                    format!("ChatGPT browser OAuth failed: {}", redact_auth_error(&err.to_string())),
                ),
            }
        }
        ChatGptOAuthMethod::DeviceCode => {
            let Some(code) = app
                .overlay
                .setup()
                .and_then(|recovery| recovery.chatgpt_oauth.as_ref())
                .and_then(|oauth| oauth.code.as_ref())
                .cloned()
            else {
                set_chatgpt_oauth_failure(
                    app,
                    method,
                    pending_provider_prompt,
                    String::from("ChatGPT device-code state is unavailable."),
                );
                return;
            };
            match (app.overlay.oauth_driver().poll_device_code_once)(&code) {
                Ok(auth::ChatGptCodexDevicePoll::Pending) => {
                    if let Some(oauth) = app
                        .overlay
                        .setup_mut()
                        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
                    {
                        oauth.status = String::from("Waiting for ChatGPT authorization.");
                        oauth.next_poll_tick = app
                            .runtime
                            .ui_tick
                            .wrapping_add(seconds_to_ticks_for_ms(tick_ms, code.interval.unwrap_or(5).max(1)));
                    }
                }
                Ok(auth::ChatGptCodexDevicePoll::SlowDown) => {
                    if let Some(oauth) = app
                        .overlay
                        .setup_mut()
                        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
                    {
                        oauth.status = String::from("ChatGPT asked the client to slow down; waiting.");
                        oauth.next_poll_tick = app.runtime.ui_tick.wrapping_add(seconds_to_ticks_for_ms(
                            tick_ms,
                            code.interval.unwrap_or(5).max(1).saturating_add(5),
                        ));
                    }
                }
                Ok(auth::ChatGptCodexDevicePoll::Authorized(credentials)) => {
                    finish_chatgpt_oauth(app, &credentials, pending_provider_prompt, method);
                }
                Err(err) => set_chatgpt_oauth_failure(
                    app,
                    method,
                    pending_provider_prompt,
                    format!(
                        "ChatGPT device-code polling failed: {}",
                        redact_auth_error(&err.to_string())
                    ),
                ),
            }
        }
    }
}

pub fn seconds_to_ticks(app: &App, seconds: u64) -> u64 {
    seconds_to_ticks_for_ms(app.runtime.cli.tick_rate_ms.max(1), seconds)
}

pub fn seconds_to_ticks_for_ms(tick_ms: u64, seconds: u64) -> u64 {
    seconds.saturating_mul(1000).div_ceil(tick_ms).max(1)
}

pub fn redact_auth_error(message: &str) -> String {
    let mut redacted = Vec::new();
    for part in message.split_whitespace() {
        if part.len() >= 24
            || part.contains("access_token")
            || part.contains("refresh_token")
            || part.contains("device_auth_id")
            || part.contains("device_code")
        {
            redacted.push("[redacted]");
        } else {
            redacted.push(part);
        }
    }
    redacted.join(" ")
}

pub fn selected_scope(selected: usize) -> Option<CredentialScope> {
    match selected {
        0 => Some(CredentialScope::Global),
        1 => Some(CredentialScope::Project),
        _ => None,
    }
}

pub fn store_recovery_credential(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.overlay.close();
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.overlay.show_setup(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        return;
    };

    let key = recovery.secret_input.trim();
    let path = match crate::cli::commands::auth::credential_path(scope, &app.runtime.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };

    let Some(env_var) = provider.api_key_env_var() else {
        app.overlay.show_setup(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("ChatGPT Codex uses OAuth login, not API-key storage") });
        return;
    };
    match auth::set_credential(&path, env_var, key) {
        Ok(()) => {
            if scope == CredentialScope::Project
                && let Err(err) = auth::ensure_git_exclude(&app.runtime.cwd)
            {
                app.transcript
                    .entries
                    .push(Entry::Error { text: format!("git exclude update failed: {err}") });
            }
            app.transcript
                .entries
                .push(Entry::Status { text: format!("{} credential stored in {}", provider.label(), scope.label()) });
            if app.runtime.model.trim().is_empty() {
                app.overlay.show_setup(FirstRunRecovery {
                    provider: Some(provider),
                    stage: RecoveryStage::ModelSelection,
                    pending_provider_prompt: recovery.pending_provider_prompt,
                    selected: 0,
                    secret_input: String::new(),
                    chatgpt_oauth: None,
                });
            } else {
                app.overlay.close();
            }
        }
        Err(err) => app
            .transcript
            .entries
            .push(Entry::Error { text: format!("credential write failed: {err}") }),
    }
}

pub fn remove_recovery_credential(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.overlay.close();
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.overlay.close();
        app.transcript
            .entries
            .push(Entry::Status { text: String::from("logout cancelled") });
        return;
    };
    let path = match crate::cli::commands::auth::credential_path(scope, &app.runtime.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .entries
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };
    let Some(env_var) = provider.api_key_env_var() else {
        app.overlay.close();
        app.transcript
            .entries
            .push(Entry::Error { text: String::from("ChatGPT Codex credentials are stored in ~/.thndrs/auth.json") });
        return;
    };
    match auth::remove_credential(&path, env_var) {
        Ok(()) => {
            app.overlay.close();
            app.transcript.entries.push(Entry::Status {
                text: format!("{} credential removed from {}", provider.label(), scope.label()),
            });
        }
        Err(err) => app
            .transcript
            .entries
            .push(Entry::Error { text: format!("credential remove failed: {err}") }),
    }
}

fn select_setup_model(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.overlay
            .show_setup(FirstRunRecovery::setup(SetupProviderArg::ChatgptCodex));
        return;
    };
    let options = setup_model_options(provider);
    let Some(model) = options.get(recovery.selected).map(|item| item.label.clone()) else {
        return;
    };
    app.runtime.model = model.clone();
    app.runtime.cli.model = model.clone();
    app.transcript
        .entries
        .push(Entry::Status { text: format!("model selected: {model}") });
    app.overlay.show_setup(FirstRunRecovery {
        provider: Some(provider),
        stage: RecoveryStage::ModelConfigScope,
        pending_provider_prompt: recovery.pending_provider_prompt,
        selected: 0,
        secret_input: String::new(),
        chatgpt_oauth: None,
    });
}

fn first_run_provider(selected: usize) -> Option<SetupProviderArg> {
    match selected {
        0 => Some(SetupProviderArg::ChatgptCodex),
        1 => Some(SetupProviderArg::OpencodeZen),
        2 => Some(SetupProviderArg::OpencodeGo),
        _ => None,
    }
}
