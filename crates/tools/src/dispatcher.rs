use thunderus_core::Result;
use thunderus_providers::ToolCall;
use thunderus_providers::ToolResult;

use super::ToolRegistry;

/// Executes a tool call from a provider
///
/// The dispatcher is responsible for:
/// - Finding the tool in the registry
/// - Validating arguments
/// - Executing the tool
/// - Returning results in the format expected by the agent loop
pub struct ToolDispatcher {
    registry: ToolRegistry,
}

impl ToolDispatcher {
    /// Creates a new dispatcher with the given registry
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }

    /// Executes a single tool call
    ///
    /// Takes a [ToolCall] from the provider and executes it,
    /// returning a [ToolResult] to be sent back to the agent loop
    pub fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        let tool_name = tool_call.name();
        let arguments = tool_call.arguments();
        let tool_call_id = tool_call.id.clone();

        self.registry.execute(tool_name, tool_call_id, arguments)
    }

    /// Executes multiple tool calls in order
    ///
    /// Returns a vector of results, one for each tool call
    pub fn execute_batch(&self, tool_calls: &[ToolCall]) -> Result<Vec<ToolResult>> {
        let mut results = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            let result = self.execute(tool_call)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Gets a reference to the underlying registry
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Gets a mutable reference to the underlying registry
    pub fn registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }
}

impl From<ToolRegistry> for ToolDispatcher {
    fn from(registry: ToolRegistry) -> Self {
        Self::new(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::{self, EchoTool, NoopTool};
    use serde_json;

    fn setup_dispatcher() -> ToolDispatcher {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();
        registry.register(EchoTool).unwrap();
        ToolDispatcher::new(registry)
    }

    #[test]
    fn test_execute_noop() {
        let dispatcher = setup_dispatcher();
        let tool_call = builtin::noop_tool_call("call_1");

        let result = dispatcher.execute(&tool_call);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_1");
        assert!(tool_result.is_success());
    }

    #[test]
    fn test_execute_echo() {
        let dispatcher = setup_dispatcher();
        let tool_call = builtin::echo_tool_call("call_2", "Hello");

        let result = dispatcher.execute(&tool_call);
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert_eq!(tool_result.tool_call_id, "call_2");
        assert!(tool_result.is_success());
        assert_eq!(tool_result.content, "Hello");
    }

    #[test]
    fn test_execute_batch() {
        let dispatcher = setup_dispatcher();
        let tool_calls = vec![
            builtin::noop_tool_call("call_1"),
            builtin::echo_tool_call("call_2", "First"),
            builtin::echo_tool_call("call_3", "Second"),
        ];

        let results = dispatcher.execute_batch(&tool_calls);
        assert!(results.is_ok());

        let tool_results = results.unwrap();
        assert_eq!(tool_results.len(), 3);
        assert!(tool_results.iter().all(|r| r.is_success()));
        assert_eq!(tool_results[1].content, "First");
        assert_eq!(tool_results[2].content, "Second");
    }

    #[test]
    fn test_execute_nonexistent_tool() {
        let dispatcher = setup_dispatcher();
        let tool_call = thunderus_providers::ToolCall::new("call_123", "nonexistent_tool", serde_json::json!({}));
        let result = dispatcher.execute(&tool_call);
        assert!(result.is_err());
        assert!(matches!(result, Err(thunderus_core::Error::Tool(_))));
    }

    #[test]
    fn test_registry_access() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let dispatcher = ToolDispatcher::new(registry);
        assert_eq!(dispatcher.registry().count(), 1);
        assert!(dispatcher.registry().has("noop"));
    }

    #[test]
    fn test_from_registry() {
        let registry = ToolRegistry::new();
        registry.register(NoopTool).unwrap();

        let dispatcher: ToolDispatcher = registry.into();
        assert_eq!(dispatcher.registry().count(), 1);
    }
}
