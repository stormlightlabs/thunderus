//! Application behavior tests for agent events seams.

use super::*;
use helpers::*;

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
    let mut skill = test_skill(
        skill_dir.join("SKILL.md"),
        "---\nname: review\ndescription: Reviews changes.\n---\n\n# Review\n\nInspect the diff.\n",
    );
    skill.name = "review".to_string();
    app.transcript.skills = vec![skill];
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
fn stopping_transitions_to_idle_on_cancelled_event() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.runtime.run_state, RunState::Stopping);
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.runtime.run_state, RunState::Idle);
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
    app.composer.input = PromptInput::from("inspect project");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    if let Some(message) = follow {
        update(&mut app, &message);
    }

    let config = AgentRunConfig::new(app.runtime.cwd.clone(), String::from("fake-agent"));
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
