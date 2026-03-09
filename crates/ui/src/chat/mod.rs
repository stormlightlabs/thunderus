//! Chat screen components for active conversation

mod app;
mod formatters;
mod input_render;
mod measure;
mod message_render;
mod render;
mod tool_render;

use super::components::ToolCallState;
use crate::components::wrapped_line_count;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use thndrs_core::ResponseSections;
use tool_render::TaskItem;

pub use app::ChatApp;
pub(crate) use formatters::u32_with_grouping;
pub use render::draw_chat_screen;

type ChatFileFinder = crate::finder::FuzzyFinder<PathBuf>;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum StreamingState {
    #[default]
    Idle,
    Streaming,
    Thinking,
    /// Tool name
    CallingTool(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn label(self) -> &'static str {
        match self {
            MessageRole::User => "YOU",
            MessageRole::Assistant => "ASSISTANT",
            MessageRole::Tool => "TOOL",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub id: String,
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub output: Option<String>,
    pub expanded: bool,
}

impl ToolCallDisplay {
    pub fn to_ui_state(&self) -> ToolCallState {
        tool_render::tool_call_state(self.status)
    }

    pub fn estimate_height(&self, width: u16) -> u16 {
        if !&self.expanded {
            return crate::components::ToolCallCard::collapsed_height();
        }

        let body_height = self.tool_call_expanded_height(width);

        crate::components::ToolCallCard::collapsed_height() + body_height
    }

    pub fn tool_call_expanded_height(&self, width: u16) -> u16 {
        tool_render::tool_call_expanded_height(self, width)
    }

    pub fn progress_tasks(&self) -> Vec<TaskItem> {
        match self.status {
            ToolCallStatus::Pending => vec![
                TaskItem::new("Queued").running(),
                TaskItem::new(format!("Execute {}", self.name)),
                TaskItem::new("Collect output"),
            ],
            ToolCallStatus::Running => vec![
                TaskItem::new("Queued").done(),
                TaskItem::new(format!("Execute {}", self.name)).running(),
                TaskItem::new("Collect output"),
            ],
            ToolCallStatus::Success | ToolCallStatus::Error => vec![
                TaskItem::new("Queued").done(),
                TaskItem::new(format!("Execute {}", self.name)).done(),
                TaskItem::new("Collect output").done(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncomingStreamEvent {
    Delta {
        content: Option<String>,
        reasoning_content: Option<String>,
    },
    Done {
        usage: Option<TokenUsage>,
        model: Option<String>,
    },
    Error(String),
    ToolCalling {
        name: String,
        arguments: String,
    },
    ToolCompleted {
        name: String,
        result: String,
        is_error: bool,
    },
    Thinking(String),
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub reasoning_content: Option<String>,
    pub expanded_reasoning: bool,
    pub sections: Option<ResponseSections>,
    pub tool_calls: Vec<ToolCallDisplay>,
    pub created_at: DateTime<Utc>,
}

impl ChatMessage {
    pub fn user(content: String) -> Self {
        Self::user_at(content, Utc::now())
    }

    pub fn user_at(content: String, created_at: DateTime<Utc>) -> Self {
        Self {
            role: MessageRole::User,
            content,
            reasoning_content: None,
            expanded_reasoning: false,
            sections: None,
            tool_calls: Vec::new(),
            created_at,
        }
    }

    pub fn assistant(content: String) -> Self {
        Self::assistant_at(content, Utc::now())
    }

    pub fn assistant_at(content: String, created_at: DateTime<Utc>) -> Self {
        let sections = ResponseSections::parse(&content);
        let has_sections = sections.has_content();
        Self {
            role: MessageRole::Assistant,
            content,
            reasoning_content: None,
            expanded_reasoning: false,
            sections: if has_sections { Some(sections) } else { None },
            tool_calls: Vec::new(),
            created_at,
        }
    }

    pub fn assistant_streaming(content: String) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            reasoning_content: None,
            expanded_reasoning: true,
            sections: None,
            tool_calls: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn tool(name: String, output: String) -> Self {
        Self::tool_at(name, output, Utc::now())
    }

    pub fn tool_at(_name: String, output: String, created_at: DateTime<Utc>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: output,
            reasoning_content: None,
            expanded_reasoning: false,
            sections: None,
            tool_calls: Vec::new(),
            created_at,
        }
    }

    pub fn append_reasoning_delta(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }

        match self.reasoning_content.as_mut() {
            Some(reasoning) => reasoning.push_str(delta),
            None => self.reasoning_content = Some(delta.to_string()),
        }
        self.expanded_reasoning = true;
    }

    pub fn finalize(&mut self) {
        if self.role == MessageRole::Assistant && self.sections.is_none() {
            let sections = ResponseSections::parse(&self.content);
            if sections.has_content() {
                self.sections = Some(sections);
            }
        }
        if self.role == MessageRole::Assistant
            && self
                .reasoning_content
                .as_deref()
                .is_some_and(|reasoning| !reasoning.trim().is_empty())
        {
            self.expanded_reasoning = false;
        }
    }

    pub fn add_tool_call(&mut self, name: String, arguments: String) -> usize {
        let id = format!("call_{}", self.tool_calls.len());
        self.tool_calls.push(ToolCallDisplay {
            id,
            name,
            arguments,
            status: ToolCallStatus::Running,
            output: None,
            expanded: false,
        });
        self.tool_calls.len() - 1
    }

    pub fn complete_tool_call(&mut self, index: usize, output: String, success: bool) {
        if let Some(tool_call) = self.tool_calls.get_mut(index) {
            let auto_expand = should_auto_expand_tool_call(&tool_call.name, &output);
            tool_call.output = Some(output);
            tool_call.status = if success { ToolCallStatus::Success } else { ToolCallStatus::Error };
            tool_call.expanded = auto_expand;
        }
    }

    pub fn toggle_reasoning(&mut self) -> bool {
        if self.role != MessageRole::Assistant {
            return false;
        }
        if self
            .reasoning_content
            .as_deref()
            .map(|reasoning| reasoning.trim().is_empty())
            .unwrap_or(true)
        {
            return false;
        }
        self.expanded_reasoning = !self.expanded_reasoning;
        true
    }

    pub fn estimate_height(&self, width: u16) -> u16 {
        let content_width = width.max(1);
        let mut height = 1u16;

        match self.role {
            MessageRole::Assistant => match &self.sections {
                Some(sections) => {
                    height += message_render::assistant_section_constraints(sections, content_width)
                        .iter()
                        .map(message_render::constraint_length)
                        .sum::<u16>()
                }
                None => {
                    let content = formatters::normalize_display_content(&self.content);
                    height += wrapped_line_count(&content, content_width.saturating_sub(2)) as u16;
                }
            },
            MessageRole::Tool => {
                let content = formatters::normalize_display_content(&self.content);
                height += wrapped_line_count(&content, content_width.saturating_sub(2)) as u16;
            }
            MessageRole::User => height += wrapped_line_count(&self.content, content_width.saturating_sub(2)) as u16,
        }

        if let Some(reasoning) = self.reasoning_content.as_deref()
            && !reasoning.trim().is_empty()
        {
            height +=
                message_render::assistant_reasoning_block_height(reasoning, content_width, self.expanded_reasoning);
        }

        for tool_call in &self.tool_calls {
            height += tool_call.estimate_height(content_width);
        }

        height
    }
}

fn should_auto_expand_tool_call(name: &str, output: &str) -> bool {
    if matches!(name, "memory_recall" | "write") {
        return true;
    }

    let line_count = output.lines().count().max(1);
    let char_count = output.chars().count();
    line_count <= 3 && char_count <= 240
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_user() {
        let msg = ChatMessage::user("Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.sections.is_none());
    }

    #[test]
    fn test_chat_message_assistant_with_sections() {
        let content = "Intent\n\nDo something\n\nActions\n\n- action1\n\nResult\n\nDone\n\nNext\n\nDone";
        let msg = ChatMessage::assistant(content.to_string());
        assert_eq!(msg.role, MessageRole::Assistant);
        assert!(msg.sections.is_some());
        let sections = msg.sections.unwrap();
        assert_eq!(sections.intent, Some("Do something".to_string()));
    }

    #[test]
    fn test_tool_call_expansion() {
        let mut msg = ChatMessage::assistant_streaming(String::new());
        let idx = msg.add_tool_call("read".to_string(), r#"{"path": "/test.txt"}"#.to_string());

        assert!(!msg.tool_calls[idx].expanded);

        msg.tool_calls[idx].expanded = true;
        assert!(msg.tool_calls[idx].expanded);
    }

    #[test]
    fn test_complete_tool_call_auto_expand_policy() {
        let mut msg = ChatMessage::assistant_streaming(String::new());
        let read_idx = msg.add_tool_call("read".to_string(), r#"{"path":"a.rs"}"#.to_string());
        msg.complete_tool_call(read_idx, "line1\nline2\nline3\nline4".to_string(), true);
        assert!(!msg.tool_calls[read_idx].expanded);

        let recall_idx = msg.add_tool_call("memory_recall".to_string(), r#"{"query":"x"}"#.to_string());
        msg.complete_tool_call(recall_idx, "Found memory".to_string(), true);
        assert!(msg.tool_calls[recall_idx].expanded);
    }
}
