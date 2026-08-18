//! Explicit context lifecycle, relation, and protection policy.
//!
//! Lifecycle state answers what happened to an evidence item. Request
//! visibility answers whether the item is rendered for one provider request.
//! The two values are intentionally independent: archiving an item does not
//! erase its audit trail, and omitting an item from one request does not mark
//! it as superseded.

use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, ContextLifecycleError>;

/// Auditable state of a context item across request boundaries.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextLifecycleState {
    /// The item remains an independent source of context.
    #[default]
    Active,
    /// The item has the same evidence as the canonical item named by a
    /// [`ContextRelationKind::DuplicateOf`] relation.
    Duplicate,
    /// A newer source version replaces this item.
    Superseded,
    /// An approved summary stands in for this item's detailed projection.
    Summarized,
    /// The item is outside the active lifecycle but remains auditable and,
    /// where available, recoverable.
    Archived,
}

impl ContextLifecycleState {
    /// Stable label used in inspection, exports, and session metadata.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Duplicate => "duplicate",
            Self::Superseded => "superseded",
            Self::Summarized => "summarized",
            Self::Archived => "archived",
        }
    }
}

/// Why a context item must be kept conservatively protected.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextProtectionReason {
    /// The current user request is still the task's source of truth.
    CurrentUserContext,
    /// The user or project supplied an explicit constraint.
    ExplicitConstraint,
    /// A system or application safety instruction is in force.
    SafetyState,
    /// A permission request is waiting for an explicit user decision.
    PendingPermission,
    /// The user explicitly pinned this item.
    UserPin,
    /// Evidence or recovery metadata is still available for this item.
    RecoveryMetadata,
    /// A failed operation or diagnostic must remain available for review.
    FailureEvidence,
    /// A write or edit has not received an explicit verification/release.
    UnverifiedWriteEdit,
}

impl ContextProtectionReason {
    /// Stable label used in inspection and exports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::CurrentUserContext => "current_user_context",
            Self::ExplicitConstraint => "explicit_constraint",
            Self::SafetyState => "safety_state",
            Self::PendingPermission => "pending_permission",
            Self::UserPin => "user_pin",
            Self::RecoveryMetadata => "recovery_metadata",
            Self::FailureEvidence => "failure_evidence",
            Self::UnverifiedWriteEdit => "unverified_write_edit",
        }
    }
}

/// Kind of explicit context relation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRelationKind {
    /// The source is identical to the canonical target.
    DuplicateOf,
    /// A newer source version replaces the source.
    SupersededBy,
    /// An approved summary stands in for the source.
    SummarizedBy,
    /// The target is a user-approved verification result for the source.
    VerifiedBy,
    /// The source was archived as the target or under the target handle.
    ArchivedAs,
    /// The source was recovered from the target handle or source item.
    RecoveredFrom,
}

impl ContextRelationKind {
    /// Stable label used in inspection, exports, and session metadata.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DuplicateOf => "duplicate_of",
            Self::SupersededBy => "superseded_by",
            Self::SummarizedBy => "summarized_by",
            Self::VerifiedBy => "verified_by",
            Self::ArchivedAs => "archived_as",
            Self::RecoveredFrom => "recovered_from",
        }
    }
}

/// State of an explicit relation, including verification review state.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRelationStatus {
    /// An agent proposed the relation; it has not changed lifecycle or
    /// protection state.
    Proposed,
    /// A user approved the relation or the application recorded it as
    /// established.
    Approved,
    /// A non-review relation was applied, or a verification was released.
    Applied,
    /// A user rejected the proposal.
    Rejected,
    /// A user approved the verification and explicitly released protection.
    Released,
}

impl ContextRelationStatus {
    /// Stable label used in inspection and exports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::Released => "released",
        }
    }
}

/// Pure lifecycle actions accepted by [`ContextLifecycle::apply`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContextLifecycleAction {
    /// Mark the source as an exact duplicate of the target.
    Duplicate {
        /// Applied duplicate relation.
        relation: ContextRelation,
    },
    /// Mark the source as replaced by a newer target.
    Supersede {
        /// Applied supersession relation.
        relation: ContextRelation,
    },
    /// Mark the source as represented by an approved summary target.
    Summarize {
        /// Applied summary relation.
        relation: ContextRelation,
    },
    /// Move the source to the archived lifecycle state.
    Archive {
        /// Optional archive relation naming the destination or handle.
        relation: Option<ContextRelation>,
    },
    /// Restore an archived source and record where the recovery came from.
    Recover {
        /// Applied recovery relation naming the source or artifact handle.
        relation: ContextRelation,
    },
    /// Propose a verification relation. This never changes protection.
    ProposeVerification {
        /// Proposed evidence-to-candidate relation.
        relation: ContextRelation,
    },
    /// Approve a previously proposed verification relation.
    ApproveVerification {
        /// Relation under review.
        relation_id: String,
    },
    /// Reject a previously proposed verification relation.
    RejectVerification {
        /// Relation under review.
        relation_id: String,
    },
    /// Release protection through an approved verification relation, or by an
    /// explicit direct release when no relation id is supplied.
    Release {
        /// Approved verification relation, or `None` for an explicitly
        /// direct release.
        relation_id: Option<String>,
    },
}

/// Error returned when an explicit lifecycle transition is invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ContextLifecycleError {
    /// A relation id was already present on the item.
    #[error("context relation `{0}` already exists")]
    DuplicateRelation(String),
    /// A relation id was not present on the item.
    #[error("unknown context relation `{0}`")]
    UnknownRelation(String),
    /// A relation used an unexpected kind for the requested transition.
    #[error("context relation kind must be `{expected:?}`, got `{actual:?}`")]
    WrongRelationKind {
        /// Required relation kind.
        expected: ContextRelationKind,
        /// Supplied relation kind.
        actual: ContextRelationKind,
    },
    /// A relation was proposed or reviewed from an invalid status.
    #[error("context relation `{relation_id}` must be `{expected:?}`, got `{actual:?}`")]
    InvalidVerificationStatus {
        /// Relation being reviewed.
        relation_id: String,
        /// Required current status.
        expected: ContextRelationStatus,
        /// Actual current status.
        actual: ContextRelationStatus,
    },
    /// A relation omitted a required id.
    #[error("context relation ids must be non-empty")]
    EmptyRelationId,
    /// Verification was proposed for an item that has no active protection.
    #[error("verification evidence must remain protected until review and release")]
    UnprotectedEvidence,
    /// Direct or relation-backed release was requested after protection was
    /// already explicitly released.
    #[error("context protection is already released")]
    AlreadyReleased,
}

/// Conservative protection reasons attached to one context item.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextProtection {
    /// Sorted, deduplicated reasons. An empty set means the item is explicitly
    /// unprotected; it does not mean that protection was inferred elsewhere.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub reasons: BTreeSet<ContextProtectionReason>,
}

impl ContextProtection {
    /// Build protection from one reason.
    pub fn from_reason(reason: ContextProtectionReason) -> Self {
        Self::from_reasons([reason])
    }

    /// Build sorted, deduplicated protection from an iterator of reasons.
    pub fn from_reasons<I>(reasons: I) -> Self
    where
        I: IntoIterator<Item = ContextProtectionReason>,
    {
        Self { reasons: reasons.into_iter().collect() }
    }

    /// Add a reason while preserving deterministic ordering.
    pub fn with_reason(mut self, reason: ContextProtectionReason) -> Self {
        self.reasons.insert(reason);
        self
    }

    /// Whether at least one conservative protection reason remains.
    pub fn is_protected(&self) -> bool {
        !self.reasons.is_empty()
    }

    /// Whether this protection includes a specific reason.
    pub fn contains(&self, reason: ContextProtectionReason) -> bool {
        self.reasons.contains(&reason)
    }

    /// Return stable labels for human-facing surfaces.
    pub fn labels(&self) -> Vec<&'static str> {
        self.reasons.iter().map(|reason| reason.label()).collect()
    }
}

/// Explicit relation between two context ids or recovery handles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextRelation {
    /// Stable relation id, suitable for review commands.
    pub id: String,
    /// Meaning of the source-to-target edge.
    pub kind: ContextRelationKind,
    /// Context item that owns the relation.
    pub source_id: String,
    /// Canonical item, replacement, summary, candidate result, or recovery
    /// handle named by the relation.
    pub target_id: String,
    /// Review/application state of the relation.
    pub status: ContextRelationStatus,
}

impl ContextRelation {
    /// Build an already-applied non-verification relation.
    pub fn applied(
        id: impl Into<String>, kind: ContextRelationKind, source_id: impl Into<String>, target_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            source_id: source_id.into(),
            target_id: target_id.into(),
            status: ContextRelationStatus::Applied,
        }
    }

    /// Build a proposed verification relation.
    pub fn proposed_verification(
        id: impl Into<String>, evidence_id: impl Into<String>, candidate_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: ContextRelationKind::VerifiedBy,
            source_id: evidence_id.into(),
            target_id: candidate_id.into(),
            status: ContextRelationStatus::Proposed,
        }
    }

    /// Whether this relation names a verification candidate.
    pub fn is_verification(&self) -> bool {
        self.kind == ContextRelationKind::VerifiedBy
    }

    fn validate_state_relation(&self, expected_kind: ContextRelationKind) -> Result<()> {
        if self.kind != expected_kind {
            return Err(ContextLifecycleError::WrongRelationKind { expected: expected_kind, actual: self.kind });
        }
        if self.status != ContextRelationStatus::Applied {
            return Err(ContextLifecycleError::InvalidVerificationStatus {
                relation_id: self.id.clone(),
                expected: ContextRelationStatus::Applied,
                actual: self.status,
            });
        }
        self.validate_relation_fields()
    }

    fn validate_relation_fields(&self) -> Result<()> {
        if self.id.trim().is_empty() || self.source_id.trim().is_empty() || self.target_id.trim().is_empty() {
            return Err(ContextLifecycleError::EmptyRelationId);
        }
        Ok(())
    }
}

/// Lifecycle and protection state carried by a context item.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContextLifecycle {
    /// Lifecycle state, independent of request visibility.
    pub state: ContextLifecycleState,
    /// Conservative protection reasons.
    #[serde(default)]
    pub protection: ContextProtection,
    /// Whether an explicit release allows future selection passes to leave
    /// this item unprotected until it is recovered again.
    #[serde(default, skip_serializing_if = "is_false")]
    pub protection_released: bool,
    /// Explicit, append-only-friendly relations involving this item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<ContextRelation>,
}

impl ContextLifecycle {
    /// Create active lifecycle state with the supplied protection.
    pub fn new(protection: ContextProtection) -> Self {
        Self { state: ContextLifecycleState::Active, protection, protection_released: false, relations: Vec::new() }
    }

    /// Whether protection remains for this item.
    pub fn is_protected(&self) -> bool {
        self.protection.is_protected()
    }

    /// Find a relation by its stable id.
    pub fn relation(&self, relation_id: &str) -> Option<&ContextRelation> {
        self.relations.iter().find(|relation| relation.id == relation_id)
    }

    /// Return all verification relations in insertion order.
    pub fn verification_relations(&self) -> impl Iterator<Item = &ContextRelation> {
        self.relations.iter().filter(|relation| relation.is_verification())
    }

    /// Apply one explicit lifecycle action without side effects.
    pub fn apply(&self, action: ContextLifecycleAction) -> Result<Self> {
        let mut next = self.clone();
        match action {
            ContextLifecycleAction::Duplicate { relation } => {
                relation.validate_state_relation(ContextRelationKind::DuplicateOf)?;
                next.insert_relation(relation)?;
                next.state = ContextLifecycleState::Duplicate;
            }
            ContextLifecycleAction::Supersede { relation } => {
                relation.validate_state_relation(ContextRelationKind::SupersededBy)?;
                next.insert_relation(relation)?;
                next.state = ContextLifecycleState::Superseded;
            }
            ContextLifecycleAction::Summarize { relation } => {
                relation.validate_state_relation(ContextRelationKind::SummarizedBy)?;
                next.insert_relation(relation)?;
                next.state = ContextLifecycleState::Summarized;
            }
            ContextLifecycleAction::Archive { relation } => {
                if let Some(relation) = relation {
                    relation.validate_state_relation(ContextRelationKind::ArchivedAs)?;
                    next.insert_relation(relation)?;
                }
                next.state = ContextLifecycleState::Archived;
            }
            ContextLifecycleAction::Recover { relation } => {
                relation.validate_state_relation(ContextRelationKind::RecoveredFrom)?;
                let already_applied = next
                    .relation(&relation.id)
                    .is_some_and(|existing| existing == &relation);
                if !already_applied {
                    next.insert_relation(relation)?;
                }
                next.state = ContextLifecycleState::Active;
                next.protection
                    .reasons
                    .insert(ContextProtectionReason::RecoveryMetadata);
                next.protection_released = false;
            }
            ContextLifecycleAction::ProposeVerification { relation } => {
                if !next.is_protected() {
                    return Err(ContextLifecycleError::UnprotectedEvidence);
                }
                if !relation.is_verification() {
                    return Err(ContextLifecycleError::WrongRelationKind {
                        expected: ContextRelationKind::VerifiedBy,
                        actual: relation.kind,
                    });
                }
                if relation.status != ContextRelationStatus::Proposed {
                    return Err(ContextLifecycleError::InvalidVerificationStatus {
                        relation_id: relation.id,
                        expected: ContextRelationStatus::Proposed,
                        actual: relation.status,
                    });
                }
                relation.validate_relation_fields()?;
                next.insert_relation(relation)?;
            }
            ContextLifecycleAction::ApproveVerification { relation_id } => {
                let relation = next.relation_mut(&relation_id)?;
                if !relation.is_verification() {
                    return Err(ContextLifecycleError::WrongRelationKind {
                        expected: ContextRelationKind::VerifiedBy,
                        actual: relation.kind,
                    });
                }
                if relation.status != ContextRelationStatus::Proposed {
                    return Err(ContextLifecycleError::InvalidVerificationStatus {
                        relation_id,
                        expected: ContextRelationStatus::Proposed,
                        actual: relation.status,
                    });
                }
                relation.status = ContextRelationStatus::Approved;
            }
            ContextLifecycleAction::RejectVerification { relation_id } => {
                let relation = next.relation_mut(&relation_id)?;
                if !relation.is_verification() {
                    return Err(ContextLifecycleError::WrongRelationKind {
                        expected: ContextRelationKind::VerifiedBy,
                        actual: relation.kind,
                    });
                }
                if relation.status != ContextRelationStatus::Proposed {
                    return Err(ContextLifecycleError::InvalidVerificationStatus {
                        relation_id,
                        expected: ContextRelationStatus::Proposed,
                        actual: relation.status,
                    });
                }
                relation.status = ContextRelationStatus::Rejected;
            }
            ContextLifecycleAction::Release { relation_id } => {
                if let Some(relation_id) = relation_id {
                    let relation = next.relation_mut(&relation_id)?;
                    if !relation.is_verification() {
                        return Err(ContextLifecycleError::WrongRelationKind {
                            expected: ContextRelationKind::VerifiedBy,
                            actual: relation.kind,
                        });
                    }
                    if relation.status != ContextRelationStatus::Approved {
                        return Err(ContextLifecycleError::InvalidVerificationStatus {
                            relation_id,
                            expected: ContextRelationStatus::Approved,
                            actual: relation.status,
                        });
                    }
                    relation.status = ContextRelationStatus::Released;
                }
                if !next.is_protected() {
                    return Err(ContextLifecycleError::AlreadyReleased);
                }
                next.protection = ContextProtection::default();
                next.protection_released = true;
            }
        }
        Ok(next)
    }

    /// Propose a verification relation for this item.
    pub fn propose_verification(
        &self, relation_id: impl Into<String>, evidence_id: impl Into<String>, candidate_id: impl Into<String>,
    ) -> Result<Self> {
        self.apply(ContextLifecycleAction::ProposeVerification {
            relation: ContextRelation::proposed_verification(relation_id, evidence_id, candidate_id),
        })
    }

    /// Approve a proposed verification relation without releasing protection.
    pub fn approve_verification(&self, relation_id: impl Into<String>) -> Result<Self> {
        self.apply(ContextLifecycleAction::ApproveVerification { relation_id: relation_id.into() })
    }

    /// Reject a proposed verification relation without changing protection.
    pub fn reject_verification(&self, relation_id: impl Into<String>) -> Result<Self> {
        self.apply(ContextLifecycleAction::RejectVerification { relation_id: relation_id.into() })
    }

    /// Release protection through an approved verification relation.
    pub fn release_verification(&self, relation_id: impl Into<String>) -> Result<Self> {
        self.apply(ContextLifecycleAction::Release { relation_id: Some(relation_id.into()) })
    }

    /// Explicitly release protection without a verification relation.
    pub fn release(&self) -> Result<Self> {
        self.apply(ContextLifecycleAction::Release { relation_id: None })
    }

    /// Merge protection discovered by a fresh selection pass.
    ///
    /// Selection can learn that a stable item id now represents a failed tool,
    /// an unverified write, or newly recoverable evidence. An explicit release
    /// remains authoritative until a recovery action reopens classification.
    pub fn merge_derived_protection(&mut self, protection: &ContextProtection) {
        if !self.protection_released {
            self.protection.reasons.extend(protection.reasons.iter().copied());
        }
    }

    fn insert_relation(&mut self, relation: ContextRelation) -> Result<()> {
        if self.relations.iter().any(|existing| existing.id == relation.id) {
            return Err(ContextLifecycleError::DuplicateRelation(relation.id));
        }
        self.relations.push(relation);
        Ok(())
    }

    fn relation_mut(&mut self, relation_id: &str) -> Result<&mut ContextRelation> {
        self.relations
            .iter_mut()
            .find(|relation| relation.id == relation_id)
            .ok_or_else(|| ContextLifecycleError::UnknownRelation(relation_id.to_string()))
    }
}

/// Stable relation id for a source/candidate pair.
pub fn relation_id_for(kind: ContextRelationKind, source_id: &str, target_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    kind.label().hash(&mut hasher);
    source_id.hash(&mut hasher);
    target_id.hash(&mut hasher);
    format!("rel_{}_{:016x}", kind.label(), hasher.finish())
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVIDENCE: &str = "ctx_tool_archive_evidence";
    const CANDIDATE: &str = "ctx_transcript_candidate";

    fn protected() -> ContextLifecycle {
        ContextLifecycle::new(ContextProtection::from_reasons([
            ContextProtectionReason::FailureEvidence,
            ContextProtectionReason::UnverifiedWriteEdit,
        ]))
    }

    #[test]
    fn lifecycle_state_is_separate_from_protection_and_visibility() {
        let lifecycle = protected();
        assert_eq!(lifecycle.state, ContextLifecycleState::Active);
        assert!(lifecycle.is_protected());
    }

    #[test]
    fn proposal_names_evidence_and_candidate_without_releasing_protection() {
        let lifecycle = protected();
        let relation_id = relation_id_for(ContextRelationKind::VerifiedBy, EVIDENCE, CANDIDATE);
        let proposed = lifecycle
            .propose_verification(relation_id.clone(), EVIDENCE, CANDIDATE)
            .expect("propose verification");

        assert_eq!(proposed.protection, lifecycle.protection);
        let relation = proposed.relation(&relation_id).expect("relation");
        assert_eq!(relation.source_id, EVIDENCE);
        assert_eq!(relation.target_id, CANDIDATE);
        assert_eq!(relation.status, ContextRelationStatus::Proposed);
    }

    #[test]
    fn review_table_requires_approval_before_release() {
        let lifecycle = protected();
        let relation_id = relation_id_for(ContextRelationKind::VerifiedBy, EVIDENCE, CANDIDATE);
        let proposed = lifecycle
            .propose_verification(relation_id.clone(), EVIDENCE, CANDIDATE)
            .expect("propose verification");
        assert!(proposed.release_verification(&relation_id).is_err());

        let approved = proposed
            .approve_verification(&relation_id)
            .expect("approve verification");
        assert!(approved.is_protected(), "approval is not an implicit release");
        let released = approved
            .release_verification(&relation_id)
            .expect("release verification");
        assert!(!released.is_protected());
        assert_eq!(
            released.relation(&relation_id).expect("released relation").status,
            ContextRelationStatus::Released
        );
        assert!(released.protection_released);
    }

    #[test]
    fn rejected_proposal_cannot_be_approved_or_released() {
        let lifecycle = protected();
        let relation_id = relation_id_for(ContextRelationKind::VerifiedBy, EVIDENCE, CANDIDATE);
        let rejected = lifecycle
            .propose_verification(relation_id.clone(), EVIDENCE, CANDIDATE)
            .expect("propose verification")
            .reject_verification(&relation_id)
            .expect("reject verification");
        assert!(rejected.approve_verification(&relation_id).is_err());
        assert!(rejected.release_verification(&relation_id).is_err());
        assert!(rejected.is_protected());
    }

    #[test]
    fn unprotected_evidence_cannot_start_verification() {
        let lifecycle = ContextLifecycle::default();
        let relation_id = relation_id_for(ContextRelationKind::VerifiedBy, EVIDENCE, CANDIDATE);
        assert_eq!(
            lifecycle.propose_verification(relation_id, EVIDENCE, CANDIDATE),
            Err(ContextLifecycleError::UnprotectedEvidence)
        );
    }

    #[test]
    fn explicit_release_blocks_reclassification_until_recovery() {
        let lifecycle = protected().release().expect("explicit release");
        let mut still_released = lifecycle.clone();
        still_released.merge_derived_protection(&ContextProtection::from_reason(
            ContextProtectionReason::FailureEvidence,
        ));
        assert!(!still_released.is_protected());

        let recovered = lifecycle
            .apply(ContextLifecycleAction::Recover {
                relation: ContextRelation::applied(
                    "rel-reopen",
                    ContextRelationKind::RecoveredFrom,
                    EVIDENCE,
                    "artifact-handle",
                ),
            })
            .expect("recover released evidence");
        let mut reclassified = recovered;
        reclassified.merge_derived_protection(&ContextProtection::from_reason(
            ContextProtectionReason::RecoveryMetadata,
        ));
        assert!(reclassified.is_protected());
    }

    #[test]
    fn repeated_recovery_reopens_an_already_released_relation() {
        let relation = ContextRelation::applied(
            "rel-reopen",
            ContextRelationKind::RecoveredFrom,
            EVIDENCE,
            "artifact-handle",
        );
        let released = protected()
            .apply(ContextLifecycleAction::Recover { relation: relation.clone() })
            .expect("recover")
            .release()
            .expect("release");
        let reopened = released
            .apply(ContextLifecycleAction::Recover { relation })
            .expect("repeat recovery");

        assert!(!reopened.protection_released);
        assert!(reopened.protection.contains(ContextProtectionReason::RecoveryMetadata));
        assert_eq!(reopened.relations.len(), 1);
    }

    #[test]
    fn explicit_relations_update_lifecycle_without_touching_protection() {
        let lifecycle = protected();
        let relation = ContextRelation::applied(
            "rel-supersede",
            ContextRelationKind::SupersededBy,
            EVIDENCE,
            "ctx-newer",
        );
        let superseded = lifecycle
            .apply(ContextLifecycleAction::Supersede { relation })
            .expect("supersede");
        assert_eq!(superseded.state, ContextLifecycleState::Superseded);
        assert!(superseded.is_protected());
    }

    #[test]
    fn relation_actions_set_their_declared_lifecycle_states() {
        let cases = [
            (
                ContextLifecycleAction::Duplicate {
                    relation: ContextRelation::applied(
                        "rel-duplicate",
                        ContextRelationKind::DuplicateOf,
                        EVIDENCE,
                        "ctx-canonical",
                    ),
                },
                ContextLifecycleState::Duplicate,
            ),
            (
                ContextLifecycleAction::Supersede {
                    relation: ContextRelation::applied(
                        "rel-supersede",
                        ContextRelationKind::SupersededBy,
                        EVIDENCE,
                        "ctx-newer",
                    ),
                },
                ContextLifecycleState::Superseded,
            ),
            (
                ContextLifecycleAction::Summarize {
                    relation: ContextRelation::applied(
                        "rel-summary",
                        ContextRelationKind::SummarizedBy,
                        EVIDENCE,
                        "ctx-summary",
                    ),
                },
                ContextLifecycleState::Summarized,
            ),
            (
                ContextLifecycleAction::Archive {
                    relation: Some(ContextRelation::applied(
                        "rel-archive",
                        ContextRelationKind::ArchivedAs,
                        EVIDENCE,
                        "artifact-handle",
                    )),
                },
                ContextLifecycleState::Archived,
            ),
        ];

        for (action, expected_state) in cases {
            let lifecycle = protected().apply(action).expect("lifecycle action");
            assert_eq!(lifecycle.state, expected_state);
            assert!(lifecycle.is_protected());
            assert_eq!(lifecycle.relations.len(), 1);
        }
    }

    #[test]
    fn recovery_returns_archived_item_to_active_and_keeps_audit_relation() {
        let lifecycle = ContextLifecycle::default()
            .apply(ContextLifecycleAction::Archive { relation: None })
            .expect("archive");
        let recovered = lifecycle
            .apply(ContextLifecycleAction::Recover {
                relation: ContextRelation::applied(
                    "rel-recover",
                    ContextRelationKind::RecoveredFrom,
                    EVIDENCE,
                    "artifact-handle",
                ),
            })
            .expect("recover");
        assert_eq!(recovered.state, ContextLifecycleState::Active);
        assert!(recovered.is_protected());
        assert!(recovered.protection.contains(ContextProtectionReason::RecoveryMetadata));
        assert_eq!(recovered.relations.len(), 1);
    }
}
