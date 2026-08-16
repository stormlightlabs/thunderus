//! Headless single-prompt command definitions.

use clap::{Args, ValueEnum};

/// Default maximum number of bytes accepted from piped standard input.
pub const DEFAULT_STDIN_MAX_BYTES: usize = 64 * 1024;

/// Durable-session policy selected by a machine caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SessionPolicy {
    /// Persist a local session that can be resumed after the run settles.
    Durable,
    /// Do not create a session or per-session artifacts.
    Ephemeral,
}

/// Tool authority selected by a machine caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum RunAuthority {
    /// Permit the normal workspace-scoped tool set.
    WorkspaceWrite,
    /// Permit only read-only tools.
    ReadOnly,
}

/// Run one coding prompt without opening the terminal interface.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct RunCommand {
    /// Prompt text to run through the configured provider. Omit it to use piped standard input.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
    /// Emit provider-neutral run events as versioned JSON Lines on standard output.
    #[arg(long)]
    pub jsonl: bool,
    /// Maximum number of bytes accepted from piped standard input.
    #[arg(long, default_value_t = DEFAULT_STDIN_MAX_BYTES)]
    pub stdin_max_bytes: usize,
    /// Maximum wall-clock duration for a JSONL run. Required with `--jsonl`.
    #[arg(long, value_name = "SECONDS")]
    pub timeout_secs: Option<u64>,
    /// Session policy for a JSONL run. Required with `--jsonl`.
    #[arg(long, value_enum)]
    pub session_policy: Option<SessionPolicy>,
    /// Tool authority for a JSONL run. Required with `--jsonl`.
    #[arg(long, value_enum)]
    pub authority: Option<RunAuthority>,
    /// Maximum bytes retained for one evidence item. Required with `--jsonl`.
    #[arg(long, value_name = "BYTES")]
    pub evidence_max_bytes: Option<usize>,
    /// Maximum bytes accepted by the request. Required with `--jsonl`.
    #[arg(long, value_name = "BYTES")]
    pub resource_max_bytes: Option<usize>,
}
