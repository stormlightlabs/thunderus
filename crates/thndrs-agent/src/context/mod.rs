//! Provider-neutral context control for agent turns.
//!
//! This module owns deterministic context policy only. Hosts discover files,
//! persist sessions, and render selected context at their application boundary.

mod support;

pub mod compaction;
pub mod control;
pub mod deduplication;
pub mod lifecycle;
pub mod reduction;
pub mod selection;

pub use compaction::{
    AutoCompactionDecision, CompactionConfig, CompactionMode, CompactionPolicy, CompactionReview, CompactionRisk,
    CompactionRiskSignals, ContextConfig, ManualCompactionRequest, preflight_auto_compaction,
    prepare_manual_compaction,
};
pub use control::{
    ContextBudget, ContextCounts, ContextDiagnostic, ContextItem, ContextItemKind, ContextLedger, ContextVisibility,
    DiagnosticSeverity, LiveModelMetadata, ModelContextLimits, ModelLimitConfidence, ModelLimitOverride,
    ModelLimitSource,
};
pub use control::{
    estimate_tokens, item_id_for_path, item_id_for_session_range, render_ledger_summary, render_model_dashboard,
};
pub use deduplication::{
    STATE_IDENTICAL_REDUCER_VERSION, StateProjectionCandidate, StateProjectionDecision, StateProjectionIdentity,
    StateProjectionRecord, StateProjectionReduction, reduce_state_identical,
};
pub use lifecycle::{
    ContextLifecycle, ContextLifecycleAction, ContextLifecycleError, ContextLifecycleState, ContextProtection,
    ContextProtectionReason, ContextRelation, ContextRelationKind, ContextRelationStatus, relation_id_for,
};
pub use reduction::{
    BLANK_RUN_REDUCER_VERSION, BoundedProjection, DEFAULT_PROJECTION_MAX_BYTES, MAX_BLANK_LINES,
    PROGRESS_REDRAW_REDUCER_VERSION, REDUCTION_CONFIG_VERSION, REPEATED_LINE_REDUCER_VERSION, ReducerKind,
    ReductionConfig, ReductionConfigError, ReductionDashboard, ReductionDiagnostic, ReductionResult,
    TERMINAL_CONTROL_REDUCER_VERSION, measure_lines, reduce_lines, reduce_projection, render_reduction_dashboard,
};
pub use selection::{
    CompactionSummaryCandidate, HarnessCandidate, InstructionCandidate, PendingPermissionCandidate, PinnedCandidate,
    SelectionInput, SkillCandidate, TranscriptCandidate, UserTurnCandidate, select_context,
};
