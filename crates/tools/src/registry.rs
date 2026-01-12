use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thunderus_core::Result;
use thunderus_providers::{ToolResult, ToolSpec};

use super::Tool;

/// Registry that holds all available tools
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
}

impl ToolRegistry {
    /// Creates a new empty tool registry
    pub fn new() -> Self {
        Self { tools: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Registers a new tool in the registry
    ///
    /// Returns error if a tool with the same name already exists
    pub fn register<T: Tool + 'static>(&self, tool: T) -> Result<()> {
        let name = tool.name().to_string();
        let mut tools = self.tools.write().unwrap();

        if tools.contains_key(&name) {
            return Err(thunderus_core::Error::Validation(format!(
                "Tool '{}' already registered",
                name
            )));
        }

        tools.insert(name.clone(), Box::new(tool));
        Ok(())
    }

    /// Gets a tool by name
    pub fn get(&self, _name: &str) -> Option<Arc<dyn Tool>> {
        let _tools = self.tools.read().unwrap();
        None
    }

    /// Checks if a tool exists
    pub fn has(&self, name: &str) -> bool {
        let tools = self.tools.read().unwrap();
        tools.contains_key(name)
    }

    /// Returns names of all registered tools
    pub fn list(&self) -> Vec<String> {
        let tools = self.tools.read().unwrap();
        tools.keys().cloned().collect()
    }

    /// Returns all tool specs (for sending to providers)
    pub fn specs(&self) -> Vec<ToolSpec> {
        let tools = self.tools.read().unwrap();
        tools.values().map(|tool| tool.spec()).collect()
    }

    /// Returns the number of registered tools
    pub fn count(&self) -> usize {
        let tools = self.tools.read().unwrap();
        tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to execute a tool from the registry directly
impl ToolRegistry {
    /// Executes a tool by name with given arguments
    ///
    /// This is a convenience method that combines getting a tool and executing it
    pub fn execute(&self, tool_name: &str, tool_call_id: String, arguments: &serde_json::Value) -> Result<ToolResult> {
        let tools = self.tools.read().unwrap();

        match tools.get(tool_name) {
            Some(tool) => tool.execute(tool_call_id, arguments),
            None => Err(thunderus_core::Error::Tool(format!(
                "Tool '{}' not found in registry",
                tool_name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::NoopTool;

    #[test]
    fn test_new_registry() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.count(), 0);
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_register_tool() {
        let registry = ToolRegistry::new();
        let noop = NoopTool;

        let result = registry.register(noop);
        assert!(result.is_ok());
        assert_eq!(registry.count(), 1);
        assert!(registry.has("noop"));
    }

    #[test]
    fn test_duplicate_tool() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let result = registry.register(NoopTool);
        assert!(result.is_err());
        assert!(matches!(result, Err(thunderus_core::Error::Validation(_))));
    }

    #[test]
    fn test_list_tools() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 1);
        assert!(tools.contains(&"noop".to_string()));
    }

    #[test]
    fn test_get_specs() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let specs = registry.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name(), "noop");
    }

    #[test]
    fn test_execute_tool() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let args = serde_json::json!({});
        let result = registry.execute("noop", "call_123".to_string(), &args);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_123");
        assert!(tool_result.is_success());
    }

    #[test]
    fn test_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();

        let args = serde_json::json!({});
        let result = registry.execute("nonexistent", "call_123".to_string(), &args);
        assert!(result.is_err());
        assert!(matches!(result, Err(thunderus_core::Error::Tool(_))));
    }
}
