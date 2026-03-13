//! Chat screen components for active conversation

mod app;
mod screen;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use thndrs_core::ResponseSections;

pub(crate) use app::{ChatApp, ChatMsg};
pub(crate) use app::{map_chat_key_to_msg, update};
pub(crate) use screen::{ChatFileFinderOverlay, ChatScreen};

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
    pub name: String,
    pub arguments: String,
    pub status: ToolCallStatus,
    pub output: Option<String>,
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolCallStatus {
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

pub(crate) fn u32_with_grouping(value: u32) -> String {
    let mut out = String::new();
    let digits = value.to_string();

    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }

    out.chars().rev().collect()
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
        self.tool_calls.push(ToolCallDisplay {
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
