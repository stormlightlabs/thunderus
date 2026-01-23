use serde_json::Value;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;

/// Tool for direct file writing (escape hatch, heavily gated)
///
/// WARNING: This is an ESCAPE HATCH tool that bypasses the patch system.
/// It should ONLY be used in exceptional circumstances where patch-based
/// editing is not feasible (e.g., binary files, generated files, etc.).
///
/// Direct writes are NOT reviewable through the patch queue and CANNOT be
/// undone through the patch rollback mechanism. Use ONLY when absolutely necessary.
#[derive(Debug)]
pub struct WriteTool;

impl WriteTool {
    /// Validates that the path is valid for writing
    fn validate_path(path: &Path) -> Result<()> {
        if path.exists() && path.is_dir() {
            return Err(thunderus_core::Error::Validation(format!(
                "Path is a directory, not a file: {}",
                path.display()
            )));
        }

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            return Err(thunderus_core::Error::Validation(format!(
                "Parent directory does not exist: {}",
                parent.display()
            )));
        }

        Ok(())
    }

    /// Writes content directly to the file
    fn write_file(path: &Path, content: &str) -> Result<String> {
        std::fs::write(path, content)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to write file '{}': {}", path.display(), e)))?;

        Ok(format!(
            "Successfully wrote file: {}\n\n⚠️  WARNING: Direct file writes bypass the patch system and cannot be rolled back through the patch queue.",
            path.display()
        ))
    }
}

impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "[ESCAPE HATCH] Write content directly to a file, bypassing the patch system. \
         WARNING: Not reviewable, not reversible, not conflict-aware. ONLY use when patch-based editing is not feasible."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "file_path".to_string(),
                ToolParameter::new_string("Absolute path to the file")
                    .with_description("The absolute path to the file to write"),
            ),
            (
                "content".to_string(),
                ToolParameter::new_string("File content").with_description("The complete content to write to the file"),
            ),
            (
                "justification".to_string(),
                ToolParameter::new_string("Reason for bypassing patch system")
                    .with_description("Explanation of why patch-based editing cannot be used for this operation"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Risky
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Risky,
            "WriteTool bypasses the patch system and writes directly to files. Changes are NOT reviewable through the patch queue and CANNOT be undone via patch rollback. This tool should ONLY be used when patch-based editing is not feasible (e.g., binary files, auto-generated files, etc.).",
        ).with_suggestion("Use PatchTool instead for all regular file edits. Patches are reviewable, reversible, and conflict-aware."))
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

        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| thunderus_core::Error::Validation("Missing or invalid 'content' parameter".to_string()))?;

        let _justification = arguments.get("justification").and_then(|v| v.as_str());

        let path = PathBuf::from(file_path_str);

        Self::validate_path(&path)?;

        let result = Self::write_file(&path, content)?;

        Ok(ToolResult::success(tool_call_id, result))
    }
}
