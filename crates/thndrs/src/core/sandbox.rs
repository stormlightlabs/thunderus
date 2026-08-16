//! Execution-boundary vocabulary and reports.
//!
//! A tool policy limits which built-in tools `thndrs` dispatches. It is not an
//! operating-system sandbox. This module keeps filesystem authority, network
//! authority, and isolation status separate so callers can state the boundary
//! actually used by an executor.

/// Filesystem authority relevant to an execution boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemAuthority {
    /// The executor can read the workspace but cannot write it.
    ReadOnly,
    /// The executor can read and write the workspace.
    WorkspaceWrite,
    /// The executor inherits the `thndrs` process filesystem authority.
    HostProcess,
    /// An external executor owns its filesystem authority.
    External,
}

impl FilesystemAuthority {
    /// Stable label for diagnostics and audit metadata.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::HostProcess => "host-process",
            Self::External => "external",
        }
    }
}

/// Network authority relevant to an execution boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkAuthority {
    /// The executor cannot use the network.
    Denied,
    /// The executor can use the network allowed by its sandbox policy.
    Allowed,
    /// The executor inherits the `thndrs` process network authority.
    HostProcess,
    /// An external executor owns its network authority.
    External,
}

impl NetworkAuthority {
    /// Stable label for diagnostics and audit metadata.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Allowed => "allowed",
            Self::HostProcess => "host-process",
            Self::External => "external",
        }
    }
}

/// Whether an executor is isolated by a backend that `thndrs` can verify.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Isolation {
    /// A backend enforces the stated filesystem and network authority.
    Enforced,
    /// The process inherits local host authority; no backend isolates it.
    None,
    /// An external system executes the work; `thndrs` cannot verify isolation.
    External,
}

impl Isolation {
    /// Stable label for diagnostics and audit metadata.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::None => "none",
            Self::External => "external-unverified",
        }
    }
}

/// Filesystem, network, and isolation facts for one execution surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionBoundary {
    /// Filesystem authority available to the executor.
    pub filesystem: FilesystemAuthority,
    /// Network authority available to the executor.
    pub network: NetworkAuthority,
    /// Isolation `thndrs` can verify for this executor.
    pub isolation: Isolation,
}

impl ExecutionBoundary {
    /// Render a compact, machine-readable boundary report.
    pub fn report(self) -> String {
        format!(
            "filesystem={} network={} isolation={}",
            self.filesystem.label(),
            self.network.label(),
            self.isolation.label()
        )
    }
}

/// Execution surfaces whose authority `thndrs` reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionSurface {
    /// The built-in `run_shell` tool.
    BuiltinShell,
    /// A terminal process `thndrs` starts for an ACP agent callback.
    AcpTerminalCallback,
    /// A terminal process an ACP client starts for the `thndrs` ACP server.
    AcpClientTerminal,
    /// A local stdio MCP server child process.
    McpStdioServer,
    /// A remote streamable HTTP MCP server.
    McpStreamableHttpServer,
}

impl ExecutionSurface {
    /// Return the boundary currently used by this surface.
    pub const fn boundary(self) -> ExecutionBoundary {
        match self {
            Self::BuiltinShell | Self::McpStdioServer => ExecutionBoundary {
                filesystem: FilesystemAuthority::HostProcess,
                network: NetworkAuthority::HostProcess,
                isolation: Isolation::None,
            },
            Self::AcpTerminalCallback => ExecutionBoundary {
                filesystem: FilesystemAuthority::HostProcess,
                network: NetworkAuthority::HostProcess,
                isolation: Isolation::None,
            },
            Self::AcpClientTerminal => ExecutionBoundary {
                filesystem: FilesystemAuthority::External,
                network: NetworkAuthority::External,
                isolation: Isolation::External,
            },
            Self::McpStreamableHttpServer => ExecutionBoundary {
                filesystem: FilesystemAuthority::External,
                network: NetworkAuthority::HostProcess,
                isolation: Isolation::External,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_labels_distinguish_filesystem_and_network_policy() {
        assert_eq!(FilesystemAuthority::ReadOnly.label(), "read-only");
        assert_eq!(FilesystemAuthority::WorkspaceWrite.label(), "workspace-write");
        assert_eq!(NetworkAuthority::Denied.label(), "denied");
        assert_eq!(NetworkAuthority::Allowed.label(), "allowed");
    }

    #[test]
    fn local_and_external_surfaces_do_not_claim_unavailable_isolation() {
        assert_eq!(
            ExecutionSurface::BuiltinShell.boundary().report(),
            "filesystem=host-process network=host-process isolation=none"
        );
        assert_eq!(
            ExecutionSurface::AcpClientTerminal.boundary().report(),
            "filesystem=external network=external isolation=external-unverified"
        );
    }
}
