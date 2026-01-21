//! Memory directory structure and path resolution
//!
//! Provides deterministic paths for the tiered memory system within `.thunderus/memory/`.

use crate::error::Result;
use std::path::{Path, PathBuf};

/// Base directory name for Thunderus data (repo-local, versionable)
pub const THUNDERUS_DIR_NAME: &str = ".thunderus";

/// Subdirectory for memory data
pub const MEMORY_DIR: &str = "memory";

/// Subdirectory for core memory (always-loaded project knowledge)
pub const CORE_MEMORY_DIR: &str = "core";

/// Subdirectory for semantic memory (facts + decisions)
pub const SEMANTIC_MEMORY_DIR: &str = "semantic";

/// Subdirectory for semantic facts
pub const FACTS_DIR: &str = "FACTS";

/// Subdirectory for semantic decisions (ADR-lite)
pub const DECISIONS_DIR: &str = "DECISIONS";

/// Subdirectory for procedural memory (playbooks)
pub const PROCEDURAL_MEMORY_DIR: &str = "procedural";

/// Subdirectory for procedural playbooks
pub const PLAYBOOKS_DIR: &str = "PLAYBOOKS";

/// Subdirectory for episodic memory (session recaps)
pub const EPISODIC_MEMORY_DIR: &str = "episodic";

/// Subdirectory for memory indexes (manifests + search indexes)
pub const INDEXES_DIR: &str = "indexes";

/// Core memory filename (shared project memory)
pub const CORE_MEMORY_FILE: &str = "CORE.md";

/// Local core memory filename (user-specific, gitignored)
pub const CORE_LOCAL_MEMORY_FILE: &str = "CORE.local.md";

/// Manifest filename
pub const MANIFEST_FILE: &str = "manifest.json";

/// Tags index filename
pub const TAGS_FILE: &str = "tags.json";

/// Standard memory subdirectory structure
///
/// Provides deterministic paths for all memory directories and files.
#[derive(Debug, Clone)]
pub struct MemoryPaths {
    /// Root directory containing the `.thunderus/` folder
    pub root: PathBuf,
    /// `.thunderus/memory/`
    pub root_memory: PathBuf,
    /// `.thunderus/memory/core/`
    pub core: PathBuf,
    /// `.thunderus/memory/semantic/`
    pub semantic: PathBuf,
    /// `.thunderus/memory/semantic/FACTS/`
    pub facts: PathBuf,
    /// `.thunderus/memory/semantic/DECISIONS/`
    pub decisions: PathBuf,
    /// `.thunderus/memory/procedural/`
    pub procedural: PathBuf,
    /// `.thunderus/memory/procedural/PLAYBOOKS/`
    pub playbooks: PathBuf,
    /// `.thunderus/memory/episodic/`
    pub episodic: PathBuf,
    /// `.thunderus/memory/indexes/`
    pub indexes: PathBuf,
}

impl MemoryPaths {
    /// Create MemoryPaths from a repository root directory
    pub fn from_thunderus_root(root: &Path) -> Self {
        let root_memory = root.join(THUNDERUS_DIR_NAME).join(MEMORY_DIR);
        Self {
            root: root.to_path_buf(),
            root_memory: root_memory.clone(),
            core: root_memory.join(CORE_MEMORY_DIR),
            semantic: root_memory.join(SEMANTIC_MEMORY_DIR),
            facts: root_memory.join(SEMANTIC_MEMORY_DIR).join(FACTS_DIR),
            decisions: root_memory.join(SEMANTIC_MEMORY_DIR).join(DECISIONS_DIR),
            procedural: root_memory.join(PROCEDURAL_MEMORY_DIR),
            playbooks: root_memory.join(PROCEDURAL_MEMORY_DIR).join(PLAYBOOKS_DIR),
            episodic: root_memory.join(EPISODIC_MEMORY_DIR),
            indexes: root_memory.join(INDEXES_DIR),
        }
    }

    /// Get path to CORE.md file
    pub fn core_memory_file(&self) -> PathBuf {
        self.core.join(CORE_MEMORY_FILE)
    }

    /// Get path to CORE.local.md file
    pub fn core_local_memory_file(&self) -> PathBuf {
        self.core.join(CORE_LOCAL_MEMORY_FILE)
    }

    /// Get path to manifest.json file
    pub fn manifest_file(&self) -> PathBuf {
        self.indexes.join(MANIFEST_FILE)
    }

    /// Get path to tags.json file
    pub fn tags_file(&self) -> PathBuf {
        self.indexes.join(TAGS_FILE)
    }

    /// Get episodic directory for a specific year-month
    pub fn episodic_month_dir(&self, year_month: &str) -> PathBuf {
        self.episodic.join(year_month)
    }

    /// Ensure all memory directories exist
    ///
    /// Creates the directory structure if it doesn't exist.
    pub fn ensure(&self) -> Result<()> {
        std::fs::create_dir_all(&self.core)?;
        std::fs::create_dir_all(&self.facts)?;
        std::fs::create_dir_all(&self.decisions)?;
        std::fs::create_dir_all(&self.playbooks)?;
        std::fs::create_dir_all(&self.episodic)?;
        std::fs::create_dir_all(&self.indexes)?;
        Ok(())
    }

    /// Check if a path is within the memory directory
    pub fn is_memory_path(&self, path: &Path) -> bool {
        path.starts_with(&self.root_memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_memory_paths_from_root() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(paths.root, temp.path());
        assert_eq!(paths.root_memory, temp.path().join(".thunderus").join("memory"));
        assert_eq!(paths.core, temp.path().join(".thunderus").join("memory").join("core"));
        assert_eq!(
            paths.semantic,
            temp.path().join(".thunderus").join("memory").join("semantic")
        );
        assert_eq!(
            paths.facts,
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("semantic")
                .join("FACTS")
        );
        assert_eq!(
            paths.decisions,
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("semantic")
                .join("DECISIONS")
        );
        assert_eq!(
            paths.procedural,
            temp.path().join(".thunderus").join("memory").join("procedural")
        );
        assert_eq!(
            paths.playbooks,
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("procedural")
                .join("PLAYBOOKS")
        );
        assert_eq!(
            paths.episodic,
            temp.path().join(".thunderus").join("memory").join("episodic")
        );
        assert_eq!(
            paths.indexes,
            temp.path().join(".thunderus").join("memory").join("indexes")
        );
    }

    #[test]
    fn test_core_memory_file() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(
            paths.core_memory_file(),
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("core")
                .join("CORE.md")
        );
    }

    #[test]
    fn test_core_local_memory_file() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(
            paths.core_local_memory_file(),
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("core")
                .join("CORE.local.md")
        );
    }

    #[test]
    fn test_manifest_file() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(
            paths.manifest_file(),
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("indexes")
                .join("manifest.json")
        );
    }

    #[test]
    fn test_tags_file() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(
            paths.tags_file(),
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("indexes")
                .join("tags.json")
        );
    }

    #[test]
    fn test_episodic_month_dir() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert_eq!(
            paths.episodic_month_dir("2026-01"),
            temp.path()
                .join(".thunderus")
                .join("memory")
                .join("episodic")
                .join("2026-01")
        );
    }

    #[test]
    fn test_ensure_creates_directories() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        paths.ensure().unwrap();

        assert!(paths.core.exists());
        assert!(paths.facts.exists());
        assert!(paths.decisions.exists());
        assert!(paths.playbooks.exists());
        assert!(paths.episodic.exists());
        assert!(paths.indexes.exists());
    }

    #[test]
    fn test_is_memory_path() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());

        assert!(paths.is_memory_path(&paths.core));
        assert!(paths.is_memory_path(&paths.facts));
        assert!(paths.is_memory_path(&paths.decisions));

        let outside_path = temp.path().join("other");
        assert!(!paths.is_memory_path(&outside_path));
    }

    #[test]
    fn test_constants() {
        assert_eq!(THUNDERUS_DIR_NAME, ".thunderus");
        assert_eq!(MEMORY_DIR, "memory");
        assert_eq!(CORE_MEMORY_DIR, "core");
        assert_eq!(SEMANTIC_MEMORY_DIR, "semantic");
        assert_eq!(FACTS_DIR, "FACTS");
        assert_eq!(DECISIONS_DIR, "DECISIONS");
        assert_eq!(PROCEDURAL_MEMORY_DIR, "procedural");
        assert_eq!(PLAYBOOKS_DIR, "PLAYBOOKS");
        assert_eq!(EPISODIC_MEMORY_DIR, "episodic");
        assert_eq!(INDEXES_DIR, "indexes");
        assert_eq!(CORE_MEMORY_FILE, "CORE.md");
        assert_eq!(CORE_LOCAL_MEMORY_FILE, "CORE.local.md");
        assert_eq!(MANIFEST_FILE, "manifest.json");
        assert_eq!(TAGS_FILE, "tags.json");
    }
}
