/// Model selector state for footer UI
///
/// Placeholder for future model/agent selection functionality.
#[derive(Debug, Clone, Default)]
pub struct ModelSelectorState {
    /// Currently selected model
    pub current_model: String,
    /// Available models
    pub available_models: Vec<String>,
    /// Currently selected agent (optional)
    pub current_agent: Option<String>,
}

impl ModelSelectorState {
    pub fn new(current_model: String) -> Self {
        Self {
            current_model,
            available_models: vec![
                "GLM-4.7".to_string(),
                "Gemini 3 Pro".to_string(),
                "Gemini 3 Flash".to_string(),
            ],
            current_agent: None,
        }
    }

    /// Get display name for current model
    pub fn display_name(&self) -> &str {
        &self.current_model
    }

    /// Select a model by name
    pub fn select_model(&mut self, model: String) {
        if self.available_models.contains(&model) {
            self.current_model = model;
        }
    }

    /// Get available models
    pub fn models(&self) -> &[String] {
        &self.available_models
    }

    /// Set current agent
    pub fn set_agent(&mut self, agent: String) {
        self.current_agent = Some(agent);
    }

    /// Clear current agent
    pub fn clear_agent(&mut self) {
        self.current_agent = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_selector_new() {
        let selector = ModelSelectorState::new("GLM-4.7".to_string());
        assert_eq!(selector.current_model, "GLM-4.7");
        assert_eq!(selector.available_models.len(), 3);
        assert!(selector.current_agent.is_none());
    }

    #[test]
    fn test_select_model() {
        let mut selector = ModelSelectorState::new("GLM-4.7".to_string());
        selector.select_model("Gemini 3 Pro".to_string());
        assert_eq!(selector.current_model, "Gemini 3 Pro");
    }

    #[test]
    fn test_select_invalid_model() {
        let mut selector = ModelSelectorState::new("GLM-4.7".to_string());
        selector.select_model("Invalid".to_string());
        assert_eq!(selector.current_model, "GLM-4.7");
    }

    #[test]
    fn test_set_agent() {
        let mut selector = ModelSelectorState::new("GLM-4.7".to_string());
        selector.set_agent("test-agent".to_string());
        assert_eq!(selector.current_agent, Some("test-agent".to_string()));
    }

    #[test]
    fn test_clear_agent() {
        let mut selector = ModelSelectorState::new("GLM-4.7".to_string());
        selector.set_agent("test-agent".to_string());
        selector.clear_agent();
        assert!(selector.current_agent.is_none());
    }
}
