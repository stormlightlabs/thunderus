//! Provider implementations.

use crate::tools::ToolUseRequest;

pub mod umans;

/// Provider-neutral result of one streamed model turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderTurn {
    pub tool_requests: Vec<ToolUseRequest>,
    pub assistant_text: String,
    pub stop_reason: Option<String>,
}
