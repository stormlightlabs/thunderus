use super::theme::resolve_theme;
use ::iocraft::prelude::*;

#[derive(Default, Props)]
pub struct SuggestionCardProps {
    pub icon: String,
    pub label: String,
    pub selected: bool,
}

#[component]
pub fn SuggestionCard(hooks: Hooks, props: &SuggestionCardProps) -> impl Into<AnyElement<'static>> {
    let theme = resolve_theme(&hooks);
    let border_color = if props.selected { theme.accent_cyan } else { theme.border_color };
    let label_color = if props.selected { theme.text_primary } else { theme.text_secondary };

    element! {
        View(
            width: 100pct,
            border_style: BorderStyle::Round,
            border_color: border_color,
            background_color: theme.bg_terminal,
            padding_left: 1,
            padding_right: 1,
        ) {
            MixedText(contents: vec![
                MixedTextContent::new(format!("{} ", props.icon)).color(theme.accent_cyan),
                MixedTextContent::new(&props.label).color(label_color),
            ])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SuggestionCard;
    use ::iocraft::prelude::*;

    #[test]
    fn card_renders_icon_label_and_border() {
        let actual = element! {
            View(width: 18) {
                SuggestionCard(icon: "›", label: "Review docs", selected: true)
            }
        }
        .to_string();

        assert_eq!(actual, "╭────────────────╮\n│ › Review docs  │\n╰────────────────╯\n");
    }
}
