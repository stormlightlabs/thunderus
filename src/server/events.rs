//! Pure mapping from app events to ACP server notification intents.
//!
//! The intents are intentionally small and protocol-agnostic so handlers can
//! lower them to SDK `SessionNotification` values without importing transport
//! details into event conversion.

use crate::app::{AgentEvent, ToolStatus};

/// Intent payload emitted by the ACP server for one `session/update` event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionUpdateIntent {
    /// Assistant-facing text chunk.
    AssistantDelta(String),
    /// Reasoning/thinking text chunk.
    ReasoningDelta(String),
    /// Generic status update for UIs or logs.
    Status(String),
    /// Token usage snapshot.
    Usage {
        /// Input tokens observed since the session started.
        input_tokens: u64,
        /// Output tokens observed since the session started.
        output_tokens: u64,
    },
    /// Tool call created/in-progress event.
    ToolStarted {
        /// Tool-call correlation id.
        id: String,
        /// Human-readable tool name/title.
        name: String,
        /// Human-readable argument payload.
        arguments: String,
    },
    /// Tool call completion event.
    ToolFinished {
        /// Tool-call correlation id.
        id: String,
        /// Tool result status.
        status: ToolStatusIntent,
        /// Tool output lines.
        output: Vec<String>,
    },
    /// Prompt turn failed with terminal error details.
    Failed(String),
    /// Prompt turn was cancelled.
    Cancelled,
    /// Prompt turn finished normally.
    Finished,
}

/// ACP-facing tool status intent used by [`SessionUpdateIntent::ToolFinished`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolStatusIntent {
    /// Tool execution is still running or awaiting client permission.
    InProgress,
    /// Tool execution completed successfully.
    Completed,
    /// Tool execution failed.
    Failed,
    /// Tool execution was cancelled.
    Cancelled,
}

impl From<ToolStatus> for ToolStatusIntent {
    fn from(status: ToolStatus) -> Self {
        match status {
            ToolStatus::Ok => Self::Completed,
            ToolStatus::Failed => Self::Failed,
            ToolStatus::Cancelled => Self::Cancelled,
            ToolStatus::Running => Self::InProgress,
        }
    }
}

/// Convert one app event into zero or more ACP session-update intents.
///
/// Returns an empty list for internal events that are not visible protocol
/// updates.
pub fn map_agent_event(event: &AgentEvent) -> Vec<SessionUpdateIntent> {
    match event {
        AgentEvent::AssistantDelta(text) => vec![SessionUpdateIntent::AssistantDelta(text.clone())],
        AgentEvent::ReasoningDelta(text) => vec![SessionUpdateIntent::ReasoningDelta(text.clone())],
        AgentEvent::Status(text) => vec![SessionUpdateIntent::Status(text.clone())],
        AgentEvent::Usage { input_tokens, output_tokens } => {
            vec![SessionUpdateIntent::Usage { input_tokens: *input_tokens, output_tokens: *output_tokens }]
        }
        AgentEvent::ToolStarted { id, name, arguments } => {
            vec![SessionUpdateIntent::ToolStarted { id: id.clone(), name: name.clone(), arguments: arguments.clone() }]
        }
        AgentEvent::ToolFinished { id, status, output, .. } => {
            vec![SessionUpdateIntent::ToolFinished { id: id.clone(), status: (*status).into(), output: output.clone() }]
        }
        AgentEvent::Failed(message) => vec![SessionUpdateIntent::Failed(message.clone())],
        AgentEvent::Cancelled => vec![SessionUpdateIntent::Cancelled],
        AgentEvent::Finished => vec![SessionUpdateIntent::Finished],
        AgentEvent::Started
        | AgentEvent::ModelMetadataLoaded(_)
        | AgentEvent::Retrying { .. }
        | AgentEvent::PermissionRequest(_)
        | AgentEvent::PermissionResolved { .. }
        | AgentEvent::AcpSession(_) => vec![],
    }
}
