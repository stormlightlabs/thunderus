use super::hint_bar::HintToken;
use super::theme::resolve_theme;
use super::{HintBar, InputField, SuggestionCard};
use crate::ScreenAction;
use crate::welcome as legacy_welcome;
use ::iocraft::prelude::*;

const ASCII_LOGO: &str = r#"
▗▄▄▄▖▗▖ ▗▖▗▖ ▗▖▗▖  ▗▖▗▄▄▄ ▗▄▄▄▖▗▄▄▖ ▗▖ ▗▖ ▗▄▄▖
  █  ▐▌ ▐▌▐▌ ▐▌▐▛▚▖▐▌▐▌  █▐▌   ▐▌ ▐▌▐▌ ▐▌▐▌
  █  ▐▛▀▜▌▐▌ ▐▌▐▌ ▝▜▌▐▌  █▐▛▀▀▘▐▛▀▚▖▐▌ ▐▌ ▝▀▚▖
  █  ▐▌ ▐▌▝▚▄▞▘▐▌  ▐▌▐▙▄▄▀▐▙▄▄▖▐▌ ▐▌▝▚▄▞▘▗▄▄▞▘
"#;
const PROMPT_PREFIX: &str = "❯ ";
const CONTINUATION_PREFIX: &str = "  ";
const WELCOME_GREETING: &str = "Thunderus - What can I help you build?";

#[derive(Default, Props)]
pub struct WelcomeScreenProps<'a> {
    pub suggestions: Vec<String>,
    pub overlay: Option<AnyElement<'a>>,
    pub on_submit: HandlerMut<'static, String>,
    pub on_command: HandlerMut<'static, String>,
    pub on_action: HandlerMut<'static, ScreenAction>,
    pub on_activate_file_finder: HandlerMut<'static, ()>,
}

struct WelcomeCallbacks {
    on_submit: HandlerMut<'static, String>,
    on_command: HandlerMut<'static, String>,
    on_action: HandlerMut<'static, ScreenAction>,
    on_activate_file_finder: HandlerMut<'static, ()>,
}

#[derive(Clone)]
struct RenderedInputLine {
    prefix: &'static str,
    prefix_color: RenderedPrefixColor,
    text: String,
}

#[derive(Clone, Copy)]
enum RenderedPrefixColor {
    Prompt,
    Continuation,
}

#[component]
pub fn WelcomeScreen<'a>(mut hooks: Hooks, props: &mut WelcomeScreenProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = resolve_theme(&hooks);
    let suggestions_override = if props.suggestions.is_empty() { None } else { Some(props.suggestions.clone()) };
    let model = hooks.use_state(move || initial_model(suggestions_override));
    let callbacks = hooks.use_ref(|| WelcomeCallbacks {
        on_submit: props.on_submit.take(),
        on_command: props.on_command.take(),
        on_action: props.on_action.take(),
        on_activate_file_finder: props.on_activate_file_finder.take(),
    });

    hooks.use_terminal_events({
        let mut model = model;
        let mut callbacks = callbacks;
        move |event| {
            if let TerminalEvent::Key(key) = event
                && let Some(msg) = map_terminal_key_to_msg(&key)
            {
                dispatch_welcome_message(&mut model, &mut callbacks, msg);
            }
        }
    });

    let snapshot = model.read().clone();
    let input_lines = rendered_input_lines(&snapshot.input_buffer, snapshot.cursor_position);

    element! {
        View(
            width: 100pct,
            height: 100pct,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
            position: Position::Relative,
        ) {
            View(
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                padding_left: 2,
                padding_right: 2,
            ) {
                View(
                    width: 60,
                    max_width: 100pct,
                    flex_direction: FlexDirection::Column,
                    gap: 1,
                ) {
                    Text(
                        content: logo_text(),
                        color: theme.accent_cyan,
                        wrap: TextWrap::NoWrap,
                        align: TextAlign::Center,
                    )
                    Text(
                        content: WELCOME_GREETING,
                        color: theme.text_primary,
                        weight: Weight::Bold,
                        align: TextAlign::Center,
                    )
                    Text(content: "TRY ASKING", color: theme.text_muted, align: TextAlign::Center)
                    View(
                        width: 100pct,
                        flex_direction: FlexDirection::Column,
                        gap: 1,
                    ) {
                        #(snapshot.suggestions.iter().enumerate().map(|(idx, suggestion)| {
                            let mut model = model;
                            let mut callbacks = callbacks;
                            let label = suggestion.clone();
                            let is_selected = snapshot.selected_suggestion == Some(idx);

                            element! {
                                Button(
                                    handler: move |_| {
                                        dispatch_welcome_message(
                                            &mut model,
                                            &mut callbacks,
                                            legacy_welcome::WelcomeMsg::SelectSuggestion(idx),
                                        );
                                    },
                                    has_focus: false,
                                ) {
                                    SuggestionCard(
                                        icon: "›",
                                        label: label,
                                        selected: is_selected,
                                    )
                                }
                            }
                        }))
                    }
                }
            }
            HintBar(tokens: default_hint_tokens())
            InputField(prompt: "", value: "", has_focus: false, multiline: true, on_change: |_| {}) {
                #(input_lines.into_iter().map(|line| {
                    let prefix_color = match line.prefix_color {
                        RenderedPrefixColor::Prompt => theme.accent_cyan,
                        RenderedPrefixColor::Continuation => theme.text_muted,
                    };

                    element! {
                        MixedText(contents: vec![
                            MixedTextContent::new(line.prefix).color(prefix_color),
                            MixedTextContent::new(line.text).color(theme.text_primary),
                        ])
                    }
                }))
            }
            #(if props.overlay.is_some() {
                Some(element! {
                    View(position: Position::Absolute, top: 0, left: 0, width: 100pct, height: 100pct) {
                        #(props.overlay.iter_mut())
                    }
                }.into_any())
            } else {
                None
            })
        }
    }
}

fn initial_model(suggestions: Option<Vec<String>>) -> legacy_welcome::WelcomeApp {
    let mut model = legacy_welcome::WelcomeApp::new();
    if let Some(suggestions) = suggestions {
        model.suggestions = suggestions;
    }
    model
}

fn map_terminal_key_to_msg(key: &KeyEvent) -> Option<legacy_welcome::WelcomeMsg> {
    legacy_welcome::map_welcome_key_to_msg(crossterm_key_event(key))
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

fn dispatch_welcome_message(
    model: &mut State<legacy_welcome::WelcomeApp>, callbacks: &mut Ref<WelcomeCallbacks>,
    msg: legacy_welcome::WelcomeMsg,
) {
    let mut next = model.read().clone();
    let action = legacy_welcome::update(&mut next, msg);

    {
        let mut callbacks = callbacks.write();
        if let Some(submission) = next.take_pending_submission() {
            (callbacks.on_submit)(submission);
        }
        if let Some(command) = next.take_pending_command() {
            (callbacks.on_command)(command);
        }
        if next.take_activate_file_finder() {
            (callbacks.on_activate_file_finder)(());
        }
        if action != ScreenAction::None {
            (callbacks.on_action)(action);
        }
    }

    model.set(next);
}

fn rendered_input_lines(input: &str, cursor_position: usize) -> Vec<RenderedInputLine> {
    input_with_cursor(input, cursor_position)
        .split('\n')
        .enumerate()
        .map(|(idx, segment)| RenderedInputLine {
            prefix: if idx == 0 { PROMPT_PREFIX } else { CONTINUATION_PREFIX },
            prefix_color: if idx == 0 { RenderedPrefixColor::Prompt } else { RenderedPrefixColor::Continuation },
            text: segment.to_string(),
        })
        .collect()
}

fn input_with_cursor(input: &str, cursor_position: usize) -> String {
    let mut output = input.to_string();
    let cursor = cursor_position.min(output.len());
    output.insert(cursor, '\u{2588}');
    output
}

fn logo_text() -> String {
    ASCII_LOGO.trim_matches('\n').to_string()
}

fn default_hint_tokens() -> Vec<HintToken> {
    vec![
        HintToken::Text("Type ".to_string()),
        HintToken::Key("/help".to_string()),
        HintToken::Text(" for help, ".to_string()),
        HintToken::Key("ctrl+,".to_string()),
        HintToken::Text(" for settings, ".to_string()),
        HintToken::Key("ctrl+n".to_string()),
        HintToken::Text(" for new chat, ".to_string()),
        HintToken::Key("ctrl+o".to_string()),
        HintToken::Text(" for files, ".to_string()),
        HintToken::Key("Shift+Enter".to_string()),
        HintToken::Text("/".to_string()),
        HintToken::Key("ctrl+j".to_string()),
        HintToken::Text(" newline, ".to_string()),
        HintToken::Key("@".to_string()),
        HintToken::Text(" to pin files, ".to_string()),
        HintToken::Key("ctrl+d".to_string()),
        HintToken::Text(" to quit".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::{WelcomeScreen, default_hint_tokens, rendered_input_lines};
    use crate::ScreenAction;
    use crate::iocraft::HintToken;
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use smol::block_on;

    #[derive(Default, Props)]
    struct WelcomeHarnessProps {
        suggestions: Vec<String>,
        mode: HarnessMode,
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum HarnessMode {
        #[default]
        Submit,
        Command,
        ActivateFinder,
        Quit,
    }

    #[component]
    fn WelcomeHarness(props: &WelcomeHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut submitted = hooks.use_state(String::new);
        let mut command = hooks.use_state(String::new);
        let mut activated = hooks.use_state(|| false);
        let mut quit = hooks.use_state(|| false);

        let submitted_value = submitted.read().clone();
        let command_value = command.read().clone();
        let activated_value = activated.get();
        let quit_value = quit.get();

        if !submitted_value.is_empty() || !command_value.is_empty() || activated_value || quit_value {
            system.exit();
            let status = if !submitted_value.is_empty() {
                format!("submitted:{submitted_value}")
            } else if !command_value.is_empty() {
                format!("command:{command_value}")
            } else if activated_value {
                "finder".to_string()
            } else {
                "quit".to_string()
            };

            return element! {
                Text(content: status)
            }
            .into_any();
        }

        let mode = props.mode;

        element! {
            View(width: 80, height: 24) {
                WelcomeScreen(
                    suggestions: props.suggestions.clone(),
                    on_submit: move |value| {
                        if mode == HarnessMode::Submit {
                            submitted.set(value);
                        }
                    },
                    on_command: move |value| {
                        if mode == HarnessMode::Command {
                            command.set(value);
                        }
                    },
                    on_activate_file_finder: move |_| {
                        if mode == HarnessMode::ActivateFinder {
                            activated.set(true);
                        }
                    },
                    on_action: move |action| {
                        if mode == HarnessMode::Quit && action == ScreenAction::Quit {
                            quit.set(true);
                        }
                    },
                )
            }
        }
        .into_any()
    }

    #[test]
    fn welcome_screen_renders_brand_and_hints() {
        let actual = element! {
            View(width: 80, height: 24) {
                WelcomeScreen(suggestions: vec!["First".to_string(), "Second".to_string()])
            }
        }
        .to_string();

        assert!(actual.contains("Thunderus - What can I help you build?"));
        assert!(actual.contains("TRY ASKING"));
        assert!(actual.contains("Type /help"));
    }

    #[test]
    fn rendered_input_lines_prefix_first_and_continuation_rows() {
        let lines = rendered_input_lines("hi\nthere", 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].prefix, "❯ ");
        assert_eq!(lines[0].text, "hi█");
        assert_eq!(lines[1].prefix, "  ");
        assert_eq!(lines[1].text, "there");
    }

    #[test]
    fn default_hints_include_file_finder_shortcut() {
        let tokens = default_hint_tokens();
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token, HintToken::Key(value) if value == "@"))
        );
    }

    #[test]
    fn welcome_screen_submits_typed_input() {
        block_on(async {
            let canvases = element! {
                WelcomeHarness(mode: HarnessMode::Submit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('h'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('i'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("submitted:hi")));
        });
    }

    #[test]
    fn welcome_screen_selects_suggestion_with_keyboard() {
        block_on(async {
            let canvases = element! {
                WelcomeHarness(
                    mode: HarnessMode::Submit,
                    suggestions: vec!["first".to_string(), "second".to_string()],
                )
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Down)),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Down)),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("submitted:second")));
        });
    }

    #[test]
    fn welcome_screen_routes_slash_commands_separately() {
        block_on(async {
            let canvases = element! {
                WelcomeHarness(mode: HarnessMode::Command)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('/'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('h'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('e'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('l'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Char('p'))),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("command:/help")));
        });
    }

    #[test]
    fn welcome_screen_emits_file_finder_activation() {
        block_on(async {
            let canvases = element! {
                WelcomeHarness(mode: HarnessMode::ActivateFinder)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                KeyEvent::new(KeyEventKind::Press, KeyCode::Char('@')),
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("finder")));
        });
    }

    #[test]
    fn welcome_screen_emits_quit_action() {
        block_on(async {
            let canvases = element! {
                WelcomeHarness(mode: HarnessMode::Quit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                {
                    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('d'));
                    key.modifiers = KeyModifiers::CONTROL;
                    key
                },
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("quit")));
        });
    }
}
