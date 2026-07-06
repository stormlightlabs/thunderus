use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn slash_clear_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.transcript.push(Entry::User { text: "keep me".to_string() });
    app.input = PromptInput::from("/clear");

    let result = update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(result, None, "/clear should not execute while working");
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::User { text } if text == "keep me")),
        "transcript should not be cleared while an agent can still emit events"
    );
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/clear should be rejected with a status message"
    );
    assert!(app.input.is_empty(), "input should be cleared after /clear");
}

#[test]
fn slash_help_while_working_executes_immediately() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("/help");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.prompt_accessory,
        PromptAccessory::Help,
        "/help should open help while working"
    );
    assert!(app.input.is_empty(), "input should be cleared after /help");
}

#[test]
fn slash_model_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("/model");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.prompt_accessory,
        PromptAccessory::None,
        "/model should not open picker while working"
    );
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/model should be rejected with a status message"
    );
}

#[test]
fn slash_skills_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("/skills");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.prompt_accessory,
        PromptAccessory::None,
        "/skills should not open picker while working"
    );
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/skills should be rejected with a status message"
    );
}

#[test]
fn slash_unknown_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("/unknown");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.queued_followups.is_empty(),
        "unknown slash command should not be queued as text"
    );
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "unknown slash command should be rejected with a status message"
    );
}

#[test]
fn double_slash_while_working_queues_literal_slash_followup() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("//clear after this run");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.queued_followups,
        vec!["/clear after this run".to_string()],
        "double slash should escape a literal slash-prefixed follow-up"
    );
    assert!(app.input.is_empty(), "input should be cleared after queueing");
}

#[test]
fn slash_clear_clears_transcript_and_input() {
    let mut app = fresh_app();
    app.transcript.push(Entry::User { text: String::from("old") });
    app.input = PromptInput::from("/clear");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.transcript.is_empty());
    assert_eq!(app.input.as_str(), "");
    assert!(!app.quit);
}

#[test]
fn slash_auth_config_and_doctor_append_redacted_output() {
    let mut app = fresh_app();

    app.input = PromptInput::from("/auth status");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.transcript.last(), Some(Entry::Status { text }) if text.contains("umans")));

    app.input = PromptInput::from("/config path");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(app.transcript.last(), Some(Entry::Status { text }) if text.contains("global:") && text.contains("project:"))
    );

    app.input = PromptInput::from("/config show");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(app.transcript.last(), Some(Entry::Status { text }) if text.contains("effective_config:")));

    app.input = PromptInput::from("/doctor");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let transcript = format!("{:?}", app.transcript);
    assert!(transcript.contains("thndrs doctor"));
    assert!(!transcript.contains("test-umans-key"));
}

#[test]
fn slash_config_edit_reports_cli_only() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/config edit");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(
        app.transcript.last(),
        Some(Entry::Status { text }) if text.contains("config edit is CLI-only")
    ));
}

#[test]
fn slash_command_rejects_api_key_like_extra_argument() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/login umans sk-secret-should-not-appear");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.first_run_recovery.is_none());
    let transcript = format!("{:?}", app.transcript);
    assert!(transcript.contains("do not accept API keys"));
    assert!(!transcript.contains("sk-secret-should-not-appear"));

    let mut app = fresh_app();
    app.input = PromptInput::from("/login chatgpt-codex access_token=secret-should-not-appear");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.first_run_recovery.is_none());
    let transcript = format!("{:?}", app.transcript);
    assert!(transcript.contains("do not accept API keys"));
    assert!(!transcript.contains("secret-should-not-appear"));
}

#[test]
fn slash_logout_requires_confirmation_surface() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/logout umans");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.first_run_recovery.as_ref().expect("logout recovery");
    assert_eq!(recovery.stage, RecoveryStage::LogoutConfirm);
    assert_eq!(recovery.provider, Some(SetupProviderArg::Umans));
}

#[test]
fn slash_chatgpt_codex_logout_stays_cli_only() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/logout chatgpt-codex");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.first_run_recovery.is_none());
    assert!(matches!(
        app.transcript.last(),
        Some(Entry::Status { text }) if text.contains("ChatGPT Codex logout is CLI-only")
    ));
}

#[test]
fn slash_setup_and_login_open_recovery_surfaces() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/setup");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.first_run_recovery.as_ref().map(|recovery| recovery.stage),
        Some(RecoveryStage::MissingCredential)
    ));

    app.first_run_recovery = None;
    app.input = PromptInput::from("/login opencode-go");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let recovery = app.first_run_recovery.as_ref().expect("login recovery");
    assert_eq!(recovery.stage, RecoveryStage::EnterKey);
    assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeGo));
}

#[test]
fn slash_setup_uses_chatgpt_provider_aware_recovery_for_chatgpt_model() {
    let mut app = fresh_app();
    app.model = "chatgpt-codex/gpt-5.5".to_string();
    app.cli.model = app.model.clone();
    app.input = PromptInput::from("/setup");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.first_run_recovery.as_ref().expect("setup recovery");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
}

#[test]
fn slash_chatgpt_codex_login_opens_oauth_recovery_surface() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/login chatgpt-codex");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.first_run_recovery.as_ref().expect("login recovery");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
}

#[test]
fn slash_chatgpt_codex_login_surface_starts_tui_oauth() {
    let mut app = fresh_app();
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
    };
    app.input = PromptInput::from("/login chatgpt-codex");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.first_run_recovery.as_ref().expect("oauth recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthPolling);
    assert!(recovery.chatgpt_oauth.is_some());
}

#[test]
fn slash_quit_sets_quit_flag() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/quit");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
    assert_eq!(follow, Some(Msg::Quit));
    assert_eq!(app.input.as_str(), "");
}

#[test]
fn slash_exit_also_quits() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/exit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
}

#[test]
fn unknown_slash_command_is_ignored() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/bogus");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(!app.quit);
    assert!(app.transcript.is_empty());
    assert_eq!(app.input.as_str(), "/bogus");
}

#[test]
fn slash_mcp_lists_empty_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = fresh_app();
    app.cwd = temp.path().to_path_buf();
    app.input = PromptInput::from("/mcp");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(
        matches!(app.transcript.last(), Some(Entry::Status { text }) if text.contains("no MCP servers configured"))
    );
}

#[test]
fn slash_mcp_tools_requires_name() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/mcp tools ");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(matches!(app.transcript.last(), Some(Entry::Error { text }) if text.contains("usage: /mcp tools <name>")));
}
