use super::backend::{BackendBridge, spawn_backend};
use super::model::{AppModel, BackendEvent, BootstrapState, Effect, Message, ModelMessage};
use super::model::{SessionAgeGroup, SessionSummary, TokenUsageSummary};
use super::model::{collect_workspace_files, color_hex, update_model};
use super::storage::{PersistedUiState, StateStore};
use chrono::Utc;
use iced::task::Task;
use iced::theme::Palette;
use iced::{Subscription, Theme, window};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thndrs_core::Config;
use thndrs_mem::{MemoryKind, MemoryStore, Session, SessionManager};
use thndrs_providers::create_provider;

pub struct DesktopApp {
    pub model: AppModel,
    backend: Option<BackendBridge>,
    memory_store: Option<MemoryStore>,
    session_manager: Option<SessionManager>,
    store: StateStore,
}

pub fn boot() -> DesktopApp {
    let store = StateStore::new_default();
    let persisted = store.load();

    let mut warning = None;
    let workspace_root = persisted.workspace_root.and_then(|path| {
        if path.is_dir() {
            Some(path)
        } else {
            warning = Some(format!("Saved workspace does not exist: {}", path.display()));
            None
        }
    });

    let bootstrap = BootstrapState {
        workspace_root: workspace_root.clone(),
        recent_workspaces: persisted.recent_workspaces,
        composer_text: persisted.composer_text,
        last_model: persisted.last_model,
        warning,
    };

    let mut app =
        DesktopApp { model: AppModel::new(bootstrap), backend: None, memory_store: None, session_manager: None, store };

    if let Some(workspace_root) = workspace_root {
        app.backend = Some(spawn_backend(workspace_root.clone()));
        app.initialize_persistence(&workspace_root);
    }

    app
}

impl DesktopApp {
    fn drain_backend(&mut self) -> Task<Message> {
        let _ = update_model(&mut self.model, ModelMessage::Tick);

        let Some(backend) = self.backend.as_mut() else {
            return Task::none();
        };

        let mut events = Vec::new();
        while let Ok(event) = backend.event_rx.try_recv() {
            events.push(event);
        }

        let mut all_effects = Vec::new();
        for event in events {
            let effects = update_model(&mut self.model, ModelMessage::BackendEvent(event));
            all_effects.extend(effects);
        }

        self.run_effects(all_effects)
    }

    fn initialize_persistence(&mut self, workspace_root: &Path) {
        match thndrs_mem::init(workspace_root) {
            Ok((memory_store, sessions, _logs)) => {
                self.memory_store = Some(memory_store);
                self.session_manager = Some(sessions);
                self.refresh_sessions();
            }
            Err(error) => {
                self.memory_store = None;
                self.session_manager = None;
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to initialize memory: {error}")),
                );
            }
        }
    }

    fn refresh_sessions(&mut self) {
        if !self.model.session_search_query.trim().is_empty() {
            let query = self.model.session_search_query.clone();
            self.search_sessions(&query);
            return;
        }

        let Some(session_manager) = self.session_manager.as_ref() else {
            let _ = update_model(&mut self.model, ModelMessage::SessionsLoaded(Vec::new()));
            return;
        };

        match session_manager.list_sessions(50) {
            Ok(sessions) => {
                let summaries = map_sessions(sessions, self.model.active_session_id.as_deref());
                let _ = update_model(&mut self.model, ModelMessage::SessionsLoaded(summaries));
            }
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load sessions: {error}")),
                );
            }
        }
    }

    fn persist_user_prompt(&mut self, prompt: String, title: String) {
        let Some(session_manager) = self.session_manager.as_mut() else {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed("Session manager is unavailable".to_string()),
            );
            return;
        };

        let session_id = if let Some(session_id) = self.model.active_session_id.clone() {
            session_id
        } else {
            match session_manager.create_session(Some(title)) {
                Ok(session) => {
                    let _ = update_model(&mut self.model, ModelMessage::SessionActivated(session.id.clone()));
                    session.id
                }
                Err(error) => {
                    let _ = update_model(
                        &mut self.model,
                        ModelMessage::PersistenceFailed(format!("Failed to create session: {error}")),
                    );
                    return;
                }
            }
        };

        if let Err(error) = session_manager.add_message(&session_id, &thndrs_core::Message::user(prompt)) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to save user message: {error}")),
            );
            return;
        }

        if let Some(workspace_root) = self.model.workspace_root.as_ref() {
            let workspace_patch = serde_json::json!({
                "workspace_path": workspace_root.display().to_string(),
                "title_refined": false,
            });
            let _ = session_manager.merge_metadata(&session_id, &workspace_patch);
        }

        self.refresh_sessions();
    }

    fn persist_assistant_message(&mut self, content: String) {
        let Some(session_id) = self.model.active_session_id.clone() else {
            return;
        };
        let Some(session_manager) = self.session_manager.as_mut() else {
            return;
        };

        if let Err(error) = session_manager.add_message(&session_id, &thndrs_core::Message::assistant(content)) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to save assistant message: {error}")),
            );
            return;
        }

        self.refresh_sessions();
    }

    fn persist_tool_call(&mut self, name: &str, arguments: &str, result: Option<&str>, status: &str) {
        let Some(session_id) = self.model.active_session_id.clone() else {
            return;
        };
        let Some(session_manager) = self.session_manager.as_mut() else {
            return;
        };

        let arguments_json = serde_json::from_str::<serde_json::Value>(arguments)
            .unwrap_or_else(|_| serde_json::json!({ "raw": arguments }));

        if let Err(error) = session_manager.add_tool_call(&session_id, name, &arguments_json, result, status) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to save tool call: {error}")),
            );
        }
    }

    fn resume_session(&mut self, session_id: String) {
        let Some(session_manager) = self.session_manager.as_ref() else {
            return;
        };

        let session = match session_manager.get_session(&session_id) {
            Ok(Some(session)) => session,
            Ok(None) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Session not found: {session_id}")),
                );
                return;
            }
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load session: {error}")),
                );
                return;
            }
        };

        let messages = match session_manager.get_conversation_messages(&session_id) {
            Ok(messages) => messages,
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load session messages: {error}")),
                );
                return;
            }
        };

        let _ = update_model(
            &mut self.model,
            ModelMessage::SessionResumed { session_id, title: session.display_title(), messages },
        );
        self.refresh_sessions();
    }

    fn delete_session(&mut self, session_id: String) {
        let Some(session_manager) = self.session_manager.as_mut() else {
            return;
        };

        match session_manager.delete_session(&session_id) {
            Ok(true) => {
                let _ = update_model(&mut self.model, ModelMessage::SessionDeleted(session_id));
                self.refresh_sessions();
            }
            Ok(false) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed("Session was not deleted".to_string()),
                );
            }
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to delete session: {error}")),
                );
            }
        }
    }

    fn search_sessions(&mut self, query: &str) {
        if query.trim().is_empty() {
            self.refresh_sessions();
            return;
        }

        let Some(session_manager) = self.session_manager.as_ref() else {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed("Session manager is unavailable".to_string()),
            );
            return;
        };

        match session_manager.search_sessions(query, 50) {
            Ok(sessions) => {
                let summaries = map_sessions(sessions, self.model.active_session_id.as_deref());
                let _ = update_model(&mut self.model, ModelMessage::SessionsLoaded(summaries));
            }
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to search sessions: {error}")),
                );
            }
        }
    }

    fn persist_session_title(&mut self, session_id: &str, title: &str) {
        let Some(session_manager) = self.session_manager.as_mut() else {
            return;
        };

        if let Err(error) = session_manager.update_title(session_id, title) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to update session title: {error}")),
            );
            return;
        }

        let _ = session_manager.merge_metadata(session_id, &serde_json::json!({ "title_refined": true }));
        self.refresh_sessions();
    }

    fn persist_session_metadata(&mut self, session_id: &str, model: &str, usage: TokenUsageSummary) {
        let Some(session_manager) = self.session_manager.as_mut() else {
            return;
        };

        let session = match session_manager.get_session(session_id) {
            Ok(Some(session)) => session,
            Ok(None) => return,
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load session metadata: {error}")),
                );
                return;
            }
        };

        let (prompt_total, completion_total, token_total, response_count) = existing_usage_totals(&session);
        let workspace_path = self
            .model
            .workspace_root
            .as_ref()
            .map(|path| path.display().to_string());
        let patch = serde_json::json!({
            "workspace_path": workspace_path,
            "model": model,
            "usage": {
                "prompt_tokens": usage.prompt_tokens,
                "completion_tokens": usage.completion_tokens,
                "total_tokens": usage.total_tokens
            },
            "usage_totals": {
                "prompt_tokens": prompt_total.saturating_add(usage.prompt_tokens),
                "completion_tokens": completion_total.saturating_add(usage.completion_tokens),
                "total_tokens": token_total.saturating_add(usage.total_tokens),
                "responses": response_count.saturating_add(1)
            }
        });

        if let Err(error) = session_manager.merge_metadata(session_id, &patch) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to save session metadata: {error}")),
            );
            return;
        }

        self.refresh_sessions();
    }

    fn refine_session_title(&mut self, session_id: &str, prompt: &str, assistant_content: &str) {
        let Some(session_manager) = self.session_manager.as_ref() else {
            return;
        };

        let should_refine = match session_manager.get_session(session_id) {
            Ok(Some(session)) => {
                let refined = session
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("title_refined"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                !refined
            }
            Ok(None) => false,
            Err(_) => false,
        };
        if !should_refine {
            return;
        }

        let refined_title =
            llm_refined_title(prompt, assistant_content).unwrap_or_else(|| fallback_refined_title(prompt, assistant_content));

        if let Some(summary) = self.model.sessions.iter_mut().find(|summary| summary.id == session_id) {
            summary.title = refined_title.clone();
        }
        self.persist_session_title(session_id, &refined_title);
    }

    fn export_session(&mut self, session_id: &str) {
        let Some(session_manager) = self.session_manager.as_ref() else {
            return;
        };

        let session = match session_manager.get_session(session_id) {
            Ok(Some(session)) => session,
            Ok(None) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Session not found: {session_id}")),
                );
                return;
            }
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load session: {error}")),
                );
                return;
            }
        };
        let messages = match session_manager.get_messages(session_id) {
            Ok(messages) => messages,
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load session messages: {error}")),
                );
                return;
            }
        };
        let tool_calls = match session_manager.get_tool_calls(session_id) {
            Ok(calls) => calls,
            Err(error) => {
                let _ = update_model(
                    &mut self.model,
                    ModelMessage::PersistenceFailed(format!("Failed to load tool calls: {error}")),
                );
                return;
            }
        };

        let workspace_root = self
            .model
            .workspace_root
            .as_ref()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        let export_dir = workspace_root.join(".thunderus").join("exports");
        if let Err(error) = fs::create_dir_all(&export_dir) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to create export directory: {error}")),
            );
            return;
        }

        let slug = {
            let candidate = slugify(&session.display_title());
            if candidate.is_empty() { "session".to_string() } else { candidate }
        };
        let file_name = format!(
            "{}_{}_{}.md",
            session.updated_at.format("%Y%m%d_%H%M"),
            session.short_id(),
            slug
        );
        let export_path = export_dir.join(file_name);
        let markdown = render_session_markdown(&session, &messages, &tool_calls);

        if let Err(error) = fs::write(&export_path, markdown) {
            let _ = update_model(
                &mut self.model,
                ModelMessage::PersistenceFailed(format!("Failed to export session: {error}")),
            );
            return;
        }

        let _ = update_model(
            &mut self.model,
            ModelMessage::SessionExported(export_path.display().to_string()),
        );
    }

    fn extract_session_memories(&mut self, session_id: &str, prompt: &str, assistant_content: &str) {
        let Some(memory_store) = self.memory_store.as_mut() else {
            return;
        };

        let candidates = extract_memory_candidates(prompt, assistant_content);
        if candidates.is_empty() {
            return;
        }

        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!("Failed to create memory extraction runtime: {error}");
                return;
            }
        };

        let mut stored = 0u32;
        for candidate in candidates {
            let result = runtime.block_on(memory_store.store(
                candidate,
                MemoryKind::Context,
                Some(format!("session:{session_id}")),
                vec!["desktop", "session"],
            ));
            if result.is_ok() {
                stored = stored.saturating_add(1);
            }
        }

        if stored > 0 {
            self.model.status_text = Some(format!("Stored {stored} session memories"));
            if let Some(session_manager) = self.session_manager.as_mut() {
                let _ = session_manager.merge_metadata(
                    session_id,
                    &serde_json::json!({
                        "memory_extraction": {
                            "stored": stored,
                            "updated_at": Utc::now().to_rfc3339()
                        }
                    }),
                );
            }
        }
    }

    fn run_effects(&mut self, effects: Vec<Effect>) -> Task<Message> {
        let mut tasks = Vec::new();

        for effect in effects {
            match effect {
                Effect::DispatchPrompt(prompt) => {
                    tracing::info!("Dispatching prompt from GUI shell chars={}", prompt.chars().count());
                    if let Some(backend) = self.backend.as_ref()
                        && let Err(error) = backend.request_tx.send(prompt)
                    {
                        tracing::error!("Backend request send failed: {error}");
                        let _ = update_model(
                            &mut self.model,
                            ModelMessage::BackendEvent(BackendEvent::Error(format!(
                                "Backend request failed: {}",
                                error
                            ))),
                        );
                    }
                }
                Effect::OpenWorkspacePicker => tasks
                    .push(Task::perform(async { rfd::FileDialog::new().pick_folder() }, |path| {
                        Message::Model(ModelMessage::WorkspacePicked(path))
                    })),
                Effect::DeactivateWorkspace => {
                    self.backend = None;
                    self.memory_store = None;
                    self.session_manager = None;
                    let _ = update_model(&mut self.model, ModelMessage::SessionsLoaded(Vec::new()));
                }
                Effect::ActivateWorkspace(workspace_root) => {
                    tracing::info!("Activating workspace {}", workspace_root.display());
                    self.backend = Some(spawn_backend(workspace_root));
                    if let Some(workspace_root) = self.model.workspace_root.clone() {
                        self.initialize_persistence(&workspace_root);
                    }
                }
                Effect::LoadWorkspaceFiles(workspace_root) => tasks.push(Task::perform(
                    async move { collect_workspace_files(&workspace_root) },
                    |files| Message::Model(ModelMessage::WorkspaceFilesLoaded(files)),
                )),
                Effect::LoadSessions => self.refresh_sessions(),
                Effect::PersistUserPrompt { prompt, title } => self.persist_user_prompt(prompt, title),
                Effect::PersistAssistantMessage { content } => self.persist_assistant_message(content),
                Effect::PersistToolCall { name, arguments, result, status } => {
                    self.persist_tool_call(&name, &arguments, result.as_deref(), &status)
                }
                Effect::PersistSessionMetadata { session_id, model, usage } => {
                    self.persist_session_metadata(&session_id, &model, usage)
                }
                Effect::RefineSessionTitle { session_id, prompt, assistant_content } => {
                    self.refine_session_title(&session_id, &prompt, &assistant_content)
                }
                Effect::ExtractSessionMemories { session_id, prompt, assistant_content } => {
                    self.extract_session_memories(&session_id, &prompt, &assistant_content)
                }
                Effect::SearchSessions(query) => self.search_sessions(&query),
                Effect::ExportSession(session_id) => self.export_session(&session_id),
                Effect::ResumeSession(session_id) => self.resume_session(session_id),
                Effect::DeleteSession(session_id) => self.delete_session(session_id),
                Effect::PersistState => {
                    if let Err(error) = self.store.save(&self.persisted_state()) {
                        tracing::error!("Failed to persist desktop state: {error}");
                        self.model.error_text = Some(format!("Failed to persist desktop state: {error}"));
                    }
                }
            }
        }

        if tasks.is_empty() { Task::none() } else { Task::batch(tasks) }
    }

    fn persisted_state(&self) -> PersistedUiState {
        PersistedUiState {
            workspace_root: self.model.workspace_root.clone(),
            recent_workspaces: self.model.recent_workspaces.clone(),
            composer_text: self.model.composer_text.clone(),
            last_model: self.model.last_model.clone(),
        }
    }
}

fn map_sessions(sessions: Vec<Session>, active_session_id: Option<&str>) -> Vec<SessionSummary> {
    let today = Utc::now().date_naive();

    sessions
        .into_iter()
        .map(|session| {
            let delta_days = (today - session.updated_at.date_naive()).num_days();
            let age_group = if delta_days <= 0 {
                SessionAgeGroup::Today
            } else if delta_days == 1 {
                SessionAgeGroup::Yesterday
            } else {
                SessionAgeGroup::Older
            };
            let model = session
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("model"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string);
            let total_tokens = session
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("usage_totals"))
                .and_then(|value| value.get("total_tokens"))
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok());
            let workspace = session
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("workspace_path"))
                .and_then(serde_json::Value::as_str)
                .map(short_workspace_label);
            SessionSummary {
                id: session.id.clone(),
                title: session.display_title(),
                message_count: session.message_count,
                updated_at_label: session.updated_at.format("%Y-%m-%d %H:%M").to_string(),
                model,
                total_tokens,
                workspace,
                age_group,
                is_active: active_session_id == Some(session.id.as_str()),
            }
        })
        .collect()
}

fn existing_usage_totals(session: &Session) -> (u32, u32, u32, u32) {
    let prompt = session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("usage_totals"))
        .and_then(|value| value.get("prompt_tokens"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let completion = session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("usage_totals"))
        .and_then(|value| value.get("completion_tokens"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let total = session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("usage_totals"))
        .and_then(|value| value.get("total_tokens"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let responses = session
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("usage_totals"))
        .and_then(|value| value.get("responses"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    (prompt, completion, total, responses)
}

fn short_workspace_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|segment| segment.to_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in input.chars() {
        let normalized = if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' };
        if normalized == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
            out.push('-');
            continue;
        }
        previous_dash = false;
        out.push(normalized);
    }
    out.trim_matches('-').to_string()
}

fn truncate_for_summary(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut out = String::new();
    for ch in input.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn normalize_title(raw: &str) -> Option<String> {
    let line = raw
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if line.is_empty() { None } else { Some(truncate_for_summary(line, 58)) }
}

fn llm_refined_title(prompt: &str, assistant_content: &str) -> Option<String> {
    let config = Config::load_default().ok()?;
    let provider = create_provider(&config.default_provider, &config).ok()?;
    let summary_prompt = format!(
        "User request:\n{}\n\nAssistant response:\n{}\n\nReturn one concise title (max 9 words).",
        truncate_for_summary(prompt, 500),
        truncate_for_summary(assistant_content, 800)
    );
    let messages = vec![
        thndrs_core::Message::system(
            "Generate a short conversation title. Output only the title text with no markdown or quotes.",
        ),
        thndrs_core::Message::user(summary_prompt),
    ];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let completion = runtime.block_on(async {
        match tokio::time::timeout(Duration::from_secs(4), provider.complete(&messages)).await {
            Ok(result) => result.ok(),
            Err(_) => None,
        }
    })?;

    normalize_title(&completion.content)
}

fn fallback_refined_title(prompt: &str, assistant_content: &str) -> String {
    if let Some(line) = normalize_title(prompt) {
        return line;
    }

    normalize_title(assistant_content).unwrap_or_else(|| "Session".to_string())
}

fn extract_memory_candidates(prompt: &str, assistant_content: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let prompt_line = prompt.lines().next().unwrap_or_default().trim();
    if !prompt_line.is_empty() {
        candidates.push(format!("Active task: {}", truncate_for_summary(prompt_line, 220)));
    }

    for line in assistant_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("```") || trimmed.len() < 24 {
            continue;
        }

        let normalized = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim_start_matches("1. ")
            .trim();
        if normalized.len() < 24 {
            continue;
        }

        candidates.push(truncate_for_summary(normalized, 240));
        if candidates.len() >= 4 {
            break;
        }
    }

    candidates
}

fn render_session_markdown(
    session: &Session, messages: &[thndrs_mem::session::SessionMessage],
    tool_calls: &[thndrs_mem::session::SessionToolCall],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", session.display_title()));
    out.push_str(&format!("- Session ID: `{}`\n", session.id));
    out.push_str(&format!("- Created: {}\n", session.created_at.to_rfc3339()));
    out.push_str(&format!("- Updated: {}\n", session.updated_at.to_rfc3339()));
    out.push_str(&format!("- Messages: {}\n\n", session.message_count));

    if let Some(metadata) = session.metadata.as_ref()
        && let Ok(pretty) = serde_json::to_string_pretty(metadata)
    {
        out.push_str("## Metadata\n\n");
        out.push_str("```json\n");
        out.push_str(&pretty);
        out.push_str("\n```\n\n");
    }

    out.push_str("## Transcript\n\n");
    for message in messages {
        let role = match message.role {
            thndrs_core::Role::System => "system",
            thndrs_core::Role::User => "user",
            thndrs_core::Role::Assistant => "assistant",
            thndrs_core::Role::Tool => "tool",
        };
        out.push_str(&format!("### {role}\n\n"));
        out.push_str(&message.content);
        out.push_str("\n\n");
    }

    if !tool_calls.is_empty() {
        out.push_str("## Tool Calls\n\n");
        for call in tool_calls {
            out.push_str(&format!("### {} ({})\n\n", call.tool_name, call.status));
            if let Some(arguments) = call.arguments.as_ref()
                && let Ok(pretty) = serde_json::to_string_pretty(arguments)
            {
                out.push_str("```json\n");
                out.push_str(&pretty);
                out.push_str("\n```\n\n");
            }
            if let Some(result) = call.result.as_ref() {
                out.push_str("```text\n");
                out.push_str(result);
                out.push_str("\n```\n\n");
            }
        }
    }

    out
}

pub fn update(app: &mut DesktopApp, message: Message) -> Task<Message> {
    match message {
        Message::Model(model_message) => {
            let effects = update_model(&mut app.model, model_message);
            app.run_effects(effects)
        }
        Message::PollBackend(_instant) => app.drain_backend(),
    }
}

pub fn title(_: &DesktopApp) -> String {
    "Thunderus - Desktop".to_string()
}

pub fn theme(_: &DesktopApp) -> Theme {
    Theme::custom(
        "Oxocarbon Dark".to_string(),
        Palette {
            background: color_hex("#161616"),
            text: color_hex("#f4f4f4"),
            primary: color_hex("#33b1ff"),
            success: color_hex("#42be65"),
            warning: color_hex("#f1c21b"),
            danger: color_hex("#fa4d56"),
        },
    )
}

pub fn subscription(app: &DesktopApp) -> Subscription<Message> {
    match &app.backend {
        Some(_) => iced::time::every(Duration::from_millis(super::POLL_INTERVAL_MS)).map(Message::PollBackend),
        None => Subscription::none(),
    }
}

pub fn window_settings() -> window::Settings {
    window::Settings {
        size: iced::Size::new(super::WINDOW_WIDTH, super::WINDOW_HEIGHT),
        position: window::Position::Centered,
        ..window::Settings::default()
    }
}
