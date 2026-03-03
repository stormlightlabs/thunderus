//! Session (conversation) persistence

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use thndrs_core::{Message, Role};
use uuid::Uuid;

use crate::{MemoryDatabase, MemoryError, Result};

/// A conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
    pub metadata: Option<serde_json::Value>,
}

impl Session {
    /// Create a new session with a generated ID
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title: None,
            created_at: now,
            updated_at: now,
            message_count: 0,
            metadata: None,
        }
    }

    /// Create a new session with a title
    pub fn with_title(title: impl Into<String>) -> Self {
        let mut session = Self::new();
        session.title = Some(title.into());
        session
    }

    /// Get a shortened ID for display
    pub fn short_id(&self) -> String {
        self.id.split('-').next().unwrap_or(&self.id).to_string()
    }

    /// Get a display title
    pub fn display_title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| format!("Session {}", self.short_id()))
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// A stored message with its session context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: i64,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<SessionMessage> for Message {
    fn from(msg: SessionMessage) -> Self {
        Message { role: msg.role, content: msg.content, tool_call_id: msg.tool_call_id }
    }
}

/// A stored tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolCall {
    pub id: i64,
    pub session_id: String,
    pub tool_name: String,
    pub arguments: Option<serde_json::Value>,
    pub result: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Manages session persistence
pub struct SessionManager {
    db: MemoryDatabase,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(db: MemoryDatabase) -> Self {
        Self { db }
    }

    /// Create a new session
    pub fn create_session(&mut self, title: Option<impl Into<String>>) -> Result<Session> {
        let mut session = Session::new();
        if let Some(t) = title {
            session.title = Some(t.into());
        }

        self.db.conn_mut().execute(
            "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                &session.id,
                session.title.as_ref(),
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339()
            ],
        )?;

        Ok(session)
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, title, created_at, updated_at, message_count, metadata
            FROM sessions
            WHERE id = ?1
            "#,
        )?;

        let result = stmt.query_row(params![id], |row| {
            let metadata_str: Option<String> = row.get(5)?;
            let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                message_count: row.get(4)?,
                metadata,
            })
        });

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all sessions
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, title, created_at, updated_at, message_count, metadata
            FROM sessions
            ORDER BY updated_at DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            let metadata_str: Option<String> = row.get(5)?;
            let metadata = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                message_count: row.get(4)?,
                metadata,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }

        Ok(sessions)
    }

    /// Update session title
    pub fn update_title(&mut self, id: &str, title: impl Into<String>) -> Result<bool> {
        let rows = self.db.conn_mut().execute(
            "UPDATE sessions SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![title.into(), id],
        )?;
        Ok(rows > 0)
    }

    /// Delete a session
    pub fn delete_session(&mut self, id: &str) -> Result<bool> {
        let rows = self
            .db
            .conn_mut()
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    /// Add a message to a session
    pub fn add_message(&mut self, session_id: &str, message: &Message) -> Result<i64> {
        if self.get_session(session_id)?.is_none() {
            return Err(MemoryError::SessionNotFound(session_id.to_string()));
        }

        let role_str = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let tx = self.db.conn_mut().transaction()?;

        tx.execute(
            "INSERT INTO messages (session_id, role, content, tool_call_id) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role_str, &message.content, message.tool_call_id.as_ref()],
        )?;

        let message_id = tx.last_insert_rowid();

        tx.execute(
            "UPDATE sessions SET message_count = message_count + 1, updated_at = datetime('now') WHERE id = ?1",
            params![session_id],
        )?;

        tx.commit()?;

        Ok(message_id)
    }

    /// Get all messages for a session
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<SessionMessage>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, session_id, role, content, tool_call_id, created_at
            FROM messages
            WHERE session_id = ?1
            ORDER BY created_at ASC
            "#,
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            let role_str: String = row.get(2)?;
            let role = match role_str.as_str() {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                _ => Role::User,
            };

            Ok(SessionMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                tool_call_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }

        Ok(messages)
    }

    /// Get messages as thndrs_core::Message for conversation restoration
    pub fn get_conversation_messages(&self, session_id: &str) -> Result<Vec<Message>> {
        let session_messages = self.get_messages(session_id)?;
        Ok(session_messages.into_iter().map(Message::from).collect())
    }

    /// Add a tool call to a session
    pub fn add_tool_call(
        &mut self, session_id: &str, tool_name: &str, arguments: &serde_json::Value, result: Option<&str>, status: &str,
    ) -> Result<i64> {
        if self.get_session(session_id)?.is_none() {
            return Err(MemoryError::SessionNotFound(session_id.to_string()));
        }

        self.db.conn_mut().execute(
            "INSERT INTO tool_calls (session_id, tool_name, arguments, result, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![session_id, tool_name, arguments.to_string(), result, status],
        )?;

        Ok(self.db.conn_mut().last_insert_rowid())
    }

    /// Get tool calls for a session
    pub fn get_tool_calls(&self, session_id: &str) -> Result<Vec<SessionToolCall>> {
        let mut stmt = self.db.conn().prepare(
            r#"
            SELECT id, session_id, tool_name, arguments, result, status, created_at
            FROM tool_calls
            WHERE session_id = ?1
            ORDER BY created_at ASC
            "#,
        )?;

        let rows = stmt.query_map(params![session_id], |row| {
            let args_str: Option<String> = row.get(3)?;
            let args = args_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(SessionToolCall {
                id: row.get(0)?,
                session_id: row.get(1)?,
                tool_name: row.get(2)?,
                arguments: args,
                result: row.get(4)?,
                status: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut calls = Vec::new();
        for row in rows {
            calls.push(row?);
        }

        Ok(calls)
    }

    /// Clear all messages for a session (but keep the session)
    pub fn clear_messages(&mut self, session_id: &str) -> Result<usize> {
        let tx = self.db.conn_mut().transaction()?;

        let deleted = tx.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;

        tx.execute("DELETE FROM tool_calls WHERE session_id = ?1", params![session_id])?;

        tx.execute(
            "UPDATE sessions SET message_count = 0, updated_at = datetime('now') WHERE id = ?1",
            params![session_id],
        )?;

        tx.commit()?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (TempDir, SessionManager) {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = crate::MemoryDatabase::open(&db_path, false).unwrap();
        let manager = SessionManager::new(db);
        (temp, manager)
    }

    #[test]
    fn test_create_session() {
        let (_temp, mut manager) = create_test_manager();

        let session = manager.create_session(Some("Test Session")).unwrap();
        assert!(!session.id.is_empty());
        assert_eq!(session.title, Some("Test Session".to_string()));
        assert_eq!(session.message_count, 0);
    }

    #[test]
    fn test_get_session() {
        let (_temp, mut manager) = create_test_manager();

        let session = manager.create_session(Option::<&str>::None).unwrap();
        let retrieved = manager.get_session(&session.id).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, session.id);
    }

    #[test]
    fn test_list_sessions() {
        let (_temp, mut manager) = create_test_manager();

        manager.create_session(Some("Session 1")).unwrap();
        manager.create_session(Some("Session 2")).unwrap();

        let sessions = manager.list_sessions(10).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_add_and_get_messages() {
        let (_temp, mut manager) = create_test_manager();

        let session = manager.create_session(Option::<&str>::None).unwrap();

        let msg1 = Message::user("Hello");
        let msg2 = Message::assistant("Hi there!");

        manager.add_message(&session.id, &msg1).unwrap();
        manager.add_message(&session.id, &msg2).unwrap();

        let messages = manager.get_messages(&session.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn test_clear_messages() {
        let (_temp, mut manager) = create_test_manager();

        let session = manager.create_session(Option::<&str>::None).unwrap();

        let msg = Message::user("Test");
        manager.add_message(&session.id, &msg).unwrap();

        let deleted = manager.clear_messages(&session.id).unwrap();
        assert_eq!(deleted, 1);

        let messages = manager.get_messages(&session.id).unwrap();
        assert!(messages.is_empty());
    }
}
