use crate::theme::Theme;

use std::fmt;
use thunderus_core::ApprovalDecision;

/// Detail level for action cards (progressive disclosure)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardDetailLevel {
    /// Level 1: intent + outcome (brief, scannable)
    #[default]
    Brief,
    /// Level 2: detailed context, scope, execution metadata
    Detailed,
    /// Level 3: full logs, reasoning chain, trace
    Verbose,
}

impl CardDetailLevel {
    pub fn toggle(&mut self) {
        *self = match self {
            CardDetailLevel::Brief => CardDetailLevel::Detailed,
            CardDetailLevel::Detailed => CardDetailLevel::Verbose,
            CardDetailLevel::Verbose => CardDetailLevel::Brief,
        }
    }

    pub fn cycle(&mut self, steps: i8) {
        let levels = [
            CardDetailLevel::Brief,
            CardDetailLevel::Detailed,
            CardDetailLevel::Verbose,
        ];
        let current_index = levels.iter().position(|&l| l == *self).unwrap_or(0);
        let new_index = (current_index as i8 + steps).rem_euclid(levels.len() as i8) as usize;
        *self = levels[new_index];
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CardDetailLevel::Brief => "brief",
            CardDetailLevel::Detailed => "detailed",
            CardDetailLevel::Verbose => "verbose",
        }
    }
}

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
        detail_level: CardDetailLevel,
    },
    /// Tool result with success status
    ToolResult {
        tool: String,
        result: String,
        success: bool,
        error: Option<String>,
        detail_level: CardDetailLevel,
    },
    /// Approval prompt waiting for user input
    ApprovalPrompt {
        action: String,
        risk: String,
        description: Option<String>,
        decision: Option<ApprovalDecision>,
        detail_level: CardDetailLevel,
    },
    /// System message or status
    SystemMessage { content: String },
    /// Error entry with context and optional retry option
    ErrorEntry {
        message: String,
        error_type: ErrorType,
        can_retry: bool,
        context: Option<String>,
    },
}

/// Error type classification for error entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// Provider-related error (API, rate limiting, etc.)
    Provider,
    /// Network timeout or connectivity issue
    Network,
    /// Session write failure
    SessionWrite,
    /// Terminal or TUI error
    Terminal,
    /// User cancellation
    Cancelled,
    /// Generic error
    Other,
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
        Self::ToolCall {
            tool: tool.into(),
            arguments: arguments.into(),
            risk: risk.into(),
            description: None,
            detail_level: CardDetailLevel::default(),
        }
    }

    /// Add description to a tool call
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        if let Self::ToolCall { description: desc, .. } = &mut self {
            *desc = Some(description.into());
        }
        self
    }

    /// Set detail level for tool call
    pub fn with_detail_level(mut self, level: CardDetailLevel) -> Self {
        match &mut self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. } => {
                *detail_level = level;
            }
            _ => {}
        }
        self
    }

    /// Create a tool result entry
    pub fn tool_result(tool: impl Into<String>, result: impl Into<String>, success: bool) -> Self {
        Self::ToolResult {
            tool: tool.into(),
            result: result.into(),
            success,
            error: None,
            detail_level: CardDetailLevel::default(),
        }
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
        Self::ApprovalPrompt {
            action: action.into(),
            risk: risk.into(),
            description: None,
            decision: None,
            detail_level: CardDetailLevel::default(),
        }
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

    /// Create an error entry
    pub fn error_entry(message: impl Into<String>, error_type: ErrorType) -> Self {
        Self::ErrorEntry {
            message: message.into(),
            error_type,
            can_retry: matches!(error_type, ErrorType::Network | ErrorType::Provider),
            context: None,
        }
    }

    /// Add context to an error entry
    pub fn with_error_context(mut self, context: impl Into<String>) -> Self {
        if let Self::ErrorEntry { context: ctx, .. } = &mut self {
            *ctx = Some(context.into());
        }
        self
    }

    /// Set retry option for error entry
    pub fn with_can_retry(mut self, can_retry: bool) -> Self {
        if let Self::ErrorEntry { can_retry: retry, .. } = &mut self {
            *retry = can_retry;
        }
        self
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
            Self::ErrorEntry { .. } => "error-entry",
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

    /// Check if this is an action card (can be expanded for detail)
    pub fn is_action_card(&self) -> bool {
        matches!(
            self,
            Self::ToolCall { .. } | Self::ToolResult { .. } | Self::ApprovalPrompt { .. }
        )
    }

    /// Get the detail level for this entry
    pub fn detail_level(&self) -> CardDetailLevel {
        match self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. } => *detail_level,
            _ => CardDetailLevel::Brief,
        }
    }

    /// Set the detail level for this entry
    pub fn set_detail_level(&mut self, level: CardDetailLevel) {
        match self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. } => {
                *detail_level = level;
            }
            _ => {}
        }
    }

    /// Toggle detail level for this entry
    pub fn toggle_detail_level(&mut self) {
        match self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. } => {
                detail_level.toggle();
            }
            _ => {}
        }
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
            Self::ErrorEntry { message, error_type, can_retry, .. } => {
                let type_str = match error_type {
                    ErrorType::Provider => "Provider",
                    ErrorType::Network => "Network",
                    ErrorType::SessionWrite => "Session",
                    ErrorType::Terminal => "Terminal",
                    ErrorType::Cancelled => "Cancelled",
                    ErrorType::Other => "Error",
                };
                let retry_hint = if *can_retry { " (Press R to retry)" } else { "" };
                write!(f, "[Error: {}] {}{}", type_str, message, retry_hint)
            }
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
    fn test_card_detail_level_default() {
        let level = CardDetailLevel::default();
        assert_eq!(level, CardDetailLevel::Brief);
    }

    #[test]
    fn test_card_detail_level_toggle() {
        let mut level = CardDetailLevel::Brief;
        level.toggle();
        assert_eq!(level, CardDetailLevel::Detailed);
        level.toggle();
        assert_eq!(level, CardDetailLevel::Verbose);
        level.toggle();
        assert_eq!(level, CardDetailLevel::Brief);
    }

    #[test]
    fn test_card_detail_level_cycle_forward() {
        let mut level = CardDetailLevel::Brief;
        level.cycle(1);
        assert_eq!(level, CardDetailLevel::Detailed);
        level.cycle(1);
        assert_eq!(level, CardDetailLevel::Verbose);
        level.cycle(1);
        assert_eq!(level, CardDetailLevel::Brief);
    }

    #[test]
    fn test_card_detail_level_cycle_backward() {
        let mut level = CardDetailLevel::Brief;
        level.cycle(-1);
        assert_eq!(level, CardDetailLevel::Verbose);
        level.cycle(-1);
        assert_eq!(level, CardDetailLevel::Detailed);
        level.cycle(-1);
        assert_eq!(level, CardDetailLevel::Brief);
    }

    #[test]
    fn test_card_detail_level_cycle_multiple() {
        let mut level = CardDetailLevel::Brief;
        level.cycle(5);
        assert_eq!(level, CardDetailLevel::Verbose);
    }

    #[test]
    fn test_card_detail_level_as_str() {
        assert_eq!(CardDetailLevel::Brief.as_str(), "brief");
        assert_eq!(CardDetailLevel::Detailed.as_str(), "detailed");
        assert_eq!(CardDetailLevel::Verbose.as_str(), "verbose");
    }

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

    #[test]
    fn test_tool_call_default_detail_level() {
        let entry = TranscriptEntry::tool_call("test", "{}", "safe");
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_tool_result_default_detail_level() {
        let entry = TranscriptEntry::tool_result("test", "result", true);
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_approval_prompt_default_detail_level() {
        let entry = TranscriptEntry::approval_prompt("test", "safe");
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_is_action_card() {
        let tool_call = TranscriptEntry::tool_call("test", "{}", "safe");
        let tool_result = TranscriptEntry::tool_result("test", "result", true);
        let approval = TranscriptEntry::approval_prompt("test", "safe");
        let user_msg = TranscriptEntry::user_message("test");
        let model_msg = TranscriptEntry::model_response("test");
        let system_msg = TranscriptEntry::system_message("test");

        assert!(tool_call.is_action_card());
        assert!(tool_result.is_action_card());
        assert!(approval.is_action_card());
        assert!(!user_msg.is_action_card());
        assert!(!model_msg.is_action_card());
        assert!(!system_msg.is_action_card());
    }

    #[test]
    fn test_with_detail_level() {
        let entry = TranscriptEntry::tool_call("test", "{}", "safe").with_detail_level(CardDetailLevel::Verbose);
        assert_eq!(entry.detail_level(), CardDetailLevel::Verbose);
    }

    #[test]
    fn test_set_detail_level() {
        let mut entry = TranscriptEntry::tool_call("test", "{}", "safe");
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);

        entry.set_detail_level(CardDetailLevel::Detailed);
        assert_eq!(entry.detail_level(), CardDetailLevel::Detailed);

        entry.set_detail_level(CardDetailLevel::Verbose);
        assert_eq!(entry.detail_level(), CardDetailLevel::Verbose);
    }

    #[test]
    fn test_toggle_detail_level() {
        let mut entry = TranscriptEntry::tool_call("test", "{}", "safe");
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);

        entry.toggle_detail_level();
        assert_eq!(entry.detail_level(), CardDetailLevel::Detailed);

        entry.toggle_detail_level();
        assert_eq!(entry.detail_level(), CardDetailLevel::Verbose);

        entry.toggle_detail_level();
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_non_action_card_detail_level() {
        let entry = TranscriptEntry::user_message("test");
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);

        let mut entry = TranscriptEntry::user_message("test");
        entry.set_detail_level(CardDetailLevel::Verbose);
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_with_detail_level_all_card_types() {
        let tool_call = TranscriptEntry::tool_call("test", "{}", "safe").with_detail_level(CardDetailLevel::Detailed);
        let tool_result =
            TranscriptEntry::tool_result("test", "result", true).with_detail_level(CardDetailLevel::Detailed);
        let approval = TranscriptEntry::approval_prompt("test", "safe").with_detail_level(CardDetailLevel::Detailed);

        assert_eq!(tool_call.detail_level(), CardDetailLevel::Detailed);
        assert_eq!(tool_result.detail_level(), CardDetailLevel::Detailed);
        assert_eq!(approval.detail_level(), CardDetailLevel::Detailed);
    }

    #[test]
    fn test_error_entry_creation() {
        let entry = TranscriptEntry::error_entry("Something went wrong", ErrorType::Other);
        assert_eq!(entry.type_name(), "error-entry");
    }

    #[test]
    fn test_error_entry_with_context() {
        let entry = TranscriptEntry::error_entry("Network timeout", ErrorType::Network)
            .with_error_context("Failed to reach API");

        if let TranscriptEntry::ErrorEntry { context, .. } = entry {
            assert_eq!(context, Some("Failed to reach API".to_string()));
        } else {
            panic!("Expected ErrorEntry");
        }
    }

    #[test]
    fn test_error_entry_can_retry() {
        let entry = TranscriptEntry::error_entry("API error", ErrorType::Provider)
            .with_can_retry(true);

        if let TranscriptEntry::ErrorEntry { can_retry, .. } = entry {
            assert!(can_retry);
        } else {
            panic!("Expected ErrorEntry");
        }
    }

    #[test]
    fn test_error_entry_display_with_retry() {
        let entry = TranscriptEntry::error_entry("Network error", ErrorType::Network);
        let display = entry.to_string();
        assert!(display.contains("Network"));
        assert!(display.contains("Network error"));
        assert!(display.contains("Press R to retry"));
    }

    #[test]
    fn test_error_entry_display_no_retry() {
        let entry = TranscriptEntry::error_entry("Cancelled", ErrorType::Cancelled).with_can_retry(false);
        let display = entry.to_string();
        assert!(display.contains("Cancelled"));
        assert!(!display.contains("Press R to retry"));
    }

    #[test]
    fn test_error_entry_provider_type() {
        let entry = TranscriptEntry::error_entry("API error", ErrorType::Provider);
        if let TranscriptEntry::ErrorEntry { error_type, can_retry, .. } = entry {
            assert_eq!(error_type, ErrorType::Provider);
            assert!(can_retry);
        } else {
            panic!("Expected ErrorEntry");
        }
    }

    #[test]
    fn test_error_entry_network_type() {
        let entry = TranscriptEntry::error_entry("Timeout", ErrorType::Network);
        if let TranscriptEntry::ErrorEntry { error_type, can_retry, .. } = entry {
            assert_eq!(error_type, ErrorType::Network);
            assert!(can_retry);
        } else {
            panic!("Expected ErrorEntry");
        }
    }

    #[test]
    fn test_error_entry_cancelled_type() {
        let entry = TranscriptEntry::error_entry("User cancelled", ErrorType::Cancelled);
        if let TranscriptEntry::ErrorEntry { error_type, can_retry, .. } = entry {
            assert_eq!(error_type, ErrorType::Cancelled);
            assert!(!can_retry);
        } else {
            panic!("Expected ErrorEntry");
        }
    }

    #[test]
    fn test_error_entry_is_not_action_card() {
        let entry = TranscriptEntry::error_entry("Test error", ErrorType::Other);
        assert!(!entry.is_action_card());
        assert!(!entry.is_pending());
    }

    #[test]
    fn test_error_entry_all_types() {
        let provider = TranscriptEntry::error_entry("Provider error", ErrorType::Provider);
        let network = TranscriptEntry::error_entry("Network error", ErrorType::Network);
        let session = TranscriptEntry::error_entry("Session error", ErrorType::SessionWrite);
        let terminal = TranscriptEntry::error_entry("Terminal error", ErrorType::Terminal);
        let cancelled = TranscriptEntry::error_entry("Cancelled", ErrorType::Cancelled);
        let other = TranscriptEntry::error_entry("Other error", ErrorType::Other);

        for entry in [provider, network, session, terminal, cancelled, other] {
            assert_eq!(entry.type_name(), "error-entry");
        }
    }
}
