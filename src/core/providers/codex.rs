//! ChatGPT Codex provider foundations.
//!
//! This provider targets the ChatGPT-backed Codex Responses endpoint. The
//! endpoint is not a stable OpenAI Platform API, so the public model ids keep a
//! separate `chatgpt-codex/` prefix and status copy labels it experimental.

use std::path::Path;

use crate::{
    app::AgentEvent,
    cli::WebSearchMode,
    providers::{
        self, KnownModel, ProviderContentBlock, ProviderError, ProviderMessage, ProviderMessageContent, Result,
        StreamFormat, StreamingProvider,
    },
    thndrs_core::auth::{self, ChatGptCodexAuth},
};

/// ChatGPT Codex backend base URL.
pub const BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

/// ChatGPT Codex Responses endpoint.
pub const RESPONSES_URL: &str = "https://chatgpt.com/backend-api/codex/responses";

/// User/config-facing model id prefix.
pub const MODEL_PREFIX: &str = "chatgpt-codex/";

/// Recommended request budget for known ChatGPT Codex models.
pub const DEFAULT_RECOMMENDED_MAX_TOKENS: u32 = 32_768;

/// Parsed ChatGPT Codex Responses stream event.
#[derive(Clone, Debug, PartialEq)]
pub enum ResponsesSseEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart {
        id: String,
        call_id: String,
        name: String,
    },
    ToolCallArgumentsDelta {
        id: String,
        arguments: String,
    },
    ToolCallDone {
        id: String,
        call_id: Option<String>,
        name: String,
        arguments: String,
    },
    ResponseStatus(String),
    Error(String),
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    Done,
    Malformed(String),
    Other,
}

/// Concrete ChatGPT Codex API client.
pub struct ChatGptCodexClient {
    base_url: String,
    auth: ChatGptCodexAuth,
    agent: ureq::Agent,
}

impl ChatGptCodexClient {
    /// Create a client from `CHATGPT_CODEX_ACCESS_TOKEN` or `~/.thndrs/auth.json`.
    pub fn from_env_or_dotenv(_workspace_root: &Path) -> Result<Self> {
        let auth = auth::resolve_chatgpt_codex_auth().map_err(|e| ProviderError::Auth(e.to_string()))?;
        tracing::debug!("loaded ChatGPT Codex auth");
        Ok(Self::new(BASE_URL, auth))
    }

    /// Create a client with explicit auth material.
    pub fn new(base_url: &str, auth: ChatGptCodexAuth) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), auth, agent: ureq::Agent::new_with_defaults() }
    }

    /// Build the headers for a streaming Responses request.
    pub fn build_responses_headers(&self) -> Vec<(String, String)> {
        vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", self.auth.access_token),
            ),
            ("chatgpt-account-id".to_string(), self.auth.account_id.clone()),
            ("originator".to_string(), "thndrs".to_string()),
            ("OpenAI-Beta".to_string(), "responses=experimental".to_string()),
            ("accept".to_string(), "text/event-stream".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    }

    /// Build a Responses-like streaming request body.
    pub fn build_responses_request_body(
        model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let raw_model = raw_model_id(model)?;
        let (instructions, input) = responses_input(messages);
        let mut body = serde_json::json!({
            "model": raw_model,
            "store": false,
            "stream": true,
            "instructions": instructions,
            "input": input,
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "text": { "verbosity": "low" },
            "include": ["reasoning.encrypted_content"],
        });
        if let Some(tool_schemas) = tools {
            let converted = responses_tools(tool_schemas);
            if !converted.as_array().is_some_and(|arr| arr.is_empty()) {
                body["tools"] = converted;
            }
        }
        Ok(body)
    }

    /// Send a streaming request to `POST /responses`.
    pub fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], tools: Option<&serde_json::Value>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let body = Self::build_responses_request_body(model, messages, tools)?;
        let url = format!("{}/responses", self.base_url);
        let mut request = self.agent.post(&url);
        for (key, value) in self.build_responses_headers() {
            request = request.header(&key, &value);
        }
        let mut response = request
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(&body)
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response
                .body_mut()
                .read_to_string()
                .unwrap_or_else(|e| format!("failed to read error body: {e}"));
            return Err(ProviderError::Status { code: status, body: providers::summarize_error_body(&body) });
        }
        Ok(response)
    }
}

impl StreamingProvider for ChatGptCodexClient {
    type Metadata = ();

    fn name(&self) -> &'static str {
        "ChatGPT Codex"
    }

    fn load_status(&self) -> String {
        String::from("provider: loading ChatGPT Codex auth")
    }

    fn request_status(&self, model: &str, _search_mode: WebSearchMode) -> String {
        format!("provider: POST /backend-api/codex/responses model={model} (ChatGPT-backed, experimental)")
    }

    fn from_env_or_dotenv(root: &Path) -> Result<Self> {
        ChatGptCodexClient::from_env_or_dotenv(root)
    }

    fn load_metadata(&self) -> Result<Self::Metadata> {
        Ok(())
    }

    fn metadata_loaded_event(&self, _metadata: &Self::Metadata) -> Option<AgentEvent> {
        None
    }

    fn metadata_status(&self, model: &str, _metadata: &Self::Metadata) -> Option<String> {
        raw_model_id(model)
            .ok()
            .map(|raw| format!("model: chatgpt-codex/{raw}  ChatGPT-backed experimental Codex"))
    }

    fn token_budget(&self, _model: &str, _metadata: Option<&Self::Metadata>) -> u32 {
        DEFAULT_RECOMMENDED_MAX_TOKENS
    }

    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], _max_tokens: u32, _search_mode: WebSearchMode,
        tools: &serde_json::Value,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        ChatGptCodexClient::send_streaming_request(self, model, messages, Some(tools))
    }

    fn stream_format(&self, model: &str) -> Result<StreamFormat> {
        raw_model_id(model)?;
        Ok(StreamFormat::ChatGptCodexResponses)
    }

    fn request_error_message(error: &ProviderError) -> String {
        error_message(error)
    }

    fn is_retryable_request_error(error: &ProviderError) -> bool {
        is_retryable_error(error)
    }
}

/// Whether `model` is a ChatGPT Codex model id.
pub fn is_model_id(model: &str) -> bool {
    model.strip_prefix(MODEL_PREFIX).is_some_and(|raw| !raw.is_empty())
}

/// Strip `chatgpt-codex/` from a model id.
pub fn raw_model_id(model: &str) -> Result<&str> {
    model
        .strip_prefix(MODEL_PREFIX)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| ProviderError::invalid_model_id("ChatGPT Codex", MODEL_PREFIX, model))
}

/// Current ChatGPT Codex models from the provider expansion plan.
pub fn known_models() -> Vec<KnownModel> {
    vec![
        KnownModel { id: "chatgpt-codex/gpt-5.5", description: "ChatGPT-backed Codex, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.4", description: "ChatGPT-backed Codex, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.4-mini", description: "ChatGPT-backed Codex mini, experimental" },
        KnownModel { id: "chatgpt-codex/gpt-5.3-codex-spark", description: "ChatGPT-backed Codex Spark, experimental" },
    ]
}

/// Convert a ChatGPT Codex error into a human-readable failure string.
pub fn error_message(err: &ProviderError) -> String {
    err.failure_message("ChatGPT Codex usage limit exceeded")
}

pub fn is_retryable_error(err: &ProviderError) -> bool {
    match err {
        ProviderError::Status { code, body } if terminal_status_error(*code, body) => false,
        _ => err.is_retryable(),
    }
}

/// Parse raw ChatGPT Codex Responses SSE data lines.
pub fn parse_responses_sse_chunk(chunk: &str) -> Vec<String> {
    chunk
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(|data| data.trim_start().to_string()))
        .collect()
}

/// Parse one ChatGPT Codex Responses SSE `data:` payload.
pub fn parse_responses_sse_event(data: &str) -> Vec<ResponsesSseEvent> {
    if data.trim() == "[DONE]" {
        return vec![ResponsesSseEvent::Done];
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(data) else {
        return vec![ResponsesSseEvent::Malformed(data.chars().take(120).collect())];
    };

    let mut events = Vec::new();
    let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or_default();
    if let Some(error) = extract_responses_error(&value, event_type) {
        events.push(ResponsesSseEvent::Error(error));
    }
    if let Some(status) = extract_responses_status(&value, event_type) {
        events.push(ResponsesSseEvent::ResponseStatus(status));
    }
    if let Some((input_tokens, output_tokens)) = extract_responses_usage(&value) {
        events.push(ResponsesSseEvent::Usage { input_tokens, output_tokens });
    }
    if let Some(text) = extract_responses_text_delta(&value, event_type) {
        events.push(ResponsesSseEvent::TextDelta(text));
    }
    if let Some(reasoning) = extract_responses_reasoning_delta(&value, event_type) {
        events.push(ResponsesSseEvent::ReasoningDelta(reasoning));
    }
    if let Some(tool_event) = extract_responses_tool_event(&value, event_type) {
        events.push(tool_event);
    }

    if events.is_empty() { vec![ResponsesSseEvent::Other] } else { events }
}

fn extract_responses_text_delta(value: &serde_json::Value, event_type: &str) -> Option<String> {
    if !event_type.contains("output_text") && !event_type.contains("message") {
        return None;
    }
    string_field(value, &["delta", "text"])
}

fn extract_responses_reasoning_delta(value: &serde_json::Value, event_type: &str) -> Option<String> {
    if !event_type.contains("reasoning") {
        return None;
    }
    string_field(value, &["delta", "text", "summary_text"])
}

fn extract_responses_tool_event(value: &serde_json::Value, event_type: &str) -> Option<ResponsesSseEvent> {
    if event_type == "response.output_item.added" {
        let item = value.get("item")?;
        if item.get("type").and_then(|v| v.as_str()) != Some("function_call") {
            return None;
        }
        let id = function_item_id(item)?;
        let call_id = function_call_id(item).unwrap_or_else(|| id.clone());
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        return Some(ResponsesSseEvent::ToolCallStart { id, call_id, name });
    }

    if event_type.contains("function_call_arguments.delta") {
        let id = event_item_id(value)?;
        let arguments = value.get("delta").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if arguments.is_empty() {
            return None;
        }
        return Some(ResponsesSseEvent::ToolCallArgumentsDelta { id, arguments });
    }

    if event_type.contains("function_call_arguments.done") || event_type == "response.output_item.done" {
        let item = value.get("item").unwrap_or(value);
        if item
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|kind| kind != "function_call")
        {
            return None;
        }
        let id = function_item_id(item).or_else(|| event_item_id(value))?;
        let call_id = function_call_id(item);
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let arguments = item
            .get("arguments")
            .or_else(|| value.get("arguments"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Some(ResponsesSseEvent::ToolCallDone { id, call_id, name, arguments });
    }

    None
}

fn extract_responses_error(value: &serde_json::Value, event_type: &str) -> Option<String> {
    value
        .get("error")
        .and_then(|error| match error {
            serde_json::Value::String(message) => Some(message.clone()),
            serde_json::Value::Object(_) => error
                .get("message")
                .or_else(|| error.get("code"))
                .and_then(|message| message.as_str())
                .map(str::to_string),
            _ => None,
        })
        .or_else(|| {
            if event_type == "error" || event_type.ends_with(".error") {
                string_field(value, &["message", "code"])
            } else {
                None
            }
        })
}

fn extract_responses_status(value: &serde_json::Value, event_type: &str) -> Option<String> {
    let status = value
        .get("status")
        .or_else(|| value.get("response").and_then(|response| response.get("status")))
        .and_then(|status| status.as_str())
        .or_else(|| event_type.strip_prefix("response."))
        .filter(|status| {
            matches!(
                *status,
                "completed" | "failed" | "incomplete" | "cancelled" | "canceled" | "queued" | "in_progress"
            )
        })?;
    Some(status.to_string())
}

fn extract_responses_usage(value: &serde_json::Value) -> Option<(u64, u64)> {
    let usage = value
        .get("usage")
        .or_else(|| value.get("response").and_then(|response| response.get("usage")))
        .filter(|usage| !usage.is_null())?;
    let input_tokens = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .or_else(|| usage.get("inputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .or_else(|| usage.get("outputTokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input_tokens == 0 && output_tokens == 0 { None } else { Some((input_tokens, output_tokens)) }
}

fn event_item_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["item_id", "output_item_id", "call_id"])
}

fn function_call_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["call_id"])
}

fn function_item_id(value: &serde_json::Value) -> Option<String> {
    string_field(value, &["id", "item_id", "call_id"])
}

fn string_field(value: &serde_json::Value, fields: &[&str]) -> Option<String> {
    fields
        .iter()
        .find_map(|field| value.get(*field).and_then(|v| v.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn responses_input(messages: &[ProviderMessage]) -> (String, serde_json::Value) {
    let mut instructions = String::new();
    let mut input = Vec::new();
    for message in messages {
        if message.role == "system" {
            if !instructions.is_empty() {
                instructions.push_str("\n\n");
            }
            instructions.push_str(&message.as_text());
            continue;
        }
        input.extend(responses_items_for_message(message));
    }
    (instructions, serde_json::Value::Array(input))
}

fn responses_items_for_message(message: &ProviderMessage) -> Vec<serde_json::Value> {
    match &message.content {
        ProviderMessageContent::Text(text) => {
            vec![serde_json::json!({"role": message.role, "content": text})]
        }
        ProviderMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ProviderContentBlock::Text { text } => Some(serde_json::json!({"role": message.role, "content": text})),
                ProviderContentBlock::ToolResult { tool_use_id, content, is_error } => Some(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": tool_use_id,
                    "output": content,
                    "is_error": is_error.unwrap_or(false),
                })),
                ProviderContentBlock::ToolUse { id, name, input } => Some(serde_json::json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": name,
                    "arguments": serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string()),
                })),
                ProviderContentBlock::Image { .. } => None,
            })
            .collect(),
    }
}

fn responses_tools(tools: &serde_json::Value) -> serde_json::Value {
    serde_json::Value::Array(
        tools
            .as_array()
            .into_iter()
            .flatten()
            .map(|tool| {
                let function = tool.get("function").unwrap_or(tool);
                serde_json::json!({
                    "type": "function",
                    "name": function.get("name").cloned().unwrap_or(serde_json::Value::Null),
                    "description": function.get("description").cloned().unwrap_or(serde_json::Value::String(String::new())),
                    "parameters": function.get("parameters").cloned().unwrap_or_else(|| serde_json::json!({ "type": "object" })),
                    "strict": false,
                })
            })
            .collect(),
    )
}

fn terminal_status_error(code: u16, body: &str) -> bool {
    if matches!(code, 400..=404) {
        return true;
    }
    let lower = body.to_ascii_lowercase();
    lower.contains("subscription")
        || lower.contains("balance")
        || lower.contains("quota")
        || lower.contains("monthly")
        || lower.contains("usage limit")
        || lower.contains("insufficient")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn test_auth() -> ChatGptCodexAuth {
        ChatGptCodexAuth { access_token: "test-token".to_string(), account_id: "acct_123".to_string() }
    }

    #[test]
    fn raw_model_id_requires_prefix() {
        assert_eq!(raw_model_id("chatgpt-codex/gpt-5.5").unwrap(), "gpt-5.5");
        assert!(matches!(
            raw_model_id("gpt-5.5"),
            Err(ProviderError::InvalidModelId { .. })
        ));
    }

    #[test]
    fn build_headers_include_expected_names_without_snapshotting_token() {
        let client = ChatGptCodexClient::new(BASE_URL, test_auth());
        let headers: HashMap<String, String> = client.build_responses_headers().into_iter().collect();
        assert!(headers.get("Authorization").unwrap().starts_with("Bearer "));
        assert_eq!(headers.get("chatgpt-account-id").unwrap(), "acct_123");
        assert_eq!(headers.get("originator").unwrap(), "thndrs");
        assert_eq!(headers.get("OpenAI-Beta").unwrap(), "responses=experimental");
        assert_eq!(headers.get("accept").unwrap(), "text/event-stream");
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn build_responses_body_uses_raw_model_and_response_options() {
        let messages = vec![
            ProviderMessage {
                role: "system".to_string(),
                content: ProviderMessageContent::Text("be brief".to_string()),
            },
            ProviderMessage::user("hello"),
        ];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body = ChatGptCodexClient::build_responses_request_body("chatgpt-codex/gpt-5.5", &messages, Some(&catalog))
            .expect("body");

        assert_eq!(body["model"], "gpt-5.5");
        assert_eq!(body["instructions"], "be brief");
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["text"]["verbosity"], "low");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["name"], defs[0].name.as_ref());
    }

    #[test]
    fn parse_responses_sse_chunk_accepts_optional_space_after_data_colon() {
        assert_eq!(
            parse_responses_sse_chunk("data:{\"a\":1}\ndata: {\"b\":2}\n"),
            vec!["{\"a\":1}".to_string(), "{\"b\":2}".to_string()]
        );
    }

    #[test]
    fn parse_responses_sse_text_reasoning_usage_and_statuses() {
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.output_text.delta","delta":"hi"}"#),
            vec![ResponsesSseEvent::TextDelta("hi".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.reasoning_text.delta","delta":"thinking"}"#),
            vec![ResponsesSseEvent::ReasoningDelta("thinking".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":5,"output_tokens":8}}}"#
            ),
            vec![
                ResponsesSseEvent::ResponseStatus("completed".to_string()),
                ResponsesSseEvent::Usage { input_tokens: 5, output_tokens: 8 },
            ]
        );
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"response.in_progress"}"#),
            vec![ResponsesSseEvent::ResponseStatus("in_progress".to_string())]
        );
        assert_eq!(parse_responses_sse_event("[DONE]"), vec![ResponsesSseEvent::Done]);
    }

    #[test]
    fn parse_responses_sse_tool_call_deltas_and_done() {
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.output_item.added","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"find_files"}}"#
            ),
            vec![ResponsesSseEvent::ToolCallStart {
                id: "fc_1".to_string(),
                call_id: "call_1".to_string(),
                name: "find_files".to_string(),
            }]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.function_call_arguments.delta","item_id":"fc_1","delta":"{\"pattern\":\"Cargo\""}"#
            ),
            vec![ResponsesSseEvent::ToolCallArgumentsDelta {
                id: "fc_1".to_string(),
                arguments: r#"{"pattern":"Cargo""#.to_string(),
            }]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.output_item.done","item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"find_files","arguments":"{\"pattern\":\"Cargo\"}"}}"#
            ),
            vec![ResponsesSseEvent::ToolCallDone {
                id: "fc_1".to_string(),
                call_id: Some("call_1".to_string()),
                name: "find_files".to_string(),
                arguments: r#"{"pattern":"Cargo"}"#.to_string(),
            }]
        );
    }

    #[test]
    fn parse_responses_sse_backend_errors_and_malformed_payloads() {
        assert_eq!(
            parse_responses_sse_event(r#"{"type":"error","message":"backend failed"}"#),
            vec![ResponsesSseEvent::Error("backend failed".to_string())]
        );
        assert_eq!(
            parse_responses_sse_event(
                r#"{"type":"response.failed","response":{"status":"failed"},"error":{"message":"bad auth"}}"#
            ),
            vec![
                ResponsesSseEvent::Error("bad auth".to_string()),
                ResponsesSseEvent::ResponseStatus("failed".to_string()),
            ]
        );
        assert_eq!(
            parse_responses_sse_event("{not json"),
            vec![ResponsesSseEvent::Malformed("{not json".to_string())]
        );
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "rate limit".into()
        }));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 503,
            body: "unavailable".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 402,
            body: "monthly usage limit".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 403,
            body: "subscription required".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Auth("missing".into())));
    }
}
