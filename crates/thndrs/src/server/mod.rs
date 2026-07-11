//! ACP agent server entrypoint and protocol handlers.
//!
//! This module exposes `thndrs` as an ACP agent over stdio. The initial server
//! boundary is deliberately small: stdout is reserved for JSON-RPC transport
//! and all diagnostics belong on stderr or tracing sinks configured by the
//! binary.

pub mod config_options;
pub mod events;
pub mod handlers;
pub mod session;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use agent_client_protocol::Result;

/// Runtime configuration accepted by the ACP agent binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    /// Workspace directory used for ACP sessions when the client does not
    /// provide a more specific root.
    pub cwd: PathBuf,
    /// Provider model selected for future harness turns.
    pub model: String,
    /// Web search policy label selected for future harness turns.
    pub websearch: String,
    /// Optional append-only session directory.
    pub session_dir: Option<PathBuf>,
}

impl ServerConfig {
    /// Build a server config from parsed binary flags.
    pub fn new(cwd: PathBuf, model: String, websearch: String, session_dir: Option<PathBuf>) -> Self {
        Self { cwd, model, websearch, session_dir }
    }
}

/// Run `thndrs-acp-server` over stdio until the ACP connection closes.
pub async fn run_stdio(config: ServerConfig) -> Result<()> {
    handlers::run_stdio(config).await
}
