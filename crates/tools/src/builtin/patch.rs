use serde_json::Value;
use std::path::{Path, PathBuf};
use thunderus_core::{Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult};

use crate::Tool;
use crate::patch_generator;

/// Tool for generating unified diff patches (patch-first editing approach)
///
/// This is the RECOMMENDED and DEFAULT tool for models to use when editing files.
/// It generates a patch using the Histogram diff algorithm, which produces more
/// semantically meaningful diffs than naive line-by-line comparison.
///
/// Patches are reviewable, reversible, and conflict-aware. They go through the
/// patch queue system where users can approve/reject individual hunks before applying.
#[derive(Debug)]
pub struct PatchTool;

impl PatchTool {
    /// Validates that the path exists and is a file
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

    /// Generates a unified diff patch for the file edit
    fn generate_patch(path: &Path, new_content: &str, base_snapshot: &str) -> Result<String> {
        let old_content = std::fs::read_to_string(path)
            .map_err(|e| thunderus_core::Error::Tool(format!("Failed to read file '{}': {}", path.display(), e)))?;

        if old_content == new_content {
            return Err(thunderus_core::Error::Validation(
                "No changes detected - old and new content are identical".to_string(),
            ));
        }

        patch_generator::generate_unified_diff(path, &old_content, new_content, base_snapshot)
            .map_err(thunderus_core::Error::Tool)
    }
}

impl Tool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }

    fn description(&self) -> &str {
        "Generate a unified diff patch for file edits (patch-first, RECOMMENDED). \
         Patches are reviewable, reversible, and conflict-aware. Use this tool by default for all file edits."
    }

    fn parameters(&self) -> ToolParameter {
        ToolParameter::new_object(vec![
            (
                "file_path".to_string(),
                ToolParameter::new_string("Absolute path to the file")
                    .with_description("The absolute path to the file to patch"),
            ),
            (
                "new_content".to_string(),
                ToolParameter::new_string("New file content")
                    .with_description("The complete new content of the file after the edit"),
            ),
            (
                "base_snapshot".to_string(),
                ToolParameter::new_string("Git commit hash")
                    .with_description("The git commit hash of the base snapshot (defaults to 'HEAD')"),
            ),
        ])
    }

    fn risk_level(&self) -> ToolRisk {
        ToolRisk::Risky
    }

    fn classification(&self) -> Option<thunderus_core::Classification> {
        Some(thunderus_core::Classification::new(
            ToolRisk::Risky,
            "PatchTool generates reviewable diffs using the Histogram algorithm. Patches must be approved before being applied. This is the RECOMMENDED approach for file edits.",
        ).with_suggestion("Always prefer PatchTool over WriteTool for file edits. Patches are safer because they're reviewable and reversible."))
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

        let new_content = arguments.get("new_content").and_then(|v| v.as_str()).ok_or_else(|| {
            thunderus_core::Error::Validation("Missing or invalid 'new_content' parameter".to_string())
        })?;

        let base_snapshot = arguments
            .get("base_snapshot")
            .and_then(|v| v.as_str())
            .unwrap_or("HEAD");

        let path = PathBuf::from(file_path_str);

        Self::validate_path(&path)?;

        let patch = Self::generate_patch(&path, new_content, base_snapshot)?;

        Ok(ToolResult::success(
            tool_call_id,
            format!("Generated patch for file: {}\n\n{}", path.display(), patch),
        ))
    }
}
