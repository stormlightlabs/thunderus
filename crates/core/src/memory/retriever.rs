//! Memory retriever for agent integration
//!
//! The MemoryRetriever queries the memory store before agent actions
//! and retrieves relevant chunks with full citation information.

use crate::memory::MemoryKind;
use std::pin::Pin;
use std::time::Instant;

/// Policy for when and how to retrieve memory
#[derive(Debug, Clone)]
pub struct RetrievalPolicy {
    /// Maximum chunks to inject into context
    pub max_chunks: usize,
    /// Maximum tokens from memory retrieval
    pub max_tokens: usize,
    /// Minimum BM25 score to include (lower = better match)
    pub score_threshold: f64,
    /// Always include these memory kinds regardless of query
    pub always_include: Vec<MemoryKind>,
    /// Enable semantic/vector retrieval fallback
    pub enable_vector_fallback: bool,
}

impl Default for RetrievalPolicy {
    fn default() -> Self {
        Self {
            max_chunks: 5,
            max_tokens: 2000,
            score_threshold: -5.0,
            always_include: vec![MemoryKind::Core],
            enable_vector_fallback: false,
        }
    }
}

/// Result of a memory retrieval operation
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    /// Retrieved chunks with citations
    pub chunks: Vec<RetrievedChunk>,
    /// Total tokens used
    pub total_tokens: usize,
    /// Query that was executed
    pub query: String,
    /// Search execution time (ms)
    pub search_time_ms: u64,
}

/// A chunk of memory with full citation
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    /// The memory content (may be truncated)
    pub content: String,
    /// Citation: file path
    pub path: String,
    /// Citation: heading anchor (e.g., "commands")
    pub anchor: Option<String>,
    /// Citation: event IDs for provenance
    pub event_ids: Vec<String>,
    /// Memory kind for display
    pub kind: MemoryKind,
    /// Relevance score
    pub score: f64,
}

/// Stop words for query term extraction
pub const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as", "is", "was",
    "are", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "must", "can", "this", "that", "these", "those", "it", "its", "i", "you", "he", "she",
    "we", "they", "what", "which", "who", "when", "where", "why", "how", "all", "each", "every", "both", "few", "more",
    "most", "other", "some", "such", "no", "not", "only", "own", "same", "so", "than", "too", "very", "just", "into",
    "over", "after", "before", "between", "under", "again", "there", "here", "up", "down", "off", "out",
];

impl RetrievedChunk {
    /// Format the chunk as a markdown citation
    pub fn format_citation(&self) -> String {
        let anchor_suffix = self.anchor.as_ref().map(|a| format!("#{a}")).unwrap_or_default();
        format!("[{}]({}{})", self.kind, self.path, anchor_suffix)
    }
}

/// Memory retriever for agent integration
///
/// This trait defines the interface for memory retrieval without
/// depending on the actual store implementation. This allows the
/// agent to use the retriever without a direct dependency on the store crate.
pub trait MemoryRetriever: Send + Sync {
    /// Query memory store using task intent
    ///
    /// Called automatically before edits/tool execution in the agent loop
    fn query<'a>(
        &'a self, task_intent: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<RetrievalResult, RetrievalError>> + Send + 'a>>;

    /// Get the current retrieval policy
    fn policy(&self) -> &RetrievalPolicy;
}

/// Errors that can occur during memory retrieval
#[derive(Debug, thiserror::Error)]
pub enum RetrievalError {
    /// Error from the underlying store
    #[error("Store error: {0}")]
    Store(String),

    /// Query processing error
    #[error("Query error: {0}")]
    Query(String),

    /// Budget/filter error
    #[error("Budget error: {0}")]
    Budget(String),
}

/// In-memory memory retriever for testing
///
/// This implementation allows the agent to be tested without
/// requiring a full SQLite store setup.
#[derive(Debug, Clone)]
pub struct InMemoryRetriever {
    policy: RetrievalPolicy,
}

impl InMemoryRetriever {
    /// Create a new in-memory retriever with the given policy
    pub fn new(policy: RetrievalPolicy) -> Self {
        Self { policy }
    }

    /// Create a new in-memory retriever with default policy
    pub fn with_defaults() -> Self {
        Self::default()
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
}

impl Default for InMemoryRetriever {
    fn default() -> Self {
        Self::new(RetrievalPolicy::default())
    }
}

impl MemoryRetriever for InMemoryRetriever {
    fn query<'a>(
        &'a self, task_intent: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<RetrievalResult, RetrievalError>> + Send + 'a>> {
        Box::pin(async move {
            let start = Instant::now();
            let query = self.extract_query_terms(task_intent);

            Ok(RetrievalResult {
                chunks: Vec::new(),
                total_tokens: 0,
                query,
                search_time_ms: start.elapsed().as_millis() as u64,
            })
        })
    }

    fn policy(&self) -> &RetrievalPolicy {
        &self.policy
    }
}

/// Format retrieval results as a context string for injection into agent prompts
pub fn format_memory_context(result: &RetrievalResult) -> String {
    if result.chunks.is_empty() {
        "No relevant memory found.".to_string()
    } else {
        result
            .chunks
            .iter()
            .map(|chunk| {
                let citation = chunk.format_citation();
                format!("**{}**\n{}\n", citation, chunk.content)
            })
            .collect::<Vec<_>>()
            .join("\n---\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retrieval_policy_default() {
        let policy = RetrievalPolicy::default();
        assert_eq!(policy.max_chunks, 5);
        assert_eq!(policy.max_tokens, 2000);
        assert_eq!(policy.score_threshold, -5.0);
        assert_eq!(policy.always_include, vec![MemoryKind::Core]);
        assert!(!policy.enable_vector_fallback);
    }

    #[test]
    fn test_retrieved_chunk_format_citation() {
        let chunk = RetrievedChunk {
            content: "Test content".to_string(),
            path: "memory/core/CORE.md".to_string(),
            anchor: Some("commands".to_string()),
            event_ids: vec!["evt-1".to_string()],
            kind: MemoryKind::Core,
            score: -3.5,
        };

        let citation = chunk.format_citation();
        assert_eq!(citation, "[Core](memory/core/CORE.md#commands)");
    }

    #[test]
    fn test_retrieved_chunk_format_citation_no_anchor() {
        let chunk = RetrievedChunk {
            content: "Test content".to_string(),
            path: "memory/semantic/FACTS/test.md".to_string(),
            anchor: None,
            event_ids: vec![],
            kind: MemoryKind::Fact,
            score: -2.0,
        };

        let citation = chunk.format_citation();
        assert_eq!(citation, "[Fact](memory/semantic/FACTS/test.md)");
    }

    #[test]
    fn test_extract_query_terms() {
        let retriever = InMemoryRetriever::with_defaults();
        let query = retriever.extract_query_terms("How do I run the tests in this project");
        assert!(query.contains("run"));
        assert!(query.contains("tests"));
        assert!(query.contains("project"));
        assert!(!query.contains("the"));
        assert!(!query.contains("in"));
    }

    #[test]
    fn test_extract_query_terms_short_words() {
        let retriever = InMemoryRetriever::with_defaults();
        let query = retriever.extract_query_terms("do it");
        assert!(query.is_empty(), "Short words should be filtered out");
    }

    #[test]
    fn test_format_memory_context_empty() {
        let result = RetrievalResult { chunks: vec![], total_tokens: 0, query: "test".to_string(), search_time_ms: 10 };

        let context = format_memory_context(&result);
        assert_eq!(context, "No relevant memory found.");
    }

    #[test]
    fn test_format_memory_context_with_chunks() {
        let result = RetrievalResult {
            chunks: vec![
                RetrievedChunk {
                    content: "Test content 1".to_string(),
                    path: "core/CORE.md".to_string(),
                    anchor: None,
                    event_ids: vec![],
                    kind: MemoryKind::Core,
                    score: -1.0,
                },
                RetrievedChunk {
                    content: "Test content 2".to_string(),
                    path: "semantic/FACTS/test.md".to_string(),
                    anchor: Some("section".to_string()),
                    event_ids: vec![],
                    kind: MemoryKind::Fact,
                    score: -2.0,
                },
            ],
            total_tokens: 100,
            query: "test".to_string(),
            search_time_ms: 10,
        };

        let context = format_memory_context(&result);
        assert!(context.contains("[Core](core/CORE.md)"));
        assert!(context.contains("Test content 1"));
        assert!(context.contains("[Fact](semantic/FACTS/test.md#section)"));
        assert!(context.contains("Test content 2"));
        assert!(context.contains("---"));
    }

    #[tokio::test]
    async fn test_in_memory_retriever_query() {
        let retriever = InMemoryRetriever::with_defaults();
        let result = retriever.query("test query").await.unwrap();

        assert_eq!(result.chunks.len(), 0);
        assert_eq!(result.total_tokens, 0);
        assert!(result.query.contains("test"));
        assert!(result.query.contains("query"));
    }

    #[test]
    fn test_in_memory_retriever_policy() {
        let policy = RetrievalPolicy { max_chunks: 10, max_tokens: 5000, ..Default::default() };

        let retriever = InMemoryRetriever::new(policy.clone());
        assert_eq!(retriever.policy().max_chunks, 10);
        assert_eq!(retriever.policy().max_tokens, 5000);
    }
}
