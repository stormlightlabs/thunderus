use super::hint_bar::HintToken;
use super::theme::{Theme, resolve_theme};
use super::{HintBar, InputField};
use crate::ScreenAction;
use crate::settings::{SettingItem, SettingsApp, SettingsMsg, setting_groups, update as update_settings_model};
use ::iocraft::prelude::*;

const DEFAULT_VIEWPORT_WIDTH: u16 = 100;
const DEFAULT_VIEWPORT_HEIGHT: u16 = 28;
const HINT_ROW_HEIGHT: u16 = 1;
const STATUS_ROW_HEIGHT: u16 = 2;
const SIDEBAR_WIDTH: u16 = 20;

#[derive(Props)]
pub struct SettingsScreenProps {
    pub initial_settings: Option<SettingsApp>,
    pub revision: u64,
    pub active: bool,
    pub handle_events: bool,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub on_action: HandlerMut<'static, ScreenAction>,
}

impl Default for SettingsScreenProps {
    fn default() -> Self {
        Self {
            initial_settings: None,
            revision: 0,
            active: true,
            handle_events: true,
            viewport_width: 0,
            viewport_height: 0,
            on_action: HandlerMut::default(),
        }
    }
}

struct SettingsCallbacks {
    on_action: HandlerMut<'static, ScreenAction>,
}

#[component]
pub fn SettingsScreen(mut hooks: Hooks, props: &mut SettingsScreenProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let (terminal_width, terminal_height) = hooks.use_terminal_size();
    let viewport_width = resolve_dimension(props.viewport_width, terminal_width, DEFAULT_VIEWPORT_WIDTH);
    let viewport_height = resolve_dimension(props.viewport_height, terminal_height, DEFAULT_VIEWPORT_HEIGHT);
    let mut model = hooks.use_state({
        let initial_settings = props.initial_settings.clone();
        move || initial_settings.unwrap_or_default()
    });
    let callbacks = hooks.use_ref(|| SettingsCallbacks { on_action: props.on_action.take() });

    hooks.use_effect(
        {
            let mut model = model;
            let initial_settings = props.initial_settings.clone();
            move || {
                model.set(initial_settings.clone().unwrap_or_default());
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
                dispatch_settings_message(&mut model, &mut callbacks, msg);
            }
        }
    });

    let mut snapshot = model.read().clone();
    let main_height = viewport_height
        .saturating_sub(HINT_ROW_HEIGHT)
        .saturating_sub(STATUS_ROW_HEIGHT)
        .max(1);
    let visible_rows = main_height.saturating_sub(4).max(1) as usize / 3;
    let visible_rows = visible_rows.max(1);
    let settings = snapshot.current_group_settings();
    if snapshot.scroll.page_size != visible_rows || snapshot.scroll.total != settings.len() {
        snapshot.scroll.set_viewport(settings.len(), visible_rows);
        model.set(snapshot.clone());
    }
    let status_line = truncate_text(&snapshot.status_text(), viewport_width.saturating_sub(4) as usize);

    element! {
        View(
            width: viewport_width,
            height: viewport_height,
            flex_direction: FlexDirection::Column,
            background_color: theme.bg_terminal,
            position: Position::Relative,
        ) {
            View(
                height: main_height,
                width: 100pct,
                flex_direction: FlexDirection::Row,
                gap: 1,
                padding_left: 1,
                padding_right: 1,
                padding_top: 1,
            ) {
                #(settings_sidebar(&snapshot, theme))
                #(settings_content(&snapshot, &settings, theme))
            }
            HintBar(tokens: hint_tokens())
            View(height: STATUS_ROW_HEIGHT, width: 100pct) {
                InputField(prompt: "", value: "", has_focus: false, multiline: false, on_change: |_| {}) {
                    Text(content: status_line, color: theme.text_muted, wrap: TextWrap::NoWrap)
                }
            }
            #(if snapshot.show_save_dialog || snapshot.show_reset_dialog {
                Some(confirm_dialog(
                    if snapshot.show_save_dialog {
                        "Save changes?"
                    } else {
                        "Reset to defaults?"
                    },
                    "y: yes / n: no",
                    viewport_width,
                    viewport_height,
                    theme,
                ))
            } else {
                None
            })
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

fn map_terminal_key_to_msg(key: &KeyEvent) -> Option<SettingsMsg> {
    Some(SettingsMsg::Key(crossterm_key_event(key)))
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

fn dispatch_settings_message(model: &mut State<SettingsApp>, callbacks: &mut Ref<SettingsCallbacks>, msg: SettingsMsg) {
    let mut next = model.read().clone();
    let action = update_settings_model(&mut next, msg);

    if action != ScreenAction::None {
        let mut callbacks = callbacks.write();
        (callbacks.on_action)(action);
    }

    model.set(next);
}

fn settings_sidebar(snapshot: &SettingsApp, theme: Theme) -> AnyElement<'static> {
    element! {
        View(
            width: SIDEBAR_WIDTH,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border_color,
            padding_left: 1,
            padding_right: 1,
            gap: 1,
        ) {
            Text(content: SettingsApp::version_string(), color: theme.text_muted, weight: Weight::Bold)
            #(setting_groups().iter().enumerate().map(|(idx, group)| {
                let selected = idx == snapshot.selected_group;
                let prefix = if selected { "› " } else { "  " };
                let color = if selected { theme.accent_cyan } else { theme.text_secondary };
                let weight = if selected { Weight::Bold } else { Weight::Normal };

                element! {
                    Text(content: format!("{prefix}{group}"), color: color, weight: weight)
                }
            }))
        }
    }
    .into_any()
}

fn settings_content(snapshot: &SettingsApp, settings: &[SettingItem], theme: Theme) -> AnyElement<'static> {
    let start = snapshot.scroll.offset.min(settings.len().saturating_sub(1));
    let end = (start + snapshot.scroll.page_size.max(1)).min(settings.len());
    let rows = if settings.is_empty() { &[][..] } else { &settings[start..end] };
    let actions = if snapshot.has_changes {
        "Press Ctrl+S to save or Ctrl+R to reset"
    } else {
        "Press Ctrl+R to reset to defaults"
    };

    element! {
        View(
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            border_style: BorderStyle::Round,
            border_color: theme.border_color,
            padding_left: 1,
            padding_right: 1,
            gap: 1,
        ) {
            Text(content: snapshot.current_group_name(), color: theme.text_primary, weight: Weight::Bold)
            #(if rows.is_empty() {
                Some(element! {
                    Text(content: "No settings available for this category", color: theme.text_muted)
                }.into_any())
            } else {
                None
            })
            #(rows.iter().enumerate().map(|(idx, item)| {
                let is_active = start + idx == snapshot.active_setting_index();
                setting_row(item, is_active, theme)
            }))
            View(flex_grow: 1.0, justify_content: JustifyContent::FlexEnd) {
                Text(content: actions, color: theme.text_muted, wrap: TextWrap::Wrap)
            }
        }
    }
    .into_any()
}

fn setting_row(item: &SettingItem, is_active: bool, theme: Theme) -> AnyElement<'static> {
    let background_color = if is_active { theme.bg_tertiary } else { theme.bg_terminal };

    element! {
        View(
            height: 3,
            width: 100pct,
            flex_direction: FlexDirection::Row,
            background_color: background_color,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(width: 60pct, flex_direction: FlexDirection::Column) {
                Text(content: item.name(), color: theme.text_primary, weight: Weight::Bold, wrap: TextWrap::Wrap)
                Text(content: item.description(), color: theme.text_muted, wrap: TextWrap::Wrap)
            }
            View(width: 40pct, justify_content: JustifyContent::Center, align_items: AlignItems::FlexEnd) {
                Text(content: setting_value(item), color: value_color(item, theme), wrap: TextWrap::Wrap)
            }
        }
    }
    .into_any()
}

fn setting_value(item: &SettingItem) -> String {
    match item {
        SettingItem::Toggle { value, .. } => {
            if *value {
                "[ON]".to_string()
            } else {
                "[OFF]".to_string()
            }
        }
        SettingItem::Select { value, .. } => value.clone(),
        SettingItem::Number { value, min, max, .. } => format!("{min:.1} ◀ {value:.1} ▶ {max:.1}"),
    }
}

fn value_color(item: &SettingItem, theme: Theme) -> Color {
    match item {
        SettingItem::Toggle { value, .. } => {
            if *value {
                theme.accent_cyan
            } else {
                theme.text_muted
            }
        }
        SettingItem::Select { .. } | SettingItem::Number { .. } => theme.accent_cyan,
    }
}

fn hint_tokens() -> Vec<HintToken> {
    vec![
        HintToken::Text("Press ".to_string()),
        HintToken::Key("Ctrl+↑/↓".to_string()),
        HintToken::Text(" switch groups, ".to_string()),
        HintToken::Key("↑/↓".to_string()),
        HintToken::Text(" navigate, ".to_string()),
        HintToken::Key("Enter".to_string()),
        HintToken::Text(" toggle, ".to_string()),
        HintToken::Key("Esc".to_string()),
        HintToken::Text(" exit".to_string()),
    ]
}

fn confirm_dialog(
    title: &str, hint: &str, viewport_width: u16, viewport_height: u16, theme: Theme,
) -> AnyElement<'static> {
    let dialog_width = 40u16.min(viewport_width.saturating_sub(4)).max(20);
    let dialog_height = 5u16.min(viewport_height.saturating_sub(2)).max(4);
    let left = viewport_width.saturating_sub(dialog_width) / 2;
    let top = viewport_height.saturating_sub(dialog_height) / 2;

    element! {
        View(
            position: Position::Absolute,
            left: left,
            top: top,
            width: dialog_width,
            height: dialog_height,
            border_style: BorderStyle::Round,
            border_color: theme.accent_cyan,
            background_color: theme.bg_secondary,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
        ) {
            Text(content: title, color: theme.text_primary, weight: Weight::Bold, align: TextAlign::Center)
            Text(content: hint, color: theme.text_secondary, align: TextAlign::Center)
        }
    }
    .into_any()
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
    use super::SettingsScreen;
    use crate::ScreenAction;
    use crate::settings::SettingsApp;
    use ::iocraft::prelude::*;
    use futures::stream::{self, StreamExt};
    use std::time::Duration;

    #[derive(Default, Props)]
    struct SettingsHarnessProps {
        initial_settings: Option<SettingsApp>,
        mode: HarnessMode,
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    enum HarnessMode {
        #[default]
        TimedExit,
        Exit,
    }

    #[component]
    fn SettingsHarness(props: &SettingsHarnessProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
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
            SettingsScreen(
                initial_settings: props.initial_settings.clone(),
                viewport_width: 96u16,
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
    fn settings_screen_renders_sidebar_and_general_settings() {
        let actual = element! {
            SettingsScreen(viewport_width: 96u16, viewport_height: 24u16)
        }
        .to_string();

        assert!(actual.contains("General"));
        assert!(actual.contains("Auto-save conversations"));
    }

    #[test]
    fn settings_screen_switches_groups_and_updates_values() {
        smol::block_on(async {
            let canvases = element! {
                SettingsHarness
            }
            .mock_terminal_render_loop(MockTerminalConfig::with_events(stream::iter(vec![
                TerminalEvent::Key({
                    let mut key = KeyEvent::new(KeyEventKind::Press, KeyCode::Char('j'));
                    key.modifiers = KeyModifiers::CONTROL;
                    key
                }),
                TerminalEvent::Key(KeyEvent::new(KeyEventKind::Press, KeyCode::Enter)),
            ])))
            .map(|canvas| canvas.to_string())
            .collect::<Vec<_>>()
            .await;

            assert!(canvases.iter().any(|canvas| canvas.contains("Appearance")));
            assert!(canvases.iter().any(|canvas| canvas.contains("Oxocarbon Light")));
        });
    }

    #[test]
    fn settings_screen_emits_return_action_on_escape() {
        smol::block_on(async {
            let canvases = element! {
                SettingsHarness(mode: HarnessMode::Exit)
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
