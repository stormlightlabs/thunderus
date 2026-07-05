//! Stdio ACP server binary for editor-driven `thndrs` sessions.

use std::path::PathBuf;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser, parser::ValueSource};
use thndrs_lib::cli::WebSearchMode;
use thndrs_lib::{
    config,
    server::{self, ServerConfig},
};

/// Command-line configuration accepted by the ACP stdio server.
#[derive(Debug, Parser)]
#[command(version, about = "ACP stdio server for thndrs")]
struct AcpServerCli {
    /// Working directory used for ACP sessions.
    #[arg(long, default_value = ".")]
    cwd: PathBuf,
    /// Model to use for completions.
    #[arg(long, default_value = "umans-coder")]
    model: String,
    /// Web search provider policy.
    #[arg(long, value_enum, default_value = "auto")]
    websearch: WebSearchMode,
    /// Directory for append-only session JSONL files.
    #[arg(long = "session-dir")]
    session_dir: Option<PathBuf>,
}

/// Parsed ACP CLI flags plus clap match metadata.
struct ParsedAcpServerCli {
    cwd: PathBuf,
    model: String,
    websearch: WebSearchMode,
    session_dir: Option<PathBuf>,
    matches: ArgMatches,
}

#[tokio::main]
async fn main() {
    configure_tracing();
    let server_config = match resolve_server_config() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("thndrs-acp-server: failed to resolve configuration: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = server::run_stdio(server_config).await {
        eprintln!("thndrs-acp-server: {err}");
        std::process::exit(1);
    }
}

fn configure_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

fn resolve_server_config() -> Result<ServerConfig, String> {
    let env_vars = std::env::vars()
        .filter(|(key, _)| key.starts_with("THNDRS_"))
        .collect::<Vec<_>>();
    let cli = parse_cli()?;
    let effective = load_effective_config(&cli, &env_vars)?;

    let mut cwd = effective.config.default_workspace.unwrap_or_else(|| PathBuf::from("."));
    let mut model = effective.config.model.unwrap_or_else(|| String::from("umans-coder"));
    let mut websearch = effective.config.websearch.unwrap_or(WebSearchMode::Auto);
    let mut session_dir = effective.config.session_dir;

    if is_command_line(&cli.matches, "cwd") {
        cwd = config::resolve_cli_path(&cli.cwd);
    }
    if is_command_line(&cli.matches, "model") {
        model = cli.model;
    }
    if is_command_line(&cli.matches, "websearch") {
        websearch = cli.websearch;
    }
    if is_command_line(&cli.matches, "session_dir")
        && let Some(session_dir_arg) = cli.session_dir
    {
        session_dir = Some(config::resolve_cli_path(&session_dir_arg));
    }

    Ok(ServerConfig::new(
        cwd,
        model,
        websearch.label().to_string(),
        session_dir,
    ))
}

fn parse_cli() -> Result<ParsedAcpServerCli, String> {
    let (cli, matches) = AcpServerCli::command()
        .try_get_matches_from(std::env::args_os())
        .and_then(|matches| {
            let cli = AcpServerCli::from_arg_matches(&matches)?;
            Ok((cli, matches))
        })
        .map_err(|err| err.to_string())?;
    Ok(ParsedAcpServerCli {
        cwd: cli.cwd,
        model: cli.model,
        websearch: cli.websearch,
        session_dir: cli.session_dir,
        matches,
    })
}

fn load_effective_config(
    cli: &ParsedAcpServerCli, env_vars: &[(String, String)],
) -> Result<config::EffectiveConfig, String> {
    let initial_workspace = if is_command_line(&cli.matches, "cwd") {
        config::resolve_cli_path(&cli.cwd)
    } else {
        config::default_workspace_before_project_config(env_vars).map_err(|err| err.to_string())?
    };
    config::load_effective(&initial_workspace, env_vars).map_err(|err| err.to_string())
}

fn is_command_line(matches: &ArgMatches, id: &str) -> bool {
    matches.value_source(id) == Some(ValueSource::CommandLine)
}
