#![doc = include_str!("../README.md")]
#![doc(html_root_url = "https://docs.rs/thndrs-agent/0.1.0")]
#![warn(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]

pub mod accounting;
pub mod adapters;
pub mod budget;
pub mod cancel;
pub mod context;
pub mod contracts;
pub mod replay;
pub mod run;

pub use accounting::{
    ByteMeasurement, ContextItemSnapshot, ContextReductionMode, ContextReductionReceipt, MeasurementProvenance,
    ModelProjectionMessage, ProviderRequestAccounting, ProviderUsage, ProviderUsageComponents, ProviderUsageRule,
    TOKEN_ESTIMATOR_VERSION, TokenMeasurement, USAGE_NORMALIZATION_VERSION, estimate_serialized_tokens,
    snapshot_context,
};
pub use adapters::{ToolExecutionHook, ToolPermissionHook};
pub use budget::{ToolBudgetDecision, ToolIterationBudget};
pub use cancel::CancelToken;
pub use context::reduction::{measure_lines, reduce_lines, reduce_projection, render_reduction_dashboard};
pub use contracts::{
    AgentEvent, AgentMessage, AgentTurn, RetryPolicy, ToolDefinition, ToolDisplayProjection, ToolEvidenceKind,
    ToolEvidenceMetadata, ToolModelProjection, ToolOutput, ToolPermissionDecision, ToolStatus, ToolUseRequest,
};
pub use replay::{
    BaselinePolicy, CandidatePolicy, ProjectionReport, RecordedProviderUsage, RecoveryCase, RecoveryOutcome,
    ReplayComparison, ReplayError, ReplayEvaluator, ReplayFixture, ReplayItem, ReplayItemKind, ReplayPolicy,
    ReplayProjection, ReplayReceipt, ReplayReport, ReplayScenario, ReplayTiming, RequiredFact, RequiredFactResult,
    evaluate_fixture, load_fixture, project_fixture, select_items,
};
pub use run::AgentRun;
