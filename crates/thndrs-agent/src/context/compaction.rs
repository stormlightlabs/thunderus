//! Pure compaction configuration, pressure policy, and risk classification.

use serde::{Deserialize, Serialize};

use super::{ContextBudget, ModelContextLimits};

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
}

/// Compaction configuration parsed from `[context.compaction]`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompactionConfig {
    /// Whether compaction is disabled, manually requested, or automatic.
    pub mode: CompactionMode,
    /// When a generated summary needs user review before becoming active.
    pub review: CompactionReview,
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

impl CompactionPolicy {
    /// Build a policy from parsed configuration.
    pub fn from_config(config: &CompactionConfig) -> Self {
        Self { mode: config.mode, review: config.review }
    }

    /// Whether an idle `/compact` request may start a configured-model summary.
    pub fn allows_manual(self) -> bool {
        !matches!(self.mode, CompactionMode::Off)
    }

    /// Whether auto-compaction is eligible after normal selection.
    ///
    /// The strict comparison deliberately preserves the specified 92% boundary:
    /// exactly 92% does not compact; values above it may compact.
    pub fn should_auto_compact(self, budget: &ContextBudget) -> bool {
        matches!(self.mode, CompactionMode::Auto) && budget.exceeds_auto_compaction()
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
/// Uses [`ModelContextLimits::auto_compaction_threshold`] directly so the
/// 92% boundary stays consistent with budget-based pressure checks.
pub fn preflight_auto_compaction(
    policy: CompactionPolicy, limits: &ModelContextLimits, prompt_token_estimate: u64,
) -> AutoCompactionDecision {
    if matches!(policy.mode, CompactionMode::Auto) && prompt_token_estimate > limits.auto_compaction_threshold() {
        AutoCompactionDecision::Compact
    } else {
        AutoCompactionDecision::Send
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextBudget, ModelContextLimits, ModelLimitConfidence, ModelLimitSource};

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
        let policy = CompactionPolicy { mode: CompactionMode::Auto, review: CompactionReview::Auto };
        assert!(!policy.should_auto_compact(&budget(92_000)));
        assert!(policy.should_auto_compact(&budget(92_001)));
        assert!(
            !CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Auto }
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
            CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Always }
                .requires_review(CompactionRisk::Low)
        );
        assert!(
            CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Auto }
                .requires_review(CompactionRisk::High)
        );
        assert!(
            !CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Auto }
                .requires_review(CompactionRisk::Low)
        );
        assert!(
            !CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Never }
                .requires_review(CompactionRisk::High)
        );
    }

    #[test]
    fn manual_request_uses_the_configured_model_and_is_non_mutating_on_error() {
        let policy = CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Auto };
        let request = prepare_manual_compaction(policy, "provider/model", "fixed the parser", "session:12..47")
            .expect("prepare request");
        assert_eq!(request.model, "provider/model");
        assert_eq!(request.recovery_handle, "session:12..47");
        assert!(request.prompt.contains("fixed the parser"));
        assert!(prepare_manual_compaction(policy, "", "fixed the parser", "session:12..47").is_err());
        assert!(prepare_manual_compaction(policy, "provider/model", "", "session:12..47").is_err());
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
        let auto = CompactionPolicy { mode: CompactionMode::Auto, review: CompactionReview::Auto };
        let manual = CompactionPolicy { mode: CompactionMode::Manual, review: CompactionReview::Auto };
        let off = CompactionPolicy { mode: CompactionMode::Off, review: CompactionReview::Auto };
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
