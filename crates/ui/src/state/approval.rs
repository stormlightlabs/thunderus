/// Approval state for pending approvals
#[derive(Debug, Clone)]
pub struct ApprovalState {
    /// Request ID for tracking the approval
    pub request_id: Option<u64>,
    /// Pending approval action
    pub action: String,
    /// Risk level
    pub risk: String,
    /// Description
    pub description: Option<String>,
    /// User's decision (Some(true) = approved, Some(false) = rejected, None = pending)
    pub decision: Option<bool>,
}

impl ApprovalState {
    pub fn pending(action: String, risk: String) -> Self {
        Self { request_id: None, action, risk, description: None, decision: None }
    }

    pub fn with_request_id(mut self, request_id: u64) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn is_pending(&self) -> bool {
        self.decision.is_none()
    }

    pub fn approve(&mut self) {
        self.decision = Some(true);
    }

    pub fn reject(&mut self) {
        self.decision = Some(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_state() {
        let mut approval = ApprovalState::pending("patch.feature".to_string(), "risky".to_string());

        assert!(approval.is_pending());
        assert!(approval.decision.is_none());
        assert_eq!(approval.request_id, None);

        approval.approve();
        assert!(!approval.is_pending());
        assert_eq!(approval.decision, Some(true));

        let mut approval2 = ApprovalState::pending("delete.file".to_string(), "dangerous".to_string());
        approval2.reject();
        assert_eq!(approval2.decision, Some(false));
    }

    #[test]
    fn test_approval_state_with_request_id() {
        let approval = ApprovalState::pending("test.action".to_string(), "safe".to_string()).with_request_id(123);

        assert_eq!(approval.request_id, Some(123));
        assert_eq!(approval.action, "test.action");
        assert_eq!(approval.risk, "safe");
    }
}
