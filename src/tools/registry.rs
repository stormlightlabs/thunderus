//! Built-in tool registry contract.
//!
//! The registry is a thin typed boundary around the current tool
//! implementations. Each entry has one stable model-visible name, one schema,
//! one executor, and one valid example input. During the migration the
//! executors delegate to the existing dispatch paths; later milestones can
//! move parsing and execution into each tool module without changing provider
//! catalog generation or the agent loop.
//!
//! Provider schemas are derived from [`ToolDefinition`] through
//! [`provider_tool_catalog_schemas`]. Structured side effects flow through
//! [`ToolExecution`], so the session layer consumes file-write and shell audits
//! without knowing which tool produced them.

use std::collections::HashSet;
use std::path::Path;

use thiserror::Error;

use super::{ToolDefinition, ToolOutput, ToolUseRequest, WriteResult, shell};

const TOOL_NAMES: &[&str] = &[
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
];

const TOOL_EXAMPLES: &[(&str, &str)] = &[
    ("find_files", r#"{"pattern":"Cargo.toml"}"#),
    ("list_searchable_files", r#"{"glob":"src/**/*.rs"}"#),
    ("search_text", r#"{"pattern":"fn main","glob":"src/**/*.rs"}"#),
    (
        "read_file_range",
        r#"{"path":"Cargo.toml","start_line":1,"end_line":3}"#,
    ),
    (
        "sawk",
        r#"{"action":"sed_print","path":"Cargo.toml","start_line":1,"end_line":3}"#,
    ),
    ("web_search", r#"{"query":"Rust serde documentation","max_results":3}"#),
    ("read_url", r#"{"url":"https://example.com"}"#),
    ("create_file", r#"{"path":"notes.txt","content":"hello\n"}"#),
    (
        "replace_range",
        r#"{"path":"notes.txt","old_string":"hello","new_string":"hi"}"#,
    ),
    (
        "write_patch",
        r#"{"op":"edit","path":"notes.txt","old_string":"hello","new_string":"hi"}"#,
    ),
    ("run_shell", r#"{"program":"cargo","args":["test","tools"]}"#),
];

/// Provider-specific schema shape for model-visible tool definitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderSchemaFormat {
    /// Anthropic-compatible Messages API format.
    Anthropic,
    /// OpenAI-compatible function-tool format.
    #[allow(dead_code)]
    OpenAiFunction,
}

/// Runtime context shared by tool executors.
#[derive(Clone, Copy, Debug)]
pub struct ToolContext<'a> {
    /// Workspace root used for containment and relative paths.
    pub root: &'a Path,
}

impl<'a> ToolContext<'a> {
    /// Create a tool context for a workspace root.
    pub fn new(root: &'a Path) -> Self {
        Self { root }
    }
}

/// Unified output and side-effect audits from a tool execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolExecution {
    /// Display/model output produced by the tool.
    pub output: ToolOutput,
    /// Optional file-write audit metadata for session persistence.
    pub write_result: Option<WriteResult>,
    /// Optional shell process audit metadata for session persistence.
    pub shell_result: Option<shell::ProcessResult>,
}

impl ToolExecution {
    /// Create a tool execution with only display/model output.
    pub fn output(output: ToolOutput) -> Self {
        Self { output, write_result: None, shell_result: None }
    }

    /// Create a tool execution with all structured side effects.
    pub fn full(
        output: ToolOutput, write_result: Option<WriteResult>, shell_result: Option<shell::ProcessResult>,
    ) -> Self {
        Self { output, write_result, shell_result }
    }
}

/// Registry-level errors for lookup, validation, and argument parsing.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ToolError {
    /// The requested tool is not registered.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// A registry invariant failed.
    #[error("invalid tool registry: {0}")]
    InvalidRegistry(String),
    /// The provider supplied malformed or invalid arguments.
    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),
}

/// One executable registry entry.
#[derive(Clone, Copy)]
pub struct ToolEntry {
    /// Stable provider/model-visible tool name.
    pub name: &'static str,
    /// Provider-visible tool definition.
    pub definition: fn() -> ToolDefinition,
    /// Execute the tool and return output plus structured side effects.
    pub execute: fn(&ToolUseRequest, ToolContext<'_>) -> ToolExecution,
    /// Stable valid JSON input used by registry tests and future docs.
    pub example_input: &'static str,
}

/// Return the static built-in tool registry.
pub fn builtins() -> &'static [ToolEntry] {
    static ENTRIES: std::sync::OnceLock<Vec<ToolEntry>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| TOOL_NAMES.iter().map(|name| entry(name)).collect())
}

/// Look up a built-in tool by stable name.
pub fn get(name: &str) -> Option<&'static ToolEntry> {
    builtins().iter().find(|entry| entry.name == name)
}

/// Validate registry invariants that must hold before provider catalog use.
pub fn validate() -> Result<(), ToolError> {
    let mut names = HashSet::new();
    for entry in builtins() {
        if !names.insert(entry.name) {
            return Err(ToolError::InvalidRegistry(format!(
                "duplicate tool name: {}",
                entry.name
            )));
        }

        let definition = (entry.definition)();
        if definition.name != entry.name {
            return Err(ToolError::InvalidRegistry(format!(
                "entry `{}` returned definition `{}`",
                entry.name, definition.name
            )));
        }

        if !definition.input_schema.is_object() {
            return Err(ToolError::InvalidRegistry(format!(
                "entry `{}` schema is not a JSON object",
                entry.name
            )));
        }

        let example = serde_json::from_str::<serde_json::Value>(entry.example_input).map_err(|err| {
            ToolError::InvalidArguments(format!("{} example input is invalid JSON: {err}", entry.name))
        })?;
        if !example.is_object() {
            return Err(ToolError::InvalidArguments(format!(
                "{} example input is not a JSON object",
                entry.name
            )));
        }
    }

    Ok(())
}

/// Return the provider-visible tool catalog from the registry.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    validate().expect("built-in tool registry should be valid");
    builtins().iter().map(|entry| (entry.definition)()).collect()
}

/// Execute a registered tool, or return a stable failed output for unknown names.
pub fn execute(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    match get(&request.name) {
        Some(entry) => (entry.execute)(request, ctx),
        None => ToolExecution::output(ToolOutput::failed(
            &request.name,
            ToolError::UnknownTool(request.name.clone()).to_string(),
        )),
    }
}

/// Convert the tool catalog into a provider-specific JSON schema array.
pub fn provider_tool_catalog_schemas(defs: &[ToolDefinition], format: ProviderSchemaFormat) -> serde_json::Value {
    match format {
        ProviderSchemaFormat::Anthropic => serde_json::Value::Array(
            defs.iter()
                .map(|tool| {
                    serde_json::json!({
                        "name": tool.name,
                        "description": tool.description,
                        "input_schema": tool.input_schema,
                    })
                })
                .collect(),
        ),
        ProviderSchemaFormat::OpenAiFunction => serde_json::Value::Array(
            defs.iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.input_schema,
                        }
                    })
                })
                .collect(),
        ),
    }
}

fn entry(name: &'static str) -> ToolEntry {
    ToolEntry {
        name,
        definition: definition_for_name(name),
        execute: execute_for_name(name),
        example_input: example_for_name(name),
    }
}

fn definition_for_name(name: &'static str) -> fn() -> ToolDefinition {
    match name {
        "find_files" => super::find_files::definition,
        "list_searchable_files" => super::list_searchable_files::definition,
        "search_text" => super::search_text::definition,
        "read_file_range" => super::read_file_range::definition,
        "sawk" => super::sawk::definition,
        "web_search" => super::web_search::definition,
        "read_url" => super::read_url::definition,
        "create_file" => super::create_file::definition,
        "replace_range" => super::replace_range::definition,
        "write_patch" => super::write_patch::definition,
        "run_shell" => || super::legacy_tool_definition("run_shell").expect("registered definition"),
        other => panic!("missing registry definition for {other}"),
    }
}

fn execute_for_name(name: &'static str) -> fn(&ToolUseRequest, ToolContext<'_>) -> ToolExecution {
    match name {
        "find_files" => super::find_files::execute_request,
        "list_searchable_files" => super::list_searchable_files::execute_request,
        "search_text" => super::search_text::execute_request,
        "read_file_range" => super::read_file_range::execute_request,
        "sawk" => super::sawk::execute_request,
        "web_search" => super::web_search::execute_request,
        "read_url" => super::read_url::execute_request,
        "create_file" => super::create_file::execute_request,
        "replace_range" => super::replace_range::execute_request,
        "write_patch" => super::write_patch::execute_request,
        "run_shell" => execute_legacy,
        other => panic!("missing registry executor for {other}"),
    }
}

fn example_for_name(name: &str) -> &'static str {
    TOOL_EXAMPLES
        .iter()
        .find_map(|(example_name, input)| (*example_name == name).then_some(*input))
        .expect("registered tool should have an example input")
}

fn execute_legacy(request: &ToolUseRequest, ctx: ToolContext<'_>) -> ToolExecution {
    let (output, write_result, shell_result) = super::dispatch_full_legacy(request, ctx.root);
    ToolExecution::full(output, write_result, shell_result)
}
