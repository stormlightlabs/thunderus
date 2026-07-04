//! ACP permission callback state.

use std::sync::mpsc::Sender;

use agent_client_protocol::schema::v1::{PermissionOptionKind, RequestPermissionRequest};

/// User decision for an ACP permission prompt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionDecision {
    /// The user cancelled the request or the run was interrupted.
    Cancelled,
    /// The user selected one of the agent-provided options.
    Selected(String),
}

/// One pending ACP permission request shown in the TUI.
#[derive(Clone, Debug)]
pub struct PendingPermission {
    /// ACP tool call id this request belongs to.
    pub tool_call_id: String,
    /// Tool title/name displayed by the agent.
    pub title: String,
    /// Stable option list supplied by the agent.
    pub options: Vec<PermissionOptionView>,
    /// Currently selected option index.
    pub selected: usize,
    pub(crate) responder: Sender<PermissionDecision>,
}

impl PendingPermission {
    /// Convert an ACP request into app state plus a response channel.
    pub fn from_request(request: &RequestPermissionRequest, responder: Sender<PermissionDecision>) -> Self {
        let title = request
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| request.tool_call.tool_call_id.to_string());
        let options = request
            .options
            .iter()
            .map(|option| PermissionOptionView {
                id: option.option_id.to_string(),
                name: option.name.clone(),
                kind: PermissionKindView::from(option.kind),
            })
            .collect();
        Self { tool_call_id: request.tool_call.tool_call_id.to_string(), title, options, selected: 0, responder }
    }

    /// Move the highlighted option up.
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlighted option down.
    pub fn move_down(&mut self) {
        self.selected = (self.selected + 1).min(self.options.len().saturating_sub(1));
    }

    /// Select the highlighted option and notify the ACP runner.
    pub fn select(&self) -> Option<PermissionDecision> {
        let option = self.options.get(self.selected)?;
        let decision = PermissionDecision::Selected(option.id.clone());
        let _ = self.responder.send(decision.clone());
        Some(decision)
    }

    /// Cancel the request and notify the ACP runner.
    pub fn cancel(&self) -> PermissionDecision {
        let decision = PermissionDecision::Cancelled;
        let _ = self.responder.send(decision.clone());
        decision
    }
}

impl PartialEq for PendingPermission {
    fn eq(&self, other: &Self) -> bool {
        self.tool_call_id == other.tool_call_id
            && self.title == other.title
            && self.options == other.options
            && self.selected == other.selected
    }
}

impl Eq for PendingPermission {}

/// Renderable ACP permission option.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionOptionView {
    /// ACP option id to return if selected.
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Option kind.
    pub kind: PermissionKindView,
}

/// Stable permission option kind for UI/session display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionKindView {
    /// Allow this operation once.
    AllowOnce,
    /// Always allow matching operation.
    AllowAlways,
    /// Reject this operation once.
    RejectOnce,
    /// Always reject matching operation.
    RejectAlways,
    /// Unknown future option kind.
    Unknown,
}

impl PermissionKindView {
    /// Lowercase label for display and persistence.
    pub fn label(self) -> &'static str {
        match self {
            PermissionKindView::AllowOnce => "allow once",
            PermissionKindView::AllowAlways => "allow always",
            PermissionKindView::RejectOnce => "reject once",
            PermissionKindView::RejectAlways => "reject always",
            PermissionKindView::Unknown => "unknown",
        }
    }
}

impl From<PermissionOptionKind> for PermissionKindView {
    fn from(value: PermissionOptionKind) -> Self {
        match value {
            PermissionOptionKind::AllowOnce => PermissionKindView::AllowOnce,
            PermissionOptionKind::AllowAlways => PermissionKindView::AllowAlways,
            PermissionOptionKind::RejectOnce => PermissionKindView::RejectOnce,
            PermissionOptionKind::RejectAlways => PermissionKindView::RejectAlways,
            _ => PermissionKindView::Unknown,
        }
    }
}
