//! Patch queue manager for state management
//!
//! This module provides the integration layer between the patch queue, session management,
//! and the approval system.
use crate::error::{Error, Result};
use crate::layout::{AgentDir, SessionId};
use crate::patch::{MemoryPatch, MemoryPatchParams, Patch, PatchId, PatchQueue};

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// Manager for patch queue state
///
/// The manager persists patch state to disk and provides methods for querying
/// and manipulating the queue.
#[derive(Debug, Clone)]
pub struct PatchQueueManager {
    /// Session associated with this patch queue
    session_id: SessionId,
    /// Agent directory layout
    agent_dir: AgentDir,
    /// In-memory patch queue
    queue: PatchQueue,
    /// Counter for generating unique patch IDs
    id_counter: u64,
}

impl PatchQueueManager {
    /// Create a new patch queue manager for a session
    pub fn new(session_id: SessionId, agent_dir: AgentDir) -> Self {
        let queue = PatchQueue::new("HEAD".to_string());

        Self { session_id, agent_dir, queue, id_counter: 0 }
    }

    /// Load or create a patch queue for a session
    pub fn load(mut self) -> Result<Self> {
        let queue_file = self.queue_file();

        if queue_file.exists() {
            let content = fs::read_to_string(&queue_file)
                .map_err(|e| Error::Other(format!("Failed to read patch queue: {}", e)))?;

            self.queue = serde_json::from_str(&content)
                .map_err(|e| Error::Parse(format!("Failed to parse patch queue: {}", e)))?;
        }

        Ok(self)
    }

    /// Get the patch queue file path
    fn queue_file(&self) -> PathBuf {
        self.agent_dir.session_dir(&self.session_id).join("patch_queue.json")
    }

    /// Save the patch queue to disk
    pub fn save(&self) -> Result<()> {
        let queue_file = self.queue_file();

        if let Some(parent) = queue_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("Failed to create patch queue directory: {}", e)))?;
        }

        let file =
            File::create(&queue_file).map_err(|e| Error::Other(format!("Failed to create patch queue file: {}", e)))?;

        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.queue)
            .map_err(|e| Error::Parse(format!("Failed to serialize patch queue: {}", e)))?;

        Ok(())
    }

    /// Add a new patch to the queue
    pub fn add_patch(&mut self, patch: Patch) -> Result<()> {
        self.queue.add(patch);
        self.save()
    }

    /// Remove a patch from the queue
    pub fn remove_patch(&mut self, patch_id: &PatchId) -> Result<Option<Patch>> {
        let patch = self.queue.remove(patch_id);
        self.save()?;
        Ok(patch)
    }

    /// Get a patch by ID
    pub fn get_patch(&self, patch_id: &PatchId) -> Option<&Patch> {
        self.queue.get(patch_id)
    }

    /// Get a mutable reference to a patch by ID
    pub fn get_patch_mut(&mut self, patch_id: &PatchId) -> Option<&mut Patch> {
        self.queue.get_mut(patch_id)
    }

    /// Get all patches
    pub fn patches(&self) -> &[Patch] {
        &self.queue.patches
    }

    /// Get pending patches
    pub fn pending_patches(&self) -> Vec<&Patch> {
        self.queue.pending()
    }

    /// Get failed patches
    pub fn failed_patches(&self) -> Vec<&Patch> {
        self.queue.failed()
    }

    /// Approve a patch
    pub fn approve_patch(&mut self, patch_id: &PatchId) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| crate::Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch.approve();
        self.save()
    }

    /// Reject a patch
    pub fn reject_patch(&mut self, patch_id: &PatchId) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| crate::Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch.reject();
        self.save()
    }

    /// Approve a specific hunk in a patch
    pub fn approve_hunk(&mut self, patch_id: &PatchId, file: &Path, hunk_index: usize) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch.approve_hunk(file, hunk_index).map_err(Error::Validation)?;
        self.save()
    }

    /// Reject a specific hunk in a patch
    pub fn reject_hunk(&mut self, patch_id: &PatchId, file: &Path, hunk_index: usize) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch.reject_hunk(file, hunk_index).map_err(Error::Validation)?;
        self.save()
    }

    /// Set an intent label for a hunk
    pub fn set_hunk_intent(
        &mut self, patch_id: &PatchId, file: &Path, hunk_index: usize, intent: String,
    ) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch
            .set_hunk_intent(file, hunk_index, intent)
            .map_err(Error::Validation)?;
        self.save()
    }

    /// Mark a patch as applied
    pub fn mark_applied(&mut self, patch_id: &PatchId) -> Result<()> {
        self.queue
            .mark_applied(patch_id)
            .map_err(|e| Error::Validation(e.clone()))?;
        self.save()
    }

    /// Mark a patch as failed
    pub fn mark_failed(&mut self, patch_id: &PatchId) -> Result<()> {
        let patch = self
            .queue
            .get_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Patch not found: {}", patch_id)))?;

        patch.mark_failed();
        self.save()
    }

    /// Rollback the last applied patch
    pub fn rollback_last(&mut self) -> Result<PatchId> {
        let patch_id = self.queue.rollback_last().map_err(Error::Validation)?;
        self.save()?;
        Ok(patch_id)
    }

    /// Get the last applied patch
    pub fn last_applied(&self) -> Option<&Patch> {
        self.queue.last_applied()
    }

    /// Check if there are pending patches
    pub fn has_pending(&self) -> bool {
        self.queue.has_pending()
    }

    /// Update the base snapshot (git commit) for the queue
    pub fn update_base_snapshot(&mut self, base_snapshot: String) -> Result<()> {
        self.queue.base_snapshot = base_snapshot;
        self.save()
    }

    /// Generate a unique patch ID
    pub fn generate_patch_id(&mut self) -> PatchId {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        let id = PatchId::new(format!("patch_{}_{}", timestamp, self.id_counter));
        self.id_counter += 1;
        id
    }

    /// Queue a memory update for approval
    ///
    /// Creates a memory patch from the provided information and adds it to the queue.
    /// The patch must be approved before it can be applied.
    pub fn queue_memory_update(&mut self, params: MemoryPatchParams) -> Result<PatchId> {
        let patch_id = self.generate_patch_id();
        let patch = MemoryPatch::new(patch_id.clone(), params);

        self.queue.add_memory_patch(patch);
        self.save()?;
        Ok(patch_id)
    }

    /// Get a memory patch by ID
    pub fn get_memory_patch(&self, patch_id: &PatchId) -> Option<&MemoryPatch> {
        self.queue.get_memory_patch(patch_id)
    }

    /// Get a mutable reference to a memory patch by ID
    pub fn get_memory_patch_mut(&mut self, patch_id: &PatchId) -> Option<&mut MemoryPatch> {
        self.queue.get_memory_patch_mut(patch_id)
    }

    /// Get all memory patches
    pub fn memory_patches(&self) -> Vec<&MemoryPatch> {
        self.queue.get_all_memory_patches()
    }

    /// Get pending memory patches
    pub fn pending_memory_patches(&self) -> Vec<&MemoryPatch> {
        self.queue.pending_memory_patches()
    }

    /// Approve a memory patch
    pub fn approve_memory_patch(&mut self, patch_id: &PatchId) -> Result<()> {
        let patch = self
            .queue
            .get_memory_patch_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Memory patch not found: {}", patch_id)))?;

        patch.approve();
        self.save()
    }

    /// Reject a memory patch
    pub fn reject_memory_patch(&mut self, patch_id: &PatchId) -> Result<()> {
        let patch = self
            .queue
            .get_memory_patch_mut(patch_id)
            .ok_or_else(|| Error::Validation(format!("Memory patch not found: {}", patch_id)))?;

        patch.reject();
        self.save()
    }

    /// Mark a memory patch as applied
    pub fn mark_memory_patch_applied(&mut self, patch_id: &PatchId) -> Result<()> {
        self.queue
            .mark_memory_patch_applied(patch_id)
            .map_err(Error::Validation)?;
        self.save()
    }

    /// Remove a memory patch from the queue
    pub fn remove_memory_patch(&mut self, patch_id: &PatchId) -> Result<Option<MemoryPatch>> {
        let patch = self.queue.remove_memory_patch(patch_id);
        self.save()?;
        Ok(patch)
    }

    /// Check if there are pending memory patches
    pub fn has_pending_memory_patches(&self) -> bool {
        self.queue.has_pending_memory_patches()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::PatchStatus;
    use tempfile::TempDir;

    #[test]
    fn test_patch_queue_manager_new() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session_id = SessionId::new();

        let manager = PatchQueueManager::new(session_id, agent_dir);

        assert_eq!(manager.patches().len(), 0);
        assert!(!manager.has_pending());
    }

    #[test]
    fn test_patch_queue_manager_add_remove() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session_id = SessionId::new();

        let mut manager = PatchQueueManager::new(session_id.clone(), agent_dir);

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        manager.add_patch(patch).unwrap();

        assert_eq!(manager.patches().len(), 1);
        assert!(manager.has_pending());

        let removed = manager.remove_patch(&PatchId::new("patch1")).unwrap();

        assert!(removed.is_some());
        assert_eq!(manager.patches().len(), 0);
    }

    #[test]
    fn test_patch_queue_manager_approve_reject() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session_id = SessionId::new();

        let mut manager = PatchQueueManager::new(session_id.clone(), agent_dir);

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";
        let patch = Patch::new(
            PatchId::new("patch1"),
            "test patch".to_string(),
            "abc123".to_string(),
            diff.to_string(),
            session_id,
            0,
        )
        .unwrap();

        manager.add_patch(patch).unwrap();

        manager.approve_patch(&PatchId::new("patch1")).unwrap();

        assert_eq!(
            manager.get_patch(&PatchId::new("patch1")).unwrap().status,
            PatchStatus::Approved
        );

        manager.reject_patch(&PatchId::new("patch1")).unwrap();

        assert_eq!(
            manager.get_patch(&PatchId::new("patch1")).unwrap().status,
            PatchStatus::Rejected
        );
    }

    #[test]
    fn test_patch_queue_manager_persistence() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session_id = SessionId::new();

        let diff = "diff --git a/test.txt b/test.txt\n@@ -1,1 +1,1 @@\n-old\n+new";

        {
            let mut manager = PatchQueueManager::new(session_id.clone(), agent_dir.clone());

            let patch = Patch::new(
                PatchId::new("patch1"),
                "test patch".to_string(),
                "abc123".to_string(),
                diff.to_string(),
                session_id.clone(),
                0,
            )
            .unwrap();

            manager.add_patch(patch).unwrap();
        }

        let manager2 = PatchQueueManager::new(session_id.clone(), agent_dir).load().unwrap();

        assert_eq!(manager2.patches().len(), 1);
        assert_eq!(manager2.get_patch(&PatchId::new("patch1")).unwrap().name, "test patch");
    }

    #[test]
    fn test_patch_queue_manager_generate_id() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session_id = SessionId::new();

        let mut manager = PatchQueueManager::new(session_id, agent_dir);

        let id1 = manager.generate_patch_id().value();
        let id2 = manager.generate_patch_id().value();

        assert!(id1.starts_with("patch_"));
        assert!(id2.starts_with("patch_"));
        assert_ne!(id1, id2);
    }
}
