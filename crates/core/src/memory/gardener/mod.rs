//! Memory Gardener for consolidation, hygiene, and drift detection
//!
//! The gardener transforms raw session history into durable knowledge artifacts,
//! enforces memory hygiene rules, and detects stale documents.

mod config;
mod consolidation;
mod drift;
mod entities;
mod extraction;
mod hygiene;
mod recap;

pub use config::{
    DeduplicationStrategy, DriftConfig, ExtractionConfig, GardenerConfig, HygieneConfig, RecapConfig, SizeLimits,
};
pub use consolidation::{ConsolidationJob, ConsolidationResult, FactUpdate};
pub use drift::{DriftDetector, DriftResult, StalenessInfo, StalenessSeverity};
pub use entities::{
    AdrUpdate, CommandEntity, CommandOutcome, DecisionEntity, GotchaCategory, GotchaEntity, WorkflowEntity,
    WorkflowStep,
};
pub use extraction::{EntityExtractor, ExtractedEntities};
pub use hygiene::{DuplicateGroup, FactDeduplicator, HygieneChecker, HygieneRule, HygieneViolation, Severity};
pub use recap::{RecapGenerator, RecapResult, RecapStats, RecapTemplate};

use crate::error::{Error, Result};
use crate::memory::paths::MemoryPaths;

/// Main entry point for the memory gardener
///
/// The gardener orchestrates consolidation, hygiene checks, and drift detection.
#[derive(Debug, Clone)]
pub struct Gardener {
    /// Memory directory paths
    paths: MemoryPaths,
    /// Gardener configuration
    config: GardenerConfig,
}

impl Gardener {
    /// Create a new gardener with default configuration
    pub fn new(paths: MemoryPaths) -> Self {
        Self { paths, config: GardenerConfig::default() }
    }

    /// Create a new gardener with custom configuration
    pub fn with_config(paths: MemoryPaths, config: GardenerConfig) -> Self {
        Self { paths, config }
    }

    /// Run consolidation on a completed session
    ///
    /// This extracts entities from the session events and generates
    /// memory updates (facts, ADRs, playbooks) for user approval.
    pub async fn consolidate_session(
        &self, session_id: &str, events_file: &std::path::Path,
    ) -> Result<ConsolidationResult> {
        let job = ConsolidationJob::new(session_id, events_file, self.config.clone())?;
        job.run(&self.paths).await
    }

    /// Run hygiene checks on all memory documents
    ///
    /// Returns violations that need to be resolved.
    pub fn check_hygiene(&self) -> Result<Vec<HygieneViolation>> {
        let checker = HygieneChecker::new(self.config.hygiene.clone());
        let manifest = self.load_manifest()?;
        Ok(checker.check_all(&self.paths, &manifest))
    }

    /// Check for drift between memory and repository
    ///
    /// Returns information about stale documents.
    pub fn check_drift(&self, repo: &git2::Repository) -> Result<DriftResult> {
        let detector = DriftDetector::new(repo)?;
        let manifest = self.load_manifest()?;
        detector.check_all(&manifest)
    }

    /// Check for drift between memory and repository (opens repo internally)
    ///
    /// Returns information about stale documents.
    pub fn check_drift_auto(&self) -> Result<DriftResult> {
        let repo = git2::Repository::discover(&self.paths.root)
            .map_err(|e| Error::Other(format!("Failed to open git repo: {}", e)))?;
        self.check_drift(&repo)
    }

    /// Verify a document at the current commit
    ///
    /// Marks the document as verified at the current HEAD commit.
    pub fn verify_document(&self, doc_id: &str) -> Result<()> {
        let repo = git2::Repository::discover(&self.paths.root)
            .map_err(|e| Error::Other(format!("Failed to open git repo: {}", e)))?;
        let head = repo
            .head()
            .map_err(|e| Error::Other(format!("Failed to get HEAD: {}", e)))?;
        let commit = head
            .peel_to_commit()
            .map_err(|e| Error::Other(format!("Failed to get commit: {}", e)))?;
        let commit_id = commit.id().to_string();

        let manifest = self.load_manifest()?;
        if let Some(entry) = manifest.docs.iter().find(|e| e.id == doc_id) {
            let path = self.paths.root.join(&entry.path);
            let content = std::fs::read_to_string(&path).map_err(Error::Io)?;
            let mut doc = crate::memory::document::MemoryDoc::parse(&content)
                .map_err(|e| Error::Parse(format!("Failed to parse document: {}", e)))?;

            doc.frontmatter.verification.last_verified_commit = Some(commit_id.clone());
            doc.frontmatter.verification.status = crate::memory::kinds::VerificationStatus::Verified;

            let new_content = format!("{}", doc);
            std::fs::write(&path, new_content).map_err(Error::Io)?;
            Ok(())
        } else {
            Err(Error::Other(format!("Document not found: {}", doc_id)))
        }
    }

    /// Find duplicate facts across memory
    ///
    /// Returns groups of similar or duplicate facts.
    pub fn find_duplicates(&self) -> Result<Vec<DuplicateGroup>> {
        let manifest = self.load_manifest()?;
        let dedup = FactDeduplicator::new(self.config.hygiene.dedup_strategy);
        let store = self.create_memory_store()?;
        Ok(dedup.find_duplicates(&manifest, &store))
    }

    /// Generate a session recap
    ///
    /// Creates a human-readable summary of session activity.
    pub async fn generate_recap(
        &self, session_id: &str, events: &[crate::session::LoggedEvent], entities: &ExtractedEntities,
    ) -> Result<RecapResult> {
        let generator = RecapGenerator::new(self.config.recap.clone());
        generator.generate(session_id, events, entities, &[])
    }

    /// Load the memory manifest
    fn load_manifest(&self) -> Result<crate::memory::MemoryManifest> {
        use crate::memory::MemoryManifest;

        if !self.paths.manifest_file().exists() {
            return Ok(MemoryManifest::default());
        }

        let content = std::fs::read_to_string(self.paths.manifest_file()).map_err(crate::error::Error::Io)?;
        let manifest: MemoryManifest = serde_json::from_str(&content)
            .map_err(|e| crate::error::Error::Parse(format!("Failed to parse manifest: {}", e)))?;
        Ok(manifest)
    }

    /// Create a simple in-memory store for deduplication
    fn create_memory_store(&self) -> Result<MemoryStore> {
        Ok(MemoryStore::new(self.paths.clone()))
    }
}

/// Simple in-memory store for deduplication
///
/// This provides read access to memory documents for duplicate detection.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    paths: MemoryPaths,
}

impl MemoryStore {
    pub fn new(paths: MemoryPaths) -> Self {
        Self { paths }
    }

    /// Read a document's content
    pub fn read_document(&self, path: &std::path::Path) -> Result<String> {
        std::fs::read_to_string(path).map_err(crate::error::Error::Io)
    }

    /// Get document content by ID
    pub fn get_by_id(&self, doc_id: &str, kind: &crate::memory::MemoryKind) -> Result<Option<String>> {
        use crate::memory::MemoryKind;

        let path = match kind {
            MemoryKind::Fact => {
                let filename = format!("{}.md", doc_id.replace('.', "_"));
                self.paths.facts.join(filename)
            }
            MemoryKind::Adr => {
                if let Some(seq_str) = doc_id.strip_prefix("adr.") {
                    if let Ok(seq) = seq_str.parse::<u32>() {
                        let filename = format!("ADR-{:04}.md", seq);
                        self.paths.decisions.join(filename)
                    } else {
                        return Ok(None);
                    }
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };

        if !path.exists() {
            return Ok(None);
        }

        self.read_document(&path).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_gardener() -> (TempDir, Gardener) {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();
        let gardener = Gardener::new(paths);
        (temp, gardener)
    }

    #[test]
    fn test_gardener_new() {
        let (_temp, gardener) = create_test_gardener();
        assert_eq!(gardener.config, GardenerConfig::default());
    }

    #[test]
    fn test_gardener_with_config() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let config = GardenerConfig {
            auto_consolidate: false,
            hygiene_on_change: false,
            drift_check_on_start: false,
            ..Default::default()
        };

        let gardener = Gardener::with_config(paths, config);
        assert!(!gardener.config.auto_consolidate);
        assert!(!gardener.config.hygiene_on_change);
    }

    #[test]
    fn test_memory_store_read_document() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let test_file = paths.facts.join("test_fact.md");
        std::fs::write(&test_file, "# Test Content\n\nSome content here.").unwrap();

        let store = MemoryStore::new(paths);
        let content = store.read_document(&test_file).unwrap();
        assert!(content.contains("Test Content"));
    }

    #[test]
    fn test_memory_store_get_by_id_fact() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let filename = "fact_test_commands.md";
        let test_file = paths.facts.join(filename);
        std::fs::write(&test_file, "# Commands\n\nTest content.").unwrap();

        let store = MemoryStore::new(paths);
        let content = store
            .get_by_id("fact.test.commands", &crate::memory::MemoryKind::Fact)
            .unwrap();
        assert!(content.is_some());
        assert!(content.unwrap().contains("Commands"));
    }

    #[test]
    fn test_memory_store_get_by_id_adr() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let filename = "ADR-0001.md";
        let test_file = paths.decisions.join(filename);
        std::fs::write(&test_file, "# Test ADR\n\nContent.").unwrap();

        let store = MemoryStore::new(paths);
        let content = store.get_by_id("adr.0001", &crate::memory::MemoryKind::Adr).unwrap();
        assert!(content.is_some());
        assert!(content.unwrap().contains("Test ADR"));
    }

    #[test]
    fn test_memory_store_get_by_id_not_found() {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let store = MemoryStore::new(paths);
        let content = store
            .get_by_id("fact.nonexistent", &crate::memory::MemoryKind::Fact)
            .unwrap();
        assert!(content.is_none());
    }
}
