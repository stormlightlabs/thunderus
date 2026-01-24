use crate::error::{Error, Result};

use notify::{Event, RecursiveMode, Watcher};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum DriftEvent {
    FileSystemChange(Vec<PathBuf>),
    StateMismatch { expected: String, actual: String },
}

pub struct DriftMonitor {
    _watcher: Box<dyn Watcher + Send + Sync>,
    event_tx: broadcast::Sender<DriftEvent>,
}

impl DriftMonitor {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(16);
        let tx = event_tx.clone();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res
                && (event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove())
            {
                let paths = event.paths;
                let _ = tx.send(DriftEvent::FileSystemChange(paths));
            }
        })
        .map_err(|e| Error::Watcher(e.to_string()))?;

        watcher
            .watch(path.as_ref(), RecursiveMode::Recursive)
            .map_err(|e| Error::Watcher(e.to_string()))?;

        Ok(Self { _watcher: Box::new(watcher), event_tx })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DriftEvent> {
        self.event_tx.subscribe()
    }
}

#[derive(Clone)]
pub struct SnapshotManager {
    repo_path: PathBuf,
}

impl SnapshotManager {
    pub fn new<P: Into<PathBuf>>(repo_path: P) -> Self {
        Self { repo_path: repo_path.into() }
    }

    pub fn get_current_state(&self) -> Result<String> {
        let repo = git2::Repository::discover(&self.repo_path).map_err(|e| Error::Git(e.to_string()))?;

        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);

        let statuses = repo.statuses(Some(&mut opts)).map_err(|e| Error::Git(e.to_string()))?;

        let head = repo.head().map_err(|e| Error::Git(e.to_string()))?;
        let head_target = head
            .target()
            .ok_or_else(|| Error::Git("HEAD has no target".to_string()))?;

        if statuses.is_empty() {
            Ok(head_target.to_string())
        } else {
            let mut entries: Vec<_> = statuses
                .iter()
                .map(|e| (e.path().unwrap_or("").to_string(), e.status().bits()))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));

            let mut hasher = DefaultHasher::new();
            for (path, status) in entries {
                path.hash(&mut hasher);
                status.hash(&mut hasher);
            }
            let hash = hasher.finish();

            Ok(format!("dirty-{}-{:x}", head_target, hash))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_clean_and_dirty() {
        let temp = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        let path = temp.path().join("file.txt");

        {
            let mut file = File::create(&path).unwrap();
            writeln!(file, "initial").unwrap();
        }
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        let sig = repo.signature().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[]).unwrap();

        let manager = SnapshotManager::new(temp.path());
        let clean_state = manager.get_current_state().unwrap();
        assert!(!clean_state.starts_with("dirty"));

        {
            let mut file = File::create(&path).unwrap();
            writeln!(file, "modified").unwrap();
        }

        let dirty_state = manager.get_current_state().unwrap();
        assert!(dirty_state.starts_with("dirty"));
        assert_ne!(clean_state, dirty_state);

        let dirty_state_2 = manager.get_current_state().unwrap();
        assert_eq!(dirty_state, dirty_state_2);
    }
}
