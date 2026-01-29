use crate::app::App;
use crate::transcript;
use crossterm::style::Stylize;
use thunderus_agent::{Agent, AgentEvent};
use thunderus_core::{
    ActionType, ApprovalDecision, ApprovalGate, ApprovalMode, ApprovalProtocol, PatchQueueManager, SessionId, ToolRisk,
};
use thunderus_tools::{SessionToolDispatcher, ToolDispatcher, ToolRegistry};
use tokio::sync::mpsc;

impl App {
    /// Capture current workspace snapshot state
    ///
    /// Records the current git state for drift comparison later.
    /// Should be called before agent operations to establish a baseline.
    pub fn capture_snapshot_state(&mut self) {
        if let Some(ref sm) = self.snapshot_manager {
            match sm.get_current_state() {
                Ok(state) => {
                    self.last_snapshot_state = Some(state);
                }
                Err(e) => {
                    eprintln!("Failed to capture snapshot state: {}", e);
                }
            }
        }
    }

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

    /// Check for drift by comparing current state against last captured snapshot
    ///
    /// Returns true if drift was detected (state changed), false otherwise.
    pub fn check_drift_and_trigger(&mut self) -> bool {
        let Some(ref sm) = self.snapshot_manager else {
            return false;
        };

        let Some(ref last_state) = self.last_snapshot_state else {
            self.capture_snapshot_state();
            return false;
        };

        match sm.get_current_state() {
            Ok(current_state) => {
                if current_state != *last_state {
                    self.handle_drift_event(thunderus_core::DriftEvent::StateMismatch {
                        expected: last_state.clone(),
                        actual: current_state.clone(),
                    });
                    self.last_snapshot_state = Some(current_state);
                    return true;
                }
                false
            }
            Err(e) => {
                eprintln!("Failed to get current state for drift check: {}", e);
                false
            }
        }
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

    /// Spawn agent to process a user message
    ///
    /// Creates a new agent task that will stream events back to the TUI.
    /// The agent runs in the background, sending events through the channel.
    /// Captures snapshot state before spawning for drift detection.
    pub fn spawn_agent_for_message(
        &mut self, message: String, provider: &std::sync::Arc<dyn thunderus_providers::Provider>,
    ) {
        self.capture_snapshot_state();

        let (tui_approval, approval_request_rx) = crate::tui_approval::TuiApprovalProtocol::new();
        self.approval_request_rx = Some(approval_request_rx);

        let approval_handle = crate::tui_approval::TuiApprovalHandle::from_protocol(&tui_approval);
        self.approval_handle = Some(approval_handle);

        let approval_protocol = std::sync::Arc::new(tui_approval) as std::sync::Arc<dyn ApprovalProtocol>;
        let session_id = SessionId::new();
        let cancel_token = self.cancel_token.clone();
        let provider_clone = std::sync::Arc::clone(provider);
        let approval_gate = ApprovalGate::new(self.state().config.approval_mode, self.state().config.allow_network);

        let mut agent = Agent::new(provider_clone, approval_protocol, approval_gate, session_id);
        self.set_approval_gate_handle(agent.approval_gate());

        let tool_specs = if let Some(profile) = self.profile() {
            let mut registry = ToolRegistry::with_builtin_tools();
            if let Err(e) = registry.load_skills() {
                eprintln!("{} Failed to load skills: {}", "Warning:".yellow(), e);
            }
            registry.set_profile(profile.clone());
            registry.set_approval_gate(ApprovalGate::new(
                ApprovalMode::FullAccess,
                profile.is_network_allowed(),
            ));
            let specs = registry.specs();
            if let Some(ref session) = self.session {
                let dispatcher = ToolDispatcher::new(registry);

                if self.patch_queue_manager.is_none() {
                    let agent_dir = session.agent_dir().clone();
                    let patch_queue_manager = PatchQueueManager::new(session.id.clone(), agent_dir.clone());
                    let patch_queue_manager = patch_queue_manager
                        .load()
                        .unwrap_or_else(|_| PatchQueueManager::new(session.id.clone(), agent_dir));
                    self.patch_queue_manager = Some(patch_queue_manager);
                }

                let session_dispatcher = if let Some(ref pqm) = self.patch_queue_manager {
                    SessionToolDispatcher::with_history_and_queue(dispatcher, session.clone(), pqm.clone())
                } else {
                    SessionToolDispatcher::with_new_history(dispatcher, session.clone())
                };

                agent = agent.with_tool_dispatcher(std::sync::Arc::new(std::sync::Mutex::new(session_dispatcher)));
            }
            Some(specs)
        } else {
            let registry = ToolRegistry::with_builtin_tools();
            Some(registry.specs())
        };

        if let Some(profile) = self.profile() {
            agent = agent.with_profile(profile.clone());
        }

        if let Some(retriever) = self.memory_retriever() {
            agent = agent.with_memory_retriever(std::sync::Arc::clone(&retriever));
        }
        self.state_mut().start_generation();

        let (tx, rx) = mpsc::unbounded_channel();
        self.agent_event_rx = Some(rx);

        if self.pause_token.is_cancelled() {
            self.pause_token = tokio_util::sync::CancellationToken::new();
        }
        let pause_token = self.pause_token.clone();

        let user_owned_files = if let Some(ref session) = self.session {
            session
                .file_ownership
                .iter()
                .filter(|(_, owner)| *owner == "user")
                .map(|(path, _)| path.clone())
                .collect()
        } else {
            Vec::new()
        };

        let snapshot_manager = self.snapshot_manager.clone();
        let last_snapshot_state = self.last_snapshot_state.clone();

        tokio::spawn(async move {
            let mut message_processed = false;
            while !message_processed {
                if pause_token.is_cancelled() {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }

                if let Some(ref sm) = snapshot_manager
                    && let Some(ref last_state) = last_snapshot_state
                {
                    match sm.get_current_state() {
                        Ok(current_state) => {
                            if current_state != *last_state {
                                let _ = tx.send(thunderus_agent::AgentEvent::Error(
                                    "Drift detected: Workspace state has changed. Press 'Esc' to reconcile."
                                        .to_string(),
                                ));
                                break;
                            }
                        }
                        Err(e) => eprintln!("Failed to check for drift: {}", e),
                    }
                }

                match agent
                    .process_message(
                        &message,
                        tool_specs.clone(),
                        cancel_token.clone(),
                        user_owned_files.clone(),
                    )
                    .await
                {
                    Ok(mut event_rx) => {
                        while let Some(event) = event_rx.recv().await {
                            if pause_token.is_cancelled() {
                                let _ = tx.send(thunderus_agent::AgentEvent::Error("Agent paused".to_string()));
                                break;
                            }
                            let _ = tx.send(event);
                        }
                        message_processed = true;
                    }
                    Err(e) => {
                        let _ = tx.send(thunderus_agent::AgentEvent::Error(format!("Agent error: {}", e)));
                        message_processed = true;
                    }
                }
            }
        });
    }

    /// Handle a drift event from the workspace monitor
    pub fn handle_drift_event(&mut self, event: thunderus_core::DriftEvent) {
        let show_explainer = self
            .session
            .as_ref()
            .map(|s| s.drift_explainer_shown().ok() == Some(false))
            .unwrap_or(false);

        if show_explainer {
            self.transcript_mut().add_system_message("");
            self.transcript_mut()
                .add_system_message("MIXED-INITIATIVE COLLABORATION");
            self.transcript_mut().add_system_message("");
            self.transcript_mut()
                .add_system_message("You edited files while the agent was working. This is called 'drift'.");
            self.transcript_mut()
                .add_system_message("The agent paused to avoid conflicts. You can:");
            self.transcript_mut()
                .add_system_message("  - Press 'Esc' to reconcile - let the agent re-sync with your changes");
            self.transcript_mut()
                .add_system_message("  - Continue working - the agent will wait for you to finish");
            self.transcript_mut().add_system_message("");
            if let Some(ref mut session) = self.session {
                let _ = session.mark_drift_explainer_shown();
            }
        }

        match event {
            thunderus_core::DriftEvent::FileSystemChange(paths) => {
                let paths_str = paths
                    .iter()
                    .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.transcript_mut()
                    .add_system_message(format!("External change detected in: {}", paths_str));

                if let Some(ref mut session) = self.session {
                    for path in &paths {
                        session.claim_ownership(path.clone(), "user".to_string());
                    }
                }

                if self.state().is_generating() {
                    self.pause_token.cancel();
                    self.state_mut().pause_generation();
                    self.transcript_mut().add_system_message(
                        "Agent paused due to external workspace changes. Press 'Esc' to reconcile or 'c' to continue.",
                    );
                }
            }
            thunderus_core::DriftEvent::StateMismatch { expected, actual } => {
                self.transcript_mut().add_system_message(format!(
                    "Workspace state mismatch! Expected: {}, Actual: {}",
                    expected, actual
                ));

                if self.state().is_generating() {
                    self.pause_token.cancel();
                    self.state_mut().pause_generation();
                    self.transcript_mut()
                        .add_system_message("Agent paused due to state mismatch. Press 'Esc' to reconcile.");
                }
            }
        }
    }

    /// Start the reconcile ritual after drift/interruption
    ///
    /// Presents the user with options for how to handle the detected drift:
    /// - "Update Plan & Continue": Accept changes and let agent re-assess
    /// - "Discard User Changes": Revert to agent's last known state (requires explicit confirmation)
    /// - "Stop/Reset": Stop the agent entirely
    pub fn start_reconcile_ritual(&mut self) {
        self.state_mut().start_reconcile();
        self.transcript_mut().add_system_message("RECONCILE RITUAL");

        if let Some(ref sm) = self.snapshot_manager {
            match sm.get_current_state() {
                Ok(state) => {
                    if let Some(ref expected) = self.last_snapshot_state {
                        let drift_summary = format!("Expected: {}\nActual:   {}", expected, state);
                        self.transcript_mut()
                            .add_system_message(format!("Drift detected:\n{}", drift_summary));
                    } else {
                        self.transcript_mut()
                            .add_system_message(format!("Current workspace state: {}", state));
                    }
                }
                Err(e) => {
                    self.transcript_mut()
                        .add_system_message(format!("Failed to capture workspace state: {}", e));
                }
            }
        }

        if let Some(ref session) = self.session {
            let user_files: Vec<_> = session
                .file_ownership
                .iter()
                .filter(|(_, owner)| *owner == "user")
                .map(|(path, _)| path.display().to_string())
                .collect();

            if !user_files.is_empty() {
                self.transcript_mut()
                    .add_system_message(format!("\nUser-modified files ({}):", user_files.len()));
                for file in &user_files {
                    self.transcript_mut().add_system_message(format!("  - {}", file));
                }
            }
        }

        self.transcript_mut().add_system_message("\nReconcile Options:");
        self.transcript_mut()
            .add_system_message("  [Enter] Update Plan & Continue - Agent will read your changes and re-assess");
        self.transcript_mut()
            .add_system_message("  [Esc]   Discard Changes     - Revert your changes (CAUTION: destructive)");
        self.transcript_mut()
            .add_system_message("  [q]     Stop/Reset          - Stop the agent entirely");

        if self.pause_token.is_cancelled() {
            self.pause_token = tokio_util::sync::CancellationToken::new();
        }

        self.transcript_mut()
            .add_system_message("\nPress a key to choose your reconcile action...");
    }

    /// Continue after reconciliation - agent proceeds with updated context
    pub fn reconcile_continue(&mut self) {
        self.transcript_mut()
            .add_system_message("✓ Continuing with updated plan...");
        self.transcript_mut()
            .add_system_message("Agent will re-sync with your changes and proceed.");

        if let Some(ref mut session) = self.session {
            session.file_ownership.clear();
        }

        self.capture_snapshot_state();

        self.state_mut().stop_generation();
        self.transcript_mut()
            .add_system_message("Ready. Send a message or let the agent continue.");
    }

    /// Discard user changes - reverts to last agent state (DESTRUCTIVE)
    pub fn reconcile_discard(&mut self) {
        self.transcript_mut()
            .add_system_message("[!] Discarding user changes...");
        self.transcript_mut()
            .add_system_message("Reverting to last agent snapshot state...");

        let result = std::process::Command::new("git")
            .args(["restore", "."])
            .current_dir(self.state().cwd())
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    self.transcript_mut()
                        .add_system_message("✓ All uncommitted changes have been discarded.");

                    if let Some(ref mut session) = self.session {
                        session.file_ownership.clear();
                    }

                    self.capture_snapshot_state();

                    self.state_mut().stop_generation();
                    self.transcript_mut()
                        .add_system_message("Ready. Workspace is now at the last agent state.");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    self.transcript_mut()
                        .add_system_message(format!("Failed to discard changes: {}", stderr));
                    self.transcript_mut()
                        .add_system_message("Please manually revert changes with: git restore .");
                    self.state_mut().stop_generation();
                }
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Failed to run git restore: {}", e));
                self.transcript_mut()
                    .add_system_message("Please manually revert changes with: git restore .");
                self.state_mut().stop_generation();
            }
        }
    }

    /// Stop/reset agent - exits the agent loop entirely
    pub fn reconcile_stop(&mut self) {
        self.transcript_mut().add_system_message("🛑 Stopping agent...");
        self.cancel_token.cancel();
        self.state_mut().stop_generation();
        self.transcript_mut()
            .add_system_message("Agent stopped. You can start fresh with a new message.");
    }
}
