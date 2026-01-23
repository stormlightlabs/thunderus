use serde_json::Value;
use thunderus_core::Result;
use thunderus_providers::ToolResult;

use crate::Tool;

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

    fn parameters(&self) -> thunderus_providers::ToolParameter {
        thunderus_providers::ToolParameter::new_object(vec![(
            "message".to_string(),
            thunderus_providers::ToolParameter::new_string("The message to echo back")
                .with_description("Any string value"),
        )])
    }

    fn risk_level(&self) -> thunderus_core::ToolRisk {
        thunderus_core::ToolRisk::Safe
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            thunderus_core::ToolRisk::Safe,
            "Echo tool only reflects input and produces no side effects",
        ))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let message = arguments.get("message").and_then(|v| v.as_str()).unwrap_or("");
        Ok(ToolResult::success(tool_call_id, message.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
