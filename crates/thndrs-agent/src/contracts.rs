//! Provider-neutral run contracts shared by application adapters.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Status of an executed tool call.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool started, not yet finished.
    #[default]
    Running,
    /// Tool finished successfully.
    Ok,
    /// Tool failed.
    Failed,
    /// Tool was cancelled while running.
    Cancelled,
}

impl ToolStatus {
    /// Compact session/transcript label for a file-write result.
    pub const fn icon(self) -> &'static str {
        match self {
            ToolStatus::Ok => "✓ wrote",
            ToolStatus::Failed => "✕ write failed",
            ToolStatus::Running => "⠋ writing",
            ToolStatus::Cancelled => "✕ write cancelled",
        }
    }
}

/// Decision returned by an application-owned tool permission hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolPermissionDecision {
    /// The tool call may execute.
    Allow,
    /// The tool call must be rejected before execution.
    Reject,
    /// The prompt turn was cancelled while waiting for permission.
    Cancelled,
}

impl ToolPermissionDecision {
    /// Convert an application permission option identifier into a decision.
    pub fn from_option_id(option_id: &str) -> Self {
        if option_id.starts_with("allow") { Self::Allow } else { Self::Reject }
    }

    /// Convert an ACP permission option identifier into a decision.
    ///
    /// This compatibility spelling keeps ACP-specific naming at the adapter
    /// boundary while the decision itself remains protocol-neutral.
    pub fn from_acp_option_id(option_id: &str) -> Self {
        Self::from_option_id(option_id)
    }

    /// Stable outcome label for session records.
    pub const fn outcome_label(self) -> &'static str {
        match self {
            Self::Allow => "allowed",
            Self::Reject => "rejected",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Bounded exponential retry policy for provider requests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Number of retry attempts after the initial request.
    pub max_retries: u32,
    /// Delay before the first retry.
    pub base_delay: Duration,
}

impl RetryPolicy {
    /// Build a retry policy with the supplied attempt limit and initial delay.
    pub const fn new(max_retries: u32, base_delay: Duration) -> Self {
        Self { max_retries, base_delay }
    }

    /// Return the exponential-backoff delay for a one-based retry attempt.
    pub fn delay_for_attempt(self, attempt: u32) -> Duration {
        self.base_delay * 2u32.saturating_pow(attempt.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_option_mapping_is_conservative() {
        assert_eq!(
            ToolPermissionDecision::from_option_id("allow_once"),
            ToolPermissionDecision::Allow
        );
        assert_eq!(
            ToolPermissionDecision::from_option_id("deny"),
            ToolPermissionDecision::Reject
        );
    }

    #[test]
    fn retry_delays_double_per_attempt() {
        let policy = RetryPolicy::new(3, Duration::from_millis(25));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(25));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(100));
    }
}
