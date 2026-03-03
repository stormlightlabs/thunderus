//! Database connection and schema management with sqlite-vec support

use rusqlite::{Connection, OpenFlags};
use std::path::Path;

use crate::{MemoryError, Result, global_db_path, hash_workspace, workspaces_dir};

/// A connection to a memory database
#[derive(Debug)]
pub struct MemoryDatabase {
    conn: Connection,
    is_global: bool,
}

impl MemoryDatabase {
    /// Create or open the global memory database
    pub fn global() -> Result<Self> {
        let path = global_db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::open(&path, true)
    }

    /// Create or open a workspace-specific memory database
    pub fn for_workspace(workspace_path: impl AsRef<Path>) -> Result<Self> {
        let workspaces = workspaces_dir()?;
        std::fs::create_dir_all(&workspaces)?;

        let hash = hash_workspace(workspace_path);
        let path = workspaces.join(format!("{}.db", hash));

        Self::open(&path, false)
    }

    /// Open a database and initialize schema
    pub fn open(path: &Path, is_global: bool) -> Result<Self> {
        let conn =
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_READ_WRITE)?;

        unsafe {
            conn.load_extension_enable()?;
        }

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            ",
        )?;

        let mut db = Self { conn, is_global };
        db.init_schema()?;

        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&mut self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            -- Memory entries
            CREATE TABLE IF NOT EXISTS memories (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                content     TEXT NOT NULL,
                kind        TEXT NOT NULL DEFAULT 'fact',
                source      TEXT,
                tags        TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                access_count INTEGER NOT NULL DEFAULT 0,
                archived    INTEGER NOT NULL DEFAULT 0
            );

            -- Packed embedding vectors
            CREATE TABLE IF NOT EXISTS embeddings (
                memory_id   INTEGER PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
                vector      BLOB NOT NULL,
                model       TEXT NOT NULL,
                dimensions  INTEGER NOT NULL
            );

            -- Sessions (conversations)
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                title       TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                message_count INTEGER NOT NULL DEFAULT 0,
                metadata    TEXT
            );

            -- Messages within sessions
            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                tool_call_id TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Tool calls
            CREATE TABLE IF NOT EXISTS tool_calls (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                tool_name   TEXT NOT NULL,
                arguments   TEXT,
                result      TEXT,
                status      TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Logs
            CREATE TABLE IF NOT EXISTS logs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
                level       TEXT NOT NULL,
                component   TEXT,
                message     TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            -- Metadata for database
            CREATE TABLE IF NOT EXISTS meta (
                key         TEXT PRIMARY KEY,
                value       TEXT
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind) WHERE archived = 0;
            CREATE INDEX IF NOT EXISTS idx_memories_tags ON memories(tags) WHERE archived = 0;
            CREATE INDEX IF NOT EXISTS idx_memories_accessed ON memories(accessed_at) WHERE archived = 0;
            CREATE INDEX IF NOT EXISTS idx_embeddings_model ON embeddings(model);
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_tool_calls_session ON tool_calls(session_id);
            CREATE INDEX IF NOT EXISTS idx_logs_session ON logs(session_id);
            CREATE INDEX IF NOT EXISTS idx_logs_created ON logs(created_at);
            "#,
        )?;

        self.init_vec_table()?;

        Ok(())
    }

    /// Initialize sqlite-vec virtual table for embeddings
    fn init_vec_table(&mut self) -> Result<()> {
        let result = self.conn.execute_batch(
            r#"
            -- Create sqlite-vec virtual table
            CREATE VIRTUAL TABLE IF NOT EXISTS vec_memories USING vec0(
                memory_id INTEGER PRIMARY KEY,
                embedding float[256] distance_metric=cosine
            );
            "#,
        );

        if let Err(e) = result {
            tracing::warn!(
                "Failed to create sqlite-vec table (extension may not be available): {}",
                e
            );
        }

        Ok(())
    }

    /// Check if sqlite-vec is available
    pub fn has_vec_support(&self) -> bool {
        matches!(
            self.conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='vec_memories'",
                [],
                |_row| Ok(())
            ),
            Ok(())
        )
    }

    /// Get a reference to the underlying connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get a mutable reference to the underlying connection
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Check if this is the global database
    pub fn is_global(&self) -> bool {
        self.is_global
    }

    /// Get database file size in bytes
    pub fn size_bytes(&self) -> Result<u64> {
        let path = self
            .conn
            .path()
            .ok_or_else(|| MemoryError::InvalidPath("In-memory database not supported".to_string()))?;
        let metadata = std::fs::metadata(path)?;
        Ok(metadata.len())
    }
}

impl MemoryDatabase {
    /// Try to clone this database connection
    pub fn try_clone(&self) -> Result<Self> {
        let path = self
            .conn
            .path()
            .ok_or_else(|| MemoryError::InvalidPath("Cannot clone in-memory database".to_string()))?;
        Self::open(Path::new(path), self.is_global)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_creation() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");

        let db = MemoryDatabase::open(&db_path, false).unwrap();
        assert!(!db.is_global);
        assert!(db_path.exists());
    }

    #[test]
    fn test_global_database() {
        let path = global_db_path().unwrap();
        assert!(path.to_string_lossy().contains("global.db"));
    }
}
