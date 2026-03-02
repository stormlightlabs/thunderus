//! Provider implementations for LLM APIs
//!
//! Supports multiple wire protocols:
//! - OpenAI Chat Completions
//! - Moonshot (Kimi) - OpenAI-compatible
//! - Z.ai (GLM) - OpenAI-compatible with quirks

use thiserror::Error;
use thunderus_core::{Config, Message, Role};

mod openai;
mod streaming;

pub use openai::{OpenAiClient, OpenAiMessage, OpenAiRequest, OpenAiResponse};
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

/// A provider that can make LLM calls
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Send a completion request
    async fn complete(&self, messages: &[Message]) -> Result<CompletionResponse>;

    /// Send a streaming completion request
    async fn complete_stream(&self, messages: &[Message]) -> Result<CompletionResponse>;

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
                content: m.content.clone(),
                reasoning_content: None,
            })
            .collect(),
    )
}

fn map_openai_response(response: OpenAiResponse) -> Result<CompletionResponse> {
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

    Ok(CompletionResponse {
        content: if choice.message.content.is_empty() { String::new() } else { choice.message.content },
        reasoning_content: choice.message.reasoning_content,
        finish_reason: map_finish_reason(choice.finish_reason.as_deref()),
        usage: Usage {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
        },
        model: response.model,
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
