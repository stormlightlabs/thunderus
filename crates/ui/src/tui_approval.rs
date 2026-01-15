//! TUI-based approval protocol for agent integration
//!
//! This module provides a bridge between the agent's approval requests and the TUI's
//! user input handling, using channels to communicate across the async/sync boundary.

use std::sync::{Arc, Mutex};
use thunderus_core::{ApprovalDecision, ApprovalProtocol, ApprovalRequest, Error, Result};
use tokio::sync::{mpsc, oneshot};

/// TUI approval protocol that bridges async agent and sync TUI
///
/// When the agent requests approval, it sends the request to a channel and
/// synchronously waits for a response from the TUI event loop.
pub struct TuiApprovalProtocol {
    /// Sender for approval requests (agent → TUI)
    request_tx: mpsc::UnboundedSender<ApprovalRequest>,
    /// Pending approval responses (request_id → response sender)
    pending_responses: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<ApprovalDecision>>>>,
}

impl TuiApprovalProtocol {
    /// Create a new TUI approval protocol
    ///
    /// Returns the protocol and a receiver for approval requests that should
    /// be polled in the TUI event loop.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<ApprovalRequest>) {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let protocol = Self { request_tx, pending_responses: Arc::new(Mutex::new(std::collections::HashMap::new())) };
        (protocol, request_rx)
    }

    /// Send an approval response for a pending request
    ///
    /// Called by the TUI event loop when the user responds to an approval prompt.
    pub fn respond(&self, request_id: u64, decision: ApprovalDecision) -> bool {
        let mut responses = self.pending_responses.lock().unwrap();
        if let Some(tx) = responses.remove(&request_id) { tx.send(decision).is_ok() } else { false }
    }

    /// Get the number of pending approval responses
    pub fn pending_count(&self) -> usize {
        self.pending_responses.lock().unwrap().len()
    }
}

impl ApprovalProtocol for TuiApprovalProtocol {
    fn request_approval(&self, request: &ApprovalRequest) -> Result<ApprovalDecision> {
        self.request_tx
            .send(request.clone())
            .map_err(|e| Error::Approval(format!("{:?}", e)))?;

        let (tx, rx) = oneshot::channel();

        {
            let mut responses = self.pending_responses.lock().unwrap();
            responses.insert(request.id, tx);
        }

        match rx.blocking_recv() {
            Ok(decision) => Ok(decision),
            Err(_) => Ok(ApprovalDecision::Cancelled),
        }
    }

    fn name(&self) -> &str {
        "tui"
    }
}

/// Cloneable handle to the TUI approval protocol for sending responses
#[derive(Clone)]
pub struct TuiApprovalHandle {
    pending_responses: Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<ApprovalDecision>>>>,
}

impl TuiApprovalHandle {
    /// Create a new handle from a protocol
    pub fn from_protocol(protocol: &TuiApprovalProtocol) -> Self {
        Self { pending_responses: Arc::clone(&protocol.pending_responses) }
    }

    /// Send an approval response
    pub fn respond(&self, request_id: u64, decision: ApprovalDecision) -> bool {
        let mut responses = self.pending_responses.lock().unwrap();
        if let Some(tx) = responses.remove(&request_id) { tx.send(decision).is_ok() } else { false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    use thunderus_core::{ActionType, ApprovalContext, ToolRisk};

    #[test]
    fn test_tui_approval_protocol_creation() {
        let _ = TuiApprovalProtocol::new();
    }

    #[test]
    fn test_tui_approval_protocol_request_response() {
        let (protocol, mut request_rx) = TuiApprovalProtocol::new();

        let request = ApprovalRequest {
            id: 1,
            action_type: ActionType::Tool,
            description: "Test tool".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let handle = TuiApprovalHandle::from_protocol(&protocol);
        thread::spawn(move || {
            let req = request_rx.blocking_recv().unwrap();
            assert_eq!(req.id, 1);

            handle.respond(req.id, ApprovalDecision::Approved);
        });

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Approved);
    }

    #[test]
    fn test_tui_approval_rejection() {
        let (protocol, mut request_rx) = TuiApprovalProtocol::new();

        let request = ApprovalRequest {
            id: 2,
            action_type: ActionType::Tool,
            description: "Risky tool".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Risky,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let handle = TuiApprovalHandle::from_protocol(&protocol);
        thread::spawn(move || {
            let _ = request_rx.blocking_recv();
            handle.respond(2, ApprovalDecision::Rejected);
        });

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Rejected);
    }

    #[test]
    fn test_tui_approval_cancellation() {
        let (protocol, mut request_rx) = TuiApprovalProtocol::new();

        let request = ApprovalRequest {
            id: 3,
            action_type: ActionType::Tool,
            description: "Cancel test".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let handle = TuiApprovalHandle::from_protocol(&protocol);
        thread::spawn(move || {
            let _ = request_rx.blocking_recv();
            handle.respond(3, ApprovalDecision::Cancelled);
        });

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Cancelled);
    }

    #[test]
    fn test_tui_approval_timeout() {
        let (protocol, _request_rx) = TuiApprovalProtocol::new();

        let request = ApprovalRequest {
            id: 4,
            action_type: ActionType::Tool,
            description: "Timeout test".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let handle = TuiApprovalHandle::from_protocol(&protocol);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            let _ = handle.respond(4, ApprovalDecision::Cancelled);
        });

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Cancelled);
    }

    #[test]
    fn test_pending_count() {
        let (protocol, mut request_rx) = TuiApprovalProtocol::new();

        assert_eq!(protocol.pending_count(), 0);

        let request1 = ApprovalRequest {
            id: 5,
            action_type: ActionType::Tool,
            description: "Test 1".to_string(),
            context: ApprovalContext::new(),
            risk_level: ToolRisk::Safe,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        thread::spawn(move || {
            let _ = request_rx.blocking_recv();
            thread::sleep(Duration::from_millis(100));
        });

        let request_tx = protocol.request_tx.clone();
        let pending_responses = protocol.pending_responses.clone();
        thread::spawn(move || {
            let protocol_clone = TuiApprovalProtocol { request_tx, pending_responses };
            let _ = protocol_clone.request_approval(&request1);
        });

        thread::sleep(Duration::from_millis(10));

        assert_eq!(protocol.pending_count(), 1);
    }
}
