//! OpenTelemetry metrics derived from persisted session records.

use std::io;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram, Meter, MeterProvider};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::{SdkMeterProvider, Stream};
use thndrs_agent::MeasurementProvenance;

use super::{ContextSnapshotState, SessionRecord};

/// Maximum number of attribute sets retained for each exported metric.
pub const CONTEXT_TELEMETRY_CARDINALITY_LIMIT: usize = 128;
const MAX_ATTRIBUTE_BYTES: usize = 128;

struct ContextMetrics {
    operation_duration: Histogram<f64>,
    time_to_first_token: Histogram<f64>,
    request_size: Histogram<u64>,
    token_usage: Histogram<u64>,
    tool_calls: Histogram<u64>,
    tool_duration: Histogram<f64>,
    transformation_size: Histogram<u64>,
    transformation_tokens: Histogram<u64>,
    working_set_delta: Histogram<f64>,
    errors: Counter<u64>,
}

impl ContextMetrics {
    fn new(meter: &Meter) -> Self {
        Self {
            operation_duration: meter
                .f64_histogram("gen_ai.client.operation.duration")
                .with_unit("s")
                .build(),
            time_to_first_token: meter
                .f64_histogram("thndrs.context.time_to_first_token")
                .with_unit("s")
                .build(),
            request_size: meter
                .u64_histogram("thndrs.context.request.size")
                .with_unit("By")
                .build(),
            token_usage: meter
                .u64_histogram("gen_ai.client.token.usage")
                .with_unit("{token}")
                .build(),
            tool_calls: meter
                .u64_histogram("thndrs.context.tool.calls")
                .with_unit("{call}")
                .build(),
            tool_duration: meter
                .f64_histogram("thndrs.context.tool.duration")
                .with_unit("s")
                .build(),
            transformation_size: meter
                .u64_histogram("thndrs.context.transformation.size")
                .with_unit("By")
                .build(),
            transformation_tokens: meter
                .u64_histogram("thndrs.context.transformation.tokens")
                .with_unit("{token}")
                .build(),
            working_set_delta: meter
                .f64_histogram("thndrs.context.working_set.delta")
                .with_unit("{token}")
                .build(),
            errors: meter.u64_counter("thndrs.context.errors").with_unit("{error}").build(),
        }
    }

    fn record(&self, records: &[SessionRecord]) {
        for record in records {
            match record {
                SessionRecord::ContextSnapshot { snapshot, .. }
                    if snapshot.state != ContextSnapshotState::Dispatched =>
                {
                    let attributes = request_attributes(&snapshot.provider, &snapshot.model, snapshot.state);
                    if let Some(duration_ms) = snapshot.duration_ms {
                        self.operation_duration.record(seconds(duration_ms), &attributes);
                    }
                    if let Some(duration_ms) = snapshot.time_to_first_token_ms {
                        self.time_to_first_token.record(seconds(duration_ms), &attributes);
                    }
                    if let Some(tool_count) = snapshot.tool_count {
                        self.tool_calls.record(tool_count, &attributes);
                    }
                    if let Some(duration_ms) = snapshot.tool_duration_ms {
                        self.tool_duration.record(seconds(duration_ms), &attributes);
                    }
                    if let Some(usage) = &snapshot.provider_usage {
                        record_provider_usage(&self.token_usage, usage, &attributes);
                    }
                    for receipt in &snapshot.transformations {
                        record_transformation(
                            &self.transformation_size,
                            receipt.before_bytes,
                            receipt.after_bytes,
                            &receipt.method,
                        );
                    }
                }
                SessionRecord::RequestAccounting { accounting, .. } => {
                    let attributes =
                        request_attributes(&accounting.provider, &accounting.model, ContextSnapshotState::Completed);
                    self.request_size.record(
                        accounting.serialized_bytes.value,
                        &with_provenance(attributes.clone(), &accounting.serialized_bytes.provenance),
                    );
                    if let Some(value) = accounting.estimated_input_tokens.value {
                        let mut attributes = with_provenance(attributes, &accounting.estimated_input_tokens.provenance);
                        attributes.push(KeyValue::new("gen_ai.token.type", "input"));
                        self.token_usage.record(value, &attributes);
                    }
                }
                SessionRecord::Compaction { audit, .. } => {
                    if let Some(receipt) = audit.local_receipt {
                        record_transformation(
                            &self.transformation_size,
                            receipt.before_bytes as u64,
                            receipt.after_bytes as u64,
                            "compaction",
                        );
                        record_transformation(
                            &self.transformation_tokens,
                            receipt.before_token_estimate,
                            receipt.after_token_estimate,
                            "compaction",
                        );
                    }
                }
                SessionRecord::Failed { .. } => self.errors.add(1, &[]),
                _ => {}
            }
        }

        let terminal = records.iter().filter_map(|record| match record {
            SessionRecord::ContextSnapshot { seq, snapshot, .. }
                if snapshot.state != ContextSnapshotState::Dispatched =>
            {
                Some((*seq, snapshot.ledger.used))
            }
            _ => None,
        });
        let mut previous = None;
        for (_, used) in terminal {
            if let Some(before) = previous {
                let delta = used as i128 - before as i128;
                let direction = if delta < 0 { "decrease" } else { "increase_or_equal" };
                self.working_set_delta
                    .record(delta as f64, &[KeyValue::new("thndrs.context.change", direction)]);
            }
            previous = Some(used);
        }
    }
}

/// Export content-free metrics from persisted records with the OpenTelemetry stdout exporter.
pub fn export_context_telemetry(session_id: &str, records: &[SessionRecord]) -> io::Result<()> {
    let resource = Resource::builder_empty()
        .with_service_name("thndrs")
        .with_attribute(KeyValue::new("thndrs.session.id", session_id.to_string()))
        .build();
    let provider = SdkMeterProvider::builder()
        .with_resource(resource)
        .with_periodic_exporter(opentelemetry_stdout::MetricExporter::default())
        .with_view(|_| {
            Stream::builder()
                .with_cardinality_limit(CONTEXT_TELEMETRY_CARDINALITY_LIMIT)
                .build()
                .ok()
        })
        .build();
    let meter = provider.meter("thndrs.context");
    emit_context_telemetry(&meter, records);
    provider.shutdown().map_err(io::Error::other)
}

/// Record content-free context metrics through an application-provided meter.
pub fn emit_context_telemetry(meter: &Meter, records: &[SessionRecord]) {
    ContextMetrics::new(meter).record(records);
}

fn record_provider_usage(instrument: &Histogram<u64>, usage: &thndrs_agent::ProviderUsage, attributes: &[KeyValue]) {
    if let Some(value) = usage.components.input_tokens {
        instrument.record(value, &token_attributes(attributes, "input", "provider_reported"));
    }
    if let Some(value) = usage.components.output_tokens {
        instrument.record(value, &token_attributes(attributes, "output", "provider_reported"));
    }
}

fn record_transformation(instrument: &Histogram<u64>, before: u64, after: u64, method: &str) {
    let method = cap(method);
    instrument.record(
        before,
        &[
            KeyValue::new("thndrs.context.transformation", method.clone()),
            KeyValue::new("thndrs.context.transformation.phase", "before"),
        ],
    );
    instrument.record(
        after,
        &[
            KeyValue::new("thndrs.context.transformation", method),
            KeyValue::new("thndrs.context.transformation.phase", "after"),
        ],
    );
}

fn request_attributes(provider: &str, model: &str, state: ContextSnapshotState) -> Vec<KeyValue> {
    vec![
        KeyValue::new("gen_ai.provider.name", cap(provider)),
        KeyValue::new("gen_ai.request.model", cap(model)),
        KeyValue::new("thndrs.request.state", state_label(state)),
    ]
}

fn token_attributes(attributes: &[KeyValue], token_type: &'static str, provenance: &'static str) -> Vec<KeyValue> {
    let mut output = attributes.to_vec();
    output.push(KeyValue::new("gen_ai.token.type", token_type));
    output.push(KeyValue::new("thndrs.measurement.provenance", provenance));
    output
}

fn with_provenance(mut attributes: Vec<KeyValue>, provenance: &MeasurementProvenance) -> Vec<KeyValue> {
    match provenance {
        MeasurementProvenance::ExactSerialized { boundary } => {
            attributes.push(KeyValue::new("thndrs.measurement.provenance", "exact_serialized"));
            attributes.push(KeyValue::new("thndrs.measurement.boundary", cap(boundary)));
        }
        MeasurementProvenance::Estimated { estimator, version } => {
            attributes.push(KeyValue::new("thndrs.measurement.provenance", "estimated"));
            attributes.push(KeyValue::new("thndrs.measurement.method", cap(estimator)));
            attributes.push(KeyValue::new("thndrs.measurement.version", cap(version)));
        }
        MeasurementProvenance::ProviderReported { component, .. } => {
            attributes.push(KeyValue::new("thndrs.measurement.provenance", "provider_reported"));
            attributes.push(KeyValue::new("thndrs.measurement.component", cap(component)));
        }
        MeasurementProvenance::Derived { rule, version } => {
            attributes.push(KeyValue::new("thndrs.measurement.provenance", "derived"));
            attributes.push(KeyValue::new("thndrs.measurement.method", cap(rule)));
            attributes.push(KeyValue::new("thndrs.measurement.version", cap(version)));
        }
        MeasurementProvenance::Unknown => {
            attributes.push(KeyValue::new("thndrs.measurement.provenance", "unknown"));
        }
    }
    attributes
}

const fn state_label(state: ContextSnapshotState) -> &'static str {
    match state {
        ContextSnapshotState::Dispatched => "dispatched",
        ContextSnapshotState::Completed => "completed",
        ContextSnapshotState::Failed => "failed",
        ContextSnapshotState::Interrupted => "interrupted",
    }
}

fn seconds(milliseconds: u64) -> f64 {
    milliseconds as f64 / 1_000.0
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
    use crate::session::{CapturedRequestContent, ContextLedgerMeta, ContextSnapshot, SCHEMA_VERSION};
    use opentelemetry_sdk::metrics::InMemoryMetricExporter;
    use thndrs_agent::context::{ModelLimitConfidence, ModelLimitSource};

    #[test]
    fn emits_sdk_metrics_without_captured_content() {
        let exporter = InMemoryMetricExporter::default();
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter.clone())
            .build();
        let meter = provider.meter("test");
        let records = vec![
            SessionRecord::RequestContentCaptured {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: String::new(),
                capture: CapturedRequestContent {
                    request_id: "r1".into(),
                    turn_id: "t".into(),
                    attempt: 1,
                    messages: vec![thndrs_agent::ModelProjectionMessage {
                        role: "user".into(),
                        content: "normalized secret".into(),
                    }],
                },
            },
            snapshot_record(2, "r1", 100),
            snapshot_record(3, "r2", 80),
        ];

        emit_context_telemetry(&meter, &records);
        provider.force_flush().unwrap();

        let exported = exporter.get_finished_metrics().unwrap();
        let names = exported
            .iter()
            .flat_map(|resource| resource.scope_metrics())
            .flat_map(|scope| scope.metrics())
            .map(|metric| metric.name())
            .collect::<Vec<_>>();
        assert!(names.contains(&"gen_ai.client.operation.duration"));
        assert!(names.contains(&"thndrs.context.working_set.delta"));
        assert!(!format!("{exported:?}").contains("normalized secret"));
    }

    #[test]
    fn caps_utf8_attributes_on_character_boundaries() {
        let value = "é".repeat(MAX_ATTRIBUTE_BYTES);
        let capped = cap(&value);
        assert!(capped.len() <= MAX_ATTRIBUTE_BYTES);
        assert!(value.starts_with(&capped));
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
