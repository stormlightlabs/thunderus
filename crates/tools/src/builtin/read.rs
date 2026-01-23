use serde_json::Value;
use std::io::Read as StdIoRead;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// Maximum line length for Read tool output
const MAX_LINE_LENGTH: usize = 2000;

/// Default number of lines to read
const DEFAULT_LINE_LIMIT: usize = 2000;

/// A tool that reads file contents with safety checks
///
/// This tool provides safe file reading with:
/// - Line numbers for easy navigation
/// - Offset/limit for reading large files in chunks
/// - Character truncation for long lines
/// - Binary file detection and rejection
#[derive(Debug)]
pub struct ReadTool;

impl ReadTool {
    /// Validates that the path exists and is accessible
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

    /// Detects if a file is binary by checking for null bytes
    fn is_binary(path: &Path) -> Result<bool> {
        let mut file = std::fs::File::open(path)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to open file '{}': {}", path.display(), e)))?;

        let mut buffer = [0u8; 8192];
        let bytes_read = StdIoRead::read(&mut file, &mut buffer)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to read file '{}': {}", path.display(), e)))?;

        let is_binary = buffer[..bytes_read].contains(&0u8);

        Ok(is_binary)
    }

    /// Reads the file and formats the output with line numbers
    fn read_and_format(path: &Path, offset: Option<usize>, limit: Option<usize>) -> Result<String> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(DEFAULT_LINE_LIMIT);

        if Self::is_binary(path)? {
            return Err(thunderus_core::Error::Tool(format!(
                "Cannot read binary file: {}.\n\nBinary files are not supported. This tool only supports text files including source code, markdown, and other text-based formats.",
                path.display()
            )));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to read file '{}': {}", path.display(), e)))?;

        let lines: Vec<&str> = content.lines().collect();

        if offset >= lines.len() && !lines.is_empty() {
            return Ok(format!(
                "Offset {} is beyond file length ({} lines). File: {}",
                offset,
                lines.len(),
                path.display()
            ));
        }

        let start = offset;
        let end = std::cmp::min(offset + limit, lines.len());

        let formatted: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let line_num = start + i + 1;
                let truncated_line = if line.len() > MAX_LINE_LENGTH {
                    format!(
                        "{}\n[Line truncated at {} characters]",
                        &line[..MAX_LINE_LENGTH],
                        MAX_LINE_LENGTH
                    )
                } else {
                    line.to_string()
                };
                format!("{line_num}\u{2192}{truncated_line}", line_num = line_num)
            })
            .collect();

        if formatted.is_empty() {
            Ok(format!("File is empty: {}", path.display()))
        } else {
            Ok(formatted.join("\n"))
        }
    }
}

impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read file contents with line numbers. Use this to view source code, config files, and other text files."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "file_path".to_string(),
                ToolParameter::new_string("Absolute path to the file")
                    .with_description("The absolute path to the file to read"),
            ),
            (
                "offset".to_string(),
                ToolParameter::new_number("Starting line number (0-indexed)")
                    .with_description("The line number to start reading from (0-indexed, default: 0)"),
            ),
            (
                "limit".to_string(),
                ToolParameter::new_number("Maximum number of lines to read")
                    .with_description("Maximum number of lines to read (default: 2000)"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Safe,
            "Read is a read-only operation that does not modify files or system state. It only reads file contents for display purposes.",
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

        let path = PathBuf::from(file_path_str);

        Self::validate_path(&path)?;

        let offset = arguments.get("offset").and_then(|v| v.as_u64()).map(|v| v as usize);
        let limit = arguments.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);
        let result = Self::read_and_format(&path, offset, limit)?;

        Ok(ToolResult::success(tool_call_id, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_tool_properties() {
        let tool = ReadTool;
        assert_eq!(tool.name(), "read");
        assert_eq!(
            tool.description(),
            "Read file contents with line numbers. Use this to view source code, config files, and other text files."
        );
    }

    #[test]
    fn test_read_risk_level() {
        let tool = ReadTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_read_classification() {
        let tool = ReadTool;
        let classification = tool.classification().unwrap();
        assert!(classification.risk.is_safe());
        assert!(classification.reasoning.contains("read-only"));
    }

    #[test]
    fn test_read_spec() {
        let tool = ReadTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "read");
        assert!(spec.description().is_some());
    }

    #[test]
    fn test_read_execute_missing_file_path() {
        let tool = ReadTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_read_1".to_string(), &args);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing or invalid 'file_path'")
        );
    }

    #[test]
    fn test_read_execute_empty_file_path() {
        let tool = ReadTool;
        let args = serde_json::json!({"file_path": ""});
        let result = tool.execute("call_read_2".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_path cannot be empty"));
    }

    #[test]
    fn test_read_execute_nonexistent_file() {
        let tool = ReadTool;
        let args = serde_json::json!({"file_path": "/nonexistent/path/xyz.txt"});
        let result = tool.execute("call_read_3".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_read_execute_directory_path() {
        let tool = ReadTool;
        let args = serde_json::json!({"file_path": "/tmp"});
        let result = tool.execute("call_read_4".to_string(), &args);

        if let Err(e) = result {
            let error_string = e.to_string();
            assert!(error_string.contains("Path does not exist") || error_string.contains("directory"));
        }
    }

    #[test]
    fn test_read_execute_simple_file() {
        let tool = ReadTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_simple.txt");
        let mut file = std::fs::File::create(&temp_file).unwrap();
        writeln!(file, "Line 1").unwrap();
        writeln!(file, "Line 2").unwrap();
        writeln!(file, "Line 3").unwrap();

        let args = serde_json::json!({"file_path": temp_file.to_string_lossy().as_ref()});
        let result = tool.execute("call_read_5".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_read_5");
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("1→Line 1"));
        assert!(tool_result.content.contains("2→Line 2"));
        assert!(tool_result.content.contains("3→Line 3"));
    }

    #[test]
    fn test_read_execute_with_offset() {
        let tool = ReadTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_offset.txt");
        let mut file = std::fs::File::create(&temp_file).unwrap();
        for i in 1..=10 {
            writeln!(file, "Line {}", i).unwrap();
        }

        let args = serde_json::json!({"file_path": temp_file.to_string_lossy().as_ref(), "offset": 5});
        let result = tool.execute("call_read_6".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(!tool_result.content.contains("1→Line 1"));
        assert!(!tool_result.content.contains("5→Line 5"));
        assert!(tool_result.content.contains("6→Line 6"));
        assert!(tool_result.content.contains("10→Line 10"));
    }

    #[test]
    fn test_read_execute_with_limit() {
        let tool = ReadTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_limit.txt");
        let mut file = std::fs::File::create(&temp_file).unwrap();
        for i in 1..=10 {
            writeln!(file, "Line {}", i).unwrap();
        }

        let args = serde_json::json!({"file_path": temp_file.to_string_lossy().as_ref(), "limit": 5});
        let result = tool.execute("call_read_7".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());

        let line_count = tool_result.content.lines().count();
        assert!(line_count <= 5);
        assert!(tool_result.content.contains("1→Line 1"));
        assert!(tool_result.content.contains("5→Line 5"));
    }

    #[test]
    fn test_read_execute_empty_file() {
        let tool = ReadTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_empty.txt");
        std::fs::File::create(&temp_file).unwrap();

        let args = serde_json::json!({"file_path": temp_file.to_string_lossy().as_ref()});
        let result = tool.execute("call_read_8".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("File is empty"));
    }

    #[test]
    fn test_read_execute_offset_beyond_file() {
        let tool = ReadTool;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_read_beyond.txt");
        let mut file = std::fs::File::create(&temp_file).unwrap();
        for i in 1..=5 {
            writeln!(file, "Line {}", i).unwrap();
        }

        let args = serde_json::json!({"file_path": temp_file.to_string_lossy().as_ref(), "offset": 10});
        let result = tool.execute("call_read_9".to_string(), &args);

        let _ = std::fs::remove_file(&temp_file);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("Offset 10 is beyond file length"));
    }
}
