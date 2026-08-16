//! Application behavior tests for session startup seams.

use super::*;
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
        "opencode-go",
        "opencode/big-pickle",
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
            .entries
            .iter()
            .any(|e| matches!(e, Entry::User { text } if text.contains("old message")))
    );
}

#[test]
fn resumed_startup_restores_the_session_without_creating_a_scratch_file() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = dir.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-resume",
        &dir.path().display().to_string(),
        "Saved work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create saved session");
    writer
        .append_entry(&Entry::User { text: "earlier prompt".to_string() }, "turn_1")
        .expect("append user entry");
    writer
        .append_context_capture_policy(&session::ContextCapturePolicy::retained_content())
        .expect("append retained-content policy");
    writer.append_usage(7, 11).expect("append usage");
    let saved_path = writer.path().to_path_buf();
    drop(writer);

    let cli = Cli { cwd: dir.path().to_path_buf(), session_dir: Some(sessions_dir.clone()), ..Cli::default() };
    let app = App::from_cli_resuming(&cli, "session-resume").expect("resume saved session");

    assert_eq!(app.session.id, "session-resume");
    assert_eq!(app.session.context_id_namespace, "session-resume");
    assert_eq!(app.runtime.session_tokens_in, 7);
    assert_eq!(app.runtime.session_tokens_out, 11);
    assert_eq!(app.session.turn_count, 1);
    assert!(
        !app.session.context_capture_policy.permits_content(),
        "resume requires a fresh per-run content opt-in"
    );
    assert!(
        app.transcript.context_ledger.is_some(),
        "resume refreshes the context projection"
    );
    assert_eq!(app.session.writer.as_ref().expect("session writer").path(), saved_path);
    assert_eq!(session::list_session_files(&sessions_dir), vec![saved_path]);
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "earlier prompt"))
    );
}

#[test]
fn resumed_fork_keeps_the_root_context_id_namespace() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = dir.path().join("sessions");
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "parent-session",
        &dir.path().display().to_string(),
        "Parent work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create parent session");
    writer
        .append_entry(&Entry::User { text: "fork here".to_string() }, "turn_1")
        .expect("append user entry");
    writer
        .append_entry(
            &Entry::Agent { text: "settled".to_string(), streaming: false },
            "turn_1",
        )
        .expect("append assistant entry");
    let parent_path = writer.path().to_path_buf();
    drop(writer);

    session::fork_session(&sessions_dir, &parent_path, "parent-session", "turn_1")
        .expect("create an occupied base fork id");
    let fork_id = session::fork_session(&sessions_dir, &parent_path, "parent-session", "turn_1").expect("fork session");
    let cli = Cli { cwd: dir.path().to_path_buf(), session_dir: Some(sessions_dir), ..Cli::default() };
    let app = App::from_cli_resuming(&cli, &fork_id).expect("resume fork");

    assert_eq!(app.session.context_id_namespace, "parent-session");
}

#[test]
fn ephemeral_startup_keeps_the_session_directory_empty_and_records_prompt_history() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = dir.path().join("sessions");
    std::fs::create_dir(&sessions_dir).expect("create empty session directory");
    let mut acp_agents = crate::config::AcpAgentsConfig::new();
    acp_agents.insert("local".to_string(), crate::config::AcpAgentConfig::default());
    let cli = Cli {
        cwd: dir.path().to_path_buf(),
        model: "acp:local".to_string(),
        session_dir: Some(sessions_dir.clone()),
        ephemeral: true,
        acp_agents,
        ..Cli::default()
    };

    let mut app = App::from_cli(&cli);
    app.overlay.close();

    assert!(app.is_ephemeral());
    assert_eq!(app.run_label(), "ephemeral");
    assert!(app.session.writer.is_none());
    assert!(
        std::fs::read_dir(&sessions_dir)
            .expect("read session directory")
            .next()
            .is_none()
    );

    submit_user_turn(&mut app, "remember this prompt".to_string()).expect("start ephemeral turn");

    assert!(
        std::fs::read_dir(&sessions_dir)
            .expect("read session directory")
            .next()
            .is_none()
    );
    assert_eq!(
        InputHistoryStore::for_workspace(dir.path())
            .load_recent()
            .expect("load shared input history"),
        Some(vec!["remember this prompt".to_string()])
    );
}

#[test]
fn fresh_startup_records_context_without_memory_state() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);
    let _: &[ContextSource] = &app.transcript.context_sources;

    let writer = app.session.writer.as_ref().expect("session writer");
    let records = session::SessionReader::read_records(writer.path());
    let Some(session::SessionRecord::SessionMeta { config: Some(config), .. }) = records.first() else {
        panic!("fresh session metadata record");
    };
    assert!(config.files.is_empty());
}

#[test]
fn from_cli_does_not_scan_session_history() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let sessions_dir = session::sessions_dir(dir.path());
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-old",
        "/repo",
        "old",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create old session");
    writer
        .append_entry(&Entry::User { text: String::from("old session prompt") }, "turn_1")
        .expect("append old entry");

    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert!(app.composer.input_history.is_empty());
    assert!(
        InputHistoryStore::for_workspace(dir.path())
            .load_recent()
            .expect("load dedicated history")
            .is_none()
    );
}

#[test]
fn from_cli_uses_dedicated_input_history() {
    let dir = tempfile::tempdir().expect("create temp dir");
    InputHistoryStore::for_workspace(dir.path())
        .append("history-session", "dedicated prompt")
        .expect("append dedicated history");

    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.composer.input_history, vec!["dedicated prompt".to_string()]);
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
        websearch: crate::cli::WebSearchMode::DuckDuckGo,
        websearch_url: None,
        session_dir: Some(session_dir.clone()),
        config_layers: vec![LoadedConfigLayer {
            source: ConfigSource::ProjectFile,
            config: Config::default(),
            path: None,
            display_path: Some(".thndrs/config.toml".to_string()),
            hash: Some("abc123".to_string()),
            active: true,
        }],
        config_origins: origins,
        ..Cli::default()
    };

    let app = App::from_cli(&cli);
    let path = app
        .session
        .writer
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
    assert_eq!(websearch, "duckduckgo");
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

    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let app = helpers::with_home(&home, || App::from_cli(&cli));
    let path = app
        .session
        .writer
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
        auth::OPENCODE_GO_KEY_ENV,
        "test-opencode-key",
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

    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = with_home(&home, || App::from_cli(&cli));
    app.overlay.close();
    let previous_hash = app.session.mcp_config_files[0].sha256.clone();
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
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text.starts_with("MCP config changed:")))
    );

    let path = app
        .session
        .writer
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
    assert_eq!(app.runtime.session_tokens_in, 17);
    assert_eq!(app.runtime.session_tokens_out, 10);
    assert_eq!(app.runtime.session_usage.request_count, 2);
    assert_eq!(app.runtime.session_usage.input_tokens, Some(17));
    assert_eq!(app.runtime.session_usage.output_tokens, Some(10));
    assert_eq!(app.runtime.session_usage.cache_read_input_tokens, None);
}

#[test]
fn finished_persists_final_assistant_even_after_status_row() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();

    app.composer.input = PromptInput::from("update TODO.md");
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
