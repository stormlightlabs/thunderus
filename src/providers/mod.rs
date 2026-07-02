//! Provider implementations.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app::AgentEvent;
use crate::tools::ToolUseRequest;

pub mod anthropic;
pub mod openai;
pub mod opencode;
pub mod umans;

pub type Result<T> = std::result::Result<T, ProviderError>;

/// Minimal provider trait used by the agent loop.
///
/// This trait intentionally stops at provider concerns: auth, metadata,
/// request dispatch, retry/error policy, and which stream format to parse.
///
/// The agent loop still owns cancellation, tool dispatch, and transcript events.
pub trait StreamingProvider: Sized {
    type Metadata;

    fn name(&self) -> &'static str;
    fn load_status(&self) -> String;
    fn request_status(&self, model: &str, search_mode: crate::cli::WebSearchMode) -> String;
    fn from_env_or_dotenv(root: &Path) -> Result<Self>;
    fn load_metadata(&self) -> Result<Self::Metadata>;
    fn metadata_loaded_event(&self, _metadata: &Self::Metadata) -> Option<AgentEvent> {
        None
    }
    fn metadata_status(&self, _model: &str, _metadata: &Self::Metadata) -> Option<String> {
        None
    }
    fn token_budget(&self, model: &str, metadata: Option<&Self::Metadata>) -> u32;
    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], max_tokens: u32, search_mode: crate::cli::WebSearchMode,
        tools: &serde_json::Value,
    ) -> Result<ureq::http::Response<ureq::Body>>;
    fn stream_format(&self, model: &str) -> Result<StreamFormat>;
    fn request_error_message(error: &ProviderError) -> String;
    fn is_retryable_request_error(error: &ProviderError) -> bool;
}

/// Shared provider request error shape.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Required API key environment variable is not set.
    #[error("{env} is not set")]
    MissingApiKey { env: &'static str },
    /// The selected model id is not valid for this provider.
    #[error("{provider} model ids must use {prefix}<model-id>, got {model}")]
    InvalidModelId {
        provider: &'static str,
        prefix: &'static str,
        model: String,
    },
    /// HTTP transport error.
    #[error("http error: {0}")]
    Http(String),
    /// HTTP status error (non-2xx response).
    #[error("HTTP {code}: {body}")]
    Status { code: u16, body: String },
    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(String),
}

impl ProviderError {
    pub fn missing_api_key(env: &'static str) -> Self {
        ProviderError::MissingApiKey { env }
    }

    pub fn invalid_model_id(provider: &'static str, prefix: &'static str, model: &str) -> Self {
        ProviderError::InvalidModelId { provider, prefix, model: model.to_string() }
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            ProviderError::MissingApiKey { .. } | ProviderError::InvalidModelId { .. } | ProviderError::Json(_) => {
                false
            }
            ProviderError::Status { code, .. } => *code == 429 || (500..=599).contains(code),
            ProviderError::Http(message) => {
                let lower = message.to_ascii_lowercase();
                !lower.contains("cancel") && !lower.contains("abort")
            }
        }
    }

    pub fn failure_message(&self, rate_limit_message: &str) -> String {
        match self {
            ProviderError::MissingApiKey { .. } | ProviderError::InvalidModelId { .. } => self.to_string(),
            ProviderError::Status { code, body } => match code {
                401 | 403 => format!("authentication failed (HTTP {code})"),
                429 => rate_limit_message.to_string(),
                500..=599 => format!("server error (HTTP {code}): {body}"),
                _ => format!("HTTP {code}: {body}"),
            },
            ProviderError::Http(e) => format!("network error: {e}"),
            ProviderError::Json(e) => format!("response parse error: {e}"),
        }
    }
}

/// Streaming wire format used by a provider request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFormat {
    AnthropicMessages,
    OpenAiChat,
}

/// A structured content block in the provider-neutral Anthropic-style message
/// format.
///
/// New provider routes can either use this directly or convert from it at their
/// boundary. Umans currently sends it to `/v1/messages`; a future OpenAI
/// compatible route should convert this shape into chat-completions messages
/// instead of mixing both wire formats in the agent loop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderContentBlock {
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
        /// Umans/Anthropic accept `is_error` as a bool.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Message content: either a plain string or structured content blocks.
#[derive(Clone, Debug, PartialEq)]
pub enum ProviderMessageContent {
    /// Plain string content.
    Text(String),
    /// Structured content blocks.
    Blocks(Vec<ProviderContentBlock>),
}

impl Serialize for ProviderMessageContent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ProviderMessageContent::Text(s) => serializer.serialize_str(s),
            ProviderMessageContent::Blocks(blocks) => blocks.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ProviderMessageContent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::String(s) => Ok(ProviderMessageContent::Text(s)),
            serde_json::Value::Array(arr) => {
                let blocks: Vec<ProviderContentBlock> =
                    serde_json::from_value(serde_json::Value::Array(arr)).map_err(serde::de::Error::custom)?;
                Ok(ProviderMessageContent::Blocks(blocks))
            }
            _ => Err(serde::de::Error::custom("expected string or array for message content")),
        }
    }
}

impl ProviderMessageContent {
    /// Return the concatenated text of all `Text` blocks, or the plain string.
    pub fn as_text(&self) -> String {
        match self {
            ProviderMessageContent::Text(s) => s.clone(),
            ProviderMessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    ProviderContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

/// Static model entry used by offline model pickers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnownModel {
    pub id: &'static str,
    pub description: &'static str,
}

/// Shared HTTP client state for provider clients.
pub struct ProviderHttpClient {
    base_url: String,
    api_key: String,
    agent: ureq::Agent,
}

impl ProviderHttpClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        ProviderHttpClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn agent(&self) -> &ureq::Agent {
        &self.agent
    }
}

/// Provider-neutral conversation message used by the agent loop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderMessage {
    pub role: String,
    pub content: ProviderMessageContent,
}

impl ProviderMessage {
    pub fn user(content: &str) -> Self {
        ProviderMessage { role: "user".to_string(), content: ProviderMessageContent::Text(content.to_string()) }
    }

    pub fn assistant(content: &str) -> Self {
        ProviderMessage { role: "assistant".to_string(), content: ProviderMessageContent::Text(content.to_string()) }
    }

    /// Create a `user`-role message containing one `tool_result` block.
    pub fn tool_result(tool_use_id: &str, content: &str, is_error: bool) -> Self {
        ProviderMessage {
            role: "user".to_string(),
            content: ProviderMessageContent::Blocks(vec![ProviderContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(is_error),
            }]),
        }
    }

    /// Create an `assistant`-role message from content blocks.
    pub fn assistant_blocks(blocks: Vec<ProviderContentBlock>) -> Self {
        ProviderMessage { role: "assistant".to_string(), content: ProviderMessageContent::Blocks(blocks) }
    }

    /// Return the concatenated text content of this message.
    pub fn as_text(&self) -> String {
        self.content.as_text()
    }
}

/// Provider-neutral result of one streamed model turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderTurn {
    pub tool_requests: Vec<ToolUseRequest>,
    pub assistant_text: String,
    pub stop_reason: Option<String>,
}

pub fn summarize_error_body(body: &str) -> String {
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

pub fn api_key_from_dotenv(root: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(root.join(".env"))
        .ok()?
        .lines()
        .find_map(|line| parse_api_key_line(line, name))
}

fn parse_api_key_line(line: &str, env_name: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    if key.trim() != env_name {
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
