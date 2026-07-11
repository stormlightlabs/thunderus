use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use thndrs_agent::CancelToken;

use helpers::*;

#[test]
fn ctrl_a_in_command_mode_inserts_literal_a() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("test");
    update(&mut app, &key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.input.as_str(), "testa");
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
    assert_eq!(app.mode, Mode::Command);
}

#[test]
fn colon_enters_command_mode() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.prompt_accessory, PromptAccessory::Commands { selected: 0 });
    assert!(app.input.is_empty());
}

#[test]
fn colon_does_not_enter_command_mode_while_working() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt, "should not enter command mode while working");
}

#[test]
fn command_mode_typing_appends_to_input() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
    );
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)),
    );
    assert_eq!(app.input.as_str(), "cl");
    assert_eq!(app.mode, Mode::Command);
}

#[test]
fn session_commands_are_suggested() {
    let app = fresh_app();
    let suggestions = command_suggestions_for_app(&app);

    for command in ["history", "resume", "session", "tokens", "debug log"] {
        assert!(
            suggestions.iter().any(|(suggestion, _)| *suggestion == command),
            "missing {command}"
        );
    }
}

#[test]
fn read_only_session_command_failure_preserves_prompt_draft() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/session missing");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.input.as_str(), "/session missing");
    assert!(matches!(app.transcript.last(), Some(Entry::Error { text }) if text.contains("not found")));
}

#[test]
fn resume_restores_transcript_and_usage_without_live_run_state() {
    let mut app = fresh_app();
    let sessions_dir = session::sessions_dir(&app.cwd);
    let mut writer = session::SessionWriter::create(
        &sessions_dir,
        "session-resume",
        &app.cwd.display().to_string(),
        "Saved work",
        "umans",
        "umans-coder",
        "none",
        "0.1.0",
        None,
    )
    .expect("create session");
    writer
        .append_entry(&Entry::User { text: "earlier prompt".to_string() }, "turn_1")
        .expect("append user");
    writer.append_usage(7, 11).expect("append usage");
    drop(writer);

    app.run_state = RunState::Error("stale state".to_string());
    app.queued_followups.push("stale queue".to_string());
    app.input = PromptInput::from("/resume session-res");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.session_id, "session-resume");
    assert_eq!(app.run_state, RunState::Idle);
    assert!(app.queued_followups.is_empty());
    assert_eq!(app.session_tokens_in, 7);
    assert_eq!(app.session_tokens_out, 11);
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text == "earlier prompt"))
    );
    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::Status { text } if text == "resumed session: session-resume"))
    );
}

#[test]
fn command_mode_enter_executes_and_returns_to_prompt() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
    app.input = PromptInput::from("clear");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert!(app.input.is_empty());
    assert!(app.transcript.is_empty(), "clear should clear the transcript");
}

#[test]
fn command_mode_enter_completes_partial_command() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
    app.input = PromptInput::from("cl");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.input.as_str(), "clear ");
}

#[test]
fn command_mode_esc_returns_to_prompt() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("qui");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert!(app.input.is_empty());
}

#[test]
fn command_mode_backspace_on_empty_returns_to_prompt() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input.clear();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt);
}

#[test]
fn command_mode_backspace_on_nonempty_pops_char() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("cl");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.input.as_str(), "c");
    assert_eq!(app.mode, Mode::Command);
}

#[test]
fn command_mode_quit_command_exits_app() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("quit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
}

#[test]
fn command_mode_help_command_enters_help_overlay() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("help");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::Help);
}

#[test]
fn bg_command_with_no_processes_shows_empty_message() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("bg");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("no background"))),
        "should show no background processes"
    );
}

#[test]
fn bg_command_lists_registered_background_processes() {
    let mut app = fresh_app();
    let cancel = CancelToken::new();
    let id = app.process_registry.register(
        vec!["cargo".to_string(), "build".to_string()],
        std::path::PathBuf::from("."),
        tools::shell::ProcessKind::Background,
        cancel,
    );

    app.mode = Mode::Command;
    app.input = PromptInput::from("bg");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    let status_text = app
        .transcript
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
fn model_command_opens_picker_and_selects_model() {
    let mut app = fresh_app();
    update(&mut app, &key(KeyCode::Char(':'), KeyModifiers::NONE));
    for ch in "model".chars() {
        update(&mut app, &key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.prompt_accessory, PromptAccessory::Models);
    assert!(app.picker.is_some());

    if let Some(picker) = app.picker.as_mut() {
        picker.query = "glm-5.2".to_string();
        picker.refresh_matches();
    }
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.model, "umans-glm-5.2");
    assert!(app.picker.is_none());
}

#[test]
fn skills_command_opens_picker_and_renders_selected_skill_markdown() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let markdown = "---\nname: example-skill\ndescription: Helps test.\n---\n# Example Skill\n\nUse carefully.\n";
    let mut app = fresh_app();
    app.skills = vec![test_skill(dir.path().join("example-skill").join("SKILL.md"), markdown)];
    app.input = PromptInput::from("/skills");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.prompt_accessory, PromptAccessory::Skills);

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert!(app.picker.is_none());
    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Agent { text, streaming: false }
            if text.contains("# Skill: example-skill") && text.contains("# Example Skill")
    )));
}

#[test]
fn skills_command_surfaces_activation_reference_diagnostics() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let markdown = "---\nname: example-skill\ndescription: Helps test.\n---\n# Example Skill\n";
    let mut skill = test_skill(dir.path().join("example-skill").join("SKILL.md"), markdown);
    skill.references = vec![PathBuf::from("missing.md")];

    let mut app = fresh_app();
    app.skills = vec![skill];
    app.input = PromptInput::from("/skills");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Error { text } if text.contains("missing.md") && text.contains("does not exist")
    )));
    assert!(app.transcript.iter().any(|entry| matches!(
        entry,
        Entry::Agent { text, streaming: false } if text.contains("# Example Skill")
    )));
}
