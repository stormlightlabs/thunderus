//! Provider implementations for LLM APIs
//!
//! Supports multiple wire protocols:
//! - OpenAI Chat Completions
//! - Moonshot (Kimi) - OpenAI-compatible
//! - Z.ai (GLM) - OpenAI-compatible with quirks

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use thunderus_core::{Config, Message, Role};

pub mod conversation_loop;
mod openai;
mod streaming;

pub use conversation_loop::{ConversationEvent, ConversationLoop, run_conversation_loop};
pub use openai::{OpenAiClient, OpenAiMessage, OpenAiRequest, OpenAiResponse, ToolCall, ToolDefinition};
pub use streaming::{StreamChunk, StreamEvent};

/// Errors that can occur when calling providers
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Request timeout")]
    Timeout,
}

/// Result type for provider operations
pub type Result<T> = std::result::Result<T, ProviderError>;

/// A tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub id: String,
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

impl ToolInvocation {
    /// Parse arguments from JSON string
    pub fn parse_arguments(arguments_json: &str) -> Result<HashMap<String, serde_json::Value>> {
        let args: HashMap<String, serde_json::Value> = serde_json::from_str(arguments_json)?;
        Ok(args)
    }
}

/// A message from the model that may contain tool calls
#[derive(Debug, Clone)]
pub enum ModelResponse {
    Content {
        content: String,
        reasoning_content: Option<String>,
    },
    ToolCalls {
        tool_calls: Vec<ToolInvocation>,
    },
    ContentWithToolCalls {
        content: String,
        reasoning_content: Option<String>,
        tool_calls: Vec<ToolInvocation>,
    },
}

impl ModelResponse {
    /// Check if this response contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        matches!(self, Self::ToolCalls { .. } | Self::ContentWithToolCalls { .. })
    }

    /// Get tool calls if present
    pub fn tool_calls(&self) -> Option<&[ToolInvocation]> {
        match self {
            Self::ToolCalls { tool_calls } => Some(tool_calls),
            Self::ContentWithToolCalls { tool_calls, .. } => Some(tool_calls),
            _ => None,
        }
    }

    /// Get content if present
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::Content { content, .. } => Some(content),
            Self::ContentWithToolCalls { content, .. } => Some(content),
            _ => None,
        }
    }
}

/// A provider that can make LLM calls
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Send a completion request
    async fn complete(&self, messages: &[Message]) -> Result<CompletionResponse>;

    /// Send a completion request with tools
    async fn complete_with_tools(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<CompletionResponse>;

    /// Send a streaming completion request
    async fn complete_stream(&self, messages: &[Message]) -> Result<CompletionResponse>;

    /// Send a streaming request and emit delta events as they arrive.
    async fn complete_stream_with_events(
        &self, messages: &[Message], event_tx: &std::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse>;

    /// Get the default model for this provider
    fn default_model(&self) -> &str;
}

/// Response from a completion request
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub finish_reason: FinishReason,
    pub usage: Usage,
    pub model: String,
    pub tool_calls: Vec<ToolInvocation>,
}

impl CompletionResponse {
    /// Check if this response contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Convert to ModelResponse for easier handling
    pub fn to_model_response(&self) -> ModelResponse {
        if self.tool_calls.is_empty() {
            ModelResponse::Content { content: self.content.clone(), reasoning_content: self.reasoning_content.clone() }
        } else if self.content.is_empty() {
            ModelResponse::ToolCalls { tool_calls: self.tool_calls.clone() }
        } else {
            ModelResponse::ContentWithToolCalls {
                content: self.content.clone(),
                reasoning_content: self.reasoning_content.clone(),
                tool_calls: self.tool_calls.clone(),
            }
        }
    }
}

/// Why the generation stopped
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
}

/// Token usage information
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Tool result for sending back to the model
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    /// Create a successful tool result
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { tool_call_id: tool_call_id.into(), content: content.into(), is_error: false }
    }

    /// Create an error tool result
    pub fn error(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { tool_call_id: tool_call_id.into(), content: content.into(), is_error: true }
    }

    /// Convert to OpenAI tool message format
    pub fn to_openai_message(&self) -> OpenAiMessage {
        OpenAiMessage {
            role: "tool".to_string(),
            content: Some(self.content.clone()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(self.tool_call_id.clone()),
        }
    }
}

/// Create a provider instance based on name
pub fn create_provider(name: &str, config: &Config) -> Result<Box<dyn Provider>> {
    match name {
        "moonshot" | "kimi" => {
            let api_key = config
                .require_api_key(name)
                .map_err(|e| ProviderError::Config(e.to_string()))?;
            let base_url = config
                .providers
                .moonshot
                .as_ref()
                .and_then(|p| p.base_url.clone())
                .unwrap_or_else(|| "https://api.moonshot.ai/v1".to_string());
            let model = config
                .default_model
                .clone()
                .or_else(|| config.providers.moonshot.as_ref()?.default_model.clone())
                .unwrap_or_else(|| "kimi-k2.5".to_string());

            Ok(Box::new(MoonshotProvider::new(
                api_key,
                base_url,
                model,
                config.temperature,
            )))
        }
        "zhipu" | "glm" => {
            let api_key = config
                .require_api_key(name)
                .map_err(|e| ProviderError::Config(e.to_string()))?;
            let base_url = config
                .providers
                .zhipu
                .as_ref()
                .and_then(|p| p.base_url.clone())
                .unwrap_or_else(|| "https://api.z.ai/api/coding/paas/v4".to_string());
            let model = config
                .default_model
                .clone()
                .or_else(|| config.providers.zhipu.as_ref()?.default_model.clone())
                .unwrap_or_else(|| "glm-5".to_string());

            Ok(Box::new(ZhipuProvider::new(
                api_key,
                base_url,
                model,
                config.temperature,
            )))
        }
        _ => Err(ProviderError::Config(format!("Unknown provider: {}", name))),
    }
}

/// Moonshot (Kimi) provider implementation
pub struct MoonshotProvider {
    client: OpenAiClient,
    model: String,
}

impl MoonshotProvider {
    pub fn new(api_key: &str, base_url: String, model: String, temperature: f32) -> Self {
        let client = OpenAiClient::builder()
            .base_url(base_url)
            .api_key(api_key.to_string())
            .temperature(temperature.clamp(0.0, 1.0))
            .build();

        Self { client, model }
    }
}

#[async_trait::async_trait]
impl Provider for MoonshotProvider {
    fn name(&self) -> &str {
        "moonshot"
    }

    fn default_model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, messages: &[Message]) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete(request).await?;

        map_openai_response(response)
    }

    async fn complete_with_tools(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<CompletionResponse> {
        let request = build_openai_request_with_tools(&self.model, messages, tools);

        let response = self.client.complete(request).await?;

        map_openai_response(response)
    }

    async fn complete_stream(&self, messages: &[Message]) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete_stream(request).await?;

        Ok(CompletionResponse {
            content: response.content,
            reasoning_content: response.reasoning_content,
            finish_reason: map_finish_reason(response.finish_reason.as_deref()),
            usage: response
                .usage
                .map(|u| Usage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                })
                .unwrap_or_default(),
            model: response.model,
            tool_calls: Vec::new(),
        })
    }

    async fn complete_stream_with_events(
        &self, messages: &[Message], event_tx: &std::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete_stream_with_events(request, event_tx).await?;

        Ok(CompletionResponse {
            content: response.content,
            reasoning_content: response.reasoning_content,
            finish_reason: map_finish_reason(response.finish_reason.as_deref()),
            usage: response
                .usage
                .map(|u| Usage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                })
                .unwrap_or_default(),
            model: response.model,
            tool_calls: Vec::new(),
        })
    }
}

/// Zhipu (GLM) provider implementation
pub struct ZhipuProvider {
    client: OpenAiClient,
    model: String,
}

impl ZhipuProvider {
    pub fn new(api_key: &str, base_url: String, model: String, temperature: f32) -> Self {
        let client = OpenAiClient::builder()
            .base_url(base_url)
            .api_key(api_key.to_string())
            .temperature(temperature.clamp(0.0, 1.0))
            .build();

        Self { client, model }
    }
}

#[async_trait::async_trait]
impl Provider for ZhipuProvider {
    fn name(&self) -> &str {
        "zhipu"
    }

    fn default_model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, messages: &[Message]) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete(request).await?;

        map_openai_response(response)
    }

    async fn complete_with_tools(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<CompletionResponse> {
        let request = build_openai_request_with_tools(&self.model, messages, tools);

        let response = self.client.complete(request).await?;

        map_openai_response(response)
    }

    async fn complete_stream(&self, messages: &[Message]) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete_stream(request).await?;

        Ok(CompletionResponse {
            content: response.content,
            reasoning_content: response.reasoning_content,
            finish_reason: map_finish_reason(response.finish_reason.as_deref()),
            usage: response
                .usage
                .map(|u| Usage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                })
                .unwrap_or_default(),
            model: response.model,
            tool_calls: Vec::new(),
        })
    }

    async fn complete_stream_with_events(
        &self, messages: &[Message], event_tx: &std::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse> {
        let request = build_openai_request(&self.model, messages);

        let response = self.client.complete_stream_with_events(request, event_tx).await?;

        Ok(CompletionResponse {
            content: response.content,
            reasoning_content: response.reasoning_content,
            finish_reason: map_finish_reason(response.finish_reason.as_deref()),
            usage: response
                .usage
                .map(|u| Usage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                })
                .unwrap_or_default(),
            model: response.model,
            tool_calls: Vec::new(),
        })
    }
}

fn build_openai_request(model: &str, messages: &[Message]) -> OpenAiRequest {
    OpenAiRequest::new(
        model,
        messages
            .iter()
            .map(|m| OpenAiMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string(),
                content: Some(m.content.clone()),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
            })
            .collect(),
    )
}

fn build_openai_request_with_tools(model: &str, messages: &[Message], tools: &[ToolDefinition]) -> OpenAiRequest {
    let openai_messages: Vec<OpenAiMessage> = messages
        .iter()
        .map(|m| OpenAiMessage {
            role: match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            }
            .to_string(),
            content: Some(m.content.clone()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        })
        .collect();

    OpenAiRequest::with_tools(model, openai_messages, tools)
}

fn map_openai_response(response: OpenAiResponse) -> Result<CompletionResponse> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

    let tool_calls = choice
        .message
        .tool_calls
        .as_ref()
        .map(|tcs| {
            tcs.iter()
                .filter_map(|tc| {
                    if tc.type_field == "function" {
                        let args = ToolInvocation::parse_arguments(&tc.function.arguments).ok()?;
                        Some(ToolInvocation { id: tc.id.clone(), name: tc.function.name.clone(), arguments: args })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(CompletionResponse {
        content: choice.message.content.unwrap_or_default(),
        reasoning_content: choice.message.reasoning_content,
        finish_reason: map_finish_reason(choice.finish_reason.as_deref()),
        usage: Usage {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
        },
        model: response.model,
        tool_calls,
    })
}

fn map_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("content_filter") => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thunderus_core::{ProviderConfig, ProvidersConfig};

    #[test]
    fn test_create_provider_uses_global_default_model_when_set() {
        let config = Config {
            default_model: Some("global-model".to_string()),
            providers: ProvidersConfig {
                moonshot: Some(ProviderConfig {
                    api_key: Some("sk-test".to_string()),
                    base_url: None,
                    default_model: Some("provider-model".to_string()),
                }),
                zhipu: None,
            },
            ..Default::default()
        };

        let provider = create_provider("moonshot", &config).unwrap();
        assert_eq!(provider.default_model(), "global-model");
    }

    #[test]
    fn test_create_provider_uses_provider_default_when_global_missing() {
        let config = Config {
            default_model: None,
            providers: ProvidersConfig {
                moonshot: Some(ProviderConfig {
                    api_key: Some("sk-test".to_string()),
                    base_url: None,
                    default_model: Some("provider-model".to_string()),
                }),
                zhipu: None,
            },
            ..Default::default()
        };

        let provider = create_provider("kimi", &config).unwrap();
        assert_eq!(provider.default_model(), "provider-model");
    }

    #[test]
    fn test_create_provider_requires_api_key() {
        let config = Config::default();
        let result = create_provider("zhipu", &config);
        assert!(matches!(result, Err(ProviderError::Config(_))));
    }

    #[test]
    fn test_map_finish_reason_defaults_to_stop() {
        assert_eq!(map_finish_reason(Some("unexpected")), FinishReason::Stop);
        assert_eq!(map_finish_reason(None), FinishReason::Stop);
    }

    #[test]
    fn test_tool_invocation_parse_arguments() {
        let json = r#"{"path": "/test.txt", "limit": 100}"#;
        let args = ToolInvocation::parse_arguments(json).unwrap();
        assert_eq!(args.get("path").unwrap(), "/test.txt");
        assert_eq!(args.get("limit").unwrap(), &serde_json::json!(100));
    }

    #[test]
    fn test_model_response_has_tool_calls() {
        let response = ModelResponse::ToolCalls {
            tool_calls: vec![ToolInvocation {
                id: "call_1".to_string(),
                name: "read".to_string(),
                arguments: HashMap::new(),
            }],
        };
        assert!(response.has_tool_calls());
        assert_eq!(response.tool_calls().unwrap().len(), 1);
    }

    #[test]
    fn test_completion_response_has_tool_calls() {
        let response = CompletionResponse {
            content: String::new(),
            reasoning_content: None,
            finish_reason: FinishReason::ToolCalls,
            usage: Usage::default(),
            model: "test".to_string(),
            tool_calls: vec![ToolInvocation {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: HashMap::new(),
            }],
        };
        assert!(response.has_tool_calls());
        assert_eq!(response.tool_calls.len(), 1);
    }
}
