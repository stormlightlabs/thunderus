#![doc = include_str!("../README.md")]
#![doc(html_root_url = "https://docs.rs/thndrs-agent/0.1.0")]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

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
    AgentEvent, AgentMessage, AgentTurn, RetryPolicy, ToolDefinition, ToolDisplayProjection, ToolEvidenceKind,
    ToolEvidenceMetadata, ToolModelProjection, ToolOutput, ToolPermissionDecision, ToolStatus, ToolUseRequest,
};
pub use run::AgentRun;
