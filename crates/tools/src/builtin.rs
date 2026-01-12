use serde_json::Value;
use thunderus_core::Result;
use thunderus_providers::{ToolParameter, ToolResult};

use super::{Tool, classification::ToolRisk};

/// A tool that does nothing and returns success
/// Useful for testing and tool call workflows
#[derive(Debug)]
pub struct NoopTool;

impl Tool for NoopTool {
    fn name(&self) -> &str {
        "noop"
    }

    fn description(&self) -> &str {
        "Does nothing and returns success. Used for testing."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }

    fn execute(&self, tool_call_id: String, _arguments: &Value) -> Result<ToolResult> {
        Ok(ToolResult::success(tool_call_id, "noop executed successfully"))
    }
}

/// A tool that echoes back provided input
#[derive(Debug)]
pub struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes back the provided message. Useful for testing."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![(
            "message".to_string(),
            ToolParameter::new_string("The message to echo back").with_description("Any string value"),
        )])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let message = arguments.get("message").and_then(|v| v.as_str()).unwrap_or("");
        Ok(ToolResult::success(tool_call_id, message.to_string()))
    }
}

/// Helper function to create a noop tool call for testing
#[cfg(test)]
pub fn noop_tool_call(id: &str) -> thunderus_providers::ToolCall {
    thunderus_providers::ToolCall::new(id, "noop", serde_json::json!({}))
}

/// Helper function to create an echo tool call for testing
#[cfg(test)]
pub fn echo_tool_call(id: &str, message: &str) -> thunderus_providers::ToolCall {
    thunderus_providers::ToolCall::new(id, "echo", serde_json::json!({"message": message}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_tool_properties() {
        let tool = NoopTool;
        assert_eq!(tool.name(), "noop");
        assert_eq!(
            tool.description(),
            "Does nothing and returns success. Used for testing."
        );
    }

    #[test]
    fn test_noop_risk_level() {
        let tool = NoopTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_noop_execute() {
        let tool = NoopTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_123".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_123");
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "noop executed successfully");
    }

    #[test]
    fn test_noop_spec() {
        let tool = NoopTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "noop");
        assert_eq!(
            spec.description(),
            Some("Does nothing and returns success. Used for testing.")
        );
    }

    #[test]
    fn test_echo_tool_properties() {
        let tool = EchoTool;
        assert_eq!(tool.name(), "echo");
        assert_eq!(
            tool.description(),
            "Echoes back the provided message. Useful for testing."
        );
    }

    #[test]
    fn test_echo_risk_level() {
        let tool = EchoTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_echo_execute_with_message() {
        let tool = EchoTool;
        let args = serde_json::json!({"message": "Hello, world!"});
        let result = tool.execute("call_456".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_456");
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "Hello, world!");
    }

    #[test]
    fn test_echo_execute_without_message() {
        let tool = EchoTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_789".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "");
    }

    #[test]
    fn test_echo_execute_with_null_message() {
        let tool = EchoTool;
        let args = serde_json::json!({"message": null});
        let result = tool.execute("call_abc".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "");
    }

    #[test]
    fn test_echo_spec() {
        let tool = EchoTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "echo");
        assert_eq!(
            spec.description(),
            Some("Echoes back the provided message. Useful for testing.")
        );
    }

    #[test]
    fn test_echo_with_complex_message() {
        let tool = EchoTool;
        let message = "This is a longer message with\nnewlines and\tspecial chars!";
        let args = serde_json::json!({"message": message});
        let result = tool.execute("call_xyz".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.content, message);
    }
}
