//! Vector embeddings storage and search
//!
//! This module provides optional vector similarity search functionality
//! that can augment the lexical FTS5 search when enabled.

use crate::{Error, Result};

use rusqlite::{OptionalExtension, params};
use std::sync::Arc;
use tokio_rusqlite::Connection;

/// Vector embeddings storage and search
///
/// Provides vector similarity search using cosine similarity.
/// This is an optional layer that augments FTS5 when enabled.
pub struct VectorStore {
    conn: Arc<Connection>,
}

impl VectorStore {
    /// Create a new vector store using the existing connection
    ///
    /// The vector embeddings table is created as part of the main schema
    /// during MemoryStore initialization, so no separate init is needed.
    pub(crate) fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }

    /// Store an embedding for a document
    pub async fn put_embedding(&self, doc_id: &str, embedding: &[f32]) -> Result<()> {
        let doc_id = doc_id.to_owned();
        let blob = Self::serialize_embedding(embedding);
        let dims = embedding.len() as i32;

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached(
                    r#"
                    INSERT INTO memory_embeddings (doc_id, embedding, dims)
                    VALUES (?1, ?2, ?3)
                    ON CONFLICT (doc_id) DO UPDATE SET
                        embedding = excluded.embedding,
                        dims = excluded.dims,
                        created_at = datetime('now')
                    "#,
                )?;

                stmt.execute(params![&doc_id, &blob, dims])?;
                Ok::<_, rusqlite::Error>(())
            })
            .await
            .map_err(|e| Error::database(format!("Failed to store embedding: {e}")))?;

        Ok(())
    }

    /// Get an embedding for a document
    pub async fn get_embedding(&self, doc_id: &str) -> Result<Option<Vec<f32>>> {
        let doc_id = doc_id.to_owned();

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached("SELECT embedding FROM memory_embeddings WHERE doc_id = ?1")?;

                let result = stmt
                    .query_row(params![&doc_id], |row| {
                        let blob: Vec<u8> = row.get(0)?;
                        Ok(Self::deserialize_embedding(&blob))
                    })
                    .optional()?;
                Ok::<_, rusqlite::Error>(result)
            })
            .await
            .map_err(|e| Error::database(format!("Failed to get embedding: {e}")))
    }

    /// Delete an embedding for a document
    pub async fn delete_embedding(&self, doc_id: &str) -> Result<bool> {
        let doc_id = doc_id.to_owned();

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare_cached("DELETE FROM memory_embeddings WHERE doc_id = ?1")?;
                let rows_affected = stmt.execute(params![&doc_id])?;
                Ok::<_, rusqlite::Error>(rows_affected > 0)
            })
            .await
            .map_err(|e| Error::database(format!("Failed to delete embedding: {e}")))
    }

    /// Find similar documents using cosine similarity
    ///
    /// Returns document IDs sorted by similarity (highest first).
    /// The threshold is the minimum cosine similarity (0.0 to 1.0).
    pub async fn find_similar(&self, query_embedding: &[f32], limit: usize, threshold: f64) -> Result<Vec<SimilarDoc>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let query_embedding = query_embedding.to_vec();

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT doc_id, embedding, dims
                    FROM memory_embeddings
                    "#,
                )?;

                let rows = stmt.query_map([], |row| {
                    let doc_id: String = row.get(0)?;
                    let blob: Vec<u8> = row.get(1)?;
                    let dims: i32 = row.get(2)?;
                    Ok((doc_id, blob, dims))
                })?;

                let mut similar_docs = Vec::new();

                for row in rows {
                    let (doc_id, blob, _dims) = row?;
                    let doc_embedding = Self::deserialize_embedding(&blob);
                    let similarity = Self::cosine_similarity(&query_embedding, &doc_embedding);

                    if similarity >= threshold {
                        similar_docs.push(SimilarDoc { doc_id, similarity });
                    }
                }

                similar_docs.sort_by(|a, b| {
                    b.similarity
                        .partial_cmp(&a.similarity)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                similar_docs.truncate(limit);

                Ok::<_, rusqlite::Error>(similar_docs)
            })
            .await
            .map_err(|e| Error::database(format!("Failed to find similar documents: {e}")))
    }

    /// Calculate cosine similarity between two embeddings
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let mut dot_product = 0.0_f64;
        let mut norm_a = 0.0_f64;
        let mut norm_b = 0.0_f64;

        for (x, y) in a.iter().zip(b.iter()) {
            let x = *x as f64;
            let y = *y as f64;
            dot_product += x * y;
            norm_a += x * x;
            norm_b += y * y;
        }

        let denominator = norm_a.sqrt() * norm_b.sqrt();
        if denominator == 0.0 { 0.0 } else { dot_product / denominator }
    }

    /// Serialize embedding to bytes
    fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|&f| f.to_le_bytes()).collect()
    }

    /// Deserialize embedding from bytes
    fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }
}

/// A document with similarity score
#[derive(Debug, Clone)]
pub struct SimilarDoc {
    /// Document ID
    pub doc_id: String,
    /// Cosine similarity (0.0 to 1.0, higher is better)
    pub similarity: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio_rusqlite::Connection;

    /// Helper function to create the vector embeddings table for tests
    async fn create_test_table(conn: &Connection) {
        conn.call(|conn| {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS memory_embeddings (
                    doc_id TEXT NOT NULL PRIMARY KEY,
                    embedding BLOB NOT NULL,
                    dims INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE INDEX IF NOT EXISTS idx_memory_embeddings_doc_id
                ON memory_embeddings(doc_id);
                "#,
            )
            .unwrap();
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_put_and_get_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = Connection::open(&db_path).await.unwrap();

        create_test_table(&conn).await;

        let vector_store = VectorStore::new(Arc::new(conn));

        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4];
        vector_store.put_embedding("doc-1", &embedding).await.unwrap();

        let retrieved = vector_store.get_embedding("doc-1").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved_embedding = retrieved.unwrap();
        assert_eq!(retrieved_embedding.len(), 4);

        for (i, &val) in retrieved_embedding.iter().enumerate() {
            assert!((val - embedding[i]).abs() < f32::EPSILON);
        }
    }

    #[tokio::test]
    async fn test_get_embedding_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = Connection::open(&db_path).await.unwrap();

        create_test_table(&conn).await;

        let vector_store = VectorStore::new(Arc::new(conn));
        let result = vector_store.get_embedding("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_embedding() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = Connection::open(&db_path).await.unwrap();

        create_test_table(&conn).await;

        let vector_store = VectorStore::new(Arc::new(conn));

        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4];
        vector_store.put_embedding("doc-1", &embedding).await.unwrap();

        let deleted = vector_store.delete_embedding("doc-1").await.unwrap();
        assert!(deleted);

        let retrieved = vector_store.get_embedding("doc-1").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_find_similar() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = Connection::open(&db_path).await.unwrap();

        create_test_table(&conn).await;

        let vector_store = VectorStore::new(Arc::new(conn));
        vector_store.put_embedding("doc-1", &[1.0, 0.0, 0.0]).await.unwrap();
        vector_store.put_embedding("doc-2", &[0.9, 0.1, 0.0]).await.unwrap();
        vector_store.put_embedding("doc-3", &[0.0, 1.0, 0.0]).await.unwrap();

        let query_embedding = vec![1.0_f32, 0.0, 0.0];
        let results = vector_store.find_similar(&query_embedding, 10, 0.5).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].doc_id, "doc-1");
        assert!(results[0].similarity > 0.99);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![1.0_f32, 0.0, 0.0];
        assert!((VectorStore::cosine_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);

        let c = vec![0.0_f32, 1.0, 0.0];
        assert!((VectorStore::cosine_similarity(&a, &c) - 0.0).abs() < f64::EPSILON);

        let d = vec![1.0_f32, 1.0, 0.0];
        let similarity = VectorStore::cosine_similarity(&a, &d);
        assert!(similarity > 0.0 && similarity < 1.0);
    }

    #[test]
    fn test_serialize_deserialize_embedding() {
        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4];
        let serialized = VectorStore::serialize_embedding(&embedding);
        let deserialized = VectorStore::deserialize_embedding(&serialized);

        assert_eq!(deserialized.len(), embedding.len());

        for (i, &val) in deserialized.iter().enumerate() {
            assert!((val - embedding[i]).abs() < f32::EPSILON);
        }
    }
}
