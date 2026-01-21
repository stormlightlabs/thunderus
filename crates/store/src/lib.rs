//! Memory store implementation with SQLite FTS5 backend
//!
//! Provides durable storage and full-text search for memory documents.
//!
//! # Example
//!
//! ```ignore
//! use thunderus_store::{MemoryStore, MemoryMeta, SearchFilters};
//! use thunderus_core::memory::MemoryKind;
//!
//! // Open the store
//! let store = MemoryStore::open(&db_path).await?;
//!
//! // Store a document
//! let meta = MemoryMeta {
//!     id: "doc-1".to_string(),
//!     kind: MemoryKind::Fact,
//!     title: "Testing Coverage".to_string(),
//!     tags: vec!["testing".to_string()],
//!     headings: vec!["coverage".to_string()],
//!     path: "semantic/FACTS/testing.md".to_string(),
//!     updated: chrono::Utc::now(),
//!     event_ids: vec![],
//!     patch_ids: vec![],
//!     token_count: 100,
//! };
//! store.put("semantic/facts", "testing.md", content, meta).await?;
//!
//! // Search for documents
//! let hits = store.search("coverage", SearchFilters::default()).await?;
//! for hit in hits {
//!     println!("{}: {} (score: {:.2})", hit.kind, hit.title, hit.score);
//! }
//! ```

mod error;
mod memory_store;
mod schema;

pub use error::{Error, Result};
pub use memory_store::{MemoryMeta, MemoryStore, SearchFilters, SearchHit, StoreStats};
