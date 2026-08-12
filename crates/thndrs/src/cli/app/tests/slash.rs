use super::*;
use crate::{input::PromptInput, thndrs_core};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

fn with_isolated_setup_env<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = crate::test_env::lock();
    let old_home = std::env::var_os("HOME");
    let old_umans = std::env::var_os("UMANS_API_KEY");
    let old_opencode = std::env::var_os(thndrs_core::auth::OPENCODE_GO_KEY_ENV);
    let old_opencode_zen = std::env::var_os(thndrs_core::auth::OPENCODE_ZEN_KEY_ENV);
    let old_chatgpt = std::env::var_os(thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);

    unsafe {
        std::env::set_var("HOME", home);
        std::env::remove_var("UMANS_API_KEY");
        std::env::remove_var(thndrs_core::auth::OPENCODE_GO_KEY_ENV);
        std::env::remove_var(thndrs_core::auth::OPENCODE_ZEN_KEY_ENV);
        std::env::remove_var(thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
    }

    let result = f();

    unsafe {
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(value) = old_umans {
            std::env::set_var("UMANS_API_KEY", value);
        } else {
            std::env::remove_var("UMANS_API_KEY");
        }
        if let Some(value) = old_opencode {
            std::env::set_var(thndrs_core::auth::OPENCODE_GO_KEY_ENV, value);
        } else {
            std::env::remove_var(thndrs_core::auth::OPENCODE_GO_KEY_ENV);
        }
        if let Some(value) = old_opencode_zen {
            std::env::set_var(thndrs_core::auth::OPENCODE_ZEN_KEY_ENV, value);
        } else {
            std::env::remove_var(thndrs_core::auth::OPENCODE_ZEN_KEY_ENV);
        }
        if let Some(value) = old_chatgpt {
            std::env::set_var(thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV, value);
        } else {
            std::env::remove_var(thndrs_core::auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV);
        }
    }

    result
}

#[test]
fn slash_clear_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.transcript.entries.push(Entry::User { text: "keep me".to_string() });
    app.composer.input = PromptInput::from("/clear");

    let result = update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(result, None, "/clear should not execute while working");
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::User { text } if text == "keep me")),
        "transcript should not be cleared while an agent can still emit events"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/clear should be rejected with a status message"
    );
    assert!(app.composer.input.is_empty(), "input should be cleared after /clear");
}

#[test]
fn slash_help_while_working_executes_immediately() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("/help");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::Help,
        "/help should open help while working"
    );
    assert!(app.composer.input.is_empty(), "input should be cleared after /help");
}

#[test]
fn slash_model_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("/model");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::None,
        "/model should not open picker while working"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/model should be rejected with a status message"
    );
}

#[test]
fn slash_skills_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("/skills");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::None,
        "/skills should not open picker while working"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "/skills should be rejected with a status message"
    );
}

#[test]
fn slash_unknown_while_working_is_rejected() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("/unknown");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.composer.queue.pending_count(QueueTarget::FollowUp) == 0,
        "unknown slash command should not be queued as text"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("not available"))),
        "unknown slash command should be rejected with a status message"
    );
}

#[test]
fn double_slash_while_working_queues_literal_slash_followup() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("//clear after this run");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["/clear after this run"],
        "double slash should escape a literal slash-prefixed follow-up"
    );
    assert!(app.composer.input.is_empty(), "input should be cleared after queueing");
}

#[test]
fn slash_clear_clears_transcript_and_input() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::User { text: String::from("old") });
    app.composer.input = PromptInput::from("/clear");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.transcript.entries.is_empty());
    assert_eq!(app.composer.input.as_str(), "");
    assert!(!app.runtime.quit);
}

#[test]
fn slash_auth_config_and_doctor_append_redacted_output() {
    let mut app = fresh_app();

    app.composer.input = PromptInput::from("/auth status");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Status { text }) if text.contains("chatgpt-codex") && text.contains("opencode-zen")
    ));

    app.composer.input = PromptInput::from("/config path");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Status { text }) if text.contains("global:") && text.contains("project:"))
    );

    app.composer.input = PromptInput::from("/config show");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Status { text }) if text.contains("effective_config:"))
    );

    app.composer.input = PromptInput::from("/doctor");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(transcript.contains("thndrs doctor"));
    assert!(!transcript.contains("test-umans-key"));
}

#[test]
fn context_surface_is_bounded_and_does_not_render_source_content() {
    let mut app = fresh_app();
    let source = app.runtime.cwd.join("AGENTS.md");
    std::fs::write(&source, "api_key=source-secret-that-must-not-be-rendered\n").expect("write instructions");
    app.transcript.context_sources = vec![crate::context::load_agents_md(&app.runtime.cwd).expect("load instructions")];
    app.composer.input = PromptInput::from("/context all");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::Context);
    let table = app.render_context_table();
    let text = table
        .rows
        .iter()
        .flat_map(|row| row.iter())
        .map(|cell| cell.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("next request"));
    assert!(text.contains("projected input"));
    assert!(!text.contains("source-secret-that-must-not-be-rendered"));
    assert!(table.rows.len() <= 67, "context table must stay bounded");

    let item_id = app
        .transcript
        .context_ledger
        .as_ref()
        .and_then(|ledger| ledger.items.first())
        .map(|item| item.id.clone())
        .expect("context item");
    app.overlay.close();
    app.composer.input = PromptInput::from(format!("/context item {item_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Status { text })
            if text.contains("origin")
                && text.contains("lifecycle")
                && text.contains("estimate")
                && text.contains("artifact")
                && text.contains("protected")
                && text.contains("recovery")
    ));
}

#[test]
fn tokens_command_exposes_estimate_provider_components_and_error() {
    let mut app = fresh_app();
    let mut accounting = thndrs_agent::ProviderRequestAccounting::from_serialized_request(
        "turn_1",
        "turn_1:request:1",
        1,
        "fixture",
        "fixture-model",
        b"request",
        Vec::new(),
    );
    accounting.provider_usage = Some(
        thndrs_agent::ProviderUsageComponents {
            input_tokens: Some(100),
            output_tokens: Some(7),
            cache_read_input_tokens: Some(4),
            cache_creation_input_tokens: Some(2),
            reasoning_tokens: None,
        }
        .normalize("fixture", thndrs_agent::ProviderUsageRule::AnthropicMessages),
    );
    app.session.last_request_accounting = Some(accounting);
    app.composer.input = PromptInput::from("/tokens");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let output = app
        .transcript
        .entries
        .iter()
        .find_map(|entry| match entry {
            Entry::Status { text } if text.starts_with("tokens\n") => Some(text.as_str()),
            _ => None,
        })
        .expect("token inspection output");
    assert!(output.contains("estimated/"));
    assert!(output.contains("100 input / 7 output"));
    assert!(output.contains("cache: 4 read / 2 create"));
    assert!(output.contains("normalized input: 106"));
    assert!(output.contains("estimate error:"));
}

#[test]
fn context_export_command_writes_versioned_json_and_markdown() {
    let mut app = fresh_app();
    let json_path = app.runtime.cwd.join("context-export.json");
    app.composer.input = PromptInput::from("/context export context-export.json");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let json = std::fs::read_to_string(&json_path).expect("json export");
    assert!(json.contains("context-export-v1"));
    assert!(json.contains("model_projection"));

    let markdown_path = app.runtime.cwd.join("context-export.md");
    app.composer.input = PromptInput::from("/context export context-export.md markdown");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let markdown = std::fs::read_to_string(&markdown_path).expect("markdown export");
    assert!(markdown.starts_with("# Context export"));
    assert!(markdown.contains("## Context items"));
}

#[test]
fn context_pin_drop_recover_and_failed_pin_preserve_prompt_input() {
    let mut app = fresh_app();
    let file = app.runtime.cwd.join("notes.md");
    std::fs::write(&file, "private notes").expect("write file");

    app.composer.input = PromptInput::from("/context pin notes.md");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.composer.input.is_empty(),
        "successful context action clears its command"
    );
    let pinned_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("ledger")
        .items
        .iter()
        .find(|item| item.kind == thndrs_agent::context::ContextItemKind::PinnedFile)
        .expect("pinned item")
        .id
        .clone();

    app.composer.input = PromptInput::from(format!("/context drop {pinned_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("ledger")
            .items
            .iter()
            .any(|item| item.id == pinned_id && item.visibility == thndrs_agent::context::ContextVisibility::Dropped)
    );

    app.composer.input = PromptInput::from(format!("/context recover {pinned_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("ledger")
            .items
            .iter()
            .any(|item| item.id == pinned_id && item.visibility == thndrs_agent::context::ContextVisibility::Pinned)
    );
    let recovered = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("ledger after recovery")
        .find(&pinned_id)
        .expect("recovered item");
    assert!(recovered.lifecycle.relations.iter().any(|relation| {
        relation.kind == thndrs_agent::context::ContextRelationKind::RecoveredFrom
            && relation.status == thndrs_agent::context::ContextRelationStatus::Applied
    }));

    app.composer.input = PromptInput::from("/context pin missing.md");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.composer.input.as_str(), "/context pin missing.md");
    assert!(format!("{:?}", app.transcript.entries).contains("cannot pin"));
}

#[test]
fn context_verification_requires_review_before_release_and_survives_refresh() {
    let mut app = fresh_app();
    let writer = session::SessionWriter::create(
        &session::sessions_dir(&app.runtime.cwd),
        "verification-recording",
        &app.runtime.cwd.display().to_string(),
        "verification test",
        "umans",
        "umans-coder",
        "none",
        env!("CARGO_PKG_VERSION"),
        None,
    )
    .expect("create verification session");
    let session_path = writer.path().to_path_buf();
    app.session.writer = Some(writer);
    let file = app.runtime.cwd.join("unverified.md");
    std::fs::write(&file, "pending edit").expect("write file");

    app.composer.input = PromptInput::from("/context pin unverified.md");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let evidence_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.kind == thndrs_agent::context::ContextItemKind::PinnedFile)
        .expect("pinned evidence")
        .id
        .clone();

    app.transcript
        .entries
        .push(Entry::User { text: "run the verification check".to_string() });
    app.refresh_context_ledger(None);
    let candidate_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.label == "user")
        .expect("candidate result")
        .id
        .clone();

    app.composer.input = PromptInput::from(format!("/context verify propose {evidence_id} {candidate_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let relation_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .find(&evidence_id)
        .expect("evidence")
        .lifecycle
        .verification_relations()
        .next()
        .expect("proposed relation")
        .id
        .clone();
    assert!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("context ledger")
            .find(&evidence_id)
            .expect("evidence")
            .lifecycle
            .is_protected()
    );

    app.composer.input = PromptInput::from(format!("/context verify approve {relation_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let approved = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .find(&evidence_id)
        .expect("evidence")
        .lifecycle
        .relation(&relation_id)
        .expect("approved relation");
    assert_eq!(approved.status, thndrs_agent::context::ContextRelationStatus::Approved);
    assert!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("context ledger")
            .find(&evidence_id)
            .expect("evidence")
            .lifecycle
            .is_protected()
    );

    app.composer.input = PromptInput::from(format!("/context verify release {relation_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let released = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .find(&evidence_id)
        .expect("evidence")
        .lifecycle
        .clone();
    assert!(!released.is_protected());
    assert_eq!(
        released.relation(&relation_id).expect("released relation").status,
        thndrs_agent::context::ContextRelationStatus::Released
    );

    app.refresh_context_ledger(None);
    assert_eq!(
        app.transcript
            .context_ledger
            .as_ref()
            .expect("refreshed context ledger")
            .find(&evidence_id)
            .expect("evidence after refresh")
            .lifecycle,
        released
    );

    let records = {
        let writer = app.session.writer.take().expect("verification session writer");
        assert_eq!(writer.path(), session_path.as_path());
        drop(writer);
        session::SessionReader::read_records(&session_path)
    };
    let lifecycle_statuses = records
        .iter()
        .filter_map(|record| match record {
            session::SessionRecord::ContextLifecycle { audit, .. } => audit
                .item
                .lifecycle
                .relation(&relation_id)
                .map(|relation| relation.status),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle_statuses,
        vec![
            thndrs_agent::context::ContextRelationStatus::Proposed,
            thndrs_agent::context::ContextRelationStatus::Approved,
            thndrs_agent::context::ContextRelationStatus::Released,
        ]
    );

    let cli = Cli { cwd: app.runtime.cwd.clone(), model: "umans-coder".to_string(), ..Cli::default() };
    let mut resumed = App::from_cli(&cli);
    resumed.restore_context_state(&records);
    resumed.refresh_context_ledger(None);
    let resumed_lifecycle = resumed
        .transcript
        .context_ledger
        .as_ref()
        .expect("resumed context ledger")
        .find(&evidence_id)
        .expect("resumed evidence")
        .lifecycle
        .clone();
    assert!(!resumed_lifecycle.is_protected());
    assert_eq!(
        resumed_lifecycle
            .relation(&relation_id)
            .expect("resumed relation")
            .status,
        thndrs_agent::context::ContextRelationStatus::Released
    );
}

#[test]
fn context_verification_record_failure_does_not_mutate_in_memory_state() {
    let mut app = fresh_app();
    let writer = session::SessionWriter::create(
        &session::sessions_dir(&app.runtime.cwd),
        "verification-failure",
        &app.runtime.cwd.display().to_string(),
        "verification failure test",
        "umans",
        "umans-coder",
        "none",
        env!("CARGO_PKG_VERSION"),
        None,
    )
    .expect("create verification session");
    let session_path = writer.path().to_path_buf();
    app.session.writer = Some(writer);

    let file = app.runtime.cwd.join("failure.md");
    std::fs::write(&file, "failed write evidence").expect("write file");
    app.composer.input = PromptInput::from("/context pin failure.md");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let evidence_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.kind == thndrs_agent::context::ContextItemKind::PinnedFile)
        .expect("pinned evidence")
        .id
        .clone();
    app.transcript
        .entries
        .push(Entry::User { text: "check the failed write".to_string() });
    app.refresh_context_ledger(None);
    let candidate_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.label == "user")
        .expect("candidate")
        .id
        .clone();
    let relation_id = app
        .propose_context_verification(&evidence_id, &candidate_id)
        .expect("propose verification");

    std::fs::remove_file(&session_path).expect("remove session file to force append failure");
    let error = app
        .approve_context_verification(&relation_id)
        .expect_err("approval should fail when the audit append fails");
    assert!(error.contains("failed to record context lifecycle action"));
    let lifecycle = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger after failed append")
        .find(&evidence_id)
        .expect("evidence after failed append")
        .lifecycle
        .clone();
    assert_eq!(
        lifecycle.relation(&relation_id).expect("proposal remains").status,
        thndrs_agent::context::ContextRelationStatus::Proposed
    );
    assert!(lifecycle.is_protected());
}

#[test]
fn context_verification_rejection_survives_refresh_and_keeps_protection() {
    let mut app = fresh_app();
    let file = app.runtime.cwd.join("rejected.md");
    std::fs::write(&file, "evidence awaiting review").expect("write file");
    app.composer.input = PromptInput::from("/context pin rejected.md");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let evidence_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.kind == thndrs_agent::context::ContextItemKind::PinnedFile)
        .expect("pinned evidence")
        .id
        .clone();
    app.transcript
        .entries
        .push(Entry::User { text: "review the evidence".to_string() });
    app.refresh_context_ledger(None);
    let candidate_id = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.label == "user")
        .expect("candidate")
        .id
        .clone();
    let relation_id = app
        .propose_context_verification(&evidence_id, &candidate_id)
        .expect("propose verification");

    app.composer.input = PromptInput::from(format!("/context verify reject {relation_id}"));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.refresh_context_ledger(None);
    let lifecycle = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("refreshed context ledger")
        .find(&evidence_id)
        .expect("evidence")
        .lifecycle
        .clone();
    assert!(lifecycle.is_protected());
    assert_eq!(
        lifecycle.relation(&relation_id).expect("rejected relation").status,
        thndrs_agent::context::ContextRelationStatus::Rejected
    );
}

#[test]
fn successful_commands_and_assistant_prose_never_release_write_protection() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::Tool {
        name: "create_file#successful-write".to_string(),
        arguments: "{}".to_string(),
        status: ToolStatus::Ok,
        output: vec!["created file".to_string()],
    });
    app.transcript
        .entries
        .push(Entry::Agent { text: "The verification command succeeded.".to_string(), streaming: false });
    app.refresh_context_ledger(None);

    let write_item = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger")
        .items
        .iter()
        .find(|item| item.label == "tool:create_file#successful-write")
        .expect("write evidence");
    assert!(write_item.lifecycle.is_protected());
    assert!(
        write_item
            .lifecycle
            .protection
            .contains(thndrs_agent::context::ContextProtectionReason::UnverifiedWriteEdit)
    );

    app.composer.input = PromptInput::from("/context show");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let write_item = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("context ledger after command")
        .items
        .iter()
        .find(|item| item.label == "tool:create_file#successful-write")
        .expect("write evidence after command");
    assert!(write_item.lifecycle.is_protected());
}

#[test]
fn context_reclassification_adds_failure_and_recovery_protection() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::Tool {
        name: "run_shell#dynamic-tool".to_string(),
        arguments: "{}".to_string(),
        status: ToolStatus::Running,
        output: Vec::new(),
    });
    app.refresh_context_ledger(None);
    let initial = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("initial context ledger")
        .items
        .iter()
        .find(|item| item.label == "tool:run_shell#dynamic-tool")
        .expect("running tool")
        .lifecycle
        .clone();
    assert!(!initial.is_protected());

    let Some(Entry::Tool { status, .. }) = app.transcript.entries.last_mut() else {
        panic!("running tool transcript entry");
    };
    *status = ToolStatus::Failed;
    app.transcript
        .tool_artifacts
        .insert("dynamic-tool".to_string(), "artifact_dynamic".to_string());
    app.refresh_context_ledger(None);
    let reclassified = app
        .transcript
        .context_ledger
        .as_ref()
        .expect("reclassified context ledger")
        .items
        .iter()
        .find(|item| item.label == "tool:run_shell#dynamic-tool")
        .expect("failed tool")
        .lifecycle
        .clone();
    assert!(
        reclassified
            .protection
            .contains(thndrs_agent::context::ContextProtectionReason::FailureEvidence)
    );
    assert!(
        reclassified
            .protection
            .contains(thndrs_agent::context::ContextProtectionReason::RecoveryMetadata)
    );
}

#[test]
fn slash_config_edit_reports_cli_only() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/config edit");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Status { text }) if text.contains("config edit is CLI-only")
    ));
}

#[test]
fn slash_command_rejects_api_key_like_extra_argument() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/login umans sk-secret-should-not-appear");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.setup().is_none());
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(transcript.contains("do not accept API keys"));
    assert!(!transcript.contains("sk-secret-should-not-appear"));

    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/login chatgpt-codex access_token=secret-should-not-appear");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.setup().is_none());
    let transcript = format!("{:?}", app.transcript.entries);
    assert!(transcript.contains("do not accept API keys"));
    assert!(!transcript.contains("secret-should-not-appear"));
}

#[test]
fn slash_logout_rejects_retired_umans_provider() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/logout umans");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.setup().is_none());
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Error { text }) if text.contains("no longer supported")
    ));
}

#[test]
fn slash_chatgpt_codex_logout_stays_cli_only() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/logout chatgpt-codex");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.overlay.setup().is_none());
    assert!(matches!(
        app.transcript.entries.last(),
        Some(Entry::Status { text }) if text.contains("ChatGPT Codex logout is CLI-only")
    ));
}

#[test]
fn slash_setup_and_login_open_recovery_surfaces() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/setup");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.overlay.setup().map(|recovery| recovery.stage),
        Some(RecoveryStage::ChooseProvider)
    ));

    app.overlay.close();
    app.composer.input = PromptInput::from("/login opencode-go");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let recovery = app.overlay.setup().expect("login recovery");
    assert_eq!(recovery.stage, RecoveryStage::EnterKey);
    assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeGo));
}

#[test]
fn slash_setup_uses_chatgpt_provider_aware_recovery_for_chatgpt_model() {
    let mut app = fresh_app();
    app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
    app.runtime.cli.model = app.runtime.model.clone();
    app.composer.input = PromptInput::from("/setup");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.overlay.setup().expect("setup recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChooseProvider);
    assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
}

#[test]
fn slash_setup_selects_opencode_zen_and_prompts_for_api_key() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("create home");
    with_isolated_setup_env(&home, || {
        let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.composer.input = PromptInput::from("/setup");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let recovery = app.overlay.setup().expect("credential entry");
        assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
        assert_eq!(recovery.provider, Some(SetupProviderArg::OpencodeZen));
        assert!(app.runtime.model.is_empty(), "authentication precedes model selection");
    });
}

#[test]
fn slash_setup_can_choose_chatgpt_and_write_project_model() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("create home");
    std::fs::create_dir_all(&workspace).expect("create workspace");
    with_isolated_setup_env(&home, || {
        let cli = Cli { cwd: workspace.clone(), ..Cli::default() };
        let mut app = App::from_cli(&cli);
        app.session.writer = None;
        app.overlay.close();
        app.runtime.model = "chatgpt-codex/gpt-5.5".to_string();
        app.runtime.cli.model = app.runtime.model.clone();
        app.composer.input = PromptInput::from("/setup");

        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
        update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

        let recovery = app.overlay.setup().expect("chatgpt recovery");
        assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
        assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
    });
}

#[test]
fn slash_chatgpt_codex_login_opens_oauth_recovery_surface() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/login chatgpt-codex");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.overlay.setup().expect("login recovery");
    assert_eq!(recovery.stage, RecoveryStage::MissingCredential);
    assert_eq!(recovery.provider, Some(SetupProviderArg::ChatgptCodex));
}

#[test]
fn slash_chatgpt_codex_login_surface_starts_tui_oauth() {
    let mut app = fresh_app();
    *app.overlay.oauth_driver_mut() = ChatGptOAuthDriver {
        request_device_code: oauth_request_ok,
        poll_device_code_once: oauth_poll_pending,
        write_credentials: oauth_write_ok,
        ..Default::default()
    };
    app.composer.input = PromptInput::from("/login chatgpt-codex");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Down, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let recovery = app.overlay.setup().expect("oauth recovery");
    assert_eq!(recovery.stage, RecoveryStage::ChatGptOAuthPolling);
    assert!(recovery.chatgpt_oauth.is_some());
}

#[test]
fn slash_quit_sets_quit_flag() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/quit");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.runtime.quit);
    assert_eq!(follow, Some(Msg::Quit));
    assert_eq!(app.composer.input.as_str(), "");
}

#[test]
fn slash_exit_also_quits() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/exit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.runtime.quit);
}

#[test]
fn unknown_slash_command_is_ignored() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/bogus");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(!app.runtime.quit);
    assert!(app.transcript.entries.is_empty());
    assert_eq!(app.composer.input.as_str(), "/bogus");
}

#[test]
fn slash_mcp_lists_empty_config() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut app = fresh_app();
    app.runtime.cwd = temp.path().to_path_buf();
    app.composer.input = PromptInput::from("/mcp");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Status { text }) if text.contains("no MCP servers configured"))
    );
}

#[test]
fn slash_mcp_tools_requires_name() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/mcp tools ");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("usage: /mcp tools <name>"))
    );
}
