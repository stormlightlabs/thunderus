//! Application behavior tests for setup seams.

use super::*;
use helpers::*;

#[test]
fn missing_provider_credential_opens_recovery_and_preserves_prompt() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.composer.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.composer.input.as_str(), "hello");
        assert!(app.transcript.entries.is_empty());
        let recovery = app.overlay.setup().expect("recovery");
        assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
    });
}

#[test]
fn startup_setup_can_be_skipped_but_submitted_draft_returns_to_composer() {
    let home = tempfile::tempdir().expect("create temp home");
    with_setup_home(home.path(), || {
        let cli = Cli { cwd: home.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;

        assert!(!app.overlay.setup().expect("startup setup").pending_provider_prompt);
        for _ in 0..3 {
            update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.overlay.setup().is_none(), "startup setup can be skipped");

        app.composer.input = PromptInput::from("keep this draft");
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.overlay.setup().expect("submit recovery").pending_provider_prompt);

        for _ in 0..3 {
            update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        }
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.overlay.setup().is_none(), "return to draft closes setup");
        assert_eq!(app.composer.input.as_str(), "keep this draft");
        assert!(app.transcript.entries.iter().any(|entry| matches!(
            entry,
            Entry::Status { text } if text.contains("draft is preserved")
        )));
    });
}

#[test]
fn opencode_setup_cancellation_keeps_prompt_draft_and_discards_secret_buffer() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft while setting up OpenCode Go");
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    for ch in "sk-cancelled-key".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));

    let recovery = app.overlay.setup().expect("OpenCode recovery remains available");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert!(recovery.secret_input.is_empty());
    assert_eq!(app.composer.input.as_str(), "draft while setting up OpenCode Go");
    assert!(!format!("{app:?}").contains("sk-cancelled-key"));
}

#[test]
fn opencode_provider_failure_is_actionable_and_restores_prompt_draft() {
    with_provider_env_removed(|| {
        let mut app = fresh_app();
        app.runtime.model = "opencode/big-pickle".to_string();
        app.runtime.cli.model = app.runtime.model.clone();
        app.overlay.close();
        let prompt = "make the bounded OpenCode change";

        assert_eq!(
            submit_user_turn(&mut app, prompt.to_string()),
            Some(Msg::Agent(AgentEvent::Started))
        );
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::Failed(
                "OpenCode Zen authentication failed (HTTP 401); check OPENCODE_ZEN_KEY or run `thndrs login opencode-go`"
                    .to_string(),
            )),
        );

        assert_eq!(
            app.runtime.run_state,
            RunState::Error(
                "OpenCode Zen authentication failed (HTTP 401); check OPENCODE_ZEN_KEY or run `thndrs login opencode-go`"
                    .to_string()
            )
        );
        assert_eq!(app.composer.input.as_str(), prompt);
        let recovery = app.overlay.setup().expect("sign-in recovery opens");
        assert_eq!(recovery.intent, RecoveryIntent::Reauthenticate);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
        assert_eq!(recovery.stage, RecoveryStage::EnterKey);
        assert!(app.transcript.entries.iter().any(
            |entry| matches!(entry, Entry::Status { text } if text.contains("opened sign-in recovery for opencode-zen"))
        ));
    });
}

#[test]
fn chatgpt_provider_failure_opens_browser_reauthentication_and_restores_prompt_draft() {
    with_provider_env_removed(|| {
        let mut app = fresh_app();
        app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
        app.runtime.cli.model = app.runtime.model.clone();
        app.overlay.close();
        let prompt = "continue the bounded ChatGPT change";

        assert_eq!(
            submit_user_turn(&mut app, prompt.to_string()),
            Some(Msg::Agent(AgentEvent::Started))
        );
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::Failed(
                "authentication failed (HTTP 401): ChatGPT Codex credential rejected".to_string(),
            )),
        );

        assert_eq!(app.composer.input.as_str(), prompt);
        let recovery = app.overlay.setup().expect("sign-in recovery opens");
        assert_eq!(recovery.intent, RecoveryIntent::Reauthenticate);
        assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
        assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
        assert!(recovery.pending_provider_prompt);
    });
}

#[test]
fn rejected_environment_credential_opens_restart_recovery_and_preserves_draft() {
    with_provider_env_removed(|| {
        unsafe {
            std::env::set_var(auth::OPENCODE_ZEN_KEY_ENV, "rejected-environment-key");
        }
        let mut app = fresh_app();
        app.runtime.model = "opencode/big-pickle".to_string();
        app.runtime.cli.model = app.runtime.model.clone();
        app.overlay.close();
        let prompt = "make the bounded OpenCode change";

        assert_eq!(
            submit_user_turn(&mut app, prompt.to_string()),
            Some(Msg::Agent(AgentEvent::Started))
        );
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(
            &mut app,
            &Msg::Agent(AgentEvent::Failed(
                "OpenCode Zen authentication failed (HTTP 403); check OPENCODE_ZEN_KEY or run `thndrs login opencode-go`"
                    .to_string(),
            )),
        );

        assert_eq!(app.composer.input.as_str(), prompt);
        let recovery = app.overlay.setup().expect("restart recovery opens");
        assert_eq!(recovery.intent, RecoveryIntent::Reauthenticate);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
        assert_eq!(recovery.stage, RecoveryStage::EnvironmentCredentialRejected);
        assert!(app.transcript.entries.iter().any(|entry| {
            matches!(entry, Entry::Status { text } if text.contains("OPENCODE_ZEN_KEY was rejected") && text.contains("restart thndrs"))
        }));
        assert!(!format!("{app:?}").contains("rejected-environment-key"));
    });
}

#[test]
fn rejected_credential_failure_is_persisted_before_opening_login_recovery() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.overlay.close();
        let session_path = app
            .session
            .writer
            .as_ref()
            .expect("session writer")
            .path()
            .to_path_buf();

        update(
            &mut app,
            &Msg::Agent(AgentEvent::Failed(
                "OpenCode Zen authentication failed (HTTP 401); check OPENCODE_ZEN_KEY or run `thndrs login opencode-go`"
                    .to_string(),
            )),
        );

        let records = session::SessionReader::read_records(&session_path);
        assert!(records.iter().any(|record| {
            matches!(record, session::SessionRecord::Failed { error, .. } if error.contains("authentication failed"))
        }));
        let recovery = app.overlay.setup().expect("sign-in recovery opens");
        assert_eq!(recovery.intent, RecoveryIntent::Reauthenticate);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
        assert_eq!(recovery.stage, RecoveryStage::EnterKey);
    });
}

#[test]
fn fresh_setup_authenticates_before_model_selection_and_retains_draft() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");

    with_setup_home(&home, || {
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("create workspace");
        let cli = Cli { cwd: workspace, ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.composer.input = PromptInput::from("draft before setup");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.runtime.model, "");
        assert_eq!(
            app.overlay.setup().map(|recovery| recovery.stage),
            Some(RecoveryStage::MissingCredential)
        );

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        for ch in "test-opencode-key".chars() {
            update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.runtime.model, "");
        assert_eq!(
            app.overlay.setup().map(|recovery| recovery.stage),
            Some(RecoveryStage::ModelSelection)
        );
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.runtime.model, "opencode/big-pickle");
        assert_eq!(app.composer.input.as_str(), "draft before setup");
    });
}

#[test]
fn chatgpt_submit_uses_stored_auth_without_recovery_refresh() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("create temp dir");
        let home = dir.path().join("home");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&home).expect("create home");
        std::fs::create_dir_all(&workspace).expect("create workspace");
        let old_home = std::env::var_os("HOME");
        unsafe {
            std::env::set_var("HOME", &home);
        }

        auth::write_chatgpt_codex_credentials(&auth::ChatGptCodexCredentials {
            access_token: "expired-access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            expires_at_ms: 0,
            account_id: "acct_test".to_string(),
        })
        .expect("write stored ChatGPT credentials");

        let cli = Cli { cwd: workspace, model: "chatgpt-codex/gpt-5.5".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.composer.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(
            app.overlay.setup().is_none(),
            "stored ChatGPT credentials should pass the local setup gate"
        );
        assert_eq!(app.transcript.entries, vec![Entry::User { text: "hello".to_string() }]);

        unsafe {
            if let Some(old_home) = old_home {
                std::env::set_var("HOME", old_home);
            } else {
                std::env::remove_var("HOME");
            }
        }
    });
}

#[test]
fn acp_missing_config_uses_acp_recovery_not_provider_key_setup() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), model: "acp:missing".to_string(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.composer.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let recovery = app.overlay.setup().expect("recovery");
        assert_eq!(recovery.stage, RecoveryStage::AcpMissing);
        assert_eq!(recovery.provider, None);
    });
}

#[test]
fn recovery_enter_key_stores_project_credential_without_transcript_secret() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay
            .show_setup(FirstRunRecovery::login(SetupProviderArg::OpencodeGo));

        for ch in "sk-secret-from-test".chars() {
            update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert!(!format!("{app:?}").contains("sk-secret-from-test"));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let stored = auth::read_credentials(&auth::project_credentials_path(dir.path())).expect("read credentials");
        assert_eq!(
            stored.get(auth::OPENCODE_GO_KEY_ENV).map(String::as_str),
            Some("sk-secret-from-test")
        );
        let transcript = format!("{:?}", app.transcript.entries);
        assert!(!transcript.contains("sk-secret-from-test"));
    });
}

#[test]
fn recovery_actions_handle_switch_instructions_continue_and_quit() {
    let mut app = fresh_app();
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.overlay.setup().is_none());
    assert_eq!(app.overlay.accessory(), PromptAccessory::Models);

    app.overlay.close();
    app.overlay.close();
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, true));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::Instructions)
    );

    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, true));
    for _ in 0..3 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.overlay.setup().is_none(),
        "pending provider prompts can return to their preserved draft"
    );
    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Status { text } if text.contains("draft is preserved")
    )));

    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, false));
    for _ in 0..3 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.overlay.setup().is_none(),
        "manual setup can be skipped without submitting a prompt"
    );

    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::OpencodeGo, false));
    for _ in 0..4 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    let follow = update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.runtime.quit);
    assert_eq!(follow, Some(Msg::Quit));
}

#[test]
fn chatgpt_recovery_action_order_starts_oauth_before_switching_model() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let recovery = app.overlay.setup().expect("oauth recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthPolling);
    assert!(recovery.chatgpt_oauth.is_some());

    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.overlay.setup().is_none());
    assert_eq!(app.overlay.accessory(), PromptAccessory::Models);
}

#[test]
fn chatgpt_browser_login_is_default_and_supports_pasted_redirect_recovery() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        start_browser_login: oauth_browser_start,
        open_browser: oauth_browser_open,
        poll_browser_login: oauth_browser_pending,
        complete_browser_redirect: oauth_browser_complete,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let recovery = app.overlay.setup().expect("browser recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthPolling);
    assert_eq!(
        recovery.chatgpt_oauth.as_ref().map(|oauth| oauth.method),
        Some(ChatGptOAuthMethod::Browser)
    );
    assert!(
        recovery
            .chatgpt_oauth
            .as_ref()
            .and_then(|oauth| oauth.authorization_url.as_ref())
            .is_some()
    );

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthPasteRedirect)
    );

    for ch in "http://localhost:1455/auth/callback?code=auth-code&state=state".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.setup().is_none());
    assert!(format!("{:?}", app.transcript.entries).contains("credential stored"));
    assert!(!format!("{:?}", app.transcript.entries).contains("auth-code"));
}

#[test]
fn chatgpt_recovery_cannot_enter_api_key_input() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    for _ in 0..5 {
        update(&mut app, &key(KeyCode::Char('s'), KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert_ne!(
            app.overlay.setup().map(|recovery| recovery.stage),
            Some(RecoveryStage::EnterKey)
        );
        if app.overlay.setup().is_none() {
            app.overlay
                .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));
        }
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
}

#[test]
fn chatgpt_oauth_poll_pending_preserves_prompt_without_transcript_tokens() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft prompt");
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);

    assert_eq!(app.composer.input.as_str(), "draft prompt");
    assert_eq!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthPolling)
    );
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(!transcript.contains("device-token-secret-from-test"));
    assert!(!transcript.contains("access-token-secret-from-test"));
    assert!(!transcript.contains("refresh-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_slowdown_updates_status_and_backoff() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_slow_down,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let next_poll_before = app
        .overlay
        .setup()
        .as_ref()
        .and_then(|recovery| recovery.chatgpt_oauth.as_ref())
        .expect("oauth state")
        .next_poll_tick;
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);

    let oauth = app
        .overlay
        .setup()
        .as_ref()
        .and_then(|recovery| recovery.chatgpt_oauth.as_ref())
        .expect("oauth state");
    assert_eq!(oauth.status, "ChatGPT asked the client to slow down; waiting.");
    assert!(oauth.next_poll_tick > next_poll_before);
}

#[test]
fn chatgpt_oauth_poll_success_stores_credentials_and_preserves_prompt() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft prompt");
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_authorized,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);

    assert!(app.overlay.setup().is_none());
    assert_eq!(app.composer.input.as_str(), "draft prompt");
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(transcript.contains("credential stored"));
    assert!(!transcript.contains("access-token-secret-from-test"));
    assert!(!transcript.contains("refresh-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_expiry_and_write_failure_keep_recovery_available() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_authorized,
        write_credentials: oauth_write_fail,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .expires_at_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);
    assert_eq!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthFailed)
    );
    assert!(format!("{:?}", app.transcript.entries).contains("expired"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);
    let recovery = app.overlay.setup().expect("failed recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthFailed);
    assert!(
        recovery
            .chatgpt_oauth
            .as_ref()
            .is_some_and(|oauth| oauth.status.contains("credential write failed"))
    );
}

#[test]
fn chatgpt_oauth_failures_are_redacted_and_keep_recovery_path() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_fail,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthFailed)
    );
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(transcript.contains("[redacted]"));
    assert!(!transcript.contains("device-token-secret-from-test"));

    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_fail,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.overlay
        .setup_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.runtime.ui_tick;
    update(&mut app, &Msg::Tick);
    let recovery = app.overlay.setup().expect("failed recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthFailed);
    let recovery_debug = format!("{recovery:?}");
    assert!(!recovery_debug.contains("access-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_escape_cancels_without_writing_credentials() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft prompt");
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_authorized,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));
    update(&mut app, &Msg::Tick);

    let recovery = app.overlay.setup().expect("recovery remains");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert!(recovery.chatgpt_oauth.is_none());
    assert_eq!(app.composer.input.as_str(), "draft prompt");
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn chatgpt_browser_oauth_escape_cancels_without_writing_credentials() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        start_browser_login: oauth_browser_start,
        open_browser: oauth_browser_open,
        poll_browser_login: oauth_browser_pending,
        complete_browser_redirect: oauth_browser_complete,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.overlay
        .show_setup(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));

    let recovery = app.overlay.setup().expect("recovery remains");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert!(recovery.chatgpt_oauth.is_none());
    assert!(app.overlay.browser_login().is_none());
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn offline_model_picker_includes_provider_expansion_models() {
    let items = offline_model_picker_items();

    for model in [
        "opencode/big-pickle",
        "opencode/gpt-5.6-sol",
        "opencode/gpt-5.6-luna",
        "opencode-go/deepseek-v4-flash",
    ] {
        assert!(
            items.iter().any(|item| item.label == model),
            "missing OpenCode model {model}"
        );
    }
    assert!(!items.iter().any(|item| item.label == "umans-glm-5.1"));
    assert!(items.iter().any(|item| item.label == "opencode/big-pickle"));
    assert!(items.iter().any(|item| item.label == "chatgpt-codex/gpt-5.5"));
}

#[test]
fn accepting_model_picker_selection_saves_project_config() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.session.writer = None;
    let _ = app.overlay.show_picker(
        PromptAccessory::Models,
        PickerState::new(
            vec![PickerItem::new("chatgpt-codex/gpt-5.5", "ChatGPT-backed Codex")],
            MODEL_PICKER_LIMIT,
        ),
    );

    accept_model_suggestion(&mut app);

    assert_eq!(app.runtime.model, "chatgpt-codex/gpt-5.5");
    assert_eq!(app.runtime.cli.model, "chatgpt-codex/gpt-5.5");
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".thndrs").join("config.toml")).expect("read project config"),
        "model = \"chatgpt-codex/gpt-5.5\"\n"
    );
    assert_eq!(
        app.transcript.entries.last(),
        Some(&Entry::Status { text: "model: chatgpt-codex/gpt-5.5 (saved to .thndrs/config.toml)".to_string() })
    );
}
