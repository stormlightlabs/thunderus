//! OpenCode provider implementations.

pub mod go;
pub mod zen;

pub use go::OpenCodeGoClient;

use crate::providers::KnownModel;

/// OpenAI-compatible model list response shared by OpenCode providers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ModelsResponse {
    pub object: String,
    #[serde(default)]
    pub data: Vec<ModelInfo>,
}

/// OpenAI-compatible model metadata shared by OpenCode providers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

/// Whether `model` is an OpenCode Go model id.
pub fn is_go_model_id(model: &str) -> bool {
    go::is_model_id(model)
}

/// Whether `model` is an OpenCode Zen model id.
pub fn is_zen_model_id(model: &str) -> bool {
    zen::is_model_id(model)
}

/// Whether `model` is any OpenCode-backed model id.
pub fn is_model_id(model: &str) -> bool {
    is_go_model_id(model) || is_zen_model_id(model)
}

/// Offline model picker entries for all OpenCode-backed providers.
pub fn known_models() -> Vec<KnownModel> {
    let mut models = zen::known_models();
    models.extend(go::known_models());
    models
}

/// Validate an OpenCode Go API key.
pub fn validate_go_api_key(api_key: &str) -> std::result::Result<(), String> {
    go::validate_api_key(api_key)
}

/// Validate an OpenCode Zen API key.
pub fn validate_zen_api_key(api_key: &str) -> std::result::Result<(), String> {
    zen::validate_api_key(api_key)
}

pub fn probe_go_api_key(api_key: &str) -> crate::providers::Result<()> {
    go::probe_api_key(api_key)
}

pub fn probe_zen_api_key(api_key: &str) -> crate::providers::Result<()> {
    zen::probe_api_key(api_key)
}
