use super::chat::ChatMessage;
use super::{App, ScreenMode};
use thndrs_core::Role;
use thndrs_mem::{LogStore, MemoryDatabase, MemoryStore, SessionManager};

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlashCommand {
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

pub(crate) fn execute_slash_command(app: &mut App, command: &str) {
    match parse_slash_command(command) {
        SlashCommand::DebugChat => {
            app.chat.load_debug_chat();
            app.screen_mode = ScreenMode::Chat;
        }
        SlashCommand::DebugFiles => {
            app.file_browser.load_debug_fixture();
            app.screen_mode = ScreenMode::Files;
        }
        SlashCommand::Files => {
            if let Err(error) = app.file_browser.reload_workspace() {
                app.chat.messages.push(ChatMessage::assistant(format!(
                    "Unable to load workspace files: {error}"
                )));
                app.screen_mode = ScreenMode::Chat;
            } else {
                app.screen_mode = ScreenMode::Files;
            }
        }
        SlashCommand::History => {
            let content = format_session_history().unwrap_or_else(|error| format!("Failed to load history: {error}"));
            app.push_assistant_message(content);
        }
        SlashCommand::Resume(session_id) => match load_session_chat_messages(&session_id) {
            Ok(messages) => {
                app.chat.set_messages(messages);
                app.chat.queue_backend_command(format!("/resume {}", session_id));
                app.screen_mode = ScreenMode::Chat;
            }
            Err(error) => {
                app.push_assistant_message(format!("Failed to resume session `{session_id}`: {error}"));
            }
        },
        SlashCommand::Clear => {
            app.chat.clear_chat();
            app.chat.queue_backend_command("/clear".to_string());
            app.screen_mode = ScreenMode::Chat;
        }
        SlashCommand::Tokens => {
            let content = match app.chat.last_usage {
                Some(usage) => format!(
                    "Token usage:\n- prompt: {}\n- completion: {}\n- total: {}",
                    usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
                ),
                None => "No token usage recorded yet in this chat.".to_string(),
            };
            app.push_assistant_message(content);
        }
        SlashCommand::Model => {
            let content = match app.chat.last_model.as_deref() {
                Some(model) => format!("Current model: {}", model),
                None => "Model information is not available yet.".to_string(),
            };
            app.push_assistant_message(content);
        }
        SlashCommand::DebugMemoryStats => {
            let content = format_memory_stats().unwrap_or_else(|error| format!("Failed to get memory stats: {error}"));
            app.push_assistant_message(content);
        }
        SlashCommand::DebugMemoryRecall(query) => {
            let content = format_memory_recall(&query)
                .unwrap_or_else(|error| format!("Failed to recall memory for `{query}`: {error}"));
            app.push_assistant_message(content);
        }
        SlashCommand::DebugLog(session_id) => {
            let content = format_session_logs(&session_id)
                .unwrap_or_else(|error| format!("Failed to get logs for `{session_id}`: {error}"));
            app.push_assistant_message(content);
        }
        SlashCommand::Settings => {
            app.open_settings();
        }
        SlashCommand::HelpCmd => {
            app.open_help();
        }
        SlashCommand::Unknown(raw) => {
            app.chat.messages.push(ChatMessage::assistant(format!(
                "Unknown command `{raw}`. Available: `/help`, `/settings`, `/debug chat`, `/debug files`, `/files`, `/history`, `/resume <id>`, `/clear`, `/tokens`, `/model`, `/debug memory stats`, `/debug memory recall <query>`, `/debug log <id>`."
            )));
            app.screen_mode = ScreenMode::Chat;
        }
        SlashCommand::Empty => {}
    }
}

fn parse_slash_command(raw: &str) -> SlashCommand {
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

fn format_session_history() -> std::result::Result<String, String> {
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

fn load_session_chat_messages(session_id: &str) -> std::result::Result<Vec<ChatMessage>, String> {
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

fn format_memory_stats() -> std::result::Result<String, String> {
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

fn format_memory_recall(query: &str) -> std::result::Result<String, String> {
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

fn format_session_logs(session_id: &str) -> std::result::Result<String, String> {
    let db = workspace_memory_database()?;
    let logs = LogStore::new(db)
        .get_session_logs(session_id, 200)
        .map_err(|error| error.to_string())?;

    if logs.is_empty() {
        return Ok(format!("No logs found for session `{}`.", session_id));
    }

    let mut lines = vec![format!("Logs for session `{}`:", session_id)];
    for entry in logs {
        lines.push(entry.to_string());
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_slash_command() {
        assert_eq!(parse_slash_command("/debug chat"), SlashCommand::DebugChat);
        assert_eq!(parse_slash_command("/debug files"), SlashCommand::DebugFiles);
        assert_eq!(parse_slash_command("/files"), SlashCommand::Files);
        assert_eq!(parse_slash_command("/history"), SlashCommand::History);
        assert_eq!(
            parse_slash_command("/resume abc123"),
            SlashCommand::Resume("abc123".to_string())
        );
        assert_eq!(parse_slash_command("/clear"), SlashCommand::Clear);
        assert_eq!(parse_slash_command("/tokens"), SlashCommand::Tokens);
        assert_eq!(parse_slash_command("/model"), SlashCommand::Model);
        assert_eq!(
            parse_slash_command("/debug memory stats"),
            SlashCommand::DebugMemoryStats
        );
        assert_eq!(
            parse_slash_command("/debug memory recall rust sqlite"),
            SlashCommand::DebugMemoryRecall("rust sqlite".to_string())
        );
        assert_eq!(
            parse_slash_command("/debug log sess-1"),
            SlashCommand::DebugLog("sess-1".to_string())
        );
        assert_eq!(parse_slash_command("/settings"), SlashCommand::Settings);
        assert_eq!(parse_slash_command("/help"), SlashCommand::HelpCmd);
        assert_eq!(
            parse_slash_command("/unknown"),
            SlashCommand::Unknown("/unknown".to_string())
        );
    }
}
