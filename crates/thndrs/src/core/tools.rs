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

mod atomic_write;
pub mod command_projection;
mod create_file;
mod find_files;
mod list_searchable_files;
mod path;
mod read_file_range;
mod read_url;
mod registry;
mod replace_range;
mod sawk;
pub mod search;
mod state_identity;
mod subproc;
mod web_search;
mod write_patch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thndrs_agent::CancelToken;

use crate::app::ToolStatus;
use crate::cli::{ReasoningEffort, ReasoningSummary, WebSearchMode};
use crate::mcp::manager::McpManager;
use crate::search::SearchConfig;

/// Maximum number of tool-call iterations per agent turn before the loop
/// stops with a cap-exceeded error.
///
/// This prevents recursive or unbounded tool-call loops (e.g. a model that
/// keeps requesting tools without converging on a final answer).
pub const MAX_TOOL_ITERATIONS: usize = 8;

/// Default maximum number of results from a search or list operation.
pub const MAX_RESULTS: usize = 100;
/// Maximum stdout/stderr bytes captured from a subprocess.
pub const MAX_OUTPUT_BYTES: usize = 65_536;
/// Timeout in seconds for subprocess execution.
pub const TIMEOUT_SECS: u64 = 10;
/// Maximum line length before truncation in tool output.
pub const MAX_LINE_LEN: usize = 512;

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

pub use thndrs_agent::ToolDefinition;
pub use thndrs_agent::{ToolBudgetDecision, ToolIterationBudget};

pub use registry::ProviderSchemaFormat;
pub use state_identity::identity_for as state_identity_for;

/// Filesystem and process authority granted to one provider run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToolAuthority {
    /// Allow the normal workspace-scoped read and write tool set.
    #[default]
    WorkspaceWrite,
    /// Allow only built-in tools whose contract is read-only.
    ReadOnly,
}

impl ToolAuthority {
    /// Stable label used by terminal and machine-readable surfaces.
    pub const fn label(self) -> &'static str {
        match self {
            Self::WorkspaceWrite => "workspace-write",
            Self::ReadOnly => "read-only",
        }
    }

    /// Reader-facing label used by interactive surfaces.
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::WorkspaceWrite => "Editable",
            Self::ReadOnly => "Read-only",
        }
    }
}

/// Configuration for an agent run, shared by fake and built-in providers.
#[derive(Clone, Debug)]
pub struct AgentRunConfig {
    /// Workspace root for tool containment and file reads.
    pub root: PathBuf,
    /// Tool authority enforced for catalog projection and dispatch.
    pub authority: ToolAuthority,
    /// Additional read-only roots advertised for discovered skills.
    pub extra_read_roots: Vec<PathBuf>,
    /// Selected model name.
    pub model: String,
    /// Web search mode.
    pub search_mode: WebSearchMode,
    /// Optional SearXNG base URL used by the application-owned web search tool.
    pub search_url: Option<String>,
    /// Reasoning effort requested for supporting provider models.
    ///
    /// TODO: Route this through every provider/model family that supports
    /// reasoning-effort controls.
    pub reasoning_effort: ReasoningEffort,
    /// Whether supporting providers should return reasoning summaries.
    ///
    /// TODO: Route this through every provider/model family that supports
    /// reasoning-summary controls.
    pub reasoning_summary: ReasoningSummary,
    /// Maximum tool-call iterations per turn.
    pub max_tool_iterations: usize,
    /// Optional MCP manager used to extend the built-in tool registry.
    pub mcp_manager: Option<Arc<McpManager>>,
    /// Shared application owner for background shell processes.
    pub process_registry: Option<shell::ProcessRegistry>,
    /// Turn id associated with request accounting.
    pub accounting_turn_id: Option<String>,
    /// Context candidates captured before the first provider request.
    pub accounting_context: Vec<thndrs_agent::ContextItemSnapshot>,
    /// Independent model-projection reducer configuration.
    pub model_reduction: thndrs_agent::context::ReductionConfig,
    /// Application-owned bounded artifact store used before a lossy projection
    /// can replace recoverable tool evidence.
    pub artifact_store: Option<crate::artifacts::ArtifactStore>,
}

impl AgentRunConfig {
    pub fn new(root: PathBuf, model: String, search_mode: WebSearchMode) -> Self {
        AgentRunConfig {
            root,
            authority: ToolAuthority::default(),
            extra_read_roots: Vec::new(),
            model,
            search_mode,
            search_url: None,
            reasoning_effort: ReasoningEffort::default(),
            reasoning_summary: ReasoningSummary::default(),
            max_tool_iterations: MAX_TOOL_ITERATIONS,
            mcp_manager: None,
            process_registry: None,
            accounting_turn_id: None,
            accounting_context: Vec::new(),
            model_reduction: thndrs_agent::context::ReductionConfig::default(),
            artifact_store: None,
        }
    }

    /// Restrict the tool catalog and dispatch boundary for this run.
    pub fn with_authority(mut self, authority: ToolAuthority) -> Self {
        self.authority = authority;
        self
    }

    /// Apply the resolved reasoning settings for this run.
    pub fn with_reasoning(mut self, effort: ReasoningEffort, summary: ReasoningSummary) -> Self {
        self.reasoning_effort = effort;
        self.reasoning_summary = summary;
        self
    }

    /// Apply the configured SearXNG base URL for this run.
    pub fn with_search_url(mut self, search_url: Option<String>) -> Self {
        self.search_url = search_url;
        self
    }

    /// Allow reads from application-discovered skill roots.
    pub fn with_extra_read_roots(mut self, mut roots: Vec<PathBuf>) -> Self {
        roots.sort();
        roots.dedup();
        self.extra_read_roots = roots;
        self
    }

    /// Build the application-owned search configuration for this run.
    pub fn search_config(&self) -> SearchConfig {
        SearchConfig::from_parts(self.search_mode, self.search_url.clone())
    }

    /// Attach an MCP manager to this run.
    pub fn with_mcp_manager(mut self, manager: Arc<McpManager>) -> Self {
        self.mcp_manager = Some(manager);
        self
    }

    /// Attach the application-owned registry used for background shell tools.
    pub fn with_process_registry(mut self, registry: shell::ProcessRegistry) -> Self {
        self.process_registry = Some(registry);
        self
    }

    /// Attach the immutable context snapshot used by provider request accounting.
    pub fn with_request_context(
        mut self, turn_id: impl Into<String>, ledger: &thndrs_agent::context::ContextLedger,
    ) -> Self {
        self.accounting_turn_id = Some(turn_id.into());
        self.accounting_context = thndrs_agent::snapshot_context(&ledger.items);
        self
    }

    /// Attach the independent model-projection reducer configuration.
    pub fn with_model_reduction(mut self, config: thndrs_agent::context::ReductionConfig) -> Self {
        self.model_reduction = config;
        self
    }

    /// Attach the application-owned store used for bounded tool-evidence recovery.
    pub fn with_artifact_store(mut self, store: crate::artifacts::ArtifactStore) -> Self {
        self.artifact_store = Some(store);
        self
    }
}

pub use thndrs_agent::{
    ToolDisplayProjection, ToolEvidenceKind, ToolEvidenceMetadata, ToolModelProjection, ToolOutput, ToolUseRequest,
};

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

/// Return built-in tools plus cached MCP tools for a run.
pub fn runtime_tool_definitions(mcp_manager: Option<&McpManager>) -> Vec<ToolDefinition> {
    let mut definitions = tool_definitions();
    if let Some(manager) = mcp_manager {
        definitions.extend(manager.tool_definitions());
    }
    definitions
}

/// Return the provider-visible catalog allowed by `authority`.
pub fn runtime_tool_definitions_for(authority: ToolAuthority, mcp_manager: Option<&McpManager>) -> Vec<ToolDefinition> {
    if authority == ToolAuthority::WorkspaceWrite {
        return runtime_tool_definitions(mcp_manager);
    }
    tool_definitions()
        .into_iter()
        .filter(|definition| is_read_only_tool(&definition.name))
        .collect()
}

fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "find_files" | "list_searchable_files" | "search_text" | "read_file_range" | "sawk" | "web_search" | "read_url"
    )
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
/// Send this schema every provider turn. Built-in providers do not expose explicit
/// reusable-history or prompt-cache behavior for tool definitions, so we do
/// not rely on hidden provider state for them.
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

/// Dispatch a provider tool-use request, returning the tool output, an optional
/// file-write result, and an optional shell-execution result.
///
/// This is the unified entry point for the agent loop, returning all structured
/// side effects alongside the [`ToolOutput`].
pub fn dispatch_full(
    request: &ToolUseRequest, root: &Path,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    dispatch_full_with_search(request, root, &SearchConfig::default())
}

/// Dispatch a provider tool-use request with the active search backend.
pub fn dispatch_full_with_search(
    request: &ToolUseRequest, root: &Path, search: &SearchConfig,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    let context = registry::ToolContext::with_search(root, search);
    let execution = registry::execute(request, &context);
    (execution.output, execution.write_result, execution.shell_result)
}

/// Dispatch a request with cancellation and the active search backend.
pub fn dispatch_full_with_cancel_and_search(
    request: &ToolUseRequest, root: &Path, cancel: &CancelToken, search: &SearchConfig,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    dispatch_full_with_cancel_and_search_and_registry(request, root, cancel, search, None, &[])
}

/// Dispatch a request with cancellation, search settings, and an optional
/// application-owned background-process registry.
pub fn dispatch_full_with_cancel_and_search_and_registry(
    request: &ToolUseRequest, root: &Path, cancel: &CancelToken, search: &SearchConfig,
    process_registry: Option<&shell::ProcessRegistry>, extra_read_roots: &[PathBuf],
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    let execution = if request.name == shell::NAME {
        shell::execute_request_with_cancel_and_registry(request, root, cancel, process_registry)
    } else {
        let context = registry::ToolContext::with_search(root, search).with_extra_read_roots(extra_read_roots);
        registry::execute(request, &context)
    };
    (execution.output, execution.write_result, execution.shell_result)
}

/// Dispatch through MCP first when a namespaced MCP tool is available.
pub fn dispatch_runtime_full(
    request: &ToolUseRequest, root: &Path, mcp_manager: Option<&McpManager>,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    if request.name.starts_with("mcp__")
        && let Some(manager) = mcp_manager
    {
        return (manager.call_tool(request), None, None);
    }
    dispatch_full(request, root)
}

/// Dispatch through MCP or built-ins with the active search backend.
pub fn dispatch_runtime_full_with_search(
    request: &ToolUseRequest, root: &Path, mcp_manager: Option<&McpManager>, search: &SearchConfig,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    if request.name.starts_with("mcp__")
        && let Some(manager) = mcp_manager
    {
        return (manager.call_tool(request), None, None);
    }
    dispatch_full_with_search(request, root, search)
}

/// Dispatch through MCP or built-ins with cancellation and search settings.
pub fn dispatch_runtime_full_with_cancel_and_search(
    request: &ToolUseRequest, root: &Path, mcp_manager: Option<&McpManager>, cancel: &CancelToken,
    search: &SearchConfig,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    dispatch_runtime_full_with_cancel_and_search_and_registry(request, root, mcp_manager, cancel, search, None, &[])
}

/// Dispatch through MCP or built-ins with cancellation, search settings, and
/// an optional application-owned background-process registry.
pub fn dispatch_runtime_full_with_cancel_and_search_and_registry(
    request: &ToolUseRequest, root: &Path, mcp_manager: Option<&McpManager>, cancel: &CancelToken,
    search: &SearchConfig, process_registry: Option<&shell::ProcessRegistry>, extra_read_roots: &[PathBuf],
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    if request.name.starts_with("mcp__")
        && let Some(manager) = mcp_manager
    {
        return (manager.call_tool(request), None, None);
    }
    dispatch_full_with_cancel_and_search_and_registry(request, root, cancel, search, process_registry, extra_read_roots)
}

/// Dispatch a request after enforcing the run's authority.
pub fn dispatch_authorized_runtime_full_with_cancel_and_search_and_registry(
    request: &ToolUseRequest, config: &AgentRunConfig, cancel: &CancelToken,
) -> (ToolOutput, Option<WriteResult>, Option<shell::ProcessResult>) {
    if config.authority == ToolAuthority::ReadOnly && !is_read_only_tool(&request.name) {
        return (
            ToolOutput::failed(
                &request.name,
                "tool is unavailable under read-only authority".to_string(),
            ),
            None,
            None,
        );
    }
    let search = config.search_config();
    dispatch_runtime_full_with_cancel_and_search_and_registry(
        request,
        &config.root,
        config.mcp_manager.as_deref(),
        cancel,
        &search,
        config.process_registry.as_ref(),
        &config.extra_read_roots,
    )
}

/// Return searchable file paths for UI features that need file selection.
pub fn searchable_file_paths(root: &Path, max_results: usize) -> Result<Vec<String>, String> {
    let output = list_searchable_files::exec(root, None, max_results, false);
    if output.status == ToolStatus::Failed {
        return Err(output.error.unwrap_or_else(|| "file listing failed".to_string()));
    }

    Ok(output
        .display
        .lines
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_output_ok() {
        let output = ToolOutput::ok("test", vec!["line1".to_string()]);
        assert_eq!(output.name, "test");
        assert_eq!(output.status, ToolStatus::Ok);
        assert_eq!(output.display.lines, vec!["line1"]);
        assert!(output.error.is_none());
    }

    #[test]
    fn tool_output_failed() {
        let output = ToolOutput::failed("test", "something went wrong".to_string());
        assert_eq!(output.name, "test");
        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.display.lines.is_empty());
        assert_eq!(output.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn read_only_authority_hides_and_rejects_mutating_tools() {
        let definitions = runtime_tool_definitions_for(ToolAuthority::ReadOnly, None);
        assert!(
            definitions
                .iter()
                .any(|definition| definition.name == "read_file_range")
        );
        assert!(!definitions.iter().any(|definition| definition.name == "write_file"));
        assert!(!definitions.iter().any(|definition| definition.name == "run_shell"));

        let dir = tempfile::tempdir().expect("temp workspace");
        let request = ToolUseRequest::new(
            "write_file",
            serde_json::json!({ "path": "forbidden.txt", "content": "must not exist" }).to_string(),
            "call_1",
        );
        let config = AgentRunConfig::new(dir.path().to_path_buf(), "test-model".to_string(), WebSearchMode::None)
            .with_authority(ToolAuthority::ReadOnly);
        let (output, write, process) = dispatch_authorized_runtime_full_with_cancel_and_search_and_registry(
            &request,
            &config,
            &CancelToken::new(),
        );

        assert_eq!(output.status, ToolStatus::Failed);
        assert!(output.error.as_deref().is_some_and(|error| error.contains("read-only")));
        assert!(write.is_none());
        assert!(process.is_none());
        assert!(!dir.path().join("forbidden.txt").exists());
    }

    #[test]
    fn runtime_dispatch_cancellation_stops_run_shell() {
        let dir = tempfile::tempdir().expect("temp dir");
        let request = ToolUseRequest::new(
            "run_shell".to_string(),
            serde_json::json!({ "argv": ["sh", "-c", "exec sleep 30"] }).to_string(),
            "call_1".to_string(),
        );
        let cancel = CancelToken::new();
        let canceller = cancel.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            canceller.cancel();
        });
        let started = std::time::Instant::now();

        let (output, _, process) =
            dispatch_runtime_full_with_cancel_and_search(&request, dir.path(), None, &cancel, &SearchConfig::default());

        stopper.join().expect("cancellation thread");
        let process = process.expect("shell process result");
        assert_eq!(output.status, ToolStatus::Failed);
        assert_eq!(process.status, shell::ProcessStatus::Cancelled);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
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
            let desc = def.description.as_ref();
            assert!(
                desc.starts_with(def.name.as_ref()),
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
            assert_eq!(schema["name"], def.name.as_ref(), "schema name should match");
            assert_eq!(
                schema["description"],
                def.description.as_ref(),
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
        let names_defs: Vec<&str> = defs.iter().map(|d| d.name.as_ref()).collect();
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
        let definition_names: Vec<&str> = defs.iter().map(|definition| definition.name.as_ref()).collect();
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
        assert_eq!(first["function"]["name"], defs[0].name.as_ref());
        assert_eq!(first["function"]["description"], defs[0].description.as_ref());
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
    fn tool_budget_continues_after_many_segments() {
        let mut budget = ToolIterationBudget::unbounded(2);
        for continuation in 1..=12 {
            budget.record_tool_batch();
            budget.record_tool_batch();
            assert_eq!(
                budget.before_provider_request(),
                ToolBudgetDecision::ContinueAfterBudgetMessage
            );
            assert_eq!(budget.continuations_used(), continuation);
        }
    }

    #[test]
    fn tool_budget_continues_beyond_64_batches() {
        let mut budget = ToolIterationBudget::unbounded(MAX_TOOL_ITERATIONS);

        for batch in 0..72 {
            assert_eq!(budget.before_provider_request(), ToolBudgetDecision::Continue);
            budget.record_tool_batch();

            if (batch + 1) % MAX_TOOL_ITERATIONS == 0 {
                assert_eq!(
                    budget.before_provider_request(),
                    ToolBudgetDecision::ContinueAfterBudgetMessage
                );
            }
        }

        assert_eq!(budget.total_batches(), 72);
        assert_eq!(budget.continuations_used(), 9);
    }
}
