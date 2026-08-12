use super::*;
use crate::input::PromptInput;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use helpers::*;

#[test]
fn ttft_starts_on_submit_and_ignores_status_and_usage() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(app.runtime.ttft.is_pending(), "submit should start pending TTFT");

    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Status(String::from("provider: queued"))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Usage { input_tokens: 1, output_tokens: 0 }),
    );

    assert!(
        app.runtime.ttft.is_pending(),
        "status and usage events should not stop TTFT"
    );
    assert!(app.runtime.ttft.last_completed().is_none());
}

#[test]
fn routine_model_metadata_stays_out_of_the_default_transcript() {
    let mut app = fresh_app();

    update(
        &mut app,
        &Msg::Agent(AgentEvent::Status(String::from(
            "model: chatgpt-codex/gpt-5.6-sol  ChatGPT/Codex",
        ))),
    );
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Status(String::from("model selected: opencode/big-pickle"))),
    );

    assert_eq!(
        app.transcript.entries,
        vec![Entry::Status { text: String::from("model selected: opencode/big-pickle") }]
    );
}

#[test]
fn ttft_stops_on_first_semantic_output_and_is_retained_after_finish() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("hello world");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ReasoningDelta(String::from("thinking"))),
    );

    assert!(!app.runtime.ttft.is_pending(), "semantic output should stop TTFT");
    let measured = app.runtime.ttft.last_completed().expect("measured TTFT");

    update(
        &mut app,
        &Msg::Agent(AgentEvent::AssistantDelta(String::from("answer"))),
    );
    update(&mut app, &Msg::Agent(AgentEvent::Finished));

    assert_eq!(app.runtime.ttft.last_completed(), Some(measured));
}

#[test]
fn ttft_is_preserved_across_retries_and_reset_on_next_turn() {
    let mut app = fresh_app();
    app.composer.input = PromptInput::from("first turn");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::Retrying {
            attempt: 1,
            max_attempts: 2,
            delay_ms: 10,
            error: String::from("server error"),
        }),
    );
    assert!(
        app.runtime.ttft.is_pending(),
        "retry should keep the original TTFT pending"
    );

    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("tool-1"),
            name: String::from("read_file"),
            arguments: String::from("{}"),
        }),
    );
    assert!(!app.runtime.ttft.is_pending());
    assert!(app.runtime.ttft.last_completed().is_some());

    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    app.composer.input = PromptInput::from("second turn");
    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    assert!(
        app.runtime.ttft.is_pending(),
        "next turn should start a fresh pending TTFT"
    );
}

#[test]
fn status_label_idle_when_no_transcript() {
    let app = fresh_app();
    assert_eq!(app.status_label(), "Ready");
}

#[test]
fn status_label_sending_after_user_submit() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    app.transcript.entries.push(Entry::User { text: String::from("hi") });
    assert_eq!(app.status_label(), "Sending");
}

#[test]
fn status_label_thinking_during_reasoning_stream() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::ReasoningDelta(String::from("hmm"))));
    assert_eq!(app.status_label(), "Thinking");
}

#[test]
fn status_label_working_during_assistant_stream() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("hi"))));
    assert_eq!(app.status_label(), "Responding");
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
    assert_eq!(app.status_label(), "Running read file");
}

#[test]
fn status_label_names_the_running_shell_command() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("run_shell"),
            arguments: String::from(r#"{"argv":["cargo","test","--workspace"]}"#),
        }),
    );
    assert_eq!(app.status_label(), "Running cargo test");
}

#[test]
fn status_label_names_a_legacy_running_shell_command() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(
        &mut app,
        &Msg::Agent(AgentEvent::ToolStarted {
            id: String::from("0"),
            name: String::from("run_shell"),
            arguments: String::from(r#"{"program":"cargo","args":["test","--workspace"]}"#),
        }),
    );
    assert_eq!(app.status_label(), "Running cargo test");
}

#[test]
fn status_label_done_after_finished() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::AssistantDelta(String::from("done"))));
    update(&mut app, &Msg::Agent(AgentEvent::Finished));
    assert_eq!(app.status_label(), "Ready");
}

#[test]
fn status_label_failed_after_error() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Failed(String::from("boom"))));
    assert_eq!(app.status_label(), "Failed");
}

#[test]
fn status_label_failed_after_failed_tool() {
    let mut app = fresh_app();
    app.transcript.entries.push(Entry::Tool {
        name: String::from("run_shell#0"),
        arguments: String::from("{}"),
        status: ToolStatus::Failed,
        output: vec![String::from("error")],
    });
    assert_eq!(app.status_label(), "Failed", "failed tool should remain visible");
}

#[test]
fn status_label_cancelled_after_cancel() {
    let mut app = fresh_app();
    update(&mut app, &Msg::Agent(AgentEvent::Started));
    update(&mut app, &Msg::Agent(AgentEvent::Cancelled));
    assert_eq!(app.status_label(), "Stopped");
}

#[test]
fn git_status_changed_message_updates_app_summary() {
    let mut app = fresh_app();
    assert!(app.runtime.git_status.is_none());

    update(
        &mut app,
        &Msg::GitStatusChanged(Some(renderer::git::GitStatusSummary {
            branch: Some("main".to_string()),
            added: 1,
            modified: 2,
            deleted: 3,
        })),
    );

    assert_eq!(
        app.runtime.git_status.as_ref().map(|status| status.display()),
        Some("git: main +1 ~2 -3".to_string())
    );
}
