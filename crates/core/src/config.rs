use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::Result;

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
            _ => Err(crate::Error::Config(
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
    /// OS-level sandboxing (future: containers, namespaces)
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
            _ => Err(crate::Error::Config(
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

    /// Additional configuration options
    #[serde(default)]
    pub options: HashMap<String, String>,
}

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
        let config: Config =
            toml::from_str(toml_str).map_err(|e| crate::Error::Config(format!("TOML parse error: {}", e)))?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from a file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Get the default profile
    pub fn default_profile(&self) -> Result<&Profile> {
        use crate::Error;

        self.profiles
            .get(&self.default_profile)
            .ok_or_else(|| Error::Config(ConfigError::ProfileNotFound(self.default_profile.clone()).to_string()))
    }

    /// Get a profile by name
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        use crate::Error;

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
        use crate::Error;

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
            approval_mode: ApprovalMode::default(),
            sandbox_mode: SandboxMode::default(),
            provider: ProviderConfig::Glm {
                api_key: "test-key".to_string(),
                model: "glm-4.7".to_string(),
                base_url: default_glm_base_url(),
            },
            allow_network: false,
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
            approval_mode: ApprovalMode::default(),
            sandbox_mode: SandboxMode::default(),
            provider: ProviderConfig::Glm {
                api_key: "test-key".to_string(),
                model: "glm-4.7".to_string(),
                base_url: default_glm_base_url(),
            },
            allow_network: false,
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
}
