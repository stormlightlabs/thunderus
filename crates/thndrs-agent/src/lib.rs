#![doc = include_str!("../README.md")]

pub mod adapters;
pub mod budget;
pub mod cancel;
pub mod context;
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
