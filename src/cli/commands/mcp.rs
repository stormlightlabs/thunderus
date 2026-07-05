//! MCP command definitions.

use clap::Subcommand;

/// MCP inspection and tool-call commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum McpCommand {
    /// List configured MCP servers.
    List,
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
