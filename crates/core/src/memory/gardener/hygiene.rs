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
use chrono::Utc;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

        let mut docs: Vec<MemoryDoc> = Vec::new();
        for entry in &manifest.docs {
            if let Ok(doc) = self.load_document(paths, entry) {
                violations.extend(self.check_doc(&doc, paths));
                docs.push(doc);
            }
        }

        let references = self.collect_all_references(&docs);
        for doc in &docs {
            if let Some(violation) = self.check_orphaned(doc, &references, manifest) {
                violations.push(violation);
            }
        }

        violations
    }

    /// Check a single document
    pub fn check_doc(&self, doc: &MemoryDoc, paths: &MemoryPaths) -> Vec<HygieneViolation> {
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

        let expected_path = paths
            .root
            .join(doc.frontmatter.kind.dir_name())
            .join(format!("{}.md", doc.frontmatter.id.replace('.', "_")));
        if !expected_path.exists() {
            violations.push(HygieneViolation {
                rule: HygieneRule::OrphanedDoc,
                severity: Severity::Error,
                doc_id: doc.frontmatter.id.clone(),
                message: format!("Document file missing from filesystem: {:?}", expected_path),
                suggested_fix: Some("Recreate document file or remove from manifest".to_string()),
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

    /// Collect all document references from all documents
    ///
    /// Returns a set of document IDs and paths that are referenced anywhere.
    fn collect_all_references(&self, docs: &[MemoryDoc]) -> std::collections::HashSet<String> {
        let mut references = std::collections::HashSet::new();

        references.insert("CORE".to_string());
        references.insert("CORE.local".to_string());

        let link_pattern = regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();
        let mention_pattern = regex::Regex::new(r"@([a-z]+\.[a-z.]+)").unwrap();
        let adr_pattern = regex::Regex::new(r"ADR-(\d{4})").unwrap();

        for doc in docs {
            for cap in link_pattern.captures_iter(&doc.body) {
                if let Some(target) = cap.get(2) {
                    let target = target.as_str().to_string();

                    if target.contains(".md") {
                        let id = target
                            .trim_start_matches("../")
                            .trim_start_matches("semantic/")
                            .trim_start_matches("facts/")
                            .trim_start_matches("decisions/")
                            .trim_start_matches("playbooks/")
                            .trim_end_matches(".md")
                            .replace('_', ".");
                        references.insert(id);
                    }
                }
            }

            for cap in mention_pattern.captures_iter(&doc.body) {
                if let Some(mention) = cap.get(1) {
                    references.insert(mention.as_str().to_string());
                }
            }

            for cap in adr_pattern.captures_iter(&doc.body) {
                if let Some(num) = cap.get(1) {
                    references.insert(format!("adr.{}", num.as_str()));
                }
            }
        }

        references
    }

    /// Check if a document is orphaned (not referenced by any other document)
    ///
    /// Core documents and recently created documents are never considered orphaned.
    fn check_orphaned(
        &self, doc: &MemoryDoc, references: &std::collections::HashSet<String>, manifest: &MemoryManifest,
    ) -> Option<HygieneViolation> {
        if doc.frontmatter.id == "CORE" || doc.frontmatter.id == "CORE.local" {
            return None;
        }

        let days_old = Utc::now().signed_duration_since(doc.frontmatter.created).num_days();
        if days_old < 7 {
            return None;
        }

        if matches!(doc.frontmatter.kind, MemoryKind::Adr) {
            return None;
        }

        if !references.contains(&doc.frontmatter.id) {
            let is_tagged = manifest.docs.iter().any(|other| {
                other.id != doc.frontmatter.id
                    && other.tags.iter().any(|tag| {
                        tag.to_lowercase() == doc.frontmatter.id.to_lowercase().replace("fact.", "")
                            || tag.to_lowercase() == doc.frontmatter.title.to_lowercase()
                    })
            });

            if !is_tagged {
                return Some(HygieneViolation {
                    rule: HygieneRule::OrphanedDoc,
                    severity: Severity::Warning,
                    doc_id: doc.frontmatter.id.clone(),
                    message: "Document is not referenced by any other document".to_string(),
                    suggested_fix: Some(
                        "Add links to this document from related facts, or add relevant tags".to_string(),
                    ),
                });
            }
        }

        None
    }
}

/// Detects and handles duplicate facts
#[derive(Debug, Clone)]
pub struct FactDeduplicator {
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
    ///
    /// Based on the configured strategy, generates patches to merge or remove duplicates.
    pub fn resolve(&self, groups: &[DuplicateGroup]) -> Vec<MemoryPatchParams> {
        let mut patches = Vec::new();

        for group in groups {
            match self.strategy {
                Strategy::MergeToFirst => patches.push(self.create_merge_patch(group)),
                Strategy::KeepNewest => patches.extend(self.create_removal_patches(group)),
                Strategy::FlagForReview => patches.push(self.create_review_patch(group)),
            }
        }

        patches
    }

    /// Create a patch that merges duplicate content into the canonical document
    fn create_merge_patch(&self, group: &DuplicateGroup) -> MemoryPatchParams {
        let merge_note = format!(
            "Merging {} duplicate{} into {}",
            group.duplicates.len(),
            if group.duplicates.len() == 1 { "" } else { "s" },
            group.canonical_id
        );

        MemoryPatchParams {
            path: std::path::PathBuf::from(format!("memory/{}", group.canonical_id.replace('.', "/"))),
            doc_id: group.canonical_id.clone(),
            kind: MemoryKind::Fact,
            description: merge_note,
            diff: format!(
                "<!-- Merge duplicates: {} -->\n<!-- Similarity: {:.2} -->",
                group.duplicates.join(", "),
                group.similarity
            ),
            source_events: vec![],
            session_id: crate::layout::SessionId::new(),
            seq: 0,
        }
    }

    /// Create patches to remove older duplicate documents
    fn create_removal_patches(&self, group: &DuplicateGroup) -> Vec<MemoryPatchParams> {
        group
            .duplicates
            .iter()
            .map(|dup_id| MemoryPatchParams {
                path: std::path::PathBuf::from(format!("memory/{}", dup_id.replace('.', "/"))),
                doc_id: dup_id.clone(),
                kind: MemoryKind::Fact,
                description: format!("Remove duplicate of {}", group.canonical_id),
                diff: "".to_string(),
                source_events: vec![],
                session_id: crate::layout::SessionId::new(),
                seq: 0,
            })
            .collect()
    }

    /// Create a patch that flags duplicates for manual review
    fn create_review_patch(&self, group: &DuplicateGroup) -> MemoryPatchParams {
        let review_note = format!(
            "Review required: {} potential duplicate{} of {} (similarity: {:.2})",
            group.duplicates.len(),
            if group.duplicates.len() == 1 { "" } else { "s" },
            group.canonical_id,
            group.similarity
        );

        MemoryPatchParams {
            path: std::path::PathBuf::from(format!(".thunderus/memory/review/{}", group.canonical_id)),
            doc_id: format!("review.{}", group.canonical_id),
            kind: MemoryKind::Fact,
            description: review_note,
            diff: format!(
                "# Duplicate Review: {}\n\n## Canonical: {}\n\n## Duplicates:\n{}\n\n## Similarity Score: {:.2}\n\n## Action Required\n\nPlease review and decide whether to:\n- Merge into canonical\n- Keep as separate documents\n- Remove duplicates\n",
                group.canonical_id,
                group.canonical_id,
                group
                    .duplicates
                    .iter()
                    .map(|d| format!("- {}", d))
                    .collect::<Vec<_>>()
                    .join("\n"),
                group.similarity
            ),
            source_events: vec![],
            session_id: crate::layout::SessionId::new(),
            seq: 0,
        }
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
    use crate::memory::document::{MemoryDoc, MemoryFrontmatter};
    use crate::memory::kinds::MemoryKind;
    use chrono::Utc;
    use tempfile::TempDir;

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

    #[test]
    fn test_fact_deduplicator_resolve_merge_to_first() {
        let dedup = FactDeduplicator::new(Strategy::MergeToFirst);
        let group = DuplicateGroup {
            canonical_id: "fact.commands.build".to_string(),
            duplicates: vec!["fact.build".to_string()],
            similarity: 0.85,
        };

        let patches = dedup.resolve(std::slice::from_ref(&group));
        assert_eq!(patches.len(), 1);
        assert!(patches[0].description.contains("Merging"));
        assert_eq!(patches[0].doc_id, "fact.commands.build");
    }

    #[test]
    fn test_fact_deduplicator_resolve_keep_newest() {
        let dedup = FactDeduplicator::new(Strategy::KeepNewest);
        let group = DuplicateGroup {
            canonical_id: "fact.commands.build".to_string(),
            duplicates: vec!["fact.build".to_string(), "fact.compile".to_string()],
            similarity: 0.90,
        };

        let patches = dedup.resolve(std::slice::from_ref(&group));
        assert_eq!(patches.len(), 2);
        assert!(patches[0].description.contains("Remove duplicate"));
        assert!(patches[1].description.contains("Remove duplicate"));
    }

    #[test]
    fn test_fact_deduplicator_resolve_flag_for_review() {
        let dedup = FactDeduplicator::new(Strategy::FlagForReview);
        let group = DuplicateGroup {
            canonical_id: "fact.test.coverage".to_string(),
            duplicates: vec!["fact.testing".to_string()],
            similarity: 0.82,
        };

        let patches = dedup.resolve(&[group]);
        assert_eq!(patches.len(), 1);
        assert!(patches[0].description.contains("Review required"));
        assert!(patches[0].diff.contains("# Duplicate Review"));
    }

    #[test]
    fn test_hygiene_checker_doc_size_violation() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let large_body = "x".repeat(10000);
        let doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.large".to_string(),
                title: "Large Document".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now(),
                updated: Utc::now(),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: large_body,
        };

        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let violations = checker.check_doc(&doc, &paths);
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.rule == HygieneRule::DocOverSize));
    }

    #[test]
    fn test_hygiene_checker_missing_provenance() {
        let config = HygieneConfig { require_provenance: true, ..Default::default() };
        let checker = HygieneChecker::new(config);

        let doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.no_prov".to_string(),
                title: "No Provenance".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now(),
                updated: Utc::now(),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: "Some content".to_string(),
        };

        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let violations = checker.check_doc(&doc, &paths);
        assert!(violations.iter().any(|v| v.rule == HygieneRule::MissingProvenance));
    }

    #[test]
    fn test_hygiene_checker_with_provenance() {
        let config = HygieneConfig { require_provenance: true, ..Default::default() };
        let checker = HygieneChecker::new(config);

        let mut provenance = crate::memory::kinds::Provenance::default();
        provenance.events.push("evt_001".to_string());

        let doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.with_prov".to_string(),
                title: "With Provenance".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now(),
                updated: Utc::now(),
                provenance,
                verification: Default::default(),
                session: None,
            },
            body: "Some content".to_string(),
        };

        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let violations = checker.check_doc(&doc, &paths);
        assert!(!violations.iter().any(|v| v.rule == HygieneRule::MissingProvenance));
    }

    #[test]
    fn test_hygiene_checker_orphaned_document() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let old_date = Utc::now() - chrono::Duration::days(30);
        let orphan_doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.orphaned".to_string(),
                title: "Orphaned Document".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: old_date,
                updated: old_date,
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: "Some orphaned content".to_string(),
        };

        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let manifest = MemoryManifest::default();
        let references = checker.collect_all_references(std::slice::from_ref(&orphan_doc));
        let violation = checker.check_orphaned(&orphan_doc, &references, &manifest);

        assert!(violation.is_some());
        assert_eq!(violation.unwrap().rule, HygieneRule::OrphanedDoc);
    }

    #[test]
    fn test_hygiene_checker_core_not_orphaned() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let core_doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "CORE".to_string(),
                title: "Core Memory".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now() - chrono::Duration::days(100),
                updated: Utc::now(),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: "# Core Memory\n\nProject overview".to_string(),
        };

        let manifest = MemoryManifest::default();
        let references = std::collections::HashSet::new();
        let violation = checker.check_orphaned(&core_doc, &references, &manifest);
        assert!(violation.is_none());
    }

    #[test]
    fn test_hygiene_checker_recent_doc_not_orphaned() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let recent_doc = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.recent".to_string(),
                title: "Recent Document".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now() - chrono::Duration::days(2),
                updated: Utc::now(),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: "Recent content".to_string(),
        };

        let manifest = MemoryManifest::default();
        let references = std::collections::HashSet::new();
        let violation = checker.check_orphaned(&recent_doc, &references, &manifest);

        assert!(violation.is_none());
    }

    #[test]
    fn test_hygiene_checker_adr_not_orphaned() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let old_adr = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "adr.0001".to_string(),
                title: "Old ADR".to_string(),
                kind: MemoryKind::Adr,
                tags: vec![],
                created: Utc::now() - chrono::Duration::days(100),
                updated: Utc::now() - chrono::Duration::days(90),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: "# ADR-0001\n\nSome decision".to_string(),
        };

        let manifest = MemoryManifest::default();
        let references = std::collections::HashSet::new();
        let violation = checker.check_orphaned(&old_adr, &references, &manifest);

        assert!(violation.is_none());
    }

    #[test]
    fn test_hygiene_checker_collect_references() {
        let config = HygieneConfig::default();
        let checker = HygieneChecker::new(config);

        let doc_with_links = MemoryDoc {
            frontmatter: MemoryFrontmatter {
                id: "fact.main".to_string(),
                title: "Main Document".to_string(),
                kind: MemoryKind::Fact,
                tags: vec![],
                created: Utc::now(),
                updated: Utc::now(),
                provenance: Default::default(),
                verification: Default::default(),
                session: None,
            },
            body: r#"
See [Build Commands](../facts/commands_build.md) for details.
Also check @fact.test.coverage for testing info.

Referencing ADR-0005 for context.
"#
            .to_string(),
        };

        let references = checker.collect_all_references(&[doc_with_links]);

        assert!(references.contains("CORE"));
        assert!(references.contains("CORE.local"));
        assert!(references.contains("commands.build") || references.iter().any(|r| r.contains("commands")));
        assert!(references.contains("fact.test.coverage"));
        assert!(references.contains("adr.0005"));
    }
}
