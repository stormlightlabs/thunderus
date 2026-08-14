//! Persisted context inspection command definitions.

use clap::{Args, Subcommand};

/// Inspect context records from the latest or selected session.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct ContextCommand {
    /// Emit the versioned metadata-only JSON projection.
    #[arg(long, global = true)]
    pub json: bool,
    /// Exact session id or unique prefix; defaults to the latest session.
    #[arg(long, global = true)]
    pub session: Option<String>,
    #[command(subcommand)]
    pub command: Option<ContextSubcommand>,
}

/// Historical context views.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ContextSubcommand {
    /// Compare the latest two terminal request attempts.
    Changes {
        /// Earlier request id, optionally suffixed with `#attempt`.
        from_request_id: Option<String>,
        /// Later request id, optionally suffixed with `#attempt`.
        to_request_id: Option<String>,
    },
    /// Export provider-neutral OpenTelemetry observations from persisted records.
    Telemetry,
}

/// Report persisted provider usage from the latest or selected session.
#[derive(Clone, Debug, Eq, PartialEq, Args)]
pub struct UsageCommand {
    /// Emit stable JSON.
    #[arg(long)]
    pub json: bool,
    /// Exact session id or unique prefix; defaults to the latest session.
    #[arg(long)]
    pub session: Option<String>,
}
