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
        } => find_files::exec(
            pattern,
            tool_root,
            glob.as_deref(),
            extensions,
            *max_depth,
            *max_results,
            *include_hidden,
            *follow_symlinks,
        ),
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
}
