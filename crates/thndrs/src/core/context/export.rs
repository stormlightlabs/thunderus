//! Versioned, bounded context inspection and export projections.
//!
//! The export model is application-owned because artifact storage and
//! redaction belong to the host. It contains context metadata and the selected
//! model projection, never raw provider payloads or unbounded artifact bodies.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use thndrs_agent::accounting::{
    ContextReductionReceipt, MeasurementProvenance, ModelProjectionMessage, ProviderRequestAccounting,
};
use thndrs_agent::context::{
    ContextItem, ContextItemKind, ContextLedger, ContextLifecycleState, ContextProtection, ContextRelation,
    ContextRelationKind, ContextVisibility,
};

use crate::artifacts::{ArtifactMetadata, ArtifactRecovery};
use crate::tools::shell::redact_secrets;

/// Version of the user-facing context export contract.
pub const CONTEXT_EXPORT_SCHEMA_VERSION: &str = "context-export-v1";
/// Version of the bounded export redaction/cap policy.
pub const CONTEXT_EXPORT_POLICY_VERSION: &str = "redacted-bounded-v1";
/// Maximum bytes in one exported text field after redaction.
pub const EXPORT_FIELD_MAX_BYTES: usize = 16 * 1024;
/// Maximum bytes in the rendered model projection.
pub const EXPORT_PROJECTION_MAX_BYTES: usize = 128 * 1024;

/// Output format for a context export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextExportFormat {
    /// Deterministic versioned JSON.
    Json,
    /// Deterministic human-readable Markdown.
    Markdown,
}

impl ContextExportFormat {
    /// Parse a user-facing format label.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "markdown" | "md" => Some(Self::Markdown),
            _ => None,
        }
    }

    /// Stable format label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Markdown => "markdown",
        }
    }
}

/// One model-visible message included in an export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportProjectionMessage {
    /// Provider-neutral message role.
    pub role: String,
    /// Redacted and bounded rendered content.
    pub content: String,
}

/// Content-free context item details shown by `/context` and export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportContextItem {
    /// Stable context item id.
    pub id: String,
    /// Context item kind.
    pub kind: ContextItemKind,
    /// Item visibility at inspection time.
    pub state: ContextVisibility,
    /// Lifecycle state independent of request visibility.
    pub lifecycle: ContextLifecycleState,
    /// Stable policy reason code.
    pub reason_code: String,
    /// Redacted policy explanation.
    pub reason: String,
    /// Replacement context id, when one exists.
    pub replacement: Option<String>,
    /// Whether explicit protection reasons remain.
    pub protected: bool,
    /// Explicit protection reasons.
    pub protection: ContextProtection,
    /// Whether the protection was explicitly released.
    pub protection_released: bool,
    /// Verification relation, when one exists.
    pub verification: Option<String>,
    /// All explicit relations known for this item.
    pub relations: Vec<ContextRelation>,
    /// Whether bounded redacted evidence can be recovered.
    pub recovery_available: bool,
    /// Recovery handle, when available.
    pub recovery_handle: Option<String>,
    /// Original item byte count.
    pub byte_count: usize,
    /// Selection token estimate.
    pub token_estimate: usize,
    /// Redacted display label.
    pub label: String,
}

/// Export-side artifact metadata and optional bounded body.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportArtifact {
    /// Stable artifact handle.
    pub handle: String,
    /// Artifact metadata, if its sidecar was readable.
    pub metadata: Option<ArtifactMetadata>,
    /// Bounded redacted body; absent unless explicitly requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Safe recovery diagnostic, if the artifact is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

/// Versioned export of one selected request and its context ledger.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextExport {
    /// Export schema version.
    pub schema_version: String,
    /// Redaction and bounding policy version.
    pub policy_version: String,
    /// Session identity.
    pub session_id: String,
    /// Selected request accounting, when a provider request has completed.
    pub accounting: Option<ProviderRequestAccounting>,
    /// Context budget at inspection time.
    pub budget: ExportBudget,
    /// Ordered context candidate metadata.
    pub items: Vec<ExportContextItem>,
    /// Bounded model-facing projection for the selected request.
    pub model_projection: Vec<ExportProjectionMessage>,
    /// Shadow, applied, and baseline-fallback reduction receipts.
    pub receipts: Vec<ContextReductionReceipt>,
    /// Export-safe diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Artifact metadata and optional explicitly requested bodies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ExportArtifact>,
}

/// Context budget metadata included in an export.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportBudget {
    /// Estimated rendered tokens.
    pub used: u64,
    /// Selection target.
    pub target: u64,
    /// Available input budget.
    pub available_input: u64,
    /// Automatic compaction threshold.
    pub auto_compaction_threshold: u64,
    /// Provider/model limit provenance.
    pub limits_source: String,
    /// Provider/model limit confidence.
    pub limits_confidence: String,
}

impl ContextExport {
    /// Build a redacted export from one ledger and the selected request.
    pub fn from_parts(
        session_id: impl Into<String>, ledger: &ContextLedger, accounting: Option<ProviderRequestAccounting>,
        artifacts: Vec<ExportArtifact>, diagnostics: Vec<String>,
    ) -> Self {
        let model_projection = accounting
            .as_ref()
            .map(|accounting| accounting.model_projection.iter().map(redact_projection).collect())
            .unwrap_or_default();
        let receipts = accounting
            .as_ref()
            .map(ProviderRequestAccounting::reduction_receipts)
            .unwrap_or_default();
        Self {
            schema_version: CONTEXT_EXPORT_SCHEMA_VERSION.to_string(),
            policy_version: CONTEXT_EXPORT_POLICY_VERSION.to_string(),
            session_id: session_id.into(),
            accounting,
            budget: ExportBudget {
                used: ledger.budget.used,
                target: ledger.budget.target,
                available_input: ledger.budget.available_input,
                auto_compaction_threshold: ledger.budget.auto_compaction_threshold,
                limits_source: ledger.budget.limits.source.label().to_string(),
                limits_confidence: ledger.budget.limits.confidence.label().to_string(),
            },
            items: ledger.items.iter().map(export_item).collect(),
            model_projection: cap_projection(model_projection),
            receipts,
            diagnostics: diagnostics
                .into_iter()
                .map(|diagnostic| cap_text(&diagnostic))
                .collect(),
            artifacts: artifacts.into_iter().map(bound_artifact).collect(),
        }
    }

    /// Serialize this export as deterministic pretty JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Render this same typed export as deterministic Markdown.
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();
        let _ = writeln!(
            output,
            "# Context export\n\n- Schema: `{}`\n- Policy: `{}`\n- Session: `{}`",
            self.schema_version, self.policy_version, self.session_id
        );
        let _ = writeln!(
            output,
            "\n## Budget\n\n- Used: {} estimated tokens\n- Target: {} estimated tokens\n- Available input: {} estimated tokens\n- Auto-compaction threshold: {} estimated tokens\n- Limits: {} ({})",
            self.budget.used,
            self.budget.target,
            self.budget.available_input,
            self.budget.auto_compaction_threshold,
            self.budget.limits_source,
            self.budget.limits_confidence
        );
        if let Some(accounting) = &self.accounting {
            let _ = writeln!(
                output,
                "\n## Request\n\n- Request: `{}`\n- Turn: `{}`\n- Attempt: {}\n- Provider/model: `{}` / `{}`\n- Serialized bytes: {}\n- Estimated input tokens: {}",
                accounting.request_id,
                accounting.turn_id,
                accounting.attempt,
                accounting.provider,
                accounting.model,
                accounting.serialized_bytes.value,
                display_measurement(
                    &accounting.estimated_input_tokens.value,
                    &accounting.estimated_input_tokens.provenance
                )
            );
            if let Some(usage) = &accounting.provider_usage {
                let _ = writeln!(
                    output,
                    "- Provider input/output: {} / {}\n- Cache read/create: {} / {}\n- Reasoning: {}\n- Inclusive input: {} ({})",
                    display_optional(usage.components.input_tokens),
                    display_optional(usage.components.output_tokens),
                    display_optional(usage.components.cache_read_input_tokens),
                    display_optional(usage.components.cache_creation_input_tokens),
                    display_optional(usage.components.reasoning_tokens),
                    display_optional(usage.inclusive_input_tokens.value),
                    usage.rule.label()
                );
                if let (Some(estimate), Some(provider)) = (
                    accounting.estimated_input_tokens.value,
                    usage.inclusive_input_tokens.value,
                ) {
                    let _ = writeln!(
                        output,
                        "- Estimate error: {} tokens",
                        provider as i128 - estimate as i128
                    );
                }
            } else {
                let _ = writeln!(output, "- Provider usage: unknown");
            }
            let shadow = self
                .receipts
                .iter()
                .filter(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Shadow)
                .count();
            let applied = self
                .receipts
                .iter()
                .filter(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::Applied)
                .count();
            let fallback = self
                .receipts
                .iter()
                .filter(|receipt| receipt.mode == thndrs_agent::ContextReductionMode::BaselineFallback)
                .count();
            let _ = writeln!(
                output,
                "- Reduction receipts: {} total ({} shadow, {} applied, {} baseline fallback)",
                self.receipts.len(),
                shadow,
                applied,
                fallback
            );
        } else {
            let _ = writeln!(output, "\n## Request\n\nNo completed provider request is selected.");
        }
        let _ = writeln!(
            output,
            "\n## Context items\n\n| ID | Kind | Visibility | Lifecycle | Reason | Protection | Relations | Recovery | Replacement |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- |"
        );
        for item in &self.items {
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                markdown_cell(&item.id),
                item.kind.label(),
                item.state.label(),
                item.lifecycle.label(),
                markdown_cell(&item.reason_code),
                markdown_cell(&protection_label(item)),
                markdown_cell(&relations_label(&item.relations)),
                if item.recovery_available { "yes" } else { "no" },
                markdown_cell(item.replacement.as_deref().unwrap_or("none"))
            );
        }
        let _ = writeln!(output, "\n## Model projection\n");
        for message in &self.model_projection {
            let _ = writeln!(output, "### {}\n", message.role);
            for line in message.content.lines() {
                let _ = writeln!(output, "    {line}");
            }
        }
        if !self.artifacts.is_empty() {
            let _ = writeln!(output, "\n## Artifacts\n");
            for artifact in &self.artifacts {
                let state = artifact
                    .metadata
                    .as_ref()
                    .map(|metadata| format!("{:?}", metadata.retention).to_ascii_lowercase())
                    .unwrap_or_else(|| "unavailable".to_string());
                let _ = writeln!(output, "- `{}`: {}", artifact.handle, state);
                if let Some(diagnostic) = &artifact.diagnostic {
                    let _ = writeln!(output, "  - diagnostic: {diagnostic}");
                }
                if let Some(body) = &artifact.body {
                    for line in body.lines() {
                        let _ = writeln!(output, "    {line}");
                    }
                }
            }
        }
        if !self.diagnostics.is_empty() {
            let _ = writeln!(output, "\n## Diagnostics\n");
            for diagnostic in &self.diagnostics {
                let _ = writeln!(output, "- {diagnostic}");
            }
        }
        output
    }
}

/// Convert an artifact recovery result to export metadata.
pub fn artifact_from_recovery(recovery: ArtifactRecovery, include_body: bool) -> ExportArtifact {
    let body = include_body.then_some(recovery.content).flatten();
    ExportArtifact {
        handle: recovery.metadata.handle.clone(),
        metadata: Some(recovery.metadata),
        body,
        diagnostic: recovery.diagnostic.map(|diagnostic| diagnostic.message),
    }
}

/// Return the inspection details used by both the table and export.
pub fn export_item(item: &ContextItem) -> ExportContextItem {
    let relations = item.lifecycle.relations.iter().map(bound_relation).collect::<Vec<_>>();
    let replacement = relations.iter().find_map(|relation| {
        matches!(
            relation.kind,
            ContextRelationKind::DuplicateOf | ContextRelationKind::SupersededBy | ContextRelationKind::SummarizedBy
        )
        .then(|| cap_text(&relation.target_id))
    });
    let verification = relations
        .iter()
        .find(|relation| relation.is_verification())
        .map(|relation| {
            format!(
                "{} -> {} ({})",
                cap_text(&relation.id),
                cap_text(&relation.target_id),
                relation.status.label()
            )
        });
    let protected = item.lifecycle.is_protected();
    ExportContextItem {
        id: cap_text(&item.id),
        kind: item.kind.clone(),
        state: item.visibility.clone(),
        lifecycle: item.lifecycle.state,
        reason_code: cap_text(&item.reason_code),
        reason: cap_text(&item.reason),
        replacement,
        protected,
        protection: item.lifecycle.protection.clone(),
        protection_released: item.lifecycle.protection_released,
        verification,
        relations,
        recovery_available: item.artifact_handle.is_some() || !item.visibility.is_rendered(),
        recovery_handle: item.artifact_handle.as_ref().map(|handle| cap_text(handle)),
        byte_count: item.byte_count,
        token_estimate: item.token_estimate,
        label: cap_text(&item.label),
    }
}

fn bound_relation(relation: &ContextRelation) -> ContextRelation {
    ContextRelation {
        id: cap_text(&relation.id),
        kind: relation.kind,
        source_id: cap_text(&relation.source_id),
        target_id: cap_text(&relation.target_id),
        status: relation.status,
    }
}

fn protection_label(item: &ExportContextItem) -> String {
    if !item.protected {
        return if item.protection_released { "released" } else { "none" }.to_string();
    }
    item.protection.labels().join(",")
}

fn relations_label(relations: &[ContextRelation]) -> String {
    if relations.is_empty() {
        return "none".to_string();
    }
    relations
        .iter()
        .map(|relation| {
            format!(
                "{}:{}->{}",
                relation.kind.label(),
                cap_text(&relation.target_id),
                relation.status.label()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn redact_projection(message: &ModelProjectionMessage) -> ExportProjectionMessage {
    ExportProjectionMessage { role: cap_text(&message.role), content: cap_text(&redact_secrets(&message.content)) }
}

fn cap_projection(messages: Vec<ExportProjectionMessage>) -> Vec<ExportProjectionMessage> {
    let mut remaining = EXPORT_PROJECTION_MAX_BYTES;
    messages
        .into_iter()
        .filter_map(|mut message| {
            if remaining == 0 {
                return None;
            }
            message.content = truncate_utf8(&cap_text(&message.content), remaining);
            remaining = remaining.saturating_sub(message.content.len());
            Some(message)
        })
        .collect()
}

fn bound_artifact(mut artifact: ExportArtifact) -> ExportArtifact {
    artifact.handle = cap_text(&artifact.handle);
    artifact.diagnostic = artifact.diagnostic.map(|diagnostic| cap_text(&diagnostic));
    artifact.body = artifact.body.map(|body| cap_text(&redact_secrets(&body)));
    artifact
}

fn cap_text(value: &str) -> String {
    let redacted = redact_secrets(value);
    truncate_utf8(&redacted, EXPORT_FIELD_MAX_BYTES)
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

fn display_measurement(value: &Option<u64>, provenance: &MeasurementProvenance) -> String {
    format!("{} ({})", display_optional(*value), provenance_label(provenance))
}

fn provenance_label(provenance: &MeasurementProvenance) -> &'static str {
    match provenance {
        MeasurementProvenance::ExactSerialized { .. } => "exact",
        MeasurementProvenance::Estimated { .. } => "estimated",
        MeasurementProvenance::ProviderReported { .. } => "provider-reported",
        MeasurementProvenance::Derived { .. } => "derived",
        MeasurementProvenance::Unknown => "unknown",
    }
}

fn display_optional(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use thndrs_agent::accounting::{
        ContextReductionMode, ContextReductionReceipt, ModelProjectionMessage, ProviderUsageComponents,
        ProviderUsageRule,
    };
    use thndrs_agent::context::{
        ContextBudget, ContextLifecycle, ContextLifecycleAction, ContextLifecycleState, ContextProtection,
        ContextProtectionReason, ContextRelation, ContextRelationStatus, DiagnosticSeverity, ModelContextLimits,
        ModelLimitConfidence, ModelLimitSource,
    };

    fn ledger() -> ContextLedger {
        let limits = ModelContextLimits {
            provider: "fixture".to_string(),
            model: "fixture-model".to_string(),
            context_window: 8_192,
            max_completion_tokens: 1_024,
            recommended_completion_tokens: 512,
            source: ModelLimitSource::Static,
            confidence: ModelLimitConfidence::ProviderReported,
        };
        let lifecycle = ContextLifecycle::new(ContextProtection::from_reason(ContextProtectionReason::FailureEvidence))
            .apply(ContextLifecycleAction::ProposeVerification {
                relation: ContextRelation::proposed_verification("rel-export", "ctx_tool_1", "ctx_candidate"),
            })
            .expect("verification relation");
        let item = ContextItem {
            id: "ctx_tool_1".to_string(),
            kind: ContextItemKind::ToolArchive,
            label: "tool output api_key=source-secret-that-must-not-be-rendered".to_string(),
            source_path: Some(PathBuf::from("/workspace/out.txt")),
            scope: ".".to_string(),
            content_hash: Some(42),
            artifact_handle: Some("artifact_v1_safe".to_string()),
            byte_count: 1_024,
            content: None,
            token_estimate: 358,
            visibility: ContextVisibility::Archived,
            reason_code: "budget_eviction".to_string(),
            reason: "archived after the request".to_string(),
            lifecycle,
        };
        ContextLedger {
            budget: ContextBudget::from_limits(limits, std::slice::from_ref(&item)),
            items: vec![item],
            diagnostics: vec![thndrs_agent::context::ContextDiagnostic {
                severity: DiagnosticSeverity::Info,
                code: "fixture".to_string(),
                message: "safe diagnostic".to_string(),
            }],
        }
    }

    fn accounting() -> ProviderRequestAccounting {
        let accounting = ProviderRequestAccounting::from_serialized_request(
            "turn_1",
            "turn_1:request:1",
            1,
            "fixture",
            "fixture-model",
            b"serialized request",
            Vec::new(),
        )
        .with_reduction_receipts(vec![ContextReductionReceipt {
            item_id: "tool:call_2".to_string(),
            method: "state_identical_evidence".to_string(),
            version: thndrs_agent::context::STATE_IDENTICAL_REDUCER_VERSION.to_string(),
            before_bytes: 42,
            after_bytes: 0,
            lossy: true,
            mode: ContextReductionMode::Applied,
            diagnostic: None,
        }])
        .with_model_projection(vec![ModelProjectionMessage {
            role: "user".to_string(),
            content: "visible api_key=source-secret-that-must-not-be-rendered".to_string(),
        }]);
        let mut accounting = accounting;
        accounting.provider_usage = Some(
            ProviderUsageComponents {
                input_tokens: Some(100),
                output_tokens: Some(12),
                cache_read_input_tokens: Some(4),
                cache_creation_input_tokens: Some(2),
                reasoning_tokens: None,
            }
            .normalize("fixture", ProviderUsageRule::AnthropicMessages),
        );
        accounting
    }

    #[test]
    fn json_and_markdown_share_bounded_redacted_facts() {
        let export = ContextExport::from_parts("session-1", &ledger(), Some(accounting()), Vec::new(), Vec::new());
        let json = export.to_json().expect("json");
        let markdown = export.to_markdown();

        assert!(!json.contains("source-secret-that-must-not-be-rendered"));
        assert!(!markdown.contains("source-secret-that-must-not-be-rendered"));
        assert!(json.contains(CONTEXT_EXPORT_SCHEMA_VERSION));
        assert!(json.contains("budget_eviction"));
        assert!(json.contains("failure_evidence"));
        assert!(json.contains("verified_by"));
        assert!(json.contains("state_identical_evidence"));
        assert!(markdown.contains("Inclusive input: 106"));
        assert!(markdown.contains("1 total (0 shadow, 1 applied, 0 baseline fallback)"));
        assert!(markdown.contains("Recovery"));
        assert!(markdown.contains("Lifecycle"));
        assert!(markdown.contains("verified_by"));
        let round_trip: ContextExport = serde_json::from_str(&json).expect("round trip");
        assert!(round_trip.items[0].recovery_available);
        assert!(round_trip.items[0].protected);
        assert_eq!(round_trip.items[0].lifecycle, ContextLifecycleState::Active);
        assert_eq!(round_trip.items[0].relations[0].status, ContextRelationStatus::Proposed);
        assert_eq!(
            round_trip.accounting.as_ref().expect("accounting").model_projection,
            Vec::new()
        );
    }

    #[test]
    fn export_rendering_is_deterministic_and_artifact_bodies_are_opt_in() {
        let body = ArtifactRecovery {
            metadata: serde_json::from_str(
                r#"{"schema_version":1,"identity":"tool","kind":"tool_evidence","handle":"artifact_v1_safe","content_hash":"hash","original_byte_count":10,"bounded_byte_count":10,"truncated":false,"redacted":true,"created_at":"now","created_at_unix":1,"expires_at":null,"expires_at_unix":null,"retention":"active"}"#,
            )
            .expect("metadata"),
            content: Some("bounded api_key=source-secret-that-must-not-be-rendered".to_string()),
            diagnostic: None,
        };
        let artifact_without_body = artifact_from_recovery(body.clone(), false);
        let artifact_with_body = artifact_from_recovery(body, true);
        let first = ContextExport::from_parts(
            "session-1",
            &ledger(),
            Some(accounting()),
            vec![artifact_without_body],
            Vec::new(),
        );
        let second = ContextExport::from_parts(
            "session-1",
            &ledger(),
            Some(accounting()),
            vec![artifact_with_body],
            Vec::new(),
        );
        assert_eq!(first.to_json().expect("json"), first.to_json().expect("json"));
        assert!(first.artifacts[0].body.is_none());
        assert_eq!(second.artifacts[0].body.as_deref(), Some("bounded api_key=[REDACTED]"));
        assert!(!second.to_markdown().contains("source-secret-that-must-not-be-rendered"));
    }
}
