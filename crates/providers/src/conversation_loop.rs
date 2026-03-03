//! Conversation loop with tool execution support
//!
//! Implements the multi-turn tool calling flow where the model can:
//! 1. Receive a user message
//! 2. Call tools as needed
//! 3. Receive tool results
//! 4. Continue to final response

use crate::{CompletionResponse, Provider, ToolResult};
use std::path::Path;
use thunderus_core::{Conversation, Message};
use thunderus_tools::execute_tool;

/// Event emitted during conversation loop
#[derive(Debug, Clone)]
pub enum ConversationEvent {
    /// Assistant is thinking
    Thinking(String),
    /// Tool is being called
    ToolCalling { name: String, arguments: String },
    /// Tool execution completed
    ToolCompleted { name: String, result: String },
    /// Assistant produced final content
    Content(String),
    /// Error occurred
    Error(String),
}

/// Handles multi-turn conversation with tool execution
pub struct ConversationLoop {
    conversation: Conversation,
    sandbox_path: std::path::PathBuf,
    max_iterations: usize,
}

impl ConversationLoop {
    /// Create a new conversation loop
    pub fn new(sandbox_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            conversation: Conversation::with_default_system_prompt(),
            sandbox_path: sandbox_path.into(),
            max_iterations: 10,
        }
    }

    /// Create with custom system prompt
    pub fn with_system_prompt(sandbox_path: impl Into<std::path::PathBuf>, prompt: impl Into<String>) -> Self {
        Self {
            conversation: Conversation::with_system_prompt(prompt),
            sandbox_path: sandbox_path.into(),
            max_iterations: 10,
        }
    }

    /// Set maximum tool call iterations
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Get the conversation
    pub fn conversation(&self) -> &Conversation {
        &self.conversation
    }

    /// Process a user message with tool support
    ///
    /// This implements the multi-turn loop where the model may call tools
    /// multiple times before producing a final response.
    pub async fn process_message<P, F>(
        &mut self, provider: &P, user_message: &str, mut event_handler: F,
    ) -> Result<String, String>
    where
        P: Provider,
        F: FnMut(ConversationEvent),
    {
        self.conversation.add_user_message(user_message);

        let mut iterations = 0;
        loop {
            if iterations >= self.max_iterations {
                return Err(format!(
                    "Maximum tool call iterations ({}) reached",
                    self.max_iterations
                ));
            }
            iterations += 1;

            let tools = thunderus_tools::get_tool_schemas();
            let tool_definitions: Vec<crate::ToolDefinition> = tools
                .iter()
                .map(|t| t.to_openai_function())
                .map(|v| serde_json::from_value(v.get("function").unwrap().clone()).unwrap())
                .collect();

            let response = provider
                .complete_with_tools(&self.conversation.messages, &tool_definitions)
                .await
                .map_err(|e| format!("Provider error: {}", e))?;

            if response.has_tool_calls() {
                let tool_results = self.process_tool_calls(&response, &mut event_handler).await?;

                let assistant_message = if response.content.is_empty() {
                    Message::assistant(String::new())
                } else {
                    Message::assistant(&response.content)
                };
                self.conversation.add_message(assistant_message);

                for result in tool_results {
                    self.conversation
                        .add_message(Message::tool(&result.tool_call_id, &result.content));
                }
            } else {
                let content = response.content.clone();

                self.conversation.add_assistant_message(&content);

                event_handler(ConversationEvent::Content(content.clone()));

                return Ok(content);
            }
        }
    }

    /// Process tool calls from a response
    async fn process_tool_calls<F>(
        &self, response: &CompletionResponse, event_handler: &mut F,
    ) -> Result<Vec<ToolResult>, String>
    where
        F: FnMut(ConversationEvent),
    {
        let mut results = Vec::new();

        for tool_call in &response.tool_calls {
            let args_json = serde_json::to_string(&tool_call.arguments).unwrap_or_default();
            event_handler(ConversationEvent::ToolCalling {
                name: tool_call.name.clone(),
                arguments: args_json.clone(),
            });

            let result = execute_tool(&tool_call.name, &tool_call.arguments, &self.sandbox_path).await;

            let tool_result = match result.status {
                thunderus_tools::ToolStatus::Success => ToolResult::success(&tool_call.id, result.content.clone()),
                _ => ToolResult::error(&tool_call.id, result.content.clone()),
            };

            event_handler(ConversationEvent::ToolCompleted {
                name: tool_call.name.clone(),
                result: result.content.clone(),
            });

            results.push(tool_result);
        }

        Ok(results)
    }
}

/// Run a conversation loop with streaming support
///
/// This is a higher-level convenience function for the most common use case.
pub async fn run_conversation_loop<P, F>(
    provider: &P, sandbox_path: &Path, user_message: &str, event_handler: F,
) -> Result<String, String>
where
    P: Provider,
    F: FnMut(ConversationEvent),
{
    let mut loop_handler = ConversationLoop::new(sandbox_path);
    loop_handler
        .process_message(provider, user_message, event_handler)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_loop_new() {
        let loop_handler = ConversationLoop::new("/test/path");
        assert_eq!(loop_handler.max_iterations, 10);
        assert_eq!(loop_handler.conversation().len(), 1);
    }

    #[test]
    fn test_conversation_loop_max_iterations() {
        let loop_handler = ConversationLoop::new("/test/path").max_iterations(5);
        assert_eq!(loop_handler.max_iterations, 5);
    }

    #[test]
    fn test_conversation_event_variants() {
        let events = vec![
            ConversationEvent::Thinking("Thinking...".to_string()),
            ConversationEvent::ToolCalling {
                name: "read".to_string(),
                arguments: r#"{"path": "/test.txt"}"#.to_string(),
            },
            ConversationEvent::ToolCompleted { name: "read".to_string(), result: "file contents".to_string() },
            ConversationEvent::Content("Final response".to_string()),
            ConversationEvent::Error("Something went wrong".to_string()),
        ];

        for event in events {
            match event {
                ConversationEvent::Thinking(_) => {}
                ConversationEvent::ToolCalling { .. } => {}
                ConversationEvent::ToolCompleted { .. } => {}
                ConversationEvent::Content(_) => {}
                ConversationEvent::Error(_) => {}
            }
        }
    }
}
