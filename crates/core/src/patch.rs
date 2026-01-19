use crate::SessionId;
/// Patch queue module for diff-first editing
///
/// This module implements the patch queue system that enables reviewable,
/// reversible, and conflict-aware edits through a unified diff workflow.
use crate::session::PatchStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A unique identifier for a patch
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchId {
    id: String,
}

impl PatchId {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl std::fmt::Display for PatchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl PatchId {
    pub fn value(&self) -> String {
        self.id.clone()
    }
}

struct DiffResult(Vec<PathBuf>, HashMap<PathBuf, Vec<Hunk>>);

/// A single hunk within a unified diff
///
/// A hunk represents a contiguous section of changes in a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hunk {
    /// Line number in the original file where this hunk starts (1-indexed)
    pub old_start: usize,
    /// Number of lines in the original file
    pub old_lines: usize,
    /// Line number in the new file where this hunk starts (1-indexed)
    pub new_start: usize,
    /// Number of lines in the new file
    pub new_lines: usize,
    /// The hunk content (lines starting with ' ', '-', or '+')
    pub content: String,
    /// Optional semantic label describing the intent of this hunk
    pub intent: Option<String>,
    /// Whether this hunk is approved for application
    pub approved: bool,
}

impl Hunk {
    /// Parse a hunk from a unified diff header line
    ///
    /// Expected format: `@@ -old_start,old_lines +new_start,new_lines @@`
    pub fn parse_from_header(line: &str) -> Option<Self> {
        let line = line.trim();
        if !line.starts_with("@@") {
            return None;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

        let old_part = parts.get(1)?.strip_prefix('-')?;
        let new_part = parts.get(2)?.strip_prefix('+')?;

        let mut old_split = old_part.split(',');
        let mut new_split = new_part.split(',');

        let old_start = old_split.next()?.parse().ok()?;
        let old_lines = old_split.next().unwrap_or("1").parse().ok()?;
        let new_start = new_split.next()?.parse().ok()?;
        let new_lines = new_split.next().unwrap_or("1").parse().ok()?;

        Some(Hunk { old_start, old_lines, new_start, new_lines, content: String::new(), intent: None, approved: false })
    }

    /// Add content lines to this hunk
    pub fn with_content(mut self, content: String) -> Self {
        self.content = content;
        self
    }

    /// Add an intent label to this hunk
    pub fn with_intent(mut self, intent: String) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Approve this hunk for application
    pub fn approve(&mut self) {
        self.approved = true;
    }

    /// Reject this hunk
    pub fn reject(&mut self) {
        self.approved = false;
    }

    /// Get the hunk header line in unified diff format
    pub fn header(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start, self.old_lines, self.new_start, self.new_lines
        )
    }

    /// Parse hunk lines from content, returning (original_lines, new_lines)
    pub fn parse_lines(&self) -> (Vec<String>, Vec<String>) {
        let mut original = Vec::new();
        let mut new = Vec::new();

        for line in self.content.lines() {
            if let Some(rest) = line.strip_prefix('-') {
                original.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix('+') {
                new.push(rest.to_string());
            } else if let Some(rest) = line.strip_prefix(' ') {
                original.push(format!(" {}", rest));
                new.push(format!(" {}", rest));
            }
        }

        (original, new)
    }
}

/// A patch representing a unified diff for one or more files
///
/// Patches are the primary unit of change in the diff-first workflow.
/// They can be proposed, approved, applied, or rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Patch {
    /// Unique identifier for this patch
    pub id: PatchId,
    /// Human-readable name for this patch
    pub name: String,
    /// Current status of this patch
    pub status: PatchStatus,
    /// Files affected by this patch
    pub files: Vec<PathBuf>,
    /// Base snapshot ID (git commit hash) that this patch applies to
    pub base_snapshot: String,
    /// Unified diff content
    pub diff: String,
    /// Parsed hunks for each file
    ///
    /// Maps file path to list of hunks in that file
    pub hunks: HashMap<PathBuf, Vec<Hunk>>,
    /// Session ID that created this patch
    pub session_id: SessionId,
    /// Sequence number of the patch event in the session
    pub seq: u64,
    /// When the patch was created
    pub created_at: String,
}

impl Patch {
    /// Create a new patch from a unified diff
    pub fn new(
        id: PatchId, name: String, base_snapshot: String, diff: String, session_id: SessionId, seq: u64,
    ) -> Result<Self, String> {
        let DiffResult(files, hunks) = Self::parse_diff(&diff)?;

        let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        Ok(Patch {
            id,
            name,
            status: PatchStatus::Proposed,
            files,
            base_snapshot,
            diff,
            hunks,
            session_id,
            seq,
            created_at,
        })
    }

    /// Parse a unified diff and extract files and hunks
    fn parse_diff(diff: &str) -> Result<DiffResult, String> {
        let mut files = Vec::new();
        let mut hunks: HashMap<PathBuf, Vec<Hunk>> = HashMap::new();

        let mut current_file: Option<PathBuf> = None;
        let mut current_hunk: Option<Hunk> = None;
        let mut hunk_lines = Vec::new();

        for line in diff.lines() {
            if line.starts_with("diff --git") {
                if let (Some(file), Some(hunk)) = (&current_file, &current_hunk) {
                    hunks
                        .entry(file.clone())
                        .or_default()
                        .push(hunk.clone().with_content(hunk_lines.join("\n")));
                    hunk_lines.clear();
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let file_path = parts[3].strip_prefix("b/").unwrap_or(parts[3]);
                    current_file = Some(PathBuf::from(file_path));
                    if !files.contains(&current_file.as_ref().unwrap().clone()) {
                        files.push(current_file.clone().unwrap());
                    }
                }
            } else if line.starts_with("@@") {
                if let (Some(file), Some(hunk)) = (&current_file, &current_hunk) {
                    hunks
                        .entry(file.clone())
                        .or_default()
                        .push(hunk.clone().with_content(hunk_lines.join("\n")));
                }

                if let Some(parsed) = Hunk::parse_from_header(line) {
                    current_hunk = Some(parsed);
                    hunk_lines.clear();
                }
            } else if current_hunk.is_some()
                && (line.starts_with(' ') || line.starts_with('-') || line.starts_with('+'))
            {
                hunk_lines.push(line.to_string());
            }
        }

        if let (Some(file), Some(hunk)) = (&current_file, &current_hunk) {
            hunks
                .entry(file.clone())
                .or_default()
                .push(hunk.clone().with_content(hunk_lines.join("\n")));
        }

        Ok(DiffResult(files, hunks))
    }

    /// Approve this patch for application
    pub fn approve(&mut self) {
        self.status = PatchStatus::Approved;
    }

    /// Reject this patch
    pub fn reject(&mut self) {
        self.status = PatchStatus::Rejected;
    }

    /// Mark this patch as applied
    pub fn mark_applied(&mut self) {
        self.status = PatchStatus::Applied;
    }

    /// Mark this patch as failed
    pub fn mark_failed(&mut self) {
        self.status = PatchStatus::Failed;
    }

    /// Approve a specific hunk in a specific file
    pub fn approve_hunk(&mut self, file: &Path, hunk_index: usize) -> Result<(), String> {
        let hunks = self
            .hunks
            .get_mut(file)
            .ok_or_else(|| format!("File not found in patch: {}", file.display()))?;

        let hunk = hunks
            .get_mut(hunk_index)
            .ok_or_else(|| format!("Hunk index {} out of bounds for file {}", hunk_index, file.display()))?;

        hunk.approve();
        Ok(())
    }

    /// Reject a specific hunk in a specific file
    pub fn reject_hunk(&mut self, file: &Path, hunk_index: usize) -> Result<(), String> {
        let hunks = self
            .hunks
            .get_mut(file)
            .ok_or_else(|| format!("File not found in patch: {}", file.display()))?;

        let hunk = hunks
            .get_mut(hunk_index)
            .ok_or_else(|| format!("Hunk index {} out of bounds for file {}", hunk_index, file.display()))?;

        hunk.reject();
        Ok(())
    }

    /// Get all approved hunks for a file
    pub fn approved_hunks(&self, file: &Path) -> Vec<&Hunk> {
        self.hunks
            .get(file)
            .map(|hunks| hunks.iter().filter(|h| h.approved).collect())
            .unwrap_or_default()
    }

    /// Check if the patch has any approved hunks
    pub fn has_approved_hunks(&self) -> bool {
        self.hunks.values().flatten().any(|h| h.approved)
    }

    /// Set an intent label for a specific hunk
    pub fn set_hunk_intent(&mut self, file: &Path, hunk_index: usize, intent: String) -> Result<(), String> {
        let hunks = self
            .hunks
            .get_mut(file)
            .ok_or_else(|| format!("File not found in patch: {}", file.display()))?;

        let hunk = hunks
            .get_mut(hunk_index)
            .ok_or_else(|| format!("Hunk index {} out of bounds for file {}", hunk_index, file.display()))?;

        hunk.intent = Some(intent);
        Ok(())
    }

    /// Get the number of hunks in a specific file
    pub fn hunk_count(&self, file: &Path) -> Option<usize> {
        self.hunks.get(file).map(|h| h.len())
    }

    /// Get total number of hunks across all files
    pub fn total_hunk_count(&self) -> usize {
        self.hunks.values().map(|h| h.len()).sum()
    }
}

/// A queue of patches waiting to be applied
///
/// The patch queue manages the lifecycle of patches from proposal
/// through approval, application, and potential rollback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchQueue {
    /// Patches in the queue, ordered by creation time
    pub patches: Vec<Patch>,
    /// IDs of applied patches, in order of application
    pub applied_patches: Vec<PatchId>,
    /// Base snapshot (git commit) for the working directory
    pub base_snapshot: String,
}

impl PatchQueue {
    /// Create a new empty patch queue
    pub fn new(base_snapshot: String) -> Self {
        PatchQueue { patches: Vec::new(), applied_patches: Vec::new(), base_snapshot }
    }

    /// Add a patch to the queue
    pub fn add(&mut self, patch: Patch) {
        self.patches.push(patch);
    }

    /// Remove a patch from the queue
    pub fn remove(&mut self, patch_id: &PatchId) -> Option<Patch> {
        if let Some(pos) = self.patches.iter().position(|p| &p.id == patch_id) {
            Some(self.patches.remove(pos))
        } else {
            None
        }
    }

    /// Get a patch by ID
    pub fn get(&self, patch_id: &PatchId) -> Option<&Patch> {
        self.patches.iter().find(|p| &p.id == patch_id)
    }

    /// Get a mutable reference to a patch by ID
    pub fn get_mut(&mut self, patch_id: &PatchId) -> Option<&mut Patch> {
        self.patches.iter_mut().find(|p| &p.id == patch_id)
    }

    /// Get all patches in a specific status
    pub fn by_status(&self, status: PatchStatus) -> Vec<&Patch> {
        self.patches.iter().filter(|p| p.status == status).collect()
    }

    /// Mark a patch as applied
    pub fn mark_applied(&mut self, patch_id: &PatchId) -> Result<(), String> {
        let patch = self
            .get_mut(patch_id)
            .ok_or_else(|| format!("Patch not found: {}", patch_id.id))?;

        patch.mark_applied();
        self.applied_patches.push(patch_id.clone());

        Ok(())
    }

    /// Get the last applied patch
    pub fn last_applied(&self) -> Option<&Patch> {
        self.applied_patches.last().and_then(|id| self.get(id))
    }

    /// Rollback the last applied patch
    pub fn rollback_last(&mut self) -> Result<PatchId, String> {
        let patch_id = self
            .applied_patches
            .pop()
            .ok_or_else(|| "No patches to rollback".to_string())?;

        let patch = self
            .get_mut(&patch_id)
            .ok_or_else(|| format!("Patch not found: {}", patch_id.id))?;

        patch.status = PatchStatus::Proposed;

        Ok(patch_id)
    }

    /// Get pending patches (proposed or approved)
    pub fn pending(&self) -> Vec<&Patch> {
        self.patches
            .iter()
            .filter(|p| matches!(p.status, PatchStatus::Proposed | PatchStatus::Approved))
            .collect()
    }

    /// Get failed patches
    pub fn failed(&self) -> Vec<&Patch> {
        self.by_status(PatchStatus::Failed)
    }

    /// Check if there are any pending patches
    pub fn has_pending(&self) -> bool {
        !self.pending().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SessionId;

    #[test]
    fn test_hunk_parse_from_header() {
        let line = "@@ -1,4 +1,5 @@";
        let hunk = Hunk::parse_from_header(line).unwrap();

        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 4);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 5);
    }

    #[test]
    fn test_hunk_with_single_line() {
        let line = "@@ -1 +1,2 @@";
        let hunk = Hunk::parse_from_header(line).unwrap();

        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_lines, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_lines, 2);
    }

    #[test]
    fn test_hunk_header() {
        let hunk = Hunk {
            old_start: 10,
            old_lines: 5,
            new_start: 10,
            new_lines: 6,
            content: String::new(),
            intent: None,
            approved: false,
        };

        assert_eq!(hunk.header(), "@@ -10,5 +10,6 @@");
    }

    #[test]
    fn test_hunk_approve_reject() {
        let mut hunk = Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            content: String::new(),
            intent: None,
            approved: false,
        };

        assert!(!hunk.approved);
        hunk.approve();
        assert!(hunk.approved);
        hunk.reject();
        assert!(!hunk.approved);
    }

    #[test]
    fn test_hunk_with_intent() {
        let hunk = Hunk {
            old_start: 1,
            old_lines: 1,
            new_start: 1,
            new_lines: 1,
            content: String::new(),
            intent: None,
            approved: false,
        };

        let hunk = hunk.with_intent("Add error handling".to_string());
        assert_eq!(hunk.intent, Some("Add error handling".to_string()));
    }

    #[test]
    fn test_hunk_parse_lines() {
        let hunk = Hunk {
            old_start: 1,
            old_lines: 3,
            new_start: 1,
            new_lines: 3,
            content: " line1\n-line2\n+line2_new\n line3".to_string(),
            intent: None,
            approved: false,
        };

        let (original, new) = hunk.parse_lines();

        assert_eq!(original, vec![" line1", "line2", " line3"]);
        assert_eq!(new, vec![" line1", "line2_new", " line3"]);
    }

    #[test]
    fn test_patch_new() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        assert_eq!(patch.id, PatchId::new("patch1"));
        assert_eq!(patch.status, PatchStatus::Proposed);
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0], PathBuf::from("test.txt"));
    }

    #[test]
    fn test_patch_approve_reject() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let mut patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        patch.approve();
        assert_eq!(patch.status, PatchStatus::Approved);

        patch.reject();
        assert_eq!(patch.status, PatchStatus::Rejected);
    }

    #[test]
    fn test_patch_queue_new() {
        let queue = PatchQueue::new("base123".to_string());

        assert!(queue.patches.is_empty());
        assert!(queue.applied_patches.is_empty());
        assert_eq!(queue.base_snapshot, "base123");
    }

    #[test]
    fn test_patch_queue_add_remove() {
        let mut queue = PatchQueue::new("base123".to_string());

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        queue.add(patch.clone());

        assert_eq!(queue.patches.len(), 1);
        assert_eq!(queue.get(&PatchId::new("patch1")).unwrap().id, PatchId::new("patch1"));

        let removed = queue.remove(&PatchId::new("patch1"));
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, PatchId::new("patch1"));
        assert!(queue.get(&PatchId::new("patch1")).is_none());
    }

    #[test]
    fn test_patch_queue_mark_applied() {
        let mut queue = PatchQueue::new("base123".to_string());

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        queue.add(patch);
        queue.mark_applied(&PatchId::new("patch1")).unwrap();

        assert_eq!(queue.applied_patches.len(), 1);
        assert_eq!(queue.applied_patches[0], PatchId::new("patch1"));
        assert_eq!(queue.get(&PatchId::new("patch1")).unwrap().status, PatchStatus::Applied);
    }

    #[test]
    fn test_patch_queue_rollback() {
        let mut queue = PatchQueue::new("base123".to_string());

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        queue.add(patch);
        queue.mark_applied(&PatchId::new("patch1")).unwrap();

        let rolled_back = queue.rollback_last().unwrap();
        assert_eq!(rolled_back, PatchId::new("patch1"));
        assert_eq!(queue.applied_patches.len(), 0);
        assert_eq!(
            queue.get(&PatchId::new("patch1")).unwrap().status,
            PatchStatus::Proposed
        );
    }

    #[test]
    fn test_patch_queue_pending() {
        let mut queue = PatchQueue::new("base123".to_string());

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let session_id = SessionId::new();

        let patch1 = Patch::new(
            PatchId::new("patch1"),
            "test patch 1".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id.clone(),
            0,
        )
        .unwrap();

        let mut patch2 = Patch::new(
            PatchId::new("patch2"),
            "test patch 2".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            1,
        )
        .unwrap();

        patch2.approve();

        queue.add(patch1);
        queue.add(patch2);

        let pending = queue.pending();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_patch_approve_hunk() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,2 +1,2 @@\n line1\n-old\n+new";
        let session_id = SessionId::new();
        let mut patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        let file = PathBuf::from("test.txt");
        assert!(patch.hunk_count(&file).is_some());

        patch.approve_hunk(&file, 0).unwrap();

        let hunks = patch.hunks.get(&file).unwrap();
        assert!(hunks[0].approved);
    }

    #[test]
    fn test_patch_set_hunk_intent() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,2 +1,2 @@\n line1\n-old\n+new";
        let session_id = SessionId::new();
        let mut patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        let file = PathBuf::from("test.txt");
        patch
            .set_hunk_intent(&file, 0, "Fix variable name".to_string())
            .unwrap();

        let hunks = patch.hunks.get(&file).unwrap();
        assert_eq!(hunks[0].intent, Some("Fix variable name".to_string()));
    }

    #[test]
    fn test_patch_parse_diff_with_multiple_hunks() {
        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,2 +1,2 @@\n-old1\n+new1\n@@ -5,2 +5,2 @@\n-old2\n+new2";
        let session_id = SessionId::new();
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        assert_eq!(patch.total_hunk_count(), 2);
    }
}
