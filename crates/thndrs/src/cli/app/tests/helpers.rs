use super::*;
use crate::skills::{SkillMetadata, SkillSource};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

pub fn with_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
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

pub fn with_provider_env_removed<T>(f: impl FnOnce() -> T) -> T {
    let _guard = crate::test_env::lock();
    let old_umans = std::env::var_os(auth::UMANS_API_KEY_ENV);
    let old_opencode = std::env::var_os(auth::OPENCODE_GO_KEY_ENV);
    let old_opencode_zen = std::env::var_os(auth::OPENCODE_ZEN_KEY_ENV);
    let old_chatgpt = std::env::var_os(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);

    unsafe {
        std::env::remove_var(auth::UMANS_API_KEY_ENV);
        std::env::remove_var(auth::OPENCODE_GO_KEY_ENV);
        std::env::remove_var(auth::OPENCODE_ZEN_KEY_ENV);
        std::env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
    }

    let result = f();

    unsafe {
        if let Some(value) = old_umans {
            std::env::set_var(auth::UMANS_API_KEY_ENV, value);
        } else {
            std::env::remove_var(auth::UMANS_API_KEY_ENV);
        }
        if let Some(value) = old_opencode {
            std::env::set_var(auth::OPENCODE_GO_KEY_ENV, value);
        } else {
            std::env::remove_var(auth::OPENCODE_GO_KEY_ENV);
        }
        if let Some(value) = old_opencode_zen {
            std::env::set_var(auth::OPENCODE_ZEN_KEY_ENV, value);
        } else {
            std::env::remove_var(auth::OPENCODE_ZEN_KEY_ENV);
        }
        if let Some(value) = old_chatgpt {
            std::env::set_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV, value);
        } else {
            std::env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        }
    }

    result
}

pub fn with_setup_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = crate::test_env::lock();
    let old_home = std::env::var_os("HOME");
    let old_umans = std::env::var_os(auth::UMANS_API_KEY_ENV);
    let old_opencode = std::env::var_os(auth::OPENCODE_GO_KEY_ENV);
    let old_opencode_zen = std::env::var_os(auth::OPENCODE_ZEN_KEY_ENV);
    let old_chatgpt = std::env::var_os(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);

    unsafe {
        std::env::set_var("HOME", home);
        std::env::remove_var(auth::UMANS_API_KEY_ENV);
        std::env::remove_var(auth::OPENCODE_GO_KEY_ENV);
        std::env::remove_var(auth::OPENCODE_ZEN_KEY_ENV);
        std::env::remove_var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
    }

    let result = f();

    unsafe {
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        for (name, value) in [
            (auth::UMANS_API_KEY_ENV, old_umans),
            (auth::OPENCODE_GO_KEY_ENV, old_opencode),
            (auth::OPENCODE_ZEN_KEY_ENV, old_opencode_zen),
            (auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV, old_chatgpt),
        ] {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }

    result
}

pub fn key(code: KeyCode, modifiers: KeyModifiers) -> Msg {
    Msg::Key(KeyEvent::new(code, modifiers))
}

pub fn fresh_app() -> App {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cwd = dir.path().to_path_buf();
    auth::set_credential(
        &auth::project_credentials_path(&cwd),
        auth::UMANS_API_KEY_ENV,
        "test-umans-key",
    )
    .expect("seed test credential");
    auth::set_credential(
        &auth::project_credentials_path(&cwd),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed test Zen credential");
    let _kept = dir.keep();
    let cli = Cli { cwd, model: "umans-coder".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.session_writer = None;
    app.first_run_recovery = None;
    app
}

pub fn picker_from_paths(paths: Vec<String>) -> PickerState {
    PickerState::new(
        paths.into_iter().map(|path| PickerItem::new(path, "")).collect(),
        LARGE_PICKER_LIMIT,
    )
}

pub fn test_skill(path: std::path::PathBuf, markdown: &str) -> SkillMetadata {
    std::fs::create_dir_all(path.parent().expect("skill parent")).expect("create skill dir");
    std::fs::write(&path, markdown).expect("write skill");
    SkillMetadata {
        name: "example-skill".to_string(),
        description: "Helps test the skills picker.".to_string(),
        root: path.parent().expect("skill root").to_path_buf(),
        path,
        content_hash: tools::hash_content(markdown),
        byte_count: markdown.len(),
        source: SkillSource::Project,
        allowed_tools: Vec::new(),
        license: None,
        compatibility: None,
        metadata: None,
        references: Vec::new(),
    }
}

pub fn test_chatgpt_device_code() -> auth::ChatGptCodexDeviceCode {
    auth::ChatGptCodexDeviceCode {
        device_auth_id: "device-auth-secret-from-test".to_string(),
        user_code: "USER-CODE".to_string(),
        verification_uri: Some("https://auth.example.test/device".to_string()),
        verification_uri_complete: None,
        expires_in: Some(900),
        interval: Some(1),
    }
}

pub fn test_chatgpt_credentials() -> auth::ChatGptCodexCredentials {
    auth::ChatGptCodexCredentials {
        access_token: "access-token-secret-from-test".to_string(),
        refresh_token: "refresh-token-secret-from-test".to_string(),
        expires_at_ms: 999_999,
        account_id: "acct_test".to_string(),
    }
}

pub fn oauth_request_ok() -> Result<auth::ChatGptCodexDeviceCode, auth::AuthError> {
    Ok(test_chatgpt_device_code())
}

pub fn oauth_request_fail() -> Result<auth::ChatGptCodexDeviceCode, auth::AuthError> {
    Err(auth::AuthError::ChatGptCodex(
        "device_auth_id device-auth-secret-from-test unavailable".to_string(),
    ))
}

pub fn oauth_poll_pending(_: &auth::ChatGptCodexDeviceCode) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError> {
    Ok(auth::ChatGptCodexDevicePoll::Pending)
}

pub fn oauth_poll_slow_down(_: &auth::ChatGptCodexDeviceCode) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError> {
    Ok(auth::ChatGptCodexDevicePoll::SlowDown)
}

pub fn oauth_poll_authorized(
    _: &auth::ChatGptCodexDeviceCode,
) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError> {
    Ok(auth::ChatGptCodexDevicePoll::Authorized(test_chatgpt_credentials()))
}

pub fn oauth_poll_fail(_: &auth::ChatGptCodexDeviceCode) -> Result<auth::ChatGptCodexDevicePoll, auth::AuthError> {
    Err(auth::AuthError::ChatGptCodex(
        "access_token access-token-secret-from-test rejected".to_string(),
    ))
}

pub fn oauth_write_ok(_: &auth::ChatGptCodexCredentials) -> Result<(), auth::AuthError> {
    Ok(())
}

pub fn oauth_write_fail(_: &auth::ChatGptCodexCredentials) -> Result<(), auth::AuthError> {
    Err(auth::AuthError::ChatGptCodex(String::from(
        "credential store unavailable",
    )))
}

pub fn oauth_browser_start() -> Result<auth::ChatGptCodexBrowserLogin, auth::AuthError> {
    Ok(auth::test_chatgpt_codex_browser_login())
}

pub fn oauth_browser_open(_: &str) -> Result<(), auth::AuthError> {
    Ok(())
}

pub fn oauth_browser_pending(
    _: &mut auth::ChatGptCodexBrowserLogin,
) -> Result<auth::ChatGptCodexBrowserPoll, auth::AuthError> {
    Ok(auth::ChatGptCodexBrowserPoll::Pending)
}

pub fn oauth_browser_complete(
    _: &auth::ChatGptCodexBrowserLogin, _: &str,
) -> Result<auth::ChatGptCodexCredentials, auth::AuthError> {
    Ok(test_chatgpt_credentials())
}

pub fn working_app_with_streaming() -> App {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.transcript.push(Entry::User { text: "do the thing".to_string() });
    app.transcript
        .push(Entry::Agent { text: "working on it".to_string(), streaming: true });
    app
}

pub fn pending_permission(tx: mpsc::Sender<PermissionDecision>) -> PendingPermission {
    PendingPermission {
        tool_call_id: "call_1".to_string(),
        title: "Edit file".to_string(),
        options: vec![
            PermissionOptionView {
                id: "reject".to_string(),
                name: "Reject".to_string(),
                kind: PermissionKindView::RejectOnce,
            },
            PermissionOptionView {
                id: "allow".to_string(),
                name: "Allow".to_string(),
                kind: PermissionKindView::AllowOnce,
            },
        ],
        selected: 0,
        responder: tx,
    }
}
