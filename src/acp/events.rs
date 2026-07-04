//! Mapping from ACP session updates into existing app events.

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, Plan, SessionUpdate, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
};

use crate::app::{AgentEvent, ToolStatus};

const MAX_TOOL_FIELD_CHARS: usize = 4096;

/// Convert a stable ACP v1 session update into one or more UI agent events.
pub fn map_session_update(update: SessionUpdate) -> Vec<AgentEvent> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => text_from_content_chunk(chunk)
            .map(AgentEvent::AssistantDelta)
            .into_iter()
            .collect(),
        SessionUpdate::AgentThoughtChunk(chunk) => text_from_content_chunk(chunk)
            .map(AgentEvent::ReasoningDelta)
            .into_iter()
            .collect(),
        SessionUpdate::UserMessageChunk(_) => {
            vec![AgentEvent::Status("acp: received user message echo".to_string())]
        }
        SessionUpdate::ToolCall(tool_call) => map_tool_call(&tool_call),
        SessionUpdate::ToolCallUpdate(update) => map_tool_call_update(update),
        SessionUpdate::Plan(plan) => map_plan(&plan),
        SessionUpdate::UsageUpdate(usage) => {
            vec![AgentEvent::Usage { input_tokens: usage.used, output_tokens: usage.size.saturating_sub(usage.used) }]
        }
        SessionUpdate::AvailableCommandsUpdate(_) => {
            vec![AgentEvent::Status("acp: available commands updated".to_string())]
        }
        SessionUpdate::CurrentModeUpdate(_) => {
            vec![AgentEvent::Status("acp: current mode updated".to_string())]
        }
        SessionUpdate::ConfigOptionUpdate(_) => {
            vec![AgentEvent::Status("acp: config options updated".to_string())]
        }
        SessionUpdate::SessionInfoUpdate(_) => {
            vec![AgentEvent::Status("acp: session info updated".to_string())]
        }
        _ => vec![AgentEvent::Status("acp: unsupported session update".to_string())],
    }
}

fn map_tool_call(tool_call: &ToolCall) -> Vec<AgentEvent> {
    let id = tool_call.tool_call_id.to_string();
    let mut events = vec![AgentEvent::ToolStarted {
        id: id.clone(),
        name: tool_call.title.clone(),
        arguments: capped_json(tool_call.raw_input.as_ref()),
    }];
    if matches!(tool_call.status, ToolCallStatus::Completed | ToolCallStatus::Failed) || tool_call.raw_output.is_some()
    {
        events.push(tool_finished(
            id,
            tool_call_content_output(&tool_call.content, tool_call.raw_output.as_ref()),
            tool_status(tool_call.status),
        ));
    }
    events
}

fn map_tool_call_update(update: ToolCallUpdate) -> Vec<AgentEvent> {
    let id = update.tool_call_id.to_string();
    let name = update.fields.title.unwrap_or_else(|| "acp tool".to_string());
    let status = update.fields.status.unwrap_or(ToolCallStatus::InProgress);
    if matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed) || update.fields.raw_output.is_some() {
        vec![tool_finished(
            id,
            tool_call_content_output(
                &update.fields.content.unwrap_or_default(),
                update.fields.raw_output.as_ref(),
            ),
            tool_status(status),
        )]
    } else {
        vec![AgentEvent::ToolStarted { id, name, arguments: capped_json(update.fields.raw_input.as_ref()) }]
    }
}

fn map_plan(plan: &Plan) -> Vec<AgentEvent> {
    if plan.entries.is_empty() {
        return vec![AgentEvent::Status("acp: plan updated".to_string())];
    }
    let text = plan
        .entries
        .iter()
        .map(|entry| format!("- {:?}: {}", entry.status, entry.content))
        .collect::<Vec<_>>()
        .join("\n");
    vec![AgentEvent::ReasoningDelta(format!("Plan:\n{text}"))]
}

fn text_from_content_chunk(chunk: ContentChunk) -> Option<String> {
    match chunk.content {
        ContentBlock::Text(text) => Some(text.text),
        ContentBlock::Image(_) => Some("[image content]".to_string()),
        ContentBlock::Audio(_) => Some("[audio content]".to_string()),
        ContentBlock::ResourceLink(link) => Some(format!("[resource: {}]", link.uri)),
        ContentBlock::Resource(_) => Some("[embedded resource]".to_string()),
        _ => Some("[unsupported content]".to_string()),
    }
}

fn tool_call_content_output(content: &[ToolCallContent], raw_output: Option<&serde_json::Value>) -> Vec<String> {
    let mut output = content.iter().filter_map(tool_content_line).collect::<Vec<_>>();
    if let Some(raw_output) = raw_output {
        output.push(format!("raw_output: {}", cap(&redact(&raw_output.to_string()))));
    }
    if output.is_empty() {
        output.push("acp tool completed".to_string());
    }
    output
}

fn tool_content_line(content: &ToolCallContent) -> Option<String> {
    match content {
        ToolCallContent::Content(content) => match &content.content {
            ContentBlock::Text(text) => Some(cap(&redact(&text.text))),
            ContentBlock::Image(_) => Some("[image content]".to_string()),
            ContentBlock::Audio(_) => Some("[audio content]".to_string()),
            ContentBlock::ResourceLink(link) => Some(format!("[resource: {}]", link.uri)),
            ContentBlock::Resource(_) => Some("[embedded resource]".to_string()),
            _ => Some("[unsupported content]".to_string()),
        },
        ToolCallContent::Diff(diff) => Some(format!("diff: {}", diff.path.display())),
        ToolCallContent::Terminal(_) => Some("[terminal output]".to_string()),
        _ => Some("[unsupported tool content]".to_string()),
    }
}

fn tool_finished(id: String, output: Vec<String>, status: ToolStatus) -> AgentEvent {
    AgentEvent::ToolFinished { id, output, status, write_result: None, shell_result: None }
}

fn tool_status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::Failed => ToolStatus::Failed,
        ToolCallStatus::Pending | ToolCallStatus::InProgress | ToolCallStatus::Completed => ToolStatus::Ok,
        _ => ToolStatus::Ok,
    }
}

fn capped_json(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(value) => cap(&redact(&value.to_string())),
        None => "{}".to_string(),
    }
}

fn cap(value: &str) -> String {
    if value.len() <= MAX_TOOL_FIELD_CHARS {
        value.to_string()
    } else {
        format!("{}...[truncated]", &value[..MAX_TOOL_FIELD_CHARS])
    }
}

fn redact(value: &str) -> String {
    value.replace("sk-", "sk-[redacted]-")
}
