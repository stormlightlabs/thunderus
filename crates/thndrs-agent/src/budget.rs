//! Provider-neutral tool-iteration budgeting for one agent turn.

/// Decision returned by [`ToolIterationBudget`] before a provider request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolBudgetDecision {
    /// The next provider request can proceed normally.
    Continue,
    /// The segment cap was reached and a provider-visible continuation
    /// message should be appended before proceeding.
    ContinueAfterBudgetMessage,
    /// The full per-turn tool budget has been exhausted.
    Exhausted {
        /// Batches in the final segment.
        segment_iterations: usize,
        /// All tool batches run in this turn.
        total_batches: usize,
        /// Continuations already consumed.
        continuations_used: usize,
    },
}

/// Tool-batch segment budget for one agent turn, with optional exhaustion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIterationBudget {
    segment_limit: usize,
    continuation_limit: Option<usize>,
    segment_iterations: usize,
    total_batches: usize,
    continuations_used: usize,
}

impl ToolIterationBudget {
    /// Create a budget with a segment limit and total continuation limit.
    pub fn new(segment_limit: usize, continuation_limit: usize) -> Self {
        Self {
            segment_limit,
            continuation_limit: Some(continuation_limit),
            segment_iterations: 0,
            total_batches: 0,
            continuations_used: 0,
        }
    }

    /// Create a budget that sends segment reminders without ending the turn.
    pub fn unbounded(segment_limit: usize) -> Self {
        Self {
            segment_limit,
            continuation_limit: None,
            segment_iterations: 0,
            total_batches: 0,
            continuations_used: 0,
        }
    }

    /// Record a completed provider-requested tool batch.
    pub fn record_tool_batch(&mut self) {
        self.segment_iterations = self.segment_iterations.saturating_add(1);
        self.total_batches = self.total_batches.saturating_add(1);
    }

    /// Decide whether the next provider request fits the remaining budget.
    pub fn before_provider_request(&mut self) -> ToolBudgetDecision {
        if self.segment_iterations < self.segment_limit {
            return ToolBudgetDecision::Continue;
        }

        if self
            .continuation_limit
            .is_none_or(|limit| self.continuations_used < limit)
        {
            self.segment_iterations = 0;
            self.continuations_used = self.continuations_used.saturating_add(1);
            return ToolBudgetDecision::ContinueAfterBudgetMessage;
        }

        ToolBudgetDecision::Exhausted {
            segment_iterations: self.segment_iterations,
            total_batches: self.total_batches,
            continuations_used: self.continuations_used,
        }
    }

    /// Return the total number of completed tool batches.
    pub const fn total_batches(&self) -> usize {
        self.total_batches
    }

    /// Return the number of consumed continuation segments.
    pub const fn continuations_used(&self) -> usize {
        self.continuations_used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_allows_bounded_continuations_before_exhausting() {
        let mut budget = ToolIterationBudget::new(2, 1);
        assert_eq!(budget.before_provider_request(), ToolBudgetDecision::Continue);
        budget.record_tool_batch();
        budget.record_tool_batch();
        assert_eq!(
            budget.before_provider_request(),
            ToolBudgetDecision::ContinueAfterBudgetMessage
        );
        budget.record_tool_batch();
        budget.record_tool_batch();
        assert_eq!(
            budget.before_provider_request(),
            ToolBudgetDecision::Exhausted { segment_iterations: 2, total_batches: 4, continuations_used: 1 }
        );
        assert_eq!(budget.total_batches(), 4);
        assert_eq!(budget.continuations_used(), 1);
    }

    #[test]
    fn unbounded_budget_continues_across_segment_boundaries() {
        let mut budget = ToolIterationBudget::unbounded(2);
        for continuation in 1..=12 {
            budget.record_tool_batch();
            budget.record_tool_batch();
            assert_eq!(
                budget.before_provider_request(),
                ToolBudgetDecision::ContinueAfterBudgetMessage
            );
            assert_eq!(budget.continuations_used(), continuation);
        }
    }
}
