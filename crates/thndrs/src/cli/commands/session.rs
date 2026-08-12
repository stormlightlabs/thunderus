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
    /// Create a new session from a settled turn boundary.
    Fork {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
        /// Replayable settled turn to use as the fork boundary.
        turn_id: String,
    },
    /// Assign or change the display name without changing session identity.
    Rename {
        /// Exact id or unique id prefix, without the `.jsonl` suffix.
        session_id: String,
        /// Short display name used by session surfaces.
        name: String,
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
    /// Preview or apply retention to unprotected live sessions.
    Prune {
        /// Select sessions whose durable activity is older than this many days.
        #[arg(long)]
        older_than: Option<u64>,
        /// Keep at least this many unprotected live sessions.
        #[arg(long)]
        keep_count: Option<usize>,
        /// Report the exact plan without changing storage.
        #[arg(long)]
        dry_run: bool,
        /// Output format.
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Report workspace session storage and policy-reclaimable bytes.
    Storage {
        /// Output format.
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Move a live session to the archive.
    Archive {
        session_id: String,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Return an archived session to the live set.
    Unarchive {
        session_id: String,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Protect a live or archived session from automatic retention.
    Pin {
        session_id: String,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Remove retention protection from a session.
    Unpin {
        session_id: String,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Preview or reversibly delete a session.
    Delete {
        session_id: String,
        /// Apply the previewed deletion.
        #[arg(long)]
        yes: bool,
        /// Explicitly permit deletion of a pinned session.
        #[arg(long)]
        allow_pinned: bool,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Restore a session within its configured trash-retention period.
    Restore {
        session_id: String,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
    /// Preview or remove all eligible session state owned by this workspace.
    Purge {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        allow_pinned: bool,
        #[arg(long, value_enum, default_value_t = SessionReportFormat::Human)]
        format: SessionReportFormat,
    },
}

/// Stable human or JSON reporting for session administration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SessionReportFormat {
    Human,
    Json,
}

/// Session command output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SessionDataFormat {
    /// A single JSON document.
    Json,
    /// One JSON value per line.
    Jsonl,
    /// A human-readable Markdown review copy.
    Markdown,
    /// A self-contained HTML review copy.
    Html,
}
