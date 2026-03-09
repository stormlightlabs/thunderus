//! Settings screen and configuration management for Thunderus
//!
//! Provides a TUI settings interface with:
//! - Split pane layout: sidebar navigation + settings content
//! - Setting groups: General, Appearance, Editor, Keyboard, AI Model, Tools, Privacy
//! - Toggle switches, select dropdowns
//! - Save/reset actions
//! - Persistence to ~/.thunderus/config.toml

use super::components::{HintFooter, HintToken, TopBorderedInputRow, app_version_string};
use super::layout::{ConstraintSpec, split as split_rects};
use super::screen::ScreenAction;
use super::{colors, scroll::ScrollState};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thndrs_core::{Config as CoreConfig, CoreError};
use thndrs_ui_macros::AreaSpec;

const SIDEBAR_WIDTH: u16 = 18;
const SETTING_GROUPS: &[&str] = &[
    "General",
    "Appearance",
    "Editor",
    "Keyboard",
    "AI Model",
    "Tools",
    "Privacy",
];
const MAX_TEMPERATURE: f32 = 1.0;
const MIN_TEMPERATURE: f32 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsMsg {
    Key(KeyEvent),
}

pub type SettingsModel = SettingsApp;

/// UI-specific settings that extend the core configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiSettings {
    /// Auto-save conversations to memory
    #[serde(default = "default_auto_save")]
    pub auto_save_conversations: bool,

    /// Show tool executions in chat
    #[serde(default = "default_show_tools")]
    pub show_tool_executions: bool,

    /// Play terminal bell sounds for key events
    #[serde(default = "default_false")]
    pub sound_effects: bool,

    /// Preferred UI theme label
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Preferred editor font size label
    #[serde(default = "default_font_size")]
    pub font_size: String,

    /// Line numbers in file view
    #[serde(default = "default_true")]
    pub show_line_numbers: bool,

    /// Word wrap in file view
    #[serde(default = "default_true")]
    pub word_wrap: bool,

    /// Keyboard mode for navigation and editing
    #[serde(default = "default_keymap")]
    pub keymap: String,

    /// Ask for confirmation before exiting the app
    #[serde(default = "default_true")]
    pub confirm_before_quit: bool,

    /// Allow tool calls without confirmation prompts
    #[serde(default = "default_false")]
    pub auto_approve_safe_tools: bool,

    /// Enable web/network-connected tools when available
    #[serde(default = "default_true")]
    pub enable_network_tools: bool,

    /// Redact workspace paths in diagnostics and logs
    #[serde(default = "default_true")]
    pub redact_workspace_paths: bool,

    /// Share anonymous usage metrics
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

/// Complete settings including core config and UI settings
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    #[serde(flatten)]
    pub core: CoreConfig,

    #[serde(default)]
    pub ui: UiSettings,
}

impl Settings {
    /// Load settings from the default location (~/.thunderus/config.toml)
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

    /// Save settings to the default location
    pub fn save_default(&self) -> Result<(), CoreError> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(CoreError::ConfigSerialize)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf, CoreError> {
        let home = dirs::home_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Home directory not found"))?;
        Ok(home.join(".thunderus").join("config.toml"))
    }

    /// Reset to default settings
    pub fn reset_to_defaults(&mut self) {
        *self = Self::default();
    }
}

/// A setting item that can be rendered and edited
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
    pub fn key(&self) -> &str {
        match self {
            Self::Toggle { key, .. } | Self::Select { key, .. } | Self::Number { key, .. } => key,
        }
    }

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

/// Settings application state
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

    pub fn handle_input(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.show_save_dialog {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Err(e) = self.save_settings() {
                        self.status_message = Some(format!("Error saving: {}", e));
                    } else {
                        self.status_message = Some("Settings saved successfully".to_string());
                    }
                    self.show_save_dialog = false;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.show_save_dialog = false;
                }
                _ => {}
            }
            return;
        }

        if self.show_reset_dialog {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.temp_settings.reset_to_defaults();
                    self.has_changes = true;
                    self.status_message = Some("Settings reset to defaults. Press Ctrl+S to save.".to_string());
                    self.show_reset_dialog = false;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.show_reset_dialog = false;
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc => {}
            KeyCode::Up | KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.selected_group > 0 {
                    self.selected_group -= 1;
                    self.active_setting_index = 0;
                    self.scroll.set_offset(0);
                    self.sync_scroll_state();
                }
            }
            KeyCode::Down | KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.selected_group + 1 < SETTING_GROUPS.len() {
                    self.selected_group += 1;
                    self.active_setting_index = 0;
                    self.scroll.set_offset(0);
                    self.sync_scroll_state();
                }
            }
            KeyCode::Up => {
                if self.active_setting_index > 0 {
                    self.active_setting_index -= 1;
                }
            }
            KeyCode::Down => {
                let count = self.current_group_settings().len();
                if self.active_setting_index + 1 < count {
                    self.active_setting_index += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle_current_setting();
            }
            KeyCode::Left => {
                self.decrement_current_setting();
            }
            KeyCode::Right => {
                self.increment_current_setting();
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_save_dialog = true;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_reset_dialog = true;
            }
            _ => {}
        }

        self.sync_scroll_state();
    }

    fn sync_scroll_state(&mut self) {
        let total = self.current_group_settings().len();
        self.scroll.set_viewport(total, 8);
        self.scroll.ensure_visible(self.active_setting_index);
    }

    fn current_group_settings(&self) -> Vec<SettingItem> {
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
        let themes = vec![
            "Oxocarbon Dark".to_string(),
            "Oxocarbon Light".to_string(),
            "Dracula".to_string(),
        ];
        let font_sizes = vec!["12px".to_string(), "14px".to_string(), "16px".to_string()];

        vec![
            SettingItem::Select {
                name: "Theme".to_string(),
                description: "Select color scheme".to_string(),
                value: self.temp_settings.ui.theme.clone(),
                options: themes,
                key: "theme".to_string(),
            },
            SettingItem::Select {
                name: "Font size".to_string(),
                description: "Editor font size".to_string(),
                value: self.temp_settings.ui.font_size.clone(),
                options: font_sizes,
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
        let keymaps = vec!["Default".to_string(), "Vim".to_string()];

        vec![
            SettingItem::Select {
                name: "Keyboard preset".to_string(),
                description: "Navigation/editing keymap".to_string(),
                value: self.temp_settings.ui.keymap.clone(),
                options: keymaps,
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
        let providers = vec!["moonshot".to_string(), "zhipu".to_string()];
        let models = vec![
            "kimi-k2.5".to_string(),
            "glm-5".to_string(),
            "glm-5-code".to_string(),
            "glm-4.7".to_string(),
            "glm-4.7-flashx".to_string(),
        ];

        vec![
            SettingItem::Select {
                name: "Default provider".to_string(),
                description: "Provider for new conversations".to_string(),
                value: self.temp_settings.core.default_provider.clone(),
                options: providers,
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
                options: models,
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
                SettingItem::Toggle { key, value, .. } => {
                    let new_value = !value;
                    self.set_setting_value(key, new_value.to_string());
                }
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let next_idx = (current_idx + 1) % options.len();
                    self.set_setting_value(key, options[next_idx].clone());
                }
                _ => {}
            }
        }
    }

    fn increment_current_setting(&mut self) {
        let settings = self.current_group_settings();
        if let Some(item) = settings.get(self.active_setting_index) {
            match item {
                SettingItem::Number { key, value, max, step, .. } => {
                    let new_value = (*value + *step).min(*max);
                    self.set_setting_value(key, new_value.to_string());
                }
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let next_idx = (current_idx + 1) % options.len();
                    self.set_setting_value(key, options[next_idx].clone());
                }
                _ => {}
            }
        }
    }

    fn decrement_current_setting(&mut self) {
        let settings = self.current_group_settings();
        if let Some(item) = settings.get(self.active_setting_index) {
            match item {
                SettingItem::Number { key, value, min, step, .. } => {
                    let new_value = (*value - *step).max(*min);
                    self.set_setting_value(key, new_value.to_string());
                }
                SettingItem::Select { key, options, value, .. } => {
                    let current_idx = options.iter().position(|o| o == value).unwrap_or(0);
                    let prev_idx = if current_idx == 0 { options.len() - 1 } else { current_idx - 1 };
                    self.set_setting_value(key, options[prev_idx].clone());
                }
                _ => {}
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
        app_version_string()
    }
}

pub(crate) fn map_settings_key_to_msg(key: KeyEvent) -> Option<SettingsMsg> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    Some(SettingsMsg::Key(key))
}

pub(crate) fn update(model: &mut SettingsModel, msg: SettingsMsg) -> ScreenAction {
    match msg {
        SettingsMsg::Key(key) => {
            model.handle_input(key);
            if key.code == KeyCode::Esc { ScreenAction::ReturnToPrevious } else { ScreenAction::None }
        }
    }
}

pub fn view(frame: &mut Frame, model: &SettingsModel) {
    draw_settings_screen(frame, model);
}

#[derive(AreaSpec)]
pub struct SettingsShell;

impl ConstraintSpec for SettingsShell {
    fn direction(&self) -> Direction {
        Direction::Vertical
    }

    fn constraints(&self, _area: Rect) -> Vec<Constraint> {
        vec![Constraint::Min(0), Constraint::Length(1), Constraint::Length(3)]
    }
}

pub fn draw_settings_screen(frame: &mut Frame, app: &SettingsModel) {
    let size = frame.area();
    let clear = Block::default().style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(clear, size);

    let shell = SettingsShell.split(size);
    if shell.len() < 3 {
        return;
    }

    draw_settings_main(frame, shell[0], app);
    draw_settings_hints(frame, shell[1]);
    draw_settings_status(frame, shell[2], app);

    if app.show_save_dialog {
        draw_confirm_dialog(frame, size, "Save changes?", "y: yes / n: no");
    } else if app.show_reset_dialog {
        draw_confirm_dialog(frame, size, "Reset to defaults?", "y: yes / n: no");
    }
}

fn draw_settings_main(frame: &mut Frame, area: Rect, app: &SettingsApp) {
    let layout = split_rects(
        area,
        Direction::Horizontal,
        vec![Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)],
    );

    if layout.len() < 2 {
        return;
    }

    draw_sidebar(frame, layout[0], app);
    draw_settings_content(frame, layout[1], app);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, app: &SettingsApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines = Vec::new();

    lines.push(Line::from(vec![Span::styled(
        SettingsApp::version_string(),
        Style::default().fg(colors::TEXT_MUTED).add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for (idx, group) in SETTING_GROUPS.iter().enumerate() {
        let is_selected = idx == app.selected_group;
        let style = if is_selected {
            Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::TEXT_SECONDARY)
        };

        let prefix = if is_selected { "› " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled((*group).to_string(), style),
        ]));
    }

    let text = Text::from(lines);
    let paragraph = Paragraph::new(text).style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(paragraph, inner);
}

fn draw_settings_content(frame: &mut Frame, area: Rect, app: &SettingsApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::BORDER_COLOR))
        .style(Style::default().bg(colors::BG_TERMINAL));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let settings = app.current_group_settings();
    if settings.is_empty() {
        let placeholder = Paragraph::new("No settings available for this category")
            .style(Style::default().fg(colors::TEXT_MUTED))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(placeholder, inner);
        return;
    }

    let layout = split_rects(
        inner,
        Direction::Vertical,
        vec![Constraint::Length(1), Constraint::Min(0), Constraint::Length(2)],
    );
    if layout.len() < 3 {
        return;
    }

    let group_title = SETTING_GROUPS[app.selected_group];
    let title = Paragraph::new(Span::styled(
        group_title.to_string(),
        Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(title, layout[0]);

    let settings_area = layout[1];
    let visible_slots = (settings_area.height / 3).max(1) as usize;
    let start = app.scroll.offset.min(settings.len().saturating_sub(1));
    let end = (start + visible_slots).min(settings.len());
    let visible_count = end.saturating_sub(start);
    let mut row_constraints = vec![Constraint::Length(3); visible_count];
    row_constraints.push(Constraint::Min(0));
    let setting_rows = split_rects(settings_area, Direction::Vertical, row_constraints);

    for (visible_idx, setting_idx) in (start..end).enumerate() {
        if let Some(slot) = setting_rows.get(visible_idx) {
            let setting = &settings[setting_idx];
            let is_active = setting_idx == app.active_setting_index;
            draw_setting_item(frame, *slot, setting, is_active);
        }
    }

    if let Some(actions_area) = layout.get(2) {
        let hint_style = Style::default().fg(colors::TEXT_MUTED);
        let key_style = Style::default().fg(colors::ACCENT_CYAN);

        let actions_text = if app.has_changes {
            Line::from(vec![
                Span::styled("Press ", hint_style),
                Span::styled("Ctrl+S", key_style),
                Span::styled(" to save or ", hint_style),
                Span::styled("Ctrl+R", key_style),
                Span::styled(" to reset", hint_style),
            ])
        } else {
            Line::from(vec![
                Span::styled("Press ", hint_style),
                Span::styled("Ctrl+R", key_style),
                Span::styled(" to reset to defaults", hint_style),
            ])
        };

        let actions = Paragraph::new(actions_text);
        frame.render_widget(actions, *actions_area);
    }
}

fn draw_setting_item(frame: &mut Frame, area: Rect, item: &SettingItem, is_active: bool) {
    let bg_style = if is_active {
        Style::default().bg(colors::BG_TERTIARY)
    } else {
        Style::default().bg(colors::BG_TERMINAL)
    };

    let fill = Block::default().style(bg_style);
    frame.render_widget(fill, area);

    let layout = split_rects(
        area,
        Direction::Horizontal,
        vec![Constraint::Percentage(60), Constraint::Percentage(40)],
    );
    if layout.len() < 2 {
        return;
    }

    let name_style = Style::default().fg(colors::TEXT_PRIMARY).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(colors::TEXT_MUTED);

    let label_lines = vec![
        Line::from(vec![Span::styled(item.name().to_string(), name_style)]),
        Line::from(vec![Span::styled(item.description().to_string(), desc_style)]),
    ];
    let label_text = Text::from(label_lines);
    let label = Paragraph::new(label_text).style(bg_style).wrap(Wrap { trim: true });
    frame.render_widget(label, layout[0]);

    match item {
        SettingItem::Toggle { value, .. } => {
            let toggle_style = Style::default().fg(colors::ACCENT_CYAN);
            let off_style = Style::default().fg(colors::TEXT_MUTED);

            let toggle_text =
                if *value { Span::styled("[ON]", toggle_style) } else { Span::styled("[OFF]", off_style) };

            let control = Paragraph::new(Line::from(vec![toggle_text])).style(bg_style);
            frame.render_widget(control, layout[1]);
        }
        SettingItem::Select { value, options, .. } => {
            let current_idx = options.iter().position(|o| o == value).unwrap_or(0);

            let mut spans = vec![];
            for (idx, option) in options.iter().enumerate() {
                if idx == current_idx {
                    spans.push(Span::styled(
                        format!(" [{}] ", option),
                        Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
                    ));
                } else {
                    spans.push(Span::styled(
                        format!("  {}  ", option),
                        Style::default().fg(colors::TEXT_MUTED),
                    ));
                }
            }

            let control = Paragraph::new(Line::from(spans)).style(bg_style);
            frame.render_widget(control, layout[1]);
        }
        SettingItem::Number { value, min, max, .. } => {
            let value_text = format!("{:.1}", value);
            let control_text = Line::from(vec![
                Span::styled(format!("{:.1} ", min), Style::default().fg(colors::TEXT_MUTED)),
                Span::styled(
                    "◀ ",
                    Style::default().fg(if *value > *min { colors::ACCENT_CYAN } else { colors::TEXT_MUTED }),
                ),
                Span::styled(
                    value_text,
                    Style::default().fg(colors::ACCENT_CYAN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " ▶",
                    Style::default().fg(if *value < *max { colors::ACCENT_CYAN } else { colors::TEXT_MUTED }),
                ),
                Span::styled(format!(" {:.1}", max), Style::default().fg(colors::TEXT_MUTED)),
            ]);

            let control = Paragraph::new(control_text).style(bg_style);
            frame.render_widget(control, layout[1]);
        }
    }
}

fn draw_settings_hints(frame: &mut Frame, area: Rect) {
    let tokens = [
        HintToken::Text("Press "),
        HintToken::Key("Ctrl+↑/↓"),
        HintToken::Text(" switch groups, "),
        HintToken::Key("↑/↓"),
        HintToken::Text(" navigate, "),
        HintToken::Key("Enter"),
        HintToken::Text(" toggle, "),
        HintToken::Key("Esc"),
        HintToken::Text(" exit"),
    ];
    HintFooter.render(frame, area, &tokens);
}

fn draw_settings_status(frame: &mut Frame, area: Rect, app: &SettingsApp) {
    let status = app
        .status_message
        .as_deref()
        .map_or_else(|| "Settings".to_string(), |s| s.to_string());

    TopBorderedInputRow.render(frame, area, &status, false);
}

fn draw_confirm_dialog(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
    let dialog_width = 40u16.min(area.width.saturating_sub(4)).max(20);
    let dialog_height = 5u16.min(area.height.saturating_sub(2));

    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let block = Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors::ACCENT_CYAN))
        .style(Style::default().bg(colors::BG_SECONDARY));
    frame.render_widget(block.clone(), dialog_area);

    let inner = block.inner(dialog_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let hint_style = Style::default().fg(colors::TEXT_SECONDARY);
    let hint_para = Paragraph::new(hint.to_string())
        .style(hint_style)
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(hint_para, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert!(settings.ui.auto_save_conversations);
        assert!(settings.ui.show_tool_executions);
        assert!(!settings.ui.sound_effects);
        assert_eq!(settings.ui.theme, "Oxocarbon Dark");
        assert_eq!(settings.ui.font_size, "14px");
        assert!(settings.ui.show_line_numbers);
        assert!(settings.ui.word_wrap);
        assert_eq!(settings.ui.keymap, "Default");
        assert!(settings.ui.confirm_before_quit);
        assert!(!settings.ui.auto_approve_safe_tools);
        assert!(settings.ui.enable_network_tools);
        assert!(settings.ui.redact_workspace_paths);
        assert!(!settings.ui.anonymous_metrics);
    }

    #[test]
    fn test_settings_app_new() {
        let app = SettingsApp::new();
        assert_eq!(app.selected_group, 0);
        assert!(!app.has_changes);
    }

    #[test]
    fn test_setting_groups() {
        assert_eq!(SETTING_GROUPS.len(), 7);
        assert_eq!(SETTING_GROUPS[0], "General");
        assert_eq!(SETTING_GROUPS[3], "Keyboard");
        assert_eq!(SETTING_GROUPS[4], "AI Model");
        assert_eq!(SETTING_GROUPS[6], "Privacy");
    }

    #[test]
    fn test_setting_item_key() {
        let item = SettingItem::Toggle {
            name: "Test".to_string(),
            description: "Description".to_string(),
            value: true,
            key: "test_key".to_string(),
        };
        assert_eq!(item.key(), "test_key");
    }
}
