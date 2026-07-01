//! Umans Code provider — Anthropic-compatible Messages API.
//!
//! Uses `POST /v1/messages` with `x-api-key` and `anthropic-version` headers.
//! Streaming responses arrive as SSE events, parsed into [`AgentEvent`].

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app::AgentEvent;
use crate::cli::WebSearchMode;

/// Umans Code base URL.
pub const BASE_URL: &str = "https://api.code.umans.ai";

/// Required Anthropic version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Header name for the Umans web search provider selector.
pub const WEBSEARCH_HEADER: &str = "X-Umans-Websearch-Provider";

/// Environment variable name for the API key.
pub const API_KEY_ENV: &str = "UMANS_API_KEY";

type Result<T> = std::result::Result<T, UmansError>;

/// Recommended completion token budget for known Umans models.
pub fn max_tokens_for_model(model: &str) -> u32 {
    match model {
        "umans-glm-5.2" | "umans-glm-5.1" => 131_071,
        "umans-minimax-m2.5" => 8_192,
        _ => 32_768,
    }
}

/// Static model entry used by the offline model picker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownModel {
    pub id: &'static str,
    pub description: &'static str,
}

/// Current Umans Code models from the public docs.
///
/// Live metadata can still be fetched with [`UmansClient::fetch_models_info`],
/// but the picker should remain useful before credentials or network are ready.
pub fn known_models() -> Vec<KnownModel> {
    vec![
        KnownModel { id: "umans-coder", description: "Default route, currently Kimi K2.7-Code" },
        KnownModel { id: "umans-kimi-k2.7", description: "Hard coding tasks, always-on reasoning" },
        KnownModel { id: "umans-glm-5.2", description: "Latest GLM, largest context window" },
        KnownModel { id: "umans-glm-5.1", description: "Previous GLM for text-first workflows" },
        KnownModel { id: "umans-flash", description: "Fast light model for context and summaries" },
    ]
}

/// Errors from the Umans client.
#[derive(Debug, thiserror::Error)]
pub enum UmansError {
    /// `UMANS_API_KEY` is not set.
    #[error("UMANS_API_KEY is not set")]
    MissingApiKey,
    /// HTTP transport error.
    #[error("http error: {0}")]
    Http(String),
    /// HTTP status error (non-2xx response).
    #[error("HTTP {code}: {body}")]
    Status { code: u16, body: String },
    /// JSON serialization/deserialization error.
    ///
    /// TODO: display model-metadata parse failures in the model picker/status
    /// UI once `/v1/models/info` is wired into the live app.
    #[error("json error: {0}")]
    Json(String),
}

/// Concrete Umans Code API client.
pub struct UmansClient {
    base_url: String,
    api_key: String,
    agent: ureq::Agent,
}

impl UmansClient {
    /// Create a client from `UMANS_API_KEY`, falling back to workspace `.env`.
    pub fn from_env_or_dotenv(workspace_root: &Path) -> Result<Self> {
        match env::var(API_KEY_ENV) {
            Ok(api_key) => {
                tracing::debug!(source = "environment", "loaded Umans API key");
                Ok(Self::new(BASE_URL, &api_key))
            }
            Err(_) => api_key_from_dotenv(workspace_root)
                .map(|api_key| {
                    tracing::debug!(source = ".env", path = %workspace_root.join(".env").display(), "loaded Umans API key");
                    Self::new(BASE_URL, &api_key)
                })
                .ok_or_else(|| {
                    tracing::error!(env = API_KEY_ENV, cwd = %workspace_root.display(), "missing Umans API key");
                    UmansError::MissingApiKey
                }),
        }
    }

    /// Create a client with an explicit base URL and API key.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        model_metadata_display_todo_marker();
        UmansClient {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    /// Fetch model metadata from `GET /v1/models/info`.
    ///
    /// TODO: call this during startup or model selection and display context
    /// window/tool/reasoning capability metadata in the TUI.
    pub fn fetch_models_info(&self) -> Result<HashMap<String, ModelInfo>> {
        let url = format!("{}/v1/models/info", self.base_url);
        let mut resp = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| UmansError::Http(e.to_string()))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| UmansError::Http(e.to_string()))?;

        serde_json::from_str::<HashMap<String, ModelInfo>>(&body).map_err(|e| UmansError::Json(e.to_string()))
    }

    /// Build the request body for `POST /v1/messages`.
    ///
    /// This is a pure function — no network — making it testable in isolation.
    ///
    /// When `tools` is `Some`, the tool schemas are included in the `tools`
    /// field so the model can issue `tool_use` blocks.
    ///
    /// The schema is sent every turn because Umans does not expose reusable-history
    /// for tool definitions.
    pub fn build_messages_request_body(
        model: &str, messages: &[Message], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": stream,
        });
        if let Some(t) = tools
            && !t.as_array().is_some_and(|arr| arr.is_empty())
        {
            body["tools"] = t.clone();
        }
        body
    }

    /// Build the HTTP headers map for a Messages API request.
    pub fn build_headers(&self, search_mode: WebSearchMode) -> Vec<(String, String)> {
        vec![
            ("x-api-key".to_string(), self.api_key.clone()),
            ("anthropic-version".to_string(), ANTHROPIC_VERSION.to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
            (WEBSEARCH_HEADER.to_string(), search_mode.header_value().to_string()),
        ]
    }

    /// Send a streaming `POST /v1/messages` request and return the HTTP
    /// response.
    ///
    /// Includes the `X-Umans-Websearch-Provider` header so Umans knows which
    /// search backend to use (or `none` to pass a local `web_search` tool
    /// through unchanged).
    ///
    /// When `tools` is `Some`, the compact tool schema is included in the
    /// request body so the model can issue `tool_use` blocks. The schema is
    /// sent every turn.
    ///
    /// The caller reads lines from the response body and feeds them to
    /// [`parse_sse_chunk`] and [`parse_sse_event`].
    pub fn send_streaming_request(
        &self, model: &str, messages: &[Message], max_tokens: u32, mode: WebSearchMode,
        tools: Option<&serde_json::Value>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = Self::build_messages_request_body(model, messages, max_tokens, true, tools);
        let tool_count = tools.and_then(|t| t.as_array()).map_or(0, Vec::len);
        tracing::info!(
            model,
            max_tokens,
            search = %mode.header_value(),
            messages = messages.len(),
            tools = tool_count,
            "sending Umans streaming request"
        );

        let mut request = self.agent.post(&url);
        for (key, value) in self.build_headers(mode) {
            request = request.header(&key, &value);
        }

        let mut response = request
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(&body)
            .map_err(|e| {
                tracing::error!(error = %e, "Umans request failed before HTTP response");
                UmansError::Http(e.to_string())
            })?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|e| format!("failed to read error body: {e}"));
            let body = summarize_error_body(&body);
            tracing::error!(status, error = %body, "Umans request returned non-success status");
            return Err(UmansError::Status { code: status, body });
        }

        tracing::info!(status, "Umans streaming request connected");
        Ok(response)
    }
}

fn summarize_error_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty response body)".to_string();
    }

    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("message"))
                .and_then(|m| m.as_str())
                .map(|m| m.to_string())
        })
        .unwrap_or_else(|| trimmed.chars().take(500).collect())
}

fn api_key_from_dotenv(workspace_root: &Path) -> Option<String> {
    let contents = fs::read_to_string(workspace_root.join(".env")).ok()?;
    contents.lines().find_map(parse_api_key_line)
}

fn parse_api_key_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    if key.trim() != API_KEY_ENV {
        return None;
    }

    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value);

    if value.is_empty() { None } else { Some(value.to_string()) }
}

/// Model information from `GET /v1/models/info`.
///
/// TODO: display this in the model picker/status UI instead of keeping model
/// capability knowledge only in docs and fixtures.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub base_model: BaseModel,
    pub capabilities: Capabilities,
    #[serde(default)]
    pub benchmarks: serde_json::Value,
}

/// Base model descriptor.
///
/// TODO: surface provider/family/base model labels in model metadata display.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BaseModel {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oss_base: Option<String>,
}

/// Model capabilities.
///
/// TODO: show context window, recommended token cap, tool support, and
/// reasoning support when the user inspects or switches models.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    pub max_completion_tokens: u64,
    pub recommended_max_tokens: u64,
    pub context_window: u64,
    pub supports_vision: serde_json::Value,
    pub supports_tools: bool,
    pub reasoning: Reasoning,
}

/// Reasoning configuration.
///
/// TODO: display available reasoning levels once model metadata is part of the
/// model selection UI.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reasoning {
    pub supported: bool,
    pub can_disable: bool,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(default)]
    pub default_level: Option<String>,
}

fn model_metadata_display_todo_marker() {
    // TODO: display `/v1/models/info` metadata in the model picker/status UI.
    let _ = UmansClient::fetch_models_info;
    let _ = std::mem::size_of::<ModelInfo>();
    let _ = std::mem::size_of::<BaseModel>();
    let _ = std::mem::size_of::<Capabilities>();
    let _ = std::mem::size_of::<Reasoning>();
}

/// A structured content block in the Anthropic Messages API format.
///
/// The API allows a message's `content` to be either a plain string or an
/// array of typed blocks. Tool use requires `tool_use` blocks (from the
/// assistant) and `tool_result` blocks (in the following user message).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// A plain text block.
    Text { text: String },
    /// A tool-use request emitted by the assistant.
    ToolUse {
        /// Provider-assigned id (e.g. `toolu_01`), echoed back in `tool_result`.
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool result returned to the model in a `user`-role message.
    ToolResult {
        /// Must match the `id` of the originating `tool_use` block.
        tool_use_id: String,
        content: String,
        /// `"ok"` or `"error"`. Umans/Anthropic accept `is_error` as a bool;
        /// we send the string form for compatibility.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// The `content` field of a [`Message`]: either a plain string or an array
/// of structured [`ContentBlock`]s.
///
/// Serialized as a bare string when only text is present, and as a JSON array
/// when blocks are used, matching the Anthropic Messages API.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageContent {
    /// Plain string content (text-only messages).
    Text(String),
    /// Structured content blocks (tool_use / tool_result / mixed).
    Blocks(Vec<ContentBlock>),
}

impl Serialize for MessageContent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MessageContent::Text(s) => serializer.serialize_str(s),
            MessageContent::Blocks(blocks) => blocks.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for MessageContent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::String(s) => Ok(MessageContent::Text(s)),
            serde_json::Value::Array(arr) => {
                let blocks: Vec<ContentBlock> =
                    serde_json::from_value(serde_json::Value::Array(arr)).map_err(serde::de::Error::custom)?;
                Ok(MessageContent::Blocks(blocks))
            }
            _ => Err(serde::de::Error::custom("expected string or array for message content")),
        }
    }
}

impl MessageContent {
    /// Return the concatenated text of all `Text` blocks, or the plain string.
    ///
    /// `tool_use` and `tool_result` blocks are ignored; this is intended for
    /// debug display and legacy callers that only care about text.
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// A message in the Anthropic Messages API format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Message { role: "user".to_string(), content: MessageContent::Text(content.to_string()) }
    }

    pub fn assistant(content: &str) -> Self {
        Message { role: "assistant".to_string(), content: MessageContent::Text(content.to_string()) }
    }

    /// Create a `user`-role message containing one `tool_result` block.
    pub fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        Message {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(is_error),
            }]),
        }
    }

    /// Create an `assistant`-role message from content blocks.
    pub fn assistant_blocks(blocks: Vec<ContentBlock>) -> Self {
        Message { role: "assistant".to_string(), content: MessageContent::Blocks(blocks) }
    }

    /// Return the concatenated text content of this message.
    pub fn as_text(&self) -> String {
        self.content.as_text()
    }
}

/// A parsed SSE event from the streaming response.
#[derive(Clone, Debug, PartialEq)]
pub enum SseEvent {
    /// `event: message_start`
    MessageStart,
    /// `event: content_block_start` with content type `thinking`.
    ThinkingStart,
    /// `event: content_block_start` with content type `text`.
    TextStart,
    /// `event: content_block_delta` with a thinking delta.
    ThinkingDelta(String),
    /// `event: content_block_delta` with a text delta.
    TextDelta(String),
    /// `event: content_block_delta` with a partial tool input JSON delta.
    InputJsonDelta { index: usize, partial_json: String },
    /// `event: content_block_stop`
    ContentBlockStop { index: Option<usize> },
    /// `event: message_delta` with stop reason.
    MessageDelta { stop_reason: Option<String> },
    /// `event: message_stop`
    MessageStop,
    /// `event: error`
    Error(String),
    /// An unhandled event type.
    Other(String),
}

/// Parse a single SSE line pair into an [`SseEvent`].
///
/// SSE format: lines starting with `event:` give the event type, lines starting
/// with `data:` give the JSON payload.
///
/// This function takes the event type and the data JSON string.
pub fn parse_sse_event(event_type: &str, data: &str) -> SseEvent {
    match event_type {
        "message_start" => SseEvent::MessageStart,
        "content_block_start" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let content_type = v
                .get("content_block")
                .and_then(|cb| cb.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match content_type {
                "thinking" => SseEvent::ThinkingStart,
                "text" => SseEvent::TextStart,
                _ => SseEvent::Other(format!("content_block_start: {content_type}")),
            }
        }
        "content_block_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let index = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let delta_type = v
                .get("delta")
                .and_then(|d| d.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("");
            match delta_type {
                "thinking_delta" => {
                    let text = v
                        .get("delta")
                        .and_then(|d| d.get("thinking"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    SseEvent::ThinkingDelta(text)
                }
                "text_delta" => {
                    let text = v
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .replace("<think>", "")
                        .replace("</think>", "");
                    SseEvent::TextDelta(text)
                }
                "input_json_delta" => {
                    let partial_json = v
                        .get("delta")
                        .and_then(|d| d.get("partial_json"))
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    SseEvent::InputJsonDelta { index, partial_json }
                }
                _ => SseEvent::Other(format!("content_block_delta: {delta_type}")),
            }
        }
        "content_block_stop" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            SseEvent::ContentBlockStop { index: v.get("index").and_then(|i| i.as_u64()).map(|i| i as usize) }
        }
        "message_delta" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let stop_reason = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            SseEvent::MessageDelta { stop_reason }
        }
        "message_stop" => SseEvent::MessageStop,
        "error" => {
            let v: serde_json::Value = serde_json::from_str(data).unwrap_or(serde_json::Value::Null);
            let msg = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            SseEvent::Error(msg)
        }
        other => SseEvent::Other(other.to_string()),
    }
}

/// Convert an [`SseEvent`] into an [`AgentEvent`].
///
/// `TextDelta` → `AssistantDelta`, `ThinkingDelta` → `ReasoningDelta`,
/// `Error` → `Failed`.
///
/// `MessageStop` is intentionally not converted here. Only the agent loop
/// knows whether a provider turn ended because the assistant is done or
/// because tool calls must be dispatched and fed back.
impl From<&SseEvent> for Option<AgentEvent> {
    fn from(event: &SseEvent) -> Self {
        match event {
            SseEvent::MessageStart => Some(AgentEvent::Started),
            SseEvent::TextDelta(text) => Some(AgentEvent::AssistantDelta(text.clone())),
            SseEvent::ThinkingDelta(text) => Some(AgentEvent::ReasoningDelta(text.clone())),
            SseEvent::Error(msg) => Some(AgentEvent::Failed(msg.clone())),
            _ => None,
        }
    }
}

/// Convenience wrapper for `Option<AgentEvent>::from(&sse_event)`.
pub fn sse_to_agent_event(event: &SseEvent) -> Option<AgentEvent> {
    event.into()
}

/// Parse a raw SSE chunk (multiple lines) and extract any event/data pairs.
///
/// Returns a list of `(event_type, data)` tuples. Lines starting with `event:`
/// set the event type; lines starting with `data:` provide the JSON payload.
/// A blank line delimits events.
pub fn parse_sse_chunk(chunk: &str) -> Vec<(String, String)> {
    let mut events = Vec::new();
    let mut current_event = String::new();
    let mut current_data = String::new();

    for line in chunk.lines() {
        if line.is_empty() {
            if !current_event.is_empty() {
                events.push((current_event.clone(), current_data.clone()));
            }
            current_event.clear();
            current_data.clear();
            continue;
        }
        if let Some(rest) = line.strip_prefix("event: ") {
            current_event = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            if current_data.is_empty() {
                current_data = rest.to_string();
            } else {
                current_data.push('\n');
                current_data.push_str(rest);
            }
        }
    }

    if !current_event.is_empty() {
        events.push((current_event, current_data));
    }

    events
}

/// Convert an [`UmansError`] into an [`AgentEvent::Failed`] with a
/// human-readable message.
///
/// HTTP status errors include the status code.
///
/// Auth errors (401/403) and rate-limit errors (429) are labeled distinctly.
pub fn error_to_agent_event(err: &UmansError) -> AgentEvent {
    let msg = match err {
        UmansError::MissingApiKey => "UMANS_API_KEY is not set".to_string(),
        UmansError::Status { code, body } => match code {
            401 | 403 => format!("authentication failed (HTTP {code})"),
            429 => "rate limit exceeded".to_string(),
            500..=599 => format!("server error (HTTP {code}): {body}"),
            _ => format!("HTTP {code}: {body}"),
        },
        UmansError::Http(e) => format!("network error: {e}"),
        UmansError::Json(e) => format!("response parse error: {e}"),
    };
    AgentEvent::Failed(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cli::WebSearchMode, providers};

    #[test]
    fn build_messages_request_body_has_required_fields() {
        let messages = vec![Message::user("Hello!")];
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 4096, true, None);
        assert_eq!(body["model"], "umans-coder");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello!");
        assert!(body.get("tools").is_none(), "no tools when None passed");
    }

    #[test]
    fn build_messages_request_body_non_stream() {
        let messages = vec![Message::user("test")];
        let body = UmansClient::build_messages_request_body("umans-glm-5.2", &messages, 8192, false, None);
        assert_eq!(body["model"], "umans-glm-5.2");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_messages_request_body_includes_tools() {
        let messages = vec![Message::user("find files")];
        let tools = serde_json::json!([{
            "name": "find_files",
            "description": "locate files",
            "input_schema": {"type": "object"}
        }]);
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 4096, true, Some(&tools));
        assert_eq!(
            body["tools"][0]["name"], "find_files",
            "tools should be included when provided"
        );
    }

    #[test]
    fn build_messages_request_body_omits_empty_tools() {
        let messages = vec![Message::user("hi")];
        let empty_tools = serde_json::json!([]);
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 4096, true, Some(&empty_tools));
        assert!(body.get("tools").is_none(), "empty tool array should be omitted");
    }

    #[test]
    fn max_tokens_for_model_matches_model_guidance() {
        assert_eq!(max_tokens_for_model("umans-coder"), 32_768);
        assert_eq!(max_tokens_for_model("umans-glm-5.2"), 131_071);
        assert_eq!(max_tokens_for_model("umans-minimax-m2.5"), 8_192);
    }

    #[test]
    fn build_headers_include_api_key_and_version() {
        let client = UmansClient::new(BASE_URL, "sk-test-key");
        let headers = client.build_headers(WebSearchMode::Native);
        let header_map: HashMap<String, String> = headers.into_iter().collect();
        assert_eq!(header_map.get("x-api-key").unwrap(), "sk-test-key");
        assert_eq!(header_map.get("anthropic-version").unwrap(), ANTHROPIC_VERSION);
        assert_eq!(header_map.get("Content-Type").unwrap(), "application/json");
        assert_eq!(header_map.get(WEBSEARCH_HEADER).unwrap(), "native");
    }

    #[test]
    fn build_headers_websearch_varies_by_mode() {
        let client = UmansClient::new(BASE_URL, "sk-test-key");
        let native_headers = client.build_headers(WebSearchMode::Native);
        let native_map: HashMap<String, String> = native_headers.into_iter().collect();
        assert_eq!(native_map.get(WEBSEARCH_HEADER).unwrap(), "native");

        let exa_headers = client.build_headers(WebSearchMode::Exa);
        let exa_map: HashMap<String, String> = exa_headers.into_iter().collect();
        assert_eq!(exa_map.get(WEBSEARCH_HEADER).unwrap(), "exa");

        let none_headers = client.build_headers(WebSearchMode::None);
        let none_map: HashMap<String, String> = none_headers.into_iter().collect();
        assert_eq!(none_map.get(WEBSEARCH_HEADER).unwrap(), "none");
    }

    #[test]
    fn from_env_or_dotenv_missing_key_returns_error() {
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let result = UmansClient::from_env_or_dotenv(dir.path());
        assert!(matches!(result, Err(UmansError::MissingApiKey)));
    }

    #[test]
    fn from_env_or_dotenv_reads_workspace_env_file() {
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "UMANS_API_KEY=sk-dotenv-key\n").unwrap();

        let client = UmansClient::from_env_or_dotenv(dir.path()).unwrap();
        let headers: HashMap<String, String> = client.build_headers(WebSearchMode::Native).into_iter().collect();

        assert_eq!(headers.get("x-api-key").unwrap(), "sk-dotenv-key");
    }

    #[test]
    fn from_env_or_dotenv_reads_exported_quoted_env_file_value() {
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "export UMANS_API_KEY=\"sk-quoted-dotenv-key\"\n",
        )
        .unwrap();

        let client = UmansClient::from_env_or_dotenv(dir.path()).unwrap();
        let headers: HashMap<String, String> = client.build_headers(WebSearchMode::Native).into_iter().collect();

        assert_eq!(headers.get("x-api-key").unwrap(), "sk-quoted-dotenv-key");
    }

    #[test]
    fn parse_models_info_fixture() {
        let json = r#"{
            "umans-coder": {
                "name": "umans-coder",
                "display_name": "Umans Coder",
                "description": "Recommended model.",
                "base_model": {"name": "kimi-k2.7-code", "provider": "Moonshot", "oss_base": "Kimi K2.7-Code"},
                "capabilities": {
                    "max_completion_tokens": 262144,
                    "recommended_max_tokens": 32768,
                    "context_window": 262144,
                    "supports_vision": true,
                    "supports_tools": true,
                    "reasoning": {"supported": true, "can_disable": false, "levels": [], "default_level": null}
                },
                "benchmarks": {}
            },
            "umans-glm-5.2": {
                "name": "umans-glm-5.2",
                "display_name": "Umans GLM 5.2",
                "description": "Latest GLM.",
                "base_model": {"name": "GLM-5.2", "family": "GLM"},
                "capabilities": {
                    "max_completion_tokens": 131072,
                    "recommended_max_tokens": 131071,
                    "context_window": 405504,
                    "supports_vision": "via-handoff",
                    "supports_tools": true,
                    "reasoning": {"supported": true, "can_disable": true, "levels": ["none", "high", "max"], "default_level": "high"}
                },
                "benchmarks": {}
            }
        }"#;

        let models: HashMap<String, ModelInfo> = serde_json::from_str(json).expect("parse");
        let coder = models.get("umans-coder").expect("umans-coder present");
        assert_eq!(coder.display_name, "Umans Coder");
        assert_eq!(coder.capabilities.context_window, 262144);
        assert_eq!(coder.capabilities.recommended_max_tokens, 32768);
        assert!(coder.capabilities.supports_tools);
        assert!(coder.capabilities.reasoning.supported);
        assert!(!coder.capabilities.reasoning.can_disable);

        let glm = models.get("umans-glm-5.2").expect("umans-glm-5.2 present");
        assert_eq!(glm.capabilities.context_window, 405504);
        assert!(glm.capabilities.reasoning.can_disable);
        assert_eq!(glm.capabilities.reasoning.default_level.as_deref(), Some("high"));
        assert_eq!(glm.capabilities.reasoning.levels, vec!["none", "high", "max"]);
    }

    #[test]
    fn parse_models_info_from_live_fixture() {
        let json = include_str!("./fixtures/model_info.json");
        let models: HashMap<String, ModelInfo> = serde_json::from_str(json).expect("parse");
        let coder = &models["umans-coder"];
        assert_eq!(coder.capabilities.context_window, 262144);
    }

    #[test]
    fn parse_sse_event_text_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let event = parse_sse_event("content_block_delta", data);
        assert_eq!(event, SseEvent::TextDelta("Hello".to_string()));
    }

    #[test]
    fn parse_sse_event_text_delta_strips_think_tags() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"</think>\nDone"}}"#;
        let event = parse_sse_event("content_block_delta", data);
        assert_eq!(event, SseEvent::TextDelta("\nDone".to_string()));
    }

    #[test]
    fn parse_sse_event_thinking_delta() {
        let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think..."}}"#;
        let event = parse_sse_event("content_block_delta", data);
        assert_eq!(event, SseEvent::ThinkingDelta("Let me think...".to_string()));
    }

    #[test]
    fn parse_sse_event_message_start() {
        let event = parse_sse_event("message_start", "{}");
        assert_eq!(event, SseEvent::MessageStart);
    }

    #[test]
    fn parse_sse_event_content_block_start_thinking() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;
        let event = parse_sse_event("content_block_start", data);
        assert_eq!(event, SseEvent::ThinkingStart);
    }

    #[test]
    fn parse_sse_event_content_block_start_text() {
        let data = r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        let event = parse_sse_event("content_block_start", data);
        assert_eq!(event, SseEvent::TextStart);
    }

    #[test]
    fn parse_sse_event_message_stop() {
        let event = parse_sse_event("message_stop", "{}");
        assert_eq!(event, SseEvent::MessageStop);
    }

    #[test]
    fn parse_sse_event_input_json_delta() {
        let data = serde_json::json!({
            "type": "content_block_delta",
            "index": 1,
            "delta": {
                "type": "input_json_delta",
                "partial_json": "{\"pattern\""
            }
        })
        .to_string();
        let event = parse_sse_event("content_block_delta", &data);
        assert_eq!(
            event,
            SseEvent::InputJsonDelta { index: 1, partial_json: "{\"pattern\"".to_string() }
        );
    }

    #[test]
    fn parse_sse_event_content_block_stop_with_index() {
        let data = r#"{"type":"content_block_stop","index":2}"#;
        let event = parse_sse_event("content_block_stop", data);
        assert_eq!(event, SseEvent::ContentBlockStop { index: Some(2) });
    }

    #[test]
    fn parse_sse_event_error() {
        let data = r#"{"type":"error","error":{"type":"overloaded_error","message":"Server overloaded"}}"#;
        let event = parse_sse_event("error", data);
        assert_eq!(event, SseEvent::Error("Server overloaded".to_string()));
    }

    #[test]
    fn parse_sse_event_message_delta_with_stop_reason() {
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#;
        let event = parse_sse_event("message_delta", data);
        assert_eq!(
            event,
            SseEvent::MessageDelta { stop_reason: Some("end_turn".to_string()) }
        );
    }

    #[test]
    fn sse_to_agent_text_delta() {
        let event = SseEvent::TextDelta("response".to_string());
        assert_eq!(
            sse_to_agent_event(&event),
            Some(AgentEvent::AssistantDelta("response".to_string()))
        );
    }

    #[test]
    fn sse_to_agent_thinking_delta() {
        let event = SseEvent::ThinkingDelta("reasoning".to_string());
        assert_eq!(
            sse_to_agent_event(&event),
            Some(AgentEvent::ReasoningDelta("reasoning".to_string()))
        );
    }

    #[test]
    fn sse_to_agent_message_start() {
        assert_eq!(sse_to_agent_event(&SseEvent::MessageStart), Some(AgentEvent::Started));
    }

    #[test]
    fn sse_to_agent_message_stop() {
        assert_eq!(sse_to_agent_event(&SseEvent::MessageStop), None);
    }

    #[test]
    fn sse_to_agent_error() {
        assert_eq!(
            sse_to_agent_event(&SseEvent::Error("fail".to_string())),
            Some(AgentEvent::Failed("fail".to_string()))
        );
    }

    #[test]
    fn sse_to_agent_content_block_start_no_event() {
        assert_eq!(sse_to_agent_event(&SseEvent::TextStart), None);
        assert_eq!(sse_to_agent_event(&SseEvent::ThinkingStart), None);
    }

    #[test]
    fn parse_sse_chunk_multiple_events() {
        let chunk = "event: message_start\ndata: {}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\nevent: message_stop\ndata: {}\n\n";
        let events = parse_sse_chunk(chunk);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].0, "message_start");
        assert_eq!(events[1].0, "content_block_start");
        assert_eq!(events[2].0, "content_block_delta");
        assert_eq!(events[3].0, "message_stop");
    }

    #[test]
    fn parse_sse_chunk_empty() {
        let events = parse_sse_chunk("");
        assert!(events.is_empty());
    }

    #[test]
    fn parse_sse_chunk_no_trailing_newline() {
        let chunk = "event: message_stop\ndata: {}";
        let events = parse_sse_chunk(chunk);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "message_stop");
    }

    #[test]
    fn parse_full_stream_fixture_into_agent_events() {
        let sse = include_str!("./fixtures/stream_response.sse");
        let chunks = parse_sse_chunk(sse);
        let agent_events: Vec<AgentEvent> = chunks
            .iter()
            .filter_map(|(event_type, data)| {
                let sse_event = parse_sse_event(event_type, data);
                sse_to_agent_event(&sse_event)
            })
            .collect();

        assert_eq!(agent_events.len(), 3);
        assert_eq!(agent_events[0], AgentEvent::Started);
        assert_eq!(agent_events[1], AgentEvent::ReasoningDelta("Analyzing...".to_string()));
        assert_eq!(agent_events[2], AgentEvent::AssistantDelta("Hello world".to_string()));
    }

    #[test]
    fn error_to_agent_event_missing_api_key() {
        let event = error_to_agent_event(&UmansError::MissingApiKey);
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("UMANS_API_KEY")));
    }

    #[test]
    fn error_to_agent_event_auth_failure() {
        let event = error_to_agent_event(&UmansError::Status { code: 401, body: "Unauthorized".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("authentication failed")));
    }

    #[test]
    fn error_to_agent_event_rate_limit() {
        let event = error_to_agent_event(&UmansError::Status { code: 429, body: "Too Many Requests".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("rate limit")));
    }

    #[test]
    fn error_to_agent_event_server_error() {
        let event = error_to_agent_event(&UmansError::Status { code: 500, body: "Internal Server Error".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("server error")));
    }

    #[test]
    fn error_to_agent_event_network_error() {
        let event = error_to_agent_event(&UmansError::Http("connection refused".into()));
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("network error")));
    }

    #[test]
    fn error_to_agent_event_json_error() {
        let event = error_to_agent_event(&UmansError::Json("bad metadata".into()));
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("response parse error")));
    }

    #[test]
    fn no_network_request_construction() {
        let messages = vec![Message::user("test"), Message::assistant("response")];
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 8192, true, None);
        assert_eq!(body["model"], "umans-coder");
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn no_network_stream_parsing_from_fixture() {
        let sse = include_str!("./fixtures/stream_response.sse");
        let chunks = parse_sse_chunk(sse);
        assert!(!chunks.is_empty());

        let events: Vec<SseEvent> = chunks.iter().map(|(et, data)| parse_sse_event(et, data)).collect();
        assert!(events.iter().any(|e| matches!(e, SseEvent::ThinkingDelta(_))));
        assert!(events.iter().any(|e| matches!(e, SseEvent::TextDelta(_))));
        assert!(events.contains(&SseEvent::MessageStart));
        assert!(events.contains(&SseEvent::MessageStop));
    }

    #[test]
    fn no_network_metadata_parsing_from_fixture() {
        let json = include_str!("./fixtures/model_info.json");
        let models: HashMap<String, ModelInfo> = serde_json::from_str(json).expect("parse");
        assert!(models.contains_key("umans-coder"));

        let coder = &models["umans-coder"];
        assert!(coder.capabilities.supports_tools);
        assert!(!coder.capabilities.reasoning.can_disable);
    }

    #[test]
    #[ignore = "requires UMANS_API_KEY and network access"]
    fn live_smoke_test_models_info() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = UmansClient::from_env_or_dotenv(&workspace_root).expect("UMANS_API_KEY must be set");
        let models = client.fetch_models_info().expect("fetch models info");
        assert!(models.contains_key("umans-coder"));
    }

    #[test]
    fn message_user_constructor() {
        let msg = Message::user("test prompt");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.as_text(), "test prompt");
    }

    #[test]
    fn message_assistant_constructor() {
        let msg = Message::assistant("response");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.as_text(), "response");
    }

    #[test]
    fn tool_result_message_has_correct_shape() {
        let msg = Message::tool_result("toolu_01", "found 2 files", false);
        assert_eq!(msg.role, "user");

        match &msg.content {
            providers::umans::MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    providers::umans::ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        assert_eq!(tool_use_id, "toolu_01");
                        assert_eq!(content, "found 2 files");
                        assert_eq!(*is_error, Some(false));
                    }
                    other => panic!("expected ToolResult, got {other:?}"),
                }
            }
            other => panic!("expected Blocks, got {other:?}"),
        }
    }

    #[test]
    fn assistant_blocks_message_serializes_tool_use() {
        let blocks = vec![providers::umans::ContentBlock::ToolUse {
            id: "toolu_01".to_string(),
            name: "find_files".to_string(),
            input: serde_json::json!({"pattern": "Cargo"}),
        }];
        let msg = Message::assistant_blocks(blocks);
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["role"], "assistant");

        let content = &json["content"];
        assert!(content.is_array(), "content should be an array for blocks");
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "toolu_01");
        assert_eq!(content[0]["name"], "find_files");
        assert_eq!(content[0]["input"]["pattern"], "Cargo");
    }

    #[test]
    fn text_message_content_serializes_as_string() {
        let msg = Message::user("hello");
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["content"], "hello");
        assert!(json["content"].is_string(), "text content should serialize as a string");
    }

    #[test]
    fn tool_result_block_serializes_with_is_error() {
        let msg = Message::tool_result("toolu_02", "command failed", true);
        let json = serde_json::to_value(&msg).expect("serialize");
        let block = &json["content"][0];
        assert_eq!(block["type"], "tool_result");
        assert_eq!(block["tool_use_id"], "toolu_02");
        assert_eq!(block["content"], "command failed");
        assert_eq!(block["is_error"], true);
    }

    #[test]
    fn assistant_blocks_with_text_and_tool_use_serializes_in_order() {
        let blocks = vec![
            providers::umans::ContentBlock::Text { text: "Let me search.".to_string() },
            providers::umans::ContentBlock::ToolUse {
                id: "toolu_03".to_string(),
                name: "search_text".to_string(),
                input: serde_json::json!({"pattern": "fn main"}),
            },
        ];
        let msg = Message::assistant_blocks(blocks);
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Let me search.");
        assert_eq!(json["content"][1]["type"], "tool_use");
        assert_eq!(json["content"][1]["id"], "toolu_03");
    }

    #[test]
    fn message_content_round_trips_through_json() {
        let original = Message::tool_result("toolu_99", "result text", false);
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: Message = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }
}
