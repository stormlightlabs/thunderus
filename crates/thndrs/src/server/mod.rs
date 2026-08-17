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

use crate::cli::{ReasoningEffort, ReasoningSummary};
use crate::tools::ToolAuthority;
use agent_client_protocol::Result;
use thndrs_agent::context::ReductionConfig;

/// Runtime configuration accepted by the ACP agent binary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    /// Workspace directory used for ACP sessions when the client does not
    /// provide a more specific root.
    pub cwd: PathBuf,
    /// Provider model selected for future harness turns.
    pub model: String,
    /// Tool authority enforced for future harness turns.
    pub authority: ToolAuthority,
    /// Default reasoning effort for ChatGPT Codex GPT-5.6 sessions.
    pub reasoning_effort: ReasoningEffort,
    /// Default reasoning-summary policy for ChatGPT Codex GPT-5.6 sessions.
    pub reasoning_summary: ReasoningSummary,
    /// Optional append-only session directory.
    pub session_dir: Option<PathBuf>,
    /// Independent model-projection reducer configuration.
    pub model_reduction: ReductionConfig,
}

impl ServerConfig {
    /// Build a server config from parsed binary flags.
    pub fn new(cwd: PathBuf, model: String, session_dir: Option<PathBuf>) -> Self {
        Self {
            cwd,
            model,
            authority: ToolAuthority::default(),
            reasoning_effort: ReasoningEffort::default(),
            reasoning_summary: ReasoningSummary::default(),
            session_dir,
            model_reduction: ReductionConfig::default(),
        }
    }

    /// Apply resolved reasoning defaults from local configuration.
    pub fn with_reasoning(mut self, effort: ReasoningEffort, summary: ReasoningSummary) -> Self {
        self.reasoning_effort = effort;
        self.reasoning_summary = summary;
        self
    }

    /// Apply the configured tool authority.
    pub fn with_authority(mut self, authority: ToolAuthority) -> Self {
        self.authority = authority;
        self
    }

    /// Apply independent model-projection reducer settings.
    pub fn with_model_reduction(mut self, config: ReductionConfig) -> Self {
        self.model_reduction = config;
        self
    }
}

/// Run `thndrs acp serve` over stdio until the ACP connection closes.
pub async fn run_stdio(config: ServerConfig) -> Result<()> {
    handlers::run_stdio(config).await
}
