use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::Write;

fn key(code: KeyCode, modifiers: KeyModifiers) -> Msg {
    Msg::Key(KeyEvent::new(code, modifiers))
}

fn fresh_app() -> App {
    let dir = tempfile::tempdir().expect("create temp dir");
    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let mut app = App::from_cli(&cli);
    app.session_writer = None;
    app
}

fn picker_from_paths(paths: Vec<String>) -> PickerState {
    PickerState::new(
        paths.into_iter().map(|path| PickerItem::new(path, "")).collect(),
        FILE_PICKER_LIMIT,
    )
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
        "umans",
        "umans-coder",
        "none",
        "0.1.0",
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

    app.input = PromptInput::from_str("update TODO.md");
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
fn q_appends_to_input_and_does_not_quit() {
    let mut app = fresh_app();
    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
    );
    assert!(!app.quit, "q should not quit");
    assert_eq!(app.input.as_str(), "q", "q should append to input");
    assert_eq!(follow, None);
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
fn ctrl_d_works_even_with_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("some text");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(!app.quit);
    assert!(app.ctrl_d_pending.is_some());

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
    );
    assert!(app.quit, "Ctrl+D should quit even with input present");
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
fn file_picker_escape_closes_without_changing_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("read");
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Forced);
    app.picker = Some(picker_from_paths(vec!["README.md".to_string()]));

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.input.as_str(), "read");
    assert!(app.picker.is_none());
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
fn printable_chars_append_to_input() {
    let mut app = fresh_app();
    for ch in "hello".chars() {
        update(
            &mut app,
            &Msg::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
        );
    }
    assert_eq!(app.input.as_str(), "hello");
    assert!(app.transcript.is_empty());
}

#[test]
fn backspace_removes_last_char() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("abc");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.input.as_str(), "ab");
}

#[test]
fn backspace_on_empty_input_is_noop() {
    let mut app = fresh_app();
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(app.input.as_str(), "");
}

#[test]
fn enter_submits_user_entry_and_clears_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("explain this repo");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input.as_str(), "");
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0],
        Entry::User { text: String::from("explain this repo") }
    );
}

#[test]
fn enter_on_empty_input_does_nothing() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input.as_str(), "");
    assert!(app.transcript.is_empty());
}

#[test]
fn enter_trims_whitespace_before_submit() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("  hello  ");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input.as_str(), "");
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(app.transcript[0], Entry::User { text: String::from("hello") });
}

#[test]
fn slash_clear_clears_transcript_and_input() {
    let mut app = fresh_app();
    app.transcript.push(Entry::User { text: String::from("old") });
    app.input = PromptInput::from_str("/clear");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.transcript.is_empty());
    assert_eq!(app.input.as_str(), "");
    assert!(!app.quit);
}

#[test]
fn slash_quit_sets_quit_flag() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("/quit");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
    assert_eq!(follow, Some(Msg::Quit));
    assert_eq!(app.input.as_str(), "");
}

#[test]
fn slash_exit_also_quits() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("/exit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
}

#[test]
fn unknown_slash_command_is_ignored() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("/bogus");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(!app.quit);
    assert!(app.transcript.is_empty());
    assert_eq!(app.input.as_str(), "/bogus");
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
fn q_does_not_quit_even_when_input_empty() {
    let mut app = fresh_app();
    let follow = update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
    );
    assert!(!app.quit, "q should never quit");
    assert_eq!(follow, None);
    assert_eq!(app.input.as_str(), "q");
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
        Entry::Assistant { text: String::from("Hello"), streaming: true }
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
        Entry::Assistant { text: String::from("Hello world"), streaming: true }
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
        Entry::Assistant { text: String::from("first"), streaming: false }
    );
    assert_eq!(
        app.transcript[1],
        Entry::Assistant { text: String::from("second"), streaming: true }
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
            Entry::Assistant { text: String::from("Done."), streaming: true },
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
        Entry::Assistant { streaming, .. } => assert!(!*streaming),
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

    if let Entry::Assistant { streaming, .. } = &app.transcript[0] {
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
        Entry::Assistant { streaming, .. } => assert!(!*streaming),
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
        Entry::Assistant { streaming, .. } => assert!(!*streaming),
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
    app.input = PromptInput::from_str("queued message");
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
    app.input = PromptInput::from_str("look at tests first");

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
    app.input = PromptInput::from_str("explain this repo");
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

    assert_eq!(app.transcript.len(), 1);
    match &app.transcript[0] {
        Entry::Status { text } => assert!(text.contains("loaded")),
        _ => panic!("expected Status entry for context source"),
    }
}

#[test]
fn app_with_oversized_agents_md_marks_truncation() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let big_content = "x".repeat(context::AGENTS_MD_SIZE_CAP + 1000);
    let agents_path = dir.path().join("AGENTS.md");
    let mut f = std::fs::File::create(&agents_path).expect("create AGENTS.md");
    f.write_all(big_content.as_bytes()).expect("write AGENTS.md");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.context_sources.len(), 1);
    let source = &app.context_sources[0];
    assert!(source.truncated);
    assert!(source.content.len() <= context::AGENTS_MD_SIZE_CAP);

    match &app.transcript[0] {
        Entry::Status { text } => assert!(text.contains("truncated")),
        _ => panic!("expected Status entry"),
    }
}

#[test]
fn context_sources_are_guidance_not_permission() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let content = "# Instructions\n\nModel: gpt-4\nAllow: rm -rf\n";
    let mut f = std::fs::File::create(dir.path().join("AGENTS.md")).expect("create");
    f.write_all(content.as_bytes()).expect("write");

    let cli = Cli { cwd: dir.path().to_path_buf(), ..Cli::default() };
    let app = App::from_cli(&cli);

    assert_eq!(app.model, "umans-coder");
    assert!(app.context_sources[0].content.contains("Model: gpt-4"));
}

#[test]
fn status_label_idle_when_no_transcript() {
    let app = fresh_app();
    assert_eq!(app.status_label(), "idle");
}

#[test]
fn status_label_sending_after_user_submit() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    app.transcript.push(Entry::User { text: String::from("hi") });
    assert_eq!(app.status_label(), "sending");
}

#[test]
fn status_label_thinking_during_reasoning_stream() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::ReasoningDelta(String::from("hmm"))));
    assert_eq!(app.status_label(), "thinking");
}

#[test]
fn status_label_streaming_during_assistant_stream() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
    assert_eq!(app.status_label(), "streaming");
}

#[test]
fn status_label_running_tool_when_tool_active() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    assert_eq!(app.status_label(), "running tool");
}

#[test]
fn status_label_done_after_finished() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("done"))));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(app.status_label(), "done");
}

#[test]
fn status_label_failed_after_error() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.status_label(), "failed");
}

#[test]
fn status_label_failed_after_failed_tool() {
    let mut app = fresh_app();
    app.transcript.push(Entry::Tool {
        name: String::from("run_shell#0"),
        arguments: String::from("{}"),
        status: ToolStatus::Failed,
        output: vec![String::from("error")],
    });
    assert_eq!(
        app.status_label(),
        "failed",
        "failed tool should show 'failed' not 'done'"
    );
}

#[test]
fn status_label_cancelled_after_cancel() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.status_label(), "cancelled");
}

#[test]
fn prompt_state_editable_when_idle() {
    let app = fresh_app();
    assert_eq!(app.prompt_state(), PromptState::Editable);
}

#[test]
fn prompt_state_streaming_during_assistant_delta() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
    assert_eq!(app.prompt_state(), PromptState::Streaming);
}

#[test]
fn prompt_state_running_tool_when_tool_active() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    assert_eq!(app.prompt_state(), PromptState::RunningTool);
}

#[test]
fn prompt_state_stopped_after_cancel() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.prompt_state(), PromptState::Stopped);
}

#[test]
fn prompt_state_errored_after_failure() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.prompt_state(), PromptState::Errored);
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
    app.input = PromptInput::from_str("retry");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(follow.is_some());
    if let Some(msg) = follow {
        update(&mut app, &msg);
    }
    assert_eq!(app.run_state, RunState::Working);
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
fn queued_running_input_is_recorded_in_history() {
    let mut app = fresh_app();
    app.run_state = RunState::Working;
    app.input = PromptInput::from_str("steer here");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.input_history, vec![String::from("steer here")]);
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
fn command_mode_enter_executes_and_returns_to_prompt() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };
    app.input = PromptInput::from_str("clear");
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
    app.input = PromptInput::from_str("cl");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert_eq!(app.input.as_str(), "clear ");
}

#[test]
fn command_mode_esc_returns_to_prompt() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from_str("qui");
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
    app.input = PromptInput::from_str("cl");
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
    app.input = PromptInput::from_str("quit");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.quit);
}

#[test]
fn command_mode_help_command_enters_help_overlay() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from_str("help");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::Help);
}

#[test]
fn question_key_does_not_enter_help_when_input_nonempty() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)),
    );
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.input.as_str(), "hello?");
}

#[test]
fn bg_command_with_no_processes_shows_empty_message() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from_str("bg");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(
        app.transcript
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("no background"))),
        "should show no background processes"
    );
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
fn bg_command_lists_registered_background_processes() {
    let mut app = fresh_app();
    let cancel = tools::shell::CancelFlag::new();
    let id = app.process_registry.register(
        vec!["cargo".to_string(), "build".to_string()],
        std::path::PathBuf::from("."),
        tools::shell::ProcessKind::Background,
        cancel,
    );

    app.mode = Mode::Command;
    app.input = PromptInput::from_str("bg");
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
fn quit_cancels_all_background_processes() {
    let mut app = fresh_app();
    let cancel = tools::shell::CancelFlag::new();
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
fn tab_is_ignored_in_prompt_mode() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
    assert_eq!(app.mode, Mode::Prompt);
    assert!(app.input.is_empty());
}

#[test]
fn ctrl_k_kills_to_end_of_line() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello world");
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
    app.input = PromptInput::from_str("hello world");
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
    app.input = PromptInput::from_str("foo bar baz");
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
    app.input = PromptInput::from_str("hello ");
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
    app.input = PromptInput::from_str("ab");
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
    app.input = PromptInput::from_str("foo bar baz");
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
    app.input = PromptInput::from_str("foo bar baz");
    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT)),
    );
    assert_eq!(app.input.as_str(), "foo bar ");
    assert_eq!(app.kill_ring, vec!["baz"]);
}

#[test]
fn failed_provider_restores_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello world");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.input.is_empty(), "input should be cleared after submit");
    assert_eq!(app.last_input, Some("hello world".to_string()));

    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.input.as_str(), "hello world", "input should be restored on failure");
    assert_eq!(app.run_state, RunState::Error("boom".to_string()));
}

#[test]
fn finished_clears_last_input() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("test prompt");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.last_input.is_some());

    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert!(app.last_input.is_none(), "last_input should be cleared on finish");
    assert!(app.input.is_empty(), "input should remain empty on finish");
}

#[test]
fn left_arrow_moves_cursor_left() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("abc");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.input.cursor(), 2);
}

#[test]
fn right_arrow_moves_cursor_right() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("abc");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(app.input.cursor(), 1);
}

#[test]
fn ctrl_b_moves_cursor_left() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("abc");
    update(&mut app, &key(KeyCode::Char('b'), KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 2);
}

#[test]
fn ctrl_f_moves_cursor_right() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("abc");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('f'), KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 1);
}

#[test]
fn home_moves_to_start() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    update(&mut app, &key(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.input.cursor(), 0);
}

#[test]
fn end_moves_to_end() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.input.cursor(), 5);
}

#[test]
fn ctrl_a_moves_to_start() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    update(&mut app, &key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 0);
}

#[test]
fn ctrl_e_moves_to_end() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 5);
}

#[test]
fn alt_left_moves_word_left() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::ALT));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn ctrl_left_moves_word_left() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    update(&mut app, &key(KeyCode::Left, KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn alt_b_moves_word_left() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    update(&mut app, &key(KeyCode::Char('b'), KeyModifiers::ALT));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn alt_right_moves_word_right() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::ALT));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn ctrl_right_moves_word_right() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Right, KeyModifiers::CONTROL));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn alt_f_moves_word_right() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("foo bar");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Char('f'), KeyModifiers::ALT));
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn shift_enter_inserts_newline() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("line1");
    update(&mut app, &key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(app.input.as_str(), "line1\n");
    assert_eq!(app.input.cursor(), 6);
}

#[test]
fn ctrl_j_inserts_newline() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("line1");
    update(&mut app, &key(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.input.as_str(), "line1\n");
}

#[test]
fn delete_key_deletes_forward() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    app.input.cursor_to_start();
    update(&mut app, &key(KeyCode::Delete, KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "ello");
    assert_eq!(app.input.cursor(), 0);
}

#[test]
fn backspace_deletes_before_cursor() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("hello");
    update(&mut app, &key(KeyCode::Backspace, KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "hell");
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn typing_inserts_at_cursor_not_at_end() {
    let mut app = fresh_app();
    app.input = PromptInput::from_str("helo");
    app.input.cursor_left();
    update(&mut app, &key(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.input.as_str(), "hello");
    assert_eq!(app.input.cursor(), 4);
}

#[test]
fn ctrl_p_still_opens_file_picker() {
    let mut app = fresh_app();
    let dir = tempfile::tempdir().expect("create temp dir");
    app.cwd = dir.path().to_path_buf();
    let _ = std::fs::write(app.cwd.join("test.txt"), "test");
    update(&mut app, &key(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.prompt_accessory, PromptAccessory::Files(FilePickerSource::Forced));
    assert!(app.picker.is_some());
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
fn git_status_changed_message_updates_app_summary() {
    let mut app = fresh_app();
    assert!(app.git_status.is_none());

    update(
        &mut app,
        &Msg::GitStatusChanged(Some(crate::renderer::git::GitStatusSummary {
            branch: Some("main".to_string()),
            added: 1,
            modified: 2,
            deleted: 3,
        })),
    );

    assert_eq!(
        app.git_status.as_ref().map(|status| status.display()),
        Some("git: main +1 ~2 -3".to_string())
    );
}

#[test]
fn ctrl_a_in_command_mode_inserts_literal_a() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from_str("test");
    update(&mut app, &key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    assert_eq!(app.input.as_str(), "testa");
}
