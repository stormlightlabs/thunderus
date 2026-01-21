//! Memory kind and related enums
//!
//! Defines the types and categories for memory documents.

use serde::{Deserialize, Serialize};

/// Kind of memory document
///
/// Each kind represents a different tier in the memory system with different
/// lifespans, purposes, and access patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    /// Core memory: always-loaded project knowledge
    Core,
    /// Semantic memory: stable facts about the project
    Fact,
    /// Semantic memory: architectural decision records (ADR-lite)
    Adr,
    /// Procedural memory: reusable playbooks and workflows
    Playbook,
    /// Episodic memory: session recaps and historical context
    Recap,
}

impl MemoryKind {
    /// Get the directory name for this memory kind
    pub fn dir_name(self) -> &'static str {
        match self {
            MemoryKind::Core => "core",
            MemoryKind::Fact => "semantic/FACTS",
            MemoryKind::Adr => "semantic/DECISIONS",
            MemoryKind::Playbook => "procedural/PLAYBOOKS",
            MemoryKind::Recap => "episodic",
        }
    }

    /// Get the file extension for this memory kind
    pub fn extension(self) -> &'static str {
        match self {
            MemoryKind::Adr => ".md",
            _ => ".md",
        }
    }

    /// Check if this kind requires hierarchical loading (always loaded)
    pub fn is_always_loaded(self) -> bool {
        matches!(self, MemoryKind::Core)
    }

    /// Check if this kind is semantic memory
    pub fn is_semantic(self) -> bool {
        matches!(self, MemoryKind::Fact | MemoryKind::Adr)
    }

    /// Check if this kind is loaded on-demand
    pub fn is_on_demand(self) -> bool {
        !self.is_always_loaded()
    }
}

/// Verification status for a memory document
///
/// Indicates whether the document's content has been verified against
/// the current state of the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    /// Document has been verified and is up-to-date
    Verified,
    /// Document content may be stale (repo changed since last verification)
    Stale,
    /// Document verification status is unknown
    Unknown,
}

impl VerificationStatus {
    /// Check if the document is considered current
    pub fn is_current(self) -> bool {
        matches!(self, VerificationStatus::Verified)
    }

    /// Check if the document may be stale
    pub fn is_stale(self) -> bool {
        matches!(self, VerificationStatus::Stale)
    }
}

/// Provenance information for a memory document
///
/// Tracks the source events, patches, and commits that created or modified
/// this document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Provenance {
    /// Event IDs that contributed to this document
    #[serde(default)]
    pub events: Vec<String>,
    /// Patch IDs that modified this document
    #[serde(default)]
    pub patches: Vec<String>,
    /// Commit hashes related to this document
    #[serde(default)]
    pub commits: Vec<String>,
}

impl Provenance {
    /// Create a new empty provenance
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event ID to the provenance
    pub fn with_event(mut self, event_id: impl Into<String>) -> Self {
        self.events.push(event_id.into());
        self
    }

    /// Add a patch ID to the provenance
    pub fn with_patch(mut self, patch_id: impl Into<String>) -> Self {
        self.patches.push(patch_id.into());
        self
    }

    /// Add a commit hash to the provenance
    pub fn with_commit(mut self, commit: impl Into<String>) -> Self {
        self.commits.push(commit.into());
        self
    }

    /// Check if the provenance is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.patches.is_empty() && self.commits.is_empty()
    }

    /// Get the total number of provenance entries
    pub fn len(&self) -> usize {
        self.events.len() + self.patches.len() + self.commits.len()
    }
}

/// Verification metadata for a memory document
///
/// Tracks verification state against repository commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    /// Last commit hash where this document was verified
    pub last_verified_commit: Option<String>,
    /// Current verification status
    pub status: VerificationStatus,
}

impl Verification {
    /// Create a new verification with unknown status
    pub fn new() -> Self {
        Self { last_verified_commit: None, status: VerificationStatus::Unknown }
    }

    /// Create a verified verification
    pub fn verified(commit: impl Into<String>) -> Self {
        Self { last_verified_commit: Some(commit.into()), status: VerificationStatus::Verified }
    }

    /// Create a stale verification
    pub fn stale(commit: impl Into<String>) -> Self {
        Self { last_verified_commit: Some(commit.into()), status: VerificationStatus::Stale }
    }

    /// Check if this verification is verified
    pub fn is_verified(&self) -> bool {
        self.status.is_current()
    }
}

impl Default for Verification {
    fn default() -> Self {
        Self::new()
    }
}

/// Session metadata for episodic memory recaps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Session identifier
    pub id: String,
    /// Session duration in minutes
    pub duration_minutes: u32,
    /// Number of events in the session
    pub event_count: usize,
    /// Number of files modified during the session
    pub files_modified: usize,
}

impl SessionMeta {
    /// Create new session metadata
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), duration_minutes: 0, event_count: 0, files_modified: 0 }
    }

    /// Set the duration
    pub fn with_duration(mut self, minutes: u32) -> Self {
        self.duration_minutes = minutes;
        self
    }

    /// Set the event count
    pub fn with_event_count(mut self, count: usize) -> Self {
        self.event_count = count;
        self
    }

    /// Set the files modified count
    pub fn with_files_modified(mut self, count: usize) -> Self {
        self.files_modified = count;
        self
    }
}

impl Default for SessionMeta {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_kind_dir_name() {
        assert_eq!(MemoryKind::Core.dir_name(), "core");
        assert_eq!(MemoryKind::Fact.dir_name(), "semantic/FACTS");
        assert_eq!(MemoryKind::Adr.dir_name(), "semantic/DECISIONS");
        assert_eq!(MemoryKind::Playbook.dir_name(), "procedural/PLAYBOOKS");
        assert_eq!(MemoryKind::Recap.dir_name(), "episodic");
    }

    #[test]
    fn test_memory_kind_is_always_loaded() {
        assert!(MemoryKind::Core.is_always_loaded());
        assert!(!MemoryKind::Fact.is_always_loaded());
        assert!(!MemoryKind::Adr.is_always_loaded());
        assert!(!MemoryKind::Playbook.is_always_loaded());
        assert!(!MemoryKind::Recap.is_always_loaded());
    }

    #[test]
    fn test_memory_kind_is_semantic() {
        assert!(!MemoryKind::Core.is_semantic());
        assert!(MemoryKind::Fact.is_semantic());
        assert!(MemoryKind::Adr.is_semantic());
        assert!(!MemoryKind::Playbook.is_semantic());
        assert!(!MemoryKind::Recap.is_semantic());
    }

    #[test]
    fn test_memory_kind_is_on_demand() {
        assert!(!MemoryKind::Core.is_on_demand());
        assert!(MemoryKind::Fact.is_on_demand());
        assert!(MemoryKind::Adr.is_on_demand());
        assert!(MemoryKind::Playbook.is_on_demand());
        assert!(MemoryKind::Recap.is_on_demand());
    }

    #[test]
    fn test_memory_kind_serialization() {
        let kinds = vec![
            MemoryKind::Core,
            MemoryKind::Fact,
            MemoryKind::Adr,
            MemoryKind::Playbook,
            MemoryKind::Recap,
        ];

        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let deserialized: MemoryKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, deserialized);
        }
    }

    #[test]
    fn test_verification_status_is_current() {
        assert!(VerificationStatus::Verified.is_current());
        assert!(!VerificationStatus::Stale.is_current());
        assert!(!VerificationStatus::Unknown.is_current());
    }

    #[test]
    fn test_verification_status_is_stale() {
        assert!(!VerificationStatus::Verified.is_stale());
        assert!(VerificationStatus::Stale.is_stale());
        assert!(!VerificationStatus::Unknown.is_stale());
    }

    #[test]
    fn test_verification_status_serialization() {
        let statuses = vec![
            VerificationStatus::Verified,
            VerificationStatus::Stale,
            VerificationStatus::Unknown,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: VerificationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_provenance_new() {
        let prov = Provenance::new();
        assert!(prov.is_empty());
        assert_eq!(prov.len(), 0);
    }

    #[test]
    fn test_provenance_with_event() {
        let prov = Provenance::new().with_event("evt_001");
        assert!(!prov.is_empty());
        assert_eq!(prov.len(), 1);
        assert_eq!(prov.events, vec!["evt_001".to_string()]);
    }

    #[test]
    fn test_provenance_with_multiple() {
        let prov = Provenance::new()
            .with_event("evt_001")
            .with_event("evt_002")
            .with_patch("patch_001")
            .with_commit("abc123");

        assert!(!prov.is_empty());
        assert_eq!(prov.len(), 4);
        assert_eq!(prov.events.len(), 2);
        assert_eq!(prov.patches.len(), 1);
        assert_eq!(prov.commits.len(), 1);
    }

    #[test]
    fn test_provenance_is_empty() {
        let prov = Provenance::new();
        assert!(prov.is_empty());

        let prov = prov.with_event("evt_001");
        assert!(!prov.is_empty());
    }

    #[test]
    fn test_provenance_len() {
        let prov = Provenance::new();
        assert_eq!(prov.len(), 0);

        let prov = prov.with_event("evt_001").with_patch("patch_001");
        assert_eq!(prov.len(), 2);
    }

    #[test]
    fn test_verification_new() {
        let verif = Verification::new();
        assert!(verif.last_verified_commit.is_none());
        assert_eq!(verif.status, VerificationStatus::Unknown);
        assert!(!verif.is_verified());
    }

    #[test]
    fn test_verification_verified() {
        let verif = Verification::verified("abc123");
        assert_eq!(verif.last_verified_commit, Some("abc123".to_string()));
        assert_eq!(verif.status, VerificationStatus::Verified);
        assert!(verif.is_verified());
    }

    #[test]
    fn test_verification_stale() {
        let verif = Verification::stale("abc123");
        assert_eq!(verif.last_verified_commit, Some("abc123".to_string()));
        assert_eq!(verif.status, VerificationStatus::Stale);
        assert!(!verif.is_verified());
    }

    #[test]
    fn test_verification_default() {
        let verif = Verification::default();
        assert!(verif.last_verified_commit.is_none());
        assert_eq!(verif.status, VerificationStatus::Unknown);
    }

    #[test]
    fn test_session_meta_new() {
        let meta = SessionMeta::new("session-123");
        assert_eq!(meta.id, "session-123");
        assert_eq!(meta.duration_minutes, 0);
        assert_eq!(meta.event_count, 0);
        assert_eq!(meta.files_modified, 0);
    }

    #[test]
    fn test_session_meta_with_duration() {
        let meta = SessionMeta::new("session-123").with_duration(45);
        assert_eq!(meta.duration_minutes, 45);
    }

    #[test]
    fn test_session_meta_with_event_count() {
        let meta = SessionMeta::new("session-123").with_event_count(150);
        assert_eq!(meta.event_count, 150);
    }

    #[test]
    fn test_session_meta_with_files_modified() {
        let meta = SessionMeta::new("session-123").with_files_modified(12);
        assert_eq!(meta.files_modified, 12);
    }

    #[test]
    fn test_session_meta_builder() {
        let meta = SessionMeta::new("session-123")
            .with_duration(65)
            .with_event_count(150)
            .with_files_modified(12);

        assert_eq!(meta.id, "session-123");
        assert_eq!(meta.duration_minutes, 65);
        assert_eq!(meta.event_count, 150);
        assert_eq!(meta.files_modified, 12);
    }

    #[test]
    fn test_session_meta_default() {
        let meta = SessionMeta::default();
        assert_eq!(meta.id, "");
        assert_eq!(meta.duration_minutes, 0);
        assert_eq!(meta.event_count, 0);
        assert_eq!(meta.files_modified, 0);
    }
}
