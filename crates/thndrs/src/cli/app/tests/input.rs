use super::super::input::finish_reasoning_effort_picker;
use super::super::onboarding::after_setup_model_config;
use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use helpers::*;

#[test]
fn q_appends_to_input_and_does_not_quit() {
    let mut app = fresh_app();
    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
    );
    assert!(!app.runtime.quit, "q should not quit");
    assert_eq!(app.composer.input.as_str(), "q", "q should append to input");
    assert_eq!(follow, None);
}

#[test]
fn mouse_wheel_does_not_edit_or_recall_prompt_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("current draft");
    app.composer.input_history.push("previous prompt".to_string());

    update(
        &mut app,
        &Msg::Mouse(MouseEvent { kind: MouseEventKind::ScrollUp, column: 0, row: 0, modifiers: KeyModifiers::NONE }),
    );

    assert_eq!(app.composer.input.as_str(), "current draft");
    assert_eq!(app.composer.history_cursor, None);
}

#[test]
fn ctrl_d_works_even_with_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("some text");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.runtime.quit);
    assert!(app.runtime.ctrl_d_pending.is_some());

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.runtime.quit, "Ctrl+D should quit even with input present");
}

#[test]
fn file_picker_escape_closes_without_changing_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("read");
    let _ = app.overlay.show_picker(
        PromptAccessory::Files(FilePickerSource::Forced),
        picker_from_paths(vec!["README.md".to_string()]),
    );

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

    assert_eq!(app.composer.mode, Mode::Prompt);
    assert_eq!(app.overlay.accessory(), PromptAccessory::None);
    assert_eq!(app.composer.input.as_str(), "read");
    assert!(app.overlay.picker().is_none());
}

#[test]
fn ctrl_o_does_not_replace_a_pending_permission() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::Tool {
        name: "read_file".to_string(),
        arguments: "{}".to_string(),
        status: ToolStatus::Ok,
        output: vec!["details".to_string()],
    });
    let (tx, rx) = mpsc::channel();
    app.overlay.show_permission(pending_permission(tx));

    update(&mut app, &key(KeyCode::Char('o'), KeyModifiers::CONTROL));

    assert!(app.overlay.permission().is_some());
    assert!(!app.overlay.is_detail());
    assert_eq!(rx.try_recv(), Err(mpsc::TryRecvError::Empty));
}

#[test]
fn setup_reasoning_picker_advances_after_selection() {
    let mut app = fresh_app();
    app.runtime.model = "chatgpt-codex/gpt-5.6-terra".to_string();
    app.overlay.show_setup(FirstRunRecovery::setup(SetupProviderArg::Umans));

    after_setup_model_config(&mut app, SetupProviderArg::Umans, CredentialScope::Project);
    assert_eq!(app.overlay.accessory(), PromptAccessory::ReasoningEffort);
    assert!(app.overlay.pending_setup_reasoning_effort().is_some());

    finish_reasoning_effort_picker(&mut app);

    assert!(app.overlay.setup().is_some());
    assert!(app.overlay.pending_setup_reasoning_effort().is_none());
}

#[test]
fn printable_chars_append_to_input() {
    let mut app = fresh_app();
    for ch in "hello".chars() {
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
        );
    }
    assert_eq!(app.composer.input.as_str(), "hello");
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn backspace_on_empty_input_is_noop() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.composer.input.as_str(), "");
}

#[test]
fn enter_submits_user_entry_and_clears_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("explain this repo");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(
        app.transcript.entries[0],
        Entry::User { text: String::from("explain this repo") }
    );
}

#[test]
fn enter_on_empty_input_does_nothing() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert!(app.transcript.entries.is_empty());
}

#[test]
fn q_does_not_quit_even_when_input_empty() {
    let mut app = fresh_app();
    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
    );
    assert!(!app.runtime.quit, "q should never quit");
    assert_eq!(follow, None);
    assert_eq!(app.composer.input.as_str(), "q");
}

#[test]
fn queued_running_input_is_recorded_in_history() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("steer here");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input_history, vec![String::from("steer here")]);
}

#[test]
fn failed_provider_restores_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.composer.input.is_empty(), "input should be cleared after submit");
    assert_eq!(app.composer.last_input, Some("hello world".to_string()));

    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(
        app.composer.input.as_str(),
        "hello world",
        "input should be restored on failure"
    );
    assert_eq!(app.runtime.run_state, RunState::Error("boom".to_string()));
}

#[test]
fn failed_provider_preserves_draft_typed_during_run() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    app.composer.input = PromptInput::from("draft follow-up");

    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));

    assert_eq!(app.composer.input.as_str(), "draft follow-up");
    assert_eq!(app.runtime.run_state, RunState::Error("boom".to_string()));
}

#[test]
fn finished_clears_last_input() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("test prompt");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.composer.last_input.is_some());

    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert!(
        app.composer.last_input.is_none(),
        "last_input should be cleared on finish"
    );
    assert!(app.composer.input.is_empty(), "input should remain empty on finish");
}

#[test]
fn typing_while_streaming_appends_to_input() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("hel");

    update(&mut app, &key(KeyCode::Char('l'), KeyModifiers::NONE));
    update(&mut app, &key(KeyCode::Char('o'), KeyModifiers::NONE));
    assert_eq!(
        app.composer.input.as_str(),
        "hello",
        "typing should work while streaming"
    );
}

#[test]
fn multiline_input_while_working() {
    let mut app = working_app_with_streaming();
    app.composer.input = PromptInput::from("line one");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::SHIFT));

    assert!(
        app.composer.input.as_str().contains('\n'),
        "Shift+Enter should insert newline while working"
    );
    assert_eq!(app.runtime.run_state, RunState::Working, "run state should not change");
}

#[test]
fn queued_input_persisted_to_session_writer() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.overlay.close();
    app.runtime.run_state = RunState::Working;
    app.composer.queue_target = QueueTarget::FollowUp;
    app.composer.input = PromptInput::from("persisted follow-up");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    let session_path = app
        .session
        .writer
        .as_ref()
        .expect("session writer should exist")
        .path()
        .to_path_buf();
    let content = std::fs::read_to_string(&session_path).expect("read session file");
    assert!(
        content.contains("queued_input"),
        "session file should contain a queued_input record: {content}"
    );
    assert!(
        content.contains("persisted follow-up"),
        "session file should contain the queued text: {content}"
    );
    assert!(
        content.contains("follow-up"),
        "session file should contain the kind field: {content}"
    );
}

#[test]
fn queued_input_append_failure_is_visible() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), model: "opencode/big-pickle".to_string(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.overlay.close();
    let session_path = app
        .session
        .writer
        .as_ref()
        .expect("session writer should exist")
        .path()
        .to_path_buf();
    std::fs::remove_file(&session_path).expect("remove session file to force append failure");
    app.runtime.run_state = RunState::Working;
    app.composer.queue_target = QueueTarget::FollowUp;
    app.composer.input = PromptInput::from("cannot audit this");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.composer.queued_followups, vec!["cannot audit this".to_string()]);
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Error { text } if text.contains("failed to record queued follow-up"))),
        "append failure should be surfaced in the transcript"
    );
}
