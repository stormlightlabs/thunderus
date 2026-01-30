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
}
