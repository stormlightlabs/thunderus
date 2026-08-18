//! MCP command definitions.

use clap::{Args, Subcommand, ValueEnum};

/// Destination for an MCP server definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum McpConfigScope {
    /// The user-wide `~/.thndrs/mcp.toml` file.
    Global,
    /// The current workspace's `.thndrs/mcp.toml` file.
    Project,
}

/// Read-only discovery and configuration of MCP server catalogs.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum McpCatalogCommand {
    /// List global catalog sources and their discovery labels.
    List,
    /// Add or replace a global API-compatible catalog source.
    Add {
        /// Local source name.
        name: String,
        /// HTTPS base URL for the catalog API.
        url: String,
        /// Catalog-provided curation claim to display with its entries.
        #[arg(long)]
        curation: Option<String>,
    },
    /// Remove a global custom catalog source.
    Remove {
        /// Local source name.
        name: String,
    },
    /// Enable one global catalog source.
    Enable {
        /// Local source name.
        name: String,
    },
    /// Disable one global catalog source.
    Disable {
        /// Local source name.
        name: String,
    },
    /// Search enabled catalog metadata without starting a server.
    Search(CatalogSearchArgs),
    /// Inspect one catalog server entry without starting a server.
    Show(CatalogShowArgs),
}

/// Query options shared by catalog search.
#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct CatalogSearchArgs {
    /// Name or description text to search for.
    pub query: String,
    /// Maximum entries requested from each catalog (1-50).
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    /// Opaque cursor returned by a previous search for the same catalog.
    #[arg(long)]
    pub cursor: Option<String>,
    /// Use only the last successful metadata snapshot.
    #[arg(long)]
    pub offline: bool,
}

/// Query options for catalog detail.
#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct CatalogShowArgs {
    /// Catalog server name, such as `io.example/weather`.
    pub name: String,
    /// Restrict the lookup to one enabled source.
    #[arg(long)]
    pub source: Option<String>,
    /// Version to inspect. `latest` is the registry default.
    #[arg(long, default_value = "latest")]
    pub version: String,
    /// Use only the last successful metadata snapshot.
    #[arg(long)]
    pub offline: bool,
}

/// MCP inspection, discovery, and tool-call commands.
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
    /// Discover MCP server metadata from global catalogs.
    #[command(subcommand)]
    Catalog(McpCatalogCommand),
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
