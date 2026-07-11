//! ACP command definitions.

use std::path::PathBuf;

use clap::Subcommand;

/// ACP inspection and smoke-test commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum AcpCommand {
    /// List configured ACP agents.
    List,
    /// Show one configured ACP agent with redacted values.
    Inspect {
        /// Configured ACP agent name.
        name: String,
    },
    /// Initialize an ACP agent, create a temporary session, send one prompt, and print events.
    Smoke {
        /// Configured ACP agent name.
        name: String,
        /// Prompt text to send.
        #[arg(long)]
        prompt: String,
    },
    /// Log out through the ACP agent when it advertises logout support.
    Logout {
        /// Configured ACP agent name.
        name: String,
    },
    /// List sessions owned by an ACP agent.
    ListSessions {
        /// Configured ACP agent name.
        name: String,
    },
    /// Load an ACP agent-owned session and print replayed updates.
    LoadSession {
        /// Configured ACP agent name.
        name: String,
        /// Opaque external ACP session id.
        session_id: String,
    },
    /// Resume an ACP agent-owned session without replaying history.
    ResumeSession {
        /// Configured ACP agent name.
        name: String,
        /// Opaque external ACP session id.
        session_id: String,
    },
    /// Close an ACP agent-owned session.
    CloseSession {
        /// Configured ACP agent name.
        name: String,
        /// Opaque external ACP session id.
        session_id: String,
    },
    /// List available agents from the read-only ACP Registry.
    Registry {
        /// Read registry JSON from a local file instead of the official CDN.
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Install one registry agent into workspace ACP config metadata.
    Install {
        /// Registry agent id.
        agent_id: String,
        /// Local ACP agent name. Defaults to the registry id without a trailing `-acp`.
        #[arg(long)]
        name: Option<String>,
        /// Read registry JSON from a local file instead of the official CDN.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Confirm writing workspace config and install metadata.
        #[arg(long)]
        yes: bool,
    },
    /// Update one registry-managed ACP agent in workspace config metadata.
    Update {
        /// Local ACP agent name.
        name: String,
        /// Read registry JSON from a local file instead of the official CDN.
        #[arg(long)]
        file: Option<PathBuf>,
        /// Confirm writing workspace config and install metadata.
        #[arg(long)]
        yes: bool,
    },
}
