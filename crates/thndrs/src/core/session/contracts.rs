//! Stable, content-free metadata contracts for append-only coding-agent sessions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::context::ContextSource;
use thndrs_agent::context::{
    ContextItem, ContextItemKind, ContextLedger, ContextLifecycle, ContextLifecycleAction, ContextProjection,
    ContextVisibility, ModelLimitConfidence, ModelLimitSource,
};
use thndrs_agent::{ContextReductionReceipt, ProviderUsage};

/// Metadata for a loaded context source, without its model-visible content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextSourceMeta {
    /// Absolute path to the source file.
    pub path: String,
    /// Scope label — `"."` for root, or a relative subtree path.
    pub scope: String,
    /// Stable hash of the full original content before truncation.
    pub content_hash: u64,
    /// Whether the source was truncated before projection.
    pub truncated: bool,
    /// Original byte count of the source file.
    pub byte_count: usize,
}

impl From<&ContextSource> for ContextSourceMeta {
    fn from(source: &ContextSource) -> Self {
        Self {
            path: source.path.display().to_string(),
            scope: source.scope.clone(),
            content_hash: source.content_hash,
            truncated: source.truncated,
            byte_count: source.byte_count,
        }
    }
}

impl ContextSourceMeta {
    /// Extract audit metadata from a context source while omitting its content.
    pub fn from_source(source: &ContextSource) -> Self {
        Self::from(source)
    }
}

/// Content-free audit metadata for one context-ledger item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextItemMeta {
    /// Stable context item id.
    pub id: String,
    /// Domain kind used by context selection.
    pub kind: ContextItemKind,
    /// File-backed source path, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Context scope, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Hash of the source content, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<u64>,
    /// Stable handle for bounded redacted recovery, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_handle: Option<String>,
    /// Source byte size.
    pub byte_count: usize,
    /// Conservative token estimate used by selection.
    pub token_estimate: usize,
    /// Inclusion state for this snapshot or action.
    pub visibility: ContextVisibility,
    /// Stable policy code for this state.
    #[serde(default)]
    pub reason_code: String,
    /// Redacted explanation of the assigned visibility.
    pub reason: String,
    /// Lifecycle and protection state independent of request visibility.
    #[serde(default)]
    pub lifecycle: ContextLifecycle,
}

impl From<&ContextItem> for ContextItemMeta {
    fn from(item: &ContextItem) -> Self {
        Self {
            id: item.id.clone(),
            kind: item.kind.clone(),
            source_path: item.source_path.as_ref().map(|path| path.display().to_string()),
            scope: Some(item.scope.clone()),
            content_hash: item.content_hash,
            artifact_handle: item.artifact_handle.clone(),
            byte_count: item.byte_count,
            token_estimate: item.token_estimate,
            visibility: item.visibility.clone(),
            reason_code: item.reason_code.clone(),
            reason: redact_audit_text(&item.reason),
            lifecycle: item.lifecycle.clone(),
        }
    }
}

/// Content-free context ledger snapshot persisted for a prompt turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextLedgerMeta {
    /// All candidate and selected item metadata for this turn.
    pub items: Vec<ContextItemMeta>,
    /// Available input tokens after completion reservation and provider overhead.
    pub available_input: u64,
    /// Target token budget used for normal selection.
    pub target: u64,
    /// Token threshold that may trigger automatic compaction.
    pub auto_compaction_threshold: u64,
    /// Estimated tokens of rendered items.
    pub used: u64,
    /// Provenance of the resolved model limit.
    #[serde(default = "default_limit_source")]
    pub limit_source: ModelLimitSource,
    /// Confidence in the resolved model limit.
    #[serde(default = "default_limit_confidence")]
    pub limit_confidence: ModelLimitConfidence,
    /// Stable aggregate projection consumed by inspection surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<ContextProjection>,
    /// Content-free diagnostic summaries emitted by context selection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<ContextDiagnosticMeta>,
}

impl From<&ContextLedger> for ContextLedgerMeta {
    fn from(ledger: &ContextLedger) -> Self {
        Self {
            items: ledger.items.iter().map(ContextItemMeta::from).collect(),
            available_input: ledger.budget.available_input,
            target: ledger.budget.target,
            auto_compaction_threshold: ledger.budget.auto_compaction_threshold,
            used: ledger.budget.used,
            limit_source: ledger.budget.limits.source,
            limit_confidence: ledger.budget.limits.confidence,
            projection: Some(ledger.projection()),
            diagnostics: ledger
                .diagnostics
                .iter()
                .map(|diagnostic| ContextDiagnosticMeta {
                    severity: diagnostic.severity.label().to_string(),
                    code: diagnostic.code.clone(),
                    message: redact_audit_text(&diagnostic.message),
                })
                .collect(),
        }
    }
}

fn default_limit_source() -> ModelLimitSource {
    ModelLimitSource::Fallback
}

fn default_limit_confidence() -> ModelLimitConfidence {
    ModelLimitConfidence::Conservative
}

/// Lifecycle state of a request-bound context snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSnapshotState {
    /// Request was serialized and handed to the provider adapter.
    Dispatched,
    /// Provider completed the request and returned usage when available.
    Completed,
    /// Request attempt ended with an error.
    Failed,
    /// Request attempt was cancelled before completion.
    Interrupted,
}

/// Versioned, content-free context snapshot for one provider request attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// Snapshot schema version.
    pub snapshot_version: u32,
    /// Session that owns the request.
    pub session_id: String,
    /// Stable request identity shared by lifecycle updates.
    pub request_id: String,
    /// Turn that initiated the request.
    pub turn_id: String,
    /// One-based provider attempt number.
    pub attempt: u32,
    /// Provider adapter label.
    pub provider: String,
    /// Model selected for this attempt.
    pub model: String,
    /// Human-readable provider/model route.
    pub route: String,
    /// Lifecycle state represented by this append-only record.
    pub state: ContextSnapshotState,
    /// Content-free context candidates, aggregates, limits, and diagnostics.
    pub ledger: ContextLedgerMeta,
    /// Exact request bytes, unavailable before serialization completes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serialized_bytes: Option<u64>,
    /// Estimated input tokens; missing measurements remain unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_input_tokens: Option<u64>,
    /// Reduction receipts applied or evaluated for this request.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transformations: Vec<ContextReductionReceipt>,
    /// Provider-reported usage and normalization provenance after completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_usage: Option<ProviderUsage>,
}

/// Content-free audit record for an explicit lifecycle transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextLifecycleAudit {
    /// Pure action applied to the item's lifecycle state.
    pub action: ContextLifecycleAction,
    /// Post-transition item metadata, including lifecycle and protection.
    pub item: ContextItemMeta,
    /// Redacted user or application reason for the transition.
    pub reason: String,
}

/// Content-free diagnostic emitted while selecting context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextDiagnosticMeta {
    /// Diagnostic severity label.
    pub severity: String,
    /// Stable diagnostic code.
    pub code: String,
    /// Redacted human-readable message.
    pub message: String,
}

/// Effective configuration provenance persisted with a session.
///
/// The structure contains paths, hashes, origins, and diagnostics, never raw
/// configuration secrets or prompt content.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigMeta {
    /// Effective session directory used for append-only JSONL files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<String>,
    /// Loaded config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SessionConfigFile>,
    /// Per-key origin labels, such as `"env:THNDRS_MODEL"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub origins: BTreeMap<String, String>,
    /// Non-fatal config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Loaded MCP config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_files: Vec<SessionConfigFile>,
    /// Non-fatal MCP config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_diagnostics: Vec<String>,
}

/// A loaded config file recorded in session metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigFile {
    /// Display path: workspace-relative, `~`-relative, or absolute.
    pub path: String,
    /// Source label: `"global"` or `"project"`.
    pub source: String,
    /// Lowercase hex SHA-256 of file bytes.
    pub sha256: String,
}

/// MCP-specific identity attached to an external tool call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpToolSessionMeta {
    /// Configured MCP server name.
    pub server_name: String,
    /// Original MCP tool name before provider-visible namespacing.
    pub original_tool_name: String,
    /// MCP capability class used by the call.
    #[serde(default = "default_mcp_tool_capability")]
    pub capability: String,
    /// Authority requested at the application boundary.
    #[serde(default = "default_mcp_requested_authority")]
    pub requested_authority: String,
}

fn default_mcp_tool_capability() -> String {
    "tool".to_string()
}

fn default_mcp_requested_authority() -> String {
    "external MCP server access with thndrs process permissions".to_string()
}

/// A permission option recorded without protocol payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpPermissionOptionRecord {
    /// ACP option id.
    pub id: String,
    /// Human-readable option label.
    pub name: String,
    /// Lowercase option kind.
    pub kind: String,
}

/// External-agent session identity persisted once per run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpSessionMetadata {
    /// Configured ACP agent name.
    pub agent_name: String,
    /// Opaque external ACP session id returned by the agent.
    pub acp_session_id: String,
    /// Redacted command display used to start the agent.
    pub command: String,
    /// Selected ACP protocol version.
    pub protocol_version: String,
    /// Optional ACP agent info name.
    pub agent_info_name: Option<String>,
    /// Optional ACP agent info version.
    pub agent_info_version: Option<String>,
    /// Optional ACP client info name.
    pub client_info_name: Option<String>,
    /// Optional ACP client info version.
    pub client_info_version: Option<String>,
}

fn redact_audit_text(value: &str) -> String {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let mut redacted = Vec::with_capacity(tokens.len());
    let mut index = 0;

    while let Some(token) = tokens.get(index) {
        if token.eq_ignore_ascii_case("bearer")
            && tokens.get(index + 1).is_some_and(|next| {
                next.trim_matches(|character: char| character.is_ascii_punctuation())
                    .len()
                    >= 10
            })
        {
            redacted.push("Bearer [REDACTED]".to_string());
            index += 2;
        } else {
            redacted.push(redact_audit_token(token));
            index += 1;
        }
    }

    redacted.join(" ")
}

fn redact_audit_token(token: &str) -> String {
    let trimmed = token.trim_matches(|character: char| matches!(character, ',' | ';' | '(' | ')' | '[' | ']'));
    let lower = trimmed.to_ascii_lowercase();

    if trimmed.starts_with("sk-") && trimmed.len() >= 11 {
        return token.replacen(trimmed, "sk-[REDACTED]", 1);
    }
    if lower.starts_with("bearer") && trimmed.len() >= 17 {
        return "Bearer [REDACTED]".to_string();
    }
    if let Some((key, value)) = trimmed.split_once(['=', ':'])
        && matches!(
            key.to_ascii_lowercase().as_str(),
            "password" | "passwd" | "api_key" | "apikey" | "access_token" | "secret"
        )
        && value.len() >= 4
    {
        return token.replacen(trimmed, &format!("{key}=[REDACTED]"), 1);
    }

    token.to_string()
}
