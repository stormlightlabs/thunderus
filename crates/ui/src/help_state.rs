use crate::app::ScreenAction;
use crate::scroll::ScrollState;
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use thndrs_mem::{Session, SessionManager};

pub(crate) const HELP_TABS: &[&str] = &["Keyboard Shortcuts", "Commands", "Tips", "About", "Tutorial"];
pub(crate) const TIPS: &[&str] = &[
    "Use @ to reference files in your workspace. Type @ followed by the filename to include file context.",
    "Use Up/Down to cycle previous prompts in chat and recover drafts",
    "Tab toggles the latest tool call, Shift+Tab toggles all tool calls in the latest assistant response",
    "Ctrl+N starts a new conversation instantly",
    "Ctrl+O opens the workspace file browser for multi-file context",
    "Ctrl+W returns to the welcome screen from chat and files",
    "Use /debug chat to load a long chat for transcript stress testing",
    "Recent conversations are automatically saved and can be resumed",
];

pub(crate) const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigation",
        &[
            ("New chat", "Ctrl+N"),
            ("Close chat", "Ctrl+W"),
            ("Open file browser", "Ctrl+O"),
            ("Open settings", "Ctrl+,"),
            ("Open help", "/help or F1"),
            ("Quit", "Ctrl+D / Ctrl+Q"),
        ],
    ),
    (
        "Editing",
        &[
            ("Send message", "Enter"),
            ("New line", "Shift+Enter / Ctrl+J"),
            ("Clear input", "Ctrl+K"),
            ("Focus latest", "Ctrl+L"),
            ("Pin file", "@"),
        ],
    ),
    (
        "Chat",
        &[
            ("Previous input", "Up"),
            ("Next input", "Down"),
            ("Toggle tool", "Tab"),
            ("Toggle all tools", "Shift+Tab"),
            ("Clear chat", "/clear"),
            ("Show history", "/history"),
            ("Show tokens", "/tokens"),
        ],
    ),
];

pub(crate) const COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show this help menu"),
    ("/clear", "Clear current conversation"),
    ("/model", "Show current AI model"),
    ("/tokens", "Show token usage"),
    ("/history", "Show saved sessions"),
    ("/resume <id>", "Resume a session"),
    ("/settings", "Open settings"),
    ("/files", "Open file browser"),
    ("/debug chat", "Load debug chat"),
    ("/debug files", "Load debug file tree"),
    ("/debug memory stats", "Show memory statistics"),
    ("/debug memory recall <query>", "Recall memories"),
    ("/debug log <id>", "Show session logs"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpMsg {
    Key(KeyEvent),
}

#[derive(Debug, Clone)]
pub struct HelpApp {
    pub selected_tab: usize,
    pub scroll: ScrollState,
    pub tip_index: usize,
    pub recent_sessions: Vec<Session>,
    pub status_message: Option<String>,
}

impl Default for HelpApp {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpApp {
    pub fn new() -> Self {
        let recent_sessions = Self::load_recent_sessions();
        let mut app = Self {
            selected_tab: 0,
            scroll: ScrollState::with_viewport(0, 20),
            tip_index: 0,
            recent_sessions,
            status_message: None,
        };
        app.sync_scroll_state();
        app
    }

    fn load_recent_sessions() -> Vec<Session> {
        if let Ok(workspace_path) = std::env::current_dir()
            && let Ok(db) = thndrs_mem::MemoryDatabase::for_workspace(&workspace_path)
        {
            let manager = SessionManager::new(db);
            if let Ok(sessions) = manager.list_sessions(10) {
                return sessions;
            }
        }
        Vec::new()
    }

    pub(crate) fn handle_input(&mut self, key: KeyEvent) -> ScreenAction {
        let Some(msg) = map_help_key_to_msg(key) else {
            return ScreenAction::None;
        };
        update(self, msg)
    }

    fn sync_scroll_state(&mut self) {
        self.scroll.set_viewport(self.tab_line_count(), 20);
    }

    fn tab_line_count(&self) -> usize {
        match HELP_TABS[self.selected_tab] {
            "Keyboard Shortcuts" => SHORTCUTS.iter().map(|(_, rows)| rows.len() + 3).sum::<usize>() + 3,
            "Commands" => COMMANDS.len() + 4,
            "Tips" => TIPS.len() + 10,
            "About" => 30,
            "Tutorial" => 24 + self.recent_sessions.len().min(5),
            _ => 20,
        }
    }

    fn refresh_sessions(&mut self) {
        self.recent_sessions = Self::load_recent_sessions();
        self.status_message = Some("Session list refreshed".to_string());
        self.sync_scroll_state();
    }

    fn next_tip(&mut self) {
        self.tip_index = (self.tip_index + 1) % TIPS.len();
    }

    pub fn current_tip(&self) -> &str {
        TIPS[self.tip_index % TIPS.len()]
    }

    pub fn version_string() -> String {
        match option_env!("CARGO_PKG_VERSION") {
            Some(version) => format!("Thunderus v{version}"),
            None => "Thunderus v0.1.0".to_string(),
        }
    }

    pub fn build_info() -> String {
        format!(
            "Build: {} | {} {}",
            Self::version_string(),
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    }
}

pub(crate) fn map_help_key_to_msg(key: KeyEvent) -> Option<HelpMsg> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    Some(HelpMsg::Key(key))
}

pub(crate) fn update(model: &mut HelpApp, msg: HelpMsg) -> ScreenAction {
    match msg {
        HelpMsg::Key(key) => match key.code {
            KeyCode::Esc => ScreenAction::ReturnToPrevious,
            KeyCode::Left | KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if model.selected_tab > 0 {
                    model.selected_tab -= 1;
                    model.scroll.set_offset(0);
                    model.sync_scroll_state();
                }
                ScreenAction::None
            }
            KeyCode::Right | KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if model.selected_tab + 1 < HELP_TABS.len() {
                    model.selected_tab += 1;
                    model.scroll.set_offset(0);
                    model.sync_scroll_state();
                }
                ScreenAction::None
            }
            KeyCode::Tab => {
                model.selected_tab = (model.selected_tab + 1) % HELP_TABS.len();
                model.scroll.set_offset(0);
                model.sync_scroll_state();
                ScreenAction::None
            }
            KeyCode::BackTab => {
                model.selected_tab = if model.selected_tab == 0 { HELP_TABS.len() - 1 } else { model.selected_tab - 1 };
                model.scroll.set_offset(0);
                model.sync_scroll_state();
                ScreenAction::None
            }
            KeyCode::Up => {
                model.scroll.scroll_up(1);
                ScreenAction::None
            }
            KeyCode::Down => {
                model.scroll.scroll_down(1);
                ScreenAction::None
            }
            KeyCode::PageUp => {
                model.scroll.page_up();
                ScreenAction::None
            }
            KeyCode::PageDown => {
                model.scroll.page_down();
                ScreenAction::None
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                model.refresh_sessions();
                ScreenAction::None
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                model.next_tip();
                ScreenAction::None
            }
            _ => ScreenAction::None,
        },
    }
}

pub(crate) fn help_tabs() -> &'static [&'static str] {
    HELP_TABS
}

pub(crate) fn format_help_time_ago(timestamp: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*timestamp);
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if hours < 24 {
        format!("{hours}h ago")
    } else {
        format!("{days}d ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_tabs() {
        assert_eq!(help_tabs(), HELP_TABS);
    }

    #[test]
    fn test_help_app_new() {
        let app = HelpApp::new();
        assert_eq!(app.selected_tab, 0);
    }

    #[test]
    fn test_current_tip() {
        let app = HelpApp::new();
        assert!(!app.current_tip().is_empty());
    }

    #[test]
    fn test_next_tip() {
        let mut app = HelpApp::new();
        let first_tip = app.current_tip().to_string();
        app.next_tip();
        assert_ne!(app.current_tip(), first_tip);
    }

    #[test]
    fn test_version_string() {
        assert!(HelpApp::version_string().starts_with("Thunderus v"));
    }

    #[test]
    fn test_format_time_ago() {
        let timestamp = Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_help_time_ago(&timestamp), "5m ago");
    }
}
