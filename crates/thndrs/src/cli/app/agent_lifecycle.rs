//! Agent event lifecycle, cancellation, and session persistence.

use super::*;
use crate::artifacts;
use crate::mcp;

fn persist_active_context_snapshot(app: &mut App, state: session::ContextSnapshotState) {
    let Some(accounting) = app.session.active_request_accounting.take() else { return };
    app.runtime.request_observation.finish_request(&accounting);
    persist_context_snapshot(app, &accounting, state, true);
}

fn update_context_usage(app: &mut App, accounting: &thndrs_agent::ProviderRequestAccounting) {
    if let Some(used) = observed_context_usage(accounting)
        && let Some(ledger) = app.transcript.context_ledger.as_mut()
    {
        ledger.budget.used = used;
    }
}

pub(super) fn observed_context_usage(accounting: &thndrs_agent::ProviderRequestAccounting) -> Option<u64> {
    accounting
        .provider_usage
        .as_ref()
        .and_then(|usage| usage.inclusive_input_tokens.value)
        .or(accounting.estimated_input_tokens.value)
}

fn disable_context_content_capture(app: &mut App, reason: &str) {
    let policy = session::ContextCapturePolicy::metadata_only();
    if let Some(writer) = app.session.writer.as_mut() {
        let _ = writer.append_context_capture_policy(&policy);
    }
    app.session.context_capture_policy = policy;
    app.session.config_diagnostics.push(reason.to_string());
}

fn capture_request_content(app: &mut App, accounting: &thndrs_agent::ProviderRequestAccounting) {
    match app.session.context_capture_policy.capture_request(accounting) {
        Ok(Some(capture)) => {
            if let Some(writer) = app.session.writer.as_mut()
                && writer.append_captured_request(capture).is_err()
            {
                disable_context_content_capture(
                    app,
                    "context content capture stopped because the session write failed",
                );
            }
        }
        Ok(None) => {}
        Err(_) => disable_context_content_capture(
            app,
            "context content capture stopped because sanitization or size validation failed",
        ),
    }
}

fn persist_context_snapshot(
    app: &mut App, accounting: &thndrs_agent::ProviderRequestAccounting, state: session::ContextSnapshotState,
    emit_context_event: bool,
) {
    let Some(ledger) = app.transcript.context_ledger.as_ref() else { return };
    let transcript_entries = if app.runtime.request_observation.matches(accounting) {
        app.transcript
            .entries
            .blocks()
            .skip(app.runtime.request_observation.transcript_start)
            .map(|block| block.id.to_string())
            .collect()
    } else {
        Vec::new()
    };
    let snapshot = session::ContextSnapshot {
        snapshot_version: 1,
        session_id: app.session.id.clone(),
        request_id: accounting.request_id.clone(),
        turn_id: accounting.turn_id.clone(),
        attempt: accounting.attempt,
        provider: accounting.provider.clone(),
        model: accounting.model.clone(),
        route: format!("{}/{}", accounting.provider, accounting.model),
        state,
        ledger: session::ContextLedgerMeta::from(ledger),
        serialized_bytes: Some(accounting.serialized_bytes.value),
        estimated_input_tokens: accounting.estimated_input_tokens.value,
        transformations: accounting.reduction_receipts(),
        provider_usage: accounting.provider_usage.clone(),
        duration_ms: app.runtime.request_observation.duration_ms(accounting),
        time_to_first_token_ms: app.runtime.request_observation.time_to_first_token_ms(accounting),
        tool_count: accounting.tool_count,
        tool_duration_ms: app.runtime.request_observation.tool_duration_ms(accounting),
        transcript_entries,
    };
    let persisted = match app.session.writer.as_mut() {
        Some(writer) => match writer.append_context_snapshot(snapshot.clone()) {
            Ok(()) => true,
            Err(error) => {
                app.transcript
                    .entries
                    .push(Entry::Error { text: format!("failed to record context snapshot: {error}") });
                false
            }
        },
        None => true,
    };
    if !persisted {
        return;
    }
    if emit_context_event && let Some(text) = session::ContextHistory::live_reduction_event(&snapshot) {
        app.transcript.entries.push_context_event(
            format!("context:reduction:live:{}:{}", snapshot.request_id, snapshot.attempt),
            text,
        );
    }
    app.transcript.context_history.record_snapshot(snapshot);
}

fn persist_completed_observation(app: &mut App) {
    let Some(accounting) = app.session.last_request_accounting.clone() else { return };
    if app.runtime.request_observation.matches(&accounting) {
        persist_context_snapshot(app, &accounting, session::ContextSnapshotState::Completed, false);
    }
}

/// Process an [`AgentEvent`] and mutate `app` accordingly.
pub fn handle_agent_event(app: &mut App, event: AgentEvent) -> Option<Msg> {
    match event {
        AgentEvent::Started => {
            // A `Started` event can still be queued when the user stops the
            // run. Do not let it revive a run that is already winding down.
            if app.runtime.run_state != RunState::Stopping {
                app.runtime.turn_timing.ensure_started();
                app.runtime.stopping_deadline = None;
                app.runtime.run_state = RunState::Working;
            }
            None
        }
        AgentEvent::Status(text) => {
            if app.runtime.verbose || !is_verbose_status(&text) {
                app.transcript.entries.push(Entry::Status { text });
            }
            None
        }
        AgentEvent::Usage { input_tokens, output_tokens } => {
            app.runtime.session_tokens_in = app.runtime.session_tokens_in.saturating_add(input_tokens);
            app.runtime.session_tokens_out = app.runtime.session_tokens_out.saturating_add(output_tokens);
            app.runtime
                .session_usage
                .record(Some(&thndrs_agent::ProviderUsageComponents::new(
                    input_tokens,
                    output_tokens,
                )));
            if let Some(ref mut writer) = app.session.writer {
                let _ = writer.append_usage(input_tokens, output_tokens);
            }
            None
        }
        AgentEvent::CodexUsage(usage) => {
            app.runtime.codex_usage = Some(usage);
            None
        }
        AgentEvent::RequestStarted(accounting) => {
            app.runtime.provider_retry = None;
            app.runtime.turn_timing.ensure_started();
            app.runtime
                .request_observation
                .start(&accounting, app.transcript.entries.len());
            app.session.active_request_accounting = Some(accounting.as_ref().clone());
            update_context_usage(app, &accounting);
            capture_request_content(app, &accounting);
            persist_context_snapshot(app, &accounting, session::ContextSnapshotState::Dispatched, false);
            None
        }
        AgentEvent::RequestAccounting(accounting) => {
            app.runtime.request_observation.finish_request(&accounting);
            update_context_usage(app, &accounting);
            persist_context_snapshot(app, &accounting, session::ContextSnapshotState::Completed, true);
            app.session.active_request_accounting = None;
            app.session.last_request_accounting = Some(accounting.as_ref().clone());
            app.runtime
                .session_usage
                .record(accounting.provider_usage.as_ref().map(|usage| &usage.components));
            if let Some(usage) = &accounting.provider_usage {
                if let Some(input_tokens) = usage.components.input_tokens {
                    app.runtime.session_tokens_in = app.runtime.session_tokens_in.saturating_add(input_tokens);
                }
                if let Some(output_tokens) = usage.components.output_tokens {
                    app.runtime.session_tokens_out = app.runtime.session_tokens_out.saturating_add(output_tokens);
                }
            }
            if let Some(writer) = app.session.writer.as_mut() {
                let _ = writer.append_request_accounting(&accounting.turn_id, &accounting);
            }
            None
        }
        AgentEvent::AssistantDelta(delta) => {
            app.runtime.provider_retry = None;
            app.runtime.ttft.stop_on_semantic_output();
            app.runtime.request_observation.stop_on_semantic_output();
            finalize_reasoning(app);
            if let Some(Entry::Agent { text, streaming: true }) = app.transcript.entries.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript
                    .entries
                    .push(Entry::Agent { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ReasoningDelta(delta) => {
            app.runtime.provider_retry = None;
            app.runtime.ttft.stop_on_semantic_output();
            app.runtime.request_observation.stop_on_semantic_output();
            if let Some(Entry::Reasoning { text, streaming: true }) = app.transcript.entries.last_mut() {
                text.push_str(&delta);
            } else {
                app.transcript
                    .entries
                    .push(Entry::Reasoning { text: delta, streaming: true });
            }
            None
        }
        AgentEvent::ToolStarted { id, name, arguments } => {
            app.runtime.provider_retry = None;
            app.runtime.turn_timing.ensure_started();
            app.runtime.request_observation.start_tool(&id);
            record_tool_started(app, &id, &name, &arguments);
            persist_completed_observation(app);
            None
        }
        AgentEvent::ToolFinished { id, output, status, write_result, shell_result } => {
            app.runtime.provider_retry = None;
            app.runtime.ttft.stop_on_semantic_output();
            finalize_streaming(app);
            match finish_tool_output(app, &id, status, &output) {
                Ok(artifact) => persist_tool_entry_with_artifact(app, &id, artifact),
                Err(error) => {
                    app.transcript.entries.push(Entry::Error { text: error.to_string() });
                    return None;
                }
            }
            app.runtime.request_observation.finish_tool(&id);
            persist_completed_observation(app);
            record_successful_skill_read(app, &id, status);

            if let Some(result) = write_result
                && let Some(ref mut writer) = app.session.writer
            {
                let turn_id = format!("turn_{}", app.session.turn_count);
                let _ = writer.append_file_write(&turn_id, &result, status);
            }

            if let Some(result) = shell_result {
                if result.kind == tools::shell::ProcessKind::Background {
                    let process_id = result.process_id.unwrap_or_else(|| {
                        // Keep synthetic adapter/test events visible while the
                        // live shell path uses the id assigned before spawn
                        // returns.
                        app.runtime.process_registry.register(
                            result.command.clone(),
                            result.cwd.clone(),
                            result.kind,
                            CancelToken::new(),
                        )
                    });
                    app.runtime.process_registry.announce(process_id);
                    app.transcript.entries.push_child_activity(
                        process_id,
                        format!(
                            "background process [{process_id}] started: {}",
                            result.command.join(" ")
                        ),
                    );
                }

                if let Some(ref mut writer) = app.session.writer {
                    let turn_id = format!("turn_{}", app.session.turn_count);
                    let _ = writer.append_shell_exec(&turn_id, &result);
                }
            }
            app.refresh_git_status();
            None
        }
        AgentEvent::StateProjectionDecision { id, decision } => {
            app.transcript.tool_projection_decisions.insert(id, decision);
            None
        }
        AgentEvent::ModelMetadataLoaded(items) => {
            app.runtime.model_picker_items = items
                .into_iter()
                .map(|(label, detail)| PickerItem::new(label, detail))
                .collect();
            None
        }
        AgentEvent::Retrying { attempt, max_attempts, delay_ms, error } => {
            persist_active_context_snapshot(app, session::ContextSnapshotState::Failed);
            app.runtime.turn_timing.ensure_started();
            discard_retry_output(app);
            app.runtime.run_state = RunState::Working;
            app.runtime.provider_retry = Some(provider_retry_status(attempt, max_attempts, delay_ms, &error));
            None
        }
        AgentEvent::PermissionRequest(permission) => {
            app.runtime.turn_timing.ensure_started();
            finalize_streaming(app);
            if app.overlay.permission().is_some() {
                let _ = permission.cancel();
                app.transcript.entries.push(Entry::Error {
                    text: "acp: received a second permission request while one is pending; cancelled it".to_string(),
                });
                return None;
            }
            let turn_id = format!("turn_{}", app.session.turn_count);
            if let Some(ref mut writer) = app.session.writer {
                let _ = writer.append_acp_permission_request(&turn_id, &permission);
            }
            app.transcript.entries.push_permission(
                &permission.tool_call_id,
                format!(
                    "acp permission requested: {} ({})",
                    permission.title, permission.tool_call_id
                ),
            );
            app.overlay.show_permission(permission);
            app.transcript.context_ledger = None;
            None
        }
        AgentEvent::PermissionResolved { tool_call_id, outcome } => {
            let turn_id = format!("turn_{}", app.session.turn_count);
            if let Some(ref mut writer) = app.session.writer {
                let _ = writer.append_acp_permission_outcome(&turn_id, &tool_call_id, &outcome);
            }
            let text = format!("acp permission {tool_call_id}: {outcome}");
            if !app.transcript.entries.resolve_permission(&tool_call_id, text.clone()) {
                app.transcript.entries.push_permission(&tool_call_id, text);
            }
            app.transcript.context_ledger = None;
            None
        }
        AgentEvent::AcpSession(metadata) => {
            if let Some(ref mut writer) = app.session.writer {
                let _ = writer.append_acp_session(&metadata);
            }
            None
        }
        AgentEvent::Finished => {
            app.runtime.provider_retry = None;
            app.runtime.turn_timing.finish_turn();
            app.runtime.stopping_deadline = None;
            app.runtime.ttft.clear_pending();
            finalize_streaming(app);
            persist_completed_observation(app);
            cancel_pending_permission(app);
            app.runtime.run_state = RunState::Idle;
            app.composer.last_input = None;
            app.refresh_git_status();
            match context::finish_manual_compaction(app) {
                None => persist_final_response(app),
                Some(None) => {}
                Some(Some(restart)) => {
                    app.refresh_context_ledger(None);
                    return Some(restart);
                }
            }
            app.refresh_context_ledger(None);
            let id = app.composer.queue.pending_id(QueueTarget::FollowUp)?;
            let next = app.composer.queue.settle(id, QueueSettlement::Sent);
            super::input::audit_queue_transition(app, id, "sent");
            next.and_then(|next| submit_user_turn(app, next))
        }
        AgentEvent::Failed(msg) => {
            app.runtime.turn_timing.finish_turn();
            persist_active_context_snapshot(app, session::ContextSnapshotState::Failed);
            app.runtime.provider_retry = None;
            app.runtime.stopping_deadline = None;
            let manual_compaction = context::restore_failed_manual_compaction(app);
            app.runtime.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            app.transcript.entries.push(Entry::Error { text: msg.clone() });
            app.runtime.run_state = RunState::Error(msg);
            if !manual_compaction {
                let submitted_input = app.composer.last_input.take();
                if app.composer.input.is_empty()
                    && let Some(input) = submitted_input
                {
                    app.composer.input.set_text(&input);
                }
            }
            persist_last_entry(app);
            open_credential_recovery_after_rejection(app);
            persist_last_entry(app);
            app.refresh_git_status();
            app.refresh_context_ledger(None);
            None
        }
        AgentEvent::Cancelled => {
            app.runtime.turn_timing.finish_turn();
            persist_active_context_snapshot(app, session::ContextSnapshotState::Interrupted);
            app.runtime.provider_retry = None;
            app.runtime.stopping_deadline = None;
            context::restore_failed_manual_compaction(app);
            app.runtime.ttft.clear_pending();
            finalize_streaming(app);
            cancel_pending_permission(app);
            cancel_running_tools(app);
            if app.runtime.run_state == RunState::Working {
                app.transcript
                    .entries
                    .push(Entry::Status { text: String::from("cancelled") });
            }
            app.runtime.run_state = RunState::Idle;
            app.composer.last_input = None;
            let cancelled = app
                .composer
                .queue
                .pending(QueueTarget::Steering)
                .map(|item| item.id)
                .collect::<Vec<_>>();
            for id in cancelled {
                app.composer.queue.settle(id, QueueSettlement::Cancelled);
                super::input::audit_queue_transition(app, id, "cancelled");
            }
            persist_last_entry(app);
            app.refresh_git_status();
            app.refresh_context_ledger(None);
            None
        }
    }
}

fn provider_retry_status(attempt: u32, max_attempts: u32, delay_ms: u64, error: &str) -> String {
    let reason = if error.to_ascii_lowercase().contains("overload") {
        "provider overloaded"
    } else {
        "provider unavailable"
    };
    format!(
        "Waiting · {reason} · retry {attempt}/{max_attempts} in {:.1}s",
        delay_ms as f64 / 1000.0
    )
}

/// Drain completed application-owned background processes into the transcript
/// and append their terminal shell lifecycle records.
pub fn drain_background_processes(app: &mut App) {
    let results = app.runtime.process_registry.drain_completed();
    record_background_results(app, results);
}

/// Refresh the active shell transcript row with output captured since the last UI tick.
pub fn refresh_foreground_output(app: &mut App) {
    let Some(output) = app.runtime.process_registry.foreground_output() else {
        return;
    };
    let mut lines = output.stdout;
    lines.extend(output.stderr);
    if lines.is_empty() {
        return;
    }
    if let Some(Entry::Tool { output, .. }) = app
        .transcript
        .entries
        .iter_mut()
        .rev()
        .find(|entry| matches!(entry, Entry::Tool { name, status: ToolStatus::Running, .. } if name.split('#').next() == Some(tools::shell::NAME)))
    {
        *output = lines;
    }
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
        if let Some(process_id) = result.process_id {
            app.transcript.entries.push_child_activity(process_id, lines.join("\n"));
        } else {
            app.transcript.entries.push(Entry::Status { text: lines.join("\n") });
        }
        if let Some(writer) = app.session.writer.as_mut() {
            let turn_id = format!("turn_{}", app.session.turn_count);
            let _ = writer.append_shell_exec(&turn_id, &result);
        }
    }
}

pub fn handle_permission_action(app: &mut App, action: &Action) -> Option<Msg> {
    match action {
        Action::SelectPrevious => {
            if let Some(permission) = app.overlay.permission_mut() {
                permission.move_up();
            }
        }
        Action::SelectNext => {
            if let Some(permission) = app.overlay.permission_mut() {
                permission.move_down();
            }
        }
        Action::Confirm => {
            if let Some(permission) = app.overlay.take_permission()
                && let Some(PermissionDecision::Selected(option_id)) = permission.select()
            {
                app.transcript.entries.push(Entry::Status {
                    text: format!("acp permission {}: selected {option_id}", permission.tool_call_id),
                });
            }
            app.transcript.context_ledger = None;
        }
        Action::Cancel => {
            if let Some(permission) = app.overlay.take_permission() {
                let _ = permission.cancel();
                app.transcript
                    .entries
                    .push(Entry::Status { text: format!("acp permission {}: cancelled", permission.tool_call_id) });
            }
            app.transcript.context_ledger = None;
        }
        _ => {}
    }
    None
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
    app.transcript
        .entries
        .push(Entry::Status { text: String::from("cancelled") });
    app.runtime.run_state = RunState::Stopping;
    app.runtime.stopping_deadline = Some(app.runtime.ui_tick.wrapping_add(stopping_grace_ticks(app)));
    persist_last_entry(app);
}

/// Complete a stopped run once the worker has had a short chance to acknowledge
/// cancellation.
///
/// The direct loop drops the receiver after this transitions the app to idle,
/// so a worker that remains blocked cannot later mutate a subsequent run.
pub fn finish_stopping_if_due(app: &mut App) {
    let Some(deadline) = app.runtime.stopping_deadline else {
        return;
    };

    if app.runtime.run_state != RunState::Stopping {
        app.runtime.stopping_deadline = None;
        return;
    }

    if now_or_after_deadline(app.runtime.ui_tick, deadline) {
        handle_agent_event(app, AgentEvent::Cancelled);
        app.runtime.stopping_timed_out = true;
    }
}

/// Translate the fixed stop grace period to the configured tick cadence.
pub fn stopping_grace_ticks(app: &App) -> u64 {
    let tick_ms = app.runtime.cli.tick_rate_ms.max(1);
    STOPPING_GRACE_MS / tick_ms + u64::from(!STOPPING_GRACE_MS.is_multiple_of(tick_ms))
}

pub fn cancel_pending_permission(app: &mut App) {
    if let Some(permission) = app.overlay.take_permission() {
        let _ = permission.cancel();
        app.transcript.context_ledger = None;
    }
}

pub fn remember_input(app: &mut App, text: &str) {
    if text.is_empty() || app.composer.input_history.last().is_some_and(|last| last == text) {
        return;
    }
    let overflow = app
        .composer
        .input_history
        .len()
        .saturating_add(1)
        .saturating_sub(INPUT_HISTORY_LIMIT);
    if overflow > 0 {
        app.composer.input_history.drain(..overflow);
    }
    app.composer.input_history.push(text.to_string());
    let _ = app.session.input_history_store.append(&app.session.id, text);
}

pub fn recall_older_input(app: &mut App) {
    if app.composer.input_history.is_empty() {
        return;
    }

    let next = match app.composer.history_cursor {
        Some(0) => 0,
        Some(index) => index.saturating_sub(1),
        None => {
            app.composer.history_draft = app.composer.input.text();
            app.composer.input_history.len() - 1
        }
    };
    app.composer.history_cursor = Some(next);
    app.composer.input.set_text(&app.composer.input_history[next]);
}

pub fn recall_newer_input(app: &mut App) {
    let Some(index) = app.composer.history_cursor else {
        return;
    };

    if index + 1 < app.composer.input_history.len() {
        let next = index + 1;
        app.composer.history_cursor = Some(next);
        app.composer.input.set_text(&app.composer.input_history[next]);
    } else {
        app.composer.history_cursor = None;
        app.composer.input.set_text(&app.composer.history_draft);
        app.composer.history_draft.clear();
    }
}

pub fn exit_history_navigation(app: &mut App) {
    if app.composer.history_cursor.is_some() {
        app.composer.history_cursor = None;
        app.composer.history_draft.clear();
    }
}

/// Persist the last, finalized transcript entry to the session file, if a writer exists.
pub fn persist_last_entry(app: &mut App) {
    if let Some(ref mut writer) = app.session.writer
        && let Some(entry) = app.transcript.entries.last()
    {
        let turn_id = format!("turn_{}", app.session.turn_count);
        let _ = writer.append_entry(entry, &turn_id);
    }
}

fn record_tool_started(app: &mut App, id: &str, name: &str, arguments: &str) {
    app.runtime.ttft.stop_on_semantic_output();
    finalize_streaming(app);
    if let Err(error) = app
        .transcript
        .entries
        .queue_tool(id, name, arguments)
        .and_then(|()| app.transcript.entries.start_tool(id))
    {
        app.transcript.entries.push(Entry::Error { text: error.to_string() });
        return;
    }
    if let Some(ref mut writer) = app.session.writer {
        let turn_id = format!("turn_{}", app.session.turn_count);
        let _ = writer.append_tool_started(&turn_id, id, name, arguments);
    }
}

fn record_skill_read(app: &mut App, tool_name: &str, arguments: &str) {
    if tool_name.split_once('#').map_or(tool_name, |(name, _)| name) != "read_file_range" {
        return;
    }
    let Some(path) = serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|value| value.get("path")?.as_str().map(PathBuf::from))
    else {
        return;
    };
    let path = if path.is_absolute() { path } else { app.runtime.cwd.join(path) };
    let Ok(path) = path.canonicalize() else { return };
    let Some(skill) = app
        .transcript
        .skills
        .iter()
        .find(|skill| skill.path.canonicalize().is_ok_and(|skill_path| skill_path == path))
        .cloned()
    else {
        return;
    };
    if app.transcript.entries.iter().any(
        |entry| matches!(entry, Entry::Skill { path: active_path, .. } if Path::new(active_path).canonicalize().is_ok_and(|active_path| active_path == path)),
    ) {
        return;
    }
    match skills::load_skill(&skill) {
        Ok(loaded) => append_loaded_skill(app, &loaded, false),
        Err(diagnostic) => app.transcript.entries.push(Entry::Error { text: diagnostic.summary() }),
    }
}

fn record_successful_skill_read(app: &mut App, call_id: &str, status: ToolStatus) {
    if status != ToolStatus::Ok {
        return;
    }
    let Some(Entry::Tool { name, arguments, .. }) = app.transcript.entries.tool_entry(call_id) else {
        return;
    };
    let name = name.clone();
    let arguments = arguments.clone();
    record_skill_read(app, &name, &arguments);
}

/// Persist the identified tool block with its bounded artifact metadata.
fn persist_tool_entry_with_artifact(app: &mut App, call_id: &str, artifact: Option<artifacts::ArtifactMetadata>) {
    if let Some(ref mut writer) = app.session.writer
        && let Some(entry) = app.transcript.entries.tool_entry(call_id)
    {
        let turn_id = format!("turn_{}", app.session.turn_count);
        let _ = writer.append_entry_with_artifact(entry, &turn_id, artifact);
    }
}

/// Persist the final model response even if provider status rows were appended
/// after the last assistant/reasoning delta.
pub fn persist_final_response(app: &mut App) {
    if let Some(ref mut writer) = app.session.writer
        && let Some(entry) = app.transcript.entries.iter().rev().find(|entry| {
            matches!(
                entry,
                Entry::Agent { streaming: false, .. } | Entry::Reasoning { streaming: false, .. }
            )
        })
    {
        let turn_id = format!("turn_{}", app.session.turn_count);
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
    for entry in &mut app.transcript.entries {
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
    app.transcript.entries.cancel_running_tools();
}

/// Mark active reasoning entries complete when the model moves on to visible
/// assistant text or a tool call.
pub fn finalize_reasoning(app: &mut App) {
    for entry in &mut app.transcript.entries {
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
        app.transcript.entries.last(),
        Some(Entry::Agent { .. } | Entry::Reasoning { .. })
    ) {
        app.transcript.entries.pop();
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
    let (current_files, current_diagnostics) = load_mcp_config_audit(&app.runtime.cwd);
    if app.session.mcp_config_files == current_files && app.session.mcp_config_diagnostics == current_diagnostics {
        return;
    }

    let previous_files = std::mem::replace(&mut app.session.mcp_config_files, current_files.clone());
    app.session.mcp_config_diagnostics = current_diagnostics.clone();
    app.transcript
        .entries
        .push(Entry::Status { text: mcp_config_changed_status(&previous_files, &current_files) });
    if let Some(ref mut writer) = app.session.writer {
        let _ = writer.append_mcp_config_changed(turn_id, previous_files, current_files, current_diagnostics);
    }
}

fn finish_tool_output(
    app: &mut App, id: &str, status: ToolStatus, output: &[String],
) -> Result<Option<artifacts::ArtifactMetadata>, ToolLifecycleError> {
    let artifact = app
        .session
        .context_capture_policy
        .permits_content()
        .then(|| app.artifact_store())
        .flatten()
        .and_then(|store| store.create_tool_evidence(&format!("tool:{id}"), output).ok());
    let safe_output = artifact.as_ref().map_or_else(
        || artifacts::bounded_redacted_lines(output, artifacts::DEFAULT_MAX_ARTIFACT_BYTES),
        |write| write.bounded_lines.clone(),
    );
    if let Some(write) = &artifact {
        app.transcript
            .tool_artifacts
            .insert(id.to_string(), write.metadata.handle.clone());
    }
    let truncated = safe_output != output;
    app.transcript.entries.finish_tool(id, status, safe_output, truncated)?;
    Ok(artifact.map(|write| write.metadata))
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
    let RunState::Error(message) = &app.runtime.run_state else {
        return;
    };
    if !is_credential_rejection(message) || crate::acp::config::parse_model_id(&app.runtime.model).is_some() {
        return;
    }

    let provider = super::onboarding::provider_for_model(&app.runtime.model);
    if let Some(env_var) = active_environment_credential(provider, &app.runtime.cwd) {
        app.overlay.show_setup(FirstRunRecovery::rejected_environment(provider));
        app.transcript.entries.push(Entry::Status {
            text: format!(
                "{env_var} was rejected and overrides stored credentials; replace or unset it, then restart thndrs. The prompt draft is preserved"
            ),
        });
        return;
    }

    app.overlay.show_setup(FirstRunRecovery::reauthenticate(provider));
    app.transcript.entries.push(Entry::Status {
        text: format!(
            "credential rejected; opened sign-in recovery for {} while keeping the prompt draft",
            provider.label()
        ),
    });
}

fn is_credential_rejection(message: &str) -> bool {
    let message = message.trim_start().to_ascii_lowercase();
    message.starts_with("authentication failed (http 401)")
        || message.starts_with("authentication failed (http 403)")
        || message.starts_with("authentication failed:")
        || message.starts_with("opencode go authentication failed (http 401)")
        || message.starts_with("opencode go authentication failed (http 403)")
        || message.starts_with("opencode go authentication failed:")
        || message.starts_with("opencode zen authentication failed (http 401)")
        || message.starts_with("opencode zen authentication failed (http 403)")
        || message.starts_with("opencode zen authentication failed:")
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
        assert!(is_credential_rejection(
            "OpenCode Go authentication failed: invalid key"
        ));
        assert!(!is_credential_rejection(
            "server error (HTTP 500): upstream authentication failed while validating its own service"
        ));
    }
}
