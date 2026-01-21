//! Semantic memory management for Facts and ADRs
//!
//! Semantic memory stores durable, curated knowledge: stable facts and
//! architectural decisions (ADR-lite).

use crate::error::{Error, Result};
use crate::memory::document::MemoryDoc;
use crate::memory::kinds::{MemoryKind, VerificationStatus};
use crate::memory::paths::MemoryPaths;

use std::fs;
use std::path::PathBuf;

/// Manager for semantic memory documents (Facts and ADRs)
///
/// Provides methods for creating, loading, updating, and validating
/// semantic memory documents.
#[derive(Debug, Clone)]
pub struct SemanticMemory {
    /// Memory directory paths
    paths: MemoryPaths,
}

impl SemanticMemory {
    /// Create a new semantic memory manager
    pub fn new(paths: MemoryPaths) -> Self {
        Self { paths }
    }

    /// List all fact documents
    pub fn list_facts(&self) -> Result<Vec<FactDoc>> {
        let mut facts = Vec::new();

        let facts_dir = &self.paths.facts;
        if !facts_dir.exists() {
            return Ok(facts);
        }

        let entries = fs::read_dir(facts_dir).map_err(Error::Io)?;

        for entry in entries {
            let entry = entry.map_err(Error::Io)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let content = fs::read_to_string(&path).map_err(Error::Io)?;

                let doc = MemoryDoc::parse(&content)
                    .map_err(|e| Error::Parse(format!("Failed to parse fact document: {}", e)))?;

                if doc.frontmatter.kind == MemoryKind::Fact {
                    facts.push(FactDoc { path, doc });
                }
            }
        }

        Ok(facts)
    }

    /// List all ADR documents
    pub fn list_adrs(&self) -> Result<Vec<AdrDoc>> {
        let mut adrs = Vec::new();

        let decisions_dir = &self.paths.decisions;
        if !decisions_dir.exists() {
            return Ok(adrs);
        }

        let entries = fs::read_dir(decisions_dir).map_err(Error::Io)?;

        for entry in entries {
            let entry = entry.map_err(Error::Io)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let content = fs::read_to_string(&path).map_err(Error::Io)?;

                let doc = MemoryDoc::parse(&content)
                    .map_err(|e| Error::Parse(format!("Failed to parse ADR document: {}", e)))?;

                if doc.frontmatter.kind == MemoryKind::Adr {
                    adrs.push(AdrDoc { path, doc });
                }
            }
        }

        Ok(adrs)
    }

    /// Load a fact document by ID
    pub fn load_fact(&self, id: &str) -> Result<FactDoc> {
        let facts = self.list_facts()?;
        facts
            .into_iter()
            .find(|f| f.doc.frontmatter.id == id)
            .ok_or_else(|| Error::Other(format!("Fact not found: {}", id)))
    }

    /// Load an ADR document by ID
    pub fn load_adr(&self, id: &str) -> Result<AdrDoc> {
        let adrs = self.list_adrs()?;
        adrs.into_iter()
            .find(|a| a.doc.frontmatter.id == id)
            .ok_or_else(|| Error::Other(format!("ADR not found: {}", id)))
    }

    /// Create a new fact document
    pub fn create_fact(&self, fact: NewFact) -> Result<FactDoc> {
        let doc = MemoryDoc::new(fact.id, fact.title, MemoryKind::Fact, fact.tags, fact.body);

        let errors = doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid fact document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let filename = format!("{}.md", doc.frontmatter.id.replace('.', "_"));
        let path = self.paths.facts.join(filename);

        let content = doc.to_string();
        fs::write(&path, content).map_err(|e| Error::Other(format!("Failed to write fact document: {}", e)))?;

        Ok(FactDoc { path, doc })
    }

    /// Create a new ADR document with the next available sequence number
    pub fn create_adr(&self, adr: NewAdr) -> Result<AdrDoc> {
        let seq = self.next_adr_sequence()?;

        let doc = MemoryDoc::new(
            format!("adr.{:04}", seq),
            adr.title,
            MemoryKind::Adr,
            adr.tags,
            adr.body,
        );

        let errors = doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid ADR document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let filename = format!("ADR-{:04}.md", seq);
        let path = self.paths.decisions.join(filename);

        let content = doc.to_string();
        fs::write(&path, content).map_err(|e| Error::Other(format!("Failed to write ADR document: {}", e)))?;

        Ok(AdrDoc { path, doc })
    }

    /// Update an existing fact document
    pub fn update_fact(&self, id: &str, update: FactUpdate) -> Result<FactDoc> {
        let mut fact = self.load_fact(id)?;

        if let Some(body) = update.body {
            fact.doc.update_body(body);
        }

        if let Some(tags) = update.tags {
            fact.doc.frontmatter.tags = tags;
        }

        if let Some(event_id) = update.provenance_event {
            fact.doc.add_provenance_event(event_id);
        }

        let errors = fact.doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid fact document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let content = fact.doc.to_string();
        fs::write(&fact.path, content).map_err(|e| Error::Other(format!("Failed to write fact document: {}", e)))?;

        Ok(fact)
    }

    /// Update an existing ADR document
    pub fn update_adr(&self, id: &str, update: AdrUpdate) -> Result<AdrDoc> {
        let mut adr = self.load_adr(id)?;

        if let Some(body) = update.body {
            adr.doc.update_body(body);
        }

        if let Some(tags) = update.tags {
            adr.doc.frontmatter.tags = tags;
        }

        if let Some(event_id) = update.provenance_event {
            adr.doc.add_provenance_event(event_id);
        }

        if let Some(status) = update.status {
            adr.doc.frontmatter.verification.status = status;
        }

        let errors = adr.doc.validate();
        if !errors.is_empty() {
            return Err(Error::Validation(format!(
                "Invalid ADR document: {}",
                errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join("; ")
            )));
        }

        let content = adr.doc.to_string();
        fs::write(&adr.path, content).map_err(|e| Error::Other(format!("Failed to write ADR document: {}", e)))?;

        Ok(adr)
    }

    /// Delete a fact document
    pub fn delete_fact(&self, id: &str) -> Result<()> {
        let fact = self.load_fact(id)?;
        fs::remove_file(&fact.path).map_err(|e| Error::Other(format!("Failed to delete fact document: {}", e)))?;
        Ok(())
    }

    /// Delete an ADR document
    pub fn delete_adr(&self, id: &str) -> Result<()> {
        let adr = self.load_adr(id)?;
        fs::remove_file(&adr.path).map_err(|e| Error::Other(format!("Failed to delete ADR document: {}", e)))?;
        Ok(())
    }

    /// Get the next ADR sequence number
    fn next_adr_sequence(&self) -> Result<u32> {
        let mut max_seq = 0;

        if let Ok(adrs) = self.list_adrs() {
            for adr in adrs {
                if let Some(seq_str) = adr.doc.frontmatter.id.strip_prefix("adr.")
                    && let Ok(seq) = seq_str.parse::<u32>()
                {
                    max_seq = max_seq.max(seq);
                }
            }
        }

        Ok(max_seq + 1)
    }

    /// Search facts by tag
    pub fn find_facts_by_tag(&self, tag: &str) -> Result<Vec<FactDoc>> {
        let facts = self.list_facts()?;
        Ok(facts
            .into_iter()
            .filter(|f| f.doc.frontmatter.tags.iter().any(|t| t == tag))
            .collect())
    }

    /// Search ADRs by tag
    pub fn find_adrs_by_tag(&self, tag: &str) -> Result<Vec<AdrDoc>> {
        let adrs = self.list_adrs()?;
        Ok(adrs
            .into_iter()
            .filter(|a| a.doc.frontmatter.tags.iter().any(|t| t == tag))
            .collect())
    }
}

/// A fact document with its path
#[derive(Debug, Clone)]
pub struct FactDoc {
    /// Path to the fact file
    pub path: PathBuf,
    /// The parsed fact document
    pub doc: MemoryDoc,
}

/// An ADR document with its path
#[derive(Debug, Clone)]
pub struct AdrDoc {
    /// Path to the ADR file
    pub path: PathBuf,
    /// The parsed ADR document
    pub doc: MemoryDoc,
}

/// Data for creating a new fact
pub struct NewFact {
    /// Unique identifier (e.g., "fact.testing.coverage")
    pub id: String,
    /// Title of the fact
    pub title: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Body content (markdown)
    pub body: String,
}

/// Data for creating a new ADR
pub struct NewAdr {
    /// Title of the ADR (e.g., "Use imara-diff for code diffing")
    pub title: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Body content (markdown with required sections)
    pub body: String,
}

/// Data for updating a fact
pub struct FactUpdate {
    /// New body content
    pub body: Option<String>,
    /// New tags
    pub tags: Option<Vec<String>>,
    /// Provenance event to add
    pub provenance_event: Option<String>,
}

/// Data for updating an ADR
pub struct AdrUpdate {
    /// New body content
    pub body: Option<String>,
    /// New tags
    pub tags: Option<Vec<String>>,
    /// Provenance event to add
    pub provenance_event: Option<String>,
    /// Verification status
    pub status: Option<VerificationStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_semantic_memory() -> (TempDir, SemanticMemory) {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();
        let semantic = SemanticMemory::new(paths);
        (temp, semantic)
    }

    #[test]
    fn test_semantic_memory_list_empty() {
        let (_temp, semantic) = create_test_semantic_memory();

        let facts = semantic.list_facts().unwrap();
        let adrs = semantic.list_adrs().unwrap();

        assert!(facts.is_empty());
        assert!(adrs.is_empty());
    }

    #[test]
    fn test_create_fact() {
        let (_temp, semantic) = create_test_semantic_memory();

        let fact = semantic
            .create_fact(NewFact {
                id: "fact.test.coverage".to_string(),
                title: "Coverage Requirements".to_string(),
                tags: vec!["testing".to_string(), "ci".to_string()],
                body: "- Minimum line coverage: 80%".to_string(),
            })
            .unwrap();

        assert_eq!(fact.doc.frontmatter.id, "fact.test.coverage");
        assert_eq!(fact.doc.frontmatter.title, "Coverage Requirements");
        assert!(fact.path.exists());
    }

    #[test]
    fn test_create_adr() {
        let (_temp, semantic) = create_test_semantic_memory();

        let adr = semantic
            .create_adr(NewAdr {
                title: "Use imara-diff for code diffing".to_string(),
                tags: vec!["diff".to_string(), "dependencies".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nNeed semantic diffing.\n\n## Decision\n\nUse imara-diff crate.\n\n## Consequences\n\n- Positive: Better diffs\n- Negative: Additional dependency".to_string(),
            })
            .unwrap();

        assert_eq!(adr.doc.frontmatter.id, "adr.0001");
        assert!(adr.path.ends_with("ADR-0001.md"));
        assert!(adr.path.exists());
    }

    #[test]
    fn test_create_adr_sequence() {
        let (_temp, semantic) = create_test_semantic_memory();

        let adr1 = semantic
            .create_adr(NewAdr {
                title: "First ADR".to_string(),
                tags: vec!["test".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        let adr2 = semantic
            .create_adr(NewAdr {
                title: "Second ADR".to_string(),
                tags: vec!["test".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        assert_eq!(adr1.doc.frontmatter.id, "adr.0001");
        assert_eq!(adr2.doc.frontmatter.id, "adr.0002");
    }

    #[test]
    fn test_load_fact() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_fact(NewFact {
                id: "fact.test.example".to_string(),
                title: "Test Fact".to_string(),
                tags: vec!["test".to_string()],
                body: "Test content".to_string(),
            })
            .unwrap();

        let loaded = semantic.load_fact("fact.test.example").unwrap();
        assert_eq!(loaded.doc.frontmatter.id, "fact.test.example");
        assert_eq!(loaded.doc.frontmatter.title, "Test Fact");
    }

    #[test]
    fn test_load_adr() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_adr(NewAdr {
                title: "Test ADR".to_string(),
                tags: vec!["test".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        let loaded = semantic.load_adr("adr.0001").unwrap();
        assert_eq!(loaded.doc.frontmatter.id, "adr.0001");
    }

    #[test]
    fn test_update_fact() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_fact(NewFact {
                id: "fact.test.updatable".to_string(),
                title: "Original Title".to_string(),
                tags: vec!["test".to_string()],
                body: "Original content".to_string(),
            })
            .unwrap();

        let updated = semantic
            .update_fact(
                "fact.test.updatable",
                FactUpdate {
                    body: Some("Updated content".to_string()),
                    tags: Some(vec!["test".to_string(), "updated".to_string()]),
                    provenance_event: Some("evt_001".to_string()),
                },
            )
            .unwrap();

        assert!(updated.doc.body.contains("Updated content"));
        assert_eq!(updated.doc.frontmatter.tags.len(), 2);
        assert!(
            updated
                .doc
                .frontmatter
                .provenance
                .events
                .contains(&"evt_001".to_string())
        );
    }

    #[test]
    fn test_update_adr() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_adr(NewAdr {
                title: "Test ADR".to_string(),
                tags: vec!["test".to_string()],
                body: "## Status\n\nProposed\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        let updated = semantic
            .update_adr(
                "adr.0001",
                AdrUpdate {
                    body: Some("## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string()),
                    tags: Some(vec!["test".to_string(), "accepted".to_string()]),
                    provenance_event: Some("evt_002".to_string()),
                    status: Some(VerificationStatus::Verified),
                },
            )
            .unwrap();

        assert!(updated.doc.body.contains("Accepted"));
        assert_eq!(
            updated.doc.frontmatter.verification.status,
            VerificationStatus::Verified
        );
    }

    #[test]
    fn test_delete_fact() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_fact(NewFact {
                id: "fact.test.deletable".to_string(),
                title: "Delete Me".to_string(),
                tags: vec!["test".to_string()],
                body: "Content".to_string(),
            })
            .unwrap();

        semantic.delete_fact("fact.test.deletable").unwrap();

        let result = semantic.load_fact("fact.test.deletable");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_adr() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_adr(NewAdr {
                title: "Delete Me".to_string(),
                tags: vec!["test".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        semantic.delete_adr("adr.0001").unwrap();

        let result = semantic.load_adr("adr.0001");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_facts_by_tag() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_fact(NewFact {
                id: "fact.test.one".to_string(),
                title: "One".to_string(),
                tags: vec!["testing".to_string()],
                body: "Content".to_string(),
            })
            .unwrap();

        semantic
            .create_fact(NewFact {
                id: "fact.test.two".to_string(),
                title: "Two".to_string(),
                tags: vec!["testing".to_string(), "coverage".to_string()],
                body: "Content".to_string(),
            })
            .unwrap();

        semantic
            .create_fact(NewFact {
                id: "fact.build.three".to_string(),
                title: "Three".to_string(),
                tags: vec!["build".to_string()],
                body: "Content".to_string(),
            })
            .unwrap();

        let testing_facts = semantic.find_facts_by_tag("testing").unwrap();
        assert_eq!(testing_facts.len(), 2);
    }

    #[test]
    fn test_find_adrs_by_tag() {
        let (_temp, semantic) = create_test_semantic_memory();

        semantic
            .create_adr(NewAdr {
                title: "One".to_string(),
                tags: vec!["architecture".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        semantic
            .create_adr(NewAdr {
                title: "Two".to_string(),
                tags: vec!["architecture".to_string(), "tools".to_string()],
                body: "## Status\n\nAccepted\n\n## Context\n\nTest\n\n## Decision\n\nTest decision\n\n## Consequences\n\nTest consequences".to_string(),
            })
            .unwrap();

        let arch_adrs = semantic.find_adrs_by_tag("architecture").unwrap();
        assert_eq!(arch_adrs.len(), 2);
    }

    #[test]
    fn test_fact_validation_empty_id() {
        let (_temp, semantic) = create_test_semantic_memory();

        let result = semantic.create_fact(NewFact {
            id: "".to_string(),
            title: "Test".to_string(),
            tags: vec!["test".to_string()],
            body: "Content".to_string(),
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_adr_validation_missing_sections() {
        let (_temp, semantic) = create_test_semantic_memory();

        let result = semantic.create_adr(NewAdr {
            title: "Invalid ADR".to_string(),
            tags: vec!["test".to_string()],
            body: "Missing required sections".to_string(),
        });

        assert!(result.is_err());
    }
}
