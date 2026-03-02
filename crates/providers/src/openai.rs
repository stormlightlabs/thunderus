//! OpenAI Chat Completions protocol implementation
//!
//! This module implements the OpenAI Chat Completions API.

use crate::streaming::{StreamResponse, collect_stream};
use crate::{ProviderError, Result};
use serde::{Deserialize, Serialize};

/// HTTP client for OpenAI-compatible APIs
#[derive(Clone)]
pub struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    temperature: f32,
}

impl OpenAiClient {
    /// Create a new client builder
    pub fn builder() -> OpenAiClientBuilder {
        OpenAiClientBuilder::default()
    }

    /// Send a completion request
    pub async fn complete(&self, request: OpenAiRequest) -> Result<OpenAiResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request.with_temperature(self.temperature))
            .send()
            .await?;

        let status = response.status();

        if status.is_success() {
            let body: OpenAiResponse = response.json().await?;
            Ok(body)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(ProviderError::Api(format!("HTTP {}: {}", status.as_u16(), error_text)))
        }
    }

    /// Send a streaming completion request
    pub async fn complete_stream(&self, request: OpenAiRequest) -> Result<StreamResponse> {
        let url = format!("{}/chat/completions", self.base_url);

        let streaming_request = request.with_temperature(self.temperature).with_stream(true);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&streaming_request)
            .send()
            .await?;

        let status = response.status();

        if status.is_success() {
            collect_stream(response).await
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(ProviderError::Api(format!("HTTP {}: {}", status.as_u16(), error_text)))
        }
    }
}

/// Builder for OpenAiClient
#[derive(Default)]
pub struct OpenAiClientBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    temperature: f32,
    timeout: Option<std::time::Duration>,
}

impl OpenAiClientBuilder {
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = temp.clamp(0.0, 1.0);
        self
    }

    pub fn timeout(mut self, duration: std::time::Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn build(self) -> OpenAiClient {
        let http = reqwest::Client::builder().timeout(self.timeout.unwrap_or(std::time::Duration::from_secs(120)));

        OpenAiClient {
            http: http.build().expect("Failed to build HTTP client"),
            base_url: self.base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            api_key: self.api_key.expect("API key is required"),
            temperature: self.temperature,
        }
    }
}

/// Request body for chat completions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl OpenAiRequest {
    pub fn new(model: impl Into<String>, messages: Vec<OpenAiMessage>) -> Self {
        Self { model: model.into(), messages, temperature: None, max_tokens: None, top_p: None, stream: None }
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp.clamp(0.0, 1.0));
        self
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }
}

/// Message in OpenAI format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// Response from chat completions endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChoice>,
    pub usage: OpenAiUsage,
}

/// A choice in the response
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAiChoice {
    pub index: u32,
    pub message: OpenAiMessage,
    pub finish_reason: Option<String>,
}

/// Token usage information
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OpenAiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_request_serialization() {
        let request = OpenAiRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: "You are helpful".to_string(),
                    reasoning_content: None,
                },
                OpenAiMessage { role: "user".to_string(), content: "Hello".to_string(), reasoning_content: None },
            ],
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: None,
            stream: None,
        };

        let json_str = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["model"], "gpt-4");
        assert_eq!(parsed["temperature"], 0.7);
        assert_eq!(parsed["max_tokens"], 100);
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_response_deserialization() {
        let json_response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you today?",
                    "reasoning_content": "The user greeted me."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });

        let response: OpenAiResponse = serde_json::from_value(json_response).unwrap();

        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello! How can I help you today?");
        assert_eq!(
            response.choices[0].message.reasoning_content,
            Some("The user greeted me.".to_string())
        );
        assert_eq!(response.usage.total_tokens, 30);
    }

    #[test]
    fn test_temperature_clamping() {
        let request = OpenAiRequest::new("test", vec![]).with_temperature(1.5);
        assert_eq!(request.temperature, Some(1.0));

        let request2 = OpenAiRequest::new("test", vec![]).with_temperature(-0.5);
        assert_eq!(request2.temperature, Some(0.0));
    }

    #[test]
    fn test_client_builder() {
        let client = OpenAiClient::builder()
            .base_url("https://api.example.com/v1")
            .api_key("sk-test")
            .temperature(0.8)
            .timeout(std::time::Duration::from_secs(30))
            .build();

        assert_eq!(client.base_url, "https://api.example.com/v1");
        assert_eq!(client.api_key, "sk-test");
        assert_eq!(client.temperature, 0.8);
    }
}
