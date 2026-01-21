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
    /// Tool call with risk classification and teaching context
    ToolCall {
        tool: String,
        arguments: String,
        risk: String,
        /// WHAT: Plain language description of the operation
        description: Option<String>,
        /// WHY: Context for this action in task flow
        task_context: Option<String>,
        /// SCOPE: Files/paths affected, blast radius
        scope: Option<String>,
        /// RISK: Classification with reasoning (why safe/risky)
        classification_reasoning: Option<String>,
        detail_level: CardDetailLevel,
    },
    /// Tool result with success status and teaching context
    ToolResult {
        tool: String,
        result: String,
        success: bool,
        error: Option<String>,
        /// RESULT: Exit code (0-255)
        exit_code: Option<i32>,
        /// RESULT: Next steps or follow-up actions
        next_steps: Option<Vec<String>>,
        detail_level: CardDetailLevel,
    },
    /// Patch display with hunk-level intent labels
    PatchDisplay {
        patch_name: String,
        file_path: String,
        diff_content: String,
        /// Hunk-level intent labels (index -> label)
        hunk_labels: Vec<Option<String>>,
        detail_level: CardDetailLevel,
    },
    /// Approval prompt waiting for user input with teaching context
    ApprovalPrompt {
        action: String,
        risk: String,
        /// WHAT: Plain language description of the operation
        description: Option<String>,
        /// WHY: Context for this action in task flow
        task_context: Option<String>,
        /// SCOPE: Files/paths affected, blast radius
        scope: Option<String>,
        /// RISK: Classification with reasoning
        risk_reasoning: Option<String>,
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
    /// Thinking indicator showing elapsed time
    ThinkingIndicator {
        /// Duration in seconds
        duration_secs: f32,
    },
    /// Status line for current state
    StatusLine { message: String, status_type: StatusType },
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

/// Status types for the status line display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusType {
    /// Ready for input
    #[default]
    Ready,
    /// Building/compiling
    Building,
    /// Generating response
    Generating,
    /// Waiting for approval
    WaitingApproval,
    /// Interrupted by user
    Interrupted,
    /// Idle
    Idle,
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
            task_context: None,
            scope: None,
            classification_reasoning: None,
            detail_level: CardDetailLevel::default(),
        }
    }

    /// Add description to a tool call (WHAT field)
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        if let Self::ToolCall { description: desc, .. } = &mut self {
            *desc = Some(description.into());
        }
        self
    }

    /// Add task context to a tool call (WHY field)
    pub fn with_task_context(mut self, context: impl Into<String>) -> Self {
        if let Self::ToolCall { task_context, .. } = &mut self {
            *task_context = Some(context.into());
        }
        self
    }

    /// Add scope to a tool call (SCOPE field)
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        if let Self::ToolCall { scope: s, .. } = &mut self {
            *s = Some(scope.into());
        }
        self
    }

    /// Add classification reasoning to a tool call (RISK field)
    pub fn with_classification_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        if let Self::ToolCall { classification_reasoning, .. } = &mut self {
            *classification_reasoning = Some(reasoning.into());
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
            exit_code: None,
            next_steps: None,
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

    /// Add exit code to a tool result (RESULT field)
    pub fn with_exit_code(mut self, exit_code: i32) -> Self {
        if let Self::ToolResult { exit_code: ec, .. } = &mut self {
            *ec = Some(exit_code);
        }
        self
    }

    /// Add next steps to a tool result (RESULT field)
    pub fn with_next_steps(mut self, steps: Vec<String>) -> Self {
        if let Self::ToolResult { next_steps, .. } = &mut self {
            *next_steps = Some(steps);
        }
        self
    }

    /// Create a patch display entry with hunk labels
    pub fn patch_display(
        patch_name: impl Into<String>, file_path: impl Into<String>, diff_content: impl Into<String>,
        hunk_labels: Vec<Option<String>>,
    ) -> Self {
        Self::PatchDisplay {
            patch_name: patch_name.into(),
            file_path: file_path.into(),
            diff_content: diff_content.into(),
            hunk_labels,
            detail_level: CardDetailLevel::default(),
        }
    }

    /// Create an approval prompt entry
    pub fn approval_prompt(action: impl Into<String>, risk: impl Into<String>) -> Self {
        Self::ApprovalPrompt {
            action: action.into(),
            risk: risk.into(),
            description: None,
            task_context: None,
            scope: None,
            risk_reasoning: None,
            decision: None,
            detail_level: CardDetailLevel::default(),
        }
    }

    /// Add description to an approval prompt (WHAT field)
    pub fn with_approval_description(mut self, description: impl Into<String>) -> Self {
        if let Self::ApprovalPrompt { description: desc, .. } = &mut self {
            *desc = Some(description.into());
        }
        self
    }

    /// Add task context to an approval prompt (WHY field)
    pub fn with_approval_task_context(mut self, context: impl Into<String>) -> Self {
        if let Self::ApprovalPrompt { task_context, .. } = &mut self {
            *task_context = Some(context.into());
        }
        self
    }

    /// Add scope to an approval prompt (SCOPE field)
    pub fn with_approval_scope(mut self, scope: impl Into<String>) -> Self {
        if let Self::ApprovalPrompt { scope: s, .. } = &mut self {
            *s = Some(scope.into());
        }
        self
    }

    /// Add risk reasoning to an approval prompt (RISK field)
    pub fn with_approval_risk_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        if let Self::ApprovalPrompt { risk_reasoning, .. } = &mut self {
            *risk_reasoning = Some(reasoning.into());
        }
        self
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

    /// Create a thinking indicator entry
    pub fn thinking_indicator(duration_secs: f32) -> Self {
        Self::ThinkingIndicator { duration_secs }
    }

    /// Create a status line entry
    pub fn status_line(message: impl Into<String>, status_type: StatusType) -> Self {
        Self::StatusLine { message: message.into(), status_type }
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
            Self::PatchDisplay { .. } => "patch-display",
            Self::ApprovalPrompt { .. } => "approval-prompt",
            Self::SystemMessage { .. } => "system-message",
            Self::ErrorEntry { .. } => "error-entry",
            Self::ThinkingIndicator { .. } => "thinking-indicator",
            Self::StatusLine { .. } => "status-line",
        }
    }

    /// Get risk level color as ratatui Color
    pub fn risk_level_color(palette: crate::theme::ThemePalette, risk: &str) -> ratatui::style::Color {
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
        assert!(entry.to_string().contains("OK"));
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
        assert!(entry.to_string().contains("OK"));
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
        assert!(entry.to_string().contains("PENDING"));
        assert!(entry.is_approval_entry());
        assert!(!entry.is_tool_entry());
    }

    #[test]
    fn test_transcript_entry_approval_with_decision() {
        let entry =
            TranscriptEntry::approval_prompt("patch.feature", "risky").with_decision(ApprovalDecision::Approved);

        assert!(!entry.is_pending());
        assert!(entry.to_string().contains("APPROVED"));
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

        let palette = crate::theme::Theme::palette(crate::theme::ThemeVariant::Iceberg);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "safe"), palette.green);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "risky"), palette.yellow);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "dangerous"), palette.red);
        assert_eq!(TranscriptEntry::risk_level_color(palette, "unknown"), palette.muted);
    }

    #[test]
    fn test_risk_color_markers() {
        assert_eq!(TranscriptEntry::risk_emoji("safe"), "OK");
        assert_eq!(TranscriptEntry::risk_emoji("risky"), "!");
        assert_eq!(TranscriptEntry::risk_emoji("dangerous"), "X");
        assert_eq!(TranscriptEntry::risk_emoji("unknown"), "?");
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
        let entry = TranscriptEntry::error_entry("API error", ErrorType::Provider).with_can_retry(true);

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

    #[test]
    fn test_tool_call_with_teaching_context() {
        let entry = TranscriptEntry::tool_call("edit", "{path: '/test.rs'}", "risky")
            .with_description("Edit test file")
            .with_task_context("Fix authentication bug")
            .with_scope("/test.rs")
            .with_classification_reasoning("Modifies file which could break code");

        match &entry {
            TranscriptEntry::ToolCall {
                tool,
                risk,
                description,
                task_context,
                scope,
                classification_reasoning,
                ..
            } => {
                assert_eq!(tool, "edit");
                assert_eq!(risk, "risky");
                assert_eq!(description, &Some("Edit test file".to_string()));
                assert_eq!(task_context, &Some("Fix authentication bug".to_string()));
                assert_eq!(scope, &Some("/test.rs".to_string()));
                assert_eq!(
                    classification_reasoning,
                    &Some("Modifies file which could break code".to_string())
                );
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_tool_result_with_exit_code_and_next_steps() {
        let entry = TranscriptEntry::tool_result("shell", "Command failed", false)
            .with_exit_code(1)
            .with_next_steps(vec![
                "Check command syntax".to_string(),
                "Verify permissions".to_string(),
            ]);

        match &entry {
            TranscriptEntry::ToolResult { tool, success, exit_code, next_steps, .. } => {
                assert_eq!(tool, "shell");
                assert!(!success);
                assert_eq!(exit_code, &Some(1));
                assert_eq!(
                    next_steps,
                    &Some(vec![
                        "Check command syntax".to_string(),
                        "Verify permissions".to_string()
                    ])
                );
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_approval_prompt_with_teaching_context() {
        let entry = TranscriptEntry::approval_prompt("patch.feature", "risky")
            .with_approval_description("Apply feature patch")
            .with_approval_task_context("Implement user authentication")
            .with_approval_scope("src/auth/*.rs")
            .with_approval_risk_reasoning("Modifies authentication logic which is security-sensitive");

        match &entry {
            TranscriptEntry::ApprovalPrompt {
                action, risk, description, task_context, scope, risk_reasoning, ..
            } => {
                assert_eq!(action, "patch.feature");
                assert_eq!(risk, "risky");
                assert_eq!(description, &Some("Apply feature patch".to_string()));
                assert_eq!(task_context, &Some("Implement user authentication".to_string()));
                assert_eq!(scope, &Some("src/auth/*.rs".to_string()));
                assert_eq!(
                    risk_reasoning,
                    &Some("Modifies authentication logic which is security-sensitive".to_string())
                );
            }
            _ => panic!("Expected ApprovalPrompt"),
        }
    }

    #[test]
    fn test_tool_call_builder_methods_chain() {
        let entry = TranscriptEntry::tool_call("read", "{path: '/tmp/file'}", "safe")
            .with_description("Read temp file")
            .with_task_context("Debug file permissions")
            .with_scope("/tmp/file")
            .with_classification_reasoning("Read-only operation")
            .with_detail_level(CardDetailLevel::Verbose);

        assert_eq!(entry.detail_level(), CardDetailLevel::Verbose);
    }

    #[test]
    fn test_tool_result_builder_methods_chain() {
        let entry = TranscriptEntry::tool_result("grep", "Found 3 matches", true)
            .with_exit_code(0)
            .with_next_steps(vec!["Review matches".to_string()])
            .with_detail_level(CardDetailLevel::Detailed);

        assert_eq!(entry.detail_level(), CardDetailLevel::Detailed);
    }

    #[test]
    fn test_action_card_detail_levels_teaching_context() {
        let brief = TranscriptEntry::tool_call("edit", "{}", "risky").with_detail_level(CardDetailLevel::Brief);
        let detailed = TranscriptEntry::tool_call("edit", "{}", "risky").with_detail_level(CardDetailLevel::Detailed);
        let verbose = TranscriptEntry::tool_call("edit", "{}", "risky").with_detail_level(CardDetailLevel::Verbose);

        assert_eq!(brief.detail_level(), CardDetailLevel::Brief);
        assert_eq!(detailed.detail_level(), CardDetailLevel::Detailed);
        assert_eq!(verbose.detail_level(), CardDetailLevel::Verbose);
    }

    #[test]
    fn test_tool_call_serialization_with_teaching_context() {
        let entry = TranscriptEntry::tool_call("edit", "{}", "risky")
            .with_description("Test")
            .with_task_context("Fix bug")
            .with_scope("/test.rs")
            .with_classification_reasoning("Reasoning");

        match &entry {
            TranscriptEntry::ToolCall { tool, description, task_context, scope, classification_reasoning, .. } => {
                assert!(!tool.is_empty());
                assert!(description.is_some());
                assert!(task_context.is_some());
                assert!(scope.is_some());
                assert!(classification_reasoning.is_some());
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_teaching_context_fields_display_in_renderer() {
        let entry = TranscriptEntry::tool_call("read", "{path: '/test.rs'}", "safe")
            .with_description("Read file")
            .with_task_context("Understand code structure")
            .with_scope("/test.rs");

        match &entry {
            TranscriptEntry::ToolCall { description, task_context, scope, .. } => {
                assert_eq!(description, &Some("Read file".to_string()));
                assert_eq!(task_context, &Some("Understand code structure".to_string()));
                assert_eq!(scope, &Some("/test.rs".to_string()));
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[test]
    fn test_result_fields_display_in_renderer() {
        let entry = TranscriptEntry::tool_result("shell", "Output", true)
            .with_exit_code(0)
            .with_next_steps(vec!["Step 1".to_string(), "Step 2".to_string()]);

        match &entry {
            TranscriptEntry::ToolResult { exit_code, next_steps, .. } => {
                assert_eq!(exit_code, &Some(0));
                assert_eq!(next_steps, &Some(vec!["Step 1".to_string(), "Step 2".to_string()]));
            }
            _ => panic!("Expected ToolResult"),
        }
    }

    #[test]
    fn test_patch_display_creation() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let hunk_labels = vec![Some("Add test".to_string())];
        let entry = TranscriptEntry::patch_display("test patch", "test.txt", diff, hunk_labels);

        assert_eq!(entry.type_name(), "patch-display");
        assert!(entry.is_action_card());
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
    }

    #[test]
    fn test_patch_display_with_labels() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let hunk_labels = vec![
            Some("Add error handling".to_string()),
            Some("Update documentation".to_string()),
            None,
        ];
        let entry = TranscriptEntry::patch_display("multi-hunk patch", "test.txt", diff, hunk_labels);

        match &entry {
            TranscriptEntry::PatchDisplay { patch_name, file_path, hunk_labels, .. } => {
                assert_eq!(patch_name, "multi-hunk patch");
                assert_eq!(file_path, "test.txt");
                assert_eq!(hunk_labels.len(), 3);
                assert_eq!(hunk_labels[0], Some("Add error handling".to_string()));
                assert_eq!(hunk_labels[1], Some("Update documentation".to_string()));
                assert_eq!(hunk_labels[2], None);
            }
            _ => panic!("Expected PatchDisplay"),
        }
    }

    #[test]
    fn test_patch_display_detail_level() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let hunk_labels = vec![Some("Test".to_string())];
        let mut entry = TranscriptEntry::patch_display("test", "test.txt", diff, hunk_labels);

        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);

        entry.set_detail_level(CardDetailLevel::Verbose);
        assert_eq!(entry.detail_level(), CardDetailLevel::Verbose);

        entry.toggle_detail_level();
        assert_eq!(entry.detail_level(), CardDetailLevel::Brief);
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
    fn test_patch_display_empty_labels() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let hunk_labels = vec![None, None];
        let entry = TranscriptEntry::patch_display("no labels", "test.txt", diff, hunk_labels);

        match &entry {
            TranscriptEntry::PatchDisplay { hunk_labels, .. } => {
                assert_eq!(hunk_labels.len(), 2);
                assert!(hunk_labels[0].is_none());
                assert!(hunk_labels[1].is_none());
            }
            _ => panic!("Expected PatchDisplay"),
        }
    }
}
