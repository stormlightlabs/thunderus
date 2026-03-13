use crate::app::ScreenAction;
use crate::scroll::ScrollState;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thndrs_core::{Config as CoreConfig, CoreError};

const MAX_TEMPERATURE: f32 = 1.0;
const MIN_TEMPERATURE: f32 = 0.0;
const SETTING_GROUPS: &[&str] = &[
    "General",
    "Appearance",
    "Editor",
    "Keyboard",
    "AI Model",
    "Tools",
    "Privacy",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMsg {
    Key(KeyEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiSettings {
    #[serde(default = "default_auto_save")]
    pub auto_save_conversations: bool,
    #[serde(default = "default_show_tools")]
    pub show_tool_executions: bool,
    #[serde(default = "default_false")]
    pub sound_effects: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font_size")]
    pub font_size: String,
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,
    #[serde(default = "default_true")]
    pub word_wrap: bool,
    #[serde(default = "default_keymap")]
    pub keymap: String,
    #[serde(default = "default_true")]
    pub confirm_before_quit: bool,
    #[serde(default = "default_false")]
    pub auto_approve_safe_tools: bool,
    #[serde(default = "default_true")]
    pub enable_network_tools: bool,
    #[serde(default = "default_true")]
    pub redact_workspace_paths: bool,
    #[serde(default = "default_false")]
    pub anonymous_metrics: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            auto_save_conversations: default_auto_save(),
            show_tool_executions: default_show_tools(),
            sound_effects: default_false(),
            theme: default_theme(),
            font_size: default_font_size(),
            show_line_numbers: true,
            word_wrap: true,
            keymap: default_keymap(),
            confirm_before_quit: default_true(),
            auto_approve_safe_tools: default_false(),
            enable_network_tools: default_true(),
            redact_workspace_paths: default_true(),
            anonymous_metrics: default_false(),
        }
    }
}

fn default_auto_save() -> bool {
    true
}

fn default_show_tools() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_theme() -> String {
    "Oxocarbon Dark".to_string()
}

fn default_font_size() -> String {
    "14px".to_string()
}

fn default_keymap() -> String {
    "Default".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(flatten)]
    pub core: CoreConfig,
    #[serde(default)]
    pub ui: UiSettings,
}

impl Settings {
    pub fn load_default() -> Result<Self, CoreError> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let settings: Settings = toml::from_str(&content).map_err(CoreError::ConfigParse)?;
            Ok(settings)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save_default(&self) -> Result<(), CoreError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(CoreError::ConfigSerialize)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf, CoreError> {
        let home = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found"))?;
        Ok(home.join(".thunderus").join("config.toml"))
    }

    pub fn reset_to_defaults(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone)]
pub enum SettingItem {
    Toggle {
        name: String,
        description: String,
        value: bool,
        key: String,
    },
    Select {
        name: String,
        description: String,
        value: String,
        options: Vec<String>,
        key: String,
    },
    Number {
        name: String,
        description: String,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        key: String,
    },
}

impl SettingItem {
    pub fn name(&self) -> &str {
        match self {
            Self::Toggle { name, .. } | Self::Select { name, .. } | Self::Number { name, .. } => name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Toggle { description, .. } | Self::Select { description, .. } | Self::Number { description, .. } => {
                description
            }
        }
    }
}

pub(crate) fn setting_groups() -> &'static [&'static str] {
    SETTING_GROUPS
}

#[derive(Debug, Clone)]
pub struct SettingsApp {
    pub settings: Settings,
    pub selected_group: usize,
    pub has_changes: bool,
    pub show_save_dialog: bool,
    pub show_reset_dialog: bool,
    pub status_message: Option<String>,
    pub scroll: ScrollState,
    active_setting_index: usize,
    temp_settings: Settings,
}

impl Default for SettingsApp {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsApp {
    pub fn new() -> Self {
        let (settings, status_message) = match Settings::load_default() {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(format!("Failed to load settings: {error}"))),
        };

        let mut app = Self {
            temp_settings: settings.clone(),
            settings,
            selected_group: 0,
            has_changes: false,
            show_save_dialog: false,
            show_reset_dialog: false,
            status_message,
            scroll: ScrollState::with_viewport(0, 8),
            active_setting_index: 0,
        };
        app.sync_scroll_state();
        app
    }

    pub(crate) fn handle_input(&mut self, key: KeyEvent) -> ScreenAction {
        let Some(msg) = map_settings_key_to_msg(key) else {
            return ScreenAction::None;
        };
        update(self, msg)
    }

    fn sync_scroll_state(&mut self) {
        let total = self.current_group_settings().len();
        self.scroll.set_viewport(total, 8);
        self.scroll.ensure_visible(self.active_setting_index);
    }

    pub(crate) fn current_group_settings(&self) -> Vec<SettingItem> {
        match SETTING_GROUPS[self.selected_group] {
            "General" => self.general_settings(),
            "Appearance" => self.appearance_settings(),
            "Editor" => self.editor_settings(),
            "Keyboard" => self.keyboard_settings(),
            "AI Model" => self.ai_model_settings(),
            "Tools" => self.tools_settings(),
            "Privacy" => self.privacy_settings(),
            _ => Vec::new(),
        }
    }

    fn general_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Toggle {
                name: "Auto-save conversations".to_string(),
                description: "Automatically save chat history".to_string(),
                value: self.temp_settings.ui.auto_save_conversations,
                key: "auto_save_conversations".to_string(),
            },
            SettingItem::Toggle {
                name: "Show tool executions".to_string(),
                description: "Display tool calls in chat".to_string(),
                value: self.temp_settings.ui.show_tool_executions,
                key: "show_tool_executions".to_string(),
            },
            SettingItem::Toggle {
                name: "Sound effects".to_string(),
                description: "Play sounds on completion".to_string(),
                value: self.temp_settings.ui.sound_effects,
                key: "sound_effects".to_string(),
            },
        ]
    }

    fn appearance_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Select {
                name: "Theme".to_string(),
                description: "Select color scheme".to_string(),
                value: self.temp_settings.ui.theme.clone(),
                options: vec![
                    "Oxocarbon Dark".to_string(),
                    "Oxocarbon Light".to_string(),
                    "Dracula".to_string(),
                ],
                key: "theme".to_string(),
            },
            SettingItem::Select {
                name: "Font size".to_string(),
                description: "Editor font size".to_string(),
                value: self.temp_settings.ui.font_size.clone(),
                options: vec!["12px".to_string(), "14px".to_string(), "16px".to_string()],
                key: "font_size".to_string(),
            },
        ]
    }

    fn editor_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Toggle {
                name: "Show line numbers".to_string(),
                description: "Display line numbers in file view".to_string(),
                value: self.temp_settings.ui.show_line_numbers,
                key: "show_line_numbers".to_string(),
            },
            SettingItem::Toggle {
                name: "Word wrap".to_string(),
                description: "Wrap long lines in editor".to_string(),
                value: self.temp_settings.ui.word_wrap,
                key: "word_wrap".to_string(),
            },
        ]
    }

    fn keyboard_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Select {
                name: "Keyboard preset".to_string(),
                description: "Navigation/editing keymap".to_string(),
                value: self.temp_settings.ui.keymap.clone(),
                options: vec!["Default".to_string(), "Vim".to_string()],
                key: "keymap".to_string(),
            },
            SettingItem::Toggle {
                name: "Confirm before quit".to_string(),
                description: "Require confirmation before exit".to_string(),
                value: self.temp_settings.ui.confirm_before_quit,
                key: "confirm_before_quit".to_string(),
            },
        ]
    }

    fn ai_model_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Select {
                name: "Default provider".to_string(),
                description: "Provider for new conversations".to_string(),
                value: self.temp_settings.core.default_provider.clone(),
                options: vec!["moonshot".to_string(), "zhipu".to_string()],
                key: "default_provider".to_string(),
            },
            SettingItem::Select {
                name: "Default model".to_string(),
                description: "Model for new conversations".to_string(),
                value: self
                    .temp_settings
                    .core
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "kimi-k2.5".to_string()),
                options: vec![
                    "kimi-k2.5".to_string(),
                    "glm-5".to_string(),
                    "glm-5-code".to_string(),
                    "glm-4.7".to_string(),
                    "glm-4.7-flashx".to_string(),
                ],
                key: "default_model".to_string(),
            },
            SettingItem::Number {
                name: "Temperature".to_string(),
                description: "Response creativity (0-1)".to_string(),
                value: self.temp_settings.core.temperature,
                min: MIN_TEMPERATURE,
                max: MAX_TEMPERATURE,
                step: 0.1,
                key: "temperature".to_string(),
            },
        ]
    }

    fn tools_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Toggle {
                name: "Auto-approve safe tools".to_string(),
                description: "Skip confirmation for safe tool calls".to_string(),
                value: self.temp_settings.ui.auto_approve_safe_tools,
                key: "auto_approve_safe_tools".to_string(),
            },
            SettingItem::Toggle {
                name: "Enable network tools".to_string(),
                description: "Allow web/network based tools".to_string(),
                value: self.temp_settings.ui.enable_network_tools,
                key: "enable_network_tools".to_string(),
            },
        ]
    }

    fn privacy_settings(&self) -> Vec<SettingItem> {
        vec![
            SettingItem::Toggle {
                name: "Redact workspace paths".to_string(),
                description: "Hide local paths in logs and diagnostics".to_string(),
                value: self.temp_settings.ui.redact_workspace_paths,
                key: "redact_workspace_paths".to_string(),
            },
            SettingItem::Toggle {
                name: "Anonymous metrics".to_string(),
                description: "Share anonymized usage diagnostics".to_string(),
                value: self.temp_settings.ui.anonymous_metrics,
                key: "anonymous_metrics".to_string(),
            },
        ]
    }

    fn toggle_current_setting(&mut self) {
        let settings = self.current_group_settings();
        if let Some(item) = settings.get(self.active_setting_index) {
            match item {
                SettingItem::Toggle { key, value, .. } => self.set_setting_value(key, (!value).to_string()),
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let next_idx = (current_idx + 1) % options.len();
                    self.set_setting_value(key, options[next_idx].clone());
                }
                SettingItem::Number { .. } => {}
            }
        }
    }

    fn increment_current_setting(&mut self) {
        let settings = self.current_group_settings();
        if let Some(item) = settings.get(self.active_setting_index) {
            match item {
                SettingItem::Number { key, value, max, step, .. } => {
                    self.set_setting_value(key, (*value + *step).min(*max).to_string());
                }
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let next_idx = (current_idx + 1) % options.len();
                    self.set_setting_value(key, options[next_idx].clone());
                }
                SettingItem::Toggle { .. } => {}
            }
        }
    }

    fn decrement_current_setting(&mut self) {
        let settings = self.current_group_settings();
        if let Some(item) = settings.get(self.active_setting_index) {
            match item {
                SettingItem::Number { key, value, min, step, .. } => {
                    self.set_setting_value(key, (*value - *step).max(*min).to_string());
                }
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let prev_idx = if current_idx == 0 { options.len() - 1 } else { current_idx - 1 };
                    self.set_setting_value(key, options[prev_idx].clone());
                }
                SettingItem::Toggle { .. } => {}
            }
        }
    }

    fn set_setting_value(&mut self, key: &str, value: String) {
        match key {
            "auto_save_conversations" => self.temp_settings.ui.auto_save_conversations = value.parse().unwrap_or(true),
            "show_tool_executions" => self.temp_settings.ui.show_tool_executions = value.parse().unwrap_or(true),
            "sound_effects" => self.temp_settings.ui.sound_effects = value.parse().unwrap_or(false),
            "theme" => self.temp_settings.ui.theme = value,
            "font_size" => self.temp_settings.ui.font_size = value,
            "show_line_numbers" => self.temp_settings.ui.show_line_numbers = value.parse().unwrap_or(true),
            "word_wrap" => self.temp_settings.ui.word_wrap = value.parse().unwrap_or(true),
            "keymap" => self.temp_settings.ui.keymap = value,
            "confirm_before_quit" => self.temp_settings.ui.confirm_before_quit = value.parse().unwrap_or(true),
            "default_provider" => self.temp_settings.core.default_provider = value,
            "default_model" => self.temp_settings.core.default_model = Some(value),
            "temperature" => {
                if let Ok(temp) = value.parse::<f32>() {
                    self.temp_settings.core.temperature = temp.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);
                }
            }
            "auto_approve_safe_tools" => self.temp_settings.ui.auto_approve_safe_tools = value.parse().unwrap_or(false),
            "enable_network_tools" => self.temp_settings.ui.enable_network_tools = value.parse().unwrap_or(true),
            "redact_workspace_paths" => self.temp_settings.ui.redact_workspace_paths = value.parse().unwrap_or(true),
            "anonymous_metrics" => self.temp_settings.ui.anonymous_metrics = value.parse().unwrap_or(false),
            _ => {}
        }
        self.has_changes = self.temp_settings != self.settings;
    }

    fn save_settings(&mut self) -> Result<(), CoreError> {
        self.temp_settings.save_default()?;
        self.settings = self.temp_settings.clone();
        self.has_changes = false;
        Ok(())
    }

    pub fn version_string() -> String {
        match option_env!("CARGO_PKG_VERSION") {
            Some(version) => format!("Thunderus v{version}"),
            None => "Thunderus v0.1.0".to_string(),
        }
    }

    pub(crate) fn current_group_name(&self) -> &'static str {
        SETTING_GROUPS[self.selected_group]
    }

    pub(crate) fn active_setting_index(&self) -> usize {
        self.active_setting_index
    }

    pub(crate) fn status_text(&self) -> String {
        self.status_message
            .as_deref()
            .map_or_else(|| "Settings".to_string(), ToOwned::to_owned)
    }
}

pub(crate) fn map_settings_key_to_msg(key: KeyEvent) -> Option<SettingsMsg> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    Some(SettingsMsg::Key(key))
}

pub(crate) fn update(model: &mut SettingsApp, msg: SettingsMsg) -> ScreenAction {
    match msg {
        SettingsMsg::Key(key) => {
            if model.show_save_dialog {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Err(error) = model.save_settings() {
                            model.status_message = Some(format!("Error saving: {error}"));
                        } else {
                            model.status_message = Some("Settings saved successfully".to_string());
                        }
                        model.show_save_dialog = false;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => model.show_save_dialog = false,
                    _ => {}
                }
                return ScreenAction::None;
            }

            if model.show_reset_dialog {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        model.temp_settings.reset_to_defaults();
                        model.has_changes = true;
                        model.status_message = Some("Settings reset to defaults. Press Ctrl+S to save.".to_string());
                        model.show_reset_dialog = false;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => model.show_reset_dialog = false,
                    _ => {}
                }
                return ScreenAction::None;
            }

            let action = match key.code {
                KeyCode::Esc => ScreenAction::ReturnToPrevious,
                KeyCode::Up | KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if model.selected_group > 0 {
                        model.selected_group -= 1;
                        model.active_setting_index = 0;
                        model.scroll.set_offset(0);
                    }
                    ScreenAction::None
                }
                KeyCode::Down | KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if model.selected_group + 1 < SETTING_GROUPS.len() {
                        model.selected_group += 1;
                        model.active_setting_index = 0;
                        model.scroll.set_offset(0);
                    }
                    ScreenAction::None
                }
                KeyCode::Up => {
                    model.active_setting_index = model.active_setting_index.saturating_sub(1);
                    ScreenAction::None
                }
                KeyCode::Down => {
                    let count = model.current_group_settings().len();
                    if model.active_setting_index + 1 < count {
                        model.active_setting_index += 1;
                    }
                    ScreenAction::None
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    model.toggle_current_setting();
                    ScreenAction::None
                }
                KeyCode::Left => {
                    model.decrement_current_setting();
                    ScreenAction::None
                }
                KeyCode::Right => {
                    model.increment_current_setting();
                    ScreenAction::None
                }
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    model.show_save_dialog = true;
                    ScreenAction::None
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    model.show_reset_dialog = true;
                    ScreenAction::None
                }
                _ => ScreenAction::None,
            };

            model.sync_scroll_state();
            action
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setting_groups() {
        assert_eq!(setting_groups(), SETTING_GROUPS);
    }

    #[test]
    fn test_settings_app_new() {
        let app = SettingsApp::new();
        assert_eq!(app.selected_group, 0);
    }

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.ui.theme, "Oxocarbon Dark");
    }
}
