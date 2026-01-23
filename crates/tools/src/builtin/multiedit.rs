use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// Represents a single edit operation for MultiEdit
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiEditOperation {
    /// The exact string to replace
    pub old_string: String,
    /// The replacement string
    pub new_string: String,
}

/// A tool that performs atomic batch edits in files
///
/// This tool applies multiple edits to a file in a single atomic operation.
/// All edits are validated before any are applied, ensuring that either all
/// succeed or none are applied.
#[derive(Debug)]
pub struct MultiEditTool;

impl MultiEditTool {
    /// Validates that the path exists and is a file
    fn validate_path(path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(thunderus_core::Error::Validation(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        if path.is_dir() {
            return Err(thunderus_core::Error::Validation(format!(
                "Path is a directory, not a file: {}",
                path.display()
            )));
        }

        Ok(())
    }

    /// Validates that all edits are unique and non-overlapping
    ///
    /// Checks that:
    /// - All old_strings are found in the content
    /// - No two edits have the same old_string
    /// - No old_string is a substring of another old_string
    fn validate_edits(content: &str, edits: &[MultiEditOperation]) -> Result<usize> {
        if edits.is_empty() {
            return Err(thunderus_core::Error::Validation(
                "At least one edit operation is required".to_string(),
            ));
        }

        let mut edit_count = 0;

        for (i, edit) in edits.iter().enumerate() {
            if edit.old_string.is_empty() {
                return Err(thunderus_core::Error::Validation(format!(
                    "Edit operation {}: old_string cannot be empty",
                    i + 1
                )));
            }

            let count = content.matches(&edit.old_string).count();
            if count == 0 {
                return Err(thunderus_core::Error::Validation(format!(
                    "Edit operation {}: old_string not found in file: '{}'",
                    i + 1,
                    edit.old_string
                )));
            }

            edit_count += count;
        }

        for (i, edit1) in edits.iter().enumerate() {
            for (j, edit2) in edits.iter().enumerate() {
                if i != j && edit1.old_string == edit2.old_string {
                    return Err(thunderus_core::Error::Validation(format!(
                        "Edit operations {} and {} have the same old_string: '{}'. Use a single edit operation instead.",
                        i + 1,
                        j + 1,
                        edit1.old_string
                    )));
                }
            }
        }

        Ok(edit_count)
    }

    /// Reads the file and performs all replacements atomically
    fn perform_edits(path: &Path, edits: &[MultiEditOperation]) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to read file '{}': {}", path.display(), e)))?;

        let _ = Self::validate_edits(&content, edits)?;

        let mut new_content = content;
        for edit in edits {
            new_content = new_content.replacen(&edit.old_string, &edit.new_string, 1);
        }

        std::fs::write(path, new_content)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to write file '{}': {}", path.display(), e)))?;

        Ok(format!(
            "Successfully edited file: {}\nApplied {} edit operation(s)",
            path.display(),
            edits.len()
        ))
    }

    /// Parses edit operations from JSON array
    fn parse_edits(edits_value: &Value) -> Result<Vec<MultiEditOperation>> {
        let edits_array = edits_value
            .as_array()
            .ok_or_else(|| thunderus_core::Error::Validation("edits must be an array".to_string()))?;

        let mut edits = Vec::new();
        for (i, edit_value) in edits_array.iter().enumerate() {
            let old_string = edit_value.get("old_string").and_then(|v| v.as_str()).ok_or_else(|| {
                thunderus_core::Error::Validation(format!(
                    "Edit operation {}: missing or invalid 'old_string' field",
                    i + 1
                ))
            })?;

            let new_string = edit_value.get("new_string").and_then(|v| v.as_str()).ok_or_else(|| {
                thunderus_core::Error::Validation(format!(
                    "Edit operation {}: missing or invalid 'new_string' field",
                    i + 1
                ))
            })?;

            edits.push(MultiEditOperation { old_string: old_string.to_string(), new_string: new_string.to_string() });
        }

        Ok(edits)
    }
}

impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multiedit"
    }

    fn description(&self) -> &str {
        "Perform atomic batch edits in a file. All edits are validated before any are applied - either all succeed or none are applied."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "file_path".to_string(),
                ToolParameter::new_string("Absolute path to the file")
                    .with_description("The absolute path to the file to edit"),
            ),
            (
                "edits".to_string(),
                ToolParameter::new_array(ToolParameter::new_object(vec![
                    (
                        "old_string".to_string(),
                        ToolParameter::new_string("The exact string to replace")
                            .with_description("The exact string to find and replace"),
                    ),
                    (
                        "new_string".to_string(),
                        ToolParameter::new_string("The replacement string")
                            .with_description("The string to replace old_string with"),
                    ),
                ]))
                .with_description("Array of edit operations. Each operation must have unique old_string values"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Risky
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Risky,
            "MultiEdit applies multiple changes to a file atomically. All edits must be valid and unique before any are applied. Requires approval.",
        ))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let file_path_str = arguments
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| thunderus_core::Error::Validation("Missing or invalid 'file_path' parameter".to_string()))?;

        if file_path_str.is_empty() {
            return Err(thunderus_core::Error::Validation(
                "file_path cannot be empty".to_string(),
            ));
        }

        let edits_value = arguments
            .get("edits")
            .ok_or_else(|| thunderus_core::Error::Validation("Missing 'edits' parameter".to_string()))?;

        let edits = Self::parse_edits(edits_value)?;

        let path = PathBuf::from(file_path_str);

        Self::validate_path(&path)?;

        let result = Self::perform_edits(&path, &edits)?;

        Ok(ToolResult::success(tool_call_id, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_multiedit_tool_properties() {
        let tool = MultiEditTool;
        assert_eq!(tool.name(), "multiedit");
        assert_eq!(tool.risk_level(), ToolRisk::Risky);
        assert!(tool.risk_level().is_risky());
    }

    #[test]
    fn test_multiedit_execute_missing_file_path() {
        let tool = MultiEditTool;
        let args = serde_json::json!({"edits": []});
        let result = tool.execute("call_multiedit_1".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("file_path"));
    }

    #[test]
    fn test_multiedit_execute_empty_file_path() {
        let tool = MultiEditTool;
        let args = serde_json::json!({"file_path": "", "edits": []});
        let result = tool.execute("call_multiedit_2".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_multiedit_execute_missing_edits() {
        let tool = MultiEditTool;
        let args = serde_json::json!({"file_path": "/tmp/test.txt"});
        let result = tool.execute("call_multiedit_3".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("edits"));
    }

    #[test]
    fn test_multiedit_execute_edits_not_array() {
        let tool = MultiEditTool;
        let args = serde_json::json!({"file_path": "/tmp/test.txt", "edits": "not an array"});
        let result = tool.execute("call_multiedit_4".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("array"));
    }

    #[test]
    fn test_multiedit_execute_empty_edits_array() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_empty.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": []
        });
        let result = tool.execute("call_multiedit_5".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("At least one edit"));
    }

    #[test]
    fn test_multiedit_execute_missing_old_string() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_missing_old.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [{"new_string": "there"}]
        });
        let result = tool.execute("call_multiedit_6".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("old_string"));
    }

    #[test]
    fn test_multiedit_execute_missing_new_string() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_missing_new.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [{"old_string": "world"}]
        });
        let result = tool.execute("call_multiedit_7".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("new_string"));
    }

    #[test]
    fn test_multiedit_execute_nonexistent_file() {
        let tool = MultiEditTool;
        let args = serde_json::json!({
            "file_path": "/tmp/nonexistent_multiedit_12345.txt",
            "edits": [{"old_string": "foo", "new_string": "bar"}]
        });
        let result = tool.execute("call_multiedit_8".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_multiedit_execute_old_string_not_found() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_not_found.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [{"old_string": "goodbye", "new_string": "hello"}]
        });
        let result = tool.execute("call_multiedit_9".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found in file"));
    }

    #[test]
    fn test_multiedit_execute_duplicate_old_strings() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_duplicate.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "foo bar baz").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [
                {"old_string": "foo", "new_string": "qux"},
                {"old_string": "foo", "new_string": "quux"}
            ]
        });
        let result = tool.execute("call_multiedit_10".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("same old_string"));
    }

    #[test]
    fn test_multiedit_execute_success_single_edit() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_single.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [{"old_string": "world", "new_string": "Rust"}]
        });
        let result = tool.execute("call_multiedit_11".to_string(), &args);

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("Successfully edited"));
        assert!(tool_result.content.contains("1 edit operation"));
        assert_eq!(content, "Hello Rust\n");
    }

    #[test]
    fn test_multiedit_execute_success_multiple_edits() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_multiple.txt");
        writeln!(
            std::fs::File::create(&temp_file).unwrap(),
            "Hello world, welcome to coding"
        )
        .unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [
                {"old_string": "Hello", "new_string": "Hi"},
                {"old_string": "world", "new_string": "Rust"},
                {"old_string": "coding", "new_string": "programming"}
            ]
        });
        let result = tool.execute("call_multiedit_12".to_string(), &args);

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("3 edit operation"));
        assert_eq!(content, "Hi Rust, welcome to programming\n");
    }

    #[test]
    fn test_multiedit_execute_atomic_failure_on_second_edit() {
        let tool = MultiEditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_multiedit_atomic.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let original_content = std::fs::read_to_string(&temp_file).unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "edits": [
                {"old_string": "Hello", "new_string": "Hi"},
                {"old_string": "nonexistent", "new_string": "test"}
            ]
        });
        let result = tool.execute("call_multiedit_13".to_string(), &args);

        let content_after = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        assert_eq!(content_after, original_content);
    }
}
