use crate::app::App;

impl App {
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
