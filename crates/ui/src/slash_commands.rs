use crate::app::App;
use crate::state::VerbosityLevel;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thunderus_core::{ApprovalMode, MemoryPaths, ProviderConfig, SearchScope, ViewKind, ViewMaterializer};
use thunderus_providers::ProviderFactory;

impl App {
    /// Handle /model command
    pub fn handle_model_command(&mut self, model: String) {
        match model.as_str() {
            "list" => {
                let provider_name = self.state.provider_name();
                let model_name = self.state.model_name();
                self.transcript_mut().add_system_message(format!(
                    "Available models:\n  Current: {} ({})\n  Available: glm-4.7, gemini-2.5-flash",
                    provider_name, model_name
                ))
            }
            _ => {
                let new_provider = match &self.state.config.provider {
                    ProviderConfig::Glm { api_key, base_url, thinking, .. } => {
                        if !model.starts_with("glm") {
                            self.transcript_mut().add_system_message(
                                "Cannot switch to a Gemini model while using a GLM provider. Update your profile to change providers.",
                            );
                            return;
                        }
                        ProviderConfig::Glm {
                            api_key: api_key.clone(),
                            model: model.clone(),
                            base_url: base_url.clone(),
                            thinking: thinking.clone(),
                            options: Default::default(),
                        }
                    }
                    ProviderConfig::Gemini { api_key, base_url, thinking, .. } => {
                        if !model.starts_with("gemini") {
                            self.transcript_mut().add_system_message(
                                "Cannot switch to a GLM model while using a Gemini provider. Update your profile to change providers.",
                            );
                            return;
                        }
                        ProviderConfig::Gemini {
                            api_key: api_key.clone(),
                            model: model.clone(),
                            base_url: base_url.clone(),
                            thinking: thinking.clone(),
                            options: Default::default(),
                        }
                    }
                    ProviderConfig::Mock { .. } => {
                        self.transcript_mut()
                            .add_system_message("Cannot switch models while using Mock provider.");
                        return;
                    }
                };

                match ProviderFactory::create_from_config(&new_provider) {
                    Ok(provider) => {
                        self.state.config.provider = new_provider;
                        self.state.model_selector.current_model = model.clone();
                        self.set_provider(provider);
                        self.transcript_mut()
                            .add_system_message(format!("Model switched to {}", model));
                    }
                    Err(e) => self
                        .transcript_mut()
                        .add_system_message(format!("Failed to switch model: {}", e)),
                }
            }
        }
    }

    /// Handle /approvals command
    pub fn handle_approvals_command(&mut self, mode: String) {
        match mode.as_str() {
            "list" => {
                let current_mode = self.state.config.approval_mode;
                self.transcript_mut().add_system_message(format!(
                    "Available approval modes:\n  Current: {}\n  Available: read-only, auto, full-access",
                    current_mode
                ))
            }
            "read-only" => {
                let old_mode = self.state.config.approval_mode;
                self.state.config.approval_mode = ApprovalMode::ReadOnly;
                self.update_approval_gate(ApprovalMode::ReadOnly);
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → read-only", old_mode));
            }
            "auto" => {
                let old_mode = self.state.config.approval_mode;
                self.state.config.approval_mode = ApprovalMode::Auto;
                self.update_approval_gate(ApprovalMode::Auto);
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → auto", old_mode));
            }
            "full-access" => {
                let old_mode = self.state.config.approval_mode;
                self.state.config.approval_mode = ApprovalMode::FullAccess;
                self.update_approval_gate(ApprovalMode::FullAccess);
                self.transcript_mut()
                    .add_system_message(format!("Approval mode changed: {} → full-access", old_mode));
            }
            _ => self.transcript_mut().add_system_message(format!(
                "Unknown approval mode: {}. Use /approvals list to see available modes.",
                mode
            )),
        }
    }

    /// Handle /verbosity command
    pub fn handle_verbosity_command(&mut self, level: String) {
        match level.as_str() {
            "list" => {
                let current_level = self.state.verbosity();
                self.transcript_mut().add_system_message(format!(
                    "Available verbosity levels:\n  Current: {}\n  Available: quiet, default, verbose",
                    current_level.as_str()
                ))
            }
            "quiet" => {
                let old_level = self.state.verbosity();
                self.state.config.verbosity = VerbosityLevel::Quiet;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → quiet", old_level.as_str()));
            }
            "default" => {
                let old_level = self.state.verbosity();
                self.state.config.verbosity = VerbosityLevel::Default;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → default", old_level.as_str()));
            }
            "verbose" => {
                let old_level = self.state.verbosity();
                self.state.config.verbosity = VerbosityLevel::Verbose;
                self.transcript_mut()
                    .add_system_message(format!("Verbosity changed: {} → verbose", old_level.as_str()));
            }
            _ => self.transcript_mut().add_system_message(format!(
                "Unknown verbosity level: {}. Use /verbosity list to see available levels.",
                level
            )),
        }
    }

    /// Handle /status command
    pub fn handle_status_command(&mut self) {
        let profile = self.state.config.profile.clone();
        let provider_name = self.state.provider_name();
        let model_name = self.state.model_name();
        let approval_mode = self.state.config.approval_mode;
        let sandbox_mode = self.state.config.sandbox_mode;
        let verbosity = self.state.config.verbosity;
        let cwd = self.state.config.cwd.display();
        let session_events_count = self.state.session.session_events.len();
        let modified_files_count = self.state.session.modified_files.len();
        let has_pending_approval = self.state.approval_ui.pending_approval.is_some();

        let status = format!(
            "Session Status:\n\
             Profile: {}\n\
             Provider: {} ({})\n\
             Approval Mode: {}\n\
             Sandbox Mode: {}\n\
             Verbosity: {}\n\
             Working Directory: {}\n\
             Session Events: {}\n\
             Modified Files: {}\n\
             Pending Approvals: {}",
            profile,
            provider_name,
            model_name,
            approval_mode,
            sandbox_mode,
            verbosity.as_str(),
            cwd,
            session_events_count,
            modified_files_count,
            has_pending_approval
        );
        self.transcript_mut().add_system_message(status);
    }

    /// Handle /plan command
    pub fn handle_plan_command(&mut self) {
        match self.session {
            Some(ref session) => match ViewMaterializer::new(session).materialize(ViewKind::Plan) {
                Ok(content) => self
                    .transcript_mut()
                    .add_system_message(format!("## Current Plan\n\n{}", content)),
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to materialize plan: {}", e)),
            },
            None => self
                .transcript_mut()
                .add_system_message("No active session to materialize plan from"),
        }
    }

    /// Handle /review command
    pub fn handle_review_command(&mut self) {
        let patches = self.state.patches();
        if patches.is_empty() {
            self.transcript_mut()
                .add_system_message("No pending patches to review.");
        } else {
            let mut review_text = String::from("## Pending Patches for Review\n\n");
            for (idx, patch) in patches.iter().enumerate() {
                review_text.push_str(&format!("### Patch {} - {}\n\n", idx + 1, patch.name));
                let files_str: Vec<String> = patch.files.iter().map(|p| p.display().to_string()).collect();
                review_text.push_str(&format!("Files: {}\n\n", files_str.join(", ")));
                review_text.push_str("```diff\n");
                review_text.push_str(&patch.diff);
                review_text.push_str("\n```\n\n");
            }
            self.transcript_mut().add_system_message(review_text);
        }
    }

    /// Handle /memory command
    pub fn handle_memory_command(&mut self) {
        match self.session {
            Some(ref session) => match ViewMaterializer::new(session).materialize(ViewKind::Memory) {
                Ok(content) => self
                    .transcript_mut()
                    .add_system_message(format!("## Project Memory\n\n{}", content)),
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to materialize memory: {}", e)),
            },
            None => self
                .transcript_mut()
                .add_system_message("No active session to materialize memory from"),
        }
    }

    /// Auto-materialize and save all views after session updates
    fn materialize_views(&mut self) {
        if let Some(ref session) = self.session {
            let materializer = ViewMaterializer::new(session);
            let _ = materializer.materialize_all();
        }
    }

    /// Handle /plan add <item> command
    pub fn handle_plan_add_command(&mut self, item: String) {
        if let Some(ref mut session) = self.session {
            match session.append_plan_update("add", &item, None) {
                Ok(_) => {
                    self.transcript_mut()
                        .add_system_message(format!("Added to plan: {}", item));
                    self.materialize_views();
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to add plan item: {}", e)),
            }
        } else {
            self.transcript_mut()
                .add_system_message("No active session to add plan item to");
        }
    }

    /// Handle /plan done <n> command
    pub fn handle_plan_done_command(&mut self, index: usize) {
        match self.session {
            Some(ref mut session) => match ViewMaterializer::new(session).materialize(ViewKind::Plan) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let mut task_count = 0;
                    let mut target_item = None;

                    for line in lines {
                        if line.trim().starts_with("- [ ]") {
                            task_count += 1;
                            if task_count == index {
                                target_item = Some(line.trim().trim_start_matches("- [ ]").trim().to_string());
                                break;
                            }
                        }
                    }

                    match target_item {
                        Some(item) => match session.append_plan_update("complete", &item, None) {
                            Ok(_) => {
                                self.transcript_mut()
                                    .add_system_message(format!("Marked as done: {}", item));
                                self.materialize_views();
                            }
                            Err(e) => self
                                .transcript_mut()
                                .add_system_message(format!("Failed to mark item as done: {}", e)),
                        },
                        None => self
                            .transcript_mut()
                            .add_system_message(format!("No task found at index {}", index)),
                    }
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to read plan: {}", e)),
            },
            None => self
                .transcript_mut()
                .add_system_message("No active session to update plan in"),
        }
    }

    /// Handle /memory add <fact> command
    pub fn handle_memory_add_command(&mut self, fact: String) {
        if let Some(ref mut session) = self.session {
            let mut hasher = DefaultHasher::new();
            fact.hash(&mut hasher);
            let content_hash = format!("{:x}", hasher.finish());

            match session.append_memory_update("core", "MEMORY.md", "update", &content_hash) {
                Ok(_) => {
                    self.transcript_mut()
                        .add_system_message(format!("Added to memory: {}", fact));
                    self.materialize_views();
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Failed to add memory: {}", e)),
            }
        } else {
            self.transcript_mut()
                .add_system_message("No active session to add memory to");
        }
    }

    /// Handle /memory search <query> command
    ///
    /// Searches the memory store and displays results in the memory hits panel.
    pub fn handle_memory_search_command(&mut self, query: String) {
        let memory_paths = MemoryPaths::from_thunderus_root(&self.state.config.cwd);
        let db_path = memory_paths.indexes.join("memory.db");

        let store = match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(thunderus_store::MemoryStore::open(&db_path)) {
                Ok(store) => store,
                Err(e) => {
                    return self
                        .transcript_mut()
                        .add_system_message(format!("Failed to open memory store: {}", e));
                }
            },
            Err(_) => {
                return self
                    .transcript_mut()
                    .add_system_message("No tokio runtime available for memory search");
            }
        };

        let hits = match tokio::runtime::Handle::try_current() {
            Ok(handle) => match handle.block_on(store.search(&query, thunderus_store::SearchFilters::default())) {
                Ok(hits) => hits,
                Err(e) => {
                    return self
                        .transcript_mut()
                        .add_system_message(format!("Memory search failed: {}", e));
                }
            },
            Err(_) => {
                return self
                    .transcript_mut()
                    .add_system_message("No tokio runtime available for memory search");
            }
        };

        if hits.is_empty() {
            self.transcript_mut()
                .add_system_message(format!("No memory results found for '{}'", query));
            self.state_mut().memory_hits.clear();
        } else {
            self.transcript_mut()
                .add_system_message(format!("Found {} memory result(s) for '{}'", hits.len(), query));

            let start = std::time::Instant::now();
            let search_time = start.elapsed().as_millis() as u64;

            self.state_mut().memory_hits.set_hits(hits, query, search_time);
        }
    }

    /// Handle /memory pin <id> command
    ///
    /// Pins a memory document to the current context set.
    pub fn handle_memory_pin_command(&mut self, id: String) {
        if self.state().memory_hits.is_pinned(&id) {
            self.state_mut().memory_hits.unpin(&id);
            self.transcript_mut()
                .add_system_message(format!("Unpinned memory: {}", id));
        } else {
            self.state_mut().memory_hits.pin(id.clone());
            self.transcript_mut()
                .add_system_message(format!("Pinned memory: {}", id));
        }

        let pinned_count = self.state().memory_hits.pinned_count();
        if pinned_count > 0 {
            self.transcript_mut()
                .add_system_message(format!("Total pinned: {}", pinned_count));
        }
    }

    /// Handle /search <query> command
    pub fn handle_search_command(&mut self, query: String, scope: SearchScope) {
        match self.session {
            Some(ref session) => match thunderus_core::search_session(&session.session_dir(), &query, scope) {
                Ok(hits) => {
                    if hits.is_empty() {
                        self.transcript_mut()
                            .add_system_message(format!("No results found for '{}'", query));
                    } else {
                        let scope_str = match scope {
                            SearchScope::All => "all files",
                            SearchScope::Events => "events",
                            SearchScope::Views => "views",
                        };

                        let mut results_text = format!("## Search Results for '{}' in {}\n\n", query, scope_str);
                        results_text.push_str(&format!("Found {} match(es):\n\n", hits.len()));

                        for hit in hits.iter().take(20) {
                            results_text.push_str(&format!("**{}:{}**\n", hit.file, hit.line));
                            results_text.push_str(&format!("```\n{}\n```\n\n", hit.content));
                        }

                        if hits.len() > 20 {
                            results_text.push_str(&format!("... and {} more results\n", hits.len() - 20));
                        }

                        self.transcript_mut().add_system_message(results_text);
                    }
                }
                Err(e) => self
                    .transcript_mut()
                    .add_system_message(format!("Search failed: {}", e)),
            },
            None => self.transcript_mut().add_system_message("No active session to search"),
        }
    }

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
