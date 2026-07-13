//! First-run provider setup, authentication, and credential recovery.
//!
//! This module handles the setup flow for a selected model:
//!
//! 1. choosing a provider
//! 2. selecting credential and model-config scope
//! 3. collecting an API key
//! 4. writing or removing the resulting credential.
//!
//! ChatGPT Codex uses its device-code OAuth flow instead of API-key entry.
//!
//! API-key input stays in [`FirstRunRecovery::secret_input`] until it is
//! written to the provider credential store.
//!
//! It is not copied into transcript, prompt, or session metadata.

use super::*;

/// Focused first-run and credential recovery surface.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// ChatGPT OAuth device-code state. Token material is never rendered.
    pub chatgpt_oauth: Option<ChatGptOAuthRecovery>,
}

impl FirstRunRecovery {
    pub fn missing_label(&self) -> &'static str {
        match self.stage {
            RecoveryStage::ChooseProvider | RecoveryStage::ModelConfigScope => "none",
            _ => match self.provider {
                Some(crate::cli::commands::setup::SetupProviderArg::ChatgptCodex) => "ChatGPT OAuth credential",
                Some(provider) => provider.api_key_env_var().unwrap_or("credential"),
                None => "ACP agent config",
            },
        }
    }

    pub fn setup(default_provider: SetupProviderArg) -> Self {
        let selected = SetupProviderArg::ALL
            .iter()
            .position(|provider| *provider == default_provider)
            .unwrap_or(0);
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
}

/// ChatGPT OAuth state shown in the focused recovery surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatGptOAuthRecovery {
    /// Device-code response used for polling. Its debug output redacts the device token.
    pub code: auth::ChatGptCodexDeviceCode,
    /// UI tick when the next single poll is allowed.
    pub next_poll_tick: u64,
    /// OAuth expiry tick derived from the device-code lifetime.
    pub expires_at_tick: u64,
    /// Redacted status text for the recovery surface.
    pub status: String,
}

/// Step within the first-run recovery surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryStage {
    /// Choose which built-in provider to configure.
    ChooseProvider,
    /// Choose whether and where to persist the selected provider's default model.
    ModelConfigScope,
    /// Selected provider is missing an API-key credential.
    MissingCredential,
    /// Hidden API-key entry is active.
    EnterKey,
    /// Select global/project storage before writing the key.
    ConfirmStore,
    /// Show setup instructions in a focused surface.
    Instructions,
    /// Requesting a ChatGPT OAuth device code.
    ChatGptOAuthRequesting,
    /// Waiting for ChatGPT OAuth browser authorization and polling on ticks.
    ChatGptOAuthPolling,
    /// ChatGPT OAuth failed with a redacted, user-readable error.
    ChatGptOAuthFailed,
    /// Confirm logout and storage scope.
    LogoutConfirm,
    /// ACP model recovery, separate from provider API-key setup.
    AcpMissing,
}

/// Small seam for testing TUI OAuth without real network calls.
#[derive(Clone, Copy, Debug)]
pub struct ChatGptOAuthDriver {
    pub request_device_code: fn() -> Result<auth::ChatGptCodexDeviceCode, auth::AuthError>,
    pub poll_device_code_once:
        fn(&auth::ChatGptCodexDeviceCode) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError>,
    pub write_credentials: fn(&auth::ChatGptCodexCredentials) -> Result<(), auth::AuthError>,
}

impl Default for ChatGptOAuthDriver {
    fn default() -> Self {
        Self {
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
    if provider == SetupProviderArg::ChatgptCodex {
        return chatgpt_codex_auth_available_locally();
    }
    let Some(env_var) = provider.api_key_env_var() else {
        return false;
    };
    auth::credential_source(env_var, cwd).is_some()
}

pub fn chatgpt_codex_auth_available_locally() -> bool {
    if let Ok(token) = std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return auth::chatgpt_account_id_from_jwt(&token).is_ok();
    }

    matches!(auth::read_chatgpt_codex_credentials(), Ok(Some(_)))
}

pub fn selected_provider_missing(app: &App) -> Option<FirstRunRecovery> {
    if let Some(acp_name) = crate::acp::config::parse_model_id(&app.model) {
        if app.cli.acp_agents.contains_key(acp_name) {
            return None;
        }
        return Some(FirstRunRecovery::acp_missing(true));
    }

    let provider = provider_for_model(&app.model);
    if !provider_authenticated(provider, &app.cwd) {
        Some(FirstRunRecovery::missing_provider(provider, true))
    } else {
        None
    }
}

pub fn recovery_action_count(recovery: &FirstRunRecovery) -> usize {
    match recovery.stage {
        RecoveryStage::ChooseProvider => SetupProviderArg::ALL.len(),
        RecoveryStage::ModelConfigScope => 4,
        RecoveryStage::MissingCredential => 5,
        RecoveryStage::EnterKey => 1,
        RecoveryStage::ConfirmStore | RecoveryStage::LogoutConfirm => 3,
        RecoveryStage::Instructions => 2,
        RecoveryStage::ChatGptOAuthRequesting | RecoveryStage::ChatGptOAuthPolling => 1,
        RecoveryStage::ChatGptOAuthFailed => 2,
        RecoveryStage::AcpMissing => 4,
    }
}

pub fn handle_first_run_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    let recovery = app.first_run_recovery.as_mut()?;

    if recovery.stage == RecoveryStage::EnterKey {
        match key.code {
            KeyCode::Esc => {
                recovery.secret_input.clear();
                recovery.stage = RecoveryStage::MissingCredential;
                recovery.selected = 0;
            }
            KeyCode::Backspace => {
                recovery.secret_input.pop();
            }
            KeyCode::Enter => {
                if recovery.secret_input.trim().is_empty() {
                    app.transcript
                        .push(Entry::Error { text: String::from("API key cannot be empty") });
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
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            app.first_run_recovery = None;
            None
        }
        KeyCode::Up => {
            recovery.selected = recovery.selected.saturating_sub(1);
            None
        }
        KeyCode::Down => {
            let max = recovery_action_count(recovery).saturating_sub(1);
            recovery.selected = (recovery.selected + 1).min(max);
            None
        }
        KeyCode::Enter => accept_recovery_action(app),
        _ => None,
    }
}

pub fn accept_recovery_action(app: &mut App) -> Option<Msg> {
    let recovery = app.first_run_recovery.clone()?;

    match recovery.stage {
        RecoveryStage::ChooseProvider => {
            let provider = SetupProviderArg::ALL
                .get(recovery.selected)
                .copied()
                .unwrap_or_else(|| provider_for_model(&app.model));
            app.first_run_recovery = Some(FirstRunRecovery {
                provider: Some(provider),
                stage: RecoveryStage::ModelConfigScope,
                pending_provider_prompt: false,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: None,
            });
        }
        RecoveryStage::ModelConfigScope => configure_setup_model_scope(app, &recovery),
        RecoveryStage::MissingCredential if recovery.provider == Some(SetupProviderArg::ChatgptCodex) => {
            match recovery.selected {
                0 => start_chatgpt_oauth_recovery(app),
                1 => {
                    app.first_run_recovery = None;
                    open_model_picker(app);
                }
                2 => {
                    if let Some(active) = app.first_run_recovery.as_mut() {
                        active.stage = RecoveryStage::Instructions;
                        active.selected = 0;
                        active.chatgpt_oauth = None;
                    }
                }
                3 => {
                    if recovery.pending_provider_prompt {
                        app.transcript.push(Entry::Status {
                            text: String::from(
                                "setup required before submitting this ChatGPT Codex prompt; start OAuth login or switch model",
                            ),
                        });
                    } else {
                        app.first_run_recovery = None;
                        app.transcript
                            .push(Entry::Status { text: String::from("setup skipped") });
                    }
                }
                4 => {
                    app.quit = true;
                    return Some(Msg::Quit);
                }
                _ => {}
            }
        }
        RecoveryStage::MissingCredential => match recovery.selected {
            0 => {
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::EnterKey;
                    active.selected = 0;
                    active.secret_input.clear();
                }
            }
            1 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if let Some(active) = app.first_run_recovery.as_mut() {
                        active.stage = RecoveryStage::Instructions;
                        active.selected = 0;
                    }
                    return None;
                }
                app.first_run_recovery = None;
                open_model_picker(app);
            }
            2 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    if recovery.pending_provider_prompt {
                        app.transcript.push(Entry::Status {
                            text: String::from(
                                "setup required before submitting this ChatGPT Codex prompt; start ChatGPT OAuth login or switch model",
                            ),
                        });
                    } else {
                        app.first_run_recovery = None;
                        app.transcript
                            .push(Entry::Status { text: String::from("setup skipped") });
                    }
                    return None;
                }
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::Instructions;
                    active.selected = 0;
                }
            }
            3 => {
                if recovery.provider == Some(SetupProviderArg::ChatgptCodex) {
                    app.quit = true;
                    return Some(Msg::Quit);
                }
                if recovery.pending_provider_prompt {
                    app.transcript.push(Entry::Status {
                        text: String::from(
                            "setup required before submitting this provider-backed prompt; enter a key or switch model",
                        ),
                    });
                } else {
                    app.first_run_recovery = None;
                    app.transcript
                        .push(Entry::Status { text: String::from("setup skipped") });
                }
            }
            4 => {
                app.quit = true;
                return Some(Msg::Quit);
            }
            _ => {}
        },
        RecoveryStage::ConfirmStore => store_recovery_credential(app, &recovery),
        RecoveryStage::Instructions => match recovery.selected {
            0 => {
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::MissingCredential;
                    active.selected = 0;
                }
            }
            1 => app.first_run_recovery = None,
            _ => {}
        },
        RecoveryStage::ChatGptOAuthRequesting => {}
        RecoveryStage::ChatGptOAuthPolling => {
            if let Some(active) = app.first_run_recovery.as_mut() {
                active.stage = RecoveryStage::MissingCredential;
                active.selected = 0;
                active.chatgpt_oauth = None;
            }
        }
        RecoveryStage::ChatGptOAuthFailed => match recovery.selected {
            0 => start_chatgpt_oauth_recovery(app),
            1 => {
                if let Some(active) = app.first_run_recovery.as_mut() {
                    active.stage = RecoveryStage::MissingCredential;
                    active.selected = 0;
                    active.chatgpt_oauth = None;
                }
            }
            _ => {}
        },
        RecoveryStage::LogoutConfirm => remove_recovery_credential(app, &recovery),
        RecoveryStage::AcpMissing => match recovery.selected {
            0 => {
                app.first_run_recovery = None;
                open_model_picker(app);
            }
            1 => {
                app.transcript.push(Entry::Status {
                    text: String::from("ACP setup: run `thndrs acp list` or `thndrs acp registry` outside the TUI"),
                });
            }
            2 => {
                if recovery.pending_provider_prompt {
                    app.transcript.push(Entry::Status {
                        text: String::from(
                            "ACP agent config is required before submitting this prompt; switch model or configure ACP",
                        ),
                    });
                } else {
                    app.first_run_recovery = None;
                }
            }
            3 => {
                app.quit = true;
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
        app.first_run_recovery = None;
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
                .push(Entry::Status { text: String::from("model config skipped") });
            advance_after_setup_model_config(app, provider);
        }
        _ => {
            app.first_run_recovery = None;
            app.transcript
                .push(Entry::Status { text: String::from("setup skipped") });
        }
    }
}

pub fn after_setup_model_config(app: &mut App, provider: SetupProviderArg, scope: CredentialScope) {
    if codex::supports_reasoning_effort(&app.model) {
        app.first_run_recovery = None;
        app.pending_setup_reasoning_effort = Some(PendingSetupReasoningEffort { provider, scope });
        open_reasoning_effort_picker(app);
    } else {
        advance_after_setup_model_config(app, provider);
    }
}

pub fn write_setup_model_config(
    app: &mut App, provider: SetupProviderArg, scope: CredentialScope,
) -> std::io::Result<()> {
    let model = provider.default_model();
    let path = match scope {
        CredentialScope::Global => match config::global_config_path() {
            Some(path) => path,
            None => {
                let err = std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not available");
                app.transcript
                    .push(Entry::Error { text: format!("failed to save selected model to global config: {err}") });
                return Err(err);
            }
        },
        CredentialScope::Project => config::project_config_path(&app.cwd),
    };

    match config::write_model_config(&path, model) {
        Ok(()) => {
            app.model = model.to_string();
            app.cli.model = model.to_string();
            let display = match scope {
                CredentialScope::Global => config::global_config_path_display(&path),
                CredentialScope::Project => config::project_config_path_display(&path, &app.cwd),
            };
            app.transcript
                .push(Entry::Status { text: format!("model: {model} (saved to {display})") });
            Ok(())
        }
        Err(err) => {
            app.transcript.push(Entry::Error {
                text: format!("failed to save selected model to {} config: {err}", scope.label()),
            });
            Err(err)
        }
    }
}

pub fn advance_after_setup_model_config(app: &mut App, provider: SetupProviderArg) {
    if provider_authenticated(provider, &app.cwd) {
        app.first_run_recovery = None;
        app.transcript
            .push(Entry::Status { text: format!("setup complete for {}", provider.label()) });
    } else if provider == SetupProviderArg::ChatgptCodex {
        app.first_run_recovery = Some(FirstRunRecovery::missing_provider(provider, false));
    } else {
        app.first_run_recovery = Some(FirstRunRecovery::login(provider));
    }
}

pub fn start_chatgpt_oauth_recovery(app: &mut App) {
    let pending_provider_prompt = app
        .first_run_recovery
        .as_ref()
        .is_some_and(|recovery| recovery.pending_provider_prompt);
    if let Some(active) = app.first_run_recovery.as_mut() {
        active.stage = RecoveryStage::ChatGptOAuthRequesting;
        active.selected = 0;
        active.chatgpt_oauth = None;
    }

    match (app.chatgpt_oauth_driver.request_device_code)() {
        Ok(code) => {
            let next_poll_tick = app
                .ui_tick
                .wrapping_add(seconds_to_ticks(app, code.interval.unwrap_or(5).max(1)));
            let expires_at_tick = app
                .ui_tick
                .wrapping_add(seconds_to_ticks(app, code.expires_in.unwrap_or(900).max(1)));
            app.first_run_recovery = Some(FirstRunRecovery {
                provider: Some(SetupProviderArg::ChatgptCodex),
                stage: RecoveryStage::ChatGptOAuthPolling,
                pending_provider_prompt,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: Some(ChatGptOAuthRecovery {
                    code,
                    next_poll_tick,
                    expires_at_tick,
                    status: String::from("Waiting for ChatGPT authorization."),
                }),
            });
        }
        Err(err) => {
            app.first_run_recovery = Some(FirstRunRecovery {
                provider: Some(SetupProviderArg::ChatgptCodex),
                stage: RecoveryStage::ChatGptOAuthFailed,
                pending_provider_prompt,
                selected: 0,
                secret_input: String::new(),
                chatgpt_oauth: None,
            });
            app.transcript.push(Entry::Error {
                text: format!(
                    "ChatGPT OAuth device-code request failed: {}",
                    redact_auth_error(&err.to_string())
                ),
            });
        }
    }
}

pub fn poll_chatgpt_oauth_on_tick(app: &mut App) {
    let tick_ms = app.cli.tick_rate_ms.max(1);
    let Some(recovery) = app.first_run_recovery.as_mut() else {
        return;
    };
    if recovery.stage != RecoveryStage::ChatGptOAuthPolling {
        return;
    }
    let Some(oauth) = recovery.chatgpt_oauth.as_mut() else {
        recovery.stage = RecoveryStage::ChatGptOAuthFailed;
        return;
    };
    if super::agent_lifecycle::now_or_after_deadline(app.ui_tick, oauth.expires_at_tick) {
        oauth.status = String::from("ChatGPT OAuth device code expired.");
        recovery.stage = RecoveryStage::ChatGptOAuthFailed;
        recovery.selected = 0;
        return;
    }
    if !super::agent_lifecycle::now_or_after_deadline(app.ui_tick, oauth.next_poll_tick) {
        return;
    }

    match (app.chatgpt_oauth_driver.poll_device_code_once)(&oauth.code) {
        Ok(auth::ChatGptCodexDevicePoll::Pending) => {
            oauth.status = String::from("Waiting for ChatGPT authorization.");
            oauth.next_poll_tick = app.ui_tick.wrapping_add(seconds_to_ticks_for_ms(
                tick_ms,
                oauth.code.interval.unwrap_or(5).max(1),
            ));
        }
        Ok(auth::ChatGptCodexDevicePoll::SlowDown) => {
            oauth.status = String::from("Waiting for ChatGPT authorization.");
            oauth.next_poll_tick = app.ui_tick.wrapping_add(seconds_to_ticks_for_ms(
                tick_ms,
                oauth.code.interval.unwrap_or(5).max(1).saturating_add(5),
            ));
        }
        Ok(auth::ChatGptCodexDevicePoll::Authorized(credentials)) => {
            match (app.chatgpt_oauth_driver.write_credentials)(&credentials) {
                Ok(()) => {
                    app.first_run_recovery = None;
                    app.transcript.push(Entry::Status {
                        text: String::from("chatgpt-codex credential stored in global auth store"),
                    });
                }
                Err(err) => {
                    oauth.status = format!(
                        "ChatGPT OAuth credential write failed: {}",
                        redact_auth_error(&err.to_string())
                    );
                    recovery.stage = RecoveryStage::ChatGptOAuthFailed;
                    recovery.selected = 0;
                }
            }
        }
        Err(err) => {
            oauth.status = format!("ChatGPT OAuth polling failed: {}", redact_auth_error(&err.to_string()));
            recovery.stage = RecoveryStage::ChatGptOAuthFailed;
            recovery.selected = 0;
        }
    }
}

pub fn seconds_to_ticks(app: &App, seconds: u64) -> u64 {
    seconds_to_ticks_for_ms(app.cli.tick_rate_ms.max(1), seconds)
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
        app.first_run_recovery = None;
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.first_run_recovery = Some(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        return;
    };

    let key = recovery.secret_input.trim();
    let path = match crate::cli::commands::auth::credential_path(scope, &app.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };

    let Some(env_var) = provider.api_key_env_var() else {
        app.first_run_recovery = Some(FirstRunRecovery::missing_provider(
            provider,
            recovery.pending_provider_prompt,
        ));
        app.transcript
            .push(Entry::Error { text: String::from("ChatGPT Codex uses OAuth login, not API-key storage") });
        return;
    };
    match auth::set_credential(&path, env_var, key) {
        Ok(()) => {
            if scope == CredentialScope::Project {
                if let Err(err) = auth::ensure_git_exclude(&app.cwd) {
                    app.transcript
                        .push(Entry::Error { text: format!("git exclude update failed: {err}") });
                }
            }
            app.transcript
                .push(Entry::Status { text: format!("{} credential stored in {}", provider.label(), scope.label()) });
            app.first_run_recovery = None;
        }
        Err(err) => app
            .transcript
            .push(Entry::Error { text: format!("credential write failed: {err}") }),
    }
}

pub fn remove_recovery_credential(app: &mut App, recovery: &FirstRunRecovery) {
    let Some(provider) = recovery.provider else {
        app.first_run_recovery = None;
        return;
    };
    let Some(scope) = selected_scope(recovery.selected) else {
        app.first_run_recovery = None;
        app.transcript
            .push(Entry::Status { text: String::from("logout cancelled") });
        return;
    };
    let path = match crate::cli::commands::auth::credential_path(scope, &app.cwd) {
        Ok(path) => path,
        Err(err) => {
            app.transcript
                .push(Entry::Error { text: format!("credential store unavailable: {err}") });
            return;
        }
    };
    let Some(env_var) = provider.api_key_env_var() else {
        app.first_run_recovery = None;
        app.transcript
            .push(Entry::Error { text: String::from("ChatGPT Codex credentials are stored in ~/.thndrs/auth.json") });
        return;
    };
    match auth::remove_credential(&path, env_var) {
        Ok(()) => {
            app.first_run_recovery = None;
            app.transcript.push(Entry::Status {
                text: format!("{} credential removed from {}", provider.label(), scope.label()),
            });
        }
        Err(err) => app
            .transcript
            .push(Entry::Error { text: format!("credential remove failed: {err}") }),
    }
}
