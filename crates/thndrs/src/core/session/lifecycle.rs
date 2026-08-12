//! Manual lifecycle operations for the application-owned session storage graph.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::storage::{SessionStorageLayout, path_bytes, unix_now};
use super::{SessionInventory, SessionInventoryEntry, SessionStorageState};

/// A manual session lifecycle action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycleAction {
    Archive,
    Unarchive,
    Pin,
    Unpin,
    Delete,
    Restore,
    PermanentDelete,
}

impl SessionLifecycleAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Unarchive => "unarchive",
            Self::Pin => "pin",
            Self::Unpin => "unpin",
            Self::Delete => "delete",
            Self::Restore => "restore",
            Self::PermanentDelete => "permanent-delete",
        }
    }
}

/// One existing piece of state owned exclusively by a session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionOwnedState {
    pub kind: &'static str,
    pub path: PathBuf,
    pub bytes: u64,
}

/// One artifact that delete preserves because artifact cleanup is graph-aware.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeleteArtifactPreview {
    pub handle: String,
    pub bytes: u64,
    pub retained_session_ids: Vec<String>,
}

/// Exact filesystem and artifact effect shown before deleting a session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionDeletePreview {
    pub session_id: String,
    pub title: String,
    pub storage_state: SessionStorageState,
    pub pinned: bool,
    pub owned_state: Vec<SessionOwnedState>,
    pub preserved_artifacts: Vec<DeleteArtifactPreview>,
}

/// Safeguards required for a reversible delete.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeleteSessionOptions {
    pub active_session_id: Option<String>,
    pub allow_pinned: bool,
}

/// Safeguards required for irreversible deletion.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PermanentDeleteOptions {
    pub active_session_id: Option<String>,
    pub allow_pinned: bool,
    pub confirmed: bool,
}

/// Completed lifecycle operation and the paths it changed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionLifecycleReport {
    pub session_id: String,
    pub action: SessionLifecycleAction,
    pub changed_paths: Vec<PathBuf>,
}

/// Recoverable lifecycle failure.
#[derive(Debug, Error)]
pub enum SessionLifecycleError {
    #[error("session `{session_id}` is not found")]
    NotFound { session_id: String },
    #[error("session `{session_id}` is active and cannot be deleted")]
    Active { session_id: String },
    #[error("session `{session_id}` is locked and cannot be changed")]
    Locked { session_id: String },
    #[error("session `{session_id}` is pinned; explicit pinned-session confirmation is required")]
    PinnedConfirmationRequired { session_id: String },
    #[error("permanent deletion of session `{session_id}` requires explicit confirmation")]
    PermanentConfirmationRequired { session_id: String },
    #[error("session `{session_id}` is {actual}, not {expected}")]
    WrongStorageState {
        session_id: String,
        expected: &'static str,
        actual: &'static str,
    },
    #[error("session `{session_id}` cannot be restored because its trash metadata is missing or malformed")]
    MissingTrashMetadata { session_id: String },
    #[error("session `{session_id}` exceeded its restore grace period")]
    RestoreExpired { session_id: String },
    #[error("cannot change session `{session_id}` because destination `{path}` already exists")]
    DestinationExists { session_id: String, path: PathBuf },
    #[error("{action} of session `{session_id}` partially failed: {source}; recovery journal: {journal_path}")]
    PartialFailure {
        action: &'static str,
        session_id: String,
        journal_path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to {action} session `{session_id}` at `{path}`: {source}")]
    Io {
        action: &'static str,
        session_id: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Application-owned session lifecycle service.
#[derive(Clone, Debug)]
pub struct SessionLifecycle {
    sessions_dir: PathBuf,
    workspace_root: PathBuf,
}

impl SessionLifecycle {
    pub fn new(sessions_dir: impl Into<PathBuf>, workspace_root: impl Into<PathBuf>) -> Self {
        Self { sessions_dir: sessions_dir.into(), workspace_root: workspace_root.into() }
    }

    /// Build the confirmation preview without changing durable state.
    pub fn preview_delete(&self, session_id: &str) -> Result<SessionDeletePreview, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        if session.storage_state == SessionStorageState::Trash {
            return Err(wrong_state(session, "live or archived"));
        }
        let paths = self.paths(session);
        let mut owned_state = Vec::new();
        push_existing_state(&mut owned_state, "record", &paths.record);
        push_existing_state(&mut owned_state, "log", &paths.log);
        push_existing_state(&mut owned_state, "state", &paths.state);
        push_existing_state(&mut owned_state, "pin", &paths.pin);
        let preserved_artifacts = inventory
            .artifacts
            .iter()
            .filter(|artifact| session.artifact_handles.contains(&artifact.handle))
            .map(|artifact| DeleteArtifactPreview {
                handle: artifact.handle.clone(),
                bytes: artifact.metadata_bytes.saturating_add(artifact.body_bytes),
                retained_session_ids: artifact
                    .referenced_by
                    .iter()
                    .filter(|id| id.as_str() != session.id)
                    .cloned()
                    .collect(),
            })
            .collect();
        Ok(SessionDeletePreview {
            session_id: session.id.clone(),
            title: session.title.clone(),
            storage_state: session.storage_state,
            pinned: session.pinned,
            owned_state,
            preserved_artifacts,
        })
    }

    pub fn archive(&self, session_id: &str) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        self.move_record(
            session_id,
            SessionStorageState::Live,
            SessionStorageState::Archived,
            SessionLifecycleAction::Archive,
        )
    }

    pub fn unarchive(&self, session_id: &str) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        self.move_record(
            session_id,
            SessionStorageState::Archived,
            SessionStorageState::Live,
            SessionLifecycleAction::Unarchive,
        )
    }

    pub fn pin(&self, session_id: &str) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        if session.storage_state == SessionStorageState::Trash {
            return Err(wrong_state(session, "live or archived"));
        }
        let path = self.layout().pin(session.storage_state, &session.id);
        if path.exists() {
            return Ok(report(&session.id, SessionLifecycleAction::Pin, Vec::new()));
        }
        create_marker(&path).map_err(|source| io_error(SessionLifecycleAction::Pin, &session.id, &path, source))?;
        Ok(report(&session.id, SessionLifecycleAction::Pin, vec![path]))
    }

    pub fn unpin(&self, session_id: &str) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        if session.storage_state == SessionStorageState::Trash {
            return Err(wrong_state(session, "live or archived"));
        }
        let path = self.layout().pin(session.storage_state, &session.id);
        if !path.exists() {
            return Ok(report(&session.id, SessionLifecycleAction::Unpin, Vec::new()));
        }
        fs::remove_file(&path).map_err(|source| io_error(SessionLifecycleAction::Unpin, &session.id, &path, source))?;
        Ok(report(&session.id, SessionLifecycleAction::Unpin, vec![path]))
    }

    pub fn delete(
        &self, session_id: &str, options: &DeleteSessionOptions,
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        self.check_delete_protection(session, options.active_session_id.as_deref(), options.allow_pinned)?;
        if session.storage_state == SessionStorageState::Trash {
            return Err(wrong_state(session, "live or archived"));
        }
        let paths = self.paths(session);
        let metadata = TrashMetadata {
            deleted_at_unix: unix_now(),
            archived: session.storage_state == SessionStorageState::Archived,
        };
        let moves = existing_moves([
            (&paths.log, &paths.trash_log),
            (&paths.state, &paths.trash_state),
            (&paths.pin, &paths.trash_pin),
            (&paths.record, &paths.trash_record),
        ]);
        self.prepare_moves(&session.id, SessionLifecycleAction::Delete, &moves)?;
        ensure_destination_absent(&session.id, &paths.trash_metadata)?;
        write_json_atomic(&paths.trash_metadata, &metadata).map_err(|source| {
            io_error(
                SessionLifecycleAction::Delete,
                &session.id,
                &paths.trash_metadata,
                source,
            )
        })?;
        let mut report = self.move_graph_prepared(&session.id, SessionLifecycleAction::Delete, &moves)?;
        report.changed_paths.push(paths.trash_metadata);
        Ok(report)
    }

    pub fn restore(
        &self, session_id: &str, grace_period: Duration,
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        if session.storage_state != SessionStorageState::Trash {
            return Err(wrong_state(session, "trash"));
        }
        let paths = self.paths(session);
        let metadata: TrashMetadata = fs::read(&paths.trash_metadata)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| SessionLifecycleError::MissingTrashMetadata { session_id: session.id.clone() })?;
        if unix_now().saturating_sub(metadata.deleted_at_unix) > grace_period.as_secs() {
            return Err(SessionLifecycleError::RestoreExpired { session_id: session.id.clone() });
        }
        let record_target = if metadata.archived {
            self.layout().record(SessionStorageState::Archived, &session.id)
        } else {
            self.layout().record(SessionStorageState::Live, &session.id)
        };
        let moves = existing_moves([
            (&paths.trash_log, &paths.log),
            (&paths.trash_state, &paths.state),
            (&paths.trash_pin, &paths.pin),
            (&paths.trash_record, &record_target),
        ]);
        let report = self.move_graph(&session.id, SessionLifecycleAction::Restore, &moves)?;
        fs::remove_file(&paths.trash_metadata).map_err(|source| {
            io_error(
                SessionLifecycleAction::Restore,
                &session.id,
                &paths.trash_metadata,
                source,
            )
        })?;
        Ok(report)
    }

    pub fn permanently_delete(
        &self, session_id: &str, options: &PermanentDeleteOptions,
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        if !options.confirmed {
            return Err(SessionLifecycleError::PermanentConfirmationRequired { session_id: session_id.to_string() });
        }
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        self.check_delete_protection(session, options.active_session_id.as_deref(), options.allow_pinned)?;
        if session.storage_state != SessionStorageState::Trash {
            return Err(wrong_state(session, "trash"));
        }
        let paths = self.paths(session);
        let quarantine = self
            .layout()
            .trash_dir()
            .join("deleting")
            .join(format!("{}-{}", session.id, unix_now()));
        let moves = existing_moves([
            (&paths.trash_log, &quarantine.join("session.log")),
            (&paths.trash_state, &quarantine.join("state")),
            (&paths.trash_pin, &quarantine.join("pin")),
            (&paths.trash_metadata, &quarantine.join("delete.json")),
            (&paths.trash_record, &quarantine.join("session.jsonl")),
        ]);
        let mut report = self.move_graph(&session.id, SessionLifecycleAction::PermanentDelete, &moves)?;
        fs::remove_dir_all(&quarantine).map_err(|source| {
            io_error(
                SessionLifecycleAction::PermanentDelete,
                &session.id,
                &quarantine,
                source,
            )
        })?;
        report.changed_paths.push(quarantine);
        Ok(report)
    }

    fn inventory(&self) -> SessionInventory {
        SessionInventory::scan(&self.sessions_dir, &self.workspace_root)
    }

    fn move_record(
        &self, session_id: &str, expected: SessionStorageState, target_state: SessionStorageState,
        action: SessionLifecycleAction,
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let inventory = self.inventory();
        let session = find_session(&inventory, session_id)?;
        if session.storage_state != expected {
            return Err(wrong_state(session, storage_label(expected)));
        }
        if session.locked {
            return Err(SessionLifecycleError::Locked { session_id: session.id.clone() });
        }
        let target = self.layout().record(target_state, &session.id);
        ensure_destination_absent(&session.id, &target)?;
        create_parent(&target).map_err(|source| io_error(action, &session.id, &target, source))?;
        fs::rename(&session.path, &target).map_err(|source| io_error(action, &session.id, &session.path, source))?;
        Ok(report(&session.id, action, vec![session.path.clone(), target]))
    }

    fn check_delete_protection(
        &self, session: &SessionInventoryEntry, active_session_id: Option<&str>, allow_pinned: bool,
    ) -> Result<(), SessionLifecycleError> {
        if active_session_id == Some(session.id.as_str()) {
            return Err(SessionLifecycleError::Active { session_id: session.id.clone() });
        }
        if session.locked {
            return Err(SessionLifecycleError::Locked { session_id: session.id.clone() });
        }
        if session.pinned && !allow_pinned {
            return Err(SessionLifecycleError::PinnedConfirmationRequired { session_id: session.id.clone() });
        }
        Ok(())
    }

    fn paths(&self, session: &SessionInventoryEntry) -> SessionPaths {
        let id = &session.id;
        let layout = self.layout();
        SessionPaths {
            record: session.path.clone(),
            log: layout.log(SessionStorageState::Live, id),
            state: layout.state(SessionStorageState::Live, id),
            pin: layout.pin(SessionStorageState::Live, id),
            trash_record: layout.record(SessionStorageState::Trash, id),
            trash_log: layout.log(SessionStorageState::Trash, id),
            trash_state: layout.state(SessionStorageState::Trash, id),
            trash_pin: layout.pin(SessionStorageState::Trash, id),
            trash_metadata: layout.trash_metadata(id),
        }
    }

    fn layout(&self) -> SessionStorageLayout {
        SessionStorageLayout::new(&self.sessions_dir, &self.workspace_root)
    }

    fn move_graph(
        &self, session_id: &str, action: SessionLifecycleAction, moves: &[MovePlan],
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        self.prepare_moves(session_id, action, moves)?;
        self.move_graph_prepared(session_id, action, moves)
    }

    fn move_graph_prepared(
        &self, session_id: &str, action: SessionLifecycleAction, moves: &[MovePlan],
    ) -> Result<SessionLifecycleReport, SessionLifecycleError> {
        let journal = LifecycleJournal { action, session_id, moves, error: None };
        let journal_path = self.journal_path(session_id, action);
        write_json_atomic(&journal_path, &journal)
            .map_err(|source| io_error(action, session_id, &journal_path, source))?;
        let mut changed_paths = Vec::new();
        for movement in moves {
            if let Err(source) = fs::rename(&movement.from, &movement.to) {
                let failed = LifecycleJournal { action, session_id, moves, error: Some(source.to_string()) };
                let _ = write_json_atomic(&journal_path, &failed);
                return Err(SessionLifecycleError::PartialFailure {
                    action: action.label(),
                    session_id: session_id.to_string(),
                    journal_path,
                    source,
                });
            }
            changed_paths.push(movement.from.clone());
            changed_paths.push(movement.to.clone());
        }
        fs::remove_file(&journal_path).map_err(|source| io_error(action, session_id, &journal_path, source))?;
        Ok(report(session_id, action, changed_paths))
    }

    fn prepare_moves(
        &self, session_id: &str, action: SessionLifecycleAction, moves: &[MovePlan],
    ) -> Result<(), SessionLifecycleError> {
        for movement in moves {
            ensure_destination_absent(session_id, &movement.to)?;
            create_parent(&movement.to).map_err(|source| io_error(action, session_id, &movement.to, source))?;
        }
        Ok(())
    }

    fn journal_path(&self, session_id: &str, action: SessionLifecycleAction) -> PathBuf {
        self.layout().diagnostics_dir().join(format!(
            "{session_id}-{}-{}-{}.json",
            action.label(),
            unix_now(),
            std::process::id()
        ))
    }
}

#[derive(Debug)]
struct SessionPaths {
    record: PathBuf,
    log: PathBuf,
    state: PathBuf,
    pin: PathBuf,
    trash_record: PathBuf,
    trash_log: PathBuf,
    trash_state: PathBuf,
    trash_pin: PathBuf,
    trash_metadata: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct MovePlan {
    from: PathBuf,
    to: PathBuf,
}

#[derive(Serialize)]
struct LifecycleJournal<'a> {
    action: SessionLifecycleAction,
    session_id: &'a str,
    moves: &'a [MovePlan],
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct TrashMetadata {
    deleted_at_unix: u64,
    archived: bool,
}

fn find_session<'a>(
    inventory: &'a SessionInventory, session_id: &str,
) -> Result<&'a SessionInventoryEntry, SessionLifecycleError> {
    inventory
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| SessionLifecycleError::NotFound { session_id: session_id.to_string() })
}

fn wrong_state(session: &SessionInventoryEntry, expected: &'static str) -> SessionLifecycleError {
    SessionLifecycleError::WrongStorageState {
        session_id: session.id.clone(),
        expected,
        actual: storage_label(session.storage_state),
    }
}

const fn storage_label(state: SessionStorageState) -> &'static str {
    match state {
        SessionStorageState::Live => "live",
        SessionStorageState::Archived => "archived",
        SessionStorageState::Trash => "trash",
    }
}

fn existing_moves<'a, const N: usize>(pairs: [(&'a Path, &'a Path); N]) -> Vec<MovePlan> {
    pairs
        .into_iter()
        .filter(|(from, _)| from.exists())
        .map(|(from, to)| MovePlan { from: from.to_path_buf(), to: to.to_path_buf() })
        .collect()
}

fn push_existing_state(states: &mut Vec<SessionOwnedState>, kind: &'static str, path: &Path) {
    if path.exists() {
        states.push(SessionOwnedState { kind, path: path.to_path_buf(), bytes: path_bytes(path) });
    }
}

fn create_marker(path: &Path) -> io::Result<()> {
    create_parent(path)?;
    fs::OpenOptions::new().write(true).create_new(true).open(path).map(drop)
}

fn create_parent(path: &Path) -> io::Result<()> {
    path.parent().map_or(Ok(()), fs::create_dir_all)
}

fn ensure_destination_absent(session_id: &str, path: &Path) -> Result<(), SessionLifecycleError> {
    if path.exists() {
        Err(SessionLifecycleError::DestinationExists { session_id: session_id.to_string(), path: path.to_path_buf() })
    } else {
        Ok(())
    }
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> io::Result<()> {
    create_parent(path)?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn report(session_id: &str, action: SessionLifecycleAction, changed_paths: Vec<PathBuf>) -> SessionLifecycleReport {
    SessionLifecycleReport { session_id: session_id.to_string(), action, changed_paths }
}

fn io_error(action: SessionLifecycleAction, session_id: &str, path: &Path, source: io::Error) -> SessionLifecycleError {
    SessionLifecycleError::Io {
        action: action.label(),
        session_id: session_id.to_string(),
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::TempDir;

    use super::*;
    use crate::app::ToolStatus;
    use crate::artifacts::{ArtifactKind, ArtifactStore};
    use crate::session::{SCHEMA_VERSION, SessionRecord, SessionWriter};

    fn harness() -> (TempDir, PathBuf, SessionLifecycle) {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        let lifecycle = SessionLifecycle::new(&sessions, workspace.path());
        (workspace, sessions, lifecycle)
    }

    fn create_session(sessions: &Path, id: &str) {
        SessionWriter::create(sessions, id, "/repo", id, "provider", "model", "none", "1", None)
            .expect("create session");
    }

    fn append_artifact(sessions: &Path, id: &str, artifact: crate::artifacts::ArtifactMetadata) {
        let mut writer = SessionWriter::resume(&sessions.join(format!("{id}.jsonl")), id).expect("resume session");
        writer
            .append(SessionRecord::ToolFinished {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: "2026-01-02T00:00:00Z".to_string(),
                turn_id: "turn-1".to_string(),
                call_id: "call-1".to_string(),
                status: ToolStatus::Ok,
                output: Vec::new(),
                artifact: Some(artifact),
                mcp: None,
            })
            .expect("artifact record");
    }

    #[test]
    fn archive_pin_and_unarchive_do_not_rewrite_history() {
        let (_workspace, sessions, lifecycle) = harness();
        create_session(&sessions, "session");
        let live = sessions.join("session.jsonl");
        let original = fs::read(&live).expect("history");

        lifecycle.pin("session").expect("pin");
        lifecycle.archive("session").expect("archive");
        let archived = sessions.join("archive/session.jsonl");
        assert_eq!(fs::read(&archived).expect("archived history"), original);
        assert!(sessions.join("pins/session").is_file());

        lifecycle.unarchive("session").expect("unarchive");
        lifecycle.unpin("session").expect("unpin");
        assert_eq!(fs::read(live).expect("restored history"), original);
        assert!(!sessions.join("pins/session").exists());
    }

    #[test]
    fn delete_preview_lists_owned_state_and_preserved_shared_artifacts() {
        let (workspace, sessions, lifecycle) = harness();
        create_session(&sessions, "first");
        create_session(&sessions, "second");
        let artifact = ArtifactStore::new(sessions.join("artifacts"))
            .create("shared", ArtifactKind::ToolEvidence, "evidence")
            .expect("artifact");
        append_artifact(&sessions, "first", artifact.metadata.clone());
        append_artifact(&sessions, "second", artifact.metadata.clone());
        lifecycle.pin("first").expect("pin");
        let log = workspace.path().join(".thndrs/logs/sessions/thndrs-first.log");
        create_parent(&log).expect("log parent");
        fs::write(&log, "log").expect("log");
        let state = sessions.join("state/first/cache");
        create_parent(&state).expect("state parent");
        fs::write(&state, "state").expect("state");

        let preview = lifecycle.preview_delete("first").expect("preview");
        let kinds: BTreeSet<_> = preview.owned_state.iter().map(|state| state.kind).collect();
        assert_eq!(kinds, BTreeSet::from(["log", "pin", "record", "state"]));
        assert_eq!(preview.preserved_artifacts.len(), 1);
        assert_eq!(preview.preserved_artifacts[0].handle, artifact.metadata.handle);
        assert_eq!(preview.preserved_artifacts[0].retained_session_ids, ["second"]);
    }

    #[test]
    fn delete_and_restore_move_the_complete_archived_session_graph() {
        let (workspace, sessions, lifecycle) = harness();
        create_session(&sessions, "session");
        let artifact = ArtifactStore::new(sessions.join("artifacts"))
            .create("restorable", ArtifactKind::ToolEvidence, "evidence")
            .expect("artifact");
        append_artifact(&sessions, "session", artifact.metadata.clone());
        lifecycle.pin("session").expect("pin");
        lifecycle.archive("session").expect("archive");
        let archived = sessions.join("archive/session.jsonl");
        let original = fs::read(&archived).expect("history");
        let log = workspace.path().join(".thndrs/logs/sessions/thndrs-session.log");
        create_parent(&log).expect("log parent");
        fs::write(&log, "log").expect("log");
        let state = sessions.join("state/session/cache");
        create_parent(&state).expect("state parent");
        fs::write(&state, "state").expect("state");

        lifecycle
            .delete(
                "session",
                &DeleteSessionOptions { allow_pinned: true, ..Default::default() },
            )
            .expect("delete");
        assert!(sessions.join("trash/session.jsonl").is_file());
        assert!(sessions.join("trash/logs/thndrs-session.log").is_file());
        assert!(sessions.join("trash/state/session/cache").is_file());
        assert!(sessions.join("trash/pins/session").is_file());
        assert!(!archived.exists());
        let inventory = SessionInventory::scan(&sessions, workspace.path());
        assert_eq!(
            inventory
                .artifacts
                .iter()
                .find(|entry| entry.handle == artifact.metadata.handle)
                .map(|entry| entry.referenced_by.clone()),
            Some(BTreeSet::from(["session".to_string()]))
        );

        lifecycle
            .restore("session", Duration::from_secs(86_400))
            .expect("restore");
        assert_eq!(fs::read(&archived).expect("restored history"), original);
        assert!(log.is_file());
        assert!(state.is_file());
        assert!(sessions.join("pins/session").is_file());
    }

    #[test]
    fn delete_rejects_active_locked_and_unconfirmed_pinned_sessions() {
        let (_workspace, sessions, lifecycle) = harness();
        for id in ["active", "locked", "pinned"] {
            create_session(&sessions, id);
        }
        fs::write(sessions.join("locked.jsonl.lock"), "lock").expect("lock");
        lifecycle.pin("pinned").expect("pin");

        assert!(matches!(
            lifecycle.delete(
                "active",
                &DeleteSessionOptions { active_session_id: Some("active".to_string()), allow_pinned: false }
            ),
            Err(SessionLifecycleError::Active { .. })
        ));
        assert!(matches!(
            lifecycle.delete("locked", &DeleteSessionOptions::default()),
            Err(SessionLifecycleError::Locked { .. })
        ));
        assert!(matches!(
            lifecycle.delete("pinned", &DeleteSessionOptions::default()),
            Err(SessionLifecycleError::PinnedConfirmationRequired { .. })
        ));
    }

    #[test]
    fn expired_restore_and_permanent_delete_require_explicit_confirmation() {
        let (_workspace, sessions, lifecycle) = harness();
        create_session(&sessions, "session");
        lifecycle
            .delete("session", &DeleteSessionOptions::default())
            .expect("delete");
        write_json_atomic(
            &sessions.join("trash/session.delete.json"),
            &TrashMetadata { deleted_at_unix: 0, archived: false },
        )
        .expect("old trash metadata");

        assert!(matches!(
            lifecycle.restore("session", Duration::from_secs(1)),
            Err(SessionLifecycleError::RestoreExpired { .. })
        ));
        assert!(matches!(
            lifecycle.permanently_delete("session", &PermanentDeleteOptions::default()),
            Err(SessionLifecycleError::PermanentConfirmationRequired { .. })
        ));
        lifecycle
            .permanently_delete(
                "session",
                &PermanentDeleteOptions { confirmed: true, ..Default::default() },
            )
            .expect("permanent delete");
        assert!(!sessions.join("trash/session.jsonl").exists());
        assert!(!sessions.join("trash/session.delete.json").exists());
    }

    #[test]
    fn partial_move_leaves_a_recovery_journal() {
        let (_workspace, sessions, lifecycle) = harness();
        let first = sessions.join("first");
        fs::create_dir_all(&sessions).expect("sessions");
        fs::write(&first, "first").expect("first source");
        let moves = vec![
            MovePlan { from: first, to: sessions.join("moved-first") },
            MovePlan { from: sessions.join("missing"), to: sessions.join("moved-missing") },
        ];

        let error = lifecycle
            .move_graph("session", SessionLifecycleAction::Delete, &moves)
            .expect_err("partial failure");
        let SessionLifecycleError::PartialFailure { journal_path, .. } = error else {
            panic!("expected partial failure");
        };
        assert!(sessions.join("moved-first").is_file());
        let journal = fs::read_to_string(journal_path).expect("recovery journal");
        assert!(journal.contains("\"error\""));
        assert!(journal.contains("missing"));
    }
}
