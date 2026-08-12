//! Filesystem layout shared by session inventory and lifecycle operations.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thndrs_agent::CancelToken;

use super::SessionStorageState;

#[derive(Clone, Debug)]
pub(super) struct SessionStorageLayout {
    sessions_dir: PathBuf,
    workspace_root: PathBuf,
}

impl SessionStorageLayout {
    pub(super) fn new(sessions_dir: &Path, workspace_root: &Path) -> Self {
        Self { sessions_dir: sessions_dir.to_path_buf(), workspace_root: workspace_root.to_path_buf() }
    }

    pub(super) fn record_dir(&self, state: SessionStorageState) -> PathBuf {
        match state {
            SessionStorageState::Live => self.sessions_dir.clone(),
            SessionStorageState::Archived => self.sessions_dir.join("archive"),
            SessionStorageState::Trash => self.trash_dir(),
        }
    }

    pub(super) fn record(&self, state: SessionStorageState, id: &str) -> PathBuf {
        self.record_dir(state).join(format!("{id}.jsonl"))
    }

    pub(super) fn pins_dir(&self, state: SessionStorageState) -> PathBuf {
        match state {
            SessionStorageState::Live | SessionStorageState::Archived => self.sessions_dir.join("pins"),
            SessionStorageState::Trash => self.trash_dir().join("pins"),
        }
    }

    pub(super) fn pin(&self, state: SessionStorageState, id: &str) -> PathBuf {
        self.pins_dir(state).join(id)
    }

    pub(super) fn state_dir(&self, state: SessionStorageState) -> PathBuf {
        match state {
            SessionStorageState::Live | SessionStorageState::Archived => self.sessions_dir.join("state"),
            SessionStorageState::Trash => self.trash_dir().join("state"),
        }
    }

    pub(super) fn state(&self, state: SessionStorageState, id: &str) -> PathBuf {
        self.state_dir(state).join(id)
    }

    pub(super) fn logs_dir(&self, state: SessionStorageState) -> PathBuf {
        match state {
            SessionStorageState::Live | SessionStorageState::Archived => {
                self.workspace_root.join(".thndrs").join("logs").join("sessions")
            }
            SessionStorageState::Trash => self.trash_dir().join("logs"),
        }
    }

    pub(super) fn log(&self, state: SessionStorageState, id: &str) -> PathBuf {
        self.logs_dir(state).join(format!("thndrs-{id}.log"))
    }

    pub(super) fn trash_dir(&self) -> PathBuf {
        self.sessions_dir.join("trash")
    }

    pub(super) fn trash_metadata(&self, id: &str) -> PathBuf {
        self.trash_dir().join(format!("{id}.delete.json"))
    }

    pub(super) fn artifact_dir(&self) -> PathBuf {
        self.sessions_dir.join("artifacts")
    }

    pub(super) fn diagnostics_dir(&self) -> PathBuf {
        self.sessions_dir.join("lifecycle-diagnostics")
    }

    pub(super) fn collection_state(&self) -> PathBuf {
        self.sessions_dir.join("collection.json")
    }

    pub(super) fn collection_lock(&self) -> PathBuf {
        self.sessions_dir.join("collection.lock")
    }
}

pub(super) fn path_bytes(path: &Path) -> u64 {
    path_bytes_inner(path, None).unwrap_or_default()
}

pub(super) fn path_bytes_cancellable(path: &Path, cancellation: &CancelToken) -> io::Result<u64> {
    path_bytes_inner(path, Some(cancellation))
}

fn path_bytes_inner(path: &Path, cancellation: Option<&CancelToken>) -> io::Result<u64> {
    if cancellation.is_some_and(CancelToken::is_cancelled) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "session operation cancelled",
        ));
    }
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(0);
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(0);
    };
    let mut bytes = 0u64;
    for entry in entries.filter_map(Result::ok) {
        bytes = bytes.saturating_add(path_bytes_inner(&entry.path(), cancellation)?);
    }
    Ok(bytes)
}

pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
