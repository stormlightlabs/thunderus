use crate::app::App;

use thunderus_core::MemoryPaths;

impl App {
    /// Handle /garden consolidate [session-id] command
    pub fn handle_garden_consolidate_command(&mut self, session_id: String) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);

        let events_file = if session_id == "latest" {
            match &self.session {
                Some(session) => session.events_file(),
                None => {
                    return self
                        .transcript_mut()
                        .add_system_message("No active session to consolidate");
                }
            }
        } else {
            let agent_dir = thunderus_core::AgentDir::new(&self.state.config.cwd);
            let session_dir = agent_dir.sessions_dir().join(&session_id);
            std::path::PathBuf::from(&session_dir).join("events.jsonl")
        };

        let gardener = thunderus_core::memory::Gardener::new(memory_paths);
        let result = match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(gardener.consolidate_session(&session_id, &events_file)),
            Err(_) => {
                return self
                    .transcript_mut()
                    .add_system_message("No tokio runtime available for consolidation");
            }
        };

        match result {
            Ok(consolidation_result) => {
                let mut msg = format!("🌱 Garden: Consolidated session {}\n\n", session_id);

                msg.push_str("Extracted:\n");
                msg.push_str(&format!("  • {} facts\n", consolidation_result.facts.len()));
                msg.push_str(&format!("  • {} ADRs\n", consolidation_result.adrs.len()));
                msg.push_str(&format!("  • {} playbooks\n", consolidation_result.playbooks.len()));

                let recap_path = consolidation_result
                    .recap
                    .as_ref()
                    .map(|r| r.path.display().to_string());
                let warnings = consolidation_result.warnings.clone();

                let memory_patches = consolidation_result.into_memory_patches();
                let patch_count = memory_patches.len();
                for patch in &memory_patches {
                    msg.push_str(&format!("  • [{}] {}\n", patch.kind, patch.description));
                }

                if !memory_patches.is_empty() {
                    msg.push_str(&format!(
                        "\n[M] Added {} memory patch(es) to review queue\n",
                        patch_count
                    ));
                    self.state_mut().memory_patches_mut().extend(memory_patches);
                }

                if let Some(ref path) = recap_path {
                    msg.push_str(&format!("\nRecap: {}\n", path));
                }

                for warning in &warnings {
                    msg.push_str(&format!("\n[!] {}\n", warning));
                }

                self.transcript_mut().add_system_message(msg);
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Consolidation failed: {}", e));
            }
        }
    }

    /// Handle /garden hygiene command
    pub fn handle_garden_hygiene_command(&mut self) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);
        let gardener = thunderus_core::memory::Gardener::new(memory_paths.clone());

        let result = gardener.check_hygiene();

        match result {
            Ok(violations) => {
                if violations.is_empty() {
                    self.transcript_mut()
                        .add_system_message("Hygiene Check: No violations found");
                } else {
                    let mut msg = format!("Hygiene Check Results:\n\n{} violation(s):\n\n", violations.len());
                    for v in &violations {
                        let severity =
                            if matches!(v.severity, thunderus_core::memory::Severity::Error) { "[E]" } else { "[W]" };
                        msg.push_str(&format!("  {} [{}] {}\n", severity, v.doc_id, v.message));
                        if let Some(fix) = &v.suggested_fix {
                            msg.push_str(&format!("      Fix: {}\n", fix));
                        }
                    }
                    self.transcript_mut().add_system_message(msg);
                }
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Hygiene check failed: {}", e));
            }
        }
    }

    /// Handle /garden drift command
    pub fn handle_garden_drift_command(&mut self) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);
        let gardener = thunderus_core::memory::Gardener::new(memory_paths.clone());

        let result = gardener.check_drift_auto();

        match result {
            Ok(drift_result) => {
                if drift_result.stale_docs.is_empty() {
                    self.transcript_mut()
                        .add_system_message("Drift Detection: All documents verified");
                } else {
                    let mut msg = format!(
                        "Drift Detection Results:\n\n{} stale document(s):\n\n",
                        drift_result.stale_docs.len()
                    );
                    for doc in &drift_result.stale_docs {
                        let severity = match doc.severity {
                            thunderus_core::memory::StalenessSeverity::Minor => "[minor]",
                            thunderus_core::memory::StalenessSeverity::Major => "[major]",
                            thunderus_core::memory::StalenessSeverity::Critical => "[CRIT]",
                        };
                        msg.push_str(&format!(
                            "  {} {} (changed: {:?})\n",
                            severity, doc.doc_id, doc.changed_files
                        ));
                    }
                    msg.push_str(&format!("\nCurrent commit: {}\n", drift_result.current_commit));
                    self.transcript_mut().add_system_message(msg);
                }
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Drift check failed: {}", e));
            }
        }
    }

    /// Handle /garden verify <doc-id> command
    pub fn handle_garden_verify_command(&mut self, doc_id: String) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);
        let gardener = thunderus_core::memory::Gardener::new(memory_paths.clone());

        let result = gardener.verify_document(&doc_id);

        match result {
            Ok(()) => {
                self.transcript_mut()
                    .add_system_message(format!("✓ Marked {} as verified at current commit", doc_id));
            }
            Err(e) => {
                self.transcript_mut()
                    .add_system_message(format!("Verification failed: {}", e));
            }
        }
    }

    /// Handle /garden stats command
    pub fn handle_garden_stats_command(&mut self) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);

        let manifest = match std::fs::read_to_string(memory_paths.manifest_file()) {
            Ok(content) => match serde_json::from_str::<thunderus_core::memory::MemoryManifest>(&content) {
                Ok(manifest) => manifest,
                Err(e) => {
                    return self
                        .transcript_mut()
                        .add_system_message(format!("Failed to parse manifest: {}", e));
                }
            },
            Err(e) => {
                return self
                    .transcript_mut()
                    .add_system_message(format!("Failed to read manifest: {}", e));
            }
        };

        let mut msg = "🌱 Memory Gardener Statistics:\n\n".to_string();
        msg.push_str(&format!("Total documents: {}\n\n", manifest.docs.len()));

        let mut kind_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for doc in &manifest.docs {
            *kind_counts.entry(format!("{:?}", doc.kind)).or_insert(0) += 1;
        }

        msg.push_str("Documents by type:\n");
        for (kind, count) in kind_counts.iter() {
            msg.push_str(&format!("  • {}: {}\n", kind, count));
        }

        self.transcript_mut().add_system_message(msg);
    }
}
