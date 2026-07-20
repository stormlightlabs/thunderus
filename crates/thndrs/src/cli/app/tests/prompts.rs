use super::*;
use helpers::*;

use crate::prompt::templates::{PromptTemplate, PromptTemplateSource};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
fn tab_accepts_prompt_mode_command_suggestion() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/cl");
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.input.as_str(), "/clear ");
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
}

#[test]
fn tab_accepts_command_mode_command_suggestion() {
    let mut app = fresh_app();
    app.mode = Mode::Command;
    app.input = PromptInput::from("cl");
    app.prompt_accessory = PromptAccessory::Commands { selected: 0 };

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.input.as_str(), "clear ");
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
}

#[test]
fn tab_accepts_file_mention_completion() {
    let mut app = fresh_app();
    app.input = PromptInput::from("edit @ma");
    app.prompt_accessory = PromptAccessory::Files(FilePickerSource::Mention { token_start: 5 });
    app.picker = Some(PickerState::new(
        vec![PickerItem::new("main.rs".to_string(), String::new())],
        10,
    ));

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

    assert_eq!(app.input.as_str(), "edit @main.rs ");
    assert_eq!(app.prompt_accessory, PromptAccessory::None);
    assert!(app.picker.is_none());
}

#[test]
fn tab_is_noop_without_active_suggestion() {
    let mut app = fresh_app();
    app.input = PromptInput::from("hello");

    update(&mut app, &Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

    assert_eq!(app.mode, Mode::Prompt);
    assert_eq!(app.input.as_str(), "hello");
}

#[test]
fn bundled_prompt_template_is_suggested_with_argument_hint() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/adv");

    let suggestions = command_suggestions_for_app(&app);
    let suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion.name == "adversarial-review")
        .expect("bundled template suggestion");

    assert!(suggestion.detail.starts_with("[scope] — "));
}

#[test]
fn bundled_prompt_template_renders_and_submits() {
    let mut app = fresh_app();
    app.input = PromptInput::from("/review crates/thndrs");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(
        app.transcript
            .iter()
            .any(|entry| matches!(entry, Entry::User { text } if text.starts_with("Review crates/thndrs.")))
    );
    assert!(!app.input.as_str().contains("/review"));
}

#[test]
fn prompt_template_supports_named_and_positional_arguments() {
    let mut app = fresh_app();
    app.prompt_templates.push(PromptTemplate {
        name: "release-note".to_string(),
        description: "draft release note".to_string(),
        argument_hint: Some("<package> audience=<audience>".to_string()),
        body: "Package {{ arg1 }} for {{ audience }} ({{ named.audience }})".to_string(),
        source: PromptTemplateSource::Project,
        path: None,
    });
    app.input = PromptInput::from("/release-note thndrs audience=maintainers");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(matches!(
        app.transcript.last(),
        Some(Entry::User { text }) if text == "Package thndrs for maintainers (maintainers)"
    ));
}

#[test]
fn prompt_template_render_error_preserves_invocation() {
    let mut app = fresh_app();
    app.prompt_templates.push(PromptTemplate {
        name: "needs-value".to_string(),
        description: "requires a value".to_string(),
        argument_hint: None,
        body: "Use {{ required }}".to_string(),
        source: PromptTemplateSource::Project,
        path: None,
    });
    app.input = PromptInput::from("/needs-value");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.input.as_str(), "/needs-value");
    assert!(matches!(
        app.transcript.last(),
        Some(Entry::Error { text }) if text.contains("failed to render /needs-value")
    ));
}

#[test]
fn prompt_template_queues_rendered_followup_while_working() {
    let mut app = working_app_with_streaming();
    app.input = PromptInput::from("/review src/lib.rs");

    update(&mut app, &key(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.queued_followups.len(), 1);
    assert!(app.queued_followups[0].starts_with("Review src/lib.rs."));
    assert!(!app.queued_followups[0].contains("/review"));
}
