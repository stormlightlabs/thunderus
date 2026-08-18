//! Content-free context history, request diffs, and transcript events.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use thndrs_agent::{ContextReductionMode, MeasurementProvenance};

use super::{CompactionAudit, ContextItemMeta, ContextSnapshot, ContextSnapshotState, SessionRecord};

const MAX_RENDERED_CHANGES: usize = 64;
const MAX_RENDERED_LINE_BYTES: usize = 512;
const MAX_DETAIL_ITEMS: usize = 8;
const MAX_RENDERED_OUTPUT_BYTES: usize = 36 * 1024;

/// A content-free history of request snapshots and context actions.
#[derive(Clone, Debug, Default)]
pub struct ContextHistory {
    records: Vec<ContextHistoryRecord>,
}

#[derive(Clone, Debug)]
enum ContextHistoryRecord {
    Snapshot {
        seq: u64,
        snapshot: Box<ContextSnapshot>,
    },
    Compaction {
        seq: u64,
        audit: Box<CompactionAudit>,
    },
    Recovery {
        seq: u64,
        item: Box<ContextItemMeta>,
        reason: String,
    },
}

/// Invalid `/context changes` selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextChangeError(String);

impl fmt::Display for ContextChangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ContextChangeError {}

impl ContextHistory {
    /// Rebuild history from append-only session records.
    pub fn from_records(records: &[SessionRecord]) -> Self {
        let mut history = Self::default();
        for record in records {
            history.record_session_record(record);
        }
        history
    }

    /// Capture one persisted session record when it affects context history.
    pub fn record_session_record(&mut self, record: &SessionRecord) {
        match record {
            SessionRecord::ContextSnapshot { seq, snapshot, .. } => self
                .records
                .push(ContextHistoryRecord::Snapshot { seq: *seq, snapshot: snapshot.clone() }),
            SessionRecord::Compaction { seq, audit, .. } => {
                self.records
                    .push(ContextHistoryRecord::Compaction { seq: *seq, audit: Box::new(audit.clone()) });
            }
            SessionRecord::ContextRecovery { seq, item, reason, .. } => {
                self.records.push(ContextHistoryRecord::Recovery {
                    seq: *seq,
                    item: Box::new(item.clone()),
                    reason: reason.clone(),
                });
            }
            _ => {}
        }
    }

    /// Capture a live request snapshot before or without durable persistence.
    pub fn record_snapshot(&mut self, snapshot: ContextSnapshot) {
        let seq = self.records.last().map_or(0, history_seq).saturating_add(1);
        self.records
            .push(ContextHistoryRecord::Snapshot { seq, snapshot: Box::new(snapshot) });
    }

    /// Capture a live compaction before or without durable persistence.
    pub fn record_compaction(&mut self, audit: CompactionAudit) {
        let seq = self.records.last().map_or(0, history_seq).saturating_add(1);
        self.records
            .push(ContextHistoryRecord::Compaction { seq, audit: Box::new(audit) });
    }

    /// Capture a live recovery before or without durable persistence.
    pub fn record_recovery(&mut self, item: ContextItemMeta, reason: &str) {
        let seq = self.records.last().map_or(0, history_seq).saturating_add(1);
        self.records
            .push(ContextHistoryRecord::Recovery { seq, item: Box::new(item), reason: reason.to_string() });
    }

    /// Render the latest two request snapshots, or two explicit request ids.
    pub fn render_changes(&self, request_ids: &[&str]) -> Result<String, ContextChangeError> {
        if !matches!(request_ids.len(), 0 | 2) {
            return Err(ContextChangeError(
                "usage: /context changes [<from-request-id> <to-request-id>]".to_string(),
            ));
        }
        let snapshots = self.terminal_request_snapshots();
        let (before_index, before, after_index, after) = if request_ids.is_empty() {
            if snapshots.len() < 2 {
                return Err(ContextChangeError(
                    "context changes requires at least two recorded request snapshots".to_string(),
                ));
            }
            let before = &snapshots[snapshots.len() - 2];
            let after = &snapshots[snapshots.len() - 1];
            (before.0, before.1, after.0, after.1)
        } else {
            let before = select_snapshot(&snapshots, request_ids[0])?;
            let after = select_snapshot(&snapshots, request_ids[1])?;
            (before.0, before.1, after.0, after.1)
        };
        if before_index >= after_index {
            return Err(ContextChangeError(
                "context changes requires requests in chronological order".to_string(),
            ));
        }
        Ok(self.render_between(before_index, before, after_index, after))
    }

    /// Render metadata and observations for one provider request attempt.
    pub fn render_request(&self, selector: Option<&str>) -> Result<String, ContextChangeError> {
        let snapshots = self.terminal_request_snapshots();
        let (index, snapshot) = match selector {
            Some(selector) => select_snapshot(&snapshots, selector)?,
            None => snapshots.last().copied().ok_or_else(|| {
                ContextChangeError("context request requires a recorded provider request".to_string())
            })?,
        };
        let usage = snapshot.provider_usage.as_ref();
        let components = usage.map(|usage| &usage.components);
        let projected_provenance = snapshot
            .ledger
            .projection
            .as_ref()
            .map_or("unknown", |projection| projection.estimate_provenance.as_str());
        let provider_input = usage.map_or_else(
            || "unknown".to_string(),
            |usage| {
                measurement_label(
                    usage.inclusive_input_tokens.value,
                    &usage.inclusive_input_tokens.provenance,
                )
            },
        );
        let transcript_entries = if snapshot.transcript_entries.is_empty() {
            "none".to_string()
        } else {
            bounded_list(&snapshot.transcript_entries)
        };
        let (reductions, compactions, recoveries, item_changes, changes_link) =
            self.request_context_links(&snapshots, index, snapshot);

        Ok(truncate_output(format!(
            "context request {}#{} · {}\nmodel  {}\nroute  {}\n\n── Timing\nduration  {}\nfirst token  {}\n\n── Tokens\ninput projected  {} ({projected_provenance})\ninput provider  {provider_input}\ninput fresh  {}\ncache read  {}\ncache write  {}\noutput  {}\nreasoning  {}\n\n── Activity\ntools  {} · {}\ncontext changes  {reductions} reductions · {compactions} compactions · {recoveries} recoveries · {item_changes} other item changes\n\n── Links\nturn  {}\nsnapshot  {}#{}\nprovider operation  {}#{}\ntranscript  {transcript_entries}\nchanges  {changes_link}",
            snapshot.request_id,
            snapshot.attempt,
            snapshot_state_label(snapshot.state),
            snapshot.model,
            snapshot.route,
            milliseconds_label(snapshot.duration_ms),
            milliseconds_label(snapshot.time_to_first_token_ms),
            value_label(snapshot.estimated_input_tokens),
            value_label(usage.and_then(thndrs_agent::ProviderUsage::fresh_input_tokens)),
            value_label(components.and_then(|usage| usage.cache_read_input_tokens)),
            value_label(components.and_then(|usage| usage.cache_creation_input_tokens)),
            value_label(components.and_then(|usage| usage.output_tokens)),
            value_label(components.and_then(|usage| usage.reasoning_tokens)),
            value_label(snapshot.tool_count),
            milliseconds_label(snapshot.tool_duration_ms),
            snapshot.turn_id,
            snapshot.request_id,
            snapshot.attempt,
            snapshot.request_id,
            snapshot.attempt,
        )))
    }

    /// Build a semantic transcript event for a context-affecting record.
    pub fn transcript_event(&self, record: &SessionRecord) -> Option<(String, String)> {
        match record {
            SessionRecord::ContextSnapshot { seq, snapshot, .. }
                if snapshot.state != ContextSnapshotState::Dispatched
                    && snapshot
                        .transformations
                        .iter()
                        .any(|receipt| receipt.mode == ContextReductionMode::Applied) =>
            {
                Some((format!("context:reduction:{seq}"), reduction_event(snapshot)))
            }
            SessionRecord::Compaction { seq, audit, .. } => Some((
                format!("context:compaction:{seq}"),
                compaction_event(audit, self.next_request_after(*seq)),
            )),
            SessionRecord::ContextRecovery { seq, item, reason, .. } => Some((
                format!("context:recovery:{seq}"),
                recovery_event(item, reason, self.next_request_after(*seq)),
            )),
            _ => None,
        }
    }

    /// Build the semantic event for a live completed request snapshot.
    pub fn live_reduction_event(snapshot: &ContextSnapshot) -> Option<String> {
        (snapshot.state != ContextSnapshotState::Dispatched
            && snapshot
                .transformations
                .iter()
                .any(|receipt| receipt.mode == ContextReductionMode::Applied))
        .then(|| reduction_event(snapshot))
    }

    /// Build the semantic event for a live compaction.
    pub fn live_compaction_event(audit: &CompactionAudit) -> String {
        compaction_event(audit, None)
    }

    /// Build the semantic event for a live recovery.
    pub fn live_recovery_event(item: &ContextItemMeta, reason: &str) -> String {
        recovery_event(item, reason, None)
    }

    fn terminal_request_snapshots(&self) -> Vec<(usize, &ContextSnapshot)> {
        let mut by_attempt = BTreeMap::<(String, u32), (usize, &ContextSnapshot)>::new();
        for (index, record) in self.records.iter().enumerate() {
            let ContextHistoryRecord::Snapshot { snapshot, .. } = record else { continue };
            if snapshot.state != ContextSnapshotState::Dispatched {
                by_attempt.insert((snapshot.request_id.clone(), snapshot.attempt), (index, snapshot));
            }
        }
        let mut snapshots = by_attempt.into_values().collect::<Vec<_>>();
        snapshots.sort_by_key(|(index, _)| *index);
        snapshots
    }

    fn next_request_after(&self, seq: u64) -> Option<&str> {
        self.records.iter().find_map(|record| match record {
            ContextHistoryRecord::Snapshot { seq: snapshot_seq, snapshot } if *snapshot_seq > seq => {
                Some(snapshot.request_id.as_str())
            }
            _ => None,
        })
    }

    fn request_context_links(
        &self, snapshots: &[(usize, &ContextSnapshot)], index: usize, snapshot: &ContextSnapshot,
    ) -> (usize, usize, usize, &'static str, String) {
        let previous = snapshots
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate < index)
            .copied();
        let start = previous.map_or(0, |(index, _)| index.saturating_add(1));
        let records = &self.records[start..=index];
        let compactions = records
            .iter()
            .filter(|record| matches!(record, ContextHistoryRecord::Compaction { .. }))
            .count();
        let recoveries = records
            .iter()
            .filter(|record| matches!(record, ContextHistoryRecord::Recovery { .. }))
            .count();
        let reductions = snapshot
            .transformations
            .iter()
            .filter(|receipt| receipt.mode == ContextReductionMode::Applied)
            .count();
        let (item_changes, changes_link) = previous.map_or_else(
            || ("unknown", "unavailable (first recorded request)".to_string()),
            |(_, previous)| {
                (
                    if previous.ledger.items == snapshot.ledger.items { "no" } else { "yes" },
                    format!(
                        "/context changes {}#{} {}#{}",
                        previous.request_id, previous.attempt, snapshot.request_id, snapshot.attempt
                    ),
                )
            },
        );
        (reductions, compactions, recoveries, item_changes, changes_link)
    }

    fn render_between(
        &self, before_index: usize, before: &ContextSnapshot, after_index: usize, after: &ContextSnapshot,
    ) -> String {
        let mut grouped = GroupedChanges::default();
        let before_items = before
            .ledger
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        let after_items = after
            .ledger
            .items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        let ids = before_items
            .keys()
            .chain(after_items.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for id in ids {
            match (before_items.get(id), after_items.get(id)) {
                (None, Some(item)) => grouped.push("additions", item_line(item)),
                (Some(item), None) => grouped.push("removals", item_line(item)),
                (Some(old), Some(new)) => {
                    if old.lifecycle != new.lifecycle {
                        grouped.push("lifecycle changes", format!("{id}: protection/relations changed"));
                    }
                    if old.scope != new.scope && old.kind.label() == "project_instruction" {
                        grouped.push(
                            "instruction-scope changes",
                            format!("{id}: {} -> {}", option_label(&old.scope), option_label(&new.scope)),
                        );
                    }
                    if old.kind != new.kind
                        || old.content_hash != new.content_hash
                        || old.source_path != new.source_path
                        || old.artifact_handle != new.artifact_handle
                        || old.byte_count != new.byte_count
                        || old.token_estimate != new.token_estimate
                        || old.reason_code != new.reason_code
                    {
                        grouped.push(
                            "replacements",
                            format!(
                                "{id}: hash {} -> {}, {} -> {} bytes, {} -> {} tokens, artifact {} -> {}, reason {} -> {}",
                                hash_label(old.content_hash),
                                hash_label(new.content_hash),
                                old.byte_count,
                                new.byte_count,
                                old.token_estimate,
                                new.token_estimate,
                                option_label(&old.artifact_handle),
                                option_label(&new.artifact_handle),
                                old.reason_code,
                                new.reason_code,
                            ),
                        );
                    }
                    if old.visibility != new.visibility {
                        grouped.push(
                            "omissions",
                            format!(
                                "{id}: {} -> {} ({})",
                                old.visibility.label(),
                                new.visibility.label(),
                                new.reason_code
                            ),
                        );
                    }
                }
                (None, None) => {}
            }
        }
        for receipt in after
            .transformations
            .iter()
            .filter(|receipt| receipt.mode == ContextReductionMode::Applied)
        {
            grouped.push(
                "reductions",
                format!(
                    "{}: {} {} -> {} bytes (reclaimed {}, lossy {})",
                    receipt.item_id,
                    receipt.method,
                    receipt.before_bytes,
                    receipt.after_bytes,
                    receipt.before_bytes.saturating_sub(receipt.after_bytes),
                    receipt.lossy
                ),
            );
        }
        for record in &self.records[before_index.saturating_add(1)..=after_index] {
            match record {
                ContextHistoryRecord::Compaction { audit, .. } => grouped.push("compactions", compaction_detail(audit)),
                ContextHistoryRecord::Recovery { item, reason, .. } => grouped.push(
                    "recoveries",
                    format!(
                        "{}: {} · artifact {} · {}",
                        item.id,
                        reason,
                        item.artifact_handle.as_deref().unwrap_or("unavailable"),
                        item.reason_code
                    ),
                ),
                ContextHistoryRecord::Snapshot { .. } => {}
            }
        }

        let mut output = format!(
            "context changes  {} -> {}\n{}\n{}",
            request_label(before),
            request_label(after),
            aggregate_line("before", before),
            aggregate_line("after", after)
        );
        let mut rendered = 0;
        let total = grouped.total;
        for (group, lines) in grouped.groups {
            if rendered >= MAX_RENDERED_CHANGES {
                break;
            }
            output.push_str(&format!("\n\n{group}:"));
            for line in lines.into_iter().take(MAX_RENDERED_CHANGES - rendered) {
                output.push_str(&format!("\n- {line}"));
                rendered += 1;
            }
        }
        if total > rendered {
            output.push_str(&format!("\n\n{} additional changes omitted", total - rendered));
        } else if total == 0 {
            output.push_str("\n\nno item or transformation changes");
        }
        truncate_output(output)
    }
}

fn history_seq(record: &ContextHistoryRecord) -> u64 {
    match record {
        ContextHistoryRecord::Snapshot { seq, .. }
        | ContextHistoryRecord::Compaction { seq, .. }
        | ContextHistoryRecord::Recovery { seq, .. } => *seq,
    }
}

#[derive(Default)]
struct GroupedChanges {
    groups: BTreeMap<&'static str, Vec<String>>,
    total: usize,
}

impl GroupedChanges {
    fn push(&mut self, group: &'static str, line: String) {
        self.total = self.total.saturating_add(1);
        if self.groups.values().map(Vec::len).sum::<usize>() < MAX_RENDERED_CHANGES {
            self.groups.entry(group).or_default().push(truncate_line(line));
        }
    }
}

fn select_snapshot<'a>(
    snapshots: &[(usize, &'a ContextSnapshot)], selector: &str,
) -> Result<(usize, &'a ContextSnapshot), ContextChangeError> {
    let (request_id, attempt) = selector
        .rsplit_once('#')
        .and_then(|(request_id, attempt)| attempt.parse::<u32>().ok().map(|attempt| (request_id, attempt)))
        .map_or((selector, None), |(request_id, attempt)| (request_id, Some(attempt)));
    let matches = snapshots
        .iter()
        .filter(|(_, snapshot)| {
            snapshot.request_id == request_id && attempt.is_none_or(|attempt| snapshot.attempt == attempt)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(ContextChangeError(format!("unknown context request `{selector}`"))),
        [snapshot] => Ok(**snapshot),
        _ => Err(ContextChangeError(format!(
            "context request `{request_id}` has multiple attempts; select one as `{request_id}#<attempt>`"
        ))),
    }
}

fn truncate_line(mut line: String) -> String {
    if line.len() <= MAX_RENDERED_LINE_BYTES {
        return line;
    }
    let mut end = MAX_RENDERED_LINE_BYTES.saturating_sub(3);
    while !line.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    line.truncate(end);
    line.push_str("...");
    line
}

fn truncate_output(mut output: String) -> String {
    if output.len() <= MAX_RENDERED_OUTPUT_BYTES {
        return output;
    }
    let mut end = MAX_RENDERED_OUTPUT_BYTES.saturating_sub(4);
    while !output.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    output.truncate(end);
    output.push_str("\n...");
    output
}

fn item_line(item: &ContextItemMeta) -> String {
    format!(
        "{}: {} · {} bytes · {} tokens · {} ({})",
        item.id,
        item.kind.label(),
        item.byte_count,
        item.token_estimate,
        item.visibility.label(),
        item.reason_code
    )
}

fn aggregate_line(label: &str, snapshot: &ContextSnapshot) -> String {
    let projection = snapshot.ledger.projection.as_ref();
    let candidate = snapshot.ledger.items.len();
    let selected = projection.map_or_else(
        || {
            snapshot
                .ledger
                .items
                .iter()
                .filter(|item| item.visibility.is_rendered())
                .count()
        },
        |projection| projection.selected,
    );
    let provenance = projection.map_or("unknown", |projection| projection.estimate_provenance.as_str());
    let provider = snapshot.provider_usage.as_ref().map_or_else(
        || "unknown (unknown)".to_string(),
        |usage| {
            format!(
                "{} ({})",
                usage
                    .inclusive_input_tokens
                    .value
                    .map_or_else(|| "unknown".to_string(), |value| value.to_string()),
                provenance_label(&usage.inclusive_input_tokens.provenance)
            )
        },
    );
    format!(
        "{label}: candidates {candidate}, selected {selected} · budget used/target/auto/available {}/{}/{}/{} · projected {} tokens ({provenance}), {} serialized bytes (exact request body) · provider input {provider}",
        snapshot.ledger.used,
        snapshot.ledger.target,
        snapshot.ledger.auto_compaction_threshold,
        snapshot.ledger.available_input,
        snapshot
            .estimated_input_tokens
            .map_or_else(|| "unknown".to_string(), |value| value.to_string()),
        snapshot
            .serialized_bytes
            .map_or_else(|| "unknown".to_string(), |value| value.to_string()),
    )
}

fn provenance_label(provenance: &MeasurementProvenance) -> String {
    match provenance {
        MeasurementProvenance::ExactSerialized { boundary } => format!("exact {boundary}"),
        MeasurementProvenance::Estimated { estimator, version } => format!("estimated {estimator}/{version}"),
        MeasurementProvenance::ProviderReported { provider, component } => {
            format!("provider {provider}/{component}")
        }
        MeasurementProvenance::Derived { rule, version } => format!("derived {rule}/{version}"),
        MeasurementProvenance::Unknown => "unknown".to_string(),
    }
}

fn measurement_label(value: Option<u64>, provenance: &MeasurementProvenance) -> String {
    format!("{} ({})", value_label(value), provenance_label(provenance))
}

fn value_label(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn milliseconds_label(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| format!("{value}ms"))
}

fn snapshot_state_label(state: ContextSnapshotState) -> &'static str {
    match state {
        ContextSnapshotState::Dispatched => "dispatched",
        ContextSnapshotState::Completed => "completed",
        ContextSnapshotState::Failed => "failed",
        ContextSnapshotState::Interrupted => "interrupted",
    }
}

fn request_label(snapshot: &ContextSnapshot) -> String {
    format!(
        "{} (attempt {}, {})",
        snapshot.request_id, snapshot.attempt, snapshot.route
    )
}

fn compaction_detail(audit: &CompactionAudit) -> String {
    let receipt = audit.local_receipt.map_or_else(
        || "measurements unavailable".to_string(),
        |receipt| {
            format!(
                "{} -> {} estimated tokens, {} -> {} bytes, reclaimed {} estimated tokens",
                receipt.before_token_estimate,
                receipt.after_token_estimate,
                receipt.before_bytes,
                receipt.after_bytes,
                receipt
                    .before_token_estimate
                    .saturating_sub(receipt.after_token_estimate)
            )
        },
    );
    let source_refs = bounded_list(&audit.source_summary_ids);
    let recovery = bounded_list(&audit.recovery_handles);
    let usage = audit.usage.map_or_else(
        || "usage unavailable".to_string(),
        |usage| {
            format!(
                "usage {} input/{} output tokens",
                usage.input_tokens, usage.output_tokens
            )
        },
    );
    format!(
        "source range {}..={} · retained recent starts {} · summary ref {} · source refs [{}] · recovery [{}] · trigger {:?}, risk {:?}, review {} · model {} · {usage} · native edit {} · {receipt}",
        audit.covered_start_seq,
        audit.covered_end_seq,
        audit.covered_end_seq.saturating_add(1),
        audit.summary_id.as_deref().unwrap_or("unavailable"),
        source_refs,
        recovery,
        audit.trigger,
        audit.risk,
        audit.review.map_or("unavailable", |review| review.label()),
        audit.model,
        native_edit_label(audit),
    )
}

fn bounded_list(values: &[String]) -> String {
    let mut rendered = values
        .iter()
        .take(MAX_DETAIL_ITEMS)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > MAX_DETAIL_ITEMS {
        rendered.push_str(&format!(", +{} more", values.len() - MAX_DETAIL_ITEMS));
    }
    rendered
}

fn native_edit_label(audit: &CompactionAudit) -> &str {
    match audit.native_context_edit.as_ref() {
        Some(super::ProviderContextEdit::Applied { .. }) => "applied",
        Some(super::ProviderContextEdit::Unavailable { .. }) => "unavailable",
        None => "unknown",
    }
}

fn reduction_event(snapshot: &ContextSnapshot) -> String {
    let receipts = snapshot
        .transformations
        .iter()
        .filter(|receipt| receipt.mode == ContextReductionMode::Applied)
        .collect::<Vec<_>>();
    let before = receipts.iter().map(|receipt| receipt.before_bytes).sum::<u64>();
    let after = receipts.iter().map(|receipt| receipt.after_bytes).sum::<u64>();
    truncate_line(format!(
        "context reduced for request {}: {} item(s), {} -> {} bytes (reclaimed {}) · details /context changes",
        snapshot.request_id,
        receipts.len(),
        before,
        after,
        before.saturating_sub(after)
    ))
}

fn compaction_event(audit: &CompactionAudit, request_id: Option<&str>) -> String {
    truncate_line(format!(
        "context compacted for request {}: {} · details /context changes",
        request_id.unwrap_or("next request (not yet assigned)"),
        compaction_detail(audit)
    ))
}

fn recovery_event(item: &ContextItemMeta, reason: &str, request_id: Option<&str>) -> String {
    truncate_line(format!(
        "context recovered for request {}: {} · artifact {} · {} · details /context changes",
        request_id.unwrap_or("next request (not yet assigned)"),
        item.id,
        item.artifact_handle.as_deref().unwrap_or("unavailable"),
        reason
    ))
}

fn option_label(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("none")
}

fn hash_label(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use thndrs_agent::context::{
        ContextItemKind, ContextLifecycle, ContextVisibility, ModelLimitConfidence, ModelLimitSource,
    };
    use thndrs_agent::{ContextReductionMode, ContextReductionReceipt, ProviderUsageComponents, ProviderUsageRule};

    use super::*;
    use crate::session::ContextLedgerMeta;

    #[test]
    fn changes_compare_terminal_request_snapshots_by_stable_item_id() {
        let mut history = ContextHistory::default();
        history.record_snapshot(snapshot(
            "request-1",
            ContextSnapshotState::Dispatched,
            vec![item("stable", 1)],
        ));
        history.record_snapshot(snapshot(
            "request-1",
            ContextSnapshotState::Completed,
            vec![item("stable", 1)],
        ));
        let mut changed = snapshot(
            "request-2",
            ContextSnapshotState::Completed,
            vec![item("stable", 2), item("new", 1)],
        );
        changed.ledger.items[0].visibility = ContextVisibility::Candidate;
        changed.transformations.push(ContextReductionReceipt {
            item_id: "stable".to_string(),
            method: "tool-output-cap".to_string(),
            version: "1".to_string(),
            before_bytes: 900,
            after_bytes: 300,
            lossy: true,
            mode: ContextReductionMode::Applied,
            diagnostic: None,
        });
        history.record_snapshot(changed);

        let rendered = history.render_changes(&[]).expect("latest request snapshots");

        assert!(rendered.contains("request-1 (attempt 1"));
        assert!(rendered.contains("request-2 (attempt 1"));
        assert!(rendered.contains("additions:\n- new:"));
        assert!(rendered.contains("replacements:\n- stable:"));
        assert!(rendered.contains("omissions:\n- stable: visible -> candidate"));
        assert!(rendered.contains("reductions:\n- stable: tool-output-cap 900 -> 300 bytes"));
    }

    #[test]
    fn changes_require_two_explicit_request_ids_in_chronological_order() {
        let mut history = ContextHistory::default();
        history.record_snapshot(snapshot("request-1", ContextSnapshotState::Completed, vec![]));
        history.record_snapshot(snapshot("request-2", ContextSnapshotState::Completed, vec![]));

        assert!(history.render_changes(&["request-1"]).is_err());
        assert!(history.render_changes(&["request-2", "request-1"]).is_err());
        assert!(history.render_changes(&["missing", "request-2"]).is_err());
    }

    #[test]
    fn changes_ignore_dispatched_snapshots_and_require_attempt_for_ambiguous_requests() {
        let mut history = ContextHistory::default();
        history.record_snapshot(snapshot("request-1", ContextSnapshotState::Completed, vec![]));
        let mut retry = snapshot("request-1", ContextSnapshotState::Failed, vec![]);
        retry.attempt = 2;
        history.record_snapshot(retry);
        history.record_snapshot(snapshot("request-2", ContextSnapshotState::Completed, vec![]));
        history.record_snapshot(snapshot("request-3", ContextSnapshotState::Dispatched, vec![]));

        let rendered = history.render_changes(&[]).expect("latest terminal attempts");
        assert!(rendered.contains("request-1 (attempt 2"));
        assert!(rendered.contains("request-2 (attempt 1"));
        assert!(!rendered.contains("request-3"));
        assert!(history.render_changes(&["request-1", "request-2"]).is_err());
        assert!(history.render_changes(&["request-1#2", "request-2"]).is_ok());
    }

    #[test]
    fn terminal_applied_reduction_is_a_semantic_context_event() {
        let mut snapshot = snapshot("request-1", ContextSnapshotState::Failed, vec![item("tool", 1)]);
        snapshot.transformations.push(ContextReductionReceipt {
            item_id: "tool".to_string(),
            method: "cap".to_string(),
            version: "1".to_string(),
            before_bytes: 100,
            after_bytes: 40,
            lossy: true,
            mode: ContextReductionMode::Applied,
            diagnostic: None,
        });
        let record = SessionRecord::ContextSnapshot {
            schema_version: 1,
            seq: 8,
            time: "now".to_string(),
            snapshot: Box::new(snapshot),
        };
        let history = ContextHistory::from_records(std::slice::from_ref(&record));

        let (id, text) = history.transcript_event(&record).expect("context event");

        assert_eq!(id, "context:reduction:8");
        assert!(text.contains("request request-1"));
        assert!(text.contains("100 -> 40 bytes"));
        assert!(text.contains("details /context changes"));
    }

    #[test]
    fn request_details_show_observations_links_and_unknowns_without_request_content() {
        let mut history = ContextHistory::default();
        history.record_snapshot(snapshot("request-1", ContextSnapshotState::Completed, vec![]));
        let mut inspected = snapshot("request-2", ContextSnapshotState::Completed, vec![item("tool", 2)]);
        inspected.turn_id = "turn-2".to_string();
        inspected.duration_ms = Some(125);
        inspected.time_to_first_token_ms = Some(20);
        inspected.tool_count = Some(1);
        inspected.tool_duration_ms = Some(40);
        inspected.transcript_entries = vec!["block:4".to_string(), "tool:call-1".to_string()];
        inspected.provider_usage = Some(
            ProviderUsageComponents {
                input_tokens: Some(80),
                output_tokens: Some(12),
                cache_read_input_tokens: Some(15),
                cache_creation_input_tokens: Some(5),
                reasoning_tokens: None,
            }
            .normalize("provider", ProviderUsageRule::AnthropicMessages),
        );
        inspected.transformations.push(ContextReductionReceipt {
            item_id: "tool".to_string(),
            method: "cap".to_string(),
            version: "1".to_string(),
            before_bytes: 100,
            after_bytes: 40,
            lossy: true,
            mode: ContextReductionMode::Applied,
            diagnostic: None,
        });
        history.record_snapshot(inspected);

        let rendered = history.render_request(Some("request-2#1")).expect("request details");

        assert!(rendered.contains("context request request-2#1 · completed"));
        assert!(rendered.contains("duration  125ms"));
        assert!(rendered.contains("first token  20ms"));
        assert!(rendered.contains("input provider  100 (derived"));
        assert!(rendered.contains("input fresh  80"));
        assert!(rendered.contains("cache read  15"));
        assert!(rendered.contains("cache write  5"));
        assert!(rendered.contains("output  12"));
        assert!(rendered.contains("reasoning  unknown"));
        assert!(rendered.contains("tools  1 · 40ms"));
        assert!(rendered.contains("1 reductions"));
        assert!(rendered.contains("turn  turn-2"));
        assert!(rendered.contains("transcript  block:4, tool:call-1"));
        assert!(rendered.contains("/context changes request-1#1 request-2#1"));
        assert!(!rendered.contains("messages"));
    }

    #[test]
    fn request_details_default_to_latest_attempt_and_label_missing_measurements_unknown() {
        let mut history = ContextHistory::default();
        let mut first = snapshot("request-1", ContextSnapshotState::Failed, vec![]);
        first.attempt = 1;
        history.record_snapshot(first);
        let mut retry = snapshot("request-1", ContextSnapshotState::Completed, vec![]);
        retry.attempt = 2;
        history.record_snapshot(retry);

        let rendered = history.render_request(None).expect("latest request attempt");

        assert!(rendered.contains("context request request-1#2"));
        assert!(rendered.contains("duration  unknown"));
        assert!(rendered.contains("first token  unknown"));
        assert!(rendered.contains("input provider  unknown"));
        assert!(rendered.contains("tools  unknown · unknown"));
        assert!(history.render_request(Some("request-1")).is_err());
    }

    fn snapshot(request_id: &str, state: ContextSnapshotState, items: Vec<ContextItemMeta>) -> ContextSnapshot {
        ContextSnapshot {
            snapshot_version: 1,
            session_id: "session".to_string(),
            request_id: request_id.to_string(),
            turn_id: "turn".to_string(),
            attempt: 1,
            provider: "provider".to_string(),
            model: "model".to_string(),
            route: "provider/model".to_string(),
            state,
            ledger: ContextLedgerMeta {
                items,
                available_input: 10_000,
                target: 8_000,
                auto_compaction_threshold: 9_000,
                used: 1_000,
                limit_source: ModelLimitSource::Fallback,
                limit_confidence: ModelLimitConfidence::Conservative,
                projection: None,
                diagnostics: Vec::new(),
            },
            serialized_bytes: Some(3_000),
            estimated_input_tokens: Some(1_000),
            transformations: Vec::new(),
            provider_usage: None,
            duration_ms: None,
            time_to_first_token_ms: None,
            tool_count: None,
            tool_duration_ms: None,
            transcript_entries: Vec::new(),
        }
    }

    fn item(id: &str, hash: u64) -> ContextItemMeta {
        ContextItemMeta {
            id: id.to_string(),
            kind: ContextItemKind::Transcript,
            source_path: None,
            scope: Some(".".to_string()),
            content_hash: Some(hash),
            artifact_handle: None,
            byte_count: 100,
            token_estimate: 50,
            visibility: ContextVisibility::Visible,
            reason_code: "selected".to_string(),
            reason: "selected".to_string(),
            lifecycle: ContextLifecycle::default(),
        }
    }
}
