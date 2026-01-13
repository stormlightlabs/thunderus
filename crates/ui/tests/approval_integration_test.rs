use std::path::PathBuf;
use thunderus_core::{ApprovalMode, ProviderConfig};
use thunderus_ui::{App, ApprovalDecision, KeyAction, state::ApprovalState};

#[test]
fn test_approval_flow_end_to_end() {
    let mut app = create_test_app();

    app.transcript_mut().add_user_message("Add error handling");
    assert_eq!(app.transcript().len(), 1);

    app.transcript_mut()
        .add_model_response("I'll add error handling to config.rs");
    app.transcript_mut()
        .add_tool_call("file_edit", "{ path: 'src/config.rs' }", "risky");

    assert_eq!(app.transcript().len(), 3);
    assert!(!app.transcript().has_pending_approval());

    app.transcript_mut().add_approval_prompt("file_edit", "risky");

    assert!(app.transcript().has_pending_approval());
    assert_eq!(app.transcript().len(), 4);

    let entry = app.transcript().last().unwrap();
    assert!(entry.is_pending());
    assert!(entry.is_approval_entry());
    assert!(!entry.is_tool_entry());

    let success = app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    assert!(success);
    assert!(!app.transcript().has_pending_approval());

    let entry = app.transcript().last();
    assert!(entry.is_some());

    if let Some(thunderus_ui::TranscriptEntry::ApprovalPrompt { decision, .. }) = entry {
        assert_eq!(decision, &Some(ApprovalDecision::Approved));
    } else {
        panic!("Expected ApprovalPrompt");
    }

    app.transcript_mut()
        .add_tool_result("file_edit", "Applied 3 lines", true);

    assert_eq!(app.transcript().len(), 5);
    assert!(!app.transcript().has_pending_approval());

    let entries = app.transcript().entries();
    assert_eq!(entries.len(), 5);

    assert!(matches!(entries[0], thunderus_ui::TranscriptEntry::UserMessage { .. }));
    assert!(matches!(
        entries[1],
        thunderus_ui::TranscriptEntry::ModelResponse { .. }
    ));
    assert!(matches!(entries[2], thunderus_ui::TranscriptEntry::ToolCall { .. }));
    assert!(matches!(
        entries[3],
        thunderus_ui::TranscriptEntry::ApprovalPrompt { .. }
    ));
    assert!(matches!(entries[4], thunderus_ui::TranscriptEntry::ToolResult { .. }));
}

#[test]
fn test_approval_rejection_flow() {
    let mut app = create_test_app();

    app.transcript_mut()
        .add_tool_call("file_delete", "{ path: '/tmp/cache' }", "dangerous");
    app.transcript_mut().add_approval_prompt("file_delete", "dangerous");

    assert!(app.transcript().has_pending_approval());

    app.transcript_mut().set_approval_decision(ApprovalDecision::Rejected);

    assert!(!app.transcript().has_pending_approval());

    let entry = app.transcript().last();
    if let Some(thunderus_ui::TranscriptEntry::ApprovalPrompt { decision, .. }) = entry {
        assert_eq!(decision, &Some(ApprovalDecision::Rejected));
    } else {
        panic!("Expected ApprovalPrompt");
    }
}

#[test]
fn test_approval_cancellation_flow() {
    let mut app = create_test_app();

    app.transcript_mut()
        .add_tool_call("install_deps", "{ packages: ['serde'] }", "risky");
    app.transcript_mut().add_approval_prompt("install_deps", "risky");

    app.transcript_mut().set_approval_decision(ApprovalDecision::Cancelled);

    assert!(!app.transcript().has_pending_approval());

    let entry = app.transcript().last();
    if let Some(thunderus_ui::TranscriptEntry::ApprovalPrompt { decision, .. }) = entry {
        assert_eq!(decision, &Some(ApprovalDecision::Cancelled));
    } else {
        panic!("Expected ApprovalPrompt");
    }
}

#[test]
fn test_multiple_approvals_sequence() {
    let mut app = create_test_app();

    app.transcript_mut().add_tool_call("edit", "{}", "risky");
    app.transcript_mut().add_approval_prompt("edit", "risky");

    assert!(app.transcript().has_pending_approval());

    app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    app.transcript_mut().add_tool_result("edit", "Success", true);

    assert!(!app.transcript().has_pending_approval());

    app.transcript_mut().add_tool_call("delete", "{}", "dangerous");
    app.transcript_mut().add_approval_prompt("delete", "dangerous");

    assert!(app.transcript().has_pending_approval());

    app.transcript_mut().set_approval_decision(ApprovalDecision::Rejected);

    assert!(!app.transcript().has_pending_approval());

    assert_eq!(app.transcript().len(), 5);
}

#[test]
fn test_approval_with_description() {
    let mut app = create_test_app();

    app.transcript_mut().add_approval_prompt("install_crate", "risky");

    if let Some(thunderus_ui::TranscriptEntry::ApprovalPrompt { description, .. }) = app.transcript_mut().last_mut() {
        *description = Some("Install serde dependency for parsing".to_string());
    }

    app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    let entry = app.transcript().last();
    if let Some(thunderus_ui::TranscriptEntry::ApprovalPrompt { description, .. }) = entry {
        assert_eq!(description, &Some("Install serde dependency for parsing".to_string()));
    } else {
        panic!("Expected ApprovalPrompt with description");
    }
}

#[test]
fn test_approval_visual_feedback() {
    let mut app = create_test_app();

    app.transcript_mut().add_approval_prompt("test_action", "risky");

    let entries = app.transcript().entries();
    let approval_text = entries[0].to_string();

    assert!(approval_text.contains("⏳") || approval_text.contains("[Approval]"));

    app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    let approval_text = app.transcript().entries()[0].to_string();
    assert!(approval_text.contains("✅") || approval_text.contains("Approved"));

    app.transcript_mut().add_approval_prompt("test_action2", "risky");
    app.transcript_mut().set_approval_decision(ApprovalDecision::Rejected);

    let entries = app.transcript().entries();
    let rejected_text = entries[1].to_string();
    assert!(rejected_text.contains("❌") || rejected_text.contains("Rejected"));
}

#[test]
fn test_keyboard_approval_flow() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use thunderus_ui::EventHandler;

    let mut app = create_test_app();
    let state = app.state_mut();
    state.pending_approval = Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

    let event = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
    let action = EventHandler::handle_key_event(event, state);

    assert!(matches!(action, Some(KeyAction::Approve { .. })));

    let event = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
    let action = EventHandler::handle_key_event(event, state);

    assert!(matches!(action, Some(KeyAction::Reject { .. })));

    let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    let action = EventHandler::handle_key_event(event, state);

    assert!(matches!(action, Some(KeyAction::Cancel { .. })));

    let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let action = EventHandler::handle_key_event(event, state);

    assert!(matches!(action, Some(KeyAction::CancelGeneration)));
}

#[test]
fn test_approval_with_different_risk_levels() {
    let mut app = create_test_app();

    app.transcript_mut().add_approval_prompt("test_safe", "safe");
    app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    app.transcript_mut().add_approval_prompt("test_risky", "risky");
    app.transcript_mut().set_approval_decision(ApprovalDecision::Approved);

    app.transcript_mut().add_approval_prompt("test_dangerous", "dangerous");
    app.transcript_mut().set_approval_decision(ApprovalDecision::Rejected);

    assert_eq!(app.transcript().len(), 3);

    let entries = app.transcript().entries();
    assert!(entries[0].to_string().contains("safe"));
    assert!(entries[1].to_string().contains("risky"));
    assert!(entries[2].to_string().contains("dangerous"));
}

fn create_test_app() -> App {
    let state = thunderus_ui::AppState::new(
        PathBuf::from("."),
        "test".to_string(),
        ProviderConfig::Glm {
            api_key: "test".to_string(),
            model: "glm-4.7".to_string(),
            base_url: "https://api.example.com".to_string(),
        },
        ApprovalMode::Auto,
    );
    App::new(state)
}
