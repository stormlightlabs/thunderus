//! Application-owned inventory of session records and their filesystem state.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::artifacts::ArtifactMetadata;

use super::storage::{SessionStorageLayout, path_bytes, unix_now};
use super::{SessionLineageEntry, SessionReader, SessionRecord};

/// Lifecycle location of a durable session record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStorageState {
    Live,
    Archived,
    Trash,
}

/// Result of checking a session's direct parent and recorded lineage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionLineageState {
    Root,
    Valid,
    MissingParent,
    Malformed,
    Cycle,
}

impl SessionLineageState {
    /// Compact label suitable for session browsing surfaces.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Valid => "lineage ok",
            Self::MissingParent => "parent missing",
            Self::Malformed => "lineage malformed",
            Self::Cycle => "lineage cycle",
        }
    }
}

/// One session and the storage directly owned by it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionInventoryEntry {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub last_activity: Option<String>,
    pub age_seconds: Option<u64>,
    pub parent_session_id: Option<String>,
    pub source_turn_id: Option<String>,
    pub lineage: Vec<SessionLineageEntry>,
    pub lineage_state: SessionLineageState,
    pub storage_state: SessionStorageState,
    pub pinned: bool,
    pub locked: bool,
    pub corrupt: bool,
    pub record_bytes: u64,
    pub lock_bytes: u64,
    pub log_bytes: u64,
    pub state_bytes: u64,
    pub artifact_handles: BTreeSet<String>,
}

impl SessionInventoryEntry {
    /// Total bytes exclusively owned by this session, excluding shared artifacts.
    pub const fn owned_bytes(&self) -> u64 {
        self.record_bytes
            .saturating_add(self.lock_bytes)
            .saturating_add(self.log_bytes)
            .saturating_add(self.state_bytes)
    }

    /// Compact state label suitable for session lists and pickers.
    pub fn state_label(&self) -> String {
        let mut labels = Vec::new();
        if self.storage_state == SessionStorageState::Archived {
            labels.push("archived");
        }
        if self.pinned {
            labels.push("pinned");
        }
        if self.locked {
            labels.push("locked");
        }
        if self.corrupt {
            labels.push("corrupt");
        }
        if self.lineage_state != SessionLineageState::Root && self.lineage_state != SessionLineageState::Valid {
            labels.push(self.lineage_state.label());
        }
        if labels.is_empty() { "ready".to_string() } else { labels.join(", ") }
    }
}

/// One artifact sidecar and body, without loading the body itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactInventoryEntry {
    pub handle: String,
    pub metadata_path: Option<PathBuf>,
    pub body_path: Option<PathBuf>,
    pub metadata_bytes: u64,
    pub body_bytes: u64,
    pub created_at: Option<String>,
    pub age_seconds: Option<u64>,
    pub referenced_by: BTreeSet<String>,
    pub malformed: bool,
}

/// Non-fatal problem found while inventorying session-owned storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionInventoryDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub session_id: Option<String>,
    pub path: Option<PathBuf>,
}

/// Aggregate storage measurements. Shared artifact bytes are counted once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionStorageTotals {
    pub live_sessions: usize,
    pub archived_sessions: usize,
    pub pinned_sessions: usize,
    pub locked_sessions: usize,
    pub corrupt_sessions: usize,
    pub session_bytes: u64,
    pub log_bytes: u64,
    pub state_bytes: u64,
    pub artifact_count: usize,
    pub artifact_bytes: u64,
    pub trash_count: usize,
    pub trash_bytes: u64,
    pub reclaimable_bytes: u64,
}

/// Workspace storage graph used by browsing and later lifecycle policy.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionInventory {
    pub sessions: Vec<SessionInventoryEntry>,
    pub artifacts: Vec<ArtifactInventoryEntry>,
    pub diagnostics: Vec<SessionInventoryDiagnostic>,
    pub totals: SessionStorageTotals,
}

impl SessionInventory {
    /// Scan records, locks, logs, artifact sidecars/bodies, lifecycle locations,
    /// and the reserved per-session state directory.
    pub fn scan(sessions_dir: &Path, workspace_root: &Path) -> Self {
        let now = unix_now();
        let layout = SessionStorageLayout::new(sessions_dir, workspace_root);
        let archive_dir = layout.record_dir(SessionStorageState::Archived);
        let trash_dir = layout.record_dir(SessionStorageState::Trash);
        let pins_dir = layout.pins_dir(SessionStorageState::Live);
        let state_dir = layout.state_dir(SessionStorageState::Live);
        let logs_dir = layout.logs_dir(SessionStorageState::Live);
        let mut inventory = Self::default();

        inventory.scan_session_location(
            sessions_dir,
            SessionStorageState::Live,
            &pins_dir,
            &state_dir,
            &logs_dir,
            now,
        );
        inventory.scan_session_location(
            &archive_dir,
            SessionStorageState::Archived,
            &pins_dir,
            &state_dir,
            &logs_dir,
            now,
        );
        inventory.scan_session_location(
            &trash_dir,
            SessionStorageState::Trash,
            &layout.pins_dir(SessionStorageState::Trash),
            &layout.state_dir(SessionStorageState::Trash),
            &layout.logs_dir(SessionStorageState::Trash),
            now,
        );
        inventory.validate_lineage();
        inventory.scan_orphan_locks(sessions_dir, &archive_dir, &trash_dir);
        inventory.scan_orphan_session_storage(&pins_dir, &state_dir, &logs_dir);
        inventory.scan_orphan_session_storage(
            &layout.pins_dir(SessionStorageState::Trash),
            &layout.state_dir(SessionStorageState::Trash),
            &layout.logs_dir(SessionStorageState::Trash),
        );
        inventory.scan_artifacts(&layout.artifact_dir(), now);
        inventory.totals.trash_bytes = path_bytes(&trash_dir);
        inventory.totals.reclaimable_bytes = inventory
            .totals
            .reclaimable_bytes
            .saturating_add(inventory.totals.trash_bytes);
        inventory.sessions.sort_by(|left, right| {
            right
                .last_activity
                .cmp(&left.last_activity)
                .then_with(|| right.id.cmp(&left.id))
        });
        inventory
            .artifacts
            .sort_by(|left, right| left.handle.cmp(&right.handle));
        inventory
    }

    fn scan_session_location(
        &mut self, dir: &Path, storage_state: SessionStorageState, pins_dir: &Path, state_dir: &Path, logs_dir: &Path,
        now: u64,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else {
                continue;
            };
            let records = SessionReader::read_records(&path);
            let corrupt = SessionReader::read_validated_records(&path, &id).is_err();
            let summary = SessionReader::read_summary(&path);
            let fork = records.iter().find_map(|record| match record {
                SessionRecord::SessionFork { parent_session_id, parent_turn_id, lineage, .. } => {
                    Some((parent_session_id.clone(), parent_turn_id.clone(), lineage.clone()))
                }
                _ => None,
            });
            let last_activity = records.iter().filter_map(|r| r.record_time()).next_back();
            let artifact_handles = records.iter().flat_map(|r| r.artifact_handles()).collect();
            let lock_path = path.with_extension("jsonl.lock");
            let log_path = logs_dir.join(format!("thndrs-{id}.log"));
            let state_path = state_dir.join(&id);
            let record_bytes = file_bytes(&path);
            let lock_bytes = file_bytes(&lock_path);
            let log_bytes = file_bytes(&log_path);
            let state_bytes = path_bytes(&state_path);
            let (parent_session_id, source_turn_id, lineage) = fork
                .map(|(parent, turn, lineage)| (Some(parent), Some(turn), lineage))
                .unwrap_or_default();
            let pinned = pins_dir.join(&id).exists();
            let entry = SessionInventoryEntry {
                id,
                path,
                title: summary.title,
                model: summary.model,
                input_tokens: summary.input_tokens,
                output_tokens: summary.output_tokens,
                age_seconds: last_activity
                    .as_deref()
                    .and_then(parse_iso8601)
                    .map(|time| now.saturating_sub(time)),
                last_activity,
                parent_session_id,
                source_turn_id,
                lineage,
                lineage_state: SessionLineageState::Root,
                storage_state,
                pinned,
                locked: lock_path.exists(),
                corrupt,
                record_bytes,
                lock_bytes,
                log_bytes,
                state_bytes,
                artifact_handles,
            };
            self.update_session_totals(&entry);
            if storage_state == SessionStorageState::Trash {
                self.totals.trash_count = self.totals.trash_count.saturating_add(1);
            }
            if corrupt {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "session_corrupt",
                    message: format!("session `{}` contains malformed or invalid records", entry.id),
                    session_id: Some(entry.id.clone()),
                    path: Some(entry.path.clone()),
                });
            }
            self.sessions.push(entry);
        }
    }

    fn update_session_totals(&mut self, entry: &SessionInventoryEntry) {
        match entry.storage_state {
            SessionStorageState::Live => self.totals.live_sessions += 1,
            SessionStorageState::Archived => self.totals.archived_sessions += 1,
            SessionStorageState::Trash => {}
        }
        self.totals.pinned_sessions += usize::from(entry.pinned);
        self.totals.locked_sessions += usize::from(entry.locked);
        self.totals.corrupt_sessions += usize::from(entry.corrupt);
        self.totals.session_bytes = self
            .totals
            .session_bytes
            .saturating_add(entry.record_bytes.saturating_add(entry.lock_bytes));
        self.totals.log_bytes = self.totals.log_bytes.saturating_add(entry.log_bytes);
        self.totals.state_bytes = self.totals.state_bytes.saturating_add(entry.state_bytes);
    }

    fn validate_lineage(&mut self) {
        let parents: HashMap<String, Option<String>> = self
            .sessions
            .iter()
            .map(|session| (session.id.clone(), session.parent_session_id.clone()))
            .collect();
        let ids: HashSet<String> = self.sessions.iter().map(|session| session.id.clone()).collect();
        let lineages: HashMap<String, Vec<SessionLineageEntry>> = self
            .sessions
            .iter()
            .map(|session| (session.id.clone(), session.lineage.clone()))
            .collect();

        for session in &mut self.sessions {
            let Some(parent) = session.parent_session_id.as_deref() else {
                session.lineage_state = SessionLineageState::Root;
                continue;
            };
            session.lineage_state = if lineage_cycles(&session.id, &parents) {
                SessionLineageState::Cycle
            } else if !ids.contains(parent) {
                SessionLineageState::MissingParent
            } else {
                let mut expected = lineages.get(parent).cloned().unwrap_or_default();
                expected.push(SessionLineageEntry {
                    session_id: parent.to_string(),
                    turn_id: session.source_turn_id.clone().unwrap_or_default(),
                });
                if session.source_turn_id.is_some() && session.lineage == expected {
                    SessionLineageState::Valid
                } else {
                    SessionLineageState::Malformed
                }
            };
        }

        for session in &self.sessions {
            let (code, message) = match session.lineage_state {
                SessionLineageState::MissingParent => (
                    "session_parent_missing",
                    format!(
                        "session `{}` references missing parent `{}`",
                        session.id,
                        session.parent_session_id.as_deref().unwrap_or("unknown")
                    ),
                ),
                SessionLineageState::Malformed => (
                    "session_lineage_malformed",
                    format!("session `{}` has malformed lineage", session.id),
                ),
                SessionLineageState::Cycle => (
                    "session_lineage_cycle",
                    format!("session `{}` participates in a lineage cycle", session.id),
                ),
                SessionLineageState::Root | SessionLineageState::Valid => continue,
            };
            self.diagnostics.push(SessionInventoryDiagnostic {
                code,
                message,
                session_id: Some(session.id.clone()),
                path: Some(session.path.clone()),
            });
        }
    }

    fn artifact_references(&self) -> HashMap<String, BTreeSet<String>> {
        let mut references: HashMap<String, BTreeSet<String>> = HashMap::new();
        for session in self.sessions.clone() {
            for handle in &session.artifact_handles {
                references.entry(handle.clone()).or_default().insert(session.id.clone());
            }
        }
        references
    }

    fn scan_artifacts(&mut self, root: &Path, now: u64) {
        let references = self.artifact_references();
        let mut sidecars = BTreeMap::new();
        let mut bodies = BTreeMap::new();
        collect_artifact_files(root, &mut sidecars, &mut bodies);
        let mut handles: BTreeSet<String> = references.keys().cloned().collect();
        handles.extend(sidecars.keys().cloned());
        handles.extend(bodies.keys().cloned());

        for handle in handles {
            let metadata_path = sidecars.remove(&handle);
            let body_path = bodies.remove(&handle);
            let metadata_bytes = metadata_path.as_deref().map(file_bytes).unwrap_or_default();
            let body_bytes = body_path.as_deref().map(file_bytes).unwrap_or_default();
            let referenced_by = references.get(&handle).cloned().unwrap_or_default();
            let metadata = metadata_path
                .as_deref()
                .and_then(|path| fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice::<ArtifactMetadata>(&bytes).ok());
            let malformed = metadata_path.is_some() && metadata.is_none();
            let created_at = metadata.as_ref().map(|metadata| metadata.created_at.clone());
            let age_seconds = metadata
                .as_ref()
                .map(|metadata| now.saturating_sub(metadata.created_at_unix));

            if referenced_by.len() > 1 {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "artifact_multiply_referenced",
                    message: format!("artifact `{handle}` is shared by {} sessions", referenced_by.len()),
                    session_id: None,
                    path: metadata_path.clone(),
                });
            }
            if !referenced_by.is_empty() && metadata_path.is_none() {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "artifact_metadata_missing",
                    message: format!("referenced artifact `{handle}` has no metadata sidecar"),
                    session_id: None,
                    path: body_path.clone(),
                });
            }
            if malformed {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "artifact_metadata_malformed",
                    message: format!("artifact `{handle}` has malformed metadata"),
                    session_id: None,
                    path: metadata_path.clone(),
                });
            }
            if metadata_path.is_some() && body_path.is_none() {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "artifact_body_missing",
                    message: format!("artifact `{handle}` has no body"),
                    session_id: None,
                    path: metadata_path.clone(),
                });
            }
            if referenced_by.is_empty() {
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "artifact_unreferenced",
                    message: format!("artifact `{handle}` is not referenced by a retained session"),
                    session_id: None,
                    path: metadata_path.clone().or_else(|| body_path.clone()),
                });
                if !malformed {
                    self.totals.reclaimable_bytes = self
                        .totals
                        .reclaimable_bytes
                        .saturating_add(metadata_bytes.saturating_add(body_bytes));
                }
            }
            self.totals.artifact_count += 1;
            self.totals.artifact_bytes = self
                .totals
                .artifact_bytes
                .saturating_add(metadata_bytes.saturating_add(body_bytes));
            self.artifacts.push(ArtifactInventoryEntry {
                handle,
                metadata_path,
                body_path,
                metadata_bytes,
                body_bytes,
                created_at,
                age_seconds,
                referenced_by,
                malformed,
            });
        }
    }

    fn scan_orphan_session_storage(&mut self, pins_dir: &Path, state_dir: &Path, logs_dir: &Path) {
        let ids: HashSet<String> = self.sessions.iter().map(|session| session.id.clone()).collect();
        self.scan_orphan_entries(pins_dir, &ids, "session_pin_unreferenced", |path| {
            path.file_name().and_then(|name| name.to_str()).map(str::to_string)
        });
        self.scan_orphan_entries(state_dir, &ids, "session_state_unreferenced", |path| {
            path.file_name().and_then(|name| name.to_str()).map(str::to_string)
        });
        self.scan_orphan_entries(logs_dir, &ids, "session_log_unreferenced", |path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.strip_prefix("thndrs-"))
                .map(str::to_string)
        });
    }

    fn scan_orphan_locks(&mut self, live_dir: &Path, archive_dir: &Path, trash_dir: &Path) {
        let session_paths: HashSet<PathBuf> = self.sessions.iter().map(|session| session.path.clone()).collect();
        for dir in [live_dir, archive_dir, trash_dir] {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(id) = file_name.strip_suffix(".jsonl.lock") else {
                    continue;
                };
                if session_paths.contains(&dir.join(format!("{id}.jsonl"))) {
                    continue;
                }
                self.diagnostics.push(SessionInventoryDiagnostic {
                    code: "session_lock_unreferenced",
                    message: format!("lock for missing session `{id}` is unreferenced"),
                    session_id: Some(id.to_string()),
                    path: Some(path),
                });
            }
        }
    }

    fn scan_orphan_entries<F>(&mut self, dir: &Path, ids: &HashSet<String>, code: &'static str, session_id: F)
    where
        F: Fn(&Path) -> Option<String>,
    {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let Some(id) = session_id(&path) else {
                continue;
            };
            if ids.contains(&id) {
                continue;
            }
            self.diagnostics.push(SessionInventoryDiagnostic {
                code,
                message: format!("storage for missing session `{id}` is unreferenced"),
                session_id: Some(id),
                path: Some(path),
            });
        }
    }
}

fn lineage_cycles(start: &str, parents: &HashMap<String, Option<String>>) -> bool {
    let mut seen = HashSet::new();
    let mut current = Some(start);
    while let Some(id) = current {
        if !seen.insert(id) {
            return true;
        }
        current = parents.get(id).and_then(Option::as_deref);
    }
    false
}

fn collect_artifact_files(
    root: &Path, sidecars: &mut BTreeMap<String, PathBuf>, bodies: &mut BTreeMap<String, PathBuf>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if path.is_dir() {
            collect_artifact_files(&path, sidecars, bodies);
            continue;
        }
        let Some(handle) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_string) else {
            continue;
        };
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("json") => {
                sidecars.insert(handle, path);
            }
            Some("body") => {
                bodies.insert(handle, path);
            }
            _ => {}
        }
    }
}

fn file_bytes(path: &Path) -> u64 {
    fs::metadata(path).map(|metadata| metadata.len()).unwrap_or_default()
}

fn parse_iso8601(value: &str) -> Option<u64> {
    let (date, time) = value.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let year = date.next()?.ok()?;
    let month = date.next()?.ok()?;
    let day = date.next()?.ok()?;
    if date.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut time = time.split(':').map(str::parse::<u64>);
    let hour = time.next()?.ok()?;
    let minute = time.next()?.ok()?;
    let second = time.next()?.ok()?;
    if time.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
    let era = if adjusted_year >= 0 { adjusted_year } else { adjusted_year - 399 } / 400;
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days)
        .ok()
        .map(|days| days * 86_400 + hour * 3600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{ArtifactKind, ArtifactStore};
    use crate::session::{SCHEMA_VERSION, SessionWriter};

    fn create_session(root: &Path, id: &str) {
        SessionWriter::create(root, id, "/repo", "title", "provider", "model", "none", "1", None)
            .expect("create session");
    }

    #[test]
    fn reports_broken_lineage_without_hiding_valid_sessions() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        create_session(&sessions, "valid");
        let mut child = SessionWriter::create(
            &sessions, "child", "/repo", "child", "provider", "model", "none", "1", None,
        )
        .expect("child");
        child
            .append(SessionRecord::SessionFork {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: "2026-01-02T00:00:00Z".to_string(),
                parent_session_id: "missing".to_string(),
                parent_turn_id: "turn-1".to_string(),
                lineage: vec![SessionLineageEntry { session_id: "missing".to_string(), turn_id: "turn-1".to_string() }],
            })
            .expect("fork record");
        drop(child);

        let mut malformed = SessionWriter::create(
            &sessions,
            "malformed",
            "/repo",
            "malformed",
            "provider",
            "model",
            "none",
            "1",
            None,
        )
        .expect("malformed child");
        malformed
            .append(SessionRecord::SessionFork {
                schema_version: SCHEMA_VERSION,
                seq: 1,
                time: "2026-01-02T00:00:00Z".to_string(),
                parent_session_id: "valid".to_string(),
                parent_turn_id: "turn-1".to_string(),
                lineage: Vec::new(),
            })
            .expect("malformed fork record");
        drop(malformed);

        for (id, parent) in [("cycle-a", "cycle-b"), ("cycle-b", "cycle-a")] {
            let mut writer = SessionWriter::create(&sessions, id, "/repo", id, "provider", "model", "none", "1", None)
                .expect("cycle session");
            writer
                .append(SessionRecord::SessionFork {
                    schema_version: SCHEMA_VERSION,
                    seq: 1,
                    time: "2026-01-02T00:00:00Z".to_string(),
                    parent_session_id: parent.to_string(),
                    parent_turn_id: "turn-1".to_string(),
                    lineage: vec![SessionLineageEntry {
                        session_id: parent.to_string(),
                        turn_id: "turn-1".to_string(),
                    }],
                })
                .expect("cycle fork record");
        }

        let inventory = SessionInventory::scan(&sessions, workspace.path());

        assert_eq!(inventory.sessions.len(), 5);
        assert_eq!(
            inventory
                .sessions
                .iter()
                .find(|session| session.id == "child")
                .map(|session| session.lineage_state),
            Some(SessionLineageState::MissingParent)
        );
        assert_eq!(
            inventory
                .sessions
                .iter()
                .find(|session| session.id == "malformed")
                .map(|session| session.lineage_state),
            Some(SessionLineageState::Malformed)
        );
        assert!(
            inventory
                .sessions
                .iter()
                .filter(|session| session.id.starts_with("cycle-"))
                .all(|session| session.lineage_state == SessionLineageState::Cycle)
        );
        assert!(inventory.sessions.iter().any(|session| session.id == "valid"));
    }

    #[test]
    fn counts_shared_and_unreferenced_artifacts_without_reading_bodies() {
        let workspace = tempfile::tempdir().expect("workspace");
        let sessions = workspace.path().join(".thndrs/sessions");
        let store = ArtifactStore::new(sessions.join("artifacts"));
        let shared = store
            .create("shared", ArtifactKind::ToolEvidence, "evidence")
            .expect("shared artifact");
        let orphan = store
            .create("orphan", ArtifactKind::ToolEvidence, "orphan")
            .expect("orphan artifact");
        for id in ["first", "second"] {
            let mut writer = SessionWriter::create(&sessions, id, "/repo", id, "provider", "model", "none", "1", None)
                .expect("session");
            writer
                .append(SessionRecord::ToolFinished {
                    schema_version: SCHEMA_VERSION,
                    seq: 1,
                    time: "2026-01-02T00:00:00Z".to_string(),
                    turn_id: "turn-1".to_string(),
                    call_id: "call-1".to_string(),
                    status: crate::app::ToolStatus::Ok,
                    output: Vec::new(),
                    artifact: Some(shared.metadata.clone()),
                    mcp: None,
                })
                .expect("artifact record");
        }

        let inventory = SessionInventory::scan(&sessions, workspace.path());
        let shared = inventory
            .artifacts
            .iter()
            .find(|artifact| artifact.handle == shared.metadata.handle)
            .unwrap();
        let orphan = inventory
            .artifacts
            .iter()
            .find(|artifact| artifact.handle == orphan.metadata.handle)
            .unwrap();

        assert_eq!(shared.referenced_by.len(), 2);
        assert!(orphan.referenced_by.is_empty());
        assert!(inventory.totals.reclaimable_bytes >= orphan.metadata_bytes + orphan.body_bytes);
        assert!(
            inventory
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "artifact_multiply_referenced")
        );
        assert!(
            inventory
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "artifact_unreferenced")
        );
    }

    #[test]
    fn parses_record_activity_for_age_calculation() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso8601("2026-01-01T00:00:00Z"), Some(1_767_225_600));
        assert_eq!(parse_iso8601("not-a-time"), None);
    }
}
