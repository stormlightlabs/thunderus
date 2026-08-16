//! Project runtime trust command definitions.

use clap::{Subcommand, ValueEnum};

/// Inspect, grant, or revoke project runtime trust.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum TrustCommand {
    /// Show project resources, fingerprints, and trust states.
    Status,
    /// Trust the exact current fingerprint of one project resource class.
    Grant {
        /// Project resource class to trust.
        #[arg(value_enum)]
        scope: TrustScopeArg,
    },
    /// Revoke trust for one project resource class.
    Revoke {
        /// Project resource class to revoke.
        #[arg(value_enum)]
        scope: TrustScopeArg,
    },
}

/// A project resource class accepted by the trust CLI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TrustScopeArg {
    Configuration,
    PromptTemplates,
    Skills,
    Commands,
    Mcp,
    Hooks,
}
