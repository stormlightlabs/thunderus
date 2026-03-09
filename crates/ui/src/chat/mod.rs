//! Chat screen components for active conversation

mod app;
mod extractors;
mod formatters;
mod render;

use super::components::ToolCallState;
use crate::colors;
use crate::components::{ToolCallCard, wrapped_line_count};
use crate::tool::{TaskItem, parse_diff};
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use std::path::PathBuf;
use thndrs_core::ResponseSections;

pub use app::ChatApp;
pub use render::draw_chat_screen;

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
        self.status.into()
    }

    pub fn draw_write_output(&self, frame: &mut Frame, area: Rect) {
        let output = formatters::tool_output("write", self.output.as_deref().unwrap_or_default());
        let line_text = if self.status == ToolCallStatus::Success {
            formatters::write_success_ln(&output)
                .unwrap_or_else(|| output.lines().next().unwrap_or_default().to_string())
        } else {
            output.lines().next().unwrap_or_default().to_string()
        };
        let style = if self.status == ToolCallStatus::Success {
            Style::default().fg(colors::ACCENT_GREEN)
        } else {
            Style::default().fg(colors::ACCENT_RED)
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(line_text, style)]))
                .style(Style::default().bg(colors::BG_TERMINAL))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    pub fn estimate_height(&self, width: u16) -> u16 {
        if !&self.expanded {
            return ToolCallCard::collapsed_height();
        }

        let body_height = self.tool_call_expanded_height(width);

        ToolCallCard::collapsed_height() + body_height
    }

    pub fn tool_call_expanded_height(&self, width: u16) -> u16 {
        if matches!(self.status, ToolCallStatus::Pending | ToolCallStatus::Running) {
            return 5;
        }

        match self.name.as_str() {
            "read" => {
                let output = formatters::tool_output("read", self.output.as_deref().unwrap_or_default());
                let line_count = if self.status == ToolCallStatus::Error {
                    output.lines().count().max(1)
                } else {
                    render::build_read_output_lines(&output).len().max(1)
                } as u16;
                line_count.clamp(1, 20) + 1
            }
            "write" => 1,
            "edit" => extractors::edit_diff(&self.arguments)
                .map(|diff| parse_diff(&diff).len() as u16)
                .map(|line_count: u16| line_count.clamp(1, 15) + 1)
                .unwrap_or(3),
            "bash" => {
                let (output, _) = render::parse_bash_output(
                    self.output.as_deref().unwrap_or_default(),
                    self.status == ToolCallStatus::Error,
                );
                let output = formatters::tool_output("bash", &output);
                (render::bash_visible_line_count(&output, render::BASH_MAX_VISIBLE_LINES) as u16).clamp(1, 15) + 2
            }
            "research" => {
                let output = formatters::tool_output("research", self.output.as_deref().unwrap_or_default());
                (wrapped_line_count(&output, width.saturating_sub(4)) as u16).clamp(1, 15) + 1
            }
            _ => {
                let output = formatters::tool_output(&self.name, self.output.as_deref().unwrap_or_default());
                (wrapped_line_count(&output, width.saturating_sub(4)) as u16).clamp(1, 10) + 2
            }
        }
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

#[derive(Debug, Clone, Default)]
struct ChatFileFinder {
    active: bool,
    query: String,
    selected: usize,
    matches: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub reasoning_content: Option<String>,
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
    }

    pub fn finalize(&mut self) {
        if self.role == MessageRole::Assistant && self.sections.is_none() {
            let sections = ResponseSections::parse(&self.content);
            if sections.has_content() {
                self.sections = Some(sections);
            }
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
            tool_call.output = Some(output);
            tool_call.status = if success { ToolCallStatus::Success } else { ToolCallStatus::Error };
            if tool_call.name == "memory_recall" {
                tool_call.expanded = true;
            }
        }
    }

    pub fn estimate_height(&self, width: u16) -> u16 {
        let content_width = width.max(1);
        let mut height = 1u16;

        match self.role {
            MessageRole::Assistant => match &self.sections {
                Some(sections) => {
                    height += render::assistant_section_constraints(sections, content_width)
                        .iter()
                        .map(render::constraint_length)
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
            height += render::assistant_reasoning_height(reasoning, content_width);
        }

        for tool_call in &self.tool_calls {
            height += tool_call.estimate_height(content_width);
        }

        height
    }
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
}
