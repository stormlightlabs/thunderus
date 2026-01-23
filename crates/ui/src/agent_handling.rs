use crate::app::App;
use crate::transcript;
use thunderus_agent::{Agent, AgentEvent};
use thunderus_core::{ActionType, ApprovalDecision, ApprovalGate, ApprovalMode, ApprovalProtocol, SessionId, ToolRisk};
use tokio::sync::mpsc;

impl App {
    /// Handle agent streaming events
    ///
    /// Processes events from the agent (tokens, tool calls, approvals, errors)
    /// and updates the transcript and application state accordingly.
    pub fn handle_agent_event(&mut self, event: thunderus_agent::AgentEvent) {
        match event {
            AgentEvent::Token(text) => {
                if self.streaming_model_content.is_none() {
                    self.streaming_model_content = Some(String::new());
                }
                if let Some(ref mut buffer) = self.streaming_model_content {
                    buffer.push_str(&text);
                }
                self.transcript_mut().add_streaming_token(&text);
            }
            AgentEvent::ToolCall { name, args, risk, description, task_context, scope, classification_reasoning } => {
                let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                let risk_str = risk.as_str();
                self.transcript_mut().add_tool_call(&name, &args_str, risk_str);
                if let Some(entry) = self.transcript_mut().last_mut()
                    && let transcript::TranscriptEntry::ToolCall {
                        description: d,
                        task_context: tc,
                        scope: sc,
                        classification_reasoning: cr,
                        ..
                    } = entry
                {
                    if let Some(desc) = description {
                        *d = Some(desc);
                    }
                    if let Some(ctx) = task_context {
                        *tc = Some(ctx);
                    }
                    if let Some(scp) = scope {
                        *sc = Some(scp);
                    }
                    if let Some(reasoning) = classification_reasoning {
                        *cr = Some(reasoning);
                    }
                }
                self.persist_tool_call(&name, &args);
            }
            AgentEvent::ToolResult { name, result, success, error, metadata } => {
                self.transcript_mut().add_tool_result(&name, &result, success);
                if let Some(err) = error
                    && let Some(entry) = self.transcript_mut().last_mut()
                    && let transcript::TranscriptEntry::ToolResult { error: e, .. } = entry
                {
                    *e = Some(err);
                }
                let result_json = serde_json::json!({
                    "output": result
                });
                self.persist_tool_result(&name, &result_json, success, if success { None } else { Some("") });

                if let Some(entry) = self.transcript_mut().last_mut()
                    && let transcript::TranscriptEntry::ToolResult { .. } = entry
                    && let Some(exec_time) = metadata.execution_time_ms
                    && exec_time > 0
                {
                    let time_str = if exec_time < 1000 {
                        format!("{}ms", exec_time)
                    } else {
                        format!("{:.2}s", exec_time as f64 / 1000.0)
                    };
                    self.transcript_mut()
                        .add_system_message(format!("Tool execution time: {}", time_str));
                }
            }
            AgentEvent::ApprovalRequest(request) => {
                eprintln!("Unexpected approval request via agent event: {:?}", request.id)
            }
            AgentEvent::ApprovalResponse(_response) => self.state_mut().approval_ui.pending_approval = None,
            AgentEvent::Error(msg) => {
                let error_type = match msg.as_str() {
                    m if m.contains("cancelled") => transcript::ErrorType::Cancelled,
                    m if m.contains("timeout") || m.contains("network") => transcript::ErrorType::Network,
                    m if m.contains("provider") || m.contains("API") => transcript::ErrorType::Provider,
                    _ => transcript::ErrorType::Other,
                };

                eprintln!("[Agent Error] {}", msg);

                self.transcript_mut().add_error(msg, error_type);
                self.state_mut().stop_generation();
            }
            AgentEvent::Done => {
                self.transcript_mut().finish_streaming();
                self.state_mut().stop_generation();

                if let Some(content) = self.streaming_model_content.take() {
                    self.persist_model_message(&content);
                }
            }
            AgentEvent::ApprovalModeChanged { from, to } => self.transcript_mut().add_system_message(format!(
                "Approval mode changed: {} → {}",
                from.as_str(),
                to.as_str()
            )),
            AgentEvent::MemoryRetrieval { query: _, chunks, total_tokens, search_time_ms } => {
                let time_str = if search_time_ms < 1000 {
                    format!("{}ms", search_time_ms)
                } else {
                    format!("{:.2}s", search_time_ms as f64 / 1000.0)
                };
                self.transcript_mut().add_system_message(format!(
                    "Memory retrieval: {} chunks ({} tokens) in {}",
                    chunks.len(),
                    total_tokens,
                    time_str
                ));
            }
        }
    }

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
    pub fn send_approval_response(&mut self, decision: ApprovalDecision) {
        self.transcript_mut().set_approval_decision(decision);

        match self.state_mut().approval_ui.pending_approval.take() {
            Some(approval_state) => {
                let approved = matches!(decision, ApprovalDecision::Approved);
                self.persist_approval(&approval_state.action, approved);

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

    /// Spawn agent to process a user message
    ///
    /// Creates a new agent task that will stream events back to the TUI.
    /// The agent runs in the background, sending events through the channel.
    pub fn spawn_agent_for_message(
        &mut self, message: String, provider: &std::sync::Arc<dyn thunderus_providers::Provider>,
    ) {
        let (tui_approval, approval_request_rx) = crate::tui_approval::TuiApprovalProtocol::new();
        self.approval_request_rx = Some(approval_request_rx);

        let approval_handle = crate::tui_approval::TuiApprovalHandle::from_protocol(&tui_approval);
        self.approval_handle = Some(approval_handle);

        let approval_protocol = std::sync::Arc::new(tui_approval) as std::sync::Arc<dyn ApprovalProtocol>;
        let session_id = SessionId::new();
        let cancel_token = self.cancel_token.clone();
        let provider_clone = std::sync::Arc::clone(provider);
        let approval_gate = ApprovalGate::new(ApprovalMode::Auto, false);

        let mut agent = Agent::new(provider_clone, approval_protocol, approval_gate, session_id);
        self.state_mut().start_generation();

        let (tx, rx) = mpsc::unbounded_channel();
        self.agent_event_rx = Some(rx);

        tokio::spawn(async move {
            match agent.process_message(&message, None, cancel_token).await {
                Ok(mut event_rx) => {
                    while let Some(event) = event_rx.recv().await {
                        let _ = tx.send(event);
                    }
                }
                Err(e) => {
                    let _ = tx.send(thunderus_agent::AgentEvent::Error(format!("Agent error: {}", e)));
                }
            }
        });
    }
}
