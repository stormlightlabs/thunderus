//! Memory store implementation with SQLite FTS5 backend
//!
//! Provides durable storage and full-text search for memory documents.
use crate::error::{Error, Result};
use crate::migration::MigrationManager;
use crate::schema;

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, sync::Arc};
use thunderus_core::memory::MemoryKind;
use tokio_rusqlite::Connection;
use tracing::instrument;

/// Metadata associated with a stored memory document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMeta {
    /// Document ID (unique identifier)
    pub id: String,
    /// Memory kind (core, fact, adr, playbook, recap)
    pub kind: MemoryKind,
    /// Title extracted from frontmatter
    pub title: String,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Headings for anchor navigation
    #[serde(default)]
    pub headings: Vec<String>,
    /// Source file path (relative to repo root)
    pub path: String,
    /// Last updated timestamp
    pub updated: DateTime<Utc>,
    /// Provenance: event IDs that created/updated this doc
    #[serde(default)]
    pub event_ids: Vec<String>,
    /// Provenance: patch IDs associated with this doc
    #[serde(default)]
    pub patch_ids: Vec<String>,
    /// Approximate token count
    pub token_count: usize,
}

/// A search hit with snippet and citation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// The document ID
    pub id: String,
    /// Memory kind
    pub kind: MemoryKind,
    /// Document title
    pub title: String,
    /// File path for citation
    pub path: String,
    /// Matched heading anchor (if applicable)
    pub anchor: Option<String>,
    /// Snippet with highlighted matches
    pub snippet: String,
    /// BM25 relevance score (lower = better match)
    pub score: f64,
    /// Event IDs for provenance
    #[serde(default)]
    pub event_ids: Vec<String>,
}

/// Search filters for scoping queries
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Filter by memory kinds
    pub kinds: Option<Vec<MemoryKind>>,
    /// Filter by tags (any match)
    pub tags: Option<Vec<String>>,
    /// Filter by path prefix
    pub path_prefix: Option<String>,
    /// Maximum results to return
    pub limit: Option<usize>,
}

/// Store statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreStats {
    /// Total number of documents
    pub doc_count: usize,
    /// Documents by kind
    pub docs_by_kind: std::collections::HashMap<String, usize>,
    /// Index size in bytes
    pub index_size: u64,
    /// Last indexed timestamp
    pub last_indexed: DateTime<Utc>,
}

/// A handle to the memory store backed by SQLite FTS5
///
/// The store provides:
/// - Key-value access by namespace + key
/// - Full-text search with BM25 ranking
/// - Snippet extraction and highlighting
#[derive(Clone)]
pub struct MemoryStore {
    conn: Arc<Connection>,
}

impl MemoryStore {
    /// Open or create a memory store at the given path
    ///
    /// Creates the FTS5 virtual tables if they don't exist.
    #[instrument(skip_all, fields(db_path = %db_path.display()))]
    pub async fn open(db_path: &Path) -> Result<Self> {
        tracing::info!("Opening memory store at {}", db_path.display());

        let conn = Connection::open(db_path)
            .await
            .map_err(|e| Error::database(format!("Failed to open database: {e}")))?;

        conn.call(|conn| {
            tracing::debug!("Running migrations");
            MigrationManager::migrate(conn).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            tracing::trace!("Migrations complete");
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .map_err(|e| Error::database(format!("Migration failed: {e}")))?;

        tracing::info!("Memory store opened successfully");
        Ok(Self { conn: Arc::new(conn) })
    }

    /// Store a document in the memory store
    ///
    /// Indexes the content for full-text search.
    #[instrument(skip(self, content, meta), fields(namespace, key, id = %meta.id))]
    pub async fn put(&self, namespace: &str, key: &str, content: &str, meta: MemoryMeta) -> Result<()> {
        tracing::debug!("Putting document: {}/{}", namespace, key);

        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let content = content.to_owned();
        let meta_json = serde_json::to_string(&meta)?;

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    r#"
                    INSERT INTO memory_docs (id, namespace, key, content, meta_json)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT (namespace, key) DO UPDATE SET
                        id = excluded.id,
                        content = excluded.content,
                        meta_json = excluded.meta_json,
                        updated_at = datetime('now')
                    "#,
                )?;

                stmt.execute(params![&meta.id, &namespace, &key, &content, &meta_json])?;
                tracing::trace!("Document stored successfully");
                Ok::<_, rusqlite::Error>(())
            })
            .await?;

        Ok(())
    }

    /// Retrieve a document by namespace and key
    #[instrument(skip(self), fields(namespace, key))]
    pub async fn get(&self, namespace: &str, key: &str) -> Result<Option<(String, MemoryMeta)>> {
        tracing::trace!("Getting document: {}/{}", namespace, key);

        let namespace = namespace.to_owned();
        let key = key.to_owned();

        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn
                    .prepare_cached("SELECT content, meta_json FROM memory_docs WHERE namespace = ?1 AND key = ?2")?;

                let result = stmt
                    .query_row(params![&namespace, &key], |row| {
                        let content: String = row.get(0)?;
                        let meta_json: String = row.get(1)?;
                        let meta: MemoryMeta = serde_json::from_str(&meta_json)
                            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                        Ok((content, meta))
                    })
                    .optional()?;
                Ok::<_, rusqlite::Error>(result)
            })
            .await?;

        Ok(result)
    }

    /// Delete a document from the store
    #[instrument(skip(self), fields(namespace, key))]
    pub async fn delete(&self, namespace: &str, key: &str) -> Result<bool> {
        tracing::debug!("Deleting document: {}/{}", namespace, key);

        let namespace = namespace.to_owned();
        let key = key.to_owned();

        let deleted = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached("DELETE FROM memory_docs WHERE namespace = ?1 AND key = ?2")?;
                let rows_affected = stmt.execute(params![&namespace, &key])?;
                Ok::<_, rusqlite::Error>(rows_affected > 0)
            })
            .await?;

        Ok(deleted)
    }

    /// Search the memory store with a full-text query
    ///
    /// Returns ranked hits with snippets and citations.
    /// Uses FTS5 BM25 for relevance ranking.
    #[instrument(skip(self, filters), fields(query, limit = filters.limit.unwrap_or(10)))]
    pub async fn search(&self, query: &str, filters: SearchFilters) -> Result<Vec<SearchHit>> {
        tracing::debug!("Searching with query: {}", query);

        let query = query.to_owned();
        let limit = filters.limit.unwrap_or(10) as i64;
        let join_clause = "INNER JOIN memory_docs ON memory_fts.id = memory_docs.id";
        let (mut where_clauses, mut filter_values): (Vec<String>, Vec<Box<dyn rusqlite::ToSql + Send + Sync>>) =
            (Vec::new(), Vec::new());

        if let Some(kinds) = &filters.kinds {
            let kind_placeholders: Vec<String> = kinds.iter().map(|_| "?".to_string()).collect();
            let kind_filter = if kinds.len() == 1 {
                format!(
                    "CAST(json_extract(memory_docs.meta_json, '$.kind') AS TEXT) = {}",
                    kind_placeholders[0]
                )
            } else {
                format!(
                    "CAST(json_extract(memory_docs.meta_json, '$.kind') AS TEXT) IN ({})",
                    kind_placeholders.join(", ")
                )
            };
            where_clauses.push(kind_filter);
            for kind in kinds {
                let kind_json = serde_json::to_string(kind).unwrap();
                let kind_value: String = kind_json.trim_matches('"').to_string();
                filter_values.push(Box::new(kind_value));
            }
        }

        if let Some(prefix) = &filters.path_prefix {
            where_clauses.push("memory_fts.path LIKE ?".to_string());
            filter_values.push(Box::new(format!("{}%", prefix)));
        }

        let where_clause_filters = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" AND {}", where_clauses.join(" AND "))
        };
        let where_clause = format!("WHERE memory_fts MATCH ?{}", where_clause_filters);

        let sql = format!(
            r#"
            SELECT
                memory_fts.id,
                memory_fts.title,
                memory_fts.kind,
                memory_fts.path,
                snippet(memory_fts, 1, '<b>', '</b>', '...', 32) as snippet,
                bm25(memory_fts) as score,
                json_extract(memory_docs.meta_json, '$.event_ids') as event_ids
            FROM memory_fts
            {}
            {}
            ORDER BY score, memory_fts.id
            LIMIT ?
            "#,
            join_clause, where_clause
        );

        let hits = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(&sql)?;
                let mut param_values: Vec<&dyn rusqlite::ToSql> = vec![&query];
                for v in &filter_values {
                    param_values.push(v.as_ref());
                }
                param_values.push(&limit);

                let hits = stmt
                    .query_map(param_values.as_slice(), |row| {
                        let kind_raw: String = row.get(2)?;
                        let kind_str: String = serde_json::from_str(&kind_raw).unwrap_or_else(|_| kind_raw.clone());
                        let kind: MemoryKind =
                            serde_json::from_str(&format!("\"{}\"", kind_str)).unwrap_or(MemoryKind::Core);

                        let event_ids_raw: Option<String> = row.get(6)?;
                        let event_ids: Vec<String> = match event_ids_raw {
                            Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
                            None => Vec::new(),
                        };

                        Ok(SearchHit {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            kind,
                            path: row.get(3)?,
                            anchor: None,
                            snippet: row.get(4)?,
                            score: row.get(5)?,
                            event_ids,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;

                Ok::<_, rusqlite::Error>(hits)
            })
            .await?;

        tracing::debug!("Search returned {} hits", hits.len());
        Ok(hits)
    }

    /// Vector similarity search
    ///
    /// Finds similar documents using cosine similarity of embeddings.
    /// Returns SearchHits with similarity-based scores (higher = better match).
    #[instrument(skip(self, query_embedding, filters), fields(limit = filters.limit.unwrap_or(10)))]
    pub async fn vector_search(&self, query_embedding: &[f32], filters: SearchFilters) -> Result<Vec<SearchHit>> {
        tracing::debug!("Vector similarity search with {} dimensions", query_embedding.len());

        let vector_store = crate::VectorStore::new(self.conn.clone());
        let similar_docs = vector_store
            .find_similar(query_embedding, filters.limit.unwrap_or(10), 0.5)
            .await
            .map_err(|e| Error::database(format!("Vector search failed: {e}")))?;

        if similar_docs.is_empty() {
            tracing::debug!("Vector search returned no results");
            return Ok(Vec::new());
        }

        let similarity_map: std::collections::HashMap<String, f64> =
            similar_docs.iter().map(|d| (d.doc_id.clone(), d.similarity)).collect();

        let doc_ids: Vec<String> = similarity_map.keys().cloned().collect();

        let hits = self
            .conn
            .call(move |conn| {
                let placeholders: Vec<String> = doc_ids.iter().map(|_| "?".to_string()).collect();
                let placeholder_list = placeholders.join(",");

                let sql = format!(
                    r#"
                    SELECT
                        d.id,
                        json_extract(d.meta_json, '$.title') as title,
                        json_extract(d.meta_json, '$.path') as path,
                        d.content,
                        json_extract(d.meta_json, '$.event_ids') as event_ids,
                        json_extract(d.meta_json, '$.kind') as kind_raw
                    FROM memory_docs d
                    WHERE d.id IN ({})
                    "#,
                    placeholder_list
                );

                let mut stmt = conn.prepare(&sql)?;

                let mut param_values: Vec<&dyn rusqlite::ToSql> = Vec::new();
                for id in &doc_ids {
                    param_values.push(id);
                }

                let mut hits_by_id: std::collections::HashMap<String, SearchHit> = stmt
                    .query_map(param_values.as_slice(), |row| {
                        let id: String = row.get(0)?;
                        let title: String = row.get(1)?;
                        let path: String = row.get(2)?;
                        let content: String = row.get(3)?;
                        let event_ids_raw: Option<String> = row.get(4)?;
                        let kind_raw: String = row.get(5)?;

                        let event_ids: Vec<String> = match event_ids_raw {
                            Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
                            None => Vec::new(),
                        };

                        let kind: MemoryKind =
                            serde_json::from_str(&format!("\"{}\"", kind_raw)).unwrap_or(MemoryKind::Core);

                        Ok((
                            id.clone(),
                            SearchHit {
                                id,
                                kind,
                                title,
                                path,
                                anchor: None,
                                snippet: content.chars().take(200).collect::<String>() + "...",
                                score: 0.0,
                                event_ids,
                            },
                        ))
                    })?
                    .collect::<std::result::Result<_, _>>()?;

                let hits = doc_ids
                    .into_iter()
                    .filter_map(|doc_id| {
                        let similarity = similarity_map.get(&doc_id)?;
                        let mut hit = hits_by_id.remove(&doc_id)?;
                        hit.score = -similarity;
                        Some(hit)
                    })
                    .collect::<Vec<_>>();

                Ok::<_, rusqlite::Error>(hits)
            })
            .await?;

        tracing::debug!("Vector search returned {} hits", hits.len());
        Ok(hits)
    }

    /// Hybrid search: FTS5 + vector similarity
    ///
    /// Combines lexical (FTS5) and vector similarity search.
    /// Uses vector search only when lexical confidence is low.
    #[instrument(skip(self, query, query_embedding, filters), fields(limit = filters.limit.unwrap_or(10), fts_threshold))]
    pub async fn hybrid_search(
        &self, query: &str, query_embedding: &[f32], filters: SearchFilters, fts_threshold: f64,
    ) -> Result<Vec<SearchHit>> {
        tracing::debug!("Hybrid search with FTS threshold {}", fts_threshold);

        let fts_hits = self.search(query, filters.clone()).await?;

        let best_fts_score = fts_hits.first().map(|h| h.score).unwrap_or(0.0);

        if best_fts_score > fts_threshold || fts_hits.len() < 3 {
            tracing::debug!(
                "FTS confidence low (score: {}, hits: {}), augmenting with vector search",
                best_fts_score,
                fts_hits.len()
            );

            let vector_hits = self.vector_search(query_embedding, filters).await?;

            if vector_hits.is_empty() {
                tracing::debug!("Vector search returned no results, using FTS only");
                return Ok(fts_hits);
            }

            tracing::debug!(
                "Merging {} FTS hits and {} vector hits",
                fts_hits.len(),
                vector_hits.len()
            );

            return Ok(self.merge_hits(fts_hits, vector_hits));
        }

        tracing::debug!(
            "FTS confidence sufficient (score: {}), skipping vector search",
            best_fts_score
        );
        Ok(fts_hits)
    }

    /// Merge FTS and vector hits, removing duplicates and re-ranking
    ///
    /// Prioritizes documents that appear in both result sets.
    fn merge_hits(&self, mut fts_hits: Vec<SearchHit>, mut vector_hits: Vec<SearchHit>) -> Vec<SearchHit> {
        let mut merged: HashMap<String, SearchHit> = HashMap::new();

        for hit in fts_hits.drain(..) {
            merged.insert(hit.id.clone(), hit);
        }

        for hit in vector_hits.drain(..) {
            if let Some(existing) = merged.get_mut(&hit.id) {
                existing.score = (existing.score + hit.score) / 2.0;
            } else {
                merged.insert(hit.id.clone(), hit);
            }
        }

        let mut results: Vec<_> = merged.into_values().collect();
        results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Rebuild the FTS index from all stored documents
    ///
    /// Use after bulk operations or to recover from corruption.
    #[instrument(skip(self))]
    pub async fn rebuild_index(&self) -> Result<()> {
        tracing::info!("Rebuilding FTS index");

        self.conn
            .call(|conn| {
                conn.execute_batch(
                    r#"
                    DROP TABLE IF EXISTS memory_fts;
                    DROP TRIGGER IF EXISTS memory_docs_ai;
                    DROP TRIGGER IF EXISTS memory_docs_ad;
                    DROP TRIGGER IF EXISTS memory_docs_au;
                    "#,
                )?;

                conn.execute_batch(schema::MEMORY_FTS_SQL)?;
                conn.execute_batch(schema::FTS_TRIGGERS_SQL)?;

                conn.execute(
                    r#"
                    INSERT INTO memory_fts(id, title, headings, tags, body, path, kind)
                    SELECT
                        id,
                        json_extract(meta_json, '$.title'),
                        json_extract(meta_json, '$.headings'),
                        json_extract(meta_json, '$.tags'),
                        content,
                        json_extract(meta_json, '$.path'),
                        json_extract(meta_json, '$.kind')
                    FROM memory_docs
                    "#,
                    [],
                )?;

                tracing::trace!("Index rebuild complete");
                Ok::<_, rusqlite::Error>(())
            })
            .await?;

        tracing::info!("FTS index rebuilt successfully");
        Ok(())
    }

    /// Get store statistics
    #[instrument(skip(self))]
    pub async fn stats(&self) -> Result<StoreStats> {
        tracing::debug!("Getting store statistics");

        let stats = self
            .conn
            .call(|conn| {
                let doc_count: i64 = conn.query_row("SELECT COUNT(*) FROM memory_docs", [], |row| row.get(0))?;
                let mut stmt = conn.prepare(
                    "SELECT json_extract(meta_json, '$.kind'), COUNT(*) FROM memory_docs GROUP BY json_extract(meta_json, '$.kind')"
                )?;

                let kind_rows = stmt.query_map([], |row| {
                    let kind: Option<String> = row.get(0)?;
                    let count: i64 = row.get(1)?;
                    Ok((kind.unwrap_or_default(), count))
                })?;

                let mut docs_by_kind = std::collections::HashMap::new();
                for row in kind_rows {
                    let (kind, count) = row?;
                    docs_by_kind.insert(kind, count as usize);
                }

                let last_indexed = if doc_count == 0 {
                    Utc::now()
                } else {
                    let last_updated: String =
                        conn.query_row("SELECT MAX(updated_at) FROM memory_docs", [], |row| row.get(0))?;
                    DateTime::parse_from_str(&format!("{}+00:00", last_updated), "%Y-%m-%d %H:%M:%S%z")
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc)
                };

                let index_size = 0u64;

                Ok::<_, rusqlite::Error>(StoreStats {
                    doc_count: doc_count as usize,
                    docs_by_kind,
                    index_size,
                    last_indexed,
                })
            })
            .await?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::SCHEMA_VERSION;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_store_open() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await;
        assert!(store.is_ok());
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_fresh_store_opens_with_correct_version() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let _store = MemoryStore::open(&db_path).await.unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let version = MigrationManager::get_current_version(&conn).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_put_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "test-doc-1".to_string(),
            kind: MemoryKind::Fact,
            title: "Test Document".to_string(),
            tags: vec!["test".to_string(), "example".to_string()],
            headings: vec!["introduction".to_string()],
            path: "semantic/FACTS/test.md".to_string(),
            updated: Utc::now(),
            event_ids: vec!["evt-1".to_string()],
            patch_ids: vec![],
            token_count: 100,
        };

        let content = "# Test Document\n\nThis is a test document.";
        store.put("semantic/facts", "test.md", content, meta).await.unwrap();

        let retrieved = store.get("semantic/facts", "test.md").await.unwrap();
        assert!(retrieved.is_some());
        let (retrieved_content, retrieved_meta) = retrieved.unwrap();
        assert_eq!(retrieved_content, content);
        assert_eq!(retrieved_meta.id, "test-doc-1");
        assert_eq!(retrieved_meta.kind, MemoryKind::Fact);
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();
        let result = store.get("core", "nonexistent.md").await.unwrap();
        assert!(result.is_none());
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_delete() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "delete-test".to_string(),
            kind: MemoryKind::Core,
            title: "Delete Test".to_string(),
            tags: vec![],
            headings: vec![],
            path: "core/test.md".to_string(),
            updated: Utc::now(),
            event_ids: vec![],
            patch_ids: vec![],
            token_count: 50,
        };

        store.put("core", "test.md", "content", meta).await.unwrap();
        let deleted = store.delete("core", "test.md").await.unwrap();
        assert!(deleted);

        let retrieved = store.get("core", "test.md").await.unwrap();
        assert!(retrieved.is_none());
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_search() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "search-test".to_string(),
            kind: MemoryKind::Fact,
            title: "Testing Coverage".to_string(),
            tags: vec!["testing".to_string()],
            headings: vec!["coverage-requirements".to_string()],
            path: "semantic/FACTS/testing.md".to_string(),
            updated: Utc::now(),
            event_ids: vec![],
            patch_ids: vec![],
            token_count: 100,
        };

        let content = "# Testing Coverage\n\nMinimum line coverage: 80%. Use cargo llvm-cov.";
        store.put("semantic/facts", "testing.md", content, meta).await.unwrap();

        let hits = store.search("coverage", SearchFilters::default()).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].kind, MemoryKind::Fact);
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_search_with_filters() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let fact_meta = MemoryMeta {
            id: "fact-1".to_string(),
            kind: MemoryKind::Fact,
            title: "Test Fact".to_string(),
            tags: vec!["test".to_string()],
            headings: vec![],
            path: "semantic/FACTS/test.md".to_string(),
            updated: Utc::now(),
            event_ids: vec![],
            patch_ids: vec![],
            token_count: 50,
        };
        store
            .put("semantic/facts", "test.md", "test content", fact_meta)
            .await
            .unwrap();

        let adr_meta = MemoryMeta {
            id: "adr-1".to_string(),
            kind: MemoryKind::Adr,
            title: "Test ADR".to_string(),
            tags: vec!["test".to_string()],
            headings: vec![],
            path: "semantic/DECISIONS/adr-001.md".to_string(),
            updated: Utc::now(),
            event_ids: vec![],
            patch_ids: vec![],
            token_count: 50,
        };
        store
            .put("semantic/decisions", "adr-001.md", "test content", adr_meta)
            .await
            .unwrap();

        let all_hits = store.search("test", SearchFilters::default()).await.unwrap();
        assert_eq!(all_hits.len(), 2, "Search without filters should return 2 results");

        let filters = SearchFilters { kinds: Some(vec![MemoryKind::Fact]), ..Default::default() };
        let hits = store.search("test", filters).await.unwrap();
        assert_eq!(hits.len(), 1, "Search with Fact filter should return 1 result");
        assert_eq!(hits[0].kind, MemoryKind::Fact);
        let _ = temp_dir;
    }

    #[tokio::test]
    async fn test_stats() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "stats-test".to_string(),
            kind: MemoryKind::Core,
            title: "Stats Test".to_string(),
            tags: vec![],
            headings: vec![],
            path: "core/test.md".to_string(),
            updated: Utc::now(),
            event_ids: vec![],
            patch_ids: vec![],
            token_count: 75,
        };

        store.put("core", "test.md", "content", meta).await.unwrap();

        let stats = store.stats().await.unwrap();
        assert_eq!(stats.doc_count, 1);
        let _ = temp_dir;
    }
}
