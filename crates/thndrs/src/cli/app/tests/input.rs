use super::super::input::finish_reasoning_effort_picker;
use super::super::onboarding::after_setup_model_config;
use super::*;
use crate::input::PromptInput;
use crate::input::{MouseInput, TerminalInput};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use helpers::*;

#[test]
fn semantic_translation_is_table_driven_by_focus_and_mode() {
    let mut app = fresh_app();
    let cases = [
        (
            "prompt cursor",
            KeyEvent::new(KeyCode::Left, KeyModifiers::NONE),
            Action::CursorLeft,
        ),
        (
            "command submit",
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            Action::Submit,
        ),
    ];

    for (label, key, expected) in cases {
        if label == "command submit" {
            app.composer.mode = Mode::Command;
        }
        assert_eq!(
            translate_input(&app, TerminalInput::Key(key)),
            vec![expected],
            "{label}"
        );
    }

    app.composer.mode = Mode::Prompt;
    app.overlay.show_help();
    assert_eq!(
        translate_input(
            &app,
            TerminalInput::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        ),
        vec![Action::CloseOverlay]
    );
    assert_eq!(
        translate_input(
            &app,
            TerminalInput::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        ),
        vec![Action::ScrollOverlayDown]
    );

    app.overlay.close();
    app.overlay
        .show_picker(
            PromptAccessory::Files(FilePickerSource::Forced),
            picker_from_paths(vec!["a".into()]),
        )
        .unwrap();
    assert_eq!(
        translate_input(
            &app,
            TerminalInput::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        ),
        vec![Action::SelectNext]
    );
}

#[test]
fn shift_tab_cycles_supported_reasoning_effort_while_idle() {
    let mut app = fresh_app();
    app.runtime.model = "chatgpt-codex/gpt-5.6-terra".to_string();
    let options = crate::providers::reasoning_options(&app.runtime.model);
    assert!(options.len() > 1, "test model must support reasoning effort cycling");
    app.runtime.cli.reasoning_effort = options[0];

    update(&mut app, &key(KeyCode::BackTab, KeyModifiers::SHIFT));

    assert_eq!(app.runtime.cli.reasoning_effort, options[1]);
}

#[test]
fn shift_tab_does_not_cycle_reasoning_effort_while_working() {
    let mut app = fresh_app();
    app.runtime.model = "chatgpt-codex/gpt-5.6-terra".to_string();
    let options = crate::providers::reasoning_options(&app.runtime.model);
    assert!(options.len() > 1, "test model must support reasoning effort cycling");
    app.runtime.cli.reasoning_effort = options[0];
    app.runtime.run_state = RunState::Working;

    update(&mut app, &key(KeyCode::BackTab, KeyModifiers::SHIFT));

    assert_eq!(app.runtime.cli.reasoning_effort, options[0]);
}

#[test]
fn shift_tab_does_not_bypass_a_focused_picker() {
    let mut app = fresh_app();
    app.runtime.model = "chatgpt-codex/gpt-5.6-terra".to_string();
    let options = crate::providers::reasoning_options(&app.runtime.model);
    assert!(options.len() > 1, "test model must support reasoning effort cycling");
    app.runtime.cli.reasoning_effort = options[0];
    app.overlay
        .show_picker(
            PromptAccessory::Files(FilePickerSource::Forced),
            picker_from_paths(vec!["README.md".to_string()]),
        )
        .expect("show picker");

    update(&mut app, &key(KeyCode::BackTab, KeyModifiers::SHIFT));

    assert_eq!(app.runtime.cli.reasoning_effort, options[0]);
    assert_eq!(
        app.overlay.accessory(),
        PromptAccessory::Files(FilePickerSource::Forced)
    );
}

#[test]
fn help_scrolls_without_editing_the_composer() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("preserved draft");
    app.overlay.show_help();

    update(&mut app, &Msg::Action(Action::ScrollOverlayDown));
    update(&mut app, &Msg::Action(Action::ScrollOverlayDown));
    assert_eq!(app.overlay.help_scroll(), Some(2));
    assert_eq!(app.composer.input.as_str(), "preserved draft");

    update(&mut app, &Msg::Action(Action::ScrollOverlayUp));
    assert_eq!(app.overlay.help_scroll(), Some(1));
}

#[test]
fn custom_keymap_and_terminal_actions_are_deterministic() {
    let app = fresh_app();
    let mut keymap = Keymap::default();
    keymap.bind(
        InputFocus::Prompt,
        KeyBinding::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
        Action::CursorEnd,
    );
    assert_eq!(
        translate_input_with_keymap(
            &app,
            TerminalInput::Key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL)),
            &keymap,
        ),
        vec![Action::CursorEnd]
    );
    assert_eq!(
        translate_input(&app, TerminalInput::from_event(Event::Resize(80, 24)).unwrap()),
        vec![Action::Resize { width: 80, height: 24 }]
    );
    assert_eq!(
        translate_input(&app, TerminalInput::Mouse(MouseInput::ScrollUp),),
        vec![Action::ScrollTranscriptUp]
    );
}

#[test]
fn bracketed_paste_is_one_grapheme_preserving_action_and_overlay_first() {
    let mut app = fresh_app();
    let input = TerminalInput::from_event(Event::Paste("a\r\n👩‍🔬".to_string())).unwrap();
    let actions = translate_input(&app, input);
    assert_eq!(actions, vec![Action::InsertText("a\n👩‍🔬".to_string())]);
    update(&mut app, &Msg::Action(actions[0].clone()));
    assert_eq!(app.composer.input.as_str(), "a\n👩‍🔬");
    assert_eq!(app.composer.input.len_graphemes(), 3);

    app.overlay.show_help();
    update(&mut app, &Msg::Action(Action::InsertText("blocked".to_string())));
    assert_eq!(app.composer.input.as_str(), "a\n👩‍🔬");
}

#[test]
fn mouse_wheel_routes_to_the_detail_overlay_before_the_transcript() {
    let mut app = fresh_app();
    app.overlay.show_detail(0);

    assert_eq!(
        translate_input(&app, TerminalInput::Mouse(MouseInput::ScrollDown)),
        vec![Action::ScrollOverlayDown]
    );
}

#[test]
fn left_mouse_drag_routes_to_transcript_selection_with_coordinates() {
    let app = fresh_app();

    assert_eq!(
        translate_input(&app, TerminalInput::Mouse(MouseInput::LeftDrag { column: 17, row: 9 })),
        vec![Action::UpdateTranscriptSelection { column: 17, row: 9 }]
    );
}

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

    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["cannot audit this"]
    );
    assert!(matches!(
        &app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .next()
            .expect("retained queue item")
            .audit,
        QueueAuditState::Failed(_)
    ));
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Error { text } if text.contains("failed to record queued follow-up"))),
        "append failure should be surfaced in the transcript"
    );
}
