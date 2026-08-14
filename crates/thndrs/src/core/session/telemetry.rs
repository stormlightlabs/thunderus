//! Provider-neutral OpenTelemetry observations derived from persisted session records.

use std::collections::BTreeMap;
use std::io;

use serde::Serialize;
use thndrs_agent::MeasurementProvenance;

use super::{CapturedRequestContent, ContextSnapshotState, SessionRecord};

/// Version of the stable persisted-record telemetry projection.
pub const CONTEXT_TELEMETRY_SCHEMA_VERSION: &str = "context-otel-v1";
/// Maximum number of observations emitted by one export.
pub const CONTEXT_TELEMETRY_MAX_OBSERVATIONS: usize = 8_192;
const MAX_ATTRIBUTE_BYTES: usize = 128;

/// A bounded telemetry document suitable for an OpenTelemetry adapter or collector input.
#[derive(Clone, Debug, Serialize)]
pub struct ContextTelemetryExport {
    pub schema_version: &'static str,
    pub session_id: String,
    pub content_included: bool,
    pub observations: Vec<TelemetryObservation>,
}

/// One provider-neutral metric, event, or request span observation.
#[derive(Clone, Debug, Serialize)]
pub struct TelemetryObservation {
    pub signal: String,
    pub name: String,
    pub unit: String,
    pub value: f64,
    pub attributes: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<MeasurementProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<CapturedRequestContent>,
}

impl ContextTelemetryExport {
    /// Derive bounded observations only from records already persisted for the session.
    pub fn from_records(session_id: &str, records: &[SessionRecord]) -> io::Result<Self> {
        let content_permitted = records
            .iter()
            .rev()
            .find_map(|record| match record {
                SessionRecord::ContextCapturePolicy { policy, .. } => Some(policy.permits_content()),
                _ => None,
            })
            .unwrap_or(false);
        let captures = if content_permitted {
            records
                .iter()
                .filter_map(|record| match record {
                    SessionRecord::RequestContentCaptured { capture, .. } => {
                        Some(((capture.request_id.clone(), capture.attempt), capture.clone()))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        };
        let mut observations = Vec::new();
        for record in records {
            match record {
                SessionRecord::ContextSnapshot { snapshot, .. }
                    if snapshot.state != ContextSnapshotState::Dispatched =>
                {
                    let mut attributes = request_attributes(
                        &snapshot.provider,
                        &snapshot.model,
                        &snapshot.request_id,
                        snapshot.attempt,
                    );
                    attributes.insert(
                        "thndrs.request.state".to_string(),
                        format!("{:?}", snapshot.state).to_lowercase(),
                    );
                    push(
                        &mut observations,
                        "span",
                        "gen_ai.client.operation.duration",
                        "ms",
                        snapshot.duration_ms.unwrap_or(0) as f64,
                        attributes.clone(),
                        captures.get(&(snapshot.request_id.clone(), snapshot.attempt)).cloned(),
                    )?;
                    optional_metric(
                        &mut observations,
                        "thndrs.context.time_to_first_token",
                        "ms",
                        snapshot.time_to_first_token_ms,
                        &attributes,
                    )?;
                    optional_metric(
                        &mut observations,
                        "thndrs.context.request.size",
                        "By",
                        snapshot.serialized_bytes,
                        &attributes,
                    )?;
                    optional_metric(
                        &mut observations,
                        "gen_ai.client.token.usage",
                        "token",
                        snapshot.estimated_input_tokens,
                        &attributes,
                    )?;
                    optional_metric(
                        &mut observations,
                        "thndrs.context.tool.calls",
                        "{call}",
                        snapshot.tool_count,
                        &attributes,
                    )?;
                    optional_metric(
                        &mut observations,
                        "thndrs.context.tool.duration",
                        "ms",
                        snapshot.tool_duration_ms,
                        &attributes,
                    )?;
                    if let Some(usage) = &snapshot.provider_usage {
                        if let Some(value) = usage.components.input_tokens {
                            push_measurement(
                                &mut observations,
                                "gen_ai.client.token.usage",
                                "token",
                                value,
                                with_attr(&attributes, "gen_ai.token.type", "input"),
                                MeasurementProvenance::ProviderReported {
                                    provider: snapshot.provider.clone(),
                                    component: "input_tokens".to_string(),
                                },
                            )?;
                        }
                        if let Some(value) = usage.components.output_tokens {
                            push_measurement(
                                &mut observations,
                                "gen_ai.client.token.usage",
                                "token",
                                value,
                                with_attr(&attributes, "gen_ai.token.type", "output"),
                                MeasurementProvenance::ProviderReported {
                                    provider: snapshot.provider.clone(),
                                    component: "output_tokens".to_string(),
                                },
                            )?;
                        }
                    }
                    for receipt in &snapshot.transformations {
                        let attrs = with_attr(&attributes, "thndrs.context.transform", &receipt.method);
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.before",
                            "By",
                            receipt.before_bytes as f64,
                            attrs.clone(),
                            None,
                        )?;
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.after",
                            "By",
                            receipt.after_bytes as f64,
                            attrs,
                            None,
                        )?;
                    }
                }
                SessionRecord::RequestAccounting { accounting, .. } => {
                    let attributes = request_attributes(
                        &accounting.provider,
                        &accounting.model,
                        &accounting.request_id,
                        accounting.attempt,
                    );
                    push_measurement(
                        &mut observations,
                        "thndrs.context.request.size",
                        "By",
                        accounting.serialized_bytes.value,
                        attributes.clone(),
                        accounting.serialized_bytes.provenance.clone(),
                    )?;
                    if let Some(value) = accounting.estimated_input_tokens.value {
                        push_measurement(
                            &mut observations,
                            "gen_ai.client.token.usage",
                            "token",
                            value,
                            attributes,
                            accounting.estimated_input_tokens.provenance.clone(),
                        )?;
                    }
                }
                SessionRecord::Compaction { audit, .. } => {
                    let mut attributes = BTreeMap::new();
                    attributes.insert("thndrs.context.transform".to_string(), "compaction".to_string());
                    if let Some(receipt) = audit.local_receipt {
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.before",
                            "By",
                            receipt.before_bytes as f64,
                            attributes.clone(),
                            None,
                        )?;
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.after",
                            "By",
                            receipt.after_bytes as f64,
                            attributes.clone(),
                            None,
                        )?;
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.before",
                            "token",
                            receipt.before_token_estimate as f64,
                            attributes.clone(),
                            None,
                        )?;
                        push(
                            &mut observations,
                            "event",
                            "thndrs.context.transform.after",
                            "token",
                            receipt.after_token_estimate as f64,
                            attributes,
                            None,
                        )?;
                    }
                }
                SessionRecord::Failed { .. } => {
                    push(
                        &mut observations,
                        "metric",
                        "thndrs.context.errors",
                        "{error}",
                        1.0,
                        BTreeMap::new(),
                        None,
                    )?;
                }
                _ => {}
            }
        }

        let mut terminal = records
            .iter()
            .filter_map(|record| match record {
                SessionRecord::ContextSnapshot { seq, snapshot, .. }
                    if snapshot.state != ContextSnapshotState::Dispatched =>
                {
                    Some((*seq, snapshot.as_ref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        terminal.sort_by_key(|(seq, _)| *seq);
        for pair in terminal.windows(2) {
            let before = pair[0].1;
            let after = pair[1].1;
            let delta = after.ledger.used as i128 - before.ledger.used as i128;
            let mut attributes = BTreeMap::new();
            attributes.insert(
                "thndrs.context.change".to_string(),
                if delta < 0 { "decrease" } else { "increase_or_equal" }.to_string(),
            );
            push(
                &mut observations,
                "metric",
                "thndrs.context.working_set.delta",
                "token",
                delta as f64,
                attributes,
                None,
            )?;
        }
        Ok(Self {
            schema_version: CONTEXT_TELEMETRY_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            content_included: content_permitted,
            observations,
        })
    }

    /// Encode the bounded exporter document as JSON.
    pub fn to_json(&self) -> io::Result<String> {
        serde_json::to_string_pretty(self).map_err(io::Error::other)
    }
}

fn optional_metric(
    output: &mut Vec<TelemetryObservation>, name: &str, unit: &str, value: Option<u64>,
    attributes: &BTreeMap<String, String>,
) -> io::Result<()> {
    if let Some(value) = value {
        push(output, "metric", name, unit, value as f64, attributes.clone(), None)?;
    }
    Ok(())
}

fn push(
    output: &mut Vec<TelemetryObservation>, signal: &str, name: &str, unit: &str, value: f64,
    attributes: BTreeMap<String, String>, content: Option<CapturedRequestContent>,
) -> io::Result<()> {
    if output.len() >= CONTEXT_TELEMETRY_MAX_OBSERVATIONS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "context telemetry observation limit exceeded",
        ));
    }
    output.push(TelemetryObservation {
        signal: signal.to_string(),
        name: name.to_string(),
        unit: unit.to_string(),
        value,
        attributes,
        provenance: None,
        content,
    });
    Ok(())
}

fn push_measurement(
    output: &mut Vec<TelemetryObservation>, name: &str, unit: &str, value: u64, attributes: BTreeMap<String, String>,
    provenance: MeasurementProvenance,
) -> io::Result<()> {
    if output.len() >= CONTEXT_TELEMETRY_MAX_OBSERVATIONS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "context telemetry observation limit exceeded",
        ));
    }
    output.push(TelemetryObservation {
        signal: "metric".to_string(),
        name: name.to_string(),
        unit: unit.to_string(),
        value: value as f64,
        attributes,
        provenance: Some(provenance),
        content: None,
    });
    Ok(())
}

fn request_attributes(provider: &str, model: &str, _request_id: &str, _attempt: u32) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("gen_ai.provider.name".to_string(), cap(provider)),
        ("gen_ai.request.model".to_string(), cap(model)),
    ])
}

fn with_attr(attributes: &BTreeMap<String, String>, key: &str, value: &str) -> BTreeMap<String, String> {
    let mut output = attributes.clone();
    output.insert(key.to_string(), cap(value));
    output
}

fn cap(value: &str) -> String {
    let mut end = value.len().min(MAX_ATTRIBUTE_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ContextCapturePolicy, ContextLedgerMeta, ContextSnapshot, SCHEMA_VERSION};
    use thndrs_agent::context::{ModelLimitConfidence, ModelLimitSource};

    #[test]
    fn metadata_policy_omits_captured_content() {
        let records = vec![
            SessionRecord::ContextCapturePolicy {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: String::new(),
                policy: ContextCapturePolicy::metadata_only(),
            },
            SessionRecord::RequestContentCaptured {
                schema_version: SCHEMA_VERSION,
                seq: 2,
                time: String::new(),
                capture: CapturedRequestContent {
                    request_id: "r".into(),
                    turn_id: "t".into(),
                    attempt: 1,
                    messages: vec![],
                },
            },
        ];
        let export = ContextTelemetryExport::from_records("s", &records).unwrap();
        assert!(!export.content_included);
        assert!(!export.to_json().unwrap().contains("messages"));
    }

    #[test]
    fn retained_policy_includes_only_normalized_captured_content() {
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
                    request_id: "r".into(),
                    turn_id: "t".into(),
                    attempt: 1,
                    messages: vec![thndrs_agent::ModelProjectionMessage {
                        role: "user".into(),
                        content: "normalized".into(),
                    }],
                },
            },
            snapshot_record(3, "r", 100),
            snapshot_record(4, "r2", 80),
        ];

        let export = ContextTelemetryExport::from_records("s", &records).unwrap();
        assert!(export.content_included);
        let json = export.to_json().unwrap();
        assert!(json.contains("normalized"));
        assert!(json.contains("thndrs.context.working_set.delta"));
    }

    fn snapshot_record(seq: u64, request_id: &str, used: u64) -> SessionRecord {
        SessionRecord::ContextSnapshot {
            schema_version: SCHEMA_VERSION,
            seq,
            time: String::new(),
            snapshot: Box::new(ContextSnapshot {
                snapshot_version: 1,
                session_id: "s".into(),
                request_id: request_id.into(),
                turn_id: "t".into(),
                attempt: 1,
                provider: "provider".into(),
                model: "model".into(),
                route: "provider/model".into(),
                state: ContextSnapshotState::Completed,
                ledger: ContextLedgerMeta {
                    items: Vec::new(),
                    available_input: 1_000,
                    target: 800,
                    auto_compaction_threshold: 900,
                    used,
                    limit_source: ModelLimitSource::Fallback,
                    limit_confidence: ModelLimitConfidence::Conservative,
                    projection: None,
                    diagnostics: Vec::new(),
                },
                serialized_bytes: Some(400),
                estimated_input_tokens: Some(100),
                transformations: Vec::new(),
                provider_usage: None,
                duration_ms: Some(10),
                time_to_first_token_ms: None,
                tool_count: None,
                tool_duration_ms: None,
                transcript_entries: Vec::new(),
            }),
        }
    }
}
