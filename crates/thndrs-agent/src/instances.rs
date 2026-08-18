//! Provider-neutral contracts for supervised harness instances.
//!
//! These types describe a child process without exposing provider wire
//! payloads, credentials, or its transcript. Application adapters own process
//! creation, storage, and provider-specific lowering.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Maximum runtime accepted by an instance specification.
pub const MAX_INSTANCE_RUNTIME_MS: u64 = 24 * 60 * 60 * 1_000;
/// Maximum tool calls accepted by an instance specification.
pub const MAX_INSTANCE_TOOL_CALLS: u32 = 1_000;
/// Maximum retained status events accepted by an instance specification.
pub const MAX_INSTANCE_RETAINED_EVENTS: u32 = 4_096;
/// Maximum child-output bytes accepted by an instance specification.
pub const MAX_INSTANCE_OUTPUT_BYTES: u64 = 1_024 * 1_024;
/// Maximum summary bytes retained in a settled result.
pub const MAX_INSTANCE_SUMMARY_BYTES: usize = 64 * 1_024;
/// Maximum verification evidence records retained in a settled result.
pub const MAX_INSTANCE_EVIDENCE: usize = 32;
/// Maximum changed paths retained in a settled result.
pub const MAX_INSTANCE_CHANGED_PATHS: usize = 512;

/// Error returned when an instance contract violates a safety invariant.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InstanceContractError {
    /// An opaque identifier or label was empty or too long.
    #[error("{field} must contain between 1 and {max_bytes} bytes")]
    InvalidText {
        /// Name of the rejected field.
        field: &'static str,
        /// Maximum permitted UTF-8 byte length.
        max_bytes: usize,
    },
    /// A path was required to be absolute.
    #[error("working directory must be absolute")]
    RelativeWorkingDirectory,
    /// A path contained a parent-directory traversal component.
    #[error("{field} must not contain parent-directory traversal")]
    PathTraversal {
        /// Name of the rejected field.
        field: &'static str,
    },
    /// A required numerical cap was zero or above its hard limit.
    #[error("{field} must be between {minimum} and {maximum}")]
    InvalidBound {
        /// Name of the rejected bound.
        field: &'static str,
        /// Inclusive lower bound.
        minimum: u64,
        /// Inclusive upper bound.
        maximum: u64,
    },
    /// A child depth exceeded the explicit delegation budget.
    #[error("instance depth {depth} exceeds maximum delegation depth {maximum_depth}")]
    DelegationDepthExceeded {
        /// Requested instance depth.
        depth: u16,
        /// Maximum allowed depth.
        maximum_depth: u16,
    },
    /// A lifecycle transition was not in the state machine.
    #[error("cannot transition an instance from {from} to {to}")]
    InvalidLifecycleTransition {
        /// Current lifecycle state.
        from: &'static str,
        /// Requested lifecycle state.
        to: &'static str,
    },
    /// Capacity-window identifiers must be unique.
    #[error("capacity window identifiers must be unique")]
    DuplicateCapacityWindow,
    /// A percentage was outside the inclusive zero-to-one-hundred range.
    #[error("capacity percentage must not exceed 100")]
    InvalidPercentage,
    /// A settled result had too many entries.
    #[error("{field} must not contain more than {maximum} entries")]
    TooManyEntries {
        /// Name of the rejected collection.
        field: &'static str,
        /// Maximum permitted entry count.
        maximum: usize,
    },
    /// A public or deserialized field was not canonically redacted.
    #[error("{field} must use canonical secret redaction")]
    NonCanonicalRedaction {
        /// Name of the rejected field.
        field: &'static str,
    },
}

/// Opaque local identifier for one supervised harness instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(String);

impl InstanceId {
    /// Build an opaque instance identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, InstanceContractError> {
        Ok(Self(validate_text("instance id", value.into(), 128)?))
    }

    /// Return the opaque identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque handle for a durable session owned by an application.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionHandle(String);

impl SessionHandle {
    /// Build an opaque session handle.
    pub fn new(value: impl Into<String>) -> Result<Self, InstanceContractError> {
        Ok(Self(validate_text("session handle", value.into(), 256)?))
    }

    /// Return the opaque handle.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Opaque handle for an application-owned change record.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChangeHandle(String);

impl ChangeHandle {
    /// Build an opaque change handle.
    pub fn new(value: impl Into<String>) -> Result<Self, InstanceContractError> {
        Ok(Self(validate_text("change handle", value.into(), 256)?))
    }

    /// Return the opaque handle.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit authorization reference required for write-capable work.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WriteApproval(String);

impl WriteApproval {
    /// Build an opaque reference to an explicit user authorization.
    pub fn new(value: impl Into<String>) -> Result<Self, InstanceContractError> {
        Ok(Self(validate_text("write approval", value.into(), 256)?))
    }

    /// Return the opaque authorization reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Provider-neutral selection of the exact model a child must use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum InstanceModel {
    /// A ChatGPT Codex model selected by its exact configured identifier.
    ChatGptCodex {
        /// Exact model identifier.
        model: String,
    },
    /// An OpenCode Zen model selected by its exact configured identifier.
    OpenCodeZen {
        /// Exact model identifier.
        model: String,
    },
    /// An OpenCode Go model selected by its exact configured identifier.
    OpenCodeGo {
        /// Exact model identifier.
        model: String,
    },
    /// A model configured by an external ACP agent.
    ConfiguredAcp {
        /// Application-owned configured ACP agent name.
        agent: String,
        /// Exact model identifier selected for that agent.
        model: String,
    },
}

impl InstanceModel {
    /// Return the exact provider-neutral model identifier.
    pub fn model(&self) -> &str {
        match self {
            Self::ChatGptCodex { model }
            | Self::OpenCodeZen { model }
            | Self::OpenCodeGo { model }
            | Self::ConfiguredAcp { model, .. } => model,
        }
    }

    /// Validate model and configured-agent identifiers.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        let _ = validate_text("model", self.model().to_string(), 256)?;
        if let Self::ConfiguredAcp { agent, .. } = self {
            let _ = validate_text("ACP agent", agent.clone(), 128)?;
        }
        Ok(())
    }
}

/// Durable-session behavior selected for a child instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceSessionPolicy {
    /// Start a durable session with a new application-owned handle.
    DurableNew,
    /// Continue a durable application-owned session.
    Resume(SessionHandle),
    /// Do not persist a session after this instance settles.
    Ephemeral,
}

/// Reasoning effort requested through an application/provider adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSetting {
    /// Let the selected provider choose the reasoning effort.
    Auto,
    /// Request the provider's enabled/default reasoning behavior.
    On,
    /// Do not request additional reasoning effort.
    None,
    /// Use the minimal supported reasoning effort.
    Minimal,
    /// Use a low reasoning effort.
    Low,
    /// Use a medium reasoning effort.
    Medium,
    /// Use a high reasoning effort.
    High,
    /// Use an extra-high reasoning effort.
    Xhigh,
    /// Use the maximum supported reasoning effort.
    Max,
}

/// Search behavior requested through an application/provider adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSetting {
    /// Do not permit search for this instance.
    Disabled,
    /// Permit no more than the stated number of search results.
    Enabled {
        /// Maximum results retained for a search operation.
        max_results: u16,
    },
}

impl SearchSetting {
    fn validate(self) -> Result<(), InstanceContractError> {
        if let Self::Enabled { max_results } = self {
            validate_bound("search results", u64::from(max_results), 1, 100)?;
        }
        Ok(())
    }
}

/// Explicit model behavior settings for an instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettings {
    /// Requested reasoning setting.
    pub reasoning: ReasoningSetting,
    /// Requested search setting.
    pub search: SearchSetting,
}

impl InstanceSettings {
    /// Validate all settings.
    pub fn validate(self) -> Result<(), InstanceContractError> {
        self.search.validate()
    }
}

/// Authority granted to a supervised instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceAuthority {
    /// The instance may inspect but not mutate its workspace.
    ReadOnly,
    /// The instance may write only after the named authorization was obtained.
    ApprovedWrite {
        /// Opaque reference to a separate explicit authorization decision.
        approval: WriteApproval,
    },
}

/// Explicit bounds on recursive delegation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DelegationBudget {
    /// Inclusive deepest child depth allowed by the parent policy.
    pub maximum_depth: u16,
    /// Maximum concurrently active instances in this delegation tree.
    pub maximum_concurrency: u16,
}

impl DelegationBudget {
    /// Validate a current depth against this finite budget.
    pub fn validate(self, depth: u16) -> Result<(), InstanceContractError> {
        validate_bound("maximum concurrency", u64::from(self.maximum_concurrency), 1, 1_024)?;
        if depth > self.maximum_depth {
            return Err(InstanceContractError::DelegationDepthExceeded { depth, maximum_depth: self.maximum_depth });
        }
        Ok(())
    }
}

/// Resource caps that keep an instance specification and result bounded.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceBounds {
    /// Maximum wall-clock runtime in milliseconds.
    pub runtime_ms: u64,
    /// Maximum tool calls the child may execute.
    pub tool_calls: u32,
    /// Maximum semantic events retained for inspection.
    pub retained_events: u32,
    /// Maximum child-output bytes retained by the supervisor.
    pub output_bytes: u64,
    /// Maximum bytes retained for the settled semantic summary.
    pub summary_bytes: usize,
}

impl InstanceBounds {
    /// Validate that all caps are explicit and within hard safety limits.
    pub fn validate(self) -> Result<(), InstanceContractError> {
        validate_bound("runtime", self.runtime_ms, 1, MAX_INSTANCE_RUNTIME_MS)?;
        validate_bound(
            "tool calls",
            u64::from(self.tool_calls),
            1,
            u64::from(MAX_INSTANCE_TOOL_CALLS),
        )?;
        validate_bound(
            "retained events",
            u64::from(self.retained_events),
            1,
            u64::from(MAX_INSTANCE_RETAINED_EVENTS),
        )?;
        validate_bound("output bytes", self.output_bytes, 1, MAX_INSTANCE_OUTPUT_BYTES)?;
        validate_bound(
            "summary bytes",
            self.summary_bytes as u64,
            1,
            MAX_INSTANCE_SUMMARY_BYTES as u64,
        )
    }
}

/// Complete provider-neutral specification for one supervised child instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceSpecification {
    /// Exact provider-neutral model selection.
    pub model: InstanceModel,
    /// Absolute workspace directory for the child process.
    pub cwd: PathBuf,
    /// Durable or ephemeral session behavior.
    pub session: InstanceSessionPolicy,
    /// Explicit reasoning and search settings.
    pub settings: InstanceSettings,
    /// Read-only or explicitly approved write authority.
    pub authority: InstanceAuthority,
    /// Optional supervising parent instance.
    pub parent_id: Option<InstanceId>,
    /// Current depth in the delegation tree.
    pub depth: u16,
    /// Finite limits for recursive delegation.
    pub delegation: DelegationBudget,
    /// Finite limits for this instance's work and retained data.
    pub bounds: InstanceBounds,
}

impl InstanceSpecification {
    /// Validate this pure contract before process creation.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        self.model.validate()?;
        validate_working_directory(&self.cwd)?;
        self.settings.validate()?;
        self.delegation.validate(self.depth)?;
        self.bounds.validate()
    }
}

/// Stable identity and hierarchy metadata for an instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceIdentity {
    /// Opaque local instance identifier.
    pub id: InstanceId,
    /// Optional opaque parent instance identifier.
    pub parent_id: Option<InstanceId>,
    /// Current depth in the delegation tree.
    pub depth: u16,
}

/// Observable lifecycle state of a supervised child process.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceLifecycle {
    /// The process is being created and its protocol is being negotiated.
    Starting,
    /// The child is ready to accept work.
    Ready,
    /// The child is executing work.
    Running,
    /// The child is waiting for an explicit permission decision.
    WaitingPermission,
    /// The supervisor has requested orderly shutdown.
    Stopping,
    /// The child completed normally.
    Completed,
    /// The child ended with a typed failure.
    Failed,
    /// The child was cancelled cooperatively or by the supervisor.
    Cancelled,
}

impl InstanceLifecycle {
    /// Stable lifecycle label for logs and serialized status surfaces.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::WaitingPermission => "waiting_permission",
            Self::Stopping => "stopping",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Return whether this state is terminal.
    pub const fn is_settled(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Apply one valid state-machine transition.
    pub fn transition(self, next: Self) -> Result<Self, InstanceContractError> {
        let allowed = matches!(
            (self, next),
            (
                Self::Starting,
                Self::Ready | Self::Stopping | Self::Failed | Self::Cancelled
            ) | (
                Self::Ready,
                Self::Running | Self::Stopping | Self::Failed | Self::Cancelled
            ) | (
                Self::Running,
                Self::WaitingPermission | Self::Stopping | Self::Completed | Self::Failed | Self::Cancelled
            ) | (
                Self::WaitingPermission,
                Self::Running | Self::Stopping | Self::Failed | Self::Cancelled
            ) | (Self::Stopping, Self::Failed | Self::Cancelled)
        );
        if allowed {
            Ok(next)
        } else {
            Err(InstanceContractError::InvalidLifecycleTransition { from: self.label(), to: next.label() })
        }
    }
}

/// Provider-neutral source of account-capacity information.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityProvider {
    /// Capacity reported for a ChatGPT Codex account.
    ChatGptCodex,
    /// Capacity reported for an OpenCode Zen account.
    OpenCodeZen,
    /// Capacity reported for an OpenCode Go account.
    OpenCodeGo,
    /// Capacity reported by a configured ACP agent.
    ConfiguredAcp {
        /// Application-owned configured ACP agent name.
        agent: String,
    },
}

impl CapacityProvider {
    /// Validate configured ACP names.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        if let Self::ConfiguredAcp { agent } = self {
            let _ = validate_text("ACP agent", agent.clone(), 128)?;
        }
        Ok(())
    }
}

/// Freshness and provenance of one account-capacity field.
///
/// This deliberately has no estimated variant: account capacity is never
/// inferred from per-request token accounting.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provenance", rename_all = "snake_case")]
pub enum CapacityField<T> {
    /// Value returned by the provider at the snapshot observation time.
    ProviderReported {
        /// Provider-supplied field value.
        value: T,
    },
    /// Last provider-reported value is older than the caller's freshness limit.
    Stale {
        /// Previously provider-supplied field value.
        value: T,
        /// Unix time in milliseconds when the value was observed.
        observed_at_unix_ms: u64,
    },
    /// The provider did not expose this field or it could not be fetched safely.
    Unavailable,
}

impl<T> CapacityField<T> {
    /// Return whether a provider value is currently fresh.
    pub const fn is_fresh(&self) -> bool {
        matches!(self, Self::ProviderReported { .. })
    }

    /// Return the known value, if the field was available.
    pub const fn value(&self) -> Option<&T> {
        match self {
            Self::ProviderReported { value } | Self::Stale { value, .. } => Some(value),
            Self::Unavailable => None,
        }
    }
}

/// One named rate-limit or allowance window in an account snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountCapacityWindow {
    /// Stable provider-neutral window name, such as `primary` or `monthly`.
    pub name: String,
    /// Percentage used when the provider exposes it.
    pub used_percent: Option<CapacityField<u8>>,
    /// Remaining allowance when the provider exposes a count.
    pub remaining: Option<CapacityField<u64>>,
    /// Window reset time when the provider exposes it.
    pub reset_at_unix_ms: Option<CapacityField<u64>>,
}

impl AccountCapacityWindow {
    /// Validate names and percentage values.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        let _ = validate_text("capacity window name", self.name.clone(), 64)?;
        if let Some(field) = &self.used_percent {
            if field.value().is_some_and(|value| *value > 100) {
                return Err(InstanceContractError::InvalidPercentage);
            }
        }
        Ok(())
    }
}

/// Provider-neutral account-capacity snapshot.
///
/// It contains only provider-reported, stale, or unavailable capacity fields.
/// Per-request token consumption belongs to [`crate::ProviderUsage`] and is not
/// interchangeable with this account-level snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountCapacitySnapshot {
    /// Provider/account boundary that supplied the snapshot.
    pub provider: CapacityProvider,
    /// Unix time in milliseconds when this snapshot was assembled.
    pub observed_at_unix_ms: u64,
    /// Named rate-limit or allowance windows, ordered by name.
    pub windows: Vec<AccountCapacityWindow>,
    /// Provider-supplied credit or monetary balance, when applicable.
    pub credit_balance: Option<CapacityField<u64>>,
    /// Provider-supplied plan label, when applicable.
    pub plan: Option<CapacityField<String>>,
    /// Provider-supplied account limit state, when applicable.
    pub limit_state: Option<CapacityField<String>>,
}

impl AccountCapacitySnapshot {
    /// Build a snapshot with deterministic window ordering.
    pub fn new(
        provider: CapacityProvider, observed_at_unix_ms: u64, mut windows: Vec<AccountCapacityWindow>,
    ) -> Result<Self, InstanceContractError> {
        windows.sort_by(|left, right| left.name.cmp(&right.name));
        let snapshot =
            Self { provider, observed_at_unix_ms, windows, credit_balance: None, plan: None, limit_state: None };
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Validate provider provenance and window contents.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        self.provider.validate()?;
        let mut previous = None;
        for window in &self.windows {
            window.validate()?;
            if previous.is_some_and(|name: &str| name >= window.name.as_str()) {
                return Err(InstanceContractError::DuplicateCapacityWindow);
            }
            previous = Some(window.name.as_str());
        }
        Ok(())
    }
}

/// Semantic verification evidence included in a settled result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SemanticEvidence {
    /// Short semantic classification, such as `test` or `diff`.
    pub kind: String,
    /// Redacted semantic detail, never a raw transcript or credential.
    pub detail: String,
}

impl SemanticEvidence {
    /// Build redacted semantic evidence.
    pub fn new(kind: impl Into<String>, detail: impl Into<String>) -> Result<Self, InstanceContractError> {
        let detail = detail.into();
        Ok(Self {
            kind: validate_text("evidence kind", kind.into(), 64)?,
            detail: redact_and_bound("evidence detail", &detail, 4_096)?,
        })
    }

    /// Validate canonically redacted semantic evidence.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        let _ = validate_text("evidence kind", self.kind.clone(), 64)?;
        validate_canonical_redaction("evidence detail", &self.detail, 4_096)
    }
}

/// Changed workspace path metadata retained without file contents.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChangedPath(PathBuf);

impl ChangedPath {
    /// Build a contained relative path for result metadata.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, InstanceContractError> {
        let path = path.into();
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(InstanceContractError::PathTraversal { field: "changed path" });
        }
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(InstanceContractError::PathTraversal { field: "changed path" });
        }
        Ok(Self(path))
    }

    /// Return the contained relative path.
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Terminal outcome recorded by a settled instance result.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceOutcome {
    /// The child completed normally.
    Completed,
    /// The child failed after starting.
    Failed,
    /// The child was cancelled.
    Cancelled,
}

/// Result retained after an instance reaches a terminal lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SettledInstanceResult {
    /// Terminal outcome for the child.
    pub outcome: InstanceOutcome,
    /// Redacted semantic summary, never the transcript.
    pub summary: String,
    /// Optional application-owned durable session handle.
    pub session: Option<SessionHandle>,
    /// Optional application-owned change record handle.
    pub changes: Option<ChangeHandle>,
    /// Changed path metadata, ordered and deduplicated.
    pub changed_paths: Vec<ChangedPath>,
    /// Semantic verification evidence, ordered by kind and detail.
    pub verification: Vec<SemanticEvidence>,
    /// Redacted failure diagnostics when the outcome is failed.
    pub diagnostics: Vec<SemanticEvidence>,
}

impl SettledInstanceResult {
    /// Build a result with deterministic metadata ordering.
    pub fn new(
        outcome: InstanceOutcome, summary: impl Into<String>, session: Option<SessionHandle>,
        changes: Option<ChangeHandle>, mut changed_paths: Vec<ChangedPath>, mut verification: Vec<SemanticEvidence>,
        mut diagnostics: Vec<SemanticEvidence>,
    ) -> Result<Self, InstanceContractError> {
        let summary = summary.into();
        changed_paths.sort();
        changed_paths.dedup();
        verification.sort_by(|left, right| (&left.kind, &left.detail).cmp(&(&right.kind, &right.detail)));
        diagnostics.sort_by(|left, right| (&left.kind, &left.detail).cmp(&(&right.kind, &right.detail)));
        let result = Self {
            outcome,
            summary: redact_and_bound("summary", &summary, MAX_INSTANCE_SUMMARY_BYTES)?,
            session,
            changes,
            changed_paths,
            verification,
            diagnostics,
        };
        result.validate()?;
        Ok(result)
    }

    /// Validate result caps after deserialization or construction.
    pub fn validate(&self) -> Result<(), InstanceContractError> {
        validate_canonical_redaction("summary", &self.summary, MAX_INSTANCE_SUMMARY_BYTES)?;
        validate_entries("changed paths", self.changed_paths.len(), MAX_INSTANCE_CHANGED_PATHS)?;
        validate_entries("verification evidence", self.verification.len(), MAX_INSTANCE_EVIDENCE)?;
        validate_entries("diagnostics", self.diagnostics.len(), MAX_INSTANCE_EVIDENCE)?;
        for evidence in self.verification.iter().chain(&self.diagnostics) {
            evidence.validate()?;
        }
        Ok(())
    }
}

/// Status projection for an active or settled instance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstanceStatus {
    /// Identity and hierarchy metadata.
    pub identity: InstanceIdentity,
    /// Current process-driven lifecycle state.
    pub lifecycle: InstanceLifecycle,
    /// Most recently observed account capacity, when a safe boundary exposed it.
    pub capacity: Option<AccountCapacitySnapshot>,
}

impl InstanceStatus {
    /// Build a status projection with no capacity observation.
    pub fn new(identity: InstanceIdentity, lifecycle: InstanceLifecycle) -> Self {
        Self { identity, lifecycle, capacity: None }
    }
}

fn validate_working_directory(cwd: &Path) -> Result<(), InstanceContractError> {
    if !cwd.is_absolute() {
        return Err(InstanceContractError::RelativeWorkingDirectory);
    }
    if cwd
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(InstanceContractError::PathTraversal { field: "working directory" });
    }
    Ok(())
}

fn validate_text(field: &'static str, value: String, max_bytes: usize) -> Result<String, InstanceContractError> {
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(InstanceContractError::InvalidText { field, max_bytes });
    }
    Ok(value)
}

fn validate_bound(field: &'static str, value: u64, minimum: u64, maximum: u64) -> Result<(), InstanceContractError> {
    if !(minimum..=maximum).contains(&value) {
        return Err(InstanceContractError::InvalidBound { field, minimum, maximum });
    }
    Ok(())
}

fn validate_entries(field: &'static str, count: usize, maximum: usize) -> Result<(), InstanceContractError> {
    if count > maximum {
        return Err(InstanceContractError::TooManyEntries { field, maximum });
    }
    Ok(())
}

fn redact_and_bound(field: &'static str, value: &str, max_bytes: usize) -> Result<String, InstanceContractError> {
    let value = redact_secret_like_text(value);
    if value.len() > max_bytes {
        return Err(InstanceContractError::InvalidText { field, max_bytes });
    }
    Ok(value)
}

fn validate_canonical_redaction(
    field: &'static str, value: &str, max_bytes: usize,
) -> Result<(), InstanceContractError> {
    let redacted = redact_and_bound(field, value, max_bytes)?;
    if redacted != value {
        return Err(InstanceContractError::NonCanonicalRedaction { field });
    }
    Ok(())
}

fn redact_secret_like_text(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let separator = ["authorization", "api_key", "api-key", "token", "password", "secret"]
                .iter()
                .filter_map(|name| lower.find(name).map(|position| position + name.len()))
                .filter_map(|position| {
                    line[position..]
                        .find([':', '='])
                        .map(|separator| position + separator + 1)
                })
                .min();
            let bearer = lower.find("bearer ").map(|position| position + "bearer ".len());
            match separator.into_iter().chain(bearer).min() {
                Some(position) => format!("{}[REDACTED]", &line[..position]),
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specification() -> InstanceSpecification {
        InstanceSpecification {
            model: InstanceModel::ConfiguredAcp { agent: "local-reviewer".to_string(), model: "review-v2".to_string() },
            cwd: PathBuf::from("/workspace/project"),
            session: InstanceSessionPolicy::DurableNew,
            settings: InstanceSettings {
                reasoning: ReasoningSetting::Medium,
                search: SearchSetting::Enabled { max_results: 10 },
            },
            authority: InstanceAuthority::ReadOnly,
            parent_id: Some(InstanceId::new("parent-1").unwrap()),
            depth: 1,
            delegation: DelegationBudget { maximum_depth: 2, maximum_concurrency: 4 },
            bounds: InstanceBounds {
                runtime_ms: 60_000,
                tool_calls: 20,
                retained_events: 100,
                output_bytes: 16_384,
                summary_bytes: 4_096,
            },
        }
    }

    #[test]
    fn specification_requires_explicit_finite_contained_values() {
        specification().validate().unwrap();

        let mut invalid = specification();
        invalid.cwd = PathBuf::from("/workspace/project/../other");
        assert_eq!(
            invalid.validate(),
            Err(InstanceContractError::PathTraversal { field: "working directory" })
        );

        let mut invalid = specification();
        invalid.bounds.tool_calls = 0;
        assert!(matches!(
            invalid.validate(),
            Err(InstanceContractError::InvalidBound { .. })
        ));

        let mut invalid = specification();
        invalid.depth = 3;
        assert!(matches!(
            invalid.validate(),
            Err(InstanceContractError::DelegationDepthExceeded { .. })
        ));

        assert!(matches!(
            WriteApproval::new(" "),
            Err(InstanceContractError::InvalidText { field: "write approval", .. })
        ));
    }

    #[test]
    fn lifecycle_accepts_only_process_driven_transitions() {
        let lifecycle = InstanceLifecycle::Starting
            .transition(InstanceLifecycle::Ready)
            .unwrap()
            .transition(InstanceLifecycle::Running)
            .unwrap()
            .transition(InstanceLifecycle::WaitingPermission)
            .unwrap()
            .transition(InstanceLifecycle::Running)
            .unwrap()
            .transition(InstanceLifecycle::Completed)
            .unwrap();
        assert!(lifecycle.is_settled());
        assert!(matches!(
            InstanceLifecycle::Ready.transition(InstanceLifecycle::Completed),
            Err(InstanceContractError::InvalidLifecycleTransition { .. })
        ));
        assert!(matches!(
            InstanceLifecycle::Completed.transition(InstanceLifecycle::Running),
            Err(InstanceContractError::InvalidLifecycleTransition { .. })
        ));
    }

    #[test]
    fn model_contract_serializes_all_supported_provider_selections() {
        let models = vec![
            InstanceModel::ChatGptCodex { model: "gpt-5.6-terra".to_string() },
            InstanceModel::OpenCodeZen { model: "zen-pro".to_string() },
            InstanceModel::OpenCodeGo { model: "go".to_string() },
            specification().model,
        ];
        let json = serde_json::to_string(&models).unwrap();
        assert!(json.contains("chat_gpt_codex"));
        assert!(json.contains("open_code_zen"));
        assert!(json.contains("open_code_go"));
        assert!(json.contains("configured_acp"));
        assert_eq!(serde_json::from_str::<Vec<InstanceModel>>(&json).unwrap(), models);
    }

    #[test]
    fn reasoning_setting_serializes_all_provider_neutral_choices() {
        let settings = [
            (ReasoningSetting::Auto, "auto"),
            (ReasoningSetting::On, "on"),
            (ReasoningSetting::None, "none"),
            (ReasoningSetting::Minimal, "minimal"),
            (ReasoningSetting::Low, "low"),
            (ReasoningSetting::Medium, "medium"),
            (ReasoningSetting::High, "high"),
            (ReasoningSetting::Xhigh, "xhigh"),
            (ReasoningSetting::Max, "max"),
        ];

        for (setting, label) in settings {
            let serialized = serde_json::to_string(&setting).unwrap();
            assert_eq!(serialized, format!("\"{label}\""));
            assert_eq!(serde_json::from_str::<ReasoningSetting>(&serialized).unwrap(), setting);
        }
    }

    #[test]
    fn capacity_keeps_provenance_freshness_and_deterministic_order() {
        let snapshot = AccountCapacitySnapshot::new(
            CapacityProvider::ChatGptCodex,
            1_000,
            vec![
                AccountCapacityWindow {
                    name: "secondary".to_string(),
                    used_percent: Some(CapacityField::Stale { value: 80, observed_at_unix_ms: 900 }),
                    remaining: None,
                    reset_at_unix_ms: None,
                },
                AccountCapacityWindow {
                    name: "primary".to_string(),
                    used_percent: Some(CapacityField::ProviderReported { value: 20 }),
                    remaining: Some(CapacityField::Unavailable),
                    reset_at_unix_ms: None,
                },
            ],
        )
        .unwrap();

        assert_eq!(snapshot.windows[0].name, "primary");
        assert!(snapshot.windows[0].used_percent.as_ref().unwrap().is_fresh());
        assert!(!snapshot.windows[1].used_percent.as_ref().unwrap().is_fresh());
        assert_eq!(snapshot.windows[0].remaining.as_ref().unwrap().value(), None);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(json.contains("provider_reported"));
        assert!(json.contains("stale"));
        assert!(json.contains("unavailable"));
        assert!(!json.contains("input_tokens"));
    }

    #[test]
    fn deserialized_results_reject_unredacted_secret_like_text() {
        let evidence: SemanticEvidence =
            serde_json::from_str(r#"{"kind":"test","detail":"token=secret-value"}"#).unwrap();
        assert_eq!(
            evidence.validate(),
            Err(InstanceContractError::NonCanonicalRedaction { field: "evidence detail" })
        );

        let result: SettledInstanceResult = serde_json::from_str(
            r#"{
                "outcome":"failed",
                "summary":"safe",
                "session":null,
                "changes":null,
                "changed_paths":[],
                "verification":[{"kind":"test","detail":"authorization: Bearer secret-value"}],
                "diagnostics":[]
            }"#,
        )
        .unwrap();
        assert_eq!(
            result.validate(),
            Err(InstanceContractError::NonCanonicalRedaction { field: "evidence detail" })
        );

        let result: SettledInstanceResult = serde_json::from_str(
            r#"{
                "outcome":"failed",
                "summary":"token=secret-value",
                "session":null,
                "changes":null,
                "changed_paths":[],
                "verification":[],
                "diagnostics":[]
            }"#,
        )
        .unwrap();
        assert_eq!(
            result.validate(),
            Err(InstanceContractError::NonCanonicalRedaction { field: "summary" })
        );
    }

    #[test]
    fn settled_results_sort_cap_and_redact_retained_evidence() {
        let result = SettledInstanceResult::new(
            InstanceOutcome::Failed,
            "request failed: token=super-secret".to_string(),
            Some(SessionHandle::new("session-7").unwrap()),
            Some(ChangeHandle::new("change-7").unwrap()),
            vec![
                ChangedPath::new("src/z.rs").unwrap(),
                ChangedPath::new("src/a.rs").unwrap(),
            ],
            vec![SemanticEvidence::new("test", "authorization: Bearer secret-value").unwrap()],
            Vec::new(),
        )
        .unwrap();

        assert_eq!(result.changed_paths[0].as_path(), Path::new("src/a.rs"));
        assert_eq!(result.summary, "request failed: token=[REDACTED]");
        assert_eq!(result.verification[0].detail, "authorization:[REDACTED]");
        assert!(!serde_json::to_string(&result).unwrap().contains("secret-value"));

        let evidence = SemanticEvidence::new("test", "x".repeat(4_097));
        assert!(matches!(evidence, Err(InstanceContractError::InvalidText { .. })));
        assert!(matches!(
            ChangedPath::new("../outside"),
            Err(InstanceContractError::PathTraversal { field: "changed path" })
        ));
    }
}
