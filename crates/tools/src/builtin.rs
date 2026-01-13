use serde_json::Value;
use std::process::{Command, Stdio};
use thunderus_core::{Classification, Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use super::Tool;

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

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Safe,
            "No-op tool has no side effects and does not modify any state",
        ))
    }

    fn execute(&self, tool_call_id: String, _arguments: &Value) -> Result<ToolResult> {
        Ok(ToolResult::success(tool_call_id, "noop executed successfully"))
    }
}

/// A tool that executes shell commands with approval gating
/// Provides shell command execution for the composer's !cmd functionality
#[derive(Debug)]
pub struct ShellTool;

impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute shell commands locally. Use with caution - all commands are subject to approval and sandbox policies."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![(
            "command".to_string(),
            ToolParameter::new_string("The shell command to execute").with_description("Any valid shell command"),
        )])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Risky
    }

    fn classification(&self) -> Option<Classification> {
        Some(Classification::new(
            ToolRisk::Risky,
            "Shell commands can modify the system, access files, and make network requests. All shell commands require approval.",
        ))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let command = arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| thunderus_core::Error::Tool("Missing or invalid 'command' parameter".to_string()))?;

        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout).to_string();
                let stderr = String::from_utf8_lossy(&result.stderr).to_string();
                let exit_code = result.status.code().unwrap_or(-1);

                let content = if !stderr.is_empty() && exit_code != 0 {
                    format!(
                        "Command failed with exit code {}\n\nSTDERR:\n{}\n\nSTDOUT:\n{}",
                        exit_code, stderr, stdout
                    )
                } else if !stderr.is_empty() {
                    format!(
                        "Command completed with warnings\n\nSTDERR:\n{}\n\nSTDOUT:\n{}",
                        stderr, stdout
                    )
                } else {
                    stdout
                };

                Ok(ToolResult::success(tool_call_id, content))
            }
            Err(e) => Ok(ToolResult::error(
                tool_call_id,
                format!("Failed to execute command '{}': {}", command, e),
            )),
        }
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

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Safe,
            "Echo tool only reflects input and produces no side effects",
        ))
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

/// Helper function to create a shell tool call for testing
#[cfg(test)]
pub fn shell_tool_call(id: &str, command: &str) -> thunderus_providers::ToolCall {
    thunderus_providers::ToolCall::new(id, "shell", serde_json::json!({"command": command}))
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

    #[test]
    fn test_shell_tool_properties() {
        let tool = ShellTool;
        assert_eq!(tool.name(), "shell");
        assert_eq!(
            tool.description(),
            "Execute shell commands locally. Use with caution - all commands are subject to approval and sandbox policies."
        );
    }

    #[test]
    fn test_shell_risk_level() {
        let tool = ShellTool;
        assert!(tool.risk_level().is_risky());
    }

    #[test]
    fn test_shell_classification() {
        let tool = ShellTool;
        let classification = tool.classification().unwrap();
        assert!(classification.risk.is_risky());
        assert!(classification.reasoning.contains("Shell commands"));
    }

    #[test]
    fn test_shell_execute_simple_command() {
        let tool = ShellTool;
        let args = serde_json::json!({"command": "echo 'Hello, shell!'"});
        let result = tool.execute("call_shell_123".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_shell_123");
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "Hello, shell!\n");
    }

    #[test]
    fn test_shell_execute_without_command() {
        let tool = ShellTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_shell_456".to_string(), &args);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing or invalid 'command' parameter")
        );
    }

    #[test]
    fn test_shell_execute_with_null_command() {
        let tool = ShellTool;
        let args = serde_json::json!({"command": null});
        let result = tool.execute("call_shell_789".to_string(), &args);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing or invalid 'command' parameter")
        );
    }

    #[test]
    fn test_shell_execute_failing_command() {
        let tool = ShellTool;
        let args = serde_json::json!({"command": "exit 42"});
        let result = tool.execute("call_shell_fail".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_shell_fail");
        assert!(tool_result.is_success());
        assert!(tool_result.content.trim().is_empty() || tool_result.content.contains("exit code"));
    }

    #[test]
    fn test_shell_spec() {
        let tool = ShellTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "shell");
        assert_eq!(
            spec.description(),
            Some(
                "Execute shell commands locally. Use with caution - all commands are subject to approval and sandbox policies."
            )
        );
    }

    #[test]
    fn test_shell_tool_call_helper() {
        let tool_call = shell_tool_call("test_id", "ls -la");
        assert_eq!(tool_call.id, "test_id");
        assert_eq!(tool_call.name(), "shell");
        assert_eq!(*tool_call.arguments(), serde_json::json!({"command": "ls -la"}));
    }
}
