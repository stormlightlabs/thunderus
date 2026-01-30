use super::{CardDetailLevel, TranscriptEntry};
use crate::theme::ThemePalette;

use ratatui::style::Color;

impl TranscriptEntry {
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
            Self::PatchDisplay { .. } => "patch-display",
            Self::ApprovalPrompt { .. } => "approval-prompt",
            Self::SystemMessage { .. } => "system-message",
            Self::ErrorEntry { .. } => "error-entry",
            Self::ThinkingIndicator { .. } => "thinking-indicator",
            Self::StatusLine { .. } => "status-line",
        }
    }

    /// Get risk level color as ratatui Color
    pub fn risk_level_color(palette: ThemePalette, risk: &str) -> Color {
        match risk {
            "safe" => palette.green,
            "risky" => palette.yellow,
            "dangerous" => palette.red,
            _ => palette.muted,
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
            Self::ToolCall { .. } | Self::ToolResult { .. } | Self::ApprovalPrompt { .. } | Self::PatchDisplay { .. }
        )
    }

    /// Get the detail level for this entry
    pub fn detail_level(&self) -> CardDetailLevel {
        match self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. }
            | Self::PatchDisplay { detail_level, .. } => *detail_level,
            _ => CardDetailLevel::Brief,
        }
    }

    /// Set the detail level for this entry
    pub fn set_detail_level(&mut self, level: CardDetailLevel) {
        match self {
            Self::ToolCall { detail_level, .. }
            | Self::ToolResult { detail_level, .. }
            | Self::ApprovalPrompt { detail_level, .. }
            | Self::PatchDisplay { detail_level, .. } => {
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
            | Self::ApprovalPrompt { detail_level, .. }
            | Self::PatchDisplay { detail_level, .. } => {
                detail_level.toggle();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Theme, ThemeVariant,
        transcript::{ErrorType, StatusType},
    };
    use thunderus_core::ApprovalDecision;

    #[test]
    fn test_transcript_entry_is_pending() {
        let entry = TranscriptEntry::approval_prompt("patch.feature", "risky");
        assert!(entry.is_pending());

        let entry =
            TranscriptEntry::approval_prompt("patch.feature", "risky").with_decision(ApprovalDecision::Approved);
        assert!(!entry.is_pending());
    }

    #[test]
    fn test_transcript_entry_type_name() {
        let user = TranscriptEntry::user_message("Hello");
        let model = TranscriptEntry::model_response("Hi there");
        let tool = TranscriptEntry::tool_call("fs.read", "{}", "safe");
        let result = TranscriptEntry::tool_result("fs.read", "ok", true);
        let approval = TranscriptEntry::approval_prompt("patch.feature", "risky");
        let system = TranscriptEntry::system_message("System");
        let error = TranscriptEntry::error_entry("Error", ErrorType::Other);
        let thinking = TranscriptEntry::thinking_indicator(1.0);
        let status = TranscriptEntry::status_line("Ready", StatusType::Ready);

        assert_eq!(user.type_name(), "user-message");
        assert_eq!(model.type_name(), "model-response");
        assert_eq!(tool.type_name(), "tool-call");
        assert_eq!(result.type_name(), "tool-result");
        assert_eq!(approval.type_name(), "approval-prompt");
        assert_eq!(system.type_name(), "system-message");
        assert_eq!(error.type_name(), "error-entry");
        assert_eq!(thinking.type_name(), "thinking-indicator");
        assert_eq!(status.type_name(), "status-line");
    }

    #[test]
    fn test_tool_entry_flags() {
        let tool_call = TranscriptEntry::tool_call("test", "{}", "safe");
        let tool_result = TranscriptEntry::tool_result("test", "result", true);
        let approval = TranscriptEntry::approval_prompt("test", "safe");

        assert!(tool_call.is_tool_entry());
        assert!(tool_result.is_tool_entry());
        assert!(!approval.is_tool_entry());
    }

    #[test]
    fn test_approval_entry_flags() {
        let approval = TranscriptEntry::approval_prompt("test", "safe");
        let user_msg = TranscriptEntry::user_message("test");

        assert!(approval.is_approval_entry());
        assert!(!user_msg.is_approval_entry());
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
    fn test_default_detail_levels() {
        let tool_call = TranscriptEntry::tool_call("test", "{}", "safe");
        let tool_result = TranscriptEntry::tool_result("test", "result", true);
        let approval = TranscriptEntry::approval_prompt("test", "safe");

        assert_eq!(tool_call.detail_level(), CardDetailLevel::Brief);
        assert_eq!(tool_result.detail_level(), CardDetailLevel::Brief);
        assert_eq!(approval.detail_level(), CardDetailLevel::Brief);
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
    fn test_risk_level_colors() {
        let palette = Theme::palette(ThemeVariant::Iceberg);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "safe"), palette.green);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "risky"), palette.yellow);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "dangerous"), palette.red);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "unknown"), palette.muted);
    }

    #[test]
    fn test_error_entry_is_not_action_card() {
        let entry = TranscriptEntry::error_entry("Test error", ErrorType::Other);
        assert!(!entry.is_action_card());
        assert!(!entry.is_pending());
    }
}
