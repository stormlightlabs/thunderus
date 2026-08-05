//! Provider implementations.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::app::AgentEvent;
use crate::cli::{ReasoningEffort, ReasoningSummary};
use crate::tools::ToolUseRequest;

/// Bound DNS resolution and TCP/TLS setup so a disconnected provider cannot
/// hold an agent run indefinitely.
pub const PROVIDER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Bound request transmission, including request bodies such as tool schemas.
pub const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Bound the wait for a provider to begin its HTTP response.
pub const PROVIDER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Bound one stalled read from a streaming provider response.
///
/// This is deliberately not a global request deadline: active SSE streams may
/// continue longer than this interval as long as they keep producing data.
pub const PROVIDER_STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

pub mod anthropic;
pub mod codex;
pub mod openai;
pub mod opencode;

pub use codex as chatgpt_codex;
pub use opencode::zen as opencode_zen;

pub type Result<T> = std::result::Result<T, ProviderError>;

/// Create the shared HTTP transport configuration for provider clients.
///
/// `ureq` applies `timeout_recv_body` to each blocking body read. That gives
/// streaming responses a finite inactivity timeout without imposing a total
/// lifetime on an otherwise healthy SSE response.
pub fn provider_http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_resolve(Some(PROVIDER_CONNECT_TIMEOUT))
        .timeout_connect(Some(PROVIDER_CONNECT_TIMEOUT))
        .timeout_send_request(Some(PROVIDER_REQUEST_TIMEOUT))
        .timeout_send_body(Some(PROVIDER_REQUEST_TIMEOUT))
        .timeout_recv_response(Some(PROVIDER_RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(PROVIDER_STREAM_IDLE_TIMEOUT))
        .build()
        .new_agent()
}

/// Return the reasoning controls that can be safely selected for `model`.
///
/// `Auto` is always available. Provider modules intentionally own the
/// model-specific compatibility rules rather than relying on a global list.
pub fn reasoning_options(model: &str) -> Vec<ReasoningEffort> {
    if opencode::is_zen_model_id(model) {
        opencode::zen::reasoning_options(model)
    } else if opencode::is_go_model_id(model) {
        vec![ReasoningEffort::Auto]
    } else if codex::is_model_id(model) {
        codex::reasoning_options(model)
    } else {
        vec![ReasoningEffort::Auto]
    }
}

/// Return whether a requested reasoning control is valid for `model`.
pub fn reasoning_option_is_supported(model: &str, effort: ReasoningEffort) -> bool {
    reasoning_options(model).contains(&effort)
}

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
    fn request_status(&self, model: &str) -> String;
    fn from_env_or_dotenv(root: &Path) -> Result<Self>;
    fn load_metadata(&self) -> Result<Self::Metadata>;
    fn metadata_loaded_event(&self, _metadata: &Self::Metadata) -> Option<AgentEvent> {
        None
    }
    fn metadata_status(&self, _model: &str, _metadata: &Self::Metadata) -> Option<String> {
        None
    }
    fn token_budget(&self, model: &str, metadata: Option<&Self::Metadata>) -> u32;
    /// Serialize exactly the request body that the adapter sends.
    fn serialized_request_body(
        &self, model: &str, messages: &[ProviderMessage], request: &StreamingRequest<'_>,
    ) -> Result<Vec<u8>>;
    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], request: &StreamingRequest<'_>,
    ) -> Result<ureq::http::Response<ureq::Body>>;
    fn stream_format(&self, model: &str) -> Result<StreamFormat>;
    fn request_error_message(error: &ProviderError) -> String;
    fn is_retryable_request_error(error: &ProviderError) -> bool;
}

/// Serialize a provider request body at the accounting boundary.
pub fn serialize_request_body(body: &serde_json::Value) -> Result<Vec<u8>> {
    serde_json::to_vec(&body).map_err(|error| ProviderError::Json(error.to_string()))
}

/// Provider-private continuation state retained only for one active agent run.
///
/// This is intentionally crate-private: response items can contain encrypted
/// provider data and must not become a library or session-file contract.
#[derive(Clone, Debug, Default)]
pub enum ProviderContinuation {
    #[default]
    None,
    Responses {
        items: Vec<serde_json::Value>,
        consumed_messages: usize,
    },
}

impl ProviderContinuation {
    pub fn responses_items(&self) -> Option<(&[serde_json::Value], usize)> {
        match self {
            Self::None => None,
            Self::Responses { items, consumed_messages } => Some((items, *consumed_messages)),
        }
    }

    pub fn set_responses_items(&mut self, items: Vec<serde_json::Value>, consumed_messages: usize) {
        *self = Self::Responses { items, consumed_messages };
    }
}

/// Per-turn settings passed to an internal streaming provider request.
pub struct StreamingRequest<'a> {
    pub max_tokens: u32,
    /// Model-specific reasoning control, validated by the provider boundary.
    pub reasoning_effort: ReasoningEffort,
    /// Whether supporting providers should return reasoning summaries.
    pub reasoning_summary: ReasoningSummary,
    pub tools: &'a serde_json::Value,
    pub continuation: &'a ProviderContinuation,
}

/// Shared provider request error shape.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Required API key environment variable is not set.
    #[error("{env} is not set; run `thndrs setup` or store the credential with `thndrs login`")]
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
    /// Authentication error before a provider request can be sent.
    #[error("authentication failed: {0}")]
    Auth(String),
    /// Credential verification could not complete because a dependency is unavailable.
    #[error("authentication verification unavailable: {0}")]
    AuthUnavailable(String),
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

    /// Whether the provider explicitly rejected the current credential.
    pub fn is_credential_rejected(&self) -> bool {
        matches!(
            self,
            ProviderError::Status { code: 401 | 403, .. } | ProviderError::Auth(_)
        )
    }

    pub fn is_retryable(&self) -> bool {
        match self {
            ProviderError::MissingApiKey { .. }
            | ProviderError::InvalidModelId { .. }
            | ProviderError::Auth(_)
            | ProviderError::Json(_) => false,
            ProviderError::AuthUnavailable(_) => true,
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
            ProviderError::Auth(message) => format!("authentication failed: {message}"),
            ProviderError::AuthUnavailable(message) => format!("authentication verification unavailable: {message}"),
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
    ChatGptCodexResponses,
}

/// A structured content block in the provider-neutral message format.
///
/// Provider routes can use this directly or convert it at their request
/// boundary. The agent loop does not depend on a provider wire format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderContentBlock {
    /// A plain text block.
    Text { text: String },
    /// Base64-encoded image content for providers that support vision inputs.
    Image { source: ProviderImageSource },
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
        /// Anthropic-compatible APIs accept `is_error` as a bool.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

/// Provider-neutral image source payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderImageSource {
    /// Base64 image data and its media type.
    Base64 { media_type: String, data: String },
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
            agent: provider_http_agent(),
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

    /// Create a `user`-role message from content blocks.
    pub fn user_blocks(blocks: Vec<ProviderContentBlock>) -> Self {
        ProviderMessage { role: "user".to_string(), content: ProviderMessageContent::Blocks(blocks) }
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
    pub response_items: Vec<serde_json::Value>,
    /// Provider usage accumulated across stream updates, if reported.
    pub usage: Option<thndrs_agent::ProviderUsageComponents>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_http_client_bounds_setup_and_streaming_idle_timeouts() {
        let client = ProviderHttpClient::new("https://provider.example", "test-key");
        let timeouts = client.agent().config().timeouts();

        assert_eq!(
            timeouts.global, None,
            "SSE streams must not have a total lifetime deadline"
        );
        assert_eq!(timeouts.per_call, None, "SSE streams must not have a per-call deadline");
        assert_eq!(timeouts.resolve, Some(PROVIDER_CONNECT_TIMEOUT));
        assert_eq!(timeouts.connect, Some(PROVIDER_CONNECT_TIMEOUT));
        assert_eq!(timeouts.send_request, Some(PROVIDER_REQUEST_TIMEOUT));
        assert_eq!(timeouts.send_body, Some(PROVIDER_REQUEST_TIMEOUT));
        assert_eq!(timeouts.recv_response, Some(PROVIDER_RESPONSE_TIMEOUT));
        assert_eq!(timeouts.recv_body, Some(PROVIDER_STREAM_IDLE_TIMEOUT));
    }
}
