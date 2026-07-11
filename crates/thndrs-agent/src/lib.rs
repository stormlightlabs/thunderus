//! Provider-neutral coding-agent loop and contracts.
//!
//! Application adapters own provider selection, tool policy, terminal I/O,
//! and ACP transport. This crate owns shared run contracts such as cooperative
//! cancellation.

pub mod adapters;
pub mod budget;
pub mod cancel;
pub mod contracts;
pub mod run;

pub use adapters::{ToolExecutionHook, ToolPermissionHook};
pub use budget::{ToolBudgetDecision, ToolIterationBudget};
pub use cancel::CancelToken;
pub use contracts::{
    AgentEvent, AgentMessage, AgentTurn, RetryPolicy, ToolDefinition, ToolOutput, ToolPermissionDecision, ToolStatus,
    ToolUseRequest,
};
pub use run::AgentRun;
