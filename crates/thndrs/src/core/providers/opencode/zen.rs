//! OpenCode Zen provider.
//!
//! OpenCode Zen exposes an OpenAI-compatible chat-completions route. The
//! app-facing model id keeps the documented `opencode/` prefix; requests use
//! the raw Zen model id.

use std::path::Path;

use crate::{
    app::AgentEvent,
    cli::{ReasoningEffort, ReasoningSummary},
    providers::{
        self, KnownModel, ProviderContinuation, ProviderError, ProviderHttpClient, ProviderMessage, Result,
        StreamFormat, StreamingProvider, StreamingRequest,
    },
    thndrs_core::auth,
};

/// OpenCode Zen base URL.
pub const BASE_URL: &str = "https://opencode.ai/zen/v1";

/// OpenCode Zen chat-completions endpoint.
pub const CHAT_COMPLETIONS_URL: &str = "https://opencode.ai/zen/v1/chat/completions";

/// OpenCode Zen model-discovery endpoint.
pub const MODELS_URL: &str = "https://opencode.ai/zen/v1/models";

/// Environment variable name for the OpenCode Zen API key.
pub const API_KEY_ENV: &str = "OPENCODE_ZEN_KEY";

/// User/config-facing model id prefix.
pub const MODEL_PREFIX: &str = "opencode/";

pub const DEFAULT_RECOMMENDED_MAX_TOKENS: u32 = 32_768;

/// Endpoint family selected by the OpenCode Zen model id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointFamily {
    Responses,
    AnthropicMessages,
    OpenAiChat,
}

/// OpenAI-compatible model list response from `GET /models`.
pub type ModelsResponse = super::ModelsResponse;

/// Model metadata currently returned by OpenCode Zen.
pub type ModelInfo = super::ModelInfo;

/// Concrete OpenCode Zen API client.
pub struct OpenCodeZenClient {
    http: ProviderHttpClient,
}

impl OpenCodeZenClient {
    /// Create a client from `OPENCODE_ZEN_KEY`, falling back to managed stores
    /// and workspace `.env`.
    pub fn from_env_or_dotenv(workspace_root: &Path) -> Result<Self> {
        if let Some((api_key, source)) = auth::resolve_credential(API_KEY_ENV, workspace_root) {
            tracing::debug!(source = source.label(), "loaded OpenCode Zen API key");
            Ok(Self::new(BASE_URL, &api_key))
        } else {
            tracing::error!(env = API_KEY_ENV, cwd = %workspace_root.display(), "missing OpenCode Zen API key");
            Err(ProviderError::missing_api_key(API_KEY_ENV))
        }
    }

    /// Create a client with an explicit base URL and API key.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        OpenCodeZenClient { http: ProviderHttpClient::new(base_url, api_key) }
    }

    /// Fetch model metadata from `GET /models`.
    pub fn fetch_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.http.base_url());
        let mut resp = self
            .http
            .agent()
            .get(&url)
            .header("Authorization", &format!("Bearer {}", self.http.api_key()))
            .config()
            .http_status_as_error(false)
            .build()
            .call()
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !(200..=299).contains(&status) {
            return Err(ProviderError::Status { code: status, body: providers::summarize_error_body(&body) });
        }

        serde_json::from_str::<ModelsResponse>(&body)
            .map(|response| response.data)
            .map_err(|e| ProviderError::Json(e.to_string()))
    }

    /// Build the headers for a chat-completions request.
    pub fn build_chat_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Authorization".to_string(), format!("Bearer {}", self.http.api_key())),
            ("Content-Type".to_string(), "application/json".to_string()),
        ]
    }

    /// Build headers for an Anthropic-compatible Messages request.
    pub fn build_messages_headers(&self) -> Vec<(String, String)> {
        let mut headers = self.build_chat_headers();
        headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        headers
    }

    /// Build a request body for `POST /chat/completions`.
    pub fn build_chat_request_body(
        model: &str, messages: &[ProviderMessage], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let raw_model = raw_model_id(model)?;
        Ok(providers::openai::build_chat_request_body(
            raw_model, messages, max_tokens, stream, tools,
        ))
    }

    /// Build the request body for the endpoint family selected by `model`.
    pub fn build_request_body(
        model: &str, messages: &[ProviderMessage], max_tokens: u32, tools: Option<&serde_json::Value>,
        effort: ReasoningEffort, summary: ReasoningSummary, continuation: &ProviderContinuation,
    ) -> Result<serde_json::Value> {
        let raw_model = raw_model_id(model)?;
        if !reasoning_options(model).contains(&effort) {
            return Err(ProviderError::Json(format!(
                "reasoning control `{}` is not supported by {model}",
                effort.label()
            )));
        }
        Ok(match endpoint_family(raw_model) {
            EndpointFamily::Responses => providers::codex::ChatGptCodexClient::build_openai_responses_request_body(
                raw_model,
                messages,
                tools,
                effort,
                summary,
                continuation,
            ),
            EndpointFamily::AnthropicMessages => {
                let mut body =
                    providers::anthropic::build_messages_request_body(raw_model, messages, max_tokens, true, tools);
                if effort != ReasoningEffort::Auto {
                    body["output_config"] = serde_json::json!({ "effort": effort.label() });
                    if supports_adaptive_thinking(raw_model) {
                        body["thinking"] = serde_json::json!({ "type": "adaptive" });
                    }
                }
                body
            }
            EndpointFamily::OpenAiChat => {
                providers::openai::build_chat_request_body(raw_model, messages, max_tokens, true, tools)
            }
        })
    }

    /// Send a streaming request to the endpoint selected by `model`.
    #[expect(
        clippy::too_many_arguments,
        reason = "The public helper mirrors the complete provider request boundary for focused fixture tests."
    )]
    pub fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], max_tokens: u32, tools: Option<&serde_json::Value>,
        effort: ReasoningEffort, summary: ReasoningSummary, continuation: &ProviderContinuation,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let raw_model = raw_model_id(model)?;
        let family = endpoint_family(raw_model);
        let body = Self::build_request_body(model, messages, max_tokens, tools, effort, summary, continuation)?;
        let path = match family {
            EndpointFamily::Responses => "responses",
            EndpointFamily::AnthropicMessages => "messages",
            EndpointFamily::OpenAiChat => "chat/completions",
        };
        let url = format!("{}/{}", self.http.base_url(), path);
        let mut request = self.http.agent().post(&url);
        let headers = if family == EndpointFamily::AnthropicMessages {
            self.build_messages_headers()
        } else {
            self.build_chat_headers()
        };
        for (key, value) in headers {
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

impl StreamingProvider for OpenCodeZenClient {
    type Metadata = Vec<ModelInfo>;

    fn name(&self) -> &'static str {
        "OpenCode Zen"
    }

    fn load_status(&self) -> String {
        String::from("provider: loading OPENCODE_ZEN_KEY")
    }

    fn request_status(&self, model: &str) -> String {
        format!("provider: POST /zen/v1/chat/completions model={model}")
    }

    fn from_env_or_dotenv(root: &Path) -> Result<Self> {
        OpenCodeZenClient::from_env_or_dotenv(root)
    }

    fn load_metadata(&self) -> Result<Self::Metadata> {
        self.fetch_models()
    }

    fn metadata_loaded_event(&self, metadata: &Self::Metadata) -> Option<AgentEvent> {
        let mut items: Vec<(String, String)> = known_models()
            .into_iter()
            .map(|model| (model.id.to_string(), model.description.to_string()))
            .collect();
        items.extend(
            providers::opencode::go::known_models()
                .into_iter()
                .map(|model| (model.id.to_string(), model.description.to_string())),
        );
        items.extend(
            providers::chatgpt_codex::known_models()
                .into_iter()
                .map(|model| (model.id.to_string(), model.description.to_string())),
        );
        items.extend(model_picker_items(metadata));
        Some(AgentEvent::ModelMetadataLoaded(items))
    }

    fn metadata_status(&self, model: &str, metadata: &Self::Metadata) -> Option<String> {
        model_status(model, metadata)
    }

    fn token_budget(&self, _model: &str, _metadata: Option<&Self::Metadata>) -> u32 {
        DEFAULT_RECOMMENDED_MAX_TOKENS
    }

    fn serialized_request_body(
        &self, model: &str, messages: &[ProviderMessage], request: &StreamingRequest<'_>,
    ) -> Result<Vec<u8>> {
        let body = Self::build_request_body(
            model,
            messages,
            request.max_tokens,
            Some(request.tools),
            request.reasoning_effort,
            request.reasoning_summary,
            request.continuation,
        )?;
        providers::serialize_request_body(&body)
    }

    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], request: &crate::providers::StreamingRequest<'_>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        OpenCodeZenClient::send_streaming_request(
            self,
            model,
            messages,
            request.max_tokens,
            Some(request.tools),
            request.reasoning_effort,
            request.reasoning_summary,
            request.continuation,
        )
    }

    fn stream_format(&self, model: &str) -> Result<StreamFormat> {
        let raw = raw_model_id(model)?;
        Ok(match endpoint_family(raw) {
            EndpointFamily::Responses => StreamFormat::ChatGptCodexResponses,
            EndpointFamily::AnthropicMessages => StreamFormat::AnthropicMessages,
            EndpointFamily::OpenAiChat => StreamFormat::OpenAiChat,
        })
    }

    fn request_error_message(error: &ProviderError) -> String {
        error_message(error)
    }

    fn is_retryable_request_error(error: &ProviderError) -> bool {
        is_retryable_error(error)
    }
}

/// Whether `model` is an OpenCode Zen model id.
pub fn is_model_id(model: &str) -> bool {
    model.strip_prefix(MODEL_PREFIX).is_some_and(|raw| !raw.is_empty())
}

/// Strip `opencode/` from a model id.
pub fn raw_model_id(model: &str) -> Result<&str> {
    model
        .strip_prefix(MODEL_PREFIX)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| ProviderError::invalid_model_id("OpenCode Zen", MODEL_PREFIX, model))
}

/// Select the documented OpenCode Zen endpoint for a raw model id.
pub fn endpoint_family(model: &str) -> EndpointFamily {
    if model.starts_with("gpt-") {
        EndpointFamily::Responses
    } else if model.starts_with("claude-") || model.starts_with("qwen") {
        EndpointFamily::AnthropicMessages
    } else {
        EndpointFamily::OpenAiChat
    }
}

/// Return the verified reasoning choices for one Zen model.
pub fn reasoning_options(model: &str) -> Vec<ReasoningEffort> {
    match raw_model_id(model) {
        Ok(raw) => {
            if raw.starts_with("gpt-") {
                if raw.contains("-pro") {
                    return vec![ReasoningEffort::Auto, ReasoningEffort::High];
                }
                return vec![
                    ReasoningEffort::Auto,
                    ReasoningEffort::None,
                    ReasoningEffort::Minimal,
                    ReasoningEffort::Low,
                    ReasoningEffort::Medium,
                    ReasoningEffort::High,
                    ReasoningEffort::Xhigh,
                ];
            }
            if raw.starts_with("claude-") && supports_claude_effort(raw) {
                let mut options = vec![
                    ReasoningEffort::Auto,
                    ReasoningEffort::Low,
                    ReasoningEffort::Medium,
                    ReasoningEffort::High,
                ];
                if supports_claude_xhigh(raw) {
                    options.push(ReasoningEffort::Xhigh);
                }
                if !raw.contains("4-5") {
                    options.push(ReasoningEffort::Max);
                }
                return options;
            }
            vec![ReasoningEffort::Auto]
        }
        Err(_) => vec![ReasoningEffort::Auto],
    }
}

/// Current OpenCode Zen models from public docs.
pub fn known_models() -> Vec<KnownModel> {
    vec![
        KnownModel { id: "opencode/gpt-5.6-sol", description: "OpenCode Zen GPT-5.6 Sol (Responses)" },
        KnownModel { id: "opencode/gpt-5.6-terra", description: "OpenCode Zen GPT-5.6 Terra (Responses)" },
        KnownModel { id: "opencode/gpt-5.6-luna", description: "OpenCode Zen GPT-5.6 Luna (Responses)" },
        KnownModel { id: "opencode/claude-opus-4-8", description: "OpenCode Zen Claude Opus 4.8 (Messages)" },
        KnownModel { id: "opencode/claude-sonnet-5", description: "OpenCode Zen Claude Sonnet 5 (Messages)" },
        KnownModel { id: "opencode/big-pickle", description: "OpenCode Zen Big Pickle (Chat Completions)" },
    ]
}

pub fn model_picker_items(models: &[ModelInfo]) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = models
        .iter()
        .map(|info| (format!("{MODEL_PREFIX}{}", info.id), String::from("OpenCode Zen")))
        .collect();
    items.sort_by(|left, right| left.0.cmp(&right.0));
    items
}

pub fn model_status(model: &str, models: &[ModelInfo]) -> Option<String> {
    let raw = raw_model_id(model).ok()?;
    models
        .iter()
        .find(|info| info.id == raw)
        .map(|info| format!("model: opencode/{}  OpenCode Zen", info.id))
}

pub fn is_retryable_error(err: &ProviderError) -> bool {
    match err {
        ProviderError::Status { code, body } if terminal_status_error(*code, body) => false,
        _ => err.is_retryable(),
    }
}

/// Convert an OpenCode Zen error into a human-readable failure string.
pub fn error_message(err: &ProviderError) -> String {
    err.failure_message("rate limit or usage limit exceeded")
}

/// Try to validate an OpenCode Zen API key with a lightweight model-list request.
pub fn validate_api_key(api_key: &str) -> std::result::Result<(), String> {
    probe_api_key(api_key).map_err(|error| format!("validation failed: {error}"))
}

pub fn probe_api_key(api_key: &str) -> Result<()> {
    validate_api_key_at(BASE_URL, api_key)
}

fn supports_claude_effort(model: &str) -> bool {
    matches!(
        model,
        "claude-fable-5"
            | "claude-opus-4-8"
            | "claude-opus-4-7"
            | "claude-opus-4-6"
            | "claude-opus-4-5"
            | "claude-sonnet-5"
            | "claude-sonnet-4-6"
    )
}

fn supports_claude_xhigh(model: &str) -> bool {
    matches!(
        model,
        "claude-fable-5" | "claude-opus-4-8" | "claude-opus-4-7" | "claude-sonnet-5"
    )
}

fn supports_adaptive_thinking(model: &str) -> bool {
    matches!(
        model,
        "claude-fable-5"
            | "claude-opus-4-8"
            | "claude-opus-4-7"
            | "claude-opus-4-6"
            | "claude-sonnet-5"
            | "claude-sonnet-4-6"
    )
}

fn validate_api_key_at(base_url: &str, api_key: &str) -> Result<()> {
    OpenCodeZenClient::new(base_url, api_key).fetch_models().map(|_| ())
}

fn terminal_status_error(code: u16, body: &str) -> bool {
    if matches!(code, 400..=404) {
        true
    } else {
        let lower = body.to_ascii_lowercase();
        lower.contains("balance")
            || lower.contains("insufficient")
            || lower.contains("unavailable model")
            || lower.contains("model unavailable")
            || lower.contains("free period")
            || lower.contains("free-period")
            || lower.contains("ended")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;
    use crate::providers::openai::{ChatSseEvent, parse_chat_sse_event};

    fn mock_models_server(body: &'static str) -> String {
        mock_models_response_server("200 OK", body)
    }

    fn mock_models_response_server(status: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("write response");
        });
        format!("http://{addr}")
    }

    #[test]
    fn raw_model_id_requires_prefix() {
        assert_eq!(raw_model_id("opencode/big-pickle").unwrap(), "big-pickle");
        assert!(matches!(
            raw_model_id("big-pickle"),
            Err(ProviderError::InvalidModelId { .. })
        ));
    }

    #[test]
    fn build_chat_request_body_uses_raw_model_and_tools() {
        let messages = vec![ProviderMessage::user("find files")];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body =
            OpenCodeZenClient::build_chat_request_body("opencode/big-pickle", &messages, 4096, true, Some(&catalog))
                .expect("body");

        assert_eq!(body["model"], "big-pickle");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], defs[0].name.as_ref());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn build_chat_request_body_uses_raw_model_for_text_only_turns() {
        let messages = vec![
            ProviderMessage {
                role: "system".to_string(),
                content: providers::ProviderMessageContent::Text("be concise".to_string()),
            },
            ProviderMessage::user("Say ok."),
        ];
        let body = OpenCodeZenClient::build_chat_request_body("opencode/big-pickle", &messages, 128, true, None)
            .expect("body");

        assert_eq!(body["model"], "big-pickle");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "be concise");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "Say ok.");
        assert_eq!(body["max_tokens"], 128);
        assert_eq!(body["stream"], true);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn build_headers_include_expected_auth_without_redaction_helpers() {
        let client = OpenCodeZenClient::new(BASE_URL, "zen-test-key");
        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer zen-test-key");
        assert_eq!(headers.get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn from_env_or_dotenv_missing_key_returns_error() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let result = OpenCodeZenClient::from_env_or_dotenv(dir.path());
        assert!(matches!(result, Err(ProviderError::MissingApiKey { env }) if env == API_KEY_ENV));
    }

    #[test]
    fn missing_credentials_fail_before_network_access() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let result = OpenCodeZenClient::from_env_or_dotenv(dir.path());

        assert!(matches!(result, Err(ProviderError::MissingApiKey { env }) if env == API_KEY_ENV));
    }

    #[test]
    fn from_env_or_dotenv_reads_workspace_env_file_separately_from_go_key() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
            env::remove_var(auth::OPENCODE_GO_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "OPENCODE_GO_KEY=go-dotenv-key\nOPENCODE_ZEN_KEY=zen-dotenv-key\n",
        )
        .unwrap();

        let client = OpenCodeZenClient::from_env_or_dotenv(dir.path()).unwrap();
        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();

        assert_eq!(headers.get("Authorization").unwrap(), "Bearer zen-dotenv-key");
    }

    #[test]
    fn validation_does_not_persist_provider_payloads() {
        let _guard = crate::test_env::lock();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();

        let base_url = mock_models_server(
            r#"{"object":"list","data":[{"id":"big-pickle","object":"model","created":1,"owned_by":"opencode"}]}"#,
        );
        let old_home = env::var_os("HOME");
        unsafe { env::set_var("HOME", &home) };
        validate_api_key_at(&base_url, "zen-validation").expect("validation succeeds");
        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }

        assert!(!home.join(".thndrs").join("credentials.env").exists());
        assert!(!workspace.join(".thndrs").join("credentials.env").exists());
    }

    #[test]
    fn validation_preserves_rejected_credential_status() {
        let base_url = mock_models_response_server("403 Forbidden", r#"{"error":"forbidden"}"#);

        let error = validate_api_key_at(&base_url, "rejected-key").expect_err("credential should be rejected");

        assert!(matches!(error, ProviderError::Status { code: 403, .. }));
    }

    #[test]
    fn model_picker_items_prefix_live_ids_without_pricing_claims() {
        let models = vec![ModelInfo {
            id: "big-pickle".to_string(),
            object: "model".to_string(),
            created: 1,
            owned_by: "opencode".to_string(),
        }];
        let items = model_picker_items(&models);
        assert_eq!(items[0].0, "opencode/big-pickle");
        assert_eq!(items[0].1, "OpenCode Zen");
        assert!(!items[0].1.to_ascii_lowercase().contains("free"));
    }

    #[test]
    fn known_models_include_big_pickle_for_offline_picker() {
        let models = known_models();
        assert!(models.iter().any(|model| model.id == "opencode/big-pickle"));
    }

    #[test]
    fn model_status_maps_discovered_raw_ids() {
        let models = vec![ModelInfo {
            id: "big-pickle".to_string(),
            object: "model".to_string(),
            created: 1,
            owned_by: "opencode".to_string(),
        }];

        assert_eq!(
            model_status("opencode/big-pickle", &models).as_deref(),
            Some("model: opencode/big-pickle  OpenCode Zen")
        );
        assert_eq!(model_status("big-pickle", &models), None);
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(!is_retryable_error(&ProviderError::missing_api_key(API_KEY_ENV)));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "limit".into()
        }));
        for code in [500, 502, 503, 504] {
            assert!(is_retryable_error(&ProviderError::Status {
                code,
                body: "temporary server error".into()
            }));
        }
        assert!(is_retryable_error(&ProviderError::Http("request timed out".into())));
        assert!(is_retryable_error(&ProviderError::Http("connection reset".into())));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 402,
            body: "insufficient balance".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 404,
            body: "unavailable model".into()
        }));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 400,
            body: "free period ended".into()
        }));
    }

    #[test]
    fn parse_chat_sse_events_cover_zen_stream_shapes() {
        assert_eq!(
            parse_chat_sse_event(r#"{"choices":[{"delta":{"content":"hi"}}]}"#),
            vec![ChatSseEvent::TextDelta("hi".to_string())]
        );
        assert_eq!(
            parse_chat_sse_event(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"find_files","arguments":"{\"pattern\":\"Cargo\"}"}}]}}]}"#
            ),
            vec![
                ChatSseEvent::ToolCallStart { index: 0, id: "call_1".to_string(), name: "find_files".to_string() },
                ChatSseEvent::ToolCallArgumentsDelta { index: 0, arguments: r#"{"pattern":"Cargo"}"#.to_string() },
            ]
        );
        assert_eq!(
            parse_chat_sse_event(r#"{"usage":{"prompt_tokens":2,"completion_tokens":3}}"#),
            vec![ChatSseEvent::Usage { input_tokens: 2, output_tokens: 3 }]
        );
        assert_eq!(
            parse_chat_sse_event(r#"{"error":{"message":"backend failed"}}"#),
            vec![ChatSseEvent::Error("backend failed".to_string())]
        );
        assert_eq!(
            parse_chat_sse_event("{not json"),
            vec![ChatSseEvent::Malformed("{not json".to_string())]
        );
    }

    #[test]
    fn routes_and_lowers_reasoning_by_model_family() {
        assert_eq!(endpoint_family("gpt-5.6-sol"), EndpointFamily::Responses);
        assert_eq!(endpoint_family("claude-opus-4-8"), EndpointFamily::AnthropicMessages);
        assert_eq!(endpoint_family("big-pickle"), EndpointFamily::OpenAiChat);

        let messages = vec![ProviderMessage::user("hello")];
        let responses = OpenCodeZenClient::build_request_body(
            "opencode/gpt-5.6-sol",
            &messages,
            128,
            None,
            ReasoningEffort::High,
            ReasoningSummary::Off,
            &ProviderContinuation::default(),
        )
        .expect("responses body");
        assert_eq!(responses["reasoning"]["effort"], "high");

        let anthropic = OpenCodeZenClient::build_request_body(
            "opencode/claude-opus-4-8",
            &messages,
            128,
            None,
            ReasoningEffort::Xhigh,
            ReasoningSummary::Off,
            &ProviderContinuation::default(),
        )
        .expect("messages body");
        assert_eq!(anthropic["output_config"]["effort"], "xhigh");
        assert_eq!(anthropic["thinking"]["type"], "adaptive");
        assert!(
            OpenCodeZenClient::build_request_body(
                "opencode/big-pickle",
                &messages,
                128,
                None,
                ReasoningEffort::High,
                ReasoningSummary::Off,
                &ProviderContinuation::default(),
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires OPENCODE_ZEN_KEY, network access, and acceptance of Big Pickle limited-free pricing/privacy caveats"]
    fn live_models_requires_opencode_zen_key() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeZenClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_ZEN_KEY must be set");
        let models = client.fetch_models().expect("fetch Zen models with OPENCODE_ZEN_KEY");
        assert!(models.iter().any(|model| model.id == "big-pickle"));
    }

    #[test]
    #[ignore = "requires OPENCODE_ZEN_KEY, network access, and acceptance of Big Pickle limited-free pricing/privacy caveats"]
    fn live_big_pickle_text_stream_requires_opencode_zen_key() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeZenClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_ZEN_KEY must be set");
        let messages = vec![ProviderMessage::user("Reply with exactly: ok")];
        let mut response = client
            .send_streaming_request(
                "opencode/big-pickle",
                &messages,
                32,
                None,
                ReasoningEffort::Auto,
                ReasoningSummary::Off,
                &ProviderContinuation::default(),
            )
            .expect("streaming Big Pickle request with OPENCODE_ZEN_KEY");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("data:"));
    }

    #[test]
    #[ignore = "requires OPENCODE_ZEN_KEY, network access, Big Pickle tool-call support, and acceptance of limited-free pricing/privacy caveats"]
    fn live_big_pickle_tool_call_requires_opencode_zen_key() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeZenClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_ZEN_KEY must be set");
        let messages = vec![ProviderMessage::user(
            "Use the find_files tool to look for Cargo.toml, then stop.",
        )];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let mut response = client
            .send_streaming_request(
                "opencode/big-pickle",
                &messages,
                128,
                Some(&catalog),
                ReasoningEffort::Auto,
                ReasoningSummary::Off,
                &ProviderContinuation::default(),
            )
            .expect("streaming Big Pickle tool request with OPENCODE_ZEN_KEY");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("tool_calls") || body.contains("find_files"));
    }
}
