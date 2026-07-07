use super::*;
use helpers::*;

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
