use crate::{Error, Result};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Approval modes for the agent (Codex-like ergonomics)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    /// Consultative; no edits, no commands
    ReadOnly,
    /// Workspace edits + safe commands; gates risky ops (default)
    #[default]
    Auto,
    /// Still logged; you opt in explicitly
    FullAccess,
}

impl ApprovalMode {
    pub const VALUES: &[ApprovalMode] = &[ApprovalMode::ReadOnly, ApprovalMode::Auto, ApprovalMode::FullAccess];

    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalMode::ReadOnly => "read-only",
            ApprovalMode::Auto => "auto",
            ApprovalMode::FullAccess => "full-access",
        }
    }
}

impl std::fmt::Display for ApprovalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ApprovalMode {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read-only" => Ok(ApprovalMode::ReadOnly),
            "auto" => Ok(ApprovalMode::Auto),
            "full-access" => Ok(ApprovalMode::FullAccess),
            _ => Err(Error::Config(
                ConfigError::InvalidApprovalMode(s.to_string()).to_string(),
            )),
        }
    }
}

/// Sandbox modes for the agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// Policy-based sandboxing (enforce workspace roots) (default)
    #[default]
    Policy,
    /// OS-level sandboxing
    /// TODO: containers, namespaces
    Os,
    /// No sandboxing (dangerous, explicit opt-in)
    None,
}

impl SandboxMode {
    pub const VALUES: &[SandboxMode] = &[SandboxMode::Policy, SandboxMode::Os, SandboxMode::None];

    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxMode::Policy => "policy",
            SandboxMode::Os => "os",
            SandboxMode::None => "none",
        }
    }
}

impl std::fmt::Display for SandboxMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SandboxMode {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "policy" => Ok(SandboxMode::Policy),
            "os" => Ok(SandboxMode::Os),
            "none" => Ok(SandboxMode::None),
            _ => Err(Error::Config(
                ConfigError::InvalidSandboxMode(s.to_string()).to_string(),
            )),
        }
    }
}

/// Provider-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum ProviderConfig {
    /// GLM-4.7 provider configuration
    #[serde(rename = "glm")]
    Glm {
        /// API key for authentication
        api_key: String,
        /// Model name (e.g., "glm-4.7")
        model: String,
        /// Base URL for the API
        #[serde(default = "default_glm_base_url")]
        base_url: String,
    },
    /// Gemini provider configuration
    #[serde(rename = "gemini")]
    Gemini {
        /// API key for authentication
        api_key: String,
        /// Model name (e.g., "gemini-2.5-flash")
        model: String,
        /// Base URL for the API
        #[serde(default = "default_gemini_base_url")]
        base_url: String,
    },
}

// FIXME: Correct this/make configurable
fn default_glm_base_url() -> String {
    "https://open.bigmodel.cn/api/paas/v4".to_string()
}

fn default_gemini_base_url() -> String {
    "https://generativelanguage.googleapis.com/v1beta".to_string()
}

/// Profile configuration (Codex-like ergonomics)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Profile name
    pub name: String,

    /// Working root directory (absolute path)
    pub working_root: PathBuf,

    /// Extra writable roots (beyond working_root)
    #[serde(default)]
    pub extra_writable_roots: Vec<PathBuf>,

    /// Workspace sandbox configuration
    #[serde(default)]
    pub workspace: WorkspaceConfig,

    /// Approval mode
    #[serde(default)]
    pub approval_mode: ApprovalMode,

    /// Sandbox mode
    #[serde(default)]
    pub sandbox_mode: SandboxMode,

    /// Provider and model selection
    pub provider: ProviderConfig,

    /// Allow network commands (default: false)
    #[serde(default)]
    pub allow_network: bool,

    /// Network sandbox configuration
    #[serde(default)]
    pub network: NetworkConfig,

    /// Memory configuration including vector search settings
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Additional configuration options
    #[serde(default)]
    pub options: HashMap<String, String>,
}

/// Workspace sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Workspace root directories
    #[serde(default)]
    pub roots: Vec<PathBuf>,

    /// Include /tmp for scratch files
    #[serde(default)]
    pub include_temp: bool,

    /// Explicitly allowed paths (beyond workspace roots)
    #[serde(default)]
    pub allow: Vec<PathBuf>,

    /// Explicitly denied paths (always blocked)
    #[serde(default)]
    pub deny: Vec<PathBuf>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self { roots: Vec::new(), include_temp: true, allow: Vec::new(), deny: Vec::new() }
    }
}

/// Network sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkConfig {
    /// Enable network access (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Allowed domains for web operations
    #[serde(default)]
    pub allow_domains: Vec<String>,
}

/// Memory configuration including vector search settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MemoryConfig {
    /// Enable vector search (default: false - lexical-only)
    #[serde(default)]
    pub enable_vector_search: bool,

    /// Vector embedding model name (e.g., "all-MiniLM-L6-v2")
    #[serde(default)]
    pub vector_model: String,

    /// Vector embedding dimensions
    #[serde(default)]
    pub vector_dims: usize,

    /// Minimum BM25 score threshold for triggering vector fallback
    /// Lower = more likely to use vector search. Default -3.0.
    #[serde(default = "default_vector_threshold")]
    pub vector_fallback_threshold: f64,
}

impl MemoryConfig {
    /// Get default vector model
    pub fn default_vector_model() -> String {
        "all-MiniLM-L6-v2".to_string()
    }

    /// Get default vector dimensions
    pub fn default_vector_dims() -> usize {
        384
    }
}

fn default_vector_threshold() -> f64 {
    -3.0
}

impl WorkspaceConfig {
    /// Get all workspace roots (including /tmp if include_temp is true)
    pub fn all_roots(&self) -> Vec<PathBuf> {
        let mut roots = self.roots.clone();
        if self.include_temp {
            roots.push(PathBuf::from("/tmp"));
        }
        roots
    }

    /// Check if a path is in workspace roots or explicitly allowed
    pub fn is_allowed(&self, path: &Path) -> bool {
        for denied in &self.deny {
            if path.starts_with(denied) {
                return false;
            }
        }

        for allowed in &self.allow {
            if path.starts_with(allowed) {
                return true;
            }
        }

        for root in self.all_roots() {
            if path.starts_with(&root) {
                return true;
            }
        }

        false
    }

    /// Check if a path is explicitly denied
    pub fn is_denied(&self, path: &Path) -> bool {
        self.deny.iter().any(|denied| path.starts_with(denied))
    }
}

/// Sensitive directories that should always be denied
pub const SENSITIVE_DIRS: &[&str] = &[
    "~/.ssh", "~/.gnupg", "~/.aws", "~/.kube", "/etc", "/usr", "/bin", "/sbin", "/var", "/sys", "/proc", "/boot",
    "/root",
];

impl Profile {
    /// Get all writable roots (working_root + extra_writable_roots)
    pub fn writable_roots(&self) -> Vec<&PathBuf> {
        let mut roots = vec![&self.working_root];
        roots.extend(self.extra_writable_roots.iter());
        roots
    }

    /// Check if a path is within writable roots
    pub fn is_writable(&self, path: &Path) -> bool {
        self.writable_roots().iter().any(|root| path.starts_with(root))
    }

    /// Check if network access is allowed
    pub fn is_network_allowed(&self) -> bool {
        self.allow_network || self.network.enabled
    }

    /// Check if a domain is in the allow list
    pub fn is_domain_allowed(&self, domain: &str) -> bool {
        if self.network.enabled {
            return self.network.allow_domains.iter().any(|d| domain.ends_with(d));
        }
        false
    }

    /// Check if a path is a sensitive directory
    pub fn is_sensitive_dir(path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        let expanded_path = if path_str.starts_with("~/") {
            match std::env::var("HOME") {
                Ok(home) => path_str.replace("~", &home),
                Err(_) => path_str.to_string(),
            }
        } else {
            path_str.to_string()
        };

        for sensitive in SENSITIVE_DIRS {
            let expanded_sensitive = if sensitive.starts_with("~/") {
                match std::env::var("HOME") {
                    Ok(home) => sensitive.replace("~", &home),
                    Err(_) => sensitive.to_string(),
                }
            } else {
                sensitive.to_string()
            };

            if expanded_path.starts_with(&expanded_sensitive) {
                return true;
            }
        }

        false
    }

    /// Check path access based on approval mode and workspace config
    pub fn check_path_access(&self, path: &Path, mode: ApprovalMode) -> PathAccessResult {
        if Self::is_sensitive_dir(path) {
            return PathAccessResult::Denied("Sensitive directory");
        }

        if self.workspace.is_denied(path) {
            return PathAccessResult::Denied("Path in deny list");
        }

        match mode {
            ApprovalMode::ReadOnly => match self.workspace.is_allowed(path) {
                true => PathAccessResult::ReadOnly,
                false => PathAccessResult::Denied("Outside workspace in read-only mode"),
            },
            ApprovalMode::Auto => match self.workspace.is_allowed(path) || self.is_writable(path) {
                true => PathAccessResult::Allowed,
                false => PathAccessResult::NeedsApproval("Outside workspace"),
            },
            ApprovalMode::FullAccess => match self.workspace.is_denied(path) {
                true => PathAccessResult::Denied("Path in deny list"),
                false => PathAccessResult::Allowed,
            },
        }
    }
}

/// Result of checking path access
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAccessResult {
    /// Access is allowed
    Allowed,
    /// Read-only access allowed
    ReadOnly,
    /// Access denied with reason
    Denied(&'static str),
    /// Needs approval with reason
    NeedsApproval(&'static str),
}

/// Root configuration structure for config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Default profile name
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Named profiles
    pub profiles: HashMap<String, Profile>,
}

fn default_profile() -> String {
    "default".to_string()
}

impl Config {
    /// Load configuration from a TOML string
    pub fn from_toml_str(toml_str: &str) -> Result<Self> {
        let config: Config = toml::from_str(toml_str).map_err(|e| Error::Config(format!("TOML parse error: {}", e)))?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from a file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Save configuration to a file as TOML
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let toml_str = toml::to_string_pretty(self).map_err(|e| Error::Config(format!("TOML encode error: {}", e)))?;
        std::fs::write(path, toml_str).map_err(|e| Error::Config(format!("Failed to write config: {}", e)))?;
        Ok(())
    }

    /// Get the default profile
    pub fn default_profile(&self) -> Result<&Profile> {
        self.profiles
            .get(&self.default_profile)
            .ok_or_else(|| Error::Config(ConfigError::ProfileNotFound(self.default_profile.clone()).to_string()))
    }

    /// Get a profile by name
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profiles
            .get(name)
            .ok_or_else(|| Error::Config(ConfigError::ProfileNotFound(name.to_string()).to_string()))
    }

    /// Get all profile names
    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        if !self.profiles.contains_key(&self.default_profile) {
            return Err(Error::Config(
                ConfigError::ProfileNotFound(self.default_profile.clone()).to_string(),
            ));
        }

        for (name, profile) in &self.profiles {
            if !profile.working_root.is_absolute() {
                return Err(Error::Config(
                    ConfigError::AbsolutePathRequired(format!("working_root for profile '{}'", name)).to_string(),
                ));
            }

            for root in &profile.extra_writable_roots {
                if !root.is_absolute() {
                    return Err(Error::Config(
                        ConfigError::AbsolutePathRequired(format!("extra_writable_root in profile '{}'", name))
                            .to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get example configuration (as a string)
    /// Note: This returns a static example string, not from a file
    pub fn example() -> &'static str {
        r#"# Thunderus Configuration Example
# Copy this file to config.toml and customize as needed

# Default profile to use when no profile is specified
default_profile = "default"

# Named profiles
[profiles.default]
name = "default"
# Working root directory (must be absolute path)
working_root = "/path/to/workspace"
# Extra writable directories beyond working_root (optional, must be absolute paths)
extra_writable_roots = []
# Approval mode: "read-only", "auto", or "full-access"
approval_mode = "auto"
# Sandbox mode: "policy", "os", or "none"
sandbox_mode = "policy"
# Allow network commands (default: false)
allow_network = false

# Provider configuration
[profiles.default.provider]
# Provider type: "glm" or "gemini"
provider = "glm"
# API key for the provider
api_key = "your-api-key-here"
# Model name to use
model = "glm-4.7"
# Base URL (optional, defaults to provider's default)
# base_url = "https://custom.api.url"

# Additional options (optional)
# [profiles.default.options]
# max_tokens = "8192"
# temperature = "0.7"
"#
    }
}

impl Default for Config {
    fn default() -> Self {
        Config { default_profile: default_profile(), profiles: HashMap::new() }
    }
}

/// Configuration-specific errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Profile not found
    #[error("profile not found: {0}")]
    ProfileNotFound(String),

    /// Invalid approval mode
    #[error("invalid approval mode: {0}")]
    InvalidApprovalMode(String),

    /// Invalid sandbox mode
    #[error("invalid sandbox mode: {0}")]
    InvalidSandboxMode(String),

    /// Absolute path required
    #[error("absolute path required: {0}")]
    AbsolutePathRequired(String),

    /// TOML parse error
    #[error("TOML parse error: {0}")]
    TomlParse(String),
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        ConfigError::TomlParse(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_approval_mode_values() {
        assert_eq!(ApprovalMode::ReadOnly.as_str(), "read-only");
        assert_eq!(ApprovalMode::Auto.as_str(), "auto");
        assert_eq!(ApprovalMode::FullAccess.as_str(), "full-access");
    }

    #[test]
    fn test_approval_mode_from_str() {
        assert_eq!(ApprovalMode::from_str("read-only").unwrap(), ApprovalMode::ReadOnly);
        assert_eq!(ApprovalMode::from_str("AUTO").unwrap(), ApprovalMode::Auto);
        assert_eq!(ApprovalMode::from_str("Full-Access").unwrap(), ApprovalMode::FullAccess);
        assert!(ApprovalMode::from_str("invalid").is_err());
    }

    #[test]
    fn test_approval_mode_default() {
        assert_eq!(ApprovalMode::default(), ApprovalMode::Auto);
    }

    #[test]
    fn test_sandbox_mode_values() {
        assert_eq!(SandboxMode::Policy.as_str(), "policy");
        assert_eq!(SandboxMode::Os.as_str(), "os");
        assert_eq!(SandboxMode::None.as_str(), "none");
    }

    #[test]
    fn test_sandbox_mode_from_str() {
        assert_eq!(SandboxMode::from_str("policy").unwrap(), SandboxMode::Policy);
        assert_eq!(SandboxMode::from_str("OS").unwrap(), SandboxMode::Os);
        assert_eq!(SandboxMode::from_str("None").unwrap(), SandboxMode::None);
        assert!(SandboxMode::from_str("invalid").is_err());
    }

    #[test]
    fn test_sandbox_mode_default() {
        assert_eq!(SandboxMode::default(), SandboxMode::Policy);
    }

    #[test]
    fn test_profile_writable_roots() {
        let profile = Profile {
            name: "test".to_string(),
            working_root: PathBuf::from("/workspace"),
            extra_writable_roots: vec![PathBuf::from("/data"), PathBuf::from("/cache")],
            workspace: WorkspaceConfig::default(),
            approval_mode: ApprovalMode::default(),
            sandbox_mode: SandboxMode::default(),
            provider: ProviderConfig::Glm {
                api_key: "test-key".to_string(),
                model: "glm-4.7".to_string(),
                base_url: default_glm_base_url(),
            },
            allow_network: false,
            network: NetworkConfig::default(),
            memory: MemoryConfig::default(),
            options: HashMap::new(),
        };

        let roots = profile.writable_roots();
        assert_eq!(roots.len(), 3);
        assert!(roots.contains(&&PathBuf::from("/workspace")));
        assert!(roots.contains(&&PathBuf::from("/data")));
        assert!(roots.contains(&&PathBuf::from("/cache")));
    }

    #[test]
    fn test_profile_is_writable() {
        let profile = Profile {
            name: "test".to_string(),
            working_root: PathBuf::from("/workspace"),
            extra_writable_roots: vec![PathBuf::from("/data")],
            workspace: WorkspaceConfig::default(),
            approval_mode: ApprovalMode::default(),
            sandbox_mode: SandboxMode::default(),
            provider: ProviderConfig::Glm {
                api_key: "test-key".to_string(),
                model: "glm-4.7".to_string(),
                base_url: default_glm_base_url(),
            },
            allow_network: false,
            network: NetworkConfig::default(),
            memory: MemoryConfig::default(),
            options: HashMap::new(),
        };

        assert!(profile.is_writable(&PathBuf::from("/workspace/file.txt")));
        assert!(profile.is_writable(&PathBuf::from("/data/file.txt")));
        assert!(!profile.is_writable(&PathBuf::from("/etc/passwd")));
    }

    #[test]
    fn test_config_from_toml_str() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"
allow_network = false

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        assert_eq!(config.default_profile, "default");
        assert_eq!(config.profiles.len(), 1);

        let profile = config.default_profile().unwrap();
        assert_eq!(profile.name, "default");
        assert_eq!(profile.working_root, PathBuf::from("/workspace"));
        assert_eq!(profile.approval_mode, ApprovalMode::Auto);
        assert_eq!(profile.sandbox_mode, SandboxMode::Policy);
        assert!(!profile.allow_network);
    }

    #[test]
    fn test_config_from_toml_str_with_multiple_profiles() {
        let toml = r#"
default_profile = "work"

[profiles.work]
name = "work"
working_root = "/home/user/work"
approval_mode = "auto"
sandbox_mode = "policy"
allow_network = false

[profiles.work.provider]
provider = "glm"
api_key = "work-api-key"
model = "glm-4.7"

[profiles.personal]
name = "personal"
working_root = "/home/user/personal"
approval_mode = "full-access"
sandbox_mode = "policy"
allow_network = true

[profiles.personal.provider]
provider = "gemini"
api_key = "personal-api-key"
model = "gemini-2.5-flash"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        assert_eq!(config.default_profile, "work");
        assert_eq!(config.profiles.len(), 2);
        assert!(config.profiles.contains_key("work"));
        assert!(config.profiles.contains_key("personal"));

        let work = config.profile("work").unwrap();
        assert_eq!(work.approval_mode, ApprovalMode::Auto);
        assert!(!work.allow_network);

        let personal = config.profile("personal").unwrap();
        assert_eq!(personal.approval_mode, ApprovalMode::FullAccess);
        assert!(personal.allow_network);
    }

    #[test]
    fn test_config_from_toml_str_with_extra_writable_roots() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
extra_writable_roots = ["/data", "/cache"]
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let profile = config.default_profile().unwrap();
        assert_eq!(profile.extra_writable_roots.len(), 2);
        assert!(profile.extra_writable_roots.contains(&PathBuf::from("/data")));
        assert!(profile.extra_writable_roots.contains(&PathBuf::from("/cache")));
    }

    #[test]
    fn test_config_from_toml_str_with_options() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.options]
max_tokens = "8192"
temperature = "0.7"

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let profile = config.default_profile().unwrap();
        assert_eq!(profile.options.get("max_tokens"), Some(&"8192".to_string()));
        assert_eq!(profile.options.get("temperature"), Some(&"0.7".to_string()));
    }

    #[test]
    fn test_config_validation_missing_default_profile() {
        let toml = r#"
default_profile = "nonexistent"

[profiles.other]
name = "other"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.other.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let result = Config::from_toml_str(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("profile not found"));
    }

    #[test]
    fn test_config_validation_relative_working_root() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "workspace"  # Not absolute!
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let result = Config::from_toml_str(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute path required"));
    }

    #[test]
    fn test_config_validation_relative_extra_root() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
extra_writable_roots = ["data"]  # Not absolute!
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let result = Config::from_toml_str(toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute path required"));
    }

    #[test]
    fn test_config_provider_glm() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "glm"
api_key = "glm-api-key"
model = "glm-4.7"
base_url = "https://custom.api.com/v4"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let profile = config.default_profile().unwrap();

        match &profile.provider {
            ProviderConfig::Glm { api_key, model, base_url } => {
                assert_eq!(api_key, "glm-api-key");
                assert_eq!(model, "glm-4.7");
                assert_eq!(base_url, "https://custom.api.com/v4");
            }
            _ => panic!("Expected GLM provider"),
        }
    }

    #[test]
    fn test_config_provider_gemini() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "gemini"
api_key = "gemini-api-key"
model = "gemini-2.5-flash"
base_url = "https://custom.api.com/v1beta"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let profile = config.default_profile().unwrap();

        match &profile.provider {
            ProviderConfig::Gemini { api_key, model, base_url } => {
                assert_eq!(api_key, "gemini-api-key");
                assert_eq!(model, "gemini-2.5-flash");
                assert_eq!(base_url, "https://custom.api.com/v1beta");
            }
            _ => panic!("Expected Gemini provider"),
        }
    }

    #[test]
    fn test_config_provider_default_base_url() {
        let toml = r#"
default_profile = "default"

[profiles.default]
name = "default"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.default.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let profile = config.default_profile().unwrap();

        match &profile.provider {
            ProviderConfig::Glm { base_url, .. } => {
                assert_eq!(base_url, &default_glm_base_url());
            }
            _ => panic!("Expected GLM provider"),
        }
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.default_profile, "default");
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn test_config_profile_names() {
        let toml = r#"
default_profile = "work"

[profiles.work]
name = "work"
working_root = "/workspace"
approval_mode = "auto"
sandbox_mode = "policy"

[profiles.work.provider]
provider = "glm"
api_key = "test-api-key"
model = "glm-4.7"

[profiles.personal]
name = "personal"
working_root = "/personal"
approval_mode = "full-access"
sandbox_mode = "policy"

[profiles.personal.provider]
provider = "gemini"
api_key = "test-api-key"
model = "gemini-2.5-flash"
"#;

        let config = Config::from_toml_str(toml).unwrap();
        let names = config.profile_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"work".to_string()));
        assert!(names.contains(&"personal".to_string()));
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::ProfileNotFound("missing".to_string());
        assert_eq!(err.to_string(), "profile not found: missing");

        let err = ConfigError::InvalidApprovalMode("bad-mode".to_string());
        assert_eq!(err.to_string(), "invalid approval mode: bad-mode");

        let err = ConfigError::InvalidSandboxMode("bad-mode".to_string());
        assert_eq!(err.to_string(), "invalid sandbox mode: bad-mode");

        let err = ConfigError::AbsolutePathRequired("working_root".to_string());
        assert_eq!(err.to_string(), "absolute path required: working_root");

        let err = ConfigError::TomlParse("parse error".to_string());
        assert_eq!(err.to_string(), "TOML parse error: parse error");
    }

    #[test]
    fn test_sensitive_dirs() {
        assert!(SENSITIVE_DIRS.contains(&"~/.ssh"));
        assert!(SENSITIVE_DIRS.contains(&"/etc"));
        assert!(SENSITIVE_DIRS.contains(&"/usr"));
    }

    #[test]
    fn test_workspace_config_default() {
        let config = WorkspaceConfig::default();
        assert!(config.roots.is_empty());
        assert!(config.include_temp);
        assert!(config.allow.is_empty());
        assert!(config.deny.is_empty());
    }

    #[test]
    fn test_workspace_config_all_roots() {
        let mut config = WorkspaceConfig::default();
        config.roots.push(PathBuf::from("/workspace"));
        config.include_temp = false;

        let roots = config.all_roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], PathBuf::from("/workspace"));
    }

    #[test]
    fn test_workspace_config_all_roots_with_temp() {
        let mut config = WorkspaceConfig::default();
        config.roots.push(PathBuf::from("/workspace"));

        let roots = config.all_roots();
        assert_eq!(roots.len(), 2);
        assert!(roots.contains(&PathBuf::from("/workspace")));
        assert!(roots.contains(&PathBuf::from("/tmp")));
    }

    #[test]
    fn test_workspace_config_is_allowed() {
        let mut config = WorkspaceConfig::default();
        config.roots.push(PathBuf::from("/workspace"));
        config.allow.push(PathBuf::from("/opt/app"));

        assert!(config.is_allowed(Path::new("/workspace/src/main.rs")));
        assert!(config.is_allowed(Path::new("/tmp/test.txt")));
        assert!(config.is_allowed(Path::new("/opt/app/config")));
        assert!(!config.is_allowed(Path::new("/etc/passwd")));
    }

    #[test]
    fn test_workspace_config_is_denied() {
        let mut config = WorkspaceConfig::default();
        config.deny.push(PathBuf::from("/secrets"));

        assert!(!config.is_denied(Path::new("/workspace/file.txt")));
        assert!(config.is_denied(Path::new("/secrets/key.txt")));
    }

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert!(!config.enabled);
        assert!(config.allow_domains.is_empty());
    }

    #[test]
    fn test_profile_is_sensitive_dir() {
        assert!(Profile::is_sensitive_dir(Path::new("/etc/passwd")));
        assert!(Profile::is_sensitive_dir(Path::new("/usr/bin")));
        assert!(!Profile::is_sensitive_dir(Path::new("/workspace/file.txt")));
    }

    #[test]
    fn test_profile_check_path_access_readonly() {
        let profile = create_test_profile_with_workspace("/workspace");
        let result = profile.check_path_access(Path::new("/workspace/file.txt"), ApprovalMode::ReadOnly);
        assert!(matches!(result, PathAccessResult::ReadOnly));
    }

    #[test]
    fn test_profile_check_path_access_auto() {
        let profile = create_test_profile_with_workspace("/workspace");
        let result = profile.check_path_access(Path::new("/workspace/file.txt"), ApprovalMode::Auto);
        assert!(matches!(result, PathAccessResult::Allowed));
        let result = profile.check_path_access(Path::new("/etc/passwd"), ApprovalMode::Auto);
        assert!(matches!(result, PathAccessResult::Denied(_)));
    }

    #[test]
    fn test_profile_check_path_access_full_access() {
        let profile = create_test_profile_with_workspace("/workspace");
        let result = profile.check_path_access(Path::new("/workspace/file.txt"), ApprovalMode::FullAccess);
        assert!(matches!(result, PathAccessResult::Allowed));
    }

    #[test]
    fn test_profile_check_path_access_sensitive_dir() {
        let profile = create_test_profile_with_workspace("/workspace");
        let result = profile.check_path_access(Path::new("/etc/passwd"), ApprovalMode::FullAccess);
        assert!(matches!(result, PathAccessResult::Denied(_)));
    }

    #[test]
    fn test_profile_check_path_access_deny_list() {
        let mut profile = create_test_profile_with_workspace("/workspace");
        profile.workspace.deny.push(PathBuf::from("/blocked"));

        let result = profile.check_path_access(Path::new("/blocked/file.txt"), ApprovalMode::Auto);
        assert!(matches!(result, PathAccessResult::Denied(_)));
    }

    #[test]
    fn test_profile_is_network_allowed() {
        let mut profile = create_test_profile_with_workspace("/workspace");

        assert!(!profile.is_network_allowed());

        profile.allow_network = true;
        assert!(profile.is_network_allowed());

        profile.allow_network = false;
        profile.network.enabled = true;
        assert!(profile.is_network_allowed());
    }

    #[test]
    fn test_profile_is_domain_allowed() {
        let mut profile = create_test_profile_with_workspace("/workspace");
        profile.network.enabled = true;
        profile.network.allow_domains = vec!["crates.io".to_string(), "github.com".to_string()];

        assert!(profile.is_domain_allowed("crates.io"));
        assert!(profile.is_domain_allowed("api.crates.io"));
        assert!(profile.is_domain_allowed("github.com"));
        assert!(!profile.is_domain_allowed("example.com"));
    }

    fn create_test_profile_with_workspace(root: &str) -> Profile {
        let mut workspace = WorkspaceConfig::default();
        workspace.roots.push(PathBuf::from(root));

        Profile {
            name: "test".to_string(),
            working_root: PathBuf::from(root),
            extra_writable_roots: vec![],
            workspace,
            approval_mode: ApprovalMode::Auto,
            sandbox_mode: SandboxMode::Policy,
            provider: ProviderConfig::Glm {
                api_key: "test-key".to_string(),
                model: "glm-4.7".to_string(),
                base_url: default_glm_base_url(),
            },
            allow_network: false,
            network: NetworkConfig::default(),
            memory: MemoryConfig::default(),
            options: HashMap::new(),
        }
    }
}
