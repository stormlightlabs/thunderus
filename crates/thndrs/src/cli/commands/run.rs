//! Headless single-prompt command definitions.

use clap::Args;

/// Run one coding prompt without opening the terminal interface.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct RunCommand {
    /// Prompt text to run through the configured provider.
    pub prompt: String,
}
