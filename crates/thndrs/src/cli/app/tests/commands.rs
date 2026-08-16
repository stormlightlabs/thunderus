//! Application behavior tests for commands seams.

use super::*;
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
fn msg_clear_clears_transcript() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::User { text: String::from("a") });
    app.transcript.entries.push(Entry::User { text: String::from("b") });
    update(&mut app, &Msg::Clear);
    assert!(app.transcript.entries.is_empty());
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
