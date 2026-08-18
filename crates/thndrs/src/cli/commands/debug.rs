//! Redacted diagnostic log command definitions.

use clap::Subcommand;

/// Read local diagnostic logs without opening the TUI.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum DebugCommand {
    /// Read the newest workspace daily log.
    Tail {
        /// Maximum log lines to print.
        #[arg(long, default_value_t = 200)]
        lines: usize,
    },
    /// Read the diagnostic log for one local session.
    SessionLog {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
        /// Maximum log lines to print.
        #[arg(long, default_value_t = 200)]
        lines: usize,
    },
}
