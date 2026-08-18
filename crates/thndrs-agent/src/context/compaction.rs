//! Pure compaction configuration, pressure policy, and risk classification.

use serde::{Deserialize, Serialize};

use super::{ContextBudget, ModelContextLimits, ReductionConfig};

/// Schema version for provider-neutral range summaries.
pub const RANGE_SUMMARY_SCHEMA_VERSION: u32 = 1;

const DEFAULT_AUTO_COMPACTION_PERCENT: u8 = 92;
const DEFAULT_KEEP_RECENT_TOKENS: u64 = 20_000;

/// One recoverable source included in a closed compression range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RangeSource {
    /// Sequence within the requested closed range.
    pub sequence: u64,
    /// Stable id of the original context item.
    pub id: String,
    /// Application-computed hash of the rendered source content.
    pub content_hash: u64,
    /// Handle for redacted recovery of the original source.
    pub recovery_handle: String,
}

/// A fact that may not be lost while replacing a range.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtectedFact {
    /// Source item that established this fact.
    pub source_id: String,
    /// Exact protected text from that source.
    pub text: String,
}

/// Provider-neutral request for a contiguous closed range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeCompressionRequest {
    /// Configured model that must perform the semantic compression.
    pub model: String,
    /// Inclusive first source sequence in the closed range.
    pub start_seq: u64,
    /// Inclusive final source sequence in the closed range.
    pub end_seq: u64,
    /// What the continuation summary must emphasize.
    pub focus: String,
    /// Addressable source metadata, in contiguous source order.
    pub sources: Vec<RangeSource>,
    /// Facts the model must copy into its typed result.
    pub protected_facts: Vec<ProtectedFact>,
    /// Earlier summaries that this range includes as source material.
    pub source_summary_ids: Vec<String>,
    /// Prompt sent to the configured model. This is process-local only.
    pub prompt: String,
}

/// Input metadata and process-local content for preparing a range request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeCompressionInput {
    /// Inclusive first source sequence in the closed range.
    pub start_seq: u64,
    /// Inclusive final source sequence in the closed range.
    pub end_seq: u64,
    /// What the continuation summary must emphasize.
    pub focus: String,
    /// Addressable source metadata, in contiguous source order.
    pub sources: Vec<RangeSource>,
    /// Facts the model must copy into its typed result.
    pub protected_facts: Vec<ProtectedFact>,
    /// Earlier summaries that this range includes as source material.
    pub source_summary_ids: Vec<String>,
    /// Source body sent only to the configured model.
    pub source_text: String,
}

/// A versioned, typed summary returned by a configured model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RangeSummary {
    /// Version of this summary contract.
    pub schema_version: u32,
    /// The objective that was active in the covered range.
    pub objective: String,
    /// Material findings from the range.
    pub findings: Vec<String>,
    /// Decisions that constrain continuation.
    pub decisions: Vec<String>,
    /// Relevant repository paths.
    pub paths: Vec<String>,
    /// Failures or safety-relevant negative results.
    pub failures: Vec<String>,
    /// Verification performed or still required.
    pub verification: Vec<String>,
    /// Work that remains blocked or unresolved.
    pub blockers: Vec<String>,
    /// Protected facts preserved exactly from the request.
    pub protected_facts: Vec<ProtectedFact>,
    /// Source metadata copied from the request.
    pub sources: Vec<RangeSource>,
    /// Earlier summaries this summary builds on, when any.
    #[serde(default)]
    pub source_summary_ids: Vec<String>,
}

impl RangeSummary {
    /// Render the typed result for a future model request without losing its
    /// field boundaries or provenance.
    pub fn render_model_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| self.objective.clone())
    }
}

/// Why a provider result cannot replace its covered range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeSummaryValidationError {
    /// The response was not valid typed-summary JSON.
    InvalidJson(String),
    /// The summary used a schema this application does not understand.
    UnsupportedSchema(u32),
    /// A required objective was blank.
    MissingObjective,
    /// Source metadata did not exactly match the requested range.
    SourceMetadataMismatch,
    /// A protected fact was not preserved exactly.
    MissingProtectedFact(ProtectedFact),
}

impl std::fmt::Display for RangeSummaryValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(
                formatter,
                "compaction model returned invalid typed summary JSON: {error}"
            ),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "compaction summary schema version {version} is unsupported")
            }
            Self::MissingObjective => write!(formatter, "compaction summary is missing its required objective"),
            Self::SourceMetadataMismatch => write!(
                formatter,
                "compaction summary source metadata does not match the requested range"
            ),
            Self::MissingProtectedFact(fact) => write!(
                formatter,
                "compaction summary omitted protected fact from {}",
                fact.source_id
            ),
        }
    }
}

impl std::error::Error for RangeSummaryValidationError {}

/// User-selected compaction behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionMode {
    /// Never compact automatically or in response to `/compact`.
    Off,
    /// Compact only after an idle user explicitly requests `/compact`.
    #[default]
    Manual,
    /// Compact automatically when the post-selection context is under pressure.
    Auto,
}

impl CompactionMode {
    /// Stable lowercase label for diagnostics and user-facing context health.
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

/// Review policy for a proposed compaction summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReview {
    /// Require review for every compaction.
    Always,
    /// Require review only for a high-risk covered range.
    #[default]
    Auto,
    /// Never require review.
    Never,
}

/// Context-control configuration parsed from `[context]`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig {
    /// Compaction-specific options.
    pub compaction: CompactionConfig,
    /// Independent deterministic model-projection reducer options.
    pub reduction: ReductionConfig,
}

/// Compaction configuration parsed from `[context.compaction]`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompactionConfig {
    /// Whether compaction is disabled, manually requested, or automatic.
    pub mode: CompactionMode,
    /// When a generated summary needs user review before becoming active.
    pub review: CompactionReview,
    /// Percentage of the available input budget that triggers automatic compaction.
    #[serde(serialize_with = "serialize_threshold", deserialize_with = "deserialize_threshold")]
    pub threshold: u8,
    /// Approximate recent transcript tokens retained verbatim when possible.
    pub keep_recent_tokens: u64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            mode: CompactionMode::default(),
            review: CompactionReview::default(),
            threshold: DEFAULT_AUTO_COMPACTION_PERCENT,
            keep_recent_tokens: DEFAULT_KEEP_RECENT_TOKENS,
        }
    }
}

/// Risk level of material a summary would replace.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CompactionRisk {
    /// No detail requiring explicit review was detected.
    #[default]
    Low,
    /// The range contains operational or unresolved details.
    High,
}

/// Inputs considered when classifying a compaction range.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactionRiskSignals {
    /// The range contains tool output or a patch/diff.
    pub has_tool_output_or_diff: bool,
    /// The range contains an error, permission prompt, or failed command.
    pub has_failure_or_permission: bool,
    /// The range contains a correction or unfinished work.
    pub has_correction_or_unresolved_work: bool,
}

impl CompactionRiskSignals {
    /// Classify the signals conservatively.
    pub fn classify(self) -> CompactionRisk {
        if self.has_tool_output_or_diff || self.has_failure_or_permission || self.has_correction_or_unresolved_work {
            CompactionRisk::High
        } else {
            CompactionRisk::Low
        }
    }
}

/// Pure decisions made before a compaction request is sent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionPolicy {
    /// Selected compaction mode.
    pub mode: CompactionMode,
    /// Selected review policy.
    pub review: CompactionReview,
    /// Percentage of available input at which automatic compaction starts.
    pub threshold: u8,
    /// Approximate recent transcript tokens retained verbatim when possible.
    pub keep_recent_tokens: u64,
}

impl Default for CompactionPolicy {
    fn default() -> Self {
        Self::from_config(&CompactionConfig::default())
    }
}

/// A configured-model request for one manual compaction.
///
/// The source text is deliberately held only in the running process. Callers persist the
/// resulting summary and recovery handle, never this request body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManualCompactionRequest {
    /// The selected configured model; manual compaction never substitutes a
    /// local summarizer.
    pub model: String,
    /// Prompt sent to that configured model.
    pub prompt: String,
    /// Handle for reopening the original session material after success.
    pub recovery_handle: String,
}

/// Build a manual-compaction request without changing active context.
///
/// This is intentionally a prepare step: a provider error simply discards the
/// request and leaves the caller's current projection untouched.
pub fn prepare_manual_compaction(
    policy: CompactionPolicy, model: &str, source_text: &str, recovery_handle: &str,
) -> Result<ManualCompactionRequest, String> {
    if !policy.allows_manual() {
        return Err("compaction is disabled by context.compaction.mode".to_string());
    }
    if model.trim().is_empty() {
        return Err("manual compaction requires a configured model".to_string());
    }
    if source_text.trim().is_empty() {
        return Err("there is no active context to compact".to_string());
    }
    if recovery_handle.trim().is_empty() {
        return Err("manual compaction requires a recovery handle".to_string());
    }

    Ok(ManualCompactionRequest {
        model: model.to_string(),
        prompt: format!(
            "Summarize the following prior work for continuation. Preserve decisions, changed files, failures, permissions, corrections, and unresolved work. Do not invent results.\n\n<source_context>\n{source_text}\n</source_context>"
        ),
        recovery_handle: recovery_handle.to_string(),
    })
}

/// Build a typed request for one contiguous, closed context range.
///
/// This pure prepare step deliberately does not alter the active projection.
/// Callers validate the returned model response with [`validate_range_summary`]
/// before recording or applying any replacement.
pub fn prepare_range_compression(
    policy: CompactionPolicy, model: &str, input: RangeCompressionInput,
) -> Result<RangeCompressionRequest, String> {
    if !policy.allows_manual() {
        return Err("compaction is disabled by context.compaction.mode".to_string());
    }
    if model.trim().is_empty() {
        return Err("range compression requires a configured model".to_string());
    }
    if input.start_seq == 0
        || input.end_seq < input.start_seq
        || input.sources.len() != (input.end_seq - input.start_seq + 1) as usize
    {
        return Err("range compression requires one contiguous non-empty source range".to_string());
    }
    if input.focus.trim().is_empty() {
        return Err("range compression requires a focus".to_string());
    }
    if input
        .sources
        .iter()
        .any(|source| source.id.trim().is_empty() || source.recovery_handle.trim().is_empty())
    {
        return Err("range compression sources require ids and recovery handles".to_string());
    }
    if input
        .sources
        .iter()
        .enumerate()
        .any(|(index, source)| source.sequence != input.start_seq + index as u64)
        || input.sources.iter().enumerate().any(|(index, source)| {
            input.sources[..index]
                .iter()
                .any(|earlier| earlier.id == source.id || earlier.recovery_handle == source.recovery_handle)
        })
    {
        return Err("range compression sources must have unique handles and contiguous sequences".to_string());
    }
    if input.protected_facts.iter().any(|fact| {
        fact.source_id.trim().is_empty()
            || fact.text.trim().is_empty()
            || !input.sources.iter().any(|source| source.id == fact.source_id)
    }) {
        return Err("protected facts must identify a non-empty fact from a requested source".to_string());
    }
    if input.source_text.trim().is_empty() {
        return Err("there is no closed context range to compress".to_string());
    }
    if input.source_summary_ids.iter().any(|id| id.trim().is_empty())
        || input.source_summary_ids.windows(2).any(|ids| ids[0] >= ids[1])
    {
        return Err("source summary ids must be unique, sorted, and non-empty".to_string());
    }

    let source_metadata = serde_json::to_string(&input.sources).map_err(|error| error.to_string())?;
    let protected_metadata = serde_json::to_string(&input.protected_facts).map_err(|error| error.to_string())?;
    let source_summary_metadata =
        serde_json::to_string(&input.source_summary_ids).map_err(|error| error.to_string())?;
    Ok(RangeCompressionRequest {
        model: model.to_string(),
        start_seq: input.start_seq,
        end_seq: input.end_seq,
        focus: input.focus.clone(),
        sources: input.sources,
        protected_facts: input.protected_facts,
        source_summary_ids: input.source_summary_ids,
        prompt: format!(
            "Summarize the closed context range for continuation. Return JSON only, matching this exact schema: {{\"schema_version\":{RANGE_SUMMARY_SCHEMA_VERSION},\"objective\":string,\"findings\":[string],\"decisions\":[string],\"paths\":[string],\"failures\":[string],\"verification\":[string],\"blockers\":[string],\"protected_facts\":[{{\"source_id\":string,\"text\":string}}],\"sources\":[{{\"sequence\":number,\"id\":string,\"content_hash\":number,\"recovery_handle\":string}}],\"source_summary_ids\":[string]}}. Do not invent task state. Copy every protected fact, source record, and source-summary id exactly.\n\nFocus: {}\nSources: {source_metadata}\nProtected facts: {protected_metadata}\nSource summaries: {}\n\n<source_context>\n{}\n</source_context>",
            input.focus, source_summary_metadata, input.source_text
        ),
    })
}

/// Parse and fail closed unless a typed response preserves required evidence.
pub fn validate_range_summary(
    request: &RangeCompressionRequest, response: &str,
) -> Result<RangeSummary, RangeSummaryValidationError> {
    let summary: RangeSummary =
        serde_json::from_str(response).map_err(|error| RangeSummaryValidationError::InvalidJson(error.to_string()))?;
    if summary.schema_version != RANGE_SUMMARY_SCHEMA_VERSION {
        return Err(RangeSummaryValidationError::UnsupportedSchema(summary.schema_version));
    }
    if summary.objective.trim().is_empty() {
        return Err(RangeSummaryValidationError::MissingObjective);
    }
    if summary.sources != request.sources {
        return Err(RangeSummaryValidationError::SourceMetadataMismatch);
    }
    if summary.source_summary_ids != request.source_summary_ids {
        return Err(RangeSummaryValidationError::SourceMetadataMismatch);
    }
    for fact in &request.protected_facts {
        if !summary.protected_facts.contains(fact) {
            return Err(RangeSummaryValidationError::MissingProtectedFact(fact.clone()));
        }
    }
    Ok(summary)
}

impl CompactionPolicy {
    /// Build a policy from parsed configuration.
    pub fn from_config(config: &CompactionConfig) -> Self {
        Self {
            mode: config.mode,
            review: config.review,
            threshold: config.threshold,
            keep_recent_tokens: config.keep_recent_tokens,
        }
    }

    /// Whether an idle `/compact` request may start a configured-model summary.
    pub fn allows_manual(self) -> bool {
        !matches!(self.mode, CompactionMode::Off)
    }

    /// Whether auto-compaction is eligible after normal selection.
    ///
    /// The strict comparison preserves the configured boundary: exactly the
    /// threshold does not compact; values above it may compact.
    pub fn should_auto_compact(self, budget: &ContextBudget) -> bool {
        matches!(self.mode, CompactionMode::Auto)
            && budget.used > self.auto_compaction_threshold(budget.available_input)
    }

    /// Resolve the configured percentage against an available input budget.
    pub fn auto_compaction_threshold(self, available_input: u64) -> u64 {
        available_input.saturating_mul(u64::from(self.threshold)) / 100
    }

    /// Whether a proposed summary requires user review before it becomes active.
    pub fn requires_review(self, risk: CompactionRisk) -> bool {
        match self.review {
            CompactionReview::Always => true,
            CompactionReview::Auto => matches!(risk, CompactionRisk::High),
            CompactionReview::Never => false,
        }
    }
}

/// Outcome of the preflight pressure check run before a main provider request.
///
/// Auto-compaction is a preflight gate ([plan](crate::context)): when a
/// submitted turn would exceed the context policy and `mode = "auto"` permits
/// compaction, the turn stops before the provider request, compacts with the
/// configured model, rebuilds context, and restarts the same user turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoCompactionDecision {
    /// The upcoming request fits the policy; send it to the provider.
    Send,
    /// The upcoming request is oversized and auto-compaction may compact first.
    ///
    /// The known-oversized request must never be sent to the main provider.
    Compact,
}

/// Decide whether an upcoming provider request needs auto-compaction first.
///
/// `prompt_token_estimate` is the conservative token estimate of the full
/// prompt that would be sent (system + context + user turn). The decision is
/// pure: given the policy, resolved model limits, and the estimate, it
/// returns [`AutoCompactionDecision::Compact`] only when auto mode is enabled
/// and the estimate exceeds the auto-compaction threshold.
///
pub fn preflight_auto_compaction(
    policy: CompactionPolicy, limits: &ModelContextLimits, prompt_token_estimate: u64,
) -> AutoCompactionDecision {
    let threshold = policy.auto_compaction_threshold(limits.available_input_budget());
    if matches!(policy.mode, CompactionMode::Auto) && prompt_token_estimate > threshold {
        AutoCompactionDecision::Compact
    } else {
        AutoCompactionDecision::Send
    }
}

fn serialize_threshold<S>(threshold: &u8, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("{threshold}%"))
}

fn deserialize_threshold<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let percent = value
        .strip_suffix('%')
        .ok_or_else(|| serde::de::Error::custom("compaction threshold must be a percentage such as `80%`"))?
        .parse::<u8>()
        .map_err(serde::de::Error::custom)?;
    if !(1..=100).contains(&percent) {
        return Err(serde::de::Error::custom(
            "compaction threshold must be between 1% and 100%",
        ));
    }
    Ok(percent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextBudget, ModelContextLimits, ModelLimitConfidence, ModelLimitSource, ReducerKind};

    fn budget(used: u64) -> ContextBudget {
        let limits = ModelContextLimits {
            provider: "test".to_string(),
            model: "test".to_string(),
            context_window: 101_024,
            max_completion_tokens: 0,
            recommended_completion_tokens: 0,
            source: ModelLimitSource::Fallback,
            confidence: ModelLimitConfidence::Conservative,
        };
        ContextBudget { limits, available_input: 100_000, target: 80_000, auto_compaction_threshold: 92_000, used }
    }

    #[test]
    fn defaults_are_manual_and_auto_review() {
        let config: ContextConfig = serde_json::from_str(r#"{"compaction":{}}"#).expect("parse config");
        assert_eq!(config.compaction.mode, CompactionMode::Manual);
        assert_eq!(config.compaction.review, CompactionReview::Auto);
        assert_eq!(config.compaction.threshold, 92);
        assert_eq!(config.compaction.keep_recent_tokens, 20_000);
        assert!(config.reduction.shadow);
        assert!(config.reduction.enabled_reducers().is_empty());
    }

    #[test]
    fn compaction_pressure_and_recent_tail_are_configurable() {
        let config: ContextConfig =
            serde_json::from_str(r#"{"compaction":{"mode":"auto","threshold":"80%","keep_recent_tokens":12000}}"#)
                .expect("parse config");
        let policy = CompactionPolicy::from_config(&config.compaction);

        assert_eq!(policy.threshold, 80);
        assert_eq!(policy.keep_recent_tokens, 12_000);
        assert!(!policy.should_auto_compact(&budget(80_000)));
        assert!(policy.should_auto_compact(&budget(80_001)));
    }

    #[test]
    fn compaction_threshold_requires_a_bounded_percentage() {
        for threshold in [r#""80""#, r#""0%""#, r#""101%""#] {
            let input = format!(r#"{{"compaction":{{"threshold":{threshold}}}}}"#);
            assert!(serde_json::from_str::<ContextConfig>(&input).is_err());
        }
    }

    #[test]
    fn reduction_switches_are_independently_configurable() {
        let config: ContextConfig = serde_json::from_str(
            r#"{
                "reduction": {
                    "shadow": false,
                    "terminal_control": true,
                    "progress_redraw": false,
                    "blank_run": true,
                    "repeated_line": false,
                    "command_result": true,
                    "failed_tool_input": true,
                    "max_blank_lines": 2
                }
            }"#,
        )
        .expect("parse reduction config");

        assert!(!config.reduction.shadow);
        assert!(config.reduction.command_result);
        assert!(config.reduction.failed_tool_input);
        assert_eq!(
            config.reduction.enabled_reducers(),
            vec![ReducerKind::TerminalControl, ReducerKind::BlankRun]
        );
    }

    #[test]
    fn config_accepts_each_mode_and_review_choice() {
        for mode in ["off", "manual", "auto"] {
            for review in ["always", "auto", "never"] {
                let config: ContextConfig =
                    serde_json::from_str(&format!(r#"{{"compaction":{{"mode":"{mode}","review":"{review}"}}}}"#))
                        .expect("parse config");
                assert_eq!(
                    config.compaction.mode,
                    match mode {
                        "off" => CompactionMode::Off,
                        "manual" => CompactionMode::Manual,
                        _ => CompactionMode::Auto,
                    }
                );
                assert_eq!(
                    config.compaction.review,
                    match review {
                        "always" => CompactionReview::Always,
                        "auto" => CompactionReview::Auto,
                        _ => CompactionReview::Never,
                    }
                );
            }
        }
    }

    #[test]
    fn auto_compaction_starts_only_above_92_percent() {
        let policy = CompactionPolicy { mode: CompactionMode::Auto, ..Default::default() };
        assert!(!policy.should_auto_compact(&budget(92_000)));
        assert!(policy.should_auto_compact(&budget(92_001)));
        assert!(
            !CompactionPolicy { mode: CompactionMode::Manual, ..Default::default() }
                .should_auto_compact(&budget(100_000))
        );
    }

    #[test]
    fn high_risk_covers_every_required_signal() {
        for signals in [
            CompactionRiskSignals { has_tool_output_or_diff: true, ..Default::default() },
            CompactionRiskSignals { has_failure_or_permission: true, ..Default::default() },
            CompactionRiskSignals { has_correction_or_unresolved_work: true, ..Default::default() },
        ] {
            assert_eq!(signals.classify(), CompactionRisk::High);
        }
        assert_eq!(CompactionRiskSignals::default().classify(), CompactionRisk::Low);
    }

    #[test]
    fn review_policy_respects_all_choices() {
        assert!(
            CompactionPolicy { review: CompactionReview::Always, ..Default::default() }
                .requires_review(CompactionRisk::Low)
        );
        assert!(CompactionPolicy::default().requires_review(CompactionRisk::High));
        assert!(!CompactionPolicy::default().requires_review(CompactionRisk::Low));
        assert!(
            !CompactionPolicy { review: CompactionReview::Never, ..Default::default() }
                .requires_review(CompactionRisk::High)
        );
    }

    #[test]
    fn manual_request_uses_the_configured_model_and_is_non_mutating_on_error() {
        let policy = CompactionPolicy::default();
        let request = prepare_manual_compaction(policy, "provider/model", "fixed the parser", "session:12..47")
            .expect("prepare request");
        assert_eq!(request.model, "provider/model");
        assert_eq!(request.recovery_handle, "session:12..47");
        assert!(request.prompt.contains("fixed the parser"));
        assert!(prepare_manual_compaction(policy, "", "fixed the parser", "session:12..47").is_err());
        assert!(prepare_manual_compaction(policy, "provider/model", "", "session:12..47").is_err());
    }

    fn range_request() -> RangeCompressionRequest {
        prepare_range_compression(
            CompactionPolicy { review: CompactionReview::Always, ..Default::default() },
            "provider/model",
            RangeCompressionInput {
                start_seq: 4,
                end_seq: 5,
                focus: "continue the parser repair".to_string(),
                sources: vec![
                    RangeSource {
                        sequence: 4,
                        id: "ctx:4".to_string(),
                        content_hash: 4,
                        recovery_handle: "session:4".to_string(),
                    },
                    RangeSource {
                        sequence: 5,
                        id: "ctx:5".to_string(),
                        content_hash: 5,
                        recovery_handle: "session:5".to_string(),
                    },
                ],
                protected_facts: vec![ProtectedFact {
                    source_id: "ctx:5".to_string(),
                    text: "the write remains unverified".to_string(),
                }],
                source_summary_ids: vec![],
                source_text: "user: repair parser\ntool: write failed".to_string(),
            },
        )
        .expect("build range request")
    }

    #[test]
    fn typed_range_summary_requires_exact_sources_and_protected_facts() {
        let request = range_request();
        let summary = RangeSummary {
            schema_version: RANGE_SUMMARY_SCHEMA_VERSION,
            objective: "repair parser".to_string(),
            findings: vec![],
            decisions: vec![],
            paths: vec![],
            failures: vec!["write failed".to_string()],
            verification: vec![],
            blockers: vec![],
            protected_facts: request.protected_facts.clone(),
            sources: request.sources.clone(),
            source_summary_ids: vec![],
        };
        let response = serde_json::to_string(&summary).expect("serialize summary");
        assert_eq!(validate_range_summary(&request, &response), Ok(summary));

        let missing_fact = response.replace("the write remains unverified", "omitted");
        assert!(matches!(
            validate_range_summary(&request, &missing_fact),
            Err(RangeSummaryValidationError::MissingProtectedFact(_))
        ));
        let missing_source = response.replace("\"content_hash\":5", "\"content_hash\":6");
        assert_eq!(
            validate_range_summary(&request, &missing_source),
            Err(RangeSummaryValidationError::SourceMetadataMismatch)
        );

        let mut request_with_summary = request;
        request_with_summary.source_summary_ids = vec!["ctx_summary_previous".to_string()];
        assert_eq!(
            validate_range_summary(&request_with_summary, &response),
            Err(RangeSummaryValidationError::SourceMetadataMismatch)
        );
    }

    #[test]
    fn range_request_rejects_non_contiguous_or_unrecoverable_sources() {
        let mut request = range_request();
        request.sources.pop();
        assert!(
            prepare_range_compression(
                CompactionPolicy { review: CompactionReview::Always, ..Default::default() },
                "provider/model",
                RangeCompressionInput {
                    start_seq: 4,
                    end_seq: 5,
                    focus: "focus".to_string(),
                    sources: request.sources,
                    protected_facts: vec![],
                    source_summary_ids: vec![],
                    source_text: "source".to_string(),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn range_request_rejects_duplicate_or_out_of_order_source_metadata() {
        let mut request = range_request();
        request.sources[1].sequence = 7;
        assert!(
            prepare_range_compression(
                CompactionPolicy { review: CompactionReview::Always, ..Default::default() },
                "provider/model",
                RangeCompressionInput {
                    start_seq: 4,
                    end_seq: 5,
                    focus: "focus".to_string(),
                    sources: request.sources,
                    protected_facts: vec![],
                    source_summary_ids: vec![],
                    source_text: "source".to_string(),
                },
            )
            .is_err()
        );
    }

    fn limits_for(context_window: u64) -> ModelContextLimits {
        ModelContextLimits {
            provider: "test".to_string(),
            model: "test".to_string(),
            context_window,
            max_completion_tokens: 1_024,
            recommended_completion_tokens: 512,
            source: ModelLimitSource::Fallback,
            confidence: ModelLimitConfidence::Conservative,
        }
    }

    #[test]
    fn preflight_compacts_only_in_auto_mode_above_threshold() {
        let auto = CompactionPolicy { mode: CompactionMode::Auto, ..Default::default() };
        let manual = CompactionPolicy::default();
        let off = CompactionPolicy { mode: CompactionMode::Off, ..Default::default() };
        let limits = limits_for(101_024);
        let threshold = limits.auto_compaction_threshold();
        assert_eq!(
            preflight_auto_compaction(auto, &limits, threshold),
            AutoCompactionDecision::Send
        );
        assert_eq!(
            preflight_auto_compaction(auto, &limits, threshold + 1),
            AutoCompactionDecision::Compact
        );
        assert_eq!(
            preflight_auto_compaction(manual, &limits, threshold + 1),
            AutoCompactionDecision::Send,
            "manual mode never auto-compacts"
        );
        assert_eq!(
            preflight_auto_compaction(off, &limits, threshold + 1),
            AutoCompactionDecision::Send,
            "off mode never auto-compacts"
        );
    }
}
