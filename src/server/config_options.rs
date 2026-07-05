//! ACP session config option primitives.

use crate::cli::WebSearchMode;
use agent_client_protocol::schema::v1::{SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption};

/// ACP config option id for model selection.
pub const MODEL_CONFIG_OPTION_ID: &str = "model";

/// ACP config option id for web-search mode selection.
pub const WEBSEARCH_CONFIG_OPTION_ID: &str = "websearch";

/// Stable ACP option specification shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigOptionSpec {
    /// Wire identifier.
    pub id: &'static str,
    /// Human-readable option label.
    pub label: &'static str,
    /// Supported value domain.
    pub mode: &'static str,
}

/// Validated config option value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigOptionValue {
    /// ACP `model` value.
    Model(String),
    /// ACP `websearch` value.
    WebSearch(WebSearchMode),
}

impl ConfigOptionValue {
    /// Return the persisted string representation for session metadata.
    pub fn as_persisted_value(&self) -> String {
        match self {
            Self::Model(model) => model.clone(),
            Self::WebSearch(mode) => mode.label().to_string(),
        }
    }
}

/// Errors produced by config-option validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigOptionError {
    /// Unknown option id.
    UnknownOption { id: String },
    /// Value failed option-specific validation.
    InvalidValue { id: String, value: String, reason: String },
}

impl core::fmt::Display for ConfigOptionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownOption { id } => write!(f, "unknown config option `{id}`"),
            Self::InvalidValue { id, value, reason } => {
                write!(f, "invalid value `{value}` for config option `{id}`: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigOptionError {}

/// Return the initial ACP config option IDs supported by the server.
///
/// Kept in stable order for protocol responses and tests.
pub fn initial_config_option_ids() -> &'static [&'static str; 2] {
    &[MODEL_CONFIG_OPTION_ID, WEBSEARCH_CONFIG_OPTION_ID]
}

/// Return the initial server config option specs.
pub fn initial_config_options() -> &'static [ConfigOptionSpec] {
    const OPTIONS: [ConfigOptionSpec; 2] = [
        ConfigOptionSpec { id: MODEL_CONFIG_OPTION_ID, label: "Model", mode: "string" },
        ConfigOptionSpec { id: WEBSEARCH_CONFIG_OPTION_ID, label: "Web Search", mode: "enum" },
    ];
    &OPTIONS
}

/// Return ACP session config options for the current session state.
pub fn acp_config_options(model: &str, websearch: WebSearchMode) -> Vec<SessionConfigOption> {
    vec![model_config_option(model), websearch_config_option(websearch)]
}

/// Validate one config option id/value pair.
pub fn validate_config_option(option_id: &str, value: &str) -> Result<ConfigOptionValue, ConfigOptionError> {
    match option_id {
        MODEL_CONFIG_OPTION_ID => validate_model(value),
        WEBSEARCH_CONFIG_OPTION_ID => validate_websearch(value),
        _ => Err(ConfigOptionError::UnknownOption { id: option_id.to_string() }),
    }
}

fn validate_model(value: &str) -> Result<ConfigOptionValue, ConfigOptionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigOptionError::InvalidValue {
            id: MODEL_CONFIG_OPTION_ID.to_string(),
            value: value.to_string(),
            reason: "model may not be empty".to_string(),
        });
    }
    Ok(ConfigOptionValue::Model(trimmed.to_string()))
}

fn validate_websearch(value: &str) -> Result<ConfigOptionValue, ConfigOptionError> {
    let mode = match value.to_lowercase().as_str() {
        "auto" => WebSearchMode::Auto,
        "native" => WebSearchMode::Native,
        "exa" => WebSearchMode::Exa,
        "none" => WebSearchMode::None,
        other => {
            return Err(ConfigOptionError::InvalidValue {
                id: WEBSEARCH_CONFIG_OPTION_ID.to_string(),
                value: other.to_string(),
                reason: "must be one of auto/native/exa/none".to_string(),
            });
        }
    };
    Ok(ConfigOptionValue::WebSearch(mode))
}

fn model_config_option(model: &str) -> SessionConfigOption {
    SessionConfigOption::select(
        MODEL_CONFIG_OPTION_ID,
        "Model",
        model.to_string(),
        vec![SessionConfigSelectOption::new(model.to_string(), model.to_string())],
    )
    .description("Provider model for future prompt turns")
    .category(SessionConfigOptionCategory::Model)
}

fn websearch_config_option(websearch: WebSearchMode) -> SessionConfigOption {
    let current = websearch.label();
    SessionConfigOption::select(
        WEBSEARCH_CONFIG_OPTION_ID,
        "Web Search",
        current,
        vec![
            SessionConfigSelectOption::new("auto", "Auto"),
            SessionConfigSelectOption::new("native", "Native"),
            SessionConfigSelectOption::new("exa", "Exa"),
            SessionConfigSelectOption::new("none", "None"),
        ],
    )
    .description("Web search mode for future prompt turns")
    .category(SessionConfigOptionCategory::ModelConfig)
}
