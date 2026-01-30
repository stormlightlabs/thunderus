mod garden;
mod memory;
mod parser;

pub use parser::parse_slash_command;

use crate::app::App;
use crate::state::VerbosityLevel;

use thunderus_core::{ApprovalMode, ProviderConfig, SearchScope, ViewKind, ViewMaterializer};
use thunderus_providers::ProviderFactory;

impl App {
    /// Auto-materialize and save all views after session updates
    pub(super) fn materialize_views(&mut self) {
        if let Some(ref session) = self.session {
            let materializer = ViewMaterializer::new(session);
            let _ = materializer.materialize_all();
        }
    }

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
}

#[cfg(test)]
mod tests {
    use thunderus_core::ApprovalMode;

    use crate::app::create_test_app;
    use crate::transcript;

    #[test]
    fn test_handle_model_command_list() {
        let mut app = create_test_app();
        app.handle_model_command("list".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Available models"));
            assert!(content.contains("Current"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_model_command_unknown() {
        let mut app = create_test_app();
        app.handle_model_command("unknown-model".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(
                content.contains("Cannot switch")
                    || content.contains("Failed to switch")
                    || content.contains("Unknown model")
            );
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_status_command() {
        let mut app = create_test_app();

        app.handle_status_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Session Status"));
            assert!(content.contains("Profile"));
            assert!(content.contains("Provider"));
            assert!(content.contains("Approval Mode"));
            assert!(content.contains("Sandbox Mode"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_review_command() {
        let mut app = create_test_app();

        app.handle_review_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No pending patches"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_list() {
        let mut app = create_test_app();
        app.handle_approvals_command("list".to_string());

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Available approval modes"));
            assert!(content.contains("Current"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_approvals_command_read_only() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::Auto;

        app.handle_approvals_command("read-only".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::ReadOnly);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_auto() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::ReadOnly;

        app.handle_approvals_command("auto".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::Auto);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_full_access() {
        let mut app = create_test_app();
        app.state_mut().config.approval_mode = ApprovalMode::Auto;

        app.handle_approvals_command("full-access".to_string());

        assert_eq!(app.state.config.approval_mode, ApprovalMode::FullAccess);
        assert_eq!(app.transcript().len(), 1);
    }

    #[test]
    fn test_handle_approvals_command_unknown() {
        let mut app = create_test_app();
        let original_mode = app.state.config.approval_mode;

        app.handle_approvals_command("unknown-mode".to_string());

        assert_eq!(app.state.config.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Unknown approval mode"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[test]
    fn test_handle_plan_command() {
        let mut app = create_test_app();

        app.handle_plan_command();

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("No active session") || content.contains("Current Plan"));
        } else {
            panic!("Expected SystemMessage");
        }
    }
}
