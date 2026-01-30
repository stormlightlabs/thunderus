use super::App;
use crate::state;
use thunderus_core::{ActionType, ApprovalContext, ApprovalGate, ToolRisk};
use thunderus_tools::ToolRegistry;
use uuid;

pub fn execute_shell_command(app: &mut App, command: String) {
    let registry = ToolRegistry::with_builtin_tools();
    let tool_call_id = format!("shell_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
    let user_message = format!("!cmd {}", command);

    app.transcript_mut().add_user_message(&user_message);

    let mut approval_gate = ApprovalGate::new(app.state().config.approval_mode, app.state().config.allow_network);

    let command_lower = command.to_lowercase();
    let is_network_command = command_lower.contains("curl ")
        || command_lower.contains("wget ")
        || command_lower.starts_with("curl")
        || command_lower.starts_with("wget")
        || command_lower.contains("ssh ")
        || command_lower.starts_with("ssh")
        || command_lower.contains("http://")
        || command_lower.contains("https://");

    let is_destructive = command_lower.contains("rm ")
        || command_lower.contains("rmdir ")
        || command_lower.contains("del ")
        || command_lower.contains("mv ")
        || command_lower.contains("> ")
        || command_lower.contains("git clean")
        || command_lower.contains("git reset")
        || command_lower.contains("git rebase");

    let risk_level = if is_destructive || is_network_command { ToolRisk::Risky } else { ToolRisk::Safe };
    let requires_approval = approval_gate.requires_approval(risk_level, is_network_command);

    if requires_approval {
        let request_id = approval_gate.create_request(
            ActionType::Shell,
            format!("Execute: {}", command),
            ApprovalContext::new()
                .with_name("shell")
                .with_arguments(serde_json::json!({"command": &command}))
                .with_classification_reasoning(format!(
                    "Command classified as {}: {}",
                    if risk_level.is_risky() { "risky" } else { "safe" },
                    if is_destructive {
                        "destructive operation"
                    } else if is_network_command {
                        "network access"
                    } else {
                        "local command"
                    }
                )),
            risk_level,
        );

        let risk_str = if risk_level.is_risky() { "risky" } else { "safe" };
        app.transcript_mut()
            .add_approval_prompt(format!("shell:{}", command), risk_str);
        app.state_mut().approval_ui.pending_approval =
            Some(state::ApprovalState::pending(command.clone(), risk_str.to_string()).with_request_id(request_id));

        app.state_mut().approval_ui.pending_command = Some(command);
    } else {
        app.do_execute_shell_command(command, &registry, tool_call_id);
    }
}

#[cfg(test)]
mod tests {
    use crate::app::create_test_app;
    use crate::transcript;

    #[test]
    fn test_execute_shell_command_simple() {
        let mut app = create_test_app();

        app.execute_shell_command("echo 'Hello from shell'".to_string());

        let transcript = app.transcript();
        let entries = transcript.entries();

        let user_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo 'Hello from shell'"));
        }

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());
        if let transcript::TranscriptEntry::SystemMessage { content } = system_entry.unwrap() {
            assert!(content.contains("Hello from shell"));
            assert!(content.contains("```"));
        }
    }

    #[test]
    fn test_execute_shell_command_creates_session_event() {
        let mut app = create_test_app();
        let initial_event_count = app.state().session.session_events.len();

        app.execute_shell_command("pwd".to_string());

        assert_eq!(app.state().session.session_events.len(), initial_event_count + 1);

        let event = &app.state().session.session_events[initial_event_count];
        assert_eq!(event.event_type, "shell_command");
        assert!(event.message.contains("Executed: pwd"));
        assert!(!event.timestamp.is_empty());
    }
}
