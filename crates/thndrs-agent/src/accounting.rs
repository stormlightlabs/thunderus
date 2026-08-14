//! Provider-neutral request size and usage accounting.
//!
//! Accounting values deliberately carry their origin. Bytes are measured at
//! the serialized request boundary, estimates identify the heuristic version,
//! and provider values distinguish reported components from derived totals.
//! This module never stores provider payloads or request content.

use serde::{Deserialize, Serialize};

use crate::context::{ContextItem, ContextLifecycleState, ContextProtection, ContextRelation, ContextVisibility};

/// Version of the conservative serialized-byte token estimator.
pub const TOKEN_ESTIMATOR_VERSION: &str = "utf8-bytes-divisor-3-overhead-16-v1";

/// Version of the provider usage normalization rules.
pub const USAGE_NORMALIZATION_VERSION: &str = "provider-inclusive-input-v1";

/// Maximum model-projection bytes retained on an in-memory request event.
///
/// The projection is deliberately skipped by accounting serialization. It is
/// supplied to the application for inspection/export and is not session truth.
pub const MODEL_PROJECTION_MAX_BYTES: usize = 128 * 1024;

/// Why a measured value is known.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeasurementProvenance {
    /// Exact bytes from a named serialization boundary.
    ExactSerialized {
        /// Serialization boundary name.
        boundary: String,
    },
    /// Conservative local estimate.
    Estimated {
        /// Estimator name.
        estimator: String,
        /// Estimator version.
        version: String,
    },
    /// A component returned by a provider.
    ProviderReported {
        /// Provider label.
        provider: String,
        /// Provider component name.
        component: String,
    },
    /// A value derived from provider-reported components.
    Derived {
        /// Normalization rule name.
        rule: String,
        /// Normalization rule version.
        version: String,
    },
    /// The provider did not report this component.
    Unknown,
}

/// A byte measurement with its serialization boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ByteMeasurement {
    /// Measured UTF-8 request bytes.
    pub value: u64,
    /// Boundary and serialization provenance.
    pub provenance: MeasurementProvenance,
}

/// A token value which can remain unknown without conflating unknown and zero.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenMeasurement {
    /// Token count, or `None` when it was not available.
    pub value: Option<u64>,
    /// Estimator, provider component, or derivation provenance.
    pub provenance: MeasurementProvenance,
}

impl TokenMeasurement {
    /// Return an unknown token value.
    pub const fn unknown() -> Self {
        Self { value: None, provenance: MeasurementProvenance::Unknown }
    }

    /// Return a provider-reported component. Zero is intentionally retained.
    pub fn provider(provider: &str, component: &str, value: Option<u64>) -> Self {
        Self {
            value,
            provenance: if value.is_some() {
                MeasurementProvenance::ProviderReported {
                    provider: provider.to_string(),
                    component: component.to_string(),
                }
            } else {
                MeasurementProvenance::Unknown
            },
        }
    }
}

/// Provider-specific usage components before normalization.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageComponents {
    /// Provider input/prompt tokens.
    pub input_tokens: Option<u64>,
    /// Provider output/completion tokens.
    pub output_tokens: Option<u64>,
    /// Input tokens read from a provider cache.
    pub cache_read_input_tokens: Option<u64>,
    /// Input tokens written to a provider cache.
    pub cache_creation_input_tokens: Option<u64>,
    /// Output reasoning tokens, when the provider exposes the breakdown.
    pub reasoning_tokens: Option<u64>,
}

impl ProviderUsageComponents {
    /// Build the common input/output portion of a usage snapshot.
    pub const fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            reasoning_tokens: None,
        }
    }

    /// Merge a provider snapshot without double-counting repeated stream updates.
    pub fn merge_snapshot(&mut self, next: &Self) {
        merge_max(&mut self.input_tokens, next.input_tokens);
        merge_max(&mut self.output_tokens, next.output_tokens);
        merge_max(&mut self.cache_read_input_tokens, next.cache_read_input_tokens);
        merge_max(&mut self.cache_creation_input_tokens, next.cache_creation_input_tokens);
        merge_max(&mut self.reasoning_tokens, next.reasoning_tokens);
    }

    /// Normalize components using the provider's documented accounting rule.
    pub fn normalize(&self, provider: &str, rule: ProviderUsageRule) -> ProviderUsage {
        let inclusive_input_tokens = match rule {
            ProviderUsageRule::AnthropicMessages => self.input_tokens.map(|input| {
                input
                    .saturating_add(self.cache_read_input_tokens.unwrap_or(0))
                    .saturating_add(self.cache_creation_input_tokens.unwrap_or(0))
            }),
            ProviderUsageRule::OpenAiChat | ProviderUsageRule::OpenAiResponses => self.input_tokens,
        };
        ProviderUsage {
            provider: provider.to_string(),
            rule,
            components: self.clone(),
            inclusive_input_tokens: TokenMeasurement {
                value: inclusive_input_tokens,
                provenance: if inclusive_input_tokens.is_some() {
                    MeasurementProvenance::Derived {
                        rule: rule.label().to_string(),
                        version: USAGE_NORMALIZATION_VERSION.to_string(),
                    }
                } else {
                    MeasurementProvenance::Unknown
                },
            },
        }
    }
}

fn merge_max(current: &mut Option<u64>, next: Option<u64>) {
    if let Some(next) = next {
        *current = Some(current.map_or(next, |current| current.max(next)));
    }
}

/// Provider rule used to derive inclusive input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUsageRule {
    /// Anthropic input excludes cache-read and cache-creation components.
    AnthropicMessages,
    /// OpenAI-compatible prompt tokens already include cached input.
    OpenAiChat,
    /// Responses input tokens already include cached input.
    OpenAiResponses,
}

impl ProviderUsageRule {
    /// Stable rule label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::AnthropicMessages => "anthropic_input_plus_cache_components",
            Self::OpenAiChat => "openai_prompt_tokens_inclusive",
            Self::OpenAiResponses => "openai_responses_input_tokens_inclusive",
        }
    }
}

/// Normalized provider usage for one completed request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsage {
    /// Provider adapter label.
    pub provider: String,
    /// Rule used to derive the inclusive input total.
    pub rule: ProviderUsageRule,
    /// Raw components retained exactly when reported.
    pub components: ProviderUsageComponents,
    /// Derived inclusive input total.
    pub inclusive_input_tokens: TokenMeasurement,
}

impl ProviderUsage {
    /// Provider-reported input that was not served from cache, when derivable.
    pub fn fresh_input_tokens(&self) -> Option<u64> {
        let input = self.components.input_tokens?;
        match self.rule {
            ProviderUsageRule::AnthropicMessages => Some(input),
            ProviderUsageRule::OpenAiChat | ProviderUsageRule::OpenAiResponses => self
                .components
                .cache_read_input_tokens
                .map(|cached| input.saturating_sub(cached)),
        }
    }
}

/// One bounded provider-neutral message in the model-facing request projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelProjectionMessage {
    /// Message role at the provider-neutral boundary.
    pub role: String,
    /// Bounded rendered content. Structured content is represented as JSON.
    pub content: String,
}

/// One deterministic reduction receipt proposed for a request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextReductionReceipt {
    /// Stable context item id.
    pub item_id: String,
    /// Reducer or selection method name.
    pub method: String,
    /// Reducer method version.
    pub version: String,
    /// Bytes before the proposed decision.
    pub before_bytes: u64,
    /// Bytes after the proposed decision.
    pub after_bytes: u64,
    /// Whether the proposed decision may remove information.
    pub lossy: bool,
    /// Whether the reducer was measured only, applied, or rejected in favor
    /// of the baseline projection.
    #[serde(default)]
    pub mode: ContextReductionMode,
    /// Bounded diagnostic when a preservation gate rejected the candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Lifecycle of one deterministic reduction decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextReductionMode {
    /// The candidate was measured but was not used in the request.
    #[default]
    Shadow,
    /// The candidate became the model-visible projection.
    Applied,
    /// The candidate failed a preservation gate and baseline remained active.
    BaselineFallback,
}

impl ContextReductionMode {
    /// Stable label used by `/usage`, exports, and model dashboards.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Applied => "applied",
            Self::BaselineFallback => "baseline_fallback",
        }
    }
}

/// One context candidate captured at the final request boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextItemSnapshot {
    /// Stable context item id.
    pub id: String,
    /// Stable handle for bounded redacted recovery, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_handle: Option<String>,
    /// State at the request boundary.
    pub state: ContextVisibility,
    /// Stable policy reason code.
    pub reason_code: String,
    /// Redacted human-readable reason.
    pub reason: String,
    /// Lifecycle state independent of request visibility.
    #[serde(default)]
    pub lifecycle: ContextLifecycleState,
    /// Conservative protection reasons at the final request boundary.
    #[serde(default)]
    pub protection: ContextProtection,
    /// Explicit relations known at the final request boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<ContextRelation>,
}

impl From<&ContextItem> for ContextItemSnapshot {
    fn from(item: &ContextItem) -> Self {
        Self {
            id: item.id.clone(),
            artifact_handle: item.artifact_handle.clone(),
            state: item.visibility.clone(),
            reason_code: item.reason_code.clone(),
            reason: item.reason.clone(),
            lifecycle: item.lifecycle.state,
            protection: item.lifecycle.protection.clone(),
            relations: item.lifecycle.relations.clone(),
        }
    }
}

/// Build a one-pass snapshot of every candidate in a context ledger.
pub fn snapshot_context(items: &[ContextItem]) -> Vec<ContextItemSnapshot> {
    items.iter().map(ContextItemSnapshot::from).collect()
}

/// Accounting for one successful serialized provider request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequestAccounting {
    /// Session turn that owns this request.
    pub turn_id: String,
    /// Stable request identity across event delivery and session persistence.
    pub request_id: String,
    /// One-based retry attempt for this request identity.
    pub attempt: u32,
    /// Provider adapter label.
    pub provider: String,
    /// Selected model id.
    pub model: String,
    /// Exact serialized request body size.
    pub serialized_bytes: ByteMeasurement,
    /// Conservative input estimate from the same serialized bytes.
    pub estimated_input_tokens: TokenMeasurement,
    /// Provider usage, when the successful response reported it.
    pub provider_usage: Option<ProviderUsage>,
    /// Tool calls returned by this provider operation, once its response is complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_count: Option<u64>,
    /// All context candidates considered for this request, once each.
    pub context: Vec<ContextItemSnapshot>,
    /// Shadow reducer receipts for this request.
    #[serde(default)]
    pub shadow_receipts: Vec<ContextReductionReceipt>,
    /// Reducer receipts for candidates applied to this request's model
    /// projection.
    #[serde(default)]
    pub applied_receipts: Vec<ContextReductionReceipt>,
    /// Reducer receipts whose candidates failed a preservation gate and fell
    /// back to the baseline projection.
    #[serde(default)]
    pub fallback_receipts: Vec<ContextReductionReceipt>,
    /// In-memory model projection for the selected request.
    ///
    /// This is not durable session data and is skipped during serialization;
    /// the application may use it to build an explicit bounded export.
    #[serde(skip)]
    pub model_projection: Vec<ModelProjectionMessage>,
}

impl ProviderRequestAccounting {
    /// Create accounting from the exact serialized request body.
    pub fn from_serialized_request(
        turn_id: impl Into<String>, request_id: impl Into<String>, attempt: u32, provider: &str, model: &str,
        bytes: &[u8], context: Vec<ContextItemSnapshot>,
    ) -> Self {
        let byte_count = bytes.len() as u64;
        Self {
            turn_id: turn_id.into(),
            request_id: request_id.into(),
            attempt,
            provider: provider.to_string(),
            model: model.to_string(),
            serialized_bytes: ByteMeasurement {
                value: byte_count,
                provenance: MeasurementProvenance::ExactSerialized { boundary: "provider_request_body".to_string() },
            },
            estimated_input_tokens: TokenMeasurement {
                value: Some(estimate_serialized_tokens(byte_count)),
                provenance: MeasurementProvenance::Estimated {
                    estimator: "utf8_bytes_divisor_3_plus_item_overhead".to_string(),
                    version: TOKEN_ESTIMATOR_VERSION.to_string(),
                },
            },
            provider_usage: None,
            tool_count: None,
            context,
            shadow_receipts: Vec::new(),
            applied_receipts: Vec::new(),
            fallback_receipts: Vec::new(),
            model_projection: Vec::new(),
        }
    }

    /// Attach reduction receipts while keeping shadow and applied decisions
    /// separately inspectable.
    pub fn with_reduction_receipts(mut self, receipts: Vec<ContextReductionReceipt>) -> Self {
        for receipt in receipts {
            match receipt.mode {
                ContextReductionMode::Shadow => self.shadow_receipts.push(receipt),
                ContextReductionMode::Applied => self.applied_receipts.push(receipt),
                ContextReductionMode::BaselineFallback => self.fallback_receipts.push(receipt),
            }
        }
        self
    }

    /// Return all reduction receipts, grouped into shadow and applied/fallback
    /// decisions for export and inspection.
    pub fn reduction_receipts(&self) -> Vec<ContextReductionReceipt> {
        self.shadow_receipts
            .iter()
            .chain(&self.applied_receipts)
            .chain(&self.fallback_receipts)
            .cloned()
            .collect()
    }

    /// Attach a bounded in-memory model projection without changing durable
    /// accounting or provider request bytes.
    pub fn with_model_projection(mut self, projection: Vec<ModelProjectionMessage>) -> Self {
        let mut bytes = 0usize;
        self.model_projection = projection
            .into_iter()
            .filter_map(|mut message| {
                let remaining = MODEL_PROJECTION_MAX_BYTES.saturating_sub(bytes);
                if remaining == 0 {
                    return None;
                }
                message.content = truncate_utf8(&message.content, remaining);
                bytes = bytes
                    .saturating_add(message.role.len())
                    .saturating_add(message.content.len());
                Some(message)
            })
            .collect();
        self
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Estimate input tokens from serialized request bytes.
pub const fn estimate_serialized_tokens(bytes: u64) -> u64 {
    bytes.div_ceil(3).saturating_add(16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_components_preserve_unknown_and_measured_zero() {
        let components = ProviderUsageComponents {
            input_tokens: Some(0),
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: Some(0),
            reasoning_tokens: None,
        };
        assert_eq!(components.input_tokens, Some(0));
        assert_eq!(components.output_tokens, None);
        assert_eq!(components.cache_creation_input_tokens, Some(0));
    }

    #[test]
    fn repeated_stream_snapshots_are_not_added_twice() {
        let mut total = ProviderUsageComponents::new(12, 3);
        total.merge_snapshot(&ProviderUsageComponents::new(12, 3));
        total.merge_snapshot(&ProviderUsageComponents::new(15, 4));
        assert_eq!(total.input_tokens, Some(15));
        assert_eq!(total.output_tokens, Some(4));
    }

    #[test]
    fn provider_rules_normalize_anthropic_and_openai_inputs_differently() {
        let components = ProviderUsageComponents {
            input_tokens: Some(100),
            output_tokens: Some(10),
            cache_read_input_tokens: Some(20),
            cache_creation_input_tokens: Some(5),
            reasoning_tokens: Some(2),
        };
        assert_eq!(
            components
                .normalize("anthropic", ProviderUsageRule::AnthropicMessages)
                .inclusive_input_tokens
                .value,
            Some(125)
        );
        assert_eq!(
            components
                .normalize("openai", ProviderUsageRule::OpenAiChat)
                .inclusive_input_tokens
                .value,
            Some(100)
        );
        assert_eq!(
            components
                .normalize("anthropic", ProviderUsageRule::AnthropicMessages)
                .fresh_input_tokens(),
            Some(100)
        );
        assert_eq!(
            components
                .normalize("openai", ProviderUsageRule::OpenAiChat)
                .fresh_input_tokens(),
            Some(80)
        );
        let without_cache =
            ProviderUsageComponents::new(100, 10).normalize("openai", ProviderUsageRule::OpenAiResponses);
        assert_eq!(without_cache.fresh_input_tokens(), None);
    }

    #[test]
    fn request_accounting_records_exact_bytes_and_context_once() {
        let item = ContextItem {
            id: "item-1".to_string(),
            kind: crate::context::ContextItemKind::Transcript,
            label: "turn".to_string(),
            source_path: None,
            scope: ".".to_string(),
            content_hash: None,
            artifact_handle: None,
            byte_count: 4,
            content: None,
            token_estimate: 18,
            visibility: ContextVisibility::Visible,
            reason_code: "recent_transcript".to_string(),
            reason: "recent transcript entry".to_string(),
            lifecycle: crate::context::ContextLifecycle::default(),
        };
        let context = snapshot_context(std::slice::from_ref(&item));
        let accounting = ProviderRequestAccounting::from_serialized_request(
            "turn_1",
            "turn_1:request:0",
            1,
            "provider",
            "model",
            b"{}",
            context,
        );
        assert_eq!(accounting.serialized_bytes.value, 2);
        assert_eq!(accounting.estimated_input_tokens.value, Some(17));
        assert_eq!(accounting.context.len(), 1);
        assert_eq!(accounting.context[0].reason_code, "recent_transcript");
    }

    #[test]
    fn reduction_receipts_remain_inspectable_by_decision_mode() {
        let receipt = |method: &str, mode: ContextReductionMode| ContextReductionReceipt {
            item_id: "tool:1".to_string(),
            method: method.to_string(),
            version: "test-v1".to_string(),
            before_bytes: 10,
            after_bytes: 5,
            lossy: false,
            mode,
            diagnostic: None,
        };
        let accounting = ProviderRequestAccounting::from_serialized_request(
            "turn_1",
            "turn_1:request:0",
            1,
            "provider",
            "model",
            b"{}",
            Vec::new(),
        )
        .with_reduction_receipts(vec![
            receipt("terminal", ContextReductionMode::Shadow),
            receipt("blank", ContextReductionMode::Applied),
            receipt("repeat", ContextReductionMode::BaselineFallback),
        ]);

        assert_eq!(accounting.shadow_receipts.len(), 1);
        assert_eq!(accounting.applied_receipts.len(), 1);
        assert_eq!(accounting.fallback_receipts.len(), 1);
        assert_eq!(accounting.reduction_receipts().len(), 3);
        let json = serde_json::to_string(&accounting).expect("accounting serializes");
        assert!(json.contains("baseline_fallback"));
        assert!(!json.contains("model_projection"));
    }
}
