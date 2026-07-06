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
        Ok(StreamFormat::OpenAiChat)
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
