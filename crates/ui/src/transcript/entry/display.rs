use std::fmt;
use thunderus_core::ApprovalDecision;

use super::TranscriptEntry;

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
                write!(f, "[{}] {}", tool, if *success { "OK" } else { "FAIL" })
            }
            Self::PatchDisplay { patch_name, file_path, .. } => {
                write!(f, "[Patch] {} @ {}", patch_name, file_path)
            }
            Self::ApprovalPrompt { action, decision, risk, .. } => {
                let status = match decision {
                    None => "PENDING",
                    Some(ApprovalDecision::Approved) => "APPROVED",
                    Some(ApprovalDecision::Rejected) => "REJECTED",
                    Some(ApprovalDecision::Cancelled) => "CANCELLED",
                };
                write!(f, "[Approval] {} {} [{}]", action, status, risk)
            }
            Self::SystemMessage { content } => write!(f, "[System] {}", content),
            Self::ErrorEntry { message, error_type, can_retry, .. } => {
                let type_str = match error_type {
                    super::ErrorType::Provider => "Provider",
                    super::ErrorType::Network => "Network",
                    super::ErrorType::SessionWrite => "Session",
                    super::ErrorType::Terminal => "Terminal",
                    super::ErrorType::Cancelled => "Cancelled",
                    super::ErrorType::Other => "Error",
                };
                let retry_hint = if *can_retry { " (Press R to retry)" } else { "" };
                write!(f, "[Error: {}] {}{}", type_str, message, retry_hint)
            }
            Self::ThinkingIndicator { duration_secs } => {
                write!(f, "Thought for {:.0}s", duration_secs)
            }
            Self::StatusLine { message, .. } => {
                write!(f, ":: {}", message)
            }
        }
    }
}

impl TranscriptEntry {
    /// Get emoji for risk level
    pub fn risk_emoji(risk: &str) -> &'static str {
        match risk {
            "safe" => "OK",
            "risky" => "!",
            "dangerous" => "X",
            _ => "?",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::transcript::ErrorType;

    use super::*;

    #[test]
    fn test_transcript_entry_user_message_display() {
        let entry = TranscriptEntry::user_message("Hello");
        assert_eq!(entry.to_string(), "You: Hello");
    }

    #[test]
    fn test_transcript_entry_model_response_display() {
        let entry = TranscriptEntry::model_response("Hi there");
        assert_eq!(entry.to_string(), "Agent: Hi there");
    }

    #[test]
    fn test_transcript_entry_streaming_response_display() {
        let entry = TranscriptEntry::streaming_response("partial");
        assert!(entry.to_string().contains("(streaming)"));
    }

    #[test]
    fn test_transcript_entry_tool_call_display() {
        let entry = TranscriptEntry::tool_call("fs.read", "{ path: '/tmp' }", "safe");
        let display = entry.to_string();
        assert!(display.contains("[fs.read]"));
        assert!(display.contains("OK"));
    }

    #[test]
    fn test_transcript_entry_tool_result_display() {
        let entry = TranscriptEntry::tool_result("fs.read", "file content", true);
        let display = entry.to_string();
        assert!(display.contains("[fs.read]"));
        assert!(display.contains("OK"));
    }

    #[test]
    fn test_transcript_entry_approval_prompt_display() {
        let entry = TranscriptEntry::approval_prompt("patch.feature", "risky");
        let display = entry.to_string();
        assert!(display.contains("PENDING"));
        assert!(display.contains("risky"));
    }

    #[test]
    fn test_transcript_entry_approval_with_decision_display() {
        let entry =
            TranscriptEntry::approval_prompt("patch.feature", "risky").with_decision(ApprovalDecision::Approved);

        assert!(entry.to_string().contains("APPROVED"));
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
    fn test_patch_display_display_format() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let hunk_labels = vec![Some("Test".to_string())];
        let entry = TranscriptEntry::patch_display("patch1", "src/test.txt", diff, hunk_labels);

        let display = entry.to_string();
        assert!(display.contains("[Patch]"));
        assert!(display.contains("patch1"));
        assert!(display.contains("src/test.txt"));
    }

    #[test]
    fn test_risk_color_markers() {
        assert_eq!(TranscriptEntry::risk_emoji("safe"), "OK");
        assert_eq!(TranscriptEntry::risk_emoji("risky"), "!");
        assert_eq!(TranscriptEntry::risk_emoji("dangerous"), "X");
        assert_eq!(TranscriptEntry::risk_emoji("unknown"), "?");
    }
}
