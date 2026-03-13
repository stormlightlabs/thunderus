use super::StatusBar;
use super::chat::{ChatFileFinderOverlay, ChatScreen};
use super::files::FileBrowser;
use super::help::HelpScreen;
use super::settings::SettingsScreen;
use super::theme::ThemeProvider;
use super::welcome::WelcomeScreen;
use crate::event;
use crate::{App, IncomingStreamEvent, Msg, ScreenMode, enqueue_cmd, execute_cmd};
use ::iocraft::prelude::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type SharedSubmitter = Arc<Mutex<Box<dyn FnMut(String) -> std::result::Result<(), String> + Send>>>;
type SharedPoller = Arc<Mutex<Box<dyn FnMut() -> Option<IncomingStreamEvent> + Send>>>;

#[derive(Default, Props)]
pub struct AppRootProps {
    pub initial_app: Option<App>,
    pub submitter: Option<SharedSubmitter>,
    pub poller: Option<SharedPoller>,
}

#[component]
pub fn AppRoot(mut hooks: Hooks, props: &AppRootProps) -> impl Into<AnyElement<'static>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let body_height = terminal_height.saturating_sub(1).max(1);
    let app = hooks.use_state({
        let initial_app = props.initial_app.clone();
        move || initial_app.unwrap_or_default()
    });
    let revision = hooks.use_state(|| 0u64);

    hooks.use_terminal_events({
        let mut app = app;
        let mut revision = revision;
        let submitter = props.submitter.clone();
        move |event| {
            if let TerminalEvent::Key(key) = event {
                let app_snapshot = app.read().clone();
                if let Some(msg) = event::map_key(&app_snapshot, crossterm_key_event(&key)) {
                    dispatch_msg(&mut app, &mut revision, msg, submitter.as_ref());
                }
            }
        }
    });

    hooks.use_future({
        let mut app = app;
        let mut revision = revision;
        let poller = props.poller.clone();
        let submitter = props.submitter.clone();
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
                    dispatch_msg(
                        &mut app,
                        &mut revision,
                        Msg::Chat(crate::chat::ChatMsg::StreamEvent(event)),
                        submitter.as_ref(),
                    );
                } else {
                    smol::Timer::after(Duration::from_millis(16)).await;
                }
            }
        }
    });

    let snapshot = app.read().clone();
    if !snapshot.running {
        system.exit();
        return element! {
            Text(content: "")
        }
        .into_any();
    }

    let current_revision = revision.get();
    let body = active_screen(&snapshot, current_revision, terminal_width, body_height);

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

fn dispatch_msg(app: &mut State<App>, revision: &mut State<u64>, msg: Msg, submitter: Option<&SharedSubmitter>) {
    let mut next = app.read().clone();
    let mut queue = VecDeque::new();
    enqueue_cmd(&mut queue, next.update(msg));

    while let Some(cmd) = queue.pop_front() {
        let next_msg = if let Some(submitter) = submitter {
            let mut submitter = submitter.lock().expect("submitter lock should not be poisoned");
            let submitter_ref: &mut dyn FnMut(String) -> std::result::Result<(), String> = submitter.as_mut();
            execute_cmd(cmd, Some(submitter_ref))
        } else {
            execute_cmd(cmd, None)
        };

        if let Some(msg) = next_msg {
            let cmd = next.update(msg);
            enqueue_cmd(&mut queue, cmd);
        }
    }

    app.set(next);
    revision.set(revision.get().wrapping_add(1));
}

fn active_screen(app: &App, revision: u64, viewport_width: u16, viewport_height: u16) -> AnyElement<'static> {
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
                WelcomeScreen(
                    initial_model: Some(app.welcome.clone()),
                    revision: revision,
                    active: false,
                    handle_events: false,
                    suggestions: app.welcome.suggestions.clone(),
                    overlay: overlay,
                )
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
                initial_browser: Some(app.file_browser.clone()),
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

pub fn run_iocraft_app(
    initial_app: App, submitter: Option<SharedSubmitter>, poller: Option<SharedPoller>,
) -> crate::Result<()> {
    smol::block_on(async {
        element! {
            AppRoot(initial_app: Some(initial_app), submitter: submitter, poller: poller)
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
    use super::AppRoot;
    use crate::{App, ChatMessage, IncomingStreamEvent, ScreenMode, TokenUsage};
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use std::time::Duration;

    #[derive(Default, Props)]
    struct AppRootHarnessProps {
        initial_app: Option<App>,
        poller: Option<super::SharedPoller>,
    }

    #[component]
    fn AppRootHarness(props: &AppRootHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut timed_exit = hooks.use_state(|| false);

        hooks.use_future(async move {
            smol::Timer::after(Duration::from_millis(40)).await;
            timed_exit.set(true);
        });

        if timed_exit.get() {
            system.exit();
            return element! {
                Text(content: "timed")
            }
            .into_any();
        }

        element! {
            AppRoot(initial_app: props.initial_app.clone(), poller: props.poller.clone())
        }
        .into_any()
    }

    #[test]
    fn app_root_renders_welcome_and_status_bar() {
        let actual = element! {
            AppRoot(initial_app: Some(App::new()))
        }
        .to_string();

        assert!(actual.contains("WELCOME"));
        assert!(actual.contains("Thunderus"));
    }

    #[test]
    fn app_root_switches_to_settings_with_global_shortcut() {
        smol::block_on(async {
            let canvases = element! {
                AppRootHarness(initial_app: Some(App::new()))
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                {
                    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char(','));
                    key.modifiers = KeyModifiers::CONTROL;
                    key
                },
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("Auto-save conversations")));
        });
    }

    #[test]
    fn app_root_polls_stream_events_into_chat_view() {
        smol::block_on(async {
            let mut app = App::new();
            app.screen_mode = ScreenMode::Chat;
            app.chat.messages.push(ChatMessage::user("Explain".to_string()));
            app.chat
                .messages
                .push(crate::ChatMessage::assistant_streaming(String::new()));

            let events = vec![
                IncomingStreamEvent::Delta {
                    content: Some("Intent\n\nRender the root app.".to_string()),
                    reasoning_content: None,
                },
                IncomingStreamEvent::Done {
                    usage: Some(TokenUsage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 }),
                    model: Some("gpt-5".to_string()),
                },
            ];
            let mut events = events.into_iter();

            let canvases = element! {
                AppRootHarness(
                    initial_app: Some(app),
                    poller: Some(super::shared_poller(move || events.next())),
                )
            }
            .mock_terminal_render_loop(MockTerminalConfig::default())
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("Render the root app.")));
            assert!(canvases.iter().any(|canvas| canvas.contains("model: gpt-5")));
        });
    }
}
