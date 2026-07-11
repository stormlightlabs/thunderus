//! Provider-neutral context control for agent turns.
//!
//! This module owns deterministic context policy only. Hosts discover files,
//! persist sessions, and render selected context at their application boundary.

mod support;

pub mod compaction;
pub mod control;
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
pub use selection::{
    CompactionSummaryCandidate, HarnessCandidate, InstructionCandidate, PinnedCandidate, SelectionInput,
    SkillCandidate, TranscriptCandidate, UserTurnCandidate, select_context,
};
