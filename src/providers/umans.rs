//! Umans Code provider — Anthropic-compatible Messages API.
//!
//! Uses `POST /v1/messages` with `x-api-key` and `anthropic-version` headers.
//! Streaming responses arrive as SSE events, parsed into [`AgentEvent`].

use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde::{Deserialize, Serialize};

use crate::app::AgentEvent;

/// Umans Code base URL.
pub const BASE_URL: &str = "https://api.code.umans.ai";

/// Required Anthropic version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Environment variable name for the API key.
pub const API_KEY_ENV: &str = "UMANS_API_KEY";

type Result<T> = std::result::Result<T, UmansError>;

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
    #[error("json error: {0}")]
    Json(String),
    /// SSE stream parsing error.
    #[error("stream error: {0}")]
    Stream(String),
}

/// Concrete Umans Code API client.
pub struct UmansClient {
    base_url: String,
    api_key: String,
    agent: ureq::Agent,
}

impl UmansClient {
    /// Create a client from the `UMANS_API_KEY` environment variable.
    ///
    /// Returns [`UmansError::MissingApiKey`] if the env var is not set.
    pub fn from_env() -> Result<Self> {
        let api_key = env::var(API_KEY_ENV).map_err(|_| UmansError::MissingApiKey)?;
        Ok(Self::new(BASE_URL, &api_key))
    }

    /// Create a client with an explicit base URL and API key.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        UmansClient {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    /// Fetch model metadata from `GET /v1/models/info`.
    ///
    /// This is a public endpoint that does not require authentication.
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
    pub fn build_messages_request_body(
        model: &str, messages: &[Message], max_tokens: u32, stream: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "stream": stream,
        })
    }

    /// Build the HTTP headers map for a Messages API request.
    ///
    /// Returns `x-api-key`, `anthropic-version`, and `Content-Type`.
    pub fn build_headers(&self) -> Vec<(String, String)> {
        vec![
            ("x-api-key".to_string(), self.api_key.clone()),
            ("anthropic-version".to_string(), ANTHROPIC_VERSION.to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ]
    }

    /// Send a streaming `POST /v1/messages` request and return the HTTP
    /// response.
    ///
    /// The caller reads lines from the response body and feeds them to
    /// [`parse_sse_chunk`] and [`parse_sse_event`].
    pub fn send_streaming_request(
        &self, model: &str, messages: &[Message], max_tokens: u32,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let url = format!("{}/v1/messages", self.base_url);
        let body = Self::build_messages_request_body(model, messages, max_tokens, true);

        let response = self
            .agent
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| match e {
                ureq::Error::StatusCode(code) => UmansError::Status { code, body: format!("HTTP {code}") },
                other => UmansError::Http(other.to_string()),
            })?;

        Ok(response)
    }
}

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

/// Base model descriptor.
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reasoning {
    pub supported: bool,
    pub can_disable: bool,
    #[serde(default)]
    pub levels: Vec<String>,
    #[serde(default)]
    pub default_level: Option<String>,
}

/// A message in the Anthropic Messages API format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: &str) -> Self {
        Message { role: "user".to_string(), content: content.to_string() }
    }

    pub fn assistant(content: &str) -> Self {
        Message { role: "assistant".to_string(), content: content.to_string() }
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
    /// `event: content_block_stop`
    ContentBlockStop,
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
                        .to_string();
                    SseEvent::TextDelta(text)
                }
                _ => SseEvent::Other(format!("content_block_delta: {delta_type}")),
            }
        }
        "content_block_stop" => SseEvent::ContentBlockStop,
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
/// `MessageStop` → `Finished`, `Error` → `Failed`.
///
/// TODO: convert to a From/Into implementation
pub fn sse_to_agent_event(event: &SseEvent) -> Option<AgentEvent> {
    match event {
        SseEvent::MessageStart => Some(AgentEvent::Started),
        SseEvent::TextDelta(text) => Some(AgentEvent::AssistantDelta(text.clone())),
        SseEvent::ThinkingDelta(text) => Some(AgentEvent::ReasoningDelta(text.clone())),
        SseEvent::MessageStop => Some(AgentEvent::Finished),
        SseEvent::Error(msg) => Some(AgentEvent::Failed(msg.clone())),
        _ => None,
    }
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
        UmansError::Stream(e) => format!("stream error: {e}"),
    };
    AgentEvent::Failed(msg)
}

/// Read an SSE response body on a background thread, parsing events and
/// sending [`AgentEvent`] instances through a channel.
///
/// The thread reads lines from the response body, accumulates SSE events,
/// and sends `AgentEvent`s.
///
/// When the stream ends, the sender is dropped so the receiver gets `Disconnected`.
pub fn spawn_stream_reader(response: ureq::http::Response<ureq::Body>) -> Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>();

    thread::spawn(move || {
        let reader = BufReader::new(response.into_body().into_reader());
        let mut buffer = String::new();

        for line_result in reader.lines() {
            match line_result {
                Ok(line) => {
                    buffer.push_str(&line);
                    buffer.push('\n');

                    if line.is_empty() {
                        let events = parse_sse_chunk(&buffer);
                        buffer.clear();

                        for (event_type, data) in events {
                            let sse_event = parse_sse_event(&event_type, &data);
                            if let Some(agent_event) = sse_to_agent_event(&sse_event)
                                && tx.send(agent_event).is_err()
                            {
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(AgentEvent::Failed(format!("stream read error: {e}")));
                    return;
                }
            }
        }

        if !buffer.is_empty() {
            let events = parse_sse_chunk(&buffer);
            for (event_type, data) in events {
                let sse_event = parse_sse_event(&event_type, &data);
                if let Some(agent_event) = sse_to_agent_event(&sse_event)
                    && tx.send(agent_event).is_err()
                {
                    return;
                }
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_messages_request_body_has_required_fields() {
        let messages = vec![Message::user("Hello!")];
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 4096, true);
        assert_eq!(body["model"], "umans-coder");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello!");
    }

    #[test]
    fn build_messages_request_body_non_stream() {
        let messages = vec![Message::user("test")];
        let body = UmansClient::build_messages_request_body("umans-glm-5.2", &messages, 8192, false);
        assert_eq!(body["model"], "umans-glm-5.2");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_headers_include_api_key_and_version() {
        let client = UmansClient::new(BASE_URL, "sk-test-key");
        let headers = client.build_headers();
        let header_map: HashMap<String, String> = headers.into_iter().collect();
        assert_eq!(header_map.get("x-api-key").unwrap(), "sk-test-key");
        assert_eq!(header_map.get("anthropic-version").unwrap(), ANTHROPIC_VERSION);
        assert_eq!(header_map.get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn from_env_missing_key_returns_error() {
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let result = UmansClient::from_env();
        assert!(matches!(result, Err(UmansError::MissingApiKey)));
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
        assert_eq!(sse_to_agent_event(&SseEvent::MessageStop), Some(AgentEvent::Finished));
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

        assert_eq!(agent_events.len(), 4);
        assert_eq!(agent_events[0], AgentEvent::Started);
        assert_eq!(agent_events[1], AgentEvent::ReasoningDelta("Analyzing...".to_string()));
        assert_eq!(agent_events[2], AgentEvent::AssistantDelta("Hello world".to_string()));
        assert_eq!(agent_events[3], AgentEvent::Finished);
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
    fn no_network_request_construction() {
        let messages = vec![Message::user("test"), Message::assistant("response")];
        let body = UmansClient::build_messages_request_body("umans-coder", &messages, 8192, true);
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
        let client = UmansClient::from_env().expect("UMANS_API_KEY must be set");
        let models = client.fetch_models_info().expect("fetch models info");
        assert!(models.contains_key("umans-coder"));
    }

    #[test]
    fn message_user_constructor() {
        let msg = Message::user("test prompt");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "test prompt");
    }

    #[test]
    fn message_assistant_constructor() {
        let msg = Message::assistant("response");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "response");
    }
}
