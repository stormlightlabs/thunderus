//! Memory document indexer for filesystem-to-store synchronization
//!
//! The `MemoryIndexer` scans memory directories for markdown documents,
//! parses them, and populates the SQLite FTS5 store for full-text search.

use crate::{Error, MemoryMeta, MemoryStore, Result};

use std::path::{Path, PathBuf};
use thunderus_core::memory::{MemoryDoc, MemoryPaths};
use tokio::fs;

/// Result of an indexing operation
#[derive(Debug, Clone, Default)]
pub struct IndexResult {
    /// Number of documents added
    pub docs_added: usize,
    /// Number of documents updated
    pub docs_updated: usize,
    /// Number of documents deleted
    pub docs_deleted: usize,
    /// Errors encountered during indexing
    pub errors: Vec<IndexError>,
    /// Duration of the indexing operation
    pub duration_ms: u64,
}

/// An error encountered during indexing
#[derive(Debug, Clone)]
pub struct IndexError {
    /// File path that caused the error
    pub path: String,
    /// Error description
    pub message: String,
}

impl IndexError {
    /// Create a new index error
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into() }
    }
}

/// Indexes memory documents from filesystem to SQLite store
///
/// Scans the memory directory structure and populates the FTS5 store.
pub struct MemoryIndexer {
    /// The memory store to populate
    store: MemoryStore,
    /// Memory directory paths
    paths: MemoryPaths,
    /// Repository root path
    repo_root: PathBuf,
}

impl MemoryIndexer {
    /// Create a new indexer for the given store and paths
    pub fn new(store: MemoryStore, paths: MemoryPaths, repo_root: &Path) -> Self {
        Self { store, paths, repo_root: repo_root.to_path_buf() }
    }

    /// Perform a full reindex of all memory documents
    ///
    /// Scans all memory directories and updates the store.
    #[tracing::instrument(skip(self))]
    pub async fn reindex_all(&self) -> Result<IndexResult> {
        let start = std::time::Instant::now();
        tracing::info!("Starting full memory reindex");

        let mut result =
            IndexResult { docs_added: 0, docs_updated: 0, docs_deleted: 0, errors: Vec::new(), duration_ms: 0 };

        for file_entry in self.scan_memory_dirs().await {
            match file_entry {
                Ok(file_path) => match self.index_doc(&file_path).await {
                    Ok(_) => result.docs_added += 1,
                    Err(e) => {
                        tracing::warn!("Failed to index {:?}: {}", file_path, e);
                        result
                            .errors
                            .push(IndexError::new(file_path.display().to_string(), e.to_string()));
                    }
                },
                Err(e) => result.errors.push(IndexError::new("scan", e)),
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        tracing::info!(
            "Reindex complete: {} added, {} updated, {} deleted, {} errors in {}ms",
            result.docs_added,
            result.docs_updated,
            result.docs_deleted,
            result.errors.len(),
            result.duration_ms
        );

        Ok(result)
    }

    /// Perform incremental indexing of changed documents
    ///
    /// Uses file modification times to detect changes since last index.
    #[tracing::instrument(skip(self))]
    pub async fn index_changed(&self) -> Result<IndexResult> {
        let start = std::time::Instant::now();
        tracing::debug!("Starting incremental memory index");

        let mut result =
            IndexResult { docs_added: 0, docs_updated: 0, docs_deleted: 0, errors: Vec::new(), duration_ms: 0 };

        let stats = self.store.stats().await?;
        let last_indexed = stats.last_indexed;

        for file_entry in self.scan_memory_dirs().await {
            match file_entry {
                Ok(file_path) => {
                    let metadata = match fs::metadata(&file_path).await {
                        Ok(m) => m,
                        Err(e) => {
                            result
                                .errors
                                .push(IndexError::new(file_path.display().to_string(), e.to_string()));
                            continue;
                        }
                    };

                    let modified = metadata.modified().ok();
                    let modified_chrono = modified
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, d.subsec_nanos()));

                    if let Some(modified_time) = modified_chrono
                        && modified_time > last_indexed
                    {
                        match self.index_doc(&file_path).await {
                            Ok(_) => result.docs_updated += 1,
                            Err(e) => {
                                tracing::warn!("Failed to index {:?}: {}", file_path, e);
                                result
                                    .errors
                                    .push(IndexError::new(file_path.display().to_string(), e.to_string()));
                            }
                        }
                    }
                }
                Err(e) => result.errors.push(IndexError::new("scan", e)),
            }
        }

        result.duration_ms = start.elapsed().as_millis() as u64;
        tracing::debug!(
            "Incremental index complete: {} added, {} updated, {} deleted, {} errors in {}ms",
            result.docs_added,
            result.docs_updated,
            result.docs_deleted,
            result.errors.len(),
            result.duration_ms
        );

        Ok(result)
    }

    /// Index a single document by path
    #[tracing::instrument(skip(self))]
    pub async fn index_doc(&self, path: &Path) -> Result<()> {
        tracing::debug!("Indexing document: {:?}", path);

        let content = fs::read_to_string(path).await?;
        let (namespace, key, doc, meta) = self.process_doc(path, &content)?;

        self.store.put(&namespace, &key, &doc, meta).await?;
        tracing::trace!("Document indexed successfully");

        Ok(())
    }

    /// Remove a document from the index
    #[tracing::instrument(skip(self))]
    pub async fn remove_doc(&self, doc_id: &str) -> Result<bool> {
        tracing::debug!("Removing document from index: {}", doc_id);

        let parts: Vec<&str> = doc_id.split(':').collect();
        if parts.len() != 2 {
            Err(Error::database(format!("Invalid doc_id format: {}", doc_id)))
        } else {
            Ok(self.store.delete(parts[0], parts[1]).await?)
        }
    }

    /// Scan memory directories and return all markdown file paths
    async fn scan_memory_dirs(&self) -> Vec<std::result::Result<PathBuf, String>> {
        let mut paths = Vec::new();

        if self.paths.core.exists() {
            paths.extend(self.scan_dir_for_md(&self.paths.core).await);
        }

        if self.paths.facts.exists() {
            paths.extend(self.scan_dir_for_md(&self.paths.facts).await);
        }
        if self.paths.decisions.exists() {
            paths.extend(self.scan_dir_for_md(&self.paths.decisions).await);
        }

        if self.paths.playbooks.exists() {
            paths.extend(self.scan_dir_for_md(&self.paths.playbooks).await);
        }

        if self.paths.episodic.exists() {
            let episodic_files = self.scan_dir_recursive(&self.paths.episodic).await;
            paths.extend(episodic_files.into_iter());
        }

        paths
    }

    /// Scan a directory for markdown files (non-recursive)
    async fn scan_dir_for_md(&self, dir: &Path) -> Vec<std::result::Result<PathBuf, String>> {
        let mut results = Vec::new();

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => return vec![Err(format!("Failed to read directory {:?}: {}", dir, e))],
        };

        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                results.push(Ok(path));
            }
        }

        results
    }

    /// Scan a directory recursively for markdown files
    async fn scan_dir_recursive(&self, dir: &Path) -> Vec<std::result::Result<PathBuf, String>> {
        let mut results = Vec::new();
        let mut dirs_to_scan = vec![dir.to_path_buf()];

        while let Some(current_dir) = dirs_to_scan.pop() {
            let mut entries = match fs::read_dir(&current_dir).await {
                Ok(e) => e,
                Err(e) => {
                    results.push(Err(format!("Failed to read directory {:?}: {}", current_dir, e)));
                    continue;
                }
            };

            while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
                let path = entry.path();
                let file_type_result = entry.file_type().await;

                if let Ok(file_type) = file_type_result {
                    if file_type.is_dir() {
                        dirs_to_scan.push(path);
                    } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                        results.push(Ok(path));
                    }
                } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    results.push(Ok(path));
                }
            }
        }

        results
    }

    /// Process a document file into namespace, key, content, and metadata
    fn process_doc(&self, path: &Path, content: &str) -> Result<(String, String, String, MemoryMeta)> {
        let doc = MemoryDoc::parse(content).map_err(|e| Error::database(format!("Failed to parse document: {}", e)))?;

        let headings = Self::extract_headings(&doc.body);
        let namespace = self.namespace_from_path(path)?;
        let key = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Error::database(format!("Invalid filename: {:?}", path)))?
            .to_string();

        let meta = MemoryMeta {
            id: doc.frontmatter.id.clone(),
            kind: doc.frontmatter.kind,
            title: doc.frontmatter.title.clone(),
            tags: doc.frontmatter.tags.clone(),
            headings,
            path: self.relative_path(path)?,
            updated: doc.frontmatter.updated,
            event_ids: doc.frontmatter.provenance.events.clone(),
            patch_ids: doc.frontmatter.provenance.patches.clone(),
            token_count: content.len() / 4,
        };

        let full_doc = format!(
            "---\n{}---\n\n{}",
            serde_yml::to_string(&doc.frontmatter)
                .map_err(|e| Error::database(format!("Failed to serialize frontmatter: {}", e)))?,
            doc.body
        );

        Ok((namespace, key, full_doc, meta))
    }

    /// Extract markdown headings for anchor navigation
    ///
    /// Converts headings like "## Commands" into "commands" for URL anchors.
    fn extract_headings(body: &str) -> Vec<String> {
        body.lines()
            .filter(|line| line.starts_with('#'))
            .map(|line| line.trim_start_matches('#').trim().to_lowercase().replace(' ', "-"))
            .collect()
    }

    /// Build namespace from file path
    fn namespace_from_path(&self, path: &Path) -> Result<String> {
        let path_abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let core_abs = self
            .paths
            .core
            .canonicalize()
            .unwrap_or_else(|_| self.paths.core.clone());
        let facts_abs = self
            .paths
            .facts
            .canonicalize()
            .unwrap_or_else(|_| self.paths.facts.clone());
        let decisions_abs = self
            .paths
            .decisions
            .canonicalize()
            .unwrap_or_else(|_| self.paths.decisions.clone());
        let playbooks_abs = self
            .paths
            .playbooks
            .canonicalize()
            .unwrap_or_else(|_| self.paths.playbooks.clone());
        let episodic_abs = self
            .paths
            .episodic
            .canonicalize()
            .unwrap_or_else(|_| self.paths.episodic.clone());

        match path_abs {
            p if p.starts_with(&core_abs) => Ok("core".to_string()),
            p if p.starts_with(&facts_abs) => Ok("semantic/facts".to_string()),
            p if p.starts_with(&decisions_abs) => Ok("semantic/decisions".to_string()),
            p if p.starts_with(&playbooks_abs) => Ok("procedural/playbooks".to_string()),
            p if p.starts_with(&episodic_abs) => {
                let namespace = p
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .map(|s| format!("episodic/{}", s))
                    .unwrap_or_else(|| "episodic".to_string());
                Ok(namespace)
            }
            _ => Err(Error::database(format!("Unknown memory path: {:?}", path))),
        }
    }

    /// Get relative path from repository root
    fn relative_path(&self, path: &Path) -> Result<String> {
        path.strip_prefix(&self.repo_root)
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|_| Error::database(format!("Path is not in repository: {:?}", path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SearchFilters;
    use tempfile::TempDir;

    #[test]
    fn test_extract_headings() {
        let body = r#"
# Main Title

## Section One

Content here.

### Subsection 1.1

More content.

## Section Two

Final content.
"#;

        let headings = MemoryIndexer::extract_headings(body);
        assert_eq!(
            headings,
            vec!["main-title", "section-one", "subsection-1.1", "section-two"]
        );
    }

    #[tokio::test]
    async fn test_indexer_basic_flow() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("memory.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let memory_dir = temp_dir.path().join(".thunderus").join("memory");
        let core_dir = memory_dir.join("core");
        fs::create_dir_all(&core_dir).await.unwrap();

        let core_file = core_dir.join("CORE.md");
        let content = r#"---
id: core.project
title: Project Core Memory
kind: core
tags: [core, always-loaded]
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

# Project Core Memory

## Identity
This is a test project.

## Commands
cargo test

## Architecture
Rust workspace

## Conventions
Use idiomatic Rust
"#;
        fs::write(&core_file, content).await.unwrap();

        let paths = MemoryPaths::from_thunderus_root(temp_dir.path());
        let indexer = MemoryIndexer::new(store.clone(), paths, temp_dir.path());

        let result = indexer.reindex_all().await.unwrap();

        assert_eq!(result.docs_added, 1);
        assert!(result.errors.is_empty());

        let hits = store.search("project", SearchFilters::default()).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Project Core Memory");
    }

    #[tokio::test]
    async fn test_namespace_from_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("memory.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let paths = MemoryPaths::from_thunderus_root(temp_dir.path());
        let indexer = MemoryIndexer::new(store, paths, temp_dir.path());

        let core_file = temp_dir
            .path()
            .join(".thunderus")
            .join("memory")
            .join("core")
            .join("CORE.md");
        let namespace = indexer.namespace_from_path(&core_file).unwrap();
        assert_eq!(namespace, "core");

        let facts_file = temp_dir
            .path()
            .join(".thunderus")
            .join("memory")
            .join("semantic")
            .join("FACTS")
            .join("test.md");
        let namespace = indexer.namespace_from_path(&facts_file).unwrap();
        assert_eq!(namespace, "semantic/facts");
    }
}
