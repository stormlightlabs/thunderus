//! Doctor report data shape used for human and JSON output.

use serde::{Deserialize, Serialize};

/// Redacted doctor report for `thndrs doctor`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Running version of `thndrs` that produced this report.
    pub app_version: String,
    /// Workspace root chosen for resolution.
    pub workspace: String,
    /// Effective model configured for this CLI invocation.
    pub model: String,
    /// Resolved provider for the selected model.
    pub provider: String,
    /// Loaded config files and redacted metadata.
    pub config_files: Vec<DoctorConfigFile>,
    /// Config-load diagnostics from resolved config files.
    pub config_diagnostics: Vec<String>,
    /// Provider credential source for known providers.
    pub credentials: Vec<DoctorCredential>,
    /// External dependency availability.
    pub tools: DoctorToolAvailability,
    /// Session directory writeability.
    pub session: DoctorSession,
    /// MCP server load summary.
    pub mcp: DoctorMcpStatus,
    /// ACP agent load summary.
    pub acp: DoctorAcpStatus,
    /// Terminal capability summary if available.
    pub terminal: DoctorTerminalSummary,
    /// Blocking setup items found by this run.
    pub blocking_issues: Vec<String>,
    /// Friendly docs URL for follow-up instructions.
    pub docs_url: String,
    /// Optional setup hint for blocking issues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_hint: Option<String>,
}

/// One redacted config source loaded for doctor output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorConfigFile {
    /// Config source name (for example `global` or `project`).
    pub source: String,
    /// Safe display path for this config source.
    pub path: String,
    /// SHA-256 of file bytes when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Credential source status by provider name.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorCredential {
    /// Provider key name used by docs and setup guides.
    pub provider: String,
    /// Human-readable source label from `auth::CredentialSource`, or `missing`.
    pub source: String,
}

/// `rg` and `fd` availability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorToolAvailability {
    /// `rg` binary availability.
    pub rg: bool,
    /// `fd` binary availability.
    pub fd: bool,
}

/// Session directory status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorSession {
    /// Target session directory.
    pub path: String,
    /// Whether the directory is currently writable for diagnostics.
    pub writable: bool,
    /// Why the path is not writable when `writable` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// MCP server counts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorMcpStatus {
    /// Servers present in effective config and loaded after expansion.
    pub configured: usize,
    /// Servers currently enabled.
    pub ready: usize,
    /// Servers skipped because environment values were missing.
    pub skipped: usize,
    /// Servers skipped due to config file errors.
    pub failed: usize,
    /// MCP config load diagnostics.
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

/// ACP agent counts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorAcpStatus {
    /// ACP agents configured in effective config.
    pub configured: usize,
    /// Enabled ACP agents.
    pub enabled: usize,
    /// Disabled ACP agents.
    pub disabled: usize,
}

/// Terminal summary summary for setup diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DoctorTerminalSummary {
    /// Is stdout connected to a terminal.
    pub tty: bool,
    /// Terminal width in columns when known.
    pub width: u16,
    /// Terminal height in rows when known.
    pub height: u16,
    /// `$TERM` value if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_env: Option<String>,
    /// Whether `$NO_COLOR` disabled terminal coloring.
    pub no_color: bool,
}
