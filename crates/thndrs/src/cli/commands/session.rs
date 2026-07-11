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
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
    },
    /// Safely open an existing session for append-only continuation.
    Resume {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
    },
    /// Print a stable, renderer-independent session projection.
    Inspect {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = SessionDataFormat::Json)]
        format: SessionDataFormat,
    },
    /// Export redacted session records in append-only sequence order.
    Export {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = SessionDataFormat::Jsonl)]
        format: SessionDataFormat,
    },
}

/// Machine-readable session command output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SessionDataFormat {
    /// A single JSON document.
    Json,
    /// One JSON value per line.
    Jsonl,
}
