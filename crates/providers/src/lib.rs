pub mod adapter;
pub mod prompts;
pub mod schemas;
pub mod types;

pub use adapter::{GeminiProvider, GlmProvider, Provider, ProviderFactory};
pub use prompts::{
    ProviderType, base_system_prompt, build_system_prompt_for_provider, provider_prompt_adaptation,
    result_formatting_guidance, system_prompt, teaching_error_messages, tool_usage_guidance,
};
pub use schemas::{
    GeminiFunctionDeclaration, GeminiToolSchema, GlmFunction, GlmToolSchema, gemini_tool_schemas, glm_tool_schemas,
};
pub use types::{
    CancelToken, ChatMessage, ChatRequest, ChatResponse, FunctionCall, Role, StreamEvent, ToolCall, ToolParameter,
    ToolResult, ToolSpec,
};

pub use thunderus_core::{Error, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest::builder()
            .messages(vec![ChatMessage::system("System message")])
            .tools(vec![ToolSpec::new(
                "test_tool",
                "A test tool",
                ToolParameter::new_object(vec![]),
            )])
            .build();

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("system"));
        assert!(json.contains("test_tool"));
    }

    #[test]
    fn test_tool_spec_validation() {
        let valid_tool = ToolSpec::new(
            "valid_tool",
            "A valid tool",
            ToolParameter::new_object(vec![(
                "param1".to_string(),
                ToolParameter::new_string("Description".to_string()),
            )]),
        );

        assert_eq!(valid_tool.name(), "valid_tool");
        assert!(valid_tool.description().is_some());
    }

    #[test]
    fn test_tool_call_creation() {
        let tool_call = ToolCall::new("tool_123", "test_tool", serde_json::json!({"param": "value"}));

        assert_eq!(tool_call.id, "tool_123");
        assert_eq!(tool_call.name(), "test_tool");
    }

    #[test]
    fn test_stream_event_variants() {
        let token_event = StreamEvent::Token("Hello".to_string());
        let done_event = StreamEvent::Done;
        let error_event = StreamEvent::Error("Connection failed".to_string());

        assert!(matches!(token_event, StreamEvent::Token(_)));
        assert!(matches!(done_event, StreamEvent::Done));
        assert!(matches!(error_event, StreamEvent::Error(_)));
    }

    #[test]
    fn test_cancel_token() {
        let cancel = CancelToken::new();
        assert!(!cancel.is_cancelled());

        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn test_chat_message_variants() {
        let system_msg = ChatMessage::system("You are helpful");
        let user_msg = ChatMessage::user("Hello");
        let assistant_msg = ChatMessage::assistant("Hi there");

        assert!(matches!(system_msg.role, Role::System));
        assert!(matches!(user_msg.role, Role::User));
        assert!(matches!(assistant_msg.role, Role::Assistant));

        let tool_msg = ChatMessage::tool("tool_result", "Tool output");
        assert!(matches!(tool_msg.role, Role::Tool));
        assert_eq!(tool_msg.tool_call_id, Some("tool_result".to_string()));
    }

    #[test]
    fn test_function_call_serialization() {
        let func_call = FunctionCall { name: "calculate".to_string(), arguments: serde_json::json!({"x": 1, "y": 2}) };

        let json = serde_json::to_string(&func_call).unwrap();
        assert!(json.contains("calculate"));
        assert!(json.contains("\"x\""));
    }

    #[test]
    fn test_tool_result_creation() {
        let result = ToolResult::success("tool_123", "Success output");
        assert_eq!(result.tool_call_id, "tool_123");
        assert!(result.is_success());
        assert!(!result.is_error());

        let error_result = ToolResult::error("tool_123", "Failed");
        assert!(error_result.is_error());
    }

    #[test]
    fn test_chat_response_with_tool_calls() {
        let response = ChatResponse {
            message: ChatMessage::assistant("Let me help"),
            tool_calls: Some(vec![ToolCall::new(
                "id_1",
                "search",
                serde_json::json!({"query": "test"}),
            )]),
            usage: None,
            finish_reason: Some("tool_calls".to_string()),
        };

        assert!(response.tool_calls.is_some());
        assert_eq!(response.tool_calls.unwrap().len(), 1);
    }
}
