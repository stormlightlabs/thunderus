//! Stored model metadata and pricing helpers.

/// Pricing for a model in USD per 1M tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_usd_per_million: f64,
    pub cached_input_usd_per_million: Option<f64>,
    pub output_usd_per_million: f64,
}

impl ModelPricing {
    /// Estimate request cost from prompt/completion tokens.
    pub fn estimate_cost_usd(self, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        let prompt_cost = (prompt_tokens as f64 / 1_000_000.0) * self.input_usd_per_million;
        let completion_cost = (completion_tokens as f64 / 1_000_000.0) * self.output_usd_per_million;
        prompt_cost + completion_cost
    }
}

/// Static model metadata used by UI/model selection surfaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StoredModel {
    pub id: &'static str,
    pub provider: &'static str,
    pub context_window_tokens: Option<u32>,
    pub pricing: ModelPricing,
}

/// Stored model catalog.
///
/// Source: `.sandbox/prices.txt` and provider defaults in this repository.
pub const STORED_MODELS: &[StoredModel] = &[
    StoredModel {
        id: "kimi-k2.5",
        provider: "moonshot",
        context_window_tokens: Some(262_144),
        pricing: ModelPricing {
            input_usd_per_million: 0.60,
            cached_input_usd_per_million: Some(0.10),
            output_usd_per_million: 3.00,
        },
    },
    StoredModel {
        id: "glm-5",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 1.00,
            cached_input_usd_per_million: Some(0.20),
            output_usd_per_million: 3.20,
        },
    },
    StoredModel {
        id: "glm-5-code",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 1.20,
            cached_input_usd_per_million: Some(0.30),
            output_usd_per_million: 5.00,
        },
    },
    StoredModel {
        id: "glm-4.7",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.60,
            cached_input_usd_per_million: Some(0.11),
            output_usd_per_million: 2.20,
        },
    },
    StoredModel {
        id: "glm-4.7-flashx",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.07,
            cached_input_usd_per_million: Some(0.01),
            output_usd_per_million: 0.40,
        },
    },
    StoredModel {
        id: "glm-4.6",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.60,
            cached_input_usd_per_million: Some(0.11),
            output_usd_per_million: 2.20,
        },
    },
    StoredModel {
        id: "glm-4.5",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.60,
            cached_input_usd_per_million: Some(0.11),
            output_usd_per_million: 2.20,
        },
    },
    StoredModel {
        id: "glm-4.5-x",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 2.20,
            cached_input_usd_per_million: Some(0.45),
            output_usd_per_million: 8.90,
        },
    },
    StoredModel {
        id: "glm-4.5-air",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.20,
            cached_input_usd_per_million: Some(0.03),
            output_usd_per_million: 1.10,
        },
    },
    StoredModel {
        id: "glm-4.5-airx",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 1.10,
            cached_input_usd_per_million: Some(0.22),
            output_usd_per_million: 4.50,
        },
    },
    StoredModel {
        id: "glm-4-32b-0414-128k",
        provider: "zhipu",
        context_window_tokens: Some(128_000),
        pricing: ModelPricing {
            input_usd_per_million: 0.10,
            cached_input_usd_per_million: None,
            output_usd_per_million: 0.10,
        },
    },
    StoredModel {
        id: "glm-4.7-flash",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.00,
            cached_input_usd_per_million: Some(0.00),
            output_usd_per_million: 0.00,
        },
    },
    StoredModel {
        id: "glm-4.5-flash",
        provider: "zhipu",
        context_window_tokens: None,
        pricing: ModelPricing {
            input_usd_per_million: 0.00,
            cached_input_usd_per_million: Some(0.00),
            output_usd_per_million: 0.00,
        },
    },
];

/// Returns the stored model catalog.
pub fn stored_models() -> &'static [StoredModel] {
    STORED_MODELS
}

/// Looks up a stored model by ID (ASCII case-insensitive).
pub fn find_stored_model(model_id: &str) -> Option<&'static StoredModel> {
    STORED_MODELS
        .iter()
        .find(|model| model.id.eq_ignore_ascii_case(model_id))
}

/// Estimates token cost in USD if the model exists in the stored catalog.
pub fn estimate_token_cost_usd(model_id: &str, prompt_tokens: u32, completion_tokens: u32) -> Option<f64> {
    find_stored_model(model_id).map(|model| model.pricing.estimate_cost_usd(prompt_tokens, completion_tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_models_include_all_glm_entries() {
        let expected_glm_models = [
            "glm-5",
            "glm-5-code",
            "glm-4.7",
            "glm-4.7-flashx",
            "glm-4.6",
            "glm-4.5",
            "glm-4.5-x",
            "glm-4.5-air",
            "glm-4.5-airx",
            "glm-4-32b-0414-128k",
            "glm-4.7-flash",
            "glm-4.5-flash",
        ];

        for model_id in expected_glm_models {
            assert!(
                find_stored_model(model_id).is_some(),
                "missing stored model: {model_id}"
            );
        }
    }

    #[test]
    fn test_estimate_token_cost_usd() {
        let cost = estimate_token_cost_usd("glm-5", 1_000_000, 1_000_000).unwrap();
        assert!((cost - 4.2).abs() < 1e-9);
    }

    #[test]
    fn test_find_stored_model_is_case_insensitive() {
        let model = find_stored_model("GLM-4.7-FLASHX").unwrap();
        assert_eq!(model.id, "glm-4.7-flashx");
    }
}
