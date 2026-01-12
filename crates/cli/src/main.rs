use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use thunderus_core::{AgentDir, Config, Session};

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

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the interactive TUI session
    Start {
        /// Working directory (default: current directory)
        #[arg(short, long, value_name = "DIR")]
        dir: Option<PathBuf>,
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
    Status,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let config_path = cli.config.unwrap_or_else(|| PathBuf::from("config.toml"));
    let config = load_or_create_config(&config_path)?;

    if cli.verbose {
        println!("{} Using config: {}", "Info:".blue().bold(), config_path.display());
        println!(
            "{} Available profiles: {:?}",
            "Info:".blue().bold(),
            config.profile_names()
        );
    }

    match cli.command {
        Commands::Start { dir } => cmd_start(config, dir, cli.profile, cli.verbose)?,
        Commands::Exec { command, args } => cmd_exec(config, command, args, cli.verbose)?,
        Commands::Status => cmd_status(config, cli.verbose)?,
    }

    Ok(())
}

/// Load config from file or create from example
fn load_or_create_config(path: &Path) -> Result<Config> {
    if path.exists() {
        println!("{} Loading config from {}", "Info:".green().bold(), path.display());
        Config::from_file(&PathBuf::from(path)).map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
    } else {
        println!("{} Config not found at {}", "Warning:".yellow().bold(), path.display());
        println!("{} Creating config from example...", "Info:".blue().bold());

        std::fs::write(path, Config::example()).context("Failed to create config")?;

        println!(
            "{} Created config at {}. Please edit it with your settings.",
            "Success:".green().bold(),
            path.display()
        );

        anyhow::bail!("Please edit config.toml with your settings and run again")
    }
}

/// Start the interactive TUI session
fn cmd_start(config: Config, dir: Option<PathBuf>, profile_name: Option<String>, verbose: bool) -> Result<()> {
    let working_dir = if let Some(d) = dir { d } else { std::env::current_dir()? };

    let profile_name = profile_name.unwrap_or_else(|| config.default_profile.clone());
    let profile = config
        .profile(&profile_name)
        .with_context(|| format!("Failed to load profile '{}'", profile_name))?;

    if verbose {
        println!("{} Working directory: {}", "Info:".blue().bold(), working_dir.display());
        println!("{} Profile: {}", "Info:".blue().bold(), profile_name.cyan());
        println!("{} Provider: {:?}", "Info:".blue().bold(), profile.provider);
        println!("{} Approval mode: {}", "Info:".blue().bold(), profile.approval_mode);
        println!("{} Sandbox mode: {}", "Info:".blue().bold(), profile.sandbox_mode);
    }

    let agent_dir = AgentDir::new(&working_dir);
    let agent_dir_path = agent_dir.agent_dir();

    if !agent_dir_path.exists() {
        println!(
            "{} Creating .agent directory at {}",
            "Info:".blue().bold(),
            agent_dir_path.display()
        );
        std::fs::create_dir_all(&agent_dir_path).context("Failed to create .agent directory")?;
        std::fs::create_dir_all(agent_dir.sessions_dir()).context("Failed to create sessions directory")?;
        std::fs::create_dir_all(agent_dir.views_dir()).context("Failed to create views directory")?;
    }

    println!("{} Creating session...", "Info:".blue().bold());
    let mut session = Session::new(agent_dir.clone()).context("Failed to create session")?;

    if verbose {
        println!("{} Session ID: {}", "Info:".blue().bold(), session.id.cyan());
        println!(
            "{} Session directory: {}",
            "Info:".blue().bold(),
            session.session_dir().display()
        );
    }

    session
        .append_user_message("Session started")
        .context("Failed to log session start")?;

    println!(
        "{} Session {} started successfully",
        "Success:".green().bold(),
        session.id.cyan()
    );

    println!(
        "{} Interactive TUI not yet implemented. Session created and event logged.",
        "Info:".yellow().bold()
    );

    Ok(())
}

/// Execute a single command and exit
fn cmd_exec(config: Config, command: String, args: Vec<String>, verbose: bool) -> Result<()> {
    println!(
        "{} Executing: {} {}",
        "Info:".blue().bold(),
        command.cyan(),
        args.join(" ").cyan()
    );

    if verbose {
        println!("{} Profile: {}", "Info:".blue().bold(), config.default_profile.cyan());
    }

    println!("{} Command execution not yet implemented", "Info:".yellow().bold());

    Ok(())
}

/// Show current status
fn cmd_status(config: Config, verbose: bool) -> Result<()> {
    println!("{}", "Thunderus Status".green().bold().underline());
    println!();

    println!("{} Configuration", "Info:".blue().bold());
    println!("  Default profile: {}", config.default_profile.cyan());
    println!("  Available profiles:");
    for profile_name in config.profile_names() {
        let profile = config.profile(&profile_name).unwrap();
        println!("    - {} ({:?})", profile_name.cyan(), profile.provider);
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
                    let session_name = session_entry.file_name();
                    if let Some(name) = session_name.to_str() {
                        println!("    - {}", name.cyan());
                    }
                }
            }
        } else {
            println!();
            println!("{} Agent directory not initialized", "Info:".yellow().bold());
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
        assert!(matches!(cli.command, Commands::Start { .. }));

        let cli = Cli::try_parse_from(["thunderus", "start", "--dir", "/workspace"]).unwrap();
        if let Commands::Start { dir } = cli.command {
            assert_eq!(dir, Some(PathBuf::from("/workspace")));
        } else {
            panic!("Expected Start command");
        }
    }

    #[test]
    fn test_cli_exec_command() {
        let cli = Cli::try_parse_from(["thunderus", "exec", "cargo", "test"]).unwrap();
        assert!(matches!(cli.command, Commands::Exec { .. }));

        if let Commands::Exec { command, args } = cli.command {
            assert_eq!(command, "cargo");
            assert_eq!(args, vec!["test"]);
        } else {
            panic!("Expected Exec command");
        }
    }

    #[test]
    fn test_cli_exec_command_with_args() {
        let cli = Cli::try_parse_from(["thunderus", "exec", "cargo", "build", "--", "--release"]).unwrap();

        if let Commands::Exec { command, args } = cli.command {
            assert_eq!(command, "cargo");
            assert_eq!(args, vec!["build", "--release"]);
        } else {
            panic!("Expected Exec command");
        }
    }

    #[test]
    fn test_cli_status_command() {
        let cli = Cli::try_parse_from(["thunderus", "status"]).unwrap();
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn test_load_or_create_config_existing() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, Config::example()).unwrap();

        let result = load_or_create_config(&config_path);
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.default_profile, "default");
    }

    #[test]
    fn test_load_or_create_config_not_existing() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let result = load_or_create_config(&config_path);
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

        let result = load_or_create_config(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_status() {
        let config = create_test_config();
        let result = cmd_status(config, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_status_verbose() {
        let config = create_test_config();
        let result = cmd_status(config, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_exec() {
        let config = create_test_config();
        let result = cmd_exec(config, "echo".to_string(), vec!["test".to_string()], false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_exec_verbose() {
        let config = create_test_config();
        let result = cmd_exec(config, "ls".to_string(), vec![], true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_start_creates_session() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config();
        let working_dir = temp.path().to_path_buf();

        let result = cmd_start(config, Some(working_dir.clone()), None, true);
        assert!(result.is_ok());

        let agent_dir = temp.path().join(".agent");
        assert!(agent_dir.exists());
        assert!(agent_dir.join("sessions").exists());
        assert!(agent_dir.join("views").exists());
        let sessions_dir = agent_dir.join("sessions");
        let sessions: Vec<_> = std::fs::read_dir(sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        assert!(!sessions.is_empty());
    }

    #[test]
    fn test_cmd_start_with_profile() {
        let temp = TempDir::new().unwrap();
        let mut config = create_test_config();

        let work_profile = config.profiles.get("default").unwrap().clone();
        config.profiles.insert("work".to_string(), work_profile);
        config.default_profile = "work".to_string();

        let working_dir = temp.path().to_path_buf();
        let result = cmd_start(config, Some(working_dir), Some("work".to_string()), false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cmd_start_invalid_profile() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config();
        let working_dir = temp.path().to_path_buf();

        let result = cmd_start(config, Some(working_dir), Some("nonexistent".to_string()), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("profile"));
    }

    #[test]
    fn test_colored_output() {
        println!("{}", "Test".green().bold());
        println!("{}", "Test".blue());
        println!("{}", "Test".yellow().underline());
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
}
