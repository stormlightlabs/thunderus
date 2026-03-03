//! Tool execution and schema definitions for Thunderus
//!
//! This crate provides:
//! - Tool definitions and JSON schemas for LLM consumption
//! - Tool execution runtime with sandboxing
//! - Tool result envelopes

pub mod runtime;
pub mod schema;

pub use runtime::{ToolExecutor, ToolResult, ToolStatus, generate_diff};
pub use schema::{Tool, ToolParameter, ToolSchema};

use runtime::{
    execute_bash, execute_edit, execute_memory_recall, execute_memory_store, execute_read, execute_research,
    execute_write,
};
use std::collections::HashMap;

pub use runtime::init_memory_store;

/// Execute a tool call and return the result
pub async fn execute_tool(
    tool_name: &str, arguments: &HashMap<String, serde_json::Value>, sandbox_path: &std::path::Path,
) -> ToolResult {
    match tool_name {
        "read" => execute_read(arguments, sandbox_path).await,
        "write" => execute_write(arguments, sandbox_path).await,
        "edit" => execute_edit(arguments, sandbox_path).await,
        "bash" => execute_bash(arguments, sandbox_path).await,
        "research" => execute_research(arguments).await,
        "memory_store" => execute_memory_store(arguments).await,
        "memory_recall" => execute_memory_recall(arguments).await,
        _ => ToolResult::error(format!("Unknown tool: {tool_name}")),
    }
}

/// Get all available tool schemas
pub fn get_tool_schemas() -> Vec<Tool> {
    vec![
        Tool::read(),
        Tool::write(),
        Tool::edit(),
        Tool::bash(),
        Tool::research(),
        Tool::memory_store(),
        Tool::memory_recall(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_tool_schemas() {
        let schemas = get_tool_schemas();
        assert_eq!(schemas.len(), 7);

        let names: Vec<_> = schemas.iter().map(|t| t.name.clone()).collect();
        assert!(names.contains(&"read".to_string()));
        assert!(names.contains(&"write".to_string()));
        assert!(names.contains(&"edit".to_string()));
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"research".to_string()));
        assert!(names.contains(&"memory_store".to_string()));
        assert!(names.contains(&"memory_recall".to_string()));
    }
}
