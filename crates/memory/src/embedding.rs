//! Embedding computation using API-based models
//!
//! Embeddings are stored in sqlite-vec for efficient vector search.

use ndarray::Array1;

use crate::{MemoryError, Result};

/// Default embedding dimensions (text-embedding-3-small truncated)
pub const DEFAULT_DIMENSIONS: usize = 256;

/// Default embedding model name
pub const DEFAULT_MODEL: &str = "text-embedding-3-small";

/// A computed embedding vector
#[derive(Debug, Clone)]
pub struct Embedding {
    pub vector: Array1<f32>,
    pub model: String,
    pub dimensions: usize,
}

impl Embedding {
    /// Create a new embedding from a vector
    pub fn new(vector: Array1<f32>, model: impl Into<String>) -> Self {
        let dimensions = vector.len();
        Self { vector, model: model.into(), dimensions }
    }

    /// Normalize the embedding to unit length
    pub fn normalize(mut self) -> Self {
        let norm = self.vector.dot(&self.vector).sqrt();
        if norm > 0.0 {
            self.vector /= norm;
        }
        self
    }

    /// Convert to sqlite-vec format (JSON array)
    pub fn to_json(&self) -> String {
        let values: Vec<String> = self.vector.iter().map(|v| v.to_string()).collect();
        format!("[{}]", values.join(","))
    }

    /// Convert to bytes for storage
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.dimensions * 4);
        for &value in self.vector.iter() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Parse from sqlite-vec JSON format
    pub fn from_json(json: &str, model: impl Into<String>) -> Result<Self> {
        let trimmed = json.trim();
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return Err(MemoryError::Embedding("Invalid JSON format".to_string()));
        }

        let content = &trimmed[1..trimmed.len() - 1];
        let values: Result<Vec<f32>> = content
            .split(',')
            .map(|s| {
                s.trim()
                    .parse()
                    .map_err(|e| MemoryError::Embedding(format!("Parse error: {}", e)))
            })
            .collect();

        let vector = values?;
        Ok(Self::new(Array1::from(vector), model))
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8], dimensions: usize, model: impl Into<String>) -> Result<Self> {
        if bytes.len() != dimensions * 4 {
            return Err(MemoryError::Embedding(format!(
                "Invalid byte length: expected {}, got {}",
                dimensions * 4,
                bytes.len()
            )));
        }

        let mut vector = Vec::with_capacity(dimensions);
        for i in 0..dimensions {
            let offset = i * 4;
            let value = f32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]]);
            vector.push(value);
        }

        Ok(Self::new(Array1::from(vector), model))
    }
}

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
    pub dimensions: usize,
    pub api_key: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self { model: DEFAULT_MODEL.to_string(), dimensions: DEFAULT_DIMENSIONS, api_key: None }
    }
}

impl EmbeddingConfig {
    /// Load from environment
    pub fn from_env() -> Self {
        let model = std::env::var("thndrs_EMBED_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

        let dimensions = std::env::var("thndrs_EMBED_DIMENSIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DIMENSIONS);

        let api_key = std::env::var("OPENAI_API_KEY").ok();

        Self { model, dimensions, api_key }
    }

    /// Check if embeddings are enabled
    pub fn is_enabled(&self) -> bool {
        self.api_key.is_some()
    }
}

/// Generate embedding using OpenAI API
pub async fn generate_embedding(text: &str, config: &EmbeddingConfig) -> Result<Embedding> {
    use reqwest;
    use serde_json::json;

    let api_key = config
        .api_key
        .as_ref()
        .ok_or_else(|| MemoryError::Embedding("OpenAI API key not configured".to_string()))?;

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&json!({
            "input": text,
            "model": config.model,
            "dimensions": config.dimensions
        }))
        .send()
        .await
        .map_err(|e| MemoryError::Embedding(format!("API request failed: {}", e)))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(MemoryError::Embedding(format!("API error: {}", error_text)));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| MemoryError::Embedding(format!("JSON parse error: {}", e)))?;

    let embedding_data = json["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| MemoryError::Embedding("Invalid embedding response".to_string()))?;

    let vector: Vec<f32> = embedding_data
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    if vector.len() != config.dimensions {
        return Err(MemoryError::Embedding(format!(
            "Unexpected dimensions: expected {}, got {}",
            config.dimensions,
            vector.len()
        )));
    }

    Ok(Embedding::new(Array1::from(vector), &config.model).normalize())
}

/// Fallback: Generate a simple hash-based embedding for when API is unavailable
/// Note: This is NOT compatible with sqlite-vec semantic search - only for storage
pub fn generate_fallback_embedding(text: &str, dimensions: usize) -> Embedding {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vector = vec![0.0f32; dimensions];

    for (i, value) in vector.iter_mut().enumerate() {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash = hasher.finish();

        *value = (hash as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0;
    }

    Embedding::new(Array1::from(vector), "fallback-hash").normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_json_roundtrip() {
        let vector = Array1::from(vec![1.0f32, 2.0, 3.0, 4.0]);
        let embedding = Embedding::new(vector, "test-model").normalize();

        let json = embedding.to_json();
        let restored = Embedding::from_json(&json, "test-model").unwrap();

        assert_eq!(embedding.dimensions, restored.dimensions);
        for i in 0..embedding.vector.len() {
            assert!((embedding.vector[i] - restored.vector[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn test_embedding_bytes_roundtrip() {
        let vector = Array1::from(vec![1.0f32, 2.0, 3.0, 4.0]);
        let embedding = Embedding::new(vector, "test-model").normalize();

        let bytes = embedding.to_bytes();
        let restored = Embedding::from_bytes(&bytes, 4, "test-model").unwrap();

        assert_eq!(embedding.dimensions, restored.dimensions);
        for i in 0..embedding.vector.len() {
            assert!((embedding.vector[i] - restored.vector[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_config_from_env() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.dimensions, DEFAULT_DIMENSIONS);
    }
}
