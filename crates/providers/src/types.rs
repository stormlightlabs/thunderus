use serde::{Deserialize, Serialize};

/// The role of a message sender
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A single chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None, tool_calls: None }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_call_id: Some(tool_call_id.into()), tool_calls: None }
    }

    pub fn with_tool_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None, tool_calls: Some(tool_calls) }
    }
}

/// A function call initiated by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A tool call made by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            call_type: "function".to_string(),
            function: FunctionCall { name: name.into(), arguments },
        }
    }

    pub fn name(&self) -> &str {
        &self.function.name
    }

    pub fn arguments(&self) -> &serde_json::Value {
        &self.function.arguments
    }
}

/// Tool parameter type specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "properties")]
pub enum ToolParameter {
    #[serde(rename = "string")]
    String { description: Option<String> },
    #[serde(rename = "number")]
    Number { description: Option<String> },
    #[serde(rename = "boolean")]
    Boolean { description: Option<String> },
    #[serde(rename = "array")]
    Array {
        items: Box<ToolParameter>,
        description: Option<String>,
    },
    #[serde(rename = "object")]
    Object {
        properties: Vec<(String, ToolParameter)>,
        description: Option<String>,
        #[serde(rename = "required")]
        required: Option<Vec<String>>,
    },
}

impl ToolParameter {
    pub fn new_string(description: impl Into<String>) -> Self {
        Self::String { description: Some(description.into()) }
    }

    pub fn new_number(description: impl Into<String>) -> Self {
        Self::Number { description: Some(description.into()) }
    }

    pub fn new_boolean(description: impl Into<String>) -> Self {
        Self::Boolean { description: Some(description.into()) }
    }

    pub fn new_array(items: ToolParameter) -> Self {
        Self::Array { items: Box::new(items), description: None }
    }

    pub fn new_object(properties: Vec<(String, ToolParameter)>) -> Self {
        Self::Object { properties, description: None, required: None }
    }

    pub fn with_description(self, description: impl Into<String>) -> Self {
        match self {
            Self::String { .. } => Self::String { description: Some(description.into()) },
            Self::Number { .. } => Self::Number { description: Some(description.into()) },
            Self::Boolean { .. } => Self::Boolean { description: Some(description.into()) },
            Self::Array { items, .. } => Self::Array { items, description: Some(description.into()) },
            Self::Object { properties, required, .. } => {
                Self::Object { properties, description: Some(description.into()), required }
            }
        }
    }
}

/// Specification of a tool available to the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub spec_type: String,
    pub function: FunctionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    pub description: Option<String>,
    pub parameters: ToolParameter,
}

impl ToolSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: ToolParameter) -> Self {
        Self {
            spec_type: "function".to_string(),
            function: FunctionSpec { name: name.into(), description: Some(description.into()), parameters },
        }
    }

    pub fn name(&self) -> &str {
        &self.function.name
    }

    pub fn description(&self) -> Option<&str> {
        self.function.description.as_deref()
    }

    pub fn parameters(&self) -> &ToolParameter {
        &self.function.parameters
    }
}

/// Result from executing a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { tool_call_id: tool_call_id.into(), content: content.into(), error: None }
    }

    pub fn error(tool_call_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self { tool_call_id: tool_call_id.into(), content: String::new(), error: Some(error.into()) }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Usage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self { prompt_tokens, completion_tokens, total_tokens: prompt_tokens + completion_tokens }
    }
}

/// A request to a chat provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

impl ChatRequest {
    pub fn builder() -> ChatRequestBuilder {
        ChatRequestBuilder::default()
    }
}

#[derive(Default)]
pub struct ChatRequestBuilder {
    messages: Vec<ChatMessage>,
    tools: Option<Vec<ToolSpec>>,
    tool_choice: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    top_p: Option<f32>,
}

impl ChatRequestBuilder {
    pub fn messages(mut self, messages: Vec<ChatMessage>) -> Self {
        self.messages = messages;
        self
    }

    pub fn add_message(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    pub fn tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn tool_choice(mut self, choice: impl Into<String>) -> Self {
        self.tool_choice = Some(choice.into());
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    pub fn build(self) -> ChatRequest {
        ChatRequest {
            messages: self.messages,
            tools: self.tools,
            tool_choice: self.tool_choice,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            top_p: self.top_p,
        }
    }
}

/// A response from a chat provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

impl ChatResponse {
    pub fn new(message: ChatMessage) -> Self {
        Self { message, tool_calls: None, usage: None, finish_reason: None }
    }

    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(tool_calls);
        self.finish_reason = Some("tool_calls".to_string());
        self
    }

    pub fn with_usage(mut self, usage: Usage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_finish_reason(mut self, reason: impl Into<String>) -> Self {
        self.finish_reason = Some(reason.into());
        self
    }

    pub fn has_tool_calls(&self) -> bool {
        self.tool_calls.as_ref().map(|calls| !calls.is_empty()).unwrap_or(false)
    }
}

/// Events from streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum StreamEvent {
    /// A single token or chunk of content
    Token(String),
    /// Tool calls initiated by the model
    ToolCall(Vec<ToolCall>),
    /// End of stream
    Done,
    /// An error occurred during streaming
    Error(String),
}

/// Token for cancelling streaming operations
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self { cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_spec_required_fields() {
        let param = ToolParameter::new_object(vec![
            ("required_field".to_string(), ToolParameter::new_string("Required")),
            ("optional_field".to_string(), ToolParameter::new_string("Optional")),
        ]);

        if let ToolParameter::Object { properties, required, .. } = param {
            assert_eq!(properties.len(), 2);
            assert!(required.is_none());
        } else {
            panic!("Expected Object parameter");
        }
    }

    #[test]
    fn test_usage_calculation() {
        let usage = Usage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_chat_request_builder() {
        let request = ChatRequest::builder()
            .add_message(ChatMessage::user("Hello"))
            .temperature(0.7)
            .max_tokens(100)
            .build();

        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
    }

    #[test]
    fn test_chat_message_with_tool_calls() {
        let tool_calls = vec![ToolCall::new("call_1", "test", serde_json::json!({}))];

        let msg = ChatMessage::with_tool_calls("Calling tool", tool_calls);
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.role, Role::Assistant);
    }

    #[test]
    fn test_tool_call_accessors() {
        let call = ToolCall::new("id", "tool_name", serde_json::json!({"arg": "value"}));
        assert_eq!(call.name(), "tool_name");
        assert_eq!(call.arguments(), &serde_json::json!({"arg": "value"}));
    }

    #[test]
    fn test_tool_spec_accessors() {
        let spec = ToolSpec::new("my_tool", "A tool", ToolParameter::new_string("param"));
        assert_eq!(spec.name(), "my_tool");
        assert_eq!(spec.description(), Some("A tool"));
    }

    #[test]
    fn test_tool_result_status() {
        let success = ToolResult::success("id", "output");
        assert!(success.is_success());
        assert!(!success.is_error());

        let error = ToolResult::error("id", "failed");
        assert!(!error.is_success());
        assert!(error.is_error());
    }
}
