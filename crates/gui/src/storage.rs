use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PersistedUiState {
    pub workspace_root: Option<PathBuf>,
    #[serde(default)]
    pub recent_workspaces: Vec<PathBuf>,
    pub composer_text: String,
    pub last_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new_default() -> Self {
        Self { path: default_state_path() }
    }

    pub fn load(&self) -> PersistedUiState {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(_) => return PersistedUiState::default(),
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save(&self, state: &PersistedUiState) -> Result<(), String> {
        let Some(parent) = self.path.parent() else {
            return Err("State path has no parent directory".to_string());
        };
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;

        let serialized = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
        fs::write(&self.path, serialized).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn default_state_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".thunderus").join("desktop_state.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_ui_state_round_trips_json() {
        let state = PersistedUiState {
            workspace_root: Some(PathBuf::from("/tmp/demo")),
            recent_workspaces: vec![PathBuf::from("/tmp/demo"), PathBuf::from("/tmp/other")],
            composer_text: "draft".to_string(),
            last_model: Some("kimi-k2.5".to_string()),
        };

        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: PersistedUiState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, state);
    }
}
