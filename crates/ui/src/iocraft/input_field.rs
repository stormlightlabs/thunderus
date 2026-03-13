use super::theme::resolve_theme;
use ::iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps {
    pub prompt: String,
    pub value: String,
    pub has_focus: bool,
    pub multiline: bool,
    pub on_change: HandlerMut<'static, String>,
}

#[component]
pub fn InputField(hooks: Hooks, props: &mut InputFieldProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let prompt = if props.prompt.is_empty() { "❯ ".to_string() } else { props.prompt.clone() };

    element! {
        View(
            width: 100pct,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            border_style: BorderStyle::Single,
            border_edges: Edges::Top,
            border_color: theme.border_color,
            background_color: theme.bg_terminal,
            padding_left: 1,
            padding_right: 1,
        ) {
            Text(content: prompt, color: theme.accent_cyan)
            View(flex_grow: 1.0, width: 100pct) {
                TextInput(
                    color: theme.text_primary,
                    cursor_color: theme.accent_cyan,
                    value: props.value.clone(),
                    has_focus: props.has_focus,
                    multiline: props.multiline,
                    on_change: props.on_change.take(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InputField;
    use ::iocraft::prelude::*;

    #[test]
    fn input_field_renders_top_border_prompt_and_value() {
        let actual = element! {
            View(width: 16) {
                InputField(
                    prompt: "❯ ".to_string(),
                    value: "hello".to_string(),
                    has_focus: false,
                    multiline: false,
                    on_change: |_| {},
                )
            }
        }
        .to_string();

        assert_eq!(actual, "────────────────\n ❯ hello        \n");
    }
}
