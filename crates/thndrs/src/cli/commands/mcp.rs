//! MCP command definitions.

use clap::{Subcommand, ValueEnum};

/// Destination for an MCP server definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum McpConfigScope {
    /// The user-wide `~/.thndrs/mcp.toml` file.
    Global,
    /// The current workspace's `.thndrs/mcp.toml` file.
    Project,
}

/// MCP inspection and tool-call commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum McpCommand {
    /// Add or replace one MCP server definition without starting it.
    Add {
        /// Name for the server definition.
        name: String,
        /// Destination configuration file.
        #[arg(long, value_enum)]
        scope: McpConfigScope,
        /// Executable command for a stdio server.
        #[arg(long, conflicts_with = "url")]
        command: Option<String>,
        /// One argument passed to a stdio command. Repeat for each argument.
        #[arg(long = "arg", requires = "command", allow_hyphen_values = true)]
        args: Vec<String>,
        /// Streamable HTTP endpoint for a remote server.
        #[arg(long, conflicts_with = "command")]
        url: Option<String>,
    },
    /// Remove one MCP server definition without starting it.
    Remove {
        /// Name for the server definition.
        name: String,
        /// Destination configuration file.
        #[arg(long, value_enum)]
        scope: McpConfigScope,
    },
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
