//! Deterministic replay fixtures and evaluation for context projections.
//!
//! Replay data is deliberately provider-neutral. A fixture describes ordered
//! evidence and required facts; it does not prescribe the prose a projection
//! must use. Policies select items, and this module measures the resulting
//! projection and checks that required facts and recovery handles remain
//! available. The default evaluator does not sample wall-clock time, which
//! keeps JSON and Markdown reports suitable for golden-file comparisons.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::accounting::{
    ByteMeasurement, MeasurementProvenance, ProviderUsage, ProviderUsageComponents, ProviderUsageRule, TokenMeasurement,
};
use crate::accounting::{TOKEN_ESTIMATOR_VERSION, estimate_serialized_tokens};

/// Version of the on-disk context replay fixture schema.
pub const REPLAY_FIXTURE_SCHEMA_VERSION: &str = "context-replay-v1";

/// Version of the deterministic replay report schema.
pub const REPLAY_REPORT_SCHEMA_VERSION: &str = "context-replay-report-v1";

/// Policy used to select items for a replay projection.
pub trait ReplayPolicy {
    /// Stable policy name written to reports.
    fn name(&self) -> &str;

    /// Whether this item is present in the policy's model projection.
    fn include(&self, item: &ReplayItem) -> bool;
}

/// The complete unoptimized projection policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BaselinePolicy;

impl ReplayPolicy for BaselinePolicy {
    fn name(&self) -> &str {
        "baseline"
    }

    fn include(&self, _item: &ReplayItem) -> bool {
        true
    }
}

/// A named candidate policy which can omit explicit item ids.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidatePolicy {
    name: String,
    omitted_ids: BTreeSet<String>,
}

impl CandidatePolicy {
    /// Create a candidate that initially includes every fixture item.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), omitted_ids: BTreeSet::new() }
    }

    /// Return a candidate with one item omitted.
    pub fn omit(mut self, id: impl Into<String>) -> Self {
        self.omitted_ids.insert(id.into());
        self
    }

    /// Return a candidate with all supplied item ids omitted.
    pub fn omit_all<I, S>(mut self, ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.omitted_ids.extend(ids.into_iter().map(Into::into));
        self
    }

    /// Return the ids explicitly omitted by this candidate.
    pub fn omitted_ids(&self) -> &BTreeSet<String> {
        &self.omitted_ids
    }
}

impl ReplayPolicy for CandidatePolicy {
    fn name(&self) -> &str {
        &self.name
    }

    fn include(&self, item: &ReplayItem) -> bool {
        !self.omitted_ids.contains(&item.id)
    }
}

/// Error returned when fixture data or a replay policy violates an invariant.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReplayError {
    /// The fixture cannot be loaded or does not satisfy its schema.
    #[error("invalid replay fixture: {0}")]
    InvalidFixture(String),
    /// A baseline or candidate failed a required preservation invariant.
    #[error("replay invariant failed for {policy}: {message}")]
    InvariantViolation {
        /// Policy which failed validation.
        policy: String,
        /// Stable explanation of the failed invariant.
        message: String,
    },
    /// A report could not be serialized.
    #[error("replay serialization failed: {0}")]
    Serialization(String),
}

/// Kind of evidence represented by one replay item.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayItemKind {
    /// User-provided instruction or constraint.
    UserTurn,
    /// Assistant response or reasoning summary.
    Assistant,
    /// Tool output which may be repeated or overlapping.
    ToolOutput,
    /// Passing test or check output.
    PassingTest,
    /// Failing test or check output.
    FailingTest,
    /// High-volume progress output.
    Progress,
    /// An error or diagnostic.
    Error,
    /// A command invocation.
    Command,
    /// A write operation and its result.
    Write,
    /// Evidence explicitly protected from lossy removal.
    ProtectedEvidence,
}

impl ReplayItemKind {
    /// Stable fixture/report label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::UserTurn => "user_turn",
            Self::Assistant => "assistant",
            Self::ToolOutput => "tool_output",
            Self::PassingTest => "passing_test",
            Self::FailingTest => "failing_test",
            Self::Progress => "progress",
            Self::Error => "error",
            Self::Command => "command",
            Self::Write => "write",
            Self::ProtectedEvidence => "protected_evidence",
        }
    }
}

/// Scenario represented by a frozen fixture.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayScenario {
    /// Repeated reads of the same source.
    RepeatedReads,
    /// Repeated searches of the same workspace state.
    RepeatedSearches,
    /// Reads whose ranges overlap without being identical.
    OverlappingReads,
    /// Passing test output.
    PassingTests,
    /// Failing test output.
    FailingTests,
    /// Noisy progress output.
    NoisyProgress,
    /// An error which occurs in the middle of a sequence.
    MiddlePositionError,
    /// Repeated commands with a changed state fingerprint.
    StateChangingCommands,
    /// A failed write carrying a large input.
    FailedLargeWrite,
    /// Evidence protected from automatic omission.
    ProtectedEvidence,
    /// Recorded provider cache components.
    CacheComponents,
    /// Artifact recovery.
    Recovery,
}

impl ReplayScenario {
    /// Stable fixture/report label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::RepeatedReads => "repeated_reads",
            Self::RepeatedSearches => "repeated_searches",
            Self::OverlappingReads => "overlapping_reads",
            Self::PassingTests => "passing_tests",
            Self::FailingTests => "failing_tests",
            Self::NoisyProgress => "noisy_progress",
            Self::MiddlePositionError => "middle_position_error",
            Self::StateChangingCommands => "state_changing_commands",
            Self::FailedLargeWrite => "failed_large_write",
            Self::ProtectedEvidence => "protected_evidence",
            Self::CacheComponents => "cache_components",
            Self::Recovery => "recovery",
        }
    }
}

/// Controls whether evaluator timing is measured or deterministic.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplayTiming {
    /// Use zero timing so serialized reports compare byte-for-byte.
    #[default]
    Deterministic,
    /// Record wall-clock elapsed microseconds for exploratory runs.
    Measured,
}

/// One ordered piece of fixture evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayItem {
    /// Stable item id within this fixture.
    pub id: String,
    /// Kind of evidence.
    pub kind: ReplayItemKind,
    /// Human-readable source label; it is not used as a required fact.
    pub label: String,
    /// Provider-neutral projection text for this item.
    pub content: String,
    /// Original evidence size when the fixture content is a sample.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_bytes: Option<u64>,
    /// Required fact ids supported by this item.
    #[serde(default)]
    pub fact_ids: Vec<String>,
    /// Opaque recovery handle, when this item is recoverable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_handle: Option<String>,
    /// Whether this item is protected evidence.
    #[serde(default)]
    pub protected: bool,
    /// State fingerprint for state-aware identity checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_fingerprint: Option<String>,
    /// Opaque logical-source key paired with `state_fingerprint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_source: Option<String>,
}

impl ReplayItem {
    /// Exact UTF-8 size of the item content before projection framing.
    pub fn content_bytes(&self) -> u64 {
        self.content.len() as u64
    }

    /// Size represented by a receipt before any candidate projection decision.
    pub fn source_bytes(&self) -> u64 {
        self.original_bytes.unwrap_or_else(|| self.content_bytes())
    }
}

/// A fact which must remain represented, independent of expected prose.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequiredFact {
    /// Stable fact id.
    pub id: String,
    /// Maintainer-facing explanation of why the fact matters.
    pub description: String,
    /// Whether the fact is protected from lossy omission.
    #[serde(default)]
    pub protected: bool,
}

/// Expected recovery behavior for one artifact handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryCase {
    /// Stable recovery case id.
    pub id: String,
    /// Opaque artifact handle expected to be available when selected.
    pub artifact_handle: String,
    /// Whether this fixture expects the policy to retain the handle.
    pub expected_available: bool,
}

/// Provider usage explicitly recorded alongside a fixture.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordedProviderUsage {
    /// Provider adapter label.
    pub provider: String,
    /// Provider-specific normalization rule.
    pub rule: ProviderUsageRule,
    /// Raw components recorded by the fixture.
    pub components: ProviderUsageComponents,
}

impl RecordedProviderUsage {
    /// Normalize the recorded components for inclusion in a report.
    pub fn normalize(&self) -> ProviderUsage {
        self.components.normalize(&self.provider, self.rule)
    }
}

/// A versioned, deterministic context replay fixture.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayFixture {
    /// Fixture schema version.
    pub schema_version: String,
    /// Stable fixture id.
    pub id: String,
    /// Maintainer-facing description.
    pub description: String,
    /// Scenarios deliberately covered by this fixture.
    pub scenarios: Vec<ReplayScenario>,
    /// Ordered context evidence.
    pub items: Vec<ReplayItem>,
    /// Facts that the evaluator must preserve.
    pub required_facts: Vec<RequiredFact>,
    /// Recovery outcomes to check.
    #[serde(default)]
    pub recovery: Vec<RecoveryCase>,
    /// Optional provider usage; absent means the report must omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_usage: Option<RecordedProviderUsage>,
}

impl ReplayFixture {
    /// Validate schema, ids, fact references, and recovery handles.
    pub fn validate(&self) -> Result<(), ReplayError> {
        if self.schema_version != REPLAY_FIXTURE_SCHEMA_VERSION {
            return Err(ReplayError::InvalidFixture(format!(
                "schema_version must be {REPLAY_FIXTURE_SCHEMA_VERSION}"
            )));
        }
        if self.id.trim().is_empty() {
            return Err(ReplayError::InvalidFixture("id must not be empty".to_string()));
        }

        let mut item_ids = BTreeSet::new();
        for item in &self.items {
            if item.id.trim().is_empty() || !item_ids.insert(&item.id) {
                return Err(ReplayError::InvalidFixture(format!(
                    "item id is empty or duplicated: {}",
                    item.id
                )));
            }
        }

        let fact_ids: BTreeSet<&str> = self.required_facts.iter().map(|fact| fact.id.as_str()).collect();
        if fact_ids.len() != self.required_facts.len() {
            return Err(ReplayError::InvalidFixture(
                "required fact ids must be unique".to_string(),
            ));
        }
        for item in &self.items {
            for fact_id in &item.fact_ids {
                if !fact_ids.contains(fact_id.as_str()) {
                    return Err(ReplayError::InvalidFixture(format!(
                        "item {} references unknown fact {fact_id}",
                        item.id
                    )));
                }
            }
        }

        let mut recovery_ids = BTreeSet::new();
        let mut handles = BTreeSet::new();
        for case in &self.recovery {
            if case.id.trim().is_empty() || !recovery_ids.insert(&case.id) {
                return Err(ReplayError::InvalidFixture(format!(
                    "recovery id is empty or duplicated: {}",
                    case.id
                )));
            }
            if case.artifact_handle.trim().is_empty() || !handles.insert(&case.artifact_handle) {
                return Err(ReplayError::InvalidFixture(format!(
                    "recovery handle is empty or duplicated: {}",
                    case.artifact_handle
                )));
            }
        }
        Ok(())
    }

    /// Serialize this fixture as stable pretty JSON.
    pub fn to_json(&self) -> Result<String, ReplayError> {
        serde_json::to_string_pretty(self).map_err(|error| ReplayError::Serialization(error.to_string()))
    }
}

/// A reduction or selection receipt for one fixture item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayReceipt {
    /// Stable item id.
    pub item_id: String,
    /// Policy or reducer method label.
    pub method: String,
    /// Method version.
    pub version: String,
    /// Content bytes before this policy decision.
    pub before_bytes: u64,
    /// Framed projection bytes after this policy decision.
    pub after_bytes: u64,
    /// Whether the decision can remove information.
    pub lossy: bool,
}

/// Required-fact preservation result in a report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequiredFactResult {
    /// Stable fact id.
    pub id: String,
    /// Whether at least one selected item carries the fact.
    pub preserved: bool,
    /// Selected evidence item ids carrying the fact.
    pub item_ids: Vec<String>,
}

/// Recovery result in a report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoveryOutcome {
    /// Stable recovery case id.
    pub id: String,
    /// Opaque handle from the fixture.
    pub artifact_handle: String,
    /// Expected availability.
    pub expected_available: bool,
    /// Availability produced by the policy projection.
    pub available: bool,
}

/// The ephemeral projection and its deterministic measurements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayProjection {
    /// Policy name.
    pub policy: String,
    /// Selected item ids in fixture order.
    pub item_ids: Vec<String>,
    /// Framed provider-neutral projection text, retained only during replay.
    pub rendered: String,
    /// Exact bytes of the projection text.
    pub exact_bytes: ByteMeasurement,
    /// Conservative estimate based on those exact bytes.
    pub estimated_tokens: TokenMeasurement,
    /// Per-item selection receipts.
    pub receipts: Vec<ReplayReceipt>,
    /// Recovery handles present in the projection.
    pub recovery_handles: Vec<String>,
}

/// Reportable measurements and invariants for one policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionReport {
    /// Policy name.
    pub policy: String,
    /// Exact projection bytes.
    pub exact_bytes: ByteMeasurement,
    /// Estimated projection tokens.
    pub estimated_tokens: TokenMeasurement,
    /// Stable digest of the projection without persisting its body.
    pub projection_digest: String,
    /// Number of selected items.
    pub item_count: usize,
    /// Selection/reduction receipts.
    pub receipts: Vec<ReplayReceipt>,
    /// Required facts and their supporting selected items.
    pub required_facts: Vec<RequiredFactResult>,
    /// Recovery outcomes.
    pub recovery: Vec<RecoveryOutcome>,
    /// Elapsed evaluator time in microseconds; zero in deterministic mode.
    pub elapsed_micros: u64,
}

/// Size comparison between baseline and candidate projections.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayComparison {
    /// Candidate bytes minus baseline bytes.
    pub exact_bytes_delta: i64,
    /// Candidate estimated tokens minus baseline estimated tokens.
    pub estimated_tokens_delta: i64,
}

/// Complete deterministic report for one baseline/candidate evaluation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReplayReport {
    /// Report schema version.
    pub schema_version: String,
    /// Fixture id.
    pub fixture_id: String,
    /// Baseline measurements.
    pub baseline: ProjectionReport,
    /// Candidate measurements.
    pub candidate: ProjectionReport,
    /// Size comparison.
    pub comparison: ReplayComparison,
    /// Provider usage, only when the fixture explicitly records it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_usage: Option<ProviderUsage>,
}

/// Typed evaluator for baseline/candidate fixture comparisons.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplayEvaluator {
    timing: ReplayTiming,
}

impl ReplayEvaluator {
    /// Return an evaluator whose report is deterministic across runs.
    pub const fn new() -> Self {
        Self { timing: ReplayTiming::Deterministic }
    }

    /// Return an evaluator which records wall-clock timing for each policy.
    pub const fn measured() -> Self {
        Self { timing: ReplayTiming::Measured }
    }

    /// Evaluate a baseline and candidate and fail on preservation/recovery violations.
    pub fn evaluate<B: ReplayPolicy, C: ReplayPolicy>(
        &self, fixture: &ReplayFixture, baseline: &B, candidate: &C,
    ) -> Result<ReplayReport, ReplayError> {
        fixture.validate()?;
        let baseline = self.evaluate_policy(fixture, baseline)?;
        let candidate = self.evaluate_policy(fixture, candidate)?;
        let baseline_tokens = baseline.estimated_tokens.value.unwrap_or_default();
        let candidate_tokens = candidate.estimated_tokens.value.unwrap_or_default();
        Ok(ReplayReport {
            schema_version: REPLAY_REPORT_SCHEMA_VERSION.to_string(),
            fixture_id: fixture.id.clone(),
            comparison: ReplayComparison {
                exact_bytes_delta: signed_delta(candidate.exact_bytes.value, baseline.exact_bytes.value),
                estimated_tokens_delta: signed_delta(candidate_tokens, baseline_tokens),
            },
            provider_usage: fixture.provider_usage.as_ref().map(RecordedProviderUsage::normalize),
            baseline,
            candidate,
        })
    }

    fn evaluate_policy<P: ReplayPolicy>(
        &self, fixture: &ReplayFixture, policy: &P,
    ) -> Result<ProjectionReport, ReplayError> {
        let started = (self.timing == ReplayTiming::Measured).then(Instant::now);
        let projection = project_fixture(fixture, policy);
        let required_facts = required_fact_results(fixture, &projection.item_ids);
        let recovery = recovery_outcomes(fixture, &projection.recovery_handles);
        if let Some(fact) = required_facts.iter().find(|fact| !fact.preserved) {
            return Err(ReplayError::InvariantViolation {
                policy: policy.name().to_string(),
                message: format!("required fact {} was not preserved", fact.id),
            });
        }
        if let Some(outcome) = recovery
            .iter()
            .find(|outcome| outcome.available != outcome.expected_available)
        {
            return Err(ReplayError::InvariantViolation {
                policy: policy.name().to_string(),
                message: format!("recovery case {} availability was unexpected", outcome.id),
            });
        }
        Ok(ProjectionReport {
            policy: projection.policy,
            exact_bytes: projection.exact_bytes,
            estimated_tokens: projection.estimated_tokens,
            projection_digest: digest(&projection.rendered),
            item_count: projection.item_ids.len(),
            receipts: projection.receipts,
            required_facts,
            recovery,
            elapsed_micros: started.map_or(0, |started| started.elapsed().as_micros() as u64),
        })
    }
}

impl ReplayReport {
    /// Serialize the report as stable pretty JSON.
    pub fn to_json(&self) -> Result<String, ReplayError> {
        serde_json::to_string_pretty(self).map_err(|error| ReplayError::Serialization(error.to_string()))
    }

    /// Render the report as deterministic Markdown without projection bodies.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(&mut out, "# Context replay: {}", self.fixture_id);
        let _ = writeln!(&mut out, "\nSchema: `{}`", self.schema_version);
        render_projection_markdown(&mut out, &self.baseline);
        render_projection_markdown(&mut out, &self.candidate);
        let _ = writeln!(
            &mut out,
            "\n## Comparison\n\n| Measure | Delta |\n| --- | ---: |\n| Exact bytes | {} |\n| Estimated tokens | {} |",
            self.comparison.exact_bytes_delta, self.comparison.estimated_tokens_delta
        );
        render_fact_markdown(&mut out, &self.baseline, &self.candidate);
        render_recovery_markdown(&mut out, &self.baseline, &self.candidate);
        render_receipt_markdown(&mut out, &self.baseline);
        render_receipt_markdown(&mut out, &self.candidate);
        if let Some(provider_usage) = &self.provider_usage {
            let _ = writeln!(
                &mut out,
                "\n## Provider usage\n\n- Provider: `{}`\n- Rule: `{}`\n- Inclusive input tokens: `{}`",
                provider_usage.provider,
                provider_usage.rule.label(),
                display_token(provider_usage.inclusive_input_tokens.value)
            );
        }
        out
    }
}

/// Parse and validate one JSON fixture.
pub fn load_fixture(json: &str) -> Result<ReplayFixture, ReplayError> {
    let fixture: ReplayFixture =
        serde_json::from_str(json).map_err(|error| ReplayError::InvalidFixture(error.to_string()))?;
    fixture.validate()?;
    Ok(fixture)
}

/// Select fixture items using a typed policy.
pub fn select_items<'a, P: ReplayPolicy>(fixture: &'a ReplayFixture, policy: &P) -> Vec<&'a ReplayItem> {
    fixture.items.iter().filter(|item| policy.include(item)).collect()
}

/// Build a pure measured projection from fixture items selected by a policy.
pub fn project_fixture<P: ReplayPolicy>(fixture: &ReplayFixture, policy: &P) -> ReplayProjection {
    let selected = select_items(fixture, policy);
    let selected_ids = selected.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
    let mut rendered = String::new();
    let mut recovery_handles = Vec::new();
    let mut selected_contributions = BTreeSet::new();
    for item in &selected {
        let contribution = render_item(item);
        rendered.push_str(&contribution);
        selected_contributions.insert(item.id.as_str());
        if let Some(handle) = &item.recovery_handle {
            recovery_handles.push(handle.clone());
        }
    }
    let receipts = fixture
        .items
        .iter()
        .map(|item| ReplayReceipt {
            item_id: item.id.clone(),
            method: if selected_contributions.contains(item.id.as_str()) {
                policy.name().to_string()
            } else {
                "policy_omitted".to_string()
            },
            version: REPLAY_REPORT_SCHEMA_VERSION.to_string(),
            before_bytes: item.source_bytes(),
            after_bytes: if selected_contributions.contains(item.id.as_str()) {
                render_item(item).len() as u64
            } else {
                0
            },
            lossy: !selected_contributions.contains(item.id.as_str()),
        })
        .collect();
    let exact_bytes = rendered.len() as u64;
    ReplayProjection {
        policy: policy.name().to_string(),
        item_ids: selected_ids,
        rendered,
        exact_bytes: ByteMeasurement {
            value: exact_bytes,
            provenance: MeasurementProvenance::ExactSerialized { boundary: "replay_projection".to_string() },
        },
        estimated_tokens: TokenMeasurement {
            value: Some(estimate_serialized_tokens(exact_bytes)),
            provenance: MeasurementProvenance::Estimated {
                estimator: "utf8_bytes_divisor_3_plus_item_overhead".to_string(),
                version: TOKEN_ESTIMATOR_VERSION.to_string(),
            },
        },
        receipts,
        recovery_handles,
    }
}

/// Evaluate one fixture with the complete baseline policy and a candidate.
pub fn evaluate_fixture<C: ReplayPolicy>(fixture: &ReplayFixture, candidate: &C) -> Result<ReplayReport, ReplayError> {
    ReplayEvaluator::new().evaluate(fixture, &BaselinePolicy, candidate)
}

fn required_fact_results(fixture: &ReplayFixture, selected_ids: &[String]) -> Vec<RequiredFactResult> {
    fixture
        .required_facts
        .iter()
        .map(|fact| {
            let item_ids = fixture
                .items
                .iter()
                .filter(|item| selected_ids.contains(&item.id) && item.fact_ids.iter().any(|id| id == &fact.id))
                .map(|item| item.id.clone())
                .collect::<Vec<_>>();
            RequiredFactResult { id: fact.id.clone(), preserved: !item_ids.is_empty(), item_ids }
        })
        .collect()
}

fn recovery_outcomes(fixture: &ReplayFixture, selected_handles: &[String]) -> Vec<RecoveryOutcome> {
    fixture
        .recovery
        .iter()
        .map(|case| RecoveryOutcome {
            id: case.id.clone(),
            artifact_handle: case.artifact_handle.clone(),
            expected_available: case.expected_available,
            available: selected_handles.iter().any(|handle| handle == &case.artifact_handle),
        })
        .collect()
}

fn render_item(item: &ReplayItem) -> String {
    format!(
        "<replay_item id=\"{}\" kind=\"{}\" label=\"{}\">\n{}\n</replay_item>\n",
        escape_xml(&item.id),
        item.kind.label(),
        escape_xml(&item.label),
        item.content
    )
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn signed_delta(current: u64, baseline: u64) -> i64 {
    (current as i128)
        .saturating_sub(baseline as i128)
        .clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

fn digest(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn display_token(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn render_projection_markdown(out: &mut String, projection: &ProjectionReport) {
    let _ = writeln!(
        out,
        "\n## {}\n\n| Measure | Value |\n| --- | ---: |\n| Exact bytes | {} |\n| Estimated tokens | {} |\n| Items | {} |\n| Projection digest | `{}` |\n| Elapsed micros | {} |",
        projection.policy,
        projection.exact_bytes.value,
        display_token(projection.estimated_tokens.value),
        projection.item_count,
        projection.projection_digest,
        projection.elapsed_micros
    );
}

fn render_fact_markdown(out: &mut String, baseline: &ProjectionReport, candidate: &ProjectionReport) {
    let _ = writeln!(
        out,
        "\n## Required facts\n\n| Fact | Baseline | Candidate |\n| --- | --- | --- |\n{}
",
        baseline
            .required_facts
            .iter()
            .zip(&candidate.required_facts)
            .map(|(baseline, candidate)| format!(
                "| `{}` | {} | {} |",
                baseline.id, baseline.preserved, candidate.preserved
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn render_recovery_markdown(out: &mut String, baseline: &ProjectionReport, candidate: &ProjectionReport) {
    let _ = writeln!(
        out,
        "\n## Recovery\n\n| Case | Expected | Baseline | Candidate |\n| --- | --- | --- | --- |\n{}
",
        baseline
            .recovery
            .iter()
            .zip(&candidate.recovery)
            .map(|(baseline, candidate)| format!(
                "| `{}` | {} | {} | {} |",
                baseline.id, baseline.expected_available, baseline.available, candidate.available
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn render_receipt_markdown(out: &mut String, projection: &ProjectionReport) {
    let _ = writeln!(
        out,
        "\n## {} receipts\n\n| Item | Method | Before bytes | After bytes | Lossy |\n| --- | --- | ---: | ---: | --- |\n{}\n",
        projection.policy,
        projection
            .receipts
            .iter()
            .map(|receipt| format!(
                "| `{}` | `{}` | {} | {} | {} |",
                receipt.item_id, receipt.method, receipt.before_bytes, receipt.after_bytes, receipt.lossy
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/context-replay/context.json");

    #[test]
    fn fixture_round_trip_and_report_are_deterministic() {
        let fixture = load_fixture(FIXTURE).expect("fixture loads");
        let candidate = CandidatePolicy::new("candidate");
        let first = evaluate_fixture(&fixture, &candidate).expect("evaluation succeeds");
        let second = evaluate_fixture(&fixture, &candidate).expect("evaluation succeeds");
        assert_eq!(first, second);
        assert_eq!(first.to_json().expect("json"), second.to_json().expect("json"));
        assert_eq!(first.to_markdown(), second.to_markdown());
        assert!(first.baseline.exact_bytes.value > 0);
        assert!(!first.baseline.receipts.is_empty());
        assert!(first.provider_usage.is_some());
    }

    #[test]
    fn required_fact_omission_fails_independently_of_timing() {
        let fixture = load_fixture(FIXTURE).expect("fixture loads");
        let error = evaluate_fixture(&fixture, &CandidatePolicy::new("candidate").omit("error-middle"))
            .expect_err("required fact omission must fail");
        assert!(matches!(error, ReplayError::InvariantViolation { .. }));
    }

    #[test]
    fn recovery_is_checked_by_opaque_handle() {
        let mut fixture = load_fixture(FIXTURE).expect("fixture loads");
        fixture.required_facts.retain(|fact| fact.id != "protected-write");
        fixture
            .items
            .iter_mut()
            .find(|item| item.id == "protected-output")
            .expect("protected item")
            .fact_ids
            .clear();
        let error = evaluate_fixture(&fixture, &CandidatePolicy::new("candidate").omit("protected-output"))
            .expect_err("recovery omission must fail");
        assert!(error.to_string().contains("recovery"));
    }

    #[test]
    fn unknown_provider_usage_is_omitted_from_json() {
        let mut fixture = load_fixture(FIXTURE).expect("fixture loads");
        fixture.provider_usage = None;
        let report = evaluate_fixture(&fixture, &CandidatePolicy::new("candidate")).expect("evaluation succeeds");
        assert!(report.provider_usage.is_none());
        assert!(!report.to_json().expect("json").contains("provider_usage"));
    }

    #[test]
    fn malformed_fixture_is_rejected() {
        let mut fixture = load_fixture(FIXTURE).expect("fixture loads");
        fixture.items[0].fact_ids.push("missing".to_string());
        assert!(matches!(fixture.validate(), Err(ReplayError::InvalidFixture(_))));
    }

    #[test]
    fn frozen_state_identity_cases_preserve_duplicate_and_changed_state_boundaries() {
        let fixture = load_fixture(FIXTURE).expect("fixture loads");
        let report = evaluate_fixture(&fixture, &CandidatePolicy::new("state-identical"))
            .expect("frozen fixture remains evaluator-valid");
        assert_eq!(report.fixture_id, fixture.id);

        let config =
            crate::context::ReductionConfig { state_identical: true, ..crate::context::ReductionConfig::disabled() };
        let mut history = Vec::new();
        let mut decisions = std::collections::BTreeMap::new();
        for item in &fixture.items {
            let Some(identity) = item
                .state_source
                .as_ref()
                .zip(item.state_fingerprint.as_ref())
                .and_then(|(source, fingerprint)| crate::context::StateProjectionIdentity::new(source, fingerprint))
            else {
                continue;
            };
            let candidate =
                crate::context::StateProjectionCandidate::new(&item.id, vec![item.content.clone()], Some(identity));
            let candidate = if item.protected { candidate.protected() } else { candidate };
            let reduction = crate::context::reduce_state_identical(&candidate, &history, &config);
            decisions.insert(item.id.as_str(), reduction.decision.clone());
            if let Some(record) = reduction.history_record(&candidate) {
                history.push(record);
            }
        }

        assert_eq!(
            decisions.get("read-config-repeat"),
            Some(&crate::context::StateProjectionDecision::DuplicateOf {
                canonical_id: "read-config-first".to_string(),
            })
        );
        assert_eq!(
            decisions.get("search-repeat"),
            Some(&crate::context::StateProjectionDecision::DuplicateOf { canonical_id: "search-first".to_string() })
        );
        assert_eq!(
            decisions.get("command-after-state"),
            Some(&crate::context::StateProjectionDecision::Supersedes {
                previous_id: "command-before-state".to_string(),
            })
        );
    }
}
