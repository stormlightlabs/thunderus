use super::hint_bar::HintToken;
use super::theme::{Theme, resolve_theme};
use super::{HintBar, InputField};
use crate::ScreenAction;
use crate::help::{
    COMMANDS, HELP_TABS, HelpApp, HelpMsg, SHORTCUTS, TIPS, format_help_time_ago, help_tabs,
    update as update_help_model,
};
use ::iocraft::prelude::*;

const DEFAULT_VIEWPORT_WIDTH: u16 = 100;
const DEFAULT_VIEWPORT_HEIGHT: u16 = 28;
const HINT_ROW_HEIGHT: u16 = 1;
const STATUS_ROW_HEIGHT: u16 = 2;

#[derive(Props)]
pub struct HelpScreenProps {
    pub initial_help: Option<HelpApp>,
    pub revision: u64,
    pub active: bool,
    pub handle_events: bool,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub on_action: HandlerMut<'static, ScreenAction>,
}

impl Default for HelpScreenProps {
    fn default() -> Self {
        Self {
            initial_help: None,
            revision: 0,
            active: true,
            handle_events: true,
            viewport_width: 0,
            viewport_height: 0,
            on_action: HandlerMut::default(),
        }
    }
}

struct HelpCallbacks {
    on_action: HandlerMut<'static, ScreenAction>,
}

#[derive(Clone)]
struct HelpLine {
    text: String,
    color: Color,
    weight: Weight,
}

impl HelpLine {
    fn new(text: impl Into<String>, color: Color) -> Self {
        Self { text: text.into(), color, weight: Weight::Normal }
    }

    fn bold(text: impl Into<String>, color: Color) -> Self {
        Self { text: text.into(), color, weight: Weight::Bold }
    }
}

#[component]
pub fn HelpScreen(mut hooks: Hooks, props: &mut HelpScreenProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let viewport_width = resolve_dimension(props.viewport_width, terminal_width, DEFAULT_VIEWPORT_WIDTH);
    let viewport_height = resolve_dimension(props.viewport_height, terminal_height, DEFAULT_VIEWPORT_HEIGHT);
    let mut model = hooks.use_state({
        let initial_help = props.initial_help.clone();
        move || initial_help.unwrap_or_default()
    });
    let callbacks = hooks.use_ref(|| HelpCallbacks { on_action: props.on_action.take() });

    hooks.use_effect(
        {
            let mut model = model;
            let initial_help = props.initial_help.clone();
            move || {
                model.set(initial_help.clone().unwrap_or_default());
            }
        },
        [props.revision],
    );

    hooks.use_terminal_events({
        let mut model = model;
        let mut callbacks = callbacks;
        let active = props.active;
        let handle_events = props.handle_events;
        move |event| {
            if !active || !handle_events {
                return;
            }
            if let TerminalEvent::Key(key) = event
                && let Some(msg) = map_terminal_key_to_msg(&key)
            {
                dispatch_help_message(&mut model, &mut callbacks, msg);
            }
        }
    });

    let mut snapshot = model.read().clone();
    let main_height = viewport_height
        .saturating_sub(HINT_ROW_HEIGHT)
        .saturating_sub(STATUS_ROW_HEIGHT)
        .max(1);
    let lines = help_lines(&snapshot, theme);
    let page_size = main_height.saturating_sub(4).max(1) as usize;
    if snapshot.scroll.page_size != page_size || snapshot.scroll.total != lines.len() {
        snapshot.scroll.set_viewport(lines.len(), page_size);
        model.set(snapshot.clone());
    }
    let start = snapshot.scroll.offset.min(lines.len().saturating_sub(1));
    let end = (start + snapshot.scroll.page_size.max(1)).min(lines.len());
    let visible = if lines.is_empty() { Vec::new() } else { lines[start..end].to_vec() };
    let status_text = snapshot.status_message.clone().unwrap_or_else(HelpApp::version_string);
    let status_line = truncate_text(&status_text, viewport_width.saturating_sub(4) as usize);

    element! {
        View(
            width: viewport_width,
            height: viewport_height,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
        ) {
            View(
                height: main_height,
                width: 100pct,
                flex_direction: FlexDirection::Column,
                gap: 1,
                padding_left: 1,
                padding_right: 1,
                padding_top: 1,
            ) {
                View(
                    width: 100pct,
                    flex_direction: FlexDirection::Column,
                    border_style: BorderStyle::Round,
                    border_color: theme.border_color,
                    padding_left: 1,
                    padding_right: 1,
                    gap: 1,
                ) {
                    #(tab_row(&snapshot, theme))
                    #(visible.into_iter().map(|line| {
                        element! {
                            Text(content: line.text, color: line.color, weight: line.weight, wrap: TextWrap::Wrap)
                        }
                    }))
                }
            }
            HintBar(tokens: hint_tokens())
            View(height: STATUS_ROW_HEIGHT, width: 100pct) {
                InputField(prompt: "", value: "", has_focus: false, multiline: false, on_change: |_| {}) {
                    Text(content: status_line, color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
            }
        }
    }
}

fn resolve_dimension(explicit: u16, measured: u16, fallback: u16) -> u16 {
    if explicit > 0 {
        explicit
    } else if measured > 0 {
        measured
    } else {
        fallback
    }
}

fn map_terminal_key_to_msg(key: &KeyEvent) -> Option<HelpMsg> {
    Some(HelpMsg::Key(crossterm_key_event(key)))
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

fn dispatch_help_message(model: &mut State<HelpApp>, callbacks: &mut Ref<HelpCallbacks>, msg: HelpMsg) {
    let mut next = model.read().clone();
    let action = update_help_model(&mut next, msg);

    if action != ScreenAction::None {
        let mut callbacks = callbacks.write();
        (callbacks.on_action)(action);
    }

    model.set(next);
}

fn tab_row(snapshot: &HelpApp, theme: Theme) -> AnyElement<'static> {
    let contents = help_tabs()
        .iter()
        .enumerate()
        .flat_map(|(idx, tab)| {
            let mut parts = Vec::new();
            if idx > 0 {
                parts.push(MixedTextContent::new("  "));
            }

            let selected = idx == snapshot.selected_tab;
            let label = if selected { format!("[{tab}]") } else { (*tab).to_string() };
            let color = if selected { theme.accent_cyan } else { theme.text_secondary };
            let weight = if selected { Weight::Bold } else { Weight::Normal };
            parts.push(MixedTextContent::new(label).color(color).weight(weight));
            parts
        })
        .collect::<Vec<_>>();

    element! {
        MixedText(align: TextAlign::Center, contents: contents)
    }
    .into_any()
}

fn help_lines(snapshot: &HelpApp, theme: Theme) -> Vec<HelpLine> {
    match HELP_TABS[snapshot.selected_tab] {
        "Keyboard Shortcuts" => shortcut_lines(theme),
        "Commands" => command_lines(theme),
        "Tips" => tips_lines(snapshot, theme),
        "About" => about_lines(theme),
        "Tutorial" => tutorial_lines(snapshot, theme),
        _ => Vec::new(),
    }
}

fn shortcut_lines(theme: Theme) -> Vec<HelpLine> {
    let mut lines = Vec::new();
    for (section_name, shortcuts) in SHORTCUTS {
        lines.push(HelpLine::bold(*section_name, theme.accent_cyan));
        lines.push(HelpLine::new("", theme.text_secondary));

        for (name, keys) in *shortcuts {
            lines.push(HelpLine::new(format!("  {name} — {keys}"), theme.text_secondary));
        }

        lines.push(HelpLine::new("", theme.text_secondary));
    }
    lines
}

fn command_lines(theme: Theme) -> Vec<HelpLine> {
    let mut lines = vec![
        HelpLine::bold("Slash Commands", theme.accent_cyan),
        HelpLine::new("", theme.text_secondary),
    ];
    for (command, description) in COMMANDS {
        lines.push(HelpLine::new(
            format!("  {command} — {description}"),
            theme.text_secondary,
        ));
    }
    lines
}

fn tips_lines(snapshot: &HelpApp, theme: Theme) -> Vec<HelpLine> {
    let mut lines = vec![
        HelpLine::bold("Tips", theme.accent_cyan),
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold("Current Tip", theme.accent_green),
        HelpLine::new("", theme.text_secondary),
        HelpLine::new(format!("  {}", snapshot.current_tip()), theme.text_secondary),
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold("All Tips", theme.accent_green),
        HelpLine::new("", theme.text_secondary),
    ];

    for (idx, tip) in TIPS.iter().enumerate() {
        let marker = if idx == snapshot.tip_index % TIPS.len() { "▸" } else { " " };
        let preview = if tip.len() > 60 { format!("{}...", &tip[..60]) } else { (*tip).to_string() };
        lines.push(HelpLine::new(
            format!("{marker} {}. {}", idx + 1, preview),
            theme.text_secondary,
        ));
    }

    lines
}

fn about_lines(theme: Theme) -> Vec<HelpLine> {
    let logo = [
        " ▐▖   ▀▛▘▌ ▌▌ ▌▙ ▌▛▀▖▛▀▘▛▀▖▌ ▌▞▀▖",
        " ▐▝▚▖  ▌ ▙▄▌▌ ▌▌▌▌▌ ▌▙▄ ▙▄▘▌ ▌▚▄",
        " ▐▞▘   ▌ ▌ ▌▌ ▌▌▝▌▌ ▌▌  ▌▚ ▌ ▌▖ ▌",
        " ▝     ▘ ▘ ▘▝▀ ▘ ▘▀▀ ▀▀▘▘ ▘▝▀ ▝▀",
    ];

    let mut lines = logo
        .into_iter()
        .map(|line| HelpLine::new(line, theme.accent_cyan))
        .collect::<Vec<_>>();
    lines.extend([
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold(HelpApp::version_string(), theme.text_primary),
        HelpLine::new(HelpApp::build_info(), theme.text_muted),
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold("About", theme.accent_cyan),
        HelpLine::new(
            "Thunderus is an AI coding assistant that helps you write, understand, and improve code.",
            theme.text_secondary,
        ),
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold("Links", theme.accent_cyan),
        HelpLine::new("  Documentation: https://docs.thunderus.dev", theme.text_secondary),
        HelpLine::new("  GitHub: https://github.com/thunderus/thunderus", theme.text_secondary),
        HelpLine::new(
            "  Report Issues: https://github.com/thunderus/thunderus/issues",
            theme.text_secondary,
        ),
    ]);
    lines
}

fn tutorial_lines(snapshot: &HelpApp, theme: Theme) -> Vec<HelpLine> {
    let mut lines = vec![
        HelpLine::bold("Quick Start", theme.accent_cyan),
        HelpLine::new("", theme.text_secondary),
        HelpLine::bold(HelpApp::version_string(), theme.text_primary),
        HelpLine::new(HelpApp::build_info(), theme.text_muted),
        HelpLine::new("", theme.text_secondary),
        HelpLine::new("  Ctrl+N — Start a new conversation", theme.text_secondary),
        HelpLine::new("  Ctrl+O — Open the workspace file browser", theme.text_secondary),
        HelpLine::new("  Ctrl+W — Return to welcome from chat/files", theme.text_secondary),
        HelpLine::new("  /help — View keyboard shortcuts", theme.text_secondary),
        HelpLine::new("", theme.text_secondary),
    ];

    if !snapshot.recent_sessions.is_empty() {
        lines.push(HelpLine::bold("Recent Conversations", theme.accent_cyan));
        lines.push(HelpLine::new("", theme.text_secondary));
        for (idx, session) in snapshot.recent_sessions.iter().enumerate().take(5) {
            let title = session.display_title();
            let time_ago = format_help_time_ago(&session.updated_at);
            let session_id = if session.id.len() > 8 { &session.id[..8] } else { &session.id };
            lines.push(HelpLine::new(
                format!("  {}. {} — {} — {}", idx + 1, title, session_id, time_ago),
                theme.text_secondary,
            ));
        }
        lines.push(HelpLine::new("", theme.text_secondary));
    }

    lines.extend([
        HelpLine::bold("Pro Tip", theme.accent_green),
        HelpLine::new("", theme.text_secondary),
        HelpLine::new(format!("  {}", snapshot.current_tip()), theme.text_secondary),
        HelpLine::new("", theme.text_secondary),
        HelpLine::new("Tip: Press Ctrl+T to see another tip", theme.text_secondary),
    ]);
    lines
}

fn hint_tokens() -> Vec<HintToken> {
    vec![
        HintToken::Text("Press ".to_string()),
        HintToken::Key("Tab".to_string()),
        HintToken::Text("/".to_string()),
        HintToken::Key("Ctrl+←/→".to_string()),
        HintToken::Text(" switch tabs, ".to_string()),
        HintToken::Key("↑/↓".to_string()),
        HintToken::Text(" scroll, ".to_string()),
        HintToken::Key("Ctrl+T".to_string()),
        HintToken::Text(" next tip, ".to_string()),
        HintToken::Key("Ctrl+R".to_string()),
        HintToken::Text(" refresh, ".to_string()),
        HintToken::Key("Esc".to_string()),
        HintToken::Text(" exit".to_string()),
    ]
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut truncated = chars.into_iter().take(keep).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::HelpScreen;
    use crate::ScreenAction;
    use crate::help::HelpApp;
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use std::time::Duration;

    #[derive(Default, Props)]
    struct HelpHarnessProps {
        initial_help: Option<HelpApp>,
        mode: HarnessMode,
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum HarnessMode {
        #[default]
        TimedExit,
        Exit,
    }

    #[component]
    fn HelpHarness(props: &HelpHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
        let mut system = hooks.use_context_mut::<SystemContext>();
        let mut exit = hooks.use_state(|| false);
        let mut timed_exit = hooks.use_state(|| false);

        if props.mode == HarnessMode::TimedExit {
            hooks.use_future(async move {
                smol::Timer::after(Duration::from_millis(40)).await;
                timed_exit.set(true);
            });
        }

        if exit.get() || timed_exit.get() {
            system.exit();
            return element! {
                Text(content: if exit.get() { "exit" } else { "timed" })
            }
            .into_any();
        }

        let mode = props.mode;

        element! {
            HelpScreen(
                initial_help: props.initial_help.clone(),
                viewport_width: 100u16,
                viewport_height: 24u16,
                on_action: move |action| {
                    if mode == HarnessMode::Exit && action == ScreenAction::ReturnToPrevious {
                        exit.set(true);
                    }
                },
            )
        }
        .into_any()
    }

    #[test]
    fn help_screen_renders_shortcuts_tab() {
        let actual = element! {
            HelpScreen(viewport_width: 100u16, viewport_height: 24u16)
        }
        .to_string();

        assert!(actual.contains("Keyboard Shortcuts"));
        assert!(actual.contains("New chat"));
    }

    #[test]
    fn help_screen_switches_tabs() {
        smol::block_on(async {
            let canvases = element! {
                HelpHarness
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                KeyEvent::new(KeyEventKind::Press, KeyCode::Tab),
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("/help")));
        });
    }

    #[test]
    fn help_screen_emits_return_action_on_escape() {
        smol::block_on(async {
            let canvases = element! {
                HelpHarness(mode: HarnessMode::Exit)
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![TerminalEvent::Key(
                KeyEvent::new(KeyEventKind::Press, KeyCode::Esc),
            )])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("exit")));
        });
    }
}
