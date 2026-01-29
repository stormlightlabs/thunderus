//! Core types for the Skills system.
//!
//! Skills are on-demand capabilities loaded from `.thunderus/skills/` directories.
//! Each skill has a SKILL.md file with frontmatter metadata and implementation scripts.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Driver type determines execution strategy for a skill.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillDriver {
    /// Default: spawn subprocess (current behavior)
    #[default]
    Shell,
    /// WASM plugin via Extism (future)
    Wasm,
    /// Lua script via mlua (future)
    Lua,
    /// Delegate to MCP server (future)
    Mcp,
}

/// Filesystem access permissions for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FilesystemPermissions {
    /// Glob patterns for readable paths (relative to workspace)
    #[serde(default)]
    pub read: Vec<String>,
    /// Glob patterns for writable paths (relative to workspace)
    #[serde(default)]
    pub write: Vec<String>,
}

/// Network access permissions for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NetworkPermissions {
    /// Allowed host patterns (supports wildcards)
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

/// Capability-based security permissions for a skill.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkillPermissions {
    /// Filesystem access permissions
    #[serde(default)]
    pub filesystem: FilesystemPermissions,
    /// Network access permissions
    #[serde(default)]
    pub network: NetworkPermissions,
    /// Environment variables the skill can read
    #[serde(default)]
    pub env_vars: Vec<String>,
    /// Memory limit in MB (WASM only, future)
    pub memory_limit_mb: Option<u32>,
    /// CPU instruction limit (WASM only, future)
    pub instruction_limit: Option<u64>,
}

/// A function exported by a plugin (for multi-function skills).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginFunction {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// JSON Schema for parameters
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Metadata about a skill, extracted from SKILL.md frontmatter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillMeta {
    /// Unique identifier, lowercase with hyphens
    pub name: String,

    /// Agent reads to decide when to load (max 1024 chars)
    pub description: String,

    /// Semantic version
    #[serde(default)]
    pub version: String,

    /// Maintainer
    #[serde(default)]
    pub author: String,

    /// For filtering/discovery
    #[serde(default)]
    pub tags: Vec<String>,

    /// Environment variables or dependencies required
    #[serde(default)]
    pub requires: Vec<String>,

    /// Path to the skill directory
    #[serde(skip)]
    pub path: PathBuf,

    /// Risk level for approval gating
    #[serde(default)]
    pub risk_level: SkillRisk,

    /// Execution driver (shell, wasm, lua, mcp)
    #[serde(default)]
    pub driver: SkillDriver,

    /// Entry point file (e.g., plugin.wasm, script.lua, run.sh)
    #[serde(default)]
    pub entry: String,

    /// For driver: mcp - MCP server name
    #[serde(default)]
    pub mcp_server: String,

    /// For driver: mcp - MCP tool name
    #[serde(default)]
    pub mcp_tool: String,

    /// Explicit permission requirements (capability-based security)
    #[serde(default)]
    pub permissions: SkillPermissions,

    /// JSON Schema for tool parameters
    #[serde(default)]
    pub parameters: serde_json::Value,

    /// Multiple functions from single plugin
    #[serde(default)]
    pub functions: Vec<PluginFunction>,
}

/// Risk level determines approval requirements for skill execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SkillRisk {
    /// Read-only operations, no approval needed
    #[default]
    Safe,
    /// Modifies files or system state, needs approval
    Moderate,
    /// External network calls or destructive operations, needs explicit approval
    Risky,
}

/// A fully loaded skill with content ready for context injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Metadata about the skill
    #[serde(flatten)]
    pub meta: SkillMeta,

    /// Full SKILL.md content for context injection
    pub content: String,

    /// Implementation scripts found in the skill directory
    pub scripts: Vec<SkillScript>,
}

/// An executable script associated with a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScript {
    /// Name of the script (e.g., "search.sh")
    pub name: String,

    /// Path to the script
    pub path: PathBuf,

    /// Script language/type
    pub script_type: ScriptType,
}

/// The type of script based on file extension.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScriptType {
    Bash,
    JavaScript,
    Python,
    Lua,
    Unknown,
}

/// A matched skill from a query, with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    /// The matched skill
    pub skill: Skill,

    /// Relevance score (0.0 to 1.0)
    pub score: f64,

    /// Match reason (for debugging)
    pub reason: String,
}

/// Configuration for the skills system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SkillsConfig {
    /// Whether skills are enabled
    pub enabled: bool,

    /// Custom skills directory (defaults to .thunderus/skills)
    pub skills_dir: Option<PathBuf>,

    /// Enable auto-discovery based on task intent
    pub auto_discovery: bool,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self { enabled: true, skills_dir: None, auto_discovery: true }
    }
}

/// Errors that can occur when working with skills.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("Skill not found: {0}")]
    NotFound(String),

    #[error("Invalid SKILL.md frontmatter: {0}")]
    InvalidFrontmatter(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Required environment variable missing: {0}")]
    MissingEnvVar(String),

    #[error("Script execution failed: {0}")]
    ExecutionFailed(String),
}

/// Result type for skill operations.
pub type Result<T> = std::result::Result<T, SkillError>;

impl From<SkillError> for thunderus_core::Error {
    fn from(err: SkillError) -> Self {
        thunderus_core::Error::extension(err.to_string())
    }
}
