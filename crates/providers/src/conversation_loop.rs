//! Conversation loop with tool execution support
//!
//! Implements the multi-turn tool calling flow where the model can:
//! 1. Receive a user message
//! 2. Call tools as needed
//! 3. Receive tool results
//! 4. Continue to final response

use super::{CompletionResponse, Provider, ToolDefinition, ToolResult, Usage};
use std::path::Path;
use thndrs_core::{Conversation, Message, Role, build_system_prompt};
use thndrs_mem::{MemoryDatabase, MemoryStore};
use thndrs_tools::execute_tool;

/// Event emitted during conversation loop
#[derive(Debug, Clone)]
pub enum ConversationEvent {
    /// Assistant is thinking
    Thinking(String),
    /// Tool is being called
    ToolCalling {
        id: String,
        name: String,
        arguments: String,
    },
    /// Tool execution completed
    ToolCompleted {
        id: String,
        name: String,
        result: String,
        is_error: bool,
    },
    /// Assistant produced final content
    Content {
        content: String,
        usage: Usage,
        model: String,
    },
    /// Error occurred
    Error(String),
}

/// Handles multi-turn conversation with tool execution
pub struct ConversationLoop {
    conversation: Conversation,
    base_system_prompt: String,
    memory_store: Option<MemoryStore>,
    sandbox_path: std::path::PathBuf,
    max_iterations: usize,
}

impl ConversationLoop {
    /// Create a new conversation loop
    pub fn new(sandbox_path: impl Into<std::path::PathBuf>) -> Self {
        let sandbox_path = sandbox_path.into();
        let base_system_prompt = build_system_prompt();
        let memory_store = MemoryDatabase::for_workspace(&sandbox_path).ok().map(MemoryStore::new);

        Self {
            conversation: Conversation::with_system_prompt(base_system_prompt.clone()),
            base_system_prompt,
            memory_store,
            sandbox_path,
            max_iterations: 10,
        }
    }

    /// Create with custom system prompt
    pub fn with_system_prompt(sandbox_path: impl Into<std::path::PathBuf>, prompt: impl Into<String>) -> Self {
        let sandbox_path = sandbox_path.into();
        let base_system_prompt = prompt.into();
        let memory_store = MemoryDatabase::for_workspace(&sandbox_path).ok().map(MemoryStore::new);

        Self {
            conversation: Conversation::with_system_prompt(base_system_prompt.clone()),
            base_system_prompt,
            memory_store,
            sandbox_path,
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

    /// Replace conversation history while preserving the dynamic system prompt slot.
    pub fn set_history(&mut self, history: &[Message]) {
        self.conversation = Conversation::with_system_prompt(self.base_system_prompt.clone());
        self.conversation
            .messages
            .extend(history.iter().filter(|message| message.role != Role::System).cloned());
    }

    /// Process a user message with tool support
    ///
    /// This implements the multi-turn loop where the model may call tools
    /// multiple times before producing a final response.
    pub async fn process_message<P, F>(
        &mut self, provider: &P, user_message: &str, mut event_handler: F,
    ) -> Result<String, String>
    where
        P: Provider + ?Sized,
        F: FnMut(ConversationEvent),
    {
        self.conversation.add_user_message(user_message);
        self.inject_recalled_memories(user_message).await;

        let mut iterations = 0;
        loop {
            if iterations >= self.max_iterations {
                return Err(format!(
                    "Maximum tool call iterations ({}) reached",
                    self.max_iterations
                ));
            }
            iterations += 1;

            let tools = thndrs_tools::get_tool_schemas();
            let tool_definitions: Vec<ToolDefinition> = tools
                .iter()
                .map(|tool| {
                    ToolDefinition::from_schema(
                        tool.name.clone(),
                        tool.description.clone(),
                        tool.parameters.to_json_schema(),
                    )
                })
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

                event_handler(ConversationEvent::Content {
                    content: content.clone(),
                    usage: response.usage.clone(),
                    model: response.model.clone(),
                });

                return Ok(content);
            }
        }
    }

    async fn inject_recalled_memories(&mut self, user_message: &str) {
        let Some(store) = self.memory_store.as_mut() else {
            self.update_system_prompt(None);
            return;
        };

        let memories = match store.recall(user_message, 3, None, None, 0.5).await {
            Ok(results) => results,
            Err(error) => {
                tracing::warn!("Implicit memory recall failed: {}", error);
                self.update_system_prompt(None);
                return;
            }
        };

        if memories.is_empty() {
            self.update_system_prompt(None);
            return;
        }

        let mut block = String::from("<memory>\n");
        for memory in memories {
            block.push_str(&memory.to_prompt_string());
            block.push('\n');
        }
        block.push_str("</memory>");

        self.update_system_prompt(Some(block));
    }

    fn update_system_prompt(&mut self, memory_block: Option<String>) {
        let mut prompt = self.base_system_prompt.clone();
        if let Some(block) = memory_block {
            prompt.push_str("\n\n");
            prompt.push_str(&block);
        }

        if let Some(first) = self.conversation.messages.first_mut()
            && first.role == Role::System
        {
            first.content = prompt;
            return;
        }

        self.conversation.messages.insert(0, Message::system(prompt));
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
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: args_json.clone(),
            });

            let result = execute_tool(&tool_call.name, &tool_call.arguments, &self.sandbox_path).await;

            let tool_result = match result.status {
                thndrs_tools::ToolStatus::Success => ToolResult::success(&tool_call.id, result.content.clone()),
                _ => ToolResult::error(&tool_call.id, result.content.clone()),
            };

            event_handler(ConversationEvent::ToolCompleted {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                result: result.content.clone(),
                is_error: !matches!(result.status, thndrs_tools::ToolStatus::Success),
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
    P: Provider + ?Sized,
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
                id: "tool_1".to_string(),
                name: "read".to_string(),
                arguments: r#"{"path": "/test.txt"}"#.to_string(),
            },
            ConversationEvent::ToolCompleted {
                id: "tool_1".to_string(),
                name: "read".to_string(),
                result: "file contents".to_string(),
                is_error: false,
            },
            ConversationEvent::Content {
                content: "Final response".to_string(),
                usage: Usage::default(),
                model: "test-model".to_string(),
            },
            ConversationEvent::Error("Something went wrong".to_string()),
        ];

        for event in events {
            match event {
                ConversationEvent::Thinking(_) => {}
                ConversationEvent::ToolCalling { .. } => {}
                ConversationEvent::ToolCompleted { .. } => {}
                ConversationEvent::Content { .. } => {}
                ConversationEvent::Error(_) => {}
            }
        }
    }

    #[test]
    fn test_tool_definitions_build_without_deserialization() {
        let tools = thndrs_tools::get_tool_schemas();
        let tool_definitions: Vec<ToolDefinition> = tools
            .iter()
            .map(|tool| {
                ToolDefinition::from_schema(
                    tool.name.clone(),
                    tool.description.clone(),
                    tool.parameters.to_json_schema(),
                )
            })
            .collect();

        assert_eq!(tool_definitions.len(), tools.len());
        assert!(tool_definitions.iter().all(|tool| tool.type_field == "function"));
    }
}
