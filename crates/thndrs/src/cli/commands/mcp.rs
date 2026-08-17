//! MCP command definitions.

use clap::Subcommand;

/// MCP inspection and tool-call commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum McpCommand {
    /// List configured MCP servers.
    List,
    /// Show project MCP trust state and the current configuration hash.
    Status,
    /// Trust this project's current MCP configuration.
    Trust,
    /// Revoke this project's MCP trust decision.
    Revoke,
    /// Initialize one MCP server and report readiness.
    Test {
        /// Configured MCP server name.
        name: String,
    },
    /// List tools exposed by one MCP server.
    Tools {
        /// Configured MCP server name.
        name: String,
    },
    /// List compact namespaced metadata for resources advertised by one MCP server.
    Resources {
        /// Configured MCP server name.
        name: String,
    },
    /// Read one explicitly requested MCP resource as bounded JSON.
    Resource {
        /// Configured MCP server name.
        server: String,
        /// URI returned by the server's resource list.
        uri: String,
    },
    /// Call one MCP tool with JSON object arguments.
    Call {
        /// Configured MCP server name.
        server: String,
        /// Original MCP tool name.
        tool: String,
        /// JSON object arguments.
        #[arg(long = "json")]
        json: String,
    },
}
