//! Stable, content-free metadata contracts for append-only coding-agent sessions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Effective configuration provenance persisted with a session.
///
/// The structure contains paths, hashes, origins, and diagnostics, never raw
/// configuration secrets or memory body text.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigMeta {
    /// Effective session directory used for append-only JSONL files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<String>,
    /// Whether optional memory/retrieval was active for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_enabled: Option<bool>,
    /// Loaded config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<SessionConfigFile>,
    /// Per-key origin labels, such as `"env:THNDRS_MODEL"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub origins: BTreeMap<String, String>,
    /// Non-fatal config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Loaded MCP config files with source, display path, and SHA-256 hash.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_files: Vec<SessionConfigFile>,
    /// Non-fatal MCP config diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_diagnostics: Vec<String>,
}

/// A loaded config file recorded in session metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionConfigFile {
    /// Display path: workspace-relative, `~`-relative, or absolute.
    pub path: String,
    /// Source label: `"global"` or `"project"`.
    pub source: String,
    /// Lowercase hex SHA-256 of file bytes.
    pub sha256: String,
}

/// MCP-specific identity attached to an external tool call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct McpToolSessionMeta {
    /// Configured MCP server name.
    pub server_name: String,
    /// Original MCP tool name before provider-visible namespacing.
    pub original_tool_name: String,
}

/// A permission option recorded without protocol payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpPermissionOptionRecord {
    /// ACP option id.
    pub id: String,
    /// Human-readable option label.
    pub name: String,
    /// Lowercase option kind.
    pub kind: String,
}

/// External-agent session identity persisted once per run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpSessionMetadata {
    /// Configured ACP agent name.
    pub agent_name: String,
    /// Opaque external ACP session id returned by the agent.
    pub acp_session_id: String,
    /// Redacted command display used to start the agent.
    pub command: String,
    /// Selected ACP protocol version.
    pub protocol_version: String,
    /// Optional ACP agent info name.
    pub agent_info_name: Option<String>,
    /// Optional ACP agent info version.
    pub agent_info_version: Option<String>,
    /// Optional ACP client info name.
    pub client_info_name: Option<String>,
    /// Optional ACP client info version.
    pub client_info_version: Option<String>,
}
