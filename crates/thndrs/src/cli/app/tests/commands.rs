use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use thndrs_agent::CancelToken;

use helpers::*;

#[test]
fn ctrl_a_in_command_mode_inserts_literal_a() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("test");
    update(&mut app, &key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.composer.input.as_str(), "testa");
}

#[test]
fn colon_enters_command_mode_from_error_state() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Command);
}

#[test]
fn colon_enters_command_mode() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Command);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Commands { selected: 0 });
    assert!(app.composer.input.is_empty());
}

#[test]
fn colon_does_not_enter_command_mode_while_working() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
    );
    assert_eq!(
        app.composer.mode,
        Mode::Prompt,
        "should not enter command mode while working"
    );
}

#[test]
fn command_mode_typing_appends_to_input() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
    );
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.input.as_str(), "cl");
    assert_eq!(app.composer.mode, Mode::Command);
}

#[test]
fn session_commands_are_suggested() {
    let app = fresh_app();
    let suggestions = command_suggestions_for_app(&app);

    for command in ["history", "resume", "name", "session", "status", "tokens", "debug log"] {
        assert!(
            suggestions.iter().any(|suggestion| suggestion.name == command),
            "missing {command}"
        );
    }
}

#[test]
fn status_command_shows_secondary_telemetry() {
    let mut app = fresh_app();
    app.runtime.session_tokens_in = 12;
    app.runtime.session_tokens_out = 7;
    app.composer.input = PromptInput::from("/status");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let Some(Entry::Status { text }) = app.transcript.entries.last() else {
        panic!("status command should append a status entry");
    };
    assert!(text.contains("state: Ready"));
    assert!(text.contains("session tokens: 12 in / 7 out"));
    assert!(text.contains("quota:"));
    assert!(text.contains("workspace:"));
}

#[test]
fn read_only_session_command_failure_preserves_prompt_draft() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/session missing");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.composer.input.as_str(), "/session missing");
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("not found")));
}

#[test]
fn ephemeral_runs_cannot_resume_a_durable_session() {
    let mut app = fresh_app();
    app.session.run_persistence = RunPersistence::Ephemeral;

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)),
    );
    for ch in "resume previous".chars() {
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
        );
    }
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.session.writer.is_none());
    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("ephemeral mode")),
        "ephemeral mode must reject session resumption"
    );
}

#[test]
fn resume_restores_transcript_and_usage_without_live_run_state() {
    let mut app = fresh_durable_app();
    let sessions_dir = session::sessions_dir(&app.runtime.cwd);
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-resume",
        &app.runtime.cwd.display().to_string(),
        "Saved work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer
        .append_entry(&Entry::User { text: "earlier prompt".to_string() }, "turn_1")
        .expect("append user");
    writer.append_usage(7, 11).expect("append usage");
    writer
        .append_queued(42, "follow-up", "add", "persisted queue")
        .expect("append queue");
    drop(writer);

    app.runtime.run_state = RunState::Error("stale state".to_string());
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "stale queue".to_string(), "test".to_string());
    app.composer.input = PromptInput::from("/resume");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::Sessions);
    let picker = app.overlay.picker().expect("session picker");
    assert_eq!(picker.matches[0].label, "Resume — Saved work");
    assert!(picker.matches[0].detail.contains("session-resume"));
    assert!(picker.matches[0].detail.contains("opencode/big-pickle"));

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session.id, "session-resume");
    assert_eq!(app.runtime.run_state, RunState::Idle);
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| (item.id, item.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(QueueItemId(42), "persisted queue")]
    );
    assert_eq!(app.runtime.session_tokens_in, 7);
    assert_eq!(app.runtime.session_tokens_out, 11);
    assert_eq!(app.session.turn_count, 1);
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "earlier prompt"))
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text == "resumed session: session-resume"))
    );
}

#[test]
fn cancelling_the_session_picker_preserves_the_current_session_and_draft() {
    let mut app = fresh_durable_app();
    let current_id = app.session.id.clone();
    let sessions_dir = session::sessions_dir(&app.runtime.cwd);
    session::SessionWriter::create(
        &sessions_dir,
        "session-other",
        &app.runtime.cwd.display().to_string(),
        "Other work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    app.composer.input = PromptInput::from("/resume");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(app.session.id, current_id);
    assert_eq!(app.composer.input.as_str(), "/resume");
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
}

#[test]
fn locked_session_picker_selection_preserves_the_current_session_and_draft() {
    let mut app = fresh_durable_app();
    let current_id = app.session.id.clone();
    let sessions_dir = session::sessions_dir(&app.runtime.cwd);
    let _locked = session::SessionWriter::create(
        &sessions_dir,
        "session-locked",
        &app.runtime.cwd.display().to_string(),
        "Locked work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create locked session");
    app.composer.input = PromptInput::from("/resume");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session.id, current_id);
    assert_eq!(app.composer.input.as_str(), "/resume");
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("active writer")));
}

#[test]
fn corrupt_session_picker_selection_preserves_the_current_session_and_draft() {
    let mut app = fresh_durable_app();
    let current_id = app.session.id.clone();
    let sessions_dir = session::sessions_dir(&app.runtime.cwd);
    let writer = session::SessionWriter::create(
        &sessions_dir,
        "session-corrupt",
        &app.runtime.cwd.display().to_string(),
        "Corrupt work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    let path = writer.path().to_path_buf();
    drop(writer);
    std::fs::write(&path, "not json\n").expect("corrupt session");
    app.composer.input = PromptInput::from("/resume");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session.id, current_id);
    assert_eq!(app.composer.input.as_str(), "/resume");
    assert!(
        matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("corrupt")),
        "unexpected transcript entry: {:?}",
        app.transcript.entries.last()
    );
}

#[test]
fn missing_session_picker_selection_preserves_the_current_session_and_draft() {
    let mut app = fresh_durable_app();
    let current_id = app.session.id.clone();
    let sessions_dir = session::sessions_dir(&app.runtime.cwd);
    let writer = session::SessionWriter::create(
        &sessions_dir,
        "session-removed",
        &app.runtime.cwd.display().to_string(),
        "Removed work",
        "opencode-go",
        "opencode/big-pickle",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    let path = writer.path().to_path_buf();
    drop(writer);
    app.composer.input = PromptInput::from("/resume");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    std::fs::remove_file(path).expect("remove selected session");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session.id, current_id);
    assert_eq!(app.composer.input.as_str(), "/resume");
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("not found")));
}

#[test]
fn name_command_appends_changes_without_replacing_the_session_id() {
    let mut app = fresh_durable_app();
    let id = app.session.id.clone();
    let path = session::resolve_session_file(&session::sessions_dir(&app.runtime.cwd), &id).expect("current session");
    app.session.writer = Some(session::SessionWriter::resume(&path, &id).expect("resume current writer"));

    app.composer.input = PromptInput::from("/name First name");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    app.composer.input = PromptInput::from("/name Changed name");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session.id, id);
    assert_eq!(session::SessionReader::read_title(&path), "Changed name");
    assert_eq!(
        session::SessionReader::read_records(&path)
            .iter()
            .filter(|record| matches!(record, session::SessionRecord::SessionRenamed { .. }))
            .count(),
        2
    );
}

#[test]
fn invalid_name_command_preserves_the_prompt_draft() {
    let mut app = fresh_durable_app();
    let id = app.session.id.clone();
    let path = session::resolve_session_file(&session::sessions_dir(&app.runtime.cwd), &id).expect("current session");
    app.session.writer = Some(session::SessionWriter::resume(&path, &id).expect("resume current writer"));
    let draft = format!("/name {}", "x".repeat(session::MAX_SESSION_NAME_CHARS + 1));
    app.composer.input = PromptInput::from(draft.as_str());

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.composer.input.as_str(), draft);
    assert!(matches!(app.transcript.entries.last(), Some(Entry::Error { text }) if text.contains("cannot exceed")));
}

#[test]
fn command_mode_enter_executes_and_returns_to_prompt() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.overlay.show_commands();
    app.composer.input = PromptInput::from("clear");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert!(app.composer.input.is_empty());
    assert!(app.transcript.entries.is_empty(), "clear should clear the transcript");
}

#[test]
fn command_mode_enter_completes_partial_command() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.overlay.show_commands();
    app.composer.input = PromptInput::from("cl");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.composer.mode, Mode::Command);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "clear ");
}

#[test]
fn command_mode_esc_returns_to_prompt() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("qui");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert!(app.composer.input.is_empty());
}

#[test]
fn command_mode_backspace_on_empty_returns_to_prompt() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input.clear();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.mode, Mode::Prompt);
}

#[test]
fn command_mode_backspace_on_nonempty_pops_char() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("cl");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.input.as_str(), "c");
    assert_eq!(app.composer.mode, Mode::Command);
}

#[test]
fn command_mode_quit_command_exits_app() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("quit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.runtime.quit);
}

#[test]
fn command_mode_help_command_enters_help_overlay() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("help");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::Help);
}

#[test]
fn bg_command_with_no_processes_shows_empty_message() {
    let mut app = fresh_app();
    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("bg");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("no background"))),
        "should show no background processes"
    );
}

#[test]
fn bg_command_lists_registered_background_processes() {
    let mut app = fresh_app();
    let cancel = CancelToken::new();
    let id = app.runtime.process_registry.register(
        vec!["cargo".to_string(), "build".to_string()],
        std::path::PathBuf::from("."),
        tools::shell::ProcessKind::Background,
        cancel,
    );

    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from("bg");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    let status_text = app
        .transcript
        .entries
        .iter()
        .rev()
        .find_map(|e| match e {
            Entry::Status { text } if text.contains("background processes") => Some(text.clone()),
            _ => None,
        })
        .expect("should have a background processes status entry");

    assert!(status_text.contains(&format!("[{id}]")), "should list process id {id}");
    assert!(status_text.contains("cargo build"), "should list the command");
}

#[test]
fn bg_cancel_terminates_real_owned_child() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("temp dir");
    let args = crate::tools::shell::ShellArgs {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "exec sleep 30".to_string()],
        cwd: None,
        timeout: Some(std::time::Duration::from_secs(60)),
        kind: crate::tools::shell::ProcessKind::Background,
    };
    let result = crate::tools::shell::run_command_with_registry(
        &args,
        dir.path(),
        &CancelToken::new(),
        Some(&app.runtime.process_registry),
    )
    .expect("spawn background child");
    let id = result.process_id.expect("process id");
    app.runtime.process_registry.announce(id);

    app.composer.mode = Mode::Command;
    app.composer.input = PromptInput::from(format!("bg cancel {id}"));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(
        app.runtime.process_registry.get(id).is_some(),
        "cancellation is asynchronous"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| { matches!(entry, Entry::Status { text } if text.contains("cancellation requested")) })
    );

    for _ in 0..50 {
        let result = update_with_effects(&mut app, &Msg::Tick);
        assert!(result.effects.contains(&Effect::DrainBackgroundProcesses));
        let completed = app.runtime.process_registry.drain_completed();
        if !completed.is_empty() {
            update_with_effects(&mut app, &Msg::Effect(EffectResult::BackgroundProcesses(completed)));
        }
        if app.runtime.process_registry.get(id).is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        app.runtime.process_registry.get(id).is_none(),
        "child should be reaped on a tick"
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|entry| { matches!(entry, Entry::Status { text } if text.contains("cancelled")) })
    );
}

#[test]
fn model_command_opens_picker_and_selects_model() {
    let mut app = fresh_app();
    update(&mut app, &key(KeyCode::Char(':'), KeyModifiers::NONE));
    for ch in "model".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::Models);
    assert!(app.overlay.picker().is_some());

    if let Some(picker) = app.overlay.picker_mut() {
        picker.query = "opencode/gpt-5.6-luna".to_string();
        picker.refresh_matches();
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::ReasoningEffort);
    assert_eq!(app.runtime.model, "opencode/gpt-5.6-luna");
    assert!(app.overlay.picker().is_some());
}

#[test]
fn gpt_5_6_model_selection_prompts_for_and_saves_reasoning_effort() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("/model");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));
    let picker = app.overlay.picker_mut().expect("model picker");
    picker.query = "opencode/gpt-5.6-terra".to_string();
    picker.refresh_matches();

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.runtime.model, "opencode/gpt-5.6-terra");
    assert!(
        app.composer.input.is_empty(),
        "the typed /model command must not remain in the prompt"
    );
    assert_eq!(app.overlay.accessory(), PromptAccessory::ReasoningEffort);
    let picker = app.overlay.picker_mut().expect("reasoning effort picker");
    picker.query = "xhigh".to_string();
    picker.refresh_matches();

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.runtime.cli.reasoning_effort, ReasoningEffort::Xhigh);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(
        std::fs::read_to_string(app.runtime.cwd.join(".thndrs").join("config.toml")).expect("read config"),
        "model = \"opencode/gpt-5.6-terra\"\nreasoning_effort = \"xhigh\"\n"
    );
}

#[test]
fn reasoning_command_opens_the_current_effort_picker() {
    let mut app = fresh_app();
    app.runtime.model = "opencode/gpt-5.6-sol".to_string();
    app.runtime.cli.model = app.runtime.model.clone();
    app.runtime.cli.reasoning_effort = ReasoningEffort::High;
    app.composer.input = PromptInput::from("/reasoning");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay.accessory(), PromptAccessory::ReasoningEffort);
    assert_eq!(
        app.overlay
            .picker()
            .as_ref()
            .expect("reasoning picker")
            .selected()
            .expect("selection")
            .label,
        "high"
    );
}

#[test]
fn skills_command_opens_picker_and_renders_selected_skill_markdown() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let markdown = "---\nname: example-skill\ndescription: Helps test.\n---\n# Example Skill\n\nUse carefully.\n";
    let mut app = fresh_app();
    app.transcript.skills = vec![test_skill(dir.path().join("example-skill").join("SKILL.md"), markdown)];
    app.composer.input = PromptInput::from("/skills");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.overlay.accessory(), PromptAccessory::Skills);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert!(app.overlay.picker().is_none());
    assert!(app.composer.input.is_empty());
    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Skill { name, content, token_estimate, context_percent: Some(_), .. }
            if name == "example-skill"
                && content.contains("# Skill: example-skill")
                && content.contains("# Example Skill")
                && *token_estimate > 0
    )));
}

#[test]
fn skills_command_surfaces_activation_reference_diagnostics() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let markdown = "---\nname: example-skill\ndescription: Helps test.\n---\n# Example Skill\n";
    let mut skill = test_skill(dir.path().join("example-skill").join("SKILL.md"), markdown);
    skill.references = vec![PathBuf::from("missing.md")];

    let mut app = fresh_app();
    app.transcript.skills = vec![skill];
    app.composer.input = PromptInput::from("/skills");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Error { text } if text.contains("missing.md") && text.contains("does not exist")
    )));
    assert!(app.transcript.entries.iter().any(|entry| matches!(
        entry,
        Entry::Skill { content, .. } if content.contains("# Example Skill")
    )));
}
