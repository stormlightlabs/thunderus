use crate::app::App;
use crossterm::style::Stylize;
use thunderus_agent::Agent;
use thunderus_core::{ApprovalGate, ApprovalMode, ApprovalProtocol, PatchQueueManager, SessionId};
use thunderus_tools::{SessionToolDispatcher, ToolDispatcher, ToolRegistry};
use tokio::sync::mpsc;

impl App {
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
}
