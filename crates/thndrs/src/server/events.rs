//! Pure mapping from app events to ACP session-update intents.
//!
//! The intents are intentionally small and protocol-agnostic so handlers can
//! lower them to SDK `SessionNotification` values without importing transport
//! details.

use std::collections::BTreeSet;

use crate::app::{AgentEvent, ToolStatus};
use crate::tools::{WriteResult, shell::ProcessResult, shell::redact_secrets};
use agent_client_protocol::schema::v1::ToolCallLocation;

const MAX_TOOL_FIELD_CHARS: usize = 4096;
const MAX_TOOL_LOCATIONS: usize = 6;
const MAX_TOOL_PATH_TOKENS: usize = 12;

/// Intent payload emitted by the ACP server for one `session/update` event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionUpdateIntent {
    /// Assistant-facing text chunk.
    AssistantDelta(String),
    /// Reasoning/thinking text chunk.
    ReasoningDelta(String),
    /// Generic status update for UIs or logs.
    Status(String),
    /// Token usage snapshot.
    Usage {
        /// Input tokens observed since the session started.
        input_tokens: u64,
        /// Output tokens observed since the session started.
        output_tokens: u64,
    },
    /// Tool call created/in-progress event.
    ToolStarted {
        /// Tool-call correlation id.
        id: String,
        /// Human-readable tool name/title.
        name: String,
        /// Human-readable argument payload.
        arguments: String,
        /// Classified tool kind for client and permission handling.
        kind: ToolCallKind,
        /// Helpful file locations associated with this tool.
        locations: Vec<ToolCallLocation>,
    },
    /// Tool call completion event.
    ToolFinished {
        /// Tool-call correlation id.
        id: String,
        /// Tool result status.
        status: ToolStatusIntent,
        /// Tool output lines.
        output: Vec<String>,
        /// Classified tool kind for client and permission handling.
        kind: ToolCallKind,
        /// Helpful file locations associated with this tool.
        locations: Vec<ToolCallLocation>,
    },
    /// Prompt turn failed with terminal error details.
    Failed(String),
    /// Prompt turn was cancelled.
    Cancelled,
    /// Prompt turn finished normally.
    Finished,
}

/// ACP-facing tool status intent used by [`SessionUpdateIntent::ToolFinished`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatusIntent {
    /// Tool execution is still running.
    InProgress,
    /// Tool execution completed successfully.
    Completed,
    /// Tool execution failed.
    Failed,
}

impl From<ToolStatus> for ToolStatusIntent {
    fn from(status: ToolStatus) -> Self {
        Self::from_tool_status(status)
    }
}

impl ToolStatusIntent {
    /// Convert the app transcript status into the ACP-facing status.
    pub const fn from_tool_status(status: ToolStatus) -> Self {
        match status {
            ToolStatus::Ok => Self::Completed,
            ToolStatus::Failed => Self::Failed,
            ToolStatus::Cancelled => Self::Failed,
            ToolStatus::Running => Self::InProgress,
        }
    }
}

/// Coarse tool-kind taxonomy for server-side permission hooks and UI hints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolCallKind {
    /// Reading files or data.
    Read,
    /// Modifying files or content.
    Edit,
    /// Searching for information.
    Search,
    /// Running commands or code.
    Execute,
    /// Retrieving external data.
    Fetch,
    /// Internal reasoning or planning.
    Think,
    /// Catch-all for unsupported/unmapped tools.
    Other,
}

impl ToolCallKind {
    /// Whether this kind should usually require a permission gate.
    pub const fn requires_permission(self) -> bool {
        matches!(self, Self::Edit | Self::Execute)
    }

    /// Convert to the ACP schema's tool kind.
    pub const fn to_acp_kind(self) -> agent_client_protocol::schema::v1::ToolKind {
        use agent_client_protocol::schema::v1::ToolKind;
        match self {
            Self::Read => ToolKind::Read,
            Self::Edit => ToolKind::Edit,
            Self::Search => ToolKind::Search,
            Self::Execute => ToolKind::Execute,
            Self::Fetch => ToolKind::Fetch,
            Self::Think => ToolKind::Think,
            Self::Other => ToolKind::Other,
        }
    }
}

/// Return whether this tool kind should usually go through permission review.
pub const fn requires_permission(kind: ToolCallKind) -> bool {
    kind.requires_permission()
}

/// Classify a tool call name into a limited UX/permission taxonomy.
pub fn classify_tool(name: &str) -> ToolCallKind {
    match crate::mcp::adapter::tool_presentation(name) {
        crate::mcp::adapter::McpToolPresentation::Search => return ToolCallKind::Search,
        crate::mcp::adapter::McpToolPresentation::Fetch => return ToolCallKind::Fetch,
        crate::mcp::adapter::McpToolPresentation::Generic => {}
    }
    match name {
        "find_files" | "list_searchable_files" | "read_file_range" | "read" | "sawk" | "acp.fs.read_text_file" => {
            ToolCallKind::Read
        }
        "search_text" => ToolCallKind::Search,
        "create_file" | "replace_range" | "write_patch" | "acp.fs.write_text_file" => ToolCallKind::Edit,
        "run_shell" | "acp.terminal" => ToolCallKind::Execute,
        "read_url" | "fetch_url" | "http_request" => ToolCallKind::Fetch,
        "plan" | "think" => ToolCallKind::Think,
        _ => ToolCallKind::Other,
    }
}

/// Convert one app event into zero or more ACP session-update intents.
///
/// Returns an empty list for internal events that are not visible protocol
/// updates.
pub fn map_agent_event(event: &AgentEvent) -> Vec<SessionUpdateIntent> {
    match event {
        AgentEvent::AssistantDelta(text) => vec![SessionUpdateIntent::AssistantDelta(text.clone())],
        AgentEvent::ReasoningDelta(text) => vec![SessionUpdateIntent::ReasoningDelta(text.clone())],
        AgentEvent::Status(text) => vec![SessionUpdateIntent::Status(text.clone())],
        AgentEvent::Usage { input_tokens, output_tokens } => {
            vec![SessionUpdateIntent::Usage { input_tokens: *input_tokens, output_tokens: *output_tokens }]
        }
        AgentEvent::RequestAccounting(accounting) => {
            let Some(usage) = &accounting.provider_usage else {
                return vec![];
            };
            vec![SessionUpdateIntent::Usage {
                input_tokens: usage.components.input_tokens.unwrap_or(0),
                output_tokens: usage.components.output_tokens.unwrap_or(0),
            }]
        }
        AgentEvent::RequestStarted(_) => vec![],
        AgentEvent::ToolStarted { id, name, arguments } => {
            let kind = classify_tool(name);
            vec![SessionUpdateIntent::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                arguments: sanitize_tool_payload(arguments),
                kind,
                locations: infer_tool_locations_from_arguments(name, arguments),
            }]
        }
        AgentEvent::ToolFinished { id, status, output, write_result, shell_result, .. } => {
            let kind = infer_tool_kind_from_result(write_result.as_ref(), shell_result.as_deref());
            vec![SessionUpdateIntent::ToolFinished {
                id: id.clone(),
                status: (*status).into(),
                output: sanitize_tool_outputs(output),
                kind,
                locations: infer_tool_locations(output, write_result.as_ref(), shell_result.as_deref()),
            }]
        }
        AgentEvent::Failed(message) => vec![SessionUpdateIntent::Failed(message.clone())],
        AgentEvent::Cancelled => vec![SessionUpdateIntent::Cancelled],
        AgentEvent::Finished => vec![SessionUpdateIntent::Finished],
        AgentEvent::Started
        | AgentEvent::CodexUsage(_)
        | AgentEvent::StateProjectionDecision { .. }
        | AgentEvent::ModelMetadataLoaded(_)
        | AgentEvent::Retrying { .. }
        | AgentEvent::PermissionRequest(_)
        | AgentEvent::PermissionResolved { .. }
        | AgentEvent::AcpSession(_) => vec![],
    }
}

fn infer_tool_kind_from_result(
    write_result: Option<&WriteResult>, shell_result: Option<&ProcessResult>,
) -> ToolCallKind {
    match write_result {
        Some(_) => ToolCallKind::Edit,
        None => match shell_result {
            Some(_) => ToolCallKind::Execute,
            None => ToolCallKind::Other,
        },
    }
}

fn infer_tool_locations(
    output: &[String], write_result: Option<&WriteResult>, shell_result: Option<&ProcessResult>,
) -> Vec<ToolCallLocation> {
    let mut locations = BTreeSet::new();
    if let Some(result) = write_result {
        locations.insert((result.path.display().to_string(), None));
    }
    if let Some(result) = shell_result {
        let cwd = result.cwd.to_string_lossy().to_string();
        if !cwd.is_empty() {
            locations.insert((cwd, None));
        }
    }

    for line in output {
        if locations.len() >= MAX_TOOL_LOCATIONS {
            break;
        }
        collect_shell_path_candidates(line, &mut locations);
    }

    locations
        .into_iter()
        .filter_map(tool_call_location)
        .take(MAX_TOOL_LOCATIONS)
        .collect()
}

fn infer_tool_locations_from_arguments(tool_name: &str, arguments: &str) -> Vec<ToolCallLocation> {
    let mut locations = BTreeSet::new();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) {
        collect_location_values(&value, None, &mut locations);
    }
    if matches!(tool_name, "run_shell") && locations.is_empty() {
        collect_shell_path_candidates(arguments, &mut locations);
    }

    locations
        .into_iter()
        .filter_map(tool_call_location)
        .take(MAX_TOOL_LOCATIONS)
        .collect()
}

fn collect_shell_path_candidates(line: &str, locations: &mut BTreeSet<(String, Option<u32>)>) {
    let mut accepted = 0;
    for token in line.split_whitespace() {
        if accepted >= MAX_TOOL_PATH_TOKENS || locations.len() >= MAX_TOOL_LOCATIONS {
            return;
        }
        let token = token.trim_matches(|c| c == '"' || c == '\'');
        if is_file_like_path(token) {
            let _ = locations.insert((token.to_string(), None));
            accepted += 1;
        }
    }
}

fn collect_location_values(
    value: &serde_json::Value, line_hint: Option<u32>, locations: &mut BTreeSet<(String, Option<u32>)>,
) {
    match value {
        serde_json::Value::Object(entries) => {
            let mut current_line = line_hint;
            if let Some(line) = entries.get("line").and_then(as_u32) {
                current_line = Some(line);
            }
            if let Some(line) = entries.get("start_line").and_then(as_u32) {
                current_line = Some(line);
            }

            for (key, child) in entries {
                if is_path_key(key) {
                    collect_locations_for_key(key, child, current_line, locations);
                } else {
                    collect_location_values(child, current_line, locations);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_location_values(child, line_hint, locations);
            }
        }
        _ => {}
    }
}

fn collect_locations_for_key(
    _key: &str, value: &serde_json::Value, line_hint: Option<u32>, locations: &mut BTreeSet<(String, Option<u32>)>,
) {
    match value {
        serde_json::Value::String(path) if is_path_like_value(path) => {
            if is_file_like_path(path) {
                locations.insert((path.to_string(), line_hint));
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                if let serde_json::Value::String(path) = child
                    && is_file_like_path(path)
                {
                    locations.insert((path.to_string(), line_hint));
                }
            }
        }
        serde_json::Value::Object(_) => {
            collect_location_values(value, line_hint, locations);
        }
        _ => {}
    }
}

fn is_path_key(key: &str) -> bool {
    matches!(
        key,
        "path"
            | "paths"
            | "file"
            | "filename"
            | "source"
            | "source_path"
            | "target"
            | "target_path"
            | "destination"
            | "destination_path"
            | "old_path"
            | "new_path"
            | "cwd"
            | "directory"
            | "dir"
    )
}

fn is_path_like_value(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }

    let trimmed = value.trim();
    !trimmed.starts_with('-') && !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_file_like_path(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.contains('/') || trimmed.contains('.') || trimmed == ".." || trimmed == "."
}

fn as_u32(value: &serde_json::Value) -> Option<u32> {
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

fn tool_call_location((path, line): (String, Option<u32>)) -> Option<ToolCallLocation> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(ToolCallLocation::new(trimmed).line(line))
}

pub fn sanitize_tool_payload(payload: &str) -> String {
    sanitize_tool_text(payload)
}

fn sanitize_tool_outputs(outputs: &[String]) -> Vec<String> {
    outputs.iter().map(|line| sanitize_tool_text(line)).collect()
}

fn sanitize_tool_text(raw: &str) -> String {
    let value = redact_sensitive_payload(raw);
    cap_value(&value)
}

fn redact_sensitive_payload(value: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(json) => redact_json_value(json).to_string(),
        Err(_) => redact_secrets(value),
    }
}

fn redact_json_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(redact_secrets(&text)),
        serde_json::Value::Array(items) => serde_json::Value::Array(items.into_iter().map(redact_json_value).collect()),
        serde_json::Value::Object(entries) => serde_json::Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let redacted = if key_is_sensitive(&key) {
                        serde_json::Value::String(String::from("[REDACTED]"))
                    } else {
                        redact_json_value(value)
                    };
                    (key, redacted)
                })
                .collect(),
        ),
        _ => value,
    }
}

fn key_is_sensitive(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "password" | "passwd" | "api_key" | "apikey" | "access_token" | "secret" | "token"
    )
}

fn cap_value(value: &str) -> String {
    let char_len = value.chars().count();
    if char_len <= MAX_TOOL_FIELD_CHARS {
        return value.to_string();
    }

    let truncated: String = value.chars().take(MAX_TOOL_FIELD_CHARS).collect();
    format!("{truncated}...[truncated]")
}
