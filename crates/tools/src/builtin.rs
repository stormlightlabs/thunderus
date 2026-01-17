use ignore::Walk;
use serde_json::Value;
use std::io::Read as StdIoRead;
use std::path::{Path, PathBuf};
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

/// Output mode for the Grep tool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrepOutputMode {
    /// Return only file paths containing matches
    FilesWithMatches,
    /// Return matching lines with context
    Content,
    /// Return match counts per file
    Count,
}

impl GrepOutputMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "files_with_matches" | "files" => Some(Self::FilesWithMatches),
            "content" => Some(Self::Content),
            "count" => Some(Self::Count),
            _ => None,
        }
    }
}

/// Options for the Grep tool
struct GrepOptions<'a> {
    pattern: &'a str,
    path: &'a Path,
    glob: Option<&'a str>,
    output_mode: GrepOutputMode,
    context_before: Option<usize>,
    context_after: Option<usize>,
    case_insensitive: bool,
    head_limit: Option<usize>,
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

    fn classification(&self) -> Option<Classification> {
        Some(Classification::new(
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
            .and_then(GrepOutputMode::from_str)
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

/// Sort order for glob results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobSortOrder {
    /// Sort by modification time (newest first)
    ModifiedTime,
    /// Sort by file path (alphabetical)
    Path,
    /// No sorting (useful for large directories)
    None,
}

impl GlobSortOrder {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "modified" | "time" | "mtime" => Some(Self::ModifiedTime),
            "path" | "name" | "alpha" => Some(Self::Path),
            "none" | "unsorted" => Some(Self::None),
            _ => None,
        }
    }
}

/// Options for the Glob tool
struct GlobOptions<'a> {
    pattern: &'a str,
    path: &'a Path,
    sort_order: GlobSortOrder,
    respect_gitignore: bool,
    limit: Option<usize>,
}

/// A tool that finds files matching glob patterns
///
/// This tool provides fast file discovery with .gitignore awareness,
/// similar to the "fast file search" pattern in Claude Code.
#[derive(Debug)]
pub struct GlobTool;

impl GlobTool {
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

    /// Executes the glob search and returns matching files
    fn execute_and_parse(options: &GlobOptions) -> Result<String> {
        Self::validate_path(options.path)?;

        let mut results: Vec<PathBuf> = Vec::new();

        if options.respect_gitignore {
            let walk_builder = Walk::new(options.path);
            let pattern = options.pattern.trim_start_matches('/');

            for entry in walk_builder {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        if path.is_dir() {
                            continue;
                        }

                        if let Ok(rel_path) = path.strip_prefix(options.path) {
                            let rel_str = rel_path.to_string_lossy();

                            if Self::matches_glob_pattern(&rel_str, pattern) {
                                results.push(path.to_path_buf());

                                if let Some(limit) = options.limit
                                    && results.len() >= limit
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: error reading path: {}", e);
                    }
                }
            }
        } else {
            let full_pattern = if options.path == Path::new(".") {
                options.pattern.to_string()
            } else {
                format!("{}/{}", options.path.display(), options.pattern.trim_start_matches('/'))
            };

            if let Ok(glob_iter) = glob::glob(&full_pattern) {
                for entry in glob_iter {
                    match entry {
                        Ok(path) => {
                            if path.is_dir() {
                                continue;
                            }

                            results.push(path);
                        }
                        Err(e) => {
                            eprintln!("Warning: error reading path: {}", e);
                        }
                    }

                    if let Some(limit) = options.limit
                        && results.len() >= limit
                    {
                        break;
                    }
                }
            }
        }

        match options.sort_order {
            GlobSortOrder::ModifiedTime => {
                results.sort_by_key(|path| {
                    std::fs::metadata(path)
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                });
                results.reverse();
            }
            GlobSortOrder::Path => results.sort(),
            GlobSortOrder::None => (),
        }

        if results.is_empty() {
            Ok(format!("No files found matching pattern: {}", options.pattern))
        } else {
            let base_path = options
                .path
                .canonicalize()
                .unwrap_or_else(|_| options.path.to_path_buf());

            let formatted: Vec<String> = results
                .iter()
                .filter_map(|p| {
                    if let Ok(rel) = p.strip_prefix(&base_path) {
                        let rel_str = rel.display().to_string();
                        if rel_str.is_empty() {
                            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                        } else {
                            Some(rel_str)
                        }
                    } else if let Ok(rel) = p.strip_prefix(options.path) {
                        let rel_str = rel.display().to_string();
                        if rel_str.is_empty() {
                            p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
                        } else {
                            Some(rel_str)
                        }
                    } else {
                        p.to_str().map(|s| s.to_string())
                    }
                })
                .collect();

            Ok(formatted.join("\n"))
        }
    }

    /// Checks if a path matches a glob pattern
    ///
    /// This is a simplified glob matcher that supports:
    /// - * matches any sequence of non-separator characters
    /// - ** matches any sequence of characters (including separators)
    /// - ? matches any single non-separator character
    fn matches_glob_pattern(path: &str, pattern: &str) -> bool {
        if pattern == "*" || pattern == "**" {
            return true;
        }

        let regex_pattern = pattern
            .replace('\\', r"\\")
            .replace('.', r"\.")
            .replace('?', ".")
            .replace("**/", "(?:.*/)?")
            .replace("**", ".*")
            .replace('*', "[^/]*");

        if let Ok(re) = regex_lite::Regex::new(&regex_pattern) {
            re.is_match(path)
        } else {
            let simplified = pattern.replace(['*', '?'], "");
            path.contains(&simplified)
        }
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching glob patterns. Use this instead of bash find for file discovery."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "pattern".to_string(),
                ToolParameter::new_string("Glob pattern").with_description(
                    "Glob pattern to match files (e.g., '**/*.rs', 'src/**/test_*.rs', '*.{ts,tsx}')",
                ),
            ),
            (
                "path".to_string(),
                ToolParameter::new_string("Directory to search")
                    .with_description("Directory path to search in (defaults to current directory)"),
            ),
            (
                "sort_order".to_string(),
                ToolParameter::new_string("Sort order").with_description(
                    "Sort order: 'modified' (default, newest first), 'path' (alphabetical), or 'none'",
                ),
            ),
            (
                "respect_gitignore".to_string(),
                ToolParameter::new_boolean("Respect .gitignore")
                    .with_description("Whether to respect .gitignore rules (default: true)"),
            ),
            (
                "limit".to_string(),
                ToolParameter::new_number("Max results")
                    .with_description("Maximum number of results to return (default: unlimited)"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Safe
    }

    fn classification(&self) -> Option<Classification> {
        Some(Classification::new(
            ToolRisk::Safe,
            "Glob is a read-only file discovery operation that does not modify files or system state. It only reads directory listings to find matching paths.",
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
        let sort_order_str = arguments.get("sort_order").and_then(|v| v.as_str());
        let sort_order = sort_order_str
            .and_then(GlobSortOrder::from_str)
            .unwrap_or(GlobSortOrder::ModifiedTime);
        let respect_gitignore = arguments
            .get("respect_gitignore")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = arguments.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

        let path = PathBuf::from(path_str);
        Self::validate_path(&path)?;

        let options = GlobOptions { pattern, path: &path, sort_order, respect_gitignore, limit };

        let result = Self::execute_and_parse(&options)?;

        Ok(ToolResult::success(tool_call_id, result))
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

/// Helper function to create a grep tool call for testing
#[cfg(test)]
pub fn grep_tool_call(id: &str, pattern: &str, arguments: serde_json::Value) -> thunderus_providers::ToolCall {
    let mut full_args = serde_json::json!({"pattern": pattern});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    thunderus_providers::ToolCall::new(id, "grep", full_args)
}

/// Helper function to create a glob tool call for testing
#[cfg(test)]
pub fn glob_tool_call(id: &str, pattern: &str, arguments: serde_json::Value) -> thunderus_providers::ToolCall {
    let mut full_args = serde_json::json!({"pattern": pattern});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    thunderus_providers::ToolCall::new(id, "glob", full_args)
}

/// Helper function to create a read tool call for testing
#[cfg(test)]
pub fn read_tool_call(id: &str, file_path: &str, arguments: serde_json::Value) -> thunderus_providers::ToolCall {
    let mut full_args = serde_json::json!({"file_path": file_path});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    thunderus_providers::ToolCall::new(id, "read", full_args)
}

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

    fn classification(&self) -> Option<Classification> {
        Some(Classification::new(
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
            GrepOutputMode::from_str("files_with_matches"),
            Some(GrepOutputMode::FilesWithMatches)
        );
        assert_eq!(
            GrepOutputMode::from_str("files"),
            Some(GrepOutputMode::FilesWithMatches)
        );
        assert_eq!(GrepOutputMode::from_str("content"), Some(GrepOutputMode::Content));
        assert_eq!(GrepOutputMode::from_str("count"), Some(GrepOutputMode::Count));
        assert_eq!(GrepOutputMode::from_str("invalid"), None);
    }

    #[test]
    fn test_grep_tool_call_helper() {
        let tool_call = grep_tool_call("test_id", "test_pattern", serde_json::json!({"case_insensitive": true}));
        assert_eq!(tool_call.id, "test_id");
        assert_eq!(tool_call.name(), "grep");
        assert!(tool_call.arguments().get("pattern").is_some());
        assert!(tool_call.arguments().get("case_insensitive").is_some());
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

    #[test]
    fn test_glob_tool_properties() {
        let tool = GlobTool;
        assert_eq!(tool.name(), "glob");
        assert_eq!(
            tool.description(),
            "Find files matching glob patterns. Use this instead of bash find for file discovery."
        );
    }

    #[test]
    fn test_glob_risk_level() {
        let tool = GlobTool;
        assert!(tool.risk_level().is_safe());
    }

    #[test]
    fn test_glob_classification() {
        let tool = GlobTool;
        let classification = tool.classification().unwrap();
        assert!(classification.risk.is_safe());
        assert!(classification.reasoning.contains("read-only"));
    }

    #[test]
    fn test_glob_spec() {
        let tool = GlobTool;
        let spec = tool.spec();

        assert_eq!(spec.name(), "glob");
        assert!(spec.description().is_some());
    }

    #[test]
    fn test_glob_execute_missing_pattern() {
        let tool = GlobTool;
        let args = serde_json::json!({});
        let result = tool.execute("call_glob_1".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing or invalid 'pattern'"));
    }

    #[test]
    fn test_glob_execute_empty_pattern() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": ""});
        let result = tool.execute("call_glob_2".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Pattern cannot be empty"));
    }

    #[test]
    fn test_glob_execute_invalid_path() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "*.rs", "path": "/nonexistent/path/xyz"});
        let result = tool.execute("call_glob_3".to_string(), &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path does not exist"));
    }

    #[test]
    fn test_glob_sort_order_from_str() {
        assert_eq!(GlobSortOrder::from_str("modified"), Some(GlobSortOrder::ModifiedTime));
        assert_eq!(GlobSortOrder::from_str("time"), Some(GlobSortOrder::ModifiedTime));
        assert_eq!(GlobSortOrder::from_str("path"), Some(GlobSortOrder::Path));
        assert_eq!(GlobSortOrder::from_str("alpha"), Some(GlobSortOrder::Path));
        assert_eq!(GlobSortOrder::from_str("none"), Some(GlobSortOrder::None));
        assert_eq!(GlobSortOrder::from_str("unsorted"), Some(GlobSortOrder::None));
        assert_eq!(GlobSortOrder::from_str("invalid"), None);
    }

    #[test]
    fn test_glob_tool_call_helper() {
        let tool_call = glob_tool_call("test_id", "*.rs", serde_json::json!({"limit": 10}));
        assert_eq!(tool_call.id, "test_id");
        assert_eq!(tool_call.name(), "glob");
        assert!(tool_call.arguments().get("pattern").is_some());
        assert!(tool_call.arguments().get("limit").is_some());
    }

    #[test]
    fn test_glob_execute_simple_pattern() {
        let tool = GlobTool;

        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src", "limit": 20});
        let result = tool.execute("call_glob_4".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_glob_4");
        assert!(tool_result.is_success());
        assert!(!tool_result.content.is_empty());
        assert!(!tool_result.content.contains("No files found"));
    }

    #[test]
    fn test_glob_execute_with_limit() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "**/*.rs", "path": "src", "limit": 3});
        let result = tool.execute("call_glob_5".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());

        let line_count = tool_result.content.lines().count();
        assert!(line_count <= 3);
    }

    #[test]
    fn test_glob_execute_no_results() {
        let tool = GlobTool;
        let args = serde_json::json!({"pattern": "*.nonexistent", "path": "src"});
        let result = tool.execute("call_glob_6".to_string(), &args);

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(tool_result.is_success());
        assert!(tool_result.content.contains("No files found"));
    }

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
    fn test_read_tool_call_helper() {
        let tool_call = read_tool_call("test_id", "/path/to/file.txt", serde_json::json!({"offset": 10}));
        assert_eq!(tool_call.id, "test_id");
        assert_eq!(tool_call.name(), "read");
        assert!(tool_call.arguments().get("file_path").is_some());
        assert!(tool_call.arguments().get("offset").is_some());
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
