use crate::chat::{ChatApp, ChatFileFinderOverlay, ChatMessage, ChatScreen, IncomingStreamEvent};
use crate::commands::{self, SlashCommand};
use crate::files::{FileBrowser, FileBrowserApp};
use crate::help::HelpScreen;
use crate::help_state::HelpApp;
use crate::settings::SettingsScreen;
use crate::settings_state::SettingsApp;
use crate::status_bar::StatusBar;
use crate::theme::ThemeProvider;
use crate::welcome::{WelcomeOutcome, WelcomeScreen, WelcomeState, handle_key as handle_welcome_key};
use ::iocraft::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Submitter = Box<dyn FnMut(String) -> std::result::Result<(), String> + Send>;
type Poller = Box<dyn FnMut() -> Option<IncomingStreamEvent> + Send>;

pub(crate) type SharedSubmitter = Arc<Mutex<Submitter>>;
pub(crate) type SharedPoller = Arc<Mutex<Poller>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenMode {
    Welcome,
    Chat,
    Files,
    Settings,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenAction {
    None,
    Quit,
    ReturnToPrevious,
}

#[derive(Clone)]
struct UiApp {
    running: bool,
    screen_mode: ScreenMode,
    previous_screen: Option<ScreenMode>,
    welcome: WelcomeState,
    chat: ChatApp,
    files: FileBrowserApp,
    settings: SettingsApp,
    help: HelpApp,
}

impl Default for UiApp {
    fn default() -> Self {
        Self {
            running: true,
            screen_mode: ScreenMode::Welcome,
            previous_screen: None,
            welcome: WelcomeState::default(),
            chat: ChatApp::new(),
            files: FileBrowserApp::default(),
            settings: SettingsApp::new(),
            help: HelpApp::new(),
        }
    }
}

impl UiApp {
    fn apply_screen_action(&mut self, action: ScreenAction) {
        match action {
            ScreenAction::None => {}
            ScreenAction::Quit => self.running = false,
            ScreenAction::ReturnToPrevious => {
                self.screen_mode = self.previous_screen.take().unwrap_or(ScreenMode::Chat);
            }
        }
    }

    fn open_settings(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Settings;
    }

    fn open_help(&mut self) {
        self.previous_screen = Some(self.screen_mode);
        self.screen_mode = ScreenMode::Help;
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
            self.screen_mode = self.previous_screen.take().unwrap_or(ScreenMode::Chat);
        }
    }

    fn process_welcome_outcome(&mut self, outcome: WelcomeOutcome) -> Option<String> {
        match outcome {
            WelcomeOutcome::None => None,
            WelcomeOutcome::ActivateFileFinder => {
                self.chat.activate_file_finder();
                None
            }
            WelcomeOutcome::Action(action) => {
                self.apply_screen_action(action);
                None
            }
            WelcomeOutcome::Prompt(content) => {
                self.chat.submit_user_message(content);
                self.screen_mode = ScreenMode::Chat;
                self.chat.take_pending_submission()
            }
            WelcomeOutcome::Command(command) => self.handle_slash_command(&command),
        }
    }

    fn handle_global_key(&mut self, key: &KeyEvent) -> Option<String> {
        if key.kind != KeyEventKind::Press {
            return None;
        }

        match key.code {
            KeyCode::F(1) => {
                self.open_help();
                None
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.start_new_chat();
                None
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(error) = self.files.reload_workspace() {
                    self.push_assistant_message(format!("Unable to load workspace files: {error}"));
                } else {
                    self.screen_mode = ScreenMode::Files;
                }
                None
            }
            KeyCode::Char(',') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_settings();
                None
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.close_active_chat();
                None
            }
            _ => None,
        }
    }

    fn handle_active_key(&mut self, key: &KeyEvent) -> Option<String> {
        match self.screen_mode {
            ScreenMode::Welcome => {
                let outcome = handle_welcome_key(&mut self.welcome, key);
                self.process_welcome_outcome(outcome)
            }
            ScreenMode::Chat => {
                self.chat.handle_input(crossterm_key_event(key));
                if !self.chat.running {
                    self.running = false;
                }
                if let Some(command) = self.chat.take_pending_command() {
                    return self.handle_slash_command(&command);
                }
                self.chat.take_pending_submission()
            }
            ScreenMode::Files => {
                let action = self.files.handle_input(crossterm_key_event(key));
                self.apply_screen_action(map_file_browser_action(action));
                None
            }
            ScreenMode::Settings => {
                let action = self.settings.handle_input(crossterm_key_event(key));
                self.apply_screen_action(action);
                None
            }
            ScreenMode::Help => {
                let action = self.help.handle_input(crossterm_key_event(key));
                self.apply_screen_action(action);
                None
            }
        }
    }

    fn handle_stream_event(&mut self, event: IncomingStreamEvent) {
        self.chat.handle_stream_event(event);
    }

    fn handle_slash_command(&mut self, raw: &str) -> Option<String> {
        match commands::parse(raw) {
            SlashCommand::Empty => None,
            SlashCommand::DebugChat => {
                self.chat.load_debug_chat();
                self.screen_mode = ScreenMode::Chat;
                None
            }
            SlashCommand::DebugFiles => {
                self.files.load_debug_fixture();
                self.screen_mode = ScreenMode::Files;
                None
            }
            SlashCommand::Files => {
                if let Err(error) = self.files.reload_workspace() {
                    self.push_assistant_message(format!("Unable to load workspace files: {error}"));
                } else {
                    self.screen_mode = ScreenMode::Files;
                }
                None
            }
            SlashCommand::History => {
                let content = commands::format_session_history()
                    .unwrap_or_else(|error| format!("Failed to load history: {error}"));
                self.push_assistant_message(content);
                None
            }
            SlashCommand::Resume(session_id) => match commands::load_session_chat_messages(&session_id) {
                Ok(messages) => {
                    self.chat.set_messages(messages);
                    self.screen_mode = ScreenMode::Chat;
                    Some(format!("/resume {session_id}"))
                }
                Err(error) => {
                    self.push_assistant_message(format!("Failed to resume session `{session_id}`: {error}"));
                    None
                }
            },
            SlashCommand::Clear => {
                self.chat.clear_chat();
                self.screen_mode = ScreenMode::Chat;
                Some("/clear".to_string())
            }
            SlashCommand::Tokens => {
                let content = match self.chat.last_usage {
                    Some(usage) => format!(
                        "Token usage:\n- prompt: {}\n- completion: {}\n- total: {}",
                        crate::chat::u32_with_grouping(usage.prompt_tokens),
                        crate::chat::u32_with_grouping(usage.completion_tokens),
                        crate::chat::u32_with_grouping(usage.total_tokens)
                    ),
                    None => "No token usage recorded yet in this chat.".to_string(),
                };
                self.push_assistant_message(content);
                None
            }
            SlashCommand::Model => {
                let content = match self.chat.last_model.as_deref() {
                    Some(model) => format!("Current model: {model}"),
                    None => "Model information is not available yet.".to_string(),
                };
                self.push_assistant_message(content);
                None
            }
            SlashCommand::DebugMemoryStats => {
                let content = commands::format_memory_stats()
                    .unwrap_or_else(|error| format!("Failed to get memory stats: {error}"));
                self.push_assistant_message(content);
                None
            }
            SlashCommand::DebugMemoryRecall(query) => {
                let content = commands::format_memory_recall(&query)
                    .unwrap_or_else(|error| format!("Failed to recall memory for `{query}`: {error}"));
                self.push_assistant_message(content);
                None
            }
            SlashCommand::DebugLog(session_id) => {
                let content = commands::format_session_logs(&session_id)
                    .unwrap_or_else(|error| format!("Failed to get logs for `{session_id}`: {error}"));
                self.push_assistant_message(content);
                None
            }
            SlashCommand::Settings => {
                self.open_settings();
                None
            }
            SlashCommand::HelpCmd => {
                self.open_help();
                None
            }
            SlashCommand::Unknown(raw) => {
                self.push_assistant_message(format!(
                    "Unknown command `{raw}`. Available: `/help`, `/settings`, `/debug chat`, `/debug files`, `/files`, `/history`, `/resume <id>`, `/clear`, `/tokens`, `/model`, `/debug memory stats`, `/debug memory recall <query>`, `/debug log <id>`."
                ));
                None
            }
        }
    }

    fn push_assistant_message(&mut self, content: String) {
        self.chat.messages.push(ChatMessage::assistant(content));
        self.screen_mode = ScreenMode::Chat;
    }
}

#[derive(Default, Props)]
pub struct AppRootProps {
    pub submitter: Option<SharedSubmitter>,
    pub poller: Option<SharedPoller>,
}

#[component]
pub fn AppRoot(mut hooks: Hooks, props: &AppRootProps) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let body_height = terminal_height.saturating_sub(1).max(1);
    let app = hooks.use_state(UiApp::default);
    let revision = hooks.use_state(|| 0u64);

    hooks.use_terminal_events({
        let mut app = app;
        let mut revision = revision;
        let submitter = props.submitter.clone();
        move |event| {
            let TerminalEvent::Key(key) = event else {
                return;
            };

            let mut next = app.read().clone();
            let pending_submission = next.handle_global_key(&key).or_else(|| next.handle_active_key(&key));

            if let Some(payload) = pending_submission {
                submit_submission(&mut next, payload, submitter.as_ref());
            }

            app.set(next);
            revision.set(revision.get().wrapping_add(1));
        }
    });

    hooks.use_future({
        let mut app = app;
        let mut revision = revision;
        let poller = props.poller.clone();
        async move {
            let Some(poller) = poller else {
                return;
            };

            loop {
                let next_event = {
                    let mut poller = poller.lock().expect("poller lock should not be poisoned");
                    (*poller)()
                };

                if let Some(event) = next_event {
                    let mut next = app.read().clone();
                    next.handle_stream_event(event);
                    app.set(next);
                    revision.set(revision.get().wrapping_add(1));
                } else {
                    smol::Timer::after(Duration::from_millis(16)).await;
                }
            }
        }
    });

    let snapshot = app.read().clone();
    if !snapshot.running {
        system.exit();
        return element!(Text(content: "")).into_any();
    }

    let body = active_screen(&snapshot, revision.get(), terminal_width, body_height);

    element! {
        ThemeProvider {
            View(width: terminal_width.max(1), height: terminal_height.max(1), flex_direction: FlexDirection::Column) {
                StatusBar(mode: snapshot.screen_mode, chat_model: snapshot.chat.last_model.clone())
                #(body)
            }
        }
    }
    .into_any()
}

fn active_screen(app: &UiApp, revision: u64, viewport_width: u16, viewport_height: u16) -> AnyElement<'static> {
    match app.screen_mode {
        ScreenMode::Welcome => {
            let overlay = if app.chat.is_file_finder_active() {
                Some(
                    element! {
                        ChatFileFinderOverlay(
                            chat: app.chat.clone(),
                            viewport_width: viewport_width,
                            viewport_height: viewport_height,
                        )
                    }
                    .into_any(),
                )
            } else {
                None
            };

            element! {
                WelcomeScreen(model: app.welcome.clone(), overlay: overlay)
            }
            .into_any()
        }
        ScreenMode::Chat => element! {
            ChatScreen(
                initial_chat: Some(app.chat.clone()),
                revision: revision,
                active: false,
                handle_events: false,
                handle_stream: false,
                viewport_width: viewport_width,
                viewport_height: viewport_height,
            )
        }
        .into_any(),
        ScreenMode::Files => element! {
            FileBrowser(
                initial_browser: Some(app.files.clone()),
                revision: revision,
                active: false,
                handle_events: false,
                viewport_width: viewport_width,
                viewport_height: viewport_height,
            )
        }
        .into_any(),
        ScreenMode::Settings => element! {
            SettingsScreen(
                initial_settings: Some(app.settings.clone()),
                revision: revision,
                active: false,
                handle_events: false,
                viewport_width: viewport_width,
                viewport_height: viewport_height,
            )
        }
        .into_any(),
        ScreenMode::Help => element! {
            HelpScreen(
                initial_help: Some(app.help.clone()),
                revision: revision,
                active: false,
                handle_events: false,
                viewport_width: viewport_width,
                viewport_height: viewport_height,
            )
        }
        .into_any(),
    }
}

fn submit_submission(app: &mut UiApp, payload: String, submitter: Option<&SharedSubmitter>) {
    if let Some(submitter) = submitter {
        let mut submitter = submitter.lock().expect("submitter lock should not be poisoned");
        if let Err(error) = submitter(payload) {
            app.handle_stream_event(IncomingStreamEvent::Error(error));
        }
    } else {
        app.handle_stream_event(IncomingStreamEvent::Error(
            "No response backend configured for this UI mode.".to_string(),
        ));
    }
}

fn map_file_browser_action(action: crate::files::FileBrowserAction) -> ScreenAction {
    match action {
        crate::files::FileBrowserAction::None => ScreenAction::None,
        crate::files::FileBrowserAction::Quit => ScreenAction::Quit,
        crate::files::FileBrowserAction::ExitToChat => ScreenAction::ReturnToPrevious,
    }
}

fn crossterm_key_event(key: &KeyEvent) -> crossterm::event::KeyEvent {
    crossterm::event::KeyEvent {
        code: key.code,
        modifiers: key.modifiers,
        kind: match key.kind {
            KeyEventKind::Press => crossterm::event::KeyEventKind::Press,
            KeyEventKind::Repeat => crossterm::event::KeyEventKind::Repeat,
            KeyEventKind::Release => crossterm::event::KeyEventKind::Release,
        },
        state: crossterm::event::KeyEventState::NONE,
    }
}

pub fn run_iocraft_app(submitter: Option<SharedSubmitter>, poller: Option<SharedPoller>) -> crate::Result<()> {
    smol::block_on(async {
        element! {
            AppRoot(submitter: submitter, poller: poller)
        }
        .render_loop()
        .await
        .map_err(|error| crate::UiError::Terminal(error.to_string()))
    })
}

pub(crate) fn shared_submitter<S>(submitter: S) -> SharedSubmitter
where
    S: FnMut(String) -> std::result::Result<(), String> + Send + 'static,
{
    Arc::new(Mutex::new(Box::new(submitter)))
}

pub(crate) fn shared_poller<P>(poller: P) -> SharedPoller
where
    P: FnMut() -> Option<IncomingStreamEvent> + Send + 'static,
{
    Arc::new(Mutex::new(Box::new(poller)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_mode_defaults_to_welcome() {
        let app = UiApp::default();
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert!(app.running);
    }

    #[test]
    fn return_to_previous_uses_saved_screen() {
        let mut app = UiApp::default();
        app.open_settings();
        assert_eq!(app.screen_mode, ScreenMode::Settings);
        assert_eq!(app.previous_screen, Some(ScreenMode::Welcome));

        app.apply_screen_action(ScreenAction::ReturnToPrevious);
        assert_eq!(app.screen_mode, ScreenMode::Welcome);
        assert_eq!(app.previous_screen, None);
    }

    #[test]
    fn slash_clear_returns_backend_command() {
        let mut app = UiApp::default();
        app.chat.submit_user_message("hello".to_string());
        let payload = app.handle_slash_command("/clear");
        assert_eq!(payload.as_deref(), Some("/clear"));
        assert!(app.chat.messages.is_empty());
    }
}
