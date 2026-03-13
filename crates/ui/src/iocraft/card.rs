use super::theme::resolve_theme;
use super::wrapped_line_count;
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

pub fn required_height(label: &str, area_width: u16) -> u16 {
    const MIN_CARD_HEIGHT: u16 = 3;
    const LABEL_PREFIX_WIDTH: u16 = 3;

    let inner_width = area_width.saturating_sub(2);
    if inner_width <= LABEL_PREFIX_WIDTH {
        return MIN_CARD_HEIGHT;
    }

    let label_width = inner_width - LABEL_PREFIX_WIDTH;
    let label_lines = wrapped_line_count(label, label_width);
    (label_lines as u16 + 2).max(MIN_CARD_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::{SuggestionCard, required_height};
    use ::iocraft::prelude::*;

    #[test]
    fn card_required_height_respects_minimums() {
        assert_eq!(required_height("hello", 0), 3);
        assert_eq!(required_height("hello", 5), 3);
    }

    #[test]
    fn card_required_height_grows_for_wrapped_labels() {
        assert!(required_height("this wraps across multiple lines", 10) > 3);
    }

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
