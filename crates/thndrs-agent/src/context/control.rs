//! Context control ledger.
//!
//! Pure typed model of the context working set: candidate/selected items,
//! model context limits, token budgets, and diagnostics.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::lifecycle::ContextLifecycle;
use super::support::ratio_of;

/// Conservative bytes-per-token divisor used until provider tokenizers exist.
///
/// `ceil(utf8_bytes / 3)` approximates tokens for mixed English/code content.
pub const TOKEN_BYTES_DIVISOR: usize = 3;

/// Fixed overhead added to every item's token estimate.
///
/// Covers structural framing (XML tags, role markers, separators) the provider
/// adds around each context block.
pub const TOKEN_ITEM_OVERHEAD: usize = 16;

/// Fraction of the available input budget that selection targets.
///
/// Selection aims to stay at or below this ratio so headroom remains for the
/// final user turn, tool schemas, and provider wrapper overhead.
pub const TARGET_BUDGET_RATIO: f64 = 0.80;

/// Fraction of the available input budget above which auto-compaction may
/// trigger after normal eviction and summary candidates.
pub const AUTO_COMPACTION_RATIO: f64 = 0.92;

/// Reserved provider overhead (tokens) subtracted from the input budget.
///
/// Covers system framing, tool-schema wrappers, and safety policy blocks the
/// harness always sends.
pub const PROVIDER_OVERHEAD_TOKENS: u64 = 1_024;

/// Conservative fallback context window when no model metadata is available.
pub const FALLBACK_CONTEXT_WINDOW: u64 = 32_768;

/// Conservative fallback max completion tokens when no metadata is available.
pub const FALLBACK_MAX_COMPLETION_TOKENS: u64 = 4_096;

/// Conservative fallback recommended completion tokens when no metadata is
/// available.
pub const FALLBACK_RECOMMENDED_COMPLETION_TOKENS: u64 = 4_096;

/// Kind of context a [`ContextItem`] represents.
///
/// Drives selection policy and grouping.
/// Kinds are stable labels an adding a variant is a backwards-compatible ledger extension.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextItemKind {
    /// Always-loaded harness prompt fragments, environment metadata, and tool
    /// schemas. System-owned and not directly editable by context controls.
    Harness,
    /// Read-only `AGENTS.md` project instructions (root or nested scope).
    ProjectInstruction,
    /// Task-local pinned file, file range, tool result, or note.
    PinnedFile,
    /// Activated Agent Skill instructions.
    Skill,
    /// Recent transcript entries (user, assistant, reasoning, settled tool).
    Transcript,
    /// Compaction summary standing in for older transcript entries.
    Summary,
    /// Recoverable archived tool output or transcript payload.
    ToolArchive,
}

impl ContextItemKind {
    /// Stable lowercase label used in dashboards, session records, and ids.
    pub fn label(&self) -> &'static str {
        match self {
            ContextItemKind::Harness => "harness",
            ContextItemKind::ProjectInstruction => "project_instruction",
            ContextItemKind::PinnedFile => "pinned_file",
            ContextItemKind::Skill => "skill",
            ContextItemKind::Transcript => "transcript",
            ContextItemKind::Summary => "summary",
            ContextItemKind::ToolArchive => "tool_archive",
        }
    }
}

/// Inclusion status of a [`ContextItem`] in the current working set.
///
/// `Visible` and `Pinned` items are rendered into the prompt. The remaining
/// states describe why an item is omitted and how it can be recovered.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextVisibility {
    /// Selected and rendered into the prompt this turn.
    Visible,
    /// User-pinned; always rendered until dropped or expired.
    Pinned,
    /// Replaced by a compaction summary; original detail is recoverable.
    SummaryOnly,
    /// Moved out of the active working set; recoverable by id.
    Archived,
    /// Discovered but not yet selected (e.g. nested `AGENTS.md` out of scope).
    Candidate,
    /// Explicitly excluded by the user until source change or `/drop --reset`.
    Dropped,
    /// Excluded because it would exceed budget even after eviction; recoverable.
    Blocked,
}

impl ContextVisibility {
    /// Whether the item is rendered into the model prompt this turn.
    pub fn is_rendered(&self) -> bool {
        matches!(self, ContextVisibility::Visible | ContextVisibility::Pinned)
    }

    /// Stable lowercase label used in dashboards and session records.
    pub fn label(&self) -> &'static str {
        match self {
            ContextVisibility::Visible => "visible",
            ContextVisibility::Pinned => "pinned",
            ContextVisibility::SummaryOnly => "summary_only",
            ContextVisibility::Archived => "archived",
            ContextVisibility::Candidate => "candidate",
            ContextVisibility::Dropped => "dropped",
            ContextVisibility::Blocked => "blocked",
        }
    }

    /// Human-readable reason for a visibility state.
    pub fn reason(&self, base: &str) -> String {
        match self {
            ContextVisibility::Visible => base.to_string(),
            ContextVisibility::Pinned => format!("{base}: pinned"),
            ContextVisibility::SummaryOnly => format!("{base}: summary-only"),
            ContextVisibility::Archived => format!("{base}: archived"),
            ContextVisibility::Candidate => format!("{base}: candidate (not selected)"),
            ContextVisibility::Dropped => format!("{base}: dropped"),
            ContextVisibility::Blocked => format!("{base}: blocked (oversized)"),
        }
    }
}

/// Provenance of a [`ModelContextLimits`] value.
///
/// Resolution order is: user override, then live metadata, then static
/// provider metadata, then conservative fallback. See
/// [`ModelContextLimits::resolve`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLimitSource {
    /// Conservative default when nothing else is known.
    Fallback = 0,
    /// Built-in static provider metadata (no live fetch).
    Static = 1,
    /// Live provider metadata fetched at runtime.
    LiveMetadata = 2,
    /// User-supplied config override under `[model_limits."provider/model-id"]`.
    UserOverride = 3,
}

impl ModelLimitSource {
    /// Stable lowercase source label.
    pub fn label(&self) -> &'static str {
        match self {
            ModelLimitSource::Fallback => "fallback",
            ModelLimitSource::Static => "static",
            ModelLimitSource::LiveMetadata => "live-metadata",
            ModelLimitSource::UserOverride => "user-override",
        }
    }
}

/// Confidence in a [`ModelContextLimits`] value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLimitConfidence {
    /// Conservative guess; surface uncertainty in `/doctor` and `/context`.
    Conservative,
    /// Built-in static provider defaults; may lag the live model.
    ProviderReported,
    /// Reported by the provider at runtime.
    Exact,
    /// Supplied by the user; trusted but flagged by `/doctor`.
    UserSupplied,
}

impl ModelLimitConfidence {
    /// Stable lowercase confidence label.
    pub fn label(&self) -> &'static str {
        match self {
            ModelLimitConfidence::Conservative => "conservative",
            ModelLimitConfidence::ProviderReported => "provider-reported",
            ModelLimitConfidence::Exact => "exact",
            ModelLimitConfidence::UserSupplied => "user-supplied",
        }
    }
}

/// Severity of a [`ContextDiagnostic`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    /// Informational note (e.g. fallback limits in use).
    Info,
    /// Recoverable issue that may degrade context quality.
    Warning,
    /// Blocks correct context selection until resolved.
    Error,
}

impl DiagnosticSeverity {
    /// Stable lowercase severity label.
    pub fn label(&self) -> &'static str {
        match self {
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Error => "error",
        }
    }
}

/// Provider-neutral model context limits.
///
/// Adapters translate provider-specific metadata into this shape so the ledger
/// stays free of provider-client types. See [`ModelContextLimits::resolve`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelContextLimits {
    /// Provider label (e.g. `"umans"`, `"opencode-go"`).
    pub provider: String,
    /// Model id as selected by the user (e.g. `"umans-coder"`).
    pub model: String,
    /// Total context window in tokens (input + output).
    pub context_window: u64,
    /// Maximum completion tokens the model accepts.
    pub max_completion_tokens: u64,
    /// Recommended completion tokens for typical turns.
    pub recommended_completion_tokens: u64,
    /// Where the limits came from.
    pub source: ModelLimitSource,
    /// How trustworthy the limits are.
    pub confidence: ModelLimitConfidence,
}

impl ModelContextLimits {
    /// Available input budget = context window minus reserved completion budget
    /// and provider overhead.
    ///
    /// Selection targets [`TARGET_BUDGET_RATIO`] of this value.
    ///
    /// Auto-compaction may trigger above [`AUTO_COMPACTION_RATIO`].
    pub fn available_input_budget(&self) -> u64 {
        let reserved_completion = self.recommended_completion_tokens.max(self.max_completion_tokens);
        self.context_window
            .saturating_sub(reserved_completion)
            .saturating_sub(PROVIDER_OVERHEAD_TOKENS)
    }

    /// Token budget the selection policy targets (80% of available input).
    pub fn target_budget(&self) -> u64 {
        ratio_of(self.available_input_budget(), TARGET_BUDGET_RATIO)
    }

    /// Token budget above which auto-compaction may trigger (92% of available
    /// input).
    pub fn auto_compaction_threshold(&self) -> u64 {
        ratio_of(self.available_input_budget(), AUTO_COMPACTION_RATIO)
    }

    /// Resolve limits from candidate sources in precedence order:
    /// user override → live metadata → static provider metadata → fallback.
    ///
    /// Returns the chosen limits plus diagnostics for invalid overrides and
    /// fallback usage.
    pub fn resolve(
        provider: &str, model: &str, override_entry: Option<ModelLimitOverride>, live: Option<&LiveModelMetadata>,
    ) -> (ModelContextLimits, Vec<ContextDiagnostic>) {
        let mut diagnostics = Vec::new();

        if let Some(entry) = override_entry {
            match entry.validate() {
                Ok(()) => {
                    return (
                        ModelContextLimits {
                            provider: provider.to_string(),
                            model: model.to_string(),
                            context_window: entry.context_window,
                            max_completion_tokens: entry.max_completion_tokens,
                            recommended_completion_tokens: entry.recommended_completion_tokens,
                            source: ModelLimitSource::UserOverride,
                            confidence: ModelLimitConfidence::UserSupplied,
                        },
                        diagnostics,
                    );
                }
                Err(reason) => {
                    diagnostics.push(ContextDiagnostic::invalid_model_override(provider, model, &reason));
                }
            }
        }

        if let Some(live) = live
            && let Some(limits) = live.to_limits(provider, model)
        {
            return (limits, diagnostics);
        }

        if let Some(static_limits) = static_provider_limits(provider, model) {
            return (static_limits, diagnostics);
        }

        diagnostics.push(ContextDiagnostic::fallback_model_limits(provider, model));
        (
            ModelContextLimits {
                provider: provider.to_string(),
                model: model.to_string(),
                context_window: FALLBACK_CONTEXT_WINDOW,
                max_completion_tokens: FALLBACK_MAX_COMPLETION_TOKENS,
                recommended_completion_tokens: FALLBACK_RECOMMENDED_COMPLETION_TOKENS,
                source: ModelLimitSource::Fallback,
                confidence: ModelLimitConfidence::Conservative,
            },
            diagnostics,
        )
    }
}

/// Live provider metadata translated into the neutral limits shape at the
/// adapter boundary.
///
/// Construct this from a provider's `ModelInfo`/metadata before calling [`ModelContextLimits::resolve`].
///
/// Fields are `Option` so adapters can report partial metadata.
///
/// A `None` context window or completion budget falls through to the next precedence source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveModelMetadata {
    /// Total model context window in tokens, when reported by the provider.
    pub context_window: Option<u64>,
    /// Maximum completion size in tokens, when reported by the provider.
    pub max_completion_tokens: Option<u64>,
    /// Recommended completion size in tokens, when reported by the provider.
    pub recommended_completion_tokens: Option<u64>,
}

impl LiveModelMetadata {
    /// Build from explicit live values.
    pub fn new(context_window: u64, max_completion_tokens: u64, recommended_completion_tokens: u64) -> Self {
        Self {
            context_window: Some(context_window),
            max_completion_tokens: Some(max_completion_tokens),
            recommended_completion_tokens: Some(recommended_completion_tokens),
        }
    }

    /// Convert to [`ModelContextLimits`] only when a context window and a
    /// completion budget are present. Returns `None` to fall through to the
    /// next precedence source otherwise.
    fn to_limits(&self, provider: &str, model: &str) -> Option<ModelContextLimits> {
        let context_window = self.context_window?;
        let max_completion_tokens = self.max_completion_tokens?;
        let recommended = self
            .recommended_completion_tokens
            .unwrap_or_else(|| max_completion_tokens.min(context_window / 2));
        if context_window == 0 || max_completion_tokens == 0 {
            return None;
        }
        Some(ModelContextLimits {
            provider: provider.to_string(),
            model: model.to_string(),
            context_window,
            max_completion_tokens,
            recommended_completion_tokens: recommended,
            source: ModelLimitSource::LiveMetadata,
            confidence: ModelLimitConfidence::Exact,
        })
    }
}

/// User-supplied model limit override parsed from
/// `[model_limits."provider/model-id"]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelLimitOverride {
    /// Total model context window in tokens.
    pub context_window: u64,
    /// Maximum completion size in tokens.
    pub max_completion_tokens: u64,
    /// Recommended completion size in tokens.
    pub recommended_completion_tokens: u64,
}

impl ModelLimitOverride {
    /// Validate the override by confirming that all fields must be positive integers
    /// and the recommended completion tokens must not exceed max completion tokens or
    /// the context window.
    ///
    /// Returns a human-readable reason on failure.
    pub fn validate(&self) -> Result<(), String> {
        if self.context_window == 0 {
            return Err("context_window must be a positive integer".to_string());
        }
        if self.max_completion_tokens == 0 {
            return Err("max_completion_tokens must be a positive integer".to_string());
        }
        if self.recommended_completion_tokens == 0 {
            return Err("recommended_completion_tokens must be a positive integer".to_string());
        }
        if self.recommended_completion_tokens > self.max_completion_tokens {
            return Err(format!(
                "recommended_completion_tokens ({}) must not exceed max_completion_tokens ({})",
                self.recommended_completion_tokens, self.max_completion_tokens
            ));
        }
        if self.max_completion_tokens >= self.context_window {
            return Err(format!(
                "max_completion_tokens ({}) must be less than context_window ({})",
                self.max_completion_tokens, self.context_window
            ));
        }
        Ok(())
    }
}

/// A single context source considered for the working set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextItem {
    /// Stable id (see [`item_id_for_path`] / [`item_id_for_session_range`]).
    pub id: String,
    /// What kind of context this is.
    pub kind: ContextItemKind,
    /// Short human-readable label (e.g. a file path or transcript label).
    pub label: String,
    /// Absolute source path when the item is file-backed.
    pub source_path: Option<PathBuf>,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Content hash of the full original content, when applicable.
    pub content_hash: Option<u64>,
    /// Stable handle for redacted recovery, when this item has one.
    pub artifact_handle: Option<String>,
    /// Original byte count of the source content.
    pub byte_count: usize,
    /// Renderable content for the prompt projection, when this item is
    /// selected as normal context content. `None` for metadata-only items
    /// (pins rendered as handles, candidates, dropped items) and for items
    /// whose content is projected through other bundle fields (transcript
    /// entries are lowered as messages, not inlined here).
    ///
    /// The model-visible dashboard deliberately omits this field.
    pub content: Option<String>,
    /// Conservative estimated token cost (`ceil(utf8_bytes / 3) + 16`).
    pub token_estimate: usize,
    /// Inclusion status this turn.
    pub visibility: ContextVisibility,
    /// Stable policy code for why this state was assigned.
    pub reason_code: String,
    /// Why the item is visible, omitted, archived, dropped, blocked, or summary-only.
    pub reason: String,
    /// Explicit lifecycle and protection state, independent of request
    /// visibility.
    pub lifecycle: ContextLifecycle,
}

impl ContextItem {
    /// Render a compact one-line summary for `/context` and transcript rows.
    pub fn summary(&self) -> String {
        format!(
            "{}  {}  {}  {}  {} est. tokens  [{}]",
            self.id,
            self.kind.label(),
            self.visibility.label(),
            self.lifecycle.state.label(),
            self.token_estimate,
            self.label,
        )
    }
}

/// Token budget derived from [`ModelContextLimits`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextBudget {
    /// Limits the budget was derived from.
    pub limits: ModelContextLimits,
    /// Available input budget (context window − reserved completion − overhead).
    pub available_input: u64,
    /// Target selection budget (80% of available input).
    pub target: u64,
    /// Auto-compaction trigger threshold (92% of available input).
    pub auto_compaction_threshold: u64,
    /// Estimated tokens of currently rendered (`Visible` + `Pinned`) items.
    pub used: u64,
}

impl ContextBudget {
    /// Build a budget from resolved limits and the currently rendered items.
    pub fn from_limits(limits: ModelContextLimits, items: &[ContextItem]) -> Self {
        let available_input = limits.available_input_budget();
        let target = limits.target_budget();
        let auto_compaction_threshold = limits.auto_compaction_threshold();
        let used = items
            .iter()
            .filter(|item| item.visibility.is_rendered())
            .map(|item| item.token_estimate as u64)
            .sum();
        ContextBudget { limits, available_input, target, auto_compaction_threshold, used }
    }

    /// Whether rendered items exceed the target selection budget.
    pub fn exceeds_target(&self) -> bool {
        self.used > self.target
    }

    /// Whether rendered items exceed the auto-compaction threshold.
    pub fn exceeds_auto_compaction(&self) -> bool {
        self.used > self.auto_compaction_threshold
    }

    /// Remaining tokens before the target budget is reached.
    pub fn remaining_to_target(&self) -> u64 {
        self.target.saturating_sub(self.used)
    }
}

/// A diagnostic about context or model-limit state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextDiagnostic {
    /// Severity of the condition reported by this diagnostic.
    pub severity: DiagnosticSeverity,
    /// Short code (e.g. `"fallback_model_limits"`, `"invalid_model_override"`).
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
}

impl ContextDiagnostic {
    /// Fallback limits are in use because no metadata was available.
    pub fn fallback_model_limits(provider: &str, model: &str) -> Self {
        ContextDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "fallback_model_limits".to_string(),
            message: format!("no model metadata for {provider}/{model}; using conservative fallback context window"),
        }
    }

    /// A user override was rejected as invalid.
    pub fn invalid_model_override(provider: &str, model: &str, reason: &str) -> Self {
        ContextDiagnostic {
            severity: DiagnosticSeverity::Error,
            code: "invalid_model_override".to_string(),
            message: format!("model_limits override for {provider}/{model} rejected: {reason}"),
        }
    }

    /// Render a compact one-line summary.
    pub fn summary(&self) -> String {
        format!("{}  {}  {}", self.severity.label(), self.code, self.message)
    }
}

/// The context ledger: all candidate/selected items, the budget, and
/// diagnostics for a turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextLedger {
    /// Every candidate item and its selected, omitted, or blocked state.
    pub items: Vec<ContextItem>,
    /// Resolved token limits and current budget usage.
    pub budget: ContextBudget,
    /// Diagnostics produced while resolving limits or selecting context.
    pub diagnostics: Vec<ContextDiagnostic>,
}

impl ContextLedger {
    /// Items rendered into the prompt this turn.
    pub fn rendered(&self) -> Vec<&ContextItem> {
        self.items.iter().filter(|item| item.visibility.is_rendered()).collect()
    }

    /// Count of items by visibility label.
    pub fn counts(&self) -> ContextCounts {
        let mut counts = ContextCounts::default();
        for item in &self.items {
            match item.visibility {
                ContextVisibility::Visible => counts.visible += 1,
                ContextVisibility::Pinned => counts.pinned += 1,
                ContextVisibility::SummaryOnly => counts.summary_only += 1,
                ContextVisibility::Archived => counts.archived += 1,
                ContextVisibility::Candidate => counts.candidate += 1,
                ContextVisibility::Dropped => counts.dropped += 1,
                ContextVisibility::Blocked => counts.blocked += 1,
            }
        }
        counts
    }

    /// Find an item by id.
    pub fn find(&self, id: &str) -> Option<&ContextItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Build the stable, content-free projection consumed by inspection and
    /// status surfaces.
    pub fn projection(&self) -> ContextProjection {
        let mut categories = ContextCategory::ALL.map(|category| ContextCategoryTotal {
            category,
            available_items: 0,
            selected_items: 0,
            available_tokens: 0,
            selected_tokens: 0,
        });
        let mut selected = 0;
        let mut omitted = 0;
        let mut recoverable = 0;
        let mut protected = 0;
        for item in &self.items {
            let total = &mut categories[item.kind.category_index()];
            total.available_items += 1;
            total.available_tokens = total.available_tokens.saturating_add(item.token_estimate as u64);
            if item.visibility.is_rendered() {
                selected += 1;
                total.selected_items += 1;
                total.selected_tokens = total.selected_tokens.saturating_add(item.token_estimate as u64);
            } else {
                omitted += 1;
            }
            if item.artifact_handle.is_some() || !item.visibility.is_rendered() {
                recoverable += 1;
            }
            if item.lifecycle.is_protected() {
                protected += 1;
            }
        }
        let remaining_percent = (self.budget.available_input > 0).then(|| {
            self.budget
                .available_input
                .saturating_sub(self.budget.used)
                .saturating_mul(100)
                / self.budget.available_input
        });
        ContextProjection {
            used: self.budget.used,
            available_input: self.budget.available_input,
            remaining_percent,
            target: self.budget.target,
            auto_compaction_threshold: self.budget.auto_compaction_threshold,
            estimate_provenance: "conservative utf8 bytes / 3 + item overhead".to_string(),
            limit_source: self.budget.limits.source,
            limit_confidence: self.budget.limits.confidence,
            categories,
            selected,
            omitted,
            recoverable,
            protected,
        }
    }
}

impl ContextItemKind {
    fn category_index(&self) -> usize {
        match self {
            Self::Harness => 0,
            Self::ProjectInstruction => 1,
            Self::Skill => 2,
            Self::PinnedFile => 3,
            Self::Transcript => 4,
            Self::Summary => 5,
            Self::ToolArchive => 6,
        }
    }
}

/// User-facing aggregation categories for a context projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextCategory {
    /// Harness prompt and tool schema context.
    Harness,
    /// Project instruction files.
    Instructions,
    /// Activated skill instructions.
    Skills,
    /// User-pinned context.
    Pinned,
    /// Conversation transcript.
    Conversation,
    /// Compaction summaries.
    Summaries,
    /// Archived or recoverable tool results.
    ToolResults,
}

impl ContextCategory {
    /// Categories in stable display order.
    pub const ALL: [Self; 7] = [
        Self::Harness,
        Self::Instructions,
        Self::Skills,
        Self::Pinned,
        Self::Conversation,
        Self::Summaries,
        Self::ToolResults,
    ];

    /// Human-readable category label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Harness => "Harness",
            Self::Instructions => "Instructions",
            Self::Skills => "Skills",
            Self::Pinned => "Pinned",
            Self::Conversation => "Conversation",
            Self::Summaries => "Summaries",
            Self::ToolResults => "Tool results",
        }
    }
}

/// Selected and candidate totals for one context category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextCategoryTotal {
    /// Category represented by this total.
    pub category: ContextCategory,
    /// Candidate items in the category.
    pub available_items: usize,
    /// Rendered items in the category.
    pub selected_items: usize,
    /// Estimated tokens across all candidates.
    pub available_tokens: u64,
    /// Estimated tokens across rendered candidates.
    pub selected_tokens: u64,
}

/// Stable projection of context pressure and selection state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextProjection {
    /// Estimated rendered input tokens.
    pub used: u64,
    /// Input tokens available after reservations.
    pub available_input: u64,
    /// Percentage of available input not projected for use.
    pub remaining_percent: Option<u64>,
    /// Normal selection target.
    pub target: u64,
    /// Automatic compaction threshold.
    pub auto_compaction_threshold: u64,
    /// Estimator name and policy.
    pub estimate_provenance: String,
    /// Source of the resolved model limit.
    pub limit_source: ModelLimitSource,
    /// Confidence in the resolved model limit.
    pub limit_confidence: ModelLimitConfidence,
    /// Stable per-category totals.
    pub categories: [ContextCategoryTotal; 7],
    /// Number of rendered items.
    pub selected: usize,
    /// Number of omitted items.
    pub omitted: usize,
    /// Number of items with a recovery path.
    pub recoverable: usize,
    /// Number of protected items.
    pub protected: usize,
}

/// Visibility counts for a [`ContextLedger`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextCounts {
    /// Number of visible items.
    pub visible: usize,
    /// Number of pinned items.
    pub pinned: usize,
    /// Number of summary-only items.
    pub summary_only: usize,
    /// Number of archived items.
    pub archived: usize,
    /// Number of unselected candidate items.
    pub candidate: usize,
    /// Number of explicitly dropped items.
    pub dropped: usize,
    /// Number of items blocked by the budget.
    pub blocked: usize,
}

/// Conservative token estimate: `ceil(utf8_bytes / 3) + 16`.
///
/// Approximate until provider-specific tokenizers exist. Operates on UTF-8
/// bytes so multibyte content is not undercounted.
pub fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(TOKEN_BYTES_DIVISOR) + TOKEN_ITEM_OVERHEAD
}

/// Generate a stable context item id for a file-backed source.
///
/// The id is `ctx_<kind>:<hash>` where the hash is derived from the kind and
/// the canonical path string. This is kept stable across turns for the same path and kind.
pub fn item_id_for_path(kind: &ContextItemKind, path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    kind.label().hash(&mut hasher);
    path.hash(&mut hasher);
    format!("ctx_{}_{:016x}", kind.label(), hasher.finish())
}

/// Generate a stable context item id for a transcript/session source range.
///
/// `session_id` scopes the range to a session; `start` and `end` are inclusive
/// sequence numbers.
///
/// The id is stable for the same session and range.
pub fn item_id_for_session_range(kind: &ContextItemKind, session_id: &str, start: u64, end: u64) -> String {
    let mut hasher = DefaultHasher::new();
    kind.label().hash(&mut hasher);
    session_id.hash(&mut hasher);
    start.hash(&mut hasher);
    end.hash(&mut hasher);
    format!("ctx_{}_{:016x}", kind.label(), hasher.finish())
}

/// Render a compact user-visible ledger summary line.
///
/// Example: `context  9 visible · 3 pinned · 2 archived · 18k est. tokens`.
pub fn render_ledger_summary(ledger: &ContextLedger) -> String {
    let counts = ledger.counts();
    let mut parts = vec![format!("{} visible", counts.visible)];
    if counts.pinned > 0 {
        parts.push(format!("{} pinned", counts.pinned));
    }
    if counts.archived > 0 {
        parts.push(format!("{} archived", counts.archived));
    }
    if counts.summary_only > 0 {
        parts.push(format!("{} summary", counts.summary_only));
    }
    if counts.blocked > 0 {
        parts.push(format!("{} blocked", counts.blocked));
    }
    parts.push(format!("{} est. tokens", compact_token_count(ledger.budget.used)));
    format!("context  {}", parts.join(" · "))
}

/// Render a compact model-visible context dashboard.
///
/// Excludes full content: only ids, kinds, visibility, token estimates, and
/// budget pressure.
///
///  Intended for inclusion in the self-knowledge snapshot.
pub fn render_model_dashboard(ledger: &ContextLedger) -> String {
    let mut out = String::new();
    out.push_str("<context_dashboard>\n");
    out.push_str("  <budget>\n");
    element(
        &mut out,
        4,
        "available_input",
        &compact_token_count(ledger.budget.available_input),
    );
    element(&mut out, 4, "target", &compact_token_count(ledger.budget.target));
    element(
        &mut out,
        4,
        "auto_compaction_threshold",
        &compact_token_count(ledger.budget.auto_compaction_threshold),
    );
    element(&mut out, 4, "used", &compact_token_count(ledger.budget.used));
    element(
        &mut out,
        4,
        "exceeds_target",
        &ledger.budget.exceeds_target().to_string(),
    );
    element(
        &mut out,
        4,
        "exceeds_auto_compaction",
        &ledger.budget.exceeds_auto_compaction().to_string(),
    );
    element(&mut out, 4, "limit_source", ledger.budget.limits.source.label());
    element(&mut out, 4, "limit_confidence", ledger.budget.limits.confidence.label());
    out.push_str("  </budget>\n");

    out.push_str("  <items>\n");
    for item in &ledger.items {
        out.push_str("    <item>\n");
        element(&mut out, 6, "id", &item.id);
        element(&mut out, 6, "kind", item.kind.label());
        element(&mut out, 6, "visibility", item.visibility.label());
        element(&mut out, 6, "tokens", &item.token_estimate.to_string());
        element(&mut out, 6, "label", &item.label);
        if let Some(handle) = &item.artifact_handle {
            element(&mut out, 6, "recovery_handle", handle);
        }
        out.push_str("    </item>\n");
    }
    out.push_str("  </items>\n");

    if !ledger.diagnostics.is_empty() {
        out.push_str("  <diagnostics>\n");
        for diagnostic in &ledger.diagnostics {
            element(&mut out, 4, "diagnostic", &diagnostic.summary());
        }
        out.push_str("  </diagnostics>\n");
    }
    out.push_str("</context_dashboard>");
    out
}

/// Conservative static provider fallbacks for providers without live
/// context-window metadata.
///
/// Returns `None` for unknown providers so the caller falls through to the
/// conservative default.
fn static_provider_limits(provider: &str, model: &str) -> Option<ModelContextLimits> {
    let (context_window, max_completion, recommended) = match provider {
        "umans" => (200_000, 32_768, 8_192),
        "opencode-go" => (200_000, 32_768, 8_192),
        "opencode-zen" => (200_000, 32_768, 8_192),
        "chatgpt-codex" => (200_000, 32_768, 8_192),
        _ => return None,
    };
    Some(ModelContextLimits {
        provider: provider.to_string(),
        model: model.to_string(),
        context_window,
        max_completion_tokens: max_completion,
        recommended_completion_tokens: recommended,
        source: ModelLimitSource::Static,
        confidence: ModelLimitConfidence::ProviderReported,
    })
}

/// Compact token count for display (e.g. `18k`, `1M`, `1234`).
fn compact_token_count(tokens: u64) -> String {
    const K: u64 = 1_000;
    const M: u64 = K * 1_000;
    if tokens >= M && tokens.is_multiple_of(M) {
        format!("{}M", tokens / M)
    } else if tokens >= K && tokens.is_multiple_of(K) {
        format!("{}k", tokens / K)
    } else {
        tokens.to_string()
    }
}

/// Append an indented XML element `<name>value</name>` to `out`.
fn element(out: &mut String, indent: usize, name: &str, value: &str) {
    let pad = " ".repeat(indent);
    out.push_str(&format!("{pad}<{name}>{value}</{name}>\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ModelContextLimits {
        ModelContextLimits {
            provider: "fixture".to_string(),
            model: "fixture-model".to_string(),
            context_window: 1_200,
            max_completion_tokens: 100,
            recommended_completion_tokens: 100,
            source: ModelLimitSource::LiveMetadata,
            confidence: ModelLimitConfidence::Exact,
        }
    }

    fn item(index: usize, kind: ContextItemKind, visibility: ContextVisibility) -> ContextItem {
        ContextItem {
            id: format!("ctx_{index}"),
            kind,
            label: format!("item {index}"),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            artifact_handle: (index == 1).then(|| "artifact:1".to_string()),
            byte_count: 30,
            content: None,
            token_estimate: 10,
            visibility,
            reason_code: "fixture".to_string(),
            reason: "fixture selection".to_string(),
            lifecycle: ContextLifecycle::default(),
        }
    }

    #[test]
    fn projection_aggregates_every_category_and_selection_state() {
        let kinds = [
            ContextItemKind::Harness,
            ContextItemKind::ProjectInstruction,
            ContextItemKind::Skill,
            ContextItemKind::PinnedFile,
            ContextItemKind::Transcript,
            ContextItemKind::Summary,
            ContextItemKind::ToolArchive,
        ];
        let items = kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                item(
                    index,
                    kind,
                    if index < 2 { ContextVisibility::Visible } else { ContextVisibility::Archived },
                )
            })
            .collect::<Vec<_>>();
        let ledger =
            ContextLedger { budget: ContextBudget::from_limits(limits(), &items), items, diagnostics: Vec::new() };

        let projection = ledger.projection();
        assert_eq!(projection.selected, 2);
        assert_eq!(projection.omitted, 5);
        assert_eq!(projection.recoverable, 6);
        assert_eq!(
            projection.remaining_percent,
            Some((projection.available_input - projection.used) * 100 / projection.available_input)
        );
        assert!(projection.categories.iter().all(|total| total.available_items == 1));
        assert_eq!(projection.categories[0].selected_tokens, 10);
        assert_eq!(projection.categories[6].selected_tokens, 0);
        assert_eq!(projection.limit_source, ModelLimitSource::LiveMetadata);
    }

    #[test]
    fn projection_preserves_unknown_capacity_instead_of_reporting_zero_percent() {
        let mut budget = ContextBudget::from_limits(limits(), &[]);
        budget.available_input = 0;
        budget.target = 0;
        budget.auto_compaction_threshold = 0;
        let ledger = ContextLedger { items: Vec::new(), budget, diagnostics: Vec::new() };

        assert_eq!(ledger.projection().remaining_percent, None);
    }
}
