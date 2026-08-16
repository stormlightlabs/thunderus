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
fn submitting_and_cancelling_emit_run_scoped_effects() {
    let mut app = fresh_app();

    let _ = update_with_effects(&mut app, &Msg::Action(Action::InsertText("hello".to_string())));
    let submitted = update_with_effects(&mut app, &Msg::Action(Action::Submit));
    let started = update_with_effects(&mut app, &submitted.follow_up.expect("agent start follow-up"));
    let request = app.runtime.active_effect_request.clone().expect("active request");

    assert_eq!(started.effects, vec![Effect::StartAgent(request.clone())]);
    assert_eq!(app.runtime.run_state, RunState::Working);

    let cancelled = update_with_effects(&mut app, &Msg::Action(Action::Cancel));
    assert_eq!(cancelled.effects, vec![Effect::CancelAgent(request)]);
    assert_eq!(app.runtime.run_state, RunState::Stopping);
}

#[test]
fn stale_and_duplicate_agent_completions_are_ignored() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    let current = EffectRequest { session_id: app.session.id.clone(), turn: 2 };
    let stale = EffectRequest { session_id: app.session.id.clone(), turn: 1 };
    app.runtime.active_effect_request = Some(current.clone());

    let stale_result = update_with_effects(
        &mut app,
        &Msg::Effect(EffectResult::Agent { request: stale, event: AgentEvent::Finished }),
    );
    assert_eq!(app.runtime.run_state, RunState::Working);
    assert!(stale_result.effects.is_empty());

    let completed = update_with_effects(
        &mut app,
        &Msg::Effect(EffectResult::Agent { request: current.clone(), event: AgentEvent::Finished }),
    );
    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert_eq!(completed.effects, vec![Effect::SettleAgent(current.clone())]);
    assert!(app.runtime.active_effect_request.is_none());

    let duplicate = update_with_effects(
        &mut app,
        &Msg::Effect(EffectResult::Agent { request: current, event: AgentEvent::Finished }),
    );
    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert!(duplicate.effects.is_empty());
}

#[test]
fn terminal_actions_are_returned_as_effects() {
    let mut app = fresh_app();

    assert_eq!(
        update_with_effects(&mut app, &Msg::Clear).effects,
        vec![Effect::ClearTerminal]
    );
    assert_eq!(
        update_with_effects(&mut app, &Msg::Action(Action::Suspend)).effects,
        vec![Effect::SuspendTerminal]
    );
}

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

#[test]
fn compact_uses_provider_summary_and_replaces_active_context_only_after_success() {
    let mut app = fresh_app();
    app.transcript.entries = vec![
        Entry::User { text: "inspect the parser".to_string() },
        Entry::Agent { text: "the parser rejects empty input".to_string(), streaming: false },
    ]
    .into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(app.transcript.pending_manual_compaction.is_some());
    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize"))),
        "internal compaction requests must stay out of the user transcript"
    );

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "parser: empty input is rejected");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(app.transcript.pending_manual_compaction.is_none());
    assert!(matches!(app.transcript.entries.first(), Some(Entry::User { text }) if text == "inspect the parser"));
    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize")))
    );
    assert!(app.transcript.compaction_summaries.iter().any(|summary| {
        summary.latest
            && summary
                .content
                .as_deref()
                .is_some_and(|text| text.contains("parser: empty input is rejected"))
    }));
}

#[test]
fn failed_compaction_restores_active_context_without_restoring_internal_prompt() {
    let mut app = fresh_app();
    let original = vec![Entry::User { text: "inspect the parser".to_string() }];
    app.transcript.entries = original.into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(
        app.composer
            .input_history
            .iter()
            .all(|prompt| !prompt.contains("Summarize")),
        "internal compaction requests must not enter prompt history"
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Failed("provider unavailable".to_string())),
    );

    assert!(matches!(app.transcript.entries.first(), Some(Entry::User { text }) if text == "inspect the parser"));
    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize")))
    );
    assert!(app.composer.input.is_empty());
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
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    app.transcript.entries = vec![Entry::User { text: "inspect the parser".to_string() }].into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "parser summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let records = session::SessionReader::read_records(&path);
    assert!(records.iter().any(|record| matches!(
        record,
        session::SessionRecord::Compaction { audit, .. }
            if audit.trigger == session::CompactionTrigger::Manual
                && audit.summary.contains("parser summary")
                && audit.model == cli.model
                && audit.recovery_handles.len() == 1
                && audit.typed_summary.is_some()
    )));
    assert!(
        records
            .iter()
            .all(|record| !matches!(record, session::SessionRecord::User { text, .. } if text.contains("Summarize"))),
        "internal compaction requests must not be persisted as user records"
    );
}

#[test]
fn repeated_compaction_anchors_the_previous_summary_and_only_adds_new_history() {
    let dir = tempfile::tempdir().expect("create temp dir");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed credential");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    app.transcript.entries = vec![
        Entry::User { text: "inspect the parser".to_string() },
        Entry::Agent { text: "the parser rejects empty input".to_string(), streaming: false },
    ]
    .into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let first_summary = context::range_summary_response(&app, "parser summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(first_summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    let first_summary_id = app
        .transcript
        .compaction_summaries
        .iter()
        .find(|summary| summary.latest)
        .expect("latest summary")
        .id
        .clone();

    app.transcript
        .entries
        .push(Entry::User { text: "fix that behavior".to_string() });
    app.transcript
        .entries
        .push(Entry::Agent { text: "the empty-input case is fixed".to_string(), streaming: false });
    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let second_summary = context::range_summary_response(&app, "updated parser summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(second_summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let audits = session::SessionReader::read_records(&path)
        .into_iter()
        .filter_map(|record| match record {
            session::SessionRecord::Compaction { audit, .. } => Some(audit),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[1].source_summary_ids, vec![first_summary_id]);
    assert_eq!(audits[1].covered_start_seq, audits[0].covered_start_seq);
    assert!(audits[1].covered_end_seq > audits[0].covered_end_seq);
    assert!(
        audits[1]
            .source_hashes
            .iter()
            .all(|source| !audits[0].source_hashes.iter().any(|previous| previous.id == source.id))
    );
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
    app.transcript.entries = original.into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "reviewable summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(app.transcript.pending_compaction_review.is_some());
    assert_eq!(
        app.transcript.last_compaction_review,
        Some(session::CompactionReviewResult::Pending)
    );
    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Status { text } if text.contains("review pending")
    )));

    app.composer.input = PromptInput::from("/context review approve");
    handle_command(&mut app, "context review approve");
    assert!(app.transcript.pending_compaction_review.is_none());
    assert_eq!(
        app.transcript.last_compaction_review,
        Some(session::CompactionReviewResult::Approved)
    );
    assert!(app.transcript.compaction_summaries.iter().any(|summary| {
        summary.latest
            && summary
                .content
                .as_deref()
                .is_some_and(|text| text.contains("reviewable summary"))
    }));
}

#[test]
fn rejected_compaction_keeps_the_projection_and_does_not_append_a_summary_record() {
    let dir = tempfile::tempdir().expect("create temp dir");
    auth::set_credential(
        &auth::project_credentials_path(dir.path()),
        auth::OPENCODE_ZEN_KEY_ENV,
        "test-zen-key",
    )
    .expect("seed credential");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    let original = vec![
        Entry::User { text: "inspect the parser".to_string() },
        Entry::Tool {
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["details".to_string()],
        },
    ];
    app.transcript.entries = original.clone().into();

    assert_eq!(
        handle_command(&mut app, "compact"),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "inspect parser");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    handle_command(&mut app, "context review reject");

    assert_eq!(app.transcript.entries[..original.len()], original);
    assert!(
        session::SessionReader::read_records(&path)
            .iter()
            .all(|record| !matches!(record, session::SessionRecord::Compaction { .. }))
    );
}

#[test]
fn auto_compaction_restarts_the_user_turn_after_success() {
    let mut app = fresh_app();
    let original_turn = "continue the work".to_string();
    app.transcript.entries = vec![
        Entry::User { text: "long conversation".to_string() },
        Entry::Agent { text: "lots of detail".to_string(), streaming: false },
        Entry::User { text: original_turn.clone() },
    ]
    .into();

    assert_eq!(
        start_auto_compaction(&mut app, original_turn.clone()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    assert!(app.compaction_in_flight());
    assert_eq!(app.status_label(), "Compacting");
    let mut ledger = app.refresh_context_ledger(None);
    ledger.budget.used = ledger.budget.auto_compaction_threshold.saturating_add(1);
    app.transcript.context_ledger = Some(ledger);

    let mut rendered_status = String::new();
    for (label, width) in [("normal", 80), ("narrow", 40)] {
        let row = crate::renderer::live::static_status_row(&app, width);
        let frame = crate::renderer::row::Frame { rows: vec![row], width, cursor: None, cursor_visible: true };
        rendered_status.push_str(&format!("{label} ({width}):\n"));
        rendered_status.push_str(&frame.render_styled());
        rendered_status.push('\n');
    }
    assert!(rendered_status.contains("Compacting"));
    assert!(rendered_status.contains("ctx left"));
    assert!(
        !rendered_status.contains("ctx left · compact"),
        "active compaction belongs in the left operational indicator"
    );
    insta::assert_snapshot!("compaction_statusline", rendered_status);

    assert!(
        app.transcript
            .entries
            .iter()
            .all(|entry| !matches!(entry, Entry::User { text } if text.contains("Summarize"))),
        "internal compaction requests must stay out of the user transcript"
    );

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "compacted summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    let restart = update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert!(!app.compaction_in_flight());
    assert_eq!(
        app.transcript
            .entries
            .iter()
            .filter(|entry| matches!(entry, Entry::User { text } if text == &original_turn))
            .count(),
        1
    );
    assert_eq!(restart, Some(Msg::Agent(AgentEvent::Started)));
    assert!(app.transcript.compaction_summaries.iter().any(|summary| {
        summary
            .content
            .as_deref()
            .is_some_and(|text| text.contains("compacted summary"))
    }));
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "long conversation"))
    );
    assert!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("compaction refreshes context")
            .projection()
            .categories
            .iter()
            .any(
                |total| total.category == thndrs_agent::context::ContextCategory::Summaries
                    && total.available_items > 0
            )
    );
}

#[test]
fn auto_compaction_restart_waits_for_followups_until_turn_completes() {
    let mut app = fresh_app();
    let original_turn = "continue the work".to_string();
    app.transcript.entries = vec![
        Entry::User { text: "long conversation".to_string() },
        Entry::Agent { text: "old answer".to_string(), streaming: false },
        Entry::User { text: original_turn.clone() },
    ]
    .into();
    app.composer.queue.push(
        QueueTarget::FollowUp,
        "follow-up after restart".to_string(),
        "test".to_string(),
    );

    assert_eq!(
        start_auto_compaction(&mut app, original_turn.clone()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));

    let restart = update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(restart, Some(Msg::Agent(AgentEvent::Started)));
    assert_eq!(
        app.transcript
            .entries
            .iter()
            .filter(|entry| matches!(entry, Entry::User { text } if text == &original_turn))
            .count(),
        1
    );
    assert_eq!(
        app.composer.queue.pending_count(QueueTarget::FollowUp),
        1,
        "follow-up must wait until the restarted turn completes"
    );
}

#[test]
fn auto_compaction_failure_preserves_the_submitted_turn() {
    let mut app = fresh_app();
    app.transcript.entries = vec![Entry::User { text: "long conversation".to_string() }].into();
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
    assert_eq!(app.composer.last_input, Some(original_turn));
    assert!(matches!(app.transcript.entries.first(), Some(Entry::User { text }) if text == "long conversation"));
    assert!(
        !app.transcript
            .entries
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
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    app.transcript.entries = vec![Entry::User { text: "long conversation".to_string() }].into();

    assert_eq!(
        start_auto_compaction(&mut app, "continue".to_string()),
        Some(Msg::Agent(AgentEvent::Started))
    );
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    let summary = context::range_summary_response(&app, "auto summary");
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(summary)));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    let records = session::SessionReader::read_records(&path);
    assert!(records.iter().any(|record| matches!(
        record,
        session::SessionRecord::Compaction { audit, .. }
            if audit.trigger == session::CompactionTrigger::Automatic
                && audit.summary.contains("auto summary")
    )));
}

#[test]
fn ctrl_c_sets_quit_flag() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    );
    assert!(app.runtime.quit);
}

#[test]
fn ctrl_c_cancels_running_stream() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    );

    assert!(!app.runtime.quit, "Ctrl+C while running should not quit immediately");
    assert_eq!(app.runtime.run_state, RunState::Stopping);
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text == "cancelled"))
    );
}

#[test]
fn stopping_timeout_returns_to_idle_when_agent_never_acknowledges_cancel() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

    assert_eq!(app.runtime.run_state, RunState::Stopping);
    let deadline = app.runtime.stopping_deadline.expect("stopping deadline");

    for _ in 0..=deadline {
        update(&mut app, &Msg::Tick);
        if app.runtime.ui_tick > deadline || app.runtime.run_state == RunState::Idle {
            break;
        }
    }

    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert!(app.runtime.stopping_deadline.is_none());
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Status { text }) if text == "cancelled"));
}

#[test]
fn late_started_event_does_not_revive_stopping_run() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    let deadline = app.runtime.stopping_deadline;

    update(&mut app, &Msg::Agent(AgentEvent::Started));

    assert_eq!(app.runtime.run_state, RunState::Stopping);
    assert_eq!(app.runtime.stopping_deadline, deadline);
}

#[test]
fn terminal_agent_events_clear_stopping_deadline() {
    for event in [
        AgentEvent::Finished,
        AgentEvent::Failed("provider unavailable".to_string()),
        AgentEvent::Cancelled,
    ] {
        let mut app = fresh_app();
        update(&mut app, &Msg::Agent(AgentEvent::Started));
        update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(app.runtime.stopping_deadline.is_some());

        update(&mut app, &Msg::Agent(event));

        assert_ne!(app.runtime.run_state, RunState::Stopping);
        assert!(app.runtime.stopping_deadline.is_none());
    }
}

#[test]
fn ctrl_d_first_press_shows_confirmation() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.runtime.quit, "first Ctrl+D should not quit");
    assert!(app.runtime.ctrl_d_pending.is_some(), "should arm pending confirmation");
    assert!(
        app.transcript.entries.iter().any(|e| matches!(
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
    assert!(!app.runtime.quit);
    assert!(app.runtime.ctrl_d_pending.is_some());

    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.runtime.quit, "second Ctrl+D should quit");
    assert!(
        app.runtime.ctrl_d_pending.is_none(),
        "pending should be cleared on quit"
    );
    assert_eq!(follow, Some(Msg::Quit));
}

#[test]
fn ctrl_d_timeout_expires_and_requires_double_press_again() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.runtime.ctrl_d_pending.is_some());

    for _ in 0..quit_confirm_timeout_ticks(&app) + 1 {
        update(&mut app, &Msg::Tick);
    }
    assert!(
        app.runtime.ctrl_d_pending.is_none(),
        "pending should expire after timeout"
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.runtime.quit, "expired second press should not quit");
    assert!(app.runtime.ctrl_d_pending.is_some(), "should arm a fresh confirmation");
}

#[test]
fn ctrl_d_timeout_is_stable_at_the_faster_render_cadence() {
    let mut app = fresh_app();
    app.runtime.cli.tick_rate_ms = 33;

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );

    assert_eq!(app.runtime.ctrl_d_pending, Some(91));
}

#[test]
fn app_clamps_tick_rate_to_the_direct_renderer_minimum() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), tick_rate_ms: 1, ..Cli::default() };

    let app = App::from_cli(&cli);

    assert_eq!(app.runtime.cli.tick_rate_ms, crate::cli::MIN_TICK_RATE_MS);
}

#[test]
fn ctrl_d_cancelled_by_other_key() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.runtime.ctrl_d_pending.is_some());

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(
        app.runtime.ctrl_d_pending.is_none(),
        "other key should cancel pending Ctrl+D"
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.runtime.quit, "should not quit after cancellation");
    assert!(app.runtime.ctrl_d_pending.is_some());
}

#[test]
fn other_keys_do_not_quit() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(!app.runtime.quit);
    update(&mut app, &Msg::Tick);
    assert!(!app.runtime.quit);
}

#[test]
fn file_picker_selection_inserts_selected_path() {
    let mut app = fresh_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        picker_from_paths(vec!["src/main.rs".to_string(), "src/app.rs".to_string()]),
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "src/app.rs");
    assert!(app.overlay.picker().is_none());
}

#[test]
fn file_picker_arrows_and_pages_are_scrollable() {
    let mut app = fresh_app();
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        picker_from_paths((0..20).map(|i| format!("src/file_{i:02}.rs")).collect()),
    );

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
    );
    let picker = app.overlay.picker().expect("picker");
    assert_eq!(picker.selected, VISIBLE_ROWS);
    assert!(picker.scroll > 0);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)));
    let picker = app.overlay.picker().expect("picker");
    assert_eq!(picker.selected, 0);
    assert_eq!(picker.scroll, 0);
}

#[test]
fn tick_increments_ui_tick() {
    let mut app = fresh_app();
    assert_eq!(app.runtime.ui_tick, 0);
    update(&mut app, &Msg::Tick);
    assert_eq!(app.runtime.ui_tick, 1);
}

#[test]
fn status_toast_expires_after_its_timeout() {
    let mut app = fresh_app();
    app.runtime.cli.tick_rate_ms = 1_000;
    app.show_status_toast("Copied transcript selection", StatusToastKind::Success);

    update(&mut app, &Msg::Tick);
    assert!(app.runtime.status_toast.is_some());
    update(&mut app, &Msg::Tick);
    assert!(app.runtime.status_toast.is_some());
    update(&mut app, &Msg::Tick);
    assert!(app.runtime.status_toast.is_none());
}

#[test]
fn quit_message_sets_quit_flag() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Quit);
    assert!(app.runtime.quit);
}

#[test]
fn backspace_removes_last_char() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("abc");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.input.as_str(), "ab");
}

#[test]
fn enter_trims_whitespace_before_submit() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("  hello  ");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(app.transcript.entries[0], Entry::User { text: String::from("hello") });
}

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

#[test]
fn msg_clear_clears_transcript() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::User { text: String::from("a") });
    app.transcript.entries.push(Entry::User { text: String::from("b") });
    update(&mut app, &Msg::Clear);
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn agent_started_sets_working() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    assert_eq!(app.runtime.run_state, RunState::Working);
}

#[test]
fn assistant_delta_creates_streaming_entry() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("Hello"))));
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(
        app.transcript.entries[0],
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
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(
        app.transcript.entries[0],
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
    assert_eq!(app.transcript.entries.len(), 2);
    assert_eq!(
        app.transcript.entries[0],
        Entry::Agent { text: String::from("first"), streaming: false }
    );
    assert_eq!(
        app.transcript.entries[1],
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
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(
        app.transcript.entries[0],
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
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(
        app.transcript.entries[0],
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
        app.transcript.entries,
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
        app.transcript.entries.first(),
        Some(Entry::Reasoning { streaming: false, .. })
    ));
    assert!(matches!(
        app.transcript.entries.last(),
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
    assert_eq!(app.transcript.entries.len(), 1);
    match &app.transcript.entries[0] {
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
fn reading_a_discovered_skill_announces_it_during_the_run_once() {
    let dir = tempfile::tempdir().expect("create workspace");
    let skill_dir = dir.path().join(".agents/skills/review");
    std::fs::create_dir_all(&skill_dir).expect("create skill directory");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: review\ndescription: Reviews changes.\n---\n\n# Review\n\nInspect the diff.\n",
    )
    .expect("write skill");

    let mut app = fresh_app();
    app.runtime.cwd = dir.path().to_path_buf();
    app.transcript.skills = skills::discover(dir.path(), &[]).skills;
    let arguments = r#"{"path":".agents/skills/review/SKILL.md","start_line":1,"end_line":20}"#.to_string();

    for id in ["first", "second"] {
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: id.to_string(),
                name: "read_file_range".to_string(),
                arguments: arguments.clone(),
            }),
        );
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolFinished {
                id: id.to_string(),
                output: vec!["# Review".to_string()],
                status: ToolStatus::Ok,
                write_result: None,
                shell_result: None,
            }),
        );
    }

    let notices = app
        .transcript
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Skill { name, content, token_estimate, .. } => Some((name, content, token_estimate)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].0, "review");
    assert!(
        notices[0].1.is_empty(),
        "tool output supplies the skill content to the model"
    );
    assert!(*notices[0].2 > 0);
}

#[test]
fn failed_skill_read_does_not_announce_the_skill() {
    let dir = tempfile::tempdir().expect("create workspace");
    let skill_dir = dir.path().join(".agents/skills/review");
    std::fs::create_dir_all(&skill_dir).expect("create skill directory");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: review\ndescription: Reviews changes.\n---\n\n# Review\n",
    )
    .expect("write skill");

    let mut app = fresh_app();
    app.runtime.cwd = dir.path().to_path_buf();
    app.transcript.skills = skills::discover(dir.path(), &[]).skills;
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: "failed".to_string(),
            name: "read_file_range".to_string(),
            arguments: r#"{"path":".agents/skills/review/SKILL.md"}"#.to_string(),
        }),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolFinished {
            id: "failed".to_string(),
            output: vec!["permission denied".to_string()],
            status: ToolStatus::Failed,
            write_result: None,
            shell_result: None,
        }),
    );

    assert!(
        !app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Skill { .. }))
    );
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
    assert_eq!(
        app.transcript.entries.len(),
        1,
        "tool completion should update its existing entry"
    );
    match &app.transcript.entries[0] {
        Entry::Tool { status, output, .. } => {
            assert_eq!(*status, ToolStatus::Ok);
            assert_eq!(*output, vec!["line 1", "line 2"]);
        }
        _ => panic!("expected Tool entry"),
    }
}

#[test]
fn tool_artifact_bodies_require_context_capture_opt_in() {
    for (capture_context_content, should_retain_artifact) in [(false, false), (true, true)] {
        let dir = tempfile::tempdir().expect("create temp dir");
        let cli = Cli {
            cwd: dir.path().to_path_buf(),
            model: "fake-agent".to_string(),
            capture_context_content,
            ..Cli::default()
        };
        let mut app = App::from_cli(&cli);
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolStarted {
                id: "artifact-policy".to_string(),
                name: "read_file".to_string(),
                arguments: "{}".to_string(),
            }),
        );
        update(
            &mut app,
            &Msg::Agent(AgentEvent::ToolFinished {
                id: "artifact-policy".to_string(),
                output: vec!["bounded output".to_string()],
                status: ToolStatus::Ok,
                write_result: None,
                shell_result: None,
            }),
        );

        assert_eq!(
            app.transcript.tool_artifacts.contains_key("artifact-policy"),
            should_retain_artifact
        );
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
    match &app.transcript.entries[0] {
        Entry::Tool { status, .. } => assert_eq!(*status, ToolStatus::Failed),
        _ => panic!("expected Tool entry"),
    }
}

#[test]
fn state_identical_tool_decisions_appear_as_recoverable_context_relations() {
    let mut app = fresh_app();
    app.transcript.entries = vec![
        Entry::Tool {
            name: "read_file_range#call_1".to_string(),
            arguments: r#"{"path":"src/lib.rs","start_line":1,"end_line":2}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec!["1: first".to_string(), "2: second".to_string()],
        },
        Entry::Tool {
            name: "read_file_range#call_2".to_string(),
            arguments: r#"{"path":"src/lib.rs","start_line":1,"end_line":2}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec!["1: first".to_string(), "2: second".to_string()],
        },
        Entry::Tool {
            name: "read_file_range#call_3".to_string(),
            arguments: r#"{"path":"src/lib.rs","start_line":1,"end_line":2}"#.to_string(),
            status: ToolStatus::Ok,
            output: vec!["1: changed".to_string(), "2: second".to_string()],
        },
    ]
    .into();
    app.transcript
        .tool_artifacts
        .insert("call_1".to_string(), "artifact:call_1".to_string());
    app.transcript
        .tool_artifacts
        .insert("call_2".to_string(), "artifact:call_2".to_string());
    app.transcript
        .tool_artifacts
        .insert("call_3".to_string(), "artifact:call_3".to_string());
    app.transcript.tool_projection_decisions.insert(
        "call_2".to_string(),
        thndrs_agent::context::StateProjectionDecision::DuplicateOf { canonical_id: "tool:call_1".to_string() },
    );
    app.transcript.tool_projection_decisions.insert(
        "call_3".to_string(),
        thndrs_agent::context::StateProjectionDecision::Supersedes { previous_id: "tool:call_1".to_string() },
    );

    let ledger = app.refresh_context_ledger(None);
    let duplicate = ledger
        .items
        .iter()
        .find(|item| item.label == "tool:read_file_range#call_2")
        .expect("duplicate item");
    assert_eq!(
        duplicate.lifecycle.state,
        thndrs_agent::context::ContextLifecycleState::Duplicate
    );
    assert!(duplicate.artifact_handle.is_some());
    assert!(
        duplicate
            .lifecycle
            .relations
            .iter()
            .any(|relation| { relation.kind == thndrs_agent::context::ContextRelationKind::DuplicateOf })
    );
    let canonical = ledger
        .items
        .iter()
        .find(|item| item.label == "tool:read_file_range#call_1")
        .expect("canonical item");
    assert!(
        canonical
            .lifecycle
            .relations
            .iter()
            .any(|relation| { relation.kind == thndrs_agent::context::ContextRelationKind::SupersededBy })
    );

    let export = app.build_context_export(false);
    let exported = export
        .items
        .iter()
        .find(|item| item.id == duplicate.id)
        .expect("exported item");
    assert!(exported.recovery_available);
    assert_eq!(
        exported.lifecycle,
        thndrs_agent::context::ContextLifecycleState::Duplicate
    );
    assert_eq!(exported.relations.len(), 1);
}

#[test]
fn failed_tool_error_line_is_visible_and_persisted() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "fake-agent".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    let session_path = app
        .session
        .writer
        .as_ref()
        .expect("session writer")
        .path()
        .to_path_buf();
    let error = "error: missing command: provide non-empty 'argv', 'command', or 'program'";

    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("call_1"),
            name: String::from("run_shell"),
            arguments: String::from(r#"{"command":["sh","-lc","true"]}"#),
        }),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolFinished {
            id: String::from("call_1"),
            output: vec![error.to_string()],
            status: ToolStatus::Failed,
            write_result: None,
            shell_result: None,
        }),
    );

    let Some(Entry::Tool { output, .. }) = app.transcript.entries.last() else {
        panic!("expected failed tool entry");
    };
    assert_eq!(output, &vec![error.to_string()]);

    let records = session::SessionReader::read_records(&session_path);
    let persisted = records
        .iter()
        .find_map(|record| match record {
            session::SessionRecord::ToolFinished { status: ToolStatus::Failed, output, .. } => Some(output),
            _ => None,
        })
        .expect("persisted failed tool record");
    assert_eq!(persisted, &vec![error.to_string()]);
}

#[test]
fn cancelled_event_adds_status_and_returns_to_idle() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("partial"))),
    );
    assert_eq!(app.runtime.run_state, RunState::Working);

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Status { text }) if text == "cancelled"));

    match &app.transcript.entries[0] {
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

    assert_eq!(app.runtime.run_state, RunState::Working);
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(app.runtime.run_state, RunState::Idle);

    if let Entry::Agent { streaming, .. } = &app.transcript.entries[0] {
        assert!(!*streaming);
    } else {
        panic!("expected Assistant entry");
    }

    match &app.transcript.entries[1] {
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
    assert_eq!(app.runtime.run_state, RunState::Working);

    update(
        &mut app,
        &Msg::Agent(AgentEvent::Failed(String::from("connection lost"))),
    );
    assert_eq!(app.runtime.run_state, RunState::Error("connection lost".to_string()));
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text == "connection lost"));

    match &app.transcript.entries[0] {
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
    assert_eq!(app.runtime.run_state, RunState::Working);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.runtime.run_state, RunState::Stopping);

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.runtime.run_state, RunState::Idle);

    match &app.transcript.entries[0] {
        Entry::Agent { streaming, .. } => assert!(!*streaming),
        _ => panic!("expected Assistant entry"),
    }
}

#[test]
fn escape_does_nothing_when_idle() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn submit_while_working_queues_followup_by_default() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("queued message");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["queued message"]
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued follow-up")))
    );
}

#[test]
fn steering_chord_queues_running_input_as_steering() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("look at tests first");

    #[cfg(target_os = "macos")]
    let modifiers = KeyModifiers::SUPER;
    #[cfg(not(target_os = "macos"))]
    let modifiers = KeyModifiers::CONTROL;
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, modifiers)));

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::Steering)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
}

#[test]
fn ctrl_g_queues_running_input_as_steering_in_all_terminal_environments() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("look at tests first");

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
    );

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::Steering)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
}

#[test]
fn plain_submit_while_working_always_queues_a_followup() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.queue_target = QueueTarget::Steering;
    app.composer.input = PromptInput::from("look at tests first");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued follow-up")))
    );
}

#[test]
fn ctrl_o_opens_the_latest_tool_with_output() {
    let mut app = fresh_app();
    for entry in [
        Entry::Tool {
            name: "run_shell".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Failed,
            output: vec!["old failure".to_string()],
        },
        Entry::Tool {
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["latest output".to_string()],
        },
        Entry::Tool {
            name: "write_patch".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Running,
            output: Vec::new(),
        },
    ] {
        app.transcript.entries.push(entry);
    }

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );

    assert_eq!(app.overlay.detail().map(|detail| detail.entry_index), Some(1));

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );
    assert!(!app.overlay.is_detail(), "Ctrl+O should close open details");
}

#[test]
fn ctrl_o_without_tool_output_leaves_transcript_focus_unchanged() {
    let mut app = fresh_app();
    app.transcript
        .entries
        .push(Entry::Agent { text: "nothing expandable".to_string(), streaming: false });

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );

    assert!(!app.overlay.is_detail());
}

#[test]
fn finished_starts_next_followup_turn() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "next task".to_string(), "test".to_string());

    let next = update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert_eq!(next, Some(Msg::Agent(AgentEvent::Started)));
    assert_eq!(app.composer.queue.pending_count(QueueTarget::FollowUp), 0);
    assert_eq!(app.session.turn_count, 1);
    assert!(matches!(app.transcript.entries.last(), Some(Entry::User { text }) if text == "next task"));
}

#[test]
fn cancelled_clears_queued_steering_but_keeps_followups() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::Steering, "steer".to_string(), "test".to_string());
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "after".to_string(), "test".to_string());

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));

    assert_eq!(app.composer.queue.pending_count(QueueTarget::Steering), 0);
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["after"]
    );
}

#[test]
fn submit_kicks_off_agent_via_followup() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("explain this repo");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(follow, Some(Msg::Agent(AgentEvent::Started)));
}

#[test]
fn app_without_agents_md_has_no_context_sources() {
    let app = fresh_app();
    assert!(app.transcript.context_sources.is_empty());
    assert!(app.transcript.entries.is_empty());
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

    assert_eq!(app.transcript.context_sources.len(), 1);
    let source = &app.transcript.context_sources[0];
    assert_eq!(
        source.path,
        agents_path.canonicalize().unwrap_or_else(|_| agents_path.to_path_buf())
    );
    assert_eq!(source.scope, ".");
    assert!(!source.truncated);
    assert!(source.content.contains("# Project"));
    assert!(
        app.transcript.entries.is_empty(),
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

    assert_eq!(app.transcript.context_sources.len(), 1);
    let source = &app.transcript.context_sources[0];
    assert!(source.truncated);
    assert!(source.content.len() <= AGENTS_MD_SIZE_CAP);

    assert!(
        app.transcript.entries.is_empty(),
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

    assert!(app.runtime.model.is_empty());
    assert!(app.transcript.context_sources[0].content.contains("Model: gpt-4"));
}

#[test]
fn stopping_state_after_escape() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.runtime.run_state, RunState::Stopping);
    assert_eq!(app.status_label(), "Stopping");
    assert_eq!(app.prompt_state(), PromptState::Stopped);
}

#[test]
fn stopping_transitions_to_idle_on_cancelled_event() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.runtime.run_state, RunState::Stopping);
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.runtime.run_state, RunState::Idle);
}

#[test]
fn error_state_all_resubmission() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.runtime.run_state, RunState::Error("boom".to_string()));
    app.composer.input = PromptInput::from("retry");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(follow.is_some());
    if let Some(msg) = follow {
        update(&mut app, &msg);
    }
    assert_eq!(app.runtime.run_state, RunState::Working);
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

    assert_eq!(app.composer.input.as_str(), "previous!");
    assert_eq!(app.composer.input_history, vec![String::from("previous")]);
    assert_eq!(app.composer.history_cursor, None);
}

#[test]
fn remembering_input_keeps_bounded_in_memory_history() {
    let mut app = fresh_app();
    app.composer.input_history = (0..INPUT_HISTORY_LIMIT)
        .map(|index| format!("prompt {index}"))
        .collect();

    remember_input(&mut app, "newest prompt");

    assert_eq!(app.composer.input_history.len(), INPUT_HISTORY_LIMIT);
    assert_eq!(app.composer.input_history.first().map(String::as_str), Some("prompt 1"));
    assert_eq!(
        app.composer.input_history.last().map(String::as_str),
        Some("newest prompt")
    );
}

#[test]
fn question_key_enters_help_mode() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Help);
}

#[test]
fn esc_exits_help_mode() {
    let mut app = fresh_app();
    app.overlay.show_help();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
}

#[test]
fn question_key_keeps_inline_help_open() {
    let mut app = fresh_app();
    app.overlay.show_help();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Help);
}

#[test]
fn question_key_does_not_enter_help_when_input_nonempty() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.composer.input.as_str(), "hello?");
}

#[test]
fn background_shell_result_registers_in_process_registry() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("toolu_bg"),
            name: String::from("run_shell"),
            arguments: String::from(r#"{"command":["sleep","10"]}"#),
        }),
    );

    let shell_result = tools::shell::ProcessResult {
        process_id: None,
        command: vec!["sleep".to_string(), "10".to_string()],
        cwd: std::path::PathBuf::from("."),
        status: tools::shell::ProcessStatus::Ok,
        exit_code: Some(0),
        stdout: vec!["background task done".to_string()],
        stderr: vec![],
        output_truncated: false,
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
        app.runtime.process_registry.background_count(),
        1,
        "background process should be registered"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("background process"))),
        "should add a background process status entry"
    );
}

#[test]
fn quit_cancels_all_background_processes() {
    let mut app = fresh_app();
    let cancel = CancelToken::new();
    app.runtime.process_registry.register(
        vec!["sleep".to_string(), "30".to_string()],
        std::path::PathBuf::from("."),
        tools::shell::ProcessKind::Background,
        cancel.clone(),
    );
    assert_eq!(app.runtime.process_registry.background_count(), 1);

    let result = update_with_effects(&mut app, &Msg::Quit);
    assert!(app.runtime.quit);
    assert_eq!(result.effects, vec![Effect::ShutdownProcesses]);
    assert!(!cancel.is_cancelled(), "pure update should not execute process effects");

    let completed = app.runtime.process_registry.shutdown();
    update_with_effects(&mut app, &Msg::Effect(EffectResult::BackgroundProcesses(completed)));
    assert!(cancel.is_cancelled(), "cancel_all should signal cancellation");
}

#[test]
fn ctrl_k_kills_to_end_of_line() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    app.composer.input.cursor_to_start();
    app.composer.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "hello ");
    assert_eq!(app.composer.kill_ring, vec!["world"]);
}

#[test]
fn ctrl_u_kills_to_start_of_line() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    app.composer.input.cursor_to_start();
    app.composer.input.cursor_word_right();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "world");
    assert_eq!(app.composer.kill_ring, vec!["hello "]);
}

#[test]
fn ctrl_w_kills_previous_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "foo bar ");
    assert_eq!(app.composer.kill_ring, vec!["baz"]);
}

#[test]
fn ctrl_y_yanks_last_kill() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello ");
    app.composer.kill_ring.push("world".to_string());
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "hello world");
}

#[test]
fn ctrl_t_transposes_chars() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("ab");
    app.composer.input.cursor_to_start();

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    assert_eq!(app.composer.input.as_str(), "ba");
}

#[test]
fn alt_d_kills_next_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    app.composer.input.cursor_to_start();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT)),
    );
    assert_eq!(app.composer.input.as_str(), "bar baz");
    assert_eq!(app.composer.kill_ring, vec!["foo "]);
}

#[test]
fn alt_backspace_kills_previous_word() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
    );
    assert_eq!(app.composer.input.as_str(), "foo bar ");
    assert_eq!(app.composer.kill_ring, vec!["baz"]);
}

#[test]
fn retrying_provider_discards_partial_output_without_restoring_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
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
            error: String::from("Our servers are currently overloaded"),
        }),
    );

    assert!(
        app.composer.input.is_empty(),
        "retry should keep the submitted input out of the editor"
    );
    assert_eq!(app.composer.last_input, Some("hello world".to_string()));
    assert_eq!(app.runtime.run_state, RunState::Working);
    assert!(
        !app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Agent { .. } | Entry::Reasoning { .. })),
        "partial output from the failed attempt should be removed before retrying"
    );
    assert_eq!(app.status_label(), "Waiting · provider overloaded · retry 1/4 in 2.5s");
    assert!(
        !app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text.contains("retrying provider"))),
        "provider retries should not accumulate transcript rows"
    );
}

#[test]
fn tui_update_path_handles_fake_provider_turn() {
    let mut app = fresh_app();
    std::fs::write(app.runtime.cwd.join("Cargo.toml"), "[package]\nname = \"fake\"\n").expect("write fake Cargo.toml");
    app.runtime.websearch = WebSearchMode::None;
    app.composer.input = PromptInput::from("inspect project");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    if let Some(message) = follow {
        update(&mut app, &message);
    }

    let config = AgentRunConfig::new(app.runtime.cwd.clone(), String::from("fake-agent"), WebSearchMode::None);
    let handle = HarnessTurn::fake(config, String::from("inspect project")).start();
    while let Ok(event) = handle.events.recv() {
        update(&mut app, &Msg::Agent(event));
    }

    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "inspect project"))
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Reasoning { text, streaming: false } if !text.is_empty()))
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Agent { text, streaming: false } if !text.is_empty()))
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Tool { status: ToolStatus::Ok, .. }))
    );
}

#[test]
fn shift_enter_inserts_newline() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(app.composer.input.as_str(), "line1\n");
    assert_eq!(app.composer.input.cursor(), 6);
}

#[test]
fn ctrl_j_inserts_newline() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("line1");
    update(&mut app, &key(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.as_str(), "line1\n");
}

#[test]
fn delete_key_deletes_forward() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    app.composer.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "ello");
    assert_eq!(app.composer.input.cursor(), 0);
}

#[test]
fn backspace_deletes_before_cursor() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "hell");
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn typing_inserts_at_cursor_not_at_end() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("helo");
    app.composer.input.cursor_left();
    update(&mut app, &key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "hello");
    assert_eq!(app.composer.input.cursor(), 4);
}

#[test]
fn at_token_opens_file_picker_and_accepts_mention() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.runtime.cwd.join("readme.md"), "readme");

    for ch in "inspect @read".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::Files(FilePickerSource::Mention { token_start: 8 })
    );
    let picker = app.overlay.picker().expect("file picker");
    assert!(picker.matches.iter().any(|item| item.label == "readme.md"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "inspect @readme.md ");
}

#[test]
fn file_mention_picker_routes_cursor_navigation_to_the_prompt() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.runtime.cwd.join("README.md"), "readme");

    for ch in "inspect @READ".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    let end = app.composer.input.cursor();

    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), end - 1);
    update(&mut app, &key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.composer.input.cursor(), end);

    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "inspect @REA");
    assert_eq!(app.overlay.picker().expect("file picker").query, "REA");
}

#[test]
fn at_token_accepts_directory_mention() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.runtime.cwd = dir.path().to_path_buf();
    std::fs::create_dir(app.runtime.cwd.join("src")).expect("create source directory");

    for ch in "inspect @src".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }

    let picker = app.overlay.picker().expect("path picker");
    assert_eq!(picker.selected().map(|item| item.label.as_str()), Some("src/"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "inspect @src/ ");
}

#[test]
fn model_metadata_event_updates_model_picker_items() {
    let mut app = fresh_app();
    app.refresh_context_ledger(None);
    handle_agent_event(
        &mut app,
        AgentEvent::ModelMetadataLoaded(vec![(
            "opencode/test".to_string(),
            "provider · ctx 1M · out 32k · tools · reasoning".to_string(),
        )]),
    );

    open_model_picker(&mut app);

    let picker = app.overlay.picker().expect("model picker");
    assert_eq!(picker.matches[0].label, "opencode/test");
    assert_eq!(
        picker.matches[0].detail,
        "provider · ctx 1M · out 32k · tools · reasoning"
    );

    accept_model_suggestion(&mut app);
    assert_eq!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("model selection refreshes context")
            .budget
            .limits
            .model,
        "opencode/test"
    );
}

#[test]
fn completed_response_refreshes_the_next_request_projection() {
    let mut app = fresh_app();
    let before = app.refresh_context_ledger(Some("request")).projection().used;

    handle_agent_event(
        &mut app,
        AgentEvent::AssistantDelta("a response that becomes conversation context".repeat(20)),
    );
    handle_agent_event(&mut app, AgentEvent::Finished);

    let after = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("completion refreshes context")
        .projection()
        .used;
    assert!(after > before);
}

#[test]
fn request_start_updates_live_context_usage() {
    let mut app = fresh_app();
    let before = app.refresh_context_ledger(Some("request")).projection();
    let accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:1",
        1,
        "opencode",
        "big-pickle",
        &vec![b'x'; 120_000],
        Vec::new(),
    );
    let expected_used = accounting.estimated_input_tokens.value.expect("request estimate");

    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting)));

    let after = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("request keeps live context projection")
        .projection();
    assert_eq!(after.used, expected_used);
    assert_ne!(after.remaining_percent, before.remaining_percent);
}

#[test]
fn retained_provider_measurement_survives_model_switches() {
    let mut app = fresh_app();
    let ledger = app.refresh_context_ledger(None);
    let context = thndrs_agent::snapshot_context(&ledger.items);
    let mut accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:1",
        1,
        "chatgpt-codex",
        "gpt-5.6-terra",
        &vec![b'x'; 180_000],
        context,
    );
    accounting.provider_usage = Some(
        thndrs_agent::ProviderUsageComponents::new(60_000, 1_000)
            .normalize("chatgpt-codex", thndrs_agent::ProviderUsageRule::OpenAiResponses),
    );
    app.session.last_request_accounting = Some(accounting);

    app.runtime.model = "opencode/gpt-5.6-terra".to_string();
    let refreshed = app.refresh_context_ledger(None);

    assert_eq!(refreshed.budget.used, 60_000);
}

#[test]
fn request_start_counts_cached_anthropic_input_as_context() {
    let mut app = fresh_app();
    app.refresh_context_ledger(None);
    let mut accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:1",
        1,
        "opencode",
        "claude-opus-4-8",
        br#"{"messages":[]}"#,
        Vec::new(),
    );
    accounting.provider_usage = Some(
        thndrs_agent::ProviderUsageComponents {
            input_tokens: Some(100),
            output_tokens: Some(1),
            cache_read_input_tokens: Some(20),
            cache_creation_input_tokens: Some(5),
            reasoning_tokens: None,
        }
        .normalize("opencode", thndrs_agent::ProviderUsageRule::AnthropicMessages),
    );

    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting)));

    assert_eq!(app.transcript.context_ledger.expect("context ledger").budget.used, 125);
}

#[test]
fn request_start_returns_status_to_working_after_a_tool() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.transcript.entries.push(Entry::Tool {
        name: "read_file_range".to_string(),
        arguments: "{}".to_string(),
        status: ToolStatus::Ok,
        output: Vec::new(),
    });
    assert_eq!(app.status_label(), "Working");
    let accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:2",
        1,
        "opencode",
        "big-pickle",
        br#"{"messages":[]}"#,
        Vec::new(),
    );

    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting)));

    assert_eq!(app.status_label(), "Working");
}

#[test]
fn request_attempts_append_distinct_context_snapshot_lifecycle_records() {
    let dir = tempfile::tempdir().expect("create session directory");
    let mut app = fresh_app();
    app.session.id = "snapshot-session".to_string();
    app.session.writer = Some(
        session::SessionWriter::create(
            dir.path(),
            &app.session.id,
            &app.runtime.cwd.display().to_string(),
            "snapshot lifecycle",
            "opencode",
            &app.runtime.model,
            "none",
            "0.1.0",
            None,
        )
        .expect("create session writer"),
    );
    app.refresh_context_ledger(Some("inspect the request"));

    let accounting = |attempt| {
        thndrs_agent::ProviderRequestAccounting::from_serialized_request(
            "turn_1",
            "turn_1:request:1",
            attempt,
            "opencode",
            "big-pickle",
            br#"{"messages":[]}"#,
            Vec::new(),
        )
    };
    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting(1))));
    handle_agent_event(
        &mut app,
        AgentEvent::Retrying { attempt: 2, max_attempts: 3, delay_ms: 0, error: "provider unavailable".to_string() },
    );
    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting(2))));
    handle_agent_event(&mut app, AgentEvent::RequestAccounting(Box::new(accounting(2))));

    let path = app.session.writer.take().expect("session writer").path().to_path_buf();
    let snapshots = session::SessionReader::read_records(&path)
        .into_iter()
        .filter_map(|record| match record {
            session::SessionRecord::ContextSnapshot { snapshot, .. } => Some(*snapshot),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| (snapshot.attempt, snapshot.state))
            .collect::<Vec<_>>(),
        vec![
            (1, session::ContextSnapshotState::Dispatched),
            (1, session::ContextSnapshotState::Failed),
            (2, session::ContextSnapshotState::Dispatched),
            (2, session::ContextSnapshotState::Completed),
        ]
    );
}

#[test]
fn completed_request_snapshot_tracks_tool_observations_and_transcript_links() {
    let mut app = fresh_app();
    app.refresh_context_ledger(Some("inspect tools"));
    let mut accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:1",
        1,
        "opencode",
        "big-pickle",
        br#"{"messages":[]}"#,
        Vec::new(),
    );
    accounting.tool_count = Some(1);

    handle_agent_event(&mut app, AgentEvent::RequestStarted(Box::new(accounting.clone())));
    handle_agent_event(&mut app, AgentEvent::RequestAccounting(Box::new(accounting)));
    handle_agent_event(
        &mut app,
        AgentEvent::ToolStarted {
            id: "call-1".to_string(),
            name: "read_file_range".to_string(),
            arguments: "{}".to_string(),
        },
    );
    handle_agent_event(
        &mut app,
        AgentEvent::ToolFinished {
            id: "call-1".to_string(),
            output: vec!["done".to_string()],
            status: ToolStatus::Ok,
            write_result: None,
            shell_result: None,
        },
    );

    let rendered = app
        .transcript
        .context_history
        .render_request(None)
        .expect("request details");
    assert!(rendered.contains("tools  1 · "));
    assert!(!rendered.contains("tools  1 · unknown"));
    assert!(rendered.contains("tool:call-1"));
    assert!(!rendered.contains("duration  unknown"));
}

#[test]
fn backspace_while_streaming_deletes_char() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(
        app.composer.input.as_str(),
        "hell",
        "backspace should work while streaming"
    );
}

#[test]
fn history_recall_while_streaming_works() {
    let mut app = working_app_with_streaming();
    app.composer.input_history.push("previous prompt".to_string());
    app.composer.input = PromptInput::from("current draft");

    update(&mut app, &key(KeyCode::Up, KeyModifiers::NONE));

    assert_eq!(
        app.composer.input.as_str(),
        "previous prompt",
        "Up should recall history while streaming"
    );
}

#[test]
fn file_mention_activation_while_working() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("check @src");

    update(&mut app, &key(KeyCode::Char('r'), KeyModifiers::NONE));

    assert!(
        app.composer.input.as_str().contains("@srcr"),
        "typing should append after @mention"
    );
    assert!(
        matches!(app.overlay.accessory(), PromptAccessory::Files(_)),
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
    app.refresh_context_ledger(None);

    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        rx.try_recv().expect("permission decision"),
        PermissionDecision::Selected("allow".to_string())
    );
    assert!(app.overlay.permission().is_none());
    assert!(app.transcript.context_ledger.is_none());
}

#[test]
fn acp_permission_escape_cancels_request() {
    let mut app = fresh_app();
    let (tx, rx) = mpsc::channel();
    update(
        &mut app,
        &Msg::Agent(AgentEvent::PermissionRequest(pending_permission(tx))),
    );
    app.refresh_context_ledger(None);

    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(
        rx.try_recv().expect("permission decision"),
        PermissionDecision::Cancelled
    );
    assert!(app.overlay.permission().is_none());
    assert!(app.transcript.context_ledger.is_none());
}

#[test]
fn acp_permission_run_cancel_responds_cancelled() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
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
    assert!(app.overlay.permission().is_none());
    assert_eq!(app.runtime.run_state, RunState::Idle);
}

#[test]
fn transcript_search_counts_unicode_matches_without_searching_tool_output() {
    let entries = vec![
        Entry::User { text: "find 🦀 then find".to_string() },
        Entry::Tool {
            name: "lookup".to_string(),
            arguments: "find public".to_string(),
            status: ToolStatus::Ok,
            output: vec!["find hidden".to_string()],
        },
    ]
    .into();
    let mut search = TranscriptSearchState::default();
    search.query.insert_str("find");

    search.refresh(&entries);

    assert_eq!(search.matches.len(), 3);
    assert_eq!(search.current().expect("first match").entry_index, 0);
    search.previous();
    assert_eq!(search.current().expect("wrapped previous match").entry_index, 1);
}

#[test]
fn queue_edits_are_cancelable_and_send_now_settles_only_the_selected_item() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft remains");
    let first = app
        .composer
        .queue
        .push(QueueTarget::FollowUp, "first".to_string(), "now".to_string());
    let second = app
        .composer
        .queue
        .push(QueueTarget::FollowUp, "second".to_string(), "later".to_string());
    app.overlay.show_queue();

    update(&mut app, &Msg::Action(Action::QueueEdit));
    update(&mut app, &Msg::Action(Action::InsertText(" changed".to_string())));
    update(&mut app, &Msg::Action(Action::Cancel));
    assert_eq!(app.composer.queue.item(first).expect("first item").text, "first");
    assert!(
        app.overlay.queue().is_some(),
        "cancel should leave the queue open after abandoning an edit"
    );

    update(&mut app, &Msg::Action(Action::QueueSendNow));

    assert_eq!(
        app.composer.queue.item(first).expect("first item").settlement,
        QueueSettlement::Sent
    );
    assert_eq!(
        app.composer.queue.item(second).expect("second item").settlement,
        QueueSettlement::Pending
    );
    assert_eq!(app.composer.input.as_str(), "draft remains");
}
