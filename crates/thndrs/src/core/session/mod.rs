//! Append-only JSONL session persistence.
//!
//! Records session metadata, transcript events, tool audits, and context
//! control actions for audit and resume without storing full raw provider
//! payloads.
//!
//! Each [`SessionRecord`] is one append-only JSONL line tagged with
//! `schema_version`, a monotonic `seq`, `time`, and `type`.

mod capture;
mod collection;
mod context_changes;
mod context_export;
mod contracts;
mod export;
mod inventory;
mod lifecycle;
mod reader;
mod records;
mod retention;
mod storage;
mod telemetry;
#[cfg(test)]
mod tests;
mod writer;

use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::acp::permissions::PendingPermission;
use crate::app::{Entry, ToolStatus, TranscriptBlocks};
use crate::artifacts::{self, ArtifactMetadata};
use crate::context::ContextSource;
use crate::prompt::{EnvironmentMetadata, HistoryReuse, PromptBundle};
use crate::skills::{SkillActivation, SkillReferenceMeta};
use crate::tools::{WriteOp, shell};
use crate::{datetime, internals, tools};

use thndrs_agent::ProviderRequestAccounting;
use thndrs_agent::context::{ContextItem, ContextLedger, RangeSummary};

pub use capture::{
    CAPTURE_RETENTION_DAYS, CONTEXT_CAPTURE_POLICY_VERSION, CapturedRequestContent, ContextCaptureMode,
    ContextCapturePolicy, MAX_CAPTURED_REQUEST_BYTES,
};
pub use collection::{
    CollectionReport, collect_if_due, collect_now, reclaimable_bytes, reclaimable_bytes_from_inventory,
};
pub use context_changes::{ContextChangeError, ContextHistory};
pub use context_export::{
    CONTEXT_EXPORT_MAX_BYTES, CONTEXT_EXPORT_MAX_RECORDS, CONTEXT_EXPORT_POLICY_VERSION, CONTEXT_EXPORT_SCHEMA_VERSION,
    ContextSnapshotDiff, PersistedContextExport,
};
pub use contracts::{
    AcpPermissionOptionRecord, AcpSessionMetadata, ContextDiagnosticMeta, ContextItemMeta, ContextLedgerMeta,
    ContextLifecycleAudit, ContextSnapshot, ContextSnapshotState, ContextSourceMeta, McpToolSessionMeta,
    SessionConfigFile, SessionConfigMeta,
};
pub use export::{SessionExport, export_session};
pub use inventory::{
    ArtifactInventoryEntry, SessionInventory, SessionInventoryDiagnostic, SessionInventoryEntry, SessionLineageState,
    SessionStorageState, SessionStorageTotals,
};
pub use lifecycle::{
    DeleteArtifactPreview, DeleteSessionOptions, PermanentDeleteOptions, SessionDeletePreview, SessionLifecycle,
    SessionLifecycleAction, SessionLifecycleError, SessionLifecycleReport, SessionOwnedState,
};
pub use reader::{
    SessionLookupError, SessionReader, SessionSummary, fork_session, generate_session_id, latest_session_file,
    list_session_files, list_session_summaries, list_session_titles, read_redacted_log_tail, resolve_session_file,
    sessions_dir,
};
pub use records::*;
pub use retention::apply_prune_cancellable_with_progress;
pub use retention::{
    PruneCandidate, PruneFailure, PruneOverrides, PruneReason, PruneReport, SessionRetentionPolicy, apply_prune,
    apply_prune_cancellable, select_prune_candidates,
};
pub use telemetry::{CONTEXT_TELEMETRY_CARDINALITY_LIMIT, emit_context_telemetry, export_context_telemetry};
pub use writer::SessionWriter;

/// Current JSONL schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Maximum amount of redacted log text returned by a reader.
pub const MAX_LOG_OUTPUT_BYTES: usize = 32 * 1024;

/// Maximum length of a user-assigned session name.
pub const MAX_SESSION_NAME_CHARS: usize = 80;

fn redact_range_summary(summary: &RangeSummary) -> RangeSummary {
    let mut redacted = summary.clone();
    redacted.objective = tools::shell::redact_secrets(&redacted.objective);
    for values in [
        &mut redacted.findings,
        &mut redacted.decisions,
        &mut redacted.paths,
        &mut redacted.failures,
        &mut redacted.verification,
        &mut redacted.blockers,
    ] {
        for value in values {
            *value = tools::shell::redact_secrets(value);
        }
    }
    for fact in &mut redacted.protected_facts {
        fact.text = tools::shell::redact_secrets(&fact.text);
    }
    redacted
}

fn split_tool_name_id(name: &str) -> (String, String) {
    match name.rsplit_once('#') {
        Some((n, id)) => (n.to_string(), id.to_string()),
        None => (name.to_string(), "?".to_string()),
    }
}

fn mcp_tool_session_meta(name: &str) -> Option<McpToolSessionMeta> {
    let rest = name.strip_prefix("mcp__")?;
    let (server_name, original_tool_name) = rest.split_once("__")?;
    if server_name.is_empty() || original_tool_name.is_empty() {
        return None;
    }
    let capability = if original_tool_name == "resource_read" { "resource" } else { "tool" };
    Some(McpToolSessionMeta {
        server_name: server_name.to_string(),
        original_tool_name: original_tool_name.to_string(),
        capability: capability.to_string(),
        requested_authority: "external MCP server access with thndrs process permissions".to_string(),
    })
}

fn collect_handles(value: &serde_json::Value, key: Option<&str>, handles: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(fields) => {
            if key == Some("artifact")
                && let Some(handle) = fields.get("handle").and_then(serde_json::Value::as_str)
            {
                handles.insert(handle.to_string());
            }
            for (field, value) in fields {
                collect_handles(value, Some(field), handles);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_handles(value, key, handles);
            }
        }
        serde_json::Value::String(handle) if key == Some("artifact_handle") => {
            handles.insert(handle.clone());
        }
        _ => {}
    }
}
