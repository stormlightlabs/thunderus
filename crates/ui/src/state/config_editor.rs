use std::path::PathBuf;
use thunderus_core::{ApprovalMode, Config, SandboxMode};

/// Fields in the config editor form
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigEditorField {
    ProfileName,
    ApprovalMode,
    SandboxMode,
    NetworkAccess,
    ModelName,
}

impl ConfigEditorField {
    pub const ALL: &[ConfigEditorField] = &[
        ConfigEditorField::ProfileName,
        ConfigEditorField::ApprovalMode,
        ConfigEditorField::SandboxMode,
        ConfigEditorField::NetworkAccess,
        ConfigEditorField::ModelName,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ConfigEditorField::ProfileName => "Profile",
            ConfigEditorField::ApprovalMode => "Approval Mode",
            ConfigEditorField::SandboxMode => "Sandbox Mode",
            ConfigEditorField::NetworkAccess => "Network Access",
            ConfigEditorField::ModelName => "Model",
        }
    }

    pub fn is_editable(&self) -> bool {
        matches!(
            self,
            ConfigEditorField::ApprovalMode | ConfigEditorField::SandboxMode | ConfigEditorField::NetworkAccess
        )
    }
}

/// State for the config editor form
#[derive(Debug, Clone)]
pub struct ConfigEditorState {
    /// Current profile name (read-only display)
    pub profile_name: String,
    /// Current approval mode
    pub approval_mode: ApprovalMode,
    /// Current sandbox mode
    pub sandbox_mode: SandboxMode,
    /// Network access enabled
    pub network_access: bool,
    /// Current model name (read-only display)
    pub model_name: String,
    /// Currently focused field index
    pub focused_field_index: usize,
    /// Path to the config file
    pub config_path: Option<PathBuf>,
    /// Validation errors (field index -> error message)
    pub validation_errors: Vec<(usize, String)>,
    /// Whether the editor has unsaved changes
    pub has_changes: bool,
}

impl ConfigEditorState {
    /// Create a new config editor state from current app config
    pub fn new(
        profile_name: String, approval_mode: ApprovalMode, sandbox_mode: SandboxMode, network_access: bool,
        model_name: String, config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            profile_name,
            approval_mode,
            sandbox_mode,
            network_access,
            model_name,
            focused_field_index: 1,
            config_path,
            validation_errors: Vec::new(),
            has_changes: false,
        }
    }

    /// Get the currently focused field
    pub fn focused_field(&self) -> ConfigEditorField {
        ConfigEditorField::ALL
            .get(self.focused_field_index)
            .copied()
            .unwrap_or(ConfigEditorField::ApprovalMode)
    }

    /// Move to the next field
    pub fn next_field(&mut self) {
        let total = ConfigEditorField::ALL.len();
        self.focused_field_index = (self.focused_field_index + 1) % total;
    }

    /// Move to the previous field
    pub fn prev_field(&mut self) {
        let total = ConfigEditorField::ALL.len();
        self.focused_field_index = (self.focused_field_index + total - 1) % total;
    }

    /// Toggle/cycle the value of the currently focused field
    pub fn toggle_value(&mut self) {
        let field = self.focused_field();
        if !field.is_editable() {
            return;
        }

        self.has_changes = true;

        match field {
            ConfigEditorField::ApprovalMode => {
                self.approval_mode = match self.approval_mode {
                    ApprovalMode::ReadOnly => ApprovalMode::Auto,
                    ApprovalMode::Auto => ApprovalMode::FullAccess,
                    ApprovalMode::FullAccess => ApprovalMode::ReadOnly,
                };
            }
            ConfigEditorField::SandboxMode => {
                self.sandbox_mode = match self.sandbox_mode {
                    SandboxMode::Policy => SandboxMode::Os,
                    SandboxMode::Os => SandboxMode::None,
                    SandboxMode::None => SandboxMode::Policy,
                };
            }
            ConfigEditorField::NetworkAccess => {
                self.network_access = !self.network_access;
            }
            _ => {}
        }
    }

    /// Get the display value for a field
    pub fn field_value(&self, field: ConfigEditorField) -> String {
        match field {
            ConfigEditorField::ProfileName => self.profile_name.clone(),
            ConfigEditorField::ApprovalMode => self.approval_mode.as_str().to_string(),
            ConfigEditorField::SandboxMode => self.sandbox_mode.as_str().to_string(),
            ConfigEditorField::NetworkAccess => {
                if self.network_access {
                    "enabled".to_string()
                } else {
                    "disabled".to_string()
                }
            }
            ConfigEditorField::ModelName => format!("{} (use /model to change)", self.model_name),
        }
    }

    /// Validate the current configuration
    pub fn validate(&mut self) -> bool {
        self.validation_errors.clear();
        self.validation_errors.is_empty()
    }

    /// Save the configuration to the config file
    pub fn save(&self) -> Result<String, String> {
        let Some(config_path) = &self.config_path else {
            return Err("No config path set".to_string());
        };

        let mut config = if config_path.exists() {
            Config::from_file(config_path).map_err(|e| format!("Failed to load config: {}", e))?
        } else {
            return Err("Config file does not exist".to_string());
        };

        let profile = config
            .profiles
            .get_mut(&self.profile_name)
            .ok_or_else(|| format!("Profile '{}' not found", self.profile_name))?;

        profile.approval_mode = self.approval_mode;
        profile.sandbox_mode = self.sandbox_mode;
        profile.allow_network = self.network_access;

        config
            .save_to_file(config_path)
            .map_err(|e| format!("Failed to save config: {}", e))?;

        Ok(format!("Config saved to {}", config_path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> ConfigEditorState {
        ConfigEditorState::new(
            "default".to_string(),
            ApprovalMode::Auto,
            SandboxMode::Policy,
            false,
            "glm-4.7".to_string(),
            None,
        )
    }

    #[test]
    fn test_new_starts_at_first_editable_field() {
        let state = create_test_state();
        assert_eq!(state.focused_field_index, 1);
        assert_eq!(state.focused_field(), ConfigEditorField::ApprovalMode);
    }

    #[test]
    fn test_next_field_wraps() {
        let mut state = create_test_state();
        state.focused_field_index = 4;
        state.next_field();
        assert_eq!(state.focused_field_index, 0);
    }

    #[test]
    fn test_prev_field_wraps() {
        let mut state = create_test_state();
        state.focused_field_index = 0;
        state.prev_field();
        assert_eq!(state.focused_field_index, 4);
    }

    #[test]
    fn test_toggle_approval_mode() {
        let mut state = create_test_state();
        state.focused_field_index = 1;

        state.toggle_value();
        assert_eq!(state.approval_mode, ApprovalMode::FullAccess);

        state.toggle_value();
        assert_eq!(state.approval_mode, ApprovalMode::ReadOnly);

        state.toggle_value();
        assert_eq!(state.approval_mode, ApprovalMode::Auto);
    }

    #[test]
    fn test_toggle_sandbox_mode() {
        let mut state = create_test_state();
        state.focused_field_index = 2;

        state.toggle_value();
        assert_eq!(state.sandbox_mode, SandboxMode::Os);

        state.toggle_value();
        assert_eq!(state.sandbox_mode, SandboxMode::None);

        state.toggle_value();
        assert_eq!(state.sandbox_mode, SandboxMode::Policy);
    }

    #[test]
    fn test_toggle_network_access() {
        let mut state = create_test_state();
        state.focused_field_index = 3;

        state.toggle_value();
        assert!(state.network_access);

        state.toggle_value();
        assert!(!state.network_access);
    }

    #[test]
    fn test_toggle_readonly_field_does_nothing() {
        let mut state = create_test_state();
        state.focused_field_index = 0;

        let original = state.profile_name.clone();
        state.toggle_value();
        assert_eq!(state.profile_name, original);
        assert!(!state.has_changes);
    }

    #[test]
    fn test_field_value() {
        let state = create_test_state();

        assert_eq!(state.field_value(ConfigEditorField::ProfileName), "default");
        assert_eq!(state.field_value(ConfigEditorField::ApprovalMode), "auto");
        assert_eq!(state.field_value(ConfigEditorField::SandboxMode), "policy");
        assert_eq!(state.field_value(ConfigEditorField::NetworkAccess), "disabled");
        assert_eq!(
            state.field_value(ConfigEditorField::ModelName),
            "glm-4.7 (use /model to change)"
        );
    }

    #[test]
    fn test_has_changes_tracks_modifications() {
        let mut state = create_test_state();
        assert!(!state.has_changes);

        state.focused_field_index = 1;
        state.toggle_value();
        assert!(state.has_changes);
    }
}
