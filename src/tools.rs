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
//! - Arbitrary `sed`/`awk`, `sed -i`, and `awk system()` are not exposed; only
//!   typed read-only `sawk` inspection actions are available.

pub mod shell;

mod create_file;
mod find_files;
mod list_searchable_files;
mod path;
mod read_file_range;
mod read_url;
mod registry;
mod replace_range;
mod sawk;
mod search_text;
mod subproc;
mod web_search;
mod write_patch;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::ToolStatus;
use crate::cli::WebSearchMode;
use crate::search;

/// Maximum number of tool-call iterations per agent turn before the loop
/// stops with a cap-exceeded error.
///
/// This prevents recursive or unbounded tool-call loops (e.g. a model that
/// keeps requesting tools without converging on a final answer).
pub const MAX_TOOL_ITERATIONS: usize = 8;

/// Maximum number of automatic tool-budget continuations per user turn.
pub const MAX_TOOL_CONTINUATIONS: usize = 3;

/// Default maximum number of results from a search or list operation.
pub const MAX_RESULTS: usize = 100;
/// Maximum stdout/stderr bytes captured from a subprocess.
pub const MAX_OUTPUT_BYTES: usize = 65_536;
/// Timeout in seconds for subprocess execution.
pub const TIMEOUT_SECS: u64 = 10;
/// Maximum line length before truncation in tool output.
pub const MAX_LINE_LEN: usize = 512;

/// Decision returned by [`ToolIterationBudget`] before a provider request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolBudgetDecision {
    /// The next provider request can proceed normally.
    Continue,
    /// The segment cap was reached and a provider-visible continuation
    /// message should be appended before proceeding.
    ContinueAfterBudgetMessage,
    /// The full per-turn tool budget has been exhausted.
    Exhausted {
        segment_iterations: usize,
        total_batches: usize,
        continuations_used: usize,
    },
}

/// The kind of write operation performed on a file.
///
/// Used by [`WriteResult`] and the session record to audit what changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOp {
    /// Create a new file that did not previously exist.
    Create,
    /// Replace the entire contents of an existing file.
    Replace,
    /// Edit a file by replacing a unique exact string occurrence.
    Edit,
}

impl WriteOp {
    /// Lowercase label used in transcript display and session records.
    pub fn label(&self) -> &'static str {
        match self {
            WriteOp::Create => "create",
            WriteOp::Replace => "replace",
            WriteOp::Edit => "edit",
        }
    }
}

/// Bounded tool-batch budget for one user turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIterationBudget {
    segment_limit: usize,
    continuation_limit: usize,
    segment_iterations: usize,
    total_batches: usize,
    continuations_used: usize,
}

impl ToolIterationBudget {
    pub fn new(segment_limit: usize, continuation_limit: usize) -> Self {
        ToolIterationBudget {
            segment_limit,
            continuation_limit,
            segment_iterations: 0,
            total_batches: 0,
            continuations_used: 0,
        }
    }

    pub fn record_tool_batch(&mut self) {
        self.segment_iterations = self.segment_iterations.saturating_add(1);
        self.total_batches = self.total_batches.saturating_add(1);
    }

    pub fn before_provider_request(&mut self) -> ToolBudgetDecision {
        if self.segment_iterations < self.segment_limit {
            return ToolBudgetDecision::Continue;
        }

        if self.continuations_used < self.continuation_limit {
            self.segment_iterations = 0;
            self.continuations_used += 1;
            return ToolBudgetDecision::ContinueAfterBudgetMessage;
        }

        ToolBudgetDecision::Exhausted {
            segment_iterations: self.segment_iterations,
            total_batches: self.total_batches,
            continuations_used: self.continuations_used,
        }
    }

    pub fn total_batches(&self) -> usize {
        self.total_batches
    }

    pub fn continuations_used(&self) -> usize {
        self.continuations_used
    }
}

/// A tool definition exposed to the provider/model.
///
/// The `name` is what the model uses in a `tool_use` block; `description` and
/// `input_schema` are sent in the request so the model knows how to call it.
#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

pub use registry::ProviderSchemaFormat;

/// Configuration for an agent run, shared by the fake and Umans providers.
#[derive(Clone, Debug)]
pub struct AgentRunConfig {
    /// Workspace root for tool containment and file reads.
    pub root: PathBuf,
    /// Selected model name.
    pub model: String,
    /// Web search mode.
    pub search_mode: WebSearchMode,
    /// Maximum tool-call iterations per turn.
    pub max_tool_iterations: usize,
}

impl AgentRunConfig {
    pub fn new(root: PathBuf, model: String, search_mode: WebSearchMode) -> Self {
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
    /// Provider-assigned id (e.g. `toolu_01`) used to correlate the
    /// `tool_result` back to the originating `tool_use` block.
    pub tool_use_id: String,
}

impl ToolUseRequest {
    pub fn new(name: String, args: String, id: String) -> Self {
        Self { name, arguments: args, tool_use_id: id }
    }
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

/// Structured result of a file write operation.
///
/// Captures the operation type, target path, and before/after metadata needed
/// for session audit. The actual file content is never stored — only hashes
/// and byte counts, so secrets and large files are not persisted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteResult {
    /// Operation performed.
    pub op: WriteOp,
    /// Absolute path to the target file.
    pub path: PathBuf,
    /// Hash of the file content before the operation, if it existed.
    pub before_hash: Option<u64>,
    /// Byte count before the operation, if the file existed.
    pub before_bytes: Option<usize>,
    /// Hash of the file content after the operation.
    pub after_hash: u64,
    /// Byte count after the operation.
    pub after_bytes: usize,
}

impl WriteResult {
    /// Render a compact single-line summary for transcript display.
    pub fn summary(&self) -> String {
        let before = match (self.before_hash.is_some(), self.before_bytes) {
            (true, Some(n)) => format!("from {n} bytes"),
            _ => "new file".to_string(),
        };
        format!(
            "{} {} ({} → {} bytes)",
            self.op.label(),
            self.path.display(),
            before,
            self.after_bytes
        )
    }
}

/// Compute a stable hash of content using the standard library hasher.
///
/// Shared by context loading and write operations so hashes are comparable.
pub fn hash_content(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

/// Resolve a workspace path using the same containment and symlink checks as tools.
pub fn resolve_workspace_path(root: &Path, path: &Path) -> std::io::Result<PathBuf> {
    path::resolve_within_root(root, &path.display().to_string())
}

/// The catalog of tools exposed to the model.
///
/// These definitions are derived from the built-in registry so every
/// provider-visible tool has a matching registry entry.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    registry::tool_definitions()
}

fn legacy_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "web_search",
            description: r#"web_search

Search the web for current information.

Use this when the workspace does not contain the answer and you need external
documentation, API specs, or current facts. Prefer reading local files and
searching the workspace first. With native/exa modes, Umans executes server-side
search; with none, a local DuckDuckGo HTML fallback is used. Pair results with
read_url when page content is needed. Capped at 10 results by default."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "The search query." },
                    "max_results": { "type": "integer", "description": "Maximum number of results to return." }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "read_url",
            description: r#"read_url

Fetch a public HTTP/HTTPS URL and extract readable text.

Use to read a page found via web_search or referenced in the workspace. Prefer
local files when available. HTML is extracted to Markdown with Lectito; JSON,
XML, plain text, feeds, and YAML are returned raw. Binary content is rejected.
Private targets, redirects, and non-http(s) schemes are rejected. Size,
redirects, and timeouts are capped; output may truncate."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "The public HTTP/HTTPS URL to fetch." }
                },
                "required": ["url"]
            }),
        },
        ToolDefinition {
            name: "create_file",
            description: r#"create_file

Create a new file with the given content.

Use this for direct new-file writes. Prefer write_patch op=create when doing a
mixed edit. Fails if the file exists. Paths are contained to the workspace root;
escapes are rejected. Parent directories are created if needed."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path relative to the workspace root." },
                    "content": { "type": "string", "description": "The full file content to write." }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "replace_range",
            description: r#"replace_range

Replace a unique exact string occurrence in an existing file.

Use this for direct small edits. Prefer write_patch op=edit when doing a mixed
edit. old_string must match exactly and once; include surrounding context for
uniqueness. Paths are contained to the root; failed edits leave files unchanged."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path relative to the workspace root." },
                    "old_string": { "type": "string", "description": "The exact string to find. Must appear exactly once." },
                    "new_string": { "type": "string", "description": "The replacement string." },
                    "expected_before_hash": { "type": "integer", "description": "Optional current-content hash guard." }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "write_patch",
            description: r#"write_patch

Apply a structured patch to create, replace, or edit a file.

Use this as the preferred file-write tool. Set op=create for new files, op=edit
for exact replacements, or op=replace only for intentional whole-file rewrites.
Supports multi-edit arrays and stale hash guards. Paths are contained; failures leave files unchanged."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "op": { "type": "string", "enum": ["create", "replace", "edit"], "description": "The patch operation." },
                    "path": { "type": "string", "description": "Path relative to the workspace root." },
                    "content": { "type": "string", "description": "Full file content for create/replace ops." },
                    "old_string": { "type": "string", "description": "The exact string to find for legacy single edit ops." },
                    "new_string": { "type": "string", "description": "The replacement string for legacy single edit ops." },
                    "edits": { "type": "array", "items": { "type": "object", "properties": { "old_string": { "type": "string" }, "new_string": { "type": "string" } }, "required": ["old_string", "new_string"] }, "description": "Multiple disjoint replacements for edit ops; all match the original file." },
                    "expected_before_hash": { "type": "integer", "description": "Optional current-content hash guard for edit/replace ops." }
                },
                "required": ["op", "path"]
            }),
        },
        ToolDefinition {
            name: "run_shell",
            description: r#"run_shell

Run a shell command in the workspace and capture stdout, stderr, and exit status.

Prefer narrow tools when they fit: find_files, search_text, read_file_range,
create_file, replace_range, read_url. Use for build, test, format, inspection.

Runs as thndrs with its permissions — not sandboxed. Avoid destructive commands
unless explicitly requested. argv only. Output is capped, truncated, and redacted.
Timeouts enforced."#,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "program": { "type": "string", "description": "The program to run (e.g. \"cargo\", \"ls\")." },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Argv after the program." },
                    "cwd": { "type": "string", "description": "Optional working directory relative to the workspace root." },
                    "timeout_secs": { "type": "integer", "description": "Timeout in seconds." },
                    "background": { "type": "boolean", "description": "If true, run as a long-lived background process." }
                },
                "required": ["program"]
            }),
        },
    ]
}

fn legacy_tool_definition(name: &str) -> Option<ToolDefinition> {
    legacy_tool_definitions()
        .into_iter()
        .find(|definition| definition.name == name)
}

/// Convert the tool catalog into provider-compatible tool schemas.
pub fn provider_tool_catalog_schemas(defs: &[ToolDefinition], format: ProviderSchemaFormat) -> serde_json::Value {
    registry::provider_tool_catalog_schemas(defs, format)
}

/// Convert the tool catalog into Anthropic-compatible tool schemas.
///
/// Returns a compact, stably-ordered JSON array of tool definitions suitable
/// for the `tools` field of a `/v1/messages` request. Each entry carries
/// `name`, `description`, and `input_schema`.
///
/// Send this schema every provider turn. Umans does not expose explicit
/// reusable-history or prompt-cache behavior for tool definitions, so we do
/// not rely on hidden provider memory for them.
pub fn tool_catalog_schemas(defs: &[ToolDefinition]) -> serde_json::Value {
    provider_tool_catalog_schemas(defs, ProviderSchemaFormat::Anthropic)
}

/// Return a recursively sorted JSON value for deterministic debug rendering.
pub fn sorted_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(items.iter().map(sorted_json_value).collect()),
        serde_json::Value::Object(object) => {
            let mut entries: Vec<_> = object.iter().collect();
            entries.sort_by_key(|(key, _)| *key);
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted_json_value(value)))
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

/// Dispatch a provider tool-use request to the matching read-only tool
/// and execute it against `root`.
///
/// Unknown tool names produce a failed [`ToolOutput`]. Argument parsing is
/// best-effort: missing fields fall back to safe defaults rather than failing
/// the whole turn.
#[allow(dead_code)]
pub fn dispatch_tool(request: &ToolUseRequest, root: &Path) -> ToolOutput {
    registry::execute(request, registry::ToolContext::new(root)).output
}

fn dispatch_tool_legacy(request: &ToolUseRequest, root: &Path) -> ToolOutput {
    let args = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);

    match request.name.as_str() {
        "web_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(search::DEFAULT_SEARCH_LIMIT);

            match search::search_duckduckgo(query, max_results) {
                Ok(results) if results.is_empty() => ToolOutput::ok("web_search", vec!["no results found".to_string()]),
                Ok(results) => ToolOutput::ok("web_search", search::format_search_results(&results)),
                Err(e) => ToolOutput::failed("web_search", e.to_string()),
            }
        }
        "read_url" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
            match search::fetch_url(url) {
                Ok(content) => {
                    let mut lines = vec![
                        format!("title: {}", content.title),
                        format!("url: {}", content.final_url),
                        format!("status: {}", content.status),
                    ];
                    if content.truncated {
                        lines.push("(content truncated)".to_string());
                    }
                    lines.push(format!("diagnostics: {}", content.diagnostics.join(", ")));
                    lines.push(content.markdown);
                    ToolOutput::ok("read_url", lines)
                }
                Err(e) => ToolOutput::failed("read_url", e.to_string()),
            }
        }
        "create_file" => {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            create_file::exec(path_str, root, content).0
        }
        "replace_range" => {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old_string = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new_string = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            let expected_before_hash = args.get("expected_before_hash").and_then(|v| v.as_u64());
            if let Some(expected_before_hash) = expected_before_hash {
                replace_range::exec_many(
                    path_str,
                    root,
                    &[replace_range::Replacement {
                        old_string: old_string.to_string(),
                        new_string: new_string.to_string(),
                    }],
                    Some(expected_before_hash),
                )
                .0
            } else {
                replace_range::exec(path_str, root, old_string, new_string).0
            }
        }
        "write_patch" => match write_patch::Patch::from_json(&request.arguments) {
            Ok(patch) => write_patch::exec(&patch, root).0,
            Err(e) => ToolOutput::failed("write_patch", e),
        },
        "run_shell" => {
            let program = args.get("program").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let cmd_args: Vec<String> = args
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let cwd = args.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from);
            let timeout_secs = args.get("timeout_secs").and_then(|v| v.as_u64());
            let kind = if args.get("background").and_then(|v| v.as_bool()).unwrap_or(false) {
                shell::ProcessKind::Background
            } else {
                shell::ProcessKind::OneShot
            };

            if program.is_empty() {
                ToolOutput::failed("run_shell", "missing or empty 'program' field".to_string())
            } else {
                shell::exec(
                    &shell::ShellArgs { program, args: cmd_args, cwd, timeout_secs, kind },
                    root,
                )
            }
        }
        other => ToolOutput::failed(other, format!("unknown tool: {other}")),
    }
}

/// Dispatch a tool-use request that may produce a file write.
///
/// Write-capable tools return both a [`ToolOutput`] for the transcript and an
/// optional [`WriteResult`] for session audit persistence. Non-write tools
/// delegate to [`dispatch_tool`] and return `None` for the write result.
#[allow(dead_code)]
pub fn dispatch_write(request: &ToolUseRequest, root: &Path) -> (ToolOutput, Option<WriteResult>) {
    let execution = registry::execute(request, registry::ToolContext::new(root));
    (execution.output, execution.write_result)
}

fn dispatch_write_legacy(request: &ToolUseRequest, root: &Path) -> (ToolOutput, Option<WriteResult>) {
    let args = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);

    match request.name.as_str() {
        "create_file" => {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            create_file::exec(path_str, root, content)
        }
        "replace_range" => {
            let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let old_string = args.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
            let new_string = args.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
            let expected_before_hash = args.get("expected_before_hash").and_then(|v| v.as_u64());
            if let Some(expected_before_hash) = expected_before_hash {
                replace_range::exec_many(
                    path_str,
                    root,
                    &[replace_range::Replacement {
                        old_string: old_string.to_string(),
                        new_string: new_string.to_string(),
                    }],
                    Some(expected_before_hash),
                )
            } else {
                replace_range::exec(path_str, root, old_string, new_string)
            }
        }
        "write_patch" => match write_patch::Patch::from_json(&request.arguments) {
            Ok(patch) => write_patch::exec(&patch, root),
            Err(e) => (ToolOutput::failed("write_patch", e), None),
        },
        _ => (dispatch_tool_legacy(request, root), None),
    }
}

/// Dispatch a provider tool-use request, returning the tool output, an optional
/// file-write result, and an optional shell-execution result.
///
/// This is the unified entry point for the agent loop: it delegates to
/// [`dispatch_write`] for file-write tools and [`shell::run_command`] for
/// `run_shell`, returning all structured side effects alongside the
/// [`ToolOutput`].
pub fn dispatch_full(
    request: &ToolUseRequest, root: &Path,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    let execution = registry::execute(request, registry::ToolContext::new(root));
    (execution.output, execution.write_result, execution.shell_result)
}

fn dispatch_full_legacy(
    request: &ToolUseRequest, root: &Path,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    if request.name == "run_shell" {
        let (output, result) = dispatch_shell(request, root);
        return (output, None, result);
    }

    let (output, write_result) = dispatch_write_legacy(request, root);
    (output, write_result, None)
}

/// Return searchable file paths for UI features that need file selection.
pub fn searchable_file_paths(root: &Path, max_results: usize) -> Result<Vec<String>, String> {
    let output = list_searchable_files::exec(root, None, max_results, false);
    if output.status == ToolStatus::Failed {
        return Err(output.error.unwrap_or_else(|| "file listing failed".to_string()));
    }

    Ok(output
        .output
        .into_iter()
        .map(|p| normalize_tool_path(root, &p))
        .collect())
}

fn normalize_tool_path(root: &Path, path: &str) -> String {
    let path_buf = PathBuf::from(path);
    path_buf
        .strip_prefix(root)
        .unwrap_or(&path_buf)
        .to_string_lossy()
        .to_string()
}

/// Dispatch a `run_shell` tool-use request and return the tool output plus the
/// structured [`shell::ProcessResult`] for session audit.
fn dispatch_shell(request: &ToolUseRequest, root: &Path) -> (ToolOutput, Option<shell::ProcessResult>) {
    let args = serde_json::from_str(&request.arguments).unwrap_or(serde_json::Value::Null);

    let program = args.get("program").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let cmd_args: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let cwd = args.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from);
    let timeout_secs = args.get("timeout_secs").and_then(|v| v.as_u64());
    let kind = if args.get("background").and_then(|v| v.as_bool()).unwrap_or(false) {
        shell::ProcessKind::Background
    } else {
        shell::ProcessKind::OneShot
    };

    if program.is_empty() {
        return (
            ToolOutput::failed("run_shell", "missing or empty 'program' field".to_string()),
            None,
        );
    }

    let shell_args = shell::ShellArgs { program, args: cmd_args, cwd, timeout_secs, kind };
    let cancel = shell::CancelFlag::new();

    match shell::run_command(&shell_args, root, &cancel) {
        Ok(result) => {
            let output = match result.status {
                shell::ProcessStatus::Ok => ToolOutput::ok("run_shell", result.to_output_lines()),
                _ => {
                    let mut output = result.to_failed_output();
                    output.output = result.to_output_lines();
                    output
                }
            };
            (output, Some(result))
        }
        Err(e) => (ToolOutput::failed("run_shell", e), None),
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
    /// flags.
    ///
    /// The tool implementations are the only entry points for `fd`,
    /// `rg`, and `find`; they construct argv arrays from typed inputs and do
    /// not pass through `--exec`, `--exec-batch`, `--pre`, `sed`, `awk`, or
    /// any shell-string mechanism.
    #[test]
    fn no_dangerous_subprocess_flags_exposed() {
        for v in &["FindFiles", "ListSearchableFiles", "SearchText", "ReadFileRange"] {
            assert!(!v.contains("exec"), "no exec variant: {v}");
            assert!(!v.contains("shell"), "no shell variant: {v}");
            assert!(!v.contains("raw"), "no raw command variant: {v}");
        }
    }

    /// Design assertion: every tool description is minimal but complete.
    ///
    /// Each description must lead with the tool name, state its purpose, and
    /// mention at least one safety limit (containment, caps, truncation, or
    /// rejection). Descriptions stay short so the tool catalog remains compact
    /// when sent every provider turn.
    #[test]
    fn tool_descriptions_are_minimal_and_complete() {
        let defs = tool_definitions();
        assert!(!defs.is_empty(), "tool catalog should not be empty");

        for def in &defs {
            let desc = def.description;
            assert!(
                desc.starts_with(def.name),
                "description for `{}` should lead with its name, got: {desc}",
                def.name
            );
            assert!(
                desc.len() < 450,
                "description for `{}` should be concise (<450 chars), got {} chars",
                def.name,
                desc.len()
            );
            let lower = desc.to_lowercase();
            let mentions_safety = lower.contains("cap")
                || lower.contains("reject")
                || lower.contains("contain")
                || lower.contains("truncat")
                || lower.contains("enforce");
            assert!(
                mentions_safety,
                "description for `{}` should mention a safety limit (caps/rejection/containment/truncation/enforcement), got: {desc}",
                def.name
            );
        }
    }

    /// Design assertion: every tool description includes behavioral guidance
    /// telling the model *when* to use the tool, following the Claude Code
    /// pattern where tool descriptions carry usage policy.
    #[test]
    fn tool_descriptions_include_usage_guidance() {
        let defs = tool_definitions();
        for def in &defs {
            let lower = def.description.to_lowercase();
            let has_guidance = lower.contains("use this") || lower.contains("prefer") || lower.contains("when you");
            assert!(
                has_guidance,
                "description for `{}` should include usage guidance (\"use this\" / \"prefer\" / \"when you\"), got: {}",
                def.name, def.description
            );
        }
    }

    #[test]
    fn tool_catalog_schemas_produces_anthropic_format() {
        let defs = tool_definitions();
        let schemas = tool_catalog_schemas(&defs);
        let arr = schemas.as_array().expect("schemas should be a JSON array");
        assert_eq!(arr.len(), defs.len(), "schema count should match definition count");

        for (schema, def) in arr.iter().zip(defs.iter()) {
            assert_eq!(schema["name"], def.name, "schema name should match");
            assert_eq!(
                schema["description"], def.description,
                "schema description should match"
            );
            assert!(
                schema.get("input_schema").is_some(),
                "schema should have input_schema for {}",
                def.name
            );
        }
    }

    #[test]
    fn tool_catalog_schemas_is_stably_ordered() {
        let defs = tool_definitions();
        let schemas_a = tool_catalog_schemas(&defs);
        let schemas_b = tool_catalog_schemas(&defs);
        assert_eq!(schemas_a, schemas_b, "repeated calls should produce identical ordering");

        let names_a: Vec<&str> = schemas_a
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        let names_defs: Vec<&str> = defs.iter().map(|d| d.name).collect();
        assert_eq!(names_a, names_defs, "schema order should match definition order");
    }

    #[test]
    fn tool_catalog_schemas_empty_for_no_definitions() {
        let schemas = tool_catalog_schemas(&[]);
        assert!(schemas.as_array().unwrap().is_empty());
    }

    #[test]
    fn registry_validates_builtin_entries() {
        registry::validate().expect("built-in registry should validate");
    }

    #[test]
    fn registry_names_are_stable_and_unique() {
        let names: Vec<&str> = registry::builtins().iter().map(|entry| entry.name).collect();
        assert_eq!(
            names,
            vec![
                "find_files",
                "list_searchable_files",
                "search_text",
                "read_file_range",
                "sawk",
                "web_search",
                "read_url",
                "create_file",
                "replace_range",
                "write_patch",
                "run_shell",
            ]
        );

        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "tool names should be unique");
    }

    #[test]
    fn tool_definitions_are_derived_from_registry() {
        let defs = tool_definitions();
        let registry_names: Vec<&str> = registry::builtins().iter().map(|entry| entry.name).collect();
        let definition_names: Vec<&str> = defs.iter().map(|definition| definition.name).collect();
        assert_eq!(definition_names, registry_names);
    }

    #[test]
    fn registry_schemas_are_json_objects_with_required_fields() {
        for entry in registry::builtins() {
            let definition = (entry.definition)();
            let schema = &definition.input_schema;
            assert_eq!(
                schema.get("type").and_then(|value| value.as_str()),
                Some("object"),
                "{} schema should be a JSON object",
                entry.name
            );
            assert!(
                schema.get("properties").is_some_and(|value| value.is_object()),
                "{} schema should define properties",
                entry.name
            );

            match schema.get("required") {
                Some(required) => {
                    assert!(required.is_array(), "{} required fields should be an array", entry.name);
                }
                None => {
                    assert_eq!(
                        entry.name, "list_searchable_files",
                        "{} should declare required fields unless all inputs are optional",
                        entry.name
                    );
                }
            }
        }
    }

    #[test]
    fn registry_examples_are_valid_json_objects() {
        for entry in registry::builtins() {
            let value: serde_json::Value = serde_json::from_str(entry.example_input)
                .unwrap_or_else(|err| panic!("{} example should parse as JSON: {err}", entry.name));
            assert!(value.is_object(), "{} example should be a JSON object", entry.name);
        }
    }

    #[test]
    fn tool_catalog_snapshot_from_registry() {
        let schemas = tool_catalog_schemas(&tool_definitions());
        let json = serde_json::to_string_pretty(&sorted_json_value(&schemas)).expect("catalog JSON");
        insta::assert_snapshot!(json);
    }

    #[test]
    fn provider_tool_catalog_schemas_supports_openai_function_format() {
        let defs = tool_definitions();
        let schemas = provider_tool_catalog_schemas(&defs, ProviderSchemaFormat::OpenAiFunction);
        let arr = schemas.as_array().expect("OpenAI schemas should be an array");
        assert_eq!(arr.len(), defs.len());

        let first = &arr[0];
        assert_eq!(first["type"], "function");
        assert_eq!(first["function"]["name"], defs[0].name);
        assert_eq!(first["function"]["description"], defs[0].description);
        assert_eq!(first["function"]["parameters"], defs[0].input_schema);
    }

    #[test]
    fn tool_budget_continues_before_segment_cap() {
        let mut budget = ToolIterationBudget::new(2, 3);
        budget.record_tool_batch();
        assert_eq!(budget.before_provider_request(), ToolBudgetDecision::Continue);
    }

    #[test]
    fn tool_budget_continues_after_first_segment_cap() {
        let mut budget = ToolIterationBudget::new(2, 3);
        budget.record_tool_batch();
        budget.record_tool_batch();

        assert_eq!(
            budget.before_provider_request(),
            ToolBudgetDecision::ContinueAfterBudgetMessage
        );

        assert_eq!(budget.total_batches(), 2);
        assert_eq!(budget.continuations_used(), 1);
    }

    #[test]
    fn tool_budget_resets_segment_counter_after_continuation() {
        let mut budget = ToolIterationBudget::new(2, 3);
        budget.record_tool_batch();
        budget.record_tool_batch();
        assert_eq!(
            budget.before_provider_request(),
            ToolBudgetDecision::ContinueAfterBudgetMessage
        );

        budget.record_tool_batch();
        assert_eq!(budget.before_provider_request(), ToolBudgetDecision::Continue);
    }

    #[test]
    fn tool_budget_exhausts_after_three_auto_continuations() {
        let mut budget = ToolIterationBudget::new(2, 3);
        for _ in 0..3 {
            budget.record_tool_batch();
            budget.record_tool_batch();
            assert_eq!(
                budget.before_provider_request(),
                ToolBudgetDecision::ContinueAfterBudgetMessage
            );
        }

        budget.record_tool_batch();
        budget.record_tool_batch();
        assert_eq!(
            budget.before_provider_request(),
            ToolBudgetDecision::Exhausted { segment_iterations: 2, total_batches: 8, continuations_used: 3 }
        );
    }
}
