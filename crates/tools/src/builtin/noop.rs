use serde_json::Value;
use thunderus_core::Result;
use thunderus_providers::ToolResult;

use crate::Tool;

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

    fn parameters(&self) -> thunderus_providers::ToolParameter {
        thunderus_providers::ToolParameter::new_object(vec![])
    }

    fn risk_level(&self) -> thunderus_core::ToolRisk {
        thunderus_core::ToolRisk::Safe
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            thunderus_core::ToolRisk::Safe,
            "No-op tool has no side effects and does not modify any state",
        ))
    }

    fn execute(&self, tool_call_id: String, _arguments: &Value) -> Result<ToolResult> {
        Ok(ToolResult::success(tool_call_id, "noop executed successfully"))
    }
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
}
