use super::theme::resolve_theme;
use crate::app::ScreenMode;
use ::iocraft::prelude::*;

#[derive(Props)]
pub(crate) struct StatusBarProps {
    pub(crate) mode: ScreenMode,
    pub(crate) chat_model: Option<String>,
}

impl Default for StatusBarProps {
    fn default() -> Self {
        Self { mode: ScreenMode::Welcome, chat_model: None }
    }
}

#[component]
pub(crate) fn StatusBar(hooks: Hooks, props: &StatusBarProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let (left, right) = status_parts(props.mode, props.chat_model.as_deref());

    element! {
        View(
            width: 100pct,
            height: 1,
            flex_direction: FlexDirection::Row,
            background_color: theme.bg_secondary,
            padding_left: 1,
            padding_right: 1,
        ) {
            View(flex_grow: 1.0) {
                Text(content: left, color: theme.accent_cyan, weight: Weight::Bold)
            }
            View(flex_grow: 1.0, justify_content: JustifyContent::FlexEnd) {
                Text(content: right, color: theme.text_muted, align: TextAlign::Right)
            }
        }
    }
}

fn status_parts(mode: ScreenMode, chat_model: Option<&str>) -> (String, String) {
    let left = format!("{} ", screen_label(mode));
    let right = if mode == ScreenMode::Chat {
        chat_model
            .map(|model| format!("model: {model}"))
            .unwrap_or_else(app_version_string)
    } else {
        app_version_string()
    };

    (left, right)
}

fn app_version_string() -> String {
    match option_env!("CARGO_PKG_VERSION") {
        Some(version) => format!("Thunderus v{version}"),
        None => "Thunderus v0.1.0".to_string(),
    }
}

fn screen_label(mode: ScreenMode) -> &'static str {
    match mode {
        ScreenMode::Welcome => "WELCOME",
        ScreenMode::Chat => "CHAT",
        ScreenMode::Files => "FILES",
        ScreenMode::Settings => "SETTINGS",
        ScreenMode::Help => "HELP",
    }
}

#[cfg(test)]
mod tests {
    use super::{StatusBar, status_parts};
    use crate::app::ScreenMode;
    use ::iocraft::prelude::*;

    #[test]
    fn status_parts_use_model_name_only_in_chat_mode() {
        let (left, right) = status_parts(ScreenMode::Chat, Some("gpt-5"));
        assert_eq!(left, "CHAT ");
        assert_eq!(right, "model: gpt-5");

        let (_, right) = status_parts(ScreenMode::Welcome, Some("ignored"));
        assert!(right.starts_with("Thunderus v"));
    }

    #[test]
    fn status_bar_renders_both_sides() {
        let actual = element! {
            View(width: 30) {
                StatusBar(mode: ScreenMode::Chat, chat_model: Some("gpt-5".to_string()))
            }
        }
        .to_string();

        assert_eq!(actual, " CHAT             model: gpt-5\n");
    }
}
