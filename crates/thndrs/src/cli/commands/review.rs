//! Structured read-only review command definition.

use clap::{ArgGroup, Args};

/// Review exactly one resolved change target through the configured provider.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .multiple(false)
        .args(["working_tree", "revision", "range", "session"])
))]
pub struct ReviewCommand {
    /// Review staged, unstaged, and untracked working-tree changes.
    #[arg(long)]
    pub working_tree: bool,
    /// Review one Git revision.
    #[arg(long, value_name = "REVISION")]
    pub revision: Option<String>,
    /// Review a Git range written as BASE..HEAD.
    #[arg(long, value_name = "BASE..HEAD")]
    pub range: Option<String>,
    /// Review the redacted record of one local session.
    #[arg(long, value_name = "SESSION_ID")]
    pub session: Option<String>,
    /// Emit one deterministic structured result as JSON Lines.
    #[arg(long)]
    pub jsonl: bool,
}
