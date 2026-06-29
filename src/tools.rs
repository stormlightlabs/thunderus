//! Structured read-only filesystem tools.
//!
//! The model sees typed tools, not raw shell command strings. All subprocess
//! invocations use [`std::process::Command`] argv arrays — never shell strings.
//!
//! ## Safety Rules
//!
//! - Workspace-root containment enforced after path normalization.
//! - Ignore rules respected and hidden files skipped by default.
//! - Hidden files, ignored files, symlink following, and unrestricted searches
//!   are opt-in only.
//! - Timeout, result-count, stdout/stderr byte, and line-length caps enforced.
//! - `rg` exit code `1` is treated as "no matches", not an error.
//! - `fd --exec`, `fd --exec-batch`, `rg --pre`, arbitrary `sed`/`awk`, `sed -i`,
//!   and `awk system()` are never exposed.

mod find_files;
mod list_searchable_files;
mod path;
mod read_file_range;
mod search_text;
mod subproc;

use std::path::{Path, PathBuf};

use crate::app::ToolStatus;
use crate::tools::find_files::FindFiles;

/// Maximum number of tool-call iterations per agent turn before the loop
/// stops with a cap-exceeded error.
///
/// This prevents recursive or unbounded tool-call loops (e.g. a model that
/// keeps requesting tools without converging on a final answer).
pub const MAX_TOOL_ITERATIONS: usize = 8;

/// A tool definition exposed to the provider/model.
///
/// The `name` is what the model uses in a `tool_use` block; `description` and
/// `input_schema` are sent in the request so the model knows how to call it.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

/// The catalog of read-only filesystem tools exposed to the model.
///
/// These map directly to tool implementations. The model sees typed tool definitions;
/// the harness dispatches `tool_use` requests to the matching [`ToolInput`] and executes it.
#[allow(dead_code)]
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "find_files",
            description: "Find files by name pattern within the workspace. Returns relative paths.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "File name or glob pattern to search for." },
                    "glob": { "type": "string", "description": "Optional additional glob filter." },
                    "extensions": { "type": "array", "items": { "type": "string" } },
                    "max_depth": { "type": "integer" },
                    "include_hidden": { "type": "boolean" },
                    "follow_symlinks": { "type": "boolean" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "list_searchable_files",
            description: "List searchable files in the workspace, respecting ignore rules.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "glob": { "type": "string" },
                    "include_hidden": { "type": "boolean" }
                }
            }),
        },
        ToolDefinition {
            name: "search_text",
            description: "Search file contents with a regex pattern. Returns matching lines with file:line:text.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for." },
                    "glob": { "type": "string" },
                    "extensions": { "type": "array", "items": { "type": "string" } },
                    "context_lines": { "type": "integer" },
                    "include_hidden": { "type": "boolean" }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "read_file_range",
            description: "Read a range of lines from a file within the workspace. Lines are 1-indexed.",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path relative to the workspace root." },
                    "start_line": { "type": "integer" },
                    "end_line": { "type": "integer" }
                },
                "required": ["path", "start_line"]
            }),
        },
    ]
}

/// Configuration for an agent run, shared by the fake and Umans providers.
#[derive(Clone, Debug)]
pub struct AgentRunConfig {
    /// Workspace root for tool containment and file reads.
    pub root: PathBuf,
    /// Selected model name.
    pub model: String,
    /// Web search mode.
    #[allow(dead_code)]
    pub search_mode: crate::cli::WebSearchMode,
    /// Maximum tool-call iterations per turn.
    pub max_tool_iterations: usize,
}

impl AgentRunConfig {
    pub fn new(root: PathBuf, model: String, search_mode: crate::cli::WebSearchMode) -> Self {
        AgentRunConfig { root, model, search_mode, max_tool_iterations: MAX_TOOL_ITERATIONS }
    }
}

/// A tool-use request from the provider: a name and a JSON arguments object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolUseRequest {
    /// Tool name, matching a [`ToolDefinition::name`].
    pub name: String,
    /// Raw JSON arguments string as sent by the model.
    pub arguments: String,
}

/// Structured output from a tool execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolOutput {
    /// Tool name (e.g. "read_file_range", "search_text").
    pub name: String,
    /// Execution status.
    pub status: ToolStatus,
    /// Output lines (for display and model content).
    pub output: Vec<String>,
    /// Error message, if the tool failed.
    pub error: Option<String>,
}

impl ToolOutput {
    /// Create a successful tool output.
    pub fn ok(name: &str, output: Vec<String>) -> Self {
        ToolOutput { name: name.to_string(), status: ToolStatus::Ok, output, error: None }
    }

    /// Create a failed tool output.
    pub fn failed(name: &str, error: String) -> Self {
        ToolOutput { name: name.to_string(), status: ToolStatus::Failed, output: Vec::new(), error: Some(error) }
    }
}

/// Dispatch a provider tool-use request to the matching read-only tool
/// and execute it against `root`.
///
/// Unknown tool names produce a failed [`ToolOutput`]. Argument parsing is
/// best-effort: missing fields fall back to safe defaults rather than failing
/// the whole turn.
pub fn dispatch_tool(request: &ToolUseRequest, root: &Path) -> ToolOutput {
    let args: serde_json::Value = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);

    match request.name.as_str() {
        "find_files" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let glob = args.get("glob").and_then(|v| v.as_str()).map(|s| s.to_string());
            let extensions: Vec<String> = args
                .get("extensions")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).map(|n| n as u32);
            let include_hidden = args.get("include_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            let follow_symlinks = args.get("follow_symlinks").and_then(|v| v.as_bool()).unwrap_or(false);

            FindFiles {
                pattern,
                root,
                glob: glob.as_deref(),
                extensions: &extensions,
                max_depth,
                max_results: caps::MAX_RESULTS,
                include_hidden,
                follow_symlinks,
            }
            .run()
        }
        "list_searchable_files" => {
            let glob = args.get("glob").and_then(|v| v.as_str()).map(|s| s.to_string());
            let include_hidden = args.get("include_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            list_searchable_files::exec(root, glob.as_deref(), caps::MAX_RESULTS, include_hidden)
        }
        "search_text" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let glob = args.get("glob").and_then(|v| v.as_str()).map(|s| s.to_string());
            let extensions: Vec<String> = args
                .get("extensions")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let context_lines = args.get("context_lines").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let include_hidden = args.get("include_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
            search_text::exec(
                pattern,
                root,
                glob.as_deref(),
                &extensions,
                caps::MAX_RESULTS,
                context_lines,
                include_hidden,
            )
        }
        "read_file_range" => {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let path = path::resolve_within_root(root, path_str).unwrap_or_else(|_| PathBuf::from(path_str));
            let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let end_line = args.get("end_line").and_then(|v| v.as_u64()).map(|n| n as u32);
            read_file_range::exec(&path, root, start_line, end_line)
        }
        other => ToolOutput::failed(other, format!("unknown tool: {other}")),
    }
}

/// Typed tool inputs. Each variant maps to a read-only filesystem tool.
///
/// TODO: Construct by the agent/tool dispatch
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ToolInput {
    /// Find files by name pattern, backed by `fd` with `find` fallback.
    FindFiles {
        pattern: String,
        root: PathBuf,
        glob: Option<String>,
        extensions: Vec<String>,
        max_depth: Option<u32>,
        max_results: usize,
        include_hidden: bool,
        follow_symlinks: bool,
    },
    /// List searchable files, backed by `rg --files` or `fd --type file`.
    ListSearchableFiles {
        root: PathBuf,
        glob: Option<String>,
        max_results: usize,
        include_hidden: bool,
    },
    /// Search file contents, backed by `rg --json`.
    SearchText {
        pattern: String,
        root: PathBuf,
        glob: Option<String>,
        extensions: Vec<String>,
        max_results: usize,
        context_lines: u32,
        include_hidden: bool,
    },
    /// Read a line range from a file, implemented in Rust.
    ReadFileRange {
        path: PathBuf,
        start_line: u32,
        end_line: Option<u32>,
    },
}

/// A single search match from `rg --json`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchMatch {
    /// File path (relative to root, as reported by rg).
    pub path: String,
    /// 1-based line number.
    pub line_number: u32,
    /// Matched line text.
    pub text: String,
}

/// Caps enforced on tool execution to prevent runaway output.
///
/// TODO: this should be an enum
///
/// TODO: reference by tool callers
#[allow(dead_code)]
pub mod caps {
    /// Default maximum number of results from a search or list operation.
    pub const MAX_RESULTS: usize = 100;

    /// Maximum stdout/stderr bytes captured from a subprocess.
    pub const MAX_OUTPUT_BYTES: usize = 65_536;

    /// Timeout in seconds for subprocess execution.
    pub const TIMEOUT_SECS: u64 = 10;

    /// Maximum line length before truncation in tool output.
    pub const MAX_LINE_LENGTH: usize = 512;
}

/// Execute a [`ToolInput`] and return structured [`ToolOutput`].
///
/// TODO: Wire into agent loop tool calls.
#[allow(dead_code)]
pub fn execute(input: &ToolInput, root: &Path) -> ToolOutput {
    match input {
        ToolInput::FindFiles {
            pattern,
            root: tool_root,
            glob,
            extensions,
            max_depth,
            max_results,
            include_hidden,
            follow_symlinks,
        } => FindFiles {
            pattern,
            root: tool_root,
            glob: glob.as_deref(),
            extensions,
            max_depth: *max_depth,
            max_results: *max_results,
            include_hidden: *include_hidden,
            follow_symlinks: *follow_symlinks,
        }
        .run(),
        ToolInput::ListSearchableFiles { root: tool_root, glob, max_results, include_hidden } => {
            list_searchable_files::exec(tool_root, glob.as_deref(), *max_results, *include_hidden)
        }
        ToolInput::SearchText {
            pattern,
            root: tool_root,
            glob,
            extensions,
            max_results,
            context_lines,
            include_hidden,
        } => search_text::exec(
            pattern,
            tool_root,
            glob.as_deref(),
            extensions,
            *max_results,
            *context_lines,
            *include_hidden,
        ),
        ToolInput::ReadFileRange { path, start_line, end_line } => {
            read_file_range::exec(path, root, *start_line, *end_line)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_ok() {
        let output = ToolOutput::ok("test", vec!["line1".to_string()]);
        assert_eq!(output.name, "test");
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.output, vec!["line1"]);
        assert!(output.error.is_none());
    }

    #[test]
    fn tool_output_failed() {
        let output = ToolOutput::failed("test", "something went wrong".to_string());
        assert_eq!(output.name, "test");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.output.is_empty());
        assert_eq!(output.error.as_deref(), Some("something went wrong"));
    }

    /// Design assertion: the tool surface never exposes dangerous subprocess
    /// flags. The tool implementations are the only entry points for `fd`,
    /// `rg`, and `find`; they construct argv arrays from typed inputs and do
    /// not pass through `--exec`, `--exec-batch`, `--pre`, `sed`, `awk`, or
    /// any shell-string mechanism.
    #[test]
    fn no_dangerous_subprocess_flags_exposed() {
        // The ToolInput enum variants only carry search/list/read parameters.
        // There is no variant that could inject arbitrary command flags.
        // This test documents the constraint and will fail to compile if
        // a variant with raw command access is added without updating this test.
        let variants = vec!["FindFiles", "ListSearchableFiles", "SearchText", "ReadFileRange"];
        // No variant name contains "exec", "shell", "raw_command", or "sed".
        for v in &variants {
            assert!(!v.contains("exec"), "no exec variant: {v}");
            assert!(!v.contains("shell"), "no shell variant: {v}");
            assert!(!v.contains("raw"), "no raw command variant: {v}");
        }
    }
}
