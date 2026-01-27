use crate::types::*;
use thunderus_core::config::GeminiThinkingLevel;

use eventsource_stream::Eventsource;
use futures::{StreamExt, stream::Stream};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use thunderus_core::Result;

/// Parsed chunk with metadata for debugging/logging
#[derive(Debug, Clone)]
pub struct ParsedChunk {
    pub event: StreamEvent,
    pub request_id: Option<String>,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
}

/// Generic provider trait for LLM backends
#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    /// Stream chat completion with tools support
    async fn stream_chat<'a>(
        &'a self, request: ChatRequest, cancel_token: CancelToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>>;
}

/// GLM-4.7 provider implementation
pub struct GlmProvider {
    client: HttpClient,
    api_key: String,
    base_url: String,
    model: String,
    thinking_enabled: bool,
    thinking_preserved: bool,
}

impl GlmProvider {
    pub fn new(
        api_key: String, model: String, base_url: Option<String>, thinking_enabled: bool, thinking_preserved: bool,
    ) -> Self {
        Self {
            client: HttpClient::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.z.ai/api/paas/v4".to_string()),
            thinking_enabled,
            thinking_preserved,
        }
    }

    /// Check if model is a flash variant
    pub fn is_flash_model(&self) -> bool {
        self.model.contains("flash")
    }

    /// Check if model is flashx variant
    pub fn is_flashx_model(&self) -> bool {
        self.model.contains("flashx")
    }

    /// Check if model is flagship variant
    pub fn is_flagship_model(&self) -> bool {
        !self.is_flash_model() && !self.is_flashx_model()
    }

    /// Convert ChatRequest to GLM API format
    fn to_glm_request(&self, request: &ChatRequest) -> Result<GlmChatRequest> {
        let messages: Vec<GlmMessage> = request
            .messages
            .iter()
            .map(|msg| GlmMessage {
                role: match msg.role {
                    Role::System => "system".to_string(),
                    Role::User => "user".to_string(),
                    Role::Assistant => "assistant".to_string(),
                    Role::Tool => "tool".to_string(),
                },
                content: msg.content.clone(),
                tool_call_id: msg.tool_call_id.clone(),
                tool_calls: msg.tool_calls.clone(),
            })
            .collect();

        let thinking = if self.thinking_enabled {
            Some(GlmThinking { type_: "enabled".to_string(), clear_thinking: Some(!self.thinking_preserved) })
        } else {
            None
        };

        Ok(GlmChatRequest {
            model: self.model.clone(),
            messages,
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            thinking,
        })
    }

    /// Parse SSE chunk into ParsedChunk with metadata
    fn parse_chunk(&self, chunk: &str) -> ParsedChunk {
        if chunk.trim().is_empty() || chunk.starts_with("[DONE]") {
            return ParsedChunk { event: StreamEvent::Done, request_id: None, model: None, finish_reason: None };
        }

        match serde_json::from_str::<GlmChunk>(chunk) {
            Ok(data) => {
                let request_id = data.id.clone();
                let model = data.model.clone();
                let mut finish_reason = None;

                if let Some(choices) = data.choices
                    && let Some(choice) = choices.first()
                {
                    finish_reason = choice.finish_reason.clone();
                    let delta = &choice.delta;

                    if let Some(content) = &delta.content {
                        if let Some(ref role) = delta.role {
                            tracing::debug!(
                                request_id = ?request_id,
                                role = %role,
                                content_len = content.len(),
                                "GLM content chunk with role"
                            );
                        }
                        return ParsedChunk {
                            event: StreamEvent::Token(content.clone()),
                            request_id,
                            model,
                            finish_reason,
                        };
                    }

                    if let Some(tool_calls) = &delta.tool_calls
                        && !tool_calls.is_empty()
                    {
                        tracing::debug!(
                            request_id = ?request_id,
                            tool_calls_count = tool_calls.len(),
                            tool_call_types = ?tool_calls.iter().map(|tc| tc.r#type.as_deref().unwrap_or("unknown")).collect::<Vec<_>>(),
                            "GLM tool calls received"
                        );
                        let calls: Vec<ToolCall> = tool_calls
                            .iter()
                            .filter_map(|tc| {
                                tc.function.as_ref().map(|func| ToolCall {
                                    id: tc.id.clone().unwrap_or_default(),
                                    call_type: tc.r#type.clone().unwrap_or_else(|| "function".to_string()),
                                    function: FunctionCall {
                                        name: func.name.clone().unwrap_or_default(),
                                        arguments: serde_json::from_str(&func.arguments.clone().unwrap_or_default())
                                            .unwrap_or(serde_json::Value::Null),
                                    },
                                })
                            })
                            .collect();

                        if !calls.is_empty() {
                            return ParsedChunk {
                                event: StreamEvent::ToolCall(calls),
                                request_id,
                                model,
                                finish_reason,
                            };
                        }
                    }

                    if let Some(ref reasoning) = delta.reasoning_content {
                        return ParsedChunk {
                            event: StreamEvent::Token(format!("<thinking>{}</thinking>", reasoning)),
                            request_id,
                            model,
                            finish_reason,
                        };
                    }
                }
                ParsedChunk { event: StreamEvent::Done, request_id, model, finish_reason }
            }
            Err(_) => ParsedChunk {
                event: StreamEvent::Error(format!("Failed to parse chunk: {}", chunk)),
                request_id: None,
                model: None,
                finish_reason: None,
            },
        }
    }
}

#[async_trait::async_trait]
impl Provider for GlmProvider {
    async fn stream_chat<'a>(
        &'a self, request: ChatRequest, cancel_token: CancelToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>> {
        let glm_request = self.to_glm_request(&request)?;
        let url = format!("{}/chat/completions", self.base_url);
        let cancel_token_clone = cancel_token.clone();

        let stream = async_stream::stream! {
            if cancel_token.is_cancelled() {
                yield StreamEvent::Error("Cancelled before request".to_string());
                return;
            }

            let response = match self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&glm_request)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    yield StreamEvent::Error(format!("GLM request failed: {}", e));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                yield StreamEvent::Error(format!("GLM API error: {} - {}", status, body));
                return;
            }

            let eventsource = response.bytes_stream().eventsource();
            tokio::pin!(eventsource);

            while let Some(event_result) = eventsource.next().await {
                if cancel_token_clone.is_cancelled() {
                    yield StreamEvent::Error("Cancelled by user".to_string());
                    return;
                }

                match event_result {
                    Ok(event) => {
                        let parsed = self.parse_chunk(&event.data);
                        let is_done = matches!(parsed.event, StreamEvent::Done);

                        if is_done
                            && let Some(ref reason) = parsed.finish_reason {
                                if let Ok(chunk_data) = serde_json::from_str::<GlmChunk>(&event.data) {
                                    tracing::debug!(
                                        request_id = ?parsed.request_id,
                                        model = ?parsed.model,
                                        object = ?chunk_data.object,
                                        created = ?chunk_data.created,
                                        index = ?chunk_data.choices.as_ref().and_then(|c| c.first()).and_then(|c| c.index),
                                        finish_reason = %reason,
                                        "GLM stream completed"
                                    );
                                } else {
                                    tracing::debug!(
                                        request_id = ?parsed.request_id,
                                        model = ?parsed.model,
                                        finish_reason = %reason,
                                        "GLM stream completed"
                                    );
                                }
                            }

                        yield parsed.event;

                        if is_done {
                            break;
                        }
                    }
                    Err(e) => {
                        yield StreamEvent::Error(format!("SSE error: {}", e));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// GLM API request format
#[derive(Debug, Serialize)]
struct GlmChatRequest {
    model: String,
    messages: Vec<GlmMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<GlmThinking>,
}

#[derive(Debug, Serialize)]
struct GlmMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize)]
struct GlmThinking {
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    clear_thinking: Option<bool>,
}

/// GLM SSE chunk format
#[derive(Debug, Deserialize)]
struct GlmChunk {
    id: Option<String>,
    object: Option<String>,
    created: Option<u64>,
    model: Option<String>,
    choices: Option<Vec<GlmChoice>>,
}

#[derive(Debug, Deserialize)]
struct GlmChoice {
    index: Option<u32>,
    delta: GlmDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GlmDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<GlmToolCall>>,
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GlmToolCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    function: Option<GlmFunction>,
}

#[derive(Debug, Deserialize, Clone)]
struct GlmFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// Gemini provider implementation
pub struct GeminiProvider {
    client: HttpClient,
    api_key: String,
    base_url: String,
    model: String,
    thinking_level: GeminiThinkingLevel,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>, thinking_level: GeminiThinkingLevel) -> Self {
        Self {
            client: HttpClient::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string()),
            thinking_level,
        }
    }

    /// Get thinking level as string for API
    fn thinking_level_str(&self) -> &str {
        match self.thinking_level {
            GeminiThinkingLevel::Minimal => "minimal",
            GeminiThinkingLevel::Low => "low",
            GeminiThinkingLevel::Medium => "medium",
            GeminiThinkingLevel::High => "high",
        }
    }

    /// Convert ChatRequest to Gemini API format
    fn to_gemini_request(&self, request: &ChatRequest) -> Result<GeminiChatRequest> {
        let mut system_instruction = None;
        let mut contents: Vec<GeminiContent> = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::System => {
                    system_instruction = Some(GeminiSystemInstruction {
                        parts: vec![GeminiPart { text: Some(msg.content.clone()), ..Default::default() }],
                    });
                }
                Role::User => {
                    contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart { text: Some(msg.content.clone()), ..Default::default() }],
                    });
                }
                Role::Assistant => {
                    let mut parts: Vec<GeminiPart> =
                        vec![GeminiPart { text: Some(msg.content.clone()), ..Default::default() }];
                    if let Some(ref tool_calls) = msg.tool_calls {
                        for tc in tool_calls {
                            parts.push(GeminiPart {
                                function_call: Some(GeminiFunctionCall {
                                    name: tc.function.name.clone(),
                                    args: tc.function.arguments.clone(),
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    contents.push(GeminiContent { role: "assistant".to_string(), parts });
                }
                Role::Tool => {
                    if let Some(ref tool_call_id) = msg.tool_call_id {
                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts: vec![GeminiPart {
                                function_response: Some(GeminiFunctionResponse {
                                    name: tool_call_id.clone(),
                                    response: serde_json::from_str(&msg.content).unwrap_or(serde_json::Value::Null),
                                }),
                                ..Default::default()
                            }],
                        });
                    }
                }
            }
        }

        let tools = request.tools.as_ref().map(|tools| {
            vec![GeminiTool {
                function_declarations: tools
                    .iter()
                    .map(|spec| GeminiFunctionDeclaration {
                        name: spec.function.name.clone(),
                        description: spec.function.description.clone(),
                        parameters: Self::convert_to_uppercase_parameters(&spec.function.parameters),
                    })
                    .collect(),
            }]
        });

        let generation_config = Some(GeminiGenerationConfig {
            temperature: request.temperature,
            max_output_tokens: request.max_tokens,
            thinking_config: Some(GeminiThinkingConfig { thinking_level: self.thinking_level_str().to_string() }),
            tool_config: Some(GeminiToolConfig {
                function_calling_config: Some(GeminiFunctionCallingConfig {
                    mode: "AUTO".to_string(),
                    stream_function_call_arguments: true,
                }),
            }),
        });

        Ok(GeminiChatRequest { contents, system_instruction, tools, generation_config })
    }

    /// Convert parameters to uppercase types for Gemini 3 (STRING, INTEGER, OBJECT, etc.)
    fn convert_to_uppercase_parameters(params: &ToolParameter) -> ToolParameter {
        match params {
            ToolParameter::String { description } => ToolParameter::String { description: description.clone() },
            ToolParameter::Number { description } => ToolParameter::Number { description: description.clone() },
            ToolParameter::Boolean { description } => ToolParameter::Boolean { description: description.clone() },
            ToolParameter::Array { items, description } => ToolParameter::Array {
                items: Box::new(Self::convert_to_uppercase_parameters(items)),
                description: description.clone(),
            },
            ToolParameter::Object { properties, description, required } => {
                let converted_props = properties
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::convert_to_uppercase_parameters(v)))
                    .collect();
                ToolParameter::Object {
                    properties: converted_props,
                    description: description.clone(),
                    required: required.clone(),
                }
            }
        }
    }

    /// Parse Gemini API chunk into ParsedChunk with metadata
    fn parse_chunk(&self, chunk: &str) -> ParsedChunk {
        if chunk.trim().is_empty() {
            return ParsedChunk { event: StreamEvent::Done, request_id: None, model: None, finish_reason: None };
        }

        match serde_json::from_str::<GeminiChunk>(chunk) {
            Ok(data) => {
                let finish_reason = data
                    .candidates
                    .as_ref()
                    .and_then(|c| c.first())
                    .and_then(|c| c.finish_reason.clone());

                if let Some(ref candidates) = data.candidates
                    && let Some(candidate) = candidates.first()
                    && let Some(ref content) = candidate.content
                {
                    for part in &content.parts {
                        if let Some(text) = &part.text {
                            return ParsedChunk {
                                event: StreamEvent::Token(text.clone()),
                                request_id: None,
                                model: Some(self.model.clone()),
                                finish_reason: finish_reason.clone(),
                            };
                        }

                        if let Some(function_call) = &part.function_call {
                            let call = ToolCall {
                                id: format!(
                                    "gemini_{}",
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis()
                                ),
                                call_type: "function".to_string(),
                                function: FunctionCall {
                                    name: function_call.name.clone(),
                                    arguments: function_call.args.clone(),
                                },
                            };
                            return ParsedChunk {
                                event: StreamEvent::ToolCall(vec![call]),
                                request_id: None,
                                model: Some(self.model.clone()),
                                finish_reason: finish_reason.clone(),
                            };
                        }
                    }
                }

                if let Some(ref candidates) = data.candidates
                    && let Some(candidate) = candidates.first()
                    && candidate.finish_reason.is_some()
                    && candidate.finish_reason != Some("STOP".to_string())
                {
                    return ParsedChunk {
                        event: StreamEvent::Done,
                        request_id: None,
                        model: Some(self.model.clone()),
                        finish_reason,
                    };
                }

                ParsedChunk {
                    event: StreamEvent::Done,
                    request_id: None,
                    model: Some(self.model.clone()),
                    finish_reason,
                }
            }
            Err(_) => ParsedChunk {
                event: StreamEvent::Error(format!("Failed to parse chunk: {}", chunk)),
                request_id: None,
                model: None,
                finish_reason: None,
            },
        }
    }
}

#[async_trait::async_trait]
impl Provider for GeminiProvider {
    async fn stream_chat<'a>(
        &'a self, request: ChatRequest, cancel_token: CancelToken,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send + 'a>>> {
        let gemini_request = self.to_gemini_request(&request)?;
        let url = format!(
            "{}/models/{}:streamGenerateContent?key={}",
            self.base_url, self.model, self.api_key
        );
        let cancel_token_clone = cancel_token.clone();

        let stream = async_stream::stream! {
            if cancel_token.is_cancelled() {
                yield StreamEvent::Error("Cancelled before request".to_string());
                return;
            }

            let response = match self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&gemini_request)
                .send()
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    yield StreamEvent::Error(format!("Gemini request failed: {}", e));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                yield StreamEvent::Error(format!("Gemini API error: {} - {}", status, body));
                return;
            }

            let bytes_stream = response.bytes_stream();

            tokio::pin!(bytes_stream);

            let mut buffer = Vec::new();

            while let Some(item_result) = bytes_stream.next().await {
                if cancel_token_clone.is_cancelled() {
                    yield StreamEvent::Error("Cancelled by user".to_string());
                    return;
                }

                match item_result {
                    Ok(chunk) => {
                        buffer.extend_from_slice(&chunk);

                        while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                            let line_bytes = buffer.drain(..=pos).collect::<Vec<_>>();
                            buffer.drain(..1);

                            let line = String::from_utf8_lossy(&line_bytes).to_string();
                            if !line.trim().is_empty() {
                                let parsed = self.parse_chunk(&line);
                                let is_done = matches!(parsed.event, StreamEvent::Done);

                                if is_done
                                    && let Some(ref reason) = parsed.finish_reason {
                                        tracing::debug!(
                                            model = ?parsed.model,
                                            finish_reason = %reason,
                                            "Gemini stream completed"
                                        );
                                    }

                                yield parsed.event;

                                if is_done {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield StreamEvent::Error(format!("Stream error: {}", e));
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// Gemini API request format
#[derive(Debug, Serialize)]
struct GeminiChatRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Serialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: ToolParameter,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_config: Option<GeminiThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiThinkingConfig {
    thinking_level: String,
}

#[derive(Debug, Serialize)]
struct GeminiToolConfig {
    function_calling_config: Option<GeminiFunctionCallingConfig>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionCallingConfig {
    mode: String,
    stream_function_call_arguments: bool,
}

#[derive(Debug, Deserialize)]
struct GeminiChunk {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

/// Gemini function call (args is JSON object, not string)
#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

/// Gemini function response
#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

/// Factory to create providers from config
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create_from_config(config: &thunderus_core::ProviderConfig) -> Result<Arc<dyn Provider>> {
        match config {
            thunderus_core::ProviderConfig::Glm { api_key, model, base_url, thinking, options: _ } => {
                Ok(Arc::new(GlmProvider::new(
                    api_key.clone(),
                    model.clone(),
                    Some(base_url.clone()),
                    thinking.enabled,
                    thinking.preserved,
                )))
            }
            thunderus_core::ProviderConfig::Gemini { api_key, model, base_url, thinking, options: _ } => {
                Ok(Arc::new(GeminiProvider::new(
                    api_key.clone(),
                    model.clone(),
                    Some(base_url.clone()),
                    thinking.level.clone(),
                )))
            }
            thunderus_core::ProviderConfig::Mock { responses_file } => {
                Ok(Arc::new(super::mock::MockProvider::new(responses_file.clone())))
            }
        }
    }

    pub fn create_mock_provider(responses_file: Option<String>) -> Result<Arc<dyn Provider>> {
        Ok(Arc::new(super::mock::MockProvider::new(responses_file)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_provider_creation() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.model, "glm-4.7");
        assert_eq!(provider.base_url, "https://api.z.ai/api/paas/v4");
    }

    #[test]
    fn test_glm_provider_custom_url() {
        let provider = GlmProvider::new(
            "test-key".to_string(),
            "glm-4.7".to_string(),
            Some("https://custom.api.com".to_string()),
            false,
            false,
        );
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_gemini_provider_creation() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.model, "gemini-2.5-flash");
        assert_eq!(provider.base_url, "https://generativelanguage.googleapis.com/v1beta");
    }

    #[test]
    fn test_gemini_provider_custom_url() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            Some("https://custom.api.com".to_string()),
            GeminiThinkingLevel::Minimal,
        );
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_glm_request_conversion() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        let request = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let glm_req = provider.to_glm_request(&request).unwrap();
        assert_eq!(glm_req.model, "glm-4.7");
        assert_eq!(glm_req.messages.len(), 1);
        assert_eq!(glm_req.messages[0].role, "user");
    }

    #[test]
    fn test_glm_request_with_tools() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        let tool = ToolSpec::new("test_tool", "A test tool", ToolParameter::new_string("param"));
        let request = ChatRequest::builder()
            .add_message(ChatMessage::user("Hello"))
            .tools(vec![tool])
            .build();

        let glm_req = provider.to_glm_request(&request).unwrap();
        assert!(glm_req.tools.is_some());
        assert_eq!(glm_req.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_gemini_request_conversion() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        let request = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let gem_req = provider.to_gemini_request(&request).unwrap();
        assert_eq!(gem_req.contents.len(), 1);
        assert_eq!(gem_req.contents[0].role, "user");
    }

    #[test]
    fn test_gemini_request_with_system() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        let request = ChatRequest::builder()
            .add_message(ChatMessage::system("You are helpful"))
            .add_message(ChatMessage::user("Hello"))
            .build();

        let gem_req = provider.to_gemini_request(&request).unwrap();
        assert!(gem_req.system_instruction.is_some());
        assert_eq!(
            gem_req.system_instruction.as_ref().unwrap().parts[0].text,
            Some("You are helpful".to_string())
        );
    }

    #[test]
    fn test_gemini_request_with_tools() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        let tool = ToolSpec::new("test_tool", "A test tool", ToolParameter::new_string("param"));
        let request = ChatRequest::builder()
            .add_message(ChatMessage::user("Hello"))
            .tools(vec![tool])
            .build();

        let gem_req = provider.to_gemini_request(&request).unwrap();
        assert!(gem_req.tools.is_some());
        assert_eq!(gem_req.tools.unwrap().len(), 1);
    }

    #[test]
    fn test_glm_parse_chunk_text() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        let chunk =
            r#"{"id":"req-123","model":"glm-4.7","choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let parsed = provider.parse_chunk(chunk);
        assert!(matches!(parsed.event, StreamEvent::Token(_)));
        assert_eq!(parsed.request_id, Some("req-123".to_string()));
        assert_eq!(parsed.model, Some("glm-4.7".to_string()));
    }

    #[test]
    fn test_glm_parse_chunk_done() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        let parsed = provider.parse_chunk("[DONE]");
        assert!(matches!(parsed.event, StreamEvent::Done));
    }

    #[test]
    fn test_glm_parse_chunk_with_finish_reason() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None, false, false);
        let chunk = r#"{"id":"req-456","model":"glm-4.7","choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let parsed = provider.parse_chunk(chunk);
        assert_eq!(parsed.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_gemini_parse_chunk_text() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]},"finishReason":"STOP"}]}"#;
        let parsed = provider.parse_chunk(chunk);
        assert!(matches!(parsed.event, StreamEvent::Token(_)));
        assert_eq!(parsed.model, Some("gemini-2.5-flash".to_string()));
    }

    #[test]
    fn test_gemini_parse_chunk_with_finish_reason() {
        let provider = GeminiProvider::new(
            "test-key".to_string(),
            "gemini-2.5-flash".to_string(),
            None,
            GeminiThinkingLevel::Minimal,
        );
        let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Done"}]},"finishReason":"STOP"}]}"#;
        let parsed = provider.parse_chunk(chunk);
        assert_eq!(parsed.finish_reason, Some("STOP".to_string()));
    }

    #[test]
    fn test_cancel_token() {
        let cancel = CancelToken::new();
        assert!(!cancel.is_cancelled());
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }
}
