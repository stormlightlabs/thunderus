use thunderus_providers::ToolCall;

/// Helper function to create a noop tool call for testing
pub fn noop_tool_call(id: &str) -> ToolCall {
    ToolCall::new(id, "noop", serde_json::json!({}))
}

/// Helper function to create an echo tool call for testing
pub fn echo_tool_call(id: &str, message: &str) -> ToolCall {
    ToolCall::new(id, "echo", serde_json::json!({"message": message}))
}

/// Helper function to create a shell tool call for testing
pub fn shell_tool_call(id: &str, command: &str) -> ToolCall {
    ToolCall::new(id, "shell", serde_json::json!({"command": command}))
}

/// Helper function to create a grep tool call for testing
pub fn grep_tool_call(id: &str, pattern: &str, arguments: serde_json::Value) -> ToolCall {
    let mut full_args = serde_json::json!({"pattern": pattern});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    ToolCall::new(id, "grep", full_args)
}

/// Helper function to create a glob tool call for testing
pub fn glob_tool_call(id: &str, pattern: &str, arguments: serde_json::Value) -> ToolCall {
    let mut full_args = serde_json::json!({"pattern": pattern});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    ToolCall::new(id, "glob", full_args)
}

/// Helper function to create a read tool call for testing
pub fn read_tool_call(id: &str, file_path: &str, arguments: serde_json::Value) -> ToolCall {
    let mut full_args = serde_json::json!({"file_path": file_path});
    if let serde_json::Value::Object(obj) = arguments
        && let serde_json::Value::Object(ref mut base) = full_args
    {
        for (key, value) in obj {
            base.insert(key, value);
        }
    }
    ToolCall::new(id, "read", full_args)
}
