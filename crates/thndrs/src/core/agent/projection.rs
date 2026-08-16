//! Model-facing projections for tool output and failed tool input.

use super::*;

/// Build the provider-native tool result from the independent model projection.
///
/// The display projection and structured evidence remain owned by `output`.
/// An explicit applied reducer configuration adds only the bounded aggregate
/// dashboard to the model result; shadow-only measurement leaves the model
/// request unchanged.
pub(crate) fn model_tool_result(
    tool_id: &str, output: &ToolOutput, shell_result: Option<&ProcessResult>,
    config: &thndrs_agent::context::ReductionConfig,
    state_identity: Option<thndrs_agent::context::StateProjectionIdentity>, state_protected: bool,
    state_history: &[thndrs_agent::context::StateProjectionRecord],
) -> (
    ProviderMessage,
    String,
    thndrs_agent::context::ReductionResult,
    thndrs_agent::context::StateProjectionDecision,
    Option<thndrs_agent::context::StateProjectionRecord>,
) {
    let baseline = output.model_lines();
    let command_projection = shell_result.and_then(|result| {
        output.evidence.artifact_handle.as_deref().and_then(|handle| {
            tools::command_projection::project(&format!("tool:{tool_id}"), &baseline, result, handle, config)
        })
    });
    let projection_input = command_projection.as_ref().map_or_else(
        || baseline.clone(),
        |projection| {
            if projection.receipt.mode == thndrs_agent::ContextReductionMode::Applied {
                projection.lines.clone()
            } else {
                baseline.clone()
            }
        },
    );
    let mut reduced = thndrs_agent::reduce_lines(&format!("tool:{tool_id}"), projection_input, config);
    if let Some(projection) = command_projection {
        reduced.receipts.insert(0, projection.receipt.clone());
        reduced.dashboard.receipts.insert(0, projection.receipt.clone());
        if projection.receipt.mode == thndrs_agent::ContextReductionMode::Applied {
            reduced.dashboard.before_bytes = thndrs_agent::measure_lines(&baseline);
            reduced.dashboard.before_lines = baseline.len();
            reduced.dashboard.routine_omissions = reduced.dashboard.before_lines.saturating_sub(reduced.lines.len());
        }
    }
    let mut state_candidate = thndrs_agent::context::StateProjectionCandidate::new(
        format!("tool:{tool_id}"),
        reduced.lines.clone(),
        state_identity,
    );
    if state_protected {
        state_candidate = state_candidate.protected();
    }
    let state_reduction = thndrs_agent::reduce_state_identical(&state_candidate, state_history, config);
    let state_record = state_reduction.history_record(&state_candidate);
    let projection_decision = state_reduction
        .receipt
        .as_ref()
        .filter(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Applied)
        .map_or(thndrs_agent::context::StateProjectionDecision::Retained, |_| {
            state_reduction.decision.clone()
        });
    if let Some(receipt) = state_reduction.receipt {
        reduced.receipts.push(receipt.clone());
        reduced.dashboard.receipts.push(receipt.clone());
        if receipt.mode == thndrs_agent::ContextReductionMode::Applied {
            reduced.lines = state_reduction.lines;
            reduced.dashboard.after_bytes = thndrs_agent::measure_lines(&reduced.lines);
            reduced.dashboard.after_lines = reduced.lines.len();
            reduced.dashboard.routine_omissions = reduced.dashboard.before_lines.saturating_sub(reduced.lines.len());
        }
    }
    let suppressed_duplicate = matches!(
        state_reduction.decision,
        thndrs_agent::context::StateProjectionDecision::DuplicateOf { .. }
    ) && reduced.lines.is_empty();
    let mut content = if suppressed_duplicate {
        String::new()
    } else if reduced.lines.is_empty() {
        "(no output)".to_string()
    } else {
        reduced.lines.join("\n")
    };
    if reduced
        .receipts
        .iter()
        .any(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Applied)
    {
        content.push('\n');
        content.push_str(&reduced.render_dashboard());
    }
    let message = ProviderMessage::tool_result(tool_id, &content, output.status == ToolStatus::Failed);
    (message, content, reduced, projection_decision, state_record)
}

/// Replace a failed non-command tool's oversized argument body only after the
/// bounded artifact store has returned a recovery handle. Shell argv remains
/// untouched: command-aware reduction projects output, never a user command.
pub(crate) fn project_failed_tool_input(
    request: &ToolUseRequest, output: &ToolOutput, config: &thndrs_agent::context::ReductionConfig,
) -> (serde_json::Value, Option<thndrs_agent::ContextReductionReceipt>) {
    let baseline = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);
    if request.name == tools::shell::NAME
        || output.status != ToolStatus::Failed
        || request.arguments.len() < FAILED_TOOL_INPUT_MIN_BYTES
        || (!config.failed_tool_input && !config.shadow)
    {
        return (baseline, None);
    }
    let Some(handle) = output.evidence.artifact_handle.as_deref() else {
        return (baseline, None);
    };

    let projected = serde_json::json!({
        "projection": "failed tool arguments omitted after failure; recover bounded redacted evidence from the recorded artifact",
        "tool_call_id": request.tool_use_id,
        "recovery_handle": handle,
        "audit": "original arguments remain in the tool-started audit record"
    });
    let after_bytes = serde_json::to_string(&projected).map_or(0, |json| json.len() as u64);
    let mode = if config.failed_tool_input {
        thndrs_agent::ContextReductionMode::Applied
    } else {
        thndrs_agent::ContextReductionMode::Shadow
    };
    let receipt = thndrs_agent::ContextReductionReceipt {
        item_id: format!("tool_input:{}", request.tool_use_id),
        method: FAILED_TOOL_INPUT_PROJECTION_METHOD.to_string(),
        version: FAILED_TOOL_INPUT_PROJECTION_VERSION.to_string(),
        before_bytes: request.arguments.len() as u64,
        after_bytes,
        lossy: true,
        mode,
        diagnostic: None,
    };
    if mode == thndrs_agent::ContextReductionMode::Applied {
        (projected, Some(receipt))
    } else {
        (baseline, Some(receipt))
    }
}

/// Best-effort classifier for prompts that should not finish without a
/// workspace write. This is intentionally narrow: it requires both a file-ish
/// reference and an edit/action verb, unless the prompt explicitly disallows
/// file changes.
pub(crate) fn prompt_expects_workspace_write(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    if [
        "read-only",
        "read only",
        "do not edit",
        "don't edit",
        "do not modify",
        "don't modify",
        "do not write",
        "don't write",
        "without editing",
        "without modifying",
        "without writing",
        "no edits",
        "no file changes",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return false;
    }

    let fileish = lower.contains(".md")
        || lower.contains(".rs")
        || lower.contains(".toml")
        || lower.contains(".json")
        || lower.contains(".yaml")
        || lower.contains(".yml")
        || lower.contains("file")
        || lower.contains("todo");
    let action = [
        "add",
        "change",
        "document",
        "edit",
        "fix",
        "modify",
        "remove",
        "replace",
        "rewrite",
        "summarize",
        "update",
        "write",
    ]
    .iter()
    .any(|word| {
        lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|part| part == *word)
    });

    fileish && action
}
