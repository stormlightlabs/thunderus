//! Session-aware tool dispatcher that integrates tool execution with session state
//!
//! This module provides a dispatcher wrapper that automatically logs tool
//! events to the session and maintains read history for edit validation.

use thunderus_core::Result;
use thunderus_core::{BlockedCommandError, Session};
use thunderus_providers::ToolCall;
use thunderus_providers::ToolResult;

use crate::read_history::{self, ReadHistory};
use crate::{ToolDispatcher, classify_shell_command};

/// Session-aware tool dispatcher
///
/// Wraps a ToolDispatcher and integrates it with session state tracking.
/// Automatically logs tool events to the session and maintains read history.
#[derive(Debug)]
pub struct SessionToolDispatcher {
    /// The underlying tool dispatcher
    dispatcher: ToolDispatcher,
    /// Session for logging events
    session: Session,
    /// Read history for tracking file reads
    read_history: ReadHistory,
}

impl SessionToolDispatcher {
    /// Creates a new session-aware dispatcher
    pub fn new(dispatcher: ToolDispatcher, session: Session, read_history: ReadHistory) -> Self {
        Self { dispatcher, session, read_history }
    }

    /// Creates a new session-aware dispatcher with a new read history
    ///
    /// Convenience method that creates an empty ReadHistory.
    pub fn with_new_history(dispatcher: ToolDispatcher, session: Session) -> Self {
        Self::new(dispatcher, session, ReadHistory::new())
    }

    /// Executes a tool call and logs to session
    pub fn execute(&mut self, tool_call: &ToolCall) -> Result<ToolResult> {
        let tool_name = tool_call.name();
        let arguments = tool_call.arguments();

        if tool_name == "shell"
            && let Some(command) = arguments.get("command").and_then(|v| v.as_str())
        {
            let classification = classify_shell_command(command);
            if classification.risk.is_blocked() {
                let blocked_error = Self::create_blocked_error(command, &classification.reasoning);
                let _ = self.session.append_tool_call(tool_name, arguments.clone());
                let _ = self.session.append_tool_result(
                    tool_name,
                    serde_json::json!(null),
                    false,
                    Some(blocked_error.to_string()),
                );
                return Err(blocked_error.into());
            }
        }

        let _ = self.session.append_tool_call(tool_name, arguments.clone());

        let result = self.dispatcher.execute(tool_call);

        match &result {
            Ok(tool_result) => {
                let _ = self.session.append_tool_result(
                    tool_name,
                    serde_json::json!({ "content": tool_result.content }),
                    true,
                    None,
                );

                if tool_name == "read" {
                    self.track_file_read(tool_result, arguments);
                }
            }
            Err(e) => {
                let _ = self
                    .session
                    .append_tool_result(tool_name, serde_json::json!(null), false, Some(e.to_string()));
            }
        }

        result
    }

    /// Creates a blocked command error based on the command and reasoning
    fn create_blocked_error(command: &str, reasoning: &str) -> BlockedCommandError {
        let command_lower = command.to_lowercase();
        let first_word = command_lower.split_whitespace().next().unwrap_or("");

        match first_word {
            "sudo" => BlockedCommandError::sudo(command),
            "dd" => BlockedCommandError::data_destruction(command),
            "mkfs" => BlockedCommandError::data_destruction(command),
            "format" => BlockedCommandError::data_destruction(command),
            "fdisk" => BlockedCommandError::data_destruction(command),
            _ => BlockedCommandError::generic(command, reasoning),
        }
    }

    /// Executes multiple tool calls in order
    pub fn execute_batch(&mut self, tool_calls: &[ToolCall]) -> Result<Vec<ToolResult>> {
        let mut results = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            let result = self.execute(tool_call)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Tracks a file read in the read history
    ///
    /// Parses Read tool results and arguments to extract file path,
    /// line count, and offset for tracking.
    fn track_file_read(&mut self, tool_result: &ToolResult, arguments: &serde_json::Value) {
        let file_path = match arguments.get("file_path").and_then(|v| v.as_str()) {
            Some(path) => path,
            None => return,
        };

        if tool_result.is_success() {
            let line_count = tool_result.content.lines().count();
            let offset = arguments.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            self.read_history.record_read(file_path, line_count, offset);
            let _ = self.session.append_file_read(file_path, line_count, offset, true);
        } else {
            self.read_history.record_failed_read(file_path);
            let _ = self.session.append_file_read(file_path, 0, 0, false);
        }
    }

    /// Gets a reference to the underlying dispatcher
    pub fn dispatcher(&self) -> &ToolDispatcher {
        &self.dispatcher
    }

    /// Gets a mutable reference to the underlying dispatcher
    pub fn dispatcher_mut(&mut self) -> &mut ToolDispatcher {
        &mut self.dispatcher
    }

    /// Gets a reference to the session
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Gets a mutable reference to the session
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Gets a reference to the read history
    pub fn read_history(&self) -> &ReadHistory {
        &self.read_history
    }

    /// Consumes self and returns the inner components
    pub fn into_inner(self) -> (ToolDispatcher, Session, ReadHistory) {
        (self.dispatcher, self.session, self.read_history)
    }
}

/// Validates that a file has been read before allowing edits
pub fn validate_read_before_edit(dispatcher: &SessionToolDispatcher, file_path: &str) -> Result<()> {
    read_history::validate_read_before_edit(dispatcher.read_history(), file_path)
        .map_err(|e| thunderus_core::Error::Tool(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::{self, ReadTool};
    use crate::registry::ToolRegistry;
    use tempfile::TempDir;

    fn create_test_dispatcher() -> (TempDir, SessionToolDispatcher) {
        let temp = TempDir::new().unwrap();
        let agent_dir = thunderus_core::AgentDir::new(temp.path());
        let session = Session::new(agent_dir).unwrap();

        let registry = ToolRegistry::new();
        registry.register(ReadTool).unwrap();
        registry.register(builtin::ShellTool).unwrap();
        let dispatcher = ToolDispatcher::new(registry);

        let session_dispatcher = SessionToolDispatcher::with_new_history(dispatcher, session);

        (temp, session_dispatcher)
    }

    #[test]
    fn test_session_dispatcher_creation() {
        let (_temp, dispatcher) = create_test_dispatcher();
        assert!(dispatcher.read_history().is_empty());
        assert!(dispatcher.session().exists());
    }

    #[test]
    fn test_execute_and_log() {
        let (temp, mut dispatcher) = create_test_dispatcher();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "Line 1\nLine 2\nLine 3").unwrap();

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "read",
            serde_json::json!({"file_path": test_file.to_string_lossy().as_ref()}),
        );

        let result = dispatcher.execute(&tool_call);
        assert!(result.is_ok());

        assert!(!dispatcher.read_history().is_empty());
        assert_eq!(
            dispatcher.read_history().was_read(test_file.to_str().unwrap()),
            Some((3, 0))
        );

        let events = dispatcher.session().read_events().unwrap();
        assert!(events.len() >= 2);

        let has_file_read = events
            .iter()
            .any(|e| matches!(e.event, thunderus_core::Event::FileRead { .. }));
        assert!(has_file_read);
    }

    #[test]
    fn test_validate_read_before_edit() {
        let (temp, mut dispatcher) = create_test_dispatcher();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();

        if validate_read_before_edit(&dispatcher, test_file.to_str().unwrap()).is_ok() {
            panic!("Command should have been blocked")
        }

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "read",
            serde_json::json!({"file_path": test_file.to_string_lossy().as_ref()}),
        );
        dispatcher.execute(&tool_call).unwrap();

        match validate_read_before_edit(&dispatcher, test_file.to_str().unwrap()) {
            Ok(_) => (),
            Err(_) => panic!("Command should have been allowed"),
        }
    }

    #[test]
    fn test_execute_batch() {
        let (temp, mut dispatcher) = create_test_dispatcher();

        let test_file = temp.path().join("test.txt");
        std::fs::write(&test_file, "Line 1\nLine 2").unwrap();

        let tool_calls = vec![thunderus_providers::ToolCall::new(
            "call_1",
            "read",
            serde_json::json!({"file_path": test_file.to_string_lossy().as_ref()}),
        )];

        match dispatcher.execute_batch(&tool_calls) {
            Ok(res) => assert!(res.len() == 1),
            Err(_) => panic!("Command should have been allowed"),
        }
    }

    #[test]
    fn test_blocked_shell_command_rejected() {
        let (_, mut dispatcher) = create_test_dispatcher();

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "shell",
            serde_json::json!({"command": "sudo apt-get install vim"}),
        );

        match dispatcher.execute(&tool_call) {
            Err(e) => {
                let error_str = e.to_string();
                assert!(error_str.contains("blocked") || error_str.contains("superuser"));
            }
            Ok(_) => panic!("Command should have been blocked"),
        }
    }

    #[test]
    fn test_blocked_dd_command_rejected() {
        let (_, mut dispatcher) = create_test_dispatcher();

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "shell",
            serde_json::json!({"command": "dd if=/dev/zero of=/dev/sda"}),
        );

        match dispatcher.execute(&tool_call) {
            Err(e) => {
                let error_str = e.to_string();
                assert!(error_str.contains("blocked") || error_str.contains("destroy data"));
            }
            Ok(_) => panic!("Command should have been blocked"),
        }
    }

    #[test]
    fn test_safe_shell_command_allowed() {
        let (_, mut dispatcher) = create_test_dispatcher();

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "shell",
            serde_json::json!({"command": "echo 'Hello, World!'"}),
        );

        match dispatcher.execute(&tool_call) {
            Ok(res) => assert!(res.content.contains("Hello, World!")),
            Err(_) => panic!("Command should have been allowed"),
        }
    }

    #[test]
    fn test_risky_shell_command_allowed() {
        let (_, mut dispatcher) = create_test_dispatcher();

        let tool_call = thunderus_providers::ToolCall::new(
            "call_1",
            "shell",
            serde_json::json!({"command": "rm -f /tmp/test_file.txt"}),
        );

        let error_str = match dispatcher.execute(&tool_call) {
            Err(ref e) => e.to_string(),
            Ok(_) => String::new(),
        };
        assert!(!error_str.contains("blocked") && !error_str.contains("superuser"));
    }
}
