use serde_json::Value;
use thunderus_core::Result;
use thunderus_providers::{ToolParameter, ToolResult, ToolSpec};

use super::classification::ToolRisk;

/// The core trait that all tools must implement
pub trait Tool: Send + Sync + std::fmt::Debug {
    /// Returns the unique name of this tool
    fn name(&self) -> &str;

    /// Returns a description of what this tool does
    fn description(&self) -> &str;

    /// Returns the parameter schema for this tool
    fn parameters(&self) -> ToolParameter;

    /// Returns the risk level of this tool (for approval gating)
    fn risk_level(&self) -> ToolRisk;

    /// Executes the tool with the given arguments
    ///
    /// Returns a [ToolResult] containing the tool call ID and output or error
    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult>;

    /// Returns the full [ToolSpec] for this tool (for provider communication)
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(self.name(), self.description(), self.parameters())
    }
}
