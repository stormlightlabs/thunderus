//! Built-in tool registry contract.
//!
//! The registry is a thin typed boundary around the current tool
//! implementations. Each entry has one stable model-visible name, one schema,
//! one executor, and one valid example input. Built-in tools own their schemas,
//! parsing, execution, and side-effect metadata in their tool modules.
//!
//! Provider schemas are derived from [`ToolDefinition`] through
//! [`provider_tool_catalog_schemas`]. Structured side effects flow through
//! [`ToolExecution`], so the session layer consumes file-write and shell audits
//! without knowing which tool produced them.

use std::collections::HashSet;
use std::path::Path;

use thiserror::Error;

use super::{ToolDefinition, ToolOutput, ToolUseRequest, WriteResult, shell};
use crate::search::SearchConfig;

const BUILTIN_TOOLS: &[ToolEntry] = &[
    ToolEntry {
        name: "find_files",
        definition: super::find_files::definition,
        execute: super::find_files::execute_request,
        example_input: r#"{"pattern":"Cargo.toml"}"#,
    },
    ToolEntry {
        name: "list_searchable_files",
        definition: super::list_searchable_files::definition,
        execute: super::list_searchable_files::execute_request,
        example_input: r#"{"glob":"src/**/*.rs"}"#,
    },
    ToolEntry {
        name: "search_text",
        definition: super::search_text::definition,
        execute: super::search_text::execute_request,
        example_input: r#"{"pattern":"fn main","glob":"src/**/*.rs"}"#,
    },
    ToolEntry {
        name: "read_file_range",
        definition: super::read_file_range::definition,
        execute: super::read_file_range::execute_request,
        example_input: r#"{"path":"Cargo.toml","start_line":1,"end_line":3}"#,
    },
    ToolEntry {
        name: "sawk",
        definition: super::sawk::definition,
        execute: super::sawk::execute_request,
        example_input: r#"{"action":"sed_print","path":"Cargo.toml","start_line":1,"end_line":3}"#,
    },
    ToolEntry {
        name: "web_search",
        definition: super::web_search::definition,
        execute: super::web_search::execute_request,
        example_input: r#"{"query":"Rust serde documentation","max_results":3}"#,
    },
    ToolEntry {
        name: "read_url",
        definition: super::read_url::definition,
        execute: super::read_url::execute_request,
        example_input: r#"{"url":"https://example.com"}"#,
    },
    ToolEntry {
        name: "create_file",
        definition: super::create_file::definition,
        execute: super::create_file::execute_request,
        example_input: r#"{"path":"notes.txt","content":"hello\n"}"#,
    },
    ToolEntry {
        name: "replace_range",
        definition: super::replace_range::definition,
        execute: super::replace_range::execute_request,
        example_input: r#"{"path":"notes.txt","old_string":"hello","new_string":"hi"}"#,
    },
    ToolEntry {
        name: "write_patch",
        definition: super::write_patch::definition,
        execute: super::write_patch::execute_request,
        example_input: r#"{"patches":[{"op":"edit","path":"notes.txt","old_string":"hello","new_string":"hi"}]}"#,
    },
    ToolEntry {
        name: "run_shell",
        definition: super::shell::definition,
        execute: super::shell::execute_request,
        example_input: r#"{"argv":["cargo","test","tools"]}"#,
    },
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
#[derive(Clone, Debug)]
pub struct ToolContext<'a> {
    /// Workspace root used for containment and relative paths.
    pub root: &'a Path,
    /// Application-owned web-search configuration.
    pub search: SearchConfig,
}

impl<'a> ToolContext<'a> {
    /// Create a tool context for a workspace root.
    pub fn new(root: &'a Path) -> Self {
        Self { root, search: SearchConfig::default() }
    }

    /// Create a tool context with the active application-owned search backend.
    pub fn with_search(root: &'a Path, search: &SearchConfig) -> Self {
        Self { root, search: search.clone() }
    }
}

/// Evidence metadata, projections, and side-effect audits from a tool execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolExecution {
    /// Tool evidence metadata plus independent display/model projections.
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
    pub execute: fn(&ToolUseRequest, &ToolContext<'_>) -> ToolExecution,
    /// Stable valid JSON input used by registry tests and future docs.
    pub example_input: &'static str,
}

/// Return the static built-in tool registry.
pub fn builtins() -> &'static [ToolEntry] {
    BUILTIN_TOOLS
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
pub fn execute(request: &ToolUseRequest, ctx: &ToolContext<'_>) -> ToolExecution {
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
