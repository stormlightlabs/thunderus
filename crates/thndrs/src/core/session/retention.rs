//! Deterministic retention selection and application for live sessions.

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use serde::{Deserialize, Serialize};
use thndrs_agent::CancelToken;

use super::{DeleteSessionOptions, SessionInventory, SessionLifecycle, SessionStorageState};

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const MAX_PRUNE_WORKERS: usize = 8;

/// Configurable retention policy for unprotected live sessions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionRetentionPolicy {
    pub enabled: bool,
    pub max_age_days: u64,
    pub max_live_count: usize,
    pub min_age_days: u64,
    pub trash_retention_days: u64,
}

impl Default for SessionRetentionPolicy {
    fn default() -> Self {
        Self { enabled: true, max_age_days: 30, max_live_count: 200, min_age_days: 1, trash_retention_days: 7 }
    }
}

impl SessionRetentionPolicy {
    pub const fn max_age_seconds(&self) -> u64 {
        self.max_age_days.saturating_mul(SECONDS_PER_DAY)
    }

    pub const fn min_age_seconds(&self) -> u64 {
        self.min_age_days.saturating_mul(SECONDS_PER_DAY)
    }

    pub const fn trash_retention_seconds(&self) -> u64 {
        self.trash_retention_days.saturating_mul(SECONDS_PER_DAY)
    }
}

/// Explicit command-line overrides applied to the configured policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PruneOverrides {
    pub older_than_days: Option<u64>,
    pub keep_count: Option<usize>,
}

/// Why a session was selected for pruning.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PruneReason {
    MaximumAge,
    LiveCount,
}

/// Stable preview of one session selected by retention.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PruneCandidate {
    pub session_id: String,
    pub title: String,
    pub age_seconds: u64,
    pub bytes: u64,
    pub reasons: Vec<PruneReason>,
}

/// One best-effort prune failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PruneFailure {
    pub session_id: String,
    pub error: String,
}

/// Result of a prune preview or application.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PruneReport {
    pub dry_run: bool,
    pub candidates: Vec<PruneCandidate>,
    pub deleted_session_ids: Vec<String>,
    pub failures: Vec<PruneFailure>,
    pub reclaimed_bytes: u64,
}

/// Completed prune work reported while a plan is being applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PruneProgress {
    /// Lifecycle operations that have finished.
    pub completed: usize,
    /// Sessions successfully moved to trash.
    pub moved: usize,
    /// Sessions whose move failed.
    pub failed: usize,
    /// Sessions selected before application began.
    pub total: usize,
}

/// Select candidates once for previews, explicit prune, automatic collection, and tests.
pub fn select_prune_candidates(
    inventory: &SessionInventory, policy: &SessionRetentionPolicy, overrides: PruneOverrides,
    active_session_id: Option<&str>,
) -> Vec<PruneCandidate> {
    let explicit = overrides.older_than_days.is_some() || overrides.keep_count.is_some();
    if !policy.enabled && !explicit {
        return Vec::new();
    }

    let maximum_age = overrides
        .older_than_days
        .unwrap_or(policy.max_age_days)
        .saturating_mul(SECONDS_PER_DAY);
    let keep_count = overrides.keep_count.unwrap_or(policy.max_live_count);
    let minimum_age = policy.min_age_seconds();
    let unprotected_live_count = inventory
        .sessions
        .iter()
        .filter(|session| {
            session.storage_state == SessionStorageState::Live
                && !session.pinned
                && !session.locked
                && !session.corrupt
                && active_session_id != Some(session.id.as_str())
        })
        .count();
    let count_excess = unprotected_live_count.saturating_sub(keep_count);

    let mut eligible = inventory
        .sessions
        .iter()
        .filter_map(|session| {
            let age = session.age_seconds?;
            (session.storage_state == SessionStorageState::Live
                && !session.pinned
                && !session.locked
                && !session.corrupt
                && active_session_id != Some(session.id.as_str())
                && age >= minimum_age)
                .then_some((session, age))
        })
        .collect::<Vec<_>>();
    eligible
        .sort_by(|(left, left_age), (right, right_age)| right_age.cmp(left_age).then_with(|| left.id.cmp(&right.id)));

    eligible
        .into_iter()
        .enumerate()
        .filter_map(|(index, (session, age))| {
            let mut reasons = Vec::new();
            if age > maximum_age {
                reasons.push(PruneReason::MaximumAge);
            }
            if index < count_excess {
                reasons.push(PruneReason::LiveCount);
            }
            (!reasons.is_empty()).then(|| PruneCandidate {
                session_id: session.id.clone(),
                title: session.title.clone(),
                age_seconds: age,
                bytes: session.owned_bytes(),
                reasons,
            })
        })
        .collect()
}

/// Apply an already-selected plan, continuing after recoverable per-session failures.
pub fn apply_prune(
    lifecycle: &SessionLifecycle, candidates: Vec<PruneCandidate>, active_session_id: Option<&str>, dry_run: bool,
) -> PruneReport {
    apply_prune_inner(lifecycle, candidates, active_session_id, dry_run, None, |_| {}).unwrap_or_else(|report| report)
}

/// Apply a prune plan, stopping safely between session lifecycle operations.
pub fn apply_prune_cancellable(
    lifecycle: &SessionLifecycle, candidates: Vec<PruneCandidate>, active_session_id: Option<&str>, dry_run: bool,
    cancellation: &CancelToken,
) -> io::Result<PruneReport> {
    apply_prune_cancellable_with_progress(lifecycle, candidates, active_session_id, dry_run, cancellation, |_| {})
}

/// Apply a prune plan and report completed lifecycle operations from the coordinating thread.
pub(crate) fn apply_prune_cancellable_with_progress<F>(
    lifecycle: &SessionLifecycle, candidates: Vec<PruneCandidate>, active_session_id: Option<&str>, dry_run: bool,
    cancellation: &CancelToken, on_progress: F,
) -> io::Result<PruneReport>
where
    F: FnMut(PruneProgress),
{
    apply_prune_inner(
        lifecycle,
        candidates,
        active_session_id,
        dry_run,
        Some(cancellation),
        on_progress,
    )
    .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "session operation cancelled"))
}

fn apply_prune_inner<F>(
    lifecycle: &SessionLifecycle, candidates: Vec<PruneCandidate>, active_session_id: Option<&str>, dry_run: bool,
    cancellation: Option<&CancelToken>, mut on_progress: F,
) -> Result<PruneReport, PruneReport>
where
    F: FnMut(PruneProgress),
{
    let mut report =
        PruneReport { dry_run, candidates, deleted_session_ids: Vec::new(), failures: Vec::new(), reclaimed_bytes: 0 };
    if dry_run {
        return Ok(report);
    }
    let options =
        DeleteSessionOptions { active_session_id: active_session_id.map(str::to_string), allow_pinned: false };
    let next_candidate = AtomicUsize::new(0);
    let worker_count = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(MAX_PRUNE_WORKERS)
        .min(report.candidates.len());
    let mut outcomes = thread::scope(|scope| {
        let (outcome_sender, outcome_receiver) = mpsc::channel();
        let handles = (0..worker_count)
            .map(|_| {
                let outcome_sender = outcome_sender.clone();
                let next_candidate = &next_candidate;
                let candidates = &report.candidates;
                let options = &options;
                scope.spawn(move || {
                    loop {
                        if cancellation.is_some_and(CancelToken::is_cancelled) {
                            break;
                        }
                        let index = next_candidate.fetch_add(1, Ordering::Relaxed);
                        let Some(candidate) = candidates.get(index) else {
                            break;
                        };
                        if cancellation.is_some_and(CancelToken::is_cancelled) {
                            break;
                        }
                        let outcome = lifecycle.delete_live_retention_candidate(&candidate.session_id, options);
                        if outcome_sender.send((index, outcome)).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        drop(outcome_sender);
        let mut outcomes = Vec::with_capacity(report.candidates.len());
        let mut moved = 0;
        let mut failed = 0;
        for outcome in outcome_receiver {
            if outcome.1.is_ok() {
                moved += 1;
            } else {
                failed += 1;
            }
            outcomes.push(outcome);
            on_progress(PruneProgress { completed: outcomes.len(), moved, failed, total: report.candidates.len() });
        }
        for handle in handles {
            let _ = handle.join();
        }
        outcomes
    });
    outcomes.sort_by_key(|(index, _)| *index);
    for (index, outcome) in outcomes {
        let candidate = &report.candidates[index];
        match outcome {
            Ok(_) => {
                report.deleted_session_ids.push(candidate.session_id.clone());
                report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(candidate.bytes);
            }
            Err(error) => report
                .failures
                .push(PruneFailure { session_id: candidate.session_id.clone(), error: error.to_string() }),
        }
    }
    if cancellation.is_some_and(CancelToken::is_cancelled) {
        return Err(report);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::session::{SessionInventoryEntry, SessionLineageState, SessionWriter};

    fn entry(id: &str, age_days: u64) -> SessionInventoryEntry {
        SessionInventoryEntry {
            id: id.to_string(),
            path: PathBuf::new(),
            title: id.to_string(),
            model: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            last_activity: None,
            age_seconds: Some(age_days * SECONDS_PER_DAY),
            parent_session_id: None,
            source_turn_id: None,
            lineage: Vec::new(),
            lineage_state: SessionLineageState::Root,
            storage_state: SessionStorageState::Live,
            pinned: false,
            locked: false,
            corrupt: false,
            record_bytes: 10,
            lock_bytes: 0,
            log_bytes: 0,
            state_bytes: 0,
            artifact_handles: Default::default(),
        }
    }

    #[test]
    fn disabled_policy_has_no_automatic_candidates() {
        let inventory = SessionInventory { sessions: vec![entry("old", 90)], ..SessionInventory::default() };
        let policy = SessionRetentionPolicy { enabled: false, ..SessionRetentionPolicy::default() };
        assert!(select_prune_candidates(&inventory, &policy, PruneOverrides::default(), None).is_empty());
    }

    #[test]
    fn count_selection_is_oldest_first_and_respects_protection_and_minimum_age() {
        let mut pinned = entry("pinned", 90);
        pinned.pinned = true;
        let mut locked = entry("locked", 80);
        locked.locked = true;
        let inventory = SessionInventory {
            sessions: vec![
                entry("oldest", 20),
                entry("older", 10),
                entry("recent", 0),
                pinned,
                locked,
            ],
            ..SessionInventory::default()
        };
        let policy = SessionRetentionPolicy { max_age_days: 100, max_live_count: 1, ..Default::default() };
        let selected = select_prune_candidates(&inventory, &policy, PruneOverrides::default(), None);
        assert_eq!(
            selected.iter().map(|item| item.session_id.as_str()).collect::<Vec<_>>(),
            ["oldest", "older"]
        );
        assert_eq!(selected[0].reasons, [PruneReason::LiveCount]);
    }

    #[test]
    fn exact_age_boundary_is_retained() {
        let inventory = SessionInventory { sessions: vec![entry("boundary", 30)], ..SessionInventory::default() };
        assert!(select_prune_candidates(&inventory, &Default::default(), PruneOverrides::default(), None).is_empty());
    }

    #[test]
    fn explicit_override_works_when_automatic_retention_is_disabled() {
        let inventory = SessionInventory { sessions: vec![entry("old", 3)], ..SessionInventory::default() };
        let policy = SessionRetentionPolicy { enabled: false, ..Default::default() };
        let selected = select_prune_candidates(
            &inventory,
            &policy,
            PruneOverrides { older_than_days: Some(2), keep_count: None },
            None,
        );
        assert_eq!(selected[0].session_id, "old");
    }

    #[test]
    fn archives_are_excluded_and_forks_follow_the_same_retention_policy() {
        let mut archived = entry("archived", 90);
        archived.storage_state = SessionStorageState::Archived;
        let mut fork = entry("fork", 90);
        fork.parent_session_id = Some("parent".to_string());
        fork.lineage_state = SessionLineageState::Valid;
        let inventory = SessionInventory { sessions: vec![archived, fork], ..SessionInventory::default() };

        let selected = select_prune_candidates(&inventory, &Default::default(), PruneOverrides::default(), None);

        assert_eq!(
            selected.iter().map(|item| item.session_id.as_str()).collect::<Vec<_>>(),
            ["fork"]
        );
    }

    #[test]
    fn applying_a_plan_continues_after_one_session_fails() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        drop(
            SessionWriter::create(
                &sessions, "healthy", "/repo", "Healthy", "provider", "model", "none", "1", None,
            )
            .expect("healthy session"),
        );
        let candidates = vec![
            PruneCandidate {
                session_id: "missing".to_string(),
                title: "Missing".to_string(),
                age_seconds: 90 * SECONDS_PER_DAY,
                bytes: 10,
                reasons: vec![PruneReason::MaximumAge],
            },
            PruneCandidate {
                session_id: "healthy".to_string(),
                title: "Healthy".to_string(),
                age_seconds: 90 * SECONDS_PER_DAY,
                bytes: 10,
                reasons: vec![PruneReason::MaximumAge],
            },
        ];

        let report = apply_prune(
            &SessionLifecycle::new(&sessions, workspace.path()),
            candidates,
            None,
            false,
        );

        assert_eq!(report.deleted_session_ids, ["healthy"]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].session_id, "missing");
        assert!(sessions.join("trash/healthy.jsonl").is_file());
    }

    #[test]
    fn applying_a_plan_rechecks_live_protection_markers() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        drop(
            SessionWriter::create(
                &sessions,
                "protected",
                "/repo",
                "Protected",
                "provider",
                "model",
                "none",
                "1",
                None,
            )
            .expect("protected session"),
        );
        let pin = sessions.join("pins/protected");
        fs::create_dir_all(pin.parent().expect("pin parent")).expect("pin directory");
        fs::write(&pin, []).expect("pin session after selection");
        let candidate = PruneCandidate {
            session_id: "protected".to_string(),
            title: "Protected".to_string(),
            age_seconds: 90 * SECONDS_PER_DAY,
            bytes: 10,
            reasons: vec![PruneReason::MaximumAge],
        };

        let report = apply_prune(
            &SessionLifecycle::new(&sessions, workspace.path()),
            vec![candidate],
            None,
            false,
        );

        assert!(report.deleted_session_ids.is_empty());
        assert_eq!(report.failures[0].session_id, "protected");
        assert!(sessions.join("protected.jsonl").is_file());
        assert!(!sessions.join("trash/protected.jsonl").exists());
    }

    #[test]
    fn cancelled_plan_stops_before_the_next_session_mutation() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        drop(
            SessionWriter::create(
                &sessions, "healthy", "/repo", "Healthy", "provider", "model", "none", "1", None,
            )
            .expect("healthy session"),
        );
        let cancellation = CancelToken::new();
        cancellation.cancel();
        let candidate = PruneCandidate {
            session_id: "healthy".to_string(),
            title: "Healthy".to_string(),
            age_seconds: 90 * SECONDS_PER_DAY,
            bytes: 10,
            reasons: vec![PruneReason::MaximumAge],
        };

        let error = apply_prune_cancellable(
            &SessionLifecycle::new(&sessions, workspace.path()),
            vec![candidate],
            None,
            false,
            &cancellation,
        )
        .expect_err("cancelled prune");

        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert!(sessions.join("healthy.jsonl").is_file());
        assert!(!sessions.join("trash/healthy.jsonl").exists());
    }
}
