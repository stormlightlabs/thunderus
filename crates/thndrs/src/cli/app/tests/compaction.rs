//! Application behavior tests for compaction seams.

use super::*;
use helpers::*;

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
