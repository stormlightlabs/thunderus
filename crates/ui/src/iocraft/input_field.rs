use super::theme::resolve_theme;
use ::iocraft::prelude::*;

#[derive(Default, Props)]
pub struct InputFieldProps<'a> {
    pub children: Vec<AnyElement<'a>>,
    pub prompt: String,
    pub value: String,
    pub has_focus: bool,
    pub multiline: bool,
    pub on_change: HandlerMut<'static, String>,
    pub handle: Option<Ref<TextInputHandle>>,
}

#[component]
pub fn InputField<'a>(hooks: Hooks, props: &mut InputFieldProps<'a>) -> impl Into<AnyElement<'a>> {
    let theme = resolve_theme(&hooks);
    let prompt = if props.prompt.is_empty() { "❯ ".to_string() } else { props.prompt.clone() };
    let has_custom_children = !props.children.is_empty();

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
            #(if has_custom_children {
                None
            } else {
                Some(element! {
                    Text(content: prompt, color: theme.accent_cyan)
                }.into_any())
            })
            #(if has_custom_children {
                Some(element! {
                    View(flex_grow: 1.0, width: 100pct, flex_direction: FlexDirection::Column) {
                        #(props.children.drain(..))
                    }
                }.into_any())
            } else {
                Some(element! {
                    View(flex_grow: 1.0, width: 100pct) {
                        TextInput(
                            color: theme.text_primary,
                            cursor_color: theme.accent_cyan,
                            value: props.value.clone(),
                            has_focus: props.has_focus,
                            multiline: props.multiline,
                            on_change: props.on_change.take(),
                            handle: props.handle,
                        )
                    }
                }.into_any())
            })
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

    #[test]
    fn input_field_can_render_custom_multiline_children() {
        let actual = element! {
            View(width: 16) {
                InputField {
                    Text(content: "❯ hello\n  world")
                }
            }
        }
        .to_string();

        assert_eq!(actual, "────────────────\n ❯ hello        \n   world        \n");
    }
}
