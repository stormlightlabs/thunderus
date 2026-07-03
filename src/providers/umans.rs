//! Umans Code provider — Anthropic-compatible Messages API.
//!
//! Uses `POST /v1/messages` with `x-api-key` and `anthropic-version` headers.
//! Streaming responses arrive as SSE events and parsed into [`AgentEvent`].

use std::collections::HashMap;
use std::env;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::app::AgentEvent;
use crate::cli::WebSearchMode;
use crate::providers::{
    self, KnownModel, ProviderError, ProviderHttpClient, ProviderMessage, Result, StreamFormat, StreamingProvider,
};

/// Umans Code base URL.
pub const BASE_URL: &str = "https://api.code.umans.ai";

/// Required Anthropic version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Header name for the Umans web search provider selector.
pub const WEBSEARCH_HEADER: &str = "X-Umans-Websearch-Provider";

/// Environment variable name for the API key.
pub const API_KEY_ENV: &str = "UMANS_API_KEY";

pub const DEFAULT_RECOMMENDED_MAX_TOKENS: u32 = 32_768;

/// Model information from `GET /v1/models/info`.
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

/// Base model descriptor used by model metadata display.
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

/// Concrete Umans Code API client.
pub struct UmansClient {
    http: ProviderHttpClient,
}

impl UmansClient {
    /// Create a client from `UMANS_API_KEY`, falling back to workspace `.env`.
    pub fn from_env_or_dotenv(workspace_root: &Path) -> Result<Self> {
        match env::var(API_KEY_ENV) {
            Ok(api_key) => {
                tracing::debug!(source = "environment", "loaded Umans API key");
                Ok(Self::new(BASE_URL, &api_key))
            }
            Err(_) => super::api_key_from_dotenv(workspace_root, API_KEY_ENV)
                .map(|api_key| {
                    tracing::debug!(source = ".env", path = %workspace_root.join(".env").display(), "loaded Umans API key");
                    Self::new(BASE_URL, &api_key)
                })
                .ok_or_else(|| {
                    tracing::error!(env = API_KEY_ENV, cwd = %workspace_root.display(), "missing Umans API key");
                    ProviderError::missing_api_key(API_KEY_ENV)
                }),
        }
    }

    /// Create a client with an explicit base URL and API key.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        UmansClient { http: ProviderHttpClient::new(base_url, api_key) }
    }

    /// Fetch model metadata from `GET /v1/models/info`.
    pub fn fetch_models_info(&self) -> Result<HashMap<String, ModelInfo>> {
        let url = format!("{}/v1/models/info", self.http.base_url());
        let mut resp = self
            .http
            .agent()
            .get(&url)
            .call()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        serde_json::from_str::<HashMap<String, ModelInfo>>(&body).map_err(|e| ProviderError::Json(e.to_string()))
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
        model: &str, messages: &[ProviderMessage], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
    ) -> serde_json::Value {
        providers::anthropic::build_messages_request_body(model, messages, max_tokens, stream, tools)
    }

    /// Build the HTTP headers map for a Messages API request.
    pub fn build_headers(&self, search_mode: WebSearchMode) -> Vec<(String, String)> {
        vec![
            ("x-api-key".to_string(), self.http.api_key().to_string()),
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
        &self, model: &str, messages: &[ProviderMessage], max_tokens: u32, mode: WebSearchMode,
        tools: Option<&serde_json::Value>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let url = format!("{}/v1/messages", self.http.base_url());
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

        let mut request = self.http.agent().post(&url);
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
                ProviderError::Http(e.to_string())
            })?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|e| format!("failed to read error body: {e}"));
            let body = providers::summarize_error_body(&body);
            tracing::error!(status, error = %body, "Umans request returned non-success status");
            return Err(ProviderError::Status { code: status, body });
        }

        tracing::info!(status, "Umans streaming request connected");
        Ok(response)
    }
}

impl StreamingProvider for UmansClient {
    type Metadata = HashMap<String, ModelInfo>;

    fn name(&self) -> &'static str {
        "Umans"
    }

    fn load_status(&self) -> String {
        String::from("provider: loading UMANS_API_KEY")
    }

    fn request_status(&self, model: &str, search_mode: WebSearchMode) -> String {
        format!(
            "provider: POST /v1/messages model={model} search={}",
            search_mode.header_value()
        )
    }

    fn from_env_or_dotenv(root: &Path) -> Result<Self> {
        UmansClient::from_env_or_dotenv(root)
    }

    fn load_metadata(&self) -> Result<Self::Metadata> {
        self.fetch_models_info()
    }

    fn metadata_loaded_event(&self, metadata: &Self::Metadata) -> Option<AgentEvent> {
        let mut items = model_picker_items(metadata);
        items.extend(
            providers::opencode::known_models()
                .into_iter()
                .map(|model| (model.id.to_string(), model.description.to_string())),
        );
        Some(AgentEvent::ModelMetadataLoaded(items))
    }

    fn metadata_status(&self, model: &str, metadata: &Self::Metadata) -> Option<String> {
        model_status(model, metadata)
    }

    fn token_budget(&self, model: &str, metadata: Option<&Self::Metadata>) -> u32 {
        recommended_max_tokens_for_model(model, metadata)
    }

    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], max_tokens: u32, search_mode: WebSearchMode,
        tools: &serde_json::Value,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        UmansClient::send_streaming_request(self, model, messages, max_tokens, search_mode, Some(tools))
    }

    fn stream_format(&self, _model: &str) -> Result<StreamFormat> {
        Ok(StreamFormat::AnthropicMessages)
    }

    fn request_error_message(error: &ProviderError) -> String {
        match error_to_agent_event(error) {
            AgentEvent::Failed(msg) => msg,
            _ => error.to_string(),
        }
    }

    fn is_retryable_request_error(error: &ProviderError) -> bool {
        is_retryable_error(error)
    }
}

/// Model capabilities.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    pub max_completion_tokens: u64,
    pub recommended_max_tokens: u64,
    pub context_window: u64,
    pub supports_vision: serde_json::Value,
    pub supports_tools: bool,
    pub reasoning: Reasoning,
}

/// Reasoning configuration included in live model metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reasoning {
    pub supported: bool,
    pub can_disable: bool,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(default)]
    pub default_level: Option<String>,
}

/// Select the completion token budget for a model from live `/v1/models/info`
/// metadata, falling back to a conservative provider default when metadata is
/// unavailable.
pub fn recommended_max_tokens_for_model(model: &str, models: Option<&HashMap<String, ModelInfo>>) -> u32 {
    models
        .and_then(|models| models.get(model))
        .and_then(|info| u32::try_from(info.capabilities.recommended_max_tokens).ok())
        .filter(|tokens| *tokens > 0)
        .unwrap_or(DEFAULT_RECOMMENDED_MAX_TOKENS)
}

pub fn is_retryable_error(err: &ProviderError) -> bool {
    err.is_retryable()
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

pub fn model_picker_items(models: &HashMap<String, ModelInfo>) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = models
        .iter()
        .map(|(id, info)| (id.clone(), model_picker_detail(info)))
        .collect();
    items.sort_by(|left, right| left.0.cmp(&right.0));
    items
}

pub fn model_status(model: &str, models: &HashMap<String, ModelInfo>) -> Option<String> {
    models
        .get(model)
        .map(|info| format!("model: {}  {}", info.display_name, model_picker_detail(info)))
}

/// Convert a [`ProviderError`] into an [`AgentEvent::Failed`] with a
/// human-readable message.
///
/// HTTP status errors include the status code.
///
/// Auth errors (401/403) and rate-limit errors (429) are labeled distinctly.
pub fn error_to_agent_event(err: &ProviderError) -> AgentEvent {
    AgentEvent::Failed(err.failure_message("rate limit exceeded"))
}

fn model_picker_detail(info: &ModelInfo) -> String {
    let provider = info
        .base_model
        .provider
        .as_deref()
        .or(info.base_model.family.as_deref())
        .unwrap_or(info.base_model.name.as_str());
    let tools = if info.capabilities.supports_tools { "tools" } else { "no tools" };
    let reasoning = if info.capabilities.reasoning.supported { "reasoning" } else { "no reasoning" };
    format!(
        "{} · ctx {} · out {} · {} · {}",
        provider,
        compact_token_count(info.capabilities.context_window),
        compact_token_count(info.capabilities.recommended_max_tokens),
        tools,
        reasoning
    )
}

fn compact_token_count(tokens: u64) -> String {
    const K: u64 = 1024;
    const M: u64 = K * K;

    if tokens >= M && tokens.is_multiple_of(M) {
        format!("{}M", tokens / M)
    } else if tokens >= K && tokens.is_multiple_of(K) {
        format!("{}k", tokens / K)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::WebSearchMode;
    use crate::providers::anthropic::{SseEvent, parse_sse_chunk, parse_sse_event, sse_to_agent_event};
    use crate::providers::{ProviderContentBlock, ProviderMessage, ProviderMessageContent};

    #[test]
    fn build_messages_request_body_has_required_fields() {
        let messages = vec![ProviderMessage::user("Hello!")];
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
        let messages = vec![ProviderMessage::user("test")];
        let body = UmansClient::build_messages_request_body("umans-glm-5.2", &messages, 8192, false, None);
        assert_eq!(body["model"], "umans-glm-5.2");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_messages_request_body_includes_tools() {
        let messages = vec![ProviderMessage::user("find files")];
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
        let messages = vec![ProviderMessage::user("hi")];
        let empty_tools = serde_json::json!([]);
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 4096, true, Some(&empty_tools));
        assert!(body.get("tools").is_none(), "empty tool array should be omitted");
    }

    #[test]
    fn recommended_max_tokens_prefers_live_metadata_with_default_fallback() {
        let json = include_str!("./fixtures/model_info.json");
        let mut models: HashMap<String, ModelInfo> = serde_json::from_str(json).expect("parse");
        let mut high_budget = models["umans-coder"].clone();
        high_budget.capabilities.recommended_max_tokens = 131_071;
        models.insert("metadata-high-budget".to_string(), high_budget);

        assert_eq!(recommended_max_tokens_for_model("umans-coder", Some(&models)), 32_768);
        assert_eq!(
            recommended_max_tokens_for_model("metadata-high-budget", Some(&models)),
            131_071
        );
        assert_eq!(
            recommended_max_tokens_for_model("unknown-model", Some(&models)),
            DEFAULT_RECOMMENDED_MAX_TOKENS
        );
        assert_eq!(
            recommended_max_tokens_for_model("umans-glm-5.2", None),
            DEFAULT_RECOMMENDED_MAX_TOKENS
        );
    }

    #[test]
    fn model_metadata_formats_picker_items_and_status() {
        let json = include_str!("./fixtures/model_info.json");
        let models: HashMap<String, ModelInfo> = serde_json::from_str(json).expect("parse");

        let items = model_picker_items(&models);
        let coder = items
            .iter()
            .find(|(id, _)| id == "umans-coder")
            .expect("umans-coder item");
        assert!(coder.1.contains("ctx 256k"));
        assert!(coder.1.contains("out 32k"));
        assert!(coder.1.contains("tools"));

        let status = model_status("umans-coder", &models).expect("model status");
        assert!(status.starts_with("model: "));
        assert!(status.contains("ctx 256k"));
        assert!(model_status("unknown-model", &models).is_none());
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(!is_retryable_error(&ProviderError::missing_api_key(API_KEY_ENV)));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 400,
            body: "bad request".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 401,
            body: "unauthorized".into()
        }));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "too many requests".into()
        }));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 503,
            body: "service unavailable".into()
        }));
        assert!(is_retryable_error(&ProviderError::Http("connection reset".into())));
        assert!(!is_retryable_error(&ProviderError::Http("request aborted".into())));
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
        assert!(matches!(result, Err(ProviderError::MissingApiKey { env }) if env == API_KEY_ENV));
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
        let event = error_to_agent_event(&ProviderError::missing_api_key(API_KEY_ENV));
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("UMANS_API_KEY")));
    }

    #[test]
    fn error_to_agent_event_auth_failure() {
        let event = error_to_agent_event(&ProviderError::Status { code: 401, body: "Unauthorized".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("authentication failed")));
    }

    #[test]
    fn error_to_agent_event_rate_limit() {
        let event = error_to_agent_event(&ProviderError::Status { code: 429, body: "Too Many Requests".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("rate limit")));
    }

    #[test]
    fn error_to_agent_event_server_error() {
        let event = error_to_agent_event(&ProviderError::Status { code: 500, body: "Internal Server Error".into() });
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("server error")));
    }

    #[test]
    fn error_to_agent_event_network_error() {
        let event = error_to_agent_event(&ProviderError::Http("connection refused".into()));
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("network error")));
    }

    #[test]
    fn error_to_agent_event_json_error() {
        let event = error_to_agent_event(&ProviderError::Json("bad metadata".into()));
        assert!(matches!(event, AgentEvent::Failed(msg) if msg.contains("response parse error")));
    }

    #[test]
    fn no_network_request_construction() {
        let messages = vec![ProviderMessage::user("test"), ProviderMessage::assistant("response")];
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
        let msg = ProviderMessage::user("test prompt");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.as_text(), "test prompt");
    }

    #[test]
    fn message_assistant_constructor() {
        let msg = ProviderMessage::assistant("response");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.as_text(), "response");
    }

    #[test]
    fn tool_result_message_has_correct_shape() {
        let msg = ProviderMessage::tool_result("toolu_01", "found 2 files", false);
        assert_eq!(msg.role, "user");

        match &msg.content {
            ProviderMessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ProviderContentBlock::ToolResult { tool_use_id, content, is_error } => {
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
        let blocks = vec![ProviderContentBlock::ToolUse {
            id: "toolu_01".to_string(),
            name: "find_files".to_string(),
            input: serde_json::json!({"pattern": "Cargo"}),
        }];
        let msg = ProviderMessage::assistant_blocks(blocks);
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
        let msg = ProviderMessage::user("hello");
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["content"], "hello");
        assert!(json["content"].is_string(), "text content should serialize as a string");
    }

    #[test]
    fn tool_result_block_serializes_with_is_error() {
        let msg = ProviderMessage::tool_result("toolu_02", "command failed", true);
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
            ProviderContentBlock::Text { text: "Let me search.".to_string() },
            ProviderContentBlock::ToolUse {
                id: "toolu_03".to_string(),
                name: "search_text".to_string(),
                input: serde_json::json!({"pattern": "fn main"}),
            },
        ];
        let msg = ProviderMessage::assistant_blocks(blocks);
        let json = serde_json::to_value(&msg).expect("serialize");
        assert_eq!(json["content"][0]["type"], "text");
        assert_eq!(json["content"][0]["text"], "Let me search.");
        assert_eq!(json["content"][1]["type"], "tool_use");
        assert_eq!(json["content"][1]["id"], "toolu_03");
    }

    #[test]
    fn message_content_round_trips_through_json() {
        let original = ProviderMessage::tool_result("toolu_99", "result text", false);
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: ProviderMessage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, parsed);
    }
}
