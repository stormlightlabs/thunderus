use anyhow::{Context, Result};
use clap::{Command, CommandFactory, Parser, Subcommand};
use clap_complete::{Generator, Shell, generate};
use owo_colors::OwoColorize;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thunderus_core::{
    AgentDir, Config, ContextLoader, PatchQueueManager, Session,
    memory::{Gardener, MemoryPaths, MemoryRetriever, RetrievalPolicy},
};
use thunderus_core::{ApprovalGate, ApprovalProtocol, AutoApprove, init_debug};
use thunderus_providers::{CancelToken, ProviderFactory, ProviderHealthChecker};
use thunderus_store::{MemoryIndexer, MemoryStore, StoreRetriever};
use thunderus_tools::{SessionToolDispatcher, ToolDispatcher, ToolRegistry};
use thunderus_ui::state::AppState;

/// Thunderus - A high-performance coding agent harness
#[derive(Parser, Debug)]
#[command(name = "thunderus")]
#[command(about = "A TUI-based coding agent harness built in Rust", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    /// Path to config.toml (default: ./config.toml)
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Profile name to use (default: config's default_profile)
    #[arg(short, long, value_name = "PROFILE")]
    profile: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Working directory (for default start behavior)
    #[arg(short, long, value_name = "DIR")]
    dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the interactive TUI session
    Start {
        /// Working directory (default: current directory)
        #[arg(short, long, value_name = "DIR")]
        dir: Option<PathBuf>,

        /// Deterministic test mode for TUI testing
        #[arg(long)]
        test_mode: bool,
    },
    /// Execute a single command and exit (non-interactive mode)
    Exec {
        /// Command to execute
        #[arg(required = true, value_name = "CMD")]
        command: String,

        /// Arguments to pass to the command
        #[arg(value_name = "ARGS")]
        args: Vec<String>,
    },
    /// Show current status
    Status {
        /// Check provider connectivity
        #[arg(long)]
        check_providers: bool,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell type to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() {
    init_debug();

    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".red().bold(), e);
        let code = match e.downcast_ref::<clap::Error>() {
            Some(_) => 2,
            None => 1,
        };
        std::process::exit(code);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(|| PathBuf::from("config.toml"));
    let config = load_or_create_config(&config_path, cli.verbose)?;

    if cli.verbose {
        eprintln!("{} Using config: {}", "Info:".blue().bold(), config_path.display());
        eprintln!(
            "{} Available profiles: {:?}",
            "Info:".blue().bold(),
            config.profile_names()
        );
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        match cli.command {
            None | Some(Commands::Start { dir: None, test_mode: false }) => {
                cmd_start(config, config_path.clone(), cli.dir, cli.profile, cli.verbose, false).await
            }
            Some(Commands::Start { dir, test_mode }) => {
                cmd_start(config, config_path.clone(), dir, cli.profile, cli.verbose, test_mode).await
            }
            Some(Commands::Exec { command, args }) => cmd_exec(config, command, args, cli.profile, cli.verbose),
            Some(Commands::Status { check_providers }) => {
                cmd_status(config, cli.verbose, check_providers)
            }
            Some(Commands::Completions { shell }) => print_completions(shell, &mut Cli::command()),
        }
    })
}

fn print_completions<G: Generator>(generator: G, cmd: &mut Command) -> Result<()> {
    generate(generator, cmd, cmd.get_name().to_string(), &mut io::stdout());
    Ok(())
}

/// Load config from file or create from example
fn load_or_create_config(path: &Path, verbose: bool) -> Result<Config> {
    if path.exists() {
        if verbose {
            eprintln!("{} Loading config from {}", "Info:".green().bold(), path.display());
        }
        Config::from_file(&PathBuf::from(path)).map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
    } else {
        eprintln!("{} Config not found at {}", "Warning:".yellow().bold(), path.display());
        eprintln!("{} Creating config from example...", "Info:".blue().bold());
        std::fs::write(path, Config::example()).context("Failed to create config")?;

        eprintln!(
            "{} Created config at {}. Please edit it with your settings.",
            "Success:".green().bold(),
            path.display()
        );

        anyhow::bail!("Please edit config.toml with your settings and run again")
    }
}

fn detect_git_branch(path: &Path) -> Option<String> {
    match std::process::Command::new("git")
        .args(["-C", path.to_str().unwrap_or("."), "branch", "--show-current"])
        .output()
    {
        Ok(output) => match output.status.success() {
            true => Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            false => None,
        },
        Err(_) => None,
    }
}

/// Start the interactive TUI session
async fn cmd_start(
    config: Config,
    config_path: PathBuf,
    dir: Option<PathBuf>,
    profile_name: Option<String>,
    verbose: bool,
    test_mode: bool,
) -> Result<()> {
    let working_dir = if let Some(d) = dir { d } else { std::env::current_dir()? };

    let profile_name = profile_name.unwrap_or_else(|| config.default_profile.clone());
    let profile = config
        .profile(&profile_name)
        .with_context(|| format!("Failed to load profile '{}'", profile_name))?;

    if verbose {
        eprintln!("{} Working directory: {}", "Info:".blue().bold(), working_dir.display());
        eprintln!("{} Profile: {}", "Info:".blue().bold(), profile_name.cyan());
        eprintln!("{} Provider: {:?}", "Info:".blue().bold(), profile.provider);
        eprintln!("{} Approval mode: {}", "Info:".blue().bold(), profile.approval_mode);
        eprintln!("{} Sandbox mode: {}", "Info:".blue().bold(), profile.sandbox_mode);
    }

    let agent_dir = AgentDir::new(&working_dir);
    let agent_dir_path = agent_dir.agent_dir();

    if !agent_dir_path.exists() {
        if verbose {
            eprintln!(
                "{} Creating .agent directory at {}",
                "Info:".blue().bold(),
                agent_dir_path.display()
            );
        }
        std::fs::create_dir_all(&agent_dir_path).context("Failed to create .agent directory")?;
        std::fs::create_dir_all(agent_dir.sessions_dir()).context("Failed to create sessions directory")?;
        std::fs::create_dir_all(agent_dir.views_dir()).context("Failed to create views directory")?;
    }

    let (mut session, is_recovery) = {
        if verbose {
            eprintln!("{} Creating session...", "Info:".blue().bold());
        }
        (
            Session::new(agent_dir.clone()).context("Failed to create session")?,
            false,
        )
    };

    if verbose {
        eprintln!("{} Session ID: {}", "Info:".blue().bold(), session.id.cyan());
        eprintln!(
            "{} Session directory: {}",
            "Info:".blue().bold(),
            session.session_dir().display()
        );
    }

    if verbose {
        eprintln!("{} Loading context files...", "Info:".blue().bold());
    }

    let mut context_loader = ContextLoader::new(working_dir.clone());
    match context_loader.append_to_session(&mut session) {
        Ok(count) => {
            if verbose && count > 0 {
                eprintln!("{} Loaded {} context file(s)", "Info:".green().bold(), count);
            }
        }
        Err(e) => {
            if verbose {
                eprintln!(
                    "{} Warning: Failed to load context files: {}",
                    "Warning:".yellow().bold(),
                    e
                );
            }
        }
    }

    if verbose {
        eprintln!("{} Initializing memory index...", "Info:".blue().bold());
    }

    let memory_paths = MemoryPaths::from_thunderus_root(&working_dir);
    let db_path = working_dir
        .join(".thunderus")
        .join("memory")
        .join("indexes")
        .join("memory.db");

    let store_result = MemoryStore::open(&db_path).await;
    let (memory_store, index_result) = match store_result {
        Ok(store) => {
            let store_clone = store.clone();
            let indexer = MemoryIndexer::new(store, memory_paths.clone(), &working_dir);
            let result = indexer.index_changed().await;
            match result {
                Ok(r) if r.docs_added == 0 && r.docs_updated == 0 => {
                    if verbose {
                        eprintln!("{} Memory index is up to date", "Info:".green().bold());
                    }
                    (Some(store_clone), r)
                }
                Ok(_) => {
                    let full_result = indexer.reindex_all().await;
                    match full_result {
                        Ok(r) => {
                            if verbose {
                                eprintln!(
                                    "{} Memory index: {} added, {} updated",
                                    "Info:".green().bold(),
                                    r.docs_added,
                                    r.docs_updated
                                );
                            }
                            (Some(store_clone), r)
                        }
                        Err(e) => {
                            if verbose {
                                eprintln!(
                                    "{} Warning: Memory index update failed: {}",
                                    "Warning:".yellow().bold(),
                                    e
                                );
                            }

                            (Some(store_clone), thunderus_store::IndexResult::default())
                        }
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("{} Warning: Memory indexing failed: {}", "Warning:".yellow().bold(), e);
                    }
                    let result = thunderus_store::IndexResult {
                        docs_added: 0,
                        docs_updated: 0,
                        docs_deleted: 0,
                        errors: vec![],
                        duration_ms: 0,
                    };
                    (Some(store_clone), result)
                }
            }
        }
        Err(e) => {
            if verbose {
                eprintln!(
                    "{} Warning: Failed to open memory store: {}",
                    "Warning:".yellow().bold(),
                    e
                );
            }
            let result = thunderus_store::IndexResult {
                docs_added: 0,
                docs_updated: 0,
                docs_deleted: 0,
                errors: vec![],
                duration_ms: 0,
            };
            (None, result)
        }
    };

    if !index_result.errors.is_empty() && verbose {
        eprintln!(
            "{} Memory indexing had {} error(s)",
            "Warning:".yellow().bold(),
            index_result.errors.len()
        );
    }

    let memory_retriever = memory_store.map(|store| {
        let policy = RetrievalPolicy {
            enable_vector_fallback: profile.memory.enable_vector_search,
            score_threshold: profile.memory.vector_fallback_threshold,
            ..Default::default()
        };
        Arc::new(StoreRetriever::new(Arc::new(store), policy)) as Arc<dyn MemoryRetriever>
    });

    if !is_recovery {
        session
            .append_user_message("Session started")
            .context("Failed to log session start")?;
    }

    let git_branch = detect_git_branch(&working_dir);

    let provider = ProviderFactory::create_from_config(&profile.provider).context("Failed to create provider")?;

    let mut app_state = AppState::new(
        working_dir.clone(),
        profile_name.clone(),
        profile.provider.clone(),
        profile.approval_mode,
        profile.sandbox_mode,
        profile.is_network_allowed(),
    );

    app_state.config.config_path = Some(config_path.clone());

    if let Some(theme_value) = profile.options.get("theme")
        && let Some(variant) = thunderus_ui::ThemeVariant::parse_str(theme_value)
    {
        app_state.set_theme_variant(variant);
    }

    let mut app = thunderus_ui::App::with_provider(app_state, provider)
        .with_session(session.clone())
        .with_profile(profile.clone());

    if let Some(retriever) = memory_retriever {
        app = app.with_memory_retriever(retriever);
    }

    if let Some(branch) = git_branch {
        app.state_mut().config.git_branch = Some(branch);
    }

    if is_recovery {
        if let Err(e) = app.reconstruct_transcript_from_session() {
            eprintln!(
                "{} Warning: Failed to reconstruct transcript: {}",
                "Warning:".yellow().bold(),
                e
            );
            app.transcript_mut()
                .add_system_message("Note: Some previous session events could not be loaded.");
        }

        app.transcript_mut()
            .add_system_message(format!("Session recovered: {}", session.id));
    }

    if test_mode {
        app.state_mut().set_test_mode(true);
        app.transcript_mut()
            .add_system_message("Test mode enabled: deterministic behavior for testing");
    }

    match app.run().await {
        Ok(_) => {
            run_consolidation(&session, &agent_dir, &memory_paths, &working_dir, verbose)?;
            Ok(())
        }
        Err(e) => {
            eprintln!("{} TUI error: {}", "Error:".red().bold(), e);
            Err(e.into())
        }
    }
}

/// Run consolidation on a completed session
fn run_consolidation(
    session: &Session, agent_dir: &AgentDir, memory_paths: &MemoryPaths, _working_dir: &Path, verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!("{} Running memory consolidation...", "Info:".blue().bold());
    }

    let gardener = Gardener::new(memory_paths.clone());
    let session_id = session.id.to_string();
    let events_file = session.events_file();
    let rt = tokio::runtime::Runtime::new().context("Failed to create runtime for consolidation")?;
    let result = rt.block_on(async { gardener.consolidate_session(&session_id, &events_file).await });

    match result {
        Ok(consolidation_result) => {
            if !consolidation_result.patches.is_empty() {
                if verbose {
                    eprintln!(
                        "{} Memory consolidation: {} patches generated",
                        "Info:".green().bold(),
                        consolidation_result.patches.len()
                    );
                }

                let mut queue_manager = PatchQueueManager::new(session.id.clone(), agent_dir.clone())
                    .load()
                    .unwrap_or_else(|_| PatchQueueManager::new(session.id.clone(), agent_dir.clone()));

                for patch_params in consolidation_result.patches {
                    match queue_manager.queue_memory_update(patch_params.clone()) {
                        Ok(patch_id) => {
                            if verbose {
                                eprintln!("  {} Queued: {} ({})", "+".green(), patch_params.description, patch_id);
                            }
                        }
                        Err(e) => {
                            if verbose {
                                eprintln!("  {} Failed to queue patch: {}", "!".yellow(), e);
                            }
                        }
                    }
                }
            } else if verbose {
                eprintln!("{} Memory consolidation: no changes needed", "Info:".green().bold());
            }

            if let Some(recap) = consolidation_result.recap
                && verbose
            {
                eprintln!(
                    "{} Session recap written to: {}",
                    "Info:".green().bold(),
                    recap.path.display()
                );
            }

            for warning in consolidation_result.warnings {
                eprintln!("{} {}", "Warning:".yellow().bold(), warning);
            }

            Ok(())
        }
        Err(e) => {
            if verbose {
                eprintln!("{} Memory consolidation failed: {}", "Warning:".yellow().bold(), e);
            }
            Ok(())
        }
    }
}

/// Execute a single command and exit (non-interactive mode)
fn cmd_exec(
    config: Config, command: String, args: Vec<String>, profile_name: Option<String>, verbose: bool,
) -> Result<()> {
    let profile_name = profile_name.unwrap_or_else(|| config.default_profile.clone());
    let profile = config
        .profile(&profile_name)
        .with_context(|| format!("Failed to load profile '{}'", profile_name))?;

    let working_dir = std::env::current_dir()?;
    let agent_dir = AgentDir::new(&working_dir);

    if verbose {
        eprintln!(
            "{} Executing: {} {}",
            "Info:".blue().bold(),
            command.cyan(),
            args.join(" ").cyan()
        );
        eprintln!("{} Profile: {}", "Info:".blue().bold(), profile_name.cyan());
        eprintln!("{} Working directory: {}", "Info:".blue().bold(), working_dir.display());
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let session = Session::new(agent_dir).context("Failed to create session")?;
        let provider = ProviderFactory::create_from_config(&profile.provider).context("Failed to create provider")?;
        let approval_protocol = Arc::new(AutoApprove::new()) as Arc<dyn ApprovalProtocol>;
        let approval_gate = ApprovalGate::new(profile.approval_mode, profile.is_network_allowed());
        let cancel_token = CancelToken::new();

        let mut agent = thunderus_agent::Agent::new(
            Arc::clone(&provider),
            approval_protocol,
            approval_gate,
            session.id.clone(),
        );

        agent = agent.with_profile(profile.clone());

        let mut tool_registry = ToolRegistry::with_builtin_tools();
        if let Err(e) = tool_registry.load_skills()
            && verbose
        {
            eprintln!("{} Warning: Failed to load skills: {}", "Warning:".yellow(), e);
        }
        tool_registry.set_profile(profile.clone());

        let tool_specs = tool_registry.specs();
        let dispatcher = ToolDispatcher::new(tool_registry);
        let session_dispatcher = SessionToolDispatcher::with_new_history(dispatcher, session.clone());
        agent = agent.with_tool_dispatcher(std::sync::Arc::new(std::sync::Mutex::new(session_dispatcher)));

        let full_command = if args.is_empty() { command } else { format!("{} {}", command, args.join(" ")) };

        let mut event_rx = agent
            .process_message(&full_command, Some(tool_specs), cancel_token.clone(), vec![])
            .await
            .context("Failed to process message")?;

        let mut has_output = false;
        while let Some(event) = event_rx.recv().await {
            match event {
                thunderus_agent::AgentEvent::Token(text) => {
                    print!("{}", text);
                    std::io::stdout().flush().ok();
                    has_output = true;
                }
                thunderus_agent::AgentEvent::ToolCall { name, description, .. } => {
                    eprintln!("\n{} Tool: {}", "Info:".blue(), name);
                    if let Some(desc) = description {
                        eprintln!("  Description: {}", desc);
                    }
                }
                thunderus_agent::AgentEvent::ToolResult { name, result, success, error, .. } => {
                    if !has_output {
                        eprintln!();
                    }
                    if success {
                        eprintln!("{} {} completed:", "Success:".green(), name);
                        let output: String = result.chars().take(500).collect();
                        if result.len() > 500 {
                            eprintln!("    {}...\n(truncated, {} total chars)", output, result.len());
                        } else {
                            eprintln!("    {}", output);
                        }
                    } else if let Some(err) = error {
                        eprintln!("{} {} failed: {}", "Error:".red(), name, err);
                    } else {
                        eprintln!("{} {} failed", "Error:".red(), name);
                    }
                }
                // NOTE: in exec mode, we auto-approve
                thunderus_agent::AgentEvent::ApprovalRequest(_) => {}
                thunderus_agent::AgentEvent::Error(msg) => {
                    eprintln!("{} {}", "Error:".red(), msg);
                }
                thunderus_agent::AgentEvent::Done => {
                    if has_output {
                        eprintln!();
                    }
                    eprintln!("{} Execution completed", "Success:".green());
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    })
}

/// Show current status
fn cmd_status(config: Config, verbose: bool, check_providers: bool) -> Result<()> {
    println!("{}", "Thunderus Status".green().bold().underline());
    println!();

    println!("{} Configuration", "Info:".blue().bold());
    println!("  Default profile: {}", config.default_profile.cyan());
    println!("  Available profiles:");
    for profile_name in config.profile_names() {
        let profile = config.profile(&profile_name).unwrap();
        println!("    - {} ({:?})", profile_name.cyan(), profile.provider);
    }

    if check_providers {
        println!();
        println!("{} Provider Health Checks", "Info:".blue().bold());

        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            for profile_name in config.profile_names() {
                let profile = config.profile(&profile_name).unwrap();

                match ProviderFactory::create_from_config(&profile.provider) {
                    Ok(provider) => {
                        let health_checker = ProviderHealthChecker::new(provider, Duration::from_secs(10));
                        match health_checker.check().await {
                            Ok(result) => {
                                if result.healthy {
                                    println!(
                                        "  {} ({:?}): {} (latency: {}ms)",
                                        profile_name.cyan(),
                                        profile.provider,
                                        "OK".green().bold(),
                                        result.latency_ms.to_string().yellow()
                                    );
                                } else {
                                    println!(
                                        "  {} ({:?}): {} - {}",
                                        profile_name.cyan(),
                                        profile.provider,
                                        "ERROR".red().bold(),
                                        result.error.unwrap_or_else(|| "Unknown error".to_string()).red()
                                    );
                                }
                            }
                            Err(e) => {
                                println!(
                                    "  {} ({:?}): {} - {}",
                                    profile_name.cyan(),
                                    profile.provider,
                                    "ERROR".red().bold(),
                                    e.to_string().red()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!(
                            "  {} ({:?}): {} - {}",
                            profile_name.cyan(),
                            profile.provider,
                            "ERROR".red().bold(),
                            e.to_string().red()
                        );
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        })?;
    }

    if verbose {
        let agent_dir = AgentDir::from_current_dir().context("Failed to get current directory")?;
        let agent_dir_path = agent_dir.agent_dir();

        if agent_dir_path.exists() {
            println!();
            println!("{} Agent directory", "Info:".blue().bold());
            println!("  Path: {}", agent_dir_path.display().cyan());

            let sessions_dir = agent_dir.sessions_dir();
            if sessions_dir.exists() {
                let sessions: Vec<_> = std::fs::read_dir(sessions_dir)
                    .context("Failed to read sessions")?
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.path().is_dir())
                    .collect();

                println!("  Sessions: {}", sessions.len().to_string().cyan());
                for session_entry in &sessions {
                    if let Some(name) = session_entry.file_name().to_str() {
                        println!("    - {}", name.cyan());
                    }
                }
            }
        } else {
            println!("\n{} Agent directory not initialized", "Info:".yellow().bold());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use tempfile::TempDir;

    #[test]
    fn test_cli_verify() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_cli_default_values() {
        let cli = Cli::try_parse_from(["thunderus", "status"]).unwrap();
        assert!(cli.config.is_none());
        assert!(cli.profile.is_none());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_cli_with_config() {
        let cli = Cli::try_parse_from(["thunderus", "--config", "/path/to/config.toml", "status"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_cli_with_profile() {
        let cli = Cli::try_parse_from(["thunderus", "--profile", "work", "status"]).unwrap();
        assert_eq!(cli.profile, Some("work".to_string()));
    }

    #[test]
    fn test_cli_with_verbose() {
        let cli = Cli::try_parse_from(["thunderus", "--verbose", "status"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_start_command() {
        let cli = Cli::try_parse_from(["thunderus", "start"]).unwrap();
        assert!(matches!(cli.command.unwrap(), Commands::Start { .. }));

        let cli = Cli::try_parse_from(["thunderus", "start", "--dir", "/workspace"]).unwrap();
        if let Some(Commands::Start { dir, test_mode: _ }) = cli.command {
            assert_eq!(dir, Some(PathBuf::from("/workspace")));
        } else {
            panic!("Expected Start command");
        }
    }

    #[test]
    fn test_cli_exec_command() {
        let cli = Cli::try_parse_from(["thunderus", "exec", "cargo", "test"]).unwrap();
        let cmd = cli.command.unwrap();
        assert!(matches!(cmd, Commands::Exec { .. }));

        if let Commands::Exec { command, args } = cmd {
            assert_eq!(command, "cargo");
            assert_eq!(args, vec!["test"]);
        } else {
            panic!("Expected Exec command");
        }
    }

    #[test]
    fn test_cli_exec_command_with_args() {
        let cli = Cli::try_parse_from(["thunderus", "exec", "cargo", "build", "--", "--release"]).unwrap();

        if let Some(Commands::Exec { command, args }) = cli.command {
            assert_eq!(command, "cargo");
            assert_eq!(args, vec!["build", "--release"]);
        } else {
            panic!("Expected Exec command");
        }
    }

    #[test]
    fn test_cli_status_command() {
        let cli = Cli::try_parse_from(["thunderus", "status"]).unwrap();
        assert!(matches!(cli.command.unwrap(), Commands::Status { check_providers: _ }));
    }

    #[test]
    fn test_load_or_create_config_existing() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, Config::example()).unwrap();

        let result = load_or_create_config(&config_path, false);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.default_profile, "default");
    }

    #[test]
    fn test_load_or_create_config_not_existing() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        let result = load_or_create_config(&config_path, false);
        assert!(result.is_err());
        assert!(config_path.exists());

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("default_profile"));
        assert!(content.contains("[profiles.default]"));
    }

    #[test]
    fn test_load_or_create_config_invalid() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "invalid toml").unwrap();

        let result = load_or_create_config(&config_path, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_status() {
        let config = create_test_config();
        let result = cmd_status(config, false, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_status_verbose() {
        let config = create_test_config();
        let result = cmd_status(config, true, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_status_check_providers() {
        let config = create_test_config();
        let result = cmd_status(config, false, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_exec() {
        let config = create_test_config();
        let result = cmd_exec(config, "echo".to_string(), vec!["test".to_string()], None, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_exec_verbose() {
        let config = create_test_config();
        let result = cmd_exec(config, "ls".to_string(), vec![], None, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_start_creates_session() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session = Session::new(agent_dir.clone()).unwrap();
        assert!(!session.id.as_str().is_empty());
        assert!(session.session_dir().exists());
        assert!(session.events_file().exists());
        assert_eq!(session.event_count().unwrap(), 0);
    }

    #[test]
    fn test_cmd_start_loads_context_files() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        std::fs::write(temp.path().join("CLAUDE.md"), "# Project Context\n\nTest content").unwrap();

        let mut session = Session::new(agent_dir.clone()).unwrap();
        let mut context_loader = ContextLoader::new(temp.path().to_path_buf());

        let count = context_loader.append_to_session(&mut session).unwrap();
        assert_eq!(count, 1);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 1);

        if let thunderus_core::Event::ContextLoad { source, .. } = &events[0].event {
            assert_eq!(source, "CLAUDE.md");
        } else {
            panic!("Expected ContextLoad event");
        }
    }

    #[test]
    fn test_cmd_start_loads_multiple_context_files() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());

        std::fs::write(temp.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        std::fs::write(temp.path().join("AGENTS.md"), "# Agents\n").unwrap();
        std::fs::write(temp.path().join("GEMINI.md"), "# Gemini\n").unwrap();

        let mut session = Session::new(agent_dir.clone()).unwrap();
        let mut context_loader = ContextLoader::new(temp.path().to_path_buf());

        let count = context_loader.append_to_session(&mut session).unwrap();
        assert_eq!(count, 3);

        let events = session.read_events().unwrap();
        assert_eq!(events.len(), 3);

        let sources: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let thunderus_core::Event::ContextLoad { source, .. } = &e.event {
                    Some(source.as_str())
                } else {
                    None
                }
            })
            .collect();

        assert!(sources.contains(&"CLAUDE.md"));
        assert!(sources.contains(&"AGENTS.md"));
        assert!(sources.contains(&"GEMINI.md"));
    }

    #[test]
    fn test_cmd_start_with_profile() {
        let temp = TempDir::new().unwrap();
        let _config = create_test_config();
        let _working_dir = temp.path().to_path_buf();
        let profile_name = "default".to_string();
        let profile = _config.profile(&profile_name).unwrap();
        assert_eq!(profile.name, "default");
        assert_eq!(profile.approval_mode, thunderus_core::ApprovalMode::Auto);
        assert_eq!(profile.sandbox_mode, thunderus_core::SandboxMode::Policy);
    }

    #[test]
    fn test_welcome_message_format() {
        let temp = TempDir::new().unwrap();
        let agent_dir = AgentDir::new(temp.path());
        let session = Session::new(agent_dir).unwrap();

        let welcome_msg = format!(
            "Session started\n\
             Session ID: {}\n\
             Working directory: {}\n\
             Profile: {}\n\
             Approval mode: {}\n\
             Sandbox mode: {}\n\
             Quick help: Ctrl+C to cancel, Esc to clear input",
            session.id.as_str(),
            temp.path().display(),
            "test",
            "auto",
            "policy"
        );

        assert!(welcome_msg.contains("Session started"));
        assert!(welcome_msg.contains(session.id.as_str()));
        assert!(welcome_msg.contains("Profile: test"));
        assert!(welcome_msg.contains("Approval mode: auto"));
        assert!(welcome_msg.contains("Sandbox mode: policy"));
        assert!(welcome_msg.contains("Quick help: Ctrl+C to cancel, Esc to clear input"));
    }

    #[tokio::test]
    async fn test_cmd_start_invalid_profile() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config();
        let working_dir = temp.path().to_path_buf();
        let config_path = temp.path().join("config.toml");
        let result = cmd_start(
            config,
            config_path,
            Some(working_dir),
            Some("nonexistent".to_string()),
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("profile"));
    }

    #[test]
    fn test_detect_git_branch_no_repo() {
        let temp = TempDir::new().unwrap();
        let branch = detect_git_branch(temp.path());
        assert!(branch.is_none());
    }

    #[test]
    fn test_detect_git_branch_with_repo() {
        let temp = TempDir::new().unwrap();
        let working_dir = temp.path();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["checkout", "-b", "test-branch"])
            .current_dir(working_dir)
            .output()
            .unwrap();

        let branch = detect_git_branch(working_dir);
        assert_eq!(branch, Some("test-branch".to_string()));
    }

    #[test]
    fn test_cmd_start_creates_app_state() {
        let temp = TempDir::new().unwrap();
        let _config = create_test_config();
        let working_dir = temp.path().to_path_buf();

        let _profile = _config.profile("default").unwrap();
        let app_state = AppState::new(
            working_dir.clone(),
            "default".to_string(),
            _profile.provider.clone(),
            _profile.approval_mode,
            _profile.sandbox_mode,
            _profile.is_network_allowed(),
        );

        assert_eq!(app_state.config.cwd, working_dir);
        assert_eq!(app_state.config.profile, "default");
        assert_eq!(app_state.config.approval_mode, _profile.approval_mode);
        assert_eq!(app_state.config.sandbox_mode, _profile.sandbox_mode);
    }

    #[test]
    fn test_cmd_start_with_git_branch() {
        let temp = TempDir::new().unwrap();
        let _config = create_test_config();
        let working_dir = temp.path().to_path_buf();

        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&working_dir)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["checkout", "-b", "feature-test"])
            .current_dir(&working_dir)
            .output()
            .unwrap();

        let branch = detect_git_branch(&working_dir);
        assert_eq!(branch, Some("feature-test".to_string()));
    }

    #[test]
    fn test_colored_output() {
        println!("{}", "Test".green().bold());
        println!("{}", "Test".blue());
        println!("{}", "Test".yellow().underline());
    }

    #[test]
    fn test_profile_provider_config() {
        let config = create_test_config();
        let profile = config.profile("default").unwrap();
        match profile.provider {
            thunderus_core::ProviderConfig::Glm { ref model, .. } => {
                assert_eq!(model, "glm-4.7");
            }
            _ => panic!("Expected GLM provider config"),
        }
    }

    fn create_test_config() -> Config {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"
allow_network = false

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;
        Config::from_toml_str(toml).unwrap()
    }

    #[test]
    fn test_cli_completions_command_bash() {
        let cli = Cli::try_parse_from(["thunderus", "completions", "bash"]).unwrap();
        let cmd = cli.command.unwrap();
        assert!(matches!(cmd, Commands::Completions { .. }));

        if let Commands::Completions { shell } = cmd {
            assert_eq!(shell, Shell::Bash);
        } else {
            panic!("Expected Completions command");
        }
    }

    #[test]
    fn test_cli_completions_command_zsh() {
        let cli = Cli::try_parse_from(["thunderus", "completions", "zsh"]).unwrap();
        let cmd = cli.command.unwrap();
        assert!(matches!(cmd, Commands::Completions { .. }));

        if let Commands::Completions { shell } = cmd {
            assert_eq!(shell, Shell::Zsh);
        } else {
            panic!("Expected Completions command");
        }
    }

    #[test]
    fn test_cli_completions_command_fish() {
        let cli = Cli::try_parse_from(["thunderus", "completions", "fish"]).unwrap();
        let cmd = cli.command.unwrap();
        assert!(matches!(cmd, Commands::Completions { .. }));

        if let Commands::Completions { shell } = cmd {
            assert_eq!(shell, Shell::Fish);
        } else {
            panic!("Expected Completions command");
        }
    }

    #[test]
    fn test_cli_completions_invalid_shell() {
        let cli = Cli::try_parse_from(["thunderus", "completions", "invalid"]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_cli_completions_output_no_panic() {
        use clap_complete::generate_to;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut cmd = Cli::command();

        let result = generate_to(Shell::Bash, &mut cmd, "thunderus", temp_dir.path());
        assert!(result.is_ok());

        let completion_file = temp_dir.path().join("thunderus.bash");
        assert!(completion_file.exists());

        let content = std::fs::read_to_string(&completion_file).unwrap();
        assert!(content.contains("thunderus"));
    }

    #[test]
    fn test_cli_completions_all_shells() {
        use clap_complete::generate_to;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let mut cmd = Cli::command();

        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let result = generate_to(shell, &mut cmd, "thunderus", temp_dir.path());
            assert!(result.is_ok(), "Failed to generate completions for {:?}", shell);
        }
    }
}
