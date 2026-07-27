//! Agent event lifecycle, cancellation, and session persistence.

use super::*;
use crate::artifacts;
use crate::mcp;

/// Process an [`AgentEvent`] and mutate `app` accordingly.
pub fn handle_agent_event(app: &mut App, event: AgentEvent) -> Option<Msg> {
    match event {
        AgentEvent::Started => {
            // A `Started` event can still be queued when the user stops the
            // run. Do not let it revive a run that is already winding down.
            if app.run_state != RunState::Stopping {
                app.stopping_deadline = None;
                app.run_state = RunState::Working;
            }
            None
        }
        AgentEvent::Status(text) => {
            if app.verbose || !is_verbose_status(&text) {
                app.transcript.push(Entry::Status { text });
            }
            None
        }
        AgentEvent::Usage { input_tokens, output_tokens } => {
            app.session_tokens_in = app.session_tokens_in.saturating_add(input_tokens);
            app.session_tokens_out = app.session_tokens_out.saturating_add(output_tokens);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_usage(input_tokens, output_tokens);
            }
            None
        }
        AgentEvent::RequestAccounting(accounting) => {
            app.last_request_accounting = Some(accounting.as_ref().clone());
            if let Some(usage) = &accounting.provider_usage {
                if let Some(input_tokens) = usage.components.input_tokens {
                    app.session_tokens_in = app.session_tokens_in.saturating_add(input_tokens);
                }
                if let Some(output_tokens) = usage.components.output_tokens {
                    app.session_tokens_out = app.session_tokens_out.saturating_add(output_tokens);
                }
            }
            if let Some(writer) = app.session_writer.as_mut() {
                let _ = writer.append_request_accounting(&accounting.turn_id, &accounting);
            }
            None
        }
        AgentEvent::AssistantDelta(delta) => {
            app.ttft.stop_on_semantic_output();
            finalize_reasoning(app);
            if let Some(Entry::Agent { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Agent { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            app.ttft.stop_on_semantic_output();
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript.push(Entry::Reasoning { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ToolStarted { id, name, arguments } => {
            app.ttft.stop_on_semantic_output();
            finalize_streaming(app);
            app.transcript.push(Entry::Tool {
                name: format!("{name}#{id}"),
                arguments: arguments.clone(),
                status: ToolStatus::Running,
                output: Vec::new(),
            });
            if let Some(ref mut writer) = app.session_writer {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_tool_started(&turn_id, &id, &name, &arguments);
            }
            None
        }
        AgentEvent::ToolFinished { id, output, status, write_result, shell_result } => {
            app.ttft.stop_on_semantic_output();
            finalize_streaming(app);
            let artifact = finish_tool_output(app, &id, status, &output);
            persist_last_entry_with_artifact(app, artifact);

            if let Some(result) = write_result
                && let Some(ref mut writer) = app.session_writer
            {
                let turn_id = format!("turn_{}", app.turn_count);
                let _ = writer.append_file_write(&turn_id, &result, status);
            }

            if let Some(result) = shell_result {
                if result.kind == tools::shell::ProcessKind::Background {
                    let process_id = result.process_id.unwrap_or_else(|| {
                        // Keep synthetic adapter/test events visible while the
                        // live shell path uses the id assigned before spawn
                        // returns.
                        app.process_registry.register(
                            result.command.clone(),
                            result.cwd.clone(),
                            result.kind,
                            CancelToken::new(),
                        )
                    });
                    app.process_registry.announce(process_id);
                    app.transcript.push(Entry::Status {
                        text: format!(
                            "background process [{process_id}] started: {}",
                            result.command.join(" ")
                        ),
                    });
                }

                if let Some(ref mut writer) = app.session_writer {
                    let turn_id = format!("turn_{}", app.turn_count);
                    let _ = writer.append_shell_exec(&turn_id, &result);
                }
            }
            app.refresh_git_status();
            None
        }
        AgentEvent::StateProjectionDecision { id, decision } => {
            app.tool_projection_decisions.insert(id, decision);
            None
        }
        AgentEvent::ModelMetadataLoaded(items) => {
            app.model_picker_items = items
                .into_iter()
                .map(|(label, detail)| PickerItem::new(label, detail))
                .collect();
            None
        }
        AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => {
            discard_retry_output(app);
            app.run_state = RunState::Working;
            app.transcript.push(Entry::Status {
                text: format!(
                    "retrying provider request ({attempt}/{max_attempts}) in {:.1}s after: {error}",
                    delay_ms as f64 / 1000.0
                ),
            });
            None
        }
        AgentEvent::PermissionRequest(permission) => {
            finalize_streaming(app);
            if app.pending_permission.is_some() {
                let _ = permission.cancel();
                app.transcript.push(Entry::Error {
                    text: "acp: received a second permission request while one is pending; cancelled it".to_string(),
                });
                return None;
            }
            let turn_id = format!("turn_{}", app.turn_count);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_permission_request(&turn_id, &permission);
            }
            app.pending_permission = Some(permission);
            app.context_ledger = None;
            None
        }
        AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            let turn_id = format!("turn_{}", app.turn_count);
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_permission_outcome(&turn_id, &tool_call_id, &outcome);
            }
            app.context_ledger = None;
            None
        }
        AgentEvent::AcpSession(metadata) => {
            if let Some(ref mut writer) = app.session_writer {
                let _ = writer.append_acp_session(&metadata);
            }
            None
        }
        AgentEvent::Finished => {
            app.stopping_deadline = None;
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.run_state = RunState::Idle;
            app.last_input = None;
            app.refresh_git_status();
            match context::finish_manual_compaction(app) {
                None => persist_final_response(app),
                Some(None) => {}
                Some(Some(restart)) => return Some(restart),
            }
            if app.queued_followups.is_empty() {
                None
            } else {
                let next = app.queued_followups.remove(0);
                submit_user_turn(app, next)
            }
        }
        AgentEvent::Failed(msg) => {
            app.stopping_deadline = None;
            let manual_compaction = context::restore_failed_manual_compaction(app);
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.transcript.push(Entry::Error { text: msg.clone() });
            app.run_state = RunState::Error(msg);
            if !manual_compaction && let Some(input) = app.last_input.take() {
                app.input.set_text(&input);
            }
            persist_last_entry(app);
            open_credential_recovery_after_rejection(app);
            persist_last_entry(app);
            app.refresh_git_status();
            None
        }
        AgentEvent::Cancelled => {
            app.stopping_deadline = None;
            context::restore_failed_manual_compaction(app);
            app.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            cancel_running_tools(app);
            if app.run_state == RunState::Working {
                app.transcript.push(Entry::Status { text: String::from("cancelled") });
            }
            app.run_state = RunState::Idle;
            app.last_input = None;
            app.queued_steering.clear();
            persist_last_entry(app);
            app.refresh_git_status();
            None
        }
    }
}

/// Drain completed application-owned background processes into the transcript
/// and append their terminal shell lifecycle records.
pub fn drain_background_processes(app: &mut App) {
    let results = app.process_registry.drain_completed();
    record_background_results(app, results);
}

/// Record terminal results returned while the application is shutting down or
/// while a normal UI tick drains the process registry.
pub fn record_background_results(app: &mut App, results: Vec<tools::shell::ProcessResult>) {
    for result in results {
        let process_id = result.process_id.map_or_else(|| String::from("?"), |id| id.to_string());
        let mut lines = result.to_output_lines();
        if let Some(first) = lines.first_mut() {
            *first = format!("background process [{process_id}] {first}");
        }
        app.transcript.push(Entry::Status { text: lines.join("\n") });
        if let Some(writer) = app.session_writer.as_mut() {
            let turn_id = format!("turn_{}", app.turn_count);
            let _ = writer.append_shell_exec(&turn_id, &result);
        }
    }
}

pub fn handle_permission_key(app: &mut App, key: KeyEvent) -> Option<Msg> {
    match key.code {
        KeyCode::Up => {
            if let Some(permission) = app.pending_permission.as_mut() {
                permission.move_up();
            }
            None
        }
        KeyCode::Down => {
            if let Some(permission) = app.pending_permission.as_mut() {
                permission.move_down();
            }
            None
        }
        KeyCode::Enter => {
            if let Some(permission) = app.pending_permission.take()
                && let Some(PermissionDecision::Selected(option_id)) = permission.select()
            {
                app.transcript.push(Entry::Status {
                    text: format!("acp permission {}: selected {option_id}", permission.tool_call_id),
                });
            }
            app.context_ledger = None;
            None
        }
        KeyCode::Esc => {
            if let Some(permission) = app.pending_permission.take() {
                let _ = permission.cancel();
                app.transcript
                    .push(Entry::Status { text: format!("acp permission {}: cancelled", permission.tool_call_id) });
            }
            app.context_ledger = None;
            None
        }
        _ => None,
    }
}

/// Cancel an active stream by marking all streaming entries complete,
/// recording a cancelled status entry, and transitioning to `Stopping`.
///
/// The app loop observes the transition out of `Working` and drops the
/// background receiver, which stops the agent thread on its next failed send.
/// When the `Cancelled` agent event arrives (or the channel disconnects), the
/// state transitions from `Stopping` to `Idle`. A short deadline also bounds
/// this state when a worker is blocked in a non-cancellable operation.
pub fn cancel_stream(app: &mut App) {
    cancel_pending_permission(app);
    finalize_streaming(app);
    app.transcript.push(Entry::Status { text: String::from("cancelled") });
    app.run_state = RunState::Stopping;
    app.stopping_deadline = Some(app.ui_tick.wrapping_add(stopping_grace_ticks(app)));
    persist_last_entry(app);
}

/// Complete a stopped run once the worker has had a short chance to acknowledge
/// cancellation.
///
/// The direct loop drops the receiver after this transitions the app to idle,
/// so a worker that remains blocked cannot later mutate a subsequent run.
pub fn finish_stopping_if_due(app: &mut App) {
    let Some(deadline) = app.stopping_deadline else {
        return;
    };

    if app.run_state != RunState::Stopping {
        app.stopping_deadline = None;
        return;
    }

    if now_or_after_deadline(app.ui_tick, deadline) {
        handle_agent_event(app, AgentEvent::Cancelled);
    }
}

/// Translate the fixed stop grace period to the configured tick cadence.
pub fn stopping_grace_ticks(app: &App) -> u64 {
    let tick_ms = app.cli.tick_rate_ms.max(1);
    STOPPING_GRACE_MS / tick_ms + u64::from(!STOPPING_GRACE_MS.is_multiple_of(tick_ms))
}

pub fn cancel_pending_permission(app: &mut App) {
    if let Some(permission) = app.pending_permission.take() {
        let _ = permission.cancel();
        app.context_ledger = None;
    }
}

pub fn remember_input(app: &mut App, text: &str) {
    if text.is_empty() || app.input_history.last().is_some_and(|last| last == text) {
        return;
    }
    let overflow = app
        .input_history
        .len()
        .saturating_add(1)
        .saturating_sub(INPUT_HISTORY_LIMIT);
    if overflow > 0 {
        app.input_history.drain(..overflow);
    }
    app.input_history.push(text.to_string());
    let _ = app.input_history_store.append(&app.session_id, text);
}

pub fn recall_older_input(app: &mut App) {
    if app.input_history.is_empty() {
        return;
    }

    let next = match app.history_cursor {
        Some(0) => 0,
        Some(index) => index.saturating_sub(1),
        None => {
            app.history_draft = app.input.text();
            app.input_history.len() - 1
        }
    };
    app.history_cursor = Some(next);
    app.input.set_text(&app.input_history[next]);
}

pub fn recall_newer_input(app: &mut App) {
    let Some(index) = app.history_cursor else {
        return;
    };

    if index + 1 < app.input_history.len() {
        let next = index + 1;
        app.history_cursor = Some(next);
        app.input.set_text(&app.input_history[next]);
    } else {
        app.history_cursor = None;
        app.input.set_text(&app.history_draft);
        app.history_draft.clear();
    }
}

pub fn exit_history_navigation(app: &mut App) {
    if app.history_cursor.is_some() {
        app.history_cursor = None;
        app.history_draft.clear();
    }
}

/// Persist the last, finalized transcript entry to the session file, if a writer exists.
pub fn persist_last_entry(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.last()
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

/// Persist the last tool entry with its bounded artifact metadata.
fn persist_last_entry_with_artifact(app: &mut App, artifact: Option<artifacts::ArtifactMetadata>) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.last()
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry_with_artifact(entry, &turn_id, artifact);
    }
}

/// Persist the final model response even if provider status rows were appended
/// after the last assistant/reasoning delta.
pub fn persist_final_response(app: &mut App) {
    if let Some(ref mut writer) = app.session_writer
        && let Some(entry) = app.transcript.iter().rev().find(|entry| {
            matches!(
                entry,
                Entry::Agent { streaming: false, .. } | Entry::Reasoning { streaming: false, .. }
            )
        })
    {
        let turn_id = format!("turn_{}", app.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

/// Whether `ui_tick` is at or past `deadline`, accounting for wrap-around.
///
/// If `deadline` has wrapped (e.g. `ui_tick` is small and `deadline` is near
/// [`u64::MAX`]), we treat the deadline as already passed.
///
/// A wrap is so rare that expiring early is the safe choice.
pub fn now_or_after_deadline(ui_tick: u64, deadline: u64) -> bool {
    if deadline >= ui_tick { deadline.wrapping_sub(ui_tick) > u64::MAX / 2 } else { true }
}

/// Mark all streaming `Assistant` and `Reasoning` entries as complete.
pub fn finalize_streaming(app: &mut App) {
    for entry in &mut app.transcript {
        match entry {
            Entry::Agent { streaming, .. } => *streaming = false,
            Entry::Reasoning { streaming, .. } => *streaming = false,
            _ => {}
        }
    }
}

/// Mark any running tool entries as cancelled.
///
/// Called when the active run is interrupted so that the renderer can show a
/// distinct cancelled-tool row instead of leaving the tool in a running state.
pub fn cancel_running_tools(app: &mut App) {
    for entry in &mut app.transcript {
        if let Entry::Tool { status, .. } = entry
            && *status == ToolStatus::Running
        {
            *status = ToolStatus::Cancelled;
        }
    }
}

/// Mark active reasoning entries complete when the model moves on to visible
/// assistant text or a tool call.
pub fn finalize_reasoning(app: &mut App) {
    for entry in &mut app.transcript {
        if let Entry::Reasoning { streaming, .. } = entry {
            *streaming = false;
        }
    }
}

/// Remove partial assistant/reasoning output from a provider attempt that is
/// about to be retried. Tool entries and prior completed transcript context are
/// left intact.
pub fn discard_retry_output(app: &mut App) {
    while matches!(
        app.transcript.last(),
        Some(Entry::Agent { .. } | Entry::Reasoning { .. })
    ) {
        app.transcript.pop();
    }
}

pub fn load_mcp_config_audit(workspace: &Path) -> (Vec<session::SessionConfigFile>, Vec<String>) {
    let env_vars: Vec<(String, String)> = std::env::vars().collect();
    match mcp::config::load_effective_mcp(workspace, &env_vars) {
        Ok(effective) => {
            let files = effective
                .layers
                .iter()
                .filter_map(|layer| {
                    let path = layer.display_path.as_ref()?;
                    Some(session::SessionConfigFile {
                        path: path.clone(),
                        source: layer.source.as_str().to_string(),
                        sha256: layer.hash.clone().unwrap_or_default(),
                    })
                })
                .collect();
            (files, effective.diagnostics)
        }
        Err(err) => (Vec::new(), vec![format!("failed to load MCP config: {err}")]),
    }
}

pub fn refresh_mcp_config_audit(app: &mut App, turn_id: &str) {
    let (current_files, current_diagnostics) = load_mcp_config_audit(&app.cwd);
    if app.mcp_config_files == current_files && app.mcp_config_diagnostics == current_diagnostics {
        return;
    }

    let previous_files = std::mem::replace(&mut app.mcp_config_files, current_files.clone());
    app.mcp_config_diagnostics = current_diagnostics.clone();
    app.transcript
        .push(Entry::Status { text: mcp_config_changed_status(&previous_files, &current_files) });
    if let Some(ref mut writer) = app.session_writer {
        let _ = writer.append_mcp_config_changed(turn_id, previous_files, current_files, current_diagnostics);
    }
}

fn finish_tool_output(
    app: &mut App, id: &str, status: ToolStatus, output: &[String],
) -> Option<artifacts::ArtifactMetadata> {
    let artifact = app
        .artifact_store()
        .and_then(|store| store.create_tool_evidence(&format!("tool:{id}"), output).ok());
    let safe_output = artifact.as_ref().map_or_else(
        || artifacts::bounded_redacted_lines(output, artifacts::DEFAULT_MAX_ARTIFACT_BYTES),
        |write| write.bounded_lines.clone(),
    );
    if let Some(write) = &artifact {
        app.tool_artifacts.insert(id.to_string(), write.metadata.handle.clone());
    }
    for entry in app.transcript.iter_mut().rev() {
        if let Entry::Tool { name, output: out, status: entry_status, .. } = entry
            && name.ends_with(&format!("#{id}"))
        {
            *out = safe_output;
            *entry_status = status;
            break;
        }
    }
    artifact.map(|write| write.metadata)
}

fn mcp_config_changed_status(
    previous_files: &[session::SessionConfigFile], current_files: &[session::SessionConfigFile],
) -> String {
    let previous = config_file_hash_summary(previous_files);
    let current = config_file_hash_summary(current_files);
    format!("MCP config changed: {previous} -> {current}")
}

fn config_file_hash_summary(files: &[session::SessionConfigFile]) -> String {
    if files.is_empty() {
        return "none".to_string();
    }

    files
        .iter()
        .map(|file| format!("{}:{}:{}", file.source, file.path, file.sha256))
        .collect::<Vec<_>>()
        .join(", ")
}

fn open_credential_recovery_after_rejection(app: &mut App) {
    let RunState::Error(message) = &app.run_state else {
        return;
    };
    if !is_credential_rejection(message) || crate::acp::config::parse_model_id(&app.model).is_some() {
        return;
    }

    let provider = super::onboarding::provider_for_model(&app.model);
    if let Some(env_var) = active_environment_credential(provider, &app.cwd) {
        app.transcript.push(Entry::Status {
            text: format!(
                "{env_var} takes precedence over stored credentials; replace or unset it, then use `/login {}` if needed",
                provider.label()
            ),
        });
        return;
    }

    app.first_run_recovery = Some(FirstRunRecovery::login(provider));
    app.transcript.push(Entry::Status {
        text: format!(
            "credential rejected; opened `/login {}` recovery while keeping the prompt draft",
            provider.label()
        ),
    });
}

fn is_credential_rejection(message: &str) -> bool {
    let message = message.trim_start().to_ascii_lowercase();
    message.starts_with("authentication failed (http 401)")
        || message.starts_with("authentication failed (http 403)")
        || message.starts_with("authentication failed:")
        || message.starts_with("umans authentication failed (http 401)")
        || message.starts_with("umans authentication failed (http 403)")
        || message.starts_with("umans authentication failed:")
}

fn active_environment_credential(provider: SetupProviderArg, workspace: &std::path::Path) -> Option<&'static str> {
    match provider {
        SetupProviderArg::ChatgptCodex
            if std::env::var(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV).is_ok_and(|value| !value.trim().is_empty()) =>
        {
            Some(auth::CHATGPT_CODEX_ACCESS_TOKEN_ENV)
        }
        _ => provider.api_key_env_var().filter(|env_var| {
            matches!(
                auth::credential_source(env_var, workspace),
                Some(auth::CredentialSource::Environment)
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::is_credential_rejection;

    #[test]
    fn credential_rejection_detection_ignores_auth_words_in_server_errors() {
        assert!(is_credential_rejection("authentication failed (HTTP 401)"));
        assert!(is_credential_rejection("Umans authentication failed: invalid token"));
        assert!(!is_credential_rejection(
            "server error (HTTP 500): upstream authentication failed while validating its own service"
        ));
    }
}
