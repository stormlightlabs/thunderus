//! Memory manifest for fast inventory and search
//!
//! The manifest provides a cached inventory of all memory documents for
//! quick lookup, search, and UI operations.

use crate::error::{Error, Result};
use crate::memory::document::MemoryDoc;
use crate::memory::kinds::MemoryKind;
use crate::memory::paths::MemoryPaths;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Entry in the memory manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Path to the memory document
    pub path: PathBuf,
    /// Document ID
    pub id: String,
    /// Kind of memory document
    pub kind: MemoryKind,
    /// Document title
    pub title: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Last update timestamp
    pub updated: DateTime<Utc>,
    /// Size in bytes
    pub size_bytes: u64,
    /// Approximate token count
    pub token_count_approx: usize,
    /// Provenance information
    pub provenance: ProvenanceInfo,
    /// Verification information
    pub verification: VerificationInfo,
}

/// Provenance information for manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceInfo {
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub patches: Vec<String>,
    #[serde(default)]
    pub commits: Vec<String>,
}

/// Verification information for manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationInfo {
    pub last_verified_commit: Option<String>,
    pub status: String,
}

/// Statistics about the memory store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestStats {
    pub total_docs: usize,
    pub by_kind: HashMap<String, usize>,
    pub total_tokens_approx: usize,
}

/// Memory manifest for fast inventory and search
///
/// The manifest provides a cached index of all memory documents
/// for quick lookup without scanning the filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryManifest {
    pub version: u32,
    pub generated_at: DateTime<Utc>,
    pub docs: Vec<ManifestEntry>,
    pub stats: ManifestStats,
}

impl MemoryManifest {
    /// Rebuild manifest by scanning memory directories
    pub fn rebuild(paths: &MemoryPaths) -> Result<Self> {
        let mut docs = Vec::new();
        let mut by_kind = HashMap::new();
        let mut total_tokens = 0;

        if let Ok(core_doc) = Self::scan_file(paths.core_memory_file()) {
            by_kind.entry("core".to_string()).and_modify(|c| *c += 1).or_insert(1);
            total_tokens += core_doc.token_count_approx;
            docs.push(core_doc);
        }

        if let Ok(core_local_doc) = Self::scan_file(paths.core_local_memory_file()) {
            by_kind.entry("core".to_string()).and_modify(|c| *c += 1).or_insert(1);
            total_tokens += core_local_doc.token_count_approx;
            docs.push(core_local_doc);
        }

        if let Ok(entries) = Self::scan_directory(&paths.facts, MemoryKind::Fact) {
            for entry in entries {
                by_kind.entry("fact".to_string()).and_modify(|c| *c += 1).or_insert(1);
                total_tokens += entry.token_count_approx;
                docs.push(entry);
            }
        }

        if let Ok(entries) = Self::scan_directory(&paths.decisions, MemoryKind::Adr) {
            for entry in entries {
                by_kind.entry("adr".to_string()).and_modify(|c| *c += 1).or_insert(1);
                total_tokens += entry.token_count_approx;
                docs.push(entry);
            }
        }

        if let Ok(entries) = Self::scan_directory(&paths.playbooks, MemoryKind::Playbook) {
            for entry in entries {
                by_kind
                    .entry("playbook".to_string())
                    .and_modify(|c| *c += 1)
                    .or_insert(1);
                total_tokens += entry.token_count_approx;
                docs.push(entry);
            }
        }

        if paths.episodic.exists()
            && let Ok(entries) = Self::scan_recursive(&paths.episodic, MemoryKind::Recap)
        {
            for entry in entries {
                by_kind.entry("recap".to_string()).and_modify(|c| *c += 1).or_insert(1);
                total_tokens += entry.token_count_approx;
                docs.push(entry);
            }
        }

        let stats = ManifestStats { total_docs: docs.len(), by_kind, total_tokens_approx: total_tokens };

        Ok(Self { version: 1, generated_at: Utc::now(), docs, stats })
    }

    /// Load cached manifest from disk
    pub fn load(paths: &MemoryPaths) -> Result<Self> {
        let manifest_file = paths.manifest_file();
        let content =
            fs::read_to_string(&manifest_file).map_err(|e| Error::Other(format!("Failed to read manifest: {}", e)))?;

        serde_json::from_str(&content).map_err(|e| Error::Parse(format!("Failed to parse manifest: {}", e)))
    }

    /// Save manifest to disk
    pub fn save(&self, paths: &MemoryPaths) -> Result<()> {
        let manifest_file = paths.manifest_file();

        if let Some(parent) = manifest_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("Failed to create manifest directory: {}", e)))?;
        }

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Parse(format!("Failed to serialize manifest: {}", e)))?;

        fs::write(&manifest_file, content).map_err(|e| Error::Other(format!("Failed to write manifest: {}", e)))?;

        Ok(())
    }

    /// Find docs by kind
    pub fn by_kind(&self, kind: MemoryKind) -> Vec<&ManifestEntry> {
        self.docs.iter().filter(|doc| doc.kind == kind).collect()
    }

    /// Find docs by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&ManifestEntry> {
        self.docs
            .iter()
            .filter(|doc| doc.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// Find doc by ID
    pub fn by_id(&self, id: &str) -> Option<&ManifestEntry> {
        self.docs.iter().find(|doc| doc.id == id)
    }

    /// Find doc by path
    pub fn by_path(&self, path: &Path) -> Option<&ManifestEntry> {
        self.docs.iter().find(|doc| doc.path == path)
    }

    /// Get all docs sorted by update time (newest first)
    pub fn by_recent(&self) -> Vec<&ManifestEntry> {
        let mut docs = self.docs.iter().collect::<Vec<_>>();
        docs.sort_by(|a, b| b.updated.cmp(&a.updated));
        docs
    }

    /// Scan a single file and create a manifest entry
    fn scan_file(path: PathBuf) -> Result<ManifestEntry> {
        let content = fs::read_to_string(&path)
            .map_err(|e| Error::Other(format!("Failed to read file {}: {}", path.display(), e)))?;

        let doc = MemoryDoc::parse(&content)
            .map_err(|e| Error::Parse(format!("Failed to parse document {}: {}", path.display(), e)))?;

        let metadata = fs::metadata(&path)
            .map_err(|e| Error::Other(format!("Failed to read metadata {}: {}", path.display(), e)))?;

        let size_bytes = metadata.len();
        let frontmatter = doc.frontmatter.clone();

        Ok(ManifestEntry {
            path,
            id: frontmatter.id.clone(),
            kind: frontmatter.kind,
            title: frontmatter.title,
            tags: frontmatter.tags,
            updated: frontmatter.updated,
            size_bytes,
            token_count_approx: doc.approx_token_count(),
            provenance: ProvenanceInfo {
                events: doc.frontmatter.provenance.events,
                patches: doc.frontmatter.provenance.patches,
                commits: doc.frontmatter.provenance.commits,
            },
            verification: VerificationInfo {
                last_verified_commit: doc.frontmatter.verification.last_verified_commit,
                status: format!("{:?}", doc.frontmatter.verification.status).to_lowercase(),
            },
        })
    }

    /// Scan a directory for memory documents of a specific kind
    fn scan_directory(dir: &Path, kind: MemoryKind) -> Result<Vec<ManifestEntry>> {
        let mut entries = Vec::new();

        if !dir.exists() {
            return Ok(entries);
        }

        let dir_entries = fs::read_dir(dir)
            .map_err(|e| Error::Other(format!("Failed to read directory {}: {}", dir.display(), e)))?;

        for entry in dir_entries {
            let entry = entry.map_err(|e| Error::Other(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Ok(doc_entry) = Self::scan_file(path)
                && doc_entry.kind == kind
            {
                entries.push(doc_entry);
            }
        }

        Ok(entries)
    }

    /// Scan a directory recursively for memory documents
    fn scan_recursive(dir: &Path, kind: MemoryKind) -> Result<Vec<ManifestEntry>> {
        let mut entries = Vec::new();

        if !dir.exists() {
            return Ok(entries);
        }

        let dir_entries = fs::read_dir(dir)
            .map_err(|e| Error::Other(format!("Failed to read directory {}: {}", dir.display(), e)))?;

        for entry in dir_entries {
            let entry = entry.map_err(|e| Error::Other(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();

            if path.is_dir() {
                if let Ok(mut sub_entries) = Self::scan_recursive(&path, kind) {
                    entries.append(&mut sub_entries);
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("md")
                && let Ok(doc_entry) = Self::scan_file(path.clone())
                && doc_entry.kind == kind
            {
                entries.push(doc_entry);
            }
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_memory_files() -> (TempDir, MemoryPaths) {
        let temp = TempDir::new().unwrap();
        let paths = MemoryPaths::from_thunderus_root(temp.path());
        paths.ensure().unwrap();

        let fact_content = r#"---
id: fact.test.coverage
title: Coverage Requirements
kind: fact
tags: [testing, ci]
created: 2026-01-21T00:00:00Z
updated: 2026-01-21T00:00:00Z
provenance:
  events: []
  patches: []
  commits: []
verification:
  last_verified_commit: null
  status: unknown
---

# Coverage Requirements

- Minimum line coverage: 80%
"#;

        fs::write(paths.facts.join("testing.md"), fact_content).unwrap();

        let adr_content = r#"---
id: adr.0001
title: Test ADR
kind: adr
tags: [test]
created: 2026-01-21T00:00:00Z
updated: 2026-01-21T00:00:00Z
provenance:
  events: []
  patches: []
  commits: []
verification:
  last_verified_commit: null
  status: unknown
---

# ADR-0001: Test ADR

## Status
Accepted

## Context
Test context

## Decision
Test decision

## Consequences
Test consequences
"#;

        fs::write(paths.decisions.join("ADR-0001.md"), adr_content).unwrap();

        (temp, paths)
    }

    #[test]
    fn test_manifest_rebuild() {
        let (_temp, paths) = create_test_memory_files();

        let manifest = MemoryManifest::rebuild(&paths).unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.docs.len(), 2);
        assert_eq!(manifest.stats.total_docs, 2);
        assert_eq!(manifest.stats.by_kind.get("fact"), Some(&1));
        assert_eq!(manifest.stats.by_kind.get("adr"), Some(&1));
    }

    #[test]
    fn test_manifest_by_kind() {
        let (_temp, paths) = create_test_memory_files();

        let manifest = MemoryManifest::rebuild(&paths).unwrap();

        let facts = manifest.by_kind(MemoryKind::Fact);
        let adrs = manifest.by_kind(MemoryKind::Adr);

        assert_eq!(facts.len(), 1);
        assert_eq!(adrs.len(), 1);
        assert_eq!(facts[0].id, "fact.test.coverage");
        assert_eq!(adrs[0].id, "adr.0001");
    }

    #[test]
    fn test_manifest_by_tag() {
        let (_temp, paths) = create_test_memory_files();

        let manifest = MemoryManifest::rebuild(&paths).unwrap();

        let testing_docs = manifest.by_tag("testing");

        assert_eq!(testing_docs.len(), 1);
        assert_eq!(testing_docs[0].id, "fact.test.coverage");
    }

    #[test]
    fn test_manifest_by_id() {
        let (_temp, paths) = create_test_memory_files();

        let manifest = MemoryManifest::rebuild(&paths).unwrap();

        let fact = manifest.by_id("fact.test.coverage");
        let adr = manifest.by_id("adr.0001");
        let missing = manifest.by_id("fact.missing");

        assert!(fact.is_some());
        assert!(adr.is_some());
        assert!(missing.is_none());
        assert_eq!(fact.unwrap().title, "Coverage Requirements");
    }

    #[test]
    fn test_manifest_save_load() {
        let (_temp, paths) = create_test_memory_files();

        let manifest = MemoryManifest::rebuild(&paths).unwrap();
        manifest.save(&paths).unwrap();

        let loaded = MemoryManifest::load(&paths).unwrap();

        assert_eq!(loaded.version, manifest.version);
        assert_eq!(loaded.docs.len(), manifest.docs.len());
        assert_eq!(loaded.stats.total_docs, manifest.stats.total_docs);
    }

    #[test]
    fn test_manifest_by_recent() {
        let (_temp, paths) = create_test_memory_files();
        let manifest = MemoryManifest::rebuild(&paths).unwrap();
        let recent = manifest.by_recent();
        assert_eq!(recent.len(), 2);
        assert!(recent[0].updated >= recent[1].updated);
    }
}
