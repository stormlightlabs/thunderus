//! Log persistence to SQLite

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{MemoryDatabase, Result};

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "DEBUG" => Some(LogLevel::Debug),
            "INFO" => Some(LogLevel::Info),
            "WARN" | "WARNING" => Some(LogLevel::Warn),
            "ERROR" => Some(LogLevel::Error),
            _ => None,
        }
    }
}

/// A log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: i64,
    pub session_id: Option<String>,
    pub level: LogLevel,
    pub component: Option<String>,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

/// Format for display
impl std::fmt::Display for LogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] [{}] {} | {}",
            self.level.as_str(),
            self.component.as_deref().unwrap_or("unknown"),
            self.created_at.format("%Y-%m-%dT%H:%M:%S%.6f"),
            self.message
        )
    }
}

/// Stores logs to SQLite
pub struct LogStore {
    db: MemoryDatabase,
}

impl LogStore {
    /// Create a new log store
    pub fn new(db: MemoryDatabase) -> Self {
        Self { db }
    }

    /// Log a message
    pub fn log(
        &mut self, session_id: Option<&str>, level: LogLevel, component: Option<&str>, message: impl Into<String>,
    ) -> Result<i64> {
        self.db.conn_mut().execute(
            "INSERT INTO logs (session_id, level, component, message) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, level.as_str(), component, message.into()],
        )?;

        Ok(self.db.conn_mut().last_insert_rowid())
    }

    /// Convenience method for info logs
    pub fn info(&mut self, session_id: Option<&str>, component: &str, message: impl Into<String>) -> Result<i64> {
        self.log(session_id, LogLevel::Info, Some(component), message)
    }

    /// Convenience method for debug logs
    pub fn debug(&mut self, session_id: Option<&str>, component: &str, message: impl Into<String>) -> Result<i64> {
        self.log(session_id, LogLevel::Debug, Some(component), message)
    }

    /// Convenience method for warning logs
    pub fn warn(&mut self, session_id: Option<&str>, component: &str, message: impl Into<String>) -> Result<i64> {
        self.log(session_id, LogLevel::Warn, Some(component), message)
    }

    /// Convenience method for error logs
    pub fn error(&mut self, session_id: Option<&str>, component: &str, message: impl Into<String>) -> Result<i64> {
        self.log(session_id, LogLevel::Error, Some(component), message)
    }

    /// Get logs for a session
    pub fn get_session_logs(&self, session_id: &str, limit: usize) -> Result<Vec<LogEntry>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, session_id, level, component, message, created_at
            FROM logs
            WHERE session_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            let level_str: String = row.get(2)?;
            Ok(LogEntry {
                id: row.get(0)?,
                session_id: row.get(1)?,
                level: LogLevel::from_string(&level_str).unwrap_or(LogLevel::Info),
                component: row.get(3)?,
                message: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }

        logs.reverse();
        Ok(logs)
    }

    /// Get all logs with filtering
    pub fn get_logs(&self, level: Option<LogLevel>, component: Option<&str>, limit: usize) -> Result<Vec<LogEntry>> {
        let level_str = level.map(|l| l.as_str().to_string());

        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, session_id, level, component, message, created_at
            FROM logs
            WHERE (?1 IS NULL OR level = ?1)
              AND (?2 IS NULL OR component = ?2)
            ORDER BY created_at DESC
            LIMIT ?3
            "#,
        )?;

        let rows = stmt.query_map(params![level_str, component, limit as i64], |row| {
            let lvl_str: String = row.get(2)?;
            Ok(LogEntry {
                id: row.get(0)?,
                session_id: row.get(1)?,
                level: LogLevel::from_string(&lvl_str).unwrap_or(LogLevel::Info),
                component: row.get(3)?,
                message: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }

        logs.reverse();
        Ok(logs)
    }

    /// Clear old logs
    pub fn clear_old_logs(&mut self, older_than_days: i64) -> Result<usize> {
        let rows = self.db.conn_mut().execute(
            "DELETE FROM logs WHERE created_at < datetime('now', '-' || ?1 || ' days')",
            params![older_than_days],
        )?;
        Ok(rows)
    }

    /// Get log count
    pub fn count(&self) -> Result<i64> {
        let count: i64 = self
            .db
            .conn()
            .query_row("SELECT COUNT(*) FROM logs", [], |row| row.get(0))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (TempDir, LogStore) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = crate::MemoryDatabase::open(&db_path, false).unwrap();
        let store = LogStore::new(db);
        (temp, store)
    }

    #[test]
    fn test_log_entry() {
        let (_temp, mut store) = create_test_store();

        let id = store.info(None, "test", "Test message").unwrap();
        assert!(id > 0);

        let count = store.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_get_logs() {
        let (_, mut store) = create_test_store();

        store.info(None, "comp1", "Message 1").unwrap();
        store.warn(None, "comp2", "Message 2").unwrap();
        store.error(None, "comp1", "Message 3").unwrap();

        let logs = store.get_logs(None, None, 10).unwrap();
        assert_eq!(logs.len(), 3);

        let warn_logs = store.get_logs(Some(LogLevel::Warn), None, 10).unwrap();
        assert_eq!(warn_logs.len(), 1);

        let comp1_logs = store.get_logs(None, Some("comp1"), 10).unwrap();
        assert_eq!(comp1_logs.len(), 2);
    }

    #[test]
    fn test_session_logs() {
        use crate::session::SessionManager;

        let (temp, mut store) = create_test_store();

        let db_path = temp.path().join("test.db");
        let db = crate::MemoryDatabase::open(&db_path, false).unwrap();
        let mut sessions = SessionManager::new(db);

        let session1 = sessions.create_session(Some("Test Session 1")).unwrap();
        let session2 = sessions.create_session(Some("Test Session 2")).unwrap();

        store.info(Some(&session1.id), "test", "Message 1").unwrap();
        store.info(Some(&session2.id), "test", "Message 2").unwrap();
        store.info(Some(&session1.id), "test", "Message 3").unwrap();

        let logs = store.get_session_logs(&session1.id, 10).unwrap();
        assert_eq!(logs.len(), 2);
    }
}
