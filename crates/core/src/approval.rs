use crate::classification::ToolRisk;
use crate::config::ApprovalMode;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;

/// Unique identifier for an approval request
pub type ApprovalId = u64;

/// The type of action requiring approval
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionType {
    /// Tool execution
    Tool,
    /// Shell command
    Shell,
    /// File write/edit
    FileWrite,
    /// File deletion
    FileDelete,
    /// Network request
    Network,
    /// Patch application
    Patch,
    /// Generic action
    Generic,
}

impl Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// A request for approval from the user
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalRequest {
    /// Unique identifier for this request
    pub id: ApprovalId,
    /// Type of action being requested
    pub action_type: ActionType,
    /// Brief description of the action
    pub description: String,
    /// Detailed context about the action
    pub context: ApprovalContext,
    /// Risk level of this action
    pub risk_level: ToolRisk,
    /// When this request was created
    pub created_at: String,
}

/// Context information for an approval request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalContext {
    /// Tool or command name
    pub name: Option<String>,
    /// Arguments or parameters
    pub arguments: Option<serde_json::Value>,
    /// Files or paths affected
    pub affected_paths: Vec<String>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Classification reasoning (if available)
    pub classification_reasoning: Option<String>,
}

impl ApprovalContext {
    pub fn new() -> Self {
        Self {
            name: None,
            arguments: None,
            affected_paths: Vec::new(),
            metadata: HashMap::new(),
            classification_reasoning: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_arguments(mut self, arguments: serde_json::Value) -> Self {
        self.arguments = Some(arguments);
        self
    }

    pub fn with_affected_paths(mut self, paths: Vec<String>) -> Self {
        self.affected_paths = paths;
        self
    }

    pub fn add_affected_path(mut self, path: impl Into<String>) -> Self {
        self.affected_paths.push(path.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn with_classification_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.classification_reasoning = Some(reasoning.into());
        self
    }
}

impl Default for ApprovalContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalRequest {
    /// Create a new approval request
    pub fn new(
        id: ApprovalId, action_type: ActionType, description: impl Into<String>, context: ApprovalContext,
        risk_level: ToolRisk,
    ) -> Self {
        let now = chrono::Utc::now();
        let created_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self { id, action_type, description: description.into(), context, risk_level, created_at }
    }

    /// Create a builder for ApprovalRequest
    pub fn builder() -> ApprovalRequestBuilder {
        ApprovalRequestBuilder::default()
    }
}

/// Builder for ApprovalRequest
#[derive(Default)]
pub struct ApprovalRequestBuilder {
    id: Option<ApprovalId>,
    action_type: Option<ActionType>,
    description: Option<String>,
    context: Option<ApprovalContext>,
    risk_level: Option<ToolRisk>,
}

impl ApprovalRequestBuilder {
    pub fn id(mut self, id: ApprovalId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn action_type(mut self, action_type: ActionType) -> Self {
        self.action_type = Some(action_type);
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn context(mut self, context: ApprovalContext) -> Self {
        self.context = Some(context);
        self
    }

    pub fn risk_level(mut self, risk_level: ToolRisk) -> Self {
        self.risk_level = Some(risk_level);
        self
    }

    pub fn build(self) -> Result<ApprovalRequest> {
        Ok(ApprovalRequest::new(
            self.id
                .ok_or_else(|| Error::Validation("approval request id is required".to_string()))?,
            self.action_type
                .ok_or_else(|| Error::Validation("action type is required".to_string()))?,
            self.description
                .ok_or_else(|| Error::Validation("description is required".to_string()))?,
            self.context.unwrap_or_default(),
            self.risk_level
                .ok_or_else(|| Error::Validation("risk level is required".to_string()))?,
        ))
    }
}

/// User's decision on an approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    /// User approved the action
    Approved,
    /// User rejected the action
    Rejected,
    /// User cancelled (no decision)
    Cancelled,
}

impl ApprovalDecision {
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved)
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected)
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// Response to an approval request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalResponse {
    /// ID of the request this responds to
    pub request_id: ApprovalId,
    /// User's decision
    pub decision: ApprovalDecision,
    /// Optional message from the user
    pub message: Option<String>,
    /// When this response was created
    pub created_at: String,
}

impl ApprovalResponse {
    /// Create a new approval response
    pub fn new(request_id: ApprovalId, decision: ApprovalDecision) -> Self {
        let now = chrono::Utc::now();
        let created_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Self { request_id, decision, message: None, created_at }
    }

    /// Create an approved response
    pub fn approved(request_id: ApprovalId) -> Self {
        Self::new(request_id, ApprovalDecision::Approved)
    }

    /// Create a rejected response
    pub fn rejected(request_id: ApprovalId) -> Self {
        Self::new(request_id, ApprovalDecision::Rejected)
    }

    /// Create a cancelled response
    pub fn cancelled(request_id: ApprovalId) -> Self {
        Self::new(request_id, ApprovalDecision::Cancelled)
    }

    /// Add a message to the response
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Approval gate that enforces policy based on ApprovalMode and ToolRisk
#[derive(Debug, Clone)]
pub struct ApprovalGate {
    /// Current approval mode
    mode: ApprovalMode,
    /// Whether network commands are allowed
    allow_network: bool,
    /// Next approval ID to assign
    next_id: ApprovalId,
    /// Track pending approvals
    pending: HashMap<ApprovalId, ApprovalRequest>,
    /// Track decision history
    history: Vec<ApprovalRecord>,
}

/// Record of an approval decision
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalRecord {
    /// The request that was decided
    pub request: ApprovalRequest,
    /// The decision made
    pub decision: ApprovalDecision,
    /// When the decision was made
    pub decided_at: String,
}

impl ApprovalGate {
    /// Create a new approval gate
    pub fn new(mode: ApprovalMode, allow_network: bool) -> Self {
        Self { mode, allow_network, next_id: 0, pending: HashMap::new(), history: Vec::new() }
    }

    /// Check if an action requires approval based on mode and risk
    pub fn requires_approval(&self, risk_level: ToolRisk, is_network: bool) -> bool {
        match self.mode {
            ApprovalMode::ReadOnly => true,
            ApprovalMode::Auto => risk_level.is_risky() || is_network,
            ApprovalMode::FullAccess => false,
        }
    }

    /// Check if approval is required, considering network permissions
    pub fn check_requires_approval(&self, risk_level: ToolRisk, action_type: &ActionType) -> bool {
        let is_network = matches!(action_type, ActionType::Network);
        let network_allowed = is_network && self.allow_network;

        if network_allowed && self.mode == ApprovalMode::Auto && risk_level.is_safe() {
            return false;
        }

        self.requires_approval(risk_level, is_network)
    }

    /// Create an approval request and return its ID
    pub fn create_request(
        &mut self, action_type: ActionType, description: impl Into<String>, context: ApprovalContext,
        risk_level: ToolRisk,
    ) -> ApprovalId {
        let id = self.next_id;
        self.next_id += 1;

        let request = ApprovalRequest::new(id, action_type, description, context, risk_level);
        self.pending.insert(id, request.clone());
        id
    }

    /// Get a pending request by ID
    pub fn get_request(&self, id: ApprovalId) -> Option<&ApprovalRequest> {
        self.pending.get(&id)
    }

    /// Get all pending requests
    pub fn pending_requests(&self) -> Vec<&ApprovalRequest> {
        self.pending.values().collect()
    }

    /// Count of pending requests
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Record an approval decision
    pub fn record_decision(&mut self, response: ApprovalResponse) -> Result<()> {
        let request = self
            .pending
            .remove(&response.request_id)
            .ok_or_else(|| Error::Validation(format!("approval request {} not found", response.request_id)))?;

        let now = chrono::Utc::now();
        let decided_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let record = ApprovalRecord { request, decision: response.decision, decided_at };
        self.history.push(record);

        Ok(())
    }

    /// Approve a pending request
    pub fn approve(&mut self, request_id: ApprovalId) -> Result<()> {
        self.record_decision(ApprovalResponse::approved(request_id))
    }

    /// Reject a pending request
    pub fn reject(&mut self, request_id: ApprovalId) -> Result<()> {
        self.record_decision(ApprovalResponse::rejected(request_id))
    }

    /// Cancel a pending request
    pub fn cancel(&mut self, request_id: ApprovalId) -> Result<()> {
        self.record_decision(ApprovalResponse::cancelled(request_id))
    }

    /// Get approval history
    pub fn history(&self) -> &[ApprovalRecord] {
        &self.history
    }

    /// Get approval mode
    pub fn mode(&self) -> ApprovalMode {
        self.mode
    }

    /// Set approval mode
    pub fn set_mode(&mut self, mode: ApprovalMode) {
        self.mode = mode;
    }

    /// Check if network is allowed
    pub fn allow_network(&self) -> bool {
        self.allow_network
    }

    /// Set network allowance
    pub fn set_allow_network(&mut self, allow: bool) {
        self.allow_network = allow;
    }

    /// Get decision statistics
    pub fn stats(&self) -> ApprovalStats {
        let approved = self.history.iter().filter(|r| r.decision.is_approved()).count();
        let rejected = self.history.iter().filter(|r| r.decision.is_rejected()).count();
        let cancelled = self.history.iter().filter(|r| r.decision.is_cancelled()).count();

        ApprovalStats { total: self.history.len(), approved, rejected, cancelled, pending: self.pending.len() }
    }
}

/// Statistics about approvals
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalStats {
    pub total: usize,
    pub approved: usize,
    pub rejected: usize,
    pub cancelled: usize,
    pub pending: usize,
}

/// Trait for pluggable approval backends (e.g., CLI, TUI, auto)
pub trait ApprovalProtocol: Send + Sync {
    /// Request approval for an action
    fn request_approval(&self, request: &ApprovalRequest) -> Result<ApprovalDecision>;

    /// Get the name of this protocol
    fn name(&self) -> &str;
}

/// Auto-approve protocol (for FullAccess mode)
#[derive(Debug)]
pub struct AutoApprove;

impl AutoApprove {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AutoApprove {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalProtocol for AutoApprove {
    fn request_approval(&self, _request: &ApprovalRequest) -> Result<ApprovalDecision> {
        Ok(ApprovalDecision::Approved)
    }

    fn name(&self) -> &str {
        "auto-approve"
    }
}

/// Auto-reject protocol (for ReadOnly mode)
#[derive(Debug)]
pub struct AutoReject;

impl AutoReject {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AutoReject {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalProtocol for AutoReject {
    fn request_approval(&self, _request: &ApprovalRequest) -> Result<ApprovalDecision> {
        Ok(ApprovalDecision::Rejected)
    }

    fn name(&self) -> &str {
        "auto-reject"
    }
}

/// Interactive protocol (for TUI/CLI - placeholder for future implementation)
#[derive(Debug)]
pub struct Interactive;

impl Interactive {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Interactive {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalProtocol for Interactive {
    fn request_approval(&self, _request: &ApprovalRequest) -> Result<ApprovalDecision> {
        Err(Error::Other("Interactive approval not yet implemented".to_string()))
    }

    fn name(&self) -> &str {
        "interactive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_context() -> ApprovalContext {
        ApprovalContext::new()
            .with_name("test_tool")
            .with_arguments(serde_json::json!({"arg": "value"}))
            .add_affected_path("/tmp/test.txt")
            .with_metadata("key", "value")
            .with_classification_reasoning("Test is safe because it only reads")
    }

    #[test]
    fn test_approval_context_builder() {
        let ctx = ApprovalContext::new()
            .with_name("tool")
            .with_arguments(serde_json::json!({"key": "value"}))
            .add_affected_path("/path/1")
            .add_affected_path("/path/2")
            .with_metadata("meta", "data")
            .with_classification_reasoning("reasoning");

        assert_eq!(ctx.name, Some("tool".to_string()));
        assert_eq!(ctx.arguments, Some(serde_json::json!({"key": "value"})));
        assert_eq!(ctx.affected_paths.len(), 2);
        assert_eq!(ctx.metadata.get("meta"), Some(&"data".to_string()));
        assert_eq!(ctx.classification_reasoning, Some("reasoning".to_string()));
    }

    #[test]
    fn test_approval_request_builder() {
        let ctx = create_test_context();
        let request = ApprovalRequest::builder()
            .id(1)
            .action_type(ActionType::Tool)
            .description("Test tool execution")
            .context(ctx)
            .risk_level(ToolRisk::Safe)
            .build()
            .unwrap();

        assert_eq!(request.id, 1);
        assert_eq!(request.action_type, ActionType::Tool);
        assert_eq!(request.description, "Test tool execution");
        assert_eq!(request.risk_level, ToolRisk::Safe);
        assert!(!request.created_at.is_empty());
    }

    #[test]
    fn test_approval_request_builder_missing_id() {
        let result = ApprovalRequest::builder()
            .action_type(ActionType::Tool)
            .description("Test")
            .context(ApprovalContext::new())
            .risk_level(ToolRisk::Safe)
            .build();

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn test_approval_request_builder_missing_action_type() {
        let result = ApprovalRequest::builder()
            .id(1)
            .description("Test")
            .context(ApprovalContext::new())
            .risk_level(ToolRisk::Safe)
            .build();

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn test_approval_request_serialization() {
        let ctx = create_test_context();
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ctx, ToolRisk::Safe);

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"tool\""));
        assert!(json.contains("\"Safe\""));

        let deserialized: ApprovalRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 1);
        assert_eq!(deserialized.action_type, ActionType::Tool);
        assert_eq!(deserialized.risk_level, ToolRisk::Safe);
    }

    #[test]
    fn test_approval_decision_is_methods() {
        assert!(ApprovalDecision::Approved.is_approved());
        assert!(!ApprovalDecision::Approved.is_rejected());
        assert!(!ApprovalDecision::Approved.is_cancelled());

        assert!(!ApprovalDecision::Rejected.is_approved());
        assert!(ApprovalDecision::Rejected.is_rejected());
        assert!(!ApprovalDecision::Rejected.is_cancelled());

        assert!(!ApprovalDecision::Cancelled.is_approved());
        assert!(!ApprovalDecision::Cancelled.is_rejected());
        assert!(ApprovalDecision::Cancelled.is_cancelled());
    }

    #[test]
    fn test_approval_response_factory_methods() {
        let approved = ApprovalResponse::approved(1);
        assert_eq!(approved.request_id, 1);
        assert_eq!(approved.decision, ApprovalDecision::Approved);

        let rejected = ApprovalResponse::rejected(2);
        assert_eq!(rejected.request_id, 2);
        assert_eq!(rejected.decision, ApprovalDecision::Rejected);

        let cancelled = ApprovalResponse::cancelled(3);
        assert_eq!(cancelled.request_id, 3);
        assert_eq!(cancelled.decision, ApprovalDecision::Cancelled);
    }

    #[test]
    fn test_approval_response_with_message() {
        let response = ApprovalResponse::approved(1).with_message("Proceed with caution");
        assert_eq!(response.message, Some("Proceed with caution".to_string()));
    }

    #[test]
    fn test_approval_response_serialization() {
        let response = ApprovalResponse::approved(1).with_message("OK");

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"request_id\":1"));
        assert!(json.contains("\"approved\""));
        assert!(json.contains("\"OK\""));

        let deserialized: ApprovalResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.request_id, 1);
        assert_eq!(deserialized.decision, ApprovalDecision::Approved);
        assert_eq!(deserialized.message, Some("OK".to_string()));
    }

    #[test]
    fn test_approval_gate_read_only_mode() {
        let gate = ApprovalGate::new(ApprovalMode::ReadOnly, false);
        assert!(gate.requires_approval(ToolRisk::Safe, false));
        assert!(gate.requires_approval(ToolRisk::Risky, false));
        assert!(gate.requires_approval(ToolRisk::Safe, true));
    }

    #[test]
    fn test_approval_gate_auto_mode() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);
        assert!(!gate.requires_approval(ToolRisk::Safe, false));
        assert!(gate.requires_approval(ToolRisk::Risky, false));
        assert!(gate.requires_approval(ToolRisk::Safe, true));
        assert!(gate.requires_approval(ToolRisk::Risky, true));
    }

    #[test]
    fn test_approval_gate_auto_mode_with_network() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, true);
        assert!(!gate.check_requires_approval(ToolRisk::Safe, &ActionType::Network));
        assert!(gate.check_requires_approval(ToolRisk::Risky, &ActionType::Network));
    }

    #[test]
    fn test_approval_gate_full_access_mode() {
        let gate = ApprovalGate::new(ApprovalMode::FullAccess, false);
        assert!(!gate.requires_approval(ToolRisk::Safe, false));
        assert!(!gate.requires_approval(ToolRisk::Risky, false));
        assert!(!gate.requires_approval(ToolRisk::Safe, true));
    }

    #[test]
    fn test_approval_gate_check_requires_approval() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);

        assert!(!gate.check_requires_approval(ToolRisk::Safe, &ActionType::Tool));
        assert!(gate.check_requires_approval(ToolRisk::Risky, &ActionType::Tool));
        assert!(gate.check_requires_approval(ToolRisk::Safe, &ActionType::Network));
    }

    #[test]
    fn test_approval_gate_create_request() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let ctx = create_test_context();

        let id = gate.create_request(ActionType::Tool, "Test tool", ctx, ToolRisk::Safe);

        assert_eq!(id, 0);
        assert_eq!(gate.pending_count(), 1);
        assert!(gate.get_request(id).is_some());
    }

    #[test]
    fn test_approval_gate_multiple_requests() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        let id1 = gate.create_request(ActionType::Tool, "First", ApprovalContext::new(), ToolRisk::Safe);
        let id2 = gate.create_request(ActionType::Shell, "Second", ApprovalContext::new(), ToolRisk::Risky);
        let id3 = gate.create_request(ActionType::Patch, "Third", ApprovalContext::new(), ToolRisk::Safe);

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(id3, 2);
        assert_eq!(gate.pending_count(), 3);
    }

    #[test]
    fn test_approval_gate_approve() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let id = gate.create_request(ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);

        gate.approve(id).unwrap();

        assert_eq!(gate.pending_count(), 0);
        assert_eq!(gate.history().len(), 1);
        assert!(gate.get_request(id).is_none());
        assert_eq!(gate.history()[0].decision, ApprovalDecision::Approved);
    }

    #[test]
    fn test_approval_gate_reject() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let id = gate.create_request(ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Risky);

        gate.reject(id).unwrap();

        assert_eq!(gate.pending_count(), 0);
        assert_eq!(gate.history().len(), 1);
        assert_eq!(gate.history()[0].decision, ApprovalDecision::Rejected);
    }

    #[test]
    fn test_approval_gate_cancel() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        let id = gate.create_request(ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);

        gate.cancel(id).unwrap();

        assert_eq!(gate.pending_count(), 0);
        assert_eq!(gate.history().len(), 1);
        assert_eq!(gate.history()[0].decision, ApprovalDecision::Cancelled);
    }

    #[test]
    fn test_approval_gate_decision_on_nonexistent_request() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        let result = gate.approve(999);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Validation(_)));
    }

    #[test]
    fn test_approval_gate_stats() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        let id1 = gate.create_request(ActionType::Tool, "First", ApprovalContext::new(), ToolRisk::Safe);
        let id2 = gate.create_request(ActionType::Tool, "Second", ApprovalContext::new(), ToolRisk::Safe);
        let id3 = gate.create_request(ActionType::Tool, "Third", ApprovalContext::new(), ToolRisk::Safe);

        gate.approve(id1).unwrap();
        gate.reject(id2).unwrap();
        gate.cancel(id3).unwrap();

        let stats = gate.stats();
        assert_eq!(stats.total, 3);
        assert_eq!(stats.approved, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.cancelled, 1);
        assert_eq!(stats.pending, 0);
    }

    #[test]
    fn test_approval_gate_stats_with_pending() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        gate.create_request(ActionType::Tool, "First", ApprovalContext::new(), ToolRisk::Safe);
        gate.create_request(ActionType::Tool, "Second", ApprovalContext::new(), ToolRisk::Risky);

        let stats = gate.stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.approved, 0);
        assert_eq!(stats.rejected, 0);
        assert_eq!(stats.cancelled, 0);
        assert_eq!(stats.pending, 2);
    }

    #[test]
    fn test_approval_gate_mode_getter_setter() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        assert_eq!(gate.mode(), ApprovalMode::Auto);

        gate.set_mode(ApprovalMode::ReadOnly);
        assert_eq!(gate.mode(), ApprovalMode::ReadOnly);
    }

    #[test]
    fn test_approval_gate_allow_network_getter_setter() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);

        assert!(!gate.allow_network());

        gate.set_allow_network(true);
        assert!(gate.allow_network());
    }

    #[test]
    fn test_approval_gate_pending_requests() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        gate.create_request(ActionType::Tool, "First", ApprovalContext::new(), ToolRisk::Safe);
        gate.create_request(ActionType::Shell, "Second", ApprovalContext::new(), ToolRisk::Risky);

        let pending = gate.pending_requests();
        assert_eq!(pending.len(), 2);

        let descriptions: Vec<&str> = pending.iter().map(|r| r.description.as_str()).collect();
        assert!(descriptions.contains(&"First"));
        assert!(descriptions.contains(&"Second"));
    }

    #[test]
    fn test_auto_approve_protocol() {
        let protocol = AutoApprove::new();
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Risky);

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Approved);
        assert_eq!(protocol.name(), "auto-approve");
    }

    #[test]
    fn test_auto_reject_protocol() {
        let protocol = AutoReject::new();
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);

        let decision = protocol.request_approval(&request).unwrap();
        assert_eq!(decision, ApprovalDecision::Rejected);
        assert_eq!(protocol.name(), "auto-reject");
    }

    #[test]
    fn test_interactive_protocol_placeholder() {
        let protocol = Interactive::new();
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);

        let result = protocol.request_approval(&request);
        assert!(result.is_err());
        assert_eq!(protocol.name(), "interactive");
    }

    #[test]
    fn test_action_type_serialization() {
        let types = vec![
            ActionType::Tool,
            ActionType::Shell,
            ActionType::FileWrite,
            ActionType::FileDelete,
            ActionType::Network,
            ActionType::Patch,
            ActionType::Generic,
        ];

        for action_type in &types {
            let json = serde_json::to_string(action_type).unwrap();
            let deserialized: ActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(action_type, &deserialized);
        }
    }

    #[test]
    fn test_approval_record_serialization() {
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);
        let record = ApprovalRecord {
            request: request.clone(),
            decision: ApprovalDecision::Approved,
            decided_at: "2025-01-12T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ApprovalRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.request.id, 1);
        assert_eq!(deserialized.decision, ApprovalDecision::Approved);
        assert_eq!(deserialized.decided_at, "2025-01-12T00:00:00Z");
    }

    #[test]
    fn test_approval_stats_equality() {
        let stats1 = ApprovalStats { total: 10, approved: 5, rejected: 3, cancelled: 2, pending: 1 };
        let stats2 = ApprovalStats { total: 10, approved: 5, rejected: 3, cancelled: 2, pending: 1 };
        let stats3 = ApprovalStats { total: 10, approved: 5, rejected: 3, cancelled: 2, pending: 0 };

        assert_eq!(stats1, stats2);
        assert_ne!(stats1, stats3);
    }

    #[test]
    fn test_approval_context_default() {
        let ctx = ApprovalContext::default();
        assert_eq!(ctx.name, None);
        assert_eq!(ctx.arguments, None);
        assert!(ctx.affected_paths.is_empty());
        assert!(ctx.metadata.is_empty());
        assert_eq!(ctx.classification_reasoning, None);
    }

    #[test]
    fn test_approval_request_created_at_format() {
        let request = ApprovalRequest::new(1, ActionType::Tool, "Test", ApprovalContext::new(), ToolRisk::Safe);
        let parsed: chrono::DateTime<chrono::Utc> = chrono::DateTime::parse_from_rfc3339(&request.created_at)
            .unwrap()
            .into();
        assert!(parsed.timestamp() > 0);
    }

    #[test]
    fn test_multiple_decisions_tracking() {
        let mut gate = ApprovalGate::new(ApprovalMode::Auto, false);
        for i in 0..5 {
            let id = gate.create_request(
                ActionType::Tool,
                format!("Request {}", i),
                ApprovalContext::new(),
                ToolRisk::Safe,
            );
            if i % 2 == 0 {
                gate.approve(id).unwrap();
            } else {
                gate.reject(id).unwrap();
            }
        }

        let history = gate.history();
        assert_eq!(history.len(), 5);

        let approved_count = history.iter().filter(|r| r.decision.is_approved()).count();
        let rejected_count = history.iter().filter(|r| r.decision.is_rejected()).count();

        assert_eq!(approved_count, 3);
        assert_eq!(rejected_count, 2);
    }

    #[test]
    fn test_action_type_network_with_allow_network() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, true);
        assert!(!gate.check_requires_approval(ToolRisk::Safe, &ActionType::Network));
        assert!(gate.check_requires_approval(ToolRisk::Risky, &ActionType::Network));
    }

    #[test]
    fn test_action_type_all_variants() {
        let gate = ApprovalGate::new(ApprovalMode::Auto, false);

        let types = vec![
            ActionType::Tool,
            ActionType::Shell,
            ActionType::FileWrite,
            ActionType::FileDelete,
            ActionType::Network,
            ActionType::Patch,
            ActionType::Generic,
        ];

        for action_type in types {
            assert!(gate.check_requires_approval(ToolRisk::Risky, &action_type));
        }
    }
}
