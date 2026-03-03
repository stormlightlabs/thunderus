//! Core types and configuration for Thunderus
//!
//! This crate provides:
//! - Configuration loading from TOML files
//! - Common error types
//! - Message and conversation types
//! - System prompt assembly

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

mod models;
mod prompt;
pub use models::{ModelPricing, STORED_MODELS, StoredModel, estimate_token_cost_usd, find_stored_model, stored_models};
pub use prompt::{ResponseSections, build_system_message, build_system_prompt};

/// Errors that can occur in core operations
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("Config serialize error: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Missing API key for provider: {0}")]
    MissingApiKey(String),
}

/// Result type for core operations
pub type Result<T> = std::result::Result<T, CoreError>;

/// Complete configuration for Thunderus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default provider to use
    #[serde(default = "default_provider")]
    pub default_provider: String,

    /// Default model ID
    #[serde(default)]
    pub default_model: Option<String>,

    /// Temperature for generation (0.0-1.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum tokens to generate
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Provider configurations
    #[serde(default)]
    pub providers: ProvidersConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            default_model: None,
            temperature: default_temperature(),
            max_tokens: None,
            providers: ProvidersConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from a file path
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load configuration from the default location (~/.thunderus/config.toml)
    pub fn load_default() -> Result<Self> {
        let path = Self::default_config_path()?;
        if path.exists() { Self::from_file(path) } else { Ok(Self::default()) }
    }

    /// Get the default configuration file path
    pub fn default_config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found"))?;
        Ok(home.join(".thunderus").join("config.toml"))
    }

    /// Save configuration to a file
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Save configuration to the default location
    pub fn save_default(&self) -> Result<()> {
        let path = Self::default_config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.save_to_file(path)
    }

    /// Get the API key for a provider
    pub fn get_api_key(&self, provider: &str) -> Option<&String> {
        match provider {
            "moonshot" | "kimi" => self.providers.moonshot.as_ref().and_then(|p| p.api_key.as_ref()),
            "zhipu" | "glm" => self.providers.zhipu.as_ref().and_then(|p| p.api_key.as_ref()),
            _ => None,
        }
    }

    /// Require an API key for a provider
    pub fn require_api_key(&self, provider: &str) -> Result<&String> {
        self.get_api_key(provider)
            .ok_or_else(|| CoreError::MissingApiKey(provider.to_string()))
    }
}

fn default_provider() -> String {
    "moonshot".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

/// Provider-specific configurations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// Moonshot (Kimi) provider configuration
    pub moonshot: Option<ProviderConfig>,

    /// Zhipu (GLM) provider configuration
    pub zhipu: Option<ProviderConfig>,
}

/// Configuration for a single provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// API key for the provider
    pub api_key: Option<String>,

    /// Base URL for the API (optional, uses default if not set)
    pub base_url: Option<String>,

    /// Default model for this provider
    pub default_model: Option<String>,
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into(), tool_call_id: None }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_call_id: None }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None }
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_call_id: Some(tool_call_id.into()) }
    }
}

/// Role of a message sender
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A conversation with the LLM
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn new() -> Self {
        Self { messages: Vec::new() }
    }

    pub fn with_system_prompt(prompt: impl Into<String>) -> Self {
        Self { messages: vec![Message::system(prompt)] }
    }

    pub fn with_default_system_prompt() -> Self {
        Self::with_system_prompt(build_system_prompt())
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::user(content));
    }

    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::assistant(content));
    }

    pub fn last_user_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.as_str())
    }

    pub fn last_assistant_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant)
            .map(|m| m.content.as_str())
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.default_provider, "moonshot");
        assert_eq!(config.temperature, 0.7);
        assert!(config.default_model.is_none());
        assert!(config.max_tokens.is_none());
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
            default_provider = "zhipu"
            default_model = "glm-5"
            temperature = 0.5
            max_tokens = 4096

            [providers.moonshot]
            api_key = "sk-test-moonshot"
            default_model = "kimi-k2.5"

            [providers.zhipu]
            api_key = "test.zhipu-key"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_provider, "zhipu");
        assert_eq!(config.default_model, Some("glm-5".to_string()));
        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.max_tokens, Some(4096));

        let moonshot = config.providers.moonshot.unwrap();
        assert_eq!(moonshot.api_key, Some("sk-test-moonshot".to_string()));
        assert_eq!(moonshot.default_model, Some("kimi-k2.5".to_string()));

        let zhipu = config.providers.zhipu.unwrap();
        assert_eq!(zhipu.api_key, Some("test.zhipu-key".to_string()));
    }

    #[test]
    fn test_config_save_and_load() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = Config {
            default_provider: "moonshot".to_string(),
            default_model: Some("kimi-k2.5".to_string()),
            temperature: 0.8,
            max_tokens: Some(2048),
            providers: ProvidersConfig {
                moonshot: Some(ProviderConfig {
                    api_key: Some("sk-test".to_string()),
                    base_url: None,
                    default_model: Some("kimi-k2.5".to_string()),
                }),
                zhipu: None,
            },
        };

        config.save_to_file(temp_file.path()).unwrap();

        let loaded = Config::from_file(temp_file.path()).unwrap();
        assert_eq!(loaded.default_provider, config.default_provider);
        assert_eq!(loaded.default_model, config.default_model);
        assert_eq!(loaded.temperature, config.temperature);
        assert_eq!(loaded.max_tokens, config.max_tokens);
        assert!(loaded.providers.moonshot.is_some());
        assert!(loaded.providers.zhipu.is_none());
    }

    #[test]
    fn test_message_creation() {
        let system = Message::system("You are a helpful assistant");
        assert_eq!(system.role, Role::System);
        assert_eq!(system.content, "You are a helpful assistant");
        assert!(system.tool_call_id.is_none());

        let user = Message::user("Hello!");
        assert_eq!(user.role, Role::User);
        assert_eq!(user.content, "Hello!");
        assert!(user.tool_call_id.is_none());

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, Role::Assistant);
        assert_eq!(assistant.content, "Hi there!");
        assert!(assistant.tool_call_id.is_none());

        let tool = Message::tool("call_123", "File contents");
        assert_eq!(tool.role, Role::Tool);
        assert_eq!(tool.content, "File contents");
        assert_eq!(tool.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_conversation() {
        let mut conv = Conversation::with_system_prompt("You are helpful");
        assert_eq!(conv.messages.len(), 1);
        assert_eq!(conv.messages[0].role, Role::System);

        conv.add_user_message("Hello");
        assert_eq!(conv.messages.len(), 2);
        assert_eq!(conv.messages[1].role, Role::User);

        conv.add_assistant_message("Hi!");
        assert_eq!(conv.messages.len(), 3);
        assert_eq!(conv.messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_get_api_key() {
        let config = Config {
            providers: ProvidersConfig {
                moonshot: Some(ProviderConfig {
                    api_key: Some("sk-moonshot".to_string()),
                    base_url: None,
                    default_model: None,
                }),
                zhipu: Some(ProviderConfig {
                    api_key: Some("zhipu.key".to_string()),
                    base_url: None,
                    default_model: None,
                }),
            },
            ..Default::default()
        };

        assert_eq!(config.get_api_key("moonshot"), Some(&"sk-moonshot".to_string()));
        assert_eq!(config.get_api_key("kimi"), Some(&"sk-moonshot".to_string()));
        assert_eq!(config.get_api_key("zhipu"), Some(&"zhipu.key".to_string()));
        assert_eq!(config.get_api_key("glm"), Some(&"zhipu.key".to_string()));
        assert_eq!(config.get_api_key("unknown"), None);
    }

    #[test]
    fn test_require_api_key_missing() {
        let config = Config::default();
        let result = config.require_api_key("moonshot");
        assert!(matches!(result, Err(CoreError::MissingApiKey(_))));
    }
}
