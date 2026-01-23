use serde_json::Value;
use std::process::{Command, Stdio};
use thunderus_core::{Classification, Result, ToolRisk};
use thunderus_providers::ToolResult;

use crate::Tool;
use crate::classification::CommandClassifier;

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

    fn parameters(&self) -> thunderus_providers::ToolParameter {
        thunderus_providers::ToolParameter::new_object(vec![(
            "command".to_string(),
            thunderus_providers::ToolParameter::new_string("The shell command to execute")
                .with_description("Any valid shell command"),
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

    fn classify_execution(&self, arguments: &Value) -> Option<Classification> {
        let command = arguments.get("command").and_then(|v| v.as_str())?;
        let classifier = CommandClassifier::new();
        Some(classifier.classify_with_reasoning(command))
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_shell_dynamic_classification_safe() {
        let tool = ShellTool;

        let grep_args = serde_json::json!({"command": "grep pattern file.txt"});
        let grep_classification = tool.classify_execution(&grep_args);
        assert!(grep_classification.is_some());
        assert_eq!(grep_classification.unwrap().risk, ToolRisk::Safe);

        let sed_args = serde_json::json!({"command": "sed 's/old/new/g' file.txt"});
        let sed_classification = tool.classify_execution(&sed_args);
        assert!(sed_classification.is_some());
        assert_eq!(sed_classification.unwrap().risk, ToolRisk::Safe);
    }

    #[test]
    fn test_shell_dynamic_classification_risky() {
        let tool = ShellTool;

        let sed_args = serde_json::json!({"command": "sed -i 's/old/new/g' file.txt"});
        let sed_classification = tool.classify_execution(&sed_args);
        assert!(sed_classification.is_some());
        assert_eq!(sed_classification.unwrap().risk, ToolRisk::Risky);

        let awk_args = serde_json::json!({"command": "awk '{print $1}' file.txt > output.txt"});
        let awk_classification = tool.classify_execution(&awk_args);
        assert!(awk_classification.is_some());
        assert_eq!(awk_classification.unwrap().risk, ToolRisk::Risky);
    }

    #[test]
    fn test_shell_dynamic_classification_with_suggestions() {
        let tool = ShellTool;
        let sed_args = serde_json::json!({"command": "sed -i 's/old/new/g' file.txt"});
        let sed_classification = tool.classify_execution(&sed_args).unwrap();
        assert_eq!(sed_classification.risk, ToolRisk::Risky);
        assert!(sed_classification.suggestion.is_some());
        assert!(sed_classification.suggestion.as_ref().unwrap().contains("Edit tool"));
    }
}
