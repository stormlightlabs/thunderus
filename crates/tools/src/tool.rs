use serde_json::Value;
use thunderus_core::{Classification, Result, ToolRisk};
use thunderus_providers::{ToolParameter, ToolResult, ToolSpec};

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

    /// Returns true if this tool is read-only (never requires approval)
    ///
    /// Read-only tools like Grep, Glob, and Read bypass approval checks
    /// entirely since they cannot modify files or system state.
    fn is_read_only(&self) -> bool {
        false
    }

    /// Returns classification with reasoning for this tool
    ///
    /// This provides pedagogical value by explaining why a tool is classified
    /// as safe or risky, helping users understand the safety model.
    fn classification(&self) -> Option<Classification> {
        None
    }

    /// Returns dynamic classification based on execution arguments
    ///
    /// This allows tools to provide context-sensitive classification based on
    /// the actual operation being performed (e.g., ShellTool can classify
    /// specific commands as safe or risky).
    ///
    /// Default implementation falls back to static classification.
    fn classify_execution(&self, _: &Value) -> Option<Classification> {
        self.classification()
    }

    /// Executes the tool with the given arguments
    ///
    /// Returns a [ToolResult] containing the tool call ID and output or error
    fn execute(&self, tool_call_id: String, arguments: &Value) -> Result<ToolResult>;

    /// Returns the full [ToolSpec] for this tool (for provider communication)
    fn spec(&self) -> ToolSpec {
        ToolSpec::new(self.name(), self.description(), self.parameters())
    }
}
