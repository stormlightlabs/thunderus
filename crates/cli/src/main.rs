//! Thunderus CLI - Terminal AI Assistant
//!
//! Commands:
//! - `thunderus` - Launch the TUI
//! - `thunderus debug provider <provider> --model <model>` - Test provider connectivity

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use thunderus_core::{Config, Message};
use thunderus_providers::create_provider;
use thunderus_ui::run_welcome_app;

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
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO })
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    let config = if let Some(config_path) = cli.config {
        Config::from_file(&config_path).with_context(|| format!("Failed to load config from {:?}", config_path))?
    } else {
        Config::load_default().context("Failed to load default configuration")?
    };

    match cli.command {
        Some(Commands::Debug { command }) => handle_debug_command(command, &config).await?,
        None => {
            tracing::info!("Starting Thunderus TUI");
            run_welcome_app().context("Failed to run TUI")?;
        }
    }

    Ok(())
}

async fn handle_debug_command(command: DebugCommands, config: &Config) -> Result<()> {
    match command {
        DebugCommands::Provider { provider, model, prompt } => {
            debug_provider(config, &provider, model, &prompt).await?;
        }
    }
    Ok(())
}

async fn debug_provider(config: &Config, provider_name: &str, model: Option<String>, prompt: &str) -> Result<()> {
    println!("🔍 Debugging provider: {}", provider_name);
    println!("   Prompt: {}", prompt);

    if let Some(ref m) = model {
        println!("   Model: {}", m);
    }

    let provider = create_provider(provider_name, config)
        .with_context(|| format!("Failed to create provider: {}", provider_name))?;

    println!("   Default model: {}", provider.default_model());

    let messages = vec![Message::system("You are a helpful assistant."), Message::user(prompt)];

    println!("\nSending request...\n");

    let start = std::time::Instant::now();
    let response = provider.complete(&messages).await.context("Provider request failed")?;
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
        ]);

        match cli.command {
            Some(Commands::Debug { command: DebugCommands::Provider { provider, model, prompt } }) => {
                assert_eq!(provider, "moonshot");
                assert_eq!(model, Some("kimi-k2.5".to_string()));
                assert_eq!(prompt, "Test prompt");
            }
            _ => panic!("Expected debug provider command"),
        }
    }
}
