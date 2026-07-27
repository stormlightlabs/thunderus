//! Headless single-prompt command definitions.

use clap::Args;

/// Default maximum number of bytes accepted from piped standard input.
pub const DEFAULT_STDIN_MAX_BYTES: usize = 64 * 1024;

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
}
