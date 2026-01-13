use crate::theme::Theme;
use std::fmt;

/// Transcript entry types that can be displayed in transcript
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptEntry {
    /// User message
    UserMessage { content: String },
    /// Model response (may be streaming)
    ModelResponse { content: String, streaming: bool },
    /// Tool call with risk classification
    ToolCall {
        tool: String,
        arguments: String,
        risk: String,
        description: Option<String>,
    },
    /// Tool result with success status
    ToolResult {
        tool: String,
        result: String,
        success: bool,
        error: Option<String>,
    },
    /// Approval prompt waiting for user input
    ApprovalPrompt {
        action: String,
        risk: String,
        description: Option<String>,
        decision: Option<ApprovalDecision>,
    },
    /// System message or status
    SystemMessage { content: String },
}

/// Approval decision by user
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// User approved the action
    Approved,
    /// User rejected the action
    Rejected,
    /// User cancelled the operation
    Cancelled,
}

impl TranscriptEntry {
    /// Create a new user message entry
    pub fn user_message(content: impl Into<String>) -> Self {
        Self::UserMessage { content: content.into() }
    }

    /// Create a new model response entry
    pub fn model_response(content: impl Into<String>) -> Self {
        Self::ModelResponse { content: content.into(), streaming: false }
    }

    /// Create a streaming model response entry
    pub fn streaming_response(content: impl Into<String>) -> Self {
        Self::ModelResponse { content: content.into(), streaming: true }
    }

    /// Create a tool call entry
    pub fn tool_call(tool: impl Into<String>, arguments: impl Into<String>, risk: impl Into<String>) -> Self {
        Self::ToolCall { tool: tool.into(), arguments: arguments.into(), risk: risk.into(), description: None }
    }

    /// Add description to a tool call
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        if let Self::ToolCall { description: desc, .. } = &mut self {
            *desc = Some(description.into());
        }
        self
    }

    /// Create a tool result entry
    pub fn tool_result(tool: impl Into<String>, result: impl Into<String>, success: bool) -> Self {
        Self::ToolResult { tool: tool.into(), result: result.into(), success, error: None }
    }

    /// Add error to a tool result
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        if let Self::ToolResult { error: err, .. } = &mut self {
            *err = Some(error.into());
        }
        self
    }

    /// Create an approval prompt entry
    pub fn approval_prompt(action: impl Into<String>, risk: impl Into<String>) -> Self {
        Self::ApprovalPrompt { action: action.into(), risk: risk.into(), description: None, decision: None }
    }

    /// Set approval decision
    pub fn with_decision(mut self, decision: ApprovalDecision) -> Self {
        if let Self::ApprovalPrompt { decision: dec, .. } = &mut self {
            *dec = Some(decision);
        }
        self
    }

    /// Create a system message entry
    pub fn system_message(content: impl Into<String>) -> Self {
        Self::SystemMessage { content: content.into() }
    }

    /// Check if entry is pending (waiting for user action)
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::ApprovalPrompt { decision: None, .. })
    }

    /// Get entry type name for debugging
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::UserMessage { .. } => "user-message",
            Self::ModelResponse { .. } => "model-response",
            Self::ToolCall { .. } => "tool-call",
            Self::ToolResult { .. } => "tool-result",
            Self::ApprovalPrompt { .. } => "approval-prompt",
            Self::SystemMessage { .. } => "system-message",
        }
    }

    /// Get risk level color as ratatui Color
    pub fn risk_level_color_str(risk: &str) -> ratatui::style::Color {
        match risk {
            "safe" => Theme::GREEN,
            "risky" => Theme::YELLOW,
            "dangerous" => Theme::RED,
            _ => Theme::MUTED,
        }
    }

    /// Check if this is a tool-related entry
    pub fn is_tool_entry(&self) -> bool {
        matches!(self, Self::ToolCall { .. } | Self::ToolResult { .. })
    }

    /// Check if this is an approval-related entry
    pub fn is_approval_entry(&self) -> bool {
        matches!(self, Self::ApprovalPrompt { .. })
    }
}

impl fmt::Display for TranscriptEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserMessage { content } => write!(f, "You: {}", content),
            Self::ModelResponse { content, streaming } => {
                if *streaming {
                    write!(f, "Agent (streaming): {}", content)
                } else {
                    write!(f, "Agent: {}", content)
                }
            }
            Self::ToolCall { tool, risk, .. } => {
                write!(f, "[{}] {} [{}]", tool, risk, Self::risk_emoji(risk))
            }
            Self::ToolResult { tool, success, .. } => {
                write!(f, "[{}] {}", tool, if *success { "✅" } else { "❌" })
            }
            Self::ApprovalPrompt { action, decision, risk, .. } => {
                let status = match decision {
                    None => "⏳",
                    Some(ApprovalDecision::Approved) => "✅",
                    Some(ApprovalDecision::Rejected) => "❌",
                    Some(ApprovalDecision::Cancelled) => "⏹️",
                };
                write!(f, "[Approval] {} {} [{}]", action, status, risk)
            }
            Self::SystemMessage { content } => write!(f, "[System] {}", content),
        }
    }
}

impl TranscriptEntry {
    /// Get emoji for risk level
    pub fn risk_emoji(risk: &str) -> &'static str {
        match risk {
            "safe" => "🟢",
            "risky" => "🟡",
            "dangerous" => "🔴",
            _ => "⚪",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcript_entry_user_message() {
        let entry = TranscriptEntry::user_message("Hello");
        assert_eq!(entry.type_name(), "user-message");
        assert!(!entry.is_pending());
        assert_eq!(entry.to_string(), "You: Hello");
        assert!(!entry.is_tool_entry());
        assert!(!entry.is_approval_entry());
    }

    #[test]
    fn test_transcript_entry_model_response() {
        let entry = TranscriptEntry::model_response("Hi there");
        assert_eq!(entry.type_name(), "model-response");
        assert!(!entry.is_pending());
        assert_eq!(entry.to_string(), "Agent: Hi there");
    }

    #[test]
    fn test_transcript_entry_streaming_response() {
        let entry = TranscriptEntry::streaming_response("partial");
        assert_eq!(entry.type_name(), "model-response");
        assert!(entry.to_string().contains("(streaming)"));
    }

    #[test]
    fn test_transcript_entry_tool_call() {
        let entry = TranscriptEntry::tool_call("fs.read", "{ path: '/tmp' }", "safe");
        assert_eq!(entry.type_name(), "tool-call");
        assert!(!entry.is_pending());
        assert!(entry.to_string().contains("[fs.read]"));
        assert!(entry.to_string().contains("🟢"));
        assert!(entry.is_tool_entry());
        assert!(!entry.is_approval_entry());
    }

    #[test]
    fn test_transcript_entry_tool_call_with_description() {
        let entry = TranscriptEntry::tool_call("fs.write", "{ path: '/tmp/file' }", "risky")
            .with_description("Write to temporary file");

        match &entry {
            TranscriptEntry::ToolCall { description, .. } => {
                assert_eq!(description, &Some("Write to temporary file".to_string()));
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_transcript_entry_tool_result() {
        let entry = TranscriptEntry::tool_result("fs.read", "file content", true);
        assert_eq!(entry.type_name(), "tool-result");
        assert!(!entry.is_pending());
        assert!(entry.to_string().contains("[fs.read]"));
        assert!(entry.to_string().contains("✅"));
        assert!(entry.is_tool_entry());
    }

    #[test]
    fn test_transcript_entry_tool_result_failure() {
        let entry = TranscriptEntry::tool_result("fs.read", "error", false).with_error("File not found");

        match &entry {
            TranscriptEntry::ToolResult { error, .. } => {
                assert_eq!(error, &Some("File not found".to_string()));
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_transcript_entry_approval_prompt() {
        let entry = TranscriptEntry::approval_prompt("patch.feature", "risky");
        assert_eq!(entry.type_name(), "approval-prompt");
        assert!(entry.is_pending());
        assert!(entry.to_string().contains("⏳"));
        assert!(entry.is_approval_entry());
        assert!(!entry.is_tool_entry());
    }

    #[test]
    fn test_transcript_entry_approval_with_decision() {
        let entry =
            TranscriptEntry::approval_prompt("patch.feature", "risky").with_decision(ApprovalDecision::Approved);

        assert!(!entry.is_pending());
        assert!(entry.to_string().contains("✅"));
    }

    #[test]
    fn test_risk_level_colors() {
        let safe_entry = TranscriptEntry::tool_call("test", "{}", "safe");
        let risky_entry = TranscriptEntry::tool_call("test", "{}", "risky");
        let dangerous_entry = TranscriptEntry::tool_call("test", "{}", "dangerous");
        let unknown_entry = TranscriptEntry::tool_call("test", "{}", "unknown");

        if let TranscriptEntry::ToolCall { risk, .. } = safe_entry {
            assert_eq!(risk, "safe");
        }
        if let TranscriptEntry::ToolCall { risk, .. } = risky_entry {
            assert_eq!(risk, "risky");
        }
        if let TranscriptEntry::ToolCall { risk, .. } = dangerous_entry {
            assert_eq!(risk, "dangerous");
        }
        if let TranscriptEntry::ToolCall { risk, .. } = unknown_entry {
            assert_eq!(risk, "unknown");
        }

        assert_eq!(TranscriptEntry::risk_level_color_str("safe"), Theme::GREEN);
        assert_eq!(TranscriptEntry::risk_level_color_str("risky"), Theme::YELLOW);
        assert_eq!(TranscriptEntry::risk_level_color_str("dangerous"), Theme::RED);
        assert_eq!(TranscriptEntry::risk_level_color_str("unknown"), Theme::MUTED);
    }

    #[test]
    fn test_risk_color_emojis() {
        assert_eq!(TranscriptEntry::risk_emoji("safe"), "🟢");
        assert_eq!(TranscriptEntry::risk_emoji("risky"), "🟡");
        assert_eq!(TranscriptEntry::risk_emoji("dangerous"), "🔴");
        assert_eq!(TranscriptEntry::risk_emoji("unknown"), "⚪");
    }
}
