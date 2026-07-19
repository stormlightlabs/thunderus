//! State-aware deduplication for bounded model projections.
//!
//! Applications supply opaque, tool-specific identities. This module only
//! compares the logical source and its observed state fingerprint; it never
//! interprets tool arguments, files, repositories, or environment variables.
//! A duplicate is eligible only when the most recent observation of the same
//! source has the same state fingerprint. A changed fingerprint supersedes
//! that most recent observation instead of being treated as a duplicate.

use crate::accounting::{ContextReductionMode, ContextReductionReceipt};

use super::reduction::{ReductionConfig, measure_lines};

/// Stable version of state-identical projection deduplication.
pub const STATE_IDENTICAL_REDUCER_VERSION: &str = "state-identical-evidence-v1";

/// Opaque application-defined identity for one observed tool state.
///
/// `source` identifies the logical subject of an observation, such as a file
/// path and line range or a normalized search request. `fingerprint` proves
/// the relevant state observed by that tool adapter. Neither field is sent to
/// providers by this module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateProjectionIdentity {
    source: String,
    fingerprint: String,
}

impl StateProjectionIdentity {
    /// Create an identity when both adapter-defined components are non-empty.
    pub fn new(source: impl Into<String>, fingerprint: impl Into<String>) -> Option<Self> {
        let source = source.into();
        let fingerprint = fingerprint.into();
        (!source.trim().is_empty() && !fingerprint.trim().is_empty()).then_some(Self { source, fingerprint })
    }

    /// Return the opaque logical-source key.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Return the opaque observed-state fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// One bounded projection considered for state-aware reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateProjectionCandidate {
    /// Stable item or tool-call id.
    pub id: String,
    /// Bounded model-facing lines after any lossless per-result reductions.
    pub lines: Vec<String>,
    /// Opaque application-defined source/state identity.
    pub identity: Option<StateProjectionIdentity>,
    /// Whether the evidence must remain present regardless of identity.
    pub protected: bool,
    /// Whether a removed duplicate needs a short causality placeholder.
    pub requires_placeholder: bool,
}

impl StateProjectionCandidate {
    /// Construct a candidate from already-bounded model projection lines.
    pub fn new(id: impl Into<String>, lines: Vec<String>, identity: Option<StateProjectionIdentity>) -> Self {
        Self { id: id.into(), lines, identity, protected: false, requires_placeholder: false }
    }

    /// Preserve this evidence even when its state matches an earlier result.
    pub fn protected(mut self) -> Self {
        self.protected = true;
        self
    }

    /// Require a short provider-visible placeholder if this duplicate is removed.
    pub fn requiring_placeholder(mut self) -> Self {
        self.requires_placeholder = true;
        self
    }
}

/// Prior observation retained only for state-aware comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateProjectionRecord {
    /// Stable id of the earlier observation.
    pub id: String,
    /// Opaque source/state identity for that observation.
    pub identity: StateProjectionIdentity,
    /// Whether this observation was protected when it entered the history.
    pub protected: bool,
}

impl StateProjectionRecord {
    /// Build a history record when the candidate supplied an identity.
    pub fn from_candidate(candidate: &StateProjectionCandidate) -> Option<Self> {
        candidate.identity.clone().map(|identity| Self {
            id: candidate.id.clone(),
            identity,
            protected: candidate.protected,
        })
    }
}

/// Relationship proven by one state-aware comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateProjectionDecision {
    /// No state-aware lifecycle relation was applied.
    Retained,
    /// The current observation has exactly the same proven state as its canonical predecessor.
    DuplicateOf {
        /// Earlier canonical observation with the same logical source and state.
        canonical_id: String,
    },
    /// The current observation has a newer state for the same logical source.
    Supersedes {
        /// Earlier observation replaced by this newer state.
        previous_id: String,
    },
}

/// Result of a state-aware reduction decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateProjectionReduction {
    /// Model-facing lines after an applied decision; baseline lines otherwise.
    pub lines: Vec<String>,
    /// Proven lifecycle relationship, if any.
    pub decision: StateProjectionDecision,
    /// Shadow, applied, or baseline-fallback receipt for a related observation.
    pub receipt: Option<ContextReductionReceipt>,
}

impl StateProjectionReduction {
    /// Return a new record that should extend this source's state history.
    ///
    /// Exact duplicates return no record because the canonical observation is
    /// already in history. A later changed state therefore supersedes the
    /// visible canonical item, rather than an item whose projection was
    /// already omitted.
    pub fn history_record(&self, candidate: &StateProjectionCandidate) -> Option<StateProjectionRecord> {
        match &self.decision {
            StateProjectionDecision::DuplicateOf { .. } => None,
            StateProjectionDecision::Retained | StateProjectionDecision::Supersedes { .. } => {
                StateProjectionRecord::from_candidate(candidate)
            }
        }
    }
}

/// Compare a projection with the latest observation of the same logical source.
///
/// The caller owns the history and should append
/// [`StateProjectionReduction::history_record`] for every candidate with an
/// identity, including protected ones. This keeps the comparison pure and lets
/// application adapters choose exactly which repository, environment, or
/// freshness facts prove equivalence.
pub fn reduce_state_identical(
    candidate: &StateProjectionCandidate, history: &[StateProjectionRecord], config: &ReductionConfig,
) -> StateProjectionReduction {
    let baseline = candidate.lines.clone();
    let Some(identity) = candidate.identity.as_ref() else {
        return StateProjectionReduction {
            lines: baseline,
            decision: StateProjectionDecision::Retained,
            receipt: None,
        };
    };
    let Some(previous) = history
        .iter()
        .rev()
        .find(|record| record.identity.source() == identity.source())
    else {
        return StateProjectionReduction {
            lines: baseline,
            decision: StateProjectionDecision::Retained,
            receipt: None,
        };
    };
    let mode = if config.state_identical {
        ContextReductionMode::Applied
    } else if config.shadow {
        ContextReductionMode::Shadow
    } else {
        return StateProjectionReduction {
            lines: baseline,
            decision: StateProjectionDecision::Retained,
            receipt: None,
        };
    };
    let before_bytes = measure_lines(&baseline);

    if candidate.protected {
        return StateProjectionReduction {
            lines: baseline,
            decision: StateProjectionDecision::Retained,
            receipt: Some(receipt(
                &candidate.id,
                mode,
                before_bytes,
                before_bytes,
                Some("protected evidence retained".to_string()),
            )),
        };
    }

    if previous.identity.fingerprint() == identity.fingerprint() {
        let decision = StateProjectionDecision::DuplicateOf { canonical_id: previous.id.clone() };
        let lines = if config.state_identical { duplicate_projection(candidate, &previous.id) } else { baseline };
        let after_bytes = measure_lines(&lines);
        return StateProjectionReduction {
            lines,
            decision,
            receipt: Some(receipt(&candidate.id, mode, before_bytes, after_bytes, None)),
        };
    }

    if previous.protected {
        return StateProjectionReduction {
            lines: baseline,
            decision: StateProjectionDecision::Retained,
            receipt: Some(receipt(
                &candidate.id,
                mode,
                before_bytes,
                before_bytes,
                Some("protected prior evidence retained".to_string()),
            )),
        };
    }

    StateProjectionReduction {
        lines: baseline,
        decision: StateProjectionDecision::Supersedes { previous_id: previous.id.clone() },
        receipt: Some(receipt(&candidate.id, mode, before_bytes, before_bytes, None)),
    }
}

fn duplicate_projection(candidate: &StateProjectionCandidate, canonical_id: &str) -> Vec<String> {
    if candidate.requires_placeholder {
        {
            vec![format!(
                "[duplicate tool result omitted; unchanged since {canonical_id}]"
            )]
        }
    } else {
        Default::default()
    }
}

fn receipt(
    item_id: &str, mode: ContextReductionMode, before_bytes: u64, after_bytes: u64, diagnostic: Option<String>,
) -> ContextReductionReceipt {
    ContextReductionReceipt {
        item_id: item_id.to_string(),
        method: "state_identical_evidence".to_string(),
        version: STATE_IDENTICAL_REDUCER_VERSION.to_string(),
        before_bytes,
        after_bytes,
        lossy: before_bytes != after_bytes,
        mode,
        diagnostic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(applied: bool) -> ReductionConfig {
        ReductionConfig { shadow: !applied, state_identical: applied, ..ReductionConfig::disabled() }
    }

    fn candidate(id: &str, source: &str, fingerprint: &str) -> StateProjectionCandidate {
        StateProjectionCandidate::new(
            id,
            vec![format!("result from {id}")],
            StateProjectionIdentity::new(source, fingerprint),
        )
    }

    #[test]
    fn exact_latest_state_is_the_only_duplicate_case() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a");
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let duplicate = reduce_state_identical(
            &candidate("read-2", "file:src/lib.rs:1-20", "hash-a"),
            &history,
            &config(true),
        );

        assert_eq!(duplicate.lines, Vec::<String>::new());
        assert_eq!(
            duplicate.decision,
            StateProjectionDecision::DuplicateOf { canonical_id: "read-1".to_string() }
        );
        assert_eq!(duplicate.receipt.expect("receipt").mode, ContextReductionMode::Applied);
    }

    #[test]
    fn changed_state_supersedes_instead_of_deduplicating() {
        let first = candidate("command-before", "command:cargo-test", "workspace-1");
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let changed = reduce_state_identical(
            &candidate("command-after", "command:cargo-test", "workspace-2"),
            &history,
            &config(true),
        );

        assert_eq!(changed.lines, vec!["result from command-after"]);
        assert_eq!(
            changed.decision,
            StateProjectionDecision::Supersedes { previous_id: "command-before".to_string() }
        );
    }

    #[test]
    fn reverted_state_supersedes_the_latest_different_state() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a");
        let changed = candidate("read-2", "file:src/lib.rs:1-20", "hash-b");
        let history = vec![
            StateProjectionRecord::from_candidate(&first).expect("identity"),
            StateProjectionRecord::from_candidate(&changed).expect("identity"),
        ];

        let reverted = reduce_state_identical(
            &candidate("read-3", "file:src/lib.rs:1-20", "hash-a"),
            &history,
            &config(true),
        );

        assert_eq!(
            reverted.decision,
            StateProjectionDecision::Supersedes { previous_id: "read-2".to_string() }
        );
    }

    #[test]
    fn changed_state_after_a_duplicate_supersedes_the_canonical_observation() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a");
        let duplicate_candidate = candidate("read-2", "file:src/lib.rs:1-20", "hash-a");
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let duplicate = reduce_state_identical(&duplicate_candidate, &history, &config(true));
        assert!(duplicate.history_record(&duplicate_candidate).is_none());

        let changed = reduce_state_identical(
            &candidate("read-3", "file:src/lib.rs:1-20", "hash-b"),
            &history,
            &config(true),
        );

        assert_eq!(
            changed.decision,
            StateProjectionDecision::Supersedes { previous_id: "read-1".to_string() }
        );
    }

    #[test]
    fn protected_evidence_is_not_removed() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a");
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let protected = reduce_state_identical(
            &candidate("read-2", "file:src/lib.rs:1-20", "hash-a").protected(),
            &history,
            &config(true),
        );

        assert_eq!(protected.lines, vec!["result from read-2"]);
        assert_eq!(protected.decision, StateProjectionDecision::Retained);
        assert_eq!(
            protected.receipt.expect("receipt").diagnostic.as_deref(),
            Some("protected evidence retained")
        );
    }

    #[test]
    fn changed_state_does_not_supersede_protected_prior_evidence() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a").protected();
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let changed = reduce_state_identical(
            &candidate("read-2", "file:src/lib.rs:1-20", "hash-b"),
            &history,
            &config(true),
        );

        assert_eq!(changed.decision, StateProjectionDecision::Retained);
        assert_eq!(
            changed.receipt.expect("receipt").diagnostic.as_deref(),
            Some("protected prior evidence retained")
        );
    }

    #[test]
    fn shadow_receipt_preserves_projection_and_causal_placeholder_is_opt_in() {
        let first = candidate("read-1", "file:src/lib.rs:1-20", "hash-a");
        let history = vec![StateProjectionRecord::from_candidate(&first).expect("identity")];
        let shadow = reduce_state_identical(
            &candidate("read-2", "file:src/lib.rs:1-20", "hash-a").requiring_placeholder(),
            &history,
            &config(false),
        );

        assert_eq!(shadow.lines, vec!["result from read-2"]);
        assert_eq!(shadow.receipt.expect("receipt").mode, ContextReductionMode::Shadow);

        let applied = reduce_state_identical(
            &candidate("read-2", "file:src/lib.rs:1-20", "hash-a").requiring_placeholder(),
            &history,
            &config(true),
        );
        assert_eq!(
            applied.lines,
            vec!["[duplicate tool result omitted; unchanged since read-1]"]
        );
    }
}
