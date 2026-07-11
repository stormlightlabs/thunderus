//! Local session command definitions.

use clap::Subcommand;

/// Local session history commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum SessionCommand {
    /// List local sessions newest-first.
    List,
    /// Print the newest local session.
    Latest,
    /// List local session titles newest-first.
    Titles,
    /// Print replayable transcript entries for one local session id.
    Show {
        /// Session id without the `.jsonl` suffix.
        session_id: String,
    },
}
