//! Versioned, bounded exports of persisted context semantics.

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use serde::Serialize;
use serde_json::Value;
use thndrs_agent::ProviderRequestAccounting;

use super::{
    CapturedRequestContent, CompactionAudit, ContextCapturePolicy, ContextSnapshot, ContextSnapshotState,
    SCHEMA_VERSION, SessionLineageEntry, SessionRecord,
};

/// Schema version shared by persisted context inspection surfaces.
pub const CONTEXT_EXPORT_SCHEMA_VERSION: &str = "context-history-v1";
/// Policy version for metadata-only, redacted context exports.
pub const CONTEXT_EXPORT_POLICY_VERSION: &str = "metadata-only-redacted-v1";
/// Maximum records accepted by one context export.
pub const CONTEXT_EXPORT_MAX_RECORDS: usize = 4_096;
/// Maximum encoded JSON document size.
pub const CONTEXT_EXPORT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Deterministic metadata-only context history built from session records.
#[derive(Clone, Debug, Serialize)]
pub struct PersistedContextExport {
    pub schema_version: &'static str,
    pub policy_version: &'static str,
    pub session_schema_version: u32,
    pub session_id: String,
    pub lineage: Vec<SessionLineageEntry>,
    pub capture_policy: ContextCapturePolicy,
    pub redaction: ContextExportRedaction,
    pub limits: ContextExportLimits,
    pub snapshots: Vec<ContextSnapshotRecord>,
    pub diffs: Vec<ContextSnapshotDiff>,
    pub request_accounting: Vec<RequestAccountingRecord>,
    pub request_content: Vec<CapturedRequestContent>,
    pub artifact_content: Vec<CapturedArtifactContent>,
    pub transformations: Vec<ContextTransformationRecord>,
    pub diagnostics: Vec<ContextDiagnosticRecord>,
    pub measurement_provenance: Vec<MeasurementProvenanceRecord>,
}

/// Retention and redaction state carried by every export.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ContextExportRedaction {
    pub applied: bool,
    pub request_content_retained: bool,
    pub artifact_bodies_retained: bool,
}

/// Bounds applied while producing the export.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ContextExportLimits {
    pub max_records: usize,
    pub max_output_bytes: usize,
    pub artifact_bodies: usize,
}

/// One append-only context snapshot with its ordering metadata.
#[derive(Clone, Debug, Serialize)]
pub struct ContextSnapshotRecord {
    pub seq: u64,
    pub time: String,
    pub snapshot: ContextSnapshot,
}

/// Stable, content-free difference between adjacent terminal attempts.
#[derive(Clone, Debug, Serialize)]
pub struct ContextSnapshotDiff {
    pub from_request_id: String,
    pub from_attempt: u32,
    pub to_request_id: String,
    pub to_attempt: u32,
    pub used_tokens_delta: i128,
    pub added_item_ids: Vec<String>,
    pub removed_item_ids: Vec<String>,
    pub changed_item_ids: Vec<String>,
}

/// Persisted request accounting with append-only ordering metadata.
#[derive(Clone, Debug, Serialize)]
pub struct RequestAccountingRecord {
    pub seq: u64,
    pub time: String,
    pub accounting: ProviderRequestAccounting,
}

/// Sanitized, bounded tool evidence exposed only under an opted-in policy.
#[derive(Clone, Debug, Serialize)]
pub struct CapturedArtifactContent {
    pub turn_id: String,
    pub call_id: String,
    pub output: Vec<String>,
}

/// A persisted reduction, compaction, or lifecycle transformation.
#[derive(Clone, Debug, Serialize)]
pub struct ContextTransformationRecord {
    pub seq: u64,
    pub time: String,
    pub kind: String,
    pub value: Value,
}

/// One selection diagnostic tied to a stable request attempt.
#[derive(Clone, Debug, Serialize)]
pub struct ContextDiagnosticRecord {
    pub request_id: String,
    pub attempt: u32,
    pub severity: String,
    pub code: String,
    pub message: String,
}

/// Measurement provenance retained without provider request content.
#[derive(Clone, Debug, Serialize)]
pub struct MeasurementProvenanceRecord {
    pub request_id: String,
    pub attempt: u32,
    pub measurement: String,
    pub provenance: Value,
}

impl PersistedContextExport {
    /// Build one deterministic export from validated semantic records.
    pub fn from_records(session_id: &str, records: &[SessionRecord]) -> io::Result<Self> {
        if records.len() > CONTEXT_EXPORT_MAX_RECORDS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("context export exceeds the {CONTEXT_EXPORT_MAX_RECORDS} record limit"),
            ));
        }

        let mut ordered = records.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|record| record.seq());
        let mut lineage = Vec::new();
        let capture_policy = ordered
            .iter()
            .rev()
            .find_map(|record| match record {
                SessionRecord::ContextCapturePolicy { policy, .. } => Some(policy.clone()),
                _ => None,
            })
            .filter(ContextCapturePolicy::permits_content)
            .unwrap_or_default();
        let content_permitted = capture_policy.permits_content();
        let mut snapshots = Vec::new();
        let mut request_accounting = Vec::new();
        let mut request_content = Vec::new();
        let mut artifact_content = Vec::new();
        let mut transformations = Vec::new();

        for record in ordered {
            match record {
                SessionRecord::SessionFork { lineage: value, .. } => lineage = value.clone(),
                SessionRecord::RequestContentCaptured { capture, .. } if content_permitted => {
                    request_content.push(capture.clone());
                }
                SessionRecord::ToolFinished { turn_id, call_id, output, .. } if content_permitted => {
                    artifact_content.push(CapturedArtifactContent {
                        turn_id: turn_id.clone(),
                        call_id: call_id.clone(),
                        output: output.clone(),
                    });
                }
                SessionRecord::ContextSnapshot { seq, time, snapshot, .. } => snapshots.push(ContextSnapshotRecord {
                    seq: *seq,
                    time: time.clone(),
                    snapshot: (**snapshot).clone(),
                }),
                SessionRecord::RequestAccounting { seq, time, accounting, .. } => {
                    request_accounting.push(RequestAccountingRecord {
                        seq: *seq,
                        time: time.clone(),
                        accounting: (**accounting).clone(),
                    });
                }
                SessionRecord::Compaction { seq, time, audit, .. } => {
                    transformations.push(compaction_transformation(*seq, time, audit)?);
                }
                SessionRecord::ContextPin { seq, time, item, reason, .. } => {
                    transformations.push(transformation(*seq, time, "pin", &(item, reason))?)
                }
                SessionRecord::ContextDrop { seq, time, item, reason, .. } => {
                    transformations.push(transformation(*seq, time, "drop", &(item, reason))?)
                }
                SessionRecord::ContextRecovery { seq, time, item, reason, .. } => {
                    transformations.push(transformation(*seq, time, "recovery", &(item, reason))?)
                }
                SessionRecord::ContextLifecycle { seq, time, audit, .. } => {
                    transformations.push(transformation(*seq, time, "lifecycle", audit)?)
                }
                _ => {}
            }
        }

        let mut terminal_by_attempt = BTreeMap::new();
        for snapshot in snapshots
            .iter()
            .filter(|record| record.snapshot.state != ContextSnapshotState::Dispatched)
        {
            terminal_by_attempt.insert(
                (snapshot.snapshot.request_id.clone(), snapshot.snapshot.attempt),
                snapshot,
            );
        }
        let mut terminal = terminal_by_attempt.into_values().collect::<Vec<_>>();
        terminal.sort_by_key(|record| record.seq);

        for snapshot in &terminal {
            for receipt in &snapshot.snapshot.transformations {
                transformations.push(transformation(snapshot.seq, &snapshot.time, "reduction", receipt)?);
            }
        }
        transformations.sort_by(|left, right| left.seq.cmp(&right.seq).then_with(|| left.kind.cmp(&right.kind)));

        let diffs = terminal
            .windows(2)
            .map(|pair| snapshot_diff(pair[0], pair[1]))
            .collect();
        let diagnostics = terminal
            .iter()
            .flat_map(|record| {
                record
                    .snapshot
                    .ledger
                    .diagnostics
                    .iter()
                    .map(|diagnostic| ContextDiagnosticRecord {
                        request_id: record.snapshot.request_id.clone(),
                        attempt: record.snapshot.attempt,
                        severity: diagnostic.severity.clone(),
                        code: diagnostic.code.clone(),
                        message: diagnostic.message.clone(),
                    })
            })
            .collect();
        let measurement_provenance = provenance_records(&request_accounting)?;

        let export = Self {
            schema_version: CONTEXT_EXPORT_SCHEMA_VERSION,
            policy_version: CONTEXT_EXPORT_POLICY_VERSION,
            session_schema_version: SCHEMA_VERSION,
            session_id: session_id.to_string(),
            lineage,
            capture_policy,
            redaction: ContextExportRedaction {
                applied: true,
                request_content_retained: content_permitted,
                artifact_bodies_retained: content_permitted,
            },
            limits: ContextExportLimits {
                max_records: CONTEXT_EXPORT_MAX_RECORDS,
                max_output_bytes: CONTEXT_EXPORT_MAX_BYTES,
                artifact_bodies: artifact_content.len(),
            },
            snapshots,
            diffs,
            request_accounting,
            request_content,
            artifact_content,
            transformations,
            diagnostics,
            measurement_provenance,
        };
        export.to_json_with_max_bytes(CONTEXT_EXPORT_MAX_BYTES)?;
        Ok(export)
    }

    /// Encode the export while enforcing the configured output bound.
    pub fn to_json(&self) -> io::Result<String> {
        self.to_json_with_max_bytes(CONTEXT_EXPORT_MAX_BYTES)
    }

    fn to_json_with_max_bytes(&self, max_bytes: usize) -> io::Result<String> {
        let encoded = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        if encoded.len() > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("context export exceeds the {max_bytes} byte limit"),
            ));
        }
        Ok(encoded)
    }
}

fn compaction_transformation(seq: u64, time: &str, audit: &CompactionAudit) -> io::Result<ContextTransformationRecord> {
    transformation(
        seq,
        time,
        "compaction",
        &serde_json::json!({
            "summary_id": audit.summary_id,
            "summary_bytes": audit.summary.len(),
            "summary_content_exported": false,
            "covered_start_seq": audit.covered_start_seq,
            "covered_end_seq": audit.covered_end_seq,
            "source_hashes": audit.source_hashes,
            "source_summary_ids": audit.source_summary_ids,
            "trigger": audit.trigger,
            "risk": audit.risk,
            "review": audit.review,
            "recovery_handles": audit.recovery_handles,
            "model": audit.model,
            "usage": audit.usage,
            "local_receipt": audit.local_receipt,
            "native_context_edit": audit.native_context_edit,
        }),
    )
}

fn transformation<T: Serialize>(
    seq: u64, time: &str, kind: &str, value: &T,
) -> io::Result<ContextTransformationRecord> {
    Ok(ContextTransformationRecord {
        seq,
        time: time.to_string(),
        kind: kind.to_string(),
        value: serde_json::to_value(value).map_err(io::Error::other)?,
    })
}

fn snapshot_diff(before: &ContextSnapshotRecord, after: &ContextSnapshotRecord) -> ContextSnapshotDiff {
    let before_items = before
        .snapshot
        .ledger
        .items
        .iter()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let after_items = after
        .snapshot
        .ledger
        .items
        .iter()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let before_ids = before_items.keys().cloned().collect::<BTreeSet<_>>();
    let after_ids = after_items.keys().cloned().collect::<BTreeSet<_>>();
    let changed_item_ids = before_ids
        .intersection(&after_ids)
        .filter(|id| before_items.get(*id) != after_items.get(*id))
        .cloned()
        .collect();
    ContextSnapshotDiff {
        from_request_id: before.snapshot.request_id.clone(),
        from_attempt: before.snapshot.attempt,
        to_request_id: after.snapshot.request_id.clone(),
        to_attempt: after.snapshot.attempt,
        used_tokens_delta: after.snapshot.ledger.used as i128 - before.snapshot.ledger.used as i128,
        added_item_ids: after_ids.difference(&before_ids).cloned().collect(),
        removed_item_ids: before_ids.difference(&after_ids).cloned().collect(),
        changed_item_ids,
    }
}

fn provenance_records(records: &[RequestAccountingRecord]) -> io::Result<Vec<MeasurementProvenanceRecord>> {
    let mut output = Vec::new();
    for record in records {
        let accounting = &record.accounting;
        output.push(provenance(
            accounting,
            "serialized_bytes",
            &accounting.serialized_bytes.provenance,
        )?);
        output.push(provenance(
            accounting,
            "estimated_input_tokens",
            &accounting.estimated_input_tokens.provenance,
        )?);
        if let Some(usage) = &accounting.provider_usage {
            output.push(provenance(
                accounting,
                "inclusive_input_tokens",
                &usage.inclusive_input_tokens.provenance,
            )?);
        }
    }
    Ok(output)
}

fn provenance<T: Serialize>(
    accounting: &ProviderRequestAccounting, measurement: &str, value: &T,
) -> io::Result<MeasurementProvenanceRecord> {
    Ok(MeasurementProvenanceRecord {
        request_id: accounting.request_id.clone(),
        attempt: accounting.attempt,
        measurement: measurement.to_string(),
        provenance: serde_json::to_value(value).map_err(io::Error::other)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ContextLedgerMeta;
    use thndrs_agent::context::{ModelLimitConfidence, ModelLimitSource};

    #[test]
    fn persisted_export_is_metadata_only_ordered_and_diffable() {
        let audit = serde_json::from_value(serde_json::json!({
            "summary": "secret summary body",
            "covered_start_seq": 1,
            "covered_end_seq": 2,
            "trigger": "manual",
            "risk": "low",
            "model": "model"
        }))
        .expect("compaction audit");
        let records = vec![
            snapshot_record(4, "request-2", 1_250),
            snapshot_record(2, "request-1", 900),
            snapshot_record(3, "request-1", 1_000),
            SessionRecord::Compaction {
                schema_version: SCHEMA_VERSION,
                seq: 5,
                time: "2026-08-13T00:00:05Z".to_string(),
                audit,
            },
        ];

        let export = PersistedContextExport::from_records("session-1", &records).expect("build export");
        let json = export.to_json().expect("encode export");

        assert_eq!(export.snapshots[0].snapshot.request_id, "request-1");
        assert_eq!(export.diffs.len(), 1);
        assert_eq!(export.diffs[0].from_request_id, "request-1");
        assert_eq!(export.diffs[0].to_request_id, "request-2");
        assert_eq!(export.diffs[0].used_tokens_delta, 250);
        assert!(!export.redaction.request_content_retained);
        assert!(json.contains("\"context-history-v1\""));
        assert!(json.contains("\"summary_content_exported\": false"));
        assert!(!json.contains("secret summary body"));
        assert!(!json.contains("model_projection"));
    }

    #[test]
    fn persisted_export_rejects_oversized_json() {
        let export = PersistedContextExport::from_records("session-1", &[snapshot_record(1, "request", 1_000)])
            .expect("build export");

        let error = export.to_json_with_max_bytes(1).expect_err("output bound");

        assert!(error.to_string().contains("byte limit"));
    }

    #[test]
    fn persisted_export_rejects_too_many_records() {
        let record = snapshot_record(1, "request", 1_000);
        let records = vec![record; CONTEXT_EXPORT_MAX_RECORDS + 1];

        let error = PersistedContextExport::from_records("session-1", &records).expect_err("record bound");

        assert!(error.to_string().contains("record limit"));
    }

    #[test]
    fn persisted_export_includes_normalized_content_only_when_opted_in() {
        let records = vec![
            SessionRecord::ContextCapturePolicy {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: String::new(),
                policy: ContextCapturePolicy::retained_content(),
            },
            SessionRecord::RequestContentCaptured {
                schema_version: SCHEMA_VERSION,
                seq: 2,
                time: String::new(),
                capture: CapturedRequestContent {
                    request_id: "request".into(),
                    turn_id: "turn".into(),
                    attempt: 1,
                    messages: vec![thndrs_agent::ModelProjectionMessage {
                        role: "user".into(),
                        content: "normalized content".into(),
                    }],
                },
            },
        ];

        let export = PersistedContextExport::from_records("session-1", &records).expect("build export");

        assert!(export.redaction.request_content_retained);
        assert_eq!(export.request_content.len(), 1);
        assert!(export.to_json().unwrap().contains("normalized content"));
    }

    fn snapshot_record(seq: u64, request_id: &str, used: u64) -> SessionRecord {
        SessionRecord::ContextSnapshot {
            schema_version: SCHEMA_VERSION,
            seq,
            time: format!("2026-08-13T00:00:0{seq}Z"),
            snapshot: Box::new(ContextSnapshot {
                snapshot_version: 1,
                session_id: "session-1".to_string(),
                request_id: request_id.to_string(),
                turn_id: format!("turn-{seq}"),
                attempt: 1,
                provider: "provider".to_string(),
                model: "model".to_string(),
                route: "provider/model".to_string(),
                state: ContextSnapshotState::Completed,
                ledger: ContextLedgerMeta {
                    items: Vec::new(),
                    available_input: 10_000,
                    target: 8_000,
                    auto_compaction_threshold: 9_000,
                    used,
                    limit_source: ModelLimitSource::Fallback,
                    limit_confidence: ModelLimitConfidence::Conservative,
                    projection: None,
                    diagnostics: Vec::new(),
                },
                serialized_bytes: Some(used * 4),
                estimated_input_tokens: Some(used),
                transformations: Vec::new(),
                provider_usage: None,
                duration_ms: None,
                time_to_first_token_ms: None,
                tool_count: None,
                tool_duration_ms: None,
                transcript_entries: Vec::new(),
            }),
        }
    }
}
