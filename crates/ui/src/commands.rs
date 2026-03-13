use crate::chat::ChatMessage;
use thndrs_core::Role;
use thndrs_mem::{LogStore, MemoryDatabase, MemoryStore, SessionManager};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Empty,
    DebugChat,
    DebugFiles,
    Files,
    History,
    Resume(String),
    Clear,
    Tokens,
    Model,
    DebugMemoryStats,
    DebugMemoryRecall(String),
    DebugLog(String),
    Settings,
    HelpCmd,
    Unknown(String),
}

pub(crate) fn parse(raw: &str) -> SlashCommand {
    let command = raw.trim();
    if command.is_empty() || command == "/" {
        return SlashCommand::Empty;
    }

    if let Some(session_id) = command.strip_prefix("/resume ") {
        let session_id = session_id.trim();
        if !session_id.is_empty() {
            return SlashCommand::Resume(session_id.to_string());
        }
    }

    if let Some(query) = command.strip_prefix("/debug memory recall ") {
        let query = query.trim();
        if !query.is_empty() {
            return SlashCommand::DebugMemoryRecall(query.to_string());
        }
    }

    if let Some(session_id) = command.strip_prefix("/debug log ") {
        let session_id = session_id.trim();
        if !session_id.is_empty() {
            return SlashCommand::DebugLog(session_id.to_string());
        }
    }

    match command {
        "/debug chat" => SlashCommand::DebugChat,
        "/debug files" => SlashCommand::DebugFiles,
        "/files" => SlashCommand::Files,
        "/history" => SlashCommand::History,
        "/clear" => SlashCommand::Clear,
        "/tokens" => SlashCommand::Tokens,
        "/model" => SlashCommand::Model,
        "/debug memory stats" => SlashCommand::DebugMemoryStats,
        "/settings" => SlashCommand::Settings,
        "/help" => SlashCommand::HelpCmd,
        _ => SlashCommand::Unknown(command.to_string()),
    }
}

fn workspace_memory_database() -> std::result::Result<MemoryDatabase, String> {
    let workspace_path = std::env::current_dir().map_err(|error| error.to_string())?;
    MemoryDatabase::for_workspace(&workspace_path).map_err(|error| error.to_string())
}

pub(crate) fn format_session_history() -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let sessions = SessionManager::new(db)
        .list_sessions(50)
        .map_err(|error| error.to_string())?;

    if sessions.is_empty() {
        return Ok("No saved sessions found.".to_string());
    }

    let mut lines = vec!["Saved sessions:".to_string()];
    for session in sessions {
        lines.push(format!(
            "- {} | {} | {} messages | updated {}",
            session.id,
            session.display_title(),
            session.message_count,
            session.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }

    Ok(lines.join("\n"))
}

pub(crate) fn load_session_chat_messages(session_id: &str) -> std::result::Result<Vec<ChatMessage>, String> {
    let db = workspace_memory_database()?;
    let manager = SessionManager::new(db);
    let session = manager.get_session(session_id).map_err(|error| error.to_string())?;

    if session.is_none() {
        return Err("Session not found".to_string());
    }

    let messages = manager.get_messages(session_id).map_err(|error| error.to_string())?;

    let mut chat_messages = Vec::with_capacity(messages.len());
    for message in messages {
        match message.role {
            Role::User => chat_messages.push(ChatMessage::user_at(message.content, message.created_at)),
            Role::Assistant => chat_messages.push(ChatMessage::assistant_at(message.content, message.created_at)),
            Role::Tool => chat_messages.push(ChatMessage::tool_at(
                "tool".to_string(),
                message.content,
                message.created_at,
            )),
            Role::System => {}
        }
    }

    Ok(chat_messages)
}

pub(crate) fn format_memory_stats() -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let store = MemoryStore::new(db);
    let stats = store.stats().map_err(|error| error.to_string())?;

    Ok(format!(
        "Memory stats:\n- total: {}\n- archived: {}\n- sessions: {}\n- size: {} bytes ({:.2} MB)\n- embedding model: {}\n- dimensions: {}",
        stats.total_memories,
        stats.archived_memories,
        stats.total_sessions,
        stats.database_size_bytes,
        stats.database_size_bytes as f64 / 1_048_576.0,
        stats.embedding_model.as_deref().unwrap_or("unknown"),
        stats.embedding_dimensions
    ))
}

pub(crate) fn format_memory_recall(query: &str) -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let mut store = MemoryStore::new(db);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;

    let memories = runtime
        .block_on(async { store.recall(query, 5, None, None, 0.3).await })
        .map_err(|error| error.to_string())?;

    if memories.is_empty() {
        return Ok(format!("No memories found for query: {}", query));
    }

    let mut lines = vec![format!("Memory recall for `{}`:", query)];
    for memory in memories {
        let similarity = memory
            .similarity
            .map(|value| format!(" (similarity: {:.2})", value))
            .unwrap_or_default();
        lines.push(format!("- [{}] {}{}", memory.kind.as_str(), memory.content, similarity));
    }

    Ok(lines.join("\n"))
}

pub(crate) fn format_session_logs(session_id: &str) -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let logs = LogStore::new(db)
        .get_session_logs(session_id, 200)
        .map_err(|error| error.to_string())?;

    if logs.is_empty() {
        return Ok(format!("No logs found for session `{}`.", session_id));
    }

    let mut lines = vec![format!("Logs for session `{}`:", session_id)];
    for entry in logs {
        lines.push(format!(
            "[{}] [{}] {} | {}",
            entry.level.as_str(),
            entry.component.as_deref().unwrap_or("runtime"),
            entry.created_at.format("%Y-%m-%dT%H:%M:%S%.6f"),
            entry.message
        ));
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_command() {
        assert_eq!(parse("/debug chat"), SlashCommand::DebugChat);
        assert_eq!(parse("/debug files"), SlashCommand::DebugFiles);
        assert_eq!(parse("/files"), SlashCommand::Files);
        assert_eq!(parse("/history"), SlashCommand::History);
        assert_eq!(parse("/resume abc123"), SlashCommand::Resume("abc123".to_string()));
        assert_eq!(parse("/clear"), SlashCommand::Clear);
        assert_eq!(parse("/tokens"), SlashCommand::Tokens);
        assert_eq!(parse("/model"), SlashCommand::Model);
        assert_eq!(parse("/debug memory stats"), SlashCommand::DebugMemoryStats);
        assert_eq!(
            parse("/debug memory recall rust sqlite"),
            SlashCommand::DebugMemoryRecall("rust sqlite".to_string())
        );
        assert_eq!(parse("/debug log sess-1"), SlashCommand::DebugLog("sess-1".to_string()));
        assert_eq!(parse("/settings"), SlashCommand::Settings);
        assert_eq!(parse("/help"), SlashCommand::HelpCmd);
        assert_eq!(parse("/unknown"), SlashCommand::Unknown("/unknown".to_string()));
        assert_eq!(parse("/"), SlashCommand::Empty);
    }
}
