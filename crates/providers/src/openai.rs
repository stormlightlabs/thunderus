//! OpenAI Chat Completions protocol implementation
//!
//! This module implements the OpenAI Chat Completions API with tool support.

use super::streaming::{StreamEvent, StreamResponse, collect_stream};
use super::{ProviderError, Result};
use serde::{Deserialize, Serialize};

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub type_field: String,
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// Create a new tool definition from a schema
    pub fn from_schema(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            type_field: "function".to_string(),
            function: FunctionDefinition { name: name.into(), description: description.into(), parameters },
        }
    }
}

/// Function definition within a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call from the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

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
        let (event_tx, _event_rx) = std::sync::mpsc::channel::<StreamEvent>();
        self.complete_stream_with_events(request, &event_tx).await
    }

    /// Send a streaming completion request and emit parsed stream events.
    pub async fn complete_stream_with_events(
        &self, request: OpenAiRequest, event_tx: &std::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<StreamResponse> {
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
            collect_stream(response, event_tx).await
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

impl OpenAiRequest {
    pub fn new(model: impl Into<String>, messages: Vec<OpenAiMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stream: None,
            tools: None,
        }
    }

    pub fn with_tools(model: impl Into<String>, messages: Vec<OpenAiMessage>, tools: &[ToolDefinition]) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stream: None,
            tools: Some(tools.to_vec()),
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl OpenAiMessage {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a new assistant message with tool calls
    pub fn assistant_with_tool_calls(content: Option<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            reasoning_content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    /// Create a new tool message
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
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
            messages: vec![OpenAiMessage::system("You are helpful"), OpenAiMessage::user("Hello")],
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: None,
            stream: None,
            tools: None,
        };

        let json_str = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["model"], "gpt-4");
        assert_eq!(parsed["temperature"], 0.7);
        assert_eq!(parsed["max_tokens"], 100);
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_request_with_tools() {
        let tool = ToolDefinition::from_schema(
            "read",
            "Read a file",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        );

        let request = OpenAiRequest::with_tools("gpt-4", vec![OpenAiMessage::user("Read test.txt")], &[tool]);

        let json_str = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["tools"].is_array());
        assert_eq!(parsed["tools"][0]["type"], "function");
        assert_eq!(parsed["tools"][0]["function"]["name"], "read");
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
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello! How can I help you today?".to_string())
        );
        assert_eq!(
            response.choices[0].message.reasoning_content,
            Some("The user greeted me.".to_string())
        );
        assert_eq!(response.usage.total_tokens, 30);
    }

    #[test]
    fn test_response_with_tool_calls() {
        let json_response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\": \"/test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });

        let response: OpenAiResponse = serde_json::from_value(json_response).unwrap();

        assert_eq!(response.choices.len(), 1);
        let message = &response.choices[0].message;
        assert!(message.content.is_none());
        assert!(message.tool_calls.is_some());

        let tool_calls = message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_123");
        assert_eq!(tool_calls[0].type_field, "function");
        assert_eq!(tool_calls[0].function.name, "read");
        assert_eq!(tool_calls[0].function.arguments, "{\"path\": \"/test.txt\"}");
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

    #[test]
    fn test_message_helpers() {
        let system = OpenAiMessage::system("You are helpful");
        assert_eq!(system.role, "system");
        assert_eq!(system.content, Some("You are helpful".to_string()));

        let user = OpenAiMessage::user("Hello");
        assert_eq!(user.role, "user");
        assert_eq!(user.content, Some("Hello".to_string()));

        let assistant = OpenAiMessage::assistant("Hi there!");
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.content, Some("Hi there!".to_string()));

        let tool = OpenAiMessage::tool("call_123", "File contents");
        assert_eq!(tool.role, "tool");
        assert_eq!(tool.content, Some("File contents".to_string()));
        assert_eq!(tool.tool_call_id, Some("call_123".to_string()));
    }
}
