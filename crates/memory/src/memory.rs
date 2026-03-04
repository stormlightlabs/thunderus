//! Core memory operations: store, recall, forget, decay
//!
//! Uses sqlite-vec for efficient vector similarity search.

use super::{
    Embedding, MemoryDatabase, MemoryError, MemoryStats, Result,
    embedding::{EmbeddingConfig, generate_embedding, generate_fallback_embedding},
};
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Types of memories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Procedure,
    Context,
}

impl MemoryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Procedure => "procedure",
            MemoryKind::Context => "context",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "fact" => Some(MemoryKind::Fact),
            "preference" => Some(MemoryKind::Preference),
            "procedure" => Some(MemoryKind::Procedure),
            "context" => Some(MemoryKind::Context),
            _ => None,
        }
    }
}

/// A memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub content: String,
    pub kind: MemoryKind,
    pub source: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub accessed_at: DateTime<Utc>,
    pub access_count: i64,
    pub similarity: Option<f32>,
}

impl Memory {
    /// Format for display in system prompt
    pub fn to_prompt_string(&self) -> String {
        format!("- [{}] {}", self.kind.as_str(), self.content)
    }
}

/// Store and retrieve memories
pub struct MemoryStore {
    db: MemoryDatabase,
    embedding_config: EmbeddingConfig,
}

impl MemoryStore {
    /// Create a new memory store
    pub fn new(db: MemoryDatabase) -> Self {
        let embedding_config = EmbeddingConfig::from_env();
        let mut store = Self { db, embedding_config };

        if let Err(error) = store.decay_sync(90, 1) {
            tracing::warn!("Failed to run startup memory decay: {}", error);
        }

        store
    }

    /// Store a new memory
    pub async fn store(
        &mut self, content: impl Into<String>, kind: MemoryKind, source: Option<impl Into<String>>,
        tags: Vec<impl Into<String>>,
    ) -> Result<i64> {
        let content: String = content.into();
        let content_trimmed = content.trim();

        if content_trimmed.is_empty() {
            return Err(MemoryError::Embedding("Content cannot be empty".to_string()));
        }

        if content_trimmed.len() > 4096 {
            return Err(MemoryError::Embedding(format!(
                "Content too long: {} bytes (max 4096)",
                content_trimmed.len()
            )));
        }

        let embedding = if self.embedding_config.is_enabled() {
            generate_embedding(&content, &self.embedding_config).await?
        } else {
            generate_fallback_embedding(&content, self.embedding_config.dimensions)
        };

        let existing = if self.db.has_vec_support() {
            self.find_similar_vec(&embedding, 1).await?
        } else {
            self.find_similar_blob(&embedding, 1, None, None).await?
        };

        if let Some((existing_id, similarity)) = existing.first()
            && *similarity > 0.95
        {
            self.update_memory(*existing_id, &content, &embedding).await?;
            return Ok(*existing_id);
        }

        if !self.db.has_vec_support()
            && let Some(existing_id) = self.find_by_content(content_trimmed).await?
        {
            self.update_memory(existing_id, &content, &embedding).await?;
            return Ok(existing_id);
        }

        let source_str = source.map(|s| s.into());
        let tags_str = if tags.is_empty() {
            None
        } else {
            Some(tags.into_iter().map(|t| t.into()).collect::<Vec<_>>().join(","))
        };

        let has_vec = self.db.has_vec_support();

        let tx = self.db.conn_mut().transaction()?;

        tx.execute(
            "INSERT INTO memories (content, kind, source, tags) VALUES (?1, ?2, ?3, ?4)",
            params![content_trimmed, kind.as_str(), source_str, tags_str],
        )?;

        let memory_id = tx.last_insert_rowid();

        tx.execute(
            "INSERT INTO embeddings (memory_id, vector, model, dimensions) VALUES (?1, ?2, ?3, ?4)",
            params![
                memory_id,
                embedding.to_bytes(),
                &embedding.model,
                embedding.dimensions as i64
            ],
        )?;

        if has_vec {
            let _ = tx.execute(
                "INSERT INTO vec_memories (memory_id, embedding) VALUES (?1, ?2)",
                params![memory_id, embedding.to_json()],
            );
        }

        tx.commit()?;

        Ok(memory_id)
    }

    /// Recall memories matching a query
    pub async fn recall(
        &mut self, query: &str, k: usize, kind: Option<MemoryKind>, tags: Option<&[&str]>, min_similarity: f32,
    ) -> Result<Vec<Memory>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding = if self.embedding_config.is_enabled() {
            generate_embedding(query, &self.embedding_config).await?
        } else {
            generate_fallback_embedding(query, self.embedding_config.dimensions)
        };

        let similar = if self.db.has_vec_support() {
            self.find_similar_vec(&query_embedding, k * 3).await?
        } else {
            self.find_similar_blob(&query_embedding, k * 3, kind, tags).await?
        };

        let kind_str = kind.map(|k| k.as_str().to_string());
        let tag_filter = tags.map(|t| t.join(","));

        let mut results = Vec::new();
        for (memory_id, similarity) in similar {
            if similarity < min_similarity {
                continue;
            }

            if let Some(mut memory) = self.get_memory(memory_id).await? {
                if let Some(ref k) = kind_str
                    && memory.kind.as_str() != k
                {
                    continue;
                }

                if let Some(ref t) = tag_filter {
                    let has_tag = memory.tags.iter().any(|tag| t.contains(tag));
                    if !has_tag {
                        continue;
                    }
                }

                memory.similarity = Some(similarity);
                self.update_access_stats(memory_id).await?;
                results.push(memory);

                if results.len() >= k {
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Find memories similar to an embedding using sqlite-vec
    async fn find_similar_vec(&self, query_embedding: &Embedding, k: usize) -> Result<Vec<(i64, f32)>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT memory_id, distance
            FROM vec_memories
            WHERE embedding MATCH ?1
            ORDER BY distance
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![query_embedding.to_json(), k as i64], |row| {
            let memory_id: i64 = row.get(0)?;
            let distance: f64 = row.get(1)?;
            let similarity = (1.0 - distance) as f32;
            Ok((memory_id, similarity))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Find memories by brute-force cosine similarity with SQL metadata filtering.
    async fn find_similar_blob(
        &self, query_embedding: &Embedding, k: usize, kind: Option<MemoryKind>, tags: Option<&[&str]>,
    ) -> Result<Vec<(i64, f32)>> {
        let kind_str = kind.map(|k| k.as_str().to_string());
        let tag_filter = tags.and_then(|t| t.first().copied());

        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT m.id, e.vector, e.model, e.dimensions
            FROM memories m
            JOIN embeddings e ON e.memory_id = m.id
            WHERE m.archived = 0
              AND (?1 IS NULL OR m.kind = ?1)
              AND (?2 IS NULL OR m.tags LIKE '%' || ?2 || '%')
            "#,
        )?;

        let rows = stmt.query_map(params![kind_str, tag_filter], |row| {
            let id: i64 = row.get(0)?;
            let vector: Vec<u8> = row.get(1)?;
            let model: String = row.get(2)?;
            let dimensions: i64 = row.get(3)?;
            Ok((id, vector, model, dimensions as usize))
        })?;

        let mut scored = Vec::new();
        for row in rows {
            let (id, vector, model, dimensions) = row?;
            let stored = Embedding::from_bytes(&vector, dimensions, model)?;
            let similarity = cosine_similarity(query_embedding, &stored);
            scored.push((id, similarity));
        }

        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);

        Ok(scored)
    }

    /// Find a memory by exact content match
    async fn find_by_content(&self, content: &str) -> Result<Option<i64>> {
        let result = self.db.conn().query_row(
            "SELECT id FROM memories WHERE content = ?1 AND archived = 0",
            params![content],
            |row| row.get::<usize, i64>(0),
        );

        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get a memory by ID
    async fn get_memory(&self, id: i64) -> Result<Option<Memory>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, content, kind, source, tags, created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE id = ?1 AND archived = 0
            "#,
        )?;

        let result = stmt.query_row(params![id], |row| {
            let kind_str: String = row.get(2)?;
            let tags_str: Option<String> = row.get(4)?;

            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                kind: MemoryKind::from_string(&kind_str).unwrap_or(MemoryKind::Fact),
                source: row.get(3)?,
                tags: tags_str
                    .map(|t| t.split(',').map(|s| s.to_string()).collect())
                    .unwrap_or_default(),
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                accessed_at: row.get(7)?,
                access_count: row.get(8)?,
                similarity: None,
            })
        });

        match result {
            Ok(memory) => Ok(Some(memory)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update memory content and timestamp
    async fn update_memory(&mut self, id: i64, content: &str, embedding: &Embedding) -> Result<()> {
        self.db.conn_mut().execute(
            "UPDATE memories SET content = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![content, id],
        )?;

        self.db.conn_mut().execute(
            "UPDATE embeddings SET vector = ?1, model = ?2, dimensions = ?3 WHERE memory_id = ?4",
            params![embedding.to_bytes(), &embedding.model, embedding.dimensions as i64, id],
        )?;

        if self.db.has_vec_support() {
            let _ = self
                .db
                .conn_mut()
                .execute("DELETE FROM vec_memories WHERE memory_id = ?1", params![id]);

            let _ = self.db.conn_mut().execute(
                "INSERT INTO vec_memories (memory_id, embedding) VALUES (?1, ?2)",
                params![id, embedding.to_json()],
            );
        }

        Ok(())
    }

    /// Update access statistics for a memory
    async fn update_access_stats(&mut self, id: i64) -> Result<()> {
        self.db.conn_mut().execute(
            "UPDATE memories SET accessed_at = datetime('now'), access_count = access_count + 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Soft-delete (archive) a memory
    pub async fn forget(&mut self, id: i64) -> Result<bool> {
        let rows = self.db.conn_mut().execute(
            "UPDATE memories SET archived = 1, updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;

        let _ = self
            .db
            .conn_mut()
            .execute("DELETE FROM embeddings WHERE memory_id = ?1", params![id]);

        if self.db.has_vec_support() {
            let _ = self
                .db
                .conn_mut()
                .execute("DELETE FROM vec_memories WHERE memory_id = ?1", params![id]);
        }

        Ok(rows > 0)
    }

    /// Hard delete a memory
    pub async fn delete(&mut self, id: i64) -> Result<bool> {
        let _ = self
            .db
            .conn_mut()
            .execute("DELETE FROM embeddings WHERE memory_id = ?1", params![id]);

        if self.db.has_vec_support() {
            let _ = self
                .db
                .conn_mut()
                .execute("DELETE FROM vec_memories WHERE memory_id = ?1", params![id]);
        }

        let rows = self
            .db
            .conn_mut()
            .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    /// Archive old memories (decay)
    pub async fn decay(&mut self, max_age_days: i64, min_access_count: i64) -> Result<usize> {
        let ids_to_archive: Vec<i64> = {
            let mut stmt = self.db.conn().prepare(
                r#"
                SELECT id
                FROM memories
                WHERE archived = 0
                  AND accessed_at < datetime('now', '-' || ?1 || ' days')
                  AND access_count < ?2
                "#,
            )?;

            let rows = stmt.query_map(params![max_age_days, min_access_count], |row| {
                let id: i64 = row.get(0)?;
                Ok(id)
            })?;

            rows.filter_map(|r| r.ok()).collect()
        };

        if self.db.has_vec_support() {
            for id in &ids_to_archive {
                let _ = self
                    .db
                    .conn_mut()
                    .execute("DELETE FROM vec_memories WHERE memory_id = ?1", params![id]);
            }
        }

        for id in &ids_to_archive {
            let _ = self
                .db
                .conn_mut()
                .execute("DELETE FROM embeddings WHERE memory_id = ?1", params![id]);
        }

        let rows = self.db.conn_mut().execute(
            r#"
            UPDATE memories
            SET archived = 1, updated_at = datetime('now')
            WHERE archived = 0
              AND accessed_at < datetime('now', '-' || ?1 || ' days')
              AND access_count < ?2
            "#,
            params![max_age_days, min_access_count],
        )?;

        Ok(rows)
    }

    /// Get memory statistics
    pub fn stats(&self) -> Result<MemoryStats> {
        let total_memories: i64 =
            self.db
                .conn()
                .query_row("SELECT COUNT(*) FROM memories WHERE archived = 0", [], |row| row.get(0))?;

        let archived_memories: i64 =
            self.db
                .conn()
                .query_row("SELECT COUNT(*) FROM memories WHERE archived = 1", [], |row| row.get(0))?;

        let total_sessions: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;

        let db_size = self.db.size_bytes()?;

        Ok(MemoryStats {
            total_memories: total_memories as usize,
            archived_memories: archived_memories as usize,
            total_sessions: total_sessions as usize,
            database_size_bytes: db_size,
            embedding_model: Some(self.embedding_config.model.clone()),
            embedding_dimensions: self.embedding_config.dimensions,
        })
    }

    /// List all memories with optional filtering
    pub fn list(&self, kind: Option<MemoryKind>, limit: usize) -> Result<Vec<Memory>> {
        let kind_str = kind.map(|k| k.as_str().to_string());

        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, content, kind, source, tags, created_at, updated_at, accessed_at, access_count
            FROM memories
            WHERE archived = 0
              AND (?1 IS NULL OR kind = ?1)
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![kind_str, limit as i64], |row| {
            let kind_str: String = row.get(2)?;
            let tags_str: Option<String> = row.get(4)?;

            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                kind: MemoryKind::from_string(&kind_str).unwrap_or(MemoryKind::Fact),
                source: row.get(3)?,
                tags: tags_str
                    .map(|t| t.split(',').map(|s| s.to_string()).collect())
                    .unwrap_or_default(),
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                accessed_at: row.get(7)?,
                access_count: row.get(8)?,
                similarity: None,
            })
        })?;

        let mut memories = Vec::new();
        for row in rows {
            memories.push(row?);
        }

        Ok(memories)
    }

    /// Synchronous decay for startup maintenance.
    fn decay_sync(&mut self, max_age_days: i64, min_access_count: i64) -> Result<usize> {
        let ids_to_archive: Vec<i64> = {
            let mut stmt = self.db.conn().prepare(
                r#"
                SELECT id
                FROM memories
                WHERE archived = 0
                  AND accessed_at < datetime('now', '-' || ?1 || ' days')
                  AND access_count < ?2
                "#,
            )?;

            let rows = stmt.query_map(params![max_age_days, min_access_count], |row| {
                let id: i64 = row.get(0)?;
                Ok(id)
            })?;

            rows.filter_map(|r| r.ok()).collect()
        };

        for id in &ids_to_archive {
            let _ = self
                .db
                .conn_mut()
                .execute("DELETE FROM embeddings WHERE memory_id = ?1", params![id]);

            if self.db.has_vec_support() {
                let _ = self
                    .db
                    .conn_mut()
                    .execute("DELETE FROM vec_memories WHERE memory_id = ?1", params![id]);
            }
        }

        let rows = self.db.conn_mut().execute(
            r#"
            UPDATE memories
            SET archived = 1, updated_at = datetime('now')
            WHERE archived = 0
              AND accessed_at < datetime('now', '-' || ?1 || ' days')
              AND access_count < ?2
            "#,
            params![max_age_days, min_access_count],
        )?;

        Ok(rows)
    }
}

fn cosine_similarity(a: &Embedding, b: &Embedding) -> f32 {
    if a.dimensions != b.dimensions {
        return 0.0;
    }

    a.vector.dot(&b.vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_store() -> (TempDir, MemoryStore) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = crate::MemoryDatabase::open(&db_path, false).unwrap();
        let store = MemoryStore::new(db);
        (temp, store)
    }

    #[tokio::test]
    async fn test_store_and_recall() {
        let (_, mut store) = create_test_store().await;

        let id = store
            .store(
                "Rust is a systems programming language",
                MemoryKind::Fact,
                Option::<&str>::None,
                Vec::<&str>::new(),
            )
            .await
            .unwrap();

        assert!(id > 0);

        let results = store.recall("programming languages", 5, None, None, 0.0).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_deduplication() {
        let (_, mut store) = create_test_store().await;

        let id1 = store
            .store(
                "Rust is a systems programming language",
                MemoryKind::Fact,
                Option::<&str>::None,
                Vec::<&str>::new(),
            )
            .await
            .unwrap();

        let id2 = store
            .store(
                "Rust is a systems programming language",
                MemoryKind::Fact,
                Option::<&str>::None,
                Vec::<&str>::new(),
            )
            .await
            .unwrap();

        assert_eq!(id1, id2);
    }
}
