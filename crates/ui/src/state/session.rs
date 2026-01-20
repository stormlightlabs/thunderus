use thunderus_core::{Patch, TokensUsed};

/// Session statistics for the UI
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Total input tokens used
    pub input_tokens: u32,
    /// Total output tokens used
    pub output_tokens: u32,
    /// Number of approval gates triggered
    pub approval_gates: u32,
    /// Number of tools executed
    pub tools_executed: u32,
}

impl SessionStats {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }

    pub fn add_tokens(&mut self, tokens: &TokensUsed) {
        self.input_tokens += tokens.input;
        self.output_tokens += tokens.output;
    }

    pub fn increment_approval_gate(&mut self) {
        self.approval_gates += 1;
    }

    pub fn increment_tools_executed(&mut self) {
        self.tools_executed += 1;
    }
}

/// Session tracking data
#[derive(Debug, Clone)]
pub struct SessionTrackingState {
    /// Session statistics
    pub stats: SessionStats,
    /// Session events for sidebar
    pub session_events: Vec<super::SessionEvent>,
    /// Modified files list
    pub modified_files: Vec<super::ModifiedFile>,
    /// Git diff queue
    pub git_diff_queue: Vec<super::GitDiff>,
    /// Patches in the queue
    pub patches: Vec<Patch>,
    /// Last user message sent (for retry functionality)
    pub last_message: Option<String>,
}

impl SessionTrackingState {
    pub fn new() -> Self {
        Self {
            stats: SessionStats::default(),
            session_events: Vec::new(),
            modified_files: Vec::new(),
            git_diff_queue: Vec::new(),
            patches: Vec::new(),
            last_message: None,
        }
    }
}

impl Default for SessionTrackingState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_stats() {
        let mut stats = SessionStats::default();

        let tokens = TokensUsed::new(10, 20);
        stats.add_tokens(&tokens);

        assert_eq!(stats.input_tokens, 10);
        assert_eq!(stats.output_tokens, 20);
        assert_eq!(stats.total_tokens(), 30);

        stats.increment_approval_gate();
        assert_eq!(stats.approval_gates, 1);

        stats.increment_tools_executed();
        assert_eq!(stats.tools_executed, 1);
    }
}
