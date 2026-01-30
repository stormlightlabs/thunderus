use super::App;
use crate::transcript;
use thunderus_core::Event;

pub fn reconstruct_transcript_from_session(app: &mut App) -> thunderus_core::Result<()> {
    let Some(ref session) = app.session else {
        return Ok(());
    };

    let events = session.read_events()?;

    for logged_event in events {
        match logged_event.event {
            Event::UserMessage { content } => app.transcript_mut().add_user_message(&content),
            Event::ModelMessage { content, tokens_used: _ } => app.transcript_mut().add_model_response(&content),
            Event::ToolCall { tool, arguments } => {
                let args_str = serde_json::to_string_pretty(&arguments).unwrap_or_default();
                app.transcript_mut().add_tool_call(&tool, &args_str, "safe");
            }
            Event::ToolResult { tool, result, success, error } => {
                let result_str = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "Invalid JSON".to_string());
                app.transcript_mut().add_tool_result(&tool, &result_str, success);

                if let Some(error_msg) = error {
                    app.transcript_mut()
                        .add_system_message(format!("Tool error: {}", error_msg));
                }
            }
            Event::Approval { action, approved } => {
                let decision = if approved {
                    transcript::ApprovalDecision::Approved
                } else {
                    transcript::ApprovalDecision::Rejected
                };

                app.transcript_mut().add_approval_prompt(&action, "safe");
                let _ = app.transcript_mut().set_approval_decision(decision);
            }
            Event::Patch { name, status, files, diff } => {
                let status_str = format!("{:?}", status);
                app.transcript_mut().add_system_message(format!(
                    "Patch: {} ({})\nFiles: {:?}\n{}",
                    name, status_str, files, diff
                ));
            }
            Event::ShellCommand { command, args, working_dir, exit_code, output_ref } => {
                let cmd_str = if args.is_empty() { command.clone() } else { format!("{} {}", command, args.join(" ")) };
                app.transcript_mut().add_system_message(format!(
                    "Shell: {} (in {})\nExit: {:?}",
                    cmd_str,
                    working_dir.display(),
                    exit_code
                ));

                if let Some(ref output_file) = output_ref {
                    app.transcript_mut()
                        .add_system_message(format!("Output in: {}", output_file));
                }
            }
            Event::GitSnapshot { commit, branch, changed_files } => {
                app.transcript_mut().add_system_message(format!(
                    "Git snapshot: {} @ {}\nChanged files: {}",
                    commit, branch, changed_files
                ));
            }
            Event::FileRead { file_path, line_count, offset, success } => {
                if success {
                    app.transcript_mut().add_system_message(format!(
                        "File read: {} (lines: {}, offset: {})",
                        file_path, line_count, offset
                    ));
                } else {
                    app.transcript_mut()
                        .add_system_message(format!("File read failed: {}", file_path));
                }
            }
            Event::ApprovalModeChange { from, to } => app.transcript_mut().add_system_message(format!(
                "Approval mode changed: {} → {}",
                from.as_str(),
                to.as_str()
            )),
            Event::ViewEdit { view, change_type, .. } => app
                .transcript_mut()
                .add_system_message(format!("View edited: {} ({})", view, change_type)),
            Event::ContextLoad { source, path, .. } => app
                .transcript_mut()
                .add_system_message(format!("Context loaded: {} from {}", source, path)),
            Event::Checkpoint { label, description, .. } => app
                .transcript_mut()
                .add_system_message(format!("Checkpoint: {} - {}", label, description)),
            Event::PlanUpdate { action, item, reason } => {
                let reason_str = reason.as_ref().map(|r| format!(" (reason: {})", r)).unwrap_or_default();
                app.transcript_mut()
                    .add_system_message(format!("Plan {}: {}{}", action, item, reason_str));
            }
            Event::MemoryUpdate { kind, path, operation, .. } => app
                .transcript_mut()
                .add_system_message(format!("Memory {}: {} ({})", operation, path, kind)),
        }
    }

    let is_empty = app.transcript.is_empty();
    app.state_mut().set_first_session(is_empty);

    Ok(())
}
