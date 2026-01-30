use crate::app::App;
use crate::transcript;
use thunderus_agent::AgentEvent;

impl App {
    /// Check if a tool should be blocked due to file ownership
    ///
    /// Returns true if the tool is trying to write to a user-owned file.
    /// Write operations include: edit, write, create, patch, delete
    fn should_block_tool_for_ownership(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        let write_operations = [
            "edit",
            "write",
            "write_file",
            "create",
            "create_file",
            "patch",
            "apply_patch",
            "delete",
            "remove",
            "rm",
        ];

        if !write_operations.contains(&tool_name) {
            return false;
        }

        let file_path = args
            .get("file_path")
            .or_else(|| args.get("path"))
            .and_then(|v| v.as_str());

        if let Some(path_str) = file_path
            && let Some(ref session) = self.session
        {
            let path_buf = std::path::PathBuf::from(path_str);
            return session.is_owned_by_user(&path_buf);
        }

        false
    }

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
                if self.should_block_tool_for_ownership(&name, &args) {
                    self.transcript_mut()
                        .add_system_message("⛔ Write blocked: File is currently owned by user after manual edits.");
                    self.transcript_mut().add_system_message(
                        "The agent cannot write to files you've modified. Press 'Esc' to reconcile and transfer ownership back to the agent."
                    );
                    self.pause_token.cancel();
                    self.state_mut().pause_generation();
                    return;
                }

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
}
