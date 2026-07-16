//! Workspace-local prompt history persistence.
//!
//! Prompt recall has different access patterns from full session replay
//! for performance, so it uses one small append-only JSONL file instead
//! of scanning every session.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::utils::datetime;

const SCHEMA_VERSION: u32 = 1;
const HISTORY_FILE_NAME: &str = "input-history.jsonl";
const HISTORY_LOCK_FILE_NAME: &str = "input-history.jsonl.lock";
const HISTORY_MAX_BYTES: usize = 1024 * 1024;
const HISTORY_SOFT_BYTES: usize = 768 * 1024;

pub const INPUT_HISTORY_LIMIT: usize = 200;

#[derive(Debug)]
pub struct InputHistoryStore {
    path: PathBuf,
    lock_path: PathBuf,
    max_bytes: usize,
    soft_bytes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct InputHistoryRecord {
    schema_version: u32,
    session_id: String,
    time: String,
    text: String,
}

impl InputHistoryRecord {
    fn new(session_id: &str, text: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            session_id: session_id.to_string(),
            time: datetime::now_iso8601(),
            text: text.to_string(),
        }
    }
}

struct InputHistoryLock {
    path: PathBuf,
    // TODO: why is this stored if its not read?
    _file: File,
}

impl InputHistoryStore {
    pub fn for_workspace(workspace_root: &Path) -> Self {
        let dir = workspace_root.join(".thndrs");
        Self {
            path: dir.join(HISTORY_FILE_NAME),
            lock_path: dir.join(HISTORY_LOCK_FILE_NAME),
            max_bytes: HISTORY_MAX_BYTES,
            soft_bytes: HISTORY_SOFT_BYTES,
        }
    }

    /// Load the newest valid entries, or `None` when no dedicated history has
    /// been created yet.
    pub fn load_recent(&self) -> io::Result<Option<Vec<String>>> {
        let content = match std::fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(recent_texts(&content)))
    }

    /// Append one submitted prompt and compact when the hard byte cap is
    /// exceeded. The newest prompt is retained even when it alone exceeds the
    /// configured soft cap.
    pub fn append(&self, session_id: &str, text: &str) -> io::Result<()> {
        let _lock = self.acquire_lock()?;
        self.ensure_parent()?;

        let record = InputHistoryRecord::new(session_id, text);
        let mut line = serde_json::to_string(&record).map_err(io::Error::other)?;
        line.push('\n');
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path)?;
        ensure_owner_only_permissions(&file)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;

        if file.metadata()?.len() > self.max_bytes as u64 {
            self.compact()?;
        }
        Ok(())
    }

    fn compact(&self) -> io::Result<()> {
        let content = std::fs::read_to_string(&self.path)?;
        let records = parsed_records(&content);
        let retained = retain_newest_records(records, self.soft_bytes);
        self.rewrite_records(retained)
    }

    fn rewrite_records(&self, records: impl IntoIterator<Item = InputHistoryRecord>) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path)?;
        ensure_owner_only_permissions(&file)?;
        for record in records {
            let mut line = serde_json::to_string(&record).map_err(io::Error::other)?;
            line.push('\n');
            file.write_all(line.as_bytes())?;
        }
        file.flush()
    }

    fn ensure_parent(&self) -> io::Result<()> {
        let Some(parent) = self.path.parent() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "input history path has no parent",
            ));
        };
        std::fs::create_dir_all(parent)
    }

    fn acquire_lock(&self) -> io::Result<InputHistoryLock> {
        self.ensure_parent()?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.lock_path)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    io::Error::new(io::ErrorKind::WouldBlock, "input history has an active writer")
                } else {
                    error
                }
            })?;
        Ok(InputHistoryLock { path: self.lock_path.clone(), _file: file })
    }

    #[cfg(test)]
    fn with_limits(path: PathBuf, max_bytes: usize, soft_bytes: usize) -> Self {
        let lock_path = path.with_file_name(HISTORY_LOCK_FILE_NAME);
        Self { path, lock_path, max_bytes, soft_bytes }
    }
}

impl Drop for InputHistoryLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn parsed_records(content: &str) -> Vec<InputHistoryRecord> {
    content
        .lines()
        .filter_map(|line| serde_json::from_str::<InputHistoryRecord>(line).ok())
        .filter(|record| record.schema_version == SCHEMA_VERSION && !record.text.trim().is_empty())
        .collect()
}

fn recent_texts(content: &str) -> Vec<String> {
    let mut recent = VecDeque::with_capacity(INPUT_HISTORY_LIMIT);
    for record in parsed_records(content) {
        if recent.back().is_some_and(|text| text == &record.text) {
            continue;
        }
        if recent.len() == INPUT_HISTORY_LIMIT {
            recent.pop_front();
        }
        recent.push_back(record.text);
    }
    recent.into_iter().collect()
}

fn retain_newest_records(records: Vec<InputHistoryRecord>, soft_bytes: usize) -> Vec<InputHistoryRecord> {
    let mut retained = Vec::new();
    let mut retained_bytes = 0usize;
    for record in records.into_iter().rev() {
        let record_bytes = serde_json::to_vec(&record).map_or(0, |line| line.len() + 1);
        if !retained.is_empty()
            && (retained.len() >= INPUT_HISTORY_LIMIT || retained_bytes.saturating_add(record_bytes) > soft_bytes)
        {
            break;
        }
        retained_bytes = retained_bytes.saturating_add(record_bytes);
        retained.push(record);
    }
    retained.reverse();
    retained
}

#[cfg(unix)]
fn ensure_owner_only_permissions(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = file.metadata()?;
    if metadata.permissions().mode() & 0o777 != 0o600 {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owner_only_permissions(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_load_recent_preserves_order_and_collapses_dupes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = InputHistoryStore::with_limits(dir.path().join(HISTORY_FILE_NAME), 4096, 3072);

        store.append("session-a", "first").expect("append first");
        store.append("session-a", "first").expect("append duplicate");
        store.append("session-b", "second").expect("append second");

        assert_eq!(
            store.load_recent().expect("load history"),
            Some(vec!["first".to_string(), "second".to_string()])
        );
    }

    #[test]
    fn compaction_keeps_newest_records_within_soft_cap() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = InputHistoryStore::with_limits(dir.path().join(HISTORY_FILE_NAME), 350, 220);

        for index in 0..8 {
            store
                .append("session", &format!("prompt {index} {}", "x".repeat(60)))
                .expect("append prompt");
        }

        let history = store.load_recent().expect("load history").expect("history exists");
        assert!(history.last().is_some_and(|text| text.starts_with("prompt 7")));
        assert!(std::fs::metadata(&store.path).expect("history metadata").len() <= 350);
    }

    #[test]
    fn malformed_records_are_ignored() {
        let valid = serde_json::to_string(&InputHistoryRecord::new("session", "valid")).expect("serialize record");
        let content = format!("not json\n{valid}\n");

        assert_eq!(recent_texts(&content), vec!["valid".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn history_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temp dir");
        let store = InputHistoryStore::with_limits(dir.path().join(HISTORY_FILE_NAME), 4096, 3072);
        store.append("session", "private prompt").expect("append prompt");

        let mode = std::fs::metadata(&store.path)
            .expect("history metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
