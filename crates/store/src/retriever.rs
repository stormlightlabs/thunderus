//! Memory retriever implementation using SQLite store
//!
//! This module provides the concrete implementation of memory retrieval
//! that queries the SQLite FTS5 full-text search index.

use crate::memory_store::{self, SearchFilters, SearchHit};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use thunderus_core::memory::{MemoryKind, RetrievalPolicy, RetrievalResult, RetrievedChunk, STOP_WORDS};
use thunderus_core::memory::{MemoryRetriever, RetrievalError};

/// This struct bridges the `MemoryStore` with the `MemoryRetriever` trait,
/// enabling the agent to query the memory store through the trait interface.
pub struct StoreRetriever {
    store: Arc<memory_store::MemoryStore>,
    policy: RetrievalPolicy,
}

impl StoreRetriever {
    /// Create a new store retriever with the given store and policy
    pub fn new(store: Arc<memory_store::MemoryStore>, policy: RetrievalPolicy) -> Self {
        Self { store, policy }
    }

    /// Create a new store retriever with default policy
    pub fn with_defaults(store: Arc<memory_store::MemoryStore>) -> Self {
        Self::new(store, RetrievalPolicy::default())
    }

    /// Get the retrieval policy
    pub fn policy(&self) -> &RetrievalPolicy {
        &self.policy
    }

    /// Extract searchable query terms from natural language task intent
    fn extract_query_terms(&self, intent: &str) -> String {
        intent
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .filter(|w| !STOP_WORDS.contains(&w.to_lowercase().as_str()))
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    /// Convert search hits to retrieved chunks with budget filtering
    fn filter_and_budget(&self, hits: Vec<SearchHit>) -> Vec<RetrievedChunk> {
        let mut chunks = Vec::new();
        let mut token_count = 0;

        for hit in hits {
            if hit.score > self.policy.score_threshold {
                continue;
            }

            let chunk_tokens = hit.snippet.len() / 4;
            if token_count + chunk_tokens > self.policy.max_tokens {
                break;
            }

            chunks.push(RetrievedChunk {
                content: hit.snippet,
                path: hit.path,
                anchor: hit.anchor,
                event_ids: hit.event_ids,
                kind: hit.kind,
                score: hit.score,
            });

            token_count += chunk_tokens;

            if chunks.len() >= self.policy.max_chunks {
                break;
            }
        }

        chunks
    }

    /// Fetch all documents of a specific memory kind
    ///
    /// This is used for the `always_include` policy to ensure certain
    /// memory kinds (like Core) are always loaded.
    async fn fetch_by_kind(&self, kind: MemoryKind) -> Vec<RetrievedChunk> {
        let filters = SearchFilters { kinds: Some(vec![kind]), limit: Some(100), ..Default::default() };

        let query = match kind {
            MemoryKind::Core => "project OR commands OR setup OR configuration",
            _ => "memory OR document",
        };

        match self.store.search(query, filters).await {
            Ok(hits) => hits
                .into_iter()
                .map(|hit| RetrievedChunk {
                    content: hit.snippet,
                    path: hit.path,
                    anchor: hit.anchor,
                    event_ids: hit.event_ids,
                    kind: hit.kind,
                    score: hit.score,
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// Implement MemoryRetriever trait for StoreRetrieverImpl
///
/// This enables the agent to use the store retriever through the trait interface.
impl MemoryRetriever for StoreRetriever {
    fn query<'a>(
        &'a self, task_intent: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = std::result::Result<RetrievalResult, RetrievalError>> + Send + 'a>>
    {
        Box::pin(async move {
            let start = Instant::now();
            let query = self.extract_query_terms(task_intent);

            let mut chunks = Vec::new();
            let mut token_count = 0;

            for kind in &self.policy.always_include {
                let always_chunks = self.fetch_by_kind(*kind).await;

                for chunk in always_chunks {
                    let chunk_tokens = chunk.content.len() / 4;
                    if token_count + chunk_tokens > self.policy.max_tokens {
                        break;
                    }
                    token_count += chunk_tokens;
                    chunks.push(chunk);
                }
            }

            let filters = SearchFilters { limit: Some(self.policy.max_chunks * 2), ..Default::default() };

            let hits = self
                .store
                .search(&query, filters)
                .await
                .map_err(|e| RetrievalError::Store(e.to_string()))?;

            let search_chunks = self.filter_and_budget(hits);

            for chunk in search_chunks {
                let chunk_tokens = chunk.content.len() / 4;
                if token_count + chunk_tokens > self.policy.max_tokens {
                    break;
                }
                token_count += chunk_tokens;
                chunks.push(chunk);
            }

            Ok(RetrievalResult {
                chunks,
                total_tokens: token_count,
                query,
                search_time_ms: start.elapsed().as_millis() as u64,
            })
        })
    }

    fn policy(&self) -> &RetrievalPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_store::{MemoryMeta, MemoryStore};
    use chrono::Utc;
    use tempfile::TempDir;
    use thunderus_core::memory::MemoryKind;

    #[tokio::test]
    async fn test_store_retriever_impl_with_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "test-1".to_string(),
            kind: MemoryKind::Fact,
            title: "Test Fact About Coverage".to_string(),
            tags: vec!["test".to_string()],
            headings: vec![],
            path: "semantic/FACTS/test.md".to_string(),
            updated: Utc::now(),
            event_ids: vec!["evt-1".to_string(), "evt-2".to_string()],
            patch_ids: vec![],
            token_count: 100,
        };

        store
            .put(
                "semantic/facts",
                "test.md",
                "# Testing Coverage\n\nThis document describes minimum line coverage requirements for the project.",
                meta,
            )
            .await
            .unwrap();

        let policy = RetrievalPolicy { score_threshold: 100.0, ..Default::default() };
        let retriever = StoreRetriever::new(std::sync::Arc::new(store), policy);
        let result = retriever.query("coverage requirements").await.unwrap();

        assert!(
            !result.chunks.is_empty(),
            "Should find at least one chunk for 'coverage requirements', got query: '{}', chunks: {:?}",
            result.query,
            result.chunks
        );
        assert_eq!(
            result.total_tokens,
            result.chunks.iter().map(|c| c.content.len() / 4).sum::<usize>()
        );
        assert!(result.query.contains("coverage"));
        assert!(result.query.contains("requirements"));
    }

    #[tokio::test]
    async fn test_store_retriever_impl_event_ids() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        let meta = MemoryMeta {
            id: "test-events".to_string(),
            kind: MemoryKind::Core,
            title: "Core with Events".to_string(),
            tags: vec![],
            headings: vec!["commands".to_string()],
            path: "core/CORE.md".to_string(),
            updated: Utc::now(),
            event_ids: vec!["evt-abc".to_string(), "evt-def".to_string()],
            patch_ids: vec!["patch-123".to_string()],
            token_count: 50,
        };

        store
            .put("core", "CORE.md", "# Commands\n\nTest commands", meta)
            .await
            .unwrap();

        let policy = RetrievalPolicy { always_include: vec![MemoryKind::Core], ..Default::default() };
        let retriever = StoreRetriever::new(std::sync::Arc::new(store), policy);
        let result = retriever.query("any query").await.unwrap();
        assert!(!result.chunks.is_empty());

        let core_chunk = result.chunks.iter().find(|c| c.kind == MemoryKind::Core);
        assert!(core_chunk.is_some(), "Should have a Core chunk from always_include");

        let chunk = core_chunk.unwrap();
        assert_eq!(chunk.event_ids, vec!["evt-abc".to_string(), "evt-def".to_string()]);
    }

    #[tokio::test]
    async fn test_store_retriever_impl_token_budget() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();

        for i in 0..5 {
            let meta = MemoryMeta {
                id: format!("test-{}", i),
                kind: MemoryKind::Fact,
                title: format!("Test Fact {}", i),
                tags: vec![],
                headings: vec![],
                path: format!("semantic/FACTS/test{}.md", i),
                updated: Utc::now(),
                event_ids: vec![],
                patch_ids: vec![],
                token_count: 100,
            };

            store
                .put(
                    &format!("semantic/facts{}", i),
                    &format!("test{}.md", i),
                    &format!("Content {}", i),
                    meta,
                )
                .await
                .unwrap();
        }

        let policy = RetrievalPolicy { max_tokens: 50, ..Default::default() };
        let retriever = StoreRetriever::new(std::sync::Arc::new(store), policy);
        let result = retriever.query("content").await.unwrap();

        assert!(
            result.total_tokens <= 50 + 20,
            "Token budget should be respected (with small tolerance)"
        );
    }

    #[tokio::test]
    async fn test_store_retriever_impl_policy() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let store = MemoryStore::open(&db_path).await.unwrap();
        let policy = RetrievalPolicy {
            max_chunks: 3,
            max_tokens: 1000,
            score_threshold: -2.0,
            always_include: vec![],
            enable_vector_fallback: true,
        };

        let retriever = StoreRetriever::new(std::sync::Arc::new(store), policy.clone());
        assert_eq!(retriever.policy().max_chunks, 3);
        assert_eq!(retriever.policy().max_tokens, 1000);
        assert_eq!(retriever.policy().score_threshold, -2.0);
        assert!(retriever.policy().always_include.is_empty());
        assert!(retriever.policy().enable_vector_fallback);
    }
}
