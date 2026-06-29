//! Structured tool types.
//!
//! These type definitions exist before any write-capable tool is implemented.
//! The model sees typed tools, not raw shell command strings.

use crate::app::ToolStatus;

/// Structured output from a tool execution. This is what gets rendered in the
/// transcript and what gets sent back to the model.
///
/// TODO: tool implementations
#[allow(dead_code)]
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

/// TODO: tool implementations
#[allow(dead_code)]
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
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum ToolInput {
    /// Find files by name pattern, backed by `fd` with `find` fallback.
    FindFiles {
        pattern: String,
        root: std::path::PathBuf,
        glob: Option<String>,
        extensions: Vec<String>,
        max_depth: Option<u32>,
        max_results: usize,
        include_hidden: bool,
        follow_symlinks: bool,
    },
    /// List searchable files, backed by `rg --files` or `fd --type file`.
    ListSearchableFiles {
        root: std::path::PathBuf,
        glob: Option<String>,
        max_results: usize,
        include_hidden: bool,
    },
    /// Search file contents, backed by `rg --json`.
    SearchText {
        pattern: String,
        root: std::path::PathBuf,
        glob: Option<String>,
        extensions: Vec<String>,
        max_results: usize,
        context_lines: u32,
        include_hidden: bool,
    },
    /// Read a byte range from a file, implemented in Rust.
    ReadFileRange {
        path: std::path::PathBuf,
        start_line: u32,
        end_line: Option<u32>,
    },
}

/// Caps enforced on tool execution to prevent runaway output.
///
/// TODO: tool implementations
#[allow(dead_code)]
pub mod caps {
    /// Maximum number of results from a search or list operation.
    pub const MAX_RESULTS: usize = 100;

    /// Maximum stdout/stderr bytes captured from a subprocess.
    pub const MAX_OUTPUT_BYTES: usize = 65_536;

    /// Timeout in seconds for subprocess execution.
    pub const TIMEOUT_SECS: u64 = 10;

    /// Maximum line length before truncation in tool output.
    pub const MAX_LINE_LENGTH: usize = 512;
}
