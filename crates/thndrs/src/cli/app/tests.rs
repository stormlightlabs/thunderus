mod commands;
mod helpers;
mod input;
mod labels;
mod movement;
mod prompts;
mod slash;

use super::*;
use crate::acp::permissions::{PendingPermission, PermissionDecision, PermissionKindView, PermissionOptionView};
use crate::config::{Config, ConfigOrigin, ConfigSource, LoadedConfigLayer};
use crate::context::{AGENTS_MD_SIZE_CAP, ContextSource, discover_workspace_root};
use crate::harness::HarnessTurn;
use crate::input::PromptInput;
use crate::renderer;
use crate::tools::AgentRunConfig;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;
use std::sync::mpsc;
use thndrs_agent::CancelToken;

use helpers::*;

#[test]
fn from_cli_starts_with_fresh_transcript_not_latest_session() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = session::sessions_dir(dir.path());
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-old",
        "/repo",
        "old",
        "umans",
        "umans-coder",
        "none",
        "0.1.0",
        None,
    )
    .expect("create old session");
    writer
        .append_entry(
            &Entry::User { text: String::from("old message should not replay") },
            "turn_1",
        )
        .expect("append old entry");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert!(
        !app.transcript
            .iter()
            .any(|e| matches!(e, Entry::User { text } if text.contains("old message")))
    );
}

#[test]
fn fresh_startup_records_context_without_memory_state() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);
    let _: &[ContextSource] = &app.context_sources;

    let writer = app.session_writer.as_ref().expect("session writer");
    let records = session::SessionReader::read_records(writer.path());
    let Some(session::SessionRecord::SessionMeta { config: Some(config), .. }) = records.first() else {
        panic!("fresh session metadata record");
    };
    assert!(config.files.is_empty());
}

#[test]
fn from_cli_seeds_up_arrow_history_from_project_sessions() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = session::sessions_dir(dir.path());
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-old",
        "/repo",
        "old",
        "umans",
        "umans-coder",
        "none",
        "0.1.0",
        None,
    )
    .expect("create old session");
    writer
        .append_entry(&Entry::User { text: String::from("first project prompt") }, "turn_1")
        .expect("append first entry");
    writer
        .append_entry(&Entry::User { text: String::from("second project prompt") }, "turn_2")
        .expect("append second entry");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));

    assert_eq!(app.input.as_str(), "second project prompt");
    assert!(
        app.transcript.is_empty(),
        "project history should seed recall without replaying old transcript"
    );
    assert!(
        InputHistoryStore::for_workspace(dir.path())
            .load_recent()
            .expect("load dedicated history")
            .is_some(),
        "legacy session prompts should seed the dedicated history once"
    );
}

#[test]
fn from_cli_prefers_dedicated_input_history_over_session_scan() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = session::sessions_dir(dir.path());
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-old",
        "/repo",
        "old",
        "umans",
        "umans-coder",
        "none",
        "0.1.0",
        None,
    )
    .expect("create old session");
    writer
        .append_entry(&Entry::User { text: "session-derived prompt".to_string() }, "turn_1")
        .expect("append old entry");
    InputHistoryStore::for_workspace(dir.path())
        .append("history-session", "dedicated prompt")
        .expect("append dedicated history");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.input_history, vec!["dedicated prompt".to_string()]);
}

#[test]
fn from_cli_writes_effective_config_metadata_to_session_meta() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let session_dir = dir.path().join("custom-sessions");
    let mut origins = std::collections::BTreeMap::new();
    origins.insert(
        "model".to_string(),
        ConfigOrigin { source: ConfigSource::Environment, detail: "THNDRS_MODEL".to_string() },
    );
    origins.insert(
        "websearch".to_string(),
        ConfigOrigin { source: ConfigSource::ProjectFile, detail: ".thndrs/config.toml".to_string() },
    );

    let cli = Cli {
        cwd: dir.path().to_path_buf(),
        model: "env-model".to_string(),
        websearch: crate::cli::WebSearchMode::Native,
        session_dir: Some(session_dir.clone()),
        config_layers: vec![LoadedConfigLayer {
            source: ConfigSource::ProjectFile,
            config: Config::default(),
            path: None,
            display_path: Some(".thndrs/config.toml".to_string()),
            hash: Some("abc123".to_string()),
        }],
        config_origins: origins,
        ..Cli::default()
    };

    let app = App::from_cli(&cli);
    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    let records = session::SessionReader::read_records(&path);

    let session_dir_display = session_dir.display().to_string();
    let session::SessionRecord::SessionMeta { cwd, model, websearch, config, .. } = &records[0] else {
        panic!("expected first record to be session_meta");
    };
    let config = config.as_ref().expect("config metadata");

    let workspace_root = discover_workspace_root(dir.path());
    assert_eq!(cwd, &workspace_root.display().to_string());
    assert_eq!(model, "env-model");
    assert_eq!(websearch, "native");
    assert_eq!(config.session_dir.as_deref(), Some(session_dir_display.as_str()));
    assert_eq!(config.files[0].path, ".thndrs/config.toml");
    assert_eq!(config.files[0].source, "project");
    assert_eq!(config.files[0].sha256, "abc123");
    assert_eq!(
        config.origins.get("model").map(String::as_str),
        Some("env:THNDRS_MODEL")
    );
}

#[test]
fn from_cli_writes_mcp_config_metadata_to_session_meta() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(dir.path().join(".thndrs")).expect("create thndrs dir");
    std::fs::write(
        dir.path().join(".thndrs").join("mcp.toml"),
        r#"
        [servers.docs]
        command = "docs-mcp"
        "#,
    )
    .expect("write mcp config");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = helpers::with_home(&home, || App::from_cli(&cli));
    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    let records = session::SessionReader::read_records(&path);

    let session::SessionRecord::SessionMeta { config, .. } = &records[0] else {
        panic!("expected first record to be session_meta");
    };
    let config = config.as_ref().expect("config metadata");
    assert_eq!(config.mcp_files.len(), 1);
    assert_eq!(config.mcp_files[0].path, ".thndrs/mcp.toml");
    assert_eq!(config.mcp_files[0].source, "project");
    assert!(!config.mcp_files[0].sha256.is_empty());
}

#[test]
fn submit_user_turn_records_mcp_config_change_before_user() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(dir.path().join(".thndrs")).expect("create thndrs dir");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::UMANS_API_KEY_ENV,
        "test-umans-key",
    )
    .expect("seed test credential");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed test Zen credential");
    let mcp_path = dir.path().join(".thndrs").join("mcp.toml");
    std::fs::write(
        &mcp_path,
        r#"
        [servers.docs]
        command = "docs-mcp"
        "#,
    )
    .expect("write mcp config");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = with_home(&home, || App::from_cli(&cli));
    let previous_hash = app.mcp_config_files[0].sha256.clone();
    std::fs::write(
        &mcp_path,
        r#"
        [servers.docs]
        command = "docs-mcp-v2"
        "#,
    )
    .expect("update mcp config");

    with_home(&home, || {
        submit_user_turn(&mut app, "hello".to_string()).expect("agent start message");
    });

    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text.starts_with("MCP config changed:")))
    );

    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    let records = session::SessionReader::read_records(&path);

    assert!(
        records
            .iter()
            .any(|record| matches!(record, session::SessionRecord::User { text, .. } if text == "hello"))
    );
    let change = records
        .iter()
        .find_map(|record| match record {
            session::SessionRecord::McpConfigChanged { previous_files, current_files, .. } => {
                Some((previous_files, current_files))
            }
            _ => None,
        })
        .expect("mcp config change record");
    assert_eq!(change.0[0].sha256, previous_hash);
    assert_ne!(change.1[0].sha256, previous_hash);
}

#[test]
fn usage_event_accumulates_session_tokens() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Usage { input_tokens: 12, output_tokens: 3 }),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Usage { input_tokens: 5, output_tokens: 7 }),
    );
    assert_eq!(app.session_tokens_in, 17);
    assert_eq!(app.session_tokens_out, 10);
}

#[test]
fn finished_persists_final_assistant_even_after_status_row() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();

    app.input = PromptInput::from("update TODO.md");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("Done."))));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Status(String::from("provider: stream ended"))),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let records = session::SessionReader::read_records(&path);
    assert!(records.iter().any(|record| matches!(
        record,
        session::SessionRecord::AssistantFinished { text, .. } if text == "Done."
    )));
}

#[test]
fn compact_uses_provider_summary_and_replaces_active_context_only_after_success() {
    let mut app = fresh_app();
    app.transcript = vec![
        Entry::User { text: "inspect the parser".to_string() },
        Entry::Agent { text: "the parser rejects empty input".to_string(), streaming: false },
    ];

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(app.pending_manual_compaction.is_some());
    assert!(matches!(app.transcript.last(), Some(Entry::User { text }) if text.contains("Summarize")));

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(
            "parser: empty input is rejected".to_string(),
        )),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(app.pending_manual_compaction.is_none());
    assert!(matches!(app.transcript.first(), Some(Entry::User { text }) if text == "inspect the parser"));
    assert!(
        app.transcript
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize")))
    );
    assert!(
        matches!(app.transcript.last(), Some(Entry::Agent { text, .. }) if text == "parser: empty input is rejected")
    );
}

#[test]
fn failed_compaction_restores_active_context_without_restoring_internal_prompt() {
    let mut app = fresh_app();
    let original = vec![Entry::User { text: "inspect the parser".to_string() }];
    app.transcript = original.clone();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Failed("provider unavailable".to_string())),
    );

    assert!(matches!(app.transcript.first(), Some(Entry::User { text }) if text == "inspect the parser"));
    assert!(
        app.transcript
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize")))
    );
    assert!(app.input.is_empty());
}

#[test]
fn compact_writes_a_recoverable_manual_audit_record() {
    let dir = tempfile::tempdir().expect("create temp dir");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed credential");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    app.transcript = vec![Entry::User { text: "inspect the parser".to_string() }];

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta("parser summary".to_string())),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let records = session::SessionReader::read_records(&path);
    assert!(records.iter().any(|record| matches!(
        record,
        session::SessionRecord::Compaction { audit, .. }
            if audit.trigger == session::CompactionTrigger::Manual
                && audit.summary == "parser summary"
                && audit.model == cli.model
                && audit.recovery_handles.len() == 1
    )));
}

#[test]
fn risky_compaction_waits_for_review_and_preserves_context_until_approval() {
    let mut app = fresh_app();
    let original = vec![
        Entry::User { text: "inspect the parser".to_string() },
        Entry::Tool {
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["error details".to_string()],
        },
    ];
    app.transcript = original.clone();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta("reviewable summary".to_string())),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(app.pending_compaction_review.is_some());
    assert_eq!(
        app.last_compaction_review,
        Some(session::CompactionReviewResult::Pending)
    );
    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Status { text } if text.contains("review pending")
    )));

    app.input = PromptInput::from("/context review approve");
    handle_command(&mut app, "context review approve");
    assert!(app.pending_compaction_review.is_none());
    assert_eq!(
        app.last_compaction_review,
        Some(session::CompactionReviewResult::Approved)
    );
    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Agent { text, streaming: false } if text == "reviewable summary"
    )));
}

#[test]
fn auto_compaction_restarts_the_user_turn_after_success() {
    let mut app = fresh_app();
    app.transcript = vec![
        Entry::User { text: "long conversation".to_string() },
        Entry::Agent { text: "lots of detail".to_string(), streaming: false },
    ];
    let original_turn = "continue the work".to_string();

    assert_eq!(
        start_auto_compaction(&mut app, original_turn.clone()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(app.compaction_in_flight());
    assert!(matches!(app.transcript.last(), Some(Entry::User { text }) if text.contains("Summarize")));

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta("compacted summary".to_string())),
    );
    let restart = update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(!app.compaction_in_flight());
    assert!(matches!(app.transcript.last(), Some(Entry::User { text }) if *text == original_turn));
    assert_eq!(restart, Some(Msg::Agent(AgentEvent::Started)));
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Agent { text, .. } if text == "compacted summary"))
    );
    assert!(
        !app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "long conversation"))
    );
}

#[test]
fn auto_compaction_restart_waits_for_followups_until_turn_completes() {
    let mut app = fresh_app();
    app.transcript = vec![Entry::User { text: "long conversation".to_string() }];
    app.queued_followups.push("follow-up after restart".to_string());
    let original_turn = "continue the work".to_string();

    assert_eq!(
        start_auto_compaction(&mut app, original_turn.clone()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta("summary".to_string())));

    let restart = update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(restart, Some(Msg::Agent(AgentEvent::Started)));
    assert!(matches!(app.transcript.last(), Some(Entry::User { text }) if *text == original_turn));
    assert_eq!(
        app.queued_followups.len(),
        1,
        "follow-up must wait until the restarted turn completes"
    );
}

#[test]
fn auto_compaction_failure_preserves_the_submitted_turn() {
    let mut app = fresh_app();
    app.transcript = vec![Entry::User { text: "long conversation".to_string() }];
    let original_turn = "continue the work".to_string();

    assert_eq!(
        start_auto_compaction(&mut app, original_turn.clone()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(app.compaction_in_flight());

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let result = update(
        &mut app,
        &Msg::Agent(AgentEvent::Failed("provider unavailable".to_string())),
    );

    assert!(!app.compaction_in_flight());
    assert_eq!(result, None);
    assert_eq!(app.last_input, Some(original_turn));
    assert!(matches!(app.transcript.first(), Some(Entry::User { text }) if text == "long conversation"));
    assert!(
        !app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text.contains("Summarize")))
    );
}

#[test]
fn auto_compaction_writes_an_automatic_trigger_audit_record() {
    let dir = tempfile::tempdir().expect("create temp dir");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed credential");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session_writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    app.transcript = vec![Entry::User { text: "long conversation".to_string() }];

    assert_eq!(
        start_auto_compaction(&mut app, "continue".to_string()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta("auto summary".to_string())),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let records = session::SessionReader::read_records(&path);
    assert!(records.iter().any(|record| matches!(
        record,
        session::SessionRecord::Compaction { audit, .. }
            if audit.trigger == session::CompactionTrigger::Automatic
                && audit.summary == "auto summary"
    )));
}

#[test]
fn ctrl_c_sets_quit_flag() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    );
    assert!(app.quit);
}

#[test]
fn ctrl_c_cancels_running_stream() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    );

    assert!(!app.quit, "Ctrl+C while running should not quit immediately");
    assert_eq!(app.run_state, RunState::Stopping);
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text == "cancelled"))
    );
}

#[test]
fn ctrl_d_first_press_shows_confirmation() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.quit, "first Ctrl+D should not quit");
    assert!(app.ctrl_d_pending.is_some(), "should arm pending confirmation");
    assert!(
        app.transcript.iter().any(|e| matches!(
            e,
            Entry::Status { text } if text.contains("Press CTRL+D again to quit")
        )),
        "should show confirmation message"
    );
}

#[test]
fn ctrl_d_second_press_quits() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.quit);
    assert!(app.ctrl_d_pending.is_some());

    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.quit, "second Ctrl+D should quit");
    assert!(app.ctrl_d_pending.is_none(), "pending should be cleared on quit");
    assert_eq!(follow, Some(Msg::Quit));
}

#[test]
fn ctrl_d_timeout_expires_and_requires_double_press_again() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.ctrl_d_pending.is_some());

    for _ in 0..QUIT_CONFIRM_TIMEOUT_TICKS + 1 {
        update(&mut app, &Msg::Tick);
    }
    assert!(app.ctrl_d_pending.is_none(), "pending should expire after timeout");

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.quit, "expired second press should not quit");
    assert!(app.ctrl_d_pending.is_some(), "should arm a fresh confirmation");
}

#[test]
fn ctrl_d_cancelled_by_other_key() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.ctrl_d_pending.is_some());

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(app.ctrl_d_pending.is_none(), "other key should cancel pending Ctrl+D");

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.quit, "should not quit after cancellation");
    assert!(app.ctrl_d_pending.is_some());
}

#[test]
fn other_keys_do_not_quit() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(!app.quit);
    update(&mut app, &Msg::Tick);
    assert!(!app.quit);
}

#[test]
fn file_picker_selection_inserts_selected_path() {
    let mut app = fresh_app();
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);
    app.picker = Some(picker_from_paths(vec![
        "src/main.rs".to_string(),
        "src/app.rs".to_string(),
    ]));

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.input.as_str(), "src/app.rs");
    assert!(app.picker.is_none());
}

#[test]
fn file_picker_arrows_and_pages_are_scrollable() {
    let mut app = fresh_app();
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);
    app.picker = Some(picker_from_paths(
        (0..20).map(|i| format!("src/file_{i:02}.rs")).collect(),
    ));

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
    );
    let picker = app.picker.as_ref().expect("picker");
    assert_eq!(picker.selected, VISIBLE_ROWS);
    assert!(picker.scroll > 0);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)));
    let picker = app.picker.as_ref().expect("picker");
    assert_eq!(picker.selected, 0);
    assert_eq!(picker.scroll, 0);
}

#[test]
fn tick_increments_ui_tick() {
    let mut app = fresh_app();
    assert_eq!(app.ui_tick, 0);
    update(&mut app, &Msg::Tick);
    assert_eq!(app.ui_tick, 1);
}

#[test]
fn quit_message_sets_quit_flag() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Quit);
    assert!(app.quit);
}

#[test]
fn backspace_removes_last_char() {
    let mut app = fresh_app();
    app.input = PromptInput::from("abc");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.input.as_str(), "ab");
}

#[test]
fn enter_trims_whitespace_before_submit() {
    let mut app = fresh_app();
    app.input = PromptInput::from("  hello  ");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input.as_str(), "");
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(app.transcript[0], Entry::User { text: String::from("hello") });
}

#[test]
fn missing_provider_credential_opens_recovery_and_preserves_prompt() {
    with_provider_env_removed(|| {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session_writer = None;
        app.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.input.as_str(), "hello");
        assert!(app.transcript.is_empty());
        let recovery = app.first_run_recovery.as_ref().expect("recovery");
        assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
        assert!(recovery.pending_provider_prompt);
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
        app.session_writer = None;
        app.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(
            app.first_run_recovery.is_none(),
            "stored ChatGPT credentials should pass the local setup gate"
        );
        assert_eq!(app.transcript, vec![Entry::User { text: "hello".to_string() }]);

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
        app.session_writer = None;
        app.input = PromptInput::from("hello");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let recovery = app.first_run_recovery.as_ref().expect("recovery");
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
        app.session_writer = None;
        app.first_run_recovery = Some(FirstRunRecovery::login(SetupProviderArg::Umans));

        for ch in "sk-secret-from-test".chars() {
            update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let stored = auth::read_credentials(&auth::project_credentials_path(dir.path())).expect("read credentials");
        assert_eq!(
            stored.get(auth::UMANS_API_KEY_ENV).map(String::as_str),
            Some("sk-secret-from-test")
        );
        let transcript = format!("{:?}", app.transcript);
        assert!(!transcript.contains("sk-secret-from-test"));
    });
}

#[test]
fn recovery_actions_handle_switch_instructions_continue_and_quit() {
    let mut app = fresh_app();
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::Umans, true));

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.first_run_recovery.is_none());
    assert_eq!(app.prompt_accessory, PromptAccessory::Models);

    app.prompt_accessory = PromptAccessory::None;
    app.picker = None;
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::Umans, true));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(
        app.first_run_recovery.as_ref().map(|recovery| recovery.stage),
        Some(RecoveryStage::Instructions)
    );

    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::Umans, true));
    for _ in 0..3 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.first_run_recovery.is_some(),
        "pending provider prompts cannot continue without setup"
    );
    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Status { text } if text.contains("setup required before submitting")
    )));

    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::Umans, false));
    for _ in 0..3 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.first_run_recovery.is_none(),
        "manual setup can be skipped without submitting a prompt"
    );

    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::Umans, false));
    for _ in 0..4 {
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
    let follow = update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.quit);
    assert_eq!(follow, Some(Msg::Quit));
}

#[test]
fn chatgpt_recovery_action_order_starts_oauth_before_switching_model() {
    let mut app = fresh_app();
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let recovery = app.first_run_recovery.as_ref().expect("oauth recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthPolling);
    assert!(recovery.chatgpt_oauth.is_some());

    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.first_run_recovery.is_none());
    assert_eq!(app.prompt_accessory, PromptAccessory::Models);
}

#[test]
fn chatgpt_recovery_cannot_enter_api_key_input() {
    let mut app = fresh_app();
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    for _ in 0..5 {
        update(&mut app, &key(KeyCode::Char('s'), KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        assert_ne!(
            app.first_run_recovery.as_ref().map(|recovery| recovery.stage),
            Some(RecoveryStage::EnterKey)
        );
        if app.first_run_recovery.is_none() {
            app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));
        }
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    }
}

#[test]
fn chatgpt_oauth_poll_pending_preserves_prompt_without_transcript_tokens() {
    let mut app = fresh_app();
    app.input = PromptInput::from("draft prompt");
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.first_run_recovery
        .as_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.ui_tick;
    update(&mut app, &Msg::Tick);

    assert_eq!(app.input.as_str(), "draft prompt");
    assert_eq!(
        app.first_run_recovery.as_ref().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthPolling)
    );
    let transcript = format!("{:?}", app.transcript);
    assert!(!transcript.contains("device-token-secret-from-test"));
    assert!(!transcript.contains("access-token-secret-from-test"));
    assert!(!transcript.contains("refresh-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_poll_success_stores_credentials_and_preserves_prompt() {
    let mut app = fresh_app();
    app.input = PromptInput::from("draft prompt");
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_authorized,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.first_run_recovery
        .as_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.ui_tick;
    update(&mut app, &Msg::Tick);

    assert!(app.first_run_recovery.is_none());
    assert_eq!(app.input.as_str(), "draft prompt");
    let transcript = format!("{:?}", app.transcript);
    assert!(transcript.contains("credential stored"));
    assert!(!transcript.contains("access-token-secret-from-test"));
    assert!(!transcript.contains("refresh-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_failures_are_redacted_and_keep_recovery_path() {
    let mut app = fresh_app();
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_fail,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.first_run_recovery.as_ref().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChatGptOAuthFailed)
    );
    let transcript = format!("{:?}", app.transcript);
    assert!(transcript.contains("[redacted]"));
    assert!(!transcript.contains("device-token-secret-from-test"));

    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_fail,
        write_credentials: oauth_write_ok,
    };
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.first_run_recovery
        .as_mut()
        .and_then(|recovery| recovery.chatgpt_oauth.as_mut())
        .expect("oauth state")
        .next_poll_tick = app.ui_tick;
    update(&mut app, &Msg::Tick);
    let recovery = app.first_run_recovery.as_ref().expect("failed recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthFailed);
    let recovery_debug = format!("{recovery:?}");
    assert!(!recovery_debug.contains("access-token-secret-from-test"));
}

#[test]
fn chatgpt_oauth_escape_cancels_without_writing_credentials() {
    let mut app = fresh_app();
    app.input = PromptInput::from("draft prompt");
    app.chatgpt_oauth_driver = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_authorized,
        write_credentials: oauth_write_ok,
    };
    app.first_run_recovery = Some(FirstRunRecovery::missing_provider(SetupProviderArg::ChatgptCodex, true));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));
    update(&mut app, &Msg::Tick);

    let recovery = app.first_run_recovery.as_ref().expect("recovery remains");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert!(recovery.chatgpt_oauth.is_none());
    assert_eq!(app.input.as_str(), "draft prompt");
    assert!(app.transcript.is_empty());
}

#[test]
fn offline_model_picker_includes_provider_expansion_models() {
    let items = offline_model_picker_items();

    assert!(items.iter().any(|item| item.label == "opencode/big-pickle"));
    assert!(items.iter().any(|item| item.label == "chatgpt-codex/gpt-5.5"));
}

#[test]
fn accepting_model_picker_selection_saves_project_config() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.session_writer = None;
    app.picker = Some(PickerState::new(
        vec![PickerItem::new("chatgpt-codex/gpt-5.5", "ChatGPT-backed Codex")],
        MODEL_PICKER_LIMIT,
    ));
    app.prompt_accessory = PromptAccessory::Models;

    accept_model_suggestion(&mut app);

    assert_eq!(app.model, "chatgpt-codex/gpt-5.5");
    assert_eq!(app.cli.model, "chatgpt-codex/gpt-5.5");
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".thndrs").join("config.toml")).expect("read project config"),
        "model = \"chatgpt-codex/gpt-5.5\"\n"
    );
    assert_eq!(
        app.transcript.last(),
        Some(&Entry::Status { text: "model: chatgpt-codex/gpt-5.5 (saved to .thndrs/config.toml)".to_string() })
    );
}

#[test]
fn msg_clear_clears_transcript() {
    let mut app = fresh_app();
    app.transcript.push(Entry::User { text: String::from("a") });
    app.transcript.push(Entry::User { text: String::from("b") });
    update(&mut app, &Msg::Clear);
    assert!(app.transcript.is_empty());
}

#[test]
fn agent_started_sets_working() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    assert_eq!(app.run_state, RunState::Working);
}

#[test]
fn assistant_delta_creates_streaming_entry() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello"))));
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0],
        Entry::Agent { text: String::from("Hello"), streaming: true }
    );
}

#[test]
fn assistant_delta_appends_to_existing_streaming_entry() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello "))),
    );
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("world"))));
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0],
        Entry::Agent { text: String::from("Hello world"), streaming: true }
    );
}

#[test]
fn assistant_delta_creates_new_entry_after_finished() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("first"))));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("second"))),
    );
    assert_eq!(app.transcript.len(), 2);
    assert_eq!(
        app.transcript[0],
        Entry::Agent { text: String::from("first"), streaming: false }
    );
    assert_eq!(
        app.transcript[1],
        Entry::Agent { text: String::from("second"), streaming: true }
    );
}

#[test]
fn reasoning_delta_creates_streaming_entry() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Thinking..."))),
    );
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0],
        Entry::Reasoning { text: String::from("Thinking..."), streaming: true }
    );
}

#[test]
fn reasoning_delta_appends_to_existing_streaming_entry() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 1. "))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Step 2."))),
    );
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0],
        Entry::Reasoning { text: String::from("Step 1. Step 2."), streaming: true }
    );
}

#[test]
fn assistant_delta_finishes_prior_reasoning_spinner() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Thinking..."))),
    );
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("Done."))));

    assert_eq!(
        app.transcript,
        vec![
            Entry::Reasoning { text: String::from("Thinking..."), streaming: false },
            Entry::Agent { text: String::from("Done."), streaming: true },
        ]
    );
}

#[test]
fn tool_started_finishes_prior_reasoning_spinner() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("Thinking..."))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );

    assert!(matches!(
        app.transcript.first(),
        Some(Entry::Reasoning { streaming: false, .. })
    ));
    assert!(matches!(
        app.transcript.last(),
        Some(Entry::Tool { status: ToolStatus::Running, .. })
    ));
}

#[test]
fn tool_started_creates_running_tool_entry() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    assert_eq!(app.transcript.len(), 1);
    match &app.transcript[0] {
        Entry::Tool { name, arguments, status, output } => {
            assert_eq!(name, "read_file#0");
            assert_eq!(arguments, "{}");
            assert_eq!(*status, ToolStatus::Running);
            assert!(output.is_empty());
        }
        _ => panic!("expected Tool entry"),
    }
}

#[test]
fn tool_finished_sets_output_and_status() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolFinished {
            id: String::from("0"),
            output: vec![String::from("line 1"), String::from("line 2")],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        }),
    );
    match &app.transcript[0] {
        Entry::Tool { status, output, .. } => {
            assert_eq!(*status, ToolStatus::Ok);
            assert_eq!(*output, vec!["line 1", "line 2"]);
        }
        _ => panic!("expected Tool entry"),
    }
}

#[test]
fn tool_finished_marks_failed_status() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolFinished {
            id: String::from("0"),
            output: Vec::new(),
            status: ToolStatus::Failed,
            write_result: None,
            shell_result: None,
        }),
    );
    match &app.transcript[0] {
        Entry::Tool { status, .. } => assert_eq!(*status, ToolStatus::Failed),
        _ => panic!("expected Tool entry"),
    }
}

#[test]
fn cancelled_event_adds_status_and_returns_to_idle() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
    );
    assert_eq!(app.run_state, RunState::Working);

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.run_state, RunState::Idle);
    assert!(matches!(app.transcript.last(), Some(Entry::Status { text }) if text == "cancelled"));

    match &app.transcript[0] {
        Entry::Agent { streaming, .. } => assert!(!*streaming),
        _ => panic!("expected Assistant entry"),
    }
}

#[test]
fn finished_marks_streaming_false_and_returns_to_idle() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("text"))));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("thoughts"))),
    );

    assert_eq!(app.run_state, RunState::Working);
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(app.run_state, RunState::Idle);

    if let Entry::Agent { streaming, .. } = &app.transcript[0] {
        assert!(!*streaming);
    } else {
        panic!("expected Assistant entry");
    }

    match &app.transcript[1] {
        Entry::Reasoning { streaming, .. } => assert!(!*streaming),
        _ => panic!("expected Reasoning entry"),
    }
}

#[test]
fn failed_adds_error_entry_and_sets_error_state() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
    );
    assert_eq!(app.run_state, RunState::Working);

    update(
        &mut app,
        &Msg::Agent(AgentEvent::Failed(String::from("connection lost"))),
    );
    assert_eq!(app.run_state, RunState::Error("connection lost".to_string()));
    assert!(matches!(app.transcript.last(), Some(Entry::Error { text }) if text == "connection lost"));

    match &app.transcript[0] {
        Entry::Agent { streaming, .. } => assert!(!*streaming),
        _ => panic!("expected Assistant entry"),
    }
}

#[test]
fn escape_cancels_working_stream() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
    );
    assert_eq!(app.run_state, RunState::Working);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.run_state, RunState::Stopping);

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.run_state, RunState::Idle);

    match &app.transcript[0] {
        Entry::Agent { streaming, .. } => assert!(!*streaming),
        _ => panic!("expected Assistant entry"),
    }
}

#[test]
fn escape_does_nothing_when_idle() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.run_state, RunState::Idle);
    assert!(app.transcript.is_empty());
}

#[test]
fn submit_while_working_queues_followup_by_default() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.input = PromptInput::from("queued message");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.input.is_empty());
    assert_eq!(app.queued_followups, vec!["queued message".to_string()]);
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued follow-up")))
    );
}

#[test]
fn ctrl_t_toggles_running_queue_target() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    assert_eq!(app.queue_target, QueueTarget::FollowUp);

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.queue_target, QueueTarget::Steering);

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.queue_target, QueueTarget::FollowUp);
}

#[test]
fn submit_while_working_queues_steering_when_selected() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.queue_target = QueueTarget::Steering;
    app.input = PromptInput::from("look at tests first");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.input.is_empty());
    assert_eq!(app.queued_steering, vec!["look at tests first".to_string()]);
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued steering")))
    );
}

#[test]
fn finished_starts_next_followup_turn() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.queued_followups.push("next task".to_string());

    let next = update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert_eq!(next, Some(Msg::Agent(AgentEvent::Started)));
    assert!(app.queued_followups.is_empty());
    assert_eq!(app.turn_count, 1);
    assert!(matches!(app.transcript.last(), Some(Entry::User { text }) if text == "next task"));
}

#[test]
fn cancelled_clears_queued_steering_but_keeps_followups() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.queued_steering.push("steer".to_string());
    app.queued_followups.push("after".to_string());

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));

    assert!(app.queued_steering.is_empty());
    assert_eq!(app.queued_followups, vec!["after".to_string()]);
}

#[test]
fn submit_kicks_off_agent_via_followup() {
    let mut app = fresh_app();
    app.input = PromptInput::from("explain this repo");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input.as_str(), "");
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(follow, Some(Msg::Agent(AgentEvent::Started)));
}

#[test]
fn app_without_agents_md_has_no_context_sources() {
    let app = fresh_app();
    assert!(app.context_sources.is_empty());
    assert!(app.transcript.is_empty());
}

#[test]
fn app_with_agents_md_loads_context_and_adds_status() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let agents_path = dir.path().join("AGENTS.md");
    let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
    f.write_all(b"# Project\n\nBuild with cargo.\n")
        .expect("write AGENTS.md");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.context_sources.len(), 1);
    let source = &app.context_sources[0];
    assert_eq!(
        source.path,
        agents_path.canonicalize().unwrap_or_else(|_| agents_path.to_path_buf())
    );
    assert_eq!(source.scope, ".");
    assert!(!source.truncated);
    assert!(source.content.contains("# Project"));
    assert!(
        app.transcript.is_empty(),
        "transcript should be empty at startup; context is shown in the banner"
    );
}

#[test]
fn app_with_oversized_agents_md_marks_truncation() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let big_content = "x".repeat(AGENTS_MD_SIZE_CAP + 1000);
    let agents_path = dir.path().join("AGENTS.md");
    let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
    f.write_all(big_content.as_bytes()).expect("write AGENTS.md");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.context_sources.len(), 1);
    let source = &app.context_sources[0];
    assert!(source.truncated);
    assert!(source.content.len() <= AGENTS_MD_SIZE_CAP);

    assert!(
        app.transcript.is_empty(),
        "transcript should be empty at startup; context is shown in the banner"
    );
}

#[test]
fn context_sources_are_guidance_not_permission() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let content = "# Instructions\n\nModel: gpt-4\nAllow: rm -rf\n";
    let mut f = std::fs::File::create(dir.path().join("AGENTS.md")).expect("create");
    f.write_all(content.as_bytes()).expect("write");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.model, "opencode/big-pickle");
    assert!(app.context_sources[0].content.contains("Model: gpt-4"));
}

#[test]
fn stopping_state_after_escape() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.run_state, RunState::Stopping);
    assert_eq!(app.status_label(), "stopping");
    assert_eq!(app.prompt_state(), PromptState::Stopped);
}

#[test]
fn stopping_transitions_to_idle_on_cancelled_event() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.run_state, RunState::Stopping);
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.run_state, RunState::Idle);
}

#[test]
fn error_state_all_resubmission() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.run_state, RunState::Error("boom".to_string()));
    app.input = PromptInput::from("retry");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(follow.is_some());
    if let Some(msg) = follow {
        update(&mut app, &msg);
    }
    assert_eq!(app.run_state, RunState::Working);
}

#[test]
fn typing_after_recalled_history_edits_copy() {
    let mut app = fresh_app();
    submit_user_turn(&mut app, String::from("previous"));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)));
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE)),
    );

    assert_eq!(app.input.as_str(), "previous!");
    assert_eq!(app.input_history, vec![String::from("previous")]);
    assert_eq!(app.history_cursor, None);
}

#[test]
fn remembering_input_keeps_bounded_in_memory_history() {
    let mut app = fresh_app();
    app.input_history = (0..INPUT_HISTORY_LIMIT)
        .map(|index| format!("prompt {index}"))
        .collect();

    remember_input(&mut app, "newest prompt");

    assert_eq!(app.input_history.len(), INPUT_HISTORY_LIMIT);
    assert_eq!(app.input_history.first().map(String::as_str), Some("prompt 1"));
    assert_eq!(app.input_history.last().map(String::as_str), Some("newest prompt"));
}

#[test]
fn question_key_enters_help_mode() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::Help);
}

#[test]
fn esc_exits_help_mode() {
    let mut app = fresh_app();
    app.prompt_accessory = PromptAccessory::Help;
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
}

#[test]
fn question_key_keeps_inline_help_open() {
    let mut app = fresh_app();
    app.prompt_accessory = PromptAccessory::Help;
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::Help);
}

#[test]
fn question_key_does_not_enter_help_when_input_nonempty() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.input.as_str(), "hello?");
}

#[test]
fn background_shell_result_registers_in_process_registry() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));

    let shell_result = tools::shell::ProcessResult {
        command: vec!["sleep".to_string(), "10".to_string()],
        cwd: std::path::PathBuf::from("."),
        status: tools::shell::ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec!["background task done".to_string()],
        stderr: vec![],
        elapsed: std::time::Duration::from_millis(100),
        kind: tools::shell::ProcessKind::Background,
    };

    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolFinished {
            id: String::from("toolu_bg"),
            output: vec!["background task done".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: Some(Box::new(shell_result)),
        }),
    );

    assert_eq!(
        app.process_registry.background_count(),
        1,
        "background process should be registered"
    );
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("background process"))),
        "should add a background process status entry"
    );
}

#[test]
fn quit_cancels_all_background_processes() {
    let mut app = fresh_app();
    let cancel = CancelToken::new();
    app.process_registry.register(
        vec!["sleep".to_string(), "30".to_string()],
        std::path::PathBuf::from("."),
        tools::shell::ProcessKind::Background,
        cancel.clone(),
    );
    assert_eq!(app.process_registry.background_count(), 1);

    update(&mut app, &Msg::Quit);
    assert!(app.quit);
    assert!(cancel.is_cancelled(), "cancel_all should signal cancellation");
}

#[test]
fn ctrl_k_kills_to_end_of_line() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello world");
    app.input.cursor_to_start();
    app.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.input.as_str(), "hello ");
    assert_eq!(app.kill_ring, vec!["world"]);
}

#[test]
fn ctrl_u_kills_to_start_of_line() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello world");
    app.input.cursor_to_start();
    app.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.input.as_str(), "world");
    assert_eq!(app.kill_ring, vec!["hello "]);
}

#[test]
fn ctrl_w_kills_previous_word() {
    let mut app = fresh_app();
    app.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.input.as_str(), "foo bar ");
    assert_eq!(app.kill_ring, vec!["baz"]);
}

#[test]
fn ctrl_y_yanks_last_kill() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello ");
    app.kill_ring.push("world".to_string());
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.input.as_str(), "hello world");
}

#[test]
fn ctrl_t_transposes_chars() {
    let mut app = fresh_app();
    app.input = PromptInput::from("ab");
    app.input.cursor_to_start();

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.input.as_str(), "ba");
}

#[test]
fn alt_d_kills_next_word() {
    let mut app = fresh_app();
    app.input = PromptInput::from("foo bar baz");
    app.input.cursor_to_start();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT)),
    );
    assert_eq!(app.input.as_str(), "bar baz");
    assert_eq!(app.kill_ring, vec!["foo "]);
}

#[test]
fn alt_backspace_kills_previous_word() {
    let mut app = fresh_app();
    app.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
    );
    assert_eq!(app.input.as_str(), "foo bar ");
    assert_eq!(app.kill_ring, vec!["baz"]);
}

#[test]
fn retrying_provider_discards_partial_output_without_restoring_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello world");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("thinking"))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Retrying {
            attempt: 1,
            max_attempts: 4,
            delay_ms: 2500,
            error: String::from("server error (HTTP 503): unavailable"),
        }),
    );

    assert!(
        app.input.is_empty(),
        "retry should keep the submitted input out of the editor"
    );
    assert_eq!(app.last_input, Some("hello world".to_string()));
    assert_eq!(app.run_state, RunState::Working);
    assert!(
        !app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Agent { .. } | Entry::Reasoning { .. })),
        "partial output from the failed attempt should be removed before retrying"
    );
    assert!(matches!(
        app.transcript.last(),
        Some(Entry::Status { text }) if text.contains("retrying provider request (1/4)")
    ));
}

#[test]
fn tui_update_path_handles_fake_provider_turn() {
    let mut app = fresh_app();
    std::fs::write(app.cwd.join("Cargo.toml"), "[package]\nname = \"fake\"\n").expect("write fake Cargo.toml");
    app.websearch = WebSearchMode::None;
    app.input = PromptInput::from("inspect project");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    if let Some(message) = follow {
        update(&mut app, &message);
    }

    let config = AgentRunConfig::new(app.cwd.clone(), String::from("fake-agent"), WebSearchMode::None);
    let handle = HarnessTurn::fake(config, String::from("inspect project")).start();
    while let Ok(event) = handle.events.recv() {
        update(&mut app, &Msg::Agent(event));
    }

    assert_eq!(app.run_state, RunState::Idle);
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "inspect project"))
    );
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Reasoning { text, streaming: false } if !text.is_empty()))
    );
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Agent { text, streaming: false } if !text.is_empty()))
    );
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Tool { status: ToolStatus::Ok, .. }))
    );
}

#[test]
fn shift_enter_inserts_newline() {
    let mut app = fresh_app();
    app.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(app.input.as_str(), "line1\n");
    assert_eq!(app.input.cursor(), 6);
}

#[test]
fn ctrl_j_inserts_newline() {
    let mut app = fresh_app();
    app.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.input.as_str(), "line1\n");
}

#[test]
fn delete_key_deletes_forward() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "ello");
    assert_eq!(app.input.cursor(), 0);
}

#[test]
fn backspace_deletes_before_cursor() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "hell");
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn typing_inserts_at_cursor_not_at_end() {
    let mut app = fresh_app();
    app.input = PromptInput::from("helo");
    app.input.cursor_left();
    update(&mut app, &key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "hello");
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn at_token_opens_file_picker_and_accepts_mention() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.cwd.join("readme.md"), "readme");

    for ch in "inspect @read".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    assert_eq!(
        app.prompt_accessory,
        PromptAccessory::Files(FilePickerSource::Mention { token_start: 8 })
    );
    let picker = app.picker.as_ref().expect("file picker");
    assert!(picker.matches.iter().any(|item| item.label == "readme.md"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.input.as_str(), "inspect @readme.md ");
}

#[test]
fn model_metadata_event_updates_model_picker_items() {
    let mut app = fresh_app();
    handle_agent_event(
        &mut app,
        AgentEvent::ModelMetadataLoaded(vec![(
            "umans-test".to_string(),
            "provider · ctx 1M · out 32k · tools · reasoning".to_string(),
        )]),
    );

    open_model_picker(&mut app);

    let picker = app.picker.as_ref().expect("model picker");
    assert_eq!(picker.matches[0].label, "umans-test");
    assert_eq!(
        picker.matches[0].detail,
        "provider · ctx 1M · out 32k · tools · reasoning"
    );
}

#[test]
fn backspace_while_streaming_deletes_char() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "hell", "backspace should work while streaming");
}

#[test]
fn history_recall_while_streaming_works() {
    let mut app = working_app_with_streaming();
    app.input_history.push("previous prompt".to_string());
    app.input = PromptInput::from("current draft");

    update(&mut app, &key(KeyCode::Up, KeyModifiers::NONE));

    assert_eq!(
        app.input.as_str(),
        "previous prompt",
        "Up should recall history while streaming"
    );
}

#[test]
fn file_mention_activation_while_working() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("check @src");

    update(&mut app, &key(KeyCode::Char('r'), KeyModifiers::NONE));

    assert!(
        app.input.as_str().contains("@srcr"),
        "typing should append after @mention"
    );
    assert!(
        matches!(app.prompt_accessory, PromptAccessory::Files(_)),
        "@mention should activate file picker while working"
    );
}

#[test]
fn acp_permission_select_sends_selected_option() {
    let mut app = fresh_app();
    let (tx, rx) = mpsc::channel();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::PermissionRequest(pending_permission(tx))),
    );

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        rx.try_recv().expect("permission decision"),
        PermissionDecision::Selected("allow".to_string())
    );
    assert!(app.pending_permission.is_none());
}

#[test]
fn acp_permission_escape_cancels_request() {
    let mut app = fresh_app();
    let (tx, rx) = mpsc::channel();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::PermissionRequest(pending_permission(tx))),
    );

    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(
        rx.try_recv().expect("permission decision"),
        PermissionDecision::Cancelled
    );
    assert!(app.pending_permission.is_none());
}

#[test]
fn acp_permission_run_cancel_responds_cancelled() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    let (tx, rx) = mpsc::channel();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::PermissionRequest(pending_permission(tx))),
    );

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));

    assert_eq!(
        rx.try_recv().expect("permission decision"),
        PermissionDecision::Cancelled
    );
    assert!(app.pending_permission.is_none());
    assert_eq!(app.run_state, RunState::Idle);
}
