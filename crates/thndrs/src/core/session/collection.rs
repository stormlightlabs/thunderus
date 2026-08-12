//! Opportunistic, best-effort collection of expired session storage.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::storage::{SessionStorageLayout, path_bytes, unix_now};
use super::{
    PermanentDeleteOptions, PruneOverrides, SessionInventory, SessionLifecycle, SessionRetentionPolicy, apply_prune,
    select_prune_candidates,
};

const COLLECTION_INTERVAL_SECONDS: u64 = 24 * 60 * 60;
const STALE_TEMP_SECONDS: u64 = 24 * 60 * 60;
const DAILY_LOG_MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;
const DAILY_LOG_MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FAILURES: usize = 32;
const POLICY_VERSION: u32 = 1;

/// Durable audit of the most recent completed collection pass.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CollectionReport {
    pub policy_version: u32,
    pub completed_at_unix: u64,
    pub reclaimed_bytes: u64,
    pub skipped: Vec<String>,
    pub failures: Vec<String>,
}

/// Run collection when the last successful pass is absent or at least 24 hours old.
pub fn collect_if_due(
    sessions_dir: &Path, workspace_root: &Path, policy: &SessionRetentionPolicy, active_session_id: Option<&str>,
) -> io::Result<Option<CollectionReport>> {
    let layout = SessionStorageLayout::new(sessions_dir, workspace_root);
    let state_path = layout.collection_state();
    let now = unix_now();
    if let Ok(bytes) = fs::read(&state_path)
        && let Ok(previous) = serde_json::from_slice::<CollectionReport>(&bytes)
        && now.saturating_sub(previous.completed_at_unix) < COLLECTION_INTERVAL_SECONDS
    {
        return Ok(None);
    }
    collect_now(sessions_dir, workspace_root, policy, active_session_id).map(Some)
}

/// Run one idempotent best-effort collection pass and record its bounded audit.
pub fn collect_now(
    sessions_dir: &Path, workspace_root: &Path, policy: &SessionRetentionPolicy, active_session_id: Option<&str>,
) -> io::Result<CollectionReport> {
    let layout = SessionStorageLayout::new(sessions_dir, workspace_root);
    fs::create_dir_all(sessions_dir)?;
    let lock_path = layout.collection_lock();
    let _lock = match CollectionLock::acquire(lock_path.clone()).or_else(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists
            && file_age_seconds(&lock_path).is_some_and(|age| age > COLLECTION_INTERVAL_SECONDS)
        {
            fs::remove_file(&lock_path)?;
            CollectionLock::acquire(lock_path.clone())
        } else {
            Err(error)
        }
    }) {
        Ok(lock) => lock,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(CollectionReport {
                policy_version: POLICY_VERSION,
                completed_at_unix: unix_now(),
                reclaimed_bytes: 0,
                skipped: vec!["another collection pass is active".to_string()],
                failures: Vec::new(),
            });
        }
        Err(error) => return Err(error),
    };

    let mut report = CollectionReport {
        policy_version: POLICY_VERSION,
        completed_at_unix: unix_now(),
        reclaimed_bytes: 0,
        skipped: Vec::new(),
        failures: Vec::new(),
    };
    let lifecycle = SessionLifecycle::new(sessions_dir, workspace_root);
    let inventory = SessionInventory::scan(sessions_dir, workspace_root);
    let candidates = select_prune_candidates(&inventory, policy, PruneOverrides::default(), active_session_id);
    let prune = apply_prune(&lifecycle, candidates, active_session_id, false);
    for failure in prune.failures {
        push_failure(&mut report, format!("prune {}: {}", failure.session_id, failure.error));
    }

    expire_trash(
        &lifecycle,
        sessions_dir,
        workspace_root,
        policy,
        active_session_id,
        &mut report,
    );
    remove_orphan_session_state(sessions_dir, workspace_root, &mut report);
    remove_unreferenced_artifacts(sessions_dir, workspace_root, &mut report);
    remove_stale_temporary_files(&layout, &mut report);
    enforce_daily_log_limits(workspace_root, &mut report);
    report.completed_at_unix = unix_now();
    write_json_atomic(&layout.collection_state(), &report)?;
    Ok(report)
}

/// Bytes a collection pass could physically remove under the current policy.
pub fn reclaimable_bytes(
    sessions_dir: &Path, workspace_root: &Path, policy: &SessionRetentionPolicy, active_session_id: Option<&str>,
) -> u64 {
    let layout = SessionStorageLayout::new(sessions_dir, workspace_root);
    let inventory = SessionInventory::scan(sessions_dir, workspace_root);
    let pruning = select_prune_candidates(&inventory, policy, PruneOverrides::default(), active_session_id)
        .iter()
        .map(|candidate| candidate.bytes)
        .sum::<u64>();
    let expired_trash = inventory
        .sessions
        .iter()
        .filter(|session| {
            session.storage_state == super::SessionStorageState::Trash && !session.locked && !session.corrupt
        })
        .filter_map(|session| {
            let metadata = layout.trash_metadata(&session.id);
            let deleted_at = fs::read(&metadata)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|value| value.get("deleted_at_unix").and_then(serde_json::Value::as_u64))?;
            (unix_now().saturating_sub(deleted_at) > policy.trash_retention_seconds())
                .then(|| session.owned_bytes().saturating_add(path_bytes(&metadata)))
        })
        .sum::<u64>();
    let artifacts = if inventory.sessions.iter().any(|session| session.corrupt) {
        0
    } else {
        inventory
            .artifacts
            .iter()
            .filter(|artifact| artifact.referenced_by.is_empty() && !artifact.malformed)
            .map(|artifact| artifact.metadata_bytes.saturating_add(artifact.body_bytes))
            .sum::<u64>()
    };
    pruning.saturating_add(expired_trash).saturating_add(artifacts)
}

fn expire_trash(
    lifecycle: &SessionLifecycle, sessions_dir: &Path, workspace_root: &Path, policy: &SessionRetentionPolicy,
    active_session_id: Option<&str>, report: &mut CollectionReport,
) {
    let layout = SessionStorageLayout::new(sessions_dir, workspace_root);
    let inventory = SessionInventory::scan(sessions_dir, workspace_root);
    for session in inventory
        .sessions
        .iter()
        .filter(|session| session.storage_state == super::SessionStorageState::Trash)
    {
        if session.locked || session.corrupt {
            report.skipped.push(format!(
                "trash {} is {}",
                session.id,
                if session.locked { "locked" } else { "corrupt" },
            ));
            continue;
        }
        let metadata_path = layout.trash_metadata(&session.id);
        let deleted_at = fs::read(&metadata_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("deleted_at_unix").and_then(serde_json::Value::as_u64));
        let Some(deleted_at) = deleted_at else {
            report
                .skipped
                .push(format!("trash {} has uncertain deletion metadata", session.id));
            continue;
        };
        if unix_now().saturating_sub(deleted_at) <= policy.trash_retention_seconds() {
            continue;
        }
        let bytes = session.owned_bytes().saturating_add(path_bytes(&metadata_path));
        let options = PermanentDeleteOptions {
            active_session_id: active_session_id.map(str::to_string),
            allow_pinned: false,
            confirmed: true,
        };
        match lifecycle.permanently_delete(&session.id, &options) {
            Ok(_) => report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes),
            Err(error) => push_failure(report, format!("expire trash {}: {error}", session.id)),
        }
    }
}

fn remove_orphan_session_state(sessions_dir: &Path, workspace_root: &Path, report: &mut CollectionReport) {
    let inventory = SessionInventory::scan(sessions_dir, workspace_root);
    for diagnostic in inventory.diagnostics.iter().filter(|diagnostic| {
        matches!(
            diagnostic.code,
            "session_pin_unreferenced" | "session_state_unreferenced" | "session_log_unreferenced"
        )
    }) {
        let Some(path) = &diagnostic.path else { continue };
        let bytes = path_bytes(path);
        let result = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
        match result {
            Ok(()) => report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => push_failure(report, format!("remove orphan state {}: {error}", path.display())),
        }
    }
}

fn remove_unreferenced_artifacts(sessions_dir: &Path, workspace_root: &Path, report: &mut CollectionReport) {
    let inventory = SessionInventory::scan(sessions_dir, workspace_root);
    if inventory.sessions.iter().any(|session| session.corrupt) {
        report
            .skipped
            .push("artifact reachability is uncertain while corrupt sessions exist".to_string());
        return;
    }
    for artifact in inventory
        .artifacts
        .iter()
        .filter(|artifact| artifact.referenced_by.is_empty())
    {
        if artifact.malformed {
            report
                .skipped
                .push(format!("artifact {} has uncertain metadata", artifact.handle));
            continue;
        }
        for path in [&artifact.metadata_path, &artifact.body_path].into_iter().flatten() {
            let bytes = path_bytes(path);
            match fs::remove_file(path) {
                Ok(()) => report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => push_failure(report, format!("remove artifact {}: {error}", artifact.handle)),
            }
        }
    }
}

fn remove_stale_temporary_files(layout: &SessionStorageLayout, report: &mut CollectionReport) {
    for root in [layout.artifact_dir(), layout.diagnostics_dir(), layout.trash_dir()] {
        visit_files(&root, &mut |path| {
            let is_temp = path.extension().and_then(|extension| extension.to_str()) == Some("tmp");
            if !is_temp || file_age_seconds(path).is_none_or(|age| age <= STALE_TEMP_SECONDS) {
                return;
            }
            let bytes = path_bytes(path);
            match fs::remove_file(path) {
                Ok(()) => report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => push_failure(report, format!("remove temporary file {}: {error}", path.display())),
            }
        });
    }
}

fn enforce_daily_log_limits(workspace_root: &Path, report: &mut CollectionReport) {
    let root = workspace_root.join(".thndrs").join("logs");
    let Ok(entries) = fs::read_dir(&root) else { return };
    let mut logs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .map(|path| {
            let age = file_age_seconds(&path).unwrap_or_default();
            let bytes = path_bytes(&path);
            (path, age, bytes)
        })
        .collect::<Vec<_>>();
    logs.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let mut retained_bytes = logs.iter().map(|(_, _, bytes)| *bytes).sum::<u64>();
    for (path, age, bytes) in logs {
        if age <= DAILY_LOG_MAX_AGE_SECONDS && retained_bytes <= DAILY_LOG_MAX_BYTES {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                retained_bytes = retained_bytes.saturating_sub(bytes);
                report.reclaimed_bytes = report.reclaimed_bytes.saturating_add(bytes);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => push_failure(report, format!("remove daily log {}: {error}", path.display())),
        }
    }
}

fn visit_files(root: &Path, visitor: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            visit_files(&path, visitor);
        } else {
            visitor(&path);
        }
    }
}

fn file_age_seconds(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(std::time::SystemTime::now().duration_since(modified).ok()?.as_secs())
}

fn push_failure(report: &mut CollectionReport, failure: String) {
    if report.failures.len() < MAX_FAILURES {
        report.failures.push(failure);
    }
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)
}

struct CollectionLock(PathBuf);

impl CollectionLock {
    fn acquire(path: PathBuf) -> io::Result<Self> {
        OpenOptions::new().write(true).create_new(true).open(&path)?;
        Ok(Self(path))
    }
}

impl Drop for CollectionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{File, FileTimes};
    use std::time::UNIX_EPOCH;

    use super::*;
    use crate::artifacts::{ArtifactKind, ArtifactStore};

    #[test]
    fn collection_is_daily_idempotent_and_removes_known_orphan_state() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs").join("sessions");
        let orphan = sessions.join("state").join("missing-session");
        fs::create_dir_all(&orphan).expect("orphan state");
        fs::write(orphan.join("state.json"), b"orphan").expect("orphan contents");

        let report =
            collect_now(&sessions, workspace.path(), &SessionRetentionPolicy::default(), None).expect("collect");

        assert!(!orphan.exists());
        assert!(report.reclaimed_bytes >= 6);
        assert!(sessions.join("collection.json").is_file());
        assert_eq!(
            collect_if_due(&sessions, workspace.path(), &SessionRetentionPolicy::default(), None)
                .expect("check schedule"),
            None,
        );
    }

    #[test]
    fn collection_preserves_corrupt_trash_and_records_the_skip() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs").join("sessions");
        let corrupt = sessions.join("trash").join("corrupt.jsonl");
        fs::create_dir_all(corrupt.parent().expect("trash parent")).expect("trash");
        fs::write(&corrupt, b"not json\n").expect("corrupt record");

        let report =
            collect_now(&sessions, workspace.path(), &SessionRetentionPolicy::default(), None).expect("collect");

        assert!(corrupt.is_file());
        assert!(report.skipped.iter().any(|item| item == "trash corrupt is corrupt"));
    }

    #[test]
    fn collection_reclaims_a_stale_collector_lock() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs").join("sessions");
        fs::create_dir_all(&sessions).expect("sessions");
        let lock_path = sessions.join("collection.lock");
        let lock = File::create(&lock_path).expect("collection lock");
        lock.set_times(FileTimes::new().set_modified(UNIX_EPOCH))
            .expect("stale modified time");

        let report = collect_now(&sessions, workspace.path(), &SessionRetentionPolicy::default(), None)
            .expect("collect with stale lock");

        assert!(report.failures.is_empty());
        assert!(!lock_path.exists());
    }

    #[test]
    fn collection_preserves_artifacts_when_corruption_makes_reachability_uncertain() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs").join("sessions");
        let corrupt = sessions.join("corrupt.jsonl");
        fs::create_dir_all(&sessions).expect("sessions");
        fs::write(&corrupt, b"not json\n").expect("corrupt record");
        let artifact_root = sessions.join("artifacts");
        let artifact = ArtifactStore::new(&artifact_root)
            .create("possibly referenced", ArtifactKind::ToolEvidence, "evidence")
            .expect("artifact");

        let report = collect_now(&sessions, workspace.path(), &SessionRetentionPolicy::default(), None)
            .expect("collect with corrupt session");

        assert!(
            artifact_root
                .join(format!("{}.json", artifact.metadata.handle))
                .is_file()
        );
        assert!(
            artifact_root
                .join(format!("{}.body", artifact.metadata.handle))
                .is_file()
        );
        assert!(
            report
                .skipped
                .iter()
                .any(|item| item.contains("reachability is uncertain"))
        );
        assert_eq!(
            reclaimable_bytes(&sessions, workspace.path(), &Default::default(), None),
            0
        );
    }
}
