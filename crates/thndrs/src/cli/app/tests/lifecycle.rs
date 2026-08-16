//! Application behavior tests for lifecycle seams.

use super::*;
use helpers::*;

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
