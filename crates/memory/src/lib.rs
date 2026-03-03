//! Memory system for Thunderus
//!
//! Provides persistent memory across conversations with:
//! - SQLite-backed storage (per-workspace and global)
//! - Embedding-based similarity search
//! - Session persistence
//! - Log storage

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod database;
pub mod embedding;
pub mod logging;
pub mod memory;
pub mod session;

pub use database::MemoryDatabase;
pub use embedding::{Embedding, EmbeddingConfig};
pub use logging::{LogEntry, LogLevel, LogStore};
pub use memory::{Memory, MemoryKind, MemoryStore};
pub use session::{Session, SessionManager};

/// Errors that can occur in memory operations
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Memory not found: {0}")]
    MemoryNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Result type for memory operations
pub type Result<T> = std::result::Result<T, MemoryError>;

/// Statistics about memory storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub archived_memories: usize,
    pub total_sessions: usize,
    pub database_size_bytes: u64,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: usize,
}

/// Initialize the memory system for a workspace
pub fn init(workspace_path: impl AsRef<Path>) -> Result<(MemoryStore, SessionManager, LogStore)> {
    let db = MemoryDatabase::for_workspace(workspace_path)?;
    let store_db = db.try_clone()?;
    let session_db = db.try_clone()?;
    let log_db = db;

    let store = MemoryStore::new(store_db);
    let sessions = SessionManager::new(session_db);
    let logs = LogStore::new(log_db);
    Ok((store, sessions, logs))
}

/// Get the memory directory path (~/.thunderus/memory)
pub fn memory_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| MemoryError::InvalidPath("Home directory not found".to_string()))?;
    Ok(home.join(".thunderus").join("memory"))
}

/// Get the global memory database path
pub fn global_db_path() -> Result<PathBuf> {
    Ok(memory_dir()?.join("global.db"))
}

/// Get the workspace databases directory
pub fn workspaces_dir() -> Result<PathBuf> {
    Ok(memory_dir()?.join("workspaces"))
}

/// Hash a workspace path to get a database filename
pub fn hash_workspace(path: impl AsRef<Path>) -> String {
    use sha2::{Digest, Sha256};
    let path_str = path.as_ref().to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    hex::encode(hasher.finalize())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_workspace() {
        let hash1 = hash_workspace("/home/user/project");
        let hash2 = hash_workspace("/home/user/project");
        let hash3 = hash_workspace("/home/user/other");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16);
    }
}
