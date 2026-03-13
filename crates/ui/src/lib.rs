//! Terminal UI for Thunderus

mod commands;

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use std::collections::VecDeque;
use thiserror::Error;

pub mod chat;
pub mod components;
pub mod elements;
pub mod event;
pub mod files;
pub mod finder;
pub mod help;
pub mod iocraft;
pub mod layout;
pub mod scroll;
pub mod settings;
pub mod welcome;

pub use chat::{ChatApp, ChatMessage, IncomingStreamEvent, StreamingState, TokenUsage};
pub use chat::{ToolCallDisplay, ToolCallStatus};
use files::FileBrowserApp;
use help::HelpApp;
use settings::SettingsApp;
use welcome::WelcomeApp;

type Submitter<'a> = dyn FnMut(String) -> std::result::Result<(), String> + 'a;

/// UI Errors
#[derive(Error, Debug)]
pub enum UiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Terminal error: {0}")]
    Terminal(String),
}

pub type Result<T> = std::result::Result<T, UiError>;

/// Color scheme based on designs/README.md Oxocarbon Dark theme
pub mod colors {
    use ratatui::style::Color;

    pub const ACCENT_CYAN: Color = Color::Rgb(0x33, 0xb1, 0xff);
    pub const ACCENT_PINK: Color = Color::Rgb(0xff, 0x7e, 0xb6);
    pub const ACCENT_PURPLE: Color = Color::Rgb(0xbe, 0x95, 0xff);
    pub const ACCENT_GREEN: Color = Color::Rgb(0x42, 0xbe, 0x65);
    pub const ACCENT_YELLOW: Color = Color::Rgb(0xf1, 0xc2, 0x1b);
    pub const ACCENT_RED: Color = Color::Rgb(0xfa, 0x4d, 0x56);
    pub const BG_PRIMARY: Color = Color::Rgb(0x16, 0x16, 0x16);
    pub const BG_SECONDARY: Color = Color::Rgb(0x1c, 0x1c, 0x1c);
    pub const BG_TERTIARY: Color = Color::Rgb(0x26, 0x26, 0x26);
    pub const BG_TERMINAL: Color = Color::Rgb(0x0c, 0x0c, 0x0c);
    pub const TEXT_PRIMARY: Color = Color::Rgb(0xf4, 0xf4, 0xf4);
    pub const TEXT_SECONDARY: Color = Color::Rgb(0xc6, 0xc6, 0xc6);
    pub const TEXT_MUTED: Color = Color::Rgb(0x8d, 0x8d, 0x8d);
    pub const BORDER_COLOR: Color = Color::Rgb(0x39, 0x39, 0x39);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    Welcome,
    Chat,
    Files,
    Settings,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenAction {
    None,
    Quit,
    SwitchTo(ScreenMode),
    ReturnToPrevious,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Quit,
    OpenSettings,
    StartNewChat,
    OpenFiles,
    CloseActiveChat,
    OpenHelp,
    Welcome(welcome::WelcomeMsg),
    Chat(chat::ChatMsg),
    Files(files::FilesMsg),
    Settings(settings::SettingsMsg),
    Help(help::HelpMsg),
    RunSlash(commands::SlashCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cmd {
    #[default]
    None,
    Batch(Vec<Cmd>),
    SubmitToBackend(String),
    RunSlash(commands::SlashCommand),
}

/// The TUI application state
#[derive(Clone)]
pub struct App {
    pub running: bool,
    pub screen_mode: ScreenMode,
    pub welcome: WelcomeApp,
    pub chat: ChatApp,
    pub file_browser: FileBrowserApp,
    pub settings: SettingsApp,
    pub help: HelpApp,
    previous_screen: Option<ScreenMode>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            running: true,
            screen_mode: ScreenMode::Welcome,
            welcome: WelcomeApp::new(),
            chat: ChatApp::new(),
            file_browser: FileBrowserApp::default(),
            settings: SettingsApp::new(),
            help: HelpApp::new(),
            previous_screen: None,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_input(&mut self, key: KeyEvent) {
        if let Some(msg) = event::map_key(self, key) {
            let cmd = self.update(msg);
            self.process_cmds_inline(cmd);
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent, frame_size: Rect) {
        if let Some(msg) = event::map_mouse(self, mouse, frame_size) {
            let cmd = self.update(msg);
            self.process_cmds_inline(cmd);
        }
    }

    pub fn update(&mut self, msg: Msg) -> Cmd {
        let cmd = match msg {
            Msg::Quit => {
                self.running = false;
                Cmd::None
            }
            Msg::OpenSettings => {
                self.open_settings();
                Cmd::None
            }
            Msg::StartNewChat => {
                self.start_new_chat();
                Cmd::None
            }
            Msg::OpenFiles => commands::dispatch("/files"),
            Msg::CloseActiveChat => {
                self.close_active_chat();
                Cmd::None
            }
            Msg::OpenHelp => {
                self.open_help();
                Cmd::None
            }
            Msg::Welcome(sub_msg) => {
                let action = welcome::update(&mut self.welcome, sub_msg);
                self.apply_screen_action(action);
                Cmd::None
            }
            Msg::Chat(sub_msg) => {
                let action = chat::update(&mut self.chat, sub_msg);
                self.apply_screen_action(action);
                Cmd::None
            }
            Msg::Files(sub_msg) => {
                let action = files::update(&mut self.file_browser, sub_msg);
                self.apply_screen_action(action);
                Cmd::None
            }
            Msg::Settings(sub_msg) => {
                let action = settings::update(&mut self.settings, sub_msg);
                self.apply_screen_action(action);
                Cmd::None
            }
            Msg::Help(sub_msg) => {
                let action = help::update(&mut self.help, sub_msg);
                self.apply_screen_action(action);
                Cmd::None
            }
            Msg::RunSlash(command) => {
                match command {
                    commands::SlashCommand::Empty => {}
                    commands::SlashCommand::DebugChat => {
                        self.chat.load_debug_chat();
                        self.screen_mode = ScreenMode::Chat;
                    }
                    commands::SlashCommand::DebugFiles => {
                        self.file_browser.load_debug_fixture();
                        self.screen_mode = ScreenMode::Files;
                    }
                    commands::SlashCommand::Files => {
                        if let Err(error) = self.file_browser.reload_workspace() {
                            self.push_assistant_message(format!("Unable to load workspace files: {error}"));
                        } else {
                            self.screen_mode = ScreenMode::Files;
                        }
                    }
                    commands::SlashCommand::History => {
                        let content = commands::format_session_history()
                            .unwrap_or_else(|error| format!("Failed to load history: {error}"));
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::Resume(session_id) => {
                        match commands::load_session_chat_messages(&session_id) {
                            Ok(messages) => {
                                self.chat.set_messages(messages);
                                self.chat.queue_backend_command(format!("/resume {}", session_id));
                                self.screen_mode = ScreenMode::Chat;
                            }
                            Err(error) => {
                                self.push_assistant_message(format!(
                                    "Failed to resume session `{session_id}`: {error}"
                                ));
                            }
                        }
                    }
                    commands::SlashCommand::Clear => {
                        self.chat.clear_chat();
                        self.chat.queue_backend_command("/clear".to_string());
                        self.screen_mode = ScreenMode::Chat;
                    }
                    commands::SlashCommand::Tokens => {
                        let content = match self.chat.last_usage {
                            Some(usage) => format!(
                                "Token usage:\n- prompt: {}\n- completion: {}\n- total: {}",
                                chat::u32_with_grouping(usage.prompt_tokens),
                                chat::u32_with_grouping(usage.completion_tokens),
                                chat::u32_with_grouping(usage.total_tokens)
                            ),
                            None => "No token usage recorded yet in this chat.".to_string(),
                        };
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::Model => {
                        let content = match self.chat.last_model.as_deref() {
                            Some(model) => format!("Current model: {}", model),
                            None => "Model information is not available yet.".to_string(),
                        };
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::DebugMemoryStats => {
                        let content = commands::format_memory_stats()
                            .unwrap_or_else(|error| format!("Failed to get memory stats: {error}"));
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::DebugMemoryRecall(query) => {
                        let content = commands::format_memory_recall(&query)
                            .unwrap_or_else(|error| format!("Failed to recall memory for `{query}`: {error}"));
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::DebugLog(session_id) => {
                        let content = commands::format_session_logs(&session_id)
                            .unwrap_or_else(|error| format!("Failed to get logs for `{session_id}`: {error}"));
                        self.push_assistant_message(content);
                    }
                    commands::SlashCommand::Settings => {
                        self.open_settings();
                    }
                    commands::SlashCommand::HelpCmd => {
                        self.open_help();
                    }
                    commands::SlashCommand::Unknown(raw) => {
                        self.push_assistant_message(format!(
                            "Unknown command `{raw}`. Available: `/help`, `/settings`, `/debug chat`, `/debug files`, `/files`, `/history`, `/resume <id>`, `/clear`, `/tokens`, `/model`, `/debug memory stats`, `/debug memory recall <query>`, `/debug log <id>`."
                        ));
                    }
                }
                Cmd::None
            }
        };

        combine_cmds(cmd, self.collect_pending_cmds())
    }

    pub(crate) fn open_settings(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Settings;
    }

    pub(crate) fn open_help(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Help;
    }

    fn exit_to_previous_or_chat(&mut self) {
        self.screen_mode = self.previous_screen.take().unwrap_or(ScreenMode::Chat);
    }

    fn start_new_chat(&mut self) {
        self.chat.clear_chat();
        self.chat.deactivate_file_finder();
        self.screen_mode = ScreenMode::Chat;
        self.previous_screen = None;
    }

    fn close_active_chat(&mut self) {
        self.chat.deactivate_file_finder();
        if matches!(
            self.screen_mode,
            ScreenMode::Chat | ScreenMode::Files | ScreenMode::Welcome
        ) {
            self.screen_mode = ScreenMode::Welcome;
            self.previous_screen = None;
        } else {
            self.exit_to_previous_or_chat();
        }
    }

    fn apply_screen_action(&mut self, action: ScreenAction) {
        match action {
            ScreenAction::None => {}
            ScreenAction::Quit => self.running = false,
            ScreenAction::SwitchTo(mode) => {
                if matches!(mode, ScreenMode::Settings | ScreenMode::Help) {
                    self.previous_screen = Some(self.screen_mode);
                }
                self.screen_mode = mode;
            }
            ScreenAction::ReturnToPrevious => self.exit_to_previous_or_chat(),
        }
    }

    fn collect_pending_cmds(&mut self) -> Cmd {
        let mut cmds = Vec::new();

        if self.welcome.take_activate_file_finder() {
            self.chat.activate_file_finder();
        }

        if let Some(content) = self.welcome.take_pending_submission() {
            self.chat.submit_user_message(content);
            self.screen_mode = ScreenMode::Chat;
        }

        if let Some(submission) = self.chat.take_pending_submission() {
            cmds.push(Cmd::SubmitToBackend(submission));
        }

        if let Some(command) = self.welcome.take_pending_command() {
            cmds.push(commands::dispatch(&command));
        }

        if let Some(command) = self.chat.take_pending_command() {
            cmds.push(commands::dispatch(&command));
        }

        if cmds.is_empty() { Cmd::None } else { Cmd::Batch(cmds) }
    }

    pub(crate) fn push_assistant_message(&mut self, content: String) {
        self.chat.messages.push(ChatMessage::assistant(content));
        self.screen_mode = ScreenMode::Chat;
    }

    fn process_cmds_inline(&mut self, cmd: Cmd) {
        let mut queue = VecDeque::new();
        enqueue_cmd(&mut queue, cmd);

        while let Some(next) = queue.pop_front() {
            if let Some(msg) = execute_cmd(next, None) {
                let follow_up = self.update(msg);
                enqueue_cmd(&mut queue, follow_up);
            }
        }
    }
}

pub(crate) fn combine_cmds(first: Cmd, second: Cmd) -> Cmd {
    match (first, second) {
        (Cmd::None, other) => other,
        (other, Cmd::None) => other,
        (Cmd::Batch(mut left), Cmd::Batch(right)) => {
            left.extend(right);
            Cmd::Batch(left)
        }
        (Cmd::Batch(mut left), right) => {
            left.push(right);
            Cmd::Batch(left)
        }
        (left, Cmd::Batch(mut right)) => {
            let mut cmds = vec![left];
            cmds.append(&mut right);
            Cmd::Batch(cmds)
        }
        (left, right) => Cmd::Batch(vec![left, right]),
    }
}

pub(crate) fn enqueue_cmd(queue: &mut VecDeque<Cmd>, cmd: Cmd) {
    match cmd {
        Cmd::None => {}
        Cmd::Batch(cmds) => {
            for entry in cmds {
                enqueue_cmd(queue, entry);
            }
        }
        other => queue.push_back(other),
    }
}

pub(crate) fn execute_cmd(cmd: Cmd, submitter: Option<&mut Submitter<'_>>) -> Option<Msg> {
    match cmd {
        Cmd::None => None,
        Cmd::Batch(_) => None,
        Cmd::SubmitToBackend(payload) => {
            if let Some(submitter) = submitter {
                match submitter(payload) {
                    Ok(()) => None,
                    Err(error) => Some(Msg::Chat(chat::ChatMsg::StreamEvent(IncomingStreamEvent::Error(error)))),
                }
            } else {
                Some(Msg::Chat(chat::ChatMsg::StreamEvent(IncomingStreamEvent::Error(
                    "No response backend configured for this UI mode.".to_string(),
                ))))
            }
        }
        Cmd::RunSlash(command) => Some(Msg::RunSlash(command)),
    }
}

/// Run the welcome screen TUI
pub fn run_welcome_app() -> Result<()> {
    iocraft::run_iocraft_app(App::new(), None, None)
}

/// Run the welcome screen TUI with streaming callbacks for conversation handling.
pub fn run_welcome_app_with_streaming<S, P>(submit_message: S, poll_event: P) -> Result<()>
where
    S: FnMut(String) -> std::result::Result<(), String> + Send + 'static,
    P: FnMut() -> Option<IncomingStreamEvent> + Send + 'static,
{
    let submitter = Some(iocraft::app::shared_submitter(submit_message));
    let poller = Some(iocraft::app::shared_poller(poll_event));
    iocraft::run_iocraft_app(App::new(), submitter, poller)
}

/// Run the chat screen TUI
pub fn run_chat_app() -> Result<()> {
    let mut app = App::new();
    app.screen_mode = ScreenMode::Chat;
    iocraft::run_iocraft_app(app, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    #[test]
    fn test_app_default() {
        let app = App::new();
        assert!(app.running);
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert!(!app.welcome.suggestions.is_empty());
    }

    #[test]
    fn test_question_mark_is_inserted_in_input() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('?')));

        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert_eq!(app.welcome.input_buffer, "?");
        assert_eq!(app.welcome.cursor_position, 1);
    }

    #[test]
    fn test_f1_opens_help() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::from(KeyCode::F(1)));

        assert_eq!(app.screen_mode, ScreenMode::Help);
        assert_eq!(app.previous_screen, Some(ScreenMode::Chat));
    }

    #[test]
    fn test_mouse_click_submits_suggestion() {
        let mut app = App::new();
        let frame_area = Rect::new(0, 0, 120, 40);
        let target_idx = app.welcome.suggestions.len().saturating_sub(1);
        let target_suggestion =
            welcome::suggestion_areas(frame_area, &app.welcome.suggestions, &app.welcome.input_buffer)[target_idx];
        let click_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target_suggestion.x + 1,
            row: target_suggestion.y + 1,
            modifiers: KeyModifiers::NONE,
        };

        app.handle_mouse(click_event, frame_area);

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].role, chat::MessageRole::User);
    }

    #[test]
    fn test_enter_transitions_to_chat() {
        let mut app = App::new();

        app.handle_input(KeyEvent::from(KeyCode::Char('h')));
        app.handle_input(KeyEvent::from(KeyCode::Char('i')));
        app.handle_input(KeyEvent::from(KeyCode::Enter));

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].role, chat::MessageRole::User);
        assert_eq!(app.chat.messages[0].content, "hi");
    }

    #[test]
    fn test_at_from_welcome_opens_chat_file_picker() {
        let mut app = App::new();
        app.handle_input(KeyEvent::from(KeyCode::Char('@')));
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert!(app.chat.is_file_finder_active());
    }

    #[test]
    fn test_ctrl_d_quits_in_chat_mode() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        let quit_event = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL);
        app.handle_input(quit_event);

        assert!(!app.running);
    }

    #[test]
    fn test_ctrl_n_starts_new_chat() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Welcome;
        app.chat.messages.push(ChatMessage::user("old".to_string()));
        app.chat.input_buffer = "draft".to_string();

        app.handle_input(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert!(app.chat.messages.is_empty());
        assert!(app.chat.input_buffer.is_empty());
    }

    #[test]
    fn test_ctrl_o_opens_file_browser() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Files);
    }

    #[test]
    fn test_ctrl_w_returns_to_welcome() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.handle_input(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));

        assert_eq!(app.screen_mode, ScreenMode::Welcome);
    }

    #[test]
    fn test_screen_modes() {
        assert_ne!(ScreenMode::Welcome, ScreenMode::Chat);
        assert_ne!(ScreenMode::Settings, ScreenMode::Help);
        assert_eq!(ScreenMode::Files, ScreenMode::Files);
    }

    #[test]
    fn test_app_has_settings_and_help() {
        let app = App::new();
        assert!(!app.settings.has_changes);
    }

    #[test]
    fn test_open_settings_and_help() {
        let mut app = App::new();
        app.screen_mode = ScreenMode::Chat;

        app.open_settings();
        assert_eq!(app.screen_mode, ScreenMode::Settings);
        assert_eq!(app.previous_screen, Some(ScreenMode::Chat));

        app.exit_to_previous_or_chat();
        assert_eq!(app.screen_mode, ScreenMode::Chat);
        assert_eq!(app.previous_screen, None);

        app.open_help();
        assert_eq!(app.screen_mode, ScreenMode::Help);

        app.exit_to_previous_or_chat();
        assert_eq!(app.screen_mode, ScreenMode::Chat);
    }
}
