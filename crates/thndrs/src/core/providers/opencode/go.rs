//! OpenCode Go provider.
//!
//! OpenCode Go exposes both OpenAI-compatible chat completions and
//! Anthropic-compatible messages. The app-facing model id keeps the
//! documented `opencode-go/` prefix; the request body uses the raw
//! OpenCode Go model id.

use std::path::Path;

use crate::{
    WebSearchMode,
    app::AgentEvent,
    providers::{
        self, KnownModel, ProviderError, ProviderHttpClient, ProviderMessage, Result, StreamFormat, StreamingProvider,
    },
    thndrs_core::auth,
};

/// OpenCode Go base URL.
pub const BASE_URL: &str = "https://opencode.ai/zen/go/v1";

/// Environment variable name for the OpenCode Go API key.
pub const API_KEY_ENV: &str = "OPENCODE_GO_KEY";

/// User/config-facing model id prefix.
pub const MODEL_PREFIX: &str = "opencode-go/";

/// Required Anthropic version header value for `/messages`.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

pub const DEFAULT_RECOMMENDED_MAX_TOKENS: u32 = 32_768;

/// Endpoint family used by a model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointFamily {
    OpenAiChat,
    AnthropicMessages,
}

impl EndpointFamily {
    pub fn label(self) -> &'static str {
        match self {
            EndpointFamily::OpenAiChat => "chat/completions",
            EndpointFamily::AnthropicMessages => "messages",
        }
    }
}

/// OpenAI-compatible model list response from `GET /models`.
pub type ModelsResponse = super::ModelsResponse;

/// Model metadata currently returned by OpenCode Go.
pub type ModelInfo = super::ModelInfo;

/// Concrete OpenCode Go API client.
pub struct OpenCodeGoClient {
    http: ProviderHttpClient,
}

impl OpenCodeGoClient {
    /// Create a client from `OPENCODE_GO_KEY`, falling back to workspace `.env`.
    pub fn from_env_or_dotenv(workspace_root: &Path) -> Result<Self> {
        if let Some((api_key, source)) = auth::resolve_credential(API_KEY_ENV, workspace_root) {
            tracing::debug!(source = source.label(), "loaded OpenCode Go API key");
            Ok(Self::new(BASE_URL, &api_key))
        } else {
            tracing::error!(env = API_KEY_ENV, cwd = %workspace_root.display(), "missing OpenCode Go API key");
            Err(ProviderError::missing_api_key(API_KEY_ENV))
        }
    }

    /// Create a client with an explicit base URL and API key.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        OpenCodeGoClient { http: ProviderHttpClient::new(base_url, api_key) }
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

    /// Build the headers for an OpenAI-compatible chat-completions request.
    pub fn build_chat_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Authorization".to_string(), format!("Bearer {}", self.http.api_key())),
            ("Content-Type".to_string(), "application/json".to_string()),
        ]
    }

    /// Build the headers for an Anthropic-compatible messages request.
    pub fn build_messages_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Authorization".to_string(), format!("Bearer {}", self.http.api_key())),
            ("x-api-key".to_string(), self.http.api_key().to_string()),
            ("anthropic-version".to_string(), ANTHROPIC_VERSION.to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ]
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

    /// Build a request body for `POST /messages`.
    pub fn build_messages_request_body(
        model: &str, messages: &[ProviderMessage], max_tokens: u32, stream: bool, tools: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let raw_model = raw_model_id(model)?;
        Ok(providers::anthropic::build_messages_request_body(
            raw_model, messages, max_tokens, stream, tools,
        ))
    }

    /// Send a streaming request to the route selected by `model`.
    pub fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], max_tokens: u32, tools: Option<&serde_json::Value>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let raw_model = raw_model_id(model)?;
        let family = endpoint_family(raw_model);
        let (url, body, headers) = match family {
            EndpointFamily::OpenAiChat => (
                format!("{}/chat/completions", self.http.base_url()),
                Self::build_chat_request_body(model, messages, max_tokens, true, tools)?,
                self.build_chat_headers(),
            ),
            EndpointFamily::AnthropicMessages => (
                format!("{}/messages", self.http.base_url()),
                Self::build_messages_request_body(model, messages, max_tokens, true, tools)?,
                self.build_messages_headers(),
            ),
        };

        let mut request = self.http.agent().post(&url);
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

impl StreamingProvider for OpenCodeGoClient {
    type Metadata = Vec<ModelInfo>;

    fn name(&self) -> &'static str {
        "OpenCode Go"
    }

    fn load_status(&self) -> String {
        String::from("provider: loading OPENCODE_GO_KEY")
    }

    fn request_status(&self, model: &str, _search_mode: WebSearchMode) -> String {
        let route = raw_model_id(model)
            .map(endpoint_family)
            .map(|family| family.label())
            .unwrap_or("unknown");
        format!("provider: POST /zen/go/v1/{route} model={model}")
    }

    fn from_env_or_dotenv(root: &Path) -> Result<Self> {
        OpenCodeGoClient::from_env_or_dotenv(root)
    }

    fn load_metadata(&self) -> Result<Self::Metadata> {
        self.fetch_models()
    }

    fn metadata_loaded_event(&self, metadata: &Self::Metadata) -> Option<AgentEvent> {
        let mut items: Vec<(String, String)> = providers::umans::known_models()
            .into_iter()
            .map(|model| (model.id.to_string(), model.description.to_string()))
            .collect();
        items.extend(
            providers::opencode::zen::known_models()
                .into_iter()
                .map(|model| (model.id.to_string(), model.description.to_string())),
        );
        items.extend(model_picker_items(metadata));
        items.extend(
            providers::chatgpt_codex::known_models()
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

    /// TODO: Map reasoning effort and summaries when the OpenCode Go backend
    /// exposes model-specific controls for them.
    fn send_streaming_request(
        &self, model: &str, messages: &[ProviderMessage], request: &crate::providers::StreamingRequest<'_>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        OpenCodeGoClient::send_streaming_request(self, model, messages, request.max_tokens, Some(request.tools))
    }

    fn stream_format(&self, model: &str) -> Result<StreamFormat> {
        let raw_model = raw_model_id(model)?;
        Ok(match endpoint_family(raw_model) {
            EndpointFamily::OpenAiChat => StreamFormat::OpenAiChat,
            EndpointFamily::AnthropicMessages => StreamFormat::AnthropicMessages,
        })
    }

    fn request_error_message(error: &ProviderError) -> String {
        error_message(error)
    }

    fn is_retryable_request_error(error: &ProviderError) -> bool {
        is_retryable_error(error)
    }
}

/// Whether `model` is an OpenCode Go model id.
pub fn is_model_id(model: &str) -> bool {
    model.strip_prefix(MODEL_PREFIX).is_some_and(|raw| !raw.is_empty())
}

/// Strip `opencode-go/` from a model id.
pub fn raw_model_id(model: &str) -> Result<&str> {
    model
        .strip_prefix(MODEL_PREFIX)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| ProviderError::invalid_model_id("OpenCode Go", MODEL_PREFIX, model))
}

/// Resolve the endpoint family for a raw OpenCode Go model id.
///
/// The public `/models` response currently exposes ids only, so route family is
/// derived from the documented families plus observed id prefixes.
pub fn endpoint_family(raw_model: &str) -> EndpointFamily {
    if raw_model.starts_with("minimax-") || raw_model.starts_with("qwen") {
        EndpointFamily::AnthropicMessages
    } else {
        EndpointFamily::OpenAiChat
    }
}

/// Current OpenCode Go models from the public docs and live model list.
pub fn known_models() -> Vec<KnownModel> {
    vec![
        KnownModel { id: "opencode-go/kimi-k2.7-code", description: "OpenCode Go Kimi K2.7 Code, tools and reasoning" },
        KnownModel { id: "opencode-go/deepseek-v4-flash", description: "OpenCode Go fast low-cost chat route" },
        KnownModel { id: "opencode-go/glm-5.2", description: "OpenCode Go GLM route" },
        KnownModel { id: "opencode-go/minimax-m3", description: "OpenCode Go Anthropic-compatible route" },
        KnownModel { id: "opencode-go/qwen3.7-plus", description: "OpenCode Go Qwen Anthropic-compatible route" },
    ]
}

pub fn model_picker_items(models: &[ModelInfo]) -> Vec<(String, String)> {
    let mut items: Vec<(String, String)> = models
        .iter()
        .map(|info| {
            let family = endpoint_family(&info.id);
            (
                format!("{MODEL_PREFIX}{}", info.id),
                format!("OpenCode Go · {}", family.label()),
            )
        })
        .collect();
    items.sort_by(|left, right| left.0.cmp(&right.0));
    items
}

pub fn model_status(model: &str, models: &[ModelInfo]) -> Option<String> {
    let raw = raw_model_id(model).ok()?;
    models.iter().find(|info| info.id == raw).map(|info| {
        format!(
            "model: opencode-go/{}  OpenCode Go · {}",
            info.id,
            endpoint_family(&info.id).label()
        )
    })
}

pub fn recommended_max_tokens_for_model(_model: &str, _models: Option<&Vec<ModelInfo>>) -> u32 {
    DEFAULT_RECOMMENDED_MAX_TOKENS
}

pub fn is_retryable_error(err: &ProviderError) -> bool {
    err.is_retryable()
}

/// Convert an OpenCode Go error into a human-readable failure string.
pub fn error_message(err: &ProviderError) -> String {
    err.failure_message("rate limit or usage limit exceeded")
}

/// Try to validate an OpenCode Go API key with a lightweight model-list request.
pub fn validate_api_key(api_key: &str) -> std::result::Result<(), String> {
    probe_api_key(api_key).map_err(|error| format!("validation failed: {error}"))
}

pub fn probe_api_key(api_key: &str) -> Result<()> {
    validate_api_key_at(BASE_URL, api_key)
}

fn validate_api_key_at(base_url: &str, api_key: &str) -> Result<()> {
    OpenCodeGoClient::new(base_url, api_key).fetch_models().map(|_| ())
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
    use crate::providers::{ProviderContentBlock, ProviderMessage};

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
        assert_eq!(raw_model_id("opencode-go/kimi-k2.7-code").unwrap(), "kimi-k2.7-code");
        assert!(matches!(
            raw_model_id("kimi-k2.7-code"),
            Err(ProviderError::InvalidModelId { .. })
        ));
    }

    #[test]
    fn endpoint_family_uses_documented_prefixes() {
        assert_eq!(endpoint_family("kimi-k2.7-code"), EndpointFamily::OpenAiChat);
        assert_eq!(endpoint_family("deepseek-v4-flash"), EndpointFamily::OpenAiChat);
        assert_eq!(endpoint_family("minimax-m3"), EndpointFamily::AnthropicMessages);
        assert_eq!(endpoint_family("qwen3.7-plus"), EndpointFamily::AnthropicMessages);
    }

    #[test]
    fn build_chat_request_body_converts_tools() {
        let messages = vec![ProviderMessage::user("find files")];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body = OpenCodeGoClient::build_chat_request_body(
            "opencode-go/kimi-k2.7-code",
            &messages,
            4096,
            true,
            Some(&catalog),
        )
        .expect("body");

        assert_eq!(body["model"], "kimi-k2.7-code");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], defs[0].name.as_ref());
        assert_eq!(body["tools"][0]["function"]["parameters"], defs[0].input_schema);
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn build_chat_request_body_converts_tool_history() {
        let messages = vec![
            ProviderMessage::assistant_blocks(vec![
                ProviderContentBlock::Text { text: "Looking.".to_string() },
                ProviderContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "find_files".to_string(),
                    input: serde_json::json!({"pattern":"Cargo"}),
                },
            ]),
            ProviderMessage::tool_result("call_1", "Cargo.toml", false),
        ];

        let body = OpenCodeGoClient::build_chat_request_body("opencode-go/kimi-k2.7-code", &messages, 4096, true, None)
            .expect("body");

        assert_eq!(body["messages"][0]["role"], "assistant");
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(body["messages"][1]["role"], "tool");
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    }

    #[test]
    fn build_chat_request_body_converts_user_image_blocks() {
        let messages = vec![ProviderMessage::user_blocks(vec![
            ProviderContentBlock::Text { text: "describe this".to_string() },
            ProviderContentBlock::Image {
                source: crate::providers::ProviderImageSource::Base64 {
                    media_type: "image/png".to_string(),
                    data: "aGVsbG8=".to_string(),
                },
            },
        ])];

        let body = OpenCodeGoClient::build_chat_request_body("opencode-go/kimi-k2.7-code", &messages, 4096, true, None)
            .expect("body");

        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "describe this");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,aGVsbG8=");
    }

    #[test]
    fn build_messages_request_body_uses_anthropic_shape() {
        let messages = vec![ProviderMessage::user("hello")];
        let defs = crate::tools::tool_definitions();
        let catalog = crate::tools::tool_catalog_schemas(&defs);
        let body = OpenCodeGoClient::build_messages_request_body(
            "opencode-go/minimax-m3",
            &messages,
            1024,
            true,
            Some(&catalog),
        )
        .expect("body");

        assert_eq!(body["model"], "minimax-m3");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["tools"][0]["name"], defs[0].name.as_ref());
    }

    #[test]
    fn build_headers_include_expected_auth() {
        let client = OpenCodeGoClient::new(BASE_URL, "go-test-key");
        let chat: HashMap<String, String> = client.build_chat_headers().into_iter().collect();
        assert_eq!(chat.get("Authorization").unwrap(), "Bearer go-test-key");

        let messages: HashMap<String, String> = client.build_messages_headers().into_iter().collect();
        assert_eq!(messages.get("Authorization").unwrap(), "Bearer go-test-key");
        assert_eq!(messages.get("x-api-key").unwrap(), "go-test-key");
        assert_eq!(messages.get("anthropic-version").unwrap(), ANTHROPIC_VERSION);
    }

    #[test]
    fn from_env_or_dotenv_missing_key_returns_error() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let result = OpenCodeGoClient::from_env_or_dotenv(dir.path());
        assert!(matches!(result, Err(ProviderError::MissingApiKey { env }) if env == API_KEY_ENV));
    }

    #[test]
    fn from_env_or_dotenv_reads_workspace_env_file() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "OPENCODE_GO_KEY=go-dotenv-key\n").unwrap();

        let client = OpenCodeGoClient::from_env_or_dotenv(dir.path()).unwrap();
        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();

        assert_eq!(headers.get("Authorization").unwrap(), "Bearer go-dotenv-key");
    }

    #[test]
    fn from_env_or_dotenv_reads_exported_quoted_env_file_value() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "export OPENCODE_GO_KEY=\"go-quoted-dotenv-key\"\n",
        )
        .unwrap();

        let client = OpenCodeGoClient::from_env_or_dotenv(dir.path()).unwrap();
        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();

        assert_eq!(headers.get("Authorization").unwrap(), "Bearer go-quoted-dotenv-key");
    }

    #[test]
    fn from_env_or_dotenv_reads_global_credential_store() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(home.join(".thndrs")).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(
            home.join(".thndrs").join("credentials.env"),
            "OPENCODE_GO_KEY=go-global-key\n",
        )
        .unwrap();

        let old_home = env::var_os("HOME");
        unsafe { env::set_var("HOME", &home) };
        let client = OpenCodeGoClient::from_env_or_dotenv(&workspace).unwrap();
        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }

        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer go-global-key");
    }

    #[test]
    fn from_env_or_dotenv_reads_project_credential_store() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(workspace.join(".thndrs")).unwrap();
        std::fs::write(
            workspace.join(".thndrs").join("credentials.env"),
            "OPENCODE_GO_KEY=go-project-key\n",
        )
        .unwrap();

        let old_home = env::var_os("HOME");
        unsafe { env::set_var("HOME", &home) };
        let client = OpenCodeGoClient::from_env_or_dotenv(&workspace).unwrap();
        unsafe {
            if let Some(home) = old_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }

        let headers: HashMap<String, String> = client.build_chat_headers().into_iter().collect();
        assert_eq!(headers.get("Authorization").unwrap(), "Bearer go-project-key");
    }

    #[test]
    fn missing_key_error_includes_setup_hint() {
        let _guard = crate::test_env::lock();
        unsafe {
            env::remove_var(API_KEY_ENV);
        }
        let dir = tempfile::tempdir().unwrap();
        let result = match OpenCodeGoClient::from_env_or_dotenv(dir.path()) {
            Ok(_) => panic!("missing key should fail"),
            Err(err) => err,
        };
        let message = result.to_string();
        assert!(message.contains("thndrs setup"));
        assert!(message.contains("thndrs login"));
        assert!(!message.contains("go-"));
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
            r#"{"object":"list","data":[{"id":"kimi-k2.7-code","object":"model","created":1,"owned_by":"opencode"}]}"#,
        );
        let old_home = env::var_os("HOME");
        unsafe { env::set_var("HOME", &home) };
        validate_api_key_at(&base_url, "go-validation").expect("validation succeeds");
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
        let base_url = mock_models_response_server("401 Unauthorized", r#"{"error":"invalid key"}"#);

        let error = validate_api_key_at(&base_url, "rejected-key").expect_err("credential should be rejected");

        assert!(matches!(error, ProviderError::Status { code: 401, .. }));
    }

    #[test]
    fn parse_models_response() {
        let json = r#"{"object":"list","data":[{"id":"kimi-k2.7-code","object":"model","created":1782964872,"owned_by":"opencode"}]}"#;
        let response: ModelsResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(response.data[0].id, "kimi-k2.7-code");
    }

    #[test]
    fn model_picker_items_prefix_live_ids() {
        let models = vec![ModelInfo {
            id: "kimi-k2.7-code".to_string(),
            object: "model".to_string(),
            created: 1,
            owned_by: "opencode".to_string(),
        }];
        let items = model_picker_items(&models);
        assert_eq!(items[0].0, "opencode-go/kimi-k2.7-code");
        assert!(items[0].1.contains("OpenCode Go"));
    }

    #[test]
    fn metadata_loaded_event_keeps_opencode_zen_known_models() {
        let client = OpenCodeGoClient::new(BASE_URL, "go-test-key");
        let metadata = vec![ModelInfo {
            id: "kimi-k2.7-code".to_string(),
            object: "model".to_string(),
            created: 1,
            owned_by: "opencode".to_string(),
        }];
        let Some(AgentEvent::ModelMetadataLoaded(items)) = client.metadata_loaded_event(&metadata) else {
            panic!("expected model metadata event");
        };

        assert!(items.iter().any(|(id, _)| id == "opencode/big-pickle"));
        assert!(items.iter().any(|(id, _)| id == "opencode-go/kimi-k2.7-code"));
    }

    #[test]
    fn parse_chat_sse_text_reasoning_usage_and_done() {
        let data = r#"{"choices":[{"finish_reason":null,"delta":{"content":"ok","reasoning_content":"think"}}],"usage":{"prompt_tokens":2,"completion_tokens":3}}"#;
        let events = parse_chat_sse_event(data);
        assert!(events.contains(&ChatSseEvent::TextDelta("ok".to_string())));
        assert!(events.contains(&ChatSseEvent::ReasoningDelta("think".to_string())));
        assert!(events.contains(&ChatSseEvent::Usage { input_tokens: 2, output_tokens: 3 }));
        assert_eq!(parse_chat_sse_event("[DONE]"), vec![ChatSseEvent::Done]);
    }

    #[test]
    fn parse_chat_sse_tool_call_deltas() {
        let start = r#"{"choices":[{"finish_reason":null,"delta":{"tool_calls":[{"index":0,"id":"lookup_0","type":"function","function":{"name":"lookup","arguments":""}}]}}]}"#;
        let args = r#"{"choices":[{"finish_reason":null,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"query\":\"abc\"}"}}]}}]}"#;
        assert_eq!(
            parse_chat_sse_event(start),
            vec![ChatSseEvent::ToolCallStart { index: 0, id: "lookup_0".to_string(), name: "lookup".to_string() }]
        );
        assert_eq!(
            parse_chat_sse_event(args),
            vec![ChatSseEvent::ToolCallArgumentsDelta { index: 0, arguments: "{\"query\":\"abc\"}".to_string() }]
        );
    }

    #[test]
    fn retryable_error_classification_matches_policy() {
        assert!(!is_retryable_error(&ProviderError::missing_api_key(API_KEY_ENV)));
        assert!(!is_retryable_error(&ProviderError::Status {
            code: 401,
            body: "unauthorized".into()
        }));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 429,
            body: "limit".into()
        }));
        assert!(is_retryable_error(&ProviderError::Status {
            code: 503,
            body: "unavailable".into()
        }));
    }

    #[test]
    #[ignore = "requires OPENCODE_GO_KEY and network access"]
    fn live_models() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeGoClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_GO_KEY must be set");
        let models = client.fetch_models().expect("fetch models");
        assert!(models.iter().any(|model| model.id == "kimi-k2.7-code"));
    }

    #[test]
    #[ignore = "requires OPENCODE_GO_KEY and network access"]
    fn live_chat_stream() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeGoClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_GO_KEY must be set");
        let messages = vec![ProviderMessage::user("Reply with exactly: ok")];
        let mut response = client
            .send_streaming_request("opencode-go/deepseek-v4-flash", &messages, 32, None)
            .expect("streaming chat request");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("data: "));
        assert!(body.contains("[DONE]"));
    }

    #[test]
    #[ignore = "requires OPENCODE_GO_KEY and network access"]
    fn live_messages_stream() {
        let workspace_root = env::current_dir().expect("current dir");
        let client = OpenCodeGoClient::from_env_or_dotenv(&workspace_root).expect("OPENCODE_GO_KEY must be set");
        let messages = vec![ProviderMessage::user("Reply with exactly: ok")];
        let mut response = client
            .send_streaming_request("opencode-go/minimax-m3", &messages, 16, None)
            .expect("streaming messages request");
        let body = response.body_mut().read_to_string().expect("read body");
        assert!(body.contains("event: message_start"));
        assert!(body.contains("event: message_stop"));
    }
}
