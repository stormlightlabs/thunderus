//! Stdio ACP agent binary for editor-driven `thndrs` sessions.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use thndrs::acp_server::{self, ServerConfig};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum WebSearchFlag {
    #[default]
    Auto,
    Native,
    Exa,
    None,
}

impl WebSearchFlag {
    fn label(self) -> &'static str {
        match self {
            WebSearchFlag::Auto => "auto",
            WebSearchFlag::Native => "native",
            WebSearchFlag::Exa => "exa",
            WebSearchFlag::None => "none",
        }
    }
}

#[derive(Debug, Parser)]
#[command(version, about = "ACP stdio agent for thndrs")]
struct AcpAgentCli {
    /// Working directory used for ACP sessions.
    #[arg(long, default_value = ".")]
    cwd: PathBuf,
    /// Model to use for completions.
    #[arg(long, default_value = "umans-coder")]
    model: String,
    /// Web search provider policy.
    #[arg(long, value_enum, default_value = "auto")]
    websearch: WebSearchFlag,
    /// Directory for append-only session JSONL files.
    #[arg(long = "session-dir")]
    session_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let cli = AcpAgentCli::parse();
    let config = ServerConfig::new(cli.cwd, cli.model, cli.websearch.label().to_string(), cli.session_dir);

    if let Err(err) = acp_server::run_stdio(config).await {
        eprintln!("thndrs-acp-agent: {err}");
        std::process::exit(1);
    }
}
