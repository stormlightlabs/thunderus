//! Hygiene checking and deduplication
//!
//! Enforces memory quality rules and prevents bloat.

use crate::MemoryPatchParams;
use crate::error::Result;
use crate::memory::document::MemoryDoc;
use crate::memory::gardener::config::{DeduplicationStrategy as Strategy, HygieneConfig, SizeLimits};
use crate::memory::kinds::MemoryKind;
use crate::memory::manifest::MemoryManifest;
use crate::memory::paths::MemoryPaths;

/// A hygiene violation
#[derive(Debug, Clone)]
pub struct HygieneViolation {
    pub rule: HygieneRule,
    pub severity: Severity,
    pub doc_id: String,
    pub message: String,
    pub suggested_fix: Option<String>,
}

/// Hygiene rule that was violated
#[derive(Debug, Clone, Copy)]
pub enum HygieneRule {
    /// Duplicate fact detected
    DuplicateFact,
    /// Core memory over size limit
    CoreOverSize,
    /// Document over size limit
    DocOverSize,
    /// Missing provenance links
    MissingProvenance,
    /// Orphaned document (no references)
    OrphanedDoc,
}

/// Severity level
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Warning,
    Error,
}

/// Validates memory hygiene rules
#[derive(Debug, Clone)]
pub struct HygieneChecker {
    config: HygieneConfig,
}

impl HygieneChecker {
    /// Create a new hygiene checker
    pub fn new(config: HygieneConfig) -> Self {
        Self { config }
    }

    /// Check all memory documents for hygiene violations
    pub fn check_all(&self, paths: &MemoryPaths, manifest: &MemoryManifest) -> Vec<HygieneViolation> {
        let mut violations = Vec::new();

        if let Ok(core) = self.load_core_memory(paths)
            && let Some(violation) = self.check_core_size(&core)
        {
            violations.push(violation);
        }

        for entry in &manifest.docs {
            if let Ok(doc) = self.load_document(paths, entry) {
                violations.extend(self.check_doc(&doc, paths));
            }
        }

        violations
    }

    /// Check a single document
    pub fn check_doc(&self, doc: &MemoryDoc, _paths: &MemoryPaths) -> Vec<HygieneViolation> {
        // TODO: Use paths to check for orphaned documents
        let mut violations = Vec::new();

        let limits = SizeLimits::new(self.config.doc_soft_limit, self.config.doc_hard_limit);
        let token_count = self.estimate_tokens(&doc.body);

        if limits.exceeds_hard(token_count) {
            violations.push(HygieneViolation {
                rule: HygieneRule::DocOverSize,
                severity: Severity::Error,
                doc_id: doc.frontmatter.id.clone(),
                message: format!("Document exceeds hard limit: {} tokens > {}", token_count, limits.hard),
                suggested_fix: Some(format!(
                    "Split document into multiple documents under {} tokens",
                    limits.soft
                )),
            });
        } else if limits.exceeds_soft(token_count) {
            violations.push(HygieneViolation {
                rule: HygieneRule::DocOverSize,
                severity: Severity::Warning,
                doc_id: doc.frontmatter.id.clone(),
                message: format!("Document exceeds soft limit: {} tokens > {}", token_count, limits.soft),
                suggested_fix: Some("Consider splitting document or moving less critical content".to_string()),
            });
        }

        if self.config.require_provenance && doc.frontmatter.provenance.events.is_empty() {
            violations.push(HygieneViolation {
                rule: HygieneRule::MissingProvenance,
                severity: Severity::Warning,
                doc_id: doc.frontmatter.id.clone(),
                message: "Document missing provenance links".to_string(),
                suggested_fix: Some("Add source event IDs to document frontmatter".to_string()),
            });
        }

        violations
    }

    /// Check core memory size
    fn check_core_size(&self, core: &str) -> Option<HygieneViolation> {
        let token_count = self.estimate_tokens(core);
        let limits = SizeLimits::new(self.config.core_soft_limit, self.config.core_hard_limit);

        if limits.exceeds_hard(token_count) {
            Some(HygieneViolation {
                rule: HygieneRule::CoreOverSize,
                severity: Severity::Error,
                doc_id: "CORE.md".to_string(),
                message: format!(
                    "Core memory exceeds hard limit: {} tokens > {}",
                    token_count, limits.hard
                ),
                suggested_fix: Some("Move verbose sections to FACTS/ or PLAYBOOKS/".to_string()),
            })
        } else if limits.exceeds_soft(token_count) {
            Some(HygieneViolation {
                rule: HygieneRule::CoreOverSize,
                severity: Severity::Warning,
                doc_id: "CORE.md".to_string(),
                message: format!(
                    "Core memory exceeds soft limit: {} tokens > {}",
                    token_count, limits.soft
                ),
                suggested_fix: Some("Consider moving some content to semantic memory".to_string()),
            })
        } else {
            None
        }
    }

    /// Estimate token count (rough approximation: ~4 chars per token)
    fn estimate_tokens(&self, text: &str) -> usize {
        text.chars().count() / 4
    }

    /// Load core memory content
    fn load_core_memory(&self, paths: &MemoryPaths) -> Result<String> {
        let core_path = paths.core.join("CORE.md");
        std::fs::read_to_string(&core_path).map_err(crate::error::Error::Io)
    }

    /// Load a document by manifest entry
    fn load_document(&self, paths: &MemoryPaths, entry: &crate::memory::manifest::ManifestEntry) -> Result<MemoryDoc> {
        let path = paths.root.join(&entry.path);
        let content = std::fs::read_to_string(&path).map_err(crate::error::Error::Io)?;
        MemoryDoc::parse(&content).map_err(|e| crate::error::Error::Parse(format!("Failed to parse: {}", e)))
    }
}

/// Detects and handles duplicate facts
#[derive(Debug, Clone)]
pub struct FactDeduplicator {
    // TODO: Use strategy in resolve() method for duplicate resolution
    #[allow(dead_code)]
    strategy: Strategy,
}

impl FactDeduplicator {
    /// Create a new deduplicator
    pub fn new(strategy: Strategy) -> Self {
        Self { strategy }
    }

    /// Find duplicate or near-duplicate facts
    pub fn find_duplicates(&self, manifest: &MemoryManifest, store: &super::MemoryStore) -> Vec<DuplicateGroup> {
        let mut groups = Vec::new();
        let mut processed: Vec<String> = Vec::new();

        for entry in manifest.docs.iter().filter(|e| e.kind == MemoryKind::Fact) {
            if processed.contains(&entry.id) {
                continue;
            }

            let content = match store.get_by_id(&entry.id, &MemoryKind::Fact) {
                Ok(Some(c)) => c,
                _ => continue,
            };

            let mut duplicates = Vec::new();
            for other_entry in manifest
                .docs
                .iter()
                .filter(|e| e.kind == MemoryKind::Fact && e.id != entry.id)
            {
                if processed.contains(&other_entry.id) {
                    continue;
                }

                if let Ok(Some(other_content)) = store.get_by_id(&other_entry.id, &MemoryKind::Fact) {
                    let similarity = self.compute_similarity(&content, &other_content);
                    if similarity > 0.8 {
                        duplicates.push((other_entry.id.clone(), similarity));
                    }
                }
            }

            if !duplicates.is_empty() {
                groups.push(DuplicateGroup {
                    canonical_id: entry.id.clone(),
                    duplicates: duplicates.clone().into_iter().map(|(id, _)| id).collect(),
                    similarity: 0.9,
                });
                processed.push(entry.id.clone());
                for (id, _) in duplicates {
                    processed.push(id);
                }
            }

            processed.push(entry.id.clone());
        }

        groups
    }

    /// Compute text similarity (simple word overlap)
    fn compute_similarity(&self, a: &str, b: &str) -> f64 {
        let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
        let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        intersection as f64 / union as f64
    }

    /// Generate patches to resolve duplicates
    pub fn resolve(&self, _groups: &[DuplicateGroup]) -> Vec<MemoryPatchParams> {
        // TODO: Implement duplicate resolution strategy
        Vec::new()
    }
}

/// A group of duplicate documents
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// The canonical document ID
    pub canonical_id: String,
    /// Duplicate document IDs
    pub duplicates: Vec<String>,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);
        let text = "Hello world this is a test";
        let tokens = checker.estimate_tokens(text);
        assert!(tokens > 5 && tokens < 15);
    }

    #[test]
    fn test_size_limits() {
        let limits = SizeLimits::new(100, 200);

        assert!(!limits.exceeds_soft(90));
        assert!(limits.exceeds_soft(150));
        assert!(!limits.exceeds_hard(150));
        assert!(limits.exceeds_hard(250));
    }

    #[test]
    fn test_fact_deduplicator() {
        let dedup = FactDeduplicator::new(Strategy::MergeToFirst);
        let content_a = "Run cargo build to compile the project";
        let content_b = "Use cargo build for compilation";
        let similarity = dedup.compute_similarity(content_a, content_b);
        assert!(similarity > 0.1);
    }

    #[test]
    fn test_fact_deduplicator_no_similarity() {
        let dedup = FactDeduplicator::new(Strategy::MergeToFirst);
        let content_a = "Run cargo build to compile";
        let content_b = "The weather is nice today";
        let similarity = dedup.compute_similarity(content_a, content_b);
        assert!(similarity < 0.3);
    }
}
