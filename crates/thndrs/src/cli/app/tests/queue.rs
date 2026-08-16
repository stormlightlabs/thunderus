//! Application behavior tests for queue seams.

use super::*;
use helpers::*;

#[test]
fn submit_while_working_queues_followup_by_default() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("queued message");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["queued message"]
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued follow-up")))
    );
}

#[test]
fn steering_chord_queues_running_input_as_steering() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("look at tests first");

    #[cfg(target_os = "macos")]
    let modifiers = KeyModifiers::SUPER;
    #[cfg(not(target_os = "macos"))]
    let modifiers = KeyModifiers::CONTROL;
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, modifiers)));

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::Steering)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
}

#[test]
fn ctrl_g_queues_running_input_as_steering_in_all_terminal_environments() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.input = PromptInput::from("look at tests first");

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
    );

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::Steering)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
}

#[test]
fn plain_submit_while_working_always_queues_a_followup() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer.queue_target = QueueTarget::Steering;
    app.composer.input = PromptInput::from("look at tests first");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(app.composer.input.is_empty());
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["look at tests first"]
    );
    assert!(
        app.transcript
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Status { text } if text.contains("queued follow-up")))
    );
}

#[test]
fn ctrl_o_opens_the_latest_tool_with_output() {
    let mut app = fresh_app();
    for entry in [
        Entry::Tool {
            name: "run_shell".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Failed,
            output: vec!["old failure".to_string()],
        },
        Entry::Tool {
            name: "read_file".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Ok,
            output: vec!["latest output".to_string()],
        },
        Entry::Tool {
            name: "write_patch".to_string(),
            arguments: "{}".to_string(),
            status: ToolStatus::Running,
            output: Vec::new(),
        },
    ] {
        app.transcript.entries.push(entry);
    }

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );

    assert_eq!(app.overlay.detail().map(|detail| detail.entry_index), Some(1));

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );
    assert!(!app.overlay.is_detail(), "Ctrl+O should close open details");
}

#[test]
fn ctrl_o_without_tool_output_leaves_transcript_focus_unchanged() {
    let mut app = fresh_app();
    app.transcript
        .entries
        .push(Entry::Agent { text: "nothing expandable".to_string(), streaming: false });

    update(
        &mut app,
        &Msg::Key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
    );

    assert!(!app.overlay.is_detail());
}

#[test]
fn finished_starts_next_followup_turn() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "next task".to_string(), "test".to_string());

    let next = update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert_eq!(next, Some(Msg::Agent(AgentEvent::Started)));
    assert_eq!(app.composer.queue.pending_count(QueueTarget::FollowUp), 0);
    assert_eq!(app.session.turn_count, 1);
    assert!(matches!(app.transcript.entries.last(), Some(Entry::User { text }) if text == "next task"));
}

#[test]
fn cancelled_clears_queued_steering_but_keeps_followups() {
    let mut app = fresh_app();
    app.runtime.run_state = RunState::Working;
    app.composer
        .queue
        .push(QueueTarget::Steering, "steer".to_string(), "test".to_string());
    app.composer
        .queue
        .push(QueueTarget::FollowUp, "after".to_string(), "test".to_string());

    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));

    assert_eq!(app.composer.queue.pending_count(QueueTarget::Steering), 0);
    assert_eq!(
        app.composer
            .queue
            .pending(QueueTarget::FollowUp)
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>(),
        vec!["after"]
    );
}

#[test]
fn submit_kicks_off_agent_via_followup() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("explain this repo");
    let follow = update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(app.composer.input.as_str(), "");
    assert_eq!(app.transcript.entries.len(), 1);
    assert_eq!(follow, Some(Msg::Agent(AgentEvent::Started)));
}

#[test]
fn queue_edits_are_cancelable_and_send_now_settles_only_the_selected_item() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("draft remains");
    let first = app
        .composer
        .queue
        .push(QueueTarget::FollowUp, "first".to_string(), "now".to_string());
    let second = app
        .composer
        .queue
        .push(QueueTarget::FollowUp, "second".to_string(), "later".to_string());
    app.overlay.show_queue();

    update(&mut app, &Msg::Action(Action::QueueEdit));
    update(&mut app, &Msg::Action(Action::InsertText(" changed".to_string())));
    update(&mut app, &Msg::Action(Action::Cancel));
    assert_eq!(app.composer.queue.item(first).expect("first item").text, "first");
    assert!(
        app.overlay.queue().is_some(),
        "cancel should leave the queue open after abandoning an edit"
    );

    update(&mut app, &Msg::Action(Action::QueueSendNow));

    assert_eq!(
        app.composer.queue.item(first).expect("first item").settlement,
        QueueSettlement::Sent
    );
    assert_eq!(
        app.composer.queue.item(second).expect("second item").settlement,
        QueueSettlement::Pending
    );
    assert_eq!(app.composer.input.as_str(), "draft remains");
}
