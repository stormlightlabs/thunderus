//! Thunderus CLI - Terminal AI Assistant
//!
//! Commands:
//! - `thunderus` - Launch the TUI
//! - `thunderus debug provider <provider> --model <model>` - Test provider connectivity
//! - `thunderus debug tail` - Print recent runtime logs for this workspace
//! - `thunderus debug attach` - Stream runtime logs for this workspace

use anyhow::anyhow;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use thndrs_core::{Config, Message, build_system_message};
use thndrs_mem::{LogStore, MemoryDatabase, MemoryStore, SessionManager, hash_workspace, memory_dir};
use thndrs_providers::{ConversationEvent, ConversationLoop, create_provider};
use thndrs_ui::{IncomingStreamEvent, TokenUsage, run_welcome_app_with_streaming};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{self, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const LOG_FILE_BASENAME: &str = "runtime.log";
const DEFAULT_TAIL_LINES: usize = 120;
const DEFAULT_ATTACH_POLL_MS: u64 = 250;

/// Thunderus - Terminal AI Assistant
#[derive(Parser)]
#[command(name = "thunderus")]
#[command(about = "Terminal AI Assistant", version = "0.1.0")]
struct Cli {
    /// Config file path (defaults to ~/.thunderus/config.toml)
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
#[command(visible_alias = "dbg")]
enum Commands {
    /// Debug utilities for testing components
    Debug {
        #[command(subcommand)]
        command: DebugCommands,
    },
}

#[derive(Subcommand)]
enum DebugCommands {
    /// Test provider connectivity with a hardcoded prompt
    Provider {
        /// Provider name (e.g., moonshot, zhipu)
        provider: String,

        /// Model to use (optional, uses provider default if not specified)
        #[arg(short, long)]
        model: Option<String>,

        /// Test prompt to send
        #[arg(short, long, default_value = "Say hello and tell me your name.")]
        prompt: String,

        /// Use streaming mode
        #[arg(short, long)]
        stream: bool,
    },

    /// Print recent runtime logs for this workspace
    Tail {
        /// Number of lines to show from the end of the log file
        #[arg(short = 'n', long, default_value_t = DEFAULT_TAIL_LINES)]
        lines: usize,
    },

    /// Stream runtime logs for this workspace
    Attach {
        /// Number of lines to print before attaching
        #[arg(short = 'n', long, default_value_t = DEFAULT_TAIL_LINES)]
        lines: usize,

        /// Poll interval in milliseconds while following the file
        #[arg(long, default_value_t = DEFAULT_ATTACH_POLL_MS, value_name = "MS")]
        poll_ms: u64,
    },

    /// Memory debug utilities
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Test memory recall with a query
    Recall {
        /// Query to search for
        query: String,

        /// Number of results to return
        #[arg(short, long, default_value = "5")]
        count: usize,
    },

    /// Show memory statistics
    Stats,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace_path = std::env::current_dir().context("Failed to resolve workspace path")?;

    let (log_dir, _log_guard) = init_tracing(cli.verbose, &workspace_path)?;
    tracing::debug!("Workspace log directory: {}", log_dir.display());

    let config = if let Some(config_path) = cli.config {
        Config::from_file(&config_path).with_context(|| format!("Failed to load config from {:?}", config_path))?
    } else {
        Config::load_default().context("Failed to load default configuration")?
    };

    match cli.command {
        Some(Commands::Debug { command }) => handle_debug_command(command, &config, &workspace_path).await?,
        None => {
            tracing::info!("Starting Thunderus TUI");
            run_tui_with_provider(&config, workspace_path).context("Failed to run TUI")?;
        }
    }

    Ok(())
}

fn run_tui_with_provider(config: &Config, workspace_path: PathBuf) -> Result<()> {
    let provider_name = config.default_provider.clone();
    let provider = create_provider(&provider_name, config)
        .with_context(|| format!("Failed to create provider: {}", provider_name))?;

    let (request_tx, request_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<IncomingStreamEvent>();

    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build worker runtime");

        runtime.block_on(async move {
            if let Err(error) = thndrs_tools::init_memory_store(&workspace_path) {
                let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                    "Failed to initialize memory tools: {}",
                    error
                )));
            }

            let (_store, mut sessions, mut logs) = match thndrs_mem::init(&workspace_path) {
                Ok(parts) => parts,
                Err(error) => {
                    let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                        "Failed to initialize workspace memory: {}",
                        error
                    )));
                    return;
                }
            };

            let mut active_session_id: Option<String> = None;

            while let Ok(user_input) = request_rx.recv() {
                if let Some(command) = parse_backend_command(&user_input) {
                    handle_backend_command(command, &mut sessions, &mut logs, &mut active_session_id);
                    continue;
                }

                let session_id = match ensure_active_session(&mut sessions, &mut active_session_id, &user_input) {
                    Ok(id) => id,
                    Err(error) => {
                        let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                            "Session initialization failed: {}",
                            error
                        )));
                        continue;
                    }
                };

                let _ = logs.info(
                    Some(&session_id),
                    "runtime",
                    format!("User message received ({} chars)", user_input.chars().count()),
                );

                let history = match sessions.get_conversation_messages(&session_id) {
                    Ok(messages) => messages,
                    Err(error) => {
                        let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                            "Failed to load session history: {}",
                            error
                        )));
                        continue;
                    }
                };

                let mut loop_handler = ConversationLoop::new(workspace_path.clone());
                loop_handler.set_history(&history);

                let mut pending_calls = std::collections::HashMap::<String, (String, String)>::new();
                let mut completed_calls = Vec::<(String, String, String, String, bool)>::new();

                let response = loop_handler
                    .process_message(provider.as_ref(), &user_input, |event| match event {
                        ConversationEvent::Thinking(content) => {
                            let _ = event_tx.send(IncomingStreamEvent::Thinking(content));
                        }
                        ConversationEvent::ToolCalling { id, name, arguments } => {
                            pending_calls.insert(id, (name.clone(), arguments.clone()));
                            let _ = logs.info(
                                Some(&session_id),
                                "runtime",
                                format!("Tool call: {} {}", name, arguments),
                            );
                            let _ = event_tx.send(IncomingStreamEvent::ToolCalling { name, arguments });
                        }
                        ConversationEvent::ToolCompleted { id, name, result, is_error } => {
                            let arguments = pending_calls.remove(&id).map(|(_, args)| args).unwrap_or_default();
                            completed_calls.push((id, name.clone(), arguments, result.clone(), is_error));
                            let _ = logs.info(
                                Some(&session_id),
                                "runtime",
                                format!("Tool result: {} (error={})", name, is_error),
                            );
                            let _ = event_tx.send(IncomingStreamEvent::ToolCompleted { name, result, is_error });
                        }
                        ConversationEvent::Content { content, usage, model } => {
                            let _ = event_tx
                                .send(IncomingStreamEvent::Delta { content: Some(content), reasoning_content: None });
                            let _ = event_tx.send(IncomingStreamEvent::Done {
                                usage: Some(TokenUsage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                }),
                                model: Some(model),
                            });
                        }
                        ConversationEvent::Error(error) => {
                            let _ = event_tx.send(IncomingStreamEvent::Error(error));
                        }
                    })
                    .await;

                match response {
                    Ok(content) => {
                        if let Err(error) = sessions.add_message(&session_id, &Message::user(&user_input)) {
                            let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                                "Failed to persist user message: {}",
                                error
                            )));
                        }

                        for (tool_call_id, tool_name, arguments_json, result, is_error) in completed_calls {
                            let arguments = serde_json::from_str::<Value>(&arguments_json)
                                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

                            let status = if is_error { "error" } else { "success" };
                            let _ = sessions.add_tool_call(&session_id, &tool_name, &arguments, Some(&result), status);
                            let _ = sessions.add_message(&session_id, &Message::tool(&tool_call_id, &result));
                        }

                        if let Err(error) = sessions.add_message(&session_id, &Message::assistant(&content)) {
                            let _ = event_tx.send(IncomingStreamEvent::Error(format!(
                                "Failed to persist assistant message: {}",
                                error
                            )));
                        } else {
                            let _ = logs.info(Some(&session_id), "runtime", "Assistant response completed");
                        }
                    }
                    Err(error) => {
                        let _ = logs.error(Some(&session_id), "runtime", format!("Conversation error: {}", error));
                        let _ = event_tx.send(IncomingStreamEvent::Error(error));
                    }
                }
            }
        });
    });

    let ui_result = run_welcome_app_with_streaming(
        move |user_input| request_tx.send(user_input).map_err(|error| error.to_string()),
        move || event_rx.try_recv().ok(),
    );

    let _ = worker.join();
    ui_result?;

    Ok(())
}

#[derive(Debug, Clone)]
enum BackendCommand {
    Clear,
    Resume(String),
}

fn parse_backend_command(raw: &str) -> Option<BackendCommand> {
    let command = raw.trim();
    if command == "/clear" {
        return Some(BackendCommand::Clear);
    }

    command
        .strip_prefix("/resume ")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| BackendCommand::Resume(id.to_string()))
}

fn ensure_active_session(
    sessions: &mut SessionManager, active_session_id: &mut Option<String>, seed_message: &str,
) -> std::result::Result<String, String> {
    if let Some(session_id) = active_session_id.as_ref() {
        match sessions.get_session(session_id) {
            Ok(Some(_)) => return Ok(session_id.clone()),
            Ok(None) => {
                *active_session_id = None;
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    let title = seed_message
        .lines()
        .next()
        .unwrap_or("Session")
        .trim()
        .chars()
        .take(72)
        .collect::<String>();

    let session = sessions
        .create_session(Some(if title.is_empty() { "Session" } else { &title }))
        .map_err(|error| error.to_string())?;

    *active_session_id = Some(session.id.clone());
    Ok(session.id)
}

fn handle_backend_command(
    command: BackendCommand, sessions: &mut SessionManager, logs: &mut LogStore, active_session_id: &mut Option<String>,
) {
    match command {
        BackendCommand::Clear => {
            if let Some(session_id) = active_session_id.as_ref()
                && let Ok(deleted) = sessions.clear_messages(session_id)
            {
                let _ = logs.info(
                    Some(session_id),
                    "runtime",
                    format!("Cleared session history ({} rows)", deleted),
                );
            }
        }
        BackendCommand::Resume(session_id) => {
            if sessions.get_session(&session_id).ok().flatten().is_some() {
                let _ = logs.info(Some(&session_id), "runtime", format!("Resumed session {}", session_id));
                *active_session_id = Some(session_id);
            }
        }
    }
}

fn init_tracing(verbose: bool, workspace_path: &Path) -> Result<(PathBuf, WorkerGuard)> {
    let log_dir = workspace_log_dir(workspace_path)?;
    let file_appender = rolling::RollingFileAppender::new(Rotation::HOURLY, &log_dir, LOG_FILE_BASENAME);
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(file_appender);

    let max_level = if verbose { tracing::Level::DEBUG } else { tracing::Level::INFO };

    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking_writer);

    tracing_subscriber::registry()
        .with(LevelFilter::from_level(max_level))
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .context("Failed to set tracing subscriber")?;

    Ok((log_dir, guard))
}

fn workspace_log_dir(workspace_path: &Path) -> Result<PathBuf> {
    let memory_root = memory_dir().context("Failed to resolve memory directory")?;
    let thunderus_root = memory_root
        .parent()
        .ok_or_else(|| anyhow!("Memory directory has no parent: {}", memory_root.display()))?;
    let log_dir = thunderus_root
        .join("logs")
        .join("workspaces")
        .join(hash_workspace(workspace_path));

    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create workspace log directory at {}", log_dir.display()))?;

    Ok(log_dir)
}

fn list_workspace_log_files(log_dir: &Path) -> Result<Vec<PathBuf>> {
    if !log_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(log_dir).with_context(|| format!("Failed to read {}", log_dir.display()))? {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", log_dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if file_name.contains(LOG_FILE_BASENAME) {
            entries.push(path);
        }
    }

    entries.sort_by_key(|path| std::fs::metadata(path).and_then(|meta| meta.modified()).ok());
    Ok(entries)
}

fn latest_workspace_log_file(log_dir: &Path) -> Result<Option<PathBuf>> {
    Ok(list_workspace_log_files(log_dir)?.into_iter().last())
}

fn read_last_lines(path: &Path, lines: usize) -> Result<Vec<String>> {
    if lines == 0 {
        return Ok(Vec::new());
    }

    let file = File::open(path).with_context(|| format!("Failed to open log file at {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut ring = VecDeque::with_capacity(lines);

    for line in reader.lines() {
        let line = line.with_context(|| format!("Failed to read from log file at {}", path.display()))?;
        if ring.len() == lines {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    Ok(ring.into_iter().collect())
}

fn stream_new_log_content(path: &Path, mut offset: u64) -> Result<u64> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error).with_context(|| format!("Failed to stat log file at {}", path.display())),
    };

    if metadata.len() < offset {
        offset = 0;
    }

    let mut file = File::open(path).with_context(|| format!("Failed to open log file at {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))
        .with_context(|| format!("Failed to seek in log file at {}", path.display()))?;

    let mut chunk = String::new();
    file.read_to_string(&mut chunk)
        .with_context(|| format!("Failed to read log file at {}", path.display()))?;

    if !chunk.is_empty() {
        print!("{chunk}");
        std::io::stdout()
            .flush()
            .context("Failed to flush stdout while streaming logs")?;
    }

    file.stream_position()
        .with_context(|| format!("Failed to read position from log file at {}", path.display()))
}

fn debug_tail(workspace_path: &Path, lines: usize) -> Result<()> {
    let log_dir = workspace_log_dir(workspace_path)?;
    let files = list_workspace_log_files(&log_dir)?;
    if files.is_empty() {
        println!("No log file found for this workspace yet: {}", log_dir.display());
        return Ok(());
    }

    println!("Log directory: {}", log_dir.display());
    if let Some(active) = files.last() {
        println!("Active log file: {}", active.display());
    }

    if lines == 0 {
        return Ok(());
    }

    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut remaining = lines;
    for file in files.iter().rev() {
        let chunk = read_last_lines(file, remaining)?;
        remaining = remaining.saturating_sub(chunk.len());
        chunks.push(chunk);
        if remaining == 0 {
            break;
        }
    }

    for chunk in chunks.into_iter().rev() {
        for line in chunk {
            println!("{line}");
        }
    }

    Ok(())
}

fn debug_attach(workspace_path: &Path, lines: usize, poll_ms: u64) -> Result<()> {
    let log_dir = workspace_log_dir(workspace_path)?;
    println!("Attaching to workspace logs: {}", log_dir.display());
    println!("Press Ctrl-C to detach.");

    if lines > 0 {
        debug_tail(workspace_path, lines)?;
    }

    let mut active_file = latest_workspace_log_file(&log_dir)?;
    let mut offset = active_file
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok().map(|metadata| metadata.len()))
        .unwrap_or(0);
    let poll_interval = Duration::from_millis(poll_ms.max(1));

    loop {
        thread::sleep(poll_interval);
        let latest_file = latest_workspace_log_file(&log_dir)?;
        if latest_file != active_file {
            active_file = latest_file;
            offset = 0;
            if let Some(path) = active_file.as_ref() {
                println!("\nSwitched to log file: {}", path.display());
            }
        }
        if let Some(path) = active_file.as_ref() {
            offset = stream_new_log_content(path, offset)?;
        }
    }
}

async fn handle_debug_command(command: DebugCommands, config: &Config, workspace_path: &Path) -> Result<()> {
    match command {
        DebugCommands::Provider { provider, model, prompt, stream } => {
            debug_provider(config, &provider, model, &prompt, stream).await?;
        }
        DebugCommands::Tail { lines } => {
            debug_tail(workspace_path, lines)?;
        }
        DebugCommands::Attach { lines, poll_ms } => {
            debug_attach(workspace_path, lines, poll_ms)?;
        }
        DebugCommands::Memory { command } => {
            handle_memory_command(command, workspace_path).await?;
        }
    }
    Ok(())
}

async fn handle_memory_command(command: MemoryCommands, workspace_path: &Path) -> Result<()> {
    let db = MemoryDatabase::for_workspace(workspace_path).context("Failed to open memory database")?;
    let mut store = MemoryStore::new(db);

    match command {
        MemoryCommands::Recall { query, count } => {
            println!("Searching memory for: {}", query);
            println!();

            let results = store
                .recall(&query, count, None, None, 0.3)
                .await
                .context("Failed to recall memories")?;

            if results.is_empty() {
                println!("No relevant memories found.");
            } else {
                println!("Found {} memories:", results.len());
                for memory in results {
                    let sim = memory
                        .similarity
                        .map(|s| format!(" (similarity: {:.2})", s))
                        .unwrap_or_default();
                    println!("  - [{}] {}{}", memory.kind.as_str(), memory.content, sim);
                }
            }
        }
        MemoryCommands::Stats => {
            let stats = store.stats().context("Failed to get memory stats")?;

            println!("Memory Statistics:");
            println!("  Total memories: {}", stats.total_memories);
            println!("  Archived memories: {}", stats.archived_memories);
            println!("  Total sessions: {}", stats.total_sessions);
            println!(
                "  Database size: {} bytes ({:.2} MB)",
                stats.database_size_bytes,
                stats.database_size_bytes as f64 / 1_048_576.0
            );
            if let Some(ref model) = stats.embedding_model {
                println!("  Embedding model: {}", model);
            }
            println!("  Embedding dimensions: {}", stats.embedding_dimensions);
        }
    }

    Ok(())
}

async fn debug_provider(
    config: &Config, provider_name: &str, model: Option<String>, prompt: &str, stream: bool,
) -> Result<()> {
    println!("Debugging provider: {}", provider_name);
    println!("   Prompt: {}", prompt);

    if let Some(ref m) = model {
        println!("   Model: {}", m);
    }

    println!("   Streaming: {}", if stream { "enabled" } else { "disabled" });

    let provider = create_provider(provider_name, config)
        .with_context(|| format!("Failed to create provider: {}", provider_name))?;

    println!("   Default model: {}", provider.default_model());

    let messages = vec![build_system_message(), Message::user(prompt)];

    println!("\nSending request...\n");

    let start = std::time::Instant::now();
    let response = if stream {
        provider
            .complete_stream(&messages)
            .await
            .context("Provider streaming request failed")?
    } else {
        provider.complete(&messages).await.context("Provider request failed")?
    };
    let elapsed = start.elapsed();

    println!("Response received in {:?}\n", elapsed);

    println!("Model: {}", response.model);
    println!("Finish reason: {:?}", response.finish_reason);
    println!(
        "Usage: {} prompt + {} completion = {} total tokens",
        response.usage.prompt_tokens, response.usage.completion_tokens, response.usage.total_tokens
    );

    if let Some(ref reasoning) = response.reasoning_content {
        println!("\nReasoning:");
        println!("{}", reasoning);
    }

    println!("\nResponse content:");
    println!("{}", response.content);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_no_args() {
        let cli = Cli::parse_from([&"thunderus"]);
        assert!(cli.config.is_none());
        assert!(!cli.verbose);
        assert!(cli.command.is_none());
    }

    #[test]
    fn test_cli_parse_with_config() {
        let cli = Cli::parse_from(["thunderus", "--config", "/path/to/config.toml", "--verbose"]);
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_parse_debug_provider() {
        let cli = Cli::parse_from([
            "thunderus",
            "debug",
            "provider",
            "moonshot",
            "--model",
            "kimi-k2.5",
            "--prompt",
            "Test prompt",
            "--stream",
        ]);

        match cli.command {
            Some(Commands::Debug { command: DebugCommands::Provider { provider, model, prompt, stream } }) => {
                assert_eq!(provider, "moonshot");
                assert_eq!(model, Some("kimi-k2.5".to_string()));
                assert_eq!(prompt, "Test prompt");
                assert!(stream);
            }
            _ => panic!("Expected debug provider command"),
        }
    }

    #[test]
    fn test_cli_parse_debug_tail() {
        let cli = Cli::parse_from(["thunderus", "debug", "tail", "--lines", "42"]);

        match cli.command {
            Some(Commands::Debug { command: DebugCommands::Tail { lines } }) => {
                assert_eq!(lines, 42);
            }
            _ => panic!("Expected debug tail command"),
        }
    }

    #[test]
    fn test_cli_parse_debug_attach() {
        let cli = Cli::parse_from(["thunderus", "debug", "attach", "--lines", "25", "--poll-ms", "100"]);

        match cli.command {
            Some(Commands::Debug { command: DebugCommands::Attach { lines, poll_ms } }) => {
                assert_eq!(lines, 25);
                assert_eq!(poll_ms, 100);
            }
            _ => panic!("Expected debug attach command"),
        }
    }

    #[test]
    fn test_read_last_lines() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("thunderus-log-test-{}.log", unique));
        std::fs::write(&path, "line-1\nline-2\nline-3\nline-4\n").unwrap();

        let lines = read_last_lines(&path, 2).unwrap();
        assert_eq!(lines, vec!["line-3".to_string(), "line-4".to_string()]);

        let _ = std::fs::remove_file(path);
    }
}
