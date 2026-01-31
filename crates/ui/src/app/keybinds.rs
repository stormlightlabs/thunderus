use super::App;
use crate::components::Inspector;
use crate::event_handler::{EventHandler, KeyAction};
use crate::state::MainView;
use crate::transcript::{self, CardDetailLevel};

use std::path::PathBuf;
use thunderus_core::{ApprovalDecision, ApprovalMode, MemoryDoc, TrajectoryWalker};

pub async fn handle_event(app: &mut App, event: crossterm::event::Event) {
    if let Some(action) = EventHandler::handle_event(&event, app.state_mut()) {
        match action {
            KeyAction::SendMessage { message } => {
                let is_fork_mode = app.state().input.is_in_fork_mode();
                let fork_point = app.state().input.fork_point_index;

                if is_fork_mode {
                    if let Some(idx) = fork_point {
                        let history: Vec<String> = app.state().input.message_history.clone();
                        app.state_mut().input.replace_history_entry(idx, message.clone());
                        app.state_mut().input.truncate_history_from(idx);
                        app.transcript_mut().clear();
                        for hist_msg in history.iter() {
                            app.transcript_mut().add_user_message(hist_msg);
                        }

                        app.transcript_mut().add_system_message(format!(
                            "Forked at position {} - conversation truncated and restarted",
                            idx + 1
                        ));
                    }
                    app.state_mut().input.exit_fork_mode();
                } else {
                    app.state_mut().input.add_to_history(message.clone());
                }

                app.state_mut().session.last_message = Some(message.clone());
                app.transcript_mut().add_user_message(&message);
                app.persist_user_message(&message);
                app.state_mut().exit_first_session();

                match app.provider.clone() {
                    Some(provider) => app.spawn_agent_for_message(message, &provider),
                    None => app
                        .transcript_mut()
                        .add_system_message("No provider configured. Cannot process message."),
                }
            }
            KeyAction::ExecuteShellCommand { command } => app.execute_shell_command(command),
            KeyAction::Approve { action: _, risk: _ } => app.send_approval_response(ApprovalDecision::Approved),
            KeyAction::Reject { action: _, risk: _ } => app.send_approval_response(ApprovalDecision::Rejected),
            KeyAction::Cancel { action: _, risk: _ } => app.send_approval_response(ApprovalDecision::Cancelled),
            KeyAction::CancelGeneration => {
                app.cancel_token.cancel();
                app.pause_token.cancel();
                app.state_mut().stop_generation();
                app.transcript_mut()
                    .mark_streaming_cancelled("Generation cancelled by user");
            }
            KeyAction::StartReconcileRitual => app.start_reconcile_ritual(),
            KeyAction::ReconcileContinue => app.reconcile_continue(),
            KeyAction::ReconcileDiscard => app.reconcile_discard(),
            KeyAction::ReconcileStop => app.reconcile_stop(),
            KeyAction::RewindLastMessage => {
                let history_len = app.state().input.message_history.len();
                if history_len > 0 {
                    app.state_mut().input.message_history.pop();
                    app.transcript_mut().add_system_message(format!(
                        "Rewound: Removed last message from history ({} messages remaining)",
                        history_len - 1
                    ));
                } else {
                    app.transcript_mut()
                        .add_system_message("No messages in history to rewind.");
                }
            }
            KeyAction::Exit => app.should_exit = true,
            KeyAction::RetryLastFailedAction => {
                let has_retryable_error = app
                    .transcript()
                    .entries()
                    .iter()
                    .any(|entry| matches!(entry, transcript::TranscriptEntry::ErrorEntry { can_retry: true, .. }));

                match app.state_mut().last_message().cloned() {
                    Some(last_message) => match has_retryable_error {
                        true => {
                            app.transcript_mut().add_system_message("Retrying last message...");
                            app.state_mut().input.add_to_history(last_message.clone());
                            app.transcript_mut().add_user_message(last_message.clone());

                            if let Some(provider) = app.provider.clone() {
                                app.spawn_agent_for_message(last_message.clone(), &provider);
                            }
                        }
                        false => app
                            .transcript_mut()
                            .add_system_message("No retryable error found. Use message history to re-send a message."),
                    },
                    None => app.transcript_mut().add_system_message("No previous message to retry."),
                }
            }
            KeyAction::ToggleTheme => {
                app.state_mut().toggle_theme_variant();
                app.persist_theme_variant();
                let variant = app.state.theme_variant();
                app.transcript_mut()
                    .add_system_message(format!("Theme switched to {}", variant));
            }
            KeyAction::ToggleAdvisorMode => {
                let new_mode = match app.state.config.approval_mode {
                    ApprovalMode::ReadOnly => ApprovalMode::Auto,
                    _ => ApprovalMode::ReadOnly,
                };
                app.state_mut().config.approval_mode = new_mode;

                app.transcript_mut().add_system_message(format!(
                    "Advisor mode: {} (Ctrl+A to toggle)",
                    if new_mode == ApprovalMode::ReadOnly { "ON" } else { "OFF" }
                ));

                if let Some(ref mut session) = app.session {
                    let _ = session.set_approval_mode(new_mode);
                }
            }
            KeyAction::ToggleSidebar | KeyAction::ToggleVerbosity | KeyAction::ToggleSidebarSection => (),
            KeyAction::OpenExternalEditor => app.open_external_editor(),
            KeyAction::NavigateHistory => (),
            KeyAction::ActivateFuzzyFinder => {
                let input = app.state.input.buffer.clone();
                let cursor = app.state.input.cursor;
                app.state_mut().enter_fuzzy_finder(input, cursor);
            }
            KeyAction::SelectFileInFinder { path } => {
                app.state_mut().exit_fuzzy_finder();
                let input = app.state_mut().input.buffer.clone();
                let cursor = app.state_mut().input.cursor;

                let mut new_input = input[..cursor].to_string();
                new_input.push('@');
                new_input.push_str(&path);
                new_input.push_str(&input[cursor..]);

                app.state_mut().input.buffer = new_input;
                app.state_mut().input.cursor = cursor + 1 + path.len();
            }
            KeyAction::NavigateFinderUp
            | KeyAction::NavigateFinderDown
            | KeyAction::ToggleFinderSort
            | KeyAction::CancelFuzzyFinder => (),
            KeyAction::SlashCommandModel { model } => app.handle_model_command(model),
            KeyAction::SlashCommandApprovals { mode } => app.handle_approvals_command(mode),
            KeyAction::SlashCommandVerbosity { level } => app.handle_verbosity_command(level),
            KeyAction::SlashCommandStatus => app.handle_status_command(),
            KeyAction::SlashCommandPlan => app.handle_plan_command(),
            KeyAction::SlashCommandPlanAdd { item } => app.handle_plan_add_command(item),
            KeyAction::SlashCommandPlanDone { index } => app.handle_plan_done_command(index),
            KeyAction::SlashCommandReview => app.handle_review_command(),
            KeyAction::SlashCommandMemory => app.handle_memory_command(),
            KeyAction::SlashCommandMemoryAdd { fact } => app.handle_memory_add_command(fact),
            KeyAction::SlashCommandMemorySearch { query } => app.handle_memory_search_command(query),
            KeyAction::SlashCommandMemoryPin { id } => app.handle_memory_pin_command(id),
            KeyAction::SlashCommandSearch { query, scope } => app.handle_search_command(query, scope),
            KeyAction::SlashCommandClear => {
                app.transcript_mut().clear();
                app.transcript_mut()
                    .add_system_message("Transcript cleared (session history preserved)");
            }
            KeyAction::SlashCommandGardenConsolidate { session_id } => {
                app.handle_garden_consolidate_command(session_id)
            }
            KeyAction::SlashCommandGardenHygiene => app.handle_garden_hygiene_command(),
            KeyAction::SlashCommandGardenDrift => app.handle_garden_drift_command(),
            KeyAction::SlashCommandGardenVerify { doc_id } => app.handle_garden_verify_command(doc_id),
            KeyAction::SlashCommandGardenStats => app.handle_garden_stats_command(),
            KeyAction::NavigateCardNext => {
                if !app.transcript_mut().focus_next_card() {
                    app.transcript_mut().scroll_down(1);
                    app.state_mut().scroll_vertical(1);
                }
            }
            KeyAction::NavigateCardPrev => {
                if !app.transcript_mut().focus_prev_card() {
                    app.transcript_mut().scroll_up(1);
                    app.state_mut().scroll_vertical(-1);
                }
            }
            KeyAction::ToggleCardExpand => {
                app.transcript_mut().toggle_focused_card_detail_level();
            }
            KeyAction::ToggleCardVerbose => {
                let _ = app
                    .transcript_mut()
                    .set_focused_card_detail_level(CardDetailLevel::Verbose);
            }
            KeyAction::ScrollUp => match app.transcript().entries().iter().any(|e| e.is_action_card()) {
                true => {
                    app.transcript_mut().focus_prev_card();
                }
                false => {
                    app.transcript_mut().scroll_up(1);
                    app.state_mut().scroll_vertical(-1);
                }
            },
            KeyAction::ScrollDown => match app.transcript().entries().iter().any(|e| e.is_action_card()) {
                true => {
                    app.transcript_mut().focus_next_card();
                }
                false => {
                    app.transcript_mut().scroll_down(1);
                    app.state_mut().scroll_vertical(1);
                }
            },
            KeyAction::PageUp => {
                app.transcript_mut().scroll_up(10);
                app.state_mut().scroll_vertical(-10);
            }
            KeyAction::PageDown => {
                app.transcript_mut().scroll_down(10);
                app.state_mut().scroll_vertical(10);
            }
            KeyAction::ScrollToTop => {
                app.transcript_mut().scroll_up(usize::MAX);
                app.state_mut().ui.scroll_vertical = 0;
            }
            KeyAction::ScrollToBottom => {
                app.transcript_mut().scroll_to_bottom();
                app.state_mut().ui.scroll_vertical = 0;
            }
            KeyAction::CollapseSidebarSection => app.state_mut().ui.sidebar_collapse_state.collapse_prev(),
            KeyAction::ExpandSidebarSection => app.state_mut().ui.sidebar_collapse_state.expand_next(),
            KeyAction::FocusSlashCommand => {
                app.state_mut().input.buffer = "/".to_string();
                app.state_mut().input.cursor = 1;
            }
            KeyAction::ClearTranscriptView => {
                app.transcript_mut().clear();
                app.transcript_mut()
                    .add_system_message("Transcript cleared (session history preserved)");
            }
            KeyAction::NavigateNextPatch => {
                let total = app.state().patches().len() + app.state().memory_patches().len();
                app.state_mut().next_patch(total);
            }
            KeyAction::NavigatePrevPatch => {
                let total = app.state().patches().len() + app.state().memory_patches().len();
                app.state_mut().prev_patch(total);
            }
            KeyAction::NavigateNextHunk => {
                let Some(patch_idx) = app.state().selected_patch_index() else {
                    return;
                };
                let Some(file_path_str) = app.state().selected_file_path() else {
                    return;
                };

                let file_path = PathBuf::from(file_path_str);

                let total_hunks = app
                    .state()
                    .patches()
                    .get(patch_idx)
                    .and_then(|p| p.hunk_count(&file_path))
                    .unwrap_or(0);

                app.state_mut().next_hunk(total_hunks);

                let has_files = app
                    .state()
                    .patches()
                    .get(patch_idx)
                    .map(|p| !p.files.is_empty())
                    .unwrap_or(false);

                if app.state().selected_hunk_index().is_none() && has_files {
                    app.state_mut()
                        .set_selected_file(file_path.to_str().unwrap_or("").to_string());
                }
            }
            KeyAction::NavigatePrevHunk => {
                let Some(patch_idx) = app.state().selected_patch_index() else {
                    return;
                };
                let Some(file_path_str) = app.state().selected_file_path() else {
                    return;
                };

                let file_path = PathBuf::from(file_path_str);

                let total_hunks = app
                    .state()
                    .patches()
                    .get(patch_idx)
                    .and_then(|p| p.hunk_count(&file_path))
                    .unwrap_or(0);

                app.state_mut().prev_hunk(total_hunks);

                let has_files = app
                    .state()
                    .patches()
                    .get(patch_idx)
                    .map(|p| !p.files.is_empty())
                    .unwrap_or(false);

                if app.state().selected_hunk_index().is_none() && has_files {
                    app.state_mut()
                        .set_selected_file(file_path.to_str().unwrap_or("").to_string());
                }
            }
            KeyAction::ApproveHunk => {
                let Some(patch_idx) = app.state().selected_patch_index() else {
                    return;
                };

                let file_patch_count = app.state().patches().len();

                if patch_idx >= file_patch_count {
                    let mem_idx = patch_idx - file_patch_count;
                    let result = app
                        .state_mut()
                        .memory_patches_mut()
                        .get_mut(mem_idx)
                        .ok_or_else(|| "Memory patch not found".to_string())
                        .and_then(|patch| {
                            patch.approve();
                            patch.apply().map_err(|e| format!("Failed to apply: {}", e))?;
                            Ok(format!("Applied memory patch: {}", patch.doc_id))
                        });

                    match result {
                        Ok(msg) => app.transcript_mut().add_system_message(msg),
                        Err(e) => app
                            .transcript_mut()
                            .add_system_message(format!("Failed to apply: {}", e)),
                    }
                } else {
                    let Some(hunk_idx) = app.state().selected_hunk_index() else {
                        return;
                    };
                    let Some(file_path_str) = app.state().selected_file_path() else {
                        return;
                    };

                    let file_path = PathBuf::from(file_path_str);

                    let result = app
                        .state_mut()
                        .patches_mut()
                        .get_mut(patch_idx)
                        .ok_or_else(|| "Patch not found".to_string())
                        .and_then(|patch| patch.approve_hunk(&file_path, hunk_idx));

                    if let Err(e) = result {
                        app.transcript_mut()
                            .add_system_message(format!("Failed to approve hunk: {}", e));
                    }
                }
            }
            KeyAction::RejectHunk => {
                let Some(patch_idx) = app.state().selected_patch_index() else {
                    return;
                };

                let file_patch_count = app.state().patches().len();

                if patch_idx >= file_patch_count {
                    let mem_idx = patch_idx - file_patch_count;
                    let result = app
                        .state_mut()
                        .memory_patches_mut()
                        .get_mut(mem_idx)
                        .ok_or_else(|| "Memory patch not found".to_string())
                        .map(|patch| {
                            patch.reject();
                            format!("Rejected memory patch: {}", patch.doc_id)
                        });

                    match result {
                        Ok(msg) => app.transcript_mut().add_system_message(msg),
                        Err(e) => app
                            .transcript_mut()
                            .add_system_message(format!("Failed to reject: {}", e)),
                    }
                } else {
                    let Some(hunk_idx) = app.state().selected_hunk_index() else {
                        return;
                    };
                    let Some(file_path_str) = app.state().selected_file_path() else {
                        return;
                    };

                    let file_path = PathBuf::from(file_path_str);

                    let result = app
                        .state_mut()
                        .patches_mut()
                        .get_mut(patch_idx)
                        .ok_or_else(|| "Patch not found".to_string())
                        .and_then(|patch| patch.reject_hunk(&file_path, hunk_idx));

                    if let Err(e) = result {
                        app.transcript_mut()
                            .add_system_message(format!("Failed to reject hunk: {}", e));
                    }
                }
            }
            KeyAction::ToggleHunkDetails => app.state_mut().toggle_hunk_details(),
            KeyAction::MemoryHitsNavigate => {}
            KeyAction::MemoryHitsOpen { path } => {
                app.transcript_mut()
                    .add_system_message(format!("Opening memory document: {}", path));
                app.state_mut().memory_hits.clear();
            }
            KeyAction::MemoryHitsPin { id } => {
                let is_pinned = app.state().memory_hits.is_pinned(&id);
                app.transcript_mut().add_system_message(format!(
                    "{}: {}",
                    if is_pinned { "Unpinned" } else { "Pinned" },
                    id
                ));
            }
            KeyAction::MemoryHitsClose => app.transcript_mut().add_system_message("Memory panel closed"),
            KeyAction::ToggleInspector => app.state_mut().ui.toggle_inspector(),
            KeyAction::InspectMemory { path } => {
                let content = tokio::fs::read_to_string(&path).await;
                match content {
                    Ok(c) => match MemoryDoc::parse(&c) {
                        Ok(doc) => {
                            let session_dir = app.session.as_ref().map(|s| s.session_dir());
                            if let Some(dir) = session_dir {
                                let walker = TrajectoryWalker::new(dir);
                                match walker.walk(&doc).await {
                                    Ok(nodes) => {
                                        app.state_mut().evidence.set_nodes(nodes);
                                        app.state_mut().ui.active_view = MainView::Inspector;
                                        app.transcript_mut().add_system_message(format!("Inspecting: {}", path));
                                    }
                                    Err(e) => {
                                        app.transcript_mut()
                                            .add_system_message(format!("Failed to walk trajectory: {}", e));
                                    }
                                }
                            } else {
                                app.transcript_mut()
                                    .add_system_message("No active session for trajectory walk.");
                            }
                        }
                        Err(e) => {
                            app.transcript_mut()
                                .add_system_message(format!("Failed to parse memory doc: {}", e));
                        }
                    },
                    Err(e) => {
                        app.transcript_mut()
                            .add_system_message(format!("Failed to read memory doc: {}", e));
                    }
                }
            }
            KeyAction::InspectorNavigate => {}
            KeyAction::InspectorOpenFile { .. } => {
                let inspector = Inspector::new(&app.state);
                let files = inspector.affected_files();
                if let Some(path) = files.first() {
                    app.transcript_mut()
                        .add_system_message(format!("Opening affected file: {}", path));
                } else {
                    app.transcript_mut()
                        .add_system_message("No affected files for this event.");
                }
            }
            KeyAction::NoOp => (),
            KeyAction::SlashCommandConfig => {
                app.state_mut().open_config_editor();
            }
            KeyAction::ConfigEditorSave => {
                let editor_values = app.state().config_editor.as_ref().map(|editor| {
                    (
                        editor.approval_mode,
                        editor.sandbox_mode,
                        editor.network_access,
                        editor.save(),
                    )
                });

                match editor_values {
                    Some((approval_mode, sandbox_mode, network_access, save_result)) => match save_result {
                        Ok(msg) => {
                            app.state_mut().config.approval_mode = approval_mode;
                            app.state_mut().config.sandbox_mode = sandbox_mode;
                            app.state_mut().config.allow_network = network_access;
                            app.transcript_mut().add_system_message(msg);
                        }
                        Err(e) => app.transcript_mut().add_system_message(format!("Error: {}", e)),
                    },
                    None => {
                        app.transcript_mut()
                            .add_system_message("No config editor open".to_string());
                    }
                }
                app.state_mut().close_config_editor();
            }
            KeyAction::ConfigEditorCancel => {
                app.state_mut().close_config_editor();
                app.transcript_mut().add_system_message("Config editor cancelled");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::create_test_app;
    use crate::state::ApprovalState;
    use crate::transcript;
    use thunderus_core::ApprovalMode;

    #[tokio::test]
    async fn test_handle_event_send_message() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test message".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 2);
        assert_eq!(app.state().input.buffer, "");
    }

    #[tokio::test]
    async fn test_handle_event_send_message_empty() {
        let mut app = create_test_app();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 0);
    }

    #[tokio::test]
    async fn test_handle_shell_command_event() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "!cmd echo test".to_string();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        app.handle_event(event).await;

        let transcript = app.transcript();
        let entries = transcript.entries();

        let user_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::UserMessage { .. }));
        assert!(user_entry.is_some());
        if let transcript::TranscriptEntry::UserMessage { content } = user_entry.unwrap() {
            assert!(content.contains("!cmd echo test"));
        }
    }

    #[tokio::test]
    async fn test_handle_event_approve_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_reject_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_cancel_action() {
        let mut app = create_test_app();
        app.state_mut().approval_ui.pending_approval =
            Some(ApprovalState::pending("test.action".to_string(), "risky".to_string()));

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert!(app.state().approval_ui.pending_approval.is_none());
    }

    #[tokio::test]
    async fn test_handle_event_cancel_generation() {
        let mut app = create_test_app();
        app.state_mut().start_generation();

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        app.handle_event(event).await;

        assert!(!app.state().is_generating());
    }

    #[tokio::test]
    async fn test_handle_event_char_input() {
        let mut app = create_test_app();

        for c in "Hello".chars() {
            let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
            app.handle_event(event).await;
        }

        assert_eq!(app.state().input.buffer, "Hello");
    }

    #[tokio::test]
    async fn test_handle_event_open_external_editor() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Initial content".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
        ));

        app.handle_event(event).await;

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[tokio::test]
    async fn test_handle_clear_command() {
        let mut app = create_test_app();

        app.transcript_mut().add_user_message("Test message");
        app.transcript_mut().add_system_message("Test system message");

        assert_eq!(app.transcript().len(), 2);

        app.state_mut().input.buffer = "/clear".to_string();
        app.handle_event(crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )))
        .await;

        assert_eq!(app.transcript().len(), 1);
        if let transcript::TranscriptEntry::SystemMessage { content } = app.transcript().last().unwrap() {
            assert!(content.contains("Transcript cleared"));
        } else {
            panic!("Expected SystemMessage");
        }
    }

    #[tokio::test]
    async fn test_slash_command_integration() {
        let mut app = create_test_app();

        app.state_mut().input.buffer = "/status".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.transcript().len(), 1);
        assert!(app.state().input.buffer.is_empty());
    }

    #[tokio::test]
    async fn test_slash_command_with_args_integration() {
        let mut app = create_test_app();
        let original_mode = app.state().config.approval_mode;

        app.state_mut().input.buffer = "/approvals read-only".to_string();
        let event = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle_event(event).await;

        assert_eq!(app.state().config.approval_mode, ApprovalMode::ReadOnly);
        assert_ne!(app.state().config.approval_mode, original_mode);
        assert_eq!(app.transcript().len(), 1);
    }
}
