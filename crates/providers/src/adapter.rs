use eventsource_stream::Eventsource;
use futures::{StreamExt, stream::Stream};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;

use crate::types::*;
use thunderus_core::Result;

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
}

impl GlmProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Self {
        Self {
            client: HttpClient::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.z.ai/api/paas/v4".to_string()),
        }
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

        Ok(GlmChatRequest {
            model: self.model.clone(),
            messages,
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            stream: true,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            thinking: Some(GlmThinking { type_: "enabled".to_string() }),
        })
    }

    /// Parse SSE chunk into StreamEvent
    fn parse_chunk(&self, chunk: &str) -> StreamEvent {
        if chunk.trim().is_empty() || chunk.starts_with("[DONE]") {
            return StreamEvent::Done;
        }

        match serde_json::from_str::<GlmChunk>(chunk) {
            Ok(data) => {
                if let Some(choices) = data.choices
                    && let Some(choice) = choices.first()
                {
                    let delta = &choice.delta;

                    if let Some(content) = &delta.content {
                        return StreamEvent::Token(content.clone());
                    }

                    if let Some(tool_calls) = &delta.tool_calls
                        && !tool_calls.is_empty()
                    {
                        let calls: Vec<ToolCall> = tool_calls
                            .iter()
                            .filter_map(|tc| {
                                tc.function.as_ref().map(|func| ToolCall {
                                    id: tc.id.clone().unwrap_or_default(),
                                    call_type: "function".to_string(),
                                    function: FunctionCall {
                                        name: func.name.clone().unwrap_or_default(),
                                        arguments: serde_json::from_str(&func.arguments.clone().unwrap_or_default())
                                            .unwrap_or(serde_json::Value::Null),
                                    },
                                })
                            })
                            .collect();

                        if !calls.is_empty() {
                            return StreamEvent::ToolCall(calls);
                        }
                    }

                    if let Some(ref reasoning) = delta.reasoning_content {
                        return StreamEvent::Token(format!("<thinking>{}</thinking>", reasoning));
                    }
                }
                StreamEvent::Done
            }
            Err(_) => StreamEvent::Error(format!("Failed to parse chunk: {}", chunk)),
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
                        let is_done = matches!(parsed, StreamEvent::Done);
                        yield parsed;

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
}

/// GLM SSE chunk format
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GlmChunk {
    id: Option<String>,
    object: Option<String>,
    created: Option<u64>,
    model: Option<String>,
    choices: Option<Vec<GlmChoice>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GlmChoice {
    index: Option<u32>,
    delta: GlmDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Self {
        Self {
            client: HttpClient::new(),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string()),
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
                        parameters: spec.function.parameters.clone(),
                    })
                    .collect(),
            }]
        });

        Ok(GeminiChatRequest {
            contents,
            system_instruction,
            tools,
            generation_config: Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        })
    }

    /// Parse Gemini API chunk into StreamEvent
    fn parse_chunk(&self, chunk: &str) -> StreamEvent {
        if chunk.trim().is_empty() {
            return StreamEvent::Done;
        }

        match serde_json::from_str::<GeminiChunk>(chunk) {
            Ok(data) => {
                if let Some(ref candidates) = data.candidates
                    && let Some(candidate) = candidates.first()
                    && let Some(ref content) = candidate.content
                {
                    for part in &content.parts {
                        if let Some(text) = &part.text {
                            return StreamEvent::Token(text.clone());
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
                            return StreamEvent::ToolCall(vec![call]);
                        }
                    }
                }

                if let Some(ref candidates) = data.candidates
                    && let Some(candidate) = candidates.first()
                    && candidate.finish_reason.is_some()
                    && candidate.finish_reason != Some("STOP".to_string())
                {
                    return StreamEvent::Done;
                }

                StreamEvent::Done
            }
            Err(_) => StreamEvent::Error(format!("Failed to parse chunk: {}", chunk)),
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
                                let is_done = matches!(parsed, StreamEvent::Done);
                                yield parsed;

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
}

#[derive(Debug, Deserialize)]
struct GeminiChunk {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
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
            thunderus_core::ProviderConfig::Glm { api_key, model, base_url } => Ok(Arc::new(GlmProvider::new(
                api_key.clone(),
                model.clone(),
                Some(base_url.clone()),
            ))),
            thunderus_core::ProviderConfig::Gemini { api_key, model, base_url } => Ok(Arc::new(GeminiProvider::new(
                api_key.clone(),
                model.clone(),
                Some(base_url.clone()),
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm_provider_creation() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None);
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
        );
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_gemini_provider_creation() {
        let provider = GeminiProvider::new("test-key".to_string(), "gemini-2.5-flash".to_string(), None);
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
        );
        assert_eq!(provider.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_glm_request_conversion() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None);
        let request = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let glm_req = provider.to_glm_request(&request).unwrap();
        assert_eq!(glm_req.model, "glm-4.7");
        assert_eq!(glm_req.messages.len(), 1);
        assert_eq!(glm_req.messages[0].role, "user");
    }

    #[test]
    fn test_glm_request_with_tools() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None);
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
        let provider = GeminiProvider::new("test-key".to_string(), "gemini-2.5-flash".to_string(), None);
        let request = ChatRequest::builder().add_message(ChatMessage::user("Hello")).build();

        let gem_req = provider.to_gemini_request(&request).unwrap();
        assert_eq!(gem_req.contents.len(), 1);
        assert_eq!(gem_req.contents[0].role, "user");
    }

    #[test]
    fn test_gemini_request_with_system() {
        let provider = GeminiProvider::new("test-key".to_string(), "gemini-2.5-flash".to_string(), None);
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
        let provider = GeminiProvider::new("test-key".to_string(), "gemini-2.5-flash".to_string(), None);
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
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None);
        let chunk = r#"{"choices":[{"delta":{"content":"Hello"}}]}"#;
        let event = provider.parse_chunk(chunk);
        assert!(matches!(event, StreamEvent::Token(_)));
    }

    #[test]
    fn test_glm_parse_chunk_done() {
        let provider = GlmProvider::new("test-key".to_string(), "glm-4.7".to_string(), None);
        let event = provider.parse_chunk("[DONE]");
        assert!(matches!(event, StreamEvent::Done));
    }

    #[test]
    fn test_cancel_token() {
        let cancel = CancelToken::new();
        assert!(!cancel.is_cancelled());
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }
}
