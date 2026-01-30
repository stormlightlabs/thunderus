use crate::app::App;

use thunderus_core::{ActionType, ApprovalDecision, ToolRisk};

impl App {
    /// Handle an approval request from the agent
    ///
    /// Called when the agent requests approval for an action (tool call, etc).
    /// Displays the approval prompt in the transcript and sets pending state.
    pub fn handle_approval_request(&mut self, request: thunderus_core::ApprovalRequest) {
        let action_type_str = match request.action_type {
            ActionType::Tool => "tool",
            ActionType::Shell => "shell",
            ActionType::FileWrite => "file write",
            ActionType::FileDelete => "file delete",
            ActionType::Network => "network",
            ActionType::Patch => "patch",
            ActionType::Generic => "generic",
        };
        let risk_str = match request.risk_level {
            ToolRisk::Safe => "safe",
            ToolRisk::Risky => "risky",
            ToolRisk::Blocked => "blocked",
        };

        if let Some(ref mut session) = self.session
            && request.risk_level.is_risky()
        {
            let action_type_for_hint = match request.action_type {
                ActionType::Tool => "tool",
                ActionType::Shell => "shell",
                ActionType::FileWrite => "file_write",
                ActionType::FileDelete => "file_delete",
                ActionType::Network => "network",
                ActionType::Patch => "patch",
                ActionType::Generic => "generic",
            };

            if let Some(concept) = thunderus_core::teaching::suggest_concept(
                action_type_for_hint,
                request.risk_level,
                request.description.as_str(),
            ) && let Ok(Some(hint)) = session.get_hint_for_concept(&concept)
            {
                self.state_mut().show_hint(hint);
            }
        }

        self.transcript_mut()
            .add_approval_prompt(format!("{}:{}", action_type_str, request.description), risk_str);
        self.state_mut().approval_ui.pending_approval = Some(
            crate::state::ApprovalState::pending(request.description.clone(), risk_str.to_string())
                .with_request_id(request.id),
        );
    }

    /// Send approval response back to agent
    ///
    /// Called when user responds to an approval prompt (y/n/c).
    /// Sends the decision back to the agent via TuiApprovalHandle and updates the transcript.
    ///
    /// Also handles direct shell command approvals (!cmd) by executing approved commands.
    pub fn send_approval_response(&mut self, decision: ApprovalDecision) {
        self.transcript_mut().set_approval_decision(decision);

        let pending_command = self.state_mut().approval_ui.pending_command.take();

        match self.state_mut().approval_ui.pending_approval.take() {
            Some(approval_state) => {
                let approved = matches!(decision, ApprovalDecision::Approved);
                self.persist_approval(&approval_state.action, approved);

                if approved && let Some(command) = pending_command {
                    let registry = thunderus_tools::ToolRegistry::with_builtin_tools();
                    let tool_call_id = format!("shell_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
                    self.do_execute_shell_command(command, &registry, tool_call_id);

                    let decision_str = match decision {
                        ApprovalDecision::Approved => "approved",
                        ApprovalDecision::Rejected => "rejected",
                        ApprovalDecision::Cancelled => "cancelled",
                    };
                    self.transcript_mut()
                        .add_system_message(format!("Shell command {}.", decision_str));
                    return;
                }

                if let Some(request_id) = approval_state.request_id
                    && let Some(ref handle) = self.approval_handle
                {
                    if handle.respond(request_id, decision) {
                        let decision_str = match decision {
                            ApprovalDecision::Approved => "approved",
                            ApprovalDecision::Rejected => "rejected",
                            ApprovalDecision::Cancelled => "cancelled",
                        };
                        self.transcript_mut()
                            .add_system_message(format!("Action {}.", decision_str));
                    } else {
                        self.transcript_mut()
                            .add_system_message("Approval request timed out or was already cancelled.");
                    }
                }
            }
            None => {
                self.transcript_mut().set_approval_decision(decision);
            }
        }
    }

    /// Actually execute the shell command (after approval or auto-approval)
    pub(crate) fn do_execute_shell_command(
        &mut self, command: String, registry: &thunderus_tools::ToolRegistry, tool_call_id: String,
    ) {
        match registry.execute("shell", tool_call_id.clone(), &serde_json::json!({"command": command})) {
            Ok(result) => match result.is_success() {
                true => {
                    self.transcript_mut()
                        .add_system_message(format!("Shell command output:\n```\n{}\n```", result.content));

                    self.state_mut()
                        .session
                        .session_events
                        .push(crate::state::SessionEvent {
                            event_type: "shell_command".to_string(),
                            message: format!("Executed: {}", command),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        });
                }
                false => self
                    .transcript_mut()
                    .add_system_message(format!("Shell command failed: {}", result.content)),
            },
            Err(e) => self
                .transcript_mut()
                .add_system_message(format!("Failed to execute shell command: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::create_test_app;
    use crate::state::ApprovalState;
    use crate::tui_approval::{TuiApprovalHandle, TuiApprovalProtocol};
    use thunderus_core::ApprovalDecision;

    #[test]
    fn test_send_approval_response_with_handle() {
        let mut app = create_test_app();

        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "safe".to_string()).with_request_id(123));

        app.transcript_mut().add_approval_prompt("test.action", "safe");

        app.send_approval_response(ApprovalDecision::Approved);
        assert!(app.state().approval_ui.pending_approval.is_none());
        assert!(app.transcript().len() >= 2);
    }

    #[test]
    fn test_send_approval_response_without_handle() {
        let mut app = create_test_app();

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "safe".to_string()).with_request_id(456));

        app.transcript_mut().add_approval_prompt("test.action", "safe");

        app.send_approval_response(ApprovalDecision::Approved);
        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[test]
    fn test_send_approval_response_reject() {
        let mut app = create_test_app();
        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("delete.file".to_string(), "dangerous".to_string()).with_request_id(789));

        app.transcript_mut().add_approval_prompt("delete.file", "dangerous");
        app.send_approval_response(ApprovalDecision::Rejected);

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[test]
    fn test_send_approval_response_cancel() {
        let mut app = create_test_app();

        let (tui_approval, _rx) = TuiApprovalProtocol::new();
        let handle = TuiApprovalHandle::from_protocol(&tui_approval);
        app.approval_handle = Some(handle);

        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("install.crate".to_string(), "risky".to_string()).with_request_id(999));

        app.transcript_mut().add_approval_prompt("install.crate", "risky");
        app.send_approval_response(ApprovalDecision::Cancelled);

        assert!(app.state().approval_ui.pending_approval.is_none());
    }
}
