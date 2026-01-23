use serde_json::Value;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// A tool that performs safe find-replace edits in files
///
/// This tool provides atomic, exact string replacement in files with safety
/// validations including uniqueness checks and read-before-edit enforcement.
#[derive(Debug)]
pub struct EditTool;

impl EditTool {
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

    /// Validates that old_string is unique in the file content
    ///
    /// Returns an error if old_string appears multiple times, unless replace_all is true.
    fn validate_uniqueness(content: &str, old_string: &str, replace_all: bool) -> Result<()> {
        if !replace_all {
            let count = content.matches(old_string).count();
            if count == 0 {
                return Err(thunderus_core::Error::Validation(format!(
                    "old_string not found in file: '{}'",
                    old_string
                )));
            }
            if count > 1 {
                return Err(thunderus_core::Error::Validation(format!(
                    "old_string appears {} times in file. Use replace_all=true to replace all occurrences, or provide more context to make the string unique.\n\nold_string: '{}'",
                    count, old_string
                )));
            }
        }
        Ok(())
    }

    /// Reads the file and performs the replacement
    fn perform_edit(path: &Path, old_string: &str, new_string: &str, replace_all: bool) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to read file '{}': {}", path.display(), e)))?;

        Self::validate_uniqueness(&content, old_string, replace_all)?;

        let new_content = if replace_all {
            content.replacen(old_string, new_string, usize::MAX)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        std::fs::write(path, new_content)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to write file '{}': {}", path.display(), e)))?;

        Ok(format!(
            "Successfully edited file: {}\nReplaced '{}' with '{}'",
            path.display(),
            old_string,
            new_string
        ))
    }
}

impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Perform safe find-replace edits in files. Requires prior Read tool use for safety validation."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "file_path".to_string(),
                ToolParameter::new_string("Absolute path to the file")
                    .with_description("The absolute path to the file to edit"),
            ),
            (
                "old_string".to_string(),
                ToolParameter::new_string("The exact string to replace")
                    .with_description("The exact string to find and replace. Must be unique unless replace_all is true"),
            ),
            (
                "new_string".to_string(),
                ToolParameter::new_string("The replacement string")
                    .with_description("The string to replace old_string with"),
            ),
            (
                "replace_all".to_string(),
                ToolParameter::new_boolean("Replace all occurrences")
                    .with_description("If true, replace all occurrences of old_string. If false (default), old_string must be unique in the file"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Risky
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Risky,
            "Edit modifies file contents and can break code if used incorrectly. All edits require approval and should be reviewed carefully.",
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

        let old_string = arguments.get("old_string").and_then(|v| v.as_str()).ok_or_else(|| {
            thunderus_core::Error::Validation("Missing or invalid 'old_string' parameter".to_string())
        })?;

        let new_string = arguments.get("new_string").and_then(|v| v.as_str()).ok_or_else(|| {
            thunderus_core::Error::Validation("Missing or invalid 'new_string' parameter".to_string())
        })?;

        let replace_all = arguments.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);
        let path = PathBuf::from(file_path_str);

        Self::validate_path(&path)?;

        let result = Self::perform_edit(&path, old_string, new_string, replace_all)?;
        Ok(ToolResult::success(tool_call_id, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_edit_tool_properties() {
        let tool = EditTool;
        assert_eq!(tool.name(), "edit");
        assert_eq!(tool.risk_level(), ToolRisk::Risky);
        assert!(tool.risk_level().is_risky());
    }

    #[test]
    fn test_edit_execute_missing_file_path() {
        let tool = EditTool;
        let args = serde_json::json!({"old_string": "foo", "new_string": "bar"});
        let result = tool.execute("call_edit_1".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("file_path"));
    }

    #[test]
    fn test_edit_execute_empty_file_path() {
        let tool = EditTool;
        let args = serde_json::json!({"file_path": "", "old_string": "foo", "new_string": "bar"});
        let result = tool.execute("call_edit_2".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_edit_execute_missing_old_string() {
        let tool = EditTool;
        let args = serde_json::json!({"file_path": "/tmp/test.txt", "new_string": "bar"});
        let result = tool.execute("call_edit_3".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("old_string"));
    }

    #[test]
    fn test_edit_execute_missing_new_string() {
        let tool = EditTool;
        let args = serde_json::json!({"file_path": "/tmp/test.txt", "old_string": "foo"});
        let result = tool.execute("call_edit_4".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("new_string"));
    }

    #[test]
    fn test_edit_execute_nonexistent_file() {
        let tool = EditTool;
        let args = serde_json::json!({"file_path": "/tmp/nonexistent_file_12345.txt", "old_string": "foo", "new_string": "bar"});
        let result = tool.execute("call_edit_5".to_string(), &args);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_edit_execute_old_string_not_found() {
        let tool = EditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_not_found.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "old_string": "goodbye",
            "new_string": "hello"
        });
        let result = tool.execute("call_edit_6".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found in file"));
    }

    #[test]
    fn test_edit_execute_old_string_not_unique() {
        let tool = EditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_not_unique.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "foo bar foo").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "old_string": "foo",
            "new_string": "baz"
        });
        let result = tool.execute("call_edit_7".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("appears 2 times"));
        assert!(err.to_string().contains("replace_all=true"));
    }

    #[test]
    fn test_edit_execute_success() {
        let tool = EditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_success.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "old_string": "world",
            "new_string": "Rust"
        });
        let result = tool.execute("call_edit_8".to_string(), &args);

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("Successfully edited"));
        assert_eq!(content, "Hello Rust\n");
    }

    #[test]
    fn test_edit_execute_replace_all() {
        let tool = EditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_replace_all.txt");
        writeln!(std::fs::File::create(&temp_file).unwrap(), "foo bar foo baz foo").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "old_string": "foo",
            "new_string": "qux",
            "replace_all": true
        });
        let result = tool.execute("call_edit_9".to_string(), &args);

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert_eq!(content, "qux bar qux baz qux\n");
    }

    #[test]
    fn test_edit_execute_with_multiline_strings() {
        let tool = EditTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_edit_multiline.txt");
        let mut file = std::fs::File::create(&temp_file).unwrap();
        writeln!(file, "Line 1\nOldLine\nLine 3").unwrap();

        let args = serde_json::json!({
            "file_path": temp_file.to_string_lossy().as_ref(),
            "old_string": "OldLine",
            "new_string": "NewLine"
        });
        let result = tool.execute("call_edit_10".to_string(), &args);

        let content = std::fs::read_to_string(&temp_file).unwrap();
        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(content.contains("NewLine"));
        assert!(!content.contains("OldLine"));
    }
}
