use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// Output mode for the Grep tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrepOutputMode {
    /// Return only file paths containing matches
    FilesWithMatches,
    /// Return matching lines with context
    Content,
    /// Return match counts per file
    Count,
}

impl GrepOutputMode {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "files_with_matches" | "files" => Some(Self::FilesWithMatches),
            "content" => Some(Self::Content),
            "count" => Some(Self::Count),
            _ => None,
        }
    }
}

/// Options for the Grep tool
pub struct GrepOptions<'a> {
    pub pattern: &'a str,
    pub path: &'a Path,
    pub glob: Option<&'a str>,
    pub output_mode: GrepOutputMode,
    pub context_before: Option<usize>,
    pub context_after: Option<usize>,
    pub case_insensitive: bool,
    pub head_limit: Option<usize>,
}

/// A tool that searches for patterns in files using ripgrep
///
/// This tool provides fast, code-aware pattern search with structured output.
/// It uses ripgrep (rg) when available, falling back to grep if needed.
#[derive(Debug)]
pub struct GrepTool;

impl GrepTool {
    /// Checks if ripgrep is available on the system
    fn has_rg() -> bool {
        Command::new("rg")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Builds the ripgrep or grep command with appropriate arguments
    fn build_command(options: &GrepOptions) -> Vec<String> {
        let use_rg = Self::has_rg();
        let mut cmd = if use_rg { vec!["rg".to_string()] } else { vec!["grep".to_string()] };

        if options.case_insensitive {
            cmd.push("-i".to_string());
        }
        if let Some(n) = options.context_before {
            cmd.push(format!("-B{n}"));
        }
        if let Some(n) = options.context_after {
            cmd.push(format!("-A{n}"));
        }

        match options.output_mode {
            GrepOutputMode::FilesWithMatches => {
                cmd.push("-l".to_string());
            }
            GrepOutputMode::Content => {
                cmd.push(if use_rg { "-N" } else { "-n" }.to_string());
            }
            GrepOutputMode::Count => {
                cmd.push("-c".to_string());
            }
        }

        if use_rg {
            if let Some(g) = options.glob {
                cmd.push("--glob".to_string());
                cmd.push(g.to_string());
            }
            cmd.push("-.".to_string());
        }

        if !use_rg {
            cmd.push("-r".to_string());
            cmd.push("-n".to_string());
        }

        cmd.push(options.pattern.to_string());

        if options.path != Path::new(".") {
            cmd.push(options.path.display().to_string());
        }

        cmd
    }

    /// Executes the grep command and parses the output
    fn execute_and_parse(options: &GrepOptions) -> Result<String> {
        let cmd_args = Self::build_command(options);

        let program = &cmd_args[0];
        let args = &cmd_args[1..];

        let output = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                thunderus_core::Error::Tool(format!(
                    "Failed to execute {} command: {}",
                    if Self::has_rg() { "ripgrep" } else { "grep" },
                    e
                ))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.stdout.is_empty() && !stderr.is_empty() {
                return Ok(format!("No matches found. Search pattern: {}", options.pattern));
            }
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        let result = if let Some(limit) = options.head_limit {
            let lines: Vec<&str> = stdout.lines().take(limit).collect();
            lines.join("\n")
        } else {
            stdout.into_owned()
        };

        if result.is_empty() {
            Ok(format!("No matches found for pattern: {}", options.pattern))
        } else {
            Ok(result)
        }
    }

    /// Validates that the path exists and is accessible
    fn validate_path(path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(thunderus_core::Error::Validation(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for patterns in files using ripgrep. Use this instead of bash grep for code search."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "pattern".to_string(),
                ToolParameter::new_string("Regex pattern to search")
                    .with_description("The regex pattern to search for in files"),
            ),
            (
                "path".to_string(),
                ToolParameter::new_string("Directory or file to search")
                    .with_description("Directory or file path to search in (defaults to current directory)"),
            ),
            (
                "glob".to_string(),
                ToolParameter::new_string("File filter pattern")
                    .with_description("Glob pattern to filter files (e.g., '*.rs', '*.{ts,tsx}')"),
            ),
            (
                "output_mode".to_string(),
                ToolParameter::new_string("Output format")
                    .with_description("Output mode: 'files_with_matches' (default), 'content', or 'count'"),
            ),
            (
                "context_before".to_string(),
                ToolParameter::new_number("Lines before match")
                    .with_description("Number of lines to show before each match (like grep -B)"),
            ),
            (
                "context_after".to_string(),
                ToolParameter::new_number("Lines after match")
                    .with_description("Number of lines to show after each match (like grep -A)"),
            ),
            (
                "case_insensitive".to_string(),
                ToolParameter::new_boolean("Case-insensitive search")
                    .with_description("Perform case-insensitive search (like grep -i)"),
            ),
            (
                "head_limit".to_string(),
                ToolParameter::new_number("Max results")
                    .with_description("Maximum number of results to return (default: 100 files/lines)"),
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
            "Grep is a read-only search operation that does not modify files or system state. It only reads file contents to find matching patterns.",
        ))
    }

    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult> {
        let pattern = arguments
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| thunderus_core::Error::Validation("Missing or invalid 'pattern' parameter".to_string()))?;

        if pattern.is_empty() {
            return Err(thunderus_core::Error::Validation("Pattern cannot be empty".to_string()));
        }

        let path_str = arguments.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let glob = arguments.get("glob").and_then(|v| v.as_str());
        let output_mode = arguments
            .get("output_mode")
            .and_then(|v| v.as_str())
            .and_then(GrepOutputMode::parse_str)
            .unwrap_or(GrepOutputMode::FilesWithMatches);
        let context_before = arguments
            .get("context_before")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let context_after = arguments
            .get("context_after")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let case_insensitive = arguments
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let head_limit = arguments.get("head_limit").and_then(|v| v.as_u64()).map(|v| v as usize);

        let path = PathBuf::from(path_str);
        Self::validate_path(&path)?;

        let options = GrepOptions {
            pattern,
            path: &path,
            glob,
            output_mode,
            context_before,
            context_after,
            case_insensitive,
            head_limit: head_limit.or(Some(100)),
        };

        let result = Self::execute_and_parse(&options)?;

        Ok(ToolResult::success(tool_call_id, result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_tool_properties() {
        let tool = GrepTool;
        assert_eq!(tool.name(), "grep");
        assert_eq!(
            tool.description(),
            "Search for patterns in files using ripgrep. Use this instead of bash grep for code search."
        );
    }

    #[test]
    fn test_grep_risk_level() {
        let tool = GrepTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_grep_classification() {
        let tool = GrepTool;
        let classification = tool.classification().unwrap();
        assert!(classification.risk.is_safe());
        assert!(classification.reasoning.contains("read-only"));
    }

    #[test]
    fn test_grep_spec() {
        let tool = GrepTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "grep");
        assert!(spec.description().is_some());
    }

    #[test]
    fn test_grep_execute_missing_pattern() {
        let tool = GrepTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_grep_1".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing or invalid 'pattern'"));
    }

    #[test]
    fn test_grep_execute_empty_pattern() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": ""});
        let result = tool.execute("call_grep_2".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Pattern cannot be empty"));
    }

    #[test]
    fn test_grep_execute_invalid_path() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "test", "path": "/nonexistent/path/xyz"});
        let result = tool.execute("call_grep_3".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_grep_output_mode_from_str() {
        assert_eq!(
            GrepOutputMode::parse_str("files_with_matches"),
            Some(GrepOutputMode::FilesWithMatches)
        );
        assert_eq!(
            GrepOutputMode::parse_str("files"),
            Some(GrepOutputMode::FilesWithMatches)
        );
        assert_eq!(GrepOutputMode::parse_str("content"), Some(GrepOutputMode::Content));
        assert_eq!(GrepOutputMode::parse_str("count"), Some(GrepOutputMode::Count));
        assert_eq!(GrepOutputMode::parse_str("invalid"), None);
    }

    #[test]
    fn test_grep_execute_simple_pattern() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "pub fn", "path": "src"});
        let result = tool.execute("call_grep_4".to_string(), &args);

        if let Err(ref e) = result {
            eprintln!("Error: {:?}", e);
        }
        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_grep_4");
        assert!(tool_result.is_success());
    }

    #[test]
    fn test_grep_execute_with_head_limit() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "use", "path": "src", "head_limit": 5});
        let result = tool.execute("call_grep_5".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());

        let line_count = tool_result.content.lines().count();
        assert!(line_count <= 5);
    }

    #[test]
    fn test_grep_execute_case_insensitive() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "TEST", "path": "src", "case_insensitive": true, "head_limit": 10});
        let result = tool.execute("call_grep_6".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
    }

    #[test]
    fn test_grep_execute_with_output_mode() {
        let tool = GrepTool;
        let args = serde_json::json!({"pattern": "pub fn", "path": "src", "output_mode": "count", "head_limit": 20});
        let result = tool.execute("call_grep_7".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
    }
}
